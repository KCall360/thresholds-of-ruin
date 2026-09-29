//! Internal action boundary shared by every actor and command source.
//!
//! Validation is read-only and includes recovery-time overflow checks. Effects
//! and scheduling run immediately afterward, with no callback, yield, or external
//! mutation between them. A prepared action is transient, never a saved job or a
//! client-visible promise. Persisted attack progress revalidates at each boundary.

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
    /// are free. Immediate actions apply effects before recovery; attacks commit
    /// wind-up progress. Recovery is not partially completed work and cannot resume.
    pub fn act(&mut self, id: ActorId, action: Action) -> Result<ActionOutcome, GameError> {
        let prepared = self.prepare_action(id, action)?;
        if let Some((expected, ai)) = self.choose_ai(id) {
            if expected != action {
                return Err(GameError::InvalidLocation);
            }
            self.combat.ai.insert(id, ai);
        }
        self.physics.impacts.clear();
        self.combat.input_boundaries.remove(&id);
        self.physics.displaced.clear();
        self.combat.events.clear();
        self.apply_action_effect(&prepared);
        Ok(self.finish_action(prepared))
    }

    /// Action-specific validity and timing are settled before any mutation.
    fn prepare_action(&self, id: ActorId, action: Action) -> Result<PreparedAction, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        self.next_item_id
            .checked_add(self.actors.len() as u64)
            .ok_or(GameError::IdentityExhausted)?;
        if actor.combat.as_ref().is_some_and(|c| c.hp == 0) {
            return Err(GameError::UnknownActor);
        }
        if self.next_actor() != Some(id) {
            return Err(GameError::NotActorsTurn);
        }
        let (kind, duration) = match action {
            Action::Attack { target } => {
                if !self.attack_available(id, target) {
                    return Err(GameError::InvalidLocation);
                }
                let c = actor.combat.as_ref().unwrap();
                let duration = c
                    .pending
                    .as_ref()
                    .filter(|p| p.target == target)
                    .map_or(c.spec.attack.wind_up, |p| p.remaining);
                self.tick
                    .checked_add(duration)
                    .and_then(|t| t.checked_add(c.spec.attack.recovery))
                    .ok_or(GameError::TimeExhausted)?;
                (OutcomeKind::AttackStarted { target }, duration)
            }
            Action::SetDoor { door, open } => {
                let location = self
                    .world
                    .door_location(door)
                    .ok_or(GameError::DoorUnavailable)?;
                let current = self.world.door(location).expect("existing door");
                let cells: Vec<_> = self.world.door_cells(location).collect();
                if current.open == open
                    || !self.door_reachable_from(actor.location, location)
                    || !self
                        .observe(id)?
                        .visible_cells
                        .iter()
                        .any(|c| cells.contains(&c.location))
                    || (!open && self.door_obstructed(location, current.height))
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
        if !matches!(
            kind,
            OutcomeKind::Waited | OutcomeKind::AttackStarted { .. }
        ) {
            if let Some(c) = actor.combat.as_mut() {
                c.pending = None;
            }
        }
        match kind {
            OutcomeKind::AttackStarted { target } => self.start_attack(id, target),
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
        self.resolve_attacks();
        self.check_objective();
        loop {
            if self.combat.outcome.terminal {
                break;
            }
            let decision = self.next_actor().map(|id| self.actors[&id].ready_at);
            let next_tick = match (decision, self.next_attack_tick()) {
                (Some(a), Some(b)) => a.min(b),
                (Some(t), None) | (None, Some(t)) => t,
                (None, None) => self.tick,
            };
            let previous_tick = self.tick;
            self.tick = self.advance_physics(next_tick);
            if self.tick != previous_tick {
                self.resolve_attacks();
                self.check_objective();
            }
            if self
                .next_actor()
                .is_none_or(|id| self.actors[&id].ready_at <= self.tick)
            {
                break;
            }
        }
        let next_actor = self.next_actor();
        ActionOutcome {
            actor: id,
            at_tick,
            kind,
            next_actor,
            next_tick: self.tick,
        }
    }
}
