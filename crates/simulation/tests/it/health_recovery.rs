//! Health recovery keeps injury authoritative and uses active time only.
use super::creature_fixture as fixture;
use tor_simulation::creatures::CreatureState;

#[test]
fn injured_creature_recovers_after_active_rest_without_spending_a_resource() {
    let mut creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    creature.damage(3);
    let resources = creature.costs().resources().clone();
    creature.advance_active(1_200);
    assert_eq!(creature.health().injury(), 2);
    assert_eq!(creature.costs().resources(), &resources);
    creature.advance_active(u64::MAX);
    assert_eq!(creature.health().injury(), 0);
    assert_eq!(creature.next_change_in(), None);
}

use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::{
    abilities::{Outcome, OutcomeStatus, ResolvedEffect},
    creatures::{record::CreatureRecord, CreatureBuild, EquipmentSource},
    health_recovery::RecoveryNutrition,
    progression::HdSource,
    rules::{Modifier, ModifierOp, Scope, Stat},
    Action, ActorId, CreatureIdentity, EquipmentSlotId, Game, ItemId, MemoryRecords,
    RegionTransition,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn modifier(stat: Stat, value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add { stat, value },
    }
}
fn with_modifiers(modifiers: Vec<Modifier>) -> CreatureBuild {
    let mut species = fixture::species();
    species.modifiers.extend(modifiers);
    fixture::build(species)
}
fn game(build: CreatureBuild) -> Game {
    let world = World::new(
        (1..=3)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room {id}"),
                bounds: Extent::new(8, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, build.content().clone());
    for region in [1, 3] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(region),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(1).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {region}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        Default::default(),
    )
    .unwrap();
    game.add_default_reference_points().unwrap();
    game.refresh_navigation();
    game
}
fn rest(game: &mut Game, ticks: u64) {
    let until = game.tick() + ticks;
    while game.tick() < until {
        let next = game.next_actor().unwrap();
        game.act(next, Action::Wait).unwrap();
    }
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let data = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&data).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}
fn damage(game: &mut Game, source: ActorId, target: ActorId, amounts: &[u32]) {
    game.apply(Outcome {
        actor: source,
        target,
        ability: None,
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: amounts
            .iter()
            .map(|amount| ResolvedEffect::Damage { amount: *amount })
            .collect(),
    })
    .unwrap();
}

#[test]
fn recovery_period_shortens_by_hd_and_stops_at_the_authored_floor() {
    for (hd, period) in [
        (1, 1200),
        (6, 1000),
        (11, 800),
        (16, 600),
        (21, 400),
        (26, 200),
        (256, 200),
    ] {
        let build = fixture::build_with(fixture::species(), vec![], &vec![HdSource::Racial; hd]);
        assert_eq!(build.derive().unwrap().health_recovery_period, period);
    }
}
#[test]
fn species_and_equipment_recovery_modifiers_use_the_shared_fold_and_round_up() {
    let build = with_modifiers(vec![
        modifier(Stat::HealthRecoveryPeriod, -199),
        modifier(Stat::HealthRecoveryPeriodPercent, -25),
    ]);
    let mut creature = CreatureState::new(build).unwrap();
    assert_eq!(creature.derived().health_recovery_period, 751);
    creature.damage(3);
    creature.advance_active(500);
    creature
        .set_equipment(vec![EquipmentSource {
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            attack: None,
            modifiers: vec![modifier(Stat::HealthRecoveryPeriodPercent, -25)],
        }])
        .unwrap();
    assert_eq!(creature.derived().health_recovery_period, 501);
    assert_eq!(creature.health_recovery().elapsed(), 333);
    assert_eq!(creature.health().injury(), 3);
    creature.advance_active(168);
    assert_eq!(creature.health().injury(), 2);
}
#[test]
fn recovery_modifiers_reject_unbounded_or_nonpositive_percentages() {
    for (stat, amount) in [
        (Stat::HealthRecoveryPeriodPercent, -100),
        (Stat::HealthRecoveryPeriodPercent, 10_000),
        (Stat::HealthRecoveryResetPercent, 1),
        (Stat::HealthRecoveryResetPercent, -101),
    ] {
        let mut species = fixture::species();
        species.modifiers.push(modifier(stat, amount));
        let Ok(content) = tor_simulation::creatures::Content::new(
            fixture::ruleset(),
            tor_simulation::creatures::ScenarioCatalog {
                species: [(species.id.clone(), species.clone())].into(),
                templates: Default::default(),
            },
        ) else {
            continue;
        };
        assert!(CreatureBuild::new(
            content,
            species.id,
            tor_simulation::progression::HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            tor_simulation::attributes::ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion
            ],
        )
        .is_err());
    }
}
#[test]
fn damage_to_health_resets_quiet_but_zero_damage_does_not() {
    let mut creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    creature.damage(3);
    creature.advance_active(1199);
    creature.damage(0);
    assert_eq!(creature.health_recovery().elapsed(), 1199);
    creature.damage(1);
    assert_eq!(creature.health_recovery().elapsed(), 0);
    creature.advance_active(1199);
    assert_eq!(creature.health().injury(), 4);
    creature.advance_active(1);
    assert_eq!(creature.health().injury(), 3);
}
#[test]
fn resolved_damage_resets_both_actors_once_including_multi_effect_self_damage() {
    for (source, target) in [(ActorId(1), ActorId(2)), (ActorId(1), ActorId(1))] {
        let build = with_modifiers(vec![modifier(Stat::HealthRecoveryResetPercent, -50)]);
        let mut game = game(build);
        fixture::injure(&mut game, ActorId(1), 3).unwrap();
        fixture::injure(&mut game, ActorId(2), 3).unwrap();
        rest(&mut game, 200);
        damage(&mut game, source, target, &[1, 1]);
        assert_eq!(
            game.creature(source).unwrap().health_recovery().elapsed(),
            100
        );
        assert_eq!(
            game.creature(target).unwrap().health_recovery().elapsed(),
            100
        );
        damage(&mut game, source, target, &[0]);
        assert_eq!(
            game.creature(source).unwrap().health_recovery().elapsed(),
            100
        );
        assert_eq!(
            game.creature(target).unwrap().health_recovery().elapsed(),
            100
        );
    }
}
#[test]
fn quiet_bypass_retains_progress_after_dealing_and_taking_actual_damage() {
    let mut game = game(with_modifiers(vec![modifier(
        Stat::HealthRecoveryResetPercent,
        -100,
    )]));
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    fixture::injure(&mut game, ActorId(2), 3).unwrap();
    rest(&mut game, 200);
    damage(&mut game, ActorId(1), ActorId(2), &[1]);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .health_recovery()
            .elapsed(),
        200
    );
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .elapsed(),
        200
    );
}
#[test]
fn frozen_and_detached_regions_never_heal_or_catch_up_after_checkpoint() {
    for detach in [false, true] {
        let mut game = game(fixture::build(fixture::species()));
        let actor = ActorId(2);
        fixture::injure(&mut game, actor, 3).unwrap();
        rest(&mut game, 400);
        let mut records = MemoryRecords::default();
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1)]),
                loaded: if detach {
                    BTreeSet::from([RegionId(1)])
                } else {
                    BTreeSet::from([RegionId(1), RegionId(3)])
                },
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.next_creature_event_tick(), None);
        rest(&mut game, 2400);
        game = round_trip(&game);
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(3)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
        assert_eq!(
            game.creature(actor).unwrap().health_recovery().elapsed(),
            400
        );
        assert_eq!(game.next_creature_event_tick(), Some(game.tick() + 200));
        let mut replay = round_trip(&game);
        for state in [&mut game, &mut replay] {
            rest(state, 200);
            assert_eq!(
                state
                    .creature(actor)
                    .unwrap()
                    .received_health_loss_remaining(),
                0
            );
            assert_eq!(state.creature(actor).unwrap().health().injury(), 3);
            assert_eq!(state.next_creature_event_tick(), Some(state.tick() + 600));
            rest(state, 600);
            assert_eq!(state.creature(actor).unwrap().health().injury(), 2);
            assert_eq!(
                state.creature(actor).unwrap().health_recovery().elapsed(),
                0
            );
        }
        assert_eq!(game, replay);
    }
}
#[test]
fn crossing_a_region_seam_preserves_injury_and_earned_progress() {
    let mut game = game(fixture::build(fixture::species()));
    let actor = ActorId(1);
    fixture::injure(&mut game, actor, 3).unwrap();
    rest(&mut game, 400);
    for region in [3, 1, 3, 1] {
        game.teleport(
            actor,
            Location {
                region: RegionId(region),
                position: Position { x: 2, y: 1, z: 0 },
            },
        )
        .unwrap();
        assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
        assert_eq!(
            game.creature(actor).unwrap().health_recovery().elapsed(),
            400
        );
    }
}
#[test]
fn nutrition_changes_settle_old_active_time_and_never_recover_stopped_ticks() {
    let mut game = game(fixture::build(fixture::species()));
    let actor = ActorId(1);
    fixture::injure(&mut game, actor, 3).unwrap();
    rest(&mut game, 400);
    game.set_recovery_nutrition(actor, RecoveryNutrition::Slowed { multiplier: 2 })
        .unwrap();
    assert_eq!(
        game.creature(actor).unwrap().health_recovery().elapsed(),
        800
    );
    rest(&mut game, 200);
    game.set_recovery_nutrition(actor, RecoveryNutrition::Stopped)
        .unwrap();
    assert_eq!(
        game.creature(actor).unwrap().health_recovery().elapsed(),
        500
    );
    rest(&mut game, 2000);
    game = round_trip(&game);
    game.set_recovery_nutrition(actor, RecoveryNutrition::Normal)
        .unwrap();
    rest(&mut game, 700);
    assert_eq!(game.creature(actor).unwrap().health().injury(), 2);
    assert_eq!(game.creature(actor).unwrap().health_recovery().elapsed(), 0);
}
#[test]
fn strict_health_recovery_records_reject_missing_progress_policy_and_impossible_state() {
    let mut creature = CreatureState::new(fixture::build(fixture::species())).unwrap();
    creature.damage(3);
    creature.advance_active(400);
    let original = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    let restored = serde_json::from_value::<CreatureRecord>(original.clone())
        .unwrap()
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
    let rejects = |value| {
        !serde_json::from_value::<CreatureRecord>(value).is_ok_and(|record| {
            record
                .restore(creature.build().content().clone(), vec![])
                .is_ok()
        })
    };
    let mut changed = original.clone();
    changed.as_object_mut().unwrap().remove("health_recovery");
    assert!(rejects(changed));
    for elapsed in [1200, u64::MAX] {
        let mut changed = original.clone();
        changed["health_recovery"]["elapsed"] = elapsed.into();
        assert!(rejects(changed));
    }
    let mut changed = original.clone();
    changed["injury"] = 0.into();
    assert!(rejects(changed));
    let mut changed = original.clone();
    changed["dead"] = true.into();
    assert!(rejects(changed));
    let mut changed = original.clone();
    changed["health_recovery"]["nutrition"] = serde_json::json!({"kind":"slowed","multiplier":0});
    assert!(rejects(changed));
    for kind in ["normal", "stopped"] {
        let mut changed = original.clone();
        changed["health_recovery"]["nutrition"] = serde_json::json!({"kind":kind,"unused":true});
        assert!(rejects(changed));
    }
}

#[test]
fn health_rest_to_full_baseline_measures_seeded_warrior_hd_bands() {
    use tor_simulation::{
        attributes::{Attributes, ByAttribute, ManaBinding},
        content_ids::ClassId,
        progression::HdLedger,
    };
    let mut species = fixture::unmodified_species("recovery_baseline");
    species.default_attributes = Attributes::new(ByAttribute::from_fn(|_| 1)).unwrap();
    let id = species.id.clone();
    let content = fixture::content_with(species, vec![]);
    for hd in [1, 6, 11, 16, 21, 26] {
        let mut times = Vec::new();
        let mut health = Vec::new();
        for seed in 0..32 {
            let build = CreatureBuild::new(
                content.clone(),
                id.clone(),
                HdLedger::seeded(
                    vec![HdSource::Class(ClassId::new("warrior").unwrap()); hd],
                    seed,
                )
                .unwrap(),
                ManaBinding::Intellect,
                [
                    tor_simulation::attributes::Skill::Deception,
                    tor_simulation::attributes::Skill::Persuasion,
                ],
            )
            .unwrap();
            let mut creature = CreatureState::new(build).unwrap();
            health.push(creature.health().maximum());
            let injury = creature.health().maximum() - 1;
            creature.damage(u32::try_from(injury).unwrap());
            let mut ticks = 0;
            while creature.health().injury() > 0 {
                let next = creature.next_change_in().unwrap();
                creature.advance_active(next);
                ticks += next;
            }
            assert_eq!(creature.health_recovery().elapsed(), 0);
            times.push(ticks);
        }
        times.sort_unstable();
        health.sort_unstable();
        eprintln!("health-rest-baseline hd={hd} n=32 max_health_p50={} ticks_p50={} ticks_p95={} ticks_max={}", health[15], times[15], times[30], times[31]);
    }
}

#[test]
fn immune_damage_and_overkill_use_actual_application_for_recovery_activity() {
    use tor_simulation::{
        combat::DamageType,
        rules::{AmountExpr, DamageComponentExpr, DamageExpr, Effect, Selector},
    };
    let build = with_modifiers(vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Category {
                category: DamageType::Impact,
            },
        },
    }]);
    let mut game = game(build);
    let actor = ActorId(1);
    fixture::injure(&mut game, actor, 3).unwrap();
    rest(&mut game, 400);
    game.apply_effects(
        actor,
        &[Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: DamageType::Impact,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 1_000_000 },
                }],
            },
        }],
    )
    .unwrap();
    assert_eq!(
        game.creature(actor).unwrap().health_recovery().elapsed(),
        400
    );
    assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
    game.apply_effects(
        actor,
        &[Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 1_000_000 },
                }],
            },
        }],
    )
    .unwrap();
    let creature = game.creature(actor).unwrap();
    assert!(creature.health().dead());
    assert_eq!(creature.health_recovery().elapsed(), 0);
    assert_eq!(creature.next_change_in(), None);
}
#[test]
fn expiring_recovery_modifier_settles_the_old_period_then_rephases_remaining_work() {
    use tor_simulation::{
        attributes::ManaBinding,
        content_ids::ConditionId,
        creatures::Content,
        progression::HdLedger,
        rules::{ConditionDef, Eviction, Ruleset, Stacking},
    };
    let build = fixture::build(fixture::species());
    let mut rules = build.ruleset().document().clone();
    let condition = ConditionId::new("recovery_boost").unwrap();
    rules.conditions.push(ConditionDef {
        id: condition.clone(),
        family: tor_simulation::content_ids::ConditionFamilyId::new(condition.as_str()).unwrap(),
        lifecycle: tor_simulation::rules::ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: "Recovery boost".into(),
        modifiers: vec![modifier(Stat::HealthRecoveryPeriodPercent, -50)],
        stacking: Stacking::Longest,
        source_scoped: false,
        descriptors: vec![],
        capacity: Eviction::Oldest,
    });
    let content = Content::new(
        Ruleset::validate(rules).unwrap(),
        (*build.content().catalog()).clone(),
    )
    .unwrap();
    let build = CreatureBuild::new(
        content,
        build.species().id.clone(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        *build.starting_skills(),
    )
    .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    creature.damage(3);
    creature
        .apply_condition(&condition, ActorId(1), 600)
        .unwrap();
    creature.advance_active(1200);
    assert_eq!(creature.health().injury(), 2);
    assert_eq!(creature.health_recovery().period(), 1200);
    assert_eq!(creature.health_recovery().elapsed(), 600);
    creature.advance_active(600);
    assert_eq!(creature.health().injury(), 1);
}

#[test]
fn ordinary_portal_crossing_earns_only_its_action_ticks_and_never_heals_at_the_seam() {
    use tor_world::{Direction, Passage};
    let build = fixture::build(fixture::species());
    let mut world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("seam {id}"),
                bounds: Extent::new(3, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let at = |region, x| Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    };
    for (from, direction, to) in [
        (at(1, 2), Direction::East, at(2, 0)),
        (at(2, 0), Direction::West, at(1, 2)),
    ] {
        world
            .connect_area(
                Passage {
                    from,
                    direction,
                    to,
                },
                0,
                1,
                1,
            )
            .unwrap();
    }
    let mut game = Game::new(world, 42, build.content().clone());
    let actor = game
        .spawn_actor(at(1, 2), NonZeroU64::new(10).unwrap())
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "walker".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    fixture::injure(&mut game, actor, 3).unwrap();
    rest(&mut game, 400);
    for direction in [
        Direction::East,
        Direction::West,
        Direction::East,
        Direction::West,
    ] {
        game.act(actor, Action::Move(direction)).unwrap();
        assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
        assert_eq!(
            game.creature(actor).unwrap().health_recovery().elapsed(),
            game.tick()
        );
    }
    assert_eq!(game.tick(), 440);
}

#[test]
fn health_cannot_be_selected_as_an_ability_cost_resource() {
    assert!(serde_json::from_str::<tor_simulation::resources::Resource>("\"health\"").is_err());
    assert!(
        serde_json::from_value::<tor_simulation::rules::Cost>(serde_json::json!({
            "resource": "health", "start": 1, "resolution": 1,
        }))
        .is_err()
    );
}

#[test]
fn personal_recovery_disclosure_projects_lazy_progress_without_mutating_the_game() {
    let mut game = game(fixture::build(fixture::species()));
    let actor = ActorId(1);
    fixture::injure(&mut game, actor, 3).unwrap();
    rest(&mut game, 400);
    let before = game.clone();
    let stats = game
        .observe(actor)
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert_eq!(stats.health_recovery.elapsed(), 400);
    assert_eq!(
        stats.health_recovery,
        game.inspect_creature(actor).unwrap().stats.health_recovery
    );
    assert_eq!(game, before);
}

#[test]
fn damage_resets_quiet_before_later_effects_can_change_its_policy() {
    use tor_simulation::{
        attributes::ManaBinding,
        content_ids::ConditionId,
        creatures::Content,
        progression::HdLedger,
        rules::{ConditionDef, Eviction, Ruleset, Stacking},
    };
    let original = fixture::build(fixture::species());
    let mut rules = original.ruleset().document().clone();
    let condition = ConditionId::new("steady_progress").unwrap();
    rules.conditions.push(ConditionDef {
        id: condition.clone(),
        family: tor_simulation::content_ids::ConditionFamilyId::new(condition.as_str()).unwrap(),
        lifecycle: tor_simulation::rules::ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: "Steady progress".into(),
        modifiers: vec![modifier(Stat::HealthRecoveryResetPercent, -100)],
        stacking: Stacking::Longest,
        source_scoped: false,
        descriptors: vec![],
        capacity: Eviction::Oldest,
    });
    let content = Content::new(
        Ruleset::validate(rules).unwrap(),
        (*original.content().catalog()).clone(),
    )
    .unwrap();
    let build = CreatureBuild::new(
        content,
        original.species().id.clone(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        *original.starting_skills(),
    )
    .unwrap();
    for target in [ActorId(1), ActorId(2)] {
        for damage_first in [true, false] {
            let mut game = game(build.clone());
            fixture::injure(&mut game, ActorId(1), 3).unwrap();
            fixture::injure(&mut game, ActorId(2), 3).unwrap();
            rest(&mut game, 400);
            let damage = ResolvedEffect::Damage { amount: 1 };
            let boost = ResolvedEffect::ApplyCondition {
                id: condition.clone(),
                duration: 1000,
            };
            let effects = if damage_first {
                vec![damage, boost]
            } else {
                vec![boost, damage]
            };
            game.apply(Outcome {
                actor: ActorId(1),
                target,
                ability: None,
                intention: None,
                result: OutcomeStatus::Applied,
                check: None,
                participation: tor_simulation::abilities::AttemptedParticipation::target(),
                target_rider_participated: false,
                effects,
            })
            .unwrap();
            assert_eq!(
                game.creature(target).unwrap().health_recovery().elapsed(),
                if damage_first { 0 } else { 400 }
            );
            if target != ActorId(1) {
                assert_eq!(
                    game.creature(ActorId(1))
                        .unwrap()
                        .health_recovery()
                        .elapsed(),
                    0
                );
            }
        }
    }
}

#[test]
fn publication_events_record_actual_damage_and_healing_without_overkill() {
    use tor_simulation::combat::CombatEvent;
    let mut game = game(fixture::build(fixture::species()));
    let source = ActorId(1);
    let target = ActorId(2);
    damage(&mut game, source, target, &[3]);
    game.apply(Outcome {
        actor: source,
        target,
        ability: None,
        intention: None,
        result: OutcomeStatus::Applied,
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![ResolvedEffect::Heal { amount: u32::MAX }],
    })
    .unwrap();
    let applied = game
        .combat_events()
        .iter()
        .rev()
        .find_map(|event| match event {
            CombatEvent::Resolved { outcome, .. } => Some(&outcome.effects),
            _ => None,
        })
        .unwrap();
    assert_eq!(applied, &[ResolvedEffect::Heal { amount: 3 }]);
    let remaining = game.creature(target).unwrap().health().current();
    damage(&mut game, source, target, &[u32::MAX, 5]);
    let applied = game
        .combat_events()
        .iter()
        .rev()
        .find_map(|event| match event {
            CombatEvent::Resolved { outcome, .. } => Some(&outcome.effects),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        applied,
        &[ResolvedEffect::Damage {
            amount: u32::try_from(remaining).unwrap()
        }]
    );
}

#[test]
fn publication_does_not_report_applied_when_all_effects_are_unchanged() {
    use tor_simulation::combat::CombatEvent;
    let mut game = game(fixture::build(fixture::species()));
    for (result, effects, expected) in [
        (
            OutcomeStatus::Applied,
            vec![ResolvedEffect::Heal { amount: 99 }],
            OutcomeStatus::Unaffected,
        ),
        (
            OutcomeStatus::Applied,
            vec![ResolvedEffect::Damage { amount: 0 }],
            OutcomeStatus::Unaffected,
        ),
        (OutcomeStatus::Miss, vec![], OutcomeStatus::Miss),
    ] {
        game.apply(Outcome {
            actor: ActorId(1),
            target: ActorId(2),
            ability: None,
            intention: None,
            result,
            check: None,
            participation: if result == OutcomeStatus::Miss {
                tor_simulation::abilities::AttemptedParticipation {
                    check: tor_simulation::abilities::CheckParticipation::None,
                    payload: tor_simulation::abilities::PayloadParticipation::Unselected {
                        reason: tor_simulation::abilities::UnselectedReason::UnavailableTarget,
                    },
                }
            } else {
                tor_simulation::abilities::AttemptedParticipation::target()
            },
            target_rider_participated: false,
            effects,
        })
        .unwrap();
        let applied = game
            .combat_events()
            .iter()
            .rev()
            .find_map(|event| match event {
                CombatEvent::Resolved { outcome, .. } => Some(outcome),
                _ => None,
            })
            .unwrap();
        assert_eq!(applied.result, expected);
        assert!(applied.effects.iter().all(|effect| matches!(
            effect,
            ResolvedEffect::Damage { amount: 0 } | ResolvedEffect::Heal { amount: 0 }
        )));
    }
}

#[test]
fn lethal_quiet_reset_cannot_restore_a_source_token_consumed_by_target_death() {
    use tor_simulation::{
        content_ids::{AbilityId, ConditionId},
        IntentionOrigin,
    };
    let (mut game, target, source) = super::abilities::duel(42);
    fixture::injure(&mut game, source, 3).unwrap();
    rest(&mut game, 100);
    // The earlier actor prepares while the injured source remains ready at
    // the same tick, so the scheduler exposes the in-progress reservation.
    assert_eq!(game.next_actor(), Some(target));
    game.apply_condition(source, &ConditionId::new("exposed").unwrap(), source, 500)
        .unwrap();
    game.admit_intention(
        target,
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: source,
            },
            supplies: Default::default(),
        },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.preparation(target).unwrap().plan.tokens().len(), 1);
    assert!(!game.creature(source).unwrap().conditions().is_empty());
    assert!(game.creature(source).unwrap().health_recovery().elapsed() > 0);
    let lethal = game.creature(target).unwrap().health().current();
    damage(&mut game, source, target, &[u32::try_from(lethal).unwrap()]);
    assert!(!game.alive(target));
    assert!(game.preparation(target).is_none());
    assert!(game.creature(source).unwrap().conditions().is_empty());
    assert_eq!(
        game.creature(source).unwrap().health_recovery().elapsed(),
        0
    );
    assert_eq!(game, round_trip(&game));
}

#[test]
fn game_advancement_settles_lazy_health_work_before_purchase_drain_and_authored_restore() {
    use tor_simulation::{
        content_ids::TalentId,
        creatures::{BuildAdvancement, Content, PlannedAdvancement, RestorationPolicy},
        progression::HdLedger,
        rules::Ruleset,
    };

    let base = fixture::build(fixture::species());
    let mut document = base.ruleset().document().clone();
    let talent = TalentId::new("fixture_health_recovery").unwrap();
    document.talents.push(fixture::talent(
        "fixture_health_recovery",
        vec![modifier(Stat::Health, 2)],
    ));
    document
        .talents
        .iter_mut()
        .find(|definition| definition.id == talent)
        .unwrap()
        .modifiers
        .push(modifier(Stat::HealthRecoveryPeriodPercent, -25));
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        base.content().catalog().clone(),
    )
    .unwrap();
    let empty = CreatureBuild::new(
        content,
        base.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap(),
        base.binding(),
        *base.starting_skills(),
    )
    .unwrap();
    let mut intended = empty.clone();
    intended
        .select_talent(tor_simulation::HdIndex(1), talent.clone())
        .unwrap();
    let restoration = RestorationPolicy::Authored(
        intended
            .ledger()
            .entries()
            .iter()
            .zip(intended.choices())
            .map(|(die, choices)| PlannedAdvancement {
                source: die.source(),
                choices: choices.clone(),
            })
            .collect(),
    );
    // Current purchases may differ from the immutable authored restoration plan.
    let build = CreatureBuild::from_recorded(
        empty.content().clone(),
        empty.species_id().clone(),
        empty.initial_attributes(),
        empty.ledger().clone(),
        empty.binding(),
        BuildAdvancement {
            restoration,
            ..empty.advancement().clone()
        },
        vec![],
    )
    .unwrap();

    for nutrition in [
        RecoveryNutrition::Normal,
        RecoveryNutrition::Slowed { multiplier: 3 },
        RecoveryNutrition::Stopped,
    ] {
        let mut game = game(build.clone());
        let actor = ActorId(1);
        fixture::injure(&mut game, actor, 3).unwrap();
        rest(&mut game, 137);
        game.set_recovery_nutrition(actor, nutrition).unwrap();
        let multiplier = if matches!(nutrition, RecoveryNutrition::Slowed { .. }) {
            3_u64
        } else {
            1
        };
        assert_eq!(
            game.creature(actor).unwrap().health_recovery().elapsed(),
            137 * multiplier
        );

        for (edit, base_period, hit_dice) in [
            ("purchase", 870_u64, 2),
            ("drain", 1200, 1),
            ("restore", 870, 2),
        ] {
            rest(&mut game, 29);
            let projected = game.creature(actor).unwrap();
            if nutrition != RecoveryNutrition::Stopped {
                assert!(
                    matches!(&projected, std::borrow::Cow::Owned(_)),
                    "{edit}: exercise uncommitted active-time work"
                );
            }
            let before = projected.into_owned();
            let progress = before.health_recovery();
            assert!(progress.elapsed() > 0);
            let mut next = before.build().clone();
            match edit {
                "purchase" => next
                    .select_talent(tor_simulation::HdIndex(1), talent.clone())
                    .unwrap(),
                "drain" => {
                    next.remove_latest().unwrap();
                }
                "restore" => next.restore_hit_die(42).unwrap(),
                _ => unreachable!(),
            }
            game.rebuild_creature(actor, next).unwrap();
            let after = game.creature(actor).unwrap();
            let period = base_period * multiplier;
            assert_eq!(after.health_recovery().period(), period, "{edit}");
            assert_eq!(
                after.health_recovery().elapsed(),
                (u128::from(progress.elapsed()) * u128::from(period)
                    / u128::from(progress.period())) as u64,
                "{edit}: settle the old clock before flooring its fraction"
            );
            assert_eq!(after.health_recovery().nutrition(), nutrition, "{edit}");
            assert_eq!(after.health().injury(), 3, "{edit}");
            assert!(!after.health().dead(), "{edit}");
            assert_eq!(after.costs(), before.costs(), "{edit}");
            assert_eq!(after.conditions(), before.conditions(), "{edit}");
            assert_eq!(after.build().ledger().entries().len(), hit_dice, "{edit}");
            assert_eq!(after.build().starting_skills(), build.starting_skills());
            assert_eq!(
                after.build().restoration_policy(),
                build.restoration_policy()
            );
            assert_eq!(
                after.build().initial_attributes(),
                build.initial_attributes()
            );
            assert_eq!(after.build().unspent_points(), 1, "{edit}");
            assert_eq!(
                after.derived().active_talents.contains(&talent),
                edit != "drain",
                "{edit}"
            );
            assert_eq!(
                game.next_creature_event_tick(),
                (nutrition != RecoveryNutrition::Stopped)
                    .then(|| game.tick() + period - after.health_recovery().elapsed())
                    .into_iter()
                    .chain((game.tick() < 600).then_some(600))
                    .min(),
                "{edit}: earliest of rephased recovery and the unchanged received-loss deadline"
            );
            assert_eq!(after.received_health_loss_remaining(), 600 - game.tick());
            if edit == "restore" {
                assert_eq!(after.build().ledger(), intended.ledger());
                assert_eq!(after.build().choices(), intended.choices());
            }
            assert_eq!(game, round_trip(&game), "{edit}: strict mutable replay");
        }

        if nutrition == RecoveryNutrition::Stopped {
            let stopped = game.creature(actor).unwrap().health_recovery();
            rest(&mut game, 2000);
            assert_eq!(game.creature(actor).unwrap().health_recovery(), stopped);
            game.set_recovery_nutrition(actor, RecoveryNutrition::Normal)
                .unwrap();
        }
        let progress = game.creature(actor).unwrap().health_recovery();
        let remaining = progress.period() - progress.elapsed();
        let mut replay = round_trip(&game);
        for state in [&mut game, &mut replay] {
            rest(state, remaining - 1);
            assert_eq!(state.creature(actor).unwrap().health().injury(), 3);
            rest(state, 1);
            assert_eq!(state.creature(actor).unwrap().health().injury(), 2);
            assert_eq!(
                state.creature(actor).unwrap().health_recovery().elapsed(),
                0
            );
        }
        assert_eq!(
            game, replay,
            "{nutrition:?}: identical next recovery boundary"
        );
    }
}
