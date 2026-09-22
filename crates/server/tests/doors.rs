use tempfile::tempdir;
use tor_protocol::{Action, ActorId, ErrorCode, Position};
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

fn send(
    engine: &mut Engine,
    id: &str,
    command: Command,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    engine.command(
        "player",
        "text",
        ActorId(1),
        id,
        &engine.branch().clone(),
        command,
    )
}
fn act(engine: &mut Engine, id: &str, action: Action) -> tor_server::CommandResult {
    let command = Command::Act {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        action,
    };
    send(engine, id, command).unwrap()
}
fn wizard(
    engine: &mut Engine,
    id: &str,
    operation: &str,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    let command = Command::from_wire(&tor_protocol::Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: operation.into(),
    })?;
    send(engine, id, command)
}

#[test]
fn doors_replay_retry_and_rewind_with_disclosed_reach_and_approaches() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("doors.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let view = engine.observation(ActorId(1)).unwrap();
    let cell = view
        .visible_cells
        .iter()
        .find(|c| c.door.is_some())
        .unwrap();
    let door = cell.door.as_ref().unwrap().id;
    assert!(cell.door.as_ref().unwrap().open);
    assert!(!cell.door.as_ref().unwrap().reachable);
    assert!(cell
        .door
        .as_ref()
        .unwrap()
        .approaches
        .iter()
        .all(|key| view.visible_cells.iter().any(|c| &c.key == key)));
    for step in 0..3 {
        act(
            &mut engine,
            &format!("approach-{step}"),
            Action::Move {
                direction: tor_protocol::Direction::East,
            },
        );
    }
    let before_close = engine.state(ActorId(1)).unwrap();
    let command = Command::Act {
        expected_revision: before_close.revision,
        action: Action::SetDoor { door, open: false },
    };
    let closed = send(&mut engine, "close", command.clone()).unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    assert_eq!(expected.observation.tick, 400);
    assert!(
        !expected
            .observation
            .visible_cells
            .iter()
            .find_map(|c| c.door.as_ref())
            .unwrap()
            .open
    );
    drop(engine);
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    assert!(send(&mut engine, "close", command).unwrap().duplicate);
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    engine.enable_wizard().unwrap();
    act(&mut engine, "open", Action::SetDoor { door, open: true });
    wizard(
        &mut engine,
        "rewind",
        &format!("rewind {}", closed.entry.id.0),
    )
    .unwrap();
    assert!(
        !engine
            .observation(ActorId(1))
            .unwrap()
            .visible_cells
            .iter()
            .find_map(|c| c.door.as_ref())
            .unwrap()
            .open
    );
}

#[test]
fn wizard_interior_door_hides_contents_and_does_not_reveal_topology() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    assert_eq!(
        wizard(&mut engine, "no", "door 1 0 0 0 closed")
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    for (id, operation) in [
        ("room", "room 3 6 1 1 Secret hallway"),
        ("item", "item tablet 3 4 0 0"),
        ("door", "door 3 2 0 0 closed"),
        ("visit", "teleport 1 3 1 0 0"),
    ] {
        wizard(&mut engine, id, operation).unwrap();
    }
    let view = engine.observation(ActorId(1)).unwrap();
    assert!(view.ground_items.is_empty());
    let door = view
        .visible_cells
        .iter()
        .find_map(|c| c.door.as_ref())
        .unwrap();
    assert!(door.reachable);
    assert_eq!(door.approaches.len(), 1);
    let encoded = serde_json::to_string(&view).unwrap();
    for hidden in ["Secret", "region", "portal", "stone tablet"] {
        assert!(!encoded.contains(hidden));
    }
    act(
        &mut engine,
        "open",
        Action::SetDoor {
            door: door.id,
            open: true,
        },
    );
    let view = engine.observation(ActorId(1)).unwrap();
    assert_eq!(view.ground_items.len(), 1);
    let destination = view
        .visible_cells
        .iter()
        .find(|c| c.position == Position { x: 3, y: 0, z: 0 })
        .unwrap()
        .key
        .clone();
    assert!(engine.travel_route(ActorId(1), &destination).is_ok());
    act(
        &mut engine,
        "close",
        Action::SetDoor {
            door: door.id,
            open: false,
        },
    );
    assert!(engine.travel_route(ActorId(1), &destination).is_err());
}

#[test]
fn new_fixture_places_its_only_door_in_an_unhinted_one_cell_hall() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    let view = engine.observation(ActorId(1)).unwrap();
    let door = view
        .visible_cells
        .iter()
        .find(|c| c.door.is_some())
        .unwrap();
    assert_eq!(door.position, Position { x: 4, y: 0, z: 0 });
    assert!(!door.place_hint);
    assert_eq!(
        view.visible_cells
            .iter()
            .filter(|c| c.door.is_some())
            .count(),
        1
    );
    for y in [-1, 1] {
        assert!(view
            .visible_cells
            .iter()
            .any(|c| c.position == Position { x: 4, y, z: 0 } && c.wall));
    }
    assert_eq!(
        view.visible_cells.iter().filter(|c| c.place_hint).count(),
        2
    );
    for step in 0..3 {
        act(
            &mut engine,
            &format!("approach-{step}"),
            Action::Move {
                direction: tor_protocol::Direction::East,
            },
        );
    }
    act(
        &mut engine,
        "close",
        Action::SetDoor {
            door: 1,
            open: false,
        },
    );
    assert!(!engine
        .observation(ActorId(1))
        .unwrap()
        .ground_items
        .iter()
        .any(|i| i.item.name == "stone tablet"));
    act(
        &mut engine,
        "open",
        Action::SetDoor {
            door: 1,
            open: true,
        },
    );
    for step in 0..2 {
        act(
            &mut engine,
            &format!("cross-{step}"),
            Action::Move {
                direction: tor_protocol::Direction::East,
            },
        );
    }
    assert_eq!(
        engine
            .observation(ActorId(1))
            .unwrap()
            .ground_items
            .iter()
            .find(|i| i.item.name == "stone tablet")
            .unwrap()
            .position,
        Position { x: 2, y: 0, z: 0 }
    );
}
