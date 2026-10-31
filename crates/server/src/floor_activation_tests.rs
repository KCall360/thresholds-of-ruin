//! First-red coverage for authoritative entry population through ordinary Engine APIs.
//! The proposed serialized floor_activation definition is intentionally not supported
//! yet. The first checkpoint must fail at that semantic configuration boundary,
//! not because a future Rust type or method is missing. Once admitted, these cases
//! assert the committed gameplay, timing and independent persistence contracts.
use super::*;
use crate::scenario_package::{Manifest, Package};
use serde_json::{json, Value};
use tor_world::{Location, Position as WorldPosition, RegionId};

#[path = "theft_residency_tests.rs"]
mod theft_residency;

#[path = "wizard_creature_spawn_tests.rs"]
mod named_spawning;

#[path = "floor_activation_checkpoint_tests.rs"]
mod checkpoints;

#[path = "floor_population_depth_tests.rs"]
mod depth;

#[path = "room_cluster_tests.rs"]
mod room_cluster;

#[path = "spawn_carried_stock_tests.rs"]
mod carried_stock;
#[path = "floor_population_growth_tests.rs"]
mod growth;

#[path = "idle_autonomy_tests.rs"]
mod idle_autonomy;

#[path = "hobgoblin_roaming_tests.rs"]
mod hobgoblin_roaming;

#[path = "deep_population_atomicity_tests.rs"]
mod deep_atomicity;

#[path = "unique_retrieval_tests.rs"]
mod retrieval;

fn fixture_with_activation(activation: bool) -> Scenario {
    // This activation-only witness deliberately uses the retained feature-free
    // geometry package, not the evolving production discovery/trap recipe.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
    let template = crate::scenario_package::resolve_source(&root).unwrap();
    let mut source = serde_json::to_value(&template.manifest).unwrap();
    source["id"] = json!("floor-activation");
    // An explicit stripped geometry fixture keeps this contract independent of
    // the real Rogue package's evolving starter, foes and survival tuning.
    source["default_character"] = json!(1);
    source["characters"] = json!([{"id":1,"anchor":"dungeon/1/start","turn_ticks":100}]);
    source["creatures"] = json!({});
    source["capabilities"] = Value::Null;
    source["factions"] = json!({});
    source["ai_profiles"] = json!({});
    source["archetypes"] = json!({"marker":{"name":"activation marker"}});
    source["objective"] = Value::Null;
    source["unique_retrieval"] = Value::Null;
    source["arena"] = Value::Null;
    // Deliberately no Amulet, hunger policy, difficulty bands, or wandering.
    source["floor_activation"] = json!({
        "version":1,
        "protagonist":1,
        "families":{"dungeon":"baseline"},
        "profiles":{"baseline":{"version":1,"stages":[
            {"id":"marker","version":1,"kind":"items",
             "count":[1,1],"bands":[{"depth":{"kind":"open","minimum":1},"choices":[{"archetype":"marker","weight":1}]}]},
            {"id":"later","version":1,"kind":"items",
             "count":[1,1],"bands":[{"depth":{"kind":"open","minimum":3},"choices":[{"archetype":"marker","weight":1}]}]}
        ]}}
    });
    if !activation {
        source.as_object_mut().unwrap().remove("floor_activation");
    }
    let manifest: Manifest = serde_json::from_value(source)
        .expect("first red: admit the bounded typed floor-activation definition");
    let package = Package::from_parts(
        manifest,
        vec![],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    Scenario {
        seed: 42,
        actors: vec![],
        regions: 0,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: Some(crate::regions::Streaming {
            active_radius: 0,
            load_radius: 0,
        }),
    }
}

fn fixture() -> Scenario {
    fixture_with_activation(true)
}

// One member per floor makes the spawned creature's region necessarily active,
// avoiding an accidental all-frozen population as the scheduler control.
fn single_member_foe_fixture(activation: bool) -> Scenario {
    let mut scenario = fixture_with_activation(activation);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    let creatures = crate::scenario_package::resolve_source(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler"),
    )
    .unwrap();
    source["creatures"] = serde_json::to_value(&creatures.manifest.creatures).unwrap();
    source["factions"] = serde_json::to_value(&creatures.manifest.factions).unwrap();
    source["ai_profiles"] = serde_json::to_value(&creatures.manifest.ai_profiles).unwrap();
    source["assets"] = serde_json::to_value(&creatures.manifest.assets).unwrap();
    // The borrowed palette names must be declared even though this stripped
    // geometry fixture does not import the catalog scenario's region zones.
    source["themes"] =
        serde_json::to_value(creatures.manifest.assets.keys().collect::<Vec<_>>()).unwrap();
    source["archetypes"]["rat"] =
        serde_json::to_value(&creatures.manifest.archetypes["rat"]).unwrap();
    source["procedural_worlds"]["dungeon"]["grid"] = json!([1, 1]);
    source["characters"].as_array_mut().unwrap().push(json!({
        "id":2,"anchor":"dungeon/1/start","turn_ticks":100,"unselected":"omit"
    }));
    if activation {
        source["floor_activation"]["profiles"]["baseline"]["stages"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "id":"foe","version":1,"kind":"actors",
                "ai":"wander","count":[1,1],"bands":[{"depth":{"kind":"open","minimum":2},
                "choices":[{"archetype":"rat","weight":1}]}]
            }));
    }
    let package = Package::from_parts(
        serde_json::from_value(source).unwrap(),
        vec![],
        &original.rules_source,
    )
    .unwrap();
    scenario.package = Some(Arc::new(package));
    scenario
}

// The normal load horizon publishes far floor members without making them
// active. More than one room's worth of independent attempts exercises births
// in those frozen members; the explicit precondition below prevents a vacuous
// all-active placement from passing this regression.
fn multi_member_foe_fixture(activation: bool) -> Scenario {
    let mut scenario = single_member_foe_fixture(activation);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["procedural_worlds"]["dungeon"]["grid"] = json!([3, 3]);
    if activation {
        let stages = source["floor_activation"]["profiles"]["baseline"]["stages"]
            .as_array_mut()
            .unwrap();
        stages[0]["count"] = json!([16, 16]);
        stages[2]["count"] = json!([16, 16]);
    }
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    scenario.streaming = Some(crate::regions::Streaming {
        active_radius: 0,
        load_radius: 6,
    });
    scenario
}

fn authority(engine: &Engine) -> Value {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let game = engine.game.checkpoint(&mut shared);
    json!({"game":game,"shared":shared})
}

fn activation(game: &Value, shared: &Value) -> Value {
    let reference = &game["floor_activation"];
    let index = reference["state"]
        .as_u64()
        .expect("activation must have an authoritative persisted pool reference");
    let mut activation = shared["floor_activations"][index as usize].clone();
    assert!(activation.is_object());
    assert!(activation["wandering"].is_null());
    activation["wandering"] = reference
        .get("wandering")
        .expect("the complete boundary clock authority is required")
        .clone();
    activation
}

fn decisions(engine: &Engine) -> Value {
    let state = authority(engine);
    let activation = activation(&state["game"], &state["shared"]);
    json!({"floors": activation["floors"], "order": activation["order"],
           "decision_count": activation["decision_count"]})
}

fn floor(engine: &Engine, depth: u32) -> Value {
    let state = decisions(engine);
    state["floors"][format!("dungeon/{depth}")]
        .as_object()
        .expect("a committed first entry must save its exact floor decision");
    state["floors"][format!("dungeon/{depth}")].clone()
}

fn exact_equal(actual: &Engine, expected: &Engine) {
    assert_eq!(
        actual.game, expected.game,
        "all gameplay state, clocks, IDs, RNG and queue"
    );
    assert_eq!(actual.archive.records.len(), expected.archive.records.len());
    assert_eq!(actual.receipts, expected.receipts);
    assert_eq!(actual.current_branch, expected.current_branch);
    assert_eq!(
        serde_json::to_value(&actual.archive).unwrap(),
        serde_json::to_value(&expected.archive).unwrap()
    );
    assert_eq!(actual.boundaries.len(), expected.boundaries.len());
    for (a, b) in actual.boundaries.iter().zip(&expected.boundaries) {
        assert_eq!(
            (&a.id, a.selectable, &a.game),
            (&b.id, b.selectable, &b.game)
        );
    }
}

fn restored_both_ways(engine: &Engine) {
    exact_equal(
        &Engine::replay(engine.archive.clone(), None, None).unwrap(),
        engine,
    );
    let disk =
        Checkpoint::capture(engine).encode("activation-save", engine.archive.records.len() as u64);
    exact_equal(&disk.restore(engine.archive.clone()).unwrap(), engine);
}

fn admit(engine: &mut Engine, request: &str, direction: Direction) -> CommandResult {
    engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            request,
            &engine.branch().clone(),
            Command::AdmitIntention {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::Move { direction },
            },
        )
        .unwrap()
}

fn step(engine: &mut Engine, direction: Direction) {
    admit(engine, &uuid::Uuid::new_v4().to_string(), direction);
    let result = engine.execute_next_intention().unwrap().unwrap();
    assert!(
        matches!(
            result.entry.content,
            JournalContent::IntentionStarted { .. }
        ),
        "fixture step must execute normally, not merely be queued"
    );
}

// Engine-layer setup route over the fixed generated terrain, following the
// established generation_groups.rs pattern. Actual-process coverage instead uses
// process_harness.explore_to_stairs and only disclosed client cells.
fn walk_to_port(engine: &mut Engine, depth: u32, port: &str) {
    let package = engine.archive.scenario.package.as_ref().unwrap();
    let family = &package.manifest.procedural_worlds["dungeon"];
    let identity = format!("dungeon/{depth}");
    let group = family.group("dungeon", 42, depth).unwrap();
    let defs = crate::generation_recipe::materialize(
        &identity,
        &group,
        &package.manifest.generation_recipes[&group.recipe],
        group
            .members
            .iter()
            .map(|id| family.describe("dungeon", 42, *id).unwrap().unwrap())
            .collect(),
        42,
    )
    .unwrap();
    let first = group.members[0];
    let coordinate = |at: Location| {
        let slot = (at.region.0 - first) as i32;
        (at.position.x + slot % 3 * 26, at.position.y + slot / 3 * 7)
    };
    let (owner, at) = defs
        .regions
        .iter()
        .find_map(|(id, def)| def.anchors.get(port).map(|at| (*id, *at)))
        .unwrap();
    let target = coordinate(Location {
        region: RegionId(owner),
        position: WorldPosition {
            x: at[0],
            y: at[1],
            z: at[2],
        },
    });
    let start = coordinate(engine.game.observe(SimActor(1)).unwrap().location);
    let mut open = BTreeSet::new();
    for (id, def) in &defs.regions {
        for y in 0..7 {
            for x in 0..26 {
                if !def.walls.contains(&[x, y, 0]) {
                    open.insert(coordinate(Location {
                        region: RegionId(*id),
                        position: WorldPosition { x, y, z: 0 },
                    }));
                }
            }
        }
    }
    let mut back = BTreeMap::new();
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(at) = queue.pop_front() {
        if at == target {
            break;
        }
        for (next, dir) in [
            ((at.0 - 1, at.1), Direction::West),
            ((at.0 + 1, at.1), Direction::East),
            ((at.0, at.1 - 1), Direction::North),
            ((at.0, at.1 + 1), Direction::South),
        ] {
            if open.contains(&next) && seen.insert(next) {
                back.insert(next, (at, dir));
                queue.push_back(next);
            }
        }
    }
    let mut route = Vec::new();
    let mut at = target;
    while at != start {
        let (old, dir) = back[&at];
        route.push(dir);
        at = old;
    }
    for direction in route.into_iter().rev() {
        step(engine, direction);
    }
}

#[test]
fn admitted_initial_entry_has_exact_placed_and_skipped_base_receipt() {
    let engine = Engine::memory(fixture()).unwrap();
    let first = floor(&engine, 1);
    assert_eq!(first["entrant"], json!(1));
    assert_eq!(first["decisions"][0]["result"]["kind"], json!("evaluated"));
    assert_eq!(
        first["decisions"][0]["result"]["attempts"][0]["kind"],
        json!("placed")
    );
    assert_eq!(first["decisions"][1]["result"]["kind"], json!("skipped"));
    assert_eq!(
        first["decisions"][1]["result"]["reason"],
        json!("no_eligible_content")
    );
    let archive = serde_json::to_value(&engine.archive).unwrap();
    assert_eq!(archive["initial_activations"], json!([first]));
    assert!(
        engine.archive.records.is_empty(),
        "initialization is not a fake player turn"
    );
    restored_both_ways(&engine);
}

#[test]
fn speculative_preparation_and_published_visibility_do_not_activate_floor_two() {
    let mut engine = Engine::memory(fixture()).unwrap();
    let before_preload = engine.game.clone();
    engine.start_preloading();
    engine.settle_preloading();
    assert_eq!(
        engine.game, before_preload,
        "worker-only preparation changes no authority"
    );
    let first = decisions(&engine);
    let before_walk = authority(&engine);
    let package = engine.archive.scenario.package.as_ref().unwrap().clone();
    assert_eq!(first["floors"].as_object().unwrap().len(), 1);
    walk_to_port(&mut engine, 1, "down");
    // Standing on stairs exposes/pins the ordinary landing but is not entry.
    assert!(
        engine.game.loaded_regions().any(|r| r.0 >= 10),
        "negative must actually publish floor-two geometry"
    );
    assert_eq!(decisions(&engine), first);
    let after_walk = authority(&engine);
    for field in [
        "rules_rng",
        "next_actor_id",
        "next_item_id",
        "next_door_id",
        "scenario_policy",
    ] {
        assert_eq!(
            after_walk["game"][field], before_walk["game"][field],
            "speculative publication changes {field}"
        );
    }
    assert_eq!(
        activation(&after_walk["game"], &after_walk["shared"])["plan"],
        activation(&before_walk["game"], &before_walk["shared"])["plan"]
    );
    assert!(
        package
            .population_configuration(engine.game.seed())
            .unwrap()
            .valid_game(&engine.game),
        "new prepared scopes must match the exact immutable geometry published"
    );
    let mut control = Engine::memory(fixture_with_activation(false)).unwrap();
    walk_to_port(&mut control, 1, "down");
    let actor = |engine: &Engine| {
        let state = authority(engine);
        let pool = state["game"]["actors"].as_u64().unwrap() as usize;
        state["shared"]["actors"][pool]["1"].clone()
    };
    assert_eq!(
        actor(&engine),
        actor(&control),
        "only ordinary walking changes the protagonist"
    );
    let admitted = admit(&mut engine, "entry", Direction::Down);
    assert_eq!(
        decisions(&engine),
        first,
        "queued admission is not movement"
    );
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(decisions(&engine)["floors"].as_object().unwrap().len(), 2);
    let entered = engine.diagnostic_copy();
    let receipt = engine
        .archive
        .records
        .iter()
        .find(|r| r.entry.id == admitted.entry.id)
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    assert!(engine.apply_command(&receipt, None).unwrap().duplicate);
    exact_equal(&engine, &entered);
    restored_both_ways(&engine);
}

#[test]
fn population_decisions_and_entities_are_equal_for_demand_prepared_and_racing_workers() {
    let mut expected = None;
    for mode in ["demand", "prepared", "racing"] {
        let mut engine = Engine::memory(fixture()).unwrap();
        if mode != "demand" {
            engine.start_preloading();
        }
        if mode == "prepared" {
            engine.settle_preloading();
        }
        walk_to_port(&mut engine, 1, "down");
        step(&mut engine, Direction::Down);
        let actual = (engine.game.clone(), decisions(&engine));
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected, "{mode}");
        } else {
            expected = Some(actual);
        }
    }
}

#[test]
fn revisit_preserves_skips_and_never_restocks_initial_items() {
    let mut engine = Engine::memory(fixture()).unwrap();
    let initial = floor(&engine, 1);
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    let second = floor(&engine, 2);
    let allocated = authority(&engine)["game"]["next_item_id"].clone();
    step(&mut engine, Direction::Up); // paired landing, ordinary entry
    assert_eq!(floor(&engine, 1), initial);
    assert_eq!(floor(&engine, 2), second);
    step(&mut engine, Direction::Down);
    assert_eq!(floor(&engine, 2), second);
    assert_eq!(
        authority(&engine)["game"]["next_item_id"],
        allocated,
        "revisits cannot allocate any replacement initial supplies"
    );
    restored_both_ways(&engine);
}

#[test]
fn late_store_failure_discards_successful_entry_population_and_all_authority() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("activation.db");
    let mut engine = Engine::open(&path, fixture()).unwrap();
    walk_to_port(&mut engine, 1, "down");
    admit(&mut engine, "entry", Direction::Down);
    engine.flush().unwrap();
    drop(engine);
    let mut rejected = Engine::open_with_policy(
        &path,
        Scenario::two_room(0),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let before = rejected.diagnostic_copy();
    let mut healthy = Engine::replay(rejected.archive.clone(), None, None).unwrap();
    healthy.execute_next_intention().unwrap().unwrap();
    assert_eq!(
        decisions(&healthy)["floors"].as_object().unwrap().len(),
        2,
        "positive control proves the exact rejected work really creates population"
    );
    assert!(rejected.execute_next_intention().is_err());
    exact_equal(&rejected, &before);
    assert_eq!(authority(&rejected), authority(&before));
    assert_eq!(
        rejected.save_status().accepted_sequence,
        before.save_status().accepted_sequence
    );
    assert_eq!(decisions(&rejected)["floors"].as_object().unwrap().len(), 1);
}

#[test]
fn entry_population_is_born_before_movement_recovery_advances_the_scheduler() {
    let mut engine = Engine::memory(single_member_foe_fixture(true)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    let entry_tick = engine.game.tick();
    assert!(
        engine.game.loaded_actor_ids().all(|id| id == SimActor(1)),
        "destination geometry may be loaded but its actor must not exist yet"
    );
    step(&mut engine, Direction::Down);
    let spawned: Vec<_> = engine
        .game
        .loaded_actor_ids()
        .filter(|id| *id != SimActor(1))
        .collect();
    assert_eq!(spawned.len(), 1);
    assert!(
        spawned[0].0 > 2,
        "dynamic foes cannot reuse the omitted authored character identity"
    );
    assert!(engine.game.is_ai(spawned[0]));
    assert_eq!(
        engine.game.tick(),
        entry_tick,
        "newborn NPC decision must stop the scheduler before player movement recovery elapses"
    );
    assert_eq!(engine.game.next_actor(), Some(spawned[0]));
    assert!(
        engine.causal_boundary(ActorId(spawned[0].0)).is_ok(),
        "new actor registration cannot depend on a nonempty region transition"
    );
    assert!(engine.actors().contains(&ActorId(spawned[0].0)));
    // Loaded actor membership is owned solely by Game; no duplicate revision map remains.
    assert!(engine.state(ActorId(spawned[0].0)).is_ok());
    assert!(
        engine.game.known_cells(spawned[0]).next().is_some(),
        "the ordinary AI decision needs its own initial navigation knowledge"
    );
    let state = authority(&engine);
    let pool = state["game"]["actors"].as_u64().unwrap() as usize;
    let actor = &state["shared"]["actors"][pool][spawned[0].0.to_string()];
    assert_eq!(actor["active_since"], json!(entry_tick));
    assert_eq!(actor["ready_at"], json!(entry_tick));
    restored_both_ways(&engine);
}

#[test]
fn loaded_inactive_member_births_use_entry_tick_and_individual_freeze_stamps() {
    let mut engine = Engine::memory(multi_member_foe_fixture(true)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    let before = authority(&engine);
    let lifecycle = before["game"]["lifecycle"].as_u64().unwrap() as usize;
    let frozen_before = before["shared"]["lifecycles"][lifecycle]["frozen"]
        .as_array()
        .unwrap();
    assert!(
        frozen_before
            .iter()
            .any(|id| (10..=18).contains(&id.as_u64().unwrap())),
        "negative control requires already-published frozen destination geometry"
    );
    let entry_tick = engine.game.tick();
    assert!(
        entry_tick > 0,
        "birth must differ from initial speculative geometry time"
    );
    assert!(engine.game.loaded_actor_ids().all(|id| id == SimActor(1)));
    assert!(decisions(&engine)["floors"].get("dungeon/2").is_none());
    step(&mut engine, Direction::Down);
    let state = authority(&engine);
    let lifecycle = state["game"]["lifecycle"].as_u64().unwrap() as usize;
    let lifecycle = &state["shared"]["lifecycles"][lifecycle];
    let frozen = lifecycle["frozen"].as_array().unwrap();
    let actors = state["game"]["actors"].as_u64().unwrap() as usize;
    let mut frozen_births = 0;
    for (id, actor) in state["shared"]["actors"][actors].as_object().unwrap() {
        if id == "1" {
            continue;
        }
        if frozen.contains(&actor["location"]["region"]) {
            frozen_births += 1;
            assert_eq!(actor["active_since"], json!(entry_tick));
            assert_eq!(actor["ready_at"], json!(entry_tick));
            assert_eq!(
                lifecycle["stamps"][id],
                json!(entry_tick),
                "new actor must not inherit its region's older speculative freeze time"
            );
        }
    }
    assert!(
        frozen_births > 0,
        "fixture must actually place at least one newborn actor in a loaded inactive member"
    );
    let items = state["game"]["items"].as_u64().unwrap() as usize;
    let frozen_items = state["shared"]["items"][items]
        .as_object()
        .unwrap()
        .values()
        .filter(|item| {
            let region = &item["location"]["Ground"]["region"];
            region.as_u64().is_some_and(|id| (10..=18).contains(&id)) && frozen.contains(region)
        })
        .count();
    assert!(
        frozen_items > 0,
        "ordinary new items must also exist in a loaded inactive destination member"
    );
    // Items have no actor active_since field; their normal containing region
    // freezes their motion. The authoritative ordinary state, including actor
    // stamps, must survive both independent reconstruction paths unchanged.
    restored_both_ways(&engine);
}

fn assert_selected_starter(engine: &Engine, selected: u64) -> Value {
    let observation = engine.game.observe(SimActor(selected)).unwrap();
    let quantities: BTreeMap<_, _> = observation
        .inventory
        .iter()
        .map(|item| (item.name.as_str(), item.quantity))
        .collect();
    assert_eq!(
        quantities,
        BTreeMap::from([
            ("mace", 1),
            ("ring mail", 1),
            ("clean bandage", 3),
            ("food ration", 1),
        ]),
        "first red: selected Rogue preset starts with exact ordinary inventory"
    );
    assert_eq!(
        observation.inventory.len(),
        4,
        "loadout creates exactly one stack per declared row, never duplicate grants"
    );
    assert!(!engine.game.is_ai(SimActor(selected)));
    let omitted = if selected == 1 { 2 } else { 1 };
    assert!(
        !engine.game.known_actor_ids().contains(&SimActor(omitted)),
        "the unselected authored preset must not become an actor or a dynamic foe"
    );
    let state = authority(engine);
    let actor_pool = state["game"]["actors"].as_u64().unwrap() as usize;
    let actor = &state["shared"]["actors"][actor_pool][selected.to_string()];
    let equipped: BTreeSet<u64> = actor["equipment"]
        .as_object()
        .unwrap()
        .values()
        .map(|item| item.as_u64().unwrap())
        .collect();
    let intended: BTreeSet<u64> = observation
        .inventory
        .iter()
        .filter(|item| item.name == "mace" || item.name == "ring mail")
        .map(|item| item.id.0)
        .collect();
    assert_eq!(
        equipped, intended,
        "ordinary weapon and armor must actually be equipped"
    );
    let item_pool = state["game"]["items"].as_u64().unwrap() as usize;
    let items = state["shared"]["items"][item_pool].as_object().unwrap();
    assert!(
        items
            .values()
            .all(|item| item["location"]["Carried"] != json!(omitted)),
        "the omitted preset must not leave carried loot behind"
    );
    let carried: BTreeMap<_, _> = items
        .iter()
        .filter(|(_, item)| item["location"]["Carried"] == json!(selected))
        .map(|(id, item)| (id.clone(), item.clone()))
        .collect();
    assert_eq!(carried.len(), 4);
    json!({"carried":carried,"equipment":actor["equipment"]})
}

fn selected_starter_persists(selected: u64) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    for checkpoint_interval in [0, 1] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("starter.db");
        let scenario = crate::scenario_package::load(&root, 42, Some(selected), true).expect(
            "first red: each authored Rogue preset must load through the ordinary package selector",
        );
        let policy = crate::SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, scenario, policy.clone()).unwrap();
        let initial = assert_selected_starter(&engine, selected);
        // Admit without executing to create a journal/checkpoint boundary while
        // preserving the exact initial loadout and avoiding combat/tuning effects.
        let owner = ActorId(selected);
        engine
            .command(
                "player",
                "starter-test",
                owner,
                "wait",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_boundary: engine.causal_boundary(owner).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert_eq!(assert_selected_starter(&engine, selected), initial);
        engine.flush().unwrap();
        // A diagnostic copy also clones Regions and its Store/preloader, so
        // clearing only Engine.store/lock still keeps the journal writer alive.
        // Reconstruct without any Store to retain a genuinely detached oracle.
        let before = Engine::replay(engine.archive.clone(), None, None).unwrap();
        exact_equal(&before, &engine);
        drop(engine);
        // The save's selected identity, compiled input and ordinary items own
        // recovery. The unrelated supplied scenario must not create a new kit.
        let resumed = Engine::open_with_policy(&path, Scenario::two_room(0), policy).unwrap();
        assert_eq!(
            resumed.recovery_profile().checkpoint_sequence > 0,
            checkpoint_interval == 1,
            "exercise genuinely distinct recovery paths"
        );
        assert_eq!(assert_selected_starter(&resumed, selected), initial);
        exact_equal(&resumed, &before);
        restored_both_ways(&resumed);
    }
}

#[test]
fn rogue_warrior_initial_loadout_is_exact_once_across_journal_and_checkpoint_restart() {
    selected_starter_persists(1);
}

#[test]
fn rogue_mage_initial_loadout_is_exact_once_across_journal_and_checkpoint_restart() {
    selected_starter_persists(2);
}

#[test]
fn control_existing_exploration_uses_the_same_ordinary_entry_and_replay_path() {
    let mut engine = Engine::memory(fixture_with_activation(false)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    assert!((10..=18).contains(&engine.game.observe(SimActor(1)).unwrap().location.region.0));
    step(&mut engine, Direction::Up);
    assert!((1..=9).contains(&engine.game.observe(SimActor(1)).unwrap().location.region.0));
    restored_both_ways(&engine);
}

#[test]
fn control_existing_catalog_and_single_member_geometry_construct_without_activation() {
    let mut engine = Engine::memory(single_member_foe_fixture(false)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    assert_eq!(
        engine.game.observe(SimActor(1)).unwrap().location.region,
        RegionId(2)
    );
    assert_eq!(
        engine.game.loaded_actor_ids().collect::<Vec<_>>(),
        vec![SimActor(1)]
    );
    restored_both_ways(&engine);
}

fn wizard(engine: &mut Engine, operation: WizardOperation) -> CommandResult {
    engine.enable_wizard().unwrap();
    engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            Command::Wizard {
                display_basis: None,
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                operation,
            },
        )
        .unwrap()
}

#[test]
fn private_journal_receipts_are_exact_entry_deltas_and_rewind_is_not_entry() {
    let mut engine = Engine::memory(fixture()).unwrap();
    walk_to_port(&mut engine, 1, "down");
    assert!(engine
        .archive
        .records
        .iter()
        .all(|record| record.activations.is_empty()));
    let target = engine.archive.records.last().unwrap().entry.id.clone();
    let before = engine.game.activations_since(0).unwrap();
    admit(&mut engine, "receipt-entry", Direction::Down);
    assert!(engine
        .archive
        .records
        .last()
        .unwrap()
        .activations
        .is_empty());
    engine.execute_next_intention().unwrap().unwrap();
    let entered = engine.archive.records.last().unwrap().activations.clone();
    assert_eq!(
        entered,
        engine.game.activations_since(before.len()).unwrap()
    );
    assert_eq!(entered.len(), 1);
    assert_eq!(
        serde_json::to_value(&entered[0]).unwrap(),
        floor(&engine, 2)
    );
    let old_floor = entered[0].clone();
    restored_both_ways(&engine);

    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(target),
        },
    );
    assert!(engine
        .archive
        .records
        .last()
        .unwrap()
        .activations
        .is_empty());
    assert_eq!(engine.game.activations_since(0).unwrap(), before);
    restored_both_ways(&engine);
    step(&mut engine, Direction::Down);
    let reentered = engine.archive.records.last().unwrap().activations.clone();
    assert_eq!(reentered.len(), 1);
    assert_eq!(reentered[0].scope, old_floor.scope);
    assert_eq!(reentered[0].ordinal, old_floor.ordinal);
    assert_ne!(
        reentered[0].decisions, old_floor.decisions,
        "rewind retains abandoned-branch identity high-water marks"
    );
    restored_both_ways(&engine);
}

#[test]
fn zero_record_base_receipt_corruption_is_rejected_with_and_without_a_checkpoint() {
    let engine = Engine::memory(fixture()).unwrap();
    assert!(engine.archive.records.is_empty());
    for mutation in ["missing", "duplicate", "tick", "profile"] {
        let mut archive = engine.archive.clone();
        match mutation {
            "missing" => archive.initial_activations.clear(),
            "duplicate" => archive
                .initial_activations
                .push(archive.initial_activations[0].clone()),
            "tick" => archive.initial_activations[0].tick += 1,
            "profile" => archive.initial_activations[0].profile = "unknown".into(),
            _ => unreachable!(),
        }
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "journal: {mutation}"
        );
        let disk = Checkpoint::capture(&engine).encode("base-activation", 0);
        assert!(disk.restore(archive).is_err(), "checkpoint: {mutation}");
    }
}

#[test]
fn journal_receipt_corruption_cannot_hide_behind_checkpoint_recovery() {
    let mut engine = Engine::memory(fixture()).unwrap();
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    let at = engine.archive.records.len() - 1;
    assert_eq!(engine.archive.records[at].activations.len(), 1);
    for mutation in [
        "missing",
        "duplicate",
        "tick",
        "entrant",
        "profile",
        "ordinal",
        "metadata",
    ] {
        let mut archive = engine.archive.clone();
        match mutation {
            "missing" => archive.records[at].activations.clear(),
            "duplicate" => {
                let duplicate = archive.records[at].activations[0].clone();
                archive.records[at].activations.push(duplicate);
            }
            "tick" => archive.records[at].activations[0].tick += 1,
            "entrant" => archive.records[at].activations[0].entrant = SimActor(0),
            "profile" => archive.records[at].activations[0].profile_version += 1,
            "ordinal" => archive.records[at].activations[0].ordinal += 1,
            "metadata" => {
                archive.records[at - 1].activations =
                    std::mem::take(&mut archive.records[at].activations);
            }
            _ => unreachable!(),
        }
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "journal: {mutation}"
        );
        let disk =
            Checkpoint::capture(&engine).encode("journal-activation", archive.records.len() as u64);
        assert!(disk.restore(archive).is_err(), "checkpoint: {mutation}");
    }
}

fn expired_abandoned_activation(scenario: Scenario) -> (Engine, usize) {
    let mut engine = Engine::memory(scenario).unwrap();
    walk_to_port(&mut engine, 1, "down");
    let target = engine.archive.records.last().unwrap().entry.id.clone();
    step(&mut engine, Direction::Down);
    let abandoned = engine.archive.records.len() - 1;
    let abandoned_id = engine.archive.records[abandoned].entry.id.clone();
    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(target),
        },
    );
    for _ in 0..REWIND_BOUNDARIES {
        engine
            .command(
                "player",
                "activation-test",
                ActorId(1),
                &uuid::Uuid::new_v4().to_string(),
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
    }
    assert!(engine
        .boundaries
        .iter()
        .all(|boundary| boundary.id.as_ref() != Some(&abandoned_id)));
    assert!(decisions(&engine)["floors"].get("dungeon/2").is_none());
    restored_both_ways(&engine);
    (engine, abandoned)
}

#[test]
fn checkpoint_validates_abandoned_activation_receipts_after_their_boundary_expires() {
    let (engine, abandoned) = expired_abandoned_activation(fixture());
    for mutation in [
        "profile",
        "identity_bound",
        "duplicate_origin",
        "scope_family",
        "scope_depth",
        "location",
    ] {
        let mut archive = serde_json::to_value(&engine.archive).unwrap();
        let origin = archive["initial_activations"][0]["decisions"][0]["result"]["attempts"][0]
            ["entity"]["id"]
            .clone();
        let activation = &mut archive["records"][abandoned]["activations"][0];
        match mutation {
            "profile" => activation["profile"] = json!("unrecognized-history"),
            "scope_family" => activation["scope"] = json!("unrecognized/2"),
            "scope_depth" => activation["scope"] = json!("dungeon/02"),
            "location" => {
                activation["decisions"][0]["result"]["attempts"][0]["location"]["region"] =
                    json!(999999)
            }
            "identity_bound" => {
                activation["decisions"][0]["result"]["attempts"][0]["entity"]["id"] =
                    json!(u64::MAX - 1)
            }
            "duplicate_origin" => {
                activation["decisions"][0]["result"]["attempts"][0]["entity"]["id"] = origin
            }
            _ => unreachable!(),
        }
        let archive = serde_json::from_value(archive).unwrap();
        let disk = Checkpoint::capture(&engine)
            .encode("abandoned-activation", engine.archive.records.len() as u64);
        assert!(disk.restore(archive).is_err(), "{mutation}");
    }
}

fn reject_expired_population_actor_receipt_using_reserved_identity(reserved: u64) {
    use tor_simulation::floor_activation::{PlacementDecision, PopulationEntity, StageResult};
    let (engine, abandoned) = expired_abandoned_activation(single_member_foe_fixture(true));
    let mut archive = engine.archive.clone();
    let activation = &mut archive.records[abandoned].activations[0];
    let actor = activation
        .decisions
        .iter_mut()
        .find_map(|decision| match &mut decision.result {
            StageResult::Evaluated { attempts, .. } => {
                attempts.iter_mut().find_map(|attempt| match attempt {
                    PlacementDecision::Placed {
                        entity: PopulationEntity::Actor { id, .. },
                        ..
                    } => Some(id),
                    _ => None,
                })
            }
            StageResult::Skipped { .. }
            | StageResult::RoomsEvaluated { .. }
            | StageResult::RoomCluster { .. } => None,
        })
        .expect("fixture must retain the abandoned actor origin");
    assert!(
        actor.0 > 2,
        "both authored character identities are reserved"
    );
    assert!(
        !engine.game.known_actor_ids().contains(actor),
        "the valid historical origin does not require a live actor"
    );
    // Keep the admitted content, position and timing. Only the origin identity
    // changes, to a reserved character ID that never belonged to population.
    *actor = SimActor(reserved);
    let disk = Checkpoint::capture(&engine).encode(
        "reserved-population-origin",
        engine.archive.records.len() as u64,
    );
    assert!(
        disk.restore(archive).is_err(),
        "checkpoint must reject expired population actor origin using reserved character {reserved}"
    );
}

#[test]
fn checkpoint_rejects_expired_population_actor_receipt_using_selected_character_identity() {
    reject_expired_population_actor_receipt_using_reserved_identity(1);
}

#[test]
fn checkpoint_rejects_expired_population_actor_receipt_using_omitted_character_identity() {
    reject_expired_population_actor_receipt_using_reserved_identity(2);
}

#[test]
fn checkpoint_rejects_expired_population_item_receipt_using_starting_kit_identity() {
    use tor_simulation::floor_activation::{PlacementDecision, PopulationEntity, StageResult};
    let mut scenario = fixture();
    let package = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&package.manifest).unwrap();
    let kit_item = json!({"archetype":"marker","quantity":1});
    source["characters"][0]["loadout"] = json!([kit_item.clone(), kit_item.clone()]);
    source["characters"].as_array_mut().unwrap().push(json!({
        "id":2,"anchor":"dungeon/1/start","unselected":"omit",
        "loadout":[kit_item.clone(), kit_item.clone(), kit_item.clone(), kit_item]
    }));
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &package.rules_source,
        )
        .unwrap(),
    ));
    // The positive recovery control must accept population above this selected
    // kit's floor, without reserving the omitted preset's unallocated loadout.
    let (engine, abandoned) = expired_abandoned_activation(scenario);
    let inventory = engine.game.observe(SimActor(1)).unwrap().inventory;
    assert_eq!(inventory.len(), 2);
    let kit_id = inventory.iter().map(|item| item.id).max().unwrap();
    assert_eq!(
        kit_id.0, 2,
        "the last selected kit row has its own identity"
    );
    let mut archive = engine.archive.clone();
    let StageResult::Evaluated { attempts, .. } =
        &mut archive.records[abandoned].activations[0].decisions[0].result
    else {
        panic!("fixture must evaluate its marker stage");
    };
    let PlacementDecision::Placed {
        entity: PopulationEntity::Item { id, .. },
        ..
    } = &mut attempts[0]
    else {
        panic!("fixture must retain the abandoned item origin");
    };
    assert!(
        id.0 > kit_id.0,
        "population is allocated after the selected kit"
    );
    *id = kit_id;
    let disk = Checkpoint::capture(&engine)
        .encode("kit-population-origin", engine.archive.records.len() as u64);
    assert!(
        disk.restore(archive).is_err(),
        "checkpoint must reject expired population item origin using starting-kit identity"
    );
}

fn reject_expired_population_receipt_using_retained_wizard_origin(
    engine: &Engine,
    abandoned: usize,
    origin: tor_simulation::floor_activation::PopulationEntity,
) {
    use tor_simulation::floor_activation::{PlacementDecision, PopulationEntity, StageResult};
    // Legitimate non-population allocations remain recoverable by both paths.
    restored_both_ways(engine);
    let mut archive = engine.archive.clone();
    let identity = archive.records[abandoned].activations[0]
        .decisions
        .iter_mut()
        .find_map(|decision| match &mut decision.result {
            StageResult::Evaluated { attempts, .. } => {
                attempts.iter_mut().find_map(|attempt| match attempt {
                    PlacementDecision::Placed { entity, .. }
                        if matches!(
                            (&*entity, &origin),
                            (
                                PopulationEntity::Actor { .. },
                                PopulationEntity::Actor { .. }
                            ) | (PopulationEntity::Item { .. }, PopulationEntity::Item { .. })
                        ) =>
                    {
                        Some(entity)
                    }
                    _ => None,
                })
            }
            StageResult::Skipped { .. }
            | StageResult::RoomsEvaluated { .. }
            | StageResult::RoomCluster { .. } => None,
        })
        .expect("fixture must retain an abandoned origin of the requested kind");
    assert_ne!(*identity, origin);
    // Only the expired population receipt changes. The retained wizard result
    // still assigns this same identity to a separate later allocation.
    *identity = origin;
    assert!(
        Engine::replay(archive.clone(), None, None).is_err(),
        "strict replay must reject the substituted population origin"
    );
    let disk = Checkpoint::capture(engine).encode(
        "wizard-population-origin-alias",
        engine.archive.records.len() as u64,
    );
    assert!(
        disk.restore(archive).is_err(),
        "checkpoint must reject two retained, contradictory allocation origins"
    );
}

#[test]
fn checkpoint_rejects_expired_population_item_receipt_aliasing_retained_wizard_birth() {
    let (mut engine, abandoned) = expired_abandoned_activation(fixture());
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let created = wizard(
        &mut engine,
        WizardOperation::PlaceItem {
            kind: crate::journal::WizardItem::Token,
            position: Position {
                region: at.region.0,
                x: at.position.x,
                y: at.position.y,
                z: at.position.z,
            },
        },
    );
    let JournalContent::Wizard {
        result: WizardResult::ItemPlaced { item },
        ..
    } = created.entry.content
    else {
        panic!("ordinary wizard allocation must retain its item identity");
    };
    reject_expired_population_receipt_using_retained_wizard_origin(
        &engine,
        abandoned,
        tor_simulation::floor_activation::PopulationEntity::Item {
            id: tor_simulation::ItemId(item),
            quantity: 1,
        },
    );
}

#[test]
fn checkpoint_rejects_expired_population_actor_receipt_aliasing_retained_wizard_birth() {
    let (mut engine, abandoned) = expired_abandoned_activation(single_member_foe_fixture(true));
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let neighbor = engine
        .game
        .known_cells(SimActor(1))
        .find(|location| {
            location.region == at.region
                && *location != at
                && engine.game.travel_route(SimActor(1), *location).is_ok()
        })
        .expect("fixture needs a disclosed, unoccupied actor spawn cell");
    let created = wizard(
        &mut engine,
        WizardOperation::SpawnActor {
            position: Position {
                region: neighbor.region.0,
                x: neighbor.position.x,
                y: neighbor.position.y,
                z: neighbor.position.z,
            },
            turn_ticks: 100,
        },
    );
    let JournalContent::Wizard {
        result: WizardResult::ActorSpawned { actor },
        ..
    } = created.entry.content
    else {
        panic!("ordinary wizard allocation must retain its actor identity");
    };
    reject_expired_population_receipt_using_retained_wizard_origin(
        &engine,
        abandoned,
        tor_simulation::floor_activation::PopulationEntity::Actor {
            id: SimActor(actor.0),
            carried_loot: None,
        },
    );
}

#[test]
fn checkpoint_rejects_duplicate_retained_wizard_item_births() {
    let mut engine = Engine::memory(fixture()).unwrap();
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let operation = WizardOperation::PlaceItem {
        kind: crate::journal::WizardItem::Token,
        position: Position {
            region: at.region.0,
            x: at.position.x,
            y: at.position.y,
            z: at.position.z,
        },
    };
    let first = wizard(&mut engine, operation.clone());
    let second = wizard(&mut engine, operation);
    let JournalContent::Wizard { result: first, .. } = first.entry.content else {
        panic!("fixture must retain its first wizard allocation");
    };
    let JournalContent::Wizard { result: second, .. } = second.entry.content else {
        panic!("fixture must retain its second wizard allocation");
    };
    assert_ne!(
        first, second,
        "ordinary allocations must have distinct identities"
    );
    restored_both_ways(&engine);
    let mut archive = engine.archive.clone();
    let JournalContent::Wizard { result, .. } =
        &mut archive.records.last_mut().unwrap().entry.content
    else {
        panic!("fixture must end with its second wizard allocation");
    };
    *result = first;
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    let disk = Checkpoint::capture(&engine).encode(
        "duplicate-wizard-origin",
        engine.archive.records.len() as u64,
    );
    assert!(
        disk.restore(archive).is_err(),
        "retained wizard allocations must also remain globally unique"
    );
}

#[test]
fn moved_population_items_preserve_origin_receipts_across_detach_and_recovery() {
    use tor_simulation::floor_activation::{PlacementDecision, PopulationEntity, StageResult};
    let mut engine = Engine::memory(fixture()).unwrap();
    let initial = engine.archive.initial_activations.clone();
    let StageResult::Evaluated { attempts, .. } = &initial[0].decisions[0].result else {
        panic!("fixture places an item");
    };
    let PlacementDecision::Placed {
        location,
        entity: PopulationEntity::Item { id, .. },
        ..
    } = attempts[0]
    else {
        panic!("fixture places an item");
    };
    wizard(
        &mut engine,
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: Position {
                region: location.region.0,
                x: location.position.x,
                y: location.position.y,
                z: location.position.z,
            },
        },
    );
    engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "take-origin-item",
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::Take {
                    item: id.0,
                    quantity: None,
                },
            },
        )
        .unwrap();
    assert!(engine
        .game
        .observe(SimActor(1))
        .unwrap()
        .inventory
        .iter()
        .any(|item| item.id == id));
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    walk_to_port(&mut engine, 2, "down");
    assert!(
        engine
            .game
            .detached_records()
            .any(|(region, _)| (1..=9).contains(&region.0)),
        "the source floor must actually leave loaded residency"
    );
    assert_eq!(engine.archive.initial_activations, initial);
    assert_eq!(engine.game.activations_since(0).unwrap()[0], initial[0]);
    restored_both_ways(&engine);
}

#[test]
fn cancelling_preparation_with_a_stair_step_commits_entry_as_a_gameplay_boundary() {
    let mut scenario = fixture();
    let original = scenario.package.as_ref().unwrap();
    let borrowed = crate::scenario_package::resolve_source(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler"),
    )
    .unwrap();
    let mut manifest = serde_json::to_value(&original.manifest).unwrap();
    manifest["creatures"] = serde_json::to_value(&borrowed.manifest.creatures).unwrap();
    manifest["factions"] = serde_json::to_value(&borrowed.manifest.factions).unwrap();
    manifest["characters"][0]["creature"] =
        serde_json::to_value(&borrowed.manifest.characters[0].creature).unwrap();
    manifest["creatures"]["species"]["delver"]["modifiers"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "scope":"always", "op":{"kind":"grant_ability","ability":"activation_windup"}
        }));
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let mut ability = serde_json::to_value(
        rules
            .abilities
            .iter()
            .find(|ability| ability.id.as_str() == "brace_for_impact")
            .unwrap(),
    )
    .unwrap();
    ability["id"] = json!("activation_windup");
    ability["cost"] = Value::Null;
    ability["timing"]["source"]["prepare"] = json!(1000);
    rules
        .abilities
        .push(serde_json::from_value(ability).unwrap());
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            vec![],
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    let mut engine = Engine::memory(scenario).unwrap();
    walk_to_port(&mut engine, 1, "down");
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let neighbor = engine
        .game
        .known_cells(SimActor(1))
        .find(|location| {
            location.region == at.region
                && *location != at
                && engine.game.travel_route(SimActor(1), *location).is_ok()
        })
        .unwrap();
    // A same-floor ordinary actor keeps the scheduler at the preparation start.
    // It does not enter or populate the destination for the protagonist.
    wizard(
        &mut engine,
        WizardOperation::SpawnActor {
            position: Position {
                region: neighbor.region.0,
                x: neighbor.position.x,
                y: neighbor.position.y,
                z: neighbor.position.z,
            },
            turn_ticks: 100,
        },
    );
    let admitted = engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "windup",
            &engine.branch().clone(),
            Command::AdmitIntention {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("activation_windup").unwrap(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: SimActor(1),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            },
        )
        .unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    assert!(engine.game.preparation(SimActor(1)).is_some());
    assert!(decisions(&engine)["floors"].get("dungeon/2").is_none());
    let escaped = engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "escape-down",
            &engine.branch().clone(),
            Command::CancelIntention {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                admission: admitted.entry.id,
                step: Some(Direction::Down),
            },
        )
        .unwrap();
    assert!(escaped.entry.content.rewindable());
    assert!(engine.game.preparation(SimActor(1)).is_none());
    assert_eq!(engine.archive.records.last().unwrap().activations.len(), 1);
    assert_eq!(
        serde_json::to_value(&engine.archive.records.last().unwrap().activations[0]).unwrap(),
        floor(&engine, 2)
    );
    // Loaded actor membership is owned solely by Game; no duplicate revision map remains.
    assert!(!engine.game.population_entry_pending());
    restored_both_ways(&engine);
}

#[test]
fn checkpoint_population_configuration_must_match_the_pinned_package() {
    let engine = Engine::memory(fixture()).unwrap();
    for mutation in [
        "capability",
        "future_depth",
        "seed",
        "scope_cells",
        "scope_protected",
    ] {
        let disk = Checkpoint::capture(&engine).encode("pinned-population", 0);
        let mut saved = serde_json::to_value(disk).unwrap();
        let mutate_game = |game: &mut Value| match mutation {
            "capability" => {
                game["scenario_policy"]["capabilities"]["npc_stair_traversal"] = json!(false)
            }
            "seed" => game["seed"] = json!(43),
            _ => (),
        };
        let mutate_activation = |state: &mut Value| match mutation {
            "future_depth" => {
                state["plan"]["profiles"]["baseline"]["stages"][1]["population"]["bands"][0]
                    ["depth"]["minimum"] = json!(4)
            }
            "scope_cells" => {
                let origin = state["floors"]["dungeon/1"]["decisions"][0]["result"]["attempts"][0]
                    ["location"]
                    .clone();
                let cells = state["scopes"]["dungeon/1"]["cells"]
                    .as_array_mut()
                    .unwrap();
                let at = cells.iter().position(|cell| *cell != origin).unwrap();
                cells.remove(at);
            }
            "scope_protected" => {
                state["scopes"]["dungeon/1"]["protected"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            _ => (),
        };
        mutate_game(&mut saved["game"]);
        for boundary in saved["boundaries"].as_array_mut().unwrap() {
            mutate_game(&mut boundary["game"]);
        }
        for state in saved["shared"]["floor_activations"].as_array_mut().unwrap() {
            mutate_activation(state);
        }
        // Current and retained copies agree and initial decisions are unchanged;
        // only comparison to the pinned scenario can establish future behavior.
        let disk: DiskCheckpoint = serde_json::from_value(saved).unwrap();
        assert!(disk.restore(engine.archive.clone()).is_err(), "{mutation}");
    }
}

#[test]
fn checkpoint_cannot_inject_scenario_policy_into_a_nonpackage_game() {
    let engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let disk = Checkpoint::capture(&engine).encode("no-population", 0);
    let mut saved = serde_json::to_value(disk).unwrap();
    let injected = json!({"capabilities":{"npc_stair_traversal":false,"owner_advancement":true,"corpses":true,"low_health_flight":true},"selected_character":1});
    saved["game"]["scenario_policy"] = injected.clone();
    for boundary in saved["boundaries"].as_array_mut().unwrap() {
        boundary["game"]["scenario_policy"] = injected.clone();
    }
    let disk: DiskCheckpoint = serde_json::from_value(saved).unwrap();
    assert!(disk.restore(engine.archive.clone()).is_err());
}

#[test]
fn checkpoint_cannot_remove_activation_inputs_from_a_published_unentered_scope() {
    let mut engine = Engine::memory(fixture()).unwrap();
    walk_to_port(&mut engine, 1, "down");
    assert!(engine
        .game
        .loaded_regions()
        .any(|region| (10..=18).contains(&region.0)));
    assert!(decisions(&engine)["floors"].get("dungeon/2").is_none());
    let disk = Checkpoint::capture(&engine).encode("missing-future-scope", 0);
    let mut saved = serde_json::to_value(disk).unwrap();
    for state in saved["shared"]["floor_activations"].as_array_mut().unwrap() {
        state["scopes"].as_object_mut().unwrap().remove("dungeon/2");
        state["members"]
            .as_object_mut()
            .unwrap()
            .retain(|_, scope| scope.as_str() != Some("dungeon/2"));
    }
    let disk: DiskCheckpoint = serde_json::from_value(saved).unwrap();
    assert!(disk.restore(engine.archive.clone()).is_err());
}

#[test]
fn checkpoint_rejects_future_entry_tick_on_expired_abandoned_branch() {
    let (engine, abandoned) = expired_abandoned_activation(fixture());
    let mut archive = engine.archive.clone();
    // The public rewind tick is the restored target tick. Its prestate must
    // instead bound receipts on the branch being abandoned, even after that
    // branch's selectable snapshots have expired.
    archive.records[abandoned].activations[0].tick = engine.game.tick() + 1;
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    let disk =
        Checkpoint::capture(&engine).encode("abandoned-entry-clock", archive.records.len() as u64);
    assert!(
        disk.restore(archive).is_err(),
        "expired abandoned entry cannot occur beyond its transaction end"
    );
}

#[test]
fn private_transaction_clocks_reject_missing_duplicate_and_forged_fields() {
    let (engine, abandoned) = expired_abandoned_activation(fixture());
    for field in ["before_tick", "after_tick"] {
        let mut missing = serde_json::to_value(&engine.archive).unwrap();
        missing["records"][abandoned]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<Archive>(missing).is_err(),
            "missing {field}"
        );
        let record = serde_json::to_string(&engine.archive.records[abandoned]).unwrap();
        let duplicate = format!("{{\"{field}\":0,{}", &record[1..]);
        assert!(
            serde_json::from_str::<Record>(&duplicate).is_err(),
            "duplicate {field}"
        );
        for index in [abandoned, abandoned + 1, engine.archive.records.len() - 1] {
            let mut forged = engine.archive.clone();
            if field == "before_tick" {
                forged.records[index].before_tick += 1;
            } else {
                forged.records[index].after_tick += 1;
            }
            assert!(
                Engine::replay(forged.clone(), None, None).is_err(),
                "journal {field} {index}"
            );
            let disk =
                Checkpoint::capture(&engine).encode("private-clocks", forged.records.len() as u64);
            assert!(disk.restore(forged).is_err(), "checkpoint {field} {index}");
        }
    }
}
