use tempfile::tempdir;
use tor_protocol::*;
use tor_server::journal::{Command, Position, RegionView, WizardItem, WizardOperation};
use tor_server::{Engine, Scenario};

fn position(region: u64, x: i32, y: i32, z: i32) -> Position {
    Position { region, x, y, z }
}

fn wizard(
    engine: &mut Engine,
    id: &str,
    operation: WizardOperation,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    let revision = engine.revision(ActorId(1)).unwrap();
    let branch = engine.branch().clone();
    engine.command(
        "developer",
        "headless",
        ActorId(1),
        id,
        &branch,
        Command::Wizard {
            expected_revision: revision,
            operation,
        },
    )
}

#[test]
fn geometry_commands_are_durable_atomic_and_rewindable() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("geometry.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let room = WizardOperation::PlaceRoom {
        region: RegionView {
            id: 3,
            name: "Upper".into(),
            width: 5,
            depth: 5,
            height: 2,
        },
    };
    assert_eq!(
        wizard(&mut engine, "denied", room.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    wizard(&mut engine, "room", room.clone()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(wizard(&mut engine, "duplicate-room", room).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    for region in [
        RegionView {
            id: 0,
            name: "Invalid".into(),
            width: 5,
            depth: 5,
            height: 1,
        },
        RegionView {
            id: 4,
            name: "Oversized".into(),
            width: i32::MAX,
            depth: 5,
            height: 1,
        },
        RegionView {
            id: 4,
            name: "bad\nname".into(),
            width: 5,
            depth: 5,
            height: 1,
        },
    ] {
        assert!(wizard(
            &mut engine,
            "invalid-room",
            WizardOperation::PlaceRoom { region }
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    wizard(
        &mut engine,
        "link",
        WizardOperation::Connect {
            from: position(1, 1, 0, 0),
            direction: Direction::North,
            to: position(3, 0, 2, 1),
            quarter_turns: 1,
        },
    )
    .unwrap();
    wizard(
        &mut engine,
        "item",
        WizardOperation::PlaceItem {
            kind: WizardItem::Tablet,
            position: position(3, 1, 2, 1),
        },
    )
    .unwrap();
    let view = engine.observation(ActorId(1)).unwrap();
    assert!(view
        .ground_items
        .iter()
        .any(|item| item.position == tor_protocol::Position { x: 0, y: -3, z: 0 }));
    assert!(!serde_json::to_string(&view).unwrap().contains("region"));
    wizard(
        &mut engine,
        "wall",
        WizardOperation::SetWall {
            position: position(3, 0, 2, 1),
            wall: true,
        },
    )
    .unwrap();
    assert!(!engine
        .observation(ActorId(1))
        .unwrap()
        .ground_items
        .iter()
        .any(|item| item.position.y == -3));
    let state = engine.state(ActorId(1)).unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    engine.enable_wizard().unwrap();
    wizard(
        &mut engine,
        "rewind",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    assert!(wizard(
        &mut engine,
        "missing",
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: position(3, 1, 1, 1)
        }
    )
    .is_err());
}

#[test]
fn public_scenes_and_history_do_not_expose_backend_geometry() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let initial = engine.observation(ActorId(1)).unwrap();
    let tablet = initial
        .ground_items
        .iter()
        .find(|i| i.item.name == "stone tablet")
        .unwrap();
    assert_eq!(tablet.position, tor_protocol::Position { x: 7, y: 0, z: 0 });
    assert!(!tablet.reachable);
    let key = initial
        .visible_cells
        .iter()
        .find(|c| c.position == tablet.position)
        .unwrap()
        .key
        .clone();
    let branch = engine.branch().clone();
    for index in 0..5 {
        engine
            .command(
                "test",
                "test",
                ActorId(1),
                &format!("move-{index}"),
                &branch,
                Command::Act {
                    expected_revision: index,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
    }
    let arrived = engine.observation(ActorId(1)).unwrap();
    assert!(arrived
        .visible_cells
        .iter()
        .any(|c| c.key == key && c.position == tor_protocol::Position { x: 2, y: 0, z: 0 }));
    engine.enable_wizard().unwrap();
    wizard(
        &mut engine,
        "room",
        WizardOperation::PlaceRoom {
            region: RegionView {
                id: 77,
                name: "SECRET_STORAGE_NAME".into(),
                width: 7,
                depth: 7,
                height: 1,
            },
        },
    )
    .unwrap();
    let encoded = serde_json::to_string(&(
        engine.state(ActorId(1)).unwrap(),
        engine.history(ActorId(1), "test", None, 100).unwrap(),
        engine.history(ActorId(1), "developer", None, 100).unwrap(),
    ))
    .unwrap();
    for forbidden in [
        "region",
        "portal",
        "quarter_turns",
        "known_places",
        "SECRET_STORAGE_NAME",
        "from",
        "\"to\"",
        "width",
        "depth",
    ] {
        assert!(!encoded.contains(forbidden), "leaked {forbidden}");
    }
}
