//! Warm actor operations must consume a compiled sheet, never replay HD records.
use tor_simulation::attributes::Attribute;
use tor_simulation::content_ids::ConditionId;
use tor_simulation::creatures::{CreatureBuild, CreatureState, EquipmentSource};
use tor_simulation::diagnostics::work_counts;
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::rules::{Modifier, ModifierOp, Scope, Stat};
use tor_simulation::{ActorId, EquipmentSlotId, ItemId};

fn creature(hit_dice: usize) -> CreatureState {
    let base = super::creature_state::build();
    let build = CreatureBuild::new(
        base.content().clone(),
        base.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial; hit_dice], 42).unwrap(),
        base.binding(),
        *base.starting_skills(),
    )
    .unwrap();
    CreatureState::new(build).unwrap()
}

#[test]
fn ordinary_observations_share_the_compiled_counts_at_two_and_256_hit_dice() {
    use std::num::NonZeroU64;
    use tor_simulation::{CreatureIdentity, Game};
    use tor_world::{Location, Position, RegionId};
    for count in [2, 256] {
        let creature = creature(count);
        let records = creature.build().ledger().clone();
        let mut game = Game::two_room_in_stone(42, creature.build().content().clone());
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
            CreatureIdentity {
                name: "observed".into(),
                faction: "neutral".into(),
            },
            creature.build().clone(),
        )
        .unwrap();
        game.refresh_navigation();
        let summary = game.creature(actor).unwrap().derived().hit_dice.clone();
        let before = work_counts();
        for _ in 0..4 {
            let stats = game
                .observe(actor)
                .unwrap()
                .combat
                .unwrap()
                .own_stats
                .unwrap();
            assert!(stats.hit_dice.shares_storage(&summary));
            assert_eq!(
                stats.hit_dice.total,
                tor_simulation::HdCount(u64::try_from(count).unwrap())
            );
            assert_eq!(
                stats.hit_dice.racial,
                tor_simulation::HdCount(u64::try_from(count).unwrap())
            );
            assert!(stats.hit_dice.classes.is_empty());
        }
        let after = work_counts();
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
        assert_eq!(
            after.advancement_choice_visits,
            before.advancement_choice_visits
        );
        assert_eq!(
            after.health_provenance_copies,
            before.health_provenance_copies
        );
        assert_eq!(after.creature_build_copies, before.creature_build_copies);
        assert_eq!(
            after.permanent_creature_compiles,
            before.permanent_creature_compiles
        );
        assert_eq!(game.creature(actor).unwrap().build().ledger(), &records);
    }
}

#[test]
fn compiled_source_counts_change_only_with_the_permanent_build() {
    use tor_simulation::content_ids::ClassId;
    let base = super::creature_state::build();
    let mage = ClassId::new("mage").unwrap();
    let warrior = ClassId::new("warrior").unwrap();
    let mut build = CreatureBuild::new(
        base.content().clone(),
        base.species_id().clone(),
        HdLedger::seeded(
            vec![
                HdSource::Class(mage.clone()),
                HdSource::Racial,
                HdSource::Class(warrior.clone()),
                HdSource::Class(mage.clone()),
            ],
            42,
        )
        .unwrap(),
        base.binding(),
        *base.starting_skills(),
    )
    .unwrap();
    build
        .set_future_growth(tor_simulation::earned_growth::FutureGrowth::Selected {
            source: tor_simulation::earned_growth::GrowthSource::Class {
                class: mage.clone(),
            },
        })
        .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    let original = creature.derived().hit_dice.clone();
    assert_eq!(original.total, tor_simulation::HdCount(4));
    assert_eq!(original.racial, tor_simulation::HdCount(1));
    assert_eq!(
        original.classes,
        std::collections::BTreeMap::from([
            (mage.clone(), tor_simulation::HdCount(2)),
            (warrior, tor_simulation::HdCount(1))
        ])
    );
    creature.lose_attribute(Attribute::Strength, 1).unwrap();
    assert!(creature.derived().hit_dice.shares_storage(&original));
    let complete = creature.build().clone();
    let mut reduced = complete.clone();
    reduced.remove_latest().unwrap();
    creature.rebuild(reduced).unwrap();
    assert_eq!(
        creature.derived().hit_dice.total,
        tor_simulation::HdCount(3)
    );
    assert_eq!(
        creature.derived().hit_dice.classes[&mage],
        tor_simulation::HdCount(1)
    );
    assert_eq!(original.classes[&mage], tor_simulation::HdCount(2));
    creature.rebuild(complete).unwrap();
    assert_eq!(creature.derived().hit_dice, original);
    while creature.derived().total_hd().get() > 0 {
        let mut reduced = creature.build().clone();
        reduced.remove_latest().unwrap();
        creature.rebuild(reduced).unwrap();
    }
    assert_eq!(
        creature.derived().hit_dice.total,
        tor_simulation::HdCount(0)
    );
    assert_eq!(
        creature.derived().hit_dice.racial,
        tor_simulation::HdCount(0)
    );
    assert!(creature.derived().hit_dice.classes.is_empty());
}

#[test]
fn warm_dynamic_sources_never_resample_owned_health_at_any_hd() {
    let mut previous_folds = None;
    for hit_dice in [2, 256] {
        let mut creature = creature(hit_dice);
        let provenance = creature.derived().health_contributions.clone();
        let records = creature.build().ledger().clone();
        let before = work_counts();
        creature
            .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(9), 10)
            .unwrap();
        creature
            .set_equipment(vec![EquipmentSource {
                weapon_profile: None,
                item: ItemId(19),
                socket: EquipmentSlotId(0),
                modifiers: vec![Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Attribute {
                            attribute: Attribute::Strength,
                        },
                        value: 1,
                    },
                }],
                attack: None,
            }])
            .unwrap();
        creature.lose_attribute(Attribute::Strength, 1).unwrap();
        creature.restore_attribute(Attribute::Strength, 1).unwrap();
        creature.lose_maximum_health(1).unwrap();
        creature.restore_maximum_health(1).unwrap();
        creature.advance_active(10);
        creature.set_equipment(vec![]).unwrap();
        let after = work_counts();
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(after.health_derivations - before.health_derivations, 0);
        assert_eq!(
            after.permanent_input_comparisons - before.permanent_input_comparisons,
            0,
            "gear, conditions and losses must not compare permanent HD inputs"
        );
        assert_eq!(
            after.permanent_creature_compiles - before.permanent_creature_compiles,
            0
        );
        assert_eq!(after.raw_hit_die_visits - before.raw_hit_die_visits, 0);
        assert_eq!(
            after.advancement_choice_visits - before.advancement_choice_visits,
            0
        );
        assert_eq!(
            after.health_provenance_copies - before.health_provenance_copies,
            0
        );
        assert_eq!(
            after.creature_build_copies - before.creature_build_copies,
            0
        );
        let folds = after.dynamic_creature_folds - before.dynamic_creature_folds;
        assert!(folds > 0);
        if let Some(previous) = previous_folds {
            assert_eq!(
                folds, previous,
                "warm work depends on sources, not HD count"
            );
        }
        previous_folds = Some(folds);
        assert!(creature
            .derived()
            .health_contributions
            .shares_storage(&provenance));
        assert_eq!(creature.derived().health_contributions, provenance);
        assert_eq!(creature.build().ledger(), &records);
        assert_eq!(creature.derived(), &creature.build().derive().unwrap());
    }
}

#[test]
fn condition_refresh_and_immune_application_do_not_resample_health() {
    let mut creature = creature(256);
    let fear = ConditionId::new("fear").unwrap();
    creature.apply_condition(&fear, ActorId(9), 10).unwrap();
    let before = work_counts();
    creature.apply_condition(&fear, ActorId(9), 20).unwrap();
    assert_eq!(
        work_counts().health_derivations - before.health_derivations,
        0
    );
    assert_eq!(
        work_counts().dynamic_creature_folds - before.dynamic_creature_folds,
        0
    );
    let mut build = creature.build().clone();
    build
        .set_templates(vec![tor_simulation::content_ids::TemplateId::new(
            "immune_form",
        )
        .unwrap()])
        .unwrap();
    creature.rebuild(build).unwrap();
    let before = work_counts();
    creature.apply_condition(&fear, ActorId(9), 20).unwrap();
    assert_eq!(
        work_counts().health_derivations - before.health_derivations,
        0
    );
    assert_eq!(
        work_counts().dynamic_creature_folds - before.dynamic_creature_folds,
        0
    );
}

#[test]
fn permanent_changes_recompile_once_and_future_source_edits_only_advance_epoch() {
    use tor_simulation::attributes::Skill;
    use tor_simulation::earned_growth::{FutureGrowth, GrowthSource};
    let mut creature = creature(2);
    let mut trained = creature.build().clone();
    trained
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    let before = work_counts();
    creature.rebuild(trained).unwrap();
    assert_eq!(
        work_counts().permanent_input_comparisons - before.permanent_input_comparisons,
        1,
        "explicit rebuild is a positive control for the comparison counter"
    );
    assert_eq!(
        work_counts().permanent_creature_compiles - before.permanent_creature_compiles,
        1
    );
    assert_eq!(
        work_counts().health_derivations - before.health_derivations,
        2
    );
    assert_eq!(creature.derived().skills.get(Skill::HeavyWeaponry), 1);
    let provenance = creature.derived().health_contributions.clone();
    let epoch = creature.build_epoch();
    let mut selected = creature.build().clone();
    selected
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    let before = work_counts();
    creature.rebuild(selected).unwrap();
    assert_eq!(
        work_counts().permanent_input_comparisons - before.permanent_input_comparisons,
        1,
        "even a future-source-only edit compares the permanent inputs once"
    );
    assert_ne!(creature.build_epoch(), epoch);
    assert_eq!(
        work_counts().permanent_creature_compiles - before.permanent_creature_compiles,
        0
    );
    assert_eq!(
        work_counts().dynamic_creature_folds - before.dynamic_creature_folds,
        0
    );
    assert!(creature
        .derived()
        .health_contributions
        .shares_storage(&provenance));
    let complete = creature.build().clone();
    let mut reduced = complete.clone();
    reduced.remove_latest().unwrap();
    let before = work_counts();
    creature.rebuild(reduced).unwrap();
    assert_eq!(
        work_counts().permanent_creature_compiles - before.permanent_creature_compiles,
        1
    );
    assert_eq!(creature.derived().total_hd().get(), 1);
    // Explicitly reinstall the exact earlier records; latest-HD effects are a separate lane.
    creature.rebuild(complete).unwrap();
    assert_eq!(creature.derived().total_hd().get(), 2);
    assert_eq!(creature.derived().health_contributions, provenance);
    assert_eq!(creature.derived(), &creature.build().derive().unwrap());
}

#[test]
fn warm_runtime_and_rejected_equipment_preserve_holds_and_exact_records() {
    use tor_simulation::costs::ResourceCost;
    use tor_simulation::resources::Resource;
    use tor_simulation::IntentionId;
    let mut creature = creature(256);
    let before = work_counts();
    creature.damage(1);
    creature
        .start_cost(
            IntentionId(1),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 1,
            })
            .unwrap(),
        )
        .unwrap();
    creature.advance_active(1);
    creature.heal(1);
    creature.restore_resource(Resource::Stamina, 1);
    let retained = creature.clone();
    assert!(creature
        .set_equipment(vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(19),
            socket: EquipmentSlotId(0),
            attack: None,
            modifiers: vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::Health,
                    value: i64::MAX
                },
            }],
        }])
        .is_err());
    assert_eq!(creature, retained);
    assert_eq!(
        work_counts().permanent_creature_compiles - before.permanent_creature_compiles,
        0
    );
    assert_eq!(
        work_counts().health_derivations - before.health_derivations,
        0
    );
    assert_eq!(
        work_counts().health_provenance_copies - before.health_provenance_copies,
        0
    );
    assert_eq!(
        work_counts().creature_build_copies - before.creature_build_copies,
        0
    );
    creature
        .lose_maximum_health(u32::try_from(creature.health().maximum()).unwrap())
        .unwrap();
    assert!(creature.health().dead());
    assert!(creature.costs().reservations().is_empty());
    creature.restore_maximum_health(1).unwrap();
    assert!(creature.health().dead());
}

#[test]
fn recorded_restore_rebuilds_private_compilation_and_preserves_warm_work() {
    use tor_simulation::creatures::record::CreatureRecord;
    let mut original = creature(256);
    let fear = ConditionId::new("fear").unwrap();
    original.apply_condition(&fear, ActorId(9), 20).unwrap();
    original.lose_attribute(Attribute::Strength, 1).unwrap();
    original.damage(1);
    let record = CreatureRecord::capture(&original);
    let bytes = serde_json::to_vec(&record).unwrap();
    let decoded: CreatureRecord = serde_json::from_slice(&bytes).unwrap();
    let mut restored = decoded
        .restore(original.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(
        serde_json::to_vec(&CreatureRecord::capture(&restored)).unwrap(),
        bytes
    );
    assert!(!restored
        .derived()
        .health_contributions
        .shares_storage(&original.derived().health_contributions));
    let before = work_counts();
    restored.advance_active(20);
    restored.restore_attribute(Attribute::Strength, 1).unwrap();
    assert_eq!(
        work_counts().permanent_creature_compiles - before.permanent_creature_compiles,
        0
    );
    assert_eq!(
        work_counts().health_derivations - before.health_derivations,
        0
    );
    assert_eq!(restored.derived(), &restored.build().derive().unwrap());
}

#[test]
fn warm_armed_folds_keep_independent_natural_operands_without_visiting_hit_dice() {
    use tor_simulation::attacks::MeleeAttack;
    use tor_simulation::attributes::{Skill, SkillCheck};
    use tor_simulation::combat::DamageType;
    for count in [2, 256] {
        let mut creature = creature(count);
        let natural = creature.derived().natural_attack.clone();
        let records = creature.build().ledger().clone();
        let before = work_counts();
        creature
            .set_equipment(vec![EquipmentSource {
                weapon_profile: None,
                item: ItemId(19),
                socket: EquipmentSlotId(0),
                modifiers: vec![Modifier {
                    cancellable: false,
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Attribute {
                            attribute: Attribute::Strength,
                        },
                        value: 1,
                    },
                }],
                attack: Some(
                    MeleeAttack::fixed(
                        Skill::LightWeaponry,
                        99,
                        900,
                        800,
                        DamageType::Keen,
                        None,
                        99,
                    )
                    .unwrap(),
                ),
            }])
            .unwrap();
        creature.lose_attribute(Attribute::Strength, 1).unwrap();
        let derived = creature.derived();
        assert_eq!(derived.natural_attack.attack, natural.attack);
        assert_eq!(derived.natural_attack.source, natural.source);
        assert_eq!(derived.natural_attack.socket, None);
        assert_ne!(
            derived.natural_attack.source,
            derived.selected_attack.source
        );
        assert_eq!(
            derived.natural_attack.check_bonus,
            SkillCheck {
                skill: natural.attack.skill(),
                binding: derived.binding(),
                modifier: natural.attack.bonus(),
                threshold: 0,
            }
            .bonus(derived.attributes, derived.skills)
        );
        assert_ne!(
            derived.natural_attack.check_bonus,
            derived.selected_attack.check_bonus
        );
        creature.restore_attribute(Attribute::Strength, 1).unwrap();
        creature.set_equipment(vec![]).unwrap();
        assert_eq!(
            creature.derived().natural_attack,
            creature.derived().selected_attack
        );
        let after = work_counts();
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(
            after.permanent_creature_compiles,
            before.permanent_creature_compiles
        );
        assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
        assert_eq!(
            after.advancement_choice_visits,
            before.advancement_choice_visits
        );
        assert_eq!(after.creature_build_copies, before.creature_build_copies);
        assert_eq!(
            after.health_provenance_copies,
            before.health_provenance_copies
        );
        assert_eq!(creature.build().ledger(), &records);
    }
}

#[test]
fn compiled_species_selected_natural_preparation_does_not_read_raw_species() {
    use std::num::NonZeroU64;
    use tor_simulation::abilities::StrikeOrigin;
    use tor_simulation::content_ids::AbilityId;
    use tor_simulation::creatures::SourceId;
    use tor_simulation::{CreatureIdentity, Game};
    use tor_world::{Location, Position, RegionId};
    for count in [2, 256] {
        let creature = creature(count);
        let records = creature.build().ledger().clone();
        let SourceId::Species(species) = creature.derived().selected_attack.source.clone() else {
            panic!("fixture selects its natural attack");
        };
        let mut game = Game::two_room_in_stone(42, creature.build().content().clone());
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
                CreatureIdentity {
                    name: format!("natural subject {x}"),
                    faction: "neutral".into(),
                },
                creature.build().clone(),
            )
            .unwrap();
        }
        let ability = AbilityId::new("basic_melee").unwrap();
        let before = work_counts();
        for _ in 0..4 {
            let plan = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
            let strikes = &plan.as_ability().unwrap().strikes;
            assert!(!strikes.is_empty());
            assert!(strikes.iter().enumerate().all(|(index, strike)|
                matches!(&strike.origin, StrikeOrigin::Natural { species: origin, ordinal }
                    if origin == &species && usize::from(*ordinal) == index)));
            assert!(plan.valid(game.content().rules()));
        }
        let after = work_counts();
        assert_eq!(after.ability_plans - before.ability_plans, 4);
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
        assert_eq!(
            game.creature(ActorId(1)).unwrap().build().ledger(),
            &records
        );
    }
}
