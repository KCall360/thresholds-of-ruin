//! Internal action boundary shared by every actor and command source.
//!
//! Validation is read-only and includes recovery-time overflow checks. Effects
//! and scheduling run immediately afterward, with no callback, yield, or external
//! mutation between them. A prepared action is transient, never a saved job or a
//! client-visible promise. Future timed actions must revalidate at each boundary.

use crate::{
    movement_cost, Action, ActionOutcome, ActorId, Game, GameError, ItemLocation, OutcomeKind,
};
use tor_world::Direction;

/// Valid only inside the uninterrupted `Game::act` call that prepared it.
/// Kept private so callers cannot retain an action across world changes.
struct PreparedAction {
    actor: ActorId,
    at_tick: u64,
    kind: OutcomeKind,
    new_orientation: u8,
    ready_at: u64,
}

impl Game {
    /// Apply one valid action atomically. Invalid requests and blocked movement
    /// are free. Current effects occur immediately, followed by recovery time;
    /// recovery is not partially completed work and cannot be resumed.
    pub fn act(&mut self, id: ActorId, action: Action) -> Result<ActionOutcome, GameError> {
        let prepared = self.prepare_action(id, action)?;
        self.apply_action_effect(&prepared);
        Ok(self.finish_action(prepared))
    }

    /// Action-specific validity and timing are settled before any mutation.
    fn prepare_action(&self, id: ActorId, action: Action) -> Result<PreparedAction, GameError> {
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
                let direction = direction.rotated(actor.orientation);
                if matches!(direction, Direction::Up | Direction::Down)
                    && self.world.passage(actor.location, direction).is_none()
                {
                    return Err(GameError::Blocked);
                }
                let (to, _) = self
                    .reach(actor.location, direction)
                    .ok_or(GameError::Blocked)?;
                if !self.world.walkable(to) {
                    return Err(GameError::Blocked);
                }
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
                    movement_cost(actor.turn_ticks.get(), direction)?,
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
            Action::Move(direction) => {
                (actor.orientation
                    + self
                        .reach(actor.location, direction.rotated(actor.orientation))
                        .expect("validated move")
                        .1)
                    % 4
            }
            _ => actor.orientation,
        };
        let ready_at = at_tick
            .checked_add(duration)
            .ok_or(GameError::TimeExhausted)?;

        Ok(PreparedAction {
            actor: id,
            at_tick,
            kind,
            new_orientation,
            ready_at,
        })
    }

    /// Apply only the already validated effect; this phase must remain infallible.
    fn apply_action_effect(&mut self, prepared: &PreparedAction) {
        let PreparedAction {
            actor: id,
            kind,
            new_orientation,
            ..
        } = *prepared;
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
    }

    /// Commit recovery and select the next actor with the shared stable scheduler.
    fn finish_action(&mut self, prepared: PreparedAction) -> ActionOutcome {
        let PreparedAction {
            actor: id,
            at_tick,
            kind,
            ready_at,
            ..
        } = prepared;
        self.actors.get_mut(&id).expect("validated actor").ready_at = ready_at;
        let next_actor = self.next_actor().expect("acting actor is still present");
        self.tick = self.actors[&next_actor].ready_at;
        ActionOutcome {
            actor: id,
            at_tick,
            kind,
            next_actor,
            next_tick: self.tick,
        }
    }
}
