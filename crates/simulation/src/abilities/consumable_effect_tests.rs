//! Shared effects do not know potion names, species, or inventory policy.
use super::{PlannedAmount, PlannedEffect, ResolutionPlan, ResolvedEffect, Snapshot};
use crate::content_ids::ConditionId;
use crate::dice::{DicePool, Edge};
use crate::rules::{AmountExpr, Effect};
use crate::ActorId;
use serde_json::json;

fn input(effects: &[PlannedEffect]) -> ResolutionPlan<'_> {
    ResolutionPlan::Effects {
        actor: ActorId(1),
        target: ActorId(2),
        effects,
    }
}
fn creature() -> crate::creatures::CreatureState {
    crate::creatures::CreatureState::new(crate::test_creatures::build(
        crate::test_creatures::species(),
    ))
    .unwrap()
}

#[test]
fn turn_duration_rolls_once_and_frozen_ticks_survive_effect_roundtrip() {
    let dice = DicePool::new(1, 85, 807).unwrap();
    let effects = [PlannedEffect::ApplyConditionForTurns {
        id: ConditionId::new("readied").unwrap(),
        duration: PlannedAmount::Rolled { dice },
        turn_ticks: 37,
    }];
    let encoded = serde_json::to_vec(&effects).unwrap();
    let restored: Vec<PlannedEffect> = serde_json::from_slice(&encoded).unwrap();
    let mut expected_rng = 42;
    let ticks = u64::from(dice.roll(&mut expected_rng, Edge::default()).total) * 37;
    let mut rng = 42;
    let snapshot = Snapshot::capture(&creature());
    let result = super::resolve(input(&restored), &snapshot, &mut rng, &mut ()).unwrap();
    assert_eq!(rng, expected_rng);
    assert_eq!(
        result.effects,
        [ResolvedEffect::ApplyCondition {
            id: ConditionId::new("readied").unwrap(),
            duration: ticks
        }]
    );
    assert!((808 * 37..=892 * 37).contains(&ticks));
    assert!(snapshot.creature.conditions().is_empty());
}

#[test]
fn removal_cures_only_selected_condition_and_is_idempotent() {
    let mut creature = creature();
    let readied = ConditionId::new("readied").unwrap();
    let unconditioned = creature.derived().clone();
    creature.apply_condition(&readied, ActorId(2), 100).unwrap();
    assert_ne!(creature.derived(), &unconditioned);
    let effects = [PlannedEffect::RemoveCondition {
        id: readied.clone(),
    }];
    let snapshot = Snapshot::capture(&creature);
    let mut rng = 42;
    let result = super::resolve(input(&effects), &snapshot, &mut rng, &mut ()).unwrap();
    assert_eq!(
        result.effects,
        [ResolvedEffect::RemoveCondition {
            id: readied.clone()
        }]
    );
    assert_eq!(rng, 42);
    assert!(creature.remove_condition(&readied).unwrap());
    assert!(!creature.remove_condition(&readied).unwrap());
    assert!(creature.conditions().is_empty());
    assert_eq!(creature.derived(), &unconditioned);
    let result = super::resolve(
        input(&effects),
        &Snapshot::capture(&creature),
        &mut rng,
        &mut (),
    )
    .unwrap();
    assert!(result.effects.is_empty());
}

#[test]
fn new_effects_require_named_complete_records_and_exact_frozen_operands() {
    for raw in [
        r#"{"kind":"apply_condition_for_turns","condition":"readied","duration":{"kind":"rolled","dice":{"count":1,"sides":2,"bonus":18}}}"#,
        r#"{"kind":"remove_condition","condition":"readied"}"#,
        r#"{"kind":"heal_per_hit_die","overflow":"none","sides":4,"bonus":0}"#,
    ] {
        let effect: Effect = serde_json::from_str(raw).unwrap();
        assert!(effect.valid_shape());
        assert_eq!(
            serde_json::from_str::<Effect>(&serde_json::to_string(&effect).unwrap()).unwrap(),
            effect
        );
        let mut bad: serde_json::Value = serde_json::from_str(raw).unwrap();
        bad["unknown"] = json!(true);
        assert!(serde_json::from_value::<Effect>(bad).is_err());
    }
    let source = Effect::ApplyConditionForTurns {
        condition: ConditionId::new("readied").unwrap(),
        duration: AmountExpr::Fixed { amount: 19 },
    };
    let planned = PlannedEffect::ApplyConditionForTurns {
        id: ConditionId::new("readied").unwrap(),
        duration: PlannedAmount::Fixed { amount: 20 },
        turn_ticks: 100,
    };
    assert!(!super::plan::effects_match_definition(
        &[source],
        &[planned]
    ));
    for raw in [
        r#"["remove_condition","readied"]"#,
        r#"{"kind":"remove_condition","id":"readied","id":"other"}"#,
        r#"{"kind":"apply_condition_for_turns","id":"readied","duration":{"kind":"fixed","amount":1}}"#,
    ] {
        assert!(serde_json::from_str::<PlannedEffect>(raw).is_err(), "{raw}");
    }
    assert!(serde_json::from_str::<ResolvedEffect>(
        r#"{"kind":"remove_condition","id":"readied","extra":1}"#
    )
    .is_err());
}

#[test]
fn turn_duration_rejects_zero_or_overflow_before_rng_and_matches_source_exactly() {
    for (duration, ticks) in [
        (PlannedAmount::Fixed { amount: 0 }, 1),
        (PlannedAmount::Fixed { amount: 1 }, 0),
        (PlannedAmount::Fixed { amount: 2 }, u64::MAX),
        (
            PlannedAmount::Rolled {
                dice: DicePool::new(1, 2, -1).unwrap(),
            },
            100,
        ),
        (
            PlannedAmount::Rolled {
                dice: DicePool::new(1, 2, 0).unwrap(),
            },
            500_001,
        ),
    ] {
        let effects = [PlannedEffect::ApplyConditionForTurns {
            id: ConditionId::new("readied").unwrap(),
            duration,
            turn_ticks: ticks,
        }];
        let mut rng = 42;
        assert!(super::resolve(
            input(&effects),
            &Snapshot::capture(&creature()),
            &mut rng,
            &mut ()
        )
        .is_err());
        assert_eq!(rng, 42);
        assert!(serde_json::from_value::<PlannedEffect>(
            serde_json::to_value(&effects[0]).unwrap()
        )
        .is_err());
    }
}

#[test]
fn hit_die_healing_uses_independent_dice_through_hd256_and_applies_bonus_once() {
    for count in [1, 2, 64, 65, 256] {
        for bonus in [0, -400, 3] {
            let dice = super::plan::compile_hit_die_healing(count, 4, bonus).unwrap();
            assert_eq!(
                dice.iter().map(|pool| u64::from(pool.count())).sum::<u64>(),
                count
            );
            let effect = PlannedEffect::HealPerHitDie {
                dice,
                bonus,
                overflow: crate::rules::HealingOverflow::None,
            };
            let effect: PlannedEffect =
                serde_json::from_slice(&serde_json::to_vec(&effect).unwrap()).unwrap();
            let mut expected_rng = 91;
            let raw: i64 = (0..count)
                .map(|_| {
                    i64::from(
                        DicePool::new(1, 4, 0)
                            .unwrap()
                            .roll(&mut expected_rng, Edge::default())
                            .total,
                    )
                })
                .sum();
            let mut creature = creature();
            let available = u32::try_from(creature.health().current().saturating_sub(1)).unwrap();
            creature.damage(available);
            let mut rng = 91;
            let result = super::resolve(
                input(std::slice::from_ref(&effect)),
                &Snapshot::capture(&creature),
                &mut rng,
                &mut (),
            )
            .unwrap();
            assert_eq!(rng, expected_rng);
            assert_eq!(
                result.effects,
                [ResolvedEffect::Heal {
                    amount: ((raw + i64::from(bonus)).max(0) as u32).min(available)
                }]
            );
            assert!(super::plan::effects_match_definition(
                &[Effect::HealPerHitDie {
                    sides: 4,
                    bonus,
                    overflow: crate::rules::HealingOverflow::None
                }],
                std::slice::from_ref(&effect)
            ));
            assert!(!super::plan::effects_match_definition(
                &[Effect::HealPerHitDie {
                    sides: 5,
                    bonus,
                    overflow: crate::rules::HealingOverflow::None
                }],
                &[effect]
            ));
        }
    }
    assert!(super::plan::compile_hit_die_healing(0, 4, 0).is_err());
    assert!(super::plan::compile_hit_die_healing(257, 4, 0).is_err());
    assert!(super::plan::compile_hit_die_healing(256, 1000, 800_000).is_err());
}

#[test]
fn hit_die_healing_and_turn_pools_have_strict_canonical_persistence_shapes() {
    for raw in [
        r#"{"kind":"heal_per_hit_die","overflow":"none","dice":[],"bonus":0}"#,
        r#"{"kind":"heal_per_hit_die","overflow":"none","dice":[[1,4,0]],"bonus":0}"#,
        r#"{"kind":"heal_per_hit_die","overflow":"none","dice":[{"count":1,"sides":4,"bonus":0}]}"#,
        r#"{"kind":"heal_per_hit_die","overflow":"none","dice":[{"count":1,"sides":4,"bonus":0},{"count":1,"sides":4,"bonus":0}],"bonus":0}"#,
        r#"{"kind":"heal_per_hit_die","overflow":"none","dice":[{"count":1,"sides":4,"bonus":1}],"bonus":0}"#,
        r#"{"kind":"heal_per_hit_die","overflow":"none","dice":[{"count":64,"sides":4,"bonus":0},{"count":1,"sides":5,"bonus":0}],"bonus":0}"#,
        r#"{"kind":"apply_condition_for_turns","id":"readied","duration":{"kind":"rolled","dice":[1,2,18]},"turn_ticks":100}"#,
    ] {
        assert!(serde_json::from_str::<PlannedEffect>(raw).is_err(), "{raw}");
    }
    let oversized = json!({"kind":"heal_per_hit_die","overflow":"none","dice":vec![json!({"count":64,"sides":4,"bonus":0});5],"bonus":0});
    assert!(serde_json::from_value::<PlannedEffect>(oversized).is_err());
}

#[test]
fn compilation_freezes_source_turn_ticks_and_compiled_hit_dice_without_rng() {
    use std::num::NonZeroU64;
    let build = crate::test_creatures::build_with(
        crate::test_creatures::species(),
        vec![],
        &vec![crate::progression::HdSource::Racial; 65],
    );
    let mut game = crate::Game::two_room_in_stone(42, build.content().clone());
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(37).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "source".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    let effects = [
        Effect::HealPerHitDie {
            sides: 4,
            bonus: 0,
            overflow: crate::rules::HealingOverflow::None,
        },
        Effect::ApplyConditionForTurns {
            condition: ConditionId::new("readied").unwrap(),
            duration: AmountExpr::Rolled {
                dice: crate::rules::DiceExpr {
                    count: 1,
                    sides: 2,
                    bonus: 18,
                },
            },
        },
    ];
    let rng_before = game.rules_rng;
    let planned = game.effect_plan(actor, &effects).unwrap();
    assert_eq!(game.rules_rng, rng_before);
    let PlannedEffect::HealPerHitDie { dice, bonus, .. } = &planned[0] else {
        panic!("scaled healing")
    };
    assert_eq!(
        dice.iter().map(|pool| pool.count()).collect::<Vec<_>>(),
        [64, 1]
    );
    assert_eq!(*bonus, 0);
    assert!(matches!(
        &planned[1],
        PlannedEffect::ApplyConditionForTurns { turn_ticks: 37, .. }
    ));
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(75).unwrap();
    let restored: Vec<PlannedEffect> =
        serde_json::from_slice(&serde_json::to_vec(&planned).unwrap()).unwrap();
    assert_eq!(restored, planned);
    assert!(super::plan::effects_valid(&restored, &game.content.rules));
    assert!(super::plan::item_effects_match_definition(
        &effects, &restored
    ));
    assert!(matches!(
        &game.effect_plan(actor, &effects).unwrap()[1],
        PlannedEffect::ApplyConditionForTurns { turn_ticks: 75, .. }
    ));
    let invalid = [Effect::ApplyConditionForTurns {
        condition: ConditionId::new("readied").unwrap(),
        duration: AmountExpr::Fixed { amount: 1_000_000 },
    }];
    assert!(game.effect_plan(actor, &invalid).is_err());
    assert_eq!(game.rules_rng, rng_before);
}

#[test]
fn generic_effect_catalog_references_and_condition_stacking_remain_strict() {
    let rules = crate::test_creatures::ruleset();
    for effect in [
        Effect::RemoveCondition {
            condition: ConditionId::new("unknown_condition").unwrap(),
        },
        Effect::ApplyConditionForTurns {
            condition: ConditionId::new("unknown_condition").unwrap(),
            duration: AmountExpr::Fixed { amount: 1 },
        },
        Effect::ApplyConditionForTurns {
            condition: ConditionId::new("readied").unwrap(),
            duration: AmountExpr::Fixed { amount: 0 },
        },
        Effect::ApplyConditionForTurns {
            condition: ConditionId::new("readied").unwrap(),
            duration: AmountExpr::Rolled {
                dice: crate::rules::DiceExpr {
                    count: 1,
                    sides: 2,
                    bonus: -1,
                },
            },
        },
        Effect::HealPerHitDie {
            sides: 1,
            bonus: 0,
            overflow: crate::rules::HealingOverflow::None,
        },
    ] {
        assert!(rules.validate_effects(&[effect]).is_err());
    }
    assert_eq!(
        serde_json::from_str::<crate::rules::Stacking>(r#"{"kind":"extend"}"#).unwrap(),
        crate::rules::Stacking::Extend
    );
    for raw in [
        r#"["extend"]"#,
        r#"{"kind":"extend","maximum":2}"#,
        r#"{"kind":"extend","kind":"extend"}"#,
    ] {
        assert!(serde_json::from_str::<crate::rules::Stacking>(raw).is_err());
    }
    assert!(serde_json::from_str::<Effect>(r#"{"kind":"apply_condition_for_turns","condition":"readied","duration":{"kind":"rolled","dice":[1,2,18]}}"#).is_err());
}

#[test]
fn healing_overflow_policy_is_required_strict_and_source_matched() {
    for overflow in [
        crate::rules::HealingOverflow::None,
        crate::rules::HealingOverflow::ImproveEarliestHitDie,
    ] {
        let source = Effect::HealPerHitDie {
            sides: 4,
            bonus: 0,
            overflow,
        };
        let planned = PlannedEffect::HealPerHitDie {
            dice: vec![DicePool::new(1, 4, 0).unwrap()],
            bonus: 0,
            overflow,
        };
        assert!(super::plan::effects_match_definition(
            std::slice::from_ref(&source),
            std::slice::from_ref(&planned)
        ));
        for mut value in [
            serde_json::to_value(source).unwrap(),
            serde_json::to_value(planned).unwrap(),
        ] {
            let is_plan = value.get("dice").is_some();
            let accepts = |value: serde_json::Value| {
                if is_plan {
                    serde_json::from_value::<PlannedEffect>(value).is_ok()
                } else {
                    serde_json::from_value::<Effect>(value).is_ok()
                }
            };
            assert!(accepts(value.clone()));
            for bad in [
                json!(null),
                json!("unknown"),
                json!({"kind":"none"}),
                json!(["none"]),
            ] {
                let mut invalid = value.clone();
                invalid["overflow"] = bad;
                assert!(!accepts(invalid));
            }
            value.as_object_mut().unwrap().remove("overflow");
            assert!(!accepts(value));
        }
    }
    assert!(!super::plan::effects_match_definition(
        &[Effect::HealPerHitDie {
            sides: 4,
            bonus: 0,
            overflow: crate::rules::HealingOverflow::None
        }],
        &[PlannedEffect::HealPerHitDie {
            dice: vec![DicePool::new(1, 4, 0).unwrap()],
            bonus: 0,
            overflow: crate::rules::HealingOverflow::ImproveEarliestHitDie
        }]
    ));
}

fn persistent_creature() -> crate::creatures::CreatureState {
    use crate::creatures::{Content, CreatureBuild, CreatureState};
    use crate::rules::{ConditionLifecycle, ConditionTiming, Stacking};
    let original = crate::test_creatures::build(crate::test_creatures::species());
    let mut document = original.ruleset().document().clone();
    for name in ["fixture_up", "fixture_down"] {
        let mut definition = document
            .conditions
            .iter()
            .find(|entry| entry.id.as_str() == "readied")
            .unwrap()
            .clone();
        definition.id = ConditionId::new(name).unwrap();
        definition.family = crate::content_ids::ConditionFamilyId::new(name).unwrap();
        definition.lifecycle = ConditionLifecycle::Timed;
        definition.timing = ConditionTiming::UntilRemoved;
        definition.stacking = Stacking::Keep;
        document.conditions.push(definition);
    }
    let rules = crate::rules::Ruleset::validate(document).unwrap();
    let content = Content::new(rules, original.content().catalog().clone()).unwrap();
    CreatureState::new(
        CreatureBuild::new(
            content,
            original.species_id().clone(),
            original.ledger().clone(),
            original.binding(),
            *original.starting_skills(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn persistent_effect_repeats_are_noops_and_opposition_cancels_to_neutral() {
    let up = ConditionId::new("fixture_up").unwrap();
    let down = ConditionId::new("fixture_down").unwrap();
    let mut creature = persistent_creature();
    let baseline = creature.derived().clone();
    let apply = |creature: &mut crate::creatures::CreatureState,
                 id: &ConditionId,
                 opposite: &ConditionId| {
        let effects = [PlannedEffect::ApplyPersistentCondition {
            id: id.clone(),
            cancels: Some(opposite.clone()),
        }];
        let mut rng = 41;
        let outcome = super::resolve(
            input(&effects),
            &Snapshot::capture(creature),
            &mut rng,
            &mut (),
        )
        .unwrap();
        assert_eq!(
            rng, 41,
            "applying or cancelling persistence consumes no random values"
        );
        for effect in &outcome.effects {
            effect
                .apply_to(
                    &mut super::ActorEffectState {
                        creature,
                        observed_plan: None,
                        placement: None,
                        recovery_context: None,
                    },
                    ActorId(1),
                )
                .unwrap();
        }
        outcome
    };
    for (first, opposite) in [(&up, &down), (&down, &up)] {
        let outcome = apply(&mut creature, first, opposite);
        assert_eq!(
            outcome.effects,
            [ResolvedEffect::ApplyPersistentCondition { id: first.clone() }]
        );
        assert_ne!(
            creature.derived(),
            &baseline,
            "persistent condition remains a normal composed source"
        );
        let retained = creature.clone();
        assert!(apply(&mut creature, first, opposite).effects.is_empty());
        assert_eq!(creature, retained);
        let outcome = apply(&mut creature, opposite, first);
        assert_eq!(
            outcome.effects,
            [ResolvedEffect::RemoveCondition { id: first.clone() }]
        );
        assert!(
            creature.conditions().is_empty(),
            "opposition only cancels, without installing the new condition"
        );
        assert_eq!(creature.derived(), &baseline);
    }
    let effects = [
        PlannedEffect::ApplyPersistentCondition {
            id: up.clone(),
            cancels: None,
        },
        PlannedEffect::ApplyPersistentCondition {
            id: down,
            cancels: Some(up.clone()),
        },
    ];
    let outcome = super::resolve(
        input(&effects),
        &Snapshot::capture(&creature),
        &mut 41,
        &mut (),
    )
    .unwrap();
    assert_eq!(
        outcome.effects,
        [
            ResolvedEffect::ApplyPersistentCondition { id: up.clone() },
            ResolvedEffect::RemoveCondition { id: up }
        ],
        "later effects see the live candidate, not an obsolete pre-effect snapshot"
    );
}

#[test]
fn persistent_effect_records_freeze_opposition_and_require_complete_current_fields() {
    let creature = persistent_creature();
    let rules = creature.build().ruleset();
    let source = Effect::ApplyPersistentCondition {
        condition: ConditionId::new("fixture_up").unwrap(),
        cancels: Some(ConditionId::new("fixture_down").unwrap()),
    };
    let planned = PlannedEffect::ApplyPersistentCondition {
        id: ConditionId::new("fixture_up").unwrap(),
        cancels: Some(ConditionId::new("fixture_down").unwrap()),
    };
    rules
        .validate_effects(std::slice::from_ref(&source))
        .unwrap();
    assert!(super::plan::effects_valid(
        std::slice::from_ref(&planned),
        rules
    ));
    assert!(super::plan::item_effects_match_definition(
        std::slice::from_ref(&source),
        std::slice::from_ref(&planned)
    ));
    let mut changed = planned.clone();
    if let PlannedEffect::ApplyPersistentCondition { cancels, .. } = &mut changed {
        *cancels = None;
    }
    assert!(!super::plan::item_effects_match_definition(
        std::slice::from_ref(&source),
        &[changed]
    ));
    for opposite in [None, Some("fixture_down")] {
        for (kind, mut value) in [
            (0, serde_json::to_value(&source).unwrap()),
            (1, serde_json::to_value(&planned).unwrap()),
        ] {
            value["cancels"] = json!(opposite);
            let accepts = |value: serde_json::Value| {
                if kind == 0 {
                    serde_json::from_value::<Effect>(value).is_ok()
                } else {
                    serde_json::from_value::<PlannedEffect>(value).is_ok()
                }
            };
            assert!(accepts(value.clone()));
            for field in value.as_object().unwrap().keys() {
                let mut bad = value.clone();
                bad.as_object_mut().unwrap().remove(field);
                assert!(!accepts(bad), "missing {field}");
            }
            let mut bad = value.clone();
            bad["duration"] = json!(1);
            assert!(!accepts(bad));
            assert!(!accepts(json!([
                "apply_persistent_condition",
                "fixture_up",
                opposite
            ])));
        }
    }
    for invalid in [
        Effect::ApplyPersistentCondition {
            condition: ConditionId::new("readied").unwrap(),
            cancels: None,
        },
        Effect::ApplyPersistentCondition {
            condition: ConditionId::new("fixture_up").unwrap(),
            cancels: Some(ConditionId::new("readied").unwrap()),
        },
        Effect::ApplyPersistentCondition {
            condition: ConditionId::new("fixture_up").unwrap(),
            cancels: Some(ConditionId::new("fixture_up").unwrap()),
        },
        Effect::ApplyCondition {
            condition: ConditionId::new("fixture_up").unwrap(),
            duration: 10,
        },
        Effect::ApplyConditionForTurns {
            condition: ConditionId::new("fixture_up").unwrap(),
            duration: AmountExpr::Fixed { amount: 1 },
        },
    ] {
        assert!(rules.validate_effects(&[invalid]).is_err());
    }
}

#[test]
fn persistent_condition_survives_inactive_region_unload_restore_and_thaw() {
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Location, Position, RegionId};
    let creature = persistent_creature();
    let build = creature.build().clone();
    let mut game = crate::Game::two_room_in_stone(42, build.content().clone());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "subject".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    let id = ConditionId::new("fixture_up").unwrap();
    game.apply_persistent_condition(actor, &id, actor).unwrap();
    let original = game
        .creature(actor)
        .unwrap()
        .conditions()
        .instances()
        .next()
        .unwrap()
        .clone();
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(
        &crate::RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::new(),
        },
        &mut records,
    )
    .unwrap();
    assert!(game.creature(actor).is_none());
    game.tick = 1_000_000;
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let (checkpoint, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(checkpoint, shared)).unwrap()).unwrap();
    let mut restored =
        crate::Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    restored
        .transition_regions(
            &crate::RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records,
        )
        .unwrap();
    assert_eq!(
        restored
            .creature(actor)
            .unwrap()
            .conditions()
            .instances()
            .next(),
        Some(&original)
    );
    restored
        .apply_persistent_condition(actor, &id, actor)
        .unwrap();
    assert_eq!(
        restored
            .creature(actor)
            .unwrap()
            .conditions()
            .instances()
            .next(),
        Some(&original)
    );
}
