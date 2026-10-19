use super::*;
use crate::{Game, GameError};

/// Live execution witnesses, separate from the immutable paid plan operands.
struct BatchExecutionFacts<'a> {
    contact: Option<&'a crate::combat::contact::MeleeContact>,
    hold_damage: Option<super::HoldDamageExecution>,
    disclosure_origin: &'a mut Option<crate::event_history::EventOrigin>,
}

impl Game {
    pub(crate) fn plan_actor_available(&self, plan: &Plan) -> bool {
        plan.as_ability()
            .is_none_or(|ability| self.frozen_weapon_available(ability.actor, &ability.weapon))
            && self
                .actors
                .get(&plan.actor())
                .is_some_and(|actor| actor.alive())
            && self.creature(plan.actor()).map_or_else(
                || {
                    matches!(plan, Plan::Equip { .. } | Plan::Unequip { .. })
                        || matches!(plan, Plan::Drink { effects, .. } if effects.is_empty())
                },
                |creature| {
                    !creature.health().dead()
                        && match plan {
                            Plan::Respec(plan) => plan.basis.build_epoch == creature.build_epoch(),
                            _ => true,
                        }
                        && plan
                            .ability_id()
                            .is_none_or(|ability| creature.derived().abilities.contains(ability))
                },
            )
    }
    fn plan_available(&self, plan: &Plan, sight: Option<&[tor_world::SightCell]>) -> bool {
        self.plan_actor_available(plan)
            && match plan {
                Plan::Escape(plan) => self.escape_plan_available(plan),
                Plan::Respec(plan) => self.respec_environment_safe(plan.basis.actor),
                Plan::Ray(_) => true,
                Plan::Ability(ability) => self.target_in_reach_snapshot(
                    ability.actor,
                    ability.target,
                    &ability.reach,
                    sight,
                ),
                Plan::Equip { .. }
                | Plan::Unequip { .. }
                | Plan::Read { .. }
                | Plan::Zap { .. }
                | Plan::Drink { .. }
                | Plan::Eat { .. } => {
                    self.item_work_available(plan.actor(), &plan.work())
                        && (plan.on_success().is_empty()
                            || self.target_in_reach_snapshot(
                                plan.actor(),
                                plan.actor(),
                                &crate::rules::Reach::SelfOnly,
                                sight,
                            ))
                }
            }
    }
    /// A paid prefix owns its frozen tail even if its old target has left reach.
    /// Funding is progress-aware: the first strike settles the only cost hold.
    pub(crate) fn preparation_still_available(
        &self,
        preparation: &crate::combat::Preparation,
        sight: Option<&[tor_world::SightCell]>,
    ) -> bool {
        preparation.progress_valid()
            && self.preparation_items_valid(preparation, false)
            && (preparation.has_prefix()
                || preparation.plan.cost().is_none_or(|cost| {
                    preparation.hold.is_some_and(|owner| {
                        self.creature(preparation.plan.actor())
                            .is_some_and(|creature| creature.validate_cost(owner, cost).is_ok())
                    })
                }))
            && if preparation.has_prefix() {
                self.plan_actor_available(&preparation.plan)
            } else {
                self.plan_available(&preparation.plan, sight)
            }
    }
    /// Resolve one due strike and its ordered riders with one residual damage budget.
    /// The caller has already advanced progress, so self-effects see the new cursor.
    pub(crate) fn resolve_strike(
        &mut self,
        ability: &AbilityPlan,
        index: usize,
        intention: Option<crate::IntentionId>,
        progress: &mut crate::combat::PreparationProgress,
    ) -> Result<Outcome, GameError> {
        let strike = ability
            .strikes
            .get(index)
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        crate::diagnostics::strike_resolution();
        let needs_contact = strike
            .on_success
            .iter()
            .chain(&strike.on_failure)
            .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
            .any(|effect| matches!(effect, PlannedEffect::PushTargetOneCell { .. }));
        // Each primary owns one actual hit witness. Ordered riders borrow that
        // witness; later primaries select a new live one. This is not a plan operand.
        let contact = needs_contact.then(|| {
            if self.actor_frozen(ability.actor) || self.actor_frozen(ability.target) {
                return crate::combat::contact::MeleeContact::Unreachable;
            }
            self.scene(ability.actor)
                .ok()
                .map_or(crate::combat::contact::MeleeContact::Unreachable, |scene| {
                    self.melee_contact_in_scene(ability.actor, ability.target, &scene)
                })
        });
        let reachable = contact.as_ref().map_or_else(
            || self.target_in_reach_snapshot(ability.actor, ability.target, &ability.reach, None),
            crate::combat::contact::MeleeContact::reachable,
        );
        if !reachable {
            let outcome = Outcome {
                actor: ability.actor,
                target: ability.target,
                ability: Some(ability.ability.clone()),
                intention,
                result: OutcomeStatus::Miss,
                check: None,
                participation: super::AttemptedParticipation::blocked(
                    None,
                    super::UnselectedReason::UnavailableTarget,
                ),
                target_rider_participated: false,
                effects: vec![],
            };
            // An unavailable/dead/detached target has no mutation or RNG draw.
            let mut origin = self.capture_event_origin(outcome.actor, outcome.target);
            if matches!(
                ability.invocation.aim,
                super::AbilityAim::ContactTarget { .. }
            ) {
                origin.cap_contact_target();
            }
            self.push_event(
                crate::combat::CombatEvent::Resolved {
                    outcome: outcome.clone(),
                    strike: Some(index as u16),
                },
                Some(origin),
            )?;
            return Ok(outcome);
        }
        let hold_damage = progress.hold_binding().and_then(|binding| {
            (ability.hold_damage_increment > 0
                && strike
                    .on_success
                    .iter()
                    .any(|effect| matches!(effect, PlannedEffect::HoldDamage { .. }))
                && self.valid_linked_hold(ability.actor, ability.target, binding.id))
            .then(|| self.linked_hold(ability.target, binding.id))
            .flatten()
            .and_then(|link| link.escalation.checked_add(ability.hold_damage_increment))
            .filter(|bonus| *bonus <= crate::linked_hold::HOLD_ESCALATION_MAX)
            .map(|bonus| super::HoldDamageExecution {
                id: binding.id,
                bonus,
            })
        });
        if progress.hold_binding().is_some_and(|binding| {
            self.valid_linked_hold(ability.actor, ability.target, binding.id)
        }) && ability.hold_damage_increment > 0
            && strike
                .on_success
                .iter()
                .any(|effect| matches!(effect, PlannedEffect::HoldDamage { .. }))
            && hold_damage.is_none()
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let mut budget = resolve::StrikeBudget::new(strike);
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        let mut disclosure_origin = None;
        let (mut primary, primary_receipt) = self.resolve_batch_in_transaction(
            ResolutionPlan::Strike { ability, strike },
            &mut budget,
            &mut activity,
            intention,
            crate::lifecycle::ResolutionPublication::Primary {
                strike: index as u16,
            },
            BatchExecutionFacts {
                contact: contact.as_ref(),
                hold_damage,
                disclosure_origin: &mut disclosure_origin,
            },
        )?;
        if strike.qualifying_primary_check()
            && primary.selected_branch == Some(crate::rules::RiderTrigger::Success)
            && primary
                .outcome
                .check
                .is_some_and(|check| check.actor == crate::rules::CheckActor::User && check.success)
        {
            progress.record_successful_primary(index);
        }
        if let Some(branch) = primary.selected_branch {
            if let Some(execution) = hold_damage {
                self.advance_linked_hold(
                    ability.actor,
                    ability.target,
                    execution.id,
                    ability.hold_damage_increment,
                )?;
            }
            let primary_success = branch == crate::rules::RiderTrigger::Success;
            progress.record_created_hold(
                primary_receipt.created_hold(),
                crate::sequence::HoldCreationSite {
                    strike: index as u16,
                    rider: None,
                    primary_success,
                },
            );
            for (ordinal, rider) in strike.riders.iter().enumerate() {
                if rider.trigger != branch {
                    continue;
                }
                if !self.creature(ability.actor).is_some_and(|actor| {
                    !actor.health().dead() && actor.derived().abilities.contains(&ability.ability)
                }) || !self
                    .creature(ability.target)
                    .is_some_and(|target| !target.health().dead())
                {
                    break;
                }
                if !progress.attempt_conditional_rider(ability, index, ordinal) {
                    continue;
                }
                crate::diagnostics::rider_resolution();
                let (_, rider_receipt) = self.resolve_batch_in_transaction(
                    ResolutionPlan::Rider {
                        ability,
                        rider,
                        ordinal: ordinal as u16,
                    },
                    &mut budget,
                    &mut activity,
                    intention,
                    crate::lifecycle::ResolutionPublication::Rider {
                        primary: primary_receipt,
                        strike: index as u16,
                        ordinal: ordinal as u16,
                    },
                    BatchExecutionFacts {
                        contact: contact.as_ref(),
                        hold_damage: None,
                        disclosure_origin: &mut disclosure_origin,
                    },
                )?;
                progress.record_created_hold(
                    rider_receipt.created_hold(),
                    crate::sequence::HoldCreationSite {
                        strike: index as u16,
                        rider: Some(ordinal as u16),
                        primary_success,
                    },
                );
                primary.outcome.target_rider_participated =
                    self.merge_committed_rider_participation(primary_receipt, rider_receipt)?;
            }
        }
        self.seal_participant_events()?;
        self.provoke_from_strike(&primary.outcome, primary_receipt)?;
        Ok(primary.outcome)
    }
    fn resolve_batch_in_transaction(
        &mut self,
        plan: ResolutionPlan<'_>,
        budget: &mut resolve::StrikeBudget,
        activity: &mut crate::lifecycle::DamageActivityGuard,
        intention: Option<crate::IntentionId>,
        site: crate::lifecycle::ResolutionPublication,
        facts: BatchExecutionFacts<'_>,
    ) -> Result<
        (
            resolve::BatchResolution,
            crate::lifecycle::PublishedResolution,
        ),
        GameError,
    > {
        let (actor, target) = match plan {
            ResolutionPlan::Ability(ability)
            | ResolutionPlan::Strike { ability, .. }
            | ResolutionPlan::Rider { ability, .. } => (ability.actor, ability.target),
            ResolutionPlan::RayEncounter { ray, target, .. } => (ray.actor, target),
            ResolutionPlan::Effects { actor, target, .. } => (actor, target),
        };
        let mut disclosure_origin =
            self.reuse_event_origin(actor, target, facts.disclosure_origin.as_ref());
        if matches!(plan,
            ResolutionPlan::Ability(ability) | ResolutionPlan::Strike { ability, .. } | ResolutionPlan::Rider { ability, .. }
                if matches!(ability.invocation.aim, super::AbilityAim::ContactTarget { .. }))
        {
            disclosure_origin.cap_contact_target();
        }
        *facts.disclosure_origin = Some(disclosure_origin.clone());
        let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
        let mut context = self.resolution_context(plan, actor, target)?;
        context.spatial = self.spatial_resolution_input(plan, facts.contact);
        context.hold_damage = facts.hold_damage;
        if plan.contextual_conditions(EffectRecipient::Target, &self.content.rules) {
            let placement = context
                .spatial
                .map(|input| match input.result {
                    MovementResult::Moved { to, .. } => to,
                    _ => input.placement,
                })
                .or_else(|| Placement::capture(self, target))
                .ok_or(GameError::UnknownActor)?;
            context.target_recovery_context =
                Some(self.recovery_context_at_placement(target, placement)?);
        }
        let snapshot = Snapshot::capture(&creature).with_context(context);
        let mut rng = self.rules_rng;
        let mut result = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
        {
            resolve::resolve_batch(plan, &snapshot, &mut rng, observer, budget)
        } else {
            resolve::resolve_batch(plan, &snapshot, &mut rng, &mut (), budget)
        }?;
        result.outcome.intention = intention;
        let receipt = if let Some(theft) = result.deferred_theft.take() {
            let transfer = self.transfer_theft(actor, target, &theft.selection, &mut rng)?;
            if let Some(transfer) = transfer {
                let event_start = self.event_log.len();
                let relocation = theft
                    .relocation
                    .as_ref()
                    .map(|policy| self.relocate_after_theft(actor, policy, &transfer, &mut rng))
                    .transpose()?
                    .flatten();
                let consequences = self.take_event_suffix(event_start)?;
                result.outcome.effects.push(ResolvedEffect::Theft {
                    quantity: transfer.quantity(),
                    relocation,
                });
                result.outcome.result = result.outcome.canonical_result();
                let receipt = self.apply_resolved_theft(
                    result.outcome.clone(),
                    site,
                    activity,
                    transfer,
                    disclosure_origin,
                )?;
                self.append_event_suffix(consequences.0, consequences.1)?;
                receipt
            } else {
                self.apply_resolved_with_disclosure(
                    result.outcome.clone(),
                    site,
                    activity,
                    disclosure_origin,
                )?
            }
        } else {
            self.apply_resolved_with_disclosure(
                result.outcome.clone(),
                site,
                activity,
                disclosure_origin,
            )?
        };
        self.rules_rng = rng;
        Ok((result, receipt))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve_ray_encounter(
        &mut self,
        ray: &RayPlan,
        target: crate::ActorId,
        previous_checks: u16,
        threshold: i64,
        first_actual_check: bool,
        intention: Option<crate::IntentionId>,
        budget: &mut resolve::StrikeBudget,
        activity: &mut crate::lifecycle::DamageActivityGuard,
    ) -> Result<Outcome, GameError> {
        crate::diagnostics::strike_resolution();
        self.settle_actor_time(target)?;
        let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
        let rules = &self.content.rules;
        let selected = creature
            .conditions()
            .qualifying_tokens(rules, target, false);
        let edge = creature
            .derived()
            .ray_recipient_edge(
                rules,
                target,
                ray.actor,
                &ray.ability,
                &ray.incoming,
                first_actual_check,
                &selected,
            )
            .map_err(|_| GameError::Rule(RuleError::InvalidPlan))?
            .checked_sub(i64::from(previous_checks))
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        let check = PlannedCheck {
            actor: crate::rules::CheckActor::Target,
            target_participates: true,
            bonus: super::ray::named_bonus(creature.derived(), ray.target_skill),
            threshold: CheckThreshold::Fixed { value: threshold },
            edge,
        };
        let plan = ResolutionPlan::RayEncounter {
            ray,
            target,
            check: &check,
        };
        let snapshot = Snapshot::capture(&creature)
            .with_context(self.resolution_context(plan, ray.actor, target)?);
        let mut rng = self.rules_rng;
        let mut result = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
        {
            resolve::resolve_batch(plan, &snapshot, &mut rng, observer, budget)
        } else {
            resolve::resolve_batch(plan, &snapshot, &mut rng, &mut (), budget)
        }?;
        result.outcome.intention = intention;
        // Whole-payload immunity does not perform a check and cannot spend a
        // recipient's next-check token or increase this ray's repeat count.
        if result.outcome.check.is_some() && !selected.is_empty() {
            let mut holder = self.actor_outcome(target)?;
            let combat = holder
                .state
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?;
            combat.reserve_tokens(&selected, ray.actor)?;
            combat.consume_tokens(&selected, ray.actor);
            self.apply_outcome(holder)?;
        }
        let origin = self.capture_event_origin(ray.actor, target);
        let receipt = self.apply_resolved_with_disclosure(
            result.outcome.clone(),
            crate::lifecycle::ResolutionPublication::Primary { strike: 0 },
            activity,
            origin,
        )?;
        self.rules_rng = rng;
        self.seal_participant_events()?;
        self.provoke_from_strike(&result.outcome, receipt)?;
        Ok(result.outcome)
    }
    pub(crate) fn resolution_context(
        &self,
        plan: ResolutionPlan<'_>,
        actor: crate::ActorId,
        target: crate::ActorId,
    ) -> Result<ResolutionContext, GameError> {
        Ok(ResolutionContext {
            hold_damage: None,
            spatial: None,
            target_recovery_context: if plan
                .contextual_conditions(EffectRecipient::Target, &self.content.rules)
            {
                Some(self.recovery_source_context(target)?)
            } else {
                None
            },
            source_recovery_context: if plan
                .contextual_conditions(EffectRecipient::Source, &self.content.rules)
            {
                Some(self.recovery_source_context(actor)?)
            } else {
                None
            },
            observation: plan
                .observes_public_plan()
                .then(|| self.capture_public_plan_observation(actor, target))
                .flatten()
                .map(tor_world::Shared::new),
            source: if plan.source_effects() {
                Some(ApplicationSnapshot {
                    creature: self
                        .creature(actor)
                        .ok_or(GameError::UnknownActor)?
                        .into_owned(),
                    observed_plan: self
                        .actors
                        .get(&actor)
                        .ok_or(GameError::UnknownActor)?
                        .public_plan
                        .clone(),
                })
            } else {
                None
            },
        })
    }
    pub(crate) fn spatial_resolution_input(
        &self,
        plan: ResolutionPlan<'_>,
        contact: Option<&crate::combat::contact::MeleeContact>,
    ) -> Option<SpatialInput> {
        let ability = match plan {
            ResolutionPlan::Ability(ability)
            | ResolutionPlan::Strike { ability, .. }
            | ResolutionPlan::Rider { ability, .. } => Some(ability),
            ResolutionPlan::RayEncounter { .. } | ResolutionPlan::Effects { .. } => None,
        };
        let spatial = ability.and_then(|ability| match plan.spatial_effect()? {
            PlannedEffect::StepSelfOneCell => self.step_input(ability),
            PlannedEffect::PushTargetOneCell { direction } => {
                self.push_input(ability, *direction, contact)
            }
            _ => None,
        });
        spatial
    }
    /// The caller has validated and spent the exact finite source in this same
    /// transaction. The frozen effects do not depend on that deleted item.
    pub(crate) fn resolve_consumed_item(
        &mut self,
        preparation: &crate::combat::Preparation,
    ) -> Result<Outcome, GameError> {
        let plan = &preparation.plan;
        let intention = preparation.intention;
        if !matches!(
            plan,
            Plan::Read { .. } | Plan::Drink { .. } | Plan::Eat { .. }
        ) || !matches!(
            preparation.item_payment,
            crate::supplies::ItemPayment::Settled
        ) || !self.plan_actor_available(plan)
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let target = self
            .creature(plan.target())
            .ok_or(GameError::UnknownActor)?;
        let resolution = plan
            .resolution()
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        let snapshot = Snapshot::capture(&target).with_context(self.resolution_context(
            resolution,
            plan.actor(),
            plan.target(),
        )?);
        let mut rng = self.rules_rng;
        let mut ignored = ();
        let next: &mut dyn crate::resolution_diagnostics::ResolutionObserver = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
            .map_or(
                &mut ignored as &mut dyn crate::resolution_diagnostics::ResolutionObserver,
                |observer| observer,
            );
        let mut observer = crate::survival_diagnostics::NourishmentObserver {
            next,
            enabled: self.survival_diagnostics.is_some(),
            incomplete: false,
            offered: 0,
            wasted: 0,
        };
        let mut outcome = resolve(resolution, &snapshot, &mut rng, &mut observer)?;
        let flow = crate::survival_diagnostics::SurvivalFlows {
            incomplete: observer.incomplete,
            nourishment_offered: observer.offered,
            nourishment_wasted: observer.wasted,
            ..Default::default()
        };
        outcome.intention = intention;
        self.apply_outcome(outcome.clone())?;
        self.record_survival(plan.target(), &flow);
        self.rules_rng = rng;
        Ok(outcome)
    }
}
