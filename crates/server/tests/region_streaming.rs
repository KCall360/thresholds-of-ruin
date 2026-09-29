//! Region streaming in the engine: a package game keeps only what its
//! reference points need, builds regions when they're first needed, keeps
//! detached regions on disk and replays exactly. See
//! docs/region-streaming.md.
use std::path::Path;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{
    journal::Command, scenario_package, Engine, RegionCounts, SavePolicy, Scenario, Streaming,
};

/// Five 20x1x1 regions in a row, joined end to end, with the character at
/// the west end of region 1 and a pebble on its path. Sight (8 cells) from
/// the middle of a region stays inside it.
fn corridor(root: &Path) -> Scenario {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("scenario.toml"),
        r#"format = 1
id = "streaming-corridor"
version = "1.0"
ruleset = "dungeon-v17"
files = ["regions.toml"]
default_character = 1
characters = [{ "id" = 1, "anchor" = "1/start", "turn_ticks" = 100 }]
"#,
    )
    .unwrap();
    let mut regions = String::new();
    for id in 1..=5 {
        let mut portals = Vec::new();
        if id < 5 {
            portals.push(format!(
                r#"{{ "at" = [19, 0, 0], "direction" = "east", "to" = "{}/west" }}"#,
                id + 1
            ));
        }
        if id > 1 {
            portals.push(format!(
                r#"{{ "at" = [0, 0, 0], "direction" = "west", "to" = "{}/east" }}"#,
                id - 1
            ));
        }
        regions.push_str(&format!(
            r#"[[regions]]
id = {id}
name = "Corridor {id}"
size = [20, 1, 1]
anchors = {{ "west" = [0, 0, 0], "east" = [19, 0, 0], "start" = [1, 0, 0] }}
portals = [{}]
{}
"#,
            portals.join(", "),
            if id == 1 {
                r#"items = [{ "id" = 1, "at" = [8, 0, 0], "name" = "pebble" }]
"#
            } else {
                ""
            }
        ));
    }
    std::fs::write(root.join("regions.toml"), regions).unwrap();
    let mut scenario = scenario_package::load(root, 5, None, true).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

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
    let scenario = corridor(&directory.path().join("package"));
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
        (1, 1, 0, 3),
        "{start:?}"
    );

    let mut sequence = 0;
    // To the middle of region 4.
    walk(&mut engine, Direction::East, 69, &mut sequence);
    let far = counts(&engine);
    // In region 4: regions 3 and 5 are loaded around it, 1 and 2 detached.
    assert_eq!(
        (far.active, far.frozen, far.detached, far.unbuilt),
        (1, 2, 2, 0),
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
    walk(&mut engine, Direction::West, 69, &mut sequence);
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
    let scenario = corridor(&directory.path().join("package"));
    let save = directory.path().join("game.db");
    let policy = SavePolicy {
        checkpoint_interval: 0,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&save, scenario.clone(), policy.clone()).unwrap();
    let mut sequence = 0;
    walk(&mut engine, Direction::East, 69, &mut sequence);
    walk(&mut engine, Direction::West, 40, &mut sequence);
    engine.flush().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    let regions = counts(&engine);
    drop(engine);
    // Every build, detach and reattach replays from the journal alone.
    let engine = Engine::open_with_policy(&save, scenario, policy).unwrap();
    assert_eq!(engine.recovery_profile().records_replayed, 109);
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
    let scenario = corridor(&directory.path().join("package"));
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
    walk(&mut engine, Direction::East, 64, &mut sequence);
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
    // Taking the pebble this time makes region 1's new record differ from
    // the abandoned one, which is still on disk.
    walk(&mut engine, Direction::East, 2, &mut sequence);
    let revision = engine.revision(ActorId(1)).unwrap();
    act(
        &mut engine,
        "take",
        Command::Act {
            expected_revision: revision,
            action: Action::Take {
                item: 1,
                quantity: None,
            },
        },
    );
    walk(&mut engine, Direction::East, 62, &mut sequence);
    assert_eq!(counts(&engine).detached, 2);
    engine.flush().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    drop(engine);

    let mut engine = Engine::open_with_policy(&save, scenario, policy).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    walk(&mut engine, Direction::West, 69, &mut sequence);
    assert_eq!(counts(&engine).detached, 3);
    assert!(counts(&engine).records_read >= 2);
    engine.flush().unwrap();
}

/// A wizard operation can reach a region nobody has needed yet: it's built
/// and activated first.
#[test]
fn a_wizard_can_teleport_into_a_region_that_was_never_built() {
    let directory = tempfile::tempdir().unwrap();
    let mut engine = Engine::memory(corridor(&directory.path().join("package"))).unwrap();
    engine.enable_wizard().unwrap();
    assert_eq!(counts(&engine).unbuilt, 3);
    let revision = engine.revision(ActorId(1)).unwrap();
    act(
        &mut engine,
        "teleport",
        Command::Wizard {
            expected_revision: revision,
            operation: tor_server::journal::WizardOperation::Teleport {
                actor: ActorId(1),
                position: tor_server::journal::Position {
                    region: 5,
                    x: 10,
                    y: 0,
                    z: 0,
                },
            },
        },
    );
    let after = counts(&engine);
    // Region 5 and its neighbour are loaded; the rest detach or stay unbuilt.
    assert_eq!((after.active, after.frozen), (1, 1), "{after:?}");
    assert_eq!(after.unbuilt + after.detached, 3, "{after:?}");
}
