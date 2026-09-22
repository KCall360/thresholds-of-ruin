use tempfile::tempdir;
use tor_protocol::{Action, ActorId, Command as WireCommand, Direction, Position};
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

fn command(engine: &mut Engine, id: &str, command: Command) -> tor_server::CommandResult {
    engine
        .command(
            "player",
            "ascii",
            ActorId(1),
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}
#[test]
fn replay_restores_knowledge_receipts_and_moves_and_rewind_forgets_future() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("travel.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let initial = engine.observation(ActorId(1)).unwrap();
    let destination = initial
        .visible_cells
        .iter()
        .find(|c| c.position == Position { x: 2, y: 0, z: 0 })
        .unwrap()
        .key
        .clone();
    let travel = Command::Travel {
        expected_revision: 0,
        destination: destination.clone(),
    };
    let receipt = command(&mut engine, "travel", travel.clone());
    assert_eq!(engine.observation(ActorId(1)).unwrap().tick, 0);
    command(
        &mut engine,
        "step",
        Command::Act {
            expected_revision: 0,
            action: Action::Move {
                direction: Direction::East,
            },
        },
    );
    let expected = engine.state(ActorId(1)).unwrap();
    let route = engine.travel_route(ActorId(1), &destination).unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    assert_eq!(
        engine.travel_route(ActorId(1), &destination).unwrap(),
        route
    );
    let retried = command(&mut engine, "travel", travel);
    assert!(retried.duplicate);
    assert_eq!(retried.entry.id, receipt.entry.id);
    engine.enable_wizard().unwrap();
    for (id, operation) in [
        ("room", "room 3 3 3 1 Hidden"),
        ("visit", "teleport 1 3 1 1 0"),
    ] {
        let setup = Command::from_wire(&WireCommand::Wizard {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            operation: operation.into(),
        })
        .unwrap();
        command(&mut engine, id, setup);
    }
    let future = engine.observation(ActorId(1)).unwrap().visible_cells[0]
        .key
        .clone();
    let rewind = Command::from_wire(&WireCommand::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: "rewind initial".into(),
    })
    .unwrap();
    command(&mut engine, "rewind", rewind);
    assert!(engine.travel_route(ActorId(1), &future).is_err());
    assert!(engine.travel_route(ActorId(1), &destination).is_ok());
}

#[test]
fn previously_seen_offscreen_destinations_remain_routable_after_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("remembered.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    for (id, operation) in [
        ("room", "room 3 20 1 1 Corridor"),
        ("teleport", "teleport 1 3 0 0 0"),
    ] {
        let setup = Command::from_wire(&WireCommand::Wizard {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            operation: operation.into(),
        })
        .unwrap();
        command(&mut engine, id, setup);
    }
    let origin = engine
        .observation(ActorId(1))
        .unwrap()
        .visible_cells
        .iter()
        .find(|c| c.position == Position { x: 0, y: 0, z: 0 })
        .unwrap()
        .key
        .clone();
    for index in 0..15 {
        let revision = engine.revision(ActorId(1)).unwrap();
        command(
            &mut engine,
            &format!("step-{index}"),
            Command::Act {
                expected_revision: revision,
                action: Action::Move {
                    direction: Direction::East,
                },
            },
        );
    }
    assert!(!engine
        .observation(ActorId(1))
        .unwrap()
        .visible_cells
        .iter()
        .any(|c| c.key == origin));
    let route = engine.travel_route(ActorId(1), &origin).unwrap();
    assert_eq!(route.len(), 15);
    drop(engine);
    let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.travel_route(ActorId(1), &origin).unwrap(), route);
}
