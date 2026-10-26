use super::*;
use crate::{Game, GameError};

/// Live execution witnesses, separate from the immutable paid plan operands.
struct BatchExecutionFacts<'a> {
    contact: Option<&'a crate::combat::contact::MeleeContact>,
    hold_damage: Option<super::HoldDamageExecution>,
    disclosure_origin: &'a mut Option<crate::event_history::EventOrigin>,
}

impl Game {
    /// Consume exactly the available outgoing sources captured by a performed
    /// resistance check, never recompute selection after primary publication.
    pub(super) fn consume_resistance_tokens(
        &mut self,
        tokens: &[crate::conditions::TokenReservation],
    ) -> Result<(), GameError> {
        let mut holders = std::collections::BTreeMap::<crate::ActorId, Vec<_>>::new();
        for token in tokens {
            holders.entry(token.holder).or_default().push(token.clone());
        }
        for (holder, selected) in holders {
            let mut outcome = self.actor_outcome(holder)?;
            let combat = outcome
                .state
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?;
            combat.reserve_tokens(&selected, holder)?;
            combat.consume_tokens(&selected, holder);
            self.apply_outcome(outcome)?;
        }
        Ok(())
    }
    pub(crate) fn plan_actor_available(&self, plan: &Plan) -> bool {
        (match plan {
            Plan::Projectile(plan) => plan.source.available(self, plan.actor),
            Plan::Equip {
                actor,
                item,
                slot,
                allocation,
                ..
            } => self.frozen_equip_available(*actor, *item, *slot, allocation),
            _ => true,
        }) && plan
            .as_ability()
            .is_none_or(|ability| self.frozen_weapon_available(ability.actor, &ability.weapon))
            && self
                .actors
                .get(&plan.actor())
                .is_some_and(|actor| actor.alive())
            && self.creature(plan.actor()).map_or_else(
                || {
                    matches!(
                        plan,
                        Plan::Equip { .. }
                            | Plan::Unequip { .. }
                            | Plan::Ignite { .. }
                            | Plan::Extinguish { .. }
                    ) || matches!(plan, Plan::Drink { effects, .. } if effects.is_empty())
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
                Plan::Ray(_) | Plan::Projectile(_) => true,
                Plan::Ability(ability) => self.target_in_reach_snapshot(
                    ability.actor,
                    ability.target,
                    &ability.reach,
                    sight,
                ),
                Plan::Equip { .. }
                | Plan::Unequip { .. }
                | Plan::Ignite { .. }
                | Plan::Extinguish { .. }
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
        let effective = self.effective_cancelled_strike(ability, strike)?;
        let strike = effective.strike.as_ref();
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
                resistance: None,
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
            // The unavailable/dead/detached target has no application or check.
            // The committed offensive source still emits its ordinary cue.
            let mut origin = self.capture_event_origin(outcome.actor, outcome.target);
            if matches!(
                ability.invocation.aim,
                super::AbilityAim::ContactTarget { .. }
            ) {
                origin.cap_contact_target();
            }
            let awareness =
                self.capture_unavailable_attack_awareness(outcome.actor, outcome.target)?;
            let receipt =
                self.publish_unavailable_primary(outcome.clone(), index as u16, origin)?;
            self.seal_participant_events()?;
            self.provoke_from_strike(&outcome, receipt, awareness)?;
            return Ok(outcome);
        }
        if self
            .content
            .rules()
            .ability(&ability.ability)
            .is_some_and(|definition| {
                definition.interaction == crate::rules::Interaction::HostileToTarget
            })
        {
            self.release_control_hold(ability.target)?;
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
        let awareness = self.capture_attack_awareness(ability.actor, ability.target)?;
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
                if effective.suppressed_riders.binary_search(&ordinal).is_ok() {
                    continue;
                }
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
        self.provoke_from_strike(&primary.outcome, primary_receipt, awareness)?;
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
            ResolutionPlan::ProjectileEncounter {
                projectile, target, ..
            } => (projectile.actor, target),
            ResolutionPlan::Effects { actor, target, .. }
            | ResolutionPlan::Exposure { actor, target, .. } => (actor, target),
            ResolutionPlan::Environment { target, .. } => (target, target),
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
        self.rules_rng = rng;
        self.consume_resistance_tokens(&result.resistance_tokens)?;
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
                self.rules_rng = rng;
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
                self.rules_rng = rng;
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
        // Live effects can refine a pure placeholder (for example room light
        // or Drain). Downstream receipt checks and rider accounting must use
        // the committed outcome, while preserving branch/check budget metadata.
        result.outcome = self.published_resolution(receipt)?.clone();
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
        disclosure: crate::event_history::SourceAbilityDisclosure,
        budget: &mut resolve::StrikeBudget,
        activity: &mut crate::lifecycle::DamageActivityGuard,
    ) -> Result<Outcome, GameError> {
        let super::contact_policy::RayResolution::Opposed {
            incoming,
            target_skill,
            ..
        } = &ray.resolution
        else {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        };
        crate::diagnostics::strike_resolution();
        if self
            .content
            .rules()
            .ability(&ray.ability)
            .is_some_and(|definition| {
                definition.interaction == crate::rules::Interaction::HostileToTarget
            })
        {
            self.release_control_hold(target)?;
        }
        let awareness = self.capture_attack_awareness(ray.actor, target)?;
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
                incoming,
                first_actual_check,
                &selected,
            )
            .map_err(|_| GameError::Rule(RuleError::InvalidPlan))?
            .checked_sub(i64::from(previous_checks))
            .ok_or(GameError::Rule(RuleError::InvalidPlan))?;
        let check = PlannedCheck {
            actor: crate::rules::CheckActor::Target,
            target_participates: true,
            bonus: super::ray::named_bonus(creature.derived(), *target_skill),
            threshold: CheckThreshold::Fixed { value: threshold },
            edge,
        };
        let plan = ResolutionPlan::RayEncounter {
            ray,
            target,
            check: &check,
        };
        let snapshot = Snapshot::capture(&creature)
            .with_claimed_check_tokens(&selected)
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
        // Publish resolution draws before ordinary contacts can consume more.
        self.rules_rng = rng;
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
        self.consume_resistance_tokens(&result.resistance_tokens)?;
        let origin = self
            .capture_event_origin(ray.actor, target)
            .with_ability_disclosure(disclosure);
        let receipt = self.apply_resolved_with_disclosure(
            result.outcome.clone(),
            crate::lifecycle::ResolutionPublication::Primary { strike: 0 },
            activity,
            origin,
        )?;
        self.seal_participant_events()?;
        self.provoke_from_strike(&result.outcome, receipt, awareness)?;
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
            relocation: self.relocation_resolution_input(plan, target)?,
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
    fn relocation_resolution_input(
        &self,
        plan: ResolutionPlan<'_>,
        target: crate::ActorId,
    ) -> Result<Option<RelocationInput>, GameError> {
        let Some(effect @ (PlannedEffect::TeleportRandom | PlannedEffect::TeleportToRayOrigin)) =
            plan.spatial_effect()
        else {
            return Ok(None);
        };
        let from = Placement::capture(self, target).ok_or(GameError::UnknownActor)?;
        let destinations = match effect {
            PlannedEffect::TeleportRandom => self.random_relocation_candidates(target)?,
            PlannedEffect::TeleportToRayOrigin => match plan {
                ResolutionPlan::RayEncounter { ray, .. } => self
                    .ray_relocation_destination(target, ray.origin, ray.direction)?
                    .into_iter()
                    .collect(),
                _ => return Err(GameError::Rule(RuleError::InvalidPlan)),
            },
            _ => unreachable!(),
        };
        Ok(Some(RelocationInput {
            from,
            destinations,
            mode: MovementMode::Forced,
        }))
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
            ResolutionPlan::RayEncounter { .. }
            | ResolutionPlan::ProjectileEncounter { .. }
            | ResolutionPlan::Effects { .. }
            | ResolutionPlan::Exposure { .. }
            | ResolutionPlan::Environment { .. } => None,
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
    /// One environmental check uses authored accuracy and the live recipient's
    /// ordinary Physical defense/protection, without a fabricated ability or caster.
    pub(crate) fn resolve_environmental_attack_in_transaction(
        &mut self,
        target: crate::ActorId,
        bonus: i32,
        effects: &[crate::rules::Effect],
        descriptors: &[crate::content_ids::DescriptorId],
    ) -> Result<bool, GameError> {
        if i64::from(bonus).unsigned_abs() > crate::limits::NUMERIC as u64 {
            return Err(GameError::Rule(RuleError::InvalidDefinition));
        }
        self.content
            .rules()
            .validate_effects(effects)
            .and_then(|_| {
                self.content
                    .rules()
                    .validate_payload_descriptors(descriptors)
            })
            .map_err(|_| GameError::Rule(RuleError::InvalidDefinition))?;
        self.settle_actor_time(target)?;
        let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
        if creature.health().dead() {
            return Ok(false);
        }
        let selected = creature
            .conditions()
            .qualifying_tokens(self.content.rules(), target, true);
        let edge = creature
            .derived()
            .environmental_incoming_edge(
                self.content.rules(),
                target,
                crate::rules::DefenseKind::Physical,
                &selected,
            )
            .map_err(|_| GameError::Rule(RuleError::InvalidDefinition))?;
        let check = PlannedCheck {
            actor: crate::rules::CheckActor::User,
            target_participates: true,
            bonus: i64::from(bonus),
            threshold: CheckThreshold::Fixed {
                value: i64::from(creature.derived().defenses.physical),
            },
            edge,
        };
        let effects = self.effect_plan(target, effects)?;
        let plan = ResolutionPlan::Environment {
            target,
            check: &check,
            effects: &effects,
            descriptors,
        };
        let snapshot = Snapshot::capture(&creature)
            .with_context(self.resolution_context(plan, target, target)?);
        let mut rng = self.rules_rng;
        let resolved = resolve::resolve_receipt(plan, &snapshot, &mut rng, &mut (), true)?;
        let outcome = resolved.outcome;
        let hit = outcome.check.is_some_and(|check| check.success);
        self.rules_rng = rng;
        if outcome.check.is_some() {
            // Actual environmental attack participation breaks a disguise on
            // hit or miss, without inventing a caster or an ability identity.
            self.reveal_guise(target)?;
        }
        if outcome.check.is_some() && !selected.is_empty() {
            let mut holder = self.actor_outcome(target)?;
            let combat = holder
                .state
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?;
            combat.reserve_tokens(&selected, target)?;
            combat.consume_tokens(&selected, target);
            self.apply_outcome(holder)?;
        }
        self.consume_resistance_tokens(&resolved.resistance_tokens)?;
        self.apply_outcome(outcome)?;
        Ok(hit)
    }
    /// No fabricated delivery attack: the payload has already reached this actor.
    pub(crate) fn apply_exposure_in_transaction(
        &mut self,
        actor: crate::ActorId,
        effects: &[crate::rules::Effect],
        descriptors: &[crate::content_ids::DescriptorId],
    ) -> Result<bool, GameError> {
        if effects.is_empty() {
            return Err(GameError::Rule(RuleError::InvalidDefinition));
        }
        self.content
            .rules()
            .validate_effects(effects)
            .and_then(|_| {
                self.content
                    .rules()
                    .validate_payload_descriptors(descriptors)
            })
            .map_err(|_| GameError::Rule(RuleError::InvalidDefinition))?;
        self.settle_actor_time(actor)?;
        let effects = self.effect_plan(actor, effects)?;
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let plan = ResolutionPlan::Exposure {
            actor,
            target: actor,
            effects: &effects,
            descriptors,
        };
        let snapshot =
            Snapshot::capture(&creature).with_context(self.resolution_context(plan, actor, actor)?);
        let mut rng = self.rules_rng;
        let resolved = resolve::resolve_receipt(plan, &snapshot, &mut rng, &mut (), true)?;
        self.rules_rng = rng;
        self.consume_resistance_tokens(&resolved.resistance_tokens)?;
        let receipt = self.apply_resolved_outcome(
            resolved.outcome,
            crate::lifecycle::ResolutionPublication::Standalone,
            &mut crate::lifecycle::DamageActivityGuard::default(),
        )?;
        Ok(self.published_resolution(receipt)?.result == OutcomeStatus::Applied)
    }
    /// The caller has validated and spent the exact finite source in this same
    /// transaction. The frozen effects do not depend on that deleted item.
    pub(crate) fn resolve_consumed_item(
        &mut self,
        preparation: &crate::combat::Preparation,
    ) -> Result<crate::detection_lifecycle::ItemCompletion, GameError> {
        let plan = &preparation.plan;
        let intention = preparation.intention;
        if !matches!(
            plan,
            Plan::Read { .. }
                | Plan::Drink { .. }
                | Plan::Eat { .. }
                | Plan::Zap {
                    delivery: super::DeviceDeliveryPlan::SelfEffect { .. },
                    ..
                }
        ) || matches!(plan, Plan::Zap { charges: 0, .. })
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
        let mut context = self.resolution_context(resolution, plan.actor(), plan.target())?;
        if matches!(plan, Plan::Read { .. }) {
            if let Some(input) = context.relocation.as_mut() {
                input.mode = MovementMode::Voluntary;
            }
        }
        let snapshot = Snapshot::capture(&target).with_context(context);
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
        let resolved = resolve::resolve_receipt(
            resolution,
            &snapshot,
            &mut rng,
            &mut observer,
            plan.on_success()
                .iter()
                .any(|effect| matches!(effect, PlannedEffect::ArrestDescent)),
        )?;
        let mut outcome = resolved.outcome;
        let flow = crate::survival_diagnostics::SurvivalFlows {
            incomplete: observer.incomplete,
            nourishment_offered: observer.offered,
            nourishment_wasted: observer.wasted,
            ..Default::default()
        };
        outcome.intention = intention;
        self.rules_rng = rng;
        self.consume_resistance_tokens(&resolved.resistance_tokens)?;
        let receipt = self.apply_resolved_outcome(
            outcome,
            crate::lifecycle::ResolutionPublication::Standalone,
            &mut crate::lifecycle::DamageActivityGuard::default(),
        )?;
        let outcome = self.published_resolution(receipt)?.clone();
        self.record_survival(plan.target(), &flow);
        crate::detection_lifecycle::ItemCompletion::new(outcome, receipt.chart_completion())
    }
}
