use std::path::Path;
use tor_protocol::ActorId;
use tor_server::journal::Action;
use tor_server::{
    journal::{Command, WizardOperation},
    scenario_package, Engine, SavePolicy, Scenario,
};

#[test]
fn physics_replay_checkpoint_retry_and_rewind_preserve_motion() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    for interval in [1, 100] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("physics.db");
        let policy = SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let scenario = scenario_package::load(&root, 42, None, false).unwrap();
        let mut engine = Engine::open_with_policy(&path, scenario, policy.clone()).unwrap();
        engine.enable_wizard().unwrap();
        let actor = ActorId(1);
        let initial = engine.state(actor).unwrap().observation;
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: engine.revision(actor).unwrap(),
            action: Action::Wait,
        };
        engine
            .command(
                "player",
                "physics-test",
                actor,
                "fall",
                &branch,
                command.clone(),
            )
            .unwrap();
        let expected = engine.state(actor).unwrap();
        assert!(expected.observation.motion.as_ref().unwrap().displaced);
        assert!(expected.observation.motion.as_ref().unwrap().velocity[2] < 0);
        engine
            .command("player", "physics-test", actor, "fall", &branch, command)
            .unwrap();
        assert_eq!(engine.state(actor).unwrap(), expected);
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, Scenario::two_room(99), policy).unwrap();
        assert_eq!(engine.state(actor).unwrap(), expected);
        engine.enable_wizard().unwrap();
        engine
            .command(
                "wizard",
                "physics-test",
                actor,
                "rewind",
                &branch,
                Command::Wizard {
                    expected_revision: engine.revision(actor).unwrap(),
                    operation: WizardOperation::Rewind { target: None },
                },
            )
            .unwrap();
        assert_eq!(engine.state(actor).unwrap().observation, initial);
        engine
            .command(
                "player",
                "physics-test",
                actor,
                "fall-again",
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert_eq!(
            engine.state(actor).unwrap().observation,
            expected.observation
        );
    }
}

#[test]
fn privileged_physics_edits_are_journaled_and_invalid_bodies_are_atomic() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    let mut engine =
        Engine::memory(scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
    engine.enable_wizard().unwrap();
    let actor = ActorId(1);
    let before = engine.state(actor).unwrap();
    let branch = engine.branch().clone();
    assert!(engine
        .command(
            "wizard",
            "physics-test",
            actor,
            "bad-body",
            &branch,
            Command::Wizard {
                expected_revision: before.revision,
                operation: WizardOperation::SetBody {
                    actor,
                    cells: vec![[0, 0, 0], [0, 0, 8]],
                    eye: [0, 0, 0],
                    mass: 80
                }
            }
        )
        .is_err());
    assert_eq!(engine.state(actor).unwrap(), before);
    engine
        .command(
            "wizard",
            "physics-test",
            actor,
            "zero-g",
            &branch,
            Command::Wizard {
                expected_revision: before.revision,
                operation: WizardOperation::SetGravity {
                    region: 1,
                    vector: [0; 3],
                },
            },
        )
        .unwrap();
    engine
        .command(
            "player",
            "physics-test",
            actor,
            "wait",
            &branch,
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    assert_eq!(
        engine
            .state(actor)
            .unwrap()
            .observation
            .motion
            .unwrap()
            .velocity,
        [0; 3]
    );
    assert!(engine.state(actor).unwrap().wizard_game);
}

#[test]
fn resting_gravity_wait_uses_readiness_fast_path() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    let mut engine =
        Engine::memory(scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
    let actor = ActorId(1);
    for turn in 0..8 {
        engine
            .command(
                "test",
                "test",
                actor,
                &format!("settle-{turn}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
    }
    let (_, profile) = engine
        .command_profiled(
            "test",
            "test",
            actor,
            "resting",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    assert_eq!(profile.scene_calls, 0);
    assert_eq!(profile.navigation_refreshes, 0);
    assert_eq!(
        engine
            .state(actor)
            .unwrap()
            .observation
            .motion
            .unwrap()
            .velocity,
        [0; 3]
    );
}

#[test]
fn unsettled_physics_refreshes_navigation_only_when_its_inputs_change() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    let template = scenario_package::load(&root, 42, None, false).unwrap();
    let package = template.package.as_ref().unwrap();
    let mut regions = package.region_defs().unwrap();
    regions[0].actors.push(scenario_package::Actor {
        anatomy: None,
        known_identities: vec![],
        combat: None,
        id: 2,
        at: [5, 2, 6],
        archetype: None,
        turn_ticks: Some(100),
        controller: "external".into(),
        ai: None,
        body: None,
        velocity: None,
    });
    let directory = tempfile::tempdir().unwrap();
    scenario_package::write_package(directory.path(), &package.manifest, &regions).unwrap();
    scenario_package::validate(directory.path()).unwrap();
    let mut engine =
        Engine::memory(scenario_package::load(directory.path(), 42, None, false).unwrap()).unwrap();
    for (step, (actor, refreshes)) in [(ActorId(1), 0), (ActorId(2), 0), (ActorId(1), 0)]
        .into_iter()
        .enumerate()
    {
        let (_, profile) = engine
            .command_profiled(
                "test",
                "test",
                actor,
                &format!("wait-{step}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        if step > 0 {
            assert!(
                profile.scene_calls > 0,
                "unsettled physics still requires perception"
            );
        }
        assert_eq!(profile.navigation_refreshes, refreshes);
        if step == 1 {
            for id in engine.actors() {
                assert!(
                    engine
                        .state(id)
                        .unwrap()
                        .observation
                        .motion
                        .unwrap()
                        .displaced
                );
            }
        }
    }
}

#[test]
fn translating_an_unchanged_visible_chart_does_not_refresh_navigation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    let template = scenario_package::load(&root, 42, None, false).unwrap();
    let package = template.package.as_ref().unwrap();
    let mut regions = package.region_defs().unwrap();
    regions[0].size = [5, 5, 1];
    regions[0].chamber = false;
    regions[0].gravity = None;
    regions[0].anchors.insert("start".into(), [1, 1, 0]);
    regions[0].items.clear();
    let mut manifest = package.manifest.clone();
    manifest.characters[0].body = None;
    let directory = tempfile::tempdir().unwrap();
    scenario_package::write_package(directory.path(), &manifest, &regions).unwrap();
    scenario_package::validate(directory.path()).unwrap();
    let mut engine =
        Engine::memory(scenario_package::load(directory.path(), 42, None, false).unwrap()).unwrap();
    let actor = ActorId(1);
    let before = engine.state(actor).unwrap().observation;
    let (_, profile) = engine
        .command_profiled(
            "test",
            "test",
            actor,
            "translate",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Move {
                    direction: tor_server::journal::Direction::East,
                },
            },
        )
        .unwrap();
    assert_eq!(profile.navigation_refreshes, 0);
    let after = engine.state(actor).unwrap().observation;
    let origin_key = |observation: &tor_protocol::Observation| {
        observation
            .visible_cells
            .iter()
            .find(|cell| cell.position == observation.position)
            .unwrap()
            .key
            .clone()
    };
    assert_ne!(origin_key(&before), origin_key(&after));
    assert_eq!(before.visible_cells.len(), after.visible_cells.len());
}
