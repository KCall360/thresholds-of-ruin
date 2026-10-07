//! Invariants that every scenario package keeps. Each package is played for a
//! few turns, its own AI included, and then:
//!
//! - a rejected command changes nothing;
//! - a retried request gets the original answer and changes nothing;
//! - a restart replays every actor's state exactly;
//! - rewinding to the start restores the initial observation.
//!
//! Feature tests don't need to repeat these checks for their own packages:
//! adding a package to `scenarios/` or `scenarios/tests/` covers it here.
use crate::support::{self, run_ai_turns};
use tor_protocol::{ActorId, StateView};
use tor_server::journal::Command;
use tor_server::journal::{Action, Direction};
use tor_server::{scenario_package, Engine, Scenario};

const TURNS: usize = 12;
const DIRECTIONS: [Direction; 8] = [
    Direction::East,
    Direction::North,
    Direction::West,
    Direction::South,
    Direction::NorthEast,
    Direction::SouthWest,
    Direction::NorthWest,
    Direction::SouthEast,
];

fn states(engine: &Engine) -> Vec<(ActorId, StateView)> {
    engine
        .actors()
        .into_iter()
        .map(|actor| (actor, engine.state(actor).unwrap()))
        .collect()
}

fn finished(engine: &Engine, character: ActorId) -> bool {
    let observation = engine.observation(character).unwrap();
    !engine.alive(character) || observation.combat.as_ref().is_some_and(|c| c.terminal)
}

/// Play the package's character for a few turns. Returns the last accepted
/// request so it can be retried.
fn play(engine: &mut Engine, character: ActorId, name: &str) -> Option<(String, Command)> {
    let mut last = None;
    for turn in 0..TURNS {
        run_ai_turns(engine);
        if finished(engine, character) {
            break;
        }
        let mut accepted = false;
        for action in [
            Action::Move {
                direction: DIRECTIONS[turn % DIRECTIONS.len()],
            },
            Action::Wait,
        ] {
            let before = states(engine);
            let request = format!("turn-{turn}-{action:?}");
            let command = Command::Act {
                expected_revision: engine.revision(character).unwrap(),
                action: action.clone(),
            };
            let branch = engine.branch().clone();
            match engine.command(
                "player",
                "test",
                character,
                &request,
                &branch,
                command.clone(),
            ) {
                Ok(_) => {
                    last = Some((request, command));
                    accepted = true;
                    break;
                }
                Err(error) => assert_eq!(
                    states(engine),
                    before,
                    "{name}: rejected {action:?} ({error}) changed the game"
                ),
            }
        }
        if !accepted {
            break;
        }
    }
    last
}

#[test]
fn every_package_rejects_retries_replays_and_rewinds_exactly() {
    let packages = support::all_packages();
    assert!(
        packages.len() >= 30,
        "found only {} packages",
        packages.len()
    );
    for path in packages {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let scenario =
            scenario_package::load(&path, 7, None, false).unwrap_or_else(|e| panic!("{name}: {e}"));
        let character = ActorId(scenario.package.as_ref().unwrap().selected);
        let directory = tempfile::tempdir().unwrap();
        let save = directory.path().join("game.db");
        let mut engine = Engine::open(&save, scenario).unwrap();
        let initial = engine.state(character).unwrap();
        let before_play = states(&engine);

        let (request, command) = play(&mut engine, character, &name)
            .unwrap_or_else(|| panic!("{name}: the character could neither move nor wait"));
        let played = states(&engine);
        assert_ne!(played, before_play, "{name}: playing changed nothing");
        let branch = engine.branch().clone();
        let retry = engine.command("player", "test", character, &request, &branch, command);
        assert!(retry.is_ok(), "{name}: a retried request was refused");
        assert_eq!(states(&engine), played, "{name}: a retry acted again");

        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&save, Scenario::two_room(0)).unwrap();
        assert_eq!(
            states(&engine),
            played,
            "{name}: the restart replayed differently"
        );

        engine.enable_wizard().unwrap();
        support::wizard(&mut engine, character, "rewind initial")
            .unwrap_or_else(|e| panic!("{name}: rewind failed: {e}"));
        assert_eq!(
            engine.state(character).unwrap().observation,
            initial.observation,
            "{name}: rewinding didn't restore the start"
        );
    }
}
