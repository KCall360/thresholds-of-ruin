use super::plan::*;
use super::*;
use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
use crate::costs::ResourceCost;
use crate::damage::{DamageComponent, DamageSpec};
use crate::dice::{roll_check, DicePool, Edge};
use crate::resources::Resource;
use crate::rules::{Against, AmountExpr, CheckActor, DamageExpr, Effect, Reach, TimingSource};
use crate::{ActorId, Game, GameError};

fn snapshot() -> Snapshot {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    let mut species = crate::test_creatures::species();
    species.modifiers = vec![
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::Health,
                value: 10,
            },
        },
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::ResourceMax {
                    resource: Resource::Stamina,
                },
                value: 10,
            },
        },
    ];
    Snapshot::capture(
        &crate::creatures::CreatureState::new(crate::test_creatures::build(species)).unwrap(),
    )
}
fn plan(effects: Vec<PlannedEffect>) -> AbilityPlan {
    AbilityPlan {
        actor: ActorId(1),
        target: ActorId(2),
        ability: AbilityId::new("basic_melee").unwrap(),
        preparation: 1,
        recovery: 0,
        cost: None,
        reach: Reach::Melee,
        check: None,
        on_success: effects,
        on_failure: vec![],
        descriptors: vec![],
        tokens: vec![],
        edge: 0,
    }
}
fn captured_plan(name: &str) -> (Game, Plan) {
    captured_plan_with_modifiers(name, vec![])
}
fn captured_plan_with_modifiers(
    name: &str,
    modifiers: Vec<crate::rules::Modifier>,
) -> (Game, Plan) {
    use crate::rules::{Modifier, ModifierOp, Scope};
    let id = AbilityId::new(name).unwrap();
    let mut species = crate::test_creatures::species();
    species.modifiers.extend(modifiers);
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: id.clone(),
        },
    });
    let build = crate::test_creatures::build(species);
    let mut game = Game::two_room_in_stone(42, build.content().clone());
    let mut actors = vec![];
    for x in [1, 2] {
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: format!("subject {x}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
        actors.push(actor);
    }
    let plan = game.ability_plan(actors[0], &id, actors[1]).unwrap();
    (game, plan)
}

fn fixed(amount: u32) -> PlannedAmount {
    PlannedAmount::Fixed { amount }
}

#[test]
fn check_only_edges_never_spill_into_or_refund_shared_damage_edge() {
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    for (shared, combined_check, expected_damage_edge) in [
        (0, 2, 0),
        (3, 4, 0),
        (3, 2, 0),
        (5, 6, 2),
        (5, 1, 2),
        (5, -1, 2),
        (-5, 1, -2),
        (-3, -4, 0),
        (-5, -6, -2),
    ] {
        let mut plan = plan(vec![PlannedEffect::Damage {
            damage: DamageSpec::new(
                vec![DamageComponent::rolled(
                    crate::combat::DamageType::Impact,
                    None,
                    DicePool::new(2, 6, 0).unwrap(),
                )],
                None,
            )
            .unwrap(),
        }]);
        plan.edge = shared;
        plan.check = Some(PlannedCheck {
            actor: CheckActor::User,
            bonus: 0,
            threshold: CheckThreshold::Fixed { value: 0 },
            edge: combined_check,
        });
        let mut trace = ResolutionTrace::default();
        resolve(&plan, &snapshot(), &mut 42, &mut trace);
        let component = trace
            .steps()
            .iter()
            .find(|step| step.id == "damage.component")
            .unwrap();
        assert_eq!(
            component
                .fields
                .iter()
                .find(|field| field.id == "edge")
                .unwrap()
                .value,
            TraceValueRecord::Signed(expected_damage_edge),
            "shared={shared}, check={combined_check}"
        );
    }
}

#[test]
fn shared_edge_owner_and_canonical_components_survive_plan_roundtrip() {
    use crate::combat::DamageType;
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    for (actor, opposed, expected) in [
        (None, false, [2, 1, 2]),
        (Some(CheckActor::Target), false, [2, 1, 2]),
        (Some(CheckActor::Target), true, [2, 0, 0]),
        (Some(CheckActor::User), false, [2, 0, 0]),
        (Some(CheckActor::User), true, [2, 0, 0]),
    ] {
        for check_edge in [-7, 0, 7] {
            let primary =
                DamageComponent::rolled(DamageType::Impact, None, DicePool::new(2, 2, 0).unwrap());
            let mut prepared = plan(vec![
                PlannedEffect::Damage {
                    // Energy sorts before Impact, but the authored primary
                    // must receive shared edge first, even after serialization.
                    damage: DamageSpec::new(
                        vec![
                            DamageComponent::rolled(
                                DamageType::Energy,
                                None,
                                DicePool::new(1, 2, 0).unwrap(),
                            ),
                            primary.clone(),
                        ],
                        Some(primary.key()),
                    )
                    .unwrap(),
                },
                PlannedEffect::Damage {
                    damage: DamageSpec::new(
                        vec![DamageComponent::rolled(
                            DamageType::Keen,
                            None,
                            DicePool::new(2, 2, 0).unwrap(),
                        )],
                        None,
                    )
                    .unwrap(),
                },
            ]);
            prepared.edge = 5;
            prepared.check = actor.map(|actor| PlannedCheck {
                actor,
                bonus: 100,
                threshold: if opposed {
                    CheckThreshold::Opposed {
                        bonus: -100,
                        edge: -check_edge,
                    }
                } else {
                    CheckThreshold::Fixed { value: 0 }
                },
                edge: check_edge,
            });
            let restored: AbilityPlan =
                serde_json::from_str(&serde_json::to_string(&prepared).unwrap()).unwrap();
            assert_eq!(restored, prepared);
            let mut traces = [ResolutionTrace::default(), ResolutionTrace::default()];
            let mut seeds = [42, 42];
            let original = resolve(&prepared, &snapshot(), &mut seeds[0], &mut traces[0]);
            let replayed = resolve(&restored, &snapshot(), &mut seeds[1], &mut traces[1]);
            assert_eq!(original, replayed);
            assert_eq!(seeds[0], seeds[1]);
            assert_eq!(traces[0], traces[1]);
            let components: Vec<_> = traces[0]
                .steps()
                .iter()
                .filter(|step| step.id == "damage.component")
                .map(|step| {
                    let field = |id| {
                        &step
                            .fields
                            .iter()
                            .find(|field| field.id == id)
                            .unwrap()
                            .value
                    };
                    (field("category").clone(), field("edge").clone())
                })
                .collect();
            assert_eq!(
                components,
                ["impact", "energy", "keen"]
                    .into_iter()
                    .zip(expected)
                    .map(|(name, edge)| (
                        TraceValueRecord::Text(name.into()),
                        TraceValueRecord::Signed(edge)
                    ))
                    .collect::<Vec<_>>(),
                "actor={actor:?}, opposed={opposed}, check={check_edge}"
            );
        }
    }
}

#[test]
fn public_application_rolls_back_partial_effects_and_failed_death_after_settlement() {
    let mut game = Game::two_room(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    crate::test_creatures::configure(
        &mut game,
        actor,
        "neutral",
        crate::test_creatures::species(),
    );
    game.apply_effects(
        actor,
        &[Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![crate::rules::DamageComponentExpr {
                    category: crate::combat::DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 1 },
                }],
            },
        }],
    )
    .unwrap();
    let before = game.clone();
    assert!(game
        .apply(Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            effects: vec![
                ResolvedEffect::Heal { amount: 1 },
                ResolvedEffect::ApplyCondition {
                    id: ConditionId::new("unknown_condition").unwrap(),
                    duration: 1
                }
            ],
        })
        .is_err());
    assert_eq!(
        game, before,
        "failed second effect must roll back the first effect and event"
    );
    game.apply_condition(actor, &ConditionId::new("fear").unwrap(), actor, 1)
        .unwrap();
    game.tick += 2;
    game.next_item_id = u64::MAX;
    let before = game.clone();
    let lethal = Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![crate::rules::DamageComponentExpr {
                category: crate::combat::DamageType::Vital,
                descriptor: None,
                amount: AmountExpr::Rolled {
                    dice: crate::rules::DiceExpr {
                        count: 1,
                        sides: 6,
                        bonus: 1000,
                    },
                },
            }],
        },
    };
    assert_eq!(
        game.apply_effects(actor, &[lethal]),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(
        game, before,
        "failed death must roll back settlement, RNG, injury and events"
    );
}

#[test]
fn final_plan_and_timing_bounds_reject_plus_one_without_mutation() {
    use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding};
    use crate::creatures::{Content, CreatureBuild, ScenarioCatalog};
    use crate::rules::{AbilityStat, Formula, Modifier, ModifierOp, Scope, Stat, Timing};
    for (preparation_adjustment, strength, timing_ok, plan_ok) in [
        (0, 0, true, true),
        (1, 0, false, false),
        (0, 1, true, false),
    ] {
        let mut document = crate::test_creatures::ruleset().document().clone();
        let mut definition = document
            .abilities
            .iter()
            .find(|value| value.basic_attack)
            .unwrap()
            .clone();
        definition.id = AbilityId::new("fixture_limits").unwrap();
        definition.basic_attack = false;
        definition.timing = Timing {
            source: TimingSource::Fixed {
                prepare: crate::limits::NUMERIC as u64,
                recover: 1,
            },
            scaled_by: None,
        };
        definition.check.as_mut().unwrap().against = Against::Difficulty {
            formula: Formula {
                base: crate::limits::NUMERIC as i32,
                skill: None,
                attribute: Some(Attribute::Strength),
            },
        };
        let ability = definition.id.clone();
        document.abilities.push(definition);
        let rules = crate::rules::Ruleset::validate(document).unwrap();
        let mut species = crate::test_creatures::species();
        species.default_attributes = Attributes::new(ByAttribute {
            strength,
            ..ByAttribute::default()
        })
        .unwrap();
        species.modifiers.extend([
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: ability.clone(),
                },
            },
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Ability {
                        ability: ability.clone(),
                        stat: AbilityStat::Preparation,
                    },
                    value: preparation_adjustment,
                },
            },
        ]);
        let species_id = species.id.clone();
        let content = Content::new(
            rules,
            ScenarioCatalog {
                species: std::collections::BTreeMap::from([(species_id.clone(), species)]),
                templates: Default::default(),
            },
        )
        .unwrap();
        let ledger = crate::test_creatures::build(crate::test_creatures::species())
            .ledger()
            .clone();
        let build = CreatureBuild::new(
            content.clone(),
            species_id,
            ledger,
            ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        );
        if !timing_ok {
            assert_eq!(build, Err(crate::creatures::BuildError::DerivedLimit));
            continue;
        }
        let build = build.unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        let mut actors = vec![];
        for x in [1, 2] {
            let actor = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    std::num::NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
            actors.push(actor);
        }
        let before = game.clone();
        assert_eq!(game.ability_timing(actors[0], &ability).is_ok(), timing_ok);
        let result = game.ability_plan(actors[0], &ability, actors[1]);
        assert_eq!(result.is_ok(), plan_ok);
        if let Ok(plan) = result {
            assert!(plan.valid(game.content().rules()));
        }
        assert_eq!(game, before);
    }
}

#[test]
fn saved_ability_plan_requires_matching_bounded_reach() {
    let rules = crate::test_creatures::ruleset();
    for name in ["basic_melee", "magic_bolt"] {
        let (_, original) = captured_plan(name);
        assert!(original.valid(&rules));
        for reach in [
            serde_json::Value::Null,
            serde_json::json!({"kind":"sight","range":0}),
            serde_json::json!({"kind":"sight","range":u16::MAX}),
            serde_json::json!({"kind":"self_only"}),
        ] {
            let mut record = serde_json::to_value(&original).unwrap();
            record["reach"] = reach;
            if let Ok(restored) = serde_json::from_value::<Plan>(record) {
                assert!(!restored.valid(&rules));
            }
        }
    }
    assert!(
        effects_valid(&[], &rules),
        "environmental effects do not require player reach"
    );
}

#[test]
fn saved_paid_plan_cannot_remove_or_change_immutable_rule_contract() {
    let (game, original) = captured_plan("fear");
    let rules = game.content().rules();
    assert!(original.valid(rules));
    for case in 0..8 {
        let mut forged = original.clone();
        let Plan::Ability(ability) = &mut forged else {
            panic!("ability plan")
        };
        match case {
            0 => ability.cost = None,
            1 => ability.cost.as_mut().unwrap().resource = Resource::Stamina,
            2 => ability.descriptors.push(DescriptorId::new("fire").unwrap()),
            3 => ability.check = None,
            4 => {
                let check = ability.check.as_mut().unwrap();
                check.actor = match check.actor {
                    CheckActor::User => CheckActor::Target,
                    CheckActor::Target => CheckActor::User,
                };
            }
            5 => {
                ability.check.as_mut().unwrap().threshold =
                    CheckThreshold::Opposed { bonus: 0, edge: 0 }
            }
            6 => ability.on_success.clear(),
            7 => ability.on_success[0] = PlannedEffect::Heal { amount: fixed(1) },
            _ => unreachable!(),
        }
        let restored: Plan = serde_json::from_value(serde_json::to_value(forged).unwrap()).unwrap();
        assert!(!restored.valid(rules), "forged contract case {case}");
    }
}

#[test]
fn captured_component_damage_keeps_immutable_keys_and_primary_but_allows_derived_dice() {
    use crate::combat::DamageType;
    let fire = DescriptorId::new("fire").unwrap();
    let expected = vec![Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![
                crate::rules::DamageComponentExpr {
                    category: DamageType::Energy,
                    descriptor: Some(fire.clone()),
                    amount: AmountExpr::Rolled {
                        dice: crate::rules::DiceExpr {
                            count: 1,
                            sides: 6,
                            bonus: 1,
                        },
                    },
                },
                crate::rules::DamageComponentExpr {
                    category: DamageType::Impact,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 3 },
                },
            ],
        },
    }];
    for case in 0..7 {
        let first = if case == 4 {
            DamageComponent::fixed(DamageType::Energy, Some(fire.clone()), 3)
        } else {
            DamageComponent::rolled(
                if case == 2 {
                    DamageType::Vital
                } else {
                    DamageType::Energy
                },
                Some(if case == 1 {
                    DescriptorId::new("cold").unwrap()
                } else {
                    fire.clone()
                }),
                DicePool::new(3, if case == 3 { 8 } else { 6 }, 5).unwrap(),
            )
        };
        let second =
            DamageComponent::fixed(DamageType::Impact, None, if case == 6 { 4 } else { 3 });
        let primary = if case == 5 { second.key() } else { first.key() };
        let damage = DamageSpec::new(vec![first, second], Some(primary)).unwrap();
        let saved = serde_json::to_value(PlannedEffect::Damage { damage }).unwrap();
        let restored: PlannedEffect = serde_json::from_value(saved).unwrap();
        assert_eq!(
            effects_match_definition(&expected, &[restored]),
            case == 0,
            "component contract case {case}"
        );
    }
}

#[test]
fn captured_item_effects_reject_changed_duration_and_dice_operands() {
    use crate::combat::DamageType;
    let id = ConditionId::new("fear").unwrap();
    let expected = vec![
        Effect::ApplyCondition {
            condition: id.clone(),
            duration: 5,
        },
        Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![crate::rules::DamageComponentExpr {
                    category: DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Rolled {
                        dice: crate::rules::DiceExpr {
                            count: 1,
                            sides: 6,
                            bonus: 2,
                        },
                    },
                }],
            },
        },
    ];
    for case in 0..4 {
        let component = DamageComponent::rolled(
            DamageType::Vital,
            None,
            DicePool::new(
                if case == 2 { 2 } else { 1 },
                6,
                if case == 3 { 100 } else { 2 },
            )
            .unwrap(),
        );
        let primary = component.key();
        let effects = vec![
            PlannedEffect::ApplyCondition {
                id: id.clone(),
                duration: if case == 1 { 1_000_000 } else { 5 },
            },
            PlannedEffect::Damage {
                damage: DamageSpec::new(vec![component], Some(primary)).unwrap(),
            },
        ];
        let restored: Vec<PlannedEffect> =
            serde_json::from_value(serde_json::to_value(effects).unwrap()).unwrap();
        assert!(effects_match_definition(&expected, &restored));
        assert_eq!(
            item_effects_match_definition(&expected, &restored),
            case == 0,
            "item numeric contract case {case}"
        );
    }
}

#[test]
fn captured_damage_rejects_undeclared_descriptor_after_record_round_trip() {
    let snapshot = snapshot();
    let rules = snapshot.creature.build().ruleset();
    let damage = DamageSpec::new(
        vec![DamageComponent::fixed(
            crate::combat::DamageType::Energy,
            Some(DescriptorId::new("fire").unwrap()),
            3,
        )],
        None,
    )
    .unwrap();
    let original = plan(vec![PlannedEffect::Damage { damage }]);
    assert!(effects_valid(&original.on_success, rules));
    let mut saved = serde_json::to_value(&original).unwrap();
    let damage = &mut saved["on_success"][0]["damage"];
    damage["components"][0]["descriptor"] = serde_json::json!("undeclared_energy");
    let restored: AbilityPlan = serde_json::from_value(saved).unwrap();
    assert!(!effects_valid(&restored.on_success, rules));
}

#[test]
fn pure_resolver_threshold_has_no_automatic_extremes() {
    for (roll, bonus, threshold, expected) in [
        (3, 7, 10, OutcomeStatus::Applied),
        (18, 0, 19, OutcomeStatus::Miss),
        (9, 0, 10, OutcomeStatus::Miss),
        (10, 0, 10, OutcomeStatus::Applied),
    ] {
        let mut rng = (0..10_000_u64)
            .find(|seed| {
                let mut state = *seed;
                roll_check(&mut state, Edge::default()).kept == roll
            })
            .expect("bounded deterministic seeds cover each 3d6 total");
        let mut snapshot = snapshot();
        snapshot.creature.damage(1);
        let mut plan = plan(vec![PlannedEffect::Heal { amount: fixed(1) }]);
        plan.check = Some(PlannedCheck {
            actor: CheckActor::User,
            bonus,
            threshold: CheckThreshold::Fixed { value: threshold },
            edge: 0,
        });
        let mut trace = crate::resolution_diagnostics::ResolutionTrace::default();
        let outcome = resolve(&plan, &snapshot, &mut rng, &mut trace);
        assert_eq!(outcome.result, expected, "natural roll {roll}");
        assert!(trace.steps().iter().any(|step| step.id == "check"
            && step.fields.iter().any(|field| field.id == "roll"
                && field.value
                    == crate::resolution_diagnostics::TraceValueRecord::Unsigned(u64::from(
                        roll
                    )))));
    }
}

#[test]
fn opposed_check_trace_retains_actual_dice_and_does_not_change_resolution() {
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    let mut snapshot = snapshot();
    snapshot.creature.damage(1);
    let mut plan = plan(vec![PlannedEffect::Heal { amount: fixed(1) }]);
    plan.check = Some(PlannedCheck {
        actor: CheckActor::User,
        bonus: 20,
        threshold: CheckThreshold::Opposed { bonus: 3, edge: 1 },
        edge: 0,
    });
    let (mut ordinary_rng, mut observed_rng) = (7, 7);
    let ordinary = resolve(&plan, &snapshot, &mut ordinary_rng, &mut ());
    let mut trace = ResolutionTrace::default();
    let observed = resolve(&plan, &snapshot, &mut observed_rng, &mut trace);
    assert_eq!((ordinary, ordinary_rng), (observed, observed_rng));
    let opposed = trace
        .steps()
        .iter()
        .find(|step| step.id == "opposed_check")
        .unwrap();
    let field = |id| {
        &opposed
            .fields
            .iter()
            .find(|field| field.id == id)
            .unwrap()
            .value
    };
    let TraceValueRecord::Dice(draws) = field("rolled") else {
        panic!("actual opposed draws")
    };
    assert_eq!(draws.len(), 4);
    let mut retained = draws.clone();
    retained.sort_unstable_by(|left, right| right.cmp(left));
    retained.truncate(3);
    assert_eq!(field("retained"), &TraceValueRecord::Dice(retained.clone()));
    assert_eq!(
        field("kept"),
        &TraceValueRecord::Unsigned(retained.iter().map(|&value| u64::from(value)).sum())
    );
    let total = field("total");
    assert!(trace
        .steps()
        .iter()
        .find(|step| step.id == "check")
        .unwrap()
        .fields
        .iter()
        .any(|field| field.id == "threshold" && &field.value == total));
}

#[test]
fn target_opposed_check_spends_caster_edge_before_either_damage_branch() {
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    for bonus in [-100, 100] {
        let damage = DamageSpec::new(
            vec![DamageComponent::rolled(
                crate::combat::DamageType::Impact,
                None,
                DicePool::new(1, 6, 0).unwrap(),
            )],
            None,
        )
        .unwrap();
        let mut plan = plan(vec![PlannedEffect::Damage {
            damage: damage.clone(),
        }]);
        plan.on_failure = vec![PlannedEffect::Damage { damage }];
        plan.edge = 1;
        plan.check = Some(PlannedCheck {
            actor: CheckActor::Target,
            bonus,
            threshold: CheckThreshold::Opposed { bonus: 0, edge: 1 },
            edge: 0,
        });
        let snapshot = snapshot();
        let (mut ordinary_rng, mut observed_rng) = (7, 7);
        let ordinary = resolve(&plan, &snapshot, &mut ordinary_rng, &mut ());
        let mut trace = ResolutionTrace::default();
        let observed = resolve(&plan, &snapshot, &mut observed_rng, &mut trace);
        assert_eq!((ordinary, ordinary_rng), (observed, observed_rng));
        let opposed = trace
            .steps()
            .iter()
            .find(|step| step.id == "opposed_check")
            .unwrap();
        assert!(opposed.fields.iter().any(|field| field.id == "rolled"
            && matches!(&field.value, TraceValueRecord::Dice(values) if values.len() == 4)));
        let damage = trace
            .steps()
            .iter()
            .find(|step| step.id == "damage.component")
            .unwrap();
        assert!(damage.fields.iter().any(|field| field.id == "rolled"
            && matches!(&field.value, TraceValueRecord::Dice(values) if values.len() == 1)));
        assert!(damage
            .fields
            .iter()
            .any(|field| field.id == "edge" && field.value == TraceValueRecord::Signed(0)));
    }
}

#[test]
fn fixed_damage_preserves_edge_and_rolled_pools_return_the_unused_budget() {
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    let rolled = DamageSpec::new(
        vec![DamageComponent::rolled(
            crate::combat::DamageType::Impact,
            None,
            DicePool::new(1, 6, 0).unwrap(),
        )],
        None,
    )
    .unwrap();
    let fixed_zero = DamageSpec::new(
        vec![DamageComponent::fixed(
            crate::combat::DamageType::Impact,
            None,
            0,
        )],
        None,
    )
    .unwrap();
    for edge in [-1, 1] {
        let mut plain = plan(vec![PlannedEffect::Damage {
            damage: rolled.clone(),
        }]);
        plain.edge = edge;
        let mut with_zero = plain.clone();
        with_zero.on_success.insert(
            0,
            PlannedEffect::Damage {
                damage: fixed_zero.clone(),
            },
        );
        let (mut ordinary_rng, mut observed_rng) = (7, 7);
        let baseline = resolve(&plain, &snapshot(), &mut ordinary_rng, &mut ());
        let observed = resolve(
            &with_zero,
            &snapshot(),
            &mut observed_rng,
            &mut ResolutionTrace::default(),
        );
        assert_eq!(ordinary_rng, observed_rng);
        assert_eq!(baseline.result, observed.result);
        assert_eq!(observed.effects[0], ResolvedEffect::Damage { amount: 0 });
        assert_eq!(baseline.effects, observed.effects[1..]);
    }
    for edge in [-2, 2] {
        let mut plan = plan(vec![
            PlannedEffect::Damage {
                damage: rolled.clone(),
            },
            PlannedEffect::Damage {
                damage: rolled.clone(),
            },
        ]);
        plan.edge = edge;
        let mut trace = ResolutionTrace::default();
        let (mut ordinary_rng, mut observed_rng) = (7, 7);
        let ordinary = resolve(&plan, &snapshot(), &mut ordinary_rng, &mut ());
        let observed = resolve(&plan, &snapshot(), &mut observed_rng, &mut trace);
        assert_eq!((ordinary, ordinary_rng), (observed, observed_rng));
        let draw_counts: Vec<_> = trace
            .steps()
            .iter()
            .filter(|step| step.id == "damage.component")
            .map(|step| {
                match &step
                    .fields
                    .iter()
                    .find(|field| field.id == "rolled")
                    .unwrap()
                    .value
                {
                    TraceValueRecord::Dice(values) => values.len(),
                    _ => panic!("damage draws"),
                }
            })
            .collect();
        assert_eq!(
            draw_counts,
            [2, 2],
            "leftover edge {edge} reaches the second one-die pool"
        );
    }
}

#[test]
fn nonweapon_effects_compile_without_a_creature_or_selected_weapon() {
    let mut game = Game::two_room(42, crate::test_creatures::content());
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    assert!(game.creature(actor).is_none());
    let effects = game
        .compile_effects(
            actor,
            &[
                Effect::Heal {
                    amount: AmountExpr::Fixed { amount: 3 },
                },
                Effect::Restore {
                    resource: Resource::Stamina,
                    amount: AmountExpr::Fixed { amount: 4 },
                },
            ],
            &crate::creatures::AbilityValues::default(),
        )
        .unwrap();
    assert_eq!(
        effects,
        vec![
            PlannedEffect::Heal { amount: fixed(3) },
            PlannedEffect::Restore {
                resource: Resource::Stamina,
                amount: fixed(4)
            }
        ]
    );
}

#[test]
fn full_health_and_resources_report_no_applied_change() {
    let result = resolve(
        &plan(vec![
            PlannedEffect::Heal { amount: fixed(100) },
            PlannedEffect::Restore {
                resource: Resource::Stamina,
                amount: fixed(100),
            },
        ]),
        &snapshot(),
        &mut 7,
        &mut (),
    );
    assert_eq!(result.result, OutcomeStatus::Unaffected);
    assert_eq!(
        result.effects,
        vec![
            ResolvedEffect::Heal { amount: 0 },
            ResolvedEffect::Restore {
                resource: Resource::Stamina,
                amount: 0
            }
        ]
    );
}

#[test]
fn repeated_effects_report_sequential_actual_healing_and_restoration() {
    let mut snapshot = snapshot();
    snapshot.creature.damage(7);
    snapshot
        .creature
        .start_cost(
            crate::IntentionId(1),
            ResourceCost {
                resource: Resource::Stamina,
                start: 3,
                resolution: 0,
            },
        )
        .unwrap();
    let result = resolve(
        &plan(vec![
            PlannedEffect::Heal { amount: fixed(5) },
            PlannedEffect::Heal { amount: fixed(5) },
            PlannedEffect::Restore {
                resource: Resource::Stamina,
                amount: fixed(2),
            },
            PlannedEffect::Restore {
                resource: Resource::Stamina,
                amount: fixed(2),
            },
        ]),
        &snapshot,
        &mut 7,
        &mut (),
    );
    assert_eq!(result.result, OutcomeStatus::Applied);
    assert_eq!(
        result.effects,
        vec![
            ResolvedEffect::Heal { amount: 5 },
            ResolvedEffect::Heal { amount: 2 },
            ResolvedEffect::Restore {
                resource: Resource::Stamina,
                amount: 2
            },
            ResolvedEffect::Restore {
                resource: Resource::Stamina,
                amount: 1
            }
        ]
    );
}

#[test]
fn unchanged_condition_refresh_reports_no_applied_change() {
    let mut snapshot = snapshot();
    let id = ConditionId::new("fear").unwrap();
    snapshot
        .creature
        .apply_condition(&id, ActorId(1), 100)
        .unwrap();
    let before = snapshot.clone();
    let result = resolve(
        &plan(vec![PlannedEffect::ApplyCondition { id, duration: 100 }]),
        &snapshot,
        &mut 7,
        &mut (),
    );
    assert_eq!(result.result, OutcomeStatus::Unaffected);
    assert!(result.effects.is_empty());
    assert_eq!(snapshot, before);
}

#[test]
fn lethal_damage_clamps_overkill_and_stops_later_effects_and_rng_draws() {
    let mut snapshot = snapshot();
    snapshot.creature.damage(7);
    let damage = DamageSpec::new(
        vec![DamageComponent::fixed(
            crate::combat::DamageType::Impact,
            None,
            100,
        )],
        None,
    )
    .unwrap();
    let mut rng = 7;
    let result = resolve(
        &plan(vec![
            PlannedEffect::Damage { damage },
            PlannedEffect::Heal {
                amount: PlannedAmount::Rolled {
                    dice: DicePool::new(1, 6, 0).unwrap(),
                },
            },
        ]),
        &snapshot,
        &mut rng,
        &mut (),
    );
    assert_eq!(result.result, OutcomeStatus::Applied);
    assert_eq!(result.effects, vec![ResolvedEffect::Damage { amount: 3 }]);
    assert_eq!(rng, 7);
}

#[test]
fn typed_item_plans_round_trip_and_reject_ability_only_fields_at_decode() {
    use crate::{EquipmentSlotId, ItemId, Work};
    let actor = ActorId(1);
    let item = ItemId(2);
    let rules = crate::test_creatures::ruleset();
    let plans = [
        Plan::Equip {
            actor,
            item,
            slot: EquipmentSlotId(0),
            preparation: 10,
        },
        Plan::Unequip {
            actor,
            item,
            preparation: 20,
        },
        Plan::Drink {
            actor,
            item,
            preparation: 30,
            effects: vec![PlannedEffect::Heal { amount: fixed(2) }],
        },
    ];
    for plan in plans {
        assert!(plan.valid(&rules));
        assert_eq!(plan.actor(), actor);
        assert_eq!(plan.target(), actor);
        assert_eq!(plan.cost(), None);
        assert_eq!(plan.recovery(), 0);
        let encoded = serde_json::to_value(&plan).unwrap();
        let restored: Plan = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(restored, plan);
        assert_eq!(restored.work(), plan.work());
        assert_eq!(
            plan.resolution().is_some(),
            matches!(plan.work(), Work::Drink { .. })
        );
        for field in [
            "ability",
            "target",
            "cost",
            "reach",
            "check",
            "on_failure",
            "descriptors",
            "edge",
            "recovery",
        ] {
            let mut forged = encoded.clone();
            forged[field] = serde_json::Value::Null;
            assert!(
                serde_json::from_value::<Plan>(forged).is_err(),
                "accepted {field} on {}",
                plan.work().name()
            );
        }
    }
}

#[test]
fn typed_ability_plan_has_one_identity_and_requires_its_reach() {
    let (game, plan) = captured_plan("fear");
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(encoded["kind"], "ability");
    assert_eq!(encoded["ability"], "fear");
    assert!(encoded.get("work").is_none());
    assert!(encoded.get("resolution").is_none());
    let restored: Plan = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored, plan);
    assert!(restored.valid(game.content().rules()));
    for field in ["ability", "reach", "target"] {
        let mut forged = encoded.clone();
        forged.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<Plan>(forged).is_err(),
            "accepted missing {field}"
        );
    }
}

#[test]
fn unconditional_resolution_has_no_ability_identity_and_preserves_observer_equivalence() {
    let effects = vec![PlannedEffect::Damage {
        damage: DamageSpec::new(
            vec![DamageComponent::rolled(
                crate::combat::DamageType::Impact,
                None,
                DicePool::new(2, 6, 0).unwrap(),
            )],
            None,
        )
        .unwrap(),
    }];
    let plan = ResolutionPlan::Effects {
        actor: ActorId(1),
        target: ActorId(2),
        effects: &effects,
    };
    let (mut ordinary_rng, mut observed_rng) = (42, 42);
    let ordinary = resolve(plan, &snapshot(), &mut ordinary_rng, &mut ());
    let mut trace = crate::resolution_diagnostics::ResolutionTrace::default();
    let observed = resolve(plan, &snapshot(), &mut observed_rng, &mut trace);
    assert_eq!(ordinary, observed);
    assert_eq!(ordinary_rng, observed_rng);
    assert_eq!(ordinary.ability, None);
    assert_eq!((ordinary.actor, ordinary.target), (ActorId(1), ActorId(2)));
    assert!(!trace.steps().is_empty());
}

#[test]
fn one_use_tokens_reserve_when_preparation_starts_and_cannot_be_refreshed() {
    let (mut game, plan) = captured_plan("basic_melee");
    let id = ConditionId::new("readied").unwrap();
    game.apply_condition(ActorId(1), &id, ActorId(2), 500)
        .unwrap();
    let plan = game
        .ability_plan(
            plan.actor(),
            &AbilityId::new("basic_melee").unwrap(),
            plan.target(),
        )
        .unwrap();
    assert_eq!(plan.as_ability().unwrap().check.as_ref().unwrap().edge, 1);
    assert_eq!(plan.tokens().len(), 1);
    // Merely validating admission does not claim a token.
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .instances()
        .next()
        .unwrap()
        .reserved_by
        .is_none());
    game.start_work(plan.clone(), None).unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .next()
            .unwrap()
            .reserved_by,
        Some(ActorId(1))
    );
    assert_eq!(
        game.apply_condition(ActorId(1), &id, ActorId(2), 900)
            .unwrap(),
        crate::conditions::ConditionUpdate::Reserved
    );
    let after = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    assert_eq!(after.as_ability().unwrap().check.as_ref().unwrap().edge, 0);
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, plan);
}

#[test]
fn braced_changes_only_checks_against_physical_defense() {
    let (mut game, _) = captured_plan("fear");
    let id = ConditionId::new("braced").unwrap();
    game.apply_condition(ActorId(2), &id, ActorId(2), 500)
        .unwrap();
    for (ability, expected) in [("basic_melee", -1), ("fear", 0)] {
        let plan = game
            .ability_plan(ActorId(1), &AbilityId::new(ability).unwrap(), ActorId(2))
            .unwrap();
        assert_eq!(
            plan.as_ability().unwrap().check.as_ref().unwrap().edge,
            expected
        );
        assert_eq!(plan.as_ability().unwrap().edge, 0);
    }
}

#[test]
fn restored_plans_reject_tokens_on_the_wrong_side_of_a_check() {
    let (mut game, _) = captured_plan("basic_melee");
    for (condition, holder) in [("readied", ActorId(1)), ("exposed", ActorId(2))] {
        game.apply_condition(holder, &ConditionId::new(condition).unwrap(), holder, 500)
            .unwrap();
        let original = game
            .ability_plan(
                ActorId(1),
                &AbilityId::new("basic_melee").unwrap(),
                ActorId(2),
            )
            .unwrap();
        assert!(original.valid(game.content().rules()));
        let mut forged = original.clone();
        let Plan::Ability(ability) = &mut forged else {
            panic!("ability")
        };
        ability
            .tokens
            .iter_mut()
            .find(|token| token.condition.as_str() == condition)
            .unwrap()
            .holder = if holder == ActorId(1) {
            ActorId(2)
        } else {
            ActorId(1)
        };
        assert!(!forged.valid(game.content().rules()));
    }
}

#[test]
fn token_vectors_are_bounded_during_checkpoint_decoding() {
    let (_, original) = captured_plan("basic_melee");
    let mut value = serde_json::to_value(&original).unwrap();
    value["tokens"] = serde_json::json!((0..=2 * crate::limits::CONDITIONS)
        .map(|order| crate::conditions::TokenReservation {
            holder: ActorId(1),
            condition: ConditionId::new("readied").unwrap(),
            source: ActorId(1),
            order: order as u64,
        })
        .collect::<Vec<_>>());
    assert!(serde_json::from_value::<Plan>(value).is_err());
}

#[test]
fn a_missed_check_consumes_tokens_and_later_expiry_never_changes_its_frozen_edge() {
    let (mut game, _) = captured_plan("basic_melee");
    game.apply_condition(
        ActorId(1),
        &ConditionId::new("readied").unwrap(),
        ActorId(1),
        1,
    )
    .unwrap();
    let mut plan = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let Plan::Ability(ability) = &mut plan else {
        panic!("ability")
    };
    ability.check.as_mut().unwrap().bonus = -1000;
    let duration = plan.preparation();
    game.start_work(plan.clone(), None).unwrap();
    game.tick = duration;
    game.resolve_preparations().unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert!(game.creature(ActorId(1)).unwrap().conditions().is_empty());
    assert!(game.event_log.iter().any(|event| matches!(event, crate::combat::CombatEvent::Resolved { outcome } if outcome.result == OutcomeStatus::Miss)));
    assert_eq!(plan.as_ability().unwrap().check.as_ref().unwrap().edge, 1);
}

#[test]
fn shared_effect_edge_and_check_only_tokens_are_frozen_separately() {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    for (shared, token, combined) in [
        (3, "readied", 4),
        (5, "readied", 6),
        (3, "distracted", 2),
        (-3, "readied", -2),
    ] {
        let (mut game, _) = captured_plan_with_modifiers(
            "basic_melee",
            vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Edge,
                    value: shared,
                },
            }],
        );
        game.apply_condition(
            ActorId(1),
            &ConditionId::new(token).unwrap(),
            ActorId(1),
            500,
        )
        .unwrap();
        let plan = game
            .ability_plan(
                ActorId(1),
                &AbilityId::new("basic_melee").unwrap(),
                ActorId(2),
            )
            .unwrap();
        let ability = plan.as_ability().unwrap();
        assert_eq!(ability.edge, shared);
        assert_eq!(ability.check.as_ref().unwrap().edge, combined);
    }
}

#[test]
fn paused_reserved_token_expiry_projects_active_time_without_scheduling_a_wakeup() {
    let (mut game, _) = captured_plan("basic_melee");
    game.apply_condition(
        ActorId(1),
        &ConditionId::new("readied").unwrap(),
        ActorId(1),
        40,
    )
    .unwrap();
    let plan = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    game.start_work(plan.clone(), None).unwrap();
    game.pause_preparation(ActorId(1)).unwrap();
    assert!(game.next_creature_event_tick().is_none());
    game.tick = 80;
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .next()
            .unwrap()
            .remaining,
        0
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, plan);
    assert!(game.next_creature_event_tick().is_none());
}

#[test]
fn an_effects_new_token_cannot_rewrite_the_check_that_created_it() {
    let (mut game, mut plan) = captured_plan("basic_melee");
    let Plan::Ability(ability) = &mut plan else {
        panic!("ability")
    };
    ability.check.as_mut().unwrap().bonus = 1000;
    ability.on_success = vec![PlannedEffect::ApplyCondition {
        id: ConditionId::new("exposed").unwrap(),
        duration: 500,
    }];
    let snapshot = Snapshot::capture(&game.creature(ActorId(2)).unwrap());
    let mut trace = crate::resolution_diagnostics::ResolutionTrace::default();
    let mut rng = 42;
    let outcome = resolve(plan.resolution().unwrap(), &snapshot, &mut rng, &mut trace);
    assert!(trace.steps().iter().any(|step| step.id == "check"
        && step.fields.iter().any(|field| field.id == "input_edge"
            && field.value == crate::resolution_diagnostics::TraceValueRecord::Signed(0))));
    game.apply(outcome).unwrap();
    let next = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    assert_eq!(next.as_ability().unwrap().check.as_ref().unwrap().edge, 1);
    assert_eq!(next.tokens().len(), 1);
    assert_eq!(plan.as_ability().unwrap().check.as_ref().unwrap().edge, 0);
}
