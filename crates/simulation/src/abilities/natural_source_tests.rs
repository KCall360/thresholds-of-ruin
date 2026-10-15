//! Reconstructed source behavior, using the existing ordinary frozen pipeline.
use super::{Plan, PlannedEffect, StrikeOrigin};
use crate::{rules::*, ActorId};
use serde_json::json;

fn damage(amount: u32) -> Effect {
    Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: crate::combat::DamageType::Impact,
                descriptor: None,
                amount: AmountExpr::Fixed { amount },
            }],
        },
    }
}
fn rider(amount: u32) -> RiderDef {
    RiderDef {
        trigger: RiderTrigger::Success,
        check: None,
        effects: vec![damage(amount)],
        descriptors: vec![],
    }
}
fn source_game() -> (crate::Game, crate::IntentionId) {
    super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![rider(3)],
            };
        },
        |_, species| {
            let natural = &species.melee;
            let strikes = [0, 10, 20]
                .into_iter()
                .enumerate()
                .map(|(ordinal, offset)| StrikeDef {
                    offset,
                    unlock: (ordinal == 2)
                        .then(|| crate::content_ids::TalentId::new("rattling_blow").unwrap()),
                    check: None,
                    on_success: vec![damage(4)],
                    on_failure: vec![],
                    descriptors: vec![],
                    riders: vec![rider(2)],
                })
                .collect();
            species.melee = crate::attacks::NaturalAttack::new(
                natural.skill(),
                natural.bonus(),
                natural.wind_up(),
                natural.recovery(),
                strikes,
            )
            .unwrap();
        },
    )
}
#[test]
fn natural_melee_source_appends_riders_and_preserves_dormant_source_ordinals() {
    let (mut game, _) = source_game();
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let plan = captured.as_ability().unwrap();
    assert_eq!(plan.strikes.len(), 2);
    for (ordinal, strike) in plan.strikes.iter().enumerate() {
        assert!(
            matches!(&strike.origin, StrikeOrigin::NaturalMelee { ordinal: source, natural_riders: 1, .. } if usize::from(*source) == ordinal)
        );
        assert_eq!(strike.riders.len(), 2);
    }
    assert!(game.plan_source_valid(&captured));
    for (tick, health) in [(10, 91), (20, 82)] {
        game.tick = tick;
        game.resolve_preparations().unwrap();
        assert_eq!(game.health(ActorId(2)).unwrap().0, health);
    }
    assert_eq!(game.actors[&ActorId(1)].ready_at, 40);
}
#[test]
fn natural_melee_source_ignores_equipped_weapon_operands_and_timing() {
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![RiderDef {
                    trigger: RiderTrigger::Success,
                    check: Some(RiderCheckDef {
                        skill: SkillSelector::SelectedWeapon,
                        defense: DefenseKind::Physical,
                    }),
                    effects: vec![Effect::Damage {
                        damage: DamageExpr::SelectedWeapon { impact_bonus: 0 },
                    }],
                    descriptors: vec![],
                }],
            };
            ability.timing.source = TimingSource::SelectedWeaponAdjusted {
                preparation: 5,
                recovery: 7,
            };
        },
        |_, _| {},
    );
    let original = game.preparation(ActorId(1)).unwrap().plan.clone();
    let mut weapon = crate::ItemSpec::ordinary("source selection probe".into());
    weapon.class = crate::ItemClass::Weapon;
    weapon.equipment = Some(crate::EquipmentSpec {
        slot: crate::EquipmentSlot::new("weapon").unwrap(),
        attack: Some(crate::test_creatures::melee(
            99,
            900,
            crate::combat::DamageType::Keen,
            99,
        )),
        modifiers: vec![],
    });
    let at = game.actors[&ActorId(1)].location;
    let item = game
        .spawn_item_stack(at, Some(ActorId(1)), 1, weapon)
        .unwrap();
    game.equip_authored(ActorId(1), item, crate::EquipmentSlotId(0))
        .unwrap();
    assert!(matches!(
        game.selected_melee_attack(ActorId(1)),
        Some(crate::attacks::SelectedMeleeAttack::Weapon(_))
    ));
    let fresh = game
        .ability_plan(
            ActorId(1),
            &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let before = original.as_ability().unwrap();
    let after = fresh.as_ability().unwrap();
    assert_eq!(after.strikes, before.strikes);
    assert_eq!(
        (after.preparation, after.recovery),
        (before.preparation, before.recovery)
    );
    assert!(game.plan_source_valid(&fresh));
}
#[test]
fn natural_melee_source_captured_plan_survives_transform_pause_and_checkpoint() {
    let (mut game, intention) = source_game();
    let mut catalog = game.content().catalog().clone();
    let mut other = catalog.species.values().next().unwrap().clone();
    other.id = crate::content_ids::SpeciesId::new("different_natural_source").unwrap();
    let mut rows = other.melee.strikes().to_vec();
    rows[0].on_success = vec![damage(17)];
    other.melee = crate::attacks::NaturalAttack::new(
        other.melee.skill(),
        other.melee.bonus(),
        other.melee.wind_up(),
        other.melee.recovery(),
        rows,
    )
    .unwrap();
    catalog.species.insert(other.id.clone(), other.clone());
    let content = crate::creatures::Content::new(game.content().rules().clone(), catalog).unwrap();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    // The source fixture parks the target for direct scheduling assertions;
    // restore its ordinary ready boundary before testing real checkpoint invariants.
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut game = crate::Game::restore_checkpoint(snapshot, &shared, content.clone()).unwrap();
    game.tick = 15;
    game.pause_preparation(ActorId(1)).unwrap();
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    let transformed = crate::creatures::CreatureBuild::new(
        content.clone(),
        other.id,
        build.ledger().clone(),
        build.binding(),
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    game.rebuild_creature(ActorId(1), transformed).unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
    let fresh = game
        .ability_plan(
            ActorId(1),
            &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    assert_ne!(
        fresh.as_ability().unwrap().strikes[0].on_success,
        captured.as_ability().unwrap().strikes[0].on_success
    );
    assert!(game.plan_source_valid(&captured));
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = crate::Game::restore_checkpoint(snapshot, &shared, content).unwrap();
    assert_eq!(restored, game);
    for candidate in [&mut game, &mut restored] {
        candidate.tick = 25;
        let resumed = candidate.plan_work(ActorId(1), captured.work()).unwrap();
        assert_eq!(resumed, captured);
        candidate.start_work(resumed, Some(intention)).unwrap();
        candidate.tick = 30;
        candidate.resolve_preparations().unwrap();
        assert_eq!(candidate.health(ActorId(2)).unwrap().0, 82);
    }
    assert_eq!(restored, game);
}
#[test]
fn natural_melee_source_rejects_forged_species_and_rider_boundary() {
    let (game, _) = source_game();
    let captured = &game.preparation(ActorId(1)).unwrap().plan;
    for (field, value) in [
        ("species", json!("missing")),
        ("ordinal", json!(15)),
        ("natural_riders", json!(0)),
    ] {
        let mut forged = serde_json::to_value(captured).unwrap();
        forged["strikes"][0]["origin"][field] = value;
        let forged: Plan = serde_json::from_value(forged).unwrap();
        assert!(!game.plan_source_valid(&forged), "{field}");
    }
    let mut missing = serde_json::to_value(captured).unwrap();
    missing["strikes"][0]["origin"]
        .as_object_mut()
        .unwrap()
        .remove("natural_riders");
    assert!(serde_json::from_value::<Plan>(missing).is_err());
}
#[test]
fn natural_melee_source_keeps_fixed_rider_values_and_shared_edge_scope() {
    let (game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![rider(3)],
            };
        },
        |rules, species| {
            let ability = rules
                .abilities
                .iter()
                .find(|ability| ability.basic_attack)
                .unwrap()
                .id
                .clone();
            for (stat, value) in [
                (
                    Stat::Ability {
                        ability,
                        stat: AbilityStat::FlatBonus,
                    },
                    5,
                ),
                (Stat::Edge, 5),
                (Stat::CheckEdge, -6),
            ] {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add { stat, value },
                });
            }
        },
    );
    let plan = game
        .preparation(ActorId(1))
        .unwrap()
        .plan
        .as_ability()
        .unwrap();
    let strike = &plan.strikes[0];
    assert_eq!(strike.edge, 5);
    assert_eq!(strike.check.as_ref().unwrap().edge, -1);
    let amount = |effect: &PlannedEffect| match effect {
        PlannedEffect::Damage { damage } => damage.components()[0].amount(),
        _ => panic!("damage"),
    };
    assert_eq!(
        amount(&strike.on_success[0]),
        crate::damage::DamageAmount::Fixed(9)
    );
    assert_eq!(
        amount(&strike.riders[0].effects[0]),
        crate::damage::DamageAmount::Fixed(3)
    );
}

#[test]
fn natural_melee_source_invalid_equipment_grant_rejects_atomically() {
    let mut document = crate::test_creatures::ruleset().document().clone();
    let mut probe = document
        .abilities
        .iter()
        .find(|ability| ability.basic_attack)
        .unwrap()
        .clone();
    probe.id = crate::content_ids::AbilityId::new("natural_grant_probe").unwrap();
    probe.basic_attack = false;
    probe.strikes = StrikeSource::NaturalMelee {
        additional_riders: vec![rider(3)],
    };
    let id = probe.id.clone();
    document.abilities.push(probe);
    let mut species = crate::test_creatures::species();
    let mut rows = species.melee.strikes().to_vec();
    rows[0].riders = vec![rider(2); 8];
    species.melee = crate::attacks::NaturalAttack::new(
        species.melee.skill(),
        species.melee.bonus(),
        species.melee.wind_up(),
        species.melee.recovery(),
        rows,
    )
    .unwrap();
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content,
        species_id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let mut creature = crate::creatures::CreatureState::new(build).unwrap();
    let before = creature.clone();
    let invalid = crate::creatures::EquipmentSource {
        item: crate::ItemId(1),
        socket: crate::EquipmentSlotId(0),
        attack: None,
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility { ability: id },
        }],
    };
    assert!(creature.set_equipment(vec![invalid]).is_err());
    assert_eq!(creature, before);
}

#[test]
fn natural_melee_source_cancel_discards_tail_without_replaying_paid_prefix() {
    let (mut game, intention) = source_game();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    game.pause_preparation(ActorId(1)).unwrap();
    game.cancel_intention_with_step(ActorId(1), intention, None)
        .unwrap();
    assert_eq!(game.health(ActorId(2)).unwrap().0, 91);
    assert!(game.preparation(ActorId(1)).is_none());
    assert!(game.resume_intention(ActorId(1), intention).is_err());
    game.tick = 100;
    game.resolve_preparations().unwrap();
    assert_eq!(game.health(ActorId(2)).unwrap().0, 91);
}

#[test]
fn natural_melee_selected_weapon_checks_keep_user_and_target_sources_separate() {
    use super::CheckThreshold;
    use crate::content_ids::AbilityId;
    let (mut game, _) = super::strike_tests::sequence_game_custom(
        &[0],
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![],
            };
        },
        |_, species| {
            let mut rows = species.melee.strikes().to_vec();
            rows[0].check = Some(CheckDef {
                actor: CheckActor::User,
                skill: SkillSelector::SelectedWeapon,
                against: Against::Opposed {
                    skill: SkillSelector::SelectedWeapon,
                },
            });
            species.melee = crate::attacks::NaturalAttack::new(
                species.melee.skill(),
                species.melee.bonus(),
                species.melee.wind_up(),
                species.melee.recovery(),
                rows,
            )
            .unwrap();
        },
    );
    for actor in [ActorId(1), ActorId(2)] {
        let mut weapon = crate::ItemSpec::ordinary("opposed source probe".into());
        weapon.class = crate::ItemClass::Weapon;
        weapon.equipment = Some(crate::EquipmentSpec {
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
            .spawn_item_stack(game.actors[&actor].location, Some(actor), 1, weapon)
            .unwrap();
        game.equip_authored(actor, item, crate::EquipmentSlotId(0))
            .unwrap();
    }
    let target = ActorId(2);
    let captured = game
        .ability_plan(ActorId(1), &AbilityId::new("basic_melee").unwrap(), target)
        .unwrap();
    let check = captured.as_ability().unwrap().strikes[0]
        .check
        .as_ref()
        .unwrap();
    assert_eq!(
        check.bonus,
        game.creature(ActorId(1))
            .unwrap()
            .derived()
            .natural_attack
            .check_bonus
    );
    let CheckThreshold::Opposed { bonus, .. } = check.threshold else {
        panic!("opposed")
    };
    assert_eq!(
        bonus,
        game.creature(target)
            .unwrap()
            .derived()
            .selected_attack
            .check_bonus
    );
}
