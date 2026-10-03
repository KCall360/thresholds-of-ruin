use futures_util::{SinkExt, StreamExt};
use std::{collections::BTreeSet, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    time::timeout,
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::*;
use tor_server::journal::{Command, Position, RegionView, WizardItem, WizardOperation};
use tor_server::{serve, Account, Engine, Scenario, Service, Simulation};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;
/// The next message, past `waiting` signals, which only some tests watch for
/// (see [`receive_any`]).
async fn receive(client: &mut Client) -> ServerMessage {
    loop {
        match receive_any(client).await {
            ServerMessage::Waiting { .. } => continue,
            message => return message,
        }
    }
}
async fn receive_any(client: &mut Client) -> ServerMessage {
    let message = timeout(Duration::from_secs(5), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
async fn request(client: &mut Client, id: &str, request: Request) {
    let message = ClientMessage::Request {
        request_id: id.into(),
        request,
    };
    client
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}
async fn connect(address: &str, role: AccessRole) -> (Client, Snapshot) {
    let (mut client, _) = connect_async(address).await.unwrap();
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: format!("test-{role:?}"),
        frontend: "wizard-tests".into(),
    };
    client
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    assert!(
        matches!(receive(&mut client).await, ServerMessage::Welcome { role: r, .. } if r == role)
    );
    request(&mut client, "attach", Request::Attach { actor: ActorId(1) }).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut client).await else {
        panic!("snapshot")
    };
    (client, *snapshot)
}

/// A served two-room game with a wizard, a player and a spectator, all the
/// same user, so receipt retries and private notes can't bypass roles.
struct Game {
    wizard: Client,
    player: Client,
    spectator: Client,
    initial: Snapshot,
    stop: oneshot::Sender<()>,
    server: tokio::task::JoinHandle<std::io::Result<Service>>,
}

impl Game {
    async fn start(wizard_mode: bool) -> Self {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        if wizard_mode {
            engine.enable_wizard().unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}", listener.local_addr().unwrap());
        let accounts = [
            AccessRole::Wizard,
            AccessRole::Player,
            AccessRole::Spectator,
        ]
        .into_iter()
        .map(|role| Account {
            user: "same-user".into(),
            token: format!("test-{role:?}"),
            role,
            actors: BTreeSet::from([ActorId(1)]),
        })
        .collect();
        let (stop, stopped) = oneshot::channel();
        let server = tokio::spawn(serve(
            listener,
            Simulation::start(Service::new(engine)),
            accounts,
            async {
                let _ = stopped.await;
            },
        ));
        let (wizard, initial) = connect(&address, AccessRole::Wizard).await;
        let (player, _) = connect(&address, AccessRole::Player).await;
        let (spectator, _) = connect(&address, AccessRole::Spectator).await;
        assert_eq!(initial.state.wizard_game, wizard_mode);
        Game {
            wizard,
            player,
            spectator,
            initial,
            stop,
            server,
        }
    }

    async fn finish(self) {
        let _ = self.stop.send(());
        self.server.await.unwrap().unwrap();
    }
}

fn origin() -> Position {
    Position {
        region: 1,
        x: 1,
        y: 1,
        z: 0,
    }
}

/// One of every wizard operation.
fn operations() -> Vec<WizardOperation> {
    vec![
        WizardOperation::PlaceChamber {
            region: RegionView {
                id: 99,
                name: "Stone chamber".into(),
                width: 5,
                depth: 3,
                height: 2,
            },
        },
        WizardOperation::PlaceDoor {
            position: Position {
                region: 1,
                x: 0,
                y: 0,
                z: 0,
            },
            open: false,
            height: 1,
        },
        WizardOperation::SetPlaceHint {
            position: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
            present: true,
        },
        WizardOperation::PlaceRoom {
            region: RegionView {
                id: 3,
                name: "Room".into(),
                width: 5,
                depth: 5,
                height: 2,
            },
        },
        WizardOperation::Connect {
            from: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
            direction: Direction::Up,
            to: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
            quarter_turns: 0,
        },
        WizardOperation::SetWall {
            position: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
            wall: true,
        },
        WizardOperation::PlaceItem {
            kind: WizardItem::Tablet,
            position: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
        },
        WizardOperation::SpawnActor {
            position: Position {
                x: 2,
                ..Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                }
            },
            turn_ticks: 100,
        },
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: Position {
                region: 2,
                ..Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                }
            },
        },
        WizardOperation::Rewind { target: None },
    ]
}

fn wizard_request(
    branch: &BranchId,
    expected_revision: u64,
    operation: WizardOperation,
) -> Request {
    Request::Command {
        branch: branch.clone(),
        command: Command::Wizard {
            expected_revision,
            operation,
        }
        .try_into()
        .unwrap(),
    }
}

/// Move the character into region 2, where the tablet lies one cell east.
fn teleport() -> WizardOperation {
    WizardOperation::Teleport {
        actor: ActorId(1),
        position: Position {
            region: 2,
            ..origin()
        },
    }
}

async fn assert_error(client: &mut Client, expected: ErrorCode) {
    match receive(client).await {
        ServerMessage::Error { code, .. } => assert_eq!(code, expected),
        other => panic!("expected {expected:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn players_and_spectators_are_refused_every_wizard_operation() {
    for wizard_mode in [false, true] {
        let mut game = Game::start(wizard_mode).await;
        for (index, operation) in operations().into_iter().enumerate() {
            let command = wizard_request(&game.initial.branch, 0, operation);
            for client in [&mut game.player, &mut game.spectator] {
                request(client, &format!("denied-{index}"), command.clone()).await;
                assert_error(client, ErrorCode::Unauthorized).await;
            }
        }
        game.finish().await;
    }
}

#[tokio::test]
async fn without_wizard_mode_the_wizard_is_refused_too() {
    let mut game = Game::start(false).await;
    for (index, operation) in operations().into_iter().enumerate() {
        let command = wizard_request(&game.initial.branch, 0, operation);
        request(&mut game.wizard, &format!("disabled-{index}"), command).await;
        assert_error(&mut game.wizard, ErrorCode::Unauthorized).await;
    }
    game.finish().await;
}

#[tokio::test]
async fn an_accepted_wizard_command_reaches_every_client_and_its_retry_is_acknowledged() {
    let mut game = Game::start(true).await;
    let command = wizard_request(&game.initial.branch, 0, teleport());
    request(&mut game.wizard, "accepted", command.clone()).await;
    for client in [&mut game.wizard, &mut game.player, &mut game.spectator] {
        let ServerMessage::Snapshot { snapshot, .. } = receive(client).await else {
            panic!("new snapshot")
        };
        assert!(snapshot
            .state
            .observation
            .ground_items
            .iter()
            .any(|i| i.item.name == "stone tablet"
                && i.position == tor_protocol::Position { x: 1, y: 0, z: 0 }));
        assert!(snapshot.state.wizard_game);
    }
    assert!(matches!(
        receive(&mut game.wizard).await,
        ServerMessage::Ack { .. }
    ));
    // The same user's receipt doesn't let another role replay the command.
    for client in [&mut game.player, &mut game.spectator] {
        request(client, "accepted", command.clone()).await;
        assert_error(client, ErrorCode::Unauthorized).await;
    }
    request(&mut game.wizard, "accepted", command).await;
    assert!(matches!(
        receive(&mut game.wizard).await,
        ServerMessage::Ack { .. }
    ));
    game.finish().await;
}

#[tokio::test]
async fn a_wizard_rewind_forks_every_client_and_the_old_branch_is_refused() {
    let mut game = Game::start(true).await;
    let branch = game.initial.branch.clone();
    request(
        &mut game.wizard,
        "teleport",
        wizard_request(&branch, 0, teleport()),
    )
    .await;
    for client in [&mut game.wizard, &mut game.player, &mut game.spectator] {
        assert!(matches!(
            receive(client).await,
            ServerMessage::Snapshot { .. }
        ));
    }
    assert!(matches!(
        receive(&mut game.wizard).await,
        ServerMessage::Ack { .. }
    ));
    let rewind = WizardOperation::Rewind { target: None };
    request(
        &mut game.wizard,
        "rewind",
        wizard_request(&branch, 1, rewind.clone()),
    )
    .await;
    for client in [&mut game.wizard, &mut game.player, &mut game.spectator] {
        let ServerMessage::Snapshot { snapshot, .. } = receive(client).await else {
            panic!("rewind snapshot")
        };
        assert!(snapshot
            .state
            .observation
            .ground_items
            .iter()
            .any(|i| i.item.name == "copper token" && i.reachable));
        assert_ne!(snapshot.branch, branch);
        assert!(snapshot.state.wizard_game);
    }
    assert!(matches!(
        receive(&mut game.wizard).await,
        ServerMessage::Ack { .. }
    ));
    request(
        &mut game.wizard,
        "stale",
        wizard_request(&branch, 0, rewind),
    )
    .await;
    assert_error(&mut game.wizard, ErrorCode::WrongBranch).await;
    game.finish().await;
}
