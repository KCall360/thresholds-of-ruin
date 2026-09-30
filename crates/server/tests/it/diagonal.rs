use tor_protocol::{Action, ActorId, Direction};
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

#[test]
fn diagonal_receipts_replay() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let branch = engine.branch().clone();
    let command = Command::Act {
        expected_revision: 0,
        action: Action::Move {
            direction: Direction::NorthEast,
        },
    };
    let result = engine
        .command(
            "player",
            "test",
            ActorId(1),
            "diagonal",
            &branch,
            command.clone(),
        )
        .unwrap();
    assert_eq!(engine.observation(ActorId(1)).unwrap().tick, 142);
    let retry = engine
        .command("player", "test", ActorId(1), "diagonal", &branch, command)
        .unwrap();
    assert_eq!(retry.entry.id, result.entry.id);
}
