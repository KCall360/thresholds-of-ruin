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
                        // This witness exists only across this synchronous release.
                        // It cannot identify a blocked change on a later return.
                        let discernible =
                            if launched && *discovery == crate::ItemDiscovery::DiscernibleEffect {
                                Some(self.capture_discernible_device_release(id)?)
                            } else {
                                None
                            };
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
                            launched && (*discovery == crate::ItemDiscovery::OnUse
                                || discernible.as_ref().is_some_and(|witness| {
                                    self_outcome.as_ref().map_or_else(
                                        || self.discernible_device_release(id, resolved.intention, witness),
                                        |completion| completion.identifies() && completion.outcome.effects.iter().any(|effect| matches!(effect,
                                            crate::abilities::ResolvedEffect::BalefulPolymorph { changed: true, .. })),
                                    )
                                })),
                            consumed_identity,
                        )?;
                        if let Some(outcome) = self_outcome {
                            self.finish_combat_diagnostic(diagnostic, outcome.outcome);
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
                            ) || (outcome.identifies()
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
                        self.finish_combat_diagnostic(diagnostic, outcome.outcome);
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

/// Event-time visual eligibility, never a saved identity or selected-form receipt.
struct DiscernibleDeviceRelease {
    first_event: usize,
    visible: std::collections::BTreeSet<crate::ActorId>,
}
impl Game {
    fn capture_discernible_device_release(
        &self,
        actor: crate::ActorId,
    ) -> Result<DiscernibleDeviceRelease, GameError> {
        let (_, sight) = self.scene_snapshot(actor)?;
        let visible = self
            .actors
            .keys()
            .copied()
            .filter(|target| {
                *target == actor || self.actor_visible_in_sight(actor, *target, &sight)
            })
            .collect();
        Ok(DiscernibleDeviceRelease {
            first_event: self.event_log.len(),
            visible,
        })
    }
    fn discernible_device_release(
        &self,
        actor: crate::ActorId,
        intention: Option<crate::IntentionId>,
        witness: &DiscernibleDeviceRelease,
    ) -> bool {
        self.event_log[witness.first_event..].iter().any(|event| {
            let CombatEvent::Resolved { outcome, .. } = event else {
                return false;
            };
            // Use the committed outcome, never the pure resolver's provisional
            // selected form. Unfit, resisted, missed and unsupported bodies do
            // not become successful identification witnesses.
            outcome.actor == actor
                && outcome.intention == intention
                && witness.visible.contains(&outcome.target)
                && outcome.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        crate::abilities::ResolvedEffect::BalefulPolymorph { changed: true, .. }
                    )
                })
        })
    }
}

#[cfg(test)]
mod discernible_device_tests {
    use super::*;
    use crate::abilities::{AttemptedParticipation, Outcome, OutcomeStatus, ResolvedEffect};
    use crate::content_ids::BodyFormId;
    use std::num::NonZeroU64;
    #[test]
    fn release_witness_is_immediate_visual_changed_only_and_survives_fatal_result() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = tor_world::Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        let mut target_at = at;
        target_at.position.x += 1;
        let target = game
            .spawn_actor(target_at, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            target,
            crate::CreatureIdentity {
                name: "target".into(),
                faction: "fixture".into(),
            },
            crate::test_creatures::build(crate::test_creatures::species()),
        )
        .unwrap();
        game.world.set_region_light(at.region, true).unwrap();
        let witness = game.capture_discernible_device_release(actor).unwrap();
        assert!(witness.visible.contains(&target));
        let mut outcome = Outcome {
            actor,
            target,
            ability: None,
            intention: None,
            result: OutcomeStatus::Unaffected,
            check: None,
            resistance: None,
            participation: AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![ResolvedEffect::BalefulPolymorph {
                form: Some(BodyFormId::new("private_form").unwrap()),
                changed: false,
            }],
        };
        // These are synthetic committed receipts, not raw unchecked history:
        // keep canonical effect status and the abilityless public-source contract.
        let publish = |game: &mut Game, outcome: Outcome| {
            assert!(outcome.valid(), "positive committed-receipt fixture");
            let origin = game.capture_event_origin(outcome.actor, outcome.target);
            game.push_event(
                CombatEvent::Resolved {
                    outcome,
                    strike: None,
                    source_disclosure: crate::event_history::SourceAbilityDisclosure::Public,
                },
                Some(origin),
            )
            .unwrap();
        };
        publish(&mut game, outcome.clone());
        assert!(
            !game.discernible_device_release(actor, None, &witness),
            "blocked form is private, not discovery"
        );
        outcome.effects.clear();
        outcome.result = OutcomeStatus::Unaffected;
        publish(&mut game, outcome.clone());
        assert!(!game.discernible_device_release(actor, None, &witness));
        outcome.result = OutcomeStatus::Applied;
        outcome.effects = vec![ResolvedEffect::BalefulPolymorph {
            form: Some(BodyFormId::new("private_form").unwrap()),
            changed: true,
        }];
        publish(&mut game, outcome.clone());
        assert!(game.discernible_device_release(actor, None, &witness));
        crate::test_creatures::injure(&mut game, target, 1000).unwrap();
        assert!(!game.alive(target));
        assert!(
            game.discernible_device_release(actor, None, &witness),
            "visible fatal transformation is still discernible"
        );
        let later = game.capture_discernible_device_release(actor).unwrap();
        assert!(
            !game.discernible_device_release(actor, None, &later),
            "never rediscover from prior history"
        );
        let unseen = DiscernibleDeviceRelease {
            first_event: 0,
            visible: [actor].into_iter().collect(),
        };
        assert!(
            !game.discernible_device_release(actor, None, &unseen),
            "unseen changed form grants no identity"
        );
        outcome.target = actor;
        publish(&mut game, outcome);
        assert!(
            game.discernible_device_release(actor, None, &later),
            "self owns its immediate change witness"
        );
    }
}
