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
fn old_saves_keep_their_geometry_and_reject_chambers() {
    for rules in [
        "two-room-v1",
        "portal-sight-v2",
        "observer-scene-v3",
        "place-hints-v4",
        "travel-v5",
        "doors-v6",
        "shadowcasting-v7",
        "doorway-v8",
    ] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("game.json");
        drop(Engine::open(&path, Scenario::two_room(42)).unwrap());
        let mut archive: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        archive["ruleset"] = rules.into();
        std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
        let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        let view = engine.observation(ActorId(1)).unwrap();
        assert!(view
            .visible_cells
            .iter()
            .all(|c| c.floor.is_none() && c.ceiling.is_none()));
        assert!(!view.visible_cells.iter().any(|c| c.position.x == -2));
        engine.enable_wizard().unwrap();
        let before = engine.state(ActorId(1)).unwrap();
        assert!(wizard(&mut engine, "chamber", "chamber 3 5 3 2 Test").is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        drop(engine);
        let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    }
}

#[test]
fn material_rim_fix_keeps_original_material_save_replay_unchanged() {
    for rules in ["material-volumes-v9", "material-rims-v10"] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("game.json");
        drop(Engine::open(&path, Scenario::two_room(42)).unwrap());
        let mut archive: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        archive["ruleset"] = rules.into();
        std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
        let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        for step in 0..3 {
            let command = Command::Act {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: tor_protocol::Action::Move {
                    direction: tor_protocol::Direction::East,
                },
            };
            engine
                .command(
                    "player",
                    "ascii",
                    ActorId(1),
                    &format!("step-{step}"),
                    &engine.branch().clone(),
                    command,
                )
                .unwrap();
        }
        let before = engine.state(ActorId(1)).unwrap();
        let corner = before
            .observation
            .visible_cells
            .iter()
            .find(|c| c.position == tor_protocol::Position { x: 2, y: -1, z: 0 })
            .unwrap();
        assert_eq!(corner.wall, rules == "material-volumes-v9");
        drop(engine);
        let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        engine.enable_wizard().unwrap();
        let anchor = wizard(&mut engine, "anchor", "chamber 3 3 3 2 Test").unwrap();
        wizard(&mut engine, "teleport", "teleport 1 3 1 1 0").unwrap();
        wizard(
            &mut engine,
            "rewind",
            &format!("rewind {}", anchor.entry.id.0),
        )
        .unwrap();
        assert_eq!(engine.observation(ActorId(1)).unwrap(), before.observation);
    }
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
