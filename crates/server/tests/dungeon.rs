use std::path::Path;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{scenario_package, Engine};

fn dungeon() -> Engine {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon");
    Engine::memory(scenario_package::load(&path, 42, None, false).unwrap()).unwrap()
}

fn act(engine: &mut Engine, actor: ActorId, action: Action, sequence: &mut u64) {
    *sequence += 1;
    engine
        .command(
            "test",
            "headless",
            actor,
            &format!("dungeon-{sequence}"),
            &engine.branch().clone(),
            tor_server::journal::Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action,
            },
        )
        .unwrap();
}

fn pump(engine: &mut Engine, sequence: &mut u64) {
    for _ in 0..100 {
        let Some((actor, action)) = engine.next_ai_action() else {
            return;
        };
        act(engine, actor, action, sequence);
    }
    panic!("AI did not return to a player boundary");
}

#[test]
fn authored_dungeon_completes_retrieval_and_escape() {
    let mut engine = dungeon();
    let mut sequence = 0;
    for _ in 0..160 {
        pump(&mut engine, &mut sequence);
        let state = engine.state(ActorId(1)).unwrap();
        assert!(!state.observation.combat.as_ref().unwrap().dead);
        if state.observation.combat.as_ref().unwrap().victory {
            return;
        }
        let returning = state.observation.inventory.iter().any(|i| i.id == 100);
        let nearby = state.observation.visible_actors.iter().find(|a| {
            a.id != ActorId(1)
                && a.position.x.abs() <= 1
                && a.position.y.abs() <= 1
                && a.position.z == 0
        });
        let action = nearby.map_or(
            Action::Move {
                direction: if returning {
                    Direction::West
                } else {
                    Direction::East
                },
            },
            |a| Action::Attack { target: a.id },
        );
        let action = if state
            .observation
            .ground_items
            .iter()
            .any(|i| i.item.id == 100 && i.reachable)
        {
            Action::Take {
                item: 100,
                quantity: None,
            }
        } else {
            action
        };
        if engine
            .command(
                "test",
                "headless",
                ActorId(1),
                &format!("player-{sequence}"),
                &engine.branch().clone(),
                tor_server::journal::Command::Act {
                    expected_revision: state.revision,
                    action,
                },
            )
            .is_err()
        {
            act(&mut engine, ActorId(1), Action::Wait, &mut sequence);
        }
        sequence += 1;
    }
    panic!("dungeon did not reach victory within the bounded walkthrough");
}

#[test]
fn victory_death_and_pending_attacks_survive_durable_restart() {
    for package in ["dungeon-loop", "dungeon-death"] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests")
            .join(package);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.db");
        let mut engine = Engine::open(
            &path,
            scenario_package::load(&root, 42, None, false).unwrap(),
        )
        .unwrap();
        let mut sequence = 0;
        act(
            &mut engine,
            ActorId(1),
            Action::Attack { target: ActorId(2) },
            &mut sequence,
        );
        let pending = engine.state(ActorId(1)).unwrap();
        assert!(
            pending
                .observation
                .combat
                .as_ref()
                .unwrap()
                .preparation_active
        );
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&path, tor_server::Scenario::two_room(0)).unwrap();
        assert_eq!(engine.state(ActorId(1)).unwrap(), pending);
        pump(&mut engine, &mut sequence);
        if package == "dungeon-loop" {
            for _ in 0..3 {
                act(
                    &mut engine,
                    ActorId(1),
                    Action::Move {
                        direction: Direction::East,
                    },
                    &mut sequence,
                );
            }
            act(
                &mut engine,
                ActorId(1),
                Action::Take {
                    item: 100,
                    quantity: None,
                },
                &mut sequence,
            );
            for _ in 0..3 {
                act(
                    &mut engine,
                    ActorId(1),
                    Action::Move {
                        direction: Direction::West,
                    },
                    &mut sequence,
                );
            }
            assert!(
                engine
                    .state(ActorId(1))
                    .unwrap()
                    .observation
                    .combat
                    .as_ref()
                    .unwrap()
                    .victory
            );
        } else {
            assert!(
                engine
                    .state(ActorId(1))
                    .unwrap()
                    .observation
                    .combat
                    .as_ref()
                    .unwrap()
                    .dead
            );
        }
        let terminal = engine.state(ActorId(1)).unwrap();
        assert!(terminal.observation.combat.as_ref().unwrap().terminal);
        assert!(engine
            .command(
                "test",
                "headless",
                ActorId(1),
                "terminal",
                &engine.branch().clone(),
                tor_server::journal::Command::Act {
                    expected_revision: terminal.revision,
                    action: Action::Wait
                }
            )
            .is_err());
        engine.flush().unwrap();
        drop(engine);
        let engine = Engine::open(&path, tor_server::Scenario::two_room(99)).unwrap();
        assert_eq!(engine.state(ActorId(1)).unwrap(), terminal);
    }
}

#[test]
fn optional_starting_ai_keeps_inventory_and_higher_id_human_gets_input_boundary() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-characters");
    let engine = Engine::memory(scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
    assert!(engine.is_ai(ActorId(1)));
    assert!(engine.state(ActorId(7)).unwrap().observation.ready);
    assert!(engine.next_ai_action().is_none());
    assert!(engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .inventory
        .iter()
        .any(|i| i.id == 100));
    let view = engine.state(ActorId(7)).unwrap().observation;
    assert!(view.combat.as_ref().unwrap().victory);
    assert!(view.combat.as_ref().unwrap().objective.is_none());
    assert!(!view.combat.as_ref().unwrap().terminal);
}

#[test]
fn suspension_is_journaled_idempotent_and_durable_with_exact_resumption() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-loop");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pause.db");
    let mut engine = Engine::open(
        &path,
        scenario_package::load(&root, 42, None, false).unwrap(),
    )
    .unwrap();
    let mut sequence = 0;
    act(
        &mut engine,
        ActorId(1),
        Action::Attack { target: ActorId(2) },
        &mut sequence,
    );
    let remaining = engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .preparation_remaining;
    let paused = engine.pause_preparation(ActorId(1)).unwrap().unwrap();
    assert!(matches!(
        paused.entry.disclosed().content,
        tor_protocol::HistoryContent::Action {
            event: tor_protocol::Event::PreparationPaused,
            ..
        }
    ));
    assert!(engine.pause_preparation(ActorId(1)).unwrap().is_none());
    let expected = engine.state(ActorId(1)).unwrap();
    assert!(expected.observation.ready);
    assert!(
        !expected
            .observation
            .combat
            .as_ref()
            .unwrap()
            .preparation_active
    );
    assert_eq!(
        expected
            .observation
            .combat
            .as_ref()
            .unwrap()
            .preparation_remaining,
        remaining
    );
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, tor_server::Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    act(
        &mut engine,
        ActorId(1),
        Action::Attack { target: ActorId(2) },
        &mut sequence,
    );
    pump(&mut engine, &mut sequence);
    assert!(!engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .visible_actors
        .iter()
        .any(|a| a.id == ActorId(2)));
    assert!(
        tor_protocol::Command::try_from(tor_server::journal::Command::PausePreparation).is_err()
    );
}

#[test]
fn active_ai_memory_survives_forced_checkpoint_and_restart() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-loop");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("ai-checkpoint.db");
    let mut engine = Engine::open_with_policy(
        &path,
        scenario_package::load(&root, 42, None, false).unwrap(),
        tor_server::SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut sequence = 0;
    act(
        &mut engine,
        ActorId(1),
        Action::Attack { target: ActorId(2) },
        &mut sequence,
    );
    pump(&mut engine, &mut sequence);
    let expected = engine.state(ActorId(1)).unwrap();
    engine.flush().unwrap();
    drop(engine);
    let engine = Engine::open(&path, tor_server::Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    assert!(engine.recovery_profile().checkpoint_sequence > 0);
}

#[test]
fn stationary_attack_does_not_rebuild_unchanged_navigation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-loop");
    let mut engine =
        Engine::memory(scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
    let (_, profile) = engine
        .command_profiled(
            "test",
            "headless",
            ActorId(1),
            "attack",
            &engine.branch().clone(),
            tor_server::journal::Command::Act {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: Action::Attack { target: ActorId(2) },
            },
        )
        .unwrap();
    assert_eq!(profile.navigation_refreshes, 0);
    assert_eq!(engine.state(ActorId(1)).unwrap().observation.tick, 0);
    assert!(
        engine
            .state(ActorId(1))
            .unwrap()
            .observation
            .combat
            .unwrap()
            .preparation_active
    );
}
