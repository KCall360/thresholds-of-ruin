//! Ordered scheduling uses frozen plans and actor-local lifecycle publication.
use crate::combat::{CombatEvent, SequenceEnd};
use crate::{Game, GameError, ScheduleError, Work};

impl Game {
    /// Caller owns the outer transaction. Actor IDs and authored ordinals break ties.
    pub(crate) fn resolve_preparations(&mut self) -> Result<(), GameError> {
        let ids: Vec<_> = self.actors.keys().copied().collect();
        crate::diagnostics::preparation_candidates(ids.len());
        for id in ids {
            if self.run_outcome().terminal {
                break;
            }
            if self.actor_frozen(id) {
                continue;
            }
            self.ensure_preparation_residency(id)?;
            if self.region_transition_pending() {
                return Ok(());
            }
            // Includes equal-offset rows without rescanning every actor per rider.
            for _ in 0..=crate::limits::STRIKES {
                // An ordinary effect may relocate this actor into a frozen
                // load-only region. Preserve the paid tail, even at equal offsets.
                if self.actor_frozen(id) {
                    break;
                }
                let Some(preparation) = self.preparation(id).cloned() else {
                    break;
                };
                let due = preparation
                    .started
                    .checked_add(preparation.remaining)
                    .ok_or(GameError::Schedule(ScheduleError::TimeExhausted))?;
                let completion_due = preparation.active && due <= self.tick;
                if completion_due {
                    self.settle_actor_time(id)?;
                    let target = preparation.plan.target();
                    if self.actors.contains_key(&target) && !self.actor_frozen(target) {
                        self.settle_actor_time(target)?;
                    }
                    if self.run_outcome().terminal {
                        break;
                    }
                    if self.preparation(id) != Some(&preparation) {
                        continue;
                    }
                    // Health recovery can cross Drain's weak threshold during
                    // this paid interval. Recheck dependencies after settling,
                    // before charge settlement or an area census.
                    if preparation
                        .plan
                        .on_success()
                        .iter()
                        .any(|effect| matches!(effect, crate::abilities::PlannedEffect::DrainLife))
                        || preparation.plan.as_ray().is_some_and(|ray| {
                            ray.traversal == crate::abilities::RayTraversal::FirstBody
                        })
                    {
                        self.ensure_preparation_residency(id)?;
                        if self.region_transition_pending() {
                            return Ok(());
                        }
                    }
                }
                let available = self.preparation_still_available(&preparation, None);
                if !available || self.physics.displaced.contains(&id) {
                    let mut outcome = self.actor_outcome(id)?;
                    outcome.interrupt(crate::lifecycle::Interruption::Discard, self.tick)?;
                    self.apply_outcome(outcome)?;
                    break;
                }
                if !preparation.active {
                    break;
                }
                if preparation.plan.work().item().is_some()
                    && !self.visible_threats(id).is_subset(&preparation.threats)
                {
                    let mut outcome = self.actor_outcome(id)?;
                    outcome.interrupt(crate::lifecycle::Interruption::Suspend, self.tick)?;
                    self.apply_outcome(outcome)?;
                    break;
                }
                if !completion_due {
                    break;
                }
                if matches!(preparation.plan, crate::abilities::Plan::Escape(_)) {
                    self.complete_escape(preparation, due)?;
                    break;
                }
                if matches!(preparation.plan, crate::abilities::Plan::Respec(_)) {
                    self.complete_respec(preparation, due)?;
                    break;
                }
                let diagnostic = preparation
                    .plan
                    .as_ability()
                    .map(|plan| plan.target)
                    .and_then(|target| self.begin_combat_diagnostic(id, target, &preparation));
                let mut completion = self.actor_outcome(id)?;
                // A resolution hold is settled once, at the first due strike.
                let paid = preparation.hold.and_then(|owner| {
                    completion
                        .state
                        .combat
                        .as_ref()?
                        .creature()
                        .costs()
                        .reservation(owner)
                });
                let Some(mut resolved) = completion.state.complete_preparation()? else {
                    break;
                };
                if self.survival_diagnostics.is_some() {
                    if let Some(cost) = paid {
                        for cost in cost.entries() {
                            crate::survival_diagnostics::checked_add(
                                completion.survival.paid_resolution.get_mut(cost.resource),
                                u64::from(cost.resolution),
                                &mut completion.survival.incomplete,
                            );
                        }
                    }
                }
                if resolved.plan.as_projectile().is_some() {
                    self.complete_projectile_release(resolved, completion, due)?;
                    break;
                }
                // Capture identification before an exact last unit disappears. Effects
                // may kill the drinker and drop only the unconsumed remainder.
                let consumed_identity = match &resolved.plan {
                    crate::abilities::Plan::Read { item, .. }
                    | crate::abilities::Plan::Zap { item, .. }
                    | crate::abilities::Plan::Drink { item, .. }
                    | crate::abilities::Plan::Eat { item, .. } => Some(
                        self.items
                            .get(item)
                            .ok_or(GameError::ItemUnavailable)?
                            .spec
                            .identity
                            .clone(),
                    ),
                    _ => None,
                };
                let drain_refused = matches!(&resolved.plan, crate::abilities::Plan::Zap { charges, .. } if *charges > 0)
                    && resolved
                        .plan
                        .on_success()
                        .iter()
                        .any(|effect| matches!(effect, crate::abilities::PlannedEffect::DrainLife))
                    && self.health(id).is_some_and(|(health, _)| health < 2);
                if matches!(
                    resolved.item_payment,
                    crate::supplies::ItemPayment::Held { .. }
                ) {
                    completion.spent_items = resolved.item_lineage();
                    if drain_refused {
                        self.release_item_payment(&resolved)?;
                        resolved.item_payment = crate::supplies::ItemPayment::Settled;
                    } else {
                        self.spend_item_payment(&mut resolved)?;
                    }
                }
                let next = resolved.next_strike().and_then(|index| {
                    resolved.plan.as_ability().and_then(|plan| {
                        plan.strikes
                            .get(index + 1)
                            .map(|strike| (index + 1, strike.offset - plan.strikes[index].offset))
                    })
                });
                if let Some((_, gap)) = next {
                    let mut pending = resolved.clone();
                    pending.hold = None;
                    pending.progress = resolved.progress.advanced();
                    pending.remaining = gap;
                    pending.started = due;
                    completion.state.pending = Some(pending);
                    completion.state.ready_at = due
                        .checked_add(gap)
                        .ok_or(GameError::Schedule(ScheduleError::TimeExhausted))?;
                } else {
                    completion.state.ready_at = due
                        .checked_add(resolved.plan.recovery())
                        .ok_or(GameError::Schedule(ScheduleError::TimeExhausted))?;
                }
                resolved.hold = None;
                self.apply_outcome(completion)?;
                if let crate::abilities::Plan::Zap { item, charges, .. } = &resolved.plan {
                    if !drain_refused {
                        self.charge_payment_observed(id, *item, *charges);
                    }
                }
                match resolved.plan.work() {
                    Work::Escape { .. } => unreachable!("Escape has a distinct atomic completion"),
                    Work::Respec { .. } => unreachable!("respec has a distinct atomic completion"),
                    Work::UseAbility { .. } => {
                        if resolved.plan.as_ray().is_some() {
                            self.resolve_prepared_ray(&resolved)?;
                            self.push_event(
                                CombatEvent::SequenceEnded {
                                    actor: id,
                                    intention: resolved.intention,
                                    reason: SequenceEnd::Completed,
                                },
                                None,
                            )?;
                            break;
                        }
                        let plan = resolved
                            .plan
                            .as_ability()
                            .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
                        let index = resolved
                            .next_strike()
                            .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
                        let expected = resolved.progress.advanced();
                        let mut progress = expected;
                        let outcome =
                            self.resolve_strike(plan, index, resolved.intention, &mut progress)?;
                        if next.is_some() && progress != expected {
                            let mut update = self.actor_outcome(id)?;
                            if let Some(pending) = update.state.pending.as_mut().filter(|pending| {
                                pending.plan == resolved.plan
                                    && pending.intention == resolved.intention
                                    && pending.origin == resolved.origin
                                    && pending.progress == expected
                            }) {
                                // Publish only history. Effects may have suspended or retired
                                // the paid tail; never reinstall a stale preparation clone.
                                pending.progress = progress;
                                self.apply_outcome(update)?;
                            }
                        }
                        self.finish_combat_diagnostic(diagnostic, outcome);
                        if next.is_none() {
                            self.push_event(
                                CombatEvent::SequenceEnded {
                                    actor: id,
                                    intention: resolved.intention,
                                    reason: SequenceEnd::Completed,
                                },
                                None,
                            )?;
                        }
                    }
                    Work::Zap { .. } => {
                        if drain_refused {
                            self.push_event(
                                CombatEvent::ItemRefused {
                                    actor: id,
                                    work: resolved.plan.work(),
                                    intention: resolved.intention,
                                    reason: crate::ItemRefusal::TooWeak,
                                },
                                None,
                            )?;
                            break;
                        }
                        let crate::abilities::Plan::Zap {
                            charges, discovery, ..
                        } = &resolved.plan
                        else {
                            unreachable!("zap owns captured device delivery")
                        };
                        let launched = *charges > 0;
                        let self_outcome = if launched {
                            if resolved.plan.as_ray().is_some() {
                                self.resolve_prepared_ray(&resolved)?;
                                None
                            } else {
                                // The shared charge transaction is already settled.
                                // Resolve only the captured item payload, never a new
                                // generic ability or a newly inferred target/aim.
                                Some(self.resolve_consumed_item(&resolved)?)
                            }
                        } else {
                            None
                        };
                        self.finish_item_work(
                            id,
                            &resolved.plan,
                            resolved.intention,
                            launched && *discovery == crate::ItemDiscovery::OnUse,
                            consumed_identity,
                        )?;
                        if let Some(outcome) = self_outcome {
                            self.finish_combat_diagnostic(diagnostic, outcome);
                        }
                    }
                    Work::Read { .. }
                        if matches!(
                            &resolved.plan,
                            crate::abilities::Plan::Read {
                                control: Some(_),
                                ..
                            }
                        ) =>
                    {
                        let crate::abilities::Plan::Read {
                            control: Some(control),
                            discovery,
                            ..
                        } = &resolved.plan
                        else {
                            unreachable!("control read")
                        };
                        let effective = self.resolve_control_read(id, control)?;
                        self.finish_item_work(
                            id,
                            &resolved.plan,
                            resolved.intention,
                            *discovery == crate::ItemDiscovery::OnUse
                                || (effective && *discovery == crate::ItemDiscovery::SelfEffect),
                            consumed_identity,
                        )?;
                    }
                    Work::Read { .. }
                        if matches!(
                            &resolved.plan,
                            crate::abilities::Plan::Read {
                                identify: Some(_),
                                ..
                            }
                        ) =>
                    {
                        self.finish_item_work(
                            id,
                            &resolved.plan,
                            resolved.intention,
                            true,
                            consumed_identity,
                        )?;
                        let crate::abilities::Plan::Read {
                            identify: Some(classes),
                            ..
                        } = &resolved.plan
                        else {
                            unreachable!("identify source")
                        };
                        if self.alive(id) {
                            let mut outcome = self.actor_outcome(id)?;
                            if outcome.state.identification.is_some() {
                                return Err(GameError::ItemUnavailable);
                            }
                            outcome.state.identification =
                                Some(crate::magic_items::IdentificationChoice {
                                    choice: resolved.origin.ok_or(GameError::ItemUnavailable)?,
                                    classes: classes.clone(),
                                });
                            self.apply_outcome(outcome)?;
                        }
                    }
                    Work::Drink { .. } if resolved.plan.on_success().is_empty() => {
                        self.finish_item_work(
                            id,
                            &resolved.plan,
                            resolved.intention,
                            false,
                            consumed_identity,
                        )?;
                    }
                    Work::Read { .. } | Work::Drink { .. } | Work::Eat { .. } => {
                        let outcome = self.resolve_consumed_item(&resolved)?;
                        self.finish_item_work(
                            id,
                            &resolved.plan,
                            resolved.intention,
                            matches!(
                                &resolved.plan,
                                crate::abilities::Plan::Read {
                                    discovery: crate::ItemDiscovery::OnUse,
                                    ..
                                } | crate::abilities::Plan::Drink {
                                    discovery: crate::ItemDiscovery::OnUse,
                                    ..
                                }
                            ) || (outcome.result == crate::abilities::OutcomeStatus::Applied
                                && !matches!(
                                    &resolved.plan,
                                    crate::abilities::Plan::Read {
                                        discovery: crate::ItemDiscovery::Never,
                                        ..
                                    } | crate::abilities::Plan::Drink {
                                        discovery: crate::ItemDiscovery::Never,
                                        ..
                                    }
                                )),
                            consumed_identity,
                        )?;
                        self.finish_combat_diagnostic(diagnostic, outcome);
                    }
                    Work::Equip { .. }
                    | Work::Unequip { .. }
                    | Work::Ignite { .. }
                    | Work::Extinguish { .. } => {
                        self.finish_item_work(
                            id,
                            &resolved.plan,
                            resolved.intention,
                            false,
                            consumed_identity,
                        )?;
                    }
                }
                if next.is_none() || self.run_outcome().terminal {
                    break;
                }
            }
        }
        Ok(())
    }
}
