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
        workload_version: None,
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
    assert!(
        one.actors_observed <= 2 && one.revision_comparisons <= 1 && one.rollback_snapshots <= 1
    );
    assert!(
        eight.actors_observed <= 16
            && eight.revision_comparisons <= 8
            && eight.rollback_snapshots <= 1
    );
    assert!(eight.actors_observed <= one.actors_observed * 8);
    assert!(eight.revision_comparisons <= one.revision_comparisons * 8);
}

#[test]
fn journal_setup_growth_is_bounded() {
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
    assert!(hundred.records_serialized <= 100);
    assert!(
        std::fs::metadata(directory.path().join("hundred.json"))
            .unwrap()
            .len()
            <= std::fs::metadata(directory.path().join("empty.json"))
                .unwrap()
                .len()
                * 200
    );
}

#[test]
fn diagnostics_obey_save_ownership_and_never_seed_an_attached_engine() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("owned.json");
    let mut attached = Engine::open(&path, scenario(1)).unwrap();
    let before = attached.state(ActorId(1)).unwrap();
    assert!(attached.seed_profile_history(1).is_err());
    assert_eq!(attached.state(ActorId(1)).unwrap(), before);
    let memory = Engine::memory(scenario(1)).unwrap();
    assert!(memory.profile_persistence(&path).is_err());
    assert!(attached.profile_persistence(&path).is_err());
}

#[test]
fn seeded_history_matches_normal_execution_and_restart() {
    let directory = tempdir().unwrap();
    for actors in [1, 8] {
        let mut seeded = Engine::memory(scenario(actors)).unwrap();
        let mut normal = Engine::memory(scenario(actors)).unwrap();
        // Repeated seeding must continue the scheduler and request sequence.
        for count in [3, 132] {
            seeded.seed_profile_history(count).unwrap();
        }
        for index in 0..135 {
            act(
                &mut normal,
                ActorId((index % actors + 1) as u64),
                &format!("n-{index}"),
            );
        }
        for actor in normal.actors() {
            let a = seeded.state(actor).unwrap();
            let b = normal.state(actor).unwrap();
            assert_eq!(a.revision, b.revision);
            assert_eq!(a.observation.tick, b.observation.tick);
            assert_eq!(a.observation.ready, b.observation.ready);
        }
        let path = directory.path().join(format!("seeded-{actors}.json"));
        seeded.profile_persistence(&path).unwrap();
        let resumed = Engine::open(&path, scenario(actors)).unwrap();
        for actor in seeded.actors() {
            assert_eq!(seeded.state(actor).unwrap(), resumed.state(actor).unwrap());
        }
    }
}

#[test]
fn ordinary_command_profiles_encoding_without_disk_io() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("profile.json");
    let mut engine = Engine::open(&path, scenario(1)).unwrap();
    let p = act(&mut engine, ActorId(1), "measured");
    assert!(p.exclusive_duration() <= p.authoritative_total);
    assert_eq!(p.simulation_transitions, 1);
    assert_eq!(p.candidate_captures, 1);
    assert_eq!(p.navigation_refreshes, 1);
    assert!(p.perception_calls >= p.actors_observed);
    assert!(p.scene_calls >= p.perception_calls);
    assert_eq!(
        (
            p.file_writes,
            p.file_flushes,
            p.file_syncs,
            p.file_replacements
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(p.bytes_written, 0);
    assert_eq!(p.records_serialized, 1);
    engine.flush().unwrap();
    assert_eq!(engine.save_status().batches, 1);
    assert_eq!(engine.profile_counts(), (1, 2));
}
