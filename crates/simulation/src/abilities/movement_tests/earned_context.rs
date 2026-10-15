//! Earned source recovery must use the legitimate actor-active context.
use crate::creatures::{
    AdvancementPurchase, BuildAdvancement, CreatureBuild, HdChoices, PlannedAdvancement,
    PurchaseSelection, RestorationPolicy,
};
use crate::{Action, ActorId, Game, MemoryRecords, RegionTransition};
use std::collections::{BTreeMap, BTreeSet};

fn location(region: u64, x: i32) -> tor_world::Location {
    tor_world::Location {
        region: tor_world::RegionId(region),
        position: tor_world::Position { x, y: 1, z: 0 },
    }
}
fn context_growth_game() -> Game {
    context_growth_game_with_rules(|_| {})
}
fn context_growth_game_with_rules(
    configure: impl FnOnce(&mut crate::rules::RulesetDocument),
) -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.growth.threshold_base = 25;
    document.growth.threshold_per_tier = 25;
    // Isolate context acquisition from the independently tested HD rate curve.
    document.health_recovery.base_period = 1200;
    document.health_recovery.reduction_per_hd = 0;
    document.health_recovery.minimum_period = 1200;
    let talent = crate::content_ids::TalentId::new("fixture_earned_quiet").unwrap();
    document.talents.push(crate::rules::TalentDef {
        id: talent.clone(),
        name: "Earned quiet".into(),
        prerequisites: vec![],
        requirement: crate::rules::Requirement::TotalHd {
            minimum: crate::HdCount(2),
        },
        modifiers: vec![crate::rules::Modifier {
            scope: crate::rules::Scope::WhileNoSensedHostile,
            op: crate::rules::ModifierOp::Add {
                stat: crate::rules::Stat::HealthRecoveryPeriodPercent,
                value: -25,
            },
        }],
        form_choices: vec![],
    });
    configure(&mut document);
    let mut species = crate::test_creatures::species();
    // Fatal-effect XP is the delivery path, not a quiet-clock reset experiment.
    species.modifiers.push(crate::rules::Modifier {
        scope: crate::rules::Scope::Always,
        op: crate::rules::ModifierOp::Set {
            stat: crate::rules::Stat::HealthRecoveryResetPercent,
            value: 0,
        },
    });
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let ordinary = CreatureBuild::new(
        content.clone(),
        species_id.clone(),
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let choice = HdChoices {
        purchases: vec![AdvancementPurchase {
            pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: PurchaseSelection::Talent(talent),
        }],
        ..Default::default()
    };
    let mut owner = CreatureBuild::from_recorded(
        content.clone(),
        species_id,
        ordinary.initial_attributes(),
        ordinary.ledger().clone(),
        ordinary.binding(),
        BuildAdvancement {
            starting_skills: *ordinary.starting_skills(),
            choices: vec![HdChoices::default()],
            restoration: RestorationPolicy::Authored(vec![
                PlannedAdvancement {
                    source: crate::progression::HdSource::Racial,
                    choices: HdChoices::default(),
                },
                PlannedAdvancement {
                    source: crate::progression::HdSource::Racial,
                    choices: choice,
                },
            ]),
        },
        vec![],
    )
    .unwrap();
    owner
        .set_future_growth(crate::earned_growth::FutureGrowth::Authored {})
        .unwrap();
    let world = tor_world::World::new(
        (1..=2)
            .map(|id| tor_world::Region {
                id: tor_world::RegionId(id),
                name: format!("region {id}"),
                bounds: tor_world::Extent::new(12, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, content);
    for (region, x, faction, build) in [
        (1, 1, "hero", ordinary.clone()),
        (2, 1, "owner", owner),
        (2, 9, "foe", ordinary.clone()),
        (1, 9, "victim", ordinary),
    ] {
        let id = game
            .spawn_actor(location(region, x), std::num::NonZeroU64::new(1).unwrap())
            .unwrap();
        game.configure_creature(
            id,
            crate::CreatureIdentity {
                name: faction.into(),
                faction: faction.into(),
            },
            build,
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        [ActorId(1)].into(),
        None,
        BTreeMap::from([("owner".into(), BTreeSet::from(["foe".into()]))]),
    )
    .unwrap();
    game.add_default_reference_points().unwrap();
    crate::test_creatures::injure(&mut game, ActorId(2), 3).unwrap();
    while game.tick() < 401 {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game
}
fn earned_death(game: &mut Game) {
    game.apply(crate::abilities::Outcome {
        actor: ActorId(2),
        target: ActorId(4),
        ability: None,
        intention: None,
        result: crate::abilities::OutcomeStatus::Applied,
        check: None,
        participation: crate::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![crate::abilities::ResolvedEffect::Damage { amount: 1_000_000 }],
    })
    .unwrap();
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        2
    );
}
fn phase(game: &Game) -> (u64, u64, u64) {
    let owner = game.creature(ActorId(2)).unwrap();
    (
        owner.health().injury(),
        owner.health_recovery().elapsed(),
        owner.health_recovery().period(),
    )
}
fn reopen(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let bytes = serde_json::to_vec(&(game.checkpoint(&mut shared), shared)).unwrap();
    let (checkpoint, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap()
}
#[test]
fn active_earned_first_context_uses_current_scene_without_double_rephase() {
    let mut game = context_growth_game();
    assert_eq!(phase(&game), (3, 401, 1200));
    earned_death(&mut game);
    assert_eq!(phase(&game), (3, 401, 1200));
    assert_eq!(reopen(&game), game);
}
#[test]
fn frozen_earned_first_context_retains_fraction_until_admitted_thaw() {
    for quiet in [false, true] {
        let mut game = context_growth_game();
        let mut records = MemoryRecords::default();
        game.transition_regions(
            &RegionTransition {
                active: [tor_world::RegionId(1)].into(),
                loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(phase(&game), (3, 401, 1200));
        let scenes = crate::diagnostics::work_counts().scenes;
        earned_death(&mut game);
        assert_eq!(
            crate::diagnostics::work_counts().scenes,
            scenes,
            "frozen credit must not query scene"
        );
        assert_eq!(
            phase(&game),
            (3, 401, 1200),
            "unresolved context retains the actual old denominator"
        );
        while game.tick() < 801 {
            game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        }
        if quiet {
            game.teleport(ActorId(3), location(1, 10)).unwrap();
        }
        let mut game = reopen(&game);
        assert_eq!(phase(&game), (3, 401, 1200));
        game.transition_regions(
            &RegionTransition {
                active: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
                loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(
            phase(&game),
            if quiet { (3, 300, 900) } else { (3, 401, 1200) }
        );
        assert_eq!(reopen(&game), game);
    }
}

fn freeze_owner(game: &mut Game, records: &mut MemoryRecords) {
    game.transition_regions(
        &RegionTransition {
            active: [tor_world::RegionId(1)].into(),
            loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
        },
        records,
    )
    .unwrap();
}
fn thaw_owner(game: &mut Game, records: &mut MemoryRecords) {
    game.transition_regions(
        &RegionTransition {
            active: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
            loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
        },
        records,
    )
    .unwrap();
}
fn owner_context(game: &Game) -> crate::health_recovery::RecoveryContext {
    game.creature(ActorId(2)).unwrap().recovery_context()
}
fn pending_owner() -> (Game, MemoryRecords) {
    let mut game = context_growth_game();
    let mut records = MemoryRecords::default();
    freeze_owner(&mut game, &mut records);
    earned_death(&mut game);
    assert_eq!(
        owner_context(&game),
        crate::health_recovery::RecoveryContext::Pending { base_period: 1200 }
    );
    (game, records)
}

#[test]
fn pending_recovery_preserves_raw_denominator_with_slowed_or_stopped_nutrition() {
    use crate::health_recovery::{RecoveryContext, RecoveryNutrition};
    for nutrition in [
        RecoveryNutrition::Slowed { multiplier: 3 },
        RecoveryNutrition::Stopped,
    ] {
        let mut game = context_growth_game();
        game.set_recovery_nutrition(ActorId(2), nutrition).unwrap();
        let before = game.creature(ActorId(2)).unwrap().health_recovery();
        let mut records = MemoryRecords::default();
        freeze_owner(&mut game, &mut records);
        earned_death(&mut game);
        assert_eq!(
            owner_context(&game),
            RecoveryContext::Pending { base_period: 1200 }
        );
        assert_eq!(game.creature(ActorId(2)).unwrap().health_recovery(), before);
        game = reopen(&game);
        game.set_region_light(tor_world::RegionId(2), false)
            .unwrap();
        let stamp = game.actor_clock(ActorId(2));
        assert_eq!(
            game.tick(),
            stamp,
            "this exercises the zero-delta thaw path"
        );
        thaw_owner(&mut game, &mut records);
        let clock = game.creature(ActorId(2)).unwrap().health_recovery();
        let expected_period = if matches!(nutrition, RecoveryNutrition::Slowed { .. }) {
            2700
        } else {
            900
        };
        assert_eq!(clock.period(), expected_period);
        assert_eq!(
            clock.elapsed(),
            before.elapsed() * expected_period / before.period()
        );
        assert_eq!(clock.nutrition(), nutrition);
        assert_eq!(
            owner_context(&game),
            RecoveryContext::Known {
                sensed_hostile: false
            }
        );
        assert_eq!(reopen(&game), game);
    }
}

#[test]
fn pending_recovery_survives_repeated_growth_equipment_and_condition_folds_until_final_removal() {
    use crate::health_recovery::RecoveryContext;
    use crate::rules::{
        ConditionDef, ConditionLifecycle, Eviction, Modifier, ModifierOp, Scope, Stacking, Stat,
    };
    let condition = crate::content_ids::ConditionId::new("ordinary_clock_change").unwrap();
    let contextual_condition = crate::content_ids::ConditionId::new("pending_quiet_clock").unwrap();
    let mut game = context_growth_game_with_rules(|document| {
        document.conditions.push(ConditionDef {
            id: contextual_condition.clone(),
            family: crate::content_ids::ConditionFamilyId::new("pending_quiet_clock").unwrap(),
            lifecycle: ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Pending quiet clock".into(),
            modifiers: vec![Modifier {
                scope: Scope::WhileNoSensedHostile,
                op: ModifierOp::Add {
                    stat: Stat::HealthRecoveryPeriodPercent,
                    value: -5,
                },
            }],
            stacking: Stacking::Longest,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
        document.conditions.push(ConditionDef {
            id: condition.clone(),
            family: crate::content_ids::ConditionFamilyId::new("ordinary_clock_change").unwrap(),
            lifecycle: ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: "Ordinary clock change".into(),
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::HealthRecoveryPeriod,
                    value: 23,
                },
            }],
            stacking: Stacking::Longest,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
    });
    let mut records = MemoryRecords::default();
    freeze_owner(&mut game, &mut records);
    earned_death(&mut game);
    let clock = game.creature(ActorId(2)).unwrap().health_recovery();
    let stamp = game.actors[&ActorId(2)].active_since;
    let scenes = crate::diagnostics::work_counts().scenes;
    let mut build = game.creature(ActorId(2)).unwrap().build().clone();
    build
        .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
            source: crate::earned_growth::GrowthSource::Racial {},
        })
        .unwrap();
    game.rebuild_creature(ActorId(2), build).unwrap();
    let mut awarded = game.settled_actor_outcome(ActorId(2)).unwrap();
    awarded
        .state
        .combat
        .as_mut()
        .unwrap()
        .stage_xp_award(100)
        .unwrap();
    game.apply_outcome(awarded).unwrap();
    assert!(
        game.creature(ActorId(2))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get()
            > 2
    );
    assert_eq!(game.creature(ActorId(2)).unwrap().health_recovery(), clock);

    let creature = game.creature(ActorId(2)).unwrap();
    let (socket, slot) = creature
        .derived()
        .anatomy
        .slots
        .iter()
        .enumerate()
        .find(|(_, slot)| slot.as_str() == "body_armor")
        .unwrap();
    let mut item = crate::ItemSpec::ordinary("clock mail".into());
    item.class = crate::ItemClass::Armor;
    item.equipment = Some(crate::EquipmentSpec {
        slot: slot.clone(),
        attack: None,
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::HealthRecoveryPeriod,
                value: 1000,
            },
        }],
    });
    drop(creature);
    let mut contextual_item = item.clone();
    contextual_item
        .equipment
        .as_mut()
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: Scope::WhileNoSensedHostile,
            op: ModifierOp::Add {
                stat: Stat::HealthRecoveryPeriodPercent,
                value: -5,
            },
        });
    // Alternate contextual-equipment path starts before the sole armor socket
    // is occupied; both branches exercise ordinary authored equip publication.
    let mut more_context = game.clone();
    game.place_item_stack(100, location(2, 1), Some(ActorId(2)), 1, item)
        .unwrap();
    game.equip_authored(
        ActorId(2),
        crate::ItemId(100),
        crate::EquipmentSlotId(socket as u16),
    )
    .unwrap();
    assert_eq!(game.creature(ActorId(2)).unwrap().health_recovery(), clock);
    game.apply_condition(ActorId(2), &condition, ActorId(1), 500)
        .unwrap();
    assert_eq!(game.creature(ActorId(2)).unwrap().health_recovery(), clock);
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Pending { base_period: 1200 }
    );
    assert_eq!(game.actors[&ActorId(2)].active_since, stamp);
    assert!(!game
        .actors
        .scheduled_timers()
        .any(|(_, id)| id == ActorId(2)));
    assert_eq!(crate::diagnostics::work_counts().scenes, scenes);
    assert_eq!(reopen(&game), game);

    more_context
        .apply_condition(ActorId(2), &contextual_condition, ActorId(1), 500)
        .unwrap();
    more_context
        .place_item_stack(101, location(2, 1), Some(ActorId(2)), 1, contextual_item)
        .unwrap();
    more_context
        .equip_authored(
            ActorId(2),
            crate::ItemId(101),
            crate::EquipmentSlotId(socket as u16),
        )
        .unwrap();
    assert_eq!(
        more_context.creature(ActorId(2)).unwrap().health_recovery(),
        clock
    );
    assert_eq!(
        owner_context(&more_context),
        RecoveryContext::Pending { base_period: 1200 }
    );
    assert_eq!(more_context.actors[&ActorId(2)].active_since, stamp);
    assert_eq!(crate::diagnostics::work_counts().scenes, scenes);
    assert_eq!(reopen(&more_context), more_context);

    let current = game.creature(ActorId(2)).unwrap().build().clone();
    let mut choices = current.advancement().clone();
    for choice in &mut choices.choices {
        choice.purchases.clear();
    }
    choices.restoration = RestorationPolicy::ChooseAgain;
    let ordinary = CreatureBuild::from_recorded(
        current.content().clone(),
        current.species_id().clone(),
        current.initial_attributes(),
        current.ledger().clone(),
        current.binding(),
        choices,
        current.template_ids().to_vec(),
    )
    .unwrap();
    game.rebuild_creature(ActorId(2), ordinary).unwrap();
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Known {
            sensed_hostile: false
        }
    );
    assert_eq!(
        phase(&game),
        (3, 334, 1000),
        "the last contextual source rephases once from the retained 401/1200"
    );
    assert_eq!(game.actors[&ActorId(2)].active_since, stamp);
    assert_eq!(reopen(&game), game);
}

#[test]
fn pending_recovery_is_cleared_by_death_without_perceiving_the_frozen_region() {
    use crate::health_recovery::RecoveryContext;
    let (mut game, _) = pending_owner();
    let scenes = crate::diagnostics::work_counts().scenes;
    game.apply(crate::abilities::Outcome {
        actor: ActorId(1),
        target: ActorId(2),
        ability: None,
        intention: None,
        result: crate::abilities::OutcomeStatus::Applied,
        check: None,
        participation: crate::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![crate::abilities::ResolvedEffect::Damage { amount: 1_000_000 }],
    })
    .unwrap();
    assert!(!game.alive(ActorId(2)));
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Known {
            sensed_hostile: false
        }
    );
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .elapsed(),
        0
    );
    assert_eq!(crate::diagnostics::work_counts().scenes, scenes);
    assert_eq!(reopen(&game), game);
}

#[test]
fn late_thaw_failure_preserves_pending_recovery_and_all_frozen_clocks_atomically() {
    let (mut game, mut records) = pending_owner();
    game.set_region_light(tor_world::RegionId(2), false)
        .unwrap();
    game.actors.get_mut(&ActorId(3)).unwrap().ready_at = u64::MAX;
    game.tick += 1;
    let before = game.clone();
    let records_before = records.ids().collect::<Vec<_>>();
    assert_eq!(
        game.apply_region_transition(
            &RegionTransition {
                active: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
                loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
            },
            &mut records
        ),
        Err(crate::TransitionError::Rule(
            crate::GameError::InvalidLocation
        ))
    );
    assert_eq!(
        game, before,
        "later actor deadline overflow must roll back the earlier owner's resolved context"
    );
    assert_eq!(records.ids().collect::<Vec<_>>(), records_before);
    assert_eq!(phase(&game), (3, 401, 1200));
    assert!(game.actor_frozen(ActorId(2)));
    assert!(!game
        .actors
        .scheduled_timers()
        .any(|(_, id)| id == ActorId(2)));
}

#[test]
fn pending_recovery_requires_settled_frozen_provenance_in_active_and_detached_records() {
    use crate::streaming::{RecordStore, RegionRecordData};
    let (mut game, mut records) = pending_owner();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let original = serde_json::to_value(&(snapshot, shared)).unwrap();
    for active_since in [400, 402] {
        let mut value = original.clone();
        value[1]["actors"][0]["2"]["active_since"] = serde_json::json!(active_since);
        let (snapshot, shared) =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }
    let mut active = context_growth_game();
    earned_death(&mut active);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = active.checkpoint(&mut shared);
    let mut value = serde_json::to_value(&(snapshot, shared)).unwrap();
    value[1]["actors"][0]["2"]["combat"]["creature"]["health_recovery"]["context"] =
        serde_json::json!({"kind":"pending","base_period":1200});
    let (snapshot, shared) = serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Game::restore_checkpoint(snapshot, &shared, active.content().clone()).is_err());

    game.apply_region_transition(
        &RegionTransition {
            active: [tor_world::RegionId(1)].into(),
            loaded: [tor_world::RegionId(1)].into(),
        },
        &mut records,
    )
    .unwrap();
    let id = game.detached_record(tor_world::RegionId(2)).unwrap();
    let record = records.get(id).unwrap();
    let original = serde_json::to_value(RegionRecordData::capture(&record)).unwrap();
    for active_since in [400, 402] {
        let mut value = original.clone();
        value["actors"]["2"]["active_since"] = serde_json::json!(active_since);
        let decoded: RegionRecordData =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        let record = decoded.restore(game.content().clone()).unwrap();
        let mut corrupted = MemoryRecords::default();
        corrupted.put(id, tor_world::Shared::new(record));
        let mut rejected = game.clone();
        assert_eq!(
            rejected.apply_region_transition(
                &RegionTransition {
                    active: [tor_world::RegionId(1)].into(),
                    loaded: [tor_world::RegionId(1), tor_world::RegionId(2)].into(),
                },
                &mut corrupted
            ),
            Err(crate::TransitionError::InvalidRecord(tor_world::RegionId(
                2
            )))
        );
        assert_eq!(rejected, game);
    }
    let decoded: RegionRecordData =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    let mut restored_records = MemoryRecords::default();
    restored_records.put(
        id,
        tor_world::Shared::new(decoded.restore(game.content().clone()).unwrap()),
    );
    game = reopen(&game);
    freeze_owner(&mut game, &mut restored_records);
    assert_eq!(phase(&game), (3, 401, 1200));
    assert_eq!(
        owner_context(&game),
        crate::health_recovery::RecoveryContext::Pending { base_period: 1200 }
    );
    thaw_owner(&mut game, &mut restored_records);
    assert_eq!(phase(&game), (3, 401, 1200));
    assert_eq!(reopen(&game), game);
}

#[test]
fn recovery_context_records_reject_noncanonical_shapes_and_invalid_pending_clocks() {
    use crate::creatures::record::CreatureRecord;
    use serde_json::json;
    let (game, _) = pending_owner();
    let original =
        serde_json::to_value(CreatureRecord::capture(&game.creature(ActorId(2)).unwrap())).unwrap();
    assert_eq!(
        original["health_recovery"]["context"],
        json!({"kind":"pending","base_period":1200})
    );
    for invalid in [
        json!(null),
        json!(false),
        json!(["pending", 1200]),
        json!({}),
        json!({"kind":"pending"}),
        json!({"kind":"pending","base_period":null}),
        json!({"kind":"pending","base_period":0}),
        json!({"kind":"pending","base_period":1_000_001}),
        json!({"kind":"pending","base_period":-1}),
        json!({"kind":"pending","base_period":1200,"sensed_hostile":false}),
        json!({"kind":"known"}),
        json!({"kind":"known","sensed_hostile":null}),
        json!({"kind":"known","sensed_hostile":false,"base_period":1200}),
        json!({"kind":"known","sensed_hostile":false,"extra":0}),
        json!({"kind":"unknown","base_period":1200}),
    ] {
        let mut value = original.clone();
        value["health_recovery"]["context"] = invalid;
        assert!(
            serde_json::from_slice::<CreatureRecord>(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
    let raw = serde_json::to_string(&original).unwrap();
    for (key, duplicate) in [
        (
            "\"kind\":\"pending\"",
            "\"kind\":\"pending\",\"kind\":\"pending\"",
        ),
        (
            "\"base_period\":1200",
            "\"base_period\":1200,\"base_period\":1200",
        ),
    ] {
        let corrupt = raw.replace(key, duplicate);
        assert_ne!(raw, corrupt);
        assert!(serde_json::from_slice::<CreatureRecord>(corrupt.as_bytes()).is_err());
    }
    for (dead, injury, elapsed) in [(true, 3, 0), (false, 0, 1), (false, 3, 1200)] {
        let mut value = original.clone();
        value["dead"] = json!(dead);
        value["injury"] = json!(injury);
        value["health_recovery"]["elapsed"] = json!(elapsed);
        let decoded: CreatureRecord =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(decoded.restore(game.content().clone(), vec![]).is_err());
    }
    let mut ordinary =
        serde_json::to_value(CreatureRecord::capture(&game.creature(ActorId(1)).unwrap())).unwrap();
    ordinary["health_recovery"]["context"] = json!({"kind":"pending","base_period":1200});
    let decoded: CreatureRecord =
        serde_json::from_slice(&serde_json::to_vec(&ordinary).unwrap()).unwrap();
    assert!(decoded.restore(game.content().clone(), vec![]).is_err());
}

#[test]
fn earned_growth_without_new_context_preserves_the_isolated_recovery_clock() {
    let mut game = context_growth_game();
    let current = game.creature(ActorId(2)).unwrap().build().clone();
    let mut advancement = current.advancement().clone();
    advancement.restoration = RestorationPolicy::ChooseAgain;
    let mut ordinary = CreatureBuild::from_recorded(
        current.content().clone(),
        current.species_id().clone(),
        current.initial_attributes(),
        current.ledger().clone(),
        current.binding(),
        advancement,
        current.template_ids().to_vec(),
    )
    .unwrap();
    ordinary
        .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
            source: crate::earned_growth::GrowthSource::Racial {},
        })
        .unwrap();
    game.rebuild_creature(ActorId(2), ordinary).unwrap();
    assert_eq!(phase(&game), (3, 401, 1200));
    earned_death(&mut game);
    assert!(!game
        .creature(ActorId(2))
        .unwrap()
        .derived()
        .contextual_health_recovery());
    assert_eq!(phase(&game), (3, 401, 1200));
    assert_eq!(reopen(&game), game);
}

#[test]
fn required_nutrition_preserves_pending_earned_context_until_admitted_thaw() {
    use crate::health_recovery::RecoveryContext;
    use crate::nutrition::{Nutrition, NutritionPolicy, NutritionRequirement};
    let mut game = context_growth_game_with_rules(|document| {
        document.nutrition = NutritionPolicy::Enabled {
            initial: 8,
            maximum: 12,
            hungry_below: 7,
            weak_below: 3,
            starve_at: -3,
            unit_ticks: 100,
            hungry_recovery: crate::health_recovery::RecoveryNutrition::Slowed { multiplier: 2 },
            weak_recovery: crate::health_recovery::RecoveryNutrition::Stopped,
            empty_recovery: crate::health_recovery::RecoveryNutrition::Stopped,
            regeneration_cost: crate::nutrition::RegenerationCost {
                denominator: 1,
                health: 0,
                stamina: 0,
                focus: 0,
                mana: 0,
            },
        };
    });
    game.configure_nutrition(ActorId(2), NutritionRequirement::Required)
        .unwrap();
    let mut records = MemoryRecords::default();
    freeze_owner(&mut game, &mut records);
    earned_death(&mut game);
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Pending { base_period: 1200 }
    );
    let before = game.creature(ActorId(2)).unwrap().into_owned();
    while game.tick() < 1001 {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game = reopen(&game);
    assert_eq!(*game.creature(ActorId(2)).unwrap(), before);
    assert_eq!(
        before.nutrition(),
        Nutrition::Required {
            reserve: 8,
            elapsed: 0,
            regeneration_remainder: 0
        }
    );
    game.set_region_light(tor_world::RegionId(2), false)
        .unwrap();
    thaw_owner(&mut game, &mut records);
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Known {
            sensed_hostile: false
        }
    );
    assert_eq!(
        game.creature(ActorId(2)).unwrap().nutrition(),
        before.nutrition()
    );
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .elapsed(),
        before.health_recovery().elapsed() * 900 / 1200
    );
    let thawed = game.creature(ActorId(2)).unwrap().into_owned();
    let mut bulk = thawed.clone();
    let mut incremental = thawed;
    bulk.advance_active(200);
    for _ in 0..200 {
        incremental.advance_active(1);
    }
    assert_eq!(bulk, incremental);
    assert_eq!(
        bulk.nutrition(),
        Nutrition::Required {
            reserve: 6,
            elapsed: 0,
            regeneration_remainder: 0
        }
    );
    assert_eq!(bulk.health_recovery().period(), 1800);
    assert_eq!(reopen(&game), game);
}

#[test]
fn frozen_hit_die_drain_and_restoration_acquire_context_only_at_the_ordinary_thaw_boundary() {
    use crate::health_recovery::RecoveryContext;
    let mut game = context_growth_game();
    earned_death(&mut game);
    let mut records = MemoryRecords::default();
    freeze_owner(&mut game, &mut records);
    let before = phase(&game);
    let scenes = crate::diagnostics::work_counts().scenes;
    game.apply_effects(ActorId(2), &[crate::rules::Effect::DrainLatestHitDie])
        .unwrap();
    assert!(!game
        .creature(ActorId(2))
        .unwrap()
        .derived()
        .contextual_health_recovery());
    assert_eq!(
        crate::diagnostics::work_counts().scenes,
        scenes,
        "a noncontextual source fold never asks a frozen owner to perceive"
    );
    game.apply_effects(ActorId(2), &[crate::rules::Effect::RestoreHitDie])
        .unwrap();
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Pending { base_period: 1200 }
    );
    assert_eq!(crate::diagnostics::work_counts().scenes, scenes);
    assert_eq!(phase(&game), before);
    game = reopen(&game);
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Pending { base_period: 1200 }
    );
    game.set_region_light(tor_world::RegionId(2), false)
        .unwrap();
    thaw_owner(&mut game, &mut records);
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Known {
            sensed_hostile: false
        }
    );
    assert_eq!(phase(&game), (3, 300, 900));
    assert_eq!(reopen(&game), game);
}

#[test]
fn active_hit_die_restoration_uses_the_current_scene_once_without_healing_or_resetting_quiet_time()
{
    use crate::health_recovery::RecoveryContext;
    let mut game = context_growth_game();
    earned_death(&mut game);
    game.apply_effects(ActorId(2), &[crate::rules::Effect::DrainLatestHitDie])
        .unwrap();
    let quiet = game
        .creature(ActorId(2))
        .unwrap()
        .received_health_loss_remaining();
    game.set_region_light(tor_world::RegionId(2), false)
        .unwrap();
    game.apply_effects(ActorId(2), &[crate::rules::Effect::RestoreHitDie])
        .unwrap();
    assert_eq!(
        owner_context(&game),
        RecoveryContext::Known {
            sensed_hostile: false
        }
    );
    assert_eq!(phase(&game), (3, 300, 900));
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .received_health_loss_remaining(),
        quiet
    );
    assert_eq!(reopen(&game), game);
}
