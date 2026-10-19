//! Small authored numbers exercise shared boundaries, not production survival balance.
//! JSON setup uses current public APIs so the first red is behavior/schema, not
//! an import of implementation symbols which do not exist yet.
use super::creature_fixture as fixture;
use serde_json::{json, Value};
use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::{
    checkpoint::SharedState,
    creatures::{record::CreatureRecord, Content, CreatureBuild, CreatureState},
    health_recovery::RecoveryNutrition,
    rules::{Modifier, ModifierOp, Ruleset, RulesetDocument, Scope, Stat},
    Action, ActorId, CreatureIdentity, Game, IntentionOrigin, ItemId, ItemSpec, MemoryRecords,
    RegionTransition,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn policy() -> Value {
    json!({
        "kind":"enabled",
        "initial":8, "maximum":12, "hungry_below":7, "weak_below":3,
        "starve_at":-3, "unit_ticks":100,
        "hungry_recovery":{"kind":"slowed", "multiplier":2},
        "weak_recovery":{"kind":"stopped"},
        "regeneration_cost":{"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0},
        "empty_recovery":{"kind":"stopped"}
    })
}

fn build() -> CreatureBuild {
    build_with_policy(policy())
}

fn build_with_policy(policy: Value) -> CreatureBuild {
    let mut species = fixture::species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::HealthRecoveryPeriod,
            value: 101,
        },
    });
    let original = fixture::build(species);
    let mut document = serde_json::to_value(original.ruleset().document()).unwrap();
    document["nutrition"] = policy;
    let rules = Ruleset::validate(
        serde_json::from_value(document).expect("rules accept explicit nutrition policy"),
    )
    .unwrap();
    let content = Content::new(rules, original.content().catalog().clone()).unwrap();
    CreatureBuild::new(
        content,
        original.species_id().clone(),
        original.ledger().clone(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap()
}

fn required(reserve: i32, elapsed: u64) -> Value {
    json!({"kind":"required", "reserve":reserve, "elapsed":elapsed, "regeneration_remainder":0})
}

fn recovery(reserve: i32) -> Value {
    if reserve < 3 {
        json!({"kind":"stopped"})
    } else if reserve < 7 {
        json!({"kind":"slowed", "multiplier":2})
    } else {
        json!({"kind":"normal"})
    }
}

fn required_creature(reserve: i32, elapsed: u64) -> CreatureState {
    let creature = CreatureState::new(build()).unwrap();
    let mut record = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    record["nutrition"] = required(reserve, elapsed);
    record["health_recovery"]["nutrition"] = recovery(reserve);
    serde_json::from_value::<CreatureRecord>(record)
        .expect("explicit required nutrition record")
        .restore(creature.build().content().clone(), vec![])
        .unwrap()
}

fn nutrition(creature: &CreatureState) -> Value {
    serde_json::to_value(CreatureRecord::capture(creature)).unwrap()["nutrition"].clone()
}

fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}

fn edit_checkpoint(game: &Game, edit: impl FnOnce(&mut Value, &mut Value)) -> Game {
    let mut shared = SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    edit(&mut snapshot, &mut pool);
    Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &serde_json::from_value(pool).unwrap(),
        game.content().clone(),
    )
    .unwrap()
}

fn game(subject: ActorId, reserve: i32, elapsed: u64) -> Game {
    game_with_build(subject, reserve, elapsed, build(), 10)
}

fn game_with_build(
    subject: ActorId,
    reserve: i32,
    elapsed: u64,
    build: CreatureBuild,
    turn_ticks: u64,
) -> Game {
    game_with_selected_character(subject, reserve, elapsed, build, turn_ticks, None)
}

fn game_with_selected_character(
    subject: ActorId,
    reserve: i32,
    elapsed: u64,
    build: CreatureBuild,
    turn_ticks: u64,
    selected_character: Option<ActorId>,
) -> Game {
    let world = World::new(
        (1..=2)
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
    if selected_character.is_some() {
        game.configure_scenario_policy(
            tor_simulation::floor_activation::ScenarioCapabilities::default(),
            selected_character,
        )
        .unwrap();
    }
    for region in 1..=2 {
        let actor = game
            .spawn_actor(at(region, 1), NonZeroU64::new(turn_ticks).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {region}"),
                faction: "allies".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.refresh_navigation();
    edit_checkpoint(&game, |snapshot, pool| {
        let table = snapshot["actors"].as_u64().unwrap() as usize;
        let record = &mut pool["actors"][table][subject.0.to_string()]["combat"]["creature"];
        record["nutrition"] = required(reserve, elapsed);
        record["health_recovery"]["nutrition"] = recovery(reserve);
    })
}

fn rest(game: &mut Game, ticks: u64) {
    let until = game.tick().checked_add(ticks).unwrap();
    while game.tick() < until {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
}

fn eat(item: u64) -> Action {
    serde_json::from_value(json!({"type":"eat", "value":{"item":item}}))
        .expect("ordinary Eat action has the same strict envelope as Drink")
}

fn food(game: &mut Game, actor: ActorId, id: u64, quantity: u64, nourishment: u32) {
    food_amount(
        game,
        actor,
        id,
        quantity,
        json!({"kind":"fixed", "amount":nourishment}),
    );
}

fn food_amount(game: &mut Game, actor: ActorId, id: u64, quantity: u64, nourishment: Value) {
    let mut spec = ItemSpec::ordinary(format!("test food {id}"));
    spec.class = tor_simulation::ItemClass::Food;
    spec.stackable = true;
    let mut encoded = serde_json::to_value(spec).unwrap();
    encoded["edible"] = json!({"nutrition":nourishment});
    let spec = serde_json::from_value(encoded).expect("food has an explicit edible capability");
    game.place_item_stack(id, at(actor.0, 1), Some(actor), quantity, spec)
        .unwrap();
}

fn quantity(game: &Game, actor: ActorId, item: u64) -> Option<u64> {
    game.observe(actor)
        .unwrap()
        .inventory
        .iter()
        .find(|entry| entry.id == ItemId(item))
        .map(|entry| entry.quantity)
}

#[test]
fn required_nutrition_uses_active_deadlines_without_query_or_frozen_catchup() {
    for detach in [false, true] {
        let subject = ActorId(2);
        let mut game = game(subject, 8, 0);
        assert_eq!(game.next_creature_event_tick(), Some(200));
        rest(&mut game, 50);
        let expected = nutrition(&game.creature(subject).unwrap());
        assert_eq!(expected, required(8, 50));
        let before = game.clone();
        for _ in 0..3 {
            assert_eq!(nutrition(&game.creature(subject).unwrap()), expected);
            game.observe(subject).unwrap();
        }
        assert_eq!(
            game, before,
            "disclosure cannot settle authority or advance time"
        );
        let mut records = MemoryRecords::default();
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1)]),
                loaded: if detach {
                    BTreeSet::from([RegionId(1)])
                } else {
                    BTreeSet::from([RegionId(1), RegionId(2)])
                },
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.next_creature_event_tick(), None);
        rest(&mut game, 2_000);
        game = edit_checkpoint(&game, |_, _| {});
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(nutrition(&game.creature(subject).unwrap()), expected);
        assert_eq!(game.next_creature_event_tick(), Some(game.tick() + 150));
        for region in [1, 2, 1, 2] {
            game.teleport(subject, at(region, 3)).unwrap();
            assert_eq!(nutrition(&game.creature(subject).unwrap()), expected);
        }
        rest(&mut game, 150);
        assert_eq!(nutrition(&game.creature(subject).unwrap()), required(6, 0));
        assert_eq!(
            game.creature(subject)
                .unwrap()
                .health_recovery()
                .nutrition(),
            RecoveryNutrition::Slowed { multiplier: 2 }
        );
    }
}

#[test]
fn hunger_boundaries_settle_old_recovery_before_rephasing() {
    let mut creature = required_creature(8, 0);
    creature.damage(20);
    creature.advance_active(199);
    assert_eq!(creature.health().injury(), 19);
    assert_eq!(creature.health_recovery().elapsed(), 98);
    creature.advance_active(1);
    assert_eq!(nutrition(&creature), required(6, 0));
    assert_eq!(creature.health_recovery().period(), 202);
    assert_eq!(creature.health_recovery().elapsed(), 198);
    creature.advance_active(400);
    assert_eq!(nutrition(&creature), required(2, 0));
    assert_eq!(creature.health().injury(), 17);
    assert!(creature.health_recovery().stopped());
    assert_eq!(creature.health_recovery().elapsed(), 97);
    creature.advance_active(100);
    assert_eq!(creature.health().injury(), 17);
    assert_eq!(creature.health_recovery().elapsed(), 97);
}

#[test]
fn large_and_incremental_active_intervals_cross_the_same_hunger_and_death_boundaries() {
    let mut original = required_creature(8, 37);
    original.damage(20);
    for ticks in [162, 163, 562, 563, 763, 1062, 1063, u64::MAX] {
        let mut bulk = original.clone();
        bulk.advance_active(ticks);
        let mut incremental = original.clone();
        for _ in 0..ticks.min(2_000) {
            incremental.advance_active(1);
        }
        assert_eq!(bulk, incremental, "active interval {ticks}");
        if ticks >= 1063 {
            assert!(bulk.health().dead());
            assert_eq!(nutrition(&bulk), required(-3, 0));
            assert_eq!(bulk.next_change_in(), None);
        }
    }
}

#[test]
fn eat_reserves_one_exact_unit_and_commits_nourishment_once() {
    let actor = ActorId(1);
    let mut game = game(actor, -1, 0);
    food(&mut game, actor, 10, 2, 6);
    food(&mut game, actor, 11, 3, 9);
    let rng = rules_rng(&game);
    let id = game
        .admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(quantity(&game, actor, 10), Some(2));
    assert_eq!(nutrition(&game.creature(actor).unwrap()), required(-1, 0));
    game.execute_next_intention().unwrap().outcome.unwrap();
    let pending = game.preparation(actor).unwrap();
    assert_eq!(pending.origin_intention(), Some(id));
    let encoded = serde_json::to_value(pending).unwrap();
    assert_eq!(encoded["plan"]["kind"], "eat");
    assert_eq!(
        encoded["item_payment"],
        json!({"kind":"held", "refs":[{
            "cost_ordinal":0, "item":10
        }]})
    );
    assert_eq!(
        quantity(&game, actor, 10),
        Some(2),
        "preparation only holds"
    );
    let mut restored = edit_checkpoint(&game, |_, _| {});
    for state in [&mut game, &mut restored] {
        while state.preparation(actor).is_some() {
            state
                .act(state.next_actor().unwrap(), Action::Wait)
                .unwrap();
        }
        assert_eq!(quantity(state, actor, 10), Some(1));
        assert_eq!(quantity(state, actor, 11), Some(3));
        assert_eq!(rules_rng(state), rng, "fixed meals never draw");
        assert_eq!(nutrition(&state.creature(actor).unwrap())["reserve"], 6);
        assert_eq!(
            state.creature(actor).unwrap().health_recovery().nutrition(),
            RecoveryNutrition::Slowed { multiplier: 2 }
        );
    }
    assert_eq!(game, restored, "recovery spends and nourishes exactly once");
}

#[test]
fn replaced_eating_releases_the_exact_hold_without_refunding_time_or_nourishing() {
    let actor = ActorId(1);
    let mut game = game(actor, 8, 0);
    food(&mut game, actor, 10, 1, 6);
    let id = game
        .admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.pause_preparation(actor).unwrap();
    rest(&mut game, 40);
    let before = game.clone();
    assert!(
        game.cancel_intention(actor, id).is_err(),
        "item work uses ordinary replacement, not explicit cancellation"
    );
    assert_eq!(game, before);
    game.act(actor, Action::Move(tor_world::Direction::East))
        .unwrap();
    assert!(game.preparation(actor).is_none());
    assert_eq!(quantity(&game, actor, 10), Some(1));
    assert_eq!(nutrition(&game.creature(actor).unwrap()), required(8, 40));
    let id = game
        .admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    while game.next_intention_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(id)
    );
}

#[test]
fn starvation_uses_shared_death_cleanup_without_awarding_xp() {
    let subject = ActorId(2);
    let mut game = game(subject, -2, 90);
    game.teleport(ActorId(1), at(2, 3)).unwrap();
    let keepsake = game
        .place_authored_item(10, at(2, 1), "keepsake".into(), Some(subject))
        .unwrap();
    let owner_before = game.creature(ActorId(1)).unwrap().earned_progress().clone();
    rest(&mut game, 10);
    assert!(!game.alive(subject));
    assert!(game.preparation(subject).is_none());
    assert!(game.observe(subject).unwrap().inventory.is_empty());
    let scene = game.observe(ActorId(1)).unwrap();
    assert!(scene.ground_items.iter().any(|item| item.id == keepsake));
    assert_eq!(
        game.creature(ActorId(1)).unwrap().earned_progress(),
        &owner_before
    );
    let restored = edit_checkpoint(&game, |_, _| {});
    assert_eq!(restored, game);
    assert_eq!(
        restored.creature(ActorId(1)).unwrap().earned_progress(),
        &owner_before
    );
}

#[test]
fn nutrition_records_reject_missing_invalid_or_inconsistent_state() {
    let exempt = CreatureState::new(fixture::build(fixture::species())).unwrap();
    assert_eq!(nutrition(&exempt), json!({"kind":"exempt"}));
    let creature = required_creature(6, 30);
    let original = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    let rejects = |value| {
        !serde_json::from_value::<CreatureRecord>(value).is_ok_and(|record| {
            record
                .restore(creature.build().content().clone(), vec![])
                .is_ok()
        })
    };
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("nutrition");
    assert!(rejects(missing));
    for invalid in [
        Value::Null,
        json!([]),
        json!({"kind":"exempt", "reserve":0}),
        json!({"kind":"required", "reserve":6}),
        required(13, 0),
        required(-3, 0),
        required(6, 100),
        required(6, u64::MAX),
    ] {
        let mut changed = original.clone();
        changed["nutrition"] = invalid;
        assert!(rejects(changed));
    }
    let mut inconsistent = original.clone();
    inconsistent["health_recovery"]["nutrition"] = json!({"kind":"normal"});
    assert!(
        rejects(inconsistent),
        "Health policy is derived from required nutrition"
    );
}

#[test]
fn nutrition_policy_rejects_invalid_thresholds_rates_and_overflow_before_mutation() {
    let mut document = serde_json::to_value(fixture::ruleset().document()).unwrap();
    document["nutrition"] = policy();
    assert!(
        serde_json::from_value::<RulesetDocument>(document.clone())
            .is_ok_and(|document| Ruleset::validate(document).is_ok()),
        "valid explicit policy"
    );
    for (field, value) in [
        ("initial", json!(13)),
        ("maximum", json!(0)),
        ("maximum", json!(u64::MAX)),
        ("hungry_below", json!(2)),
        ("weak_below", json!(0)),
        ("starve_at", json!(1)),
        ("unit_ticks", json!(0)),
        ("unit_ticks", json!(u64::MAX)),
        ("hungry_recovery", json!({"kind":"slowed", "multiplier":1})),
        (
            "hungry_recovery",
            json!({"kind":"slowed", "multiplier":101}),
        ),
    ] {
        let mut invalid = document.clone();
        invalid["nutrition"][field] = value;
        assert!(
            !serde_json::from_value::<RulesetDocument>(invalid)
                .is_ok_and(|document| Ruleset::validate(document).is_ok()),
            "{field}"
        );
    }
}

#[test]
fn eat_schedule_overflow_is_atomic_before_admission_or_item_holding() {
    let actor = ActorId(1);
    let mut base = game(actor, 8, 0);
    food(&mut base, actor, 10, 1, 6);
    // Nutrition itself has a later deadline; test the action schedule near the
    // clock ceiling using an exempt actor, so no impossible future tick is saved.
    let base = edit_checkpoint(&base, |snapshot, pool| {
        let table = snapshot["actors"].as_u64().unwrap() as usize;
        pool["actors"][table]["1"]["combat"]["creature"]["nutrition"] = json!({"kind":"exempt"});
    });
    let duration = base.content().rules().constants().item_timing.consume_turns * 10;
    let mut game = edit_checkpoint(&base, |snapshot, pool| {
        let tick = u64::MAX - duration + 1;
        snapshot["tick"] = json!(tick);
        for table in pool["actors"].as_array_mut().unwrap() {
            for actor in table.as_object_mut().unwrap().values_mut() {
                actor["ready_at"] = json!(tick);
                actor["active_since"] = json!(tick);
            }
        }
    });
    let before = game.clone();
    assert_eq!(
        game.admit_intention(actor, eat(10), IntentionOrigin::Human),
        Err(tor_simulation::GameError::Schedule(
            tor_simulation::ScheduleError::TimeExhausted
        ))
    );
    assert_eq!(
        game, before,
        "overflow cannot publish a hold, intent, timer, event, or RNG change"
    );
}
// Timer-before-action ordering also governs the last possible bite.
#[test]
fn starvation_timer_wins_an_exact_eating_completion_tie_and_releases_food_unconsumed() {
    for (elapsed, survives) in [(90, false), (89, true)] {
        let actor = ActorId(1);
        let witness = ActorId(2);
        let mut game = game(actor, -2, elapsed);
        game.teleport(witness, at(1, 3)).unwrap();
        food(&mut game, actor, 10, 1, 6);
        game.admit_intention(actor, eat(10), IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.preparation(actor).unwrap().remaining, 10);
        assert_eq!(quantity(&game, actor, 10), Some(1));
        let mut recovered = edit_checkpoint(&game, |_, _| {});
        for state in [&mut game, &mut recovered] {
            while state.preparation(actor).is_some() {
                state
                    .act(state.next_actor().unwrap(), Action::Wait)
                    .unwrap();
            }
            assert_eq!(state.tick(), 10);
            assert_eq!(state.alive(actor), survives);
            if survives {
                assert_eq!(quantity(state, actor, 10), None);
                assert_eq!(nutrition(&state.creature(actor).unwrap()), required(6, 99));
            } else {
                assert_eq!(nutrition(&state.creature(actor).unwrap()), required(-3, 0));
                let scene = state.observe(witness).unwrap();
                assert!(scene
                    .ground_items
                    .iter()
                    .any(|item| item.id == ItemId(10) && item.quantity == 1));
                assert!(
                    state
                        .creature(witness)
                        .unwrap()
                        .earned_progress()
                        .earned_xp()
                        == 0
                );
            }
        }
        assert_eq!(game, recovered);
    }
}

#[test]
fn regeneration_starvation_wins_eat_completion_tie_without_consuming_the_ration() {
    for (elapsed, survives) in [(91, false), (90, true)] {
        let actor = ActorId(1);
        let witness = ActorId(2);
        let mut authored = policy();
        for field in ["hungry_recovery", "weak_recovery", "empty_recovery"] {
            authored[field] = json!({"kind":"normal"});
        }
        authored["regeneration_cost"] =
            json!({"denominator":1,"health":3,"stamina":1,"focus":1,"mana":1});
        let game = game_with_build(actor, 8, 0, build_with_policy(authored), 10);
        let mut game = edit_checkpoint(&game, |snapshot, pool| {
            let actors = snapshot["actors"].as_u64().unwrap() as usize;
            let creature = &mut pool["actors"][actors]["1"]["combat"]["creature"];
            creature["nutrition"] = required(0, 0);
            creature["injury"] = json!(1);
            creature["health_recovery"]["elapsed"] = json!(elapsed);
        });
        game.teleport(witness, at(1, 3)).unwrap();
        food(&mut game, actor, 10, 1, 6);
        game.admit_intention(actor, eat(10), IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let mut restored = edit_checkpoint(&game, |_, _| {});
        for state in [&mut game, &mut restored] {
            while state.preparation(actor).is_some() {
                state
                    .act(state.next_actor().unwrap(), Action::Wait)
                    .unwrap();
            }
            assert_eq!(state.tick(), 10);
            assert_eq!(state.alive(actor), survives);
            if survives {
                assert_eq!(quantity(state, actor, 10), None);
                assert_eq!(nutrition(&state.creature(actor).unwrap()), required(6, 10));
            } else {
                assert_eq!(nutrition(&state.creature(actor).unwrap()), required(-3, 0));
                assert!(state
                    .observe(witness)
                    .unwrap()
                    .ground_items
                    .iter()
                    .any(|item| item.id == ItemId(10) && item.quantity == 1));
            }
        }
        assert_eq!(restored, game);
    }
}

// Both ordinary and perception-conditioned sources use the same Health clock.
fn expiry_build(scope: serde_json::Value) -> CreatureBuild {
    let original = build();
    let mut document = serde_json::to_value(original.ruleset().document()).unwrap();
    let mut condition = document["conditions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "braced")
        .unwrap()
        .clone();
    condition["id"] = json!("nutrition_expiry_probe");
    condition["family"] = json!("nutrition_expiry_probe");
    condition["modifiers"] = json!([{
        "scope":scope,
        "op":{"kind":"set", "stat":{"kind":"health_recovery_period_percent"}, "value":2}
    }]);
    document["conditions"]
        .as_array_mut()
        .unwrap()
        .push(condition);
    let rules = Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let content = Content::new(rules, original.content().catalog().clone()).unwrap();
    CreatureBuild::new(
        content,
        original.species_id().clone(),
        original.ledger().clone(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap()
}

#[test]
fn simultaneous_condition_expiry_and_hunger_rephase_health_fraction_once() {
    let mut creature = CreatureState::new(expiry_build(json!("always"))).unwrap();
    let mut record = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    record["nutrition"] = required(7, 99);
    creature = serde_json::from_value::<CreatureRecord>(record)
        .unwrap()
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    creature
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("nutrition_expiry_probe").unwrap(),
            ActorId(1),
            1,
        )
        .unwrap();
    creature.damage(20);
    assert_eq!(creature.health_recovery().period(), 3);
    let original = creature.clone();
    creature.advance_active(1);
    assert_eq!(creature.health_recovery().period(), 202);
    assert_eq!(
        creature.health_recovery().elapsed(),
        67,
        "floor(1 * 202 / 3), never floor(1 * 101 / 3) * 2 = 66"
    );
    for ticks in [1, 2, 400, 401, 999] {
        let mut large = original.clone();
        let mut incremental = original.clone();
        large.advance_active(ticks);
        for _ in 0..ticks {
            incremental.advance_active(1);
        }
        assert_eq!(large, incremental, "interval {ticks}");
    }
}

#[test]
fn enrollment_and_conflicting_recovery_mutators_reject_atomically() {
    use tor_simulation::nutrition::NutritionRequirement;
    let mut disabled = CreatureState::new(fixture::build(fixture::species())).unwrap();
    let before = disabled.clone();
    assert!(disabled
        .configure_nutrition(NutritionRequirement::Required)
        .is_err());
    assert_eq!(disabled, before);
    let mut enabled = CreatureState::new(build()).unwrap();
    assert_eq!(nutrition(&enabled), json!({"kind":"exempt"}));
    enabled
        .configure_nutrition(NutritionRequirement::Required)
        .unwrap();
    enabled.advance_active(200);
    let before = enabled.clone();
    assert!(enabled
        .configure_nutrition(NutritionRequirement::Required)
        .is_err());
    assert_eq!(enabled, before);
    assert!(enabled
        .set_recovery_nutrition(RecoveryNutrition::Normal)
        .is_err());
    assert_eq!(enabled, before);
}

#[test]
fn required_nutrition_rebuild_rejects_every_policy_change_atomically() {
    let mut policies = vec![("disabled", json!({"kind":"disabled"}), 5)];
    for (field, value, reserve) in [
        ("initial", json!(9), 5),
        ("maximum", json!(11), 12),
        ("hungry_below", json!(5), 5),
        ("weak_below", json!(6), 5),
        ("starve_at", json!(0), -2),
        ("unit_ticks", json!(25), 5),
        ("hungry_recovery", json!({"kind":"normal"}), 5),
        ("weak_recovery", json!({"kind":"slowed", "multiplier":3}), 2),
        ("empty_recovery", json!({"kind":"normal"}), -2),
    ] {
        let mut changed = policy();
        changed[field] = value;
        policies.push((field, changed, reserve));
    }
    for (field, policy, reserve) in policies {
        let replacement = build_with_policy(policy);
        for dead in [false, true] {
            let mut creature = required_creature(reserve, 30);
            creature.damage(2);
            creature
                .apply_condition(
                    &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
                    ActorId(9),
                    300,
                )
                .unwrap();
            creature.advance_active(17);
            if dead {
                creature.kill();
            }
            let before = creature.clone();
            let record = CreatureRecord::capture(&creature);
            assert_eq!(
                creature.rebuild(replacement.clone()),
                Err(tor_simulation::creatures::CreatureStateError::Inconsistent),
                "changed {field}, dead {dead}"
            );
            assert_eq!(creature, before, "changed {field}, dead {dead}");
            assert_eq!(CreatureRecord::capture(&creature), record);
            assert_eq!(
                record
                    .restore(creature.build().content().clone(), vec![])
                    .unwrap(),
                creature,
                "rejection must leave the complete runtime record restorable"
            );
        }
    }
}

#[test]
fn exempt_nutrition_rebuild_preserves_enrollment_and_independent_recovery_clock() {
    for (original, replacement) in [
        (json!({"kind":"disabled"}), policy()),
        (policy(), json!({"kind":"disabled"})),
    ] {
        let mut creature = CreatureState::new(build_with_policy(original)).unwrap();
        creature.damage(2);
        creature
            .set_recovery_nutrition(RecoveryNutrition::Slowed { multiplier: 3 })
            .unwrap();
        creature
            .apply_condition(
                &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
                ActorId(9),
                300,
            )
            .unwrap();
        creature.advance_active(17);
        let clock = creature.health_recovery();
        let mut expected = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
        let replacement = build_with_policy(replacement);
        creature.rebuild(replacement.clone()).unwrap();
        assert_eq!(creature.build(), &replacement);
        assert_eq!(creature.health_recovery(), clock);
        assert_eq!(nutrition(&creature), json!({"kind":"exempt"}));
        let record = CreatureRecord::capture(&creature);
        let actual = serde_json::to_value(&record).unwrap();
        expected["build_epoch"] = actual["build_epoch"].clone();
        assert_eq!(actual, expected);
        assert_eq!(
            record
                .restore(creature.build().content().clone(), vec![])
                .unwrap(),
            creature
        );
    }
}

#[test]
fn food_and_corpse_classes_do_not_grant_edibility_and_forged_meal_plans_reject() {
    use tor_simulation::ItemClass;
    let actor = ActorId(1);
    let mut game = game(actor, 8, 0);
    for (id, class) in [(10, ItemClass::Food), (11, ItemClass::Corpse)] {
        let mut spec = ItemSpec::ordinary("inedible fixture".into());
        spec.class = class;
        game.place_item_stack(id, at(1, 1), Some(actor), 1, spec)
            .unwrap();
        let before = game.clone();
        assert!(game
            .admit_intention(actor, eat(id), IntentionOrigin::Human)
            .is_err());
        assert_eq!(game, before);
    }
    food(&mut game, actor, 12, 1, 6);
    game.admit_intention(actor, eat(12), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let encoded = serde_json::to_value(&snapshot).unwrap();
    let table = encoded["actors"].as_u64().unwrap() as usize;
    let mut forged = serde_json::to_value(shared).unwrap();
    let amount =
        &mut forged["actors"][table]["1"]["pending"]["plan"]["effects"][0]["amount"]["amount"];
    assert_eq!(*amount, json!(6));
    *amount = json!(7);
    assert!(Game::restore_checkpoint(
        snapshot,
        &serde_json::from_value(forged).unwrap(),
        game.content().clone()
    )
    .is_err());
}

#[test]
fn tagged_enabled_nutrition_policy_is_toml_capable_without_defaults() {
    let document = build().ruleset().document().clone();
    let source = toml::to_string(&document).unwrap();
    let decoded: RulesetDocument = toml::from_str(&source).unwrap();
    assert_eq!(serde_json::to_value(decoded.nutrition).unwrap(), policy());
}

#[test]
fn contextual_expiry_and_hunger_use_one_rephase_after_pending_frozen_thaw() {
    let actor = ActorId(2);
    let condition =
        tor_simulation::content_ids::ConditionId::new("nutrition_expiry_probe").unwrap();
    for frozen in [false, true] {
        let mut game = game_with_build(
            actor,
            7,
            99,
            expiry_build(json!("while_no_sensed_hostile")),
            1,
        );
        game.apply_condition(actor, &condition, actor, 1).unwrap();
        fixture::injure(&mut game, actor, 20).unwrap();
        assert_eq!(game.creature(actor).unwrap().health_recovery().period(), 3);
        let mut records = MemoryRecords::default();
        if frozen {
            game.transition_regions(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(1)]),
                    loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                },
                &mut records,
            )
            .unwrap();
            // A valid pending record keeps its captured denominator. Genuine
            // earned-first creation of Pending is covered by earned_context.
            game = edit_checkpoint(&game, |snapshot, pool| {
                let table = snapshot["actors"].as_u64().unwrap() as usize;
                pool["actors"][table]["2"]["combat"]["creature"]["health_recovery"]["context"] =
                    json!({"kind":"pending", "base_period":3});
            });
            let before = CreatureRecord::capture(&game.creature(actor).unwrap());
            rest(&mut game, 500);
            assert_eq!(
                serde_json::to_value(CreatureRecord::capture(&game.creature(actor).unwrap()))
                    .unwrap(),
                serde_json::to_value(before).unwrap()
            );
            game = edit_checkpoint(&game, |_, _| {});
            game.transition_regions(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(1), RegionId(2)]),
                    loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                },
                &mut records,
            )
            .unwrap();
        }
        let original = game.creature(actor).unwrap().into_owned();
        rest(&mut game, 1);
        let changed = game.creature(actor).unwrap();
        assert_eq!(changed.health_recovery().period(), 202);
        assert_eq!(changed.health_recovery().elapsed(), 67);
        assert_eq!(nutrition(&changed), required(6, 0));
        let record = serde_json::to_value(CreatureRecord::capture(&changed)).unwrap();
        assert_eq!(
            record["health_recovery"]["context"],
            json!({"kind":"known","sensed_hostile":false})
        );
        for ticks in [1, 2, 400, 401, 999] {
            let mut bulk = original.clone();
            let mut incremental = original.clone();
            bulk.advance_active(ticks);
            for _ in 0..ticks {
                incremental.advance_active(1);
            }
            assert_eq!(bulk, incremental, "frozen {frozen}, interval {ticks}");
        }
        assert_eq!(game, edit_checkpoint(&game, |_, _| {}));
    }
}

fn population_game(requirement: tor_simulation::nutrition::NutritionRequirement) -> Game {
    use std::collections::BTreeMap;
    use tor_simulation::creatures::record::BuildRecord;
    use tor_simulation::floor_activation::{
        DepthBand, DepthInterval, PopulationContent, PopulationPlan, PopulationProfile,
        PopulationStage, PreparedActorChoice, PreparedScope, ScenarioCapabilities,
    };
    use tor_simulation::nutrition::NutritionRequirement;
    use tor_simulation::prepared_spawn::PreparedActor;
    let protagonist = ActorId(1);
    let build = build();
    let world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("birth room {id}"),
                bounds: Extent::new(8, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, build.content().clone());
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(protagonist))
        .unwrap();
    for region in 1..=2 {
        let id = game
            .spawn_actor(at(region, 1), NonZeroU64::new(1).unwrap())
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject {region}"),
                faction: "allies".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.configure_nutrition(protagonist, NutritionRequirement::Required)
        .unwrap();
    game.refresh_navigation();
    let actor = PreparedActor {
        build: Some(BuildRecord::capture(&build)),
        nutrition: requirement,
        identity: Some(CreatureIdentity {
            name: "newborn".into(),
            faction: "allies".into(),
        }),
        anatomy: None,
        body: None,
        ai: None,
        known_identities: vec![],
        velocity: None,
        asset: None,
    };
    game.configure_population(PopulationPlan {
        carried_item_pools: BTreeMap::new(),
        protagonist,
        profiles: BTreeMap::from([(
            "birth".into(),
            PopulationProfile {
                version: 1,
                stages: vec![PopulationStage {
                    id: "newborn".into(),
                    version: 1,
                    count: [1, 1],
                    population: PopulationContent::Actors {
                        bands: vec![DepthBand {
                            depth: DepthInterval::Open { minimum: 1 },
                            choices: vec![PreparedActorChoice {
                                advancement: vec![],
                                carried_loot: None,
                                depth_growth: tor_simulation::prepared_spawn::DepthGrowth::Fixed {},
                                content: "newborn".into(),
                                weight: 1,
                                turn_ticks: 1,
                                actor,
                            }],
                        }],
                        avoid_protagonist_sight: false,
                    },
                }],
            },
        )]),
    })
    .unwrap();
    game.register_activation_scope(PreparedScope {
        rooms: vec![],
        scope: "dungeon/2".into(),
        depth: 2,
        profile: "birth".into(),
        members: BTreeSet::from([RegionId(2)]),
        cells: vec![at(2, 5)],
        protected: BTreeSet::new(),
    })
    .unwrap();
    game
}

#[test]
fn required_population_enrolls_at_birth_without_retroactive_nutrition() {
    let protagonist = ActorId(1);
    let mut game = population_game(tor_simulation::nutrition::NutritionRequirement::Required);
    rest(&mut game, 350);
    game.teleport(protagonist, at(2, 3)).unwrap();
    let newborn = ActorId(3);
    assert_eq!(game.tick(), 350);
    assert_eq!(
        nutrition(&game.creature(protagonist).unwrap()),
        required(5, 50)
    );
    assert_eq!(
        nutrition(&game.creature(ActorId(2)).unwrap()),
        json!({"kind":"exempt"})
    );
    assert_eq!(nutrition(&game.creature(newborn).unwrap()), required(8, 0));
    let mut restored = edit_checkpoint(&game, |_, _| {});
    for state in [&mut game, &mut restored] {
        rest(state, 100);
        assert_eq!(nutrition(&state.creature(newborn).unwrap()), required(7, 0));
    }
    assert_eq!(game, restored);
}

#[test]
fn population_checkpoint_pool_preserves_complete_nutrition_recipes() {
    use tor_simulation::checkpoint::{RestoreContext, Snapshot};
    use tor_simulation::nutrition::NutritionRequirement;
    let required_game = population_game(NutritionRequirement::Required);
    let exempt_game = population_game(NutritionRequirement::Exempt);
    let mut shared = SharedState::default();
    let snapshots = vec![
        required_game.checkpoint(&mut shared),
        exempt_game.checkpoint(&mut shared),
        required_game.checkpoint(&mut shared),
    ];
    let encoded = serde_json::to_value((snapshots, shared)).unwrap();
    assert_eq!(encoded[1]["floor_activations"].as_array().unwrap().len(), 2);
    assert_eq!(encoded[0][0]["floor_activation"], json!(0));
    assert_eq!(encoded[0][1]["floor_activation"], json!(1));
    assert_eq!(encoded[0][2]["floor_activation"], json!(0));
    let recipe = "/plan/profiles/birth/stages/0/population/bands/0/choices/0/actor/nutrition";
    assert_eq!(
        encoded[1]["floor_activations"][0].pointer(recipe).unwrap(),
        &json!({"kind": "required"})
    );
    assert_eq!(
        encoded[1]["floor_activations"][1].pointer(recipe).unwrap(),
        &json!({"kind": "exempt"})
    );
    let (snapshots, shared): (Vec<Snapshot>, SharedState) =
        serde_json::from_value(encoded).unwrap();
    let mut context = RestoreContext::new(&shared, required_game.content().clone());
    let mut restored: Vec<_> = snapshots
        .into_iter()
        .map(|snapshot| context.restore(snapshot).unwrap())
        .collect();
    assert_eq!(restored[0], required_game);
    assert_eq!(restored[1], exempt_game);
    assert_eq!(restored[2], required_game);
    drop(context);
    drop(shared);

    // No live reserve belongs to a recipe. After restore, each future birth
    // enrolls at its own activation time using that boundary's exact recipe.
    for (index, game) in restored.iter_mut().enumerate() {
        rest(game, 350);
        game.teleport(ActorId(1), at(2, 3)).unwrap();
        assert_eq!(game.tick(), 350);
        let expected = if index == 1 {
            json!({"kind": "exempt"})
        } else {
            required(8, 0)
        };
        assert_eq!(nutrition(&game.creature(ActorId(3)).unwrap()), expected);
        let checkpoint = edit_checkpoint(game, |_, _| {});
        assert_eq!(&*game, &checkpoint);
    }
    assert_eq!(restored[0], restored[2]);
}

#[test]
fn prepared_population_nutrition_uses_named_exact_shapes() {
    use tor_simulation::nutrition::NutritionRequirement;
    use tor_simulation::prepared_spawn::PreparedActor;
    let game = population_game(NutritionRequirement::Required);
    let mut shared = SharedState::default();
    game.checkpoint(&mut shared);
    let encoded = serde_json::to_value(shared).unwrap();
    let actor: PreparedActor = serde_json::from_value(
        encoded["floor_activations"][0]["plan"]["profiles"]["birth"]["stages"][0]["population"]
            ["bands"][0]["choices"][0]["actor"]
            .clone(),
    )
    .unwrap();
    let good = serde_json::to_value(&actor).unwrap();
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("nutrition");
    assert!(serde_json::from_value::<PreparedActor>(missing).is_err());
    for bad in [
        json!(null),
        json!([]),
        json!({"kind":"required","extra":true}),
    ] {
        let mut changed = good.clone();
        changed["nutrition"] = bad;
        assert!(serde_json::from_value::<PreparedActor>(changed).is_err());
    }
}

fn rolled(count: u16, sides: u16, bonus: i32) -> Value {
    json!({"kind":"rolled", "dice":{"count":count,"sides":sides,"bonus":bonus}})
}

fn rules_rng(game: &Game) -> u64 {
    serde_json::to_value(game.checkpoint(&mut SharedState::default())).unwrap()["rules_rng"]
        .as_u64()
        .unwrap()
}

fn finish_eating(game: &mut Game, actor: ActorId) {
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
}

#[test]
fn edible_operands_are_bounded_strictly_positive_and_round_trip_named_json_and_toml() {
    use tor_simulation::{abilities::Plan, EdibleSpec};
    for amount in [
        json!({"kind":"fixed","amount":1}),
        json!({"kind":"fixed","amount":1000000}),
        rolled(1, 400, 1099),
        rolled(64, 1000, -63),
        rolled(1, 2, 999998),
    ] {
        let edible: EdibleSpec = serde_json::from_value(json!({"nutrition":amount})).unwrap();
        assert!(edible.valid(), "{amount}");
        assert_eq!(
            serde_json::to_value(&edible).unwrap(),
            json!({"nutrition":amount})
        );
        assert_eq!(
            toml::from_str::<EdibleSpec>(&toml::to_string(&edible).unwrap()).unwrap(),
            edible
        );
        let plan: Plan = serde_json::from_value(json!({"kind":"eat","actor":1,"item":10,
            "preparation":10,"effects":[{"kind":"nourish","amount":amount}]}))
        .unwrap();
        assert!(plan.valid(build().ruleset()), "{amount}");
    }
    for amount in [
        json!({"kind":"fixed","amount":0}),
        json!({"kind":"fixed","amount":1000001}),
        rolled(0, 2, 1),
        rolled(65, 2, 0),
        rolled(1, 1, 0),
        rolled(1, 1001, 0),
        rolled(1, 2, -1),
        rolled(64, 1000, -64),
        rolled(1, 2, -1000001),
        rolled(1, 2, 1000000),
        rolled(64, 1000, i32::MAX),
    ] {
        let edible: EdibleSpec = serde_json::from_value(json!({"nutrition":amount})).unwrap();
        assert!(!edible.valid(), "{amount}");
        let mut state = game(ActorId(1), 8, 0);
        let before = state.clone();
        let mut spec = ItemSpec::ordinary("invalid meal".into());
        spec.edible = Some(edible);
        assert!(state
            .place_item_stack(10, at(1, 1), Some(ActorId(1)), 1, spec)
            .is_err());
        assert_eq!(state, before);
        let plan = serde_json::from_value::<Plan>(json!({"kind":"eat","actor":1,"item":10,
            "preparation":10,"effects":[{"kind":"nourish","amount":amount}]}));
        assert!(
            plan.is_err() || !plan.unwrap().valid(build().ruleset()),
            "{amount}"
        );
    }
}

#[test]
fn edible_and_frozen_amounts_reject_scalar_positional_duplicate_and_inexact_records() {
    use tor_simulation::{abilities::PlannedAmount, EdibleSpec};
    for raw in [
        "null",
        "[]",
        "[1]",
        "{}",
        r#"{"nutrition":6}"#,
        r#"{"nutrition":null}"#,
        r#"{"nutrition":{"kind":"fixed","amount":6},"extra":0}"#,
        r#"{"nutrition":{"kind":"fixed","amount":6},"nutrition":{"kind":"fixed","amount":7}}"#,
    ] {
        assert!(serde_json::from_str::<EdibleSpec>(raw).is_err(), "{raw}");
    }
    for raw in [
        "null",
        "6",
        "[]",
        "[\"fixed\",6]",
        "{}",
        r#"{"kind":"fixed"}"#,
        r#"{"kind":"fixed","amount":null}"#,
        r#"{"kind":"fixed","amount":-1}"#,
        r#"{"kind":"fixed","amount":1.5}"#,
        r#"{"kind":"fixed","amount":4294967296}"#,
        r#"{"kind":"fixed","amount":1,"amount":2}"#,
        r#"{"kind":"fixed","amount":1,"extra":0}"#,
        r#"{"kind":"unknown","amount":1}"#,
        r#"{"kind":"rolled"}"#,
        r#"{"kind":"rolled","dice":null}"#,
        r#"{"kind":"rolled","dice":[1,400,1099]}"#,
        r#"{"kind":"rolled","dice":{"count":1,"sides":400}}"#,
        r#"{"kind":"rolled","dice":{"count":1,"sides":400,"bonus":1099,"extra":0}}"#,
        r#"{"kind":"rolled","dice":{"count":1,"count":2,"sides":400,"bonus":1099}}"#,
        r#"{"kind":"rolled","dice":{"count":null,"sides":400,"bonus":1099}}"#,
        r#"{"kind":"rolled","dice":{"count":1,"sides":65536,"bonus":1099}}"#,
    ] {
        let edible = format!("{{\"nutrition\":{raw}}}");
        assert!(
            serde_json::from_str::<EdibleSpec>(&edible).is_err(),
            "{raw}"
        );
        assert!(serde_json::from_str::<PlannedAmount>(raw).is_err(), "{raw}");
    }
    // Positivity is the meal contract only. Generic amounts retain zero outcomes.
    let zero = json!({"kind":"fixed","amount":0});
    let clamped = rolled(1, 2, -2);
    for amount in [zero, clamped] {
        let amount: PlannedAmount = serde_json::from_value(amount).unwrap();
        for kind in ["heal", "restore", "nourish"] {
            let mut effect = json!({"kind":kind,"amount":amount});
            if kind == "restore" {
                effect["resource"] = json!("focus");
            }
            assert!(
                serde_json::from_value::<tor_simulation::abilities::PlannedEffect>(effect).is_ok()
            );
        }
    }
}

#[test]
fn rolled_meal_freezes_its_declaration_and_draws_only_at_successful_completion() {
    use tor_simulation::dice::{DicePool, Edge};
    let actor = ActorId(1);
    let mut policy = policy();
    policy["maximum"] = json!(4000);
    let mut game = game_with_build(actor, -1, 27, build_with_policy(policy), 10);
    food_amount(&mut game, actor, 10, 1, rolled(1, 400, 1099));
    let before_rng = rules_rng(&game);
    game.admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(rules_rng(&game), before_rng, "admission does not sample");
    game.execute_next_intention().unwrap().outcome.unwrap();
    let plan = serde_json::to_value(game.preparation(actor).unwrap()).unwrap();
    assert_eq!(
        plan["plan"]["effects"],
        json!([{"kind":"nourish","amount":rolled(1,400,1099)}])
    );
    assert_eq!(
        rules_rng(&game),
        before_rng,
        "preparation only freezes the operand"
    );
    for _ in 0..3 {
        game.observe(actor).unwrap();
    }
    let mut restored = edit_checkpoint(&game, |_, _| {});
    assert_eq!(
        rules_rng(&restored),
        before_rng,
        "capture/restore/queries do not sample"
    );
    let mut expected_rng = before_rng;
    let sample = DicePool::new(1, 400, 1099)
        .unwrap()
        .roll(&mut expected_rng, Edge::default())
        .total;
    assert!((1100..=1499).contains(&sample));
    let health = game.creature(actor).unwrap().health().current();
    let xp = game.creature(actor).unwrap().earned_progress().clone();
    for state in [&mut game, &mut restored] {
        finish_eating(state, actor);
        assert_eq!(
            quantity(state, actor, 10),
            None,
            "last physical source is consumed"
        );
        assert_eq!(
            nutrition(&state.creature(actor).unwrap()),
            required(sample as i32, 37)
        );
        assert_eq!(
            rules_rng(state),
            expected_rng,
            "one neutral pool resolution"
        );
        assert_eq!(state.creature(actor).unwrap().health().current(), health);
        assert_eq!(state.creature(actor).unwrap().earned_progress(), &xp);
        assert_eq!(*state, edit_checkpoint(state, |_, _| {}));
    }
    assert_eq!(game, restored);
}

#[test]
fn rolled_meal_replacement_pause_resume_and_frozen_time_do_not_sample() {
    let actor = ActorId(2);
    let mut game = game(actor, 8, 0);
    // Leave a genuine other-actor decision before this later actor's due meal.
    game.spawn_actor(at(1, 3), NonZeroU64::new(1).unwrap())
        .unwrap();
    food_amount(&mut game, actor, 10, 2, rolled(1, 4, 3));
    let rng = rules_rng(&game);
    let id = game
        .admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    while game.next_intention_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.pause_preparation(actor).unwrap();
    let frozen = game.preparation(actor).unwrap().plan.clone();
    let mut memories = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut memories,
    )
    .unwrap();
    rest(&mut game, 500);
    game = edit_checkpoint(&game, |_, _| {});
    assert_eq!(rules_rng(&game), rng);
    assert_eq!(game.preparation(actor).unwrap().plan, frozen);
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut memories,
    )
    .unwrap();
    let mut replaced = game.clone();
    game.resume_intention(actor, id).unwrap();
    assert_eq!(rules_rng(&game), rng);
    let mut resumed = edit_checkpoint(&game, |_, _| {});
    while resumed.next_intention_actor() != Some(actor) {
        resumed
            .act(resumed.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    resumed.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(resumed.preparation(actor).unwrap().plan, frozen);
    assert_eq!(
        rules_rng(&resumed),
        rng,
        "actual resume keeps the unsampled operand"
    );
    finish_eating(&mut resumed, actor);
    assert_eq!(quantity(&resumed, actor, 10), Some(1));
    assert_ne!(rules_rng(&resumed), rng);
    let before = replaced.clone();
    assert!(replaced.cancel_intention(actor, id).is_err());
    assert_eq!(replaced, before);
    while replaced.next_actor() != Some(actor) {
        replaced
            .act(replaced.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    replaced
        .act(actor, Action::Move(tor_world::Direction::East))
        .unwrap();
    assert_eq!(rules_rng(&replaced), rng);
    assert_eq!(quantity(&replaced, actor, 10), Some(2));
    assert!(replaced.preparation(actor).is_none());
}

#[test]
fn rolled_nourish_resolves_full_and_exempt_recipients_without_skipping_the_draw() {
    use tor_simulation::abilities::{
        resolve, PlannedAmount, PlannedEffect, ResolutionPlan, ResolvedEffect, Snapshot,
    };
    use tor_simulation::dice::{DicePool, Edge};
    let dice = DicePool::new(1, 400, 1099).unwrap();
    assert_eq!(dice.minimum(), 1100);
    assert_eq!(dice.maximum(), 1499);
    let effects = [PlannedEffect::Nourish {
        amount: PlannedAmount::Rolled { dice },
    }];
    for (creature, applied) in [
        (required_creature(-2, 57), 12),
        (required_creature(10, 57), 2),
        (required_creature(12, 57), 0),
        (CreatureState::new(build()).unwrap(), 0),
    ] {
        let mut rng = 42;
        let mut expected_rng = rng;
        let sample = dice.roll(&mut expected_rng, Edge::default()).total;
        assert!((1100..=1499).contains(&sample));
        let outcome = resolve(
            ResolutionPlan::Effects {
                actor: ActorId(1),
                target: ActorId(1),
                effects: &effects,
            },
            &Snapshot::capture(&creature),
            &mut rng,
            &mut (),
        )
        .unwrap();
        assert_eq!(rng, expected_rng);
        assert_eq!(
            outcome.effects,
            vec![ResolvedEffect::Nourish { amount: applied }],
            "the canonical effect records applied gain, excluding cleared debt and the raw sample"
        );
    }
}

#[test]
fn rolled_meal_starvation_tie_does_not_draw_or_consume_the_held_unit() {
    let actor = ActorId(1);
    let mut game = game(actor, -2, 90);
    game.teleport(ActorId(2), at(1, 3)).unwrap();
    food_amount(&mut game, actor, 10, 1, rolled(1, 400, 1099));
    let rng = rules_rng(&game);
    game.admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut restored = edit_checkpoint(&game, |_, _| {});
    for state in [&mut game, &mut restored] {
        finish_eating(state, actor);
        assert!(!state.alive(actor));
        assert_eq!(rules_rng(state), rng);
        assert!(state
            .observe(ActorId(2))
            .unwrap()
            .ground_items
            .iter()
            .any(|item| item.id == ItemId(10) && item.quantity == 1));
    }
    assert_eq!(game, restored);
}

#[test]
fn checkpoint_rejects_a_changed_frozen_meal_die_even_when_both_pools_are_positive() {
    let actor = ActorId(1);
    let mut game = game(actor, 8, 0);
    food_amount(&mut game, actor, 10, 1, rolled(1, 4, 3));
    game.admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let table = serde_json::to_value(&snapshot).unwrap()["actors"]
        .as_u64()
        .unwrap() as usize;
    let encoded = serde_json::to_value(&shared).unwrap();
    for (field, value) in [("count", 2), ("sides", 5), ("bonus", 4)] {
        let mut changed = encoded.clone();
        changed["actors"][table]["1"]["pending"]["plan"]["effects"][0]["amount"]["dice"][field] =
            json!(value);
        assert!(
            Game::restore_checkpoint(
                serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap(),
                &serde_json::from_value(changed).unwrap(),
                game.content().clone()
            )
            .is_err(),
            "forged {field}"
        );
    }
}

#[test]
fn a_missing_live_held_item_cannot_be_rebuilt_from_a_frozen_rolled_operand() {
    let actor = ActorId(1);
    let mut game = game(actor, 8, 0);
    food_amount(&mut game, actor, 10, 1, rolled(1, 4, 3));
    game.admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let table = serde_json::to_value(&snapshot).unwrap()["items"]
        .as_u64()
        .unwrap() as usize;
    let mut encoded = serde_json::to_value(&shared).unwrap();
    assert!(encoded["items"][table]
        .as_object_mut()
        .unwrap()
        .remove("10")
        .is_some());
    assert!(Game::restore_checkpoint(
        snapshot,
        &serde_json::from_value(encoded).unwrap(),
        game.content().clone()
    )
    .is_err());
}

#[test]
fn pooled_population_item_recipes_retain_distinct_declared_meal_operands() {
    use std::collections::BTreeMap;
    use tor_simulation::floor_activation::{
        DepthBand, DepthInterval, PopulationContent, PopulationPlan, PopulationProfile,
        PopulationStage, PreparedItemChoice,
    };
    let mut states = Vec::new();
    for amount in [
        rolled(1, 400, 1099),
        rolled(1, 400, 1100),
        rolled(1, 400, 1099),
    ] {
        let mut game =
            game_with_selected_character(ActorId(1), 8, 0, build(), 10, Some(ActorId(1)));
        let mut spec = serde_json::to_value(ItemSpec::ordinary("future meal".into())).unwrap();
        spec["edible"] = json!({"nutrition":amount});
        let item = PreparedItemChoice {
            content: "meal".into(),
            weight: 1,
            quantity: tor_simulation::quantity::QuantityExpr::Fixed { quantity: 1 },
            spec: serde_json::from_value(spec).unwrap(),
        };
        assert_eq!(
            serde_json::from_value::<PreparedItemChoice>(serde_json::to_value(&item).unwrap())
                .unwrap(),
            item
        );
        game.configure_population(PopulationPlan {
            carried_item_pools: BTreeMap::new(),
            protagonist: ActorId(1),
            profiles: BTreeMap::from([(
                "food".into(),
                PopulationProfile {
                    version: 1,
                    stages: vec![PopulationStage {
                        id: "meal".into(),
                        version: 1,
                        count: [1, 1],
                        population: PopulationContent::Items {
                            bands: vec![DepthBand {
                                depth: DepthInterval::Open { minimum: 1 },
                                choices: vec![item],
                            }],
                        },
                    }],
                },
            )]),
        })
        .unwrap();
        states.push(game);
    }
    let mut shared = SharedState::default();
    let snapshots: Vec<_> = states
        .iter()
        .map(|game| game.checkpoint(&mut shared))
        .collect();
    let encoded = serde_json::to_value(&shared).unwrap();
    assert_eq!(
        encoded["floor_activations"].as_array().unwrap().len(),
        2,
        "pool equality includes the complete item operand"
    );
    for (snapshot, expected) in snapshots.into_iter().zip(states) {
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, expected.content().clone()).unwrap(),
            expected
        );
    }
}

#[test]
fn failure_after_a_rolled_meal_resolution_rolls_back_the_draw_item_hold_and_events() {
    assert_rolled_meal_late_failure_is_atomic(false);
}

#[test]
fn late_failure_rolls_back_actual_regeneration_cost_fraction_and_health() {
    assert_rolled_meal_late_failure_is_atomic(true);
}

fn assert_rolled_meal_late_failure_is_atomic(charge_regeneration: bool) {
    use tor_simulation::rules::{AmountExpr, DamageComponentExpr, DamageExpr, Effect};
    let actor = ActorId(1);
    let other = ActorId(2);
    let mut authored = policy();
    if charge_regeneration {
        authored["regeneration_cost"] =
            json!({"denominator":3,"health":1,"stamina":1,"focus":1,"mana":1});
    }
    let mut game = game_with_build(actor, 8, 0, build_with_policy(authored), 10);
    if charge_regeneration {
        game = edit_checkpoint(&game, |snapshot, pool| {
            let actors = snapshot["actors"].as_u64().unwrap() as usize;
            let creature = &mut pool["actors"][actors][actor.0.to_string()]["combat"]["creature"];
            creature["injury"] = json!(3);
            creature["health_recovery"]["elapsed"] = json!(96);
        });
    }
    game.configure_arena_run(
        actor,
        BTreeSet::from([actor, other]),
        std::collections::BTreeMap::new(),
    )
    .unwrap();
    food_amount(&mut game, actor, 10, 1, rolled(1, 4, 3));
    let third = game
        .spawn_actor(at(2, 3), NonZeroU64::new(1).unwrap())
        .unwrap();
    let mut potion = ItemSpec::ordinary("late failure".into());
    potion.class = tor_simulation::ItemClass::Potion;
    potion.consumable = Some(tor_simulation::ConsumableSpec {
        effects: vec![Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: tor_simulation::combat::DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 1000000 },
                }],
            },
        }],
    });
    game.place_item_stack(11, at(2, 1), Some(other), 1, potion)
        .unwrap();
    game.admit_intention(actor, eat(10), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.admit_intention(
        other,
        Action::Drink { item: ItemId(11) },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    let actors = snapshot["actors"].as_u64().unwrap() as usize;
    let due = 10;
    pool["actors"][actors][third.0.to_string()]["ready_at"] = json!(due + 1);
    snapshot["combat"]["run"] = json!({"kind":"arena","state":{
        "limits":{"ticks":100000,"actions":10000},"started_at":game.tick(),"actions":0,
        "progress":{"kind":"active","control":{"kind":"paused","remaining":0}},"deceased":null
    }});
    let shared = serde_json::from_value(pool).unwrap();
    let restore = |snapshot: Value| {
        Game::restore_checkpoint(
            serde_json::from_value(snapshot).unwrap(),
            &shared,
            game.content().clone(),
        )
        .unwrap()
    };
    let mut control = restore(snapshot.clone());
    let initial_rng = rules_rng(&control);
    control.control_arena(false, 0).unwrap();
    assert!(control.alive(actor));
    assert!(!control.alive(other));
    assert_eq!(quantity(&control, actor, 10), None);
    if charge_regeneration {
        assert_eq!(
            nutrition(&control.creature(actor).unwrap())["regeneration_remainder"],
            1
        );
        assert_eq!(control.creature(actor).unwrap().health().injury(), 2);
    }
    assert_ne!(
        rules_rng(&control),
        initial_rng,
        "earlier Eat resolved before later death cleanup"
    );
    let next_item = snapshot["next_item_id"].clone();
    snapshot["next_item_id"] = json!(u64::MAX);
    let mut failed = restore(snapshot);
    let mut reopened = edit_checkpoint(&failed, |_, _| {});
    for state in [&mut failed, &mut reopened] {
        let before = state.clone();
        assert_eq!(
            state.control_arena(false, 0),
            Err(tor_simulation::GameError::IdentityExhausted)
        );
        assert_eq!(
            *state, before,
            "outer transaction restores RNG, items, holds, actor state, timers and events"
        );
        assert_eq!(quantity(state, actor, 10), Some(1));
        assert_eq!(rules_rng(state), initial_rng);
        let mut retry = edit_checkpoint(state, |snapshot, _| {
            snapshot["next_item_id"] = next_item.clone();
        });
        retry.control_arena(false, 0).unwrap();
        assert_eq!(
            retry, control,
            "removing only the induced allocator failure gives the clean result"
        );
    }
    assert_eq!(failed, reopened);
}

#[test]
fn eating_at_full_reserve_or_while_exempt_still_consumes_one_unit_and_one_roll() {
    use tor_simulation::dice::{DicePool, Edge};
    let actor = ActorId(1);
    for exempt in [false, true] {
        let mut game = game(actor, 12, 0);
        if exempt {
            game = edit_checkpoint(&game, |snapshot, pool| {
                let actors = snapshot["actors"].as_u64().unwrap() as usize;
                pool["actors"][actors]["1"]["combat"]["creature"]["nutrition"] =
                    json!({"kind":"exempt"});
            });
        }
        food_amount(&mut game, actor, 10, 1, rolled(1, 400, 1099));
        let mut expected_rng = rules_rng(&game);
        DicePool::new(1, 400, 1099)
            .unwrap()
            .roll(&mut expected_rng, Edge::default());
        game.admit_intention(actor, eat(10), IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        finish_eating(&mut game, actor);
        assert_eq!(quantity(&game, actor, 10), None);
        assert_eq!(rules_rng(&game), expected_rng);
        assert_eq!(
            nutrition(&game.creature(actor).unwrap()),
            if exempt {
                json!({"kind":"exempt"})
            } else {
                required(12, 10)
            }
        );
    }
}

#[test]
fn eat_accepts_exactly_one_positive_nourish_and_no_other_effects() {
    use tor_simulation::abilities::Plan;
    let meal = json!({"kind":"nourish","amount":rolled(1,4,3)});
    let heal = json!({"kind":"heal","amount":{"kind":"fixed","amount":1}});
    for effects in [
        json!([]),
        json!([heal]),
        json!([meal, meal]),
        json!([meal, heal]),
    ] {
        let plan: Plan = serde_json::from_value(json!({"kind":"eat","actor":1,"item":10,
            "preparation":10,"effects":effects}))
        .unwrap();
        assert!(!plan.valid(build().ruleset()));
    }
}

fn explicit_recovery_policy(
    hungry: RecoveryNutrition,
    weak: RecoveryNutrition,
    empty: RecoveryNutrition,
) -> Value {
    let mut value = policy();
    value["hungry_recovery"] = serde_json::to_value(hungry).unwrap();
    value["weak_recovery"] = serde_json::to_value(weak).unwrap();
    value["empty_recovery"] = serde_json::to_value(empty).unwrap();
    value
}

#[test]
fn explicit_nutrition_recovery_selects_each_authored_band_and_round_trips() {
    use tor_simulation::nutrition::NutritionRequirement;
    let choices = [
        RecoveryNutrition::Normal,
        RecoveryNutrition::Slowed { multiplier: 3 },
        RecoveryNutrition::Stopped,
    ];
    for hungry in choices {
        for weak in choices {
            for empty in choices {
                let policy = explicit_recovery_policy(hungry, weak, empty);
                let build = build_with_policy(policy.clone());
                let source = toml::to_string(build.ruleset().document()).unwrap();
                let decoded: RulesetDocument = toml::from_str(&source).unwrap();
                assert_eq!(serde_json::to_value(decoded.nutrition).unwrap(), policy);
                let mut creature = CreatureState::new(build).unwrap();
                creature
                    .configure_nutrition(NutritionRequirement::Required)
                    .unwrap();
                creature.damage(20);
                assert_eq!(
                    creature.health_recovery().nutrition(),
                    RecoveryNutrition::Normal
                );
                for (ticks, reserve, expected) in [
                    (200, 6, hungry),
                    (400, 2, weak),
                    (200, 0, empty),
                    (100, -1, empty),
                ] {
                    creature.advance_active(ticks);
                    assert_eq!(nutrition(&creature), required(reserve, 0));
                    assert_eq!(creature.health_recovery().nutrition(), expected);
                    let record = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
                    let restored = serde_json::from_value::<CreatureRecord>(record.clone())
                        .unwrap()
                        .restore(creature.build().content().clone(), vec![])
                        .unwrap();
                    assert_eq!(restored, creature);
                    let mut inconsistent = record;
                    inconsistent["health_recovery"]["nutrition"] =
                        if expected == RecoveryNutrition::Normal {
                            json!({"kind":"stopped"})
                        } else {
                            json!({"kind":"normal"})
                        };
                    assert!(serde_json::from_value::<CreatureRecord>(inconsistent)
                        .unwrap()
                        .restore(creature.build().content().clone(), vec![])
                        .is_err());
                }
            }
        }
    }
}

#[test]
fn explicit_nutrition_recovery_requires_all_three_strict_valid_named_policies() {
    use tor_simulation::nutrition::NutritionPolicy;
    let good = explicit_recovery_policy(
        RecoveryNutrition::Normal,
        RecoveryNutrition::Slowed { multiplier: 100 },
        RecoveryNutrition::Stopped,
    );
    assert!(serde_json::from_value::<NutritionPolicy>(good.clone())
        .unwrap()
        .valid());
    for field in ["hungry_recovery", "weak_recovery", "empty_recovery"] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<NutritionPolicy>(missing).is_err(),
            "{field}"
        );
        for invalid in [
            Value::Null,
            json!([]),
            json!({}),
            json!({"kind":"normal", "multiplier":2}),
            json!({"kind":"stopped", "extra":0}),
            json!({"kind":"slowed"}),
            json!({"kind":"slowed", "multiplier":0}),
            json!({"kind":"slowed", "multiplier":1}),
            json!({"kind":"slowed", "multiplier":101}),
            json!({"kind":"slowed", "multiplier":u64::MAX}),
            json!({"kind":"unknown"}),
        ] {
            let mut value = good.clone();
            value[field] = invalid;
            assert!(
                !serde_json::from_value::<NutritionPolicy>(value).is_ok_and(NutritionPolicy::valid),
                "{field}"
            );
        }
        let raw = serde_json::to_string(&good).unwrap();
        let duplicate = raw.replacen(
            &format!("\"{field}\":"),
            &format!("\"{field}\":{{\"kind\":\"normal\"}},\"{field}\":"),
            1,
        );
        assert!(
            serde_json::from_str::<NutritionPolicy>(&duplicate).is_err(),
            "{field}"
        );
        let mut value = good.clone();
        value[field] = json!({"kind":"slowed", "multiplier":2});
        let duplicate = serde_json::to_string(&value).unwrap().replacen(
            "\"multiplier\":2",
            "\"multiplier\":2,\"multiplier\":3",
            1,
        );
        assert!(
            serde_json::from_str::<NutritionPolicy>(&duplicate).is_err(),
            "{field}"
        );
    }
    let mut obsolete = good;
    obsolete["hungry_recovery_multiplier"] = json!(2);
    assert!(serde_json::from_value::<NutritionPolicy>(obsolete).is_err());
}

#[test]
fn neutral_nutrition_recovery_matches_exempt_until_starvation_and_exempt_keeps_its_hook() {
    use tor_simulation::nutrition::NutritionRequirement;
    let build = build_with_policy(explicit_recovery_policy(
        RecoveryNutrition::Normal,
        RecoveryNutrition::Normal,
        RecoveryNutrition::Normal,
    ));
    let mut required = CreatureState::new(build.clone()).unwrap();
    required
        .configure_nutrition(NutritionRequirement::Required)
        .unwrap();
    let mut exempt = CreatureState::new(build.clone()).unwrap();
    required.damage(20);
    exempt.damage(20);
    for ticks in [200, 400, 200, 299] {
        required.advance_active(ticks);
        exempt.advance_active(ticks);
        assert_eq!(required.health(), exempt.health());
        assert_eq!(required.health_recovery(), exempt.health_recovery());
        assert_eq!(nutrition(&exempt), json!({"kind":"exempt"}));
    }
    required.advance_active(1);
    exempt.advance_active(1);
    assert!(required.health().dead());
    assert!(!exempt.health().dead());
    for recovery in [
        RecoveryNutrition::Normal,
        RecoveryNutrition::Slowed { multiplier: 3 },
        RecoveryNutrition::Stopped,
    ] {
        let mut exempt = CreatureState::new(build.clone()).unwrap();
        exempt.damage(20);
        exempt.set_recovery_nutrition(recovery).unwrap();
        exempt.advance_active(800);
        assert_eq!(exempt.health_recovery().nutrition(), recovery);
        assert_eq!(nutrition(&exempt), json!({"kind":"exempt"}));
    }
}

#[test]
fn explicit_nutrition_recovery_rephases_once_at_each_simultaneous_condition_boundary() {
    use tor_simulation::nutrition::NutritionRequirement;
    for (reserve, hungry, weak, empty, period, elapsed) in [
        (
            7,
            RecoveryNutrition::Stopped,
            RecoveryNutrition::Normal,
            RecoveryNutrition::Normal,
            101,
            67,
        ),
        (
            3,
            RecoveryNutrition::Stopped,
            RecoveryNutrition::Slowed { multiplier: 3 },
            RecoveryNutrition::Normal,
            303,
            101,
        ),
        (
            1,
            RecoveryNutrition::Normal,
            RecoveryNutrition::Slowed { multiplier: 3 },
            RecoveryNutrition::Slowed { multiplier: 2 },
            202,
            44,
        ),
    ] {
        let original = expiry_build(json!("always"));
        let mut document = serde_json::to_value(original.ruleset().document()).unwrap();
        let mut policy = explicit_recovery_policy(hungry, weak, empty);
        policy["initial"] = json!(reserve);
        document["nutrition"] = policy;
        let rules = Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
        let content = Content::new(rules, original.content().catalog().clone()).unwrap();
        let build = CreatureBuild::new(
            content,
            original.species_id().clone(),
            original.ledger().clone(),
            original.binding(),
            *original.starting_skills(),
        )
        .unwrap();
        let mut creature = CreatureState::new(build).unwrap();
        creature
            .configure_nutrition(NutritionRequirement::Required)
            .unwrap();
        creature
            .apply_condition(
                &tor_simulation::content_ids::ConditionId::new("nutrition_expiry_probe").unwrap(),
                ActorId(1),
                1,
            )
            .unwrap();
        creature.damage(20);
        let mut record = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
        record["nutrition"]["elapsed"] = json!(99);
        record["health_recovery"]["elapsed"] = json!(1);
        let original = serde_json::from_value::<CreatureRecord>(record)
            .unwrap()
            .restore(creature.build().content().clone(), vec![])
            .unwrap();
        let mut creature = original.clone();
        creature.advance_active(1);
        assert_eq!(
            creature.health_recovery().period(),
            period,
            "reserve {reserve}"
        );
        assert_eq!(
            creature.health_recovery().elapsed(),
            elapsed,
            "reserve {reserve}"
        );
        for ticks in [1, 2, 100, 299] {
            let mut bulk = original.clone();
            let mut incremental = original.clone();
            bulk.advance_active(ticks);
            for _ in 0..ticks {
                incremental.advance_active(1);
            }
            assert_eq!(bulk, incremental, "reserve {reserve}, ticks {ticks}");
        }
    }
}
