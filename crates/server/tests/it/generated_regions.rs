//! Generated regions in play: built when first needed, the same in any
//! order, replayed exactly, and different for each game seed. See
//! docs/scenario-packages.md#generated-regions.
use std::path::Path;
use tor_protocol::{Action, ActorId, Direction, Observation};
use tor_server::{
    journal::{Command, WizardOperation},
    scenario_package, Engine, SavePolicy, Scenario, Streaming,
};

/// Two authored halls around two generated caves, with radii of zero: the
/// first cave is built at the start (it's linked to the start hall), the
/// second when the character enters the first.
fn caves(seed: u64) -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/generated-filler");
    let mut scenario = scenario_package::load(&root, seed, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

/// From the start into the first cave, then across it into the second.
const INTO_CAVE: usize = 10;
const ACROSS_CAVE: usize = 24;

fn command_as(
    engine: &mut Engine,
    actor: ActorId,
    command: Command,
) -> Result<(), tor_server::Failure> {
    engine
        .command(
            "player",
            "test",
            actor,
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            command,
        )
        .map(|_| ())
}

fn command(engine: &mut Engine, command: Command) -> Result<(), tor_server::Failure> {
    command_as(engine, ActorId(1), command)
}

/// Act as the character, after any AI turns due first. Returns the
/// commands issued, AI turns included.
fn act(engine: &mut Engine, action: Action) -> Result<usize, tor_server::Failure> {
    let mut commands = 0;
    while let Some((actor, ai)) = engine.next_ai_action() {
        let expected_revision = engine.revision(actor).unwrap();
        command_as(
            engine,
            actor,
            Command::Act {
                expected_revision,
                action: ai,
            },
        )
        .unwrap();
        commands += 1;
    }
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    command(
        engine,
        Command::Act {
            expected_revision,
            action,
        },
    )?;
    Ok(commands + 1)
}

/// Step east `steps` times along the cave's straight corridor. A wandering
/// rat may be in the way: wait for it to move. Play is deterministic, so a
/// run always waits the same way.
fn walk_east(engine: &mut Engine, steps: usize) -> usize {
    let mut commands = 0;
    for _ in 0..steps {
        let mut tries = 0;
        loop {
            match act(
                engine,
                Action::Move {
                    direction: Direction::East,
                },
            ) {
                Ok(issued) => {
                    commands += issued;
                    break;
                }
                Err(_) => {
                    commands += act(engine, Action::Wait).unwrap();
                    tries += 1;
                    assert!(tries < 20, "blocked for good");
                }
            }
        }
    }
    commands
}

fn seen(engine: &Engine) -> Observation {
    engine.state(ActorId(1)).unwrap().observation
}

fn generated_things(observation: &Observation) -> usize {
    observation
        .ground_items
        .iter()
        .filter(|i| i.item.name == "copper coin")
        .count()
        + observation
            .visible_actors
            .iter()
            .filter(|a| a.name == "cave rat")
            .count()
}

#[test]
fn crossing_generated_caves_builds_each_when_needed_and_replays_exactly() {
    let directory = tempfile::tempdir().unwrap();
    let save = directory.path().join("game.db");
    let policy = SavePolicy {
        checkpoint_interval: 0,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&save, caves(5), policy.clone()).unwrap();
    let start = engine.region_counts().unwrap();
    // The start hall and the first cave are built; the second cave and the
    // far hall aren't.
    assert_eq!(
        (start.active + start.frozen, start.unbuilt),
        (2, 2),
        "{start:?}"
    );
    let mut commands = walk_east(&mut engine, INTO_CAVE);
    let mut things = generated_things(&seen(&engine));
    let inside = engine.region_counts().unwrap();
    assert_eq!(
        inside.unbuilt, 1,
        "entering the first cave built the second"
    );
    for _ in 0..ACROSS_CAVE {
        commands += walk_east(&mut engine, 1);
        things += generated_things(&seen(&engine));
    }
    assert_eq!(engine.region_counts().unwrap().unbuilt, 0);
    assert!(things > 0, "the walk saw a generated rat or coin");
    engine.flush().unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    // Replay from the start builds the caves again, exactly.
    let engine = Engine::open_with_policy(&save, Scenario::two_room(0), policy).unwrap();
    assert_eq!(engine.recovery_profile().records_replayed, commands);
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn rewinding_before_a_generated_build_builds_the_same_cave_again() {
    let mut engine = Engine::memory(caves(5)).unwrap();
    engine.enable_wizard().unwrap();
    walk_east(&mut engine, INTO_CAVE + ACROSS_CAVE);
    let first = seen(&engine);
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    command(
        &mut engine,
        Command::Wizard {
            expected_revision,
            operation: WizardOperation::Rewind { target: None },
        },
    )
    .unwrap();
    assert_eq!(engine.region_counts().unwrap().unbuilt, 2);
    walk_east(&mut engine, INTO_CAVE + ACROSS_CAVE);
    assert_eq!(seen(&engine), first);
}

#[test]
fn each_game_seed_generates_its_own_caves() {
    let walls = |seed: u64| {
        let mut engine = Engine::memory(caves(seed)).unwrap();
        walk_east(&mut engine, INTO_CAVE + 4);
        seen(&engine)
            .visible_cells
            .iter()
            .filter(|c| c.wall)
            .map(|c| (c.position.x, c.position.y, c.position.z))
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(walls(1), walls(1));
    assert_ne!(walls(1), walls(2));
}
