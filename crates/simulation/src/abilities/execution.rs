use super::*;
use crate::{Game, GameError};
impl Game {
    pub(crate) fn plan_actor_available(&self, plan: &Plan) -> bool {
        self.actors
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
                            .as_ability()
                            .is_none_or(|plan| creature.derived().abilities.contains(&plan.ability))
                },
            )
    }
    fn plan_available(&self, plan: &Plan, sight: Option<&[tor_world::SightCell]>) -> bool {
        self.plan_actor_available(plan)
            && match plan {
                Plan::Respec(plan) => self.respec_environment_safe(plan.basis.actor),
                Plan::Ability(ability) => self.target_in_reach_snapshot(
                    ability.actor,
                    ability.target,
                    &ability.reach,
                    sight,
                ),
                Plan::Equip { .. }
                | Plan::Unequip { .. }
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
            self.event_log.push(crate::combat::CombatEvent::Resolved {
                outcome: outcome.clone(),
                strike: Some(index as u16),
            });
            return Ok(outcome);
        }
        let mut budget = resolve::StrikeBudget::new(strike);
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        let (mut primary, primary_receipt) = self.resolve_batch_in_transaction(
            ResolutionPlan::Strike { ability, strike },
            &mut budget,
            &mut activity,
            intention,
            crate::lifecycle::ResolutionPublication::Primary {
                strike: index as u16,
            },
            contact.as_ref(),
        )?;
        if let Some(branch) = primary.selected_branch {
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
                    contact.as_ref(),
                )?;
                primary.outcome.target_rider_participated =
                    self.merge_committed_rider_participation(primary_receipt, rider_receipt)?;
            }
        }
        self.provoke_from_strike(&primary.outcome, index as u16)?;
        Ok(primary.outcome)
    }
    fn resolve_batch_in_transaction(
        &mut self,
        plan: ResolutionPlan<'_>,
        budget: &mut resolve::StrikeBudget,
        activity: &mut crate::lifecycle::DamageActivityGuard,
        intention: Option<crate::IntentionId>,
        site: crate::lifecycle::ResolutionPublication,
        contact: Option<&crate::combat::contact::MeleeContact>,
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
            ResolutionPlan::Effects { actor, target, .. } => (actor, target),
        };
        let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
        let mut context = self.resolution_context(plan, actor, target)?;
        context.spatial = self.spatial_resolution_input(plan, contact);
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
        let receipt = self.apply_resolved_outcome(result.outcome.clone(), site, activity)?;
        self.rules_rng = rng;
        Ok((result, receipt))
    }
    pub(crate) fn resolution_context(
        &self,
        plan: ResolutionPlan<'_>,
        actor: crate::ActorId,
        target: crate::ActorId,
    ) -> Result<ResolutionContext, GameError> {
        Ok(ResolutionContext {
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
            ResolutionPlan::Effects { .. } => None,
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
        if !matches!(plan, Plan::Drink { .. } | Plan::Eat { .. })
            || !matches!(
                preparation.item_payment,
                crate::supplies::ItemPayment::Settled
            )
            || !self.plan_actor_available(plan)
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
        let mut outcome = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
        {
            resolve(resolution, &snapshot, &mut rng, observer)
        } else {
            resolve(resolution, &snapshot, &mut rng, &mut ())
        }?;
        outcome.intention = intention;
        self.apply_outcome(outcome.clone())?;
        self.rules_rng = rng;
        Ok(outcome)
    }
}
