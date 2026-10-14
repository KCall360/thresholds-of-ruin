//! Authored stochastic depletion contracts. Pure creature projections never roll.
use serde_json::{json, Value};
use std::num::NonZeroU64;
use tor_simulation::{
    checkpoint::SharedState,
    content_ids::ConditionId,
    creatures::{record::CreatureRecord, Content, CreatureBuild},
    nutrition::NutritionRequirement,
    rules::{Ruleset, RulesetDocument},
    Action, ActorId, CreatureIdentity, Game,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

const ZERO_WORD_SEED: u64 = 0x61c8_8646_80b5_83eb;
const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

fn policy() -> Value {
    json!({"kind":"enabled","initial":0,"maximum":2000,"hungry_below":300,
        "weak_below":150,"starve_at":-852,"unit_ticks":100,
        "hungry_recovery":{"kind":"normal"},"weak_recovery":{"kind":"normal"},
        "empty_recovery":{"kind":"normal"}})
}
fn trigger() -> Value {
    json!({"kind":"on_empty_depletion","condition":"fixture_stupor",
        "chance":{"one_in":5},"duration":{"kind":"rolled","dice":{"count":1,"sides":8,"bonus":3}},
        "ticks_per_unit":100})
}
fn document() -> Value {
    let mut value = serde_json::to_value(super::creature_fixture::ruleset().document()).unwrap();
    value["nutrition"] = policy();
    value["nutrition_condition_trigger"] = trigger();
    value["conditions"].as_array_mut().unwrap().extend([
        super::status_conditions::definition("fixture_stupor", None),
        super::status_conditions::definition("fixture_ice", Some(400)),
    ]);
    let mut held = super::status_conditions::definition("fixture_roots", None);
    held["restrictions"] = json!(["ordinary_movement", "movement_abilities"]);
    value["conditions"].as_array_mut().unwrap().push(held);
    value
}
fn build(document: Value) -> CreatureBuild {
    let original = super::creature_fixture::build(super::creature_fixture::species());
    let rules =
        Ruleset::validate(serde_json::from_value(document).expect("authored depletion schema"))
            .unwrap();
    CreatureBuild::new(
        Content::new(rules, original.content().catalog().clone()).unwrap(),
        original.species_id().clone(),
        original.ledger().clone(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap()
}
fn at(id: u64) -> Location {
    Location {
        region: RegionId(id),
        position: Position { x: 1, y: 1, z: 0 },
    }
}
fn game(seed: u64, reserve: i32, elapsed: u64) -> Game {
    game_with_document(seed, reserve, elapsed, document())
}
fn game_with_document(seed: u64, reserve: i32, elapsed: u64, document: Value) -> Game {
    let build = build(document);
    let world = World::new(
        (1..=3)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room {id}"),
                bounds: Extent::new(3, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, seed, build.content().clone());
    for id in 1..=3 {
        let actor = game
            .spawn_actor(at(id), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {id}"),
                faction: "allies".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    // A nonplayer participates through exactly the same explicit enrollment API.
    game.configure_nutrition(ActorId(2), NutritionRequirement::Required)
        .unwrap();
    game.refresh_navigation();
    edit(&game, |snapshot, pool| {
        snapshot["rules_rng"] = json!(seed);
        let table = snapshot["actors"].as_u64().unwrap() as usize;
        pool["actors"][table]["2"]["combat"]["creature"]["nutrition"] =
            json!({"kind":"required","reserve":reserve,"elapsed":elapsed});
    })
}
fn edit(game: &Game, change: impl FnOnce(&mut Value, &mut Value)) -> Game {
    let mut shared = SharedState::default();
    let mut snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    change(&mut snapshot, &mut pool);
    Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &serde_json::from_value(pool).unwrap(),
        game.content().clone(),
    )
    .unwrap()
}
fn rng(game: &Game) -> u64 {
    serde_json::to_value(game.checkpoint(&mut SharedState::default())).unwrap()["rules_rng"]
        .as_u64()
        .unwrap()
}
fn record(game: &Game, actor: u64) -> Value {
    serde_json::to_value(CreatureRecord::capture(
        &game.creature(ActorId(actor)).unwrap(),
    ))
    .unwrap()
}
fn until(game: &mut Game, tick: u64) {
    while game.tick() < tick {
        game.act(
            game.next_actor().expect("exempt clock driver"),
            Action::Wait,
        )
        .unwrap();
    }
    assert_eq!(game.tick(), tick);
}

#[test]
fn first_empty_depletion_has_exact_draw_order_and_queries_never_sample() {
    let mut game = game(ZERO_WORD_SEED, 1, 0);
    until(&mut game, 100);
    assert_eq!(record(&game, 2)["nutrition"]["reserve"], 0);
    assert_eq!(rng(&game), ZERO_WORD_SEED, "1 to 0 does not check");
    assert_eq!(game.next_creature_event_tick(), Some(200));
    let before = game.clone();
    for _ in 0..3 {
        game.observe(ActorId(2)).unwrap();
        let mut projection = game.creature(ActorId(2)).unwrap().as_ref().clone();
        projection.advance_active(10_000);
        let restored = edit(&game, |_, _| {});
        assert_eq!(restored, game);
        assert_eq!(game, before);
    }
    until(&mut game, 200);
    assert_eq!(
        rng(&game),
        GAMMA,
        "chance then duration, two draws from the zero-word anchor"
    );
    let conditions = record(&game, 2)["conditions"].clone();
    assert_eq!(conditions[0]["condition"], "fixture_stupor");
    assert_eq!(conditions[0]["remaining"], 1100);
    assert_eq!(conditions[0]["phase"], "active");
    assert_eq!(conditions[0]["source"], 2);
    assert!(game.observe(ActorId(2)).unwrap().combat.unwrap().events.iter().any(|event|
        matches!(event, tor_simulation::combat::DisclosedCombatEvent::Condition { id, .. }
            if id.as_str() == "fixture_stupor")));
    assert!(game
        .observe(ActorId(1))
        .unwrap()
        .combat
        .unwrap()
        .events
        .iter()
        .all(|event| !matches!(
            event,
            tor_simulation::combat::DisclosedCombatEvent::Condition { .. }
        )));
}

#[test]
fn failed_chance_uses_one_draw_and_starvation_uses_none() {
    let mut game = game(42, 0, 0);
    until(&mut game, 100);
    assert_eq!(rng(&game), GAMMA.wrapping_add(42));
    assert!(record(&game, 2)["conditions"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut fatal = self::game(ZERO_WORD_SEED, -851, 0);
    until(&mut fatal, 100);
    assert!(!fatal.alive(ActorId(2)));
    assert_eq!(
        rng(&fatal),
        ZERO_WORD_SEED,
        "death precedes chance and duration"
    );
    let mut survivor = self::game(ZERO_WORD_SEED, -850, 0);
    until(&mut survivor, 100);
    assert!(survivor.alive(ActorId(2)));
    assert_eq!(record(&survivor, 2)["nutrition"]["reserve"], -851);
    assert_eq!(rng(&survivor), GAMMA);
}

#[test]
fn denied_intervals_skip_draws_but_expiry_ties_and_protection_are_eligible() {
    for duration in [250, 300] {
        let mut game = game(ZERO_WORD_SEED, 0, 0);
        game.apply_condition(
            ActorId(2),
            &ConditionId::new("fixture_ice").unwrap(),
            ActorId(1),
            duration,
        )
        .unwrap();
        until(&mut game, 200);
        assert_eq!(rng(&game), ZERO_WORD_SEED);
        until(&mut game, 300);
        assert_eq!(
            rng(&game),
            GAMMA,
            "first eligible depletion preserves phase"
        );
        assert_eq!(record(&game, 2)["nutrition"]["reserve"], -3);
        let state = record(&game, 2);
        assert!(state["conditions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["condition"] == "fixture_ice" && entry["phase"] == "protection"));
    }
    let mut rooted = game(ZERO_WORD_SEED, 0, 0);
    rooted
        .apply_condition(
            ActorId(2),
            &ConditionId::new("fixture_roots").unwrap(),
            ActorId(1),
            1000,
        )
        .unwrap();
    until(&mut rooted, 100);
    assert_eq!(
        rng(&rooted),
        GAMMA,
        "ordinary movement restrictions do not suppress checks"
    );
}

#[test]
fn trigger_policy_is_explicit_strict_bounded_and_resolves_condition_identity() {
    let valid = document();
    for (path, value) in [
        ("/nutrition_condition_trigger", json!(null)),
        ("/nutrition_condition_trigger", json!(["disabled"])),
        (
            "/nutrition_condition_trigger",
            json!({"kind":"disabled","extra":0}),
        ),
        ("/nutrition_condition_trigger/condition", json!("absent")),
        ("/nutrition_condition_trigger/chance", json!({"one_in":0})),
        (
            "/nutrition_condition_trigger/chance",
            json!({"one_in":5,"extra":0}),
        ),
        (
            "/nutrition_condition_trigger/duration",
            json!({"kind":"fixed","amount":0}),
        ),
        ("/nutrition_condition_trigger/ticks_per_unit", json!(0)),
        (
            "/nutrition_condition_trigger/ticks_per_unit",
            json!(u64::MAX),
        ),
        ("/nutrition/starve_at", json!(-1026)),
        ("/nutrition", json!({"kind":"disabled"})),
    ] {
        let mut bad = valid.clone();
        *bad.pointer_mut(path).unwrap() = value;
        let parsed = serde_json::from_value::<RulesetDocument>(bad);
        assert!(
            parsed.is_err() || Ruleset::validate(parsed.unwrap()).is_err(),
            "{path}"
        );
    }
    let mut missing = valid;
    missing
        .as_object_mut()
        .unwrap()
        .remove("nutrition_condition_trigger");
    assert!(serde_json::from_value::<RulesetDocument>(missing).is_err());
}

#[test]
fn simultaneous_enrolled_actors_resolve_in_stable_actor_order() {
    let mut game = game(ZERO_WORD_SEED, 0, 0);
    game.configure_nutrition(ActorId(3), NutritionRequirement::Required)
        .unwrap();
    until(&mut game, 100);
    let effects: Vec<_> = game
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            tor_simulation::combat::CombatEvent::Resolved { outcome, .. }
                if outcome.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        tor_simulation::abilities::ResolvedEffect::ApplyCondition { .. }
                    )
                }) =>
            {
                Some(outcome.actor)
            }
            _ => None,
        })
        .collect();
    assert_eq!(effects, vec![ActorId(2), ActorId(3)]);
    assert_eq!(record(&game, 2)["conditions"][0]["remaining"], 1100);
    assert_eq!(record(&game, 3)["conditions"][0]["remaining"], 1100);
    assert_eq!(
        rng(&game),
        GAMMA.wrapping_mul(3),
        "two chance/duration pairs in ActorId order"
    );
    assert_eq!(record(&game, 1)["nutrition"], json!({"kind":"exempt"}));
}

#[test]
fn active_denial_skips_a_whole_empty_episode_without_owed_rolls() {
    let mut game = game(ZERO_WORD_SEED, 0, 0);
    game.apply_condition(
        ActorId(2),
        &ConditionId::new("fixture_ice").unwrap(),
        ActorId(1),
        100_000,
    )
    .unwrap();
    until(&mut game, 85_200);
    assert!(!game.alive(ActorId(2)));
    assert_eq!(rng(&game), ZERO_WORD_SEED);
}

#[test]
fn required_rebuild_cannot_change_the_trigger_policy() {
    let mut creature = tor_simulation::creatures::CreatureState::new(build(document())).unwrap();
    creature
        .configure_nutrition(NutritionRequirement::Required)
        .unwrap();
    for changed in [json!({"kind":"disabled"}), {
        let mut changed = trigger();
        changed["ticks_per_unit"] = json!(99);
        changed
    }] {
        let mut modified = document();
        modified["nutrition_condition_trigger"] = changed;
        let before = creature.clone();
        assert!(creature.rebuild(build(modified)).is_err());
        assert_eq!(creature, before);
    }
}

fn meal(game: &mut Game) {
    let mut spec = tor_simulation::ItemSpec::ordinary("ration".into());
    spec.class = tor_simulation::ItemClass::Food;
    spec.stackable = true;
    spec.supply = Some(tor_simulation::content_ids::SupplyId::new("ration").unwrap());
    spec.edible = Some(tor_simulation::EdibleSpec {
        nutrition: tor_simulation::rules::AmountExpr::Fixed { amount: 1500 },
    });
    game.place_item_stack(10, at(2), Some(ActorId(2)), 1, spec)
        .unwrap();
}
fn start_meal(game: &mut Game) -> tor_simulation::IntentionId {
    let intention = game
        .admit_intention(
            ActorId(2),
            Action::Eat {
                item: tor_simulation::ItemId(10),
            },
            tor_simulation::IntentionOrigin::Human,
        )
        .unwrap();
    while game.next_intention_actor() != Some(ActorId(2)) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.execute_next_intention().unwrap().outcome.unwrap();
    intention
}

#[test]
fn successful_faint_precedes_eat_completion_and_retains_exact_paid_work_until_resume() {
    let mut game = game(ZERO_WORD_SEED, 0, 0);
    meal(&mut game);
    let intention = start_meal(&mut game);
    let plan = game.preparation(ActorId(2)).unwrap().plan.clone();
    until(&mut game, 100);
    let pending = game
        .preparation(ActorId(2))
        .expect("faint pauses rather than consumes");
    assert!(!pending.active);
    assert_eq!(pending.remaining, 0, "timer precedes exact completion tie");
    assert_eq!(pending.plan, plan);
    assert_eq!(pending.origin_intention(), Some(intention));
    let mut restored = edit(&game, |_, _| {});
    let mut replaced = game.clone();
    assert!(replaced.cancel_intention(ActorId(2), intention).is_err());
    assert_eq!(replaced, game, "item work has no explicit cancellation");
    for state in [&mut game, &mut restored] {
        // Faint expiry can itself coincide with a new eligible depletion. Wait
        // for the first actual permitted decision, bounded by the finite episode.
        for _ in 0..851 {
            if state.creature(ActorId(2)).unwrap().conditions().is_empty() {
                break;
            }
            let next = state.tick() + 100;
            until(state, next);
        }
        assert!(state.alive(ActorId(2)));
        assert!(
            !state.preparation(ActorId(2)).unwrap().active,
            "expiry never auto-resumes"
        );
        let before = rng(state);
        state.resume_intention(ActorId(2), intention).unwrap();
        while state.next_intention_actor() != Some(ActorId(2)) {
            state
                .act(state.next_actor().unwrap(), Action::Wait)
                .unwrap();
        }
        state.execute_next_intention().unwrap().outcome.unwrap();
        assert!(state.preparation(ActorId(2)).is_none());
        assert_eq!(record(state, 2)["nutrition"]["reserve"], 1500);
        assert_eq!(rng(state), before, "fixed held nourishment never samples");
        assert!(state
            .observe(ActorId(2))
            .unwrap()
            .inventory
            .iter()
            .all(|item| item.id != tor_simulation::ItemId(10)));
    }
    assert_eq!(game, restored);
    for _ in 0..851 {
        if replaced
            .creature(ActorId(2))
            .unwrap()
            .conditions()
            .is_empty()
        {
            break;
        }
        let next = replaced.tick() + 100;
        until(&mut replaced, next);
    }
    while replaced.next_actor() != Some(ActorId(2)) {
        replaced
            .act(replaced.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    replaced
        .act(ActorId(2), Action::Move(tor_world::Direction::East))
        .unwrap();
    assert!(replaced.preparation(ActorId(2)).is_none());
    assert!(replaced
        .observe(ActorId(2))
        .unwrap()
        .inventory
        .iter()
        .any(|item| item.id == tor_simulation::ItemId(10) && item.quantity == 1));
    assert!(
        record(&replaced, 2)["nutrition"]["reserve"]
            .as_i64()
            .unwrap()
            < 0
    );
}

#[test]
fn trigger_horizon_failure_rolls_back_time_rng_work_holds_and_events() {
    let base = game(ZERO_WORD_SEED, 0, 0);
    let mut failed = edit(&base, |snapshot, pool| {
        let tick = u64::MAX - 200;
        snapshot["tick"] = json!(tick);
        let table = snapshot["actors"].as_u64().unwrap() as usize;
        for actor in pool["actors"][table].as_object_mut().unwrap().values_mut() {
            actor["ready_at"] = json!(tick);
            actor["active_since"] = json!(tick);
        }
    });
    meal(&mut failed);
    start_meal(&mut failed);
    let before = failed.clone();
    assert_eq!(
        failed.act(ActorId(3), Action::Wait),
        Err(tor_simulation::GameError::Schedule(
            tor_simulation::ScheduleError::TimeExhausted
        ))
    );
    assert_eq!(failed, before);
    assert_eq!(rng(&failed), ZERO_WORD_SEED);
}

#[test]
fn personal_status_projection_hides_causers_tokens_and_foreign_conditions() {
    let mut game = game(ZERO_WORD_SEED, 0, 0);
    game.apply_condition(
        ActorId(2),
        &ConditionId::new("fixture_ice").unwrap(),
        ActorId(1),
        200,
    )
    .unwrap();
    let subject = game
        .observe(ActorId(2))
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert_eq!(subject.conditions.len(), 1);
    assert_eq!(subject.conditions[0].condition.as_str(), "fixture_ice");
    assert_eq!(
        subject.conditions[0].phase,
        tor_simulation::conditions::ConditionPhase::Active
    );
    let other = game
        .observe(ActorId(1))
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert!(other.conditions.is_empty());
    until(&mut game, 200);
    let subject = game
        .observe(ActorId(2))
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    assert!(subject
        .conditions
        .iter()
        .any(|condition| condition.condition.as_str() == "fixture_ice"
            && condition.phase == tor_simulation::conditions::ConditionPhase::Protection));
    let other = game.observe(ActorId(1)).unwrap().combat.unwrap();
    assert!(other.events.iter().all(|event| !matches!(
        event,
        tor_simulation::combat::DisclosedCombatEvent::Condition { .. }
    )));
}

#[test]
fn moving_physics_keeps_depletion_expiry_and_death_on_the_same_authoritative_boundary() {
    for (reserve, denial) in [(0, None), (0, Some(100)), (-851, None), (-851, Some(100))] {
        let mut stationary = game(ZERO_WORD_SEED, reserve, 0);
        if let Some(duration) = denial {
            stationary
                .apply_condition(
                    ActorId(2),
                    &ConditionId::new("fixture_ice").unwrap(),
                    ActorId(1),
                    duration,
                )
                .unwrap();
        }
        let mut moving = stationary.clone();
        // Fractional motion remains active without crossing a cell or impact in
        // this 100-tick interval; it exercises the actual per-tick physics path.
        moving.set_actor_velocity(ActorId(2), [1, 0, 0]).unwrap();
        until(&mut stationary, 100);
        until(&mut moving, 100);
        assert_eq!(rng(&moving), rng(&stationary));
        assert_eq!(record(&moving, 2), record(&stationary, 2));
        assert_eq!(moving.alive(ActorId(2)), stationary.alive(ActorId(2)));
        assert_eq!(moving.combat_events(), stationary.combat_events());
    }
}

#[test]
fn frozen_and_detached_empty_episodes_preserve_phase_without_catchup_draws() {
    use std::collections::BTreeSet;
    use tor_simulation::{MemoryRecords, RegionTransition};
    for detach in [false, true] {
        let mut game = game(ZERO_WORD_SEED, 0, 57);
        let mut memories = MemoryRecords::default();
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(3)]),
                loaded: if detach {
                    BTreeSet::from([RegionId(1), RegionId(3)])
                } else {
                    BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)])
                },
            },
            &mut memories,
        )
        .unwrap();
        until(&mut game, 500);
        assert_eq!(rng(&game), ZERO_WORD_SEED);
        game = edit(&game, |_, _| {});
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
            },
            &mut memories,
        )
        .unwrap();
        assert_eq!(game.next_creature_event_tick(), Some(543));
        assert_eq!(
            record(&game, 2)["nutrition"],
            json!({"kind":"required","reserve":0,"elapsed":57})
        );
        assert_eq!(rng(&game), ZERO_WORD_SEED);
        until(&mut game, 600);
        assert_eq!(rng(&game), GAMMA);
        assert_eq!(record(&game, 2)["conditions"][0]["remaining"], 1043);
    }
}

#[test]
fn full_reject_condition_book_rolls_back_trigger_time_rng_paid_meal_and_events() {
    let mut document = document();
    for index in 0..128 {
        let mut filler =
            super::status_conditions::definition(&format!("fixture_filler_{index}"), None);
        filler["restrictions"] = json!([]);
        filler["preparation"] = json!("preserve");
        document["conditions"].as_array_mut().unwrap().push(filler);
    }
    let mut game = game_with_document(ZERO_WORD_SEED, 0, 0, document);
    for index in 0..128 {
        game.apply_condition(
            ActorId(2),
            &ConditionId::new(format!("fixture_filler_{index}")).unwrap(),
            ActorId(1),
            10_000,
        )
        .unwrap();
    }
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .count(),
        128
    );
    meal(&mut game);
    start_meal(&mut game);
    let before = game.clone();
    assert_eq!(
        game.act(ActorId(3), Action::Wait),
        Err(tor_simulation::GameError::Rule(
            tor_simulation::abilities::RuleError::ConditionCapacity
        ))
    );
    assert_eq!(
        game, before,
        "failed automatic application commits no time, RNG, hold, work or event"
    );
    assert_eq!(rng(&game), ZERO_WORD_SEED);
    assert_eq!(edit(&game, |_, _| {}), before);
}

#[test]
fn immune_and_kept_trigger_applications_are_successful_noops_not_capacity_errors() {
    for (condition, duration, expected_phase, expected_remaining) in [
        ("fixture_ice", 1, "protection", 301),
        ("fixture_roots", 1000, "active", 900),
    ] {
        let mut document = document();
        document["nutrition_condition_trigger"]["condition"] = json!(condition);
        let mut game = game_with_document(ZERO_WORD_SEED, 0, 0, document);
        game.apply_condition(
            ActorId(2),
            &ConditionId::new(condition).unwrap(),
            ActorId(1),
            duration,
        )
        .unwrap();
        until(&mut game, 100);
        assert_eq!(
            rng(&game),
            GAMMA,
            "successful chance still samples its duration before immunity or Keep"
        );
        let state = record(&game, 2);
        let conditions = state["conditions"].as_array().unwrap();
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0]["phase"], expected_phase);
        assert_eq!(conditions[0]["remaining"], expected_remaining);
        assert!(game
            .observe(ActorId(2))
            .unwrap()
            .combat
            .unwrap()
            .events
            .iter()
            .all(|event| !matches!(
                event,
                tor_simulation::combat::DisclosedCombatEvent::Condition { .. }
            )));
    }
}
