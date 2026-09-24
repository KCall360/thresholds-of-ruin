use std::time::Duration;
use tempfile::tempdir;
use tor_protocol::{Action, ActorId};
use tor_server::{journal::Command, Engine, SavePolicy, Scenario};
mod support;

fn act(engine: &mut Engine, request: &str) -> tor_server::CommandResult {
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            request,
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap()
}
fn slow_policy() -> SavePolicy {
    SavePolicy {
        target_interval: Duration::from_secs(3600),
        max_unsaved_age: Duration::from_secs(7200),
        ..SavePolicy::default()
    }
}
#[test]
fn ordinary_acknowledgements_do_not_write_and_explicit_save_is_a_barrier() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.db");
    let mut engine =
        Engine::open_with_policy(&path, Scenario::two_room(42), slow_policy()).unwrap();
    let baseline = std::fs::read(&path).unwrap();
    let result = act(&mut engine, "one");
    assert!(!result.duplicate);
    assert_eq!(std::fs::read(&path).unwrap(), baseline);
    let status = engine.save_status();
    assert_eq!(status.accepted_sequence, 1);
    assert_eq!(status.durable_sequence, 0);
    engine.flush().unwrap();
    assert_eq!(engine.save_status().durable_sequence, 1);
    let rows = || {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let rows = conn
            .prepare("SELECT frame FROM journal ORDER BY sequence")
            .unwrap()
            .query_map([], |r| r.get::<_, Vec<u8>>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    let saved_prefix = rows();
    act(&mut engine, "two");
    engine.flush().unwrap();
    assert_eq!(&rows()[..2], &saved_prefix);
    let expected = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let mut resumed = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(resumed.state(ActorId(1)).unwrap(), expected);
    let duplicate = resumed
        .command(
            "player",
            "test",
            ActorId(1),
            "one",
            &resumed.branch().clone(),
            Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        )
        .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.entry.id, result.entry.id);
}
#[test]
fn unsaved_queue_is_bounded_without_mutating_a_rejected_action() {
    let dir = tempdir().unwrap();
    let policy = SavePolicy {
        max_pending_bytes: 1,
        ..slow_policy()
    };
    let mut engine =
        Engine::open_with_policy(dir.path().join("game.db"), Scenario::two_room(0), policy)
            .unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    assert!(engine
        .command(
            "player",
            "test",
            ActorId(1),
            "one",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: 0,
                action: Action::Wait
            }
        )
        .is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    assert_eq!(engine.save_status().accepted_sequence, 0);
}
#[test]
fn wizard_mark_is_durable_without_a_following_command() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.db");
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), slow_policy()).unwrap();
    engine.enable_wizard().unwrap();
    assert_eq!(
        engine.save_status().accepted_sequence,
        engine.save_status().durable_sequence
    );
    assert!(engine.state(ActorId(1)).unwrap().wizard_game);
    engine.flush().unwrap();
    drop(engine);
    assert!(
        Engine::open(&path, Scenario::two_room(0))
            .unwrap()
            .state(ActorId(1))
            .unwrap()
            .wizard_game
    );
}

#[tokio::test]
async fn server_shutdown_flushes_without_requiring_engine_drop() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.db");
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), slow_policy()).unwrap();
    act(&mut engine, "one");
    let service = std::sync::Arc::new(tokio::sync::Mutex::new(tor_server::Service::new(engine)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    tor_server::serve(
        listener,
        service.clone(),
        vec![tor_server::Account {
            user: "player".into(),
            token: "test-token".into(),
            role: tor_protocol::AccessRole::Player,
            actors: [ActorId(1)].into(),
        }],
        async {},
    )
    .await
    .unwrap();
    // Service still owns the engine: this proves the server barrier, not Drop.
    assert_eq!(support::read(&path)["records"].as_array().unwrap().len(), 1);
}

#[test]
fn damaged_committed_frames_fail_closed_and_preserve_evidence() {
    for damage in [
        "checksum",
        "gap",
        "length",
        "missing",
        "unknown",
        "identity",
        "generation",
    ] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("game.db");
        let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        act(&mut engine, "one");
        drop(engine);
        let conn = rusqlite::Connection::open(&path).unwrap();
        let mut bytes: Vec<u8> = conn
            .query_row("SELECT frame FROM journal WHERE sequence=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        match damage {
            "checksum" => bytes[24] ^= 1,
            "gap" => {
                conn.execute("UPDATE journal SET sequence=2 WHERE sequence=1", [])
                    .unwrap();
            }
            "length" => bytes.resize(1024 * 1024 + 25, 0),
            _ => {
                let mut payload: serde_json::Value = serde_json::from_slice(&bytes[24..]).unwrap();
                match damage {
                    "missing" => {
                        payload["record"].as_object_mut().unwrap().remove("receipt");
                    }
                    "unknown" => {
                        payload["record"]["extra"] = true.into();
                    }
                    "identity" => payload["save_id"] = uuid::Uuid::new_v4().to_string().into(),
                    "generation" => payload["generation"] = 1.into(),
                    _ => unreachable!(),
                }
                bytes = support::frame(1, &payload);
            }
        }
        if damage != "gap" {
            conn.execute("UPDATE journal SET frame=?1 WHERE sequence=1", [bytes])
                .unwrap();
        }
        drop(conn);
        let evidence = std::fs::read(&path).unwrap();
        assert!(
            Engine::open(&path, Scenario::two_room(0)).is_err(),
            "{damage}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), evidence, "{damage}");
    }
}
