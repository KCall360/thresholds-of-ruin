//! Deterministic rules and actor-specific observations, without I/O or a clock.
//!
//! [`Game`] and [`ActionOutcome`] are backend-only types. Network adapters must
//! disclose observations and filter events; they must not serialize raw game state.

mod fixture;
mod observation;
mod travel;
pub use travel::TravelStep;

pub use observation::{
    ActorView, CellView, ExitView, GroundItemView, ItemView, KnownPlace, Observation,
};

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use tor_world::{Direction, Location, Passage, Region, RegionId, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActorId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    SetDoor { door: u64, open: bool },
    Move(Direction),
    Take(ItemId),
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
    DoorUnavailable,
    TimeExhausted,
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    DoorChanged { door: u64, open: bool },
    Moved { from: Location, to: Location },
    Taken { item: ItemId },
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
    pub next_actor: ActorId,
    pub next_tick: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Actor {
    location: Location,
    orientation: u8,
    turn_ticks: NonZeroU64,
    ready_at: u64,
    visited: BTreeSet<RegionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ItemLocation {
    Ground(Location),
    Carried(ActorId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Item {
    name: String,
    location: ItemLocation,
}

/// In-memory authoritative state. Persistence and controller ownership live in
/// the server layer; no actor receives special player privileges here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    navigation: BTreeMap<ActorId, travel::Navigation>,
    world: World,
    legacy_perception: bool,
    scene_rules: bool,
    shadowcasting: bool,
    seed: u64,
    tick: u64,
    actors: BTreeMap<ActorId, Actor>,
    items: BTreeMap<ItemId, Item>,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
}

impl Game {
    pub fn new(world: World, seed: u64) -> Self {
        Self {
            navigation: BTreeMap::new(),
            world,
            legacy_perception: false,
            scene_rules: true,
            shadowcasting: true,
            seed,
            tick: 0,
            actors: BTreeMap::new(),
            items: BTreeMap::new(),
            next_actor_id: 1,
            next_item_id: 1,
            next_door_id: 1,
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
                location,
                orientation: 0,
                turn_ticks,
                ready_at: self.tick,
                visited: BTreeSet::from([location.region]),
            },
        );
        self.next_actor_id = next;
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
                name,
                location: ItemLocation::Ground(location),
            },
        );
        self.next_item_id = next;
        Ok(id)
    }

    /// Author a door; a closed door cannot cover an actor or ground object.
    pub fn place_door(&mut self, location: Location, open: bool) -> Result<u64, GameError> {
        if !open && self.door_obstructed(location) {
            return Err(GameError::InvalidLocation);
        }
        let next = self
            .next_door_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = self.next_door_id;
        self.world
            .place_door(location, id, open)
            .map_err(|_| GameError::InvalidLocation)?;
        self.next_door_id = next;
        Ok(id)
    }
    fn door_obstructed(&self, location: Location) -> bool {
        self.occupied(location)
            || self
                .items
                .values()
                .any(|i| i.location == ItemLocation::Ground(location))
    }
    pub fn door_reachable_from(&self, from: Location, door: Location) -> bool {
        [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ]
        .into_iter()
        .any(|direction| self.world.adjacent(from, direction) == Some(door))
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
        if self
            .actors
            .iter()
            .any(|(&other, actor)| other != id && actor.location == location)
        {
            return Err(GameError::Occupied);
        }
        let actor = self.actors.get_mut(&id).expect("validated actor");
        actor.location = location;
        actor.orientation = 0;
        actor.visited.insert(location.region);
        Ok(())
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Legacy save replay retains the original observation and vertical-step rules.
    pub fn use_legacy_perception(&mut self) {
        self.legacy_perception = true;
        self.scene_rules = false;
        self.shadowcasting = false;
    }

    pub fn use_portal_v2_rules(&mut self) {
        self.scene_rules = false;
        self.shadowcasting = false;
    }
    /// Preserve cell-centre ray perception when replaying pre-shadowcasting saves.
    pub fn use_ray_perception(&mut self) {
        self.shadowcasting = false;
    }
    pub fn uses_scene_rules(&self) -> bool {
        self.scene_rules
    }

    pub fn connect_area(
        &mut self,
        passage: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), GameError> {
        if !self.scene_rules {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .connect_area(passage, turns, width, height)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn add_region(&mut self, region: Region) -> Result<(), GameError> {
        if self.legacy_perception {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .add_region(region)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn connect(&mut self, passage: Passage, quarter_turns: u8) -> Result<(), GameError> {
        if self.legacy_perception {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .connect(passage, quarter_turns)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Map-authoring metadata; no action time, names, boundaries or travel rules.
    pub fn set_place_hint(&mut self, location: Location, present: bool) -> Result<(), GameError> {
        self.world
            .set_place_hint(location, present)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn set_wall(&mut self, location: Location, wall: bool) -> Result<(), GameError> {
        if self.legacy_perception
            || (wall
                && (self.occupied(location)
                    || self
                        .items
                        .values()
                        .any(|item| item.location == ItemLocation::Ground(location))))
        {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .set_wall(location, wall)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Stable ordering: earliest ready time, then actor identity.
    pub fn next_actor(&self) -> Option<ActorId> {
        self.actors
            .iter()
            .min_by_key(|(id, actor)| (actor.ready_at, **id))
            .map(|(id, _)| *id)
    }

    fn occupied(&self, location: Location) -> bool {
        self.actors.values().any(|actor| actor.location == location)
    }

    /// Apply one valid action atomically. Failed actions in this first slice are
    /// free; neither invalid requests nor blocked movement advance time.
    pub fn act(&mut self, id: ActorId, action: Action) -> Result<ActionOutcome, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if self.next_actor() != Some(id) {
            return Err(GameError::NotActorsTurn);
        }
        let (kind, duration) = match action {
            Action::SetDoor { door, open } => {
                let location = self
                    .world
                    .door_location(door)
                    .ok_or(GameError::DoorUnavailable)?;
                let current = self.world.door(location).expect("existing door");
                if current.open == open
                    || !self.door_reachable_from(actor.location, location)
                    || !self
                        .observe(id)?
                        .visible_cells
                        .iter()
                        .any(|c| c.location == location)
                    || (!open && self.door_obstructed(location))
                {
                    return Err(GameError::DoorUnavailable);
                }
                (
                    OutcomeKind::DoorChanged { door, open },
                    actor.turn_ticks.get(),
                )
            }
            Action::Move(direction) => {
                let direction = if self.scene_rules {
                    direction.rotated(actor.orientation)
                } else {
                    direction
                };
                if !self.legacy_perception
                    && matches!(direction, Direction::Up | Direction::Down)
                    && self.world.passage(actor.location, direction).is_none()
                {
                    return Err(GameError::Blocked);
                }
                let to = self
                    .world
                    .step(actor.location, direction)
                    .ok_or(GameError::Blocked)?;
                if self
                    .actors
                    .iter()
                    .any(|(&other_id, other)| other_id != id && other.location == to)
                {
                    return Err(GameError::Occupied);
                }
                (
                    OutcomeKind::Moved {
                        from: actor.location,
                        to,
                    },
                    actor.turn_ticks.get(),
                )
            }
            Action::Take(item) => {
                if self.items.get(&item).map(|item| item.location)
                    != Some(ItemLocation::Ground(actor.location))
                {
                    return Err(GameError::ItemUnavailable);
                }
                (
                    OutcomeKind::Taken { item },
                    actor.turn_ticks.get().div_ceil(2),
                )
            }
            Action::Wait => (OutcomeKind::Waited, actor.turn_ticks.get()),
        };
        let at_tick = self.tick;
        let new_orientation = match action {
            Action::Move(direction) if self.scene_rules => {
                (actor.orientation
                    + self
                        .world
                        .crossing_rotation(actor.location, direction.rotated(actor.orientation)))
                    % 4
            }
            _ => actor.orientation,
        };
        let ready_at = at_tick
            .checked_add(duration)
            .ok_or(GameError::TimeExhausted)?;

        // Everything that can fail has been validated before mutation.
        let actor = self.actors.get_mut(&id).expect("actor validated above");
        actor.orientation = new_orientation;
        match kind {
            OutcomeKind::DoorChanged { door, open } => {
                let location = self.world.door_location(door).expect("validated door");
                self.world.set_door(location, open);
            }
            OutcomeKind::Moved { to, .. } => {
                actor.location = to;
                actor.visited.insert(to.region);
            }
            OutcomeKind::Taken { item } => {
                self.items
                    .get_mut(&item)
                    .expect("item validated above")
                    .location = ItemLocation::Carried(id);
            }
            OutcomeKind::Waited => {}
        }
        actor.ready_at = ready_at;
        let next_actor = self.next_actor().expect("acting actor is still present");
        self.tick = self.actors[&next_actor].ready_at;
        Ok(ActionOutcome {
            actor: id,
            at_tick,
            kind,
            next_actor,
            next_tick: self.tick,
        })
    }
}
