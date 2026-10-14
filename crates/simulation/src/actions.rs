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
    plan: Option<crate::abilities::Plan>,
}

/// Fresh actions never inherit paused respec progress. Only the separately
/// validated ResumePreparation queue variant enters through the resume boundary.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionEntry {
    Fresh,
    ResumePreparation,
}

impl PreparedAction {
    fn validate_movement_context(
        &self,
        actor: &crate::Actor,
        context: Option<crate::MovementContext>,
    ) -> Result<(), GameError> {
        if let Some(context) = context {
            let matches = match self.kind {
                OutcomeKind::Moved { from, to } => {
                    context.matches(from, actor.orientation, to, self.new_orientation)
                }
                _ => self
                    .plan
                    .as_ref()
                    .and_then(|plan| plan.as_ability())
                    .and_then(|plan| plan.step)
                    .is_some_and(|step| step.context == context),
            };
            if !matches {
                return Err(GameError::InvalidLocation);
            }
        }
        Ok(())
    }
}

impl Game {
    /// Apply one valid action atomically. Invalid requests and blocked movement
    /// are free. Immediate actions apply effects before recovery; attacks commit
    /// wind-up progress. Recovery is not partially completed work and cannot resume.
    ///
    /// This low-level API does not issue an intention identity. Resource-paid
    /// abilities and finite-item work (including Drink) require `admit_intention`
    /// followed by `execute_next_intention`; direct attempts reject atomically.
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
        self.act_at_entry(id, action, intention, movement_context, ActionEntry::Fresh)
    }

    pub(crate) fn resume_preparation_with_intention(
        &mut self,
        id: ActorId,
        work: crate::Work,
        intention: crate::IntentionId,
    ) -> Result<ActionOutcome, GameError> {
        if self.is_ai(id)
            || !self.preparation(id).is_some_and(|pending| {
                pending.intention == Some(intention)
                    && pending.plan.work() == work
                    && !pending.active
            })
        {
            return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
        }
        self.act_at_entry(
            id,
            work.action(),
            Some(intention),
            None,
            ActionEntry::ResumePreparation,
        )
    }

    fn act_at_entry(
        &mut self,
        id: ActorId,
        action: Action,
        intention: Option<crate::IntentionId>,
        movement_context: Option<crate::MovementContext>,
        entry: ActionEntry,
    ) -> Result<ActionOutcome, GameError> {
        let decision = self.choose_executable_ai(id);
        if decision
            .as_ref()
            .is_some_and(|decision| decision.action != action)
        {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let (ai, plan) =
            decision.map_or((None, None), |decision| (Some(decision.ai), decision.plan));
        if self.chosen_ai_escape(id, &action, ai.as_ref()) {
            if self.next_actor() != Some(id) {
                return Err(GameError::NotActorsTurn);
            }
            let Action::Move(direction) = action else {
                unreachable!("chosen escape")
            };
            let old = self
                .preparation(id)
                .expect("chosen escape prefix")
                .intention;
            return self.cancel_preparation_with_step(id, old, direction, movement_context, ai);
        }
        let mut prepared = self.prepare_action(id, action.clone(), plan, entry)?;
        prepared.validate_movement_context(&self.actors[&id], movement_context)?;
        self.bind_preparation_owner(&mut prepared, intention)?;
        self.commit_action(prepared, ai)
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
        let ordinary_scenes = crate::diagnostics::work_counts().ordinary_scenes;
        let crate::ai::AiDecision { action, ai, plan } = self
            .choose_executable_ai(id)
            .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
        if self.chosen_ai_escape(id, &action, Some(&ai)) {
            let Action::Move(direction) = action else {
                unreachable!("chosen escape")
            };
            let old = self
                .preparation(id)
                .expect("chosen escape prefix")
                .intention;
            crate::diagnostics::ai_precommit_ordinary_scenes(
                crate::diagnostics::work_counts().ordinary_scenes - ordinary_scenes,
            );
            return Ok((
                action,
                self.cancel_preparation_with_step(id, old, direction, None, Some(ai))?,
            ));
        }
        let mut prepared = self.prepare_action(id, action.clone(), plan, ActionEntry::Fresh)?;
        self.bind_preparation_owner(&mut prepared, Some(intention))?;
        crate::diagnostics::ai_precommit_ordinary_scenes(
            crate::diagnostics::work_counts().ordinary_scenes - ordinary_scenes,
        );
        Ok((action, self.commit_action(prepared, Some(ai))?))
    }

    /// Execution-only contention gate. The public decision preview sees physical
    /// candidates; a failed finite preflight changes nothing. An ordinary Wait
    /// is then validated and recorded through the same action boundary. Replay
    /// repeats this exact gate before comparing the recorded action.
    fn choose_executable_ai(&self, actor: ActorId) -> Option<crate::ai::AiDecision> {
        let mut decision = self.choose_ai(actor)?;
        let unavailable = if let Some(plan) = decision
            .plan
            .as_ref()
            .filter(|plan| plan.needs_item_payment())
        {
            let resuming = self
                .preparation(actor)
                .is_some_and(|pending| pending.plan == *plan);
            !resuming && self.item_allocation(plan) == Err(GameError::ItemUnavailable)
        } else if let Action::Take { item, quantity } = &decision.action {
            // Loot selection sees physical items only. A privately held source
            // may be temporarily unavailable at this actual execution boundary.
            // Overflow and other invariant errors are never converted to Wait.
            self.prepare_transfer(actor, *item, *quantity, true) == Err(GameError::ItemUnavailable)
        } else {
            false
        };
        if unavailable {
            decision.action = Action::Wait;
            decision.plan = None;
        }
        Some(decision)
    }

    fn chosen_ai_escape(
        &self,
        actor: ActorId,
        action: &Action,
        ai: Option<&crate::ai::Ai>,
    ) -> bool {
        matches!(action, Action::Move(_))
            && ai.is_some_and(|ai| ai.state == crate::ai::State::Flee)
            && self
                .preparation(actor)
                .is_some_and(|pending| !pending.active && pending.has_prefix())
    }

    fn commit_action(
        &mut self,
        prepared: PreparedAction,
        ai: Option<crate::ai::Ai>,
    ) -> Result<ActionOutcome, GameError> {
        let mut candidate = self.clone();
        let id = prepared.actor;
        if let Some(ai) = ai {
            candidate.combat.ai.insert(id, ai);
        }
        candidate.begin_action_boundary(id);
        candidate.on_action_committed()?;
        candidate.apply_action_effect(&prepared)?;
        candidate.sync_actor_lifecycle(id)?;
        let outcome = candidate.finish_action(prepared)?;
        *self = candidate;
        Ok(outcome)
    }

    /// Shared publication boundary for an ordinary action or one atomic escape.
    pub(crate) fn begin_action_boundary(&mut self, actor: ActorId) {
        self.physics.impacts.clear();
        self.combat.input_boundaries.remove(&actor);
        self.physics.displaced.clear();
        self.event_log.clear();
    }

    /// Caller owns the cancellation transaction. Admission and effects are the
    /// ordinary movement path; only readiness adds previously committed work.
    pub(crate) fn escape_step_in_transaction(
        &mut self,
        actor: ActorId,
        direction: tor_world::Direction,
        movement_context: Option<crate::MovementContext>,
    ) -> Result<ActionOutcome, GameError> {
        let state = self.validate_action_actor(actor)?;
        let debt = state.ready_at.saturating_sub(self.tick);
        let mut prepared = self.prepare_available_action(
            actor,
            state,
            Action::Move(direction),
            None,
            ActionEntry::Fresh,
        )?;
        prepared.validate_movement_context(state, movement_context)?;
        prepared.ready_at = prepared
            .ready_at
            .checked_add(debt)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        self.on_action_committed()?;
        self.apply_action_effect(&prepared)?;
        self.sync_actor_lifecycle(actor)?;
        self.finish_action(prepared)
    }

    fn bind_preparation_owner(
        &self,
        prepared: &mut PreparedAction,
        intention: Option<crate::IntentionId>,
    ) -> Result<(), GameError> {
        if let Some(plan) = &prepared.plan {
            if plan.needs_item_payment() && intention.is_none_or(|id| !self.intention_issued(id)) {
                return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention));
            }
            if let Some(charge) = plan.cost() {
                let admission =
                    intention.ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
                let existing = self
                    .preparation(prepared.actor)
                    .filter(|p| p.plan.work() == plan.work());
                if existing.is_some_and(|pending| pending.has_prefix()) {
                    prepared.intention = intention;
                    return Ok(());
                }
                let owner = existing.and_then(|p| p.hold).unwrap_or(admission);
                let creature = self
                    .creature(prepared.actor)
                    .ok_or(GameError::UnknownActor)?;
                let mut costs = creature.costs().clone();
                if let Some(previous) = self
                    .preparation(prepared.actor)
                    .filter(|p| p.plan.work() != plan.work())
                    .and_then(|p| p.hold)
                {
                    costs.cancel(previous);
                }
                costs
                    .validate_start(owner, charge)
                    .map_err(|_| GameError::Rule(crate::abilities::RuleError::Unfunded))?;
            }
        }
        prepared.intention = intention;
        Ok(())
    }

    /// Action-specific validity and timing are settled before any mutation.
    fn prepare_action(
        &self,
        id: ActorId,
        action: Action,
        plan: Option<crate::abilities::Plan>,
        entry: ActionEntry,
    ) -> Result<PreparedAction, GameError> {
        let actor = self.validate_action_actor(id)?;
        if self.next_actor() != Some(id) {
            return Err(GameError::NotActorsTurn);
        }
        self.prepare_available_action(id, actor, action, plan, entry)
    }

    pub(crate) fn validate_intention_action(
        &self,
        id: ActorId,
        action: Action,
    ) -> Result<(), GameError> {
        let actor = self.validate_action_actor(id)?;
        self.prepare_available_action(id, actor, action, None, ActionEntry::Fresh)
            .map(|_| ())
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
        captured_plan: Option<crate::abilities::Plan>,
        entry: ActionEntry,
    ) -> Result<PreparedAction, GameError> {
        if matches!(
            &action,
            Action::Purchase { .. } | Action::ChooseGrowthSource { .. }
        ) && !self.player_advancement_ready(id)
        {
            return Err(GameError::ActorBusy);
        }
        if let Action::Respec { .. } = &action {
            if actor.ready_at > self.tick
                || self.actor_frozen(id)
                || actor.pending.as_ref().is_some_and(|pending| {
                    entry != ActionEntry::ResumePreparation
                        || pending.active
                        || pending.plan.work().action() != action
                        || !matches!(pending.plan, crate::abilities::Plan::Respec(_))
                })
            {
                return Err(GameError::ActorBusy);
            }
        }
        if actor.pending.as_ref().is_some_and(|pending| {
            pending.has_prefix()
                && !matches!(action, Action::Wait)
                && pending.plan.work().action() != action
        }) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let mut planned = None;
        let (kind, duration) = match action.clone() {
            Action::ChooseGrowthSource { basis, source } => {
                self.preview_player_growth_source(id, &basis, &source)?;
                (
                    OutcomeKind::GrowthSourceChosen { basis, source },
                    self.content.rules.document().growth.recovery,
                )
            }
            Action::Purchase { basis, purchase } => {
                self.preview_player_purchase(id, &basis, &purchase)?;
                (
                    OutcomeKind::Purchased { basis, purchase },
                    self.content.rules.document().advancement.purchase_recovery,
                )
            }
            Action::Respec { .. }
            | Action::UseAbility { .. }
            | Action::Equip { .. }
            | Action::Unequip { .. }
            | Action::Drink { .. } => {
                let work = match action.clone() {
                    Action::Respec {
                        basis,
                        replacement,
                        supplies,
                    } => crate::Work::Respec {
                        basis,
                        replacement,
                        supplies,
                    },
                    Action::UseAbility {
                        id,
                        invocation,
                        supplies,
                    } => crate::Work::UseAbility {
                        id,
                        invocation,
                        supplies,
                    },
                    Action::Equip { item, slot } => crate::Work::Equip { item, slot },
                    Action::Unequip { item } => crate::Work::Unequip { item },
                    Action::Drink { item } => crate::Work::Drink { item },
                    _ => return Err(GameError::Schedule(crate::ScheduleError::InvalidIntention)),
                };
                let plan = match captured_plan {
                    Some(plan) if plan.actor() == id && plan.work() == work => plan,
                    Some(_) => {
                        return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
                    }
                    None => self.plan_work(id, work.clone())?,
                };
                let duration = actor
                    .pending
                    .as_ref()
                    .filter(|p| p.plan.work() == work)
                    .map_or(plan.preparation(), |p| p.remaining);
                let commitment = if let Some(pending) = actor
                    .pending
                    .as_ref()
                    .filter(|pending| pending.plan.work() == work)
                {
                    pending.completion_delay(self.tick)?
                } else {
                    duration
                        .checked_add(
                            plan.as_ability()
                                .and_then(|plan| plan.strikes.last())
                                .map_or(0, |strike| strike.offset),
                        )
                        .and_then(|value| value.checked_add(plan.recovery()))
                        .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?
                };
                self.tick
                    .checked_add(commitment)
                    .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
                let kind = match &work {
                    crate::Work::Respec {
                        basis, replacement, ..
                    } => OutcomeKind::RespecStarted {
                        basis: basis.clone(),
                        replacement: replacement.clone(),
                    },
                    crate::Work::UseAbility {
                        id: ability,
                        invocation: crate::abilities::AbilityInvocation::ActorTarget { target },
                        ..
                    } => OutcomeKind::AbilityStarted {
                        id: ability.clone(),
                        name: self
                            .content
                            .rules
                            .ability(ability)
                            .ok_or(GameError::Rule(crate::abilities::RuleError::UnknownAbility))?
                            .name
                            .clone(),
                        target: *target,
                    },
                    _ => OutcomeKind::ItemStarted { work: work.clone() },
                };
                planned = Some(plan);
                (kind, duration)
            }
            Action::SetDoor { door, open } => {
                let location = self
                    .world
                    .door_location(door)
                    .ok_or(GameError::DoorUnavailable)?;
                let current = self
                    .world
                    .door(location)
                    .ok_or(GameError::DoorUnavailable)?;
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
                    .ok_or(GameError::Blocked)?
                    .1
            }
            _ => actor.orientation,
        };
        let ready_at = at_tick
            .checked_add(duration)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;

        Ok(PreparedAction {
            intention: None,
            actor: id,
            at_tick,
            kind,
            new_orientation,
            ready_at,
            plan: planned,
        })
    }

    /// Apply captured effects on the transaction candidate; reject stale references.
    fn apply_action_effect(&mut self, prepared: &PreparedAction) -> Result<(), GameError> {
        let id = prepared.actor;
        let kind = prepared.kind.clone();
        if let OutcomeKind::GrowthSourceChosen { basis, source } = &kind {
            let (_, outcome) = self.prepare_player_growth_source(id, basis, source)?;
            return self.publish_actor_outcome(outcome);
        }
        if let OutcomeKind::Purchased { basis, purchase } = &kind {
            // Recheck the frozen proposal inside the current action transaction.
            // No saved full-state preview can overwrite earned runtime changes.
            let (_, outcome) = self.prepare_player_purchase(id, basis, purchase)?;
            return self.publish_actor_outcome(outcome);
        }
        let new_orientation = prepared.new_orientation;
        let mut outcome = self.actor_outcome(id)?;
        let actor = &mut outcome.state;
        actor.orientation = new_orientation;
        if !matches!(
            kind,
            OutcomeKind::Waited
                | OutcomeKind::AbilityStarted { .. }
                | OutcomeKind::ItemStarted { .. }
                | OutcomeKind::RespecStarted { .. }
        ) {
            actor.cancel_preparation();
        }
        if let OutcomeKind::Moved { to, .. } = kind {
            actor.motion.active_fall = None;
            actor.location = to;
            actor.visited.insert(to.region);
        }
        self.apply_outcome(outcome)?;
        match kind {
            OutcomeKind::GrowthSourceChosen { .. } => {
                unreachable!("growth source uses one actor publication")
            }
            OutcomeKind::Purchased { .. } => unreachable!("purchase uses one actor publication"),
            OutcomeKind::AbilityStarted { .. }
            | OutcomeKind::ItemStarted { .. }
            | OutcomeKind::RespecStarted { .. } => {
                let plan = prepared
                    .plan
                    .clone()
                    .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
                self.start_work(plan, prepared.intention)?;
            }
            OutcomeKind::DoorChanged { door, open } => {
                let location = self
                    .world
                    .door_location(door)
                    .ok_or(GameError::DoorUnavailable)?;
                self.world.set_door(location, open);
            }
            OutcomeKind::Moved { .. } => {}
            OutcomeKind::Taken {
                item,
                result,
                quantity,
            } => {
                self.apply_transfer(item, result, quantity, ItemLocation::Carried(id))?;
            }
            OutcomeKind::Dropped {
                item,
                result,
                quantity,
            } => {
                let location = self.actors[&id].location;
                self.apply_transfer(item, result, quantity, ItemLocation::Ground(location))?;
            }
            OutcomeKind::Waited => {}
        }
        Ok(())
    }

    /// Commit recovery and select the next actor with the shared stable scheduler.
    fn finish_action(&mut self, prepared: PreparedAction) -> Result<ActionOutcome, GameError> {
        let PreparedAction {
            actor: id,
            at_tick,
            kind,
            ready_at,
            ..
        } = prepared;
        self.actors
            .get_mut(&id)
            .ok_or(GameError::UnknownActor)?
            .ready_at = ready_at;
        self.resolve_preparations()?;
        self.check_run_policy()?;
        self.advance_to_next_decision()?;
        let next_actor = self.next_actor();
        Ok(ActionOutcome {
            actor: id,
            at_tick,
            kind,
            next_actor,
            next_tick: self.tick,
        })
    }

    /// Advance through timer changes, physics and attacks until an actor decides.
    pub(crate) fn advance_to_next_decision(&mut self) -> Result<(), GameError> {
        loop {
            if self.population_entry_pending() {
                break;
            }
            self.reconcile_recovery_contexts()?;
            if self.run_outcome().terminal || !self.arena_execution_enabled() {
                break;
            }
            let decision = self.next_actor().map(|id| self.actors[&id].ready_at);
            let next_tick = decision
                .into_iter()
                .chain(self.next_attack_tick())
                .chain(self.next_creature_event_tick())
                .chain(self.run_deadline())
                .min()
                .unwrap_or(self.tick);
            let previous_tick = self.tick;
            self.tick = self.advance_physics(next_tick)?;
            if self.population_entry_pending() {
                break;
            }
            if self.tick != previous_tick {
                self.resolve_preparations()?;
                self.check_run_policy()?;
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
        Ok(())
    }
}
