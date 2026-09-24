use tor_protocol::{Action, ActorId, Direction};
use tor_server::{
    journal::{Command, HistoryContent},
    Engine, Scenario,
};
use tor_test_support::performance::Trace;

#[test]
fn same_tick_turn_handoff_changes_both_actors_disclosed_readiness_revisions() {
    let mut engine = Engine::memory(Scenario::performance(42, 8, 8).unwrap()).unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    let next = engine.state(ActorId(2)).unwrap();
    step(&mut engine, Action::Wait);
    let after = engine.state(ActorId(1)).unwrap();
    let ready = engine.state(ActorId(2)).unwrap();
    assert_eq!(after.observation.tick, before.observation.tick);
    assert!(!after.observation.ready && ready.observation.ready);
    assert!(after.revision > before.revision && ready.revision > next.revision);
}

fn step(engine: &mut Engine, action: Action) {
    let actor = ActorId(1);
    let revision = engine.revision(actor).unwrap();
    let result = engine
        .command(
            "trace",
            "test",
            actor,
            &format!("step-{revision}"),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: revision,
                action,
            },
        )
        .unwrap();
    assert!(matches!(
        result.entry.content,
        HistoryContent::Action { .. }
    ));
}

#[test]
fn performance_fixture_has_real_door_stairs_and_exact_region_counts() {
    for regions in [1, 8, 64, 256] {
        let mut engine = Engine::memory(Scenario::performance(42, regions, 1).unwrap()).unwrap();
        let initial = engine.state(ActorId(1)).unwrap();
        assert!(initial
            .observation
            .visible_cells
            .iter()
            .any(|c| c.stairs_up));
        step(
            &mut engine,
            Action::Move {
                direction: Direction::Up,
            },
        );
        assert_ne!(
            engine.state(ActorId(1)).unwrap().observation.visible_cells,
            initial.observation.visible_cells
        );
        step(
            &mut engine,
            Action::Move {
                direction: Direction::Down,
            },
        );
        step(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        );
        let before = engine.state(ActorId(1)).unwrap();
        let door = before
            .observation
            .visible_cells
            .iter()
            .find_map(|c| c.door.as_ref())
            .unwrap();
        assert!(!door.open && door.reachable);
        let id = door.id;
        let revision = before.revision;
        assert!(engine
            .command(
                "trace",
                "test",
                ActorId(1),
                "blocked",
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: revision,
                    action: Action::Move {
                        direction: Direction::East
                    }
                }
            )
            .is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        step(
            &mut engine,
            Action::SetDoor {
                door: id,
                open: true,
            },
        );
        assert!(
            engine
                .state(ActorId(1))
                .unwrap()
                .observation
                .visible_cells
                .len()
                > before.observation.visible_cells.len()
        );
        step(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        );
        step(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        );
        step(
            &mut engine,
            Action::SetDoor {
                door: id,
                open: false,
            },
        );
        assert!(
            !engine
                .state(ActorId(1))
                .unwrap()
                .observation
                .visible_cells
                .iter()
                .find_map(|c| c.door.as_ref())
                .unwrap()
                .open
        );
        // Three interior moves, then an actual forward/reverse boundary crossing.
        for _ in 0..3 {
            step(
                &mut engine,
                Action::Move {
                    direction: Direction::East,
                },
            );
        }
        let boundary = engine.state(ActorId(1)).unwrap();
        let crossing = Command::Act {
            expected_revision: boundary.revision,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        let result = engine.command(
            "trace",
            "test",
            ActorId(1),
            "cross",
            &engine.branch().clone(),
            crossing,
        );
        if regions == 1 {
            assert!(result.is_err());
            assert_eq!(engine.state(ActorId(1)).unwrap(), boundary);
        } else {
            assert!(result.is_ok());
            step(
                &mut engine,
                Action::Move {
                    direction: Direction::West,
                },
            );
            assert_eq!(
                engine.state(ActorId(1)).unwrap().observation.visible_cells,
                boundary.observation.visible_cells
            );
        }
    }
    for regions in [0, 257, u64::MAX] {
        assert!(Scenario::performance(42, regions, 1).is_err());
    }
    for actors in [0, 9] {
        assert!(Scenario::performance(42, 8, actors).is_err());
    }
}

#[test]
fn mixed_trace_schedules_actors_changes_los_and_replays_identical_disclosure() {
    let directory = tempfile::tempdir().unwrap();
    let trace = Trace::load();
    for (regions, actors) in [(1, 1), (8, 1), (8, 8)] {
        let scenario = Scenario::performance(42, regions, actors).unwrap();
        let path = directory.path().join(format!("r{regions}-a{actors}.json"));
        let mut engine = Engine::open(&path, scenario.clone()).unwrap();
        let mut secondary = 0;
        let mut accepted = 0;
        let mut visibility_changes = 0;
        let mut labels = std::collections::BTreeSet::new();
        for cycle in 0..2 {
            for (index, step) in trace.steps(regions).enumerate() {
                while !engine.state(ActorId(1)).unwrap().observation.ready {
                    let actor = engine
                        .actors()
                        .into_iter()
                        .find(|&a| engine.state(a).unwrap().observation.ready)
                        .unwrap();
                    let action = if actor == ActorId(2) {
                        let action = trace.secondary[secondary % trace.secondary.len()]
                            .resolve(&engine.state(actor).unwrap());
                        secondary += 1;
                        action
                    } else {
                        Action::Wait
                    };
                    let before = engine.state(ActorId(1)).unwrap();
                    let revision = engine.revision(actor).unwrap();
                    engine
                        .command(
                            "trace",
                            "test",
                            actor,
                            &format!("other-{accepted}"),
                            &engine.branch().clone(),
                            Command::Act {
                                expected_revision: revision,
                                action,
                            },
                        )
                        .unwrap();
                    let after = engine.state(ActorId(1)).unwrap();
                    let ids = |s: &tor_protocol::StateView| {
                        s.observation
                            .visible_actors
                            .iter()
                            .map(|a| a.id)
                            .collect::<std::collections::BTreeSet<_>>()
                    };
                    if ids(&before) != ids(&after) {
                        assert!(after.revision > before.revision);
                        visibility_changes += 1;
                    }
                    accepted += 1;
                }
                let before = engine.state(ActorId(1)).unwrap();
                let action = step.resolve(&before);
                let result = engine.command(
                    "trace",
                    "test",
                    ActorId(1),
                    &format!("c{cycle}-s{index}"),
                    &engine.branch().clone(),
                    Command::Act {
                        expected_revision: before.revision,
                        action,
                    },
                );
                let after = engine.state(ActorId(1)).unwrap();
                if step.expected != "blocked" {
                    assert!(
                        result.is_ok(),
                        "r{regions} a{actors} c{cycle} s{index} {}: {result:?}",
                        step.label
                    );
                }
                step.verify(&before, &after, result.is_ok());
                if result.is_ok() {
                    accepted += 1;
                }
                labels.insert(step.label.clone());
            }
        }
        assert_eq!(engine.profile_counts().0, accepted);
        assert!(engine.profile_counts().1 <= 128);
        for label in [
            "wait_same_region",
            "move_same_region",
            "move_near_obstacle",
            "open_or_close_door",
            "change_elevation",
        ] {
            assert!(labels.contains(label));
        }
        if regions > 1 {
            assert!(
                labels.contains("cross_region_boundary")
                    && labels.contains("cross_boundary_with_los_change")
            );
        }
        if actors > 1 {
            assert!(visibility_changes > 0);
        }
        let expected: Vec<_> = engine
            .actors()
            .into_iter()
            .map(|a| engine.state(a).unwrap())
            .collect();
        drop(engine);
        let resumed = Engine::open(&path, scenario).unwrap();
        for state in expected {
            assert_eq!(resumed.state(state.observation.actor).unwrap(), state);
        }
    }
}
