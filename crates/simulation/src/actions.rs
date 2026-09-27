//! Internal action boundary shared by every actor and command source.
//!
//! Validation is read-only and includes recovery-time overflow checks. Effects
//! and scheduling run immediately afterward, with no callback, yield, or external
//! mutation between them. A prepared action is transient, never a saved job or a
//! client-visible promise. Future timed actions must revalidate at each boundary.

use crate::{
    movement_cost, Action, ActionOutcome, ActorId, Game, GameError, ItemLocation, OutcomeKind,
};

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
        self.physics.impacts.clear();
        self.physics.displaced.clear();
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
                let (to, _) = self.actor_translation(id, direction).ok_or_else(|| {
                    if self
                        .reach(actor.location, direction.rotated(actor.orientation))
                        .is_some_and(|(at, _)| {
                            self.actors.iter().any(|(other, a)| {
                                *other != id
                                    && self
                                        .body_cells(a.location, a.orientation, &a.body)
                                        .is_some_and(|cells| cells.iter().any(|(p, _)| *p == at))
                            })
                        })
                    {
                        GameError::Occupied
                    } else {
                        GameError::Blocked
                    }
                })?;
                (
                    OutcomeKind::Moved {
                        from: actor.location,
                        to,
                    },
                    movement_cost(actor.turn_ticks.get(), direction)?,
                )
            }
            Action::Take { item, quantity } | Action::Drop { item, quantity } => {
                let taking = matches!(action, Action::Take { .. });
                (
                    self.prepare_transfer(id, item, quantity, taking)?,
                    actor.turn_ticks.get().div_ceil(2),
                )
            }
            Action::Wait => (OutcomeKind::Waited, actor.turn_ticks.get()),
        };
        let at_tick = self.tick;
        let new_orientation = match action {
            Action::Move(direction) => {
                self.actor_translation(id, direction)
                    .expect("validated move")
                    .1
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
            OutcomeKind::Taken {
                item,
                result,
                quantity,
            } => {
                self.apply_transfer(item, result, quantity, ItemLocation::Carried(id));
            }
            OutcomeKind::Dropped {
                item,
                result,
                quantity,
            } => {
                let location = self.actors[&id].location;
                self.apply_transfer(item, result, quantity, ItemLocation::Ground(location));
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
        let next_tick = self.actors[&next_actor].ready_at;
        self.advance_physics(next_tick);
        self.tick = next_tick;
        ActionOutcome {
            actor: id,
            at_tick,
            kind,
            next_actor,
            next_tick: self.tick,
        }
    }
}
