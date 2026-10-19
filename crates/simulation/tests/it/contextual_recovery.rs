//! Recovery context is sampled at authoritative mutation boundaries, never by reads.
use super::creature_fixture as fixture;
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    attributes::{ManaBinding, Skill},
    creatures::{Content, CreatureBuild, ScenarioCatalog},
    progression::{HdLedger, HdSource},
    rules::{Modifier, ModifierOp, Scope, Stat},
    Action, ActorId, CreatureIdentity, Game,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}
fn context_modifier() -> Modifier {
    Modifier {
        scope: Scope::WhileNoSensedHostile,
        op: ModifierOp::Add {
            stat: Stat::HealthRecoveryPeriodPercent,
            value: -25,
        },
    }
}
fn game(hostile_present: bool) -> Game {
    game_with_context(hostile_present, true)
}
fn game_with_context(hostile_present: bool, contextual: bool) -> Game {
    use tor_simulation::{
        content_ids::{ConditionFamilyId, ConditionId, TemplateId},
        creatures::Template,
        rules::{ConditionDef, ConditionLifecycle, Eviction, Ruleset, Stacking},
    };
    let mut species = fixture::species();
    if contextual {
        species.modifiers.push(context_modifier());
    }
    let mut template = Template::new(TemplateId::new("quiet_recovery").unwrap(), 0);
    template.modifiers.push(context_modifier());
    let mut document = fixture::ruleset().document().clone();
    document.conditions.push(ConditionDef {
        id: ConditionId::new("quiet_recovery").unwrap(),
        family: ConditionFamilyId::new("quiet_recovery").unwrap(),
        lifecycle: ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: "Quiet recovery".into(),
        modifiers: vec![context_modifier()],
        stacking: Stacking::Longest,
        source_scoped: false,
        descriptors: vec![],
        capacity: Eviction::Oldest,
    });
    for (id, scope, stat, value) in [
        (
            "quiet_counterweight",
            Scope::WhileNoSensedHostile,
            Stat::HealthRecoveryPeriodPercent,
            25,
        ),
        ("borrowed_vitality", Scope::Always, Stat::Health, 100),
    ] {
        document.conditions.push(ConditionDef {
            id: ConditionId::new(id).unwrap(),
            family: ConditionFamilyId::new(id).unwrap(),
            lifecycle: ConditionLifecycle::Timed,
            timing: tor_simulation::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: tor_simulation::rules::ConditionPreparation::Preserve,
            name: id.into(),
            modifiers: vec![Modifier {
                scope,
                op: ModifierOp::Add { stat, value },
            }],
            stacking: Stacking::Longest,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Oldest,
        });
    }
    let id = species.id.clone();
    let rules = Ruleset::validate(document).unwrap();
    rules
        .validate_modifiers(&species.modifiers)
        .expect("contextual recovery vocabulary must validate before testing behavior");
    let content = Content::new(
        rules,
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), species)]),
            templates: BTreeMap::from([(template.id.clone(), template)]),
        },
    )
    .unwrap();
    let build = CreatureBuild::new(
        content.clone(),
        id,
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    let world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room {id}"),
                bounds: Extent::new(12, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, content);
    for (region, x, faction) in [(1, 1, "hero"), (2, 1, "clock"), (1, 9, "foe")] {
        if faction == "foe" && !hostile_present {
            continue;
        }
        let actor = game
            .spawn_actor(at(region, x), NonZeroU64::new(1).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: faction.into(),
                faction: faction.into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("hero".into(), BTreeSet::from(["foe".into()]))]),
    )
    .unwrap();
    game.add_default_reference_points().unwrap();
    game
}
fn rest(game: &mut Game, ticks: u64) {
    let until = game.tick() + ticks;
    while game.tick() < until {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
}
fn recovery(game: &Game) -> (u64, u64, u64) {
    let creature = game.creature(ActorId(1)).unwrap();
    (
        creature.health().injury(),
        creature.health_recovery().elapsed(),
        creature.health_recovery().period(),
    )
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(checkpoint, shared)).unwrap();
    let (checkpoint, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap()
}

#[test]
fn recovery_bonus_depends_on_perceived_hostility_rather_than_hidden_population() {
    let mut empty = game(false);
    let mut hidden = game(true);
    for game in [&mut empty, &mut hidden] {
        game.set_region_light(RegionId(1), false).unwrap();
        fixture::injure(game, ActorId(1), 3).unwrap();
        rest(game, 899);
        assert_eq!(recovery(game), (3, 899, 900));
        rest(game, 1);
        assert_eq!(recovery(game), (2, 0, 900));
    }
    assert_eq!(recovery(&empty), recovery(&hidden));
}

#[test]
fn perceived_hostile_disables_the_rate_bonus_until_the_visibility_boundary_changes() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 400);
    assert_eq!(recovery(&game), (3, 400, 1200));
    game.set_region_light(RegionId(1), false).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
    rest(&mut game, 599);
    assert_eq!(recovery(&game), (3, 899, 900));
    rest(&mut game, 1);
    assert_eq!(recovery(&game), (2, 0, 900));
}

#[test]
fn lighting_flips_preserve_fraction_and_reads_never_refresh_or_grant_time() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    game.set_region_light(RegionId(1), false).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
    game.set_region_light(RegionId(1), true).unwrap();
    assert_eq!(recovery(&game), (3, 400, 1200));
    let before = game.clone();
    for _ in 0..10 {
        game.creature(ActorId(1)).unwrap();
        game.inspect_creature(ActorId(1)).unwrap();
        game.observe(ActorId(1)).unwrap();
        assert_eq!(recovery(&game), (3, 400, 1200));
    }
    assert_eq!(game, before);
}

#[test]
fn moving_a_visible_hostile_away_rephases_without_bonus_recovery() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 400);
    game.teleport(ActorId(3), at(2, 9)).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
    game.teleport(ActorId(3), at(1, 9)).unwrap();
    assert_eq!(recovery(&game), (3, 400, 1200));
}

#[test]
fn hostility_policy_changes_rephase_only_the_affected_context() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 400);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
}

#[test]
fn checkpoint_continuation_preserves_the_context_denominator_and_fraction() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    game.set_region_light(RegionId(1), false).unwrap();
    let mut replay = round_trip(&game);
    assert_eq!(recovery(&replay), (3, 300, 900));
    for state in [&mut game, &mut replay] {
        rest(state, 100);
        state.set_region_light(RegionId(1), true).unwrap();
        assert_eq!(recovery(state), (3, 533, 1200));
        rest(state, 667);
        assert_eq!(recovery(state), (2, 0, 1200));
    }
    assert_eq!(game, replay);
}

#[test]
fn newly_spawned_or_dead_perceived_hostiles_update_context_without_a_read_trigger() {
    let mut game = game(false);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 300);
    assert_eq!(recovery(&game), (3, 300, 900));
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    let foe = game
        .spawn_actor(at(1, 9), NonZeroU64::new(1).unwrap())
        .unwrap();
    game.configure_creature(
        foe,
        CreatureIdentity {
            name: "foe".into(),
            faction: "foe".into(),
        },
        build,
    )
    .unwrap();
    assert_eq!(recovery(&game), (3, 400, 1200));
    fixture::injure(&mut game, foe, u32::MAX).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
}

#[test]
fn frozen_recovery_context_reconciles_on_thaw_without_earning_hidden_elapsed_time() {
    use tor_simulation::{MemoryRecords, RegionTransition};
    let mut game = game(true);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("clock".into(), BTreeSet::from(["foe".into()]))]),
    )
    .unwrap();
    game.teleport(ActorId(3), at(2, 9)).unwrap();
    fixture::injure(&mut game, ActorId(2), 3).unwrap();
    rest(&mut game, 400);
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    rest(&mut game, 2400);
    game.teleport(ActorId(3), at(1, 9)).unwrap();
    let mut game = round_trip(&game);
    let frozen = game.creature(ActorId(2)).unwrap();
    assert_eq!(
        (
            frozen.health().injury(),
            frozen.health_recovery().elapsed(),
            frozen.health_recovery().period()
        ),
        (3, 400, 1200)
    );
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    let thawed = game.creature(ActorId(2)).unwrap();
    assert_eq!(
        (
            thawed.health().injury(),
            thawed.health_recovery().elapsed(),
            thawed.health_recovery().period()
        ),
        (3, 300, 900)
    );
}

#[test]
fn active_checkpoint_rejects_forged_context_even_when_both_denominators_fit_elapsed() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 400);
    game.set_recovery_nutrition(
        ActorId(1),
        tor_simulation::health_recovery::RecoveryNutrition::Normal,
    )
    .unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut value = serde_json::to_value(&(checkpoint, shared)).unwrap();
    let record = &mut value[1]["actors"][0]["1"]["combat"]["creature"];
    assert_eq!(record["health_recovery"]["context"]["sensed_hostile"], true);
    assert_eq!(record["health_recovery"]["elapsed"], 400);
    record["health_recovery"]["context"]["sensed_hostile"] = false.into();
    let (checkpoint, shared) =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).is_err());
    assert_eq!(
        round_trip(&game),
        game,
        "unchanged active context is a valid control"
    );
}

#[test]
fn recovery_context_record_uses_required_bounded_named_fields_on_the_byte_decoder() {
    use tor_simulation::creatures::record::CreatureRecord;
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 400);
    let original =
        serde_json::to_value(CreatureRecord::capture(&game.creature(ActorId(1)).unwrap())).unwrap();
    let restored: CreatureRecord =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert!(restored.restore(game.content().clone(), vec![]).is_ok());
    for invalid in [
        serde_json::json!({"elapsed":400,"nutrition":{"kind":"normal"}}),
        serde_json::json!({"elapsed":400,"nutrition":{"kind":"normal"},"context":null}),
        serde_json::json!({"elapsed":400,"nutrition":{"kind":"normal"},"context":{"kind":"known","sensed_hostile":true},"extra":0}),
        serde_json::json!([400,{"kind":"normal"},true]),
    ] {
        let mut value = original.clone();
        value["health_recovery"] = invalid;
        assert!(
            serde_json::from_slice::<CreatureRecord>(&serde_json::to_vec(&value).unwrap()).is_err()
        );
    }
    let raw = serde_json::to_string(&original).unwrap();
    let duplicate = raw.replace(
        "\"sensed_hostile\":true",
        "\"sensed_hostile\":true,\"sensed_hostile\":false",
    );
    assert_ne!(raw, duplicate);
    assert!(serde_json::from_slice::<CreatureRecord>(duplicate.as_bytes()).is_err());
    for (sensed, elapsed) in [(false, 900), (true, 1200)] {
        let mut value = original.clone();
        value["health_recovery"]["context"]["sensed_hostile"] = sensed.into();
        value["health_recovery"]["elapsed"] = elapsed.into();
        let decoded: CreatureRecord =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(decoded.restore(game.content().clone(), vec![]).is_err());
    }
}

#[test]
fn first_contextual_build_source_rephases_only_once_on_acquisition_and_removal() {
    use tor_simulation::content_ids::TemplateId;
    let mut game = game_with_context(true, false);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    assert_eq!(recovery(&game), (3, 401, 1200));
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .set_templates(vec![TemplateId::new("quiet_recovery").unwrap()])
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert_eq!(
        recovery(&game),
        (3, 401, 1200),
        "acquisition cannot round through an inapplicable quiet rate"
    );
    game.set_region_light(RegionId(1), false).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    assert_eq!(recovery(&game), (3, 400, 1200));
    assert_eq!(round_trip(&game), game);

    // Removing the last watched owner disables witness capture. Divergent
    // branches must still invalidate correctly when an owner is acquired again.
    let mut quiet_branch = round_trip(&game);
    game.set_region_light(RegionId(1), true).unwrap();
    for (state, expected) in [
        (&mut game, (3, 400, 1200)),
        (&mut quiet_branch, (3, 300, 900)),
    ] {
        let plain = state.creature(ActorId(1)).unwrap().build().clone();
        let mut contextual = plain.clone();
        contextual
            .set_templates(vec![TemplateId::new("quiet_recovery").unwrap()])
            .unwrap();
        state.rebuild_creature(ActorId(1), contextual).unwrap();
        assert_eq!(recovery(state), expected);
        assert_eq!(round_trip(state), *state);
        state.rebuild_creature(ActorId(1), plain).unwrap();
        assert_eq!(recovery(state), (3, 400, 1200));
        let before = tor_simulation::diagnostics::work_counts();
        rest(state, 5);
        let after = tor_simulation::diagnostics::work_counts();
        assert_eq!(
            after.perception_witness_captures - before.perception_witness_captures,
            0
        );
    }
}

#[test]
fn first_contextual_condition_source_does_not_round_through_an_inapplicable_rate() {
    let mut game = game_with_context(true, false);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    game.apply_condition(
        ActorId(1),
        &tor_simulation::content_ids::ConditionId::new("quiet_recovery").unwrap(),
        ActorId(1),
        500,
    )
    .unwrap();
    assert_eq!(recovery(&game), (3, 401, 1200));
    assert_eq!(round_trip(&game), game);
    game.set_region_light(RegionId(1), false).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
}

#[test]
fn first_contextual_equipment_source_does_not_round_through_an_inapplicable_rate() {
    use tor_simulation::{EquipmentSlotId, EquipmentSpec, ItemClass, ItemId, ItemSpec};
    let mut game = game_with_context(true, false);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    let creature = game.creature(ActorId(1)).unwrap();
    let (index, slot) = creature
        .derived()
        .anatomy
        .slots
        .iter()
        .enumerate()
        .find(|(_, slot)| slot.as_str() == "body_armor")
        .unwrap();
    let mut armor = ItemSpec::ordinary("quiet mail".into());
    armor.class = ItemClass::Armor;
    armor.equipment = Some(EquipmentSpec {
        armor: None,
        slot: slot.clone(),
        attack: None,
        modifiers: vec![context_modifier()],
    });
    game.place_item_stack(10, at(1, 1), Some(ActorId(1)), 1, armor)
        .unwrap();
    game.equip_authored(ActorId(1), ItemId(10), EquipmentSlotId(index as u16))
        .unwrap();
    assert_eq!(recovery(&game), (3, 401, 1200));
    assert_eq!(round_trip(&game), game);
    game.set_region_light(RegionId(1), false).unwrap();
    assert_eq!(recovery(&game), (3, 300, 900));
}

#[test]
fn unrelated_actors_do_not_add_observation_work_and_stationary_context_reuses_scenes() {
    use tor_simulation::diagnostics::work_counts;
    for contextual in [false, true] {
        let mut game = game_with_context(true, contextual);
        // This boundary primes only contextual owners; ordinary actors need no scene.
        game.set_region_light(RegionId(1), true).unwrap();
        let before = work_counts();
        rest(&mut game, 20);
        let after = work_counts();
        assert_eq!(after.observations - before.observations, 0);
        assert_eq!(after.scenes - before.scenes, 0);
        assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 0);
        if !contextual {
            assert_eq!(after.scene_cells_copied - before.scene_cells_copied, 0);
            assert_eq!(
                after.perception_witness_captures - before.perception_witness_captures,
                0
            );
        } else {
            assert!(
                after.perception_witness_captures > before.perception_witness_captures,
                "contextual games retain explicit mutation-witness work even when scenes are reused"
            );
        }
    }
}

#[test]
fn ordinary_resolved_condition_payload_preserves_first_contextual_source_fraction() {
    let mut game = game_with_context(true, false);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    game.apply_effects(
        ActorId(1),
        &[tor_simulation::rules::Effect::ApplyCondition {
            condition: tor_simulation::content_ids::ConditionId::new("quiet_recovery").unwrap(),
            duration: 500,
        }],
    )
    .unwrap();
    assert_eq!(recovery(&game), (3, 401, 1200));
    assert_eq!(round_trip(&game), game);
}

#[test]
fn cancelling_scoped_contributions_keep_context_until_their_expiry_exposes_a_rate() {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    game.apply_condition(
        ActorId(1),
        &tor_simulation::content_ids::ConditionId::new("quiet_counterweight").unwrap(),
        ActorId(1),
        100,
    )
    .unwrap();
    assert_eq!(recovery(&game), (3, 401, 1200));
    let mut replay = round_trip(&game);
    for state in [&mut game, &mut replay] {
        rest(state, 100);
        assert_eq!(
            recovery(state),
            (3, 501, 1200),
            "a temporary cancellation cannot discard the sensed-hostile clock context"
        );
        state.set_region_light(RegionId(1), false).unwrap();
        assert_eq!(recovery(state), (3, 375, 900));
    }
    assert_eq!(game, replay);
}

#[test]
fn simultaneous_expiry_deaths_rephase_the_survivor_once_without_full_observations() {
    use tor_simulation::{content_ids::ConditionId, diagnostics::work_counts};
    let mut game = game(true);
    let build = game.creature(ActorId(3)).unwrap().build().clone();
    let another = game
        .spawn_actor(at(1, 8), NonZeroU64::new(1).unwrap())
        .unwrap();
    game.configure_creature(
        another,
        CreatureIdentity {
            name: "foe two".into(),
            faction: "foe".into(),
        },
        build,
    )
    .unwrap();
    for foe in [ActorId(3), another] {
        let base_health = game.creature(foe).unwrap().health().maximum();
        game.apply_condition(
            foe,
            &ConditionId::new("borrowed_vitality").unwrap(),
            ActorId(1),
            401,
        )
        .unwrap();
        fixture::injure(&mut game, foe, u32::try_from(base_health + 1).unwrap()).unwrap();
        assert!(!game.creature(foe).unwrap().health().dead());
    }
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 400);
    assert_eq!(recovery(&game), (3, 400, 1200));
    let before = work_counts();
    rest(&mut game, 1);
    let after = work_counts();
    for foe in [ActorId(3), another] {
        assert!(game.creature(foe).unwrap().health().dead());
    }
    assert_eq!(recovery(&game), (3, 300, 900));
    assert_eq!(after.observations - before.observations, 0);
    assert_eq!(after.scenes - before.scenes, 0);
    assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 0);
    rest(&mut game, 600);
    assert_eq!(recovery(&game), (2, 0, 900));
    assert_eq!(round_trip(&game), game);
}

fn freeze_clock_actor(contextual: bool) -> (Game, tor_simulation::MemoryRecords) {
    let mut game = game_with_context(true, contextual);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::from([("clock".into(), BTreeSet::from(["foe".into()]))]),
    )
    .unwrap();
    game.teleport(ActorId(3), at(2, 9)).unwrap();
    fixture::injure(&mut game, ActorId(2), 3).unwrap();
    rest(&mut game, 401);
    let mut records = tor_simulation::MemoryRecords::default();
    game.transition_regions(
        &tor_simulation::RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    game.teleport(ActorId(3), at(1, 9)).unwrap();
    rest(&mut game, 20);
    (game, records)
}
fn thaw_clock_actor(game: &mut Game, records: &mut tor_simulation::MemoryRecords) {
    game.transition_regions(
        &tor_simulation::RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        records,
    )
    .unwrap();
}
fn clock_recovery(game: &Game) -> (u64, u64, u64) {
    let creature = game.creature(ActorId(2)).unwrap();
    (
        creature.health().injury(),
        creature.health_recovery().elapsed(),
        creature.health_recovery().period(),
    )
}

#[test]
fn contextual_recovery_followup_frozen_contextual_source_change_keeps_captured_perception_until_one_thaw_transition(
) {
    let (mut game, mut records) = freeze_clock_actor(true);
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    game.apply_condition(
        ActorId(2),
        &tor_simulation::content_ids::ConditionId::new("quiet_recovery").unwrap(),
        ActorId(1),
        500,
    )
    .unwrap();
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    let mut restored = round_trip(&game);
    assert_eq!(restored, game);
    thaw_clock_actor(&mut game, &mut records);
    thaw_clock_actor(&mut restored, &mut records);
    assert_eq!(clock_recovery(&game), (3, 200, 600));
    assert_eq!(restored, game);
}

#[test]
fn contextual_recovery_followup_frozen_first_contextual_condition_rejects_atomically_then_succeeds_after_thaw(
) {
    let (mut game, mut records) = freeze_clock_actor(false);
    let before = game.clone();
    let condition = tor_simulation::content_ids::ConditionId::new("quiet_recovery").unwrap();
    assert_eq!(
        game.apply_condition(ActorId(2), &condition, ActorId(1), 500),
        Err(tor_simulation::GameError::ActorBusy)
    );
    assert_eq!(game, before);
    thaw_clock_actor(&mut game, &mut records);
    game.apply_condition(ActorId(2), &condition, ActorId(1), 500)
        .unwrap();
    assert_eq!(clock_recovery(&game), (3, 300, 900));
}

#[test]
fn contextual_recovery_followup_frozen_first_contextual_build_rejects_atomically_then_succeeds_after_thaw(
) {
    let (mut game, mut records) = freeze_clock_actor(false);
    let mut build = game.creature(ActorId(2)).unwrap().build().clone();
    build
        .set_templates(vec![tor_simulation::content_ids::TemplateId::new(
            "quiet_recovery",
        )
        .unwrap()])
        .unwrap();
    let before = game.clone();
    assert_eq!(
        game.rebuild_creature(ActorId(2), build.clone()),
        Err(tor_simulation::GameError::ActorBusy)
    );
    assert_eq!(game, before);
    thaw_clock_actor(&mut game, &mut records);
    game.rebuild_creature(ActorId(2), build).unwrap();
    assert_eq!(clock_recovery(&game), (3, 300, 900));
}

#[test]
fn contextual_recovery_followup_frozen_first_contextual_equipment_rejects_atomically_then_succeeds_after_thaw(
) {
    use tor_simulation::{EquipmentSlotId, EquipmentSpec, ItemClass, ItemId, ItemSpec};
    let (mut game, mut records) = freeze_clock_actor(false);
    let creature = game.creature(ActorId(2)).unwrap();
    let (index, slot) = creature
        .derived()
        .anatomy
        .slots
        .iter()
        .enumerate()
        .find(|(_, slot)| slot.as_str() == "body_armor")
        .unwrap();
    let mut armor = ItemSpec::ordinary("quiet mail".into());
    armor.class = ItemClass::Armor;
    armor.equipment = Some(EquipmentSpec {
        armor: None,
        slot: slot.clone(),
        attack: None,
        modifiers: vec![context_modifier()],
    });
    game.place_item_stack(10, at(2, 1), Some(ActorId(2)), 1, armor)
        .unwrap();
    let before = game.clone();
    assert_eq!(
        game.equip_authored(ActorId(2), ItemId(10), EquipmentSlotId(index as u16)),
        Err(tor_simulation::GameError::ActorBusy)
    );
    assert_eq!(game, before);
    thaw_clock_actor(&mut game, &mut records);
    game.equip_authored(ActorId(2), ItemId(10), EquipmentSlotId(index as u16))
        .unwrap();
    assert_eq!(clock_recovery(&game), (3, 300, 900));
}

#[test]
fn contextual_recovery_followup_frozen_ordinary_source_change_remains_available_without_acquiring_context(
) {
    let (mut game, _) = freeze_clock_actor(false);
    game.apply_condition(
        ActorId(2),
        &tor_simulation::content_ids::ConditionId::new("borrowed_vitality").unwrap(),
        ActorId(1),
        500,
    )
    .unwrap();
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    assert!(!game
        .creature(ActorId(2))
        .unwrap()
        .derived()
        .contextual_health_recovery());
    assert_eq!(round_trip(&game), game);
}

#[test]
fn frozen_noncontextual_loss_preserves_recovery_history_across_restart_and_thaw() {
    use tor_simulation::rules::{AmountExpr, Effect};
    let (mut game, mut records) = freeze_clock_actor(false);
    let actor = ActorId(2);
    let health = game.creature(actor).unwrap().health();
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    assert!(game
        .apply_effects(
            actor,
            &[Effect::LoseMaximumHealth {
                amount: AmountExpr::Fixed { amount: 1 },
            }],
        )
        .unwrap());
    let changed = game.creature(actor).unwrap();
    assert!(!changed.health().dead());
    assert_eq!(changed.health().maximum(), health.maximum() - 1);
    assert_eq!(changed.restorable_losses().maximum_health, 1);
    assert!(!changed.derived().contextual_health_recovery());
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    let mut restored = round_trip(&game);
    assert_eq!(restored, game);
    thaw_clock_actor(&mut game, &mut records);
    thaw_clock_actor(&mut restored, &mut records);
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .restorable_losses()
            .maximum_health,
        1
    );
    assert_eq!(restored, game);
}

#[test]
fn frozen_noncontextual_zero_deficit_restoration_preserves_creature_state() {
    use tor_simulation::{
        attributes::Attribute,
        rules::{AmountExpr, Effect},
    };
    let (mut game, _) = freeze_clock_actor(false);
    let actor = ActorId(2);
    let before = game.creature(actor).unwrap().into_owned();
    assert!(!game
        .apply_effects(
            actor,
            &[
                Effect::RestoreAttribute {
                    attribute: Attribute::Strength,
                    amount: AmountExpr::Fixed { amount: 1 },
                },
                Effect::RestoreMaximumHealth {
                    amount: AmountExpr::Fixed { amount: 1 },
                },
            ],
        )
        .unwrap());
    assert_eq!(game.creature(actor).unwrap().as_ref(), &before);
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    assert_eq!(round_trip(&game), game);
}

#[test]
fn frozen_contextual_loss_keeps_captured_perception_until_one_thaw_transition() {
    use tor_simulation::rules::{AmountExpr, Effect};
    let (mut game, mut records) = freeze_clock_actor(true);
    let actor = ActorId(2);
    let maximum = game.creature(actor).unwrap().health().maximum();
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    assert!(game
        .apply_effects(
            actor,
            &[Effect::LoseMaximumHealth {
                amount: AmountExpr::Fixed { amount: 1 },
            }],
        )
        .unwrap());
    assert_eq!(
        game.creature(actor).unwrap().health().maximum(),
        maximum - 1
    );
    assert_eq!(clock_recovery(&game), (3, 401, 1200));
    let mut restored = round_trip(&game);
    assert_eq!(restored, game);
    thaw_clock_actor(&mut game, &mut records);
    thaw_clock_actor(&mut restored, &mut records);
    assert_eq!(clock_recovery(&game), (3, 300, 900));
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .restorable_losses()
            .maximum_health,
        1
    );
    assert_eq!(restored, game);
}

#[test]
fn contextual_recovery_followup_initial_contextual_construction_in_a_frozen_region_has_no_elapsed_history_to_fabricate(
) {
    let (mut game, mut records) = freeze_clock_actor(false);
    let mut build = game.creature(ActorId(2)).unwrap().build().clone();
    build
        .set_templates(vec![tor_simulation::content_ids::TemplateId::new(
            "quiet_recovery",
        )
        .unwrap()])
        .unwrap();
    let newborn = game
        .spawn_actor(at(2, 5), NonZeroU64::new(1).unwrap())
        .unwrap();
    game.configure_creature(
        newborn,
        CreatureIdentity {
            name: "newborn".into(),
            faction: "clock".into(),
        },
        build,
    )
    .unwrap();
    assert_eq!(
        game.creature(newborn).unwrap().health_recovery().elapsed(),
        0
    );
    assert_eq!(round_trip(&game), game);
    thaw_clock_actor(&mut game, &mut records);
    assert_eq!(
        game.creature(newborn).unwrap().health_recovery().elapsed(),
        0
    );
    assert_eq!(
        game.creature(newborn).unwrap().health_recovery().period(),
        900
    );
}

#[test]
fn contextual_recovery_followup_authored_body_change_reconciles_perception_without_waiting_for_an_action(
) {
    let mut game = game(true);
    game.set_region_light(RegionId(1), false).unwrap();
    game.set_cell_light(at(1, 8), true).unwrap();
    assert!(!game
        .observe(ActorId(1))
        .unwrap()
        .visible_actors
        .iter()
        .any(|actor| actor.id == ActorId(3)));
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    assert_eq!(recovery(&game), (3, 401, 900));
    game.set_body(
        ActorId(3),
        tor_simulation::BodySpec {
            cells: vec![[0, 0, 0], [-1, 0, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    assert!(
        game.observe(ActorId(1))
            .unwrap()
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(3)),
        "fixture must expose a lit hostile body cell"
    );
    assert_eq!(recovery(&game), (3, 534, 1200));
    game.set_body(ActorId(3), tor_simulation::BodySpec::default())
        .unwrap();
    assert_eq!(recovery(&game), (3, 400, 900));
}

#[test]
fn contextual_recovery_followup_authored_closed_door_reconciles_perception_and_preserves_old_fraction(
) {
    let mut game = game(true);
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    assert_eq!(recovery(&game), (3, 401, 1200));
    game.place_door(at(1, 5), false, 1).unwrap();
    assert!(
        !game
            .observe(ActorId(1))
            .unwrap()
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(3)),
        "fixture door must hide the hostile"
    );
    assert_eq!(recovery(&game), (3, 300, 900));
    assert_eq!(round_trip(&game), game);
}

#[test]
fn contextual_recovery_followup_authored_portal_reconciles_newly_perceived_hostility_at_the_geometry_boundary(
) {
    let mut game = game(true);
    game.teleport(ActorId(3), at(2, 3)).unwrap();
    game.teleport(ActorId(1), at(1, 9)).unwrap();
    fixture::injure(&mut game, ActorId(1), 3).unwrap();
    rest(&mut game, 401);
    assert_eq!(recovery(&game), (3, 401, 900));
    game.connect_portal_area(
        tor_world::Passage {
            from: at(1, 11),
            direction: tor_world::Direction::East,
            to: at(2, 0),
        },
        0,
        1,
        1,
    )
    .unwrap();
    assert!(
        game.observe(ActorId(1))
            .unwrap()
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(3)),
        "fixture portal must expose the hostile"
    );
    assert_eq!(recovery(&game), (3, 534, 1200));
    assert_eq!(round_trip(&game), game);
}
