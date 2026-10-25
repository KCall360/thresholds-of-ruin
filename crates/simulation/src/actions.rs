//! Internal action boundary shared by every actor and command source.
//!
//! Validation is read-only and includes recovery-time overflow checks. Effects
//! and scheduling run immediately afterward, with no callback, yield, or external
//! mutation between them. A prepared action is transient, never a saved job or a
//! client-visible promise. Persisted attack progress revalidates at each boundary.

use crate::{Action, ActionOutcome, ActorId, Game, GameError, OutcomeKind};

/// Valid only inside uninterrupted ordinary or queued AI action execution.
/// Kept private so callers cannot retain an action across world changes.
struct PreparedAction {
    intention: Option<crate::IntentionId>,
    actor: ActorId,
    at_tick: u64,
    kind: OutcomeKind,
    new_orientation: u8,
    ordinary_movement: Option<tor_world::Direction>,
    requested_destination: Option<(tor_world::Location, u8)>,
    touches: Vec<ActorId>,
    ready_at: u64,
    plan: Option<crate::abilities::Plan>,
    transfer: Option<crate::items::PreparedTransfer>,
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
            let matches = if let Some((to, orientation)) = self.requested_destination {
                context.matches(actor.location, actor.orientation, to, orientation)
            } else {
                match self.kind {
                    OutcomeKind::Moved { from, to } => {
                        context.matches(from, actor.orientation, to, self.new_orientation)
                    }
                    _ => self
                        .plan
                        .as_ref()
                        .and_then(|plan| plan.as_ability())
                        .and_then(|plan| plan.step)
                        .is_some_and(|step| step.context == context),
                }
            };
            if !matches {
                return Err(GameError::InvalidLocation);
            }
        }
        Ok(())
    }
}

impl Game {
    /// Apply one valid action atomically. Invalid original requests are free;
    /// a confused stay or obstructed sample pays the validated movement cost.
    /// Immediate actions apply effects before recovery; attacks commit
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
        let decision = self.choose_executable_ai(id)?;
        if decision
            .as_ref()
            .is_some_and(|decision| decision.action != action)
        {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let (ai, plan, numeric_budget_refusal) = decision.map_or((None, None, false), |decision| {
            (
                Some((decision.ai, decision.behavior)),
                decision.plan,
                decision.numeric_budget_refusal,
            )
        });
        if self.chosen_ai_escape(id, &action, ai.as_ref().map(|(ai, _)| ai)) {
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
        let outcome = self.commit_action(prepared, ai)?;
        self.record_projectile_numeric_refusal(id, numeric_budget_refusal);
        Ok(outcome)
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
        let crate::ai::AiDecision {
            action,
            ai,
            behavior,
            plan,
            numeric_budget_refusal,
        } = self
            .choose_executable_ai(id)?
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
                self.cancel_preparation_with_step(id, old, direction, None, Some((ai, behavior)))?,
            ));
        }
        let mut prepared = self.prepare_action(id, action.clone(), plan, ActionEntry::Fresh)?;
        self.bind_preparation_owner(&mut prepared, Some(intention))?;
        crate::diagnostics::ai_precommit_ordinary_scenes(
            crate::diagnostics::work_counts().ordinary_scenes - ordinary_scenes,
        );
        let outcome = self.commit_action(prepared, Some((ai, behavior)))?;
        self.record_projectile_numeric_refusal(id, numeric_budget_refusal);
        Ok((action, outcome))
    }

    /// Execution-only contention gate. The public decision preview sees physical
    /// candidates; a failed finite preflight changes nothing. An ordinary Wait
    /// is then validated and recorded through the same action boundary. Replay
    /// repeats this exact gate before comparing the recorded action.
    fn choose_executable_ai(
        &self,
        actor: ActorId,
    ) -> Result<Option<crate::ai::AiDecision>, GameError> {
        let Some(mut decision) = self.choose_ai(actor) else {
            return Ok(None);
        };
        if self.condition_blocks_action(actor, &decision.action) {
            decision.action = Action::Wait;
            decision.plan = None;
            return Ok(Some(decision));
        }
        if decision.plan.is_none() {
            if let Action::UseAbility {
                id,
                invocation,
                supplies,
            } = &decision.action
            {
                if matches!(
                    invocation.aim,
                    crate::abilities::AbilityAim::DirectionalProjectile { .. }
                ) {
                    // This is the one request already selected from disclosed
                    // structure. Never probe another unknown item after refusal.
                    match self.ability_plan_with_supplies(actor, id, *invocation, supplies.clone())
                    {
                        Ok(plan) => decision.plan = Some(plan),
                        Err(GameError::Rule(crate::abilities::RuleError::NumericBudget)) => {
                            decision.action = Action::Wait;
                            decision.numeric_budget_refusal = true;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }
        }
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
        Ok(Some(decision))
    }

    fn record_projectile_numeric_refusal(&mut self, actor: ActorId, refused: bool) {
        if refused {
            self.record_survival(
                actor,
                &crate::survival_diagnostics::SurvivalFlows {
                    projectile_numeric_budget_refusals: 1,
                    ..Default::default()
                },
            );
        }
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
        mut prepared: PreparedAction,
        ai: Option<(crate::ai::Ai, crate::behavior::Behavior)>,
    ) -> Result<ActionOutcome, GameError> {
        let mut candidate = self.clone();
        candidate.initialize_discovery()?;
        let id = prepared.actor;
        candidate.begin_action_boundary(id)?;
        candidate.redirect_confused_movement(&mut prepared);
        candidate.on_action_committed()?;
        candidate.apply_action_effect(&prepared)?;
        if let Some((ai, behavior)) = ai {
            candidate.combat.ai.insert(id, ai);
            candidate.install_behavior(id, behavior)?;
            candidate.reconcile_recovery_contexts()?;
        }
        candidate.sync_actor_lifecycle(id)?;
        let outcome = candidate.finish_action(prepared)?;
        candidate.reconcile_passive_discovery()?;
        candidate.reconcile_awareness()?;
        candidate.reconcile_control_contacts()?;
        candidate.settle_aggravation()?;
        candidate.settle_periodic_searches()?;
        candidate.seal_participant_events()?;
        *self = candidate;
        Ok(outcome)
    }

    /// The original action and its admitted movement context have already been
    /// validated. Draw only on the transaction candidate: any later failure
    /// rolls back the sample together with the movement, time and other effects.
    fn redirect_confused_movement(&mut self, prepared: &mut PreparedAction) {
        use tor_world::Direction;
        let Some(requested) = prepared.ordinary_movement else {
            return;
        };
        if requested.delta().2 != 0 {
            return;
        }
        let actor = &self.actors[&prepared.actor];
        if self
            .world
            .is_stair(actor.location, requested.rotated(actor.orientation))
        {
            return;
        }
        let Some(chance) = actor
            .locomotion()
            .confused
            .filter(|chance| chance.numerator > 0)
        else {
            return;
        };
        if crate::dice::die(&mut self.rules_rng, chance.denominator) > chance.numerator {
            return;
        }
        // Fixed row-major actor-local 3x3, including stay. Geometry never changes
        // this distribution, and a blocked sample receives no replacement draw.
        const SAMPLES: [Option<Direction>; 9] = [
            Some(Direction::NorthWest),
            Some(Direction::North),
            Some(Direction::NorthEast),
            Some(Direction::West),
            None,
            Some(Direction::East),
            Some(Direction::SouthWest),
            Some(Direction::South),
            Some(Direction::SouthEast),
        ];
        let selected = SAMPLES[usize::from(crate::dice::die(&mut self.rules_rng, 9) - 1)];
        prepared.touches.clear();
        let actor = &self.actors[&prepared.actor];
        let from = actor.location;
        let orientation = actor.orientation;
        if let Some((to, orientation)) = selected
            .and_then(|direction| self.actor_physical_translation(prepared.actor, direction))
        {
            prepared.kind = OutcomeKind::Moved { from, to };
            prepared.new_orientation = orientation;
        } else {
            prepared.kind = OutcomeKind::Waited;
            prepared.new_orientation = orientation;
            if let Some((_, contacts)) = selected
                .and_then(|direction| self.movement_guise_contacts(prepared.actor, direction))
            {
                prepared.touches = contacts;
            }
        }
        // ready_at retains the requested normal move's cost, including its
        // diagonal cost, even when the sample stays or cannot be traversed.
    }

    /// Shared publication boundary for an ordinary action or one atomic escape.
    pub(crate) fn begin_action_boundary(&mut self, actor: ActorId) -> Result<(), GameError> {
        if !self.host_region_continuation {
            self.begin_retrieval_transaction();
        }
        self.physics.impacts.clear();
        self.combat.input_boundaries.remove(&actor);
        self.physics.displaced.clear();
        self.begin_event_batch()
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
            if (plan.needs_item_payment()
                || plan
                    .as_ability()
                    .is_some_and(|ability| ability.ambush.is_some()))
                && intention.is_none_or(|id| !self.intention_issued(id))
            {
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
        if self.condition_blocks_action(id, &action) {
            return Err(GameError::ActorBusy);
        }
        if matches!(
            &action,
            Action::Purchase { .. }
                | Action::ChooseGrowthSource { .. }
                | Action::ChooseIntellectExpertise { .. }
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
        let mut transfer = None;
        let mut touches = Vec::new();
        let mut requested_destination = None;
        let (kind, duration) = match action.clone() {
            Action::ExamineTrap { trap } => {
                let location = self
                    .world
                    .trap_location(trap)
                    .ok_or(GameError::ItemUnavailable)?;
                if actor.pending.is_some()
                    || !self.feature_known(id, tor_world::ConcealedFeatureId::Trap(trap))
                    || !self.trap_reachable(id, location)
                {
                    return Err(GameError::ItemUnavailable);
                }
                (OutcomeKind::TrapExamined { trap }, 0)
            }
            Action::Identify { choice, item } => {
                self.validate_identification(id, choice, item)?;
                (OutcomeKind::IdentificationResolved { choice, item }, 0)
            }
            Action::ChooseGrowthSource { basis, source } => {
                self.preview_player_growth_source(id, &basis, &source)?;
                (
                    OutcomeKind::GrowthSourceChosen { basis, source },
                    actor
                        .action_duration()
                        .apply_bounded(self.content.rules.document().growth.recovery)
                        .map_err(GameError::Build)?,
                )
            }
            Action::ChooseIntellectExpertise { basis, choice } => {
                self.preview_player_intellect_expertise(id, &basis, &choice)?;
                (
                    OutcomeKind::IntellectExpertiseChosen { basis, choice },
                    actor
                        .action_duration()
                        .apply_bounded(self.content.rules.document().advancement.purchase_recovery)
                        .map_err(GameError::Build)?,
                )
            }
            Action::Purchase { basis, purchase } => {
                self.preview_player_purchase(id, &basis, &purchase)?;
                (
                    OutcomeKind::Purchased { basis, purchase },
                    actor
                        .action_duration()
                        .apply_bounded(self.content.rules.document().advancement.purchase_recovery)
                        .map_err(GameError::Build)?,
                )
            }
            Action::Escape { .. }
            | Action::Respec { .. }
            | Action::UseAbility { .. }
            | Action::Equip { .. }
            | Action::Unequip { .. } | Action::Ignite { .. } | Action::Extinguish { .. }
            | Action::Read { .. }
            | Action::Zap { .. }
            | Action::Drink { .. }
            | Action::Eat { .. } => {
                let work = match action.clone() {
                    Action::Escape { hold } => crate::Work::Escape { hold },
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
                    Action::Ignite { item } => crate::Work::Ignite { item },
                    Action::Extinguish { item } => crate::Work::Extinguish { item },
                    Action::Drink { item } => crate::Work::Drink { item },
                    Action::Read { item } => crate::Work::Read { item },
                    Action::Zap { item, aim } => crate::Work::Zap { item, aim },
                    Action::Eat { item } => crate::Work::Eat { item },
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
                    crate::Work::Escape { hold } => OutcomeKind::EscapeStarted { hold: *hold },
                    crate::Work::Respec {
                        basis, replacement, ..
                    } => OutcomeKind::RespecStarted {
                        basis: basis.clone(),
                        replacement: replacement.clone(),
                    },
                    crate::Work::UseAbility {
                        id: ability,
                        invocation,
                        ..
                    } if invocation.kind() == crate::abilities::InvocationKind::ActorTarget => {
                        OutcomeKind::AbilityStarted {
                            reference: invocation.reference(),
                            id: ability.clone(),
                            name: self
                                .content
                                .rules
                                .ability(ability)
                                .ok_or(GameError::Rule(
                                    crate::abilities::RuleError::UnknownAbility,
                                ))?
                                .name
                                .clone(),
                            target: invocation.target(id),
                        }
                    }
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
                    || !self.observe(id)?.visible_cells.iter().any(|c| {
                        cells.contains(&c.location)
                            && c.door.is_some_and(|visible| visible.id == door)
                    })
                    || (!open && self.door_obstructed(location, current.height))
                {
                    return Err(GameError::DoorUnavailable);
                }
                (
                    OutcomeKind::DoorChanged { door, open },
                    actor.action_duration_ticks(actor.turn_ticks.get())?,
                )
            }
            Action::Move(direction) if self.is_terminal_exit(id, actor.location, direction) => (
                OutcomeKind::Escaped {
                    item: self.prepare_terminal_escape(id, direction)?,
                },
                0,
            ),
            Action::Move(direction) => {
                if let Some((to, orientation)) = self.actor_translation(id, direction) {
                    requested_destination = Some((to, orientation));
                    (
                        OutcomeKind::Moved {
                            from: actor.location,
                            to,
                        },
                        actor.movement_duration(direction)?,
                    )
                } else if let Some((destination, contacts)) =
                    self.movement_guise_contacts(id, direction)
                {
                    requested_destination = Some(destination);
                    touches = contacts;
                    (OutcomeKind::Waited, actor.movement_duration(direction)?)
                } else {
                    return Err(
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
                        },
                    );
                }
            }
            Action::TouchApparentObject { subject, quantity } => {
                self.validate_apparent_touch(id, subject, quantity)?;
                (
                    OutcomeKind::ApparentObjectTouched { subject },
                    actor.action_duration_ticks(actor.turn_ticks.get().div_ceil(2))?,
                )
            }
            Action::Take { item, quantity } | Action::Drop { item, quantity } => {
                let taking = matches!(action, Action::Take { .. });
                let captured = self.prepare_transfer(id, item, quantity, taking)?;
                let outcome = captured.outcome(taking);
                transfer = Some(captured);
                (
                    outcome,
                    actor.action_duration_ticks(actor.turn_ticks.get().div_ceil(2))?,
                )
            }
            Action::Wait => (
                OutcomeKind::Waited,
                actor.action_duration_ticks(actor.turn_ticks.get())?,
            ),
        };
        let at_tick = self.tick;
        let new_orientation = match action {
            Action::Move(_) if !matches!(kind, OutcomeKind::Moved { .. }) => actor.orientation,
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

        let ordinary_movement = match action {
            Action::Move(direction) if !matches!(kind, OutcomeKind::Escaped { .. }) => {
                Some(direction)
            }
            _ => None,
        };
        Ok(PreparedAction {
            intention: None,
            actor: id,
            at_tick,
            kind,
            new_orientation,
            ready_at,
            plan: planned,
            transfer,
            ordinary_movement,
            requested_destination,
            touches,
        })
    }

    /// Apply captured effects on the transaction candidate; reject stale references.
    fn apply_action_effect(&mut self, prepared: &PreparedAction) -> Result<(), GameError> {
        let id = prepared.actor;
        let kind = prepared.kind.clone();
        if let OutcomeKind::Escaped { item } = kind {
            if self.prepare_terminal_escape(id, tor_world::Direction::Up)? != item {
                return Err(GameError::ItemUnavailable);
            }
        }
        if let OutcomeKind::GrowthSourceChosen { basis, source } = &kind {
            let (_, outcome) = self.prepare_player_growth_source(id, basis, source)?;
            return self.publish_actor_outcome(outcome);
        }
        if let OutcomeKind::TrapExamined { trap } = kind {
            return self.reveal_feature_family(id, tor_world::ConcealedFeatureId::Trap(trap));
        }
        if let OutcomeKind::IdentificationResolved { choice, item } = kind {
            return self.resolve_identification(id, choice, item);
        }
        if let OutcomeKind::IntellectExpertiseChosen { basis, choice } = &kind {
            let (_, outcome) = self.prepare_player_intellect_expertise(id, basis, choice)?;
            return self.publish_actor_outcome(outcome);
        }
        if let OutcomeKind::Purchased { basis, purchase } = &kind {
            // Recheck the frozen proposal inside the current action transaction.
            // No saved full-state preview can overwrite earned runtime changes.
            let (_, outcome) = self.prepare_player_purchase(id, basis, purchase)?;
            return self.publish_actor_outcome(outcome);
        }
        // Compare and move only already-free units before our own cancellation
        // releases claims. The enclosing action candidate rolls back both phases.
        // Capture the actor outcome afterward so inherited memory is retained.
        if matches!(
            kind,
            OutcomeKind::Taken { .. } | OutcomeKind::Dropped { .. } | OutcomeKind::Crumbled { .. }
        ) {
            self.apply_prepared_transfer(
                prepared
                    .transfer
                    .as_ref()
                    .ok_or(GameError::ItemUnavailable)?,
            )?;
        }
        let new_orientation = prepared.new_orientation;
        let mut outcome = self.actor_outcome(id)?;
        let actor = &mut outcome.state;
        if !matches!(kind, OutcomeKind::Waited) {
            actor.readied_ambush.retire();
        }
        actor.orientation = new_orientation;
        if prepared.ordinary_movement.is_some()
            || !matches!(
                kind,
                OutcomeKind::Waited
                    | OutcomeKind::AbilityStarted { .. }
                    | OutcomeKind::ItemStarted { .. }
                    | OutcomeKind::RespecStarted { .. }
                    | OutcomeKind::EscapeStarted { .. }
            )
        {
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
            OutcomeKind::IntellectExpertiseChosen { .. } => {
                unreachable!("Intellect expertise uses one actor publication")
            }
            OutcomeKind::Purchased { .. } => unreachable!("purchase uses one actor publication"),
            OutcomeKind::TrapExamined { .. } => {
                unreachable!("trap examination uses one actor publication")
            }
            OutcomeKind::IdentificationResolved { .. } => {
                unreachable!("identification uses one actor publication")
            }
            OutcomeKind::AbilityStarted { .. }
            | OutcomeKind::ItemStarted { .. }
            | OutcomeKind::EscapeStarted { .. }
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
                let affected = self.hold_geometry_candidates(self.world.door_cells(location));
                self.world.set_door(location, open);
                self.reconcile_holds_for_actors(affected)?;
            }
            OutcomeKind::ApparentObjectTouched { subject } => {
                self.touch_readied_ambush(subject, id)?;
            }
            OutcomeKind::Moved { .. } => {}
            OutcomeKind::Escaped { item } => self.commit_terminal_escape(id, item)?,
            OutcomeKind::Taken { .. }
            | OutcomeKind::Dropped { .. }
            | OutcomeKind::Crumbled { .. } => {}
            OutcomeKind::Waited => {}
        }
        self.refresh_light_boundary();
        for subject in &prepared.touches {
            self.touch_readied_ambush(*subject, id)?;
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
        if !matches!(kind, OutcomeKind::Escaped { .. }) && !self.region_transition_pending() {
            self.resolve_preparations()?;
            self.check_run_policy()?;
            self.advance_to_next_decision()?;
        }
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
            if self.region_transition_pending() {
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
            if self.region_transition_pending() {
                break;
            }
            if self.tick != previous_tick {
                self.resolve_preparations()?;
                self.check_run_policy()?;
            }
            if self.next_actor().map_or_else(
                // Timer wakes do not create a controller decision. When
                // everyone is preparing, continue to the next completion.
                || self.next_attack_tick().is_none() && self.next_creature_event_tick().is_none(),
                |id| self.actors[&id].ready_at <= self.tick,
            ) {
                break;
            }
        }
        Ok(())
    }
}
