//! Current format-3 file recovery tests. Proposed journal/checkpoint fault
//! schedules are documented, not implemented as Phase B storage here.
use tor_protocol::{Action, ActorId, ErrorCode};
use tor_server::{journal::Command, Engine, Scenario};

#[test]
fn acknowledgement_loss_and_restart_recover_the_original_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let branch = engine.branch().clone();
    let command = Command::Act {
        expected_revision: 0,
        action: Action::Wait,
    };
    let accepted = engine
        .command(
            "player",
            "headless",
            ActorId(1),
            "lost-ack",
            &branch,
            command.clone(),
        )
        .unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine); // The caller did not receive the result before termination.
    let mut resumed = Engine::open(&path, Scenario::two_room(999)).unwrap();
    let retry = resumed
        .command(
            "player",
            "headless",
            ActorId(1),
            "lost-ack",
            &branch,
            command,
        )
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry, accepted.entry);
    assert_eq!(resumed.state(ActorId(1)).unwrap(), expected);
    assert_eq!(resumed.profile_counts().0, 1);
    let conflict = resumed
        .command(
            "player",
            "headless",
            ActorId(1),
            "lost-ack",
            &branch,
            Command::Act {
                expected_revision: 1,
                action: Action::Wait,
            },
        )
        .unwrap_err();
    assert_eq!(conflict.code, ErrorCode::RequestConflict);
}

#[test]
fn damaged_whole_archives_fail_closed_without_overwriting_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("original.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    engine
        .command(
            "player",
            "headless",
            ActorId(1),
            "one",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        )
        .unwrap();
    drop(engine);
    let bytes = std::fs::read(&path).unwrap();
    let mut cases = vec![];
    for cut in [0, 1, bytes.len() / 2, bytes.len() - 1] {
        cases.push(bytes[..cut].to_vec());
    }
    let mut invalid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    invalid["records"][0]["entry"]["tick"] = serde_json::json!(999);
    cases.push(serde_json::to_vec(&invalid).unwrap());
    invalid["version"] = serde_json::json!(4);
    cases.push(serde_json::to_vec(&invalid).unwrap());
    for (i, corrupt) in cases.iter().enumerate() {
        let path = directory.path().join(format!("corrupt-{i}.json"));
        std::fs::write(&path, corrupt).unwrap();
        assert_eq!(
            Engine::open(&path, Scenario::two_room(42))
                .unwrap_err()
                .code,
            ErrorCode::InvalidArchive
        );
        assert_eq!(std::fs::read(&path).unwrap(), *corrupt);
    }
}
