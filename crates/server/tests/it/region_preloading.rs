//! Background preloading: regions built and rows read on another thread
//! before a command needs them. Games must play identically with and without
//! it. See docs/region-streaming.md.
use std::path::Path;
use tor_protocol::ActorId;
use tor_server::journal::{Action, Direction};
use tor_server::{journal::Command, scenario_package, Engine, SavePolicy, Scenario, Streaming};

#[test]
fn acquisition_timings_are_nested_in_the_transition_phase() {
    use std::time::Duration;
    let profile = tor_server::CommandProfile {
        region_transition: Duration::from_secs(3),
        region_acquisition: tor_server::RegionAcquisitionProfile {
            fallback_read: Duration::from_secs(1),
            fallback_build: Duration::from_secs(1),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(profile.exclusive_duration(), Duration::from_secs(3));
}

/// The seven-hall corridor with radii of zero, as in `region_streaming`.
fn corridor() -> Scenario {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
    let mut scenario = scenario_package::load(&root, 5, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

const TO_HALL_4: usize = 68;

/// Step, returning the builds and reads the preloader had ready, and all
/// the builds and reads the step needed.
fn step(engine: &mut Engine, direction: Direction, sequence: &mut usize) -> (usize, usize) {
    let revision = engine.revision(ActorId(1)).unwrap();
    let (_, profile) = engine
        .command_profiled(
            "player",
            "test",
            ActorId(1),
            &format!("step-{sequence}"),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: revision,
                action: Action::Move { direction },
            },
        )
        .unwrap_or_else(|e| panic!("step {sequence}: {e}"));
    *sequence += 1;
    let encoded = serde_json::to_value(&profile).unwrap();
    let acquisition = &encoded["region_acquisition"];
    let count = |name| acquisition[name].as_u64().expect("acquisition count") as usize;
    assert_eq!(
        count("fallback_reads") + count("prepared_reads"),
        profile.region_records_read
    );
    assert_eq!(
        count("fallback_builds") + count("prepared_builds"),
        profile.regions_built
    );
    assert_eq!(
        count("prepared_reads") + count("prepared_builds"),
        profile.regions_prepared
    );
    let duration =
        |name| serde_json::from_value::<std::time::Duration>(acquisition[name].clone()).unwrap();
    assert!(duration("fallback_read") + duration("fallback_build") <= profile.region_transition);
    if count("fallback_reads") == 0 {
        assert_eq!(duration("fallback_read"), std::time::Duration::ZERO);
    }
    if count("fallback_builds") == 0 {
        assert_eq!(duration("fallback_build"), std::time::Duration::ZERO);
    }
    (
        profile.regions_prepared,
        profile.regions_built + profile.region_records_read,
    )
}

/// What the actor sees, without cell keys: those are salted per game.
fn seen(engine: &Engine) -> tor_protocol::Observation {
    let mut observation = engine.state(ActorId(1)).unwrap().observation;
    for cell in &mut observation.visible_cells {
        cell.key.clear();
    }
    observation
}

/// Every region row in a save, by record identity.
fn rows(save: &Path) -> Vec<(i64, Vec<u8>)> {
    let db = rusqlite::Connection::open(save).unwrap();
    let mut query = db
        .prepare("SELECT record, frame FROM regions ORDER BY record")
        .unwrap();
    query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

type Played = (
    Vec<tor_protocol::Observation>,
    Vec<(i64, Vec<u8>)>,
    (usize, usize),
);

/// Walk to hall 4, restart, and walk back, with or without preloading;
/// returns every state seen, the region rows, and what was prepared.
fn play(save: &Path, preload: bool) -> Played {
    let policy = SavePolicy {
        checkpoint_interval: 4,
        ..SavePolicy::default()
    };
    let mut states = Vec::new();
    let mut prepared = (0, 0);
    let mut sequence = 0;
    for direction in [Direction::East, Direction::West] {
        let mut engine = Engine::open_with_policy(save, corridor(), policy.clone()).unwrap();
        if preload {
            engine.start_preloading();
        }
        for _ in 0..TO_HALL_4 {
            if preload {
                // Measure the prepared path: let the preloader finish.
                engine.settle_preloading();
            }
            let (ready, needed) = step(&mut engine, direction, &mut sequence);
            prepared.0 += ready;
            prepared.1 += needed;
            states.push(seen(&engine));
        }
        engine.flush().unwrap();
    }
    (states, rows(save), prepared)
}

#[test]
fn preloading_changes_nothing_a_game_can_observe() {
    let directory = tempfile::tempdir().unwrap();
    let (plain_states, plain_rows, plain_prepared) =
        play(&directory.path().join("plain.db"), false);
    let (states, rows, prepared) = play(&directory.path().join("preloaded.db"), true);
    assert_eq!(plain_prepared.0, 0);
    // Walking east builds halls ahead of need; walking back after the
    // restart reads halls from their rows. All of it was ready.
    assert_eq!(prepared.1, plain_prepared.1);
    assert_eq!(prepared.0, prepared.1);
    assert!(prepared.0 >= 5, "{prepared:?}");
    assert_eq!(states, plain_states);
    assert!(!rows.is_empty());
    assert_eq!(rows, plain_rows);
}

#[test]
fn commands_do_not_wait_for_the_preloader() {
    // Commands race the preloader freely: whatever isn't ready yet is built
    // or read on demand, with the same result.
    let directory = tempfile::tempdir().unwrap();
    let save = directory.path().join("racing.db");
    let mut engine = Engine::open(&save, corridor()).unwrap();
    engine.start_preloading();
    let mut sequence = 0;
    for _ in 0..TO_HALL_4 {
        step(&mut engine, Direction::East, &mut sequence);
    }
    let raced = seen(&engine);

    let mut plain = Engine::memory(corridor()).unwrap();
    let mut sequence = 0;
    for _ in 0..TO_HALL_4 {
        step(&mut plain, Direction::East, &mut sequence);
    }
    assert_eq!(raced, seen(&plain));
}

#[test]
fn stopping_preloading_mid_game_keeps_playing() {
    let mut engine = Engine::memory(corridor()).unwrap();
    engine.start_preloading();
    let mut sequence = 0;
    for _ in 0..TO_HALL_4 / 2 {
        engine.settle_preloading();
        step(&mut engine, Direction::East, &mut sequence);
    }
    engine.stop_preloading();
    for _ in 0..TO_HALL_4 / 2 {
        assert_eq!(step(&mut engine, Direction::East, &mut sequence).0, 0);
    }
    let mut plain = Engine::memory(corridor()).unwrap();
    let mut sequence = 0;
    for _ in 0..TO_HALL_4 / 2 * 2 {
        step(&mut plain, Direction::East, &mut sequence);
    }
    assert_eq!(seen(&engine), seen(&plain));
}
