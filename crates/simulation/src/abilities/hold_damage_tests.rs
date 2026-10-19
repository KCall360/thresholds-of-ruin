//! Relationship-owned damage and immutable sequence identity.
use super::*;
use crate::rules::*;
use crate::{ActorId, Game};

fn policy() -> crate::linked_hold::HoldPolicy {
    crate::linked_hold::HoldPolicy {
        sustaining: crate::content_ids::FlagId::new("magical").unwrap(),
        preparation: 100,
        recovery: 100,
        protection: 400,
    }
}
fn fixture(reliable: bool) -> Game {
    fixture_with(reliable, |_| {}, |_, _| {})
}
fn fixture_with(
    reliable: bool,
    configure: impl FnOnce(&mut AbilityDef),
    sources: impl FnOnce(&mut RulesetDocument, &mut crate::creatures::Species),
) -> Game {
    super::strike_tests::sequence_game_custom(
        &[0, 10, 20, 30],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            for strike in strikes {
                strike.check = Some(CheckDef {
                    actor: CheckActor::User,
                    skill: SkillSelector::Named {
                        skill: crate::attributes::Skill::Athletics,
                    },
                    against: Against::Defense {
                        defense: DefenseKind::Physical,
                    },
                });
                strike.on_success = vec![Effect::HoldDamage {
                    category: crate::combat::DamageType::Impact,
                    descriptor: None,
                }];
                strike.riders = vec![RiderDef {
                    trigger: RiderTrigger::Success,
                    condition: None,
                    check: None,
                    effects: vec![Effect::ApplyLinkedHold { policy: policy() }],
                    descriptors: vec![],
                }];
            }
            configure(ability);
        },
        |rules, species| {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Flag {
                    flag: policy().sustaining,
                },
            });
            for (stat, value) in [
                (AbilityStat::HoldDamageIncrement, 1),
                (AbilityStat::CheckBonus, if reliable { 100 } else { 0 }),
            ] {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Ability {
                            ability: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                            stat,
                        },
                        value,
                    },
                });
            }
            sources(rules, species);
        },
    )
    .0
}
fn resolve_at(game: &mut Game, tick: u64) {
    game.tick = tick;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = tick;
    game.resolve_preparations().unwrap();
}
fn primary_success(game: &Game) -> bool {
    game.combat_events()
        .iter()
        .rev()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved {
                outcome,
                strike: Some(_),
                ..
            } => outcome.check.map(|check| check.success),
            _ => None,
        })
        .unwrap()
}
#[test]
fn hold_damage_initial_zero_miss_increments_once_later_hit_uses_exact_bonus() {
    let original = fixture(false);
    let mut found = false;
    for seed in 1..=256 {
        let mut game = original.clone();
        game.rules_rng = seed;
        resolve_at(&mut game, 10);
        if !primary_success(&game) {
            continue;
        }
        let id = game.linked_holds(ActorId(2))[0].id;
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 0);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        resolve_at(&mut game, 20);
        if primary_success(&game) {
            continue;
        }
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 1);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        let once = game.clone();
        game.resolve_preparations().unwrap();
        assert_eq!(game, once);
        resolve_at(&mut game, 30);
        if !primary_success(&game) {
            continue;
        }
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 2);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 98);
        found = true;
        break;
    }
    assert!(found, "bounded seeds include ordinary hit/miss/hit");
}
#[test]
fn hold_damage_retained_sequence_never_rebinds_replacement_and_decode_requires_creation_branch() {
    let mut game = fixture(true);
    resolve_at(&mut game, 10);
    let old = game.linked_holds(ActorId(2))[0].id;
    resolve_at(&mut game, 20);
    assert_eq!(game.linked_hold(ActorId(2), old).unwrap().escalation, 1);
    let pending = game.preparation(ActorId(1)).unwrap();
    assert_eq!(pending.progress.hold_binding().unwrap().id, old);
    let mut wire = serde_json::to_value(pending).unwrap();
    wire["progress"]["linked_hold"]["created_at"]
        .as_object_mut()
        .unwrap()
        .remove("primary_success");
    assert!(serde_json::from_value::<crate::combat::Preparation>(wire).is_err());
    game.release_linked_hold(ActorId(2), old).unwrap();
    let replacement = game
        .apply_linked_hold(ActorId(1), ActorId(2), policy())
        .unwrap()
        .unwrap();
    assert_ne!(old, replacement);
    let health = game.health(ActorId(2)).unwrap().0;
    resolve_at(&mut game, 30);
    assert_eq!(game.health(ActorId(2)).unwrap().0, health);
    assert_eq!(
        game.linked_hold(ActorId(2), replacement)
            .unwrap()
            .escalation,
        0
    );
    assert_eq!(
        game.preparation(ActorId(1))
            .unwrap()
            .progress
            .hold_binding()
            .unwrap()
            .id,
        old
    );
}
#[test]
fn hold_damage_authored_shape_rejects_rider_failure_duplicate_and_ranged_channels() {
    let game = fixture(true);
    let document = game.content().rules().document().clone();
    for case in 0..4 {
        let mut changed = document.clone();
        let ability = changed
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap();
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        let effect = strikes[0].on_success[0].clone();
        match case {
            0 => strikes[0].on_failure.push(effect),
            1 => strikes[0].riders[0].effects.push(effect),
            2 => strikes[0].on_success.push(effect),
            _ => ability.reach = Reach::Sight { range: 5 },
        }
        assert!(Ruleset::validate(changed).is_err(), "case {case}");
    }
}

fn primary_strikes(ability: &mut AbilityDef) -> &mut Vec<StrikeDef> {
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    strikes
}

#[test]
fn hold_damage_initial_creation_has_no_damage_even_with_positive_shared_edge() {
    let mut game = fixture_with(
        true,
        |_| {},
        |_, species| {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Edge,
                    value: 7,
                },
            });
        },
    );
    assert_eq!(
        game.preparation(ActorId(1))
            .unwrap()
            .plan
            .as_ability()
            .unwrap()
            .strikes[0]
            .edge,
        7
    );
    resolve_at(&mut game, 10);
    assert!(primary_success(&game));
    let id = game.linked_holds(ActorId(2))[0].id;
    assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 0);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
    assert!(
        game.combat_events().iter().all(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => outcome
                .effects
                .iter()
                .all(|effect| !matches!(effect, ResolvedEffect::Damage { .. })),
            _ => true,
        }),
        "without an exact binding the damage payload is absent, not an edge-amplified zero"
    );
}

#[test]
fn hold_damage_creation_branch_survives_nonqualifying_primary_checkpoint() {
    for target_rolls in [false, true] {
        let original = fixture_with(
            true,
            |ability| {
                let first = &mut primary_strikes(ability)[0];
                first.check = Some(CheckDef {
                    actor: if target_rolls {
                        CheckActor::Target
                    } else {
                        CheckActor::User
                    },
                    skill: SkillSelector::Named {
                        skill: crate::attributes::Skill::Athletics,
                    },
                    against: if target_rolls {
                        Against::Defense {
                            defense: DefenseKind::Physical,
                        }
                    } else {
                        Against::Difficulty {
                            formula: Formula {
                                base: 0,
                                skill: None,
                                attribute: None,
                            },
                        }
                    },
                });
                first.on_success = vec![Effect::ApplyLinkedHold { policy: policy() }];
                first.riders.clear();
            },
            |_, _| {},
        );
        let mut game = (1..=256)
            .find_map(|seed| {
                let mut candidate = original.clone();
                candidate.rules_rng = seed;
                resolve_at(&mut candidate, 10);
                primary_success(&candidate).then_some(candidate)
            })
            .expect("an actual successful nonqualifying primary exists");
        let pending = game.preparation(ActorId(1)).unwrap();
        let binding = pending.progress.hold_binding().unwrap();
        assert!(binding.created_at.unwrap().primary_success);
        let crate::combat::PreparationProgress::Ability {
            successful_primaries,
            ..
        } = pending.progress
        else {
            unreachable!()
        };
        assert_eq!(
            successful_primaries, 0,
            "Rending Pair's mask retains its narrower meaning"
        );
        assert!(pending.progress_valid());
        let mut shared = crate::checkpoint::SharedState::default();
        let checkpoint =
            serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
                .unwrap();
        let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
        game = Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
        assert_eq!(
            game.preparation(ActorId(1))
                .unwrap()
                .progress
                .hold_binding(),
            Some(binding)
        );
        resolve_at(&mut game, 20);
        assert_eq!(
            game.linked_hold(ActorId(2), binding.id).unwrap().escalation,
            1
        );
    }
}

#[test]
fn hold_damage_same_strike_creation_then_push_keeps_retired_receipt_and_never_rebinds() {
    let mut game = fixture_with(
        true,
        |ability| {
            let first = &mut primary_strikes(ability)[0];
            first.riders.push(RiderDef {
                trigger: RiderTrigger::Success,
                condition: None,
                check: None,
                effects: vec![Effect::PushTargetOneCell {
                    direction: PushDirection::AwayFromSource,
                }],
                descriptors: vec![],
            });
        },
        |_, species| {
            species.displacement = Some(crate::creatures::DisplacementProfile {
                size: crate::creatures::CreatureSize::Medium,
                anchored: false,
            });
        },
    );
    let original_location = game.actors[&ActorId(2)].location;
    resolve_at(&mut game, 10);
    assert!(primary_success(&game));
    assert_ne!(
        game.actors[&ActorId(2)].location,
        original_location,
        "the push really crossed out of contact"
    );
    assert!(game.linked_holds(ActorId(2)).is_empty());
    let binding = game
        .preparation(ActorId(1))
        .unwrap()
        .progress
        .hold_binding()
        .unwrap();
    let site = binding.created_at.unwrap();
    assert_eq!(
        (site.strike, site.rider, site.primary_success),
        (0, Some(0), true)
    );
    assert!(game.preparation(ActorId(1)).unwrap().progress_valid());
    game.teleport(ActorId(2), original_location).unwrap();
    let replacement = game
        .apply_linked_hold(ActorId(1), ActorId(2), policy())
        .unwrap()
        .unwrap();
    assert_ne!(replacement, binding.id);
    let health = game.health(ActorId(2)).unwrap().0;
    resolve_at(&mut game, 20);
    assert_eq!(game.health(ActorId(2)).unwrap().0, health);
    assert_eq!(
        game.linked_hold(ActorId(2), replacement)
            .unwrap()
            .escalation,
        0
    );
    assert_eq!(
        game.preparation(ActorId(1))
            .unwrap()
            .progress
            .hold_binding(),
        Some(binding)
    );
}

#[test]
fn hold_damage_paid_scalar_survives_current_effective_grant_expiration() {
    let grant = crate::content_ids::ConditionId::new("temporary_tightening").unwrap();
    let mut game = fixture_with(
        true,
        |_| {},
        |rules, _| {
            let mut condition = rules
                .conditions
                .iter()
                .find(|value| value.id.as_str() == "braced")
                .unwrap()
                .clone();
            condition.id = grant.clone();
            condition.lifecycle = ConditionLifecycle::Timed;
            condition.modifiers = vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Ability {
                        ability: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                        stat: AbilityStat::HoldDamageIncrement,
                    },
                    value: 1,
                },
            }];
            rules.conditions.push(condition);
        },
    );
    let initial = game.preparation(ActorId(1)).unwrap().intention.unwrap();
    game.cancel_intention(ActorId(1), initial).unwrap();
    game.apply_condition(ActorId(1), &grant, ActorId(1), 15)
        .unwrap();
    let ability = crate::content_ids::AbilityId::new("basic_melee").unwrap();
    let frozen = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    assert_eq!(frozen.as_ability().unwrap().hold_damage_increment, 2);
    game.start_work(frozen.clone(), None).unwrap();
    resolve_at(&mut game, 10);
    let id = game.linked_holds(ActorId(2))[0].id;
    game.tick = 16;
    game.settle_actor_time(ActorId(1)).unwrap();
    let current = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    assert_eq!(current.as_ability().unwrap().hold_damage_increment, 1);
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, frozen);
    assert!(game.plan_source_valid(&frozen));
    resolve_at(&mut game, 20);
    assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 2);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 98);
}

#[test]
fn hold_damage_numeric_ceiling_rejects_before_rng_or_progress_even_when_primary_would_miss() {
    let mut game = fixture(false);
    let first_seed = (1..=256)
        .find(|seed| {
            let mut candidate = game.clone();
            candidate.rules_rng = *seed;
            resolve_at(&mut candidate, 10);
            primary_success(&candidate)
        })
        .unwrap();
    game.rules_rng = first_seed;
    resolve_at(&mut game, 10);
    let binding = game
        .preparation(ActorId(1))
        .unwrap()
        .progress
        .hold_binding()
        .unwrap();
    let pending = game.preparation(ActorId(1)).unwrap().clone();
    let plan = pending.plan.as_ability().unwrap().clone();
    let next = pending.progress.advanced();
    let miss_seed = (1..=256)
        .find(|seed| {
            let mut candidate = game.clone();
            candidate.rules_rng = *seed;
            let mut probe_progress = next;
            let outcome = candidate
                .resolve_strike(&plan, 1, pending.intention, &mut probe_progress)
                .unwrap();
            outcome.check.is_some_and(|check| !check.success)
        })
        .expect("the control strike would miss");
    game.rules_rng = miss_seed;
    for slot in &mut game.actors.get_mut(&ActorId(2)).unwrap().holds.incoming {
        if let crate::linked_hold::HoldSlot::Linked { link } = slot {
            if link.id == binding.id {
                link.escalation = crate::linked_hold::HOLD_ESCALATION_MAX;
            }
        }
    }
    let before = game.clone();
    let mut progress = next;
    assert_eq!(
        game.resolve_strike(&plan, 1, pending.intention, &mut progress),
        Err(crate::GameError::Rule(RuleError::InvalidPlan))
    );
    assert_eq!(
        game, before,
        "numeric refusal must precede RNG, effects and publication"
    );
    assert_eq!(progress, next);
}

fn budget_descriptors(document: &mut RulesetDocument) -> Vec<crate::content_ids::DescriptorId> {
    (0..63)
        .map(|index| {
            let id = crate::content_ids::DescriptorId::new(format!("hold_budget_{index}")).unwrap();
            document.descriptors.push(NamedDescriptor {
                id: id.clone(),
                name: format!("Hold budget {index}"),
            });
            id
        })
        .collect()
}

#[test]
fn hold_damage_descriptor_counts_enforce_authored_sequence_budget() {
    let game = fixture(true);
    let descriptor = crate::content_ids::DescriptorId::new("fire").unwrap();
    let mut document = game.content().rules().document().clone();
    let row_descriptors = budget_descriptors(&mut document);
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    for strike in primary_strikes(ability) {
        strike.descriptors = row_descriptors.clone();
        let Effect::HoldDamage {
            descriptor: value, ..
        } = &mut strike.on_success[0]
        else {
            unreachable!()
        };
        *value = Some(descriptor.clone());
    }
    Ruleset::validate(document.clone())
        .expect("252 unique row descriptors plus four damage descriptors exactly fit");
    primary_strikes(
        document
            .abilities
            .iter_mut()
            .find(|ability| ability.basic_attack)
            .unwrap(),
    )[0]
    .descriptors
    .push(descriptor);
    assert!(
        matches!(Ruleset::validate(document), Err(RulesetError::Bound { field }) if field == "sequence_descriptors")
    );
}

#[test]
fn hold_damage_descriptor_counts_enforce_frozen_sequence_budget() {
    let game = fixture(true);
    let descriptor = crate::content_ids::DescriptorId::new("fire").unwrap();
    // Selected natural rows are admitted structurally here; there is no authored
    // row equality check that could mask a missing budget charge.
    let mut document = game.content().rules().document().clone();
    let row_descriptors = budget_descriptors(&mut document);
    document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap()
        .strikes = StrikeSource::SelectedMelee;
    let rules = Ruleset::validate(document).unwrap();
    let mut frozen = game
        .preparation(ActorId(1))
        .unwrap()
        .plan
        .as_ability()
        .unwrap()
        .clone();
    for (ordinal, strike) in frozen.strikes.iter_mut().enumerate() {
        strike.origin = StrikeOrigin::Natural {
            species: crate::content_ids::SpeciesId::new("test").unwrap(),
            ordinal: ordinal as u16,
        };
        strike.descriptors = row_descriptors.clone();
        let PlannedEffect::HoldDamage {
            descriptor: value, ..
        } = &mut strike.on_success[0]
        else {
            unreachable!()
        };
        *value = Some(descriptor.clone());
    }
    // The fixture changed from authored named-skill rows to the selected natural
    // source, so its single captured weapon witness must change with the rows.
    assert!(!frozen.valid(&rules));
    frozen.weapon = FrozenWeaponSelection::Natural;
    assert!(frozen.valid(&rules));
    frozen.strikes[0].descriptors.push(descriptor);
    assert!(
        !frozen.valid(&rules),
        "the 257th descriptor is rejected at frozen-plan admission"
    );
}

#[test]
fn hold_damage_cannot_be_admitted_as_non_hostile_or_healing_care() {
    let game = fixture(true);
    let rules = game.content().rules();
    let hold = Effect::HoldDamage {
        category: crate::combat::DamageType::Impact,
        descriptor: None,
    };
    for healing in [false, true] {
        let mut document = rules.document().clone();
        let mut probe = document
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .clone();
        probe.id = crate::content_ids::AbilityId::new("care_probe").unwrap();
        probe.basic_attack = false;
        probe.interaction = Interaction::NonHostile;
        let rows = primary_strikes(&mut probe);
        rows.truncate(1);
        rows[0].riders.clear();
        rows[0].on_success = if healing {
            vec![
                Effect::Heal {
                    amount: AmountExpr::Fixed { amount: 4 },
                },
                hold.clone(),
            ]
        } else {
            vec![hold.clone()]
        };
        document.abilities.push(probe);
        assert!(
            Ruleset::validate(document).is_err(),
            "non-hostile channel, healing={healing}"
        );
    }
    let frozen = [PlannedEffect::HoldDamage {
        category: crate::combat::DamageType::Impact,
        descriptor: None,
    }];
    assert!(rules
        .validate_planned_interaction(Interaction::HostileToTarget, frozen.iter())
        .is_ok());
    assert!(rules
        .validate_planned_interaction(Interaction::NonHostile, frozen.iter())
        .is_err());
}
