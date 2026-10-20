//! Effective direct harm is validated at source changes and frozen admission.
use super::{Plan, PlannedEffect};
use crate::content_ids::{AbilityId, ConditionId};
use crate::creatures::{CreatureBuild, CreatureState, EquipmentSource};
use crate::rules::*;
use crate::{ActorId, EquipmentSlotId, ItemId};

fn damage(amount: AmountExpr) -> Effect {
    Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: crate::combat::DamageType::Impact,
                descriptor: None,
                amount,
            }],
        },
    }
}
fn rolled() -> AmountExpr {
    AmountExpr::Rolled {
        dice: DiceExpr {
            count: 1,
            sides: 2,
            bonus: -2,
        },
    }
}
fn modifiers(value: i64) -> Vec<Modifier> {
    vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Ability {
                ability: AbilityId::new("basic_melee").unwrap(),
                stat: AbilityStat::FlatBonus,
            },
            value,
        },
    }]
}
fn build(
    natural: bool,
    amount: AmountExpr,
    flat: i64,
) -> Result<CreatureBuild, crate::creatures::BuildError> {
    let base = crate::test_creatures::build(crate::test_creatures::species());
    let mut document = base.ruleset().document().clone();
    let mut catalog = base.content().catalog().clone();
    let species = catalog.species.get_mut(base.species_id()).unwrap();
    let mut strikes = species.melee.strikes().to_vec();
    strikes.truncate(1);
    strikes[0].on_success = vec![damage(amount)];
    strikes[0].on_failure.clear();
    strikes[0].riders.clear();
    species.melee = crate::attacks::NaturalAttack::new(
        species.melee.skill(),
        species.melee.bonus(),
        species.melee.wind_up(),
        species.melee.recovery(),
        strikes.clone(),
    )
    .unwrap();
    species.modifiers.extend(modifiers(flat));
    let ability = document
        .abilities
        .iter_mut()
        .find(|row| row.basic_attack)
        .unwrap();
    ability.interaction = Interaction::NonHostile;
    ability.strikes = if natural {
        StrikeSource::NaturalMelee {
            additional_riders: vec![],
        }
    } else {
        StrikeSource::Authored { strikes }
    };
    let condition = document
        .conditions
        .iter_mut()
        .find(|row| row.id.as_str() == "fear")
        .unwrap();
    condition.modifiers = modifiers(3);
    condition.restrictions.clear();
    condition.preparation = ConditionPreparation::Preserve;
    let content =
        crate::creatures::Content::new(Ruleset::validate(document).unwrap(), catalog).unwrap();
    CreatureBuild::new(
        content,
        base.species_id().clone(),
        base.ledger().clone(),
        base.binding(),
        *base.starting_skills(),
    )
}
#[test]
fn effective_interaction_initial_natural_fixed_grant_rejects_harm() {
    assert!(build(true, AmountExpr::Fixed { amount: 0 }, 1).is_err());
}
#[test]
fn effective_interaction_initial_authored_rolled_grant_rejects_harm() {
    assert!(build(false, rolled(), 3).is_err());
}
#[test]
fn effective_interaction_keeps_zero_and_authored_fixed_controls() {
    for (natural, amount, flat) in [
        (true, AmountExpr::Fixed { amount: 0 }, 0),
        (false, AmountExpr::Fixed { amount: 0 }, 3),
        (false, rolled(), 0),
    ] {
        CreatureState::new(build(natural, amount, flat).unwrap()).unwrap();
    }
}
#[test]
fn effective_interaction_later_equipment_and_condition_modifiers_reject_atomically() {
    for (natural, amount) in [(true, AmountExpr::Fixed { amount: 0 }), (false, rolled())] {
        let mut creature = CreatureState::new(build(natural, amount, 0).unwrap()).unwrap();
        let before = creature.clone();
        assert!(creature
            .set_equipment(vec![EquipmentSource {
                weapon_profile: None,
                item: ItemId(1),
                socket: EquipmentSlotId(0),
                attack: None,
                modifiers: modifiers(3),
            }])
            .is_err());
        assert_eq!(creature, before);
        assert!(creature
            .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
            .is_err());
        assert_eq!(creature, before);
    }
}
fn harmless_game() -> crate::Game {
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.interaction = Interaction::NonHostile;
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![],
            };
        },
        |_, species| {
            let mut strikes = species.melee.strikes().to_vec();
            strikes.truncate(1);
            strikes[0].on_success = vec![damage(AmountExpr::Fixed { amount: 0 })];
            strikes[0].on_failure.clear();
            strikes[0].riders.clear();
            species.melee = crate::attacks::NaturalAttack::new(
                species.melee.skill(),
                species.melee.bonus(),
                species.melee.wind_up(),
                species.melee.recovery(),
                strikes,
            )
            .unwrap();
        },
    );
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    game
}
#[test]
fn effective_interaction_frozen_plan_rejects_positive_harm_without_current_equipment() {
    let mut game = harmless_game();
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let mut weapon = crate::ItemSpec::ordinary("later weapon".into());
    weapon.class = crate::ItemClass::Weapon;
    weapon.equipment = Some(crate::EquipmentSpec {
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: crate::EquipmentSlot::new("weapon").unwrap(),
        attack: Some(crate::test_creatures::melee(
            99,
            900,
            crate::combat::DamageType::Keen,
            99,
        )),
        modifiers: vec![],
    });
    let item = game
        .spawn_item_stack(
            game.actors[&ActorId(1)].location,
            Some(ActorId(1)),
            1,
            weapon,
        )
        .unwrap();
    game.equip_authored(ActorId(1), item, EquipmentSlotId(0))
        .unwrap();
    let mut original_shared = crate::checkpoint::SharedState::default();
    let original_snapshot = game.checkpoint(&mut original_shared);
    let restored = crate::Game::restore_checkpoint(
        original_snapshot,
        &original_shared,
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(restored.preparation(ActorId(1)).unwrap().plan, captured);
    assert!(captured.valid(game.content().rules()));
    let mut forged = serde_json::to_value(&captured).unwrap();
    let component = &mut forged["strikes"][0]["on_success"][0]["damage"]["components"][0];
    // Use the ordinary serializer for the owned damage payload.
    let harmful = crate::damage::DamageComponent::fixed(crate::combat::DamageType::Impact, None, 1);
    let spec = crate::damage::DamageSpec::new(vec![harmful.clone()], Some(harmful.key())).unwrap();
    let encoded = serde_json::to_value(PlannedEffect::Damage { damage: spec }).unwrap();
    *component = encoded["damage"]["components"][0].clone();
    let forged: Plan = serde_json::from_value(forged).unwrap();
    assert!(!forged.valid(game.content().rules()));
    let before = game.clone();
    assert!(game.start_work(forged.clone(), None).is_err());
    assert_eq!(game, before, "rejected frozen admission is atomic");
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let encoded_snapshot = serde_json::to_value(&snapshot).unwrap();
    let mut encoded_shared = serde_json::to_value(&shared).unwrap();
    encoded_shared["actors"][encoded_snapshot["actors"].as_u64().unwrap() as usize]["1"]
        ["pending"]["plan"] = serde_json::to_value(forged).unwrap();
    let shared = serde_json::from_value(encoded_shared).unwrap();
    assert!(crate::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
}

#[test]
fn effective_interaction_natural_riders_keep_their_unmodified_rolled_amount() {
    let (game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.interaction = Interaction::NonHostile;
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![RiderDef {
                    condition: None,
                    trigger: RiderTrigger::Success,
                    check: None,
                    effects: vec![damage(rolled())],
                    descriptors: vec![],
                }],
            };
        },
        |_, species| {
            species.modifiers.extend(modifiers(3));
            let strikes = vec![StrikeDef {
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![Effect::Heal {
                    amount: AmountExpr::Fixed { amount: 0 },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }];
            species.melee = crate::attacks::NaturalAttack::new(
                species.melee.skill(),
                species.melee.bonus(),
                species.melee.wind_up(),
                species.melee.recovery(),
                strikes,
            )
            .unwrap();
        },
    );
    let plan = game
        .preparation(ActorId(1))
        .unwrap()
        .plan
        .as_ability()
        .unwrap();
    let PlannedEffect::Damage { damage } = &plan.strikes[0].riders[0].effects[0] else {
        panic!("damage")
    };
    assert_eq!(
        damage.components()[0].amount(),
        crate::damage::DamageAmount::Rolled(crate::dice::DicePool::new(1, 2, -2).unwrap())
    );
    assert!(plan.valid(game.content().rules()));
}

#[test]
fn effective_interaction_hit_die_drain_is_harm_but_restoration_is_not() {
    let rules = crate::test_creatures::ruleset();
    let mut strikes = crate::test_creatures::species().melee.strikes().to_vec();
    for (effect, planned, harmful) in [
        (
            Effect::DrainLatestHitDie,
            PlannedEffect::DrainLatestHitDie,
            true,
        ),
        (Effect::RestoreHitDie, PlannedEffect::RestoreHitDie, false),
    ] {
        strikes[0].on_success = vec![effect];
        strikes[0].on_failure.clear();
        strikes[0].riders.clear();
        assert_eq!(
            rules
                .validate_interaction(Interaction::NonHostile, &strikes)
                .is_err(),
            harmful
        );
        assert_eq!(
            rules
                .validate_planned_interaction(Interaction::NonHostile, std::iter::once(&planned))
                .is_err(),
            harmful
        );
    }
}

#[test]
fn effective_interaction_extra_dice_follow_rolled_and_fixed_scopes() {
    for (natural, amount, harmful) in [
        (true, AmountExpr::Fixed { amount: 0 }, false),
        (true, rolled(), true),
        (false, rolled(), true),
    ] {
        let mut creature = CreatureState::new(build(natural, amount, 0).unwrap()).unwrap();
        let before = creature.clone();
        let result = creature.set_equipment(vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            attack: None,
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Ability {
                        ability: AbilityId::new("basic_melee").unwrap(),
                        stat: AbilityStat::Dice,
                    },
                    value: 1,
                },
            }],
        }]);
        assert_eq!(result.is_err(), harmful);
        if harmful {
            assert_eq!(creature, before);
        }
    }
}

#[test]
fn effective_interaction_dormant_zero_rows_wait_for_current_unlocks() {
    let rules = crate::test_creatures::ruleset();
    let mut strikes = crate::test_creatures::species().melee.strikes().to_vec();
    let talent = crate::content_ids::TalentId::new("rattling_blow").unwrap();
    strikes[0].unlock = Some(talent.clone());
    strikes[0].on_success = vec![damage(rolled())];
    strikes[0].on_failure.clear();
    strikes[0].riders.clear();
    let values = crate::creatures::AbilityValues {
        flat: 3,
        ..Default::default()
    };
    assert!(rules
        .validate_effective_interaction(
            Interaction::NonHostile,
            &strikes,
            &values,
            false,
            &Default::default()
        )
        .is_ok());
    assert!(rules
        .validate_effective_interaction(
            Interaction::NonHostile,
            &strikes,
            &values,
            false,
            &[talent].into()
        )
        .is_err());
}

#[test]
fn effective_interaction_frozen_authored_rolled_damage_cannot_acquire_positive_operands() {
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.interaction = Interaction::NonHostile;
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            strikes[0].on_success = vec![damage(rolled())];
        },
        |_, _| {},
    );
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    assert!(captured.valid(game.content().rules()));
    let mut encoded = serde_json::to_value(&captured).unwrap();
    encoded["strikes"][0]["on_success"][0]["damage"]["components"][0]["amount"]["bonus"] =
        serde_json::json!(1);
    let forged: Plan = serde_json::from_value(encoded).unwrap();
    assert!(!forged.valid(game.content().rules()));
    let before = game.clone();
    assert!(game.start_work(forged, None).is_err());
    assert_eq!(game, before);
}

fn combined_source_creature(natural: bool) -> CreatureState {
    CreatureState::new(
        build(
            natural,
            AmountExpr::Rolled {
                dice: DiceExpr {
                    count: 1,
                    sides: 2,
                    bonus: -5,
                },
            },
            0,
        )
        .unwrap(),
    )
    .unwrap()
}
fn combined_source_gear() -> Vec<EquipmentSource> {
    vec![EquipmentSource {
        weapon_profile: None,
        item: ItemId(1),
        socket: EquipmentSlotId(0),
        attack: None,
        modifiers: modifiers(3),
    }]
}
#[test]
fn effective_interaction_combined_gear_cannot_evict_the_existing_condition() {
    for natural in [false, true] {
        let mut creature = combined_source_creature(natural);
        creature
            .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
            .unwrap();
        let before = creature.clone();
        assert!(
            creature.set_equipment(combined_source_gear()).is_err(),
            "natural={natural}"
        );
        assert_eq!(
            creature, before,
            "invalid gear must preserve the condition and all state"
        );
    }
}
#[test]
fn effective_interaction_combined_condition_cannot_evict_the_existing_gear() {
    for natural in [false, true] {
        let mut creature = combined_source_creature(natural);
        creature.set_equipment(combined_source_gear()).unwrap();
        let before = creature.clone();
        assert!(creature
            .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
            .is_err());
        assert_eq!(creature, before);
    }
}

#[test]
fn effective_interaction_combined_rebuild_cannot_evict_the_existing_condition() {
    for natural in [false, true] {
        let mut creature = combined_source_creature(natural);
        creature
            .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
            .unwrap();
        let replacement = build(
            natural,
            AmountExpr::Rolled {
                dice: DiceExpr {
                    count: 1,
                    sides: 2,
                    bonus: -5,
                },
            },
            3,
        )
        .unwrap();
        let before = creature.clone();
        assert!(creature.rebuild(replacement).is_err(), "natural={natural}");
        assert_eq!(creature, before);
    }
}

#[test]
fn effective_interaction_harmless_numeric_pool_conflict_keeps_soft_reconciliation() {
    let dice_modifier = |value| Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Ability {
                ability: AbilityId::new("basic_melee").unwrap(),
                stat: AbilityStat::Dice,
            },
            value,
        },
    };
    for natural in [false, true] {
        let base = build(
            natural,
            AmountExpr::Rolled {
                dice: DiceExpr {
                    count: 62,
                    sides: 2,
                    bonus: -1000,
                },
            },
            0,
        )
        .unwrap();
        let mut document = base.ruleset().document().clone();
        document
            .conditions
            .iter_mut()
            .find(|row| row.id.as_str() == "fear")
            .unwrap()
            .modifiers = vec![dice_modifier(1)];
        let content = crate::creatures::Content::new(
            Ruleset::validate(document).unwrap(),
            base.content().catalog().clone(),
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            base.species_id().clone(),
            base.ledger().clone(),
            base.binding(),
            *base.starting_skills(),
        )
        .unwrap();
        let mut creature = CreatureState::new(build).unwrap();
        creature
            .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
            .unwrap();
        let outcome = creature
            .set_equipment(vec![EquipmentSource {
                weapon_profile: None,
                item: ItemId(1),
                socket: EquipmentSlotId(0),
                attack: None,
                modifiers: vec![dice_modifier(2)],
            }])
            .expect("a harmless numeric pool conflict retains ordinary soft reconciliation");
        assert!(outcome.conditions_cleared);
        assert_eq!(creature.conditions().instances().count(), 0);
        assert_eq!(
            creature
                .derived()
                .ability_values(&AbilityId::new("basic_melee").unwrap())
                .dice,
            2
        );
    }
}

#[test]
fn effective_interaction_numeric_conflict_does_not_hide_later_direct_harm() {
    let rules = crate::test_creatures::ruleset();
    let mut strikes = crate::test_creatures::species().melee.strikes().to_vec();
    strikes[0].on_success = vec![
        damage(AmountExpr::Rolled {
            dice: DiceExpr {
                count: 62,
                sides: 2,
                bonus: -1000,
            },
        }),
        damage(rolled()),
    ];
    strikes[0].on_failure.clear();
    strikes[0].riders.clear();
    assert_eq!(
        rules.validate_effective_interaction(
            Interaction::NonHostile,
            &strikes,
            &crate::creatures::AbilityValues {
                dice: 3,
                ..Default::default()
            },
            false,
            &Default::default()
        ),
        Err(EffectiveInteractionError::DirectHarm)
    );
}

/// A valid owned talent can be dormant in a retained build. Growing the next
/// ordinal must not conceal the invariant error when that talent activates.
fn growth_invariant_build(minimum: u16) -> CreatureBuild {
    use crate::creatures::{AdvancementPurchase, PurchaseSelection};
    use crate::earned_growth::{FutureGrowth, GrowthSource};
    use crate::progression::{HdLedger, HdSource};
    let base = build(true, AmountExpr::Fixed { amount: 0 }, 0).unwrap();
    let mut document = base.ruleset().document().clone();
    document.growth.reward_base = 100;
    document.growth.reward_per_relative_tier = 0;
    document.growth.minimum_reward = 100;
    document.growth.maximum_reward = 100;
    let mut talent = crate::test_creatures::talent("fixture_growth_harm", modifiers(1));
    talent.requirement = Requirement::TotalHd {
        minimum: crate::HdCount(u64::from(minimum)),
    };
    let id = talent.id.clone();
    document.talents.push(talent);
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        base.content().catalog().clone(),
    )
    .unwrap();
    let mut advancement = base.advancement().clone();
    advancement.choices[0].purchases.push(AdvancementPurchase {
        pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: PurchaseSelection::Talent(id.clone()),
    });
    let mut build = CreatureBuild::from_recorded(
        content,
        base.species_id().clone(),
        base.initial_attributes(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        base.binding(),
        advancement,
        vec![],
    )
    .unwrap();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    assert!(!build.derive().unwrap().active_talents.contains(&id));
    assert_eq!(
        crate::creatures::record::BuildRecord::capture(&build)
            .restore(build.content().clone())
            .unwrap(),
        build,
        "the retained build must pass ordinary strict restore before growth",
    );
    build
}

#[test]
fn growth_invariant_failure_rolls_back_the_whole_creature_award() {
    use crate::creatures::{BuildError, CreatureStateError};
    for (minimum, award) in [(2, 100), (3, 300)] {
        let build = growth_invariant_build(minimum);
        let mut attempted = build.clone();
        for _ in 2..minimum {
            attempted
                .add_hit_die(crate::progression::HdSource::Racial, 42)
                .unwrap();
        }
        let before_failure = attempted.clone();
        assert_eq!(
            attempted.add_hit_die(crate::progression::HdSource::Racial, 42),
            Err(BuildError::InvalidInteraction)
        );
        assert_eq!(attempted, before_failure);
        let mut creature = CreatureState::new(build).unwrap();
        let before = creature.clone();
        assert_eq!(
            creature.award_xp(award),
            Err(CreatureStateError::Build(BuildError::InvalidInteraction))
        );
        assert_eq!(
            creature, before,
            "all staged ordinals and the award roll back if a later ordinal fails"
        );
    }
}

#[test]
fn growth_invariant_failure_rolls_back_fatal_damage_inventory_and_rng() {
    use crate::creatures::{BuildError, CreatureStateError};
    let build = growth_invariant_build(2);
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
                name: format!("growth subject {x}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.spawn_item_stack(
        game.actors[&ActorId(2)].location,
        Some(ActorId(2)),
        1,
        crate::ItemSpec::ordinary("retained stock".into()),
    )
    .unwrap();
    let before = game.clone();
    for _ in 0..2 {
        assert_eq!(
            game.apply(super::Outcome {
                actor: ActorId(1),
                target: ActorId(2),
                ability: None,
                intention: None,
                result: super::OutcomeStatus::Applied,
                check: None,
                participation: super::AttemptedParticipation::target(),
                target_rider_participated: false,
                effects: vec![super::ResolvedEffect::Damage { amount: 1_000_000 }],
            }),
            Err(crate::GameError::Growth(CreatureStateError::Build(
                BuildError::InvalidInteraction
            )))
        );
        assert_eq!(
            game, before,
            "fatal damage, XP, owned rows, inventory, corpse IDs, RNG and clocks roll back together"
        );
    }
}

#[test]
fn growth_harmless_numeric_pool_conflict_retains_xp_and_conditions_until_source_changes() {
    use crate::earned_growth::{FutureGrowth, GrowthSource};
    use crate::progression::{HdLedger, HdSource, HealthOrigin, HitDie};
    let base = build(
        false,
        AmountExpr::Rolled {
            dice: DiceExpr {
                count: 62,
                sides: 2,
                bonus: -1000,
            },
        },
        0,
    )
    .unwrap();
    let class = crate::content_ids::ClassId::new("mage").unwrap();
    let mut document = base.ruleset().document().clone();
    document
        .classes
        .iter_mut()
        .find(|row| row.id == class)
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Ability {
                    ability: AbilityId::new("basic_melee").unwrap(),
                    stat: AbilityStat::Dice,
                },
                value: 3,
            },
        });
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        base.content().catalog().clone(),
    )
    .unwrap();
    let mut build = CreatureBuild::from_recorded(
        content,
        base.species_id().clone(),
        base.initial_attributes(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        base.binding(),
        base.advancement().clone(),
        vec![],
    )
    .unwrap();
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Class { class },
        })
        .unwrap();
    let mut state = CreatureState::new(build.clone()).unwrap();
    state
        .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(2), 100)
        .unwrap();
    let conditions = state.conditions().clone();
    let derived = state.derived().clone();
    let epoch = state.build_epoch();
    state
        .award_xp(100)
        .expect("an authored numeric pool bound remains a retryable obstruction");
    assert_eq!(state.build(), &build);
    assert_eq!(state.derived(), &derived);
    assert_eq!(state.build_epoch(), epoch);
    assert_eq!(state.conditions(), &conditions);
    assert_eq!(state.earned_progress().held_xp(), 100);
    assert_eq!(state.earned_progress().spent_xp(), 0);
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    state.rebuild(build).unwrap();
    let seeded = HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap();
    let entries = seeded
        .entries()
        .iter()
        .enumerate()
        .map(|(ordinal, row)| {
            HitDie::from_recorded(
                row.source(),
                row.health_seed(),
                HealthOrigin {
                    templates: vec![],
                    kind: crate::content_ids::TypeId::new("humanoid").unwrap(),
                    die: 8,
                },
                0,
                ordinal,
            )
            .unwrap()
        })
        .collect();
    let expected = HdLedger::from_recorded(entries, Some(42)).unwrap();
    assert_eq!(state.build().ledger(), &expected);
    assert_eq!(state.conditions(), &conditions);
    assert_eq!(state.earned_progress().held_xp(), 0);
    assert_eq!(state.earned_progress().spent_xp(), 100);
}
