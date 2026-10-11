//! Internal action boundary shared by every actor and command source.
//!
//! Validation is read-only and includes recovery-time overflow checks. Effects
//! and scheduling run immediately afterward, with no callback, yield, or external
//! mutation between them. A prepared action is transient, never a saved job or a
//! client-visible promise. Persisted attack progress revalidates at each boundary.

use crate::{
    movement_cost, Action, ActionOutcome, ActorId, Game, GameError, ItemLocation, OutcomeKind,
};

/// Valid only inside uninterrupted ordinary or queued AI action execution.
/// Kept private so callers cannot retain an action across world changes.
struct PreparedAction {
    intention: Option<crate::IntentionId>,
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
        self.act_with_intention(id, action, None)
    }

    pub(crate) fn act_with_intention(
        &mut self,
        id: ActorId,
        action: Action,
        intention: Option<crate::IntentionId>,
    ) -> Result<ActionOutcome, GameError> {
        self.act_with_context(id, action, intention, None)
    }

    pub(crate) fn act_with_context(
        &mut self,
        id: ActorId,
        action: Action,
        intention: Option<crate::IntentionId>,
        movement_context: Option<crate::MovementContext>,
    ) -> Result<ActionOutcome, GameError> {
        let mut prepared = self.prepare_action(id, action)?;
        if let Some(context) = movement_context {
            let OutcomeKind::Moved { from, to } = prepared.kind else {
                return Err(GameError::InvalidLocation);
            };
            if !context.matches(
                from,
                self.actors[&id].orientation,
                to,
                prepared.new_orientation,
            ) {
                return Err(GameError::InvalidLocation);
            }
        }
        self.bind_preparation_owner(&mut prepared, intention)?;
        let ai = if let Some((expected, ai)) = self.choose_ai(id) {
            if expected != action {
                return Err(GameError::InvalidLocation);
            }
            Some(ai)
        } else {
            None
        };
        Ok(self.commit_action(prepared, ai))
    }

    /// Choose and execute an admitted AI decision in one uninterrupted call.
    /// Every paid preparation receives its issued admission identity. Replay
    /// validates recorded actions through the same ordinary action boundary.
    pub(crate) fn act_ai_with_intention(
        &mut self,
        id: ActorId,
        intention: crate::IntentionId,
    ) -> Result<(Action, ActionOutcome), GameError> {
        if self.next_actor() != Some(id) {
            return Err(GameError::NotActorsTurn);
        }
        let (action, ai) = self.choose_ai(id).ok_or(GameError::InvalidLocation)?;
        let mut prepared = self.prepare_action(id, action)?;
        self.bind_preparation_owner(&mut prepared, Some(intention))?;
        Ok((action, self.commit_action(prepared, Some(ai))))
    }

    fn commit_action(
        &mut self,
        prepared: PreparedAction,
        ai: Option<crate::ai::Ai>,
    ) -> ActionOutcome {
        let id = prepared.actor;
        if let Some(ai) = ai {
            self.combat.ai.insert(id, ai);
        }
        self.physics.impacts.clear();
        self.combat.input_boundaries.remove(&id);
        self.physics.displaced.clear();
        self.combat.events.clear();
        self.count_arena_action();
        self.apply_action_effect(&prepared);
        self.sync_actor_lifecycle(id);
        self.finish_action(prepared)
    }

    fn bind_preparation_owner(
        &self,
        prepared: &mut PreparedAction,
        intention: Option<crate::IntentionId>,
    ) -> Result<(), GameError> {
        if let OutcomeKind::AbilityStarted { ability, target } = prepared.kind {
            let admission = intention.ok_or(GameError::InvalidIntention)?;
            let work = crate::Work::UseAbility { ability, target };
            let existing = self.preparation(prepared.actor).filter(|p| p.work == work);
            let charge = existing
                .and_then(|p| p.charge)
                .or(crate::abilities::ability_cost(ability))
                .ok_or(GameError::InvalidLocation)?;
            let owner = existing
                .and_then(|p| p.origin_intention())
                .unwrap_or(admission);
            let creature = self
                .creature(prepared.actor)
                .ok_or(GameError::InvalidLocation)?;
            if let Some(previous) = self
                .preparation(prepared.actor)
                .filter(|p| p.work != work)
                .and_then(|p| p.origin_intention())
            {
                // Replacement releases its unpaid hold in the same commit.
                let mut prospective = creature.costs().clone();
                prospective.cancel(previous);
                prospective
                    .validate_start(owner, charge)
                    .map_err(|_| GameError::InvalidLocation)?;
            } else {
                creature
                    .validate_cost(owner, charge)
                    .map_err(|_| GameError::InvalidLocation)?;
            }
        }
        prepared.intention = intention;
        Ok(())
    }

    /// Action-specific validity and timing are settled before any mutation.
    fn prepare_action(&self, id: ActorId, action: Action) -> Result<PreparedAction, GameError> {
        let actor = self.validate_action_actor(id)?;
        if self.next_actor() != Some(id) {
            return Err(GameError::NotActorsTurn);
        }
        self.prepare_available_action(id, actor, action)
    }

    pub(crate) fn validate_intention_action(
        &self,
        id: ActorId,
        action: Action,
    ) -> Result<(), GameError> {
        let actor = self.validate_action_actor(id)?;
        self.prepare_available_action(id, actor, action).map(|_| ())
    }

    fn validate_action_actor(&self, id: ActorId) -> Result<&crate::Actor, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        self.next_item_id
            .checked_add(self.actors.len() as u64)
            .ok_or(GameError::IdentityExhausted)?;
        if actor.combat.as_ref().is_some_and(|c| c.hp() == 0) {
            return Err(GameError::UnknownActor);
        }
        Ok(actor)
    }

    fn prepare_available_action(
        &self,
        id: ActorId,
        actor: &crate::Actor,
        action: Action,
    ) -> Result<PreparedAction, GameError> {
        let (kind, duration) = match action {
            Action::UseAbility { ability, target } => {
                let work = crate::Work::UseAbility { ability, target };
                self.work_duration(id, work)?;
                let (total, recovery) = self.work_timing(id, work);
                let duration = actor
                    .pending
                    .as_ref()
                    .filter(|p| p.work == work)
                    .map_or(total, |p| p.remaining);
                self.tick
                    .checked_add(duration)
                    .and_then(|tick| tick.checked_add(recovery))
                    .ok_or(GameError::TimeExhausted)?;
                (OutcomeKind::AbilityStarted { ability, target }, duration)
            }
            Action::Attack { .. }
            | Action::Equip { .. }
            | Action::Unequip { .. }
            | Action::Drink { .. } => {
                let work = match action {
                    Action::Attack { target } => crate::Work::Attack { target },
                    Action::Equip { item, slot } => crate::Work::Equip { item, slot },
                    Action::Unequip { item } => crate::Work::Unequip { item },
                    Action::Drink { item } => crate::Work::Drink { item },
                    _ => unreachable!(),
                };
                self.work_duration(id, work)?;
                let (total, recovery) = self.work_timing(id, work);
                let duration = actor
                    .pending
                    .as_ref()
                    .filter(|p| p.work == work)
                    .map_or(total, |p| p.remaining);
                self.tick
                    .checked_add(duration)
                    .and_then(|tick| tick.checked_add(recovery))
                    .ok_or(GameError::TimeExhausted)?;
                let kind = match work {
                    crate::Work::Attack { target } => OutcomeKind::AttackStarted { target },
                    _ => OutcomeKind::ItemStarted { work },
                };
                (kind, duration)
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
                            self.actors
                                .at(&self.world, at)
                                .keys()
                                .any(|other| *other != id)
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
            intention: None,
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
        let mut actor = self.actors.get_mut(&id).expect("actor validated above");
        actor.orientation = new_orientation;
        if !matches!(
            kind,
            OutcomeKind::Waited
                | OutcomeKind::AbilityStarted { .. }
                | OutcomeKind::AttackStarted { .. }
                | OutcomeKind::ItemStarted { .. }
        ) {
            actor.cancel_preparation();
        }
        if let OutcomeKind::Moved { to, .. } = kind {
            actor.location = to;
            actor.visited.insert(to.region);
        }
        drop(actor);
        match kind {
            OutcomeKind::AbilityStarted { ability, target } => self.start_work(
                id,
                crate::Work::UseAbility { ability, target },
                prepared.intention,
            ),
            OutcomeKind::AttackStarted { target } => {
                self.start_work(id, crate::Work::Attack { target }, prepared.intention)
            }
            OutcomeKind::ItemStarted { work } => self.start_work(id, work, prepared.intention),
            OutcomeKind::DoorChanged { door, open } => {
                let location = self.world.door_location(door).expect("validated door");
                self.world.set_door(location, open);
            }
            OutcomeKind::Moved { .. } => {}
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
        self.check_arena_end();
        self.advance_to_next_decision();
        let next_actor = self.next_actor();
        ActionOutcome {
            actor: id,
            at_tick,
            kind,
            next_actor,
            next_tick: self.tick,
        }
    }

    /// Advance through timer changes, physics and attacks until an actor decides.
    pub(crate) fn advance_to_next_decision(&mut self) {
        loop {
            if self.combat.outcome.terminal || !self.arena_execution_enabled() {
                break;
            }
            let decision = self.next_actor().map(|id| self.actors[&id].ready_at);
            let next_tick = decision
                .into_iter()
                .chain(self.next_attack_tick())
                .chain(self.next_creature_event_tick())
                .chain(self.arena_deadline())
                .min()
                .unwrap_or(self.tick);
            let previous_tick = self.tick;
            self.tick = self.advance_physics(next_tick);
            if self.tick != previous_tick {
                self.resolve_attacks();
                self.check_objective();
                self.check_arena_end();
            }
            if self.next_actor().map_or_else(
                // Timer wakes do not create a controller decision. When
                // everyone is preparing, continue to the next completion.
                || self.next_attack_tick().is_none(),
                |id| self.actors[&id].ready_at <= self.tick,
            ) {
                break;
            }
        }
    }
}
