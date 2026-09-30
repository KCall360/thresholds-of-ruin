//! Region streaming in the engine: a package game keeps only what its
//! reference points need, builds regions when they're first needed, keeps
//! detached regions on disk and replays exactly. See
//! docs/region-streaming.md.
use std::path::Path;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{
    journal::Command, scenario_package, Engine, RegionCounts, SavePolicy, Scenario, Streaming,
};

/// The checked-in seven-hall corridor, with radii of zero so that only what
/// the pins require stays loaded. Each hall is 20x3x1; the character starts
/// at x = 2 in hall 1 and a pebble lies at x = 8. Sight (8 cells) from the
/// middle of a hall stays inside it.
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

/// Steps east from the start to the middle of hall 4.
const TO_HALL_4: usize = 68;

fn walk(engine: &mut Engine, direction: Direction, steps: usize, sequence: &mut usize) {
    for _ in 0..steps {
        let revision = engine.revision(ActorId(1)).unwrap();
        engine
            .command(
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
    }
}

fn counts(engine: &Engine) -> RegionCounts {
    engine.region_counts().expect("the package game streams")
}

#[test]
fn a_package_game_streams_regions_through_disk_and_replays_exactly() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = corridor();
    let save = directory.path().join("game.db");
    let policy = SavePolicy {
        checkpoint_interval: 4,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&save, scenario.clone(), policy.clone()).unwrap();
    let start = counts(&engine);
    // The character's region is active and its neighbour loaded; the rest
    // have never been needed, so they aren't built.
    assert_eq!(
        (start.active, start.frozen, start.detached, start.unbuilt),
        (1, 1, 0, 5),
        "{start:?}"
    );

    let mut sequence = 0;
    walk(&mut engine, Direction::East, TO_HALL_4, &mut sequence);
    let far = counts(&engine);
    // In hall 4: halls 3 and 5 are loaded around it, 1 and 2 detached, and
    // 6 and 7 still unbuilt.
    assert_eq!(
        (far.active, far.frozen, far.detached, far.unbuilt),
        (1, 2, 2, 2),
        "{far:?}"
    );
    engine.flush().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    drop(engine);

    // Restarting restores the checkpoint without reading any region row,
    // and strict replay of the tail reproduces every entry.
    let mut engine = Engine::open_with_policy(&save, scenario.clone(), policy.clone()).unwrap();
    assert!(engine.recovery_profile().checkpoint_sequence > 0);
    assert_eq!(counts(&engine).records_read, 0);
    assert_eq!(counts(&engine).resident_records, 0);
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);

    // Walking back reattaches the detached regions from their rows.
    walk(&mut engine, Direction::West, TO_HALL_4, &mut sequence);
    let back = counts(&engine);
    assert_eq!(
        (back.active, back.frozen, back.detached),
        (1, 1, 3),
        "{back:?}"
    );
    assert!(back.records_read >= 2, "{back:?}");
    engine.flush().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let engine = Engine::open_with_policy(&save, scenario, policy).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    assert_eq!(
        counts(&engine),
        RegionCounts {
            // A restart holds no records; they're read when needed.
            resident_records: 0,
            records_read: 0,
            ..back
        }
    );
}

#[test]
fn replaying_a_streaming_game_from_its_start_reproduces_it() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = corridor();
    let save = directory.path().join("game.db");
    let policy = SavePolicy {
        checkpoint_interval: 0,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&save, scenario.clone(), policy.clone()).unwrap();
    let mut sequence = 0;
    walk(&mut engine, Direction::East, TO_HALL_4, &mut sequence);
    walk(&mut engine, Direction::West, 40, &mut sequence);
    engine.flush().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    let regions = counts(&engine);
    drop(engine);
    // Every build, detach and reattach replays from the journal alone.
    let engine = Engine::open_with_policy(&save, scenario, policy).unwrap();
    assert_eq!(engine.recovery_profile().records_replayed, TO_HALL_4 + 40);
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    assert_eq!(counts(&engine).detached, regions.detached);
}

fn act(engine: &mut Engine, id: &str, command: Command) -> tor_server::CommandResult {
    engine
        .command(
            "wizard",
            "test",
            ActorId(1),
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A rewind restores a game whose record counter is behind records the
/// abandoned future made, which retained boundaries and the checkpoint still
/// hold. Records made after the rewind must get new identities, or a
/// different record would be written under an existing row.
#[test]
fn records_made_after_a_rewind_never_reuse_an_identity() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = corridor();
    let save = directory.path().join("game.db");
    let policy = SavePolicy {
        checkpoint_interval: 4,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&save, scenario.clone(), policy.clone()).unwrap();
    engine.enable_wizard().unwrap();
    let mut sequence = 0;
    walk(&mut engine, Direction::East, 4, &mut sequence);
    let revision = engine.revision(ActorId(1)).unwrap();
    let early = act(
        &mut engine,
        "early",
        Command::Act {
            expected_revision: revision,
            action: Action::Move {
                direction: Direction::East,
            },
        },
    )
    .entry
    .id;
    walk(&mut engine, Direction::East, TO_HALL_4 - 5, &mut sequence);
    assert_eq!(counts(&engine).detached, 2);
    engine.flush().unwrap();

    let revision = engine.revision(ActorId(1)).unwrap();
    act(
        &mut engine,
        "rewind",
        Command::Wizard {
            expected_revision: revision,
            operation: tor_server::journal::WizardOperation::Rewind {
                target: Some(early),
            },
        },
    );
    assert_eq!(counts(&engine).detached, 0);
    // Taking the pebble this time makes hall 1's new record differ from the
    // abandoned one, which is still on disk.
    walk(&mut engine, Direction::East, 1, &mut sequence);
    let revision = engine.revision(ActorId(1)).unwrap();
    act(
        &mut engine,
        "take",
        Command::Act {
            expected_revision: revision,
            action: Action::Take {
                item: 10,
                quantity: None,
            },
        },
    );
    walk(&mut engine, Direction::East, TO_HALL_4 - 6, &mut sequence);
    assert_eq!(counts(&engine).detached, 2);
    engine.flush().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    drop(engine);

    let mut engine = Engine::open_with_policy(&save, scenario, policy).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    walk(&mut engine, Direction::West, TO_HALL_4, &mut sequence);
    assert_eq!(counts(&engine).detached, 3);
    assert!(counts(&engine).records_read >= 2);
    engine.flush().unwrap();
}

/// A wizard operation can reach a region nobody has needed yet: it's built
/// and activated first.
#[test]
fn a_wizard_can_teleport_into_a_region_that_was_never_built() {
    let mut engine = Engine::memory(corridor()).unwrap();
    engine.enable_wizard().unwrap();
    assert_eq!(counts(&engine).unbuilt, 5);
    let revision = engine.revision(ActorId(1)).unwrap();
    act(
        &mut engine,
        "teleport",
        Command::Wizard {
            expected_revision: revision,
            operation: tor_server::journal::WizardOperation::Teleport {
                actor: ActorId(1),
                position: tor_server::journal::Position {
                    region: 7,
                    x: 10,
                    y: 1,
                    z: 0,
                },
            },
        },
    );
    let after = counts(&engine);
    // Hall 7 and its neighbour are loaded; the rest detach or stay unbuilt.
    assert_eq!((after.active, after.frozen), (1, 1), "{after:?}");
    assert_eq!(after.unbuilt + after.detached, 5, "{after:?}");
}

/// Per-command transition work must not grow with the size of the world:
/// the same walk through a 16-hall and a 256-hall corridor, at the default
/// radii, does exactly the same horizon planning, pin and reach work, and
/// reads and builds the same regions. Operation counts, not timings, so CI
/// enforces it.
#[test]
fn transition_work_does_not_grow_with_the_world() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
    let directory = tempfile::tempdir().unwrap();
    let work = |halls: u64| {
        let out = directory.path().join(format!("corridor-{halls}"));
        let scenario = scenario_package::streaming_corridor(&root, &out, halls, 5).unwrap();
        let mut engine = Engine::memory(scenario).unwrap();
        let mut totals = [0usize; 8];
        let mut step = 0;
        let mut run = |engine: &mut Engine, actor: ActorId, action: Action| {
            let revision = engine.revision(actor).unwrap();
            step += 1;
            let (_, profile) = engine
                .command_profiled(
                    "bench",
                    "test",
                    actor,
                    &format!("step-{step}"),
                    &engine.branch().clone(),
                    Command::Act {
                        expected_revision: revision,
                        action,
                    },
                )
                .unwrap();
            for (total, count) in totals.iter_mut().zip([
                profile.region_changes,
                profile.horizon_regions_expanded,
                profile.horizon_links_examined,
                profile.pinned_actors,
                profile.reach_lookups,
                profile.region_records_read,
                profile.regions_built,
                profile.scene_calls,
            ]) {
                *total += count;
            }
        };
        for direction in [Direction::East, Direction::West] {
            for _ in 0..100 {
                while let Some((actor, action)) = engine.next_ai_action() {
                    run(&mut engine, actor, action);
                }
                run(&mut engine, ActorId(1), Action::Move { direction });
            }
        }
        (totals, counts(&engine).active + counts(&engine).frozen)
    };
    let (small, small_loaded) = work(16);
    let (large, large_loaded) = work(256);
    assert_eq!(small, large);
    assert_eq!(small_loaded, large_loaded);
    // The walk really streamed: regions changed and were built.
    assert!(small[0] > 0 && small[6] > 0, "{small:?}");
}

/// An actor a client controls keeps its own region in play, however far it
/// is from the default character: it gets a reference point like a
/// character does.
#[test]
fn an_actor_a_client_controls_keeps_its_region_in_play() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-controlled");
    let mut scenario = scenario_package::load(&root, 5, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let engine = Engine::memory(scenario).unwrap();
    assert!(
        engine.actors().contains(&ActorId(3)),
        "{:?}",
        counts(&engine)
    );
    assert!(engine.observation(ActorId(3)).is_ok());
    // Halls 1 and 5 are active, with their neighbours 2 and 4 loaded; hall 3
    // is between them and needed by neither.
    let regions = counts(&engine);
    assert_eq!(
        (regions.active, regions.frozen, regions.unbuilt),
        (2, 2, 1),
        "{regions:?}"
    );
}

/// A transition must not disclose anything an observer can't see: when the
/// character's move changes which regions are loaded, an actor elsewhere
/// whose view didn't change (here a frozen guard, at the same tick) keeps
/// its revision, so its clients get no update.
#[test]
fn an_unseen_transition_leaves_other_observers_revisions_alone() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-controlled");
    let mut scenario = scenario_package::load(&root, 5, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut engine = Engine::memory(scenario).unwrap();
    let guard = ActorId(4);
    assert!(
        engine.observation(guard).is_ok(),
        "the guard's hall is loaded"
    );
    let mut step = 0;
    let mut act = |engine: &mut Engine, actor: ActorId, action: Action| {
        let revision = engine.revision(actor).unwrap();
        step += 1;
        engine
            .command(
                "player",
                "test",
                actor,
                &format!("step-{step}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: revision,
                    action,
                },
            )
            .unwrap();
    };
    // Both controlled actors are due each round; the character moves first,
    // so its move doesn't advance time. Check every such move that changes
    // the loaded regions.
    let mut checked = 0;
    for _ in 0..40 {
        let before = (counts(&engine), engine.revision(guard).unwrap());
        let tick = engine.state(guard).unwrap().observation.tick;
        act(
            &mut engine,
            ActorId(1),
            Action::Move {
                direction: Direction::East,
            },
        );
        if counts(&engine) != before.0 {
            assert_eq!(engine.state(guard).unwrap().observation.tick, tick);
            assert_eq!(
                engine.revision(guard).unwrap(),
                before.1,
                "{:?}",
                counts(&engine)
            );
            checked += 1;
        }
        act(&mut engine, ActorId(3), Action::Wait);
    }
    assert!(checked > 0, "no move changed the loaded regions");
}
