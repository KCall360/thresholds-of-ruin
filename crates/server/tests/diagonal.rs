use tempfile::tempdir;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

#[test]
fn diagonal_receipts_replay_and_old_rulesets_are_rejected() {
    for rules in [
        "material-rims-v10",
        "material-volumes-v9",
        "doorway-v8",
        "shadowcasting-v7",
        "doors-v6",
        "travel-v5",
        "place-hints-v4",
        "observer-scene-v3",
        "portal-sight-v2",
        "two-room-v1",
    ] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("save.json");
        drop(Engine::open(&path, Scenario::two_room(42)).unwrap());
        let mut archive: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        archive["ruleset"] = rules.into();
        std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
        assert!(
            Engine::open(&path, Scenario::two_room(0)).is_err(),
            "{rules}"
        );
    }

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
