//! Actual paid shared-device acceptance; structural trace tests are separate.
use super::*;
use crate::attributes::Skill;
use crate::content_ids::{AbilityId, ConditionFamilyId, ConditionId};
use crate::rules::*;
use crate::{ActorId, Game, ItemId};
use tor_world::{Location, Position, RegionId};
fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(cancellation: bool) -> (Game, ItemId) {
    fixture_with(cancellation, |_, _, _| {})
}
fn fixture_with(
    cancellation: bool,
    customize: impl FnOnce(&mut RulesetDocument, &mut AbilityDef, &mut crate::creatures::Species),
) -> (Game, ItemId) {
    let mut game = super::ray_tests::fixture_with(|document, ray, species| {
        let mut special = document
            .abilities
            .iter()
            .find(|a| a.basic_attack)
            .unwrap()
            .clone();
        special.id = AbilityId::new("fixture_cancellable_special").unwrap();
        special.basic_attack = false;
        special.cancellable = true;
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: special.id.clone(),
            },
        });
        document.abilities.push(special);
        let mut invisible = document
            .conditions
            .iter()
            .find(|c| c.id.as_str() == "distracted")
            .unwrap()
            .clone();
        invisible.id = ConditionId::new("fixture_cancellable_invisibility").unwrap();
        invisible.family = ConditionFamilyId::new("fixture_cancellable_invisibility").unwrap();
        invisible.lifecycle = ConditionLifecycle::Timed;
        invisible.restrictions.clear();
        invisible.modifiers = vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Perception {
                value: crate::perception::PerceptionValue::Invisible { enabled: true },
            },
        }];
        document.conditions.push(invisible);
        species.modifiers.push(Modifier {
            cancellable: true,
            scope: Scope::Always,
            op: ModifierOp::Perception {
                value: crate::perception::PerceptionValue::Invisible { enabled: true },
            },
        });
        // No inferred attack-skill bonus: this is a real captured Lore source.
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Attribute {
                    attribute: crate::attributes::Attribute::Intellect,
                },
                value: 100,
            },
        });
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 3,
            },
        });
        ray.invocation = InvocationKind::DirectionalContactRay;
        ray.reach = Reach::UntilObstruction;
        ray.cancellable = false;
        let StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        strikes[0].cancellable = false;
        strikes[0].descriptors.clear();
        strikes[0].riders.clear();
        if cancellation {
            strikes[0].check = None;
            strikes[0].on_success = vec![Effect::CancelSpecials];
            strikes[0].on_failure.clear();
        } else {
            strikes[0].check = Some(CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named {
                    skill: Skill::Agility,
                },
                against: Against::Opposed {
                    skill: SkillSelector::Named { skill: Skill::Lore },
                },
            });
            strikes[0].on_success.clear();
            strikes[0].on_failure = vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: crate::combat::DamageType::Energy,
                        descriptor: None,
                        amount: AmountExpr::Rolled {
                            dice: DiceExpr {
                                count: 1,
                                sides: 4,
                                bonus: 1,
                            },
                        },
                    }],
                },
            }];
        }
        customize(document, ray, species);
    });
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 0;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 1000;
    let mut spec = crate::ItemSpec::ordinary("contact test stick".into());
    spec.class = crate::ItemClass::Tool;
    spec.identity = "fixture.contact_device".into();
    spec.concealed = true;
    spec.device = Some(crate::DeviceSpec {
        ability: AbilityId::new("fixture_ray").unwrap(),
        initial_charges: crate::ChargeRange {
            minimum: 3,
            maximum: 3,
        },
        discovery: crate::ItemDiscovery::OnUse,
    });
    let item = game
        .place_item_stack(10, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    (game, item)
}
fn complete(game: &mut Game, item: ItemId) {
    let before = game.tick;
    let id = game
        .admit_intention(
            ActorId(1),
            crate::Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert_eq!(game.tick, before + 100);
    assert_eq!(game.items[&item].charges, Some(2));
    assert!(game.items[&item].holds.is_empty());
}
#[test]
fn paid_cancellation_is_checkless_and_preserves_owned_progression() {
    let (mut game, item) = fixture(true);
    let special = AbilityId::new("fixture_cancellable_special").unwrap();
    let before = game.creature(ActorId(2)).unwrap().into_owned();
    assert!(before.derived().abilities.contains(&special));
    assert!(before.derived().perception.invisible);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    assert_eq!(plan.as_ray().unwrap().traversal, RayTraversal::FirstBody);
    assert_eq!(plan.as_ray().unwrap().resolution, RayResolution::Checkless);
    assert_eq!(plan.tokens().len(), 0);
    let rng = game.rules_rng;
    complete(&mut game, item);
    assert_eq!(game.rules_rng, rng);
    let after = game.creature(ActorId(2)).unwrap();
    assert!(after.cancelled());
    assert!(!after.derived().abilities.contains(&special));
    assert!(!after.derived().perception.invisible);
    assert_eq!(after.build(), before.build());
    assert_eq!(after.restorable_losses(), before.restorable_losses());
    assert_eq!(after.health(), before.health());
    assert!(game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.target == ActorId(2)
        && outcome.check.is_none() && outcome.effects == [ResolvedEffect::CancelSpecials { changed: true }])));
}
#[test]
fn repeated_cancellation_clears_fresh_independent_invisibility_without_global_veto() {
    let (mut game, item) = fixture(true);
    complete(&mut game, item);
    assert!(game.creature(ActorId(2)).unwrap().cancelled());
    assert!(
        !game
            .creature(ActorId(2))
            .unwrap()
            .derived()
            .perception
            .invisible
    );
    let invisible = ConditionId::new("fixture_cancellable_invisibility").unwrap();
    game.apply_effects(
        ActorId(2),
        &[Effect::ApplyCondition {
            condition: invisible.clone(),
            duration: 1000,
        }],
    )
    .unwrap();
    assert!(
        game.creature(ActorId(2))
            .unwrap()
            .derived()
            .perception
            .invisible
    );
    let rng = game.rules_rng;
    assert!(game
        .apply_effects(ActorId(2), &[Effect::CancelSpecials])
        .unwrap());
    assert_eq!(game.rules_rng, rng);
    assert!(
        !game
            .creature(ActorId(2))
            .unwrap()
            .derived()
            .perception
            .invisible
    );
    assert!(!game
        .creature(ActorId(2))
        .unwrap()
        .conditions()
        .instances()
        .any(|row| row.condition == invisible));
}
#[test]
fn paid_missile_has_one_contest_and_one_universal_damage_bonus() {
    let (mut game, item) = fixture(false);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    let ray = plan.as_ray().unwrap();
    assert!(
        matches!(ray.resolution, RayResolution::Opposed { source_skill: Skill::Lore,
        target_skill: Skill::Agility, source_bonus, .. } if source_bonus >= 100)
    );
    let [PlannedEffect::Damage { damage }] = ray.effects.as_slice() else {
        panic!("one missile damage pool")
    };
    let crate::damage::DamageAmount::Rolled(pool) = damage.components()[0].amount() else {
        panic!("rolled missile")
    };
    assert_eq!((pool.count(), pool.sides(), pool.bonus()), (1, 4, 4));
    let hp = game.health(ActorId(2)).unwrap().0;
    game.set_combat_diagnostics(true);
    complete(&mut game, item);
    let loss = hp - game.health(ActorId(2)).unwrap().0;
    assert!((5..=8).contains(&loss));
    let diagnostics = game.combat_diagnostics().unwrap();
    let records = diagnostics.records();
    assert_eq!(records.len(), 1);
    let steps = records[0].trace.steps();
    assert_eq!(
        steps.iter().filter(|s| s.id == "ray_launch_check").count(),
        1
    );
    assert_eq!(steps.iter().filter(|s| s.id == "check").count(), 1);
    assert!(records[0]
        .outcome
        .check
        .is_some_and(|check| !check.success && check.opposed));
}

#[test]
fn ordinary_granted_contact_uses_the_same_plan_and_not_item_payment() {
    let (mut game, item) = fixture(false);
    let ability = AbilityId::new("fixture_ray").unwrap();
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .abilities
        .contains(&ability));
    let hp = game.health(ActorId(2)).unwrap().0;
    let id = game
        .admit_intention(
            ActorId(1),
            crate::Action::UseAbility {
                id: ability.clone(),
                invocation: AbilityInvocation::directional_ray(StepDirection::East),
                supplies: crate::supplies::SupplySelection::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert!(game.health(ActorId(2)).unwrap().0 < hp);
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.ability.as_ref() == Some(&ability)
        && outcome.intention == Some(id) && outcome.target == ActorId(2))));
}

#[test]
fn paid_contact_empty_wall_and_noncombat_interception_do_not_roll_or_reach_behind() {
    for cancellation in [false, true] {
        for obstruction in [0, 1, 2] {
            let (mut game, item) = fixture(cancellation);
            if obstruction == 0 {
                // Actual empty eastward flight: move the live target off the ray.
                game.actors.get_mut(&ActorId(2)).unwrap().location = Location {
                    region: RegionId(1),
                    position: Position { x: 3, y: 2, z: 0 },
                };
            } else if obstruction == 1 {
                game.world.set_wall(at(2), true).unwrap();
            } else {
                let body = game
                    .spawn_actor(at(2), std::num::NonZeroU64::new(100).unwrap())
                    .unwrap();
                game.actors.get_mut(&body).unwrap().ready_at = 1000;
                assert!(game.actors[&body].alive() && game.creature(body).is_none());
            }
            let before = game.creature(ActorId(2)).unwrap().into_owned();
            let rng = game.rules_rng;
            let events = game.combat_events().len();
            complete(&mut game, item);
            assert_eq!(
                game.rules_rng, rng,
                "mode {cancellation}, obstruction {obstruction}"
            );
            let after = game.creature(ActorId(2)).unwrap();
            assert_eq!(after.health(), before.health());
            assert_eq!(after.cancelled(), before.cancelled());
            assert_eq!(
                after.derived().perception.invisible,
                before.derived().perception.invisible
            );
            assert!(!game
                .combat_events()
                .iter()
                .skip(events)
                .any(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. })));
        }
    }
}

#[test]
fn cancellation_record_requires_state_and_roundtrips_without_reintroducing_sources() {
    use crate::creatures::record::CreatureRecord;
    let (mut game, item) = fixture(true);
    complete(&mut game, item);
    let creature = game.creature(ActorId(2)).unwrap();
    let value = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    assert_eq!(value["cancelled"], true);
    let decoded: CreatureRecord = serde_json::from_value(value.clone()).unwrap();
    let restored = decoded
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, *creature);
    for malformed in [
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!(0)),
    ] {
        let mut bad = value.clone();
        match malformed {
            None => {
                bad.as_object_mut().unwrap().remove("cancelled");
            }
            Some(value) => {
                bad["cancelled"] = value;
            }
        }
        assert!(serde_json::from_value::<CreatureRecord>(bad).is_err());
    }
}

#[test]
fn cancellation_of_tagged_health_uses_ordinary_death_and_atomic_cleanup() {
    let (mut game, _) = fixture_with(true, |_, _, species| {
        let health = species
            .modifiers
            .iter_mut()
            .find(|modifier| {
                matches!(
                    modifier.op,
                    ModifierOp::Set {
                        stat: Stat::Health,
                        ..
                    }
                )
            })
            .expect("shared ray fixture supplies one actual Health override");
        assert_eq!(
            health.op,
            ModifierOp::Set {
                stat: Stat::Health,
                value: 100
            }
        );
        health.cancellable = true;
        health.op = ModifierOp::Set {
            stat: Stat::Health,
            value: 1000,
        };
    });
    assert_eq!(game.health(ActorId(2)).unwrap(), (1000, 1000));
    let mut baseline = game.clone();
    baseline
        .apply_effects(ActorId(2), &[Effect::CancelSpecials])
        .unwrap();
    assert!(baseline.alive(ActorId(2)));
    assert!(baseline.health(ActorId(2)).unwrap().1 < 999);
    let mut injury = game.actor_outcome(ActorId(2)).unwrap();
    injury
        .state
        .combat
        .as_mut()
        .unwrap()
        .with_recovery_context(None, |creature| {
            creature.damage(999);
            Ok::<(), crate::GameError>(())
        })
        .unwrap();
    game.apply_outcome(injury).unwrap();
    assert!(game.alive(ActorId(2)));
    assert_eq!(game.health(ActorId(2)).unwrap().0, 1);
    let mut positive = game.clone();
    positive
        .apply_effects(ActorId(2), &[Effect::CancelSpecials])
        .unwrap();
    assert!(!positive.alive(ActorId(2)));
    assert!(positive
        .items
        .iter()
        .any(|(_, item)| item.spec.class == crate::ItemClass::Corpse));
    game.next_item_id = u64::MAX;
    let before = game.clone();
    assert_eq!(
        game.apply_effects(ActorId(2), &[Effect::CancelSpecials]),
        Err(crate::GameError::IdentityExhausted)
    );
    assert_eq!(
        game, before,
        "no cancellation, injury, corpse or publication escapes failed cleanup"
    );
}

#[test]
fn cancellation_live_ai_healing_selection_preserves_unmarked_healing() {
    for tagged in [false, true] {
        let id = AbilityId::new("fixture_selective_healing").unwrap();
        let (mut game, _) = fixture_with(true, |document, ray, species| {
            let mut healing = ray.clone();
            healing.id = id.clone();
            healing.invocation = InvocationKind::ActorTarget;
            healing.reach = Reach::SelfOnly;
            healing.interaction = Interaction::NonHostile;
            healing.tags.clear();
            let StrikeSource::Authored { strikes } = &mut healing.strikes else {
                unreachable!()
            };
            strikes[0].cancellable = tagged;
            strikes[0].on_success = vec![Effect::Heal {
                amount: AmountExpr::Fixed { amount: 1 },
            }];
            species.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: id.clone(),
                },
            });
            document.abilities.push(healing);
        });
        let plan = game.ability_plan(ActorId(1), &id, ActorId(1)).unwrap();
        assert!(crate::ai::safe_healing_plan(&plan));
        assert!(game.effective_safe_healing_plan(&plan));
        game.apply_effects(ActorId(1), &[Effect::CancelSpecials])
            .unwrap();
        assert!(game.plan_source_valid(&plan));
        assert!(
            crate::ai::safe_healing_plan(&plan),
            "original frozen source is unchanged"
        );
        assert_eq!(game.effective_safe_healing_plan(&plan), !tagged);
    }
}

#[test]
fn paid_missile_uses_shared_bonus_and_energy_reduction_once() {
    let mut results = Vec::new();
    for variant in 0..3 {
        let (mut game, item) = fixture_with(false, |_, _, species| {
            if variant == 1 {
                let bonus = species
                    .modifiers
                    .iter_mut()
                    .find(|modifier| {
                        matches!(
                            modifier.op,
                            ModifierOp::Add {
                                stat: Stat::AttackDamageRollBonus,
                                value: 3
                            }
                        )
                    })
                    .unwrap();
                bonus.op = ModifierOp::Add {
                    stat: Stat::AttackDamageRollBonus,
                    value: 0,
                };
            } else if variant == 2 {
                species.modifiers.push(Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Reduction {
                        selector: Selector::Category {
                            category: crate::combat::DamageType::Energy,
                        },
                        amount: 2,
                    },
                });
            }
        });
        let before = game.health(ActorId(2)).unwrap().0;
        complete(&mut game, item);
        results.push((before - game.health(ActorId(2)).unwrap().0, game.rules_rng));
    }
    assert!((5..=8).contains(&results[0].0));
    assert_eq!(results[0].0 - results[1].0, 3, "one universal bonus");
    assert_eq!(
        results[0].0 - results[2].0,
        2,
        "one ordinary Energy reduction"
    );
    assert_eq!(results[0].1, results[1].1);
    assert_eq!(results[0].1, results[2].1);
}

#[test]
fn contact_captured_policies_reject_cross_kind_and_source_mismatches() {
    let plans: Vec<_> = [true, false]
        .into_iter()
        .map(|cancellation| {
            let (game, item) = fixture(cancellation);
            let plan = game
                .plan_work(
                    ActorId(1),
                    crate::Work::Zap {
                        item,
                        aim: crate::DeviceAim::Direction {
                            direction: StepDirection::East,
                        },
                    },
                )
                .unwrap();
            let ray = plan.as_ray().unwrap().clone();
            assert!(ray.valid(game.content.rules()));
            let value = serde_json::to_value(&ray).unwrap();
            assert_eq!(
                serde_json::from_value::<RayPlan>(value.clone()).unwrap(),
                ray
            );
            (game, ray, value)
        })
        .collect();
    for (index, (game, ray, value)) in plans.iter().enumerate() {
        for field in ["resolution", "traversal"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<RayPlan>(missing).is_err());
        }
        let mut wrong = ray.clone();
        wrong.resolution = plans[1 - index].1.resolution.clone();
        assert!(!wrong.valid(game.content.rules()));
        wrong = ray.clone();
        wrong.traversal = RayTraversal::Bounded { clear_visits: 6 };
        assert!(!wrong.valid(game.content.rules()));
        wrong = ray.clone();
        wrong.delivery = RayDelivery::Bouncing;
        assert!(!wrong.valid(game.content.rules()));
        wrong = ray.clone();
        wrong.ability = AbilityId::new("basic_melee").unwrap();
        assert!(!wrong.valid(game.content.rules()));
    }
    for field in ["tokens", "incoming", "source_bonus", "target_skill"] {
        let mut extra = plans[0].2.clone();
        extra["resolution"][field] = serde_json::json!(null);
        assert!(serde_json::from_value::<RayPlan>(extra).is_err());
    }
}

#[test]
fn contact_until_obstruction_is_not_a_bouncing_or_dummy_check_source() {
    for cancellation in [false, true] {
        let (game, _) = fixture(cancellation);
        let original = game.content.rules().document().clone();
        assert!(Ruleset::validate(original.clone()).is_ok());
        for change in 0..3 {
            let mut document = original.clone();
            let ability = document
                .abilities
                .iter_mut()
                .find(|ability| ability.id.as_str() == "fixture_ray")
                .unwrap();
            match change {
                0 => ability.invocation = InvocationKind::DirectionalRay,
                1 => ability.invocation = InvocationKind::ActorTarget,
                _ => {
                    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                        unreachable!()
                    };
                    strikes[0].check = Some(CheckDef {
                        actor: CheckActor::User,
                        skill: SkillSelector::Named { skill: Skill::Lore },
                        against: Against::Defense {
                            defense: DefenseKind::Physical,
                        },
                    });
                }
            }
            assert!(
                Ruleset::validate(document).is_err(),
                "cancellation {cancellation}, change {change}"
            );
        }
    }
}
