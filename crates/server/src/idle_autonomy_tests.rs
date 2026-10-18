//! Default autonomy is tested through the production package compiler and AI queue.
use super::*;
use crate::scenario_package::RegionDef;
use tor_simulation::{Action as SimAction, ActorId as Actor};

fn location(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: WorldPosition { x, y, z: 0 },
    }
}

fn bounded_package(
    scenario: &str,
    archetype: &str,
    profile: Option<&str>,
    customize: impl FnOnce(&mut Manifest, &mut Vec<RegionDef>),
) -> Result<Package, crate::Failure> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(scenario);
    let production = crate::scenario_package::read_package(&root)?;
    let mut manifest = production.manifest.clone();
    manifest.characters.retain(|character| character.id == 1);
    manifest.characters[0].anchor = "2/start".into();
    manifest.procedural_worlds.clear();
    manifest.generation_recipes.clear();
    manifest.generation_groups.clear();
    manifest.stair_pairs.clear();
    manifest.floor_activation = None;
    manifest.unique_retrieval = None;
    manifest.objective = None;
    let mut actor = json!({"id":3,"at":[3,2,0],"archetype":archetype,"controller":"ai"});
    if archetype == "rogue.human" {
        actor.as_object_mut().unwrap().remove("archetype");
        actor["creature"] = serde_json::to_value(&manifest.characters[0].creature).unwrap();
        actor["body"] = serde_json::to_value(&manifest.characters[0].body).unwrap();
    }
    if let Some(profile) = profile {
        actor["ai"] = json!(profile);
    }
    let mut regions = vec![
        serde_json::from_value(json!({"id":1,"name":"Idle room","size":[17,9,2],"chamber":true,"gravity":[0,0,-1],"actors":[actor]})).unwrap(),
        serde_json::from_value(json!({"id":2,"name":"Unseen room","size":[9,5,2],"chamber":true,"gravity":[0,0,-1],"anchors":{"start":[2,2,0]}})).unwrap(),
    ];
    customize(&mut manifest, &mut regions);
    let rules = production
        .resolved_rules_source
        .as_deref()
        .unwrap_or(&production.rules_source);
    Package::from_parts(manifest, regions, rules)
}

fn ready(package: &Package) -> Game {
    let mut game = package.build(42, true).unwrap();
    game.act(Actor(1), SimAction::Wait).unwrap();
    assert_eq!(game.next_actor(), Some(Actor(3)));
    game
}

fn fixture(species: &str, profile: Option<&str>, explore: bool) -> Result<Game, crate::Failure> {
    let package = bounded_package("rogue-exploration", species, profile, |manifest, _| {
        if explore {
            manifest
                .creatures
                .species
                .get_mut(species)
                .unwrap()
                .behavior
                .idle = crate::creature_authoring::IdleBehavior::Explore;
        }
    })?;
    let mut game = package.build(42, true)?;
    game.act(Actor(1), SimAction::Wait).unwrap();
    assert_eq!(game.next_actor(), Some(Actor(3)));
    Ok(game)
}

#[test]
fn authored_partial_profiles_inherit_omitted_tactics() {
    let partial =
        serde_json::from_value::<crate::scenario_package::AiProfile>(json!({"care":false}));
    assert!(
        partial.is_ok(),
        "omitted task settings must inherit species defaults: {partial:?}"
    );
    assert_eq!(
        serde_json::to_value(partial.unwrap()).unwrap()["care"],
        false
    );
    assert!(serde_json::from_value::<crate::scenario_package::AiProfile>(json!({})).is_ok());
}

#[test]
fn enrolled_ai_without_named_profile_uses_species_defaults() {
    let game = fixture("rogue.hobgoblin", None, false)
        .expect("explicit AI enrollment permits omitted profile");
    assert!(matches!(
        game.next_ai_action(),
        Some((Actor(3), SimAction::Move(_)))
    ));
}

#[test]
fn every_mobile_rogue_species_acts_before_perceiving_a_foe() {
    for species in [
        "human",
        "emu",
        "snake",
        "rattlesnake",
        "hobgoblin",
        "zombie",
        "quagga",
        "centaur",
        "yeti",
        "troll",
        "black_unicorn",
        "vampire",
        "wraith",
        "jabberwock",
    ] {
        let game = fixture(&format!("rogue.{species}"), Some("rogue_hostile"), false).unwrap();
        assert!(game
            .observe(Actor(3))
            .unwrap()
            .visible_actors
            .iter()
            .all(|actor| actor.id != Actor(1)));
        assert!(
            matches!(game.next_ai_action(), Some((Actor(3), SimAction::Move(_)))),
            "{species} must have autonomous idle movement"
        );
    }
    let game = fixture("rogue.ice_monster", Some("rogue_hostile"), false).unwrap();
    assert_eq!(game.next_ai_action(), Some((Actor(3), SimAction::Wait)));
}

#[test]
fn a_roaming_animal_never_opens_a_reachable_door() {
    let mut game = fixture("rogue.emu", Some("rogue_hostile"), true).unwrap();
    let door = game.place_door(location(1, 4, 2), false, 2).unwrap();
    assert!(
        matches!(game.next_ai_action(), Some((Actor(3), SimAction::Move(_)))),
        "movement does not grant door manipulation"
    );
    assert!(game
        .observe(Actor(3))
        .unwrap()
        .visible_cells
        .iter()
        .any(|cell| cell
            .door
            .is_some_and(|state| state.id == door && !state.open)));
}

// Explicit expectations intentionally do not derive from the package under test.
// Tools are, in order: doors, care, equipment, loot.
const ROGUE_DEFAULTS: [(&str, &str, &str, [bool; 4]); 15] = [
    ("human", "passive", "explore", [true, true, true, true]),
    ("emu", "mean", "explore", [false, false, false, false]),
    ("snake", "mean", "explore", [false, false, false, false]),
    (
        "rattlesnake",
        "mean",
        "explore",
        [false, false, false, false],
    ),
    ("hobgoblin", "mean", "explore", [true, true, true, true]),
    ("zombie", "mean", "explore", [false, false, false, false]),
    ("quagga", "mean", "explore", [false, false, false, false]),
    ("centaur", "passive", "explore", [true, true, true, true]),
    ("yeti", "passive", "explore", [false, true, false, true]),
    ("troll", "mean", "explore", [true, true, true, true]),
    (
        "black_unicorn",
        "mean",
        "explore",
        [false, false, false, false],
    ),
    (
        "ice_monster",
        "passive",
        "wait",
        [false, false, false, false],
    ),
    ("vampire", "mean", "explore", [true, true, true, true]),
    ("wraith", "passive", "explore", [false, false, false, false]),
    (
        "jabberwock",
        "passive",
        "explore",
        [false, false, false, false],
    ),
];

fn expected_profile(idle: &str, tools: [bool; 4], flee: u32) -> Value {
    json!({"idle":idle,"doors":tools[0],"care":tools[1],
        "equipment":tools[2],"loot":tools[3],"gold":false,"flee_percent":flee})
}

fn saved_ai(game: &Game) -> Value {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    snapshot["combat"]["ai"]["3"].clone()
}

fn commit_ai(game: &mut Game) -> SimAction {
    assert_eq!(game.next_actor(), Some(Actor(3)));
    game.admit_ai_intention(Actor(3)).unwrap();
    let result = game.execute_next_intention().unwrap();
    result.outcome.unwrap();
    result.action.unwrap()
}

#[test]
fn all_fifteen_rogue_species_compile_exact_defaults_and_only_named_flee_override() {
    for (name, disposition, idle, tools) in ROGUE_DEFAULTS {
        let species = format!("rogue.{name}");
        for profile in [None, Some("rogue_hostile")] {
            let package =
                bounded_package("rogue-exploration", &species, profile, |_, _| {}).unwrap();
            let mut expected_behavior = expected_profile(idle, tools, 25);
            expected_behavior["disposition"] = json!(disposition);
            expected_behavior["memory_ticks"] = json!(1000);
            assert_eq!(
                serde_json::to_value(package.manifest.creatures.species[&species].behavior)
                    .unwrap(),
                expected_behavior,
                "{species}: authored species defaults"
            );
            let game = package.build(42, true).unwrap();
            assert_eq!(
                saved_ai(&game)["profile"],
                expected_profile(idle, tools, if profile.is_some() { 0 } else { 25 }),
                "{species}: effective profile {profile:?}"
            );
            assert!(
                !game.is_ai(Actor(1)),
                "a creature policy does not enroll the player"
            );
        }
    }
}

#[test]
fn every_rogue_species_uses_its_own_default_door_permission() {
    for (name, _, idle, tools) in ROGUE_DEFAULTS {
        let mut game = fixture(&format!("rogue.{name}"), None, false).unwrap();
        let door = game.place_door(location(1, 4, 2), false, 2).unwrap();
        let action = game.next_ai_action().unwrap().1;
        if tools[0] {
            assert_eq!(action, SimAction::SetDoor { door, open: true }, "{name}");
        } else if idle == "wait" {
            assert_eq!(action, SimAction::Wait, "{name}");
        } else {
            assert!(matches!(action, SimAction::Move(_)), "{name}: {action:?}");
        }
        assert_eq!(
            commit_ai(&mut game),
            action,
            "{name}: execution agrees with preview"
        );
        let state = game
            .observe(Actor(3))
            .unwrap()
            .visible_cells
            .into_iter()
            .filter_map(|cell| cell.door)
            .find(|state| state.id == door)
            .unwrap();
        assert_eq!(state.open, tools[0], "{name}: committed door state");
    }
}

#[test]
fn first_dungeon_scout_guardian_and_wisp_keep_distinct_idle_and_tool_defaults() {
    for (archetype, idle, tools) in [
        ("dungeon.scout", "explore", [true, true, true, true]),
        ("dungeon.guardian", "wait", [false, false, false, false]),
        ("dungeon.wisp", "explore", [false, false, false, false]),
    ] {
        for profile in [None, Some("tactical")] {
            let package = bounded_package("first-dungeon", archetype, profile, |_, _| {}).unwrap();
            let mut game = ready(&package);
            assert_eq!(
                saved_ai(&game)["profile"],
                expected_profile(idle, tools, 25),
                "{archetype}"
            );
            let door = game.place_door(location(1, 4, 2), false, 2).unwrap();
            let action = commit_ai(&mut game);
            match archetype {
                "dungeon.scout" => assert_eq!(action, SimAction::SetDoor { door, open: true }),
                "dungeon.guardian" => assert_eq!(action, SimAction::Wait),
                _ => assert!(matches!(action, SimAction::Move(_))),
            }
        }
    }
}

#[test]
fn omitted_species_behavior_and_empty_or_omitted_profile_use_the_generic_fallback() {
    for profile in [None, Some("inherited")] {
        let package = bounded_package(
            "rogue-exploration",
            "rogue.hobgoblin",
            profile,
            |manifest, _| {
                let species = manifest
                    .creatures
                    .species
                    .get_mut("rogue.hobgoblin")
                    .unwrap();
                let mut source = serde_json::to_value(&*species).unwrap();
                source.as_object_mut().unwrap().remove("behavior");
                *species = serde_json::from_value(source)
                    .expect("authoring omission has a generic policy");
                manifest.ai_profiles.insert(
                    "inherited".into(),
                    serde_json::from_value(json!({})).unwrap(),
                );
            },
        )
        .unwrap();
        let behavior = package.manifest.creatures.species["rogue.hobgoblin"].behavior;
        assert_eq!(
            behavior.disposition,
            crate::creature_authoring::Disposition::Passive
        );
        assert_eq!(behavior.memory_ticks, 1000);
        let mut game = ready(&package);
        assert_eq!(
            saved_ai(&game)["profile"],
            expected_profile("explore", [false; 4], 25)
        );
        let door = game.place_door(location(1, 4, 2), false, 2).unwrap();
        assert!(matches!(commit_ai(&mut game), SimAction::Move(_)));
        assert!(game
            .observe(Actor(3))
            .unwrap()
            .visible_cells
            .iter()
            .any(|cell| cell
                .door
                .is_some_and(|state| state.id == door && !state.open)));
    }
}

#[test]
fn partial_profile_fields_override_independently_including_false_wait_and_zero() {
    for (overrides, expected) in [
        (json!({}), expected_profile("explore", [true; 4], 25)),
        (
            json!({"doors":false}),
            expected_profile("explore", [false, true, true, true], 25),
        ),
        (
            json!({"care":false}),
            expected_profile("explore", [true, false, true, true], 25),
        ),
        (
            json!({"equipment":false}),
            expected_profile("explore", [true, true, false, true], 25),
        ),
        (
            json!({"loot":false}),
            expected_profile("explore", [true, true, true, false], 25),
        ),
        (json!({"gold":true}), {
            let mut expected = expected_profile("explore", [true; 4], 25);
            expected["gold"] = json!(true);
            expected
        }),
        (
            json!({"gold":false}),
            expected_profile("explore", [true; 4], 25),
        ),
        (
            json!({"idle":"wait"}),
            expected_profile("wait", [true; 4], 25),
        ),
        (
            json!({"flee_percent":0}),
            expected_profile("explore", [true; 4], 0),
        ),
        (
            json!({"idle":"wait","doors":false,"care":false,"equipment":false,"loot":false,"flee_percent":0}),
            expected_profile("wait", [false; 4], 0),
        ),
    ] {
        let package = bounded_package(
            "rogue-exploration",
            "rogue.hobgoblin",
            Some("partial"),
            |manifest, _| {
                manifest.ai_profiles.insert(
                    "partial".into(),
                    serde_json::from_value(overrides.clone()).unwrap(),
                );
            },
        )
        .unwrap();
        let game = ready(&package);
        assert_eq!(saved_ai(&game)["profile"], expected, "{overrides}");
        assert_eq!(
            matches!(game.next_ai_action(), Some((Actor(3), SimAction::Wait))),
            expected["idle"] == "wait",
            "{overrides}"
        );
    }
}

#[test]
fn a_partial_door_override_can_enable_an_animal_or_disable_a_tool_user() {
    for (species, opens) in [("rogue.emu", true), ("rogue.hobgoblin", false)] {
        let package = bounded_package(
            "rogue-exploration",
            species,
            Some("doors"),
            |manifest, _| {
                manifest.ai_profiles.insert(
                    "doors".into(),
                    serde_json::from_value(json!({"doors":opens})).unwrap(),
                );
            },
        )
        .unwrap();
        let mut game = ready(&package);
        let door = game.place_door(location(1, 4, 2), false, 2).unwrap();
        let action = commit_ai(&mut game);
        if opens {
            assert_eq!(action, SimAction::SetDoor { door, open: true });
        } else {
            assert!(matches!(action, SimAction::Move(_)));
        }
    }
}

#[test]
fn wait_override_blocks_a_useful_loot_detour_but_allows_useful_underfoot_collection() {
    for underfoot in [false, true] {
        let package = bounded_package(
            "rogue-exploration",
            "rogue.hobgoblin",
            Some("stationary"),
            |manifest, regions| {
                manifest.ai_profiles.insert(
                    "stationary".into(),
                    serde_json::from_value(json!({"idle":"wait"})).unwrap(),
                );
                regions[0].items.push(
                    serde_json::from_value(json!({
                        "id":1000,"at":[if underfoot {3} else {5},2,0],"archetype":"rogue.ring_mail"
                    }))
                    .unwrap(),
                );
            },
        )
        .unwrap();
        let mut game = ready(&package);
        assert!(game
            .observe(Actor(3))
            .unwrap()
            .ground_items
            .iter()
            .any(|item| item.id == tor_simulation::ItemId(1000)));
        let before = game.observe(Actor(3)).unwrap().location;
        let action = commit_ai(&mut game);
        if underfoot {
            assert_eq!(
                action,
                SimAction::Take {
                    item: tor_simulation::ItemId(1000),
                    quantity: Some(1)
                }
            );
            assert!(game
                .observe(Actor(3))
                .unwrap()
                .inventory
                .iter()
                .any(|item| item.id == tor_simulation::ItemId(1000)));
        } else {
            assert_eq!(action, SimAction::Wait);
        }
        assert_eq!(game.observe(Actor(3)).unwrap().location, before);
    }
}

#[test]
fn an_unknown_named_profile_does_not_silently_fall_back_to_species_defaults() {
    let failure = fixture("rogue.hobgoblin", Some("does_not_exist"), false).unwrap_err();
    assert_eq!(failure.code, ErrorCode::InvalidAction);
    assert!(
        failure
            .message
            .contains("Unknown actor AI profile \"does_not_exist\""),
        "{failure}"
    );
}

#[test]
fn omitted_or_explicit_external_control_does_not_enroll_a_creature_with_default_behavior() {
    for explicit in [false, true] {
        let package = bounded_package(
            "rogue-exploration",
            "rogue.hobgoblin",
            None,
            |_, regions| {
                let mut actor = serde_json::to_value(&regions[0].actors[0]).unwrap();
                if explicit {
                    actor["controller"] = json!("external");
                } else {
                    actor.as_object_mut().unwrap().remove("controller");
                }
                regions[0].actors[0] = serde_json::from_value(actor).unwrap();
            },
        )
        .unwrap();
        let game = ready(&package);
        assert!(!game.is_ai(Actor(3)));
        assert_eq!(game.next_ai_action(), None);
        assert!(saved_ai(&game).is_null());
    }
    let package = bounded_package(
        "rogue-exploration",
        "rogue.hobgoblin",
        Some("rogue_hostile"),
        |_, regions| {
            regions[0].actors[0].controller = "external".into();
        },
    );
    assert!(
        package.and_then(|package| package.build(42, true)).is_err(),
        "a named profile must not implicitly override explicit external control"
    );
}

#[test]
fn default_enrolled_twins_ignore_unseen_player_and_item_changes_through_committed_roaming() {
    for species in ["rogue.hobgoblin", "rogue.emu", "rogue.wraith"] {
        let mut original = fixture(species, None, false).unwrap();
        let mut changed = original.clone();
        changed.teleport(Actor(1), location(2, 6, 2)).unwrap();
        changed
            .place_item(location(2, 4, 2), "unseen treasure".into())
            .unwrap();
        for _ in 0..6 {
            assert_eq!(
                original.observe(Actor(3)).unwrap(),
                changed.observe(Actor(3)).unwrap()
            );
            let expected = original.next_ai_action().unwrap().1;
            assert!(matches!(expected, SimAction::Move(_)), "{species}");
            assert_eq!(changed.next_ai_action(), Some((Actor(3), expected.clone())));
            assert_eq!(commit_ai(&mut original), expected);
            assert_eq!(commit_ai(&mut changed), expected);
            assert_eq!(saved_ai(&original), saved_ai(&changed));
            for game in [&mut original, &mut changed] {
                for _ in 0..4 {
                    if game.next_actor() == Some(Actor(3)) {
                        break;
                    }
                    game.act(Actor(1), SimAction::Wait).unwrap();
                }
                assert_eq!(game.next_actor(), Some(Actor(3)));
            }
        }
        assert!(!saved_ai(&original)["visits"].as_array().unwrap().is_empty());
    }
}

#[test]
fn population_without_ai_remains_external_even_when_its_species_has_idle_defaults() {
    let mut scenario = single_member_foe_fixture(true);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    let stage = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][2];
    stage.as_object_mut().unwrap().remove("ai");
    stage["bands"][0]["depth"]["minimum"] = json!(1);
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    // One initial 1x1 generated member, no floor search or traversal setup.
    let engine = Engine::memory(scenario).unwrap();
    let born: Vec<_> = engine
        .actors()
        .into_iter()
        .filter(|actor| *actor != ActorId(1))
        .collect();
    assert_eq!(
        born.len(),
        1,
        "the control witness must actually create its actor"
    );
    assert!(
        !engine.is_ai(born[0]),
        "omitted population ai is external enrollment"
    );
}

fn idle_round(engine: &mut Engine, request: &str) {
    assert_eq!(engine.next_actor(), Some(ActorId(1)));
    engine
        .command(
            "player",
            "idle-default-test",
            ActorId(1),
            request,
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    let mut decisions = 0;
    for _ in 0..8 {
        if engine.next_actor() == Some(ActorId(1)) {
            break;
        }
        assert_eq!(engine.next_actor(), Some(ActorId(3)));
        assert!(matches!(
            engine.next_ai_action(),
            Some((ActorId(3), Action::Move { .. }))
        ));
        engine.advance_ai(ActorId(3)).unwrap();
        decisions += 1;
    }
    assert!(
        decisions > 0,
        "the round must commit default-driven roaming"
    );
    assert_eq!(engine.next_actor(), Some(ActorId(1)));
}

#[test]
fn default_enrollment_and_visited_history_survive_source_free_durable_reopen_and_rewind() {
    for checkpoint_interval in [0, 1] {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("idle-source");
        let package = bounded_package(
            "rogue-exploration",
            "rogue.hobgoblin",
            None,
            |manifest, regions| {
                manifest.characters[0].anchor = "1/start".into();
                regions[0].anchors.insert("start".into(), [2, 2, 0]);
                regions[0].actors[0].at = [12, 2, 0];
                regions[0]
                    .walls
                    .extend((0..9).flat_map(|y| (0..2).map(move |z| [8, y, z])));
                regions.truncate(1);
            },
        )
        .unwrap();
        crate::scenario_package::write_package(
            &source,
            &package.manifest,
            &package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap();
        crate::scenario_package::validate(&source).unwrap();
        let scenario = crate::scenario_package::load(&source, 42, None, false).unwrap();
        // One ordinary active room, with a wall separating personal knowledge.
        // Keep production streaming so the save pins its compiled region artifact.
        assert!(scenario.streaming.is_some());
        let path = directory.path().join("idle.db");
        let policy = crate::SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, scenario, policy.clone()).unwrap();
        engine.enable_wizard().unwrap();
        for index in 0..4 {
            idle_round(&mut engine, &format!("before-marker-{index}"));
        }
        let marker = engine
            .command(
                "player",
                "idle-default-test",
                ActorId(1),
                "retain-roaming-history",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    operation: WizardOperation::SetPlaceHint {
                        position: crate::journal::Position {
                            region: 1,
                            x: 2,
                            y: 2,
                            z: 0,
                        },
                        present: true,
                    },
                },
            )
            .unwrap();
        let retained_ai = saved_ai(&engine.game);
        let retained_observation = engine.game.observe(Actor(3)).unwrap();
        assert!(
            !retained_ai["visits"].as_array().unwrap().is_empty(),
            "non-vacuous personal history"
        );
        assert_eq!(
            retained_ai["profile"],
            expected_profile("explore", [true; 4], 25)
        );
        for index in 0..4 {
            idle_round(&mut engine, &format!("after-marker-{index}"));
        }
        assert_ne!(saved_ai(&engine.game)["visits"], retained_ai["visits"]);
        engine.flush().unwrap();
        let before = Engine::replay(engine.archive.clone(), None, None).unwrap();
        exact_equal(&before, &engine);
        drop(engine);
        std::fs::remove_dir_all(&source).unwrap();
        let mut resumed =
            Engine::open_with_policy(&path, Scenario::two_room(0), policy.clone()).unwrap();
        assert_eq!(
            resumed.recovery_profile().checkpoint_sequence > 0,
            checkpoint_interval == 1,
            "exercise independently verified journal-only and checkpoint recovery"
        );
        exact_equal(&resumed, &before);
        assert_eq!(saved_ai(&resumed.game), saved_ai(&before.game));
        restored_both_ways(&resumed);
        // Administrative permission is deliberately session-owned, not saved.
        resumed.enable_wizard().unwrap();
        resumed
            .command(
                "player",
                "idle-default-test",
                ActorId(1),
                "rewind-default-roaming",
                &resumed.branch().clone(),
                Command::Wizard {
                    expected_revision: resumed.revision(ActorId(1)).unwrap(),
                    operation: WizardOperation::Rewind {
                        target: Some(marker.entry.id),
                    },
                },
            )
            .unwrap();
        assert_eq!(saved_ai(&resumed.game), retained_ai);
        assert_eq!(
            resumed.game.observe(Actor(3)).unwrap(),
            retained_observation
        );
        assert!(resumed.is_ai(ActorId(3)));
        idle_round(&mut resumed, "roam-on-rewound-branch");
        resumed.flush().unwrap();
        let rewound = Engine::replay(resumed.archive.clone(), None, None).unwrap();
        drop(resumed);
        let reopened = Engine::open_with_policy(&path, Scenario::two_room(0), policy).unwrap();
        exact_equal(&reopened, &rewound);
        restored_both_ways(&reopened);
    }
}

#[test]
fn persisted_effective_ai_policy_requires_every_resolved_field_and_valid_values() {
    let game = fixture("rogue.hobgoblin", None, false).unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let saved = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    for field in [
        "idle",
        "doors",
        "care",
        "equipment",
        "loot",
        "gold",
        "flee_percent",
    ] {
        let mut malformed = saved.clone();
        malformed["combat"]["ai"]["3"]["profile"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(malformed).is_err(),
            "stored effective {field} cannot inherit authoring defaults"
        );
    }
    for (field, value) in [
        ("idle", json!("roam")),
        ("doors", Value::Null),
        ("care", json!(0)),
        ("gold", Value::Null),
        ("unexpected", json!(true)),
    ] {
        let mut malformed = saved.clone();
        malformed["combat"]["ai"]["3"]["profile"][field] = value;
        assert!(
            serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(malformed).is_err(),
            "{field}"
        );
    }
    let mut malformed = saved;
    malformed["combat"]["ai"]["3"]["profile"]["flee_percent"] = json!(101);
    let rejected = match serde_json::from_value::<tor_simulation::checkpoint::Snapshot>(malformed) {
        Err(_) => true,
        Ok(snapshot) => {
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err()
        }
    };
    assert!(
        rejected,
        "invalid resolved policy must never publish restored state"
    );
}
