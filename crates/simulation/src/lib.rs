//! Deterministic rules and actor-specific observations, without I/O or a clock.
//!
//! [`Game`] and [`ActionOutcome`] are backend-only types. Network adapters must
//! disclose observations and filter events; they must not serialize raw game state.

mod fixture;
mod observation;

pub use observation::{ActorView, ExitView, GroundItemView, ItemView, KnownPlace, Observation};

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use tor_world::{Direction, Location, RegionId, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ActorId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ItemId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
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
    TimeExhausted,
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
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
    world: World,
    seed: u64,
    tick: u64,
    actors: BTreeMap<ActorId, Actor>,
    items: BTreeMap<ItemId, Item>,
    next_actor_id: u64,
    next_item_id: u64,
}

impl Game {
    pub fn new(world: World, seed: u64) -> Self {
        Self {
            world,
            seed,
            tick: 0,
            actors: BTreeMap::new(),
            items: BTreeMap::new(),
            next_actor_id: 1,
            next_item_id: 1,
        }
    }

    /// Scenario setup operation; a network client cannot spawn an actor directly.
    /// Movement/wait use `turn_ticks`; taking an item uses half, rounded upward.
    pub fn spawn_actor(
        &mut self,
        location: Location,
        turn_ticks: NonZeroU64,
    ) -> Result<ActorId, GameError> {
        if !self.world.contains(location) {
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
        if !self.world.contains(location) {
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

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Authorized setup relocation; preserves recovery time and reveals the destination.
    pub fn teleport(&mut self, id: ActorId, location: Location) -> Result<(), GameError> {
        if !self.actors.contains_key(&id) {
            return Err(GameError::UnknownActor);
        }
        if !self.world.contains(location) {
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
        actor.visited.insert(location.region);
        Ok(())
    }

    pub fn seed(&self) -> u64 {
        self.seed
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
            Action::Move(direction) => {
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
        let ready_at = at_tick
            .checked_add(duration)
            .ok_or(GameError::TimeExhausted)?;

        // Everything that can fail has been validated before mutation.
        let actor = self.actors.get_mut(&id).expect("actor validated above");
        match kind {
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
