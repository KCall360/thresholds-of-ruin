use tempfile::tempdir;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

#[test]
fn diagonal_receipts_replay_and_legacy_rules_are_preserved() {
    for rules in [
        "diagonal-v11",
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
        let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        let before = engine.state(ActorId(1)).unwrap();
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: before.revision,
            action: Action::Move {
                direction: Direction::NorthEast,
            },
        };
        let result = engine.command(
            "player",
            "test",
            ActorId(1),
            "diagonal",
            &branch,
            command.clone(),
        );
        if rules == "diagonal-v11" {
            let result = result.unwrap();
            assert_eq!(engine.observation(ActorId(1)).unwrap().tick, 142);
            let retry = engine
                .command("player", "test", ActorId(1), "diagonal", &branch, command)
                .unwrap();
            assert_eq!(retry.entry.id, result.entry.id);
        } else {
            assert!(result.is_err(), "{rules}");
            assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        }
        let final_state = engine.state(ActorId(1)).unwrap();
        drop(engine);
        let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(engine.state(ActorId(1)).unwrap(), final_state);
    }
}
