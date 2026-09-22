use tempfile::tempdir;
use tor_protocol::{ActorId, Command as WireCommand};
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

fn wizard(
    engine: &mut Engine,
    id: &str,
    operation: &str,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    let command = Command::from_wire(&WireCommand::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: operation.into(),
    })?;
    engine.command(
        "wizard",
        "text",
        ActorId(1),
        id,
        &engine.branch().clone(),
        command,
    )
}

#[test]
fn chamber_setup_is_atomic_retryable_rewindable_and_durable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    for text in [
        "chamber 3 0 3 2 Invalid",
        "chamber 3 5 3 9 Invalid",
        "chamber 1 5 3 2 Duplicate",
    ] {
        assert!(wizard(&mut engine, "invalid", text).is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    }
    let command = Command::from_wire(&WireCommand::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: "chamber 3 5 3 2 Stone chamber".into(),
    })
    .unwrap();
    let branch = engine.branch().clone();
    let first = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "room",
            &branch,
            command.clone(),
        )
        .unwrap();
    let retry = engine
        .command("wizard", "text", ActorId(1), "room", &branch, command)
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry, first.entry);
    let entered = wizard(&mut engine, "teleport", "teleport 1 3 1 1 0").unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    assert!(wizard(&mut engine, "floor", "teleport 1 3 1 1 -1").is_err());
    assert!(wizard(&mut engine, "ceiling", "teleport 1 3 1 1 2").is_err());
    wizard(&mut engine, "hole", "wall 3 1 1 2 open").unwrap();
    assert!(engine
        .observation(ActorId(1))
        .unwrap()
        .visible_cells
        .iter()
        .find(|c| c.position.x == 0 && c.position.y == 0)
        .unwrap()
        .ceiling
        .is_none());
    wizard(
        &mut engine,
        "rewind",
        &format!("rewind {}", entered.entry.id.0),
    )
    .unwrap();
    assert_eq!(
        engine.observation(ActorId(1)).unwrap(),
        expected.observation
    );
    let final_state = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), final_state);
}
