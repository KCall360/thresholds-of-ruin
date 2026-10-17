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
            // Includes equal-offset rows without rescanning every actor per rider.
            for _ in 0..=crate::limits::STRIKES {
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
                    && !self.visible_hostiles(id).is_subset(&preparation.threats)
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
                        crate::survival_diagnostics::checked_add(
                            completion.survival.paid_resolution.get_mut(cost.resource),
                            u64::from(cost.resolution),
                            &mut completion.survival.incomplete,
                        );
                    }
                }
                // Capture identification before an exact last unit disappears. Effects
                // may kill the drinker and drop only the unconsumed remainder.
                let consumed_identity = match &resolved.plan {
                    crate::abilities::Plan::Drink { item, .. }
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
                if matches!(
                    resolved.item_payment,
                    crate::supplies::ItemPayment::Held { .. }
                ) {
                    completion.spent_items = resolved.item_lineage();
                    self.spend_item_payment(&mut resolved)?;
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
                match resolved.plan.work() {
                    Work::Escape { .. } => unreachable!("Escape has a distinct atomic completion"),
                    Work::Respec { .. } => unreachable!("respec has a distinct atomic completion"),
                    Work::UseAbility { .. } => {
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
                            self.event_log.push(CombatEvent::SequenceEnded {
                                actor: id,
                                intention: resolved.intention,
                                reason: SequenceEnd::Completed,
                            });
                        }
                    }
                    Work::Drink { .. } if resolved.plan.on_success().is_empty() => {
                        self.finish_item_work(
                            id,
                            resolved.plan.work(),
                            resolved.intention,
                            false,
                            consumed_identity,
                        )?;
                    }
                    Work::Drink { .. } | Work::Eat { .. } => {
                        let outcome = self.resolve_consumed_item(&resolved)?;
                        self.finish_item_work(
                            id,
                            resolved.plan.work(),
                            resolved.intention,
                            outcome.result == crate::abilities::OutcomeStatus::Applied,
                            consumed_identity,
                        )?;
                        self.finish_combat_diagnostic(diagnostic, outcome);
                    }
                    Work::Equip { .. } | Work::Unequip { .. } => {
                        self.finish_item_work(
                            id,
                            resolved.plan.work(),
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
