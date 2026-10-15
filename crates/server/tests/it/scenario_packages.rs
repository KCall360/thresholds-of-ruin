use crate::support;
use std::path::Path;
use tor_server::{scenario_package, Engine};

fn copied_package() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let temp = tempfile::tempdir().unwrap();
    support::copy_package(&root, temp.path());
    temp
}

#[test]
fn combat_fixtures_declare_owned_builds_without_flat_profiles() {
    let mut builds = 0;
    let mut cave_rats = 0;
    for path in support::all_packages() {
        let name = path.file_name().unwrap().to_str().unwrap();
        let scenario = scenario_package::load(&path, 42, None, false).unwrap();
        let package = scenario.package.as_ref().unwrap();
        let catalog = package
            .manifest
            .creatures
            .compile(package.rules().unwrap())
            .unwrap();
        let mut check = |creature: Option<&tor_server::creature_authoring::BuildSpec>| {
            if let Some(source) = creature {
                let build = catalog.prepare(source).unwrap().instantiate(42).unwrap();
                assert!(build.ledger().total_hd() > tor_simulation::HdCount(0));
                if name == "generated-filler" && source.species == "cave_rat" {
                    cave_rats += 1;
                    let derived = build.derive().unwrap();
                    assert_eq!(
                        derived.kind,
                        tor_simulation::content_ids::TypeId::new("animal").unwrap()
                    );
                    assert_eq!(
                        derived.selected_attack.attack.skill(),
                        tor_simulation::attributes::Skill::LightWeaponry
                    );
                    assert!(derived.anatomy.slots.is_empty());
                    assert_eq!(derived.maximum_health, 8);
                }
                builds += 1;
            }
        };
        for character in &package.manifest.characters {
            check(character.creature.as_ref());
        }
        for archetype in package.manifest.archetypes.values() {
            check(archetype.creature.as_ref());
        }
        for region in package.region_defs().unwrap() {
            for actor in &region.actors {
                check(actor.creature.as_ref());
            }
        }
    }
    assert!(builds > 0, "owned-build validation must be exercised");
    assert!(
        cave_rats > 0,
        "the cave-rat semantic fixture must remain covered"
    );
}

#[test]
fn all_converted_packages_have_current_validation_and_start_without_wizard() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests");
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let scenario = scenario_package::load(&path, 42, None, false)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let actor = scenario.package.as_ref().unwrap().selected;
        let engine = Engine::memory(scenario).unwrap();
        assert!(
            !engine
                .state(tor_protocol::ActorId(actor))
                .unwrap()
                .wizard_game
        );
        assert_eq!(engine.scenario_validation(), Some(true));
    }
}

#[test]
fn selecting_character_omits_other_inventory_and_resolves_zone_theme_replacement() {
    let temp = copied_package();
    edit(
        temp.path(),
        "scenario.toml",
        "characters = [",
        "characters = [{ id = 7, anchor = \"2/landing\", turn_ticks = 50 }, ",
    );
    edit(
        temp.path(),
        "regions",
        "\"archetype\" = \"tablet\"",
        "\"archetype\" = \"tablet\", carried_by = 7",
    );
    scenario_package::validate(temp.path()).unwrap();
    let scenario = scenario_package::load(temp.path(), 42, Some(7), false).unwrap();
    assert_eq!(
        scenario.package.as_ref().unwrap().region_themes(1).unwrap(),
        &["stone"]
    );
    let engine = Engine::memory(scenario).unwrap();
    assert_eq!(engine.actors(), vec![tor_protocol::ActorId(7)]);
    assert_eq!(
        engine
            .state(tor_protocol::ActorId(7))
            .unwrap()
            .observation
            .inventory
            .len(),
        1
    );
    let other =
        Engine::memory(scenario_package::load(temp.path(), 42, None, false).unwrap()).unwrap();
    assert_eq!(other.actors(), vec![tor_protocol::ActorId(1)]);
    assert!(scenario_package::load(temp.path(), 42, Some(999), false).is_err());
    edit(
        temp.path(),
        "scenario.toml",
        "\"entry\" = {  }",
        "\"entry\" = { themes = [\"moss\"] }",
    );
    scenario_package::validate(temp.path()).unwrap();
    let scenario = scenario_package::load(temp.path(), 42, None, false).unwrap();
    let package = scenario.package.as_ref().unwrap();
    assert_eq!(package.region_themes(1).unwrap(), &["moss"]);
    assert_eq!(package.region_themes(2).unwrap(), &["stone"]);
}

/// Edit the manifest, or with `file` "regions", the region file holding `from`.
fn edit(dir: &Path, file: &str, from: &str, to: &str) {
    if file == "regions" {
        return support::edit_region(dir, from, to);
    }
    let path = dir.join(file);
    let before = std::fs::read_to_string(&path).unwrap();
    assert!(before.contains(from), "Missing test edit: {from}");
    std::fs::write(path, before.replace(from, to)).unwrap();
}

#[test]
fn invalid_references_geometry_versions_and_unsupported_mechanics_have_diagnostics() {
    for (file, from, to, expected) in [
        (
            "scenario.toml",
            "version = \"1.1\"",
            "version = \"latest\"",
            "major.minor",
        ),
        (
            "scenario.toml",
            "rules.toml",
            "missing.toml",
            "missing.toml",
        ),
        ("scenario.toml", "1/start", "1/missing", "anchor"),
        ("regions", "size = [6, 3, 2]", "size = [0, 3, 2]", "bounds"),
        (
            "regions",
            "size = [6, 3, 2]",
            "size = [6, 3, 2]\nlighting = [{at=[99,0,0],lit=true}]",
            "lighting",
        ),
        (
            "regions",
            "size = [6, 3, 2]",
            "size = [6, 3, 2]\nlighting = [{at=[1,1,0],lit=true},{at=[1,1,0],lit=false}]",
            "lighting",
        ),
        ("regions", "2/landing", "999/landing", "anchor"),
        (
            "regions",
            "\"archetype\" = \"tablet\"",
            "\"archetype\" = \"missing\"",
            "archetype",
        ),
        // Every declared body names its eye cell, and it must be a body cell.
        ("scenario.toml", "eye = [0,0,1], ", "", "eye"),
        (
            "scenario.toml",
            "eye = [0,0,1]",
            "eye = [0,0,2]",
            "eye must be one of its cells",
        ),
        // Doors fit their space and don't leave a walled doorway open above.
        (
            "regions",
            "\"open\" = true, \"height\" = 2",
            "\"open\" = true, \"height\" = 3",
            "at most 2 cells tall",
        ),
        (
            "regions",
            "\"open\" = true, \"height\" = 2",
            "\"open\" = true, \"height\" = 1",
            "shorter than its doorway",
        ),
    ] {
        let temp = copied_package();
        edit(temp.path(), file, from, to);
        let error = scenario_package::validate(temp.path()).unwrap_err();
        assert!(error.message.contains(expected), "{error}");
    }
    {
        let (file, text, expected) = (
            "scenario.toml",
            "\nobjective = { anchor = \"1/start\", disclosed = true, continue_play = false }\n",
            "victory",
        );
        let temp = copied_package();
        let path = temp.path().join(file);
        let mut data = std::fs::read_to_string(&path).unwrap();
        data.push_str(text);
        let mut manifest: scenario_package::Manifest = toml::from_str(&data).unwrap();
        let definitions: scenario_package::Manifest = toml::from_str(include_str!(
            "../../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        manifest.creatures = definitions.creatures;
        manifest.characters[0].creature = definitions.characters[0].creature.clone();
        std::fs::write(path, toml::to_string(&manifest).unwrap()).unwrap();
        scenario_package::validate(temp.path()).unwrap();
        let scenario = scenario_package::load(temp.path(), 42, None, false).unwrap();
        let engine = Engine::memory(scenario).unwrap();
        assert!(
            !engine
                .state(tor_protocol::ActorId(1))
                .unwrap()
                .observation
                .ready
        );
        assert!(
            engine
                .state(tor_protocol::ActorId(1))
                .unwrap()
                .observation
                .combat
                .unwrap()
                .victory,
            "{expected}"
        );
    }
}

#[test]
fn pinned_inputs_survive_source_edits_and_checkpoint_restart() {
    let temp = copied_package();
    let scenario = scenario_package::load(temp.path(), 42, None, false).unwrap();
    let save = temp.path().join("save.db");
    let policy = tor_server::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut engine = Engine::open_with_policy(&save, scenario, policy.clone()).unwrap();
    let actor = tor_protocol::ActorId(1);
    engine
        .command(
            "test",
            "test",
            actor,
            "wait",
            &engine.branch().clone(),
            tor_server::journal::Command::Act {
                expected_revision: 0,
                action: tor_server::journal::Action::Wait,
            },
        )
        .unwrap();
    let expected = engine.state(actor).unwrap();
    engine.flush().unwrap();
    drop(engine);
    edit(temp.path(), "scenario.toml", "1/start", "1/missing");
    let engine =
        Engine::open_with_policy(&save, tor_server::Scenario::two_room(999), policy).unwrap();
    assert_eq!(engine.state(actor).unwrap(), expected);
    assert_eq!(engine.scenario_validation(), Some(true));
}

#[test]
fn wizard_lineage_is_independent_of_structural_validation_and_rewind() {
    use tor_server::journal::{Command, Position, WizardOperation};
    let temp = copied_package();
    let scenario = scenario_package::load(temp.path(), 42, None, false).unwrap();
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    assert_eq!(engine.scenario_validation(), Some(true));
    let actor = tor_protocol::ActorId(1);
    let mut apply = |operation| {
        engine
            .command(
                "wizard",
                "test",
                actor,
                &uuid::Uuid::new_v4().to_string(),
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: engine.revision(actor).unwrap(),
                    operation,
                },
            )
            .unwrap();
        engine.scenario_validation()
    };
    assert_eq!(
        apply(WizardOperation::SetWall {
            position: Position {
                region: 2,
                x: 0,
                y: 1,
                z: 0
            },
            wall: true
        }),
        Some(false)
    );
    assert_eq!(apply(WizardOperation::Rewind { target: None }), Some(true));
    assert!(engine.state(actor).unwrap().wizard_game);
}

#[test]
fn ordinary_package_has_no_wizard_history_and_rejects_stale_validation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let scenario = scenario_package::load(&root, 42, None, false).unwrap();
    let engine = Engine::memory(scenario).unwrap();
    assert!(!engine.state(tor_protocol::ActorId(1)).unwrap().wizard_game);
    let temp = tempfile::tempdir().unwrap();
    support::copy_package(&root, temp.path());
    // A region file edited since validation is refused when its region is
    // built, without reading the rest of the package first.
    let path = temp.path().join("regions/1.toml");
    let mut contents = std::fs::read_to_string(&path).unwrap();
    contents.push_str("\n# authored edit\n");
    std::fs::write(path, contents).unwrap();
    let scenario = scenario_package::load(temp.path(), 42, None, false).unwrap();
    let error = Engine::memory(scenario).unwrap_err();
    assert!(error.message.contains("changed since"), "{error}");
    scenario_package::validate(temp.path()).unwrap();
    assert!(Engine::memory(scenario_package::load(temp.path(), 42, None, false).unwrap()).is_ok());
    // An edited manifest is refused when the package loads.
    let path = temp.path().join("scenario.toml");
    let mut contents = std::fs::read_to_string(&path).unwrap();
    contents.push_str("\n# authored edit\n");
    std::fs::write(path, contents).unwrap();
    assert!(scenario_package::load(temp.path(), 42, None, false).is_err());
}

#[test]
fn an_authored_place_name_is_disclosed_with_its_origin_and_validated() {
    use tor_protocol::{ActorId, PlaceNameOrigin};
    // The minimal example names its rooms.
    let root = support::package("two-room");
    let engine = Engine::memory(scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
    let places = engine.state(ActorId(1)).unwrap().observation.places;
    assert!(places
        .iter()
        .any(|p| p.name == "Entry chamber" && p.origin == PlaceNameOrigin::Authored));
    // A name must be a valid place name.
    let temp = copied_package();
    edit(
        temp.path(),
        "regions",
        "name = \"Gallery\" }",
        "name = \" \" }",
    );
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("place"), "{error}");
}

#[test]
fn rogue_selected_presets_publish_real_equipped_kits_and_four_ordinary_foes() {
    for selected in [1, 2] {
        let scenario = scenario_package::load(
            &support::package("rogue-exploration"),
            42,
            Some(selected),
            false,
        )
        .unwrap();
        let engine = Engine::memory(scenario).unwrap();
        assert_eq!(
            engine.actors().len(),
            5,
            "seed 42 must place all four authored foes"
        );
        assert!(
            !engine
                .actors()
                .contains(&tor_protocol::ActorId(if selected == 1 { 2 } else { 1 })),
            "an omitted preset identity cannot be reused for a foe"
        );
        let state = engine.state(tor_protocol::ActorId(selected)).unwrap();
        assert!(!state.wizard_game);
        let view = serde_json::to_value(state.observation).unwrap();
        let items = view["inventory"].as_array().unwrap();
        assert_eq!(items.len(), 4);
        let quantities: std::collections::BTreeMap<_, _> = items
            .iter()
            .map(|item| {
                (
                    item["name"].as_str().unwrap(),
                    item["quantity"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            quantities,
            std::collections::BTreeMap::from([
                ("mace", "1"),
                ("ring mail", "1"),
                ("clean bandage", "3"),
                ("food ration", "1")
            ])
        );
        assert!(view["combat"].is_object());
        assert_eq!(
            view["interactions"]["inventory"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| !row["equipped_slot"].is_null())
                .count(),
            2
        );
    }
}

#[test]
fn rogue_geometry_fixture_stays_population_free_for_exploration_and_performance() {
    let engine = Engine::memory(support::load("rogue-geometry", 42)).unwrap();
    assert_eq!(engine.actors(), vec![tor_protocol::ActorId(1)]);
    let state = engine.state(tor_protocol::ActorId(1)).unwrap();
    assert!(state.observation.inventory.is_empty());
    assert!(state.observation.combat.is_none());
}

#[test]
fn declared_remote_character_roots_resume_and_traverse_the_selected_bounded_closure() {
    use tor_protocol::ActorId;
    use tor_server::journal::{Action, Direction};
    use tor_server::{Scenario, Streaming};

    let template =
        scenario_package::load(&support::package("rogue-geometry"), 42, None, true).unwrap();
    let mut manifest = template.package.as_ref().unwrap().manifest.clone();
    manifest.characters[0].anchor = "dungeon/26/up".into();
    let mut alternate = manifest.characters[0].clone();
    alternate.id = 2;
    alternate.anchor = "dungeon/999999/down".into();
    manifest.characters.push(alternate);
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("package");
    let rules =
        std::fs::read_to_string(support::package("rogue-geometry").join("rules.toml")).unwrap();
    scenario_package::write_package(&package, &manifest, &[], &rules).unwrap();
    let certificate = scenario_package::validate(&package).unwrap();
    assert_eq!(certificate.regions, 0);
    let mut saved = Vec::new();
    for (selected, direction) in [(1, Direction::Up), (2, Direction::Down)] {
        let mut scenario = scenario_package::load(&package, 42, Some(selected), false).unwrap();
        scenario.streaming = Some(Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let save = directory.path().join(format!("character-{selected}.db"));
        let engine = Engine::open(&save, scenario).unwrap();
        assert_eq!(engine.actors(), vec![ActorId(selected)]);
        let counts = engine.region_counts().unwrap();
        // Named-stair awareness may pin either adjacent floor even with a
        // zero-hop horizon. The remote omitted character must not add a root.
        assert!((9..=27).contains(&(counts.active + counts.frozen + counts.detached)));
        assert!(counts.unbuilt <= 6);
        let state = engine.state(ActorId(selected)).unwrap();
        engine.flush().unwrap();
        drop(engine);
        saved.push((selected, direction, save, state, counts));
    }
    std::fs::remove_dir_all(package).unwrap();
    for (selected, direction, save, state, counts) in saved {
        let mut restored = Engine::open(&save, Scenario::two_room(0)).unwrap();
        assert_eq!(restored.actors(), vec![ActorId(selected)]);
        assert_eq!(restored.state(ActorId(selected)).unwrap(), state);
        assert_eq!(restored.region_counts().unwrap(), counts);
        support::act_as(&mut restored, ActorId(selected), Action::Move { direction }).unwrap();
        let traveled = restored.state(ActorId(selected)).unwrap();
        assert_ne!(traveled.observation, state.observation);
        let counts = restored.region_counts().unwrap();
        assert!((18..=36).contains(&(counts.active + counts.frozen + counts.detached)));
        assert!(counts.unbuilt <= 6);
        restored.flush().unwrap();
        drop(restored);
        let reopened = Engine::open(&save, Scenario::two_room(0)).unwrap();
        assert_eq!(reopened.state(ActorId(selected)).unwrap(), traveled);
    }
}
