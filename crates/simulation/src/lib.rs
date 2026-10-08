//! Deterministic rules and actor-specific observations, without I/O or a clock.
//!
//! [`Game`] and [`ActionOutcome`] are backend-only types. Network adapters must
//! disclose observations and filter events; they must not serialize raw game state.

mod actions;
mod actor_store;
pub mod ai;
pub mod combat;
mod intention;
pub use intention::{
    IntentionControl, IntentionControlState, IntentionExecution, IntentionId, IntentionInput,
    IntentionOrigin, IntentionState, IntentionWork, MovementContext, QueuedIntention,
};
mod physics;
pub use physics::{BodySpec, Impact, MotionState, PhysicsEntity};
mod item_store;
mod items;
pub use items::{ItemClass, ItemSpec};
pub mod checkpoint;
pub mod diagnostics;
mod fixture;
mod navigation_map;
mod observation;
mod places;
pub use places::{NameOrigin, PlaceName};
mod streaming;
pub use streaming::{
    MemoryRecords, PinWork, RecordId, RecordStore, ReferencePoint, ReferencePointId,
    ReferenceTarget, RegionIdentities, RegionRecord, RegionRoot, RegionState, RegionTransition,
    TransitionError, TransitionReport, UnbuiltRegion,
};
mod travel;
pub use travel::TravelStep;

pub use observation::{
    ActorView, CellView, ExitView, GroundItemView, ItemView, KnownPlace, Observation,
};

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use tor_world::{Direction, Location, Passage, Position, Region, RegionId, Shared, World};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ActorId(pub u64);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ItemId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Action {
    Attack { target: ActorId },
    SetDoor { door: u64, open: bool },
    Move(Direction),
    Take { item: ItemId, quantity: Option<u64> },
    Drop { item: ItemId, quantity: Option<u64> },
    Wait,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameError {
    UnknownActor,
    NotActorsTurn,
    InvalidLocation,
    Occupied,
    Blocked,
    /// No distinction between unknown, hidden, carried, and out-of-reach items.
    ItemUnavailable,
    InvalidQuantity,
    DoorUnavailable,
    TimeExhausted,
    IdentityExhausted,
    ActorBusy,
    QueueFull,
    InvalidIntention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    AttackStarted {
        target: ActorId,
    },
    DoorChanged {
        door: u64,
        open: bool,
    },
    Moved {
        from: Location,
        to: Location,
    },
    Taken {
        item: ItemId,
        result: ItemId,
        quantity: u64,
    },
    Dropped {
        item: ItemId,
        result: ItemId,
        quantity: u64,
    },
    Waited,
}

/// An authoritative outcome, not a client message.
///
/// Actions take effect at `at_tick`; their duration is recovery time until the
/// actor can act again. `next_tick` is the next scheduled actor's decision time,
/// which may equal `at_tick` when several actors are ready together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionOutcome {
    pub actor: ActorId,
    pub at_tick: u64,
    pub kind: OutcomeKind,
    pub next_actor: Option<ActorId>,
    pub next_tick: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Actor {
    pending: Option<combat::Preparation>,
    combat: Option<combat::CombatState>,
    body: Shared<BodySpec>,
    motion: MotionState,
    location: Location,
    orientation: u8,
    turn_ticks: NonZeroU64,
    ready_at: u64,
    visited: BTreeSet<RegionId>,
    knowledge: Shared<BTreeSet<String>>,
    /// The asset clients draw it with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ItemLocation {
    Ground(Location),
    Carried(ActorId),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    motion: MotionState,
    orientation: u8,
    spec: Shared<ItemSpec>,
    quantity: u64,
    location: ItemLocation,
}

/// In-memory authoritative state. Persistence and controller ownership live in
/// the server layer; no actor receives special player privileges here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    combat: combat::CombatWorld,
    physics: physics::Physics,
    navigation: BTreeMap<ActorId, Shared<travel::Navigation>>,
    world: Shared<World>,
    seed: u64,
    tick: u64,
    actors: actor_store::ActorStore,
    items: item_store::ItemStore,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
    intentions: Shared<intention::IntentionQueue>,
    /// Region streaming state; empty unless regions were frozen or detached.
    lifecycle: streaming::Lifecycle,
}

/// Exact ceil(sqrt(2) * base), with checked integer arithmetic.
fn movement_cost(base: u64, direction: Direction) -> Result<u64, GameError> {
    if direction.components().is_none() {
        return Ok(base);
    }
    let squared = u128::from(base)
        .pow(2)
        .checked_mul(2)
        .ok_or(GameError::TimeExhausted)?;
    let root = squared.isqrt();
    u64::try_from(root + u128::from(root * root != squared)).map_err(|_| GameError::TimeExhausted)
}

impl Game {
    /// Package construction helpers keep authored identities stable across edits.
    pub fn authored_cell_valid(&self, at: Location) -> bool {
        self.world.contains(at) && !self.world.is_wall(at)
    }
    pub fn authored_links_clear(&self) -> bool {
        self.world.authored_links_clear()
    }
    /// Keep every allocation namespace beyond a retained future when restoring
    /// an older game. Existing entities keep their identity; newly created
    /// entities and work must not reuse identities from the abandoned branch.
    pub fn continue_identities(&mut self, later: &Game) {
        self.reserve_identities(later.next_actor_id, later.next_item_id, later.next_door_id);
        self.continue_record_ids(later);
        self.continue_intention_ids(later);
    }

    pub fn spawn_authored_actor(
        &mut self,
        id: u64,
        at: Location,
        ticks: NonZeroU64,
    ) -> Result<ActorId, GameError> {
        if id == 0 || id == u64::MAX || self.actors.contains_key(&ActorId(id)) {
            return Err(GameError::IdentityExhausted);
        }
        let next = self.next_actor_id;
        self.next_actor_id = id;
        let result = self.spawn_actor(at, ticks);
        self.next_actor_id = if result.is_ok() {
            self.next_actor_id.max(next)
        } else {
            next
        };
        result
    }
    pub fn place_authored_item(
        &mut self,
        id: u64,
        at: Location,
        name: String,
        owner: Option<ActorId>,
    ) -> Result<ItemId, GameError> {
        if id == 0 || id == u64::MAX || self.items.contains_key(&ItemId(id)) {
            return Err(GameError::IdentityExhausted);
        }
        if owner.is_some_and(|a| !self.actors.contains_key(&a)) {
            return Err(GameError::UnknownActor);
        }
        let next = self.next_item_id;
        self.next_item_id = id;
        let result = self.place_item(at, name);
        self.next_item_id = if result.is_ok() {
            self.next_item_id.max(next)
        } else {
            next
        };
        if let (Ok(item), Some(actor)) = (&result, owner) {
            self.items
                .edit(*item, |item| item.location = ItemLocation::Carried(actor))
                .expect("placed item");
        }
        result
    }
    pub fn place_authored_door(
        &mut self,
        id: u64,
        at: Location,
        open: bool,
        height: u8,
    ) -> Result<u64, GameError> {
        if id == 0 || id == u64::MAX || self.world.door_location(id).is_some() {
            return Err(GameError::IdentityExhausted);
        }
        let next = self.next_door_id;
        self.next_door_id = id;
        let result = self.place_door(at, open, height);
        self.next_door_id = if result.is_ok() {
            self.next_door_id.max(next)
        } else {
            next
        };
        result
    }
    fn reach(&self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        if direction.components().is_some() {
            self.world
                .diagonal_reach(from, direction, |side| side == from || !self.occupied(side))
        } else {
            self.world
                .adjacent(from, direction)
                .map(|to| (to, self.world.crossing_rotation(from, direction)))
        }
    }

    pub fn new(world: World, seed: u64) -> Self {
        Self {
            combat: combat::CombatWorld::new(seed),
            physics: physics::Physics::default(),
            navigation: BTreeMap::new(),
            world: Shared::new(world),
            seed,
            tick: 0,
            actors: actor_store::ActorStore::default(),
            items: item_store::ItemStore::default(),
            next_actor_id: 1,
            next_item_id: 1,
            next_door_id: 1,
            intentions: Shared::default(),
            lifecycle: streaming::Lifecycle::default(),
        }
    }

    /// Scenario setup operation; a network client cannot spawn an actor directly.
    /// Movement/wait use `turn_ticks`; taking an item uses half, rounded upward.
    pub fn spawn_actor(
        &mut self,
        location: Location,
        turn_ticks: NonZeroU64,
    ) -> Result<ActorId, GameError> {
        if !self.world.walkable(location) {
            return Err(GameError::InvalidLocation);
        }
        if self.occupied(location) {
            return Err(GameError::Occupied);
        }
        let next = self
            .next_actor_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = ActorId(self.next_actor_id);
        self.actors.insert(
            id,
            Actor {
                combat: None,
                pending: None,
                body: Shared::default(),
                motion: MotionState::default(),
                location,
                orientation: 0,
                turn_ticks,
                ready_at: self.tick,
                visited: BTreeSet::from([location.region]),
                knowledge: Shared::default(),
                asset: None,
            },
        );
        self.next_actor_id = next;
        self.sync_actor_lifecycle(id);
        Ok(id)
    }

    /// Scenario setup operation, distinct from player inventory manipulation.
    pub fn place_item(&mut self, location: Location, name: String) -> Result<ItemId, GameError> {
        if !self.world.walkable(location) {
            return Err(GameError::InvalidLocation);
        }
        let next = self
            .next_item_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = ItemId(self.next_item_id);
        self.items.insert(
            id,
            Item {
                motion: MotionState::default(),
                orientation: 0,
                spec: Shared::new(ItemSpec::ordinary(name)),
                quantity: 1,
                location: ItemLocation::Ground(location),
            },
        );
        self.next_item_id = next;
        Ok(id)
    }

    /// How tall a door based at `at` could be.
    pub fn door_clearance(&self, at: Location) -> u8 {
        self.world.door_clearance(at)
    }

    /// Whether a door this tall at `at` would leave its doorway open above it.
    pub fn doorway_open_above(&self, at: Location, height: u8) -> bool {
        self.world.doorway_open_above(at, height)
    }

    /// Author a door `height` cells tall; a closed door cannot cover an actor
    /// or ground object in any of its cells.
    pub fn place_door(
        &mut self,
        location: Location,
        open: bool,
        height: u8,
    ) -> Result<u64, GameError> {
        if !open && self.door_obstructed(location, height) {
            return Err(GameError::InvalidLocation);
        }
        let next = self
            .next_door_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = self.next_door_id;
        self.world
            .place_door(location, id, open, height)
            .map_err(|_| GameError::InvalidLocation)?;
        self.next_door_id = next;
        Ok(id)
    }
    /// Whether an actor or ground object is in any cell of a door based at
    /// `base` and `height` cells tall.
    fn door_obstructed(&self, base: Location, height: u8) -> bool {
        (0..i32::from(height)).any(|cells| {
            let Some(z) = base.position.z.checked_add(cells) else {
                return false;
            };
            let cell = Location {
                position: Position { z, ..base.position },
                ..base
            };
            self.occupied(cell) || self.items.at(ItemLocation::Ground(cell)).next().is_some()
        })
    }
    pub fn door_reachable_from(&self, from: Location, door: Location) -> bool {
        Direction::HORIZONTAL.into_iter().any(|direction| {
            self.reach(from, direction)
                .is_some_and(|(to, _)| to == door)
        })
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Authorized setup relocation; preserves recovery time and reveals the destination.
    pub fn teleport(&mut self, id: ActorId, location: Location) -> Result<(), GameError> {
        if !self.actors.contains_key(&id) {
            return Err(GameError::UnknownActor);
        }
        if !self.world.walkable(location) {
            return Err(GameError::InvalidLocation);
        }
        if !self.body_fits(id, location, 0, &self.actors[&id].body) {
            return Err(GameError::Occupied);
        }
        // A frozen actor's time stands at its freeze; syncing below shifts it.
        let clock = self.actor_clock(id);
        let mut actor = self.actors.get_mut(&id).expect("validated actor");
        actor.location = location;
        actor.orientation = 0;
        actor.motion = MotionState::default();
        if actor.pending.take().is_some() {
            actor.ready_at = clock;
        }
        actor.visited.insert(location.region);
        drop(actor);
        self.sync_actor_lifecycle(id);
        Ok(())
    }

    /// Set the asset clients draw an actor with.
    pub fn set_actor_asset(&mut self, id: ActorId, asset: Option<String>) -> Result<(), GameError> {
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        actor.asset = asset;
        Ok(())
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn connect_area(
        &mut self,
        passage: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), GameError> {
        self.world
            .connect_area(passage, turns, width, height)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn add_region(&mut self, region: Region) -> Result<(), GameError> {
        self.world
            .add_region(region)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn add_chamber(&mut self, region: Region) -> Result<(), GameError> {
        self.world
            .add_chamber(region)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn connect(&mut self, passage: Passage, quarter_turns: u8) -> Result<(), GameError> {
        self.world
            .connect(passage, quarter_turns)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Map-authoring metadata; no action time, names, boundaries or travel rules.
    /// A place hint the character learns by this authored name.
    pub fn set_named_place_hint(
        &mut self,
        location: Location,
        name: &str,
    ) -> Result<(), GameError> {
        if !places::valid_name(name) {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .set_named_place_hint(location, name)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn set_place_hint(&mut self, location: Location, present: bool) -> Result<(), GameError> {
        self.world
            .set_place_hint(location, present)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn set_wall(&mut self, location: Location, wall: bool) -> Result<(), GameError> {
        if wall
            && (self.occupied(location)
                || self
                    .items
                    .at(ItemLocation::Ground(location))
                    .next()
                    .is_some())
        {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .set_wall(location, wall)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Stable ordering: earliest ready time, then actor identity. Frozen
    /// actors never act.
    pub fn next_actor(&self) -> Option<ActorId> {
        if self.combat.outcome.terminal {
            return None;
        }
        if let Some(id) = self.combat.input_boundaries.iter().find(|id| {
            self.actors
                .get(id)
                .is_some_and(|a| a.alive() && a.ready_at <= self.tick)
                && !self.actor_frozen(**id)
        }) {
            return Some(*id);
        }
        self.actors
            .iter()
            .filter(|(id, a)| {
                a.alive() && a.pending.as_ref().is_none_or(|p| !p.active)
                    && !self.actor_frozen(**id)
            })
            .min_by_key(|(id, actor)| (actor.ready_at, **id))
            .map(|(id, _)| *id)
    }

    fn occupied(&self, location: Location) -> bool {
        self.actors
            .at(&self.world, location)
            .keys()
            .any(|id| self.actors[id].alive())
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;
    use tor_world::Position;
    #[test]
    fn snapshots_share_unchanged_world_items_and_navigation_but_isolate_mutations() {
        let mut game = Game::two_room(42);
        let location = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game
            .spawn_actor(location, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.refresh_navigation();
        let original = game.clone();
        game.act(actor, Action::Wait).unwrap();
        game.refresh_navigation();
        assert!(game.world.shares_storage(&original.world));
        assert!(game.items.shares_storage(&original.items));
        assert!(game.navigation[&actor].shares_storage(&original.navigation[&actor]));
        game.place_item(location, "new token".into()).unwrap();
        assert_ne!(game.items, original.items);
        assert!(game.world.shares_storage(&original.world));
        let scene = game.scene(actor).unwrap();
        let observation = game.observe(actor).unwrap();
        let counts = diagnostics::work_counts();
        let (combined, projected) = game.observe_scene(actor).unwrap();
        let after = diagnostics::work_counts();
        assert_eq!(after.scenes - counts.scenes, 1);
        assert_eq!(after.observations - counts.observations, 1);
        assert_eq!(combined, observation);
        assert_eq!(projected, scene);
    }
}
