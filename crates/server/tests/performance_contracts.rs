use tempfile::tempdir;
use tor_protocol::{Action, ActorId};
use tor_server::{journal::Command, ActorSetup, Engine, Scenario};

fn scenario(actors: usize) -> Scenario {
    let positions = [
        (1, 1),
        (2, 1),
        (3, 1),
        (1, 2),
        (2, 2),
        (3, 2),
        (4, 1),
        (4, 2),
    ];
    Scenario {
        seed: 7,
        regions: 2,
        actors: positions[..actors]
            .iter()
            .map(|&(x, y)| ActorSetup {
                position: tor_server::journal::Position {
                    region: 1,
                    x,
                    y,
                    z: 0,
                },
                turn_ticks: 100,
            })
            .collect(),
    }
}

fn act(engine: &mut Engine, actor: ActorId, request: &str) -> tor_server::CommandProfile {
    let (_, profile) = engine
        .command_profiled(
            "scale",
            "test",
            actor,
            request,
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    profile
}

#[test]
fn revision_and_rollback_operation_counts_scale_only_with_actors() {
    let one = act(&mut Engine::memory(scenario(1)).unwrap(), ActorId(1), "one");
    let eight = act(
        &mut Engine::memory(scenario(8)).unwrap(),
        ActorId(1),
        "eight",
    );
    assert_eq!(
        (
            one.actors_observed,
            one.revision_comparisons,
            one.rollback_snapshots
        ),
        (2, 1, 1)
    );
    assert_eq!(
        (
            eight.actors_observed,
            eight.revision_comparisons,
            eight.rollback_snapshots
        ),
        (16, 8, 1)
    );
    assert_eq!(eight.actors_observed / one.actors_observed, 8);
    assert_eq!(eight.revision_comparisons / one.revision_comparisons, 8);
}

#[test]
fn current_whole_archive_save_growth_is_recorded_as_a_phase_b_regression_target() {
    let directory = tempdir().unwrap();
    let mut engine = Engine::memory(scenario(1)).unwrap();
    let empty = engine
        .profile_persistence(directory.path().join("empty.json"))
        .unwrap();
    for index in 0..100 {
        act(&mut engine, ActorId(1), &format!("history-{index}"));
    }
    let hundred = engine
        .profile_persistence(directory.path().join("hundred.json"))
        .unwrap();
    assert_eq!(empty.records_serialized, 0);
    assert_eq!(hundred.records_serialized, 100);
    assert_eq!(
        empty.bytes_written,
        std::fs::metadata(directory.path().join("empty.json"))
            .unwrap()
            .len()
    );
    assert_eq!(
        hundred.bytes_written,
        std::fs::metadata(directory.path().join("hundred.json"))
            .unwrap()
            .len()
    );
    assert!(hundred.bytes_written > empty.bytes_written * 20);
}
