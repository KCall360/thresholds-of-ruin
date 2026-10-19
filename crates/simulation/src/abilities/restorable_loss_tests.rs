//! Ordinary loss effects use the frozen resolver and shared publication boundary.
use super::{
    resolve, EffectApplicationError, OutcomeStatus, PlannedAmount, PlannedEffect, ResolutionPlan,
    ResolvedEffect, Snapshot,
};
use crate::attributes::Attribute;
use crate::creatures::CreatureState;
use crate::dice::{DicePool, Edge};
use crate::rules::{AmountExpr, Effect, Modifier, ModifierOp, Scope, Stat, StrikeSource};
use crate::{ActorId, GameError};
use serde_json::json;

fn creature() -> CreatureState {
    let mut species = crate::test_creatures::species();
    species.default_attributes =
        crate::attributes::Attributes::new(crate::attributes::ByAttribute {
            strength: 5,
            ..Default::default()
        })
        .unwrap();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::Health,
            value: 100,
        },
    });
    CreatureState::new(crate::test_creatures::build(species)).unwrap()
}

fn input(effects: &[PlannedEffect]) -> ResolutionPlan<'_> {
    ResolutionPlan::Effects {
        actor: ActorId(1),
        target: ActorId(2),
        effects,
    }
}

#[test]
fn loss_and_restoration_publish_actual_capped_amounts_without_mutating_the_snapshot() {
    let creature = creature();
    let strength = u32::from(creature.derived().attributes.get(Attribute::Strength));
    let effects = [
        PlannedEffect::LoseAttribute {
            attribute: Attribute::Strength,
            amount: PlannedAmount::Fixed { amount: 1000 },
        },
        PlannedEffect::RestoreAttribute {
            attribute: Attribute::Strength,
            amount: PlannedAmount::Fixed { amount: 1000 },
        },
        PlannedEffect::LoseMaximumHealth {
            amount: PlannedAmount::Fixed { amount: 7 },
        },
        PlannedEffect::RestoreMaximumHealth {
            amount: PlannedAmount::Fixed { amount: 1000 },
        },
    ];
    let snapshot = Snapshot::capture(&creature);
    let mut rng = 42;
    let outcome = resolve(input(&effects), &snapshot, &mut rng, &mut ()).unwrap();
    assert_eq!(outcome.result, OutcomeStatus::Applied);
    assert_eq!(
        outcome.effects,
        [
            ResolvedEffect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: strength,
            },
            ResolvedEffect::RestoreAttribute {
                attribute: Attribute::Strength,
                amount: strength,
            },
            ResolvedEffect::LoseMaximumHealth { amount: 7 },
            ResolvedEffect::RestoreMaximumHealth { amount: 7 },
        ]
    );
    assert_eq!(snapshot.creature, creature);
    assert_eq!(rng, 42);
}

#[test]
fn rolled_loss_uses_the_frozen_dice_once_and_roundtrips_the_actual_result() {
    let dice = DicePool::new(1, 3, 0).unwrap();
    let effects = [PlannedEffect::LoseMaximumHealth {
        amount: PlannedAmount::Rolled { dice },
    }];
    let effects: Vec<PlannedEffect> =
        serde_json::from_value(serde_json::to_value(effects).unwrap()).unwrap();
    let mut expected_rng = 42;
    let expected = dice.roll(&mut expected_rng, Edge::default()).total;
    let mut actual_rng = 42;
    let outcome = resolve(
        input(&effects),
        &Snapshot::capture(&creature()),
        &mut actual_rng,
        &mut (),
    )
    .unwrap();
    assert_eq!(actual_rng, expected_rng);
    assert_eq!(
        outcome.effects,
        [ResolvedEffect::LoseMaximumHealth { amount: expected }]
    );
    assert_eq!(
        serde_json::from_value::<super::Outcome>(serde_json::to_value(&outcome).unwrap()).unwrap(),
        outcome
    );
}

#[test]
fn hard_loss_failure_retains_the_callers_rng_and_has_no_partial_outcome() {
    let effects = [
        PlannedEffect::LoseMaximumHealth {
            amount: PlannedAmount::Rolled {
                dice: DicePool::new(1, 3, 0).unwrap(),
            },
        },
        PlannedEffect::LoseAttribute {
            attribute: Attribute::Strength,
            amount: PlannedAmount::Fixed {
                amount: crate::limits::NUMERIC as u32 + 1,
            },
        },
    ];
    let snapshot = Snapshot::capture(&creature());
    let before = snapshot.clone();
    let mut rng = 42;
    assert!(matches!(
        resolve(input(&effects), &snapshot, &mut rng, &mut ()),
        Err(EffectApplicationError::Creature(_))
    ));
    assert_eq!(rng, 42);
    assert_eq!(snapshot, before);
}

#[test]
fn ordinary_condition_rejection_stays_soft_but_checked_resolution_rejects_atomically() {
    let effects = [
        PlannedEffect::LoseMaximumHealth {
            amount: PlannedAmount::Rolled {
                dice: DicePool::new(1, 3, 0).unwrap(),
            },
        },
        PlannedEffect::ApplyCondition {
            id: crate::content_ids::ConditionId::new("missing_condition").unwrap(),
            duration: 1,
        },
    ];
    let snapshot = Snapshot::capture(&creature());
    let ordinary = resolve(input(&effects), &snapshot, &mut 42, &mut ()).unwrap();
    assert_eq!(ordinary.effects.len(), 1);
    let mut rng = 42;
    assert!(matches!(
        super::resolve_checked(input(&effects), &snapshot, &mut rng, &mut ()),
        Err(EffectApplicationError::Condition(_))
    ));
    assert_eq!(rng, 42);
}

#[test]
fn loss_effect_shapes_are_strict_and_frozen_operands_must_match_exactly() {
    for kind in [
        "lose_attribute",
        "restore_attribute",
        "lose_maximum_health",
        "restore_maximum_health",
    ] {
        let mut value = json!({"kind":kind,"amount":{"kind":"fixed","amount":2}});
        if kind.ends_with("attribute") {
            value["attribute"] = json!("strength");
        }
        let effect: Effect = serde_json::from_value(value.clone()).unwrap();
        assert!(effect.valid_shape());
        let planned: PlannedEffect = serde_json::from_value(value.clone()).unwrap();
        assert!(super::plan::effects_match_definition(
            std::slice::from_ref(&effect),
            &[planned]
        ));
        value["amount"]["amount"] = json!(3);
        let changed: PlannedEffect = serde_json::from_value(value.clone()).unwrap();
        assert!(!super::plan::effects_match_definition(
            &[effect],
            &[changed]
        ));
        value["extra"] = json!(0);
        assert!(serde_json::from_value::<Effect>(value.clone()).is_err());
        assert!(serde_json::from_value::<PlannedEffect>(value).is_err());
        assert!(serde_json::from_value::<Effect>(json!({"kind":kind,"amount":null})).is_err());
        let mut resolved = json!({"kind":kind,"amount":1_000_001});
        if kind.ends_with("attribute") {
            resolved["attribute"] = json!("strength");
        }
        assert!(serde_json::from_value::<ResolvedEffect>(resolved).is_err());
    }
    let expected = Effect::LoseAttribute {
        attribute: Attribute::Strength,
        amount: AmountExpr::Fixed { amount: 2 },
    };
    let forged = PlannedEffect::LoseAttribute {
        attribute: Attribute::Speed,
        amount: PlannedAmount::Fixed { amount: 2 },
    };
    assert!(!super::plan::effects_match_definition(
        &[expected],
        &[forged]
    ));
}

#[test]
fn resolved_attribute_loss_rejects_an_impossible_actual_amount() {
    for kind in ["lose_attribute", "restore_attribute"] {
        assert!(serde_json::from_value::<ResolvedEffect>(json!({
            "kind":kind,"attribute":"strength","amount":1000
        }))
        .is_ok());
        assert!(serde_json::from_value::<ResolvedEffect>(json!({
            "kind":kind,"attribute":"strength","amount":1001
        }))
        .is_err());
        // Requests may be larger: the live application reports the capped result.
        let request: Effect = serde_json::from_value(json!({
            "kind":kind,"attribute":"strength","amount":{"kind":"fixed","amount":1001}
        }))
        .unwrap();
        assert!(request.valid_shape());
    }
}

fn fatal_sequence() -> (crate::Game, crate::IntentionId) {
    super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].on_success = vec![
                Effect::LoseMaximumHealth {
                    amount: AmountExpr::Fixed { amount: 100 },
                },
                Effect::RestoreMaximumHealth {
                    amount: AmountExpr::Fixed { amount: 100 },
                },
            ];
        },
        |_, _| {},
    )
}

#[test]
fn fatal_loss_uses_ordinary_death_credit_and_skips_later_restoration() {
    let (mut game, _) = fatal_sequence();
    let item_count = game.items.len();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert!(game.creature(ActorId(2)).unwrap().health().dead());
    assert_eq!(game.health(ActorId(2)), Some((0, 0)));
    assert_eq!(game.items.len(), item_count + 1);
    let outcomes: Vec<_> = game
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(
        outcomes.last().unwrap().effects,
        [ResolvedEffect::LoseMaximumHealth { amount: 100 }]
    );
    assert!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .earned_xp()
            > 0
    );
}

#[test]
fn late_corpse_failure_rolls_back_loss_rng_publication_and_paid_preparation() {
    let (mut game, _) = fatal_sequence();
    game.next_item_id = u64::MAX;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(2), crate::Action::Wait),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
}

#[test]
fn restoration_at_zero_deficit_is_a_successful_unchanged_effect() {
    let effects = [
        PlannedEffect::RestoreAttribute {
            attribute: Attribute::Strength,
            amount: PlannedAmount::Fixed { amount: 8 },
        },
        PlannedEffect::RestoreMaximumHealth {
            amount: PlannedAmount::Fixed { amount: 8 },
        },
    ];
    let outcome = resolve(
        input(&effects),
        &Snapshot::capture(&creature()),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.result, OutcomeStatus::Unaffected);
    assert_eq!(
        outcome.effects,
        [
            ResolvedEffect::RestoreAttribute {
                attribute: Attribute::Strength,
                amount: 0
            },
            ResolvedEffect::RestoreMaximumHealth { amount: 0 },
        ]
    );
}

#[test]
fn checked_loss_immunity_skips_its_draw_without_suppressing_the_primary() {
    use crate::rules::{
        DefenseKind, NamedDescriptor, RiderCheckDef, RiderDef, RiderTrigger, Selector,
        SkillSelector,
    };
    let poison = crate::content_ids::DescriptorId::new("fixture_poison").unwrap();
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].riders.push(RiderDef {
                condition: None,
                trigger: RiderTrigger::Success,
                check: Some(RiderCheckDef {
                    skill: SkillSelector::NaturalAttack,
                    defense: DefenseKind::Physical,
                }),
                descriptors: vec![poison.clone()],
                effects: vec![Effect::LoseMaximumHealth {
                    amount: AmountExpr::Rolled {
                        dice: crate::rules::DiceExpr {
                            count: 1,
                            sides: 3,
                            bonus: 0,
                        },
                    },
                }],
            });
        },
        |rules, species| {
            rules.descriptors.push(NamedDescriptor {
                id: poison.clone(),
                name: "Poison probe".into(),
            });
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Immunity {
                    selector: Selector::Descriptor {
                        descriptor: poison.clone(),
                    },
                },
            });
        },
    );
    let before_rng = game.rules_rng;
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert_eq!(game.health(ActorId(2)), Some((97, 100)));
    assert_eq!(
        game.rules_rng, before_rng,
        "fixed bite and immune rider consume no RNG"
    );
}

#[test]
fn fatal_strength_loss_preserves_injury_and_awards_the_effect_owner_once() {
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].on_success = vec![Effect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 1 },
            }];
        },
        |_, species| {
            species.modifiers.retain(|modifier| {
                !matches!(
                    modifier.op,
                    ModifierOp::Set {
                        stat: Stat::Health,
                        ..
                    }
                )
            });
            species.default_attributes =
                crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                    strength: 2,
                    ..Default::default()
                })
                .unwrap();
        },
    );
    let injury = game.health(ActorId(2)).unwrap().1 - 1;
    game.apply_effects(
        ActorId(2),
        &[Effect::Damage {
            damage: crate::rules::DamageExpr::Components {
                components: vec![crate::rules::DamageComponentExpr {
                    category: crate::combat::DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Fixed {
                        amount: u32::try_from(injury).unwrap(),
                    },
                }],
            },
        }],
    )
    .unwrap();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let health = game.creature(ActorId(2)).unwrap().health();
    assert!(health.dead());
    assert_eq!(health.injury(), injury);
    let earned = game
        .creature(ActorId(1))
        .unwrap()
        .earned_progress()
        .earned_xp();
    assert!(earned > 0);
    assert!(!game
        .apply_effects(
            ActorId(2),
            &[Effect::RestoreAttribute {
                attribute: Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 1 },
            }]
        )
        .unwrap());
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .earned_xp(),
        earned
    );
    assert!(game.creature(ActorId(2)).unwrap().health().dead());
}

#[test]
fn source_loss_and_save_resume_preserve_captured_checks_amounts_holds_and_cooldown() {
    use crate::rules::{Against, CheckActor, CheckDef, DefenseKind, SkillSelector};
    let (mut game, intention) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.cost = Some(crate::rules::Cost {
                resource: crate::resources::Resource::Stamina,
                start: 1,
                resolution: 1,
            });
            ability.cooldown = std::num::NonZeroU64::new(100);
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].check = Some(CheckDef {
                actor: CheckActor::User,
                skill: SkillSelector::NaturalAttack,
                against: Against::Defense {
                    defense: DefenseKind::Physical,
                },
            });
            strikes[0].on_success = vec![Effect::LoseMaximumHealth {
                amount: AmountExpr::Rolled {
                    dice: crate::rules::DiceExpr {
                        count: 1,
                        sides: 3,
                        bonus: 0,
                    },
                },
            }];
        },
        |_, species| {
            species.default_attributes =
                crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                    strength: 5,
                    ..Default::default()
                })
                .unwrap();
        },
    );
    let original = game.preparation(ActorId(1)).unwrap().clone();
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    game.apply_effects(
        ActorId(1),
        &[Effect::LoseAttribute {
            attribute: Attribute::Strength,
            amount: AmountExpr::Fixed { amount: 1 },
        }],
    )
    .unwrap();
    assert_eq!(game.creature(ActorId(1)).unwrap().build(), &build);
    let changed = game.preparation(ActorId(1)).unwrap();
    assert_eq!(changed.plan, original.plan);
    assert_eq!(changed.hold, original.hold);
    let ability = original.plan.as_ability().unwrap();
    assert_eq!(
        game.ability_plan(ActorId(1), &ability.ability, ActorId(2)),
        Err(GameError::Rule(super::RuleError::CoolingDown))
    );
    // Compare a fresh capture in an isolated cooldown-free copy. The live paid
    // preparation and its authoritative cooldown must remain untouched.
    let mut recapture_game = game.clone();
    let mut recapture_creature = recapture_game.creature(ActorId(1)).unwrap().into_owned();
    recapture_creature.restore_cooldowns(vec![]).unwrap();
    {
        let mut actor = recapture_game.actors.get_mut(&ActorId(1)).unwrap();
        let combat = actor.combat.as_mut().unwrap();
        *combat = crate::actor_creatures::CombatState::from_creature(
            combat.identity.clone(),
            recapture_creature,
        );
    }
    let recaptured = recapture_game
        .ability_plan(ActorId(1), &ability.ability, ActorId(2))
        .unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .cooldowns()
            .remaining(&ability.ability),
        100
    );
    assert_ne!(
        recaptured.as_ability().unwrap().strikes[0].check,
        ability.strikes[0].check
    );
    assert_eq!(
        recaptured.as_ability().unwrap().strikes[0].on_success,
        ability.strikes[0].on_success
    );
    assert!(game.plan_source_valid(&original.plan));
    game.pause_preparation(ActorId(1)).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let (saved, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(saved, shared)).unwrap()).unwrap();
    let mut restored =
        crate::Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored.preparation(ActorId(1)).unwrap().plan,
        original.plan
    );
    restored
        .start_work(original.plan.clone(), Some(intention))
        .unwrap();
    let mut expected_rng = restored.rules_rng;
    let expected = resolve(
        original.plan.resolution().unwrap(),
        &Snapshot::capture(&restored.creature(ActorId(2)).unwrap()),
        &mut expected_rng,
        &mut (),
    )
    .unwrap();
    restored.tick = 10;
    restored.resolve_preparations().unwrap();
    assert_eq!(restored.rules_rng, expected_rng);
    let actual = restored
        .combat_events()
        .iter()
        .rev()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => Some(outcome),
            _ => None,
        })
        .unwrap();
    assert_eq!(actual.effects, expected.effects);
}

fn item_game(effects: Vec<Effect>) -> (crate::Game, crate::ItemId) {
    item_game_from(creature(), effects)
}

fn item_game_from(creature: CreatureState, effects: Vec<Effect>) -> (crate::Game, crate::ItemId) {
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};
    let mut game = crate::Game::two_room_in_stone(42, creature.build().content().clone());
    for x in [1, 2] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: format!("drinker {x}"),
                faction: "neutral".into(),
            },
            creature.build().clone(),
        )
        .unwrap();
    }
    let mut potion = crate::ItemSpec::ordinary("restorable loss probe".into());
    potion.class = crate::ItemClass::Potion;
    potion.consumable = Some(crate::ConsumableSpec {
        discovery: crate::ItemDiscovery::SelfEffect,
        effects,
    });
    let at = game.actors[&ActorId(1)].location;
    let item = game
        .place_item_stack(game.next_item_id, at, Some(ActorId(1)), 1, potion)
        .unwrap();
    (game, item)
}

#[test]
fn ordinary_consumable_restores_recorded_deficits_and_spends_exactly_one_item() {
    let (mut game, item) = item_game(vec![
        Effect::RestoreAttribute {
            attribute: Attribute::Strength,
            amount: AmountExpr::Fixed { amount: 1000 },
        },
        Effect::RestoreMaximumHealth {
            amount: AmountExpr::Fixed { amount: 1000 },
        },
    ]);
    game.apply_effects(
        ActorId(1),
        &[
            Effect::LoseAttribute {
                attribute: Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 2 },
            },
            Effect::LoseMaximumHealth {
                amount: AmountExpr::Fixed { amount: 7 },
            },
        ],
    )
    .unwrap();
    game.admit_intention(
        ActorId(1),
        crate::Action::Drink { item },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.act(ActorId(2), crate::Action::Wait).unwrap();
    assert_eq!(game.health(ActorId(1)), Some((100, 100)));
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .derived()
            .attributes
            .get(Attribute::Strength),
        5
    );
    assert!(!game.items.contains_key(&item));
    let restored = game
        .combat_events()
        .iter()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. }
                if matches!(
                    outcome.effects.first(),
                    Some(ResolvedEffect::RestoreAttribute { .. })
                ) =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        restored.effects,
        [
            ResolvedEffect::RestoreAttribute {
                attribute: Attribute::Strength,
                amount: 2
            },
            ResolvedEffect::RestoreMaximumHealth { amount: 7 },
        ]
    );
}

#[test]
fn late_item_failure_restores_consumed_item_rng_deficit_and_events_together() {
    let (mut game, item) = item_game(vec![Effect::LoseMaximumHealth {
        amount: AmountExpr::Rolled {
            dice: crate::rules::DiceExpr {
                count: 1,
                sides: 3,
                bonus: 100,
            },
        },
    }]);
    game.admit_intention(
        ActorId(1),
        crate::Action::Drink { item },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.next_item_id = u64::MAX;
    game.begin_survival_diagnostics(true);
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(2), crate::Action::Wait),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
    assert!(game.items.contains_key(&item));
}

fn loss_sensitive_timing(
    rules: &mut crate::rules::RulesetDocument,
    species: &mut crate::creatures::Species,
) {
    species.default_attributes =
        crate::attributes::Attributes::new(crate::attributes::ByAttribute {
            strength: 1,
            ..Default::default()
        })
        .unwrap();
    rules.constants.duration_scale_numerator = 2;
    rules.constants.duration_scale_offset = 1;
    // Valid at the duration bound with Strength 1; losing Strength makes this
    // independent granted ability's timing overflow during the dependent fold.
    let mut ability = rules.abilities[0].clone();
    ability.id = crate::content_ids::AbilityId::new("fixture_loss_timing").unwrap();
    ability.basic_attack = false;
    ability.timing = crate::rules::Timing {
        source: crate::rules::TimingSource::Fixed {
            prepare: 1_000_000,
            recover: 1,
        },
        scaled_by: Some(Attribute::Strength),
    };
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility {
            ability: ability.id.clone(),
        },
    });
    rules.abilities.push(ability);
}

fn hard_failure_effects() -> Vec<Effect> {
    vec![
        Effect::LoseMaximumHealth {
            amount: AmountExpr::Rolled {
                dice: crate::rules::DiceExpr {
                    count: 1,
                    sides: 3,
                    bonus: 0,
                },
            },
        },
        Effect::LoseAttribute {
            attribute: Attribute::Strength,
            amount: AmountExpr::Fixed { amount: 1 },
        },
    ]
}

#[test]
fn hard_strike_rebuild_failure_rolls_back_earlier_effect_draw_and_publication() {
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].on_success = hard_failure_effects();
        },
        loss_sensitive_timing,
    );
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(2), crate::Action::Wait),
        Err(GameError::Creature(
            crate::creatures::CreatureStateError::Build(crate::creatures::BuildError::DerivedLimit)
        ))
    );
    assert_eq!(game, before);
}

#[test]
fn hard_consumable_rebuild_failure_keeps_inventory_hold_rng_and_publication() {
    let original = crate::test_creatures::build(crate::test_creatures::species());
    let mut rules = original.ruleset().document().clone();
    let mut catalog = original.content().catalog().clone();
    loss_sensitive_timing(
        &mut rules,
        catalog.species.get_mut(original.species_id()).unwrap(),
    );
    let content =
        crate::creatures::Content::new(crate::rules::Ruleset::validate(rules).unwrap(), catalog)
            .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content,
        original.species_id().clone(),
        original.ledger().clone(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    let creature = CreatureState::new(build).unwrap();
    let (mut game, item) = item_game_from(creature, hard_failure_effects());
    game.admit_intention(
        ActorId(1),
        crate::Action::Drink { item },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(2), crate::Action::Wait),
        Err(GameError::Creature(
            crate::creatures::CreatureStateError::Build(crate::creatures::BuildError::DerivedLimit)
        ))
    );
    assert_eq!(game, before);
    assert!(game.items.contains_key(&item));
}

#[test]
fn loss_capacity_cancels_a_live_hold_without_refunding_paid_start_or_cooldown() {
    use crate::resources::Resource;
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.cost = Some(crate::rules::Cost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 6,
            });
            ability.cooldown = std::num::NonZeroU64::new(100);
        },
        |_, species| {
            species.default_attributes =
                crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                    strength: 5,
                    ..Default::default()
                })
                .unwrap();
        },
    );
    let owner = ActorId(1);
    let pending = game.preparation(owner).unwrap().clone();
    let hold = pending.hold.unwrap();
    let ability = pending.plan.as_ability().unwrap().ability.clone();
    assert_eq!(
        game.creature(owner)
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Stamina),
        6
    );
    assert_eq!(
        game.creature(owner)
            .unwrap()
            .costs()
            .reservation(hold)
            .unwrap()
            .resolution,
        6
    );
    game.apply_effects(
        owner,
        &[Effect::LoseAttribute {
            attribute: Attribute::Strength,
            amount: AmountExpr::Fixed { amount: 2 },
        }],
    )
    .unwrap();
    assert!(game.preparation(owner).is_none());
    let changed = game.creature(owner).unwrap();
    assert_eq!(changed.costs().resources().maximum(Resource::Stamina), 5);
    assert_eq!(changed.costs().resources().balance(Resource::Stamina), 5);
    assert!(changed.costs().reservation(hold).is_none());
    assert_eq!(changed.cooldowns().remaining(&ability), 100);
    drop(changed);
    game.apply_effects(
        owner,
        &[Effect::RestoreAttribute {
            attribute: Attribute::Strength,
            amount: AmountExpr::Fixed { amount: 2 },
        }],
    )
    .unwrap();
    let restored = game.creature(owner).unwrap();
    assert_eq!(restored.costs().resources().maximum(Resource::Stamina), 7);
    assert_eq!(restored.costs().resources().balance(Resource::Stamina), 5);
    assert!(game.preparation(owner).is_none());
    assert_eq!(restored.cooldowns().remaining(&ability), 100);
}

fn hit_die_effect(kind: &str) -> Effect {
    serde_json::from_value(json!({"kind": kind})).unwrap()
}

fn fatal_hit_die_sequence() -> (crate::Game, crate::IntentionId) {
    super::strike_tests::sequence_game_custom(
        &[0, 10],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].on_success = vec![
                hit_die_effect("drain_latest_hit_die"),
                hit_die_effect("restore_hit_die"),
            ];
        },
        |_, _| {},
    )
}

#[test]
fn fatal_hit_die_drain_drops_inventory_attributes_owner_once_and_replays_sticky_death() {
    let (mut game, _) = fatal_hit_die_sequence();
    let at = game.actors[&ActorId(2)].location;
    let carried = game
        .place_item_stack(
            game.next_item_id,
            at,
            Some(ActorId(2)),
            1,
            crate::ItemSpec::ordinary("drained owner's item".into()),
        )
        .unwrap();
    let items = game.items.len();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let victim = game.creature(ActorId(2)).unwrap();
    assert!(victim.health().dead());
    assert_eq!(victim.build().ledger().total_hd().get(), 0);
    assert_eq!(game.items.len(), items + 1, "one corpse is created");
    assert_eq!(
        game.items[&carried].location,
        crate::ItemLocation::Ground(at)
    );
    let earned = game
        .creature(ActorId(1))
        .unwrap()
        .earned_progress()
        .earned_xp();
    assert!(earned > 0);
    let outcome = game
        .combat_events()
        .iter()
        .find_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => Some(outcome),
            _ => None,
        })
        .unwrap();
    assert_eq!(outcome.actor, ActorId(1));
    assert_eq!(
        outcome.effects,
        [ResolvedEffect::DrainLatestHitDie { amount: 1 }]
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let (saved, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(saved, shared)).unwrap()).unwrap();
    let mut reopened =
        crate::Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap();
    for state in [&mut game, &mut reopened] {
        assert!(!state
            .apply_effects(ActorId(2), &[hit_die_effect("restore_hit_die")])
            .unwrap());
        state.tick = 20;
        state.resolve_preparations().unwrap();
        assert_eq!(
            state
                .creature(ActorId(1))
                .unwrap()
                .earned_progress()
                .earned_xp(),
            earned
        );
        assert!(state.creature(ActorId(2)).unwrap().health().dead());
        assert_eq!(state.items.len(), items + 1);
    }
    assert_eq!(game, reopened);
}

#[test]
fn late_hit_die_fatal_failure_rolls_back_the_complete_paid_candidate() {
    let (mut game, _) = fatal_hit_die_sequence();
    game.next_item_id = u64::MAX;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(2), crate::Action::Wait),
        Err(GameError::IdentityExhausted)
    );
    assert_eq!(game, before);
}

fn two_hit_die_creature(original_root: bool, selected: bool) -> CreatureState {
    let original = creature();
    let seeded =
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial; 2], 42)
            .unwrap();
    let ledger = if original_root {
        seeded
    } else {
        crate::progression::HdLedger::new(seeded.entries().to_vec()).unwrap()
    };
    let mut build = crate::creatures::CreatureBuild::new(
        original.build().content().clone(),
        original.build().species_id().clone(),
        ledger,
        original.build().binding(),
        *original.build().starting_skills(),
    )
    .unwrap();
    if selected {
        build
            .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
                source: crate::earned_growth::GrowthSource::Racial {},
            })
            .unwrap();
    }
    CreatureState::new(build).unwrap()
}

#[test]
fn ineligible_hit_die_restoration_rejects_item_admission_without_time_holds_or_inventory_change() {
    for (root, selected, blocker) in [
        (
            true,
            false,
            crate::creatures::HitDieRestorationBlocker::MissingSource,
        ),
        (
            false,
            true,
            crate::creatures::HitDieRestorationBlocker::MissingHealthRoot,
        ),
    ] {
        let (mut game, item) = item_game_from(
            two_hit_die_creature(root, selected),
            vec![hit_die_effect("restore_hit_die")],
        );
        game.apply_effects(ActorId(1), &[hit_die_effect("drain_latest_hit_die")])
            .unwrap();
        let before = game.clone();
        assert_eq!(
            game.admit_intention(
                ActorId(1),
                crate::Action::Drink { item },
                crate::IntentionOrigin::Human
            ),
            Err(GameError::Creature(
                crate::creatures::CreatureStateError::HitDieRestoration(blocker)
            ))
        );
        assert_eq!(game, before);
    }
}

#[test]
fn later_ineligible_hit_die_restoration_keeps_the_paid_item_and_complete_recovery_debt() {
    let (mut game, item) = item_game_from(
        two_hit_die_creature(true, true),
        vec![hit_die_effect("restore_hit_die")],
    );
    game.apply_effects(ActorId(1), &[hit_die_effect("drain_latest_hit_die")])
        .unwrap();
    game.admit_intention(
        ActorId(1),
        crate::Action::Drink { item },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let frozen = game.preparation(ActorId(1)).unwrap().clone();
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .set_future_growth(crate::earned_growth::FutureGrowth::Disabled {})
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, frozen.plan);
    game.act(ActorId(2), crate::Action::Wait).unwrap();
    assert!(
        !game.items.contains_key(&item),
        "committed consumption cannot be refunded"
    );
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        1
    );
    assert!(game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved { outcome, .. }
        if outcome.effects == [ResolvedEffect::RestoreHitDie { amount: 0 }])));
}

#[test]
fn hit_die_effect_forms_are_strict_single_ordinal_and_never_reuse_a_different_frozen_effect() {
    for kind in ["drain_latest_hit_die", "restore_hit_die"] {
        let value = json!({"kind":kind});
        let source: Effect = serde_json::from_value(value.clone()).unwrap();
        let planned: PlannedEffect = serde_json::from_value(value.clone()).unwrap();
        assert!(source.valid_shape());
        assert!(super::plan::effects_match_definition(
            std::slice::from_ref(&source),
            &[planned]
        ));
        for extra in ["amount", "source", "seed", "ordinal"] {
            let mut invalid = value.clone();
            invalid[extra] = json!(1);
            assert!(serde_json::from_value::<Effect>(invalid.clone()).is_err());
            assert!(serde_json::from_value::<PlannedEffect>(invalid).is_err());
        }
        for amount in [0, 1] {
            let resolved: ResolvedEffect =
                serde_json::from_value(json!({"kind":kind,"amount":amount})).unwrap();
            assert_eq!(
                serde_json::to_value(resolved).unwrap(),
                json!({"kind":kind,"amount":amount})
            );
        }
        assert!(serde_json::from_value::<ResolvedEffect>(json!({"kind":kind,"amount":2})).is_err());
        assert!(!super::plan::effects_match_definition(
            &[source],
            &[PlannedEffect::StepSelfOneCell]
        ));
    }
}

#[test]
fn hard_hit_die_rebuild_failure_rolls_back_prior_roll_consumed_stock_and_paid_holds() {
    let original = crate::test_creatures::build(crate::test_creatures::species());
    // The fourth ordinal owns the Strength that keeps a granted timing in range.
    // Build both permanent facts together, avoiding an invalid intermediate.
    let mut advancement = original.advancement().clone();
    advancement.choices = vec![crate::creatures::HdChoices::default(); 4];
    advancement.choices[3].attribute = Some(Attribute::Strength);
    let mut rules = original.ruleset().document().clone();
    let mut catalog = original.content().catalog().clone();
    let species = catalog.species.get_mut(original.species_id()).unwrap();
    loss_sensitive_timing(&mut rules, species);
    species.default_attributes = crate::attributes::Attributes::new(Default::default()).unwrap();
    let content =
        crate::creatures::Content::new(crate::rules::Ruleset::validate(rules).unwrap(), catalog)
            .unwrap();
    let build = crate::creatures::CreatureBuild::from_recorded(
        content,
        original.species_id().clone(),
        crate::attributes::Attributes::new(Default::default()).unwrap(),
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial; 4], 42)
            .unwrap(),
        original.binding(),
        advancement,
        vec![],
    )
    .unwrap();
    let effects = vec![
        Effect::LoseMaximumHealth {
            amount: AmountExpr::Rolled {
                dice: crate::rules::DiceExpr {
                    count: 1,
                    sides: 3,
                    bonus: 0,
                },
            },
        },
        hit_die_effect("drain_latest_hit_die"),
    ];
    let (mut game, item) = item_game_from(CreatureState::new(build).unwrap(), effects);
    game.admit_intention(
        ActorId(1),
        crate::Action::Drink { item },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.begin_survival_diagnostics(true);
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(2), crate::Action::Wait),
        Err(GameError::Creature(
            crate::creatures::CreatureStateError::Build(crate::creatures::BuildError::DerivedLimit)
        ))
    );
    assert_eq!(game, before);
    assert!(game.items.contains_key(&item));
    assert!(game.take_survival_diagnostics().unwrap().actors.is_empty());
}

#[test]
fn hit_die_drain_preserves_valid_paid_sequence_operands_prefix_and_checkpoint_resume() {
    let (mut game, intention) = super::strike_tests::sequence_game_custom(
        &[0, 10, 20],
        |ability| {
            ability.cost = Some(crate::rules::Cost {
                resource: crate::resources::Resource::Stamina,
                start: 1,
                resolution: 1,
            });
            ability.cooldown = std::num::NonZeroU64::new(100);
        },
        |_, species| {
            species.default_attributes =
                crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                    strength: 5,
                    ..Default::default()
                })
                .unwrap();
        },
    );
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .add_hit_die(crate::progression::HdSource::Racial, 42)
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    let original = game.preparation(ActorId(1)).unwrap().plan.clone();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let before = game.preparation(ActorId(1)).unwrap().clone();
    let paid = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .balance(crate::resources::Resource::Stamina);
    game.apply_effects(ActorId(1), &[hit_die_effect("drain_latest_hit_die")])
        .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, original);
    assert_eq!(game.preparation(ActorId(1)).unwrap().next_strike(), Some(1));
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().remaining,
        before.remaining
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap().hold, before.hold);
    game.pause_preparation(ActorId(1)).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let (saved, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(saved, shared)).unwrap()).unwrap();
    let mut reopened =
        crate::Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap();
    assert_eq!(reopened.preparation(ActorId(1)).unwrap().plan, original);
    reopened.start_work(original, Some(intention)).unwrap();
    for tick in [20, 30] {
        reopened.tick = tick;
        reopened.resolve_preparations().unwrap();
    }
    assert_eq!(
        reopened.health(ActorId(2)),
        Some((91, 100)),
        "resolved prefix is not repeated"
    );
    assert_eq!(reopened.actors[&ActorId(1)].ready_at, 50);
    assert_eq!(
        reopened
            .creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        paid
    );
}

#[test]
fn hit_die_grant_loss_cancels_only_the_unresolved_tail_and_never_refunds_or_replays_the_prefix() {
    use crate::content_ids::{AbilityId, TalentId};
    use crate::resources::Resource;
    use crate::rules::{ModifierOp, Requirement, TalentDef};
    let ability = AbilityId::new("fixture_hd_owned_sequence").unwrap();
    let talent = TalentId::new("fixture_hd_owned_grant").unwrap();
    let (fixture, _) = super::strike_tests::sequence_game_custom(
        &[0, 10, 20],
        |definition| {
            definition.cost = Some(crate::rules::Cost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 1,
            });
            definition.cooldown = std::num::NonZeroU64::new(100);
        },
        |rules, species| {
            species.default_attributes =
                crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                    strength: 5,
                    ..Default::default()
                })
                .unwrap();
            let mut owned = rules
                .abilities
                .iter()
                .find(|definition| definition.basic_attack)
                .unwrap()
                .clone();
            owned.id = ability.clone();
            owned.basic_attack = false;
            rules.abilities.push(owned);
            rules.talents.push(TalentDef {
                id: talent.clone(),
                name: "HD-owned grant".into(),
                prerequisites: vec![],
                requirement: Requirement::TotalHd {
                    minimum: crate::HdCount(2),
                },
                form_choices: vec![],
                modifiers: vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::GrantAbility {
                        ability: ability.clone(),
                    },
                }],
            });
        },
    );
    // An earlier owned grant becomes dormant; the latest owned grant is removed.
    for slot in [0, 1] {
        let mut build = fixture.creature(ActorId(1)).unwrap().build().clone();
        build
            .add_hit_die(crate::progression::HdSource::Racial, 42)
            .unwrap();
        build
            .select_talent(crate::HdIndex(slot), talent.clone())
            .unwrap();
        build
            .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
                source: crate::earned_growth::GrowthSource::Racial {},
            })
            .unwrap();
        let mut game = crate::Game::two_room_in_stone(42, build.content().clone());
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
                    name: format!("HD subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        let intention = game
            .admit_intention(
                ActorId(1),
                crate::Action::UseAbility {
                    id: ability.clone(),
                    invocation: super::AbilityInvocation::ActorTarget { target: ActorId(2) },
                    supplies: Default::default(),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 1000;
        game.tick = 10;
        game.resolve_preparations().unwrap();
        assert_eq!(game.health(ActorId(2)), Some((97, 100)));
        let paid = game
            .creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Stamina);
        game.apply_effects(ActorId(1), &[hit_die_effect("drain_latest_hit_die")])
            .unwrap();
        assert!(game.preparation(ActorId(1)).is_none());
        assert_eq!(
            game.actors[&ActorId(1)].ready_at,
            50,
            "the complete frozen commitment and recovery remain due"
        );
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Stamina),
            paid
        );
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .cooldowns()
                .remaining(&ability),
            90
        );
        assert!(!game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .abilities
            .contains(&ability));
        if slot == 0 {
            assert!(game
                .creature(ActorId(1))
                .unwrap()
                .derived()
                .dormant_talents
                .contains(&talent));
        }
        game.apply_effects(ActorId(1), &[hit_die_effect("restore_hit_die")])
            .unwrap();
        assert!(game.preparation(ActorId(1)).is_none());
        for tick in [20, 30, 50] {
            game.tick = tick;
            game.resolve_preparations().unwrap();
        }
        assert_eq!(game.health(ActorId(2)), Some((97, 100)));
        assert_eq!(game.combat_events().iter().filter(|event| matches!(event,
            crate::combat::CombatEvent::SequenceEnded { actor: ActorId(1), intention: Some(id), reason: crate::combat::SequenceEnd::Abandoned } if *id == intention)).count(), 1);
    }
}
