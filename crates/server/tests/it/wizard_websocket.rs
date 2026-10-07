use futures_util::SinkExt;
use std::collections::BTreeSet;
use tokio::{net::TcpListener, sync::oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tor_protocol::*;
use tor_server::journal::{Command, Position, RegionView, WizardItem, WizardOperation};
use tor_server::{serve, Account, Engine, Scenario, Service, Simulation};

use crate::wire_client::WireClient as Client;
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
    client
        .receive()
        .await
        .expect("connected protocol test client")
}
async fn connect(address: &str, role: AccessRole) -> (Client, Snapshot) {
    let (socket, _) = connect_async(address).await.unwrap();
    let mut client = Client::new(socket);
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
    client
        .request("attach", Request::Attach { actor: ActorId(1) })
        .await;
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
            direction: tor_server::journal::Direction::Up,
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
    context: InputContext,
    branch: &BranchId,
    expected_revision: u64,
    operation: WizardOperation,
) -> Request {
    Request::Command {
        context,
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
            let command = wizard_request(
                game.wizard.input_context(),
                &game.initial.branch,
                0,
                operation,
            );
            for client in [&mut game.player, &mut game.spectator] {
                client
                    .request(&format!("denied-{index}"), command.clone())
                    .await;
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
        let command = wizard_request(
            game.wizard.input_context(),
            &game.initial.branch,
            0,
            operation,
        );
        game.wizard
            .request(&format!("disabled-{index}"), command)
            .await;
        assert_error(&mut game.wizard, ErrorCode::Unauthorized).await;
    }
    game.finish().await;
}

#[tokio::test]
async fn an_accepted_wizard_command_reaches_every_client_and_its_retry_is_acknowledged() {
    let mut game = Game::start(true).await;
    let command = wizard_request(
        game.wizard.input_context(),
        &game.initial.branch,
        0,
        teleport(),
    );
    game.wizard.request("accepted", command.clone()).await;
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
        client.request("accepted", command.clone()).await;
        assert_error(client, ErrorCode::Unauthorized).await;
    }
    game.wizard.request("accepted", command).await;
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
    game.wizard
        .request(
            "teleport",
            wizard_request(game.wizard.input_context(), &branch, 0, teleport()),
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
    game.wizard
        .request(
            "rewind",
            wizard_request(game.wizard.input_context(), &branch, 1, rewind.clone()),
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
    game.wizard
        .request(
            "stale",
            wizard_request(game.wizard.input_context(), &branch, 0, rewind),
        )
        .await;
    assert_error(&mut game.wizard, ErrorCode::WrongBranch).await;
    game.finish().await;
}

#[tokio::test]
async fn immediate_note_receipt_retry_after_rewind_keeps_the_original_branch() {
    let mut game = Game::start(true).await;
    let branch = game.initial.branch.clone();
    let note = Request::Command {
        context: game.wizard.input_context(),
        branch: branch.clone(),
        command: tor_protocol::Command::Annotate {
            anchor: Anchor::State { revision: 0 },
            text: "Original branch note".into(),
            source: ClientSource::User,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
        },
    };
    game.wizard.request("note-receipt", note.clone()).await;
    for client in [&mut game.wizard, &mut game.player, &mut game.spectator] {
        assert!(
            matches!(receive(client).await, ServerMessage::Update { update }
            if matches!(update.body, UpdateBody::Annotation { .. }))
        );
    }
    let ServerMessage::Ack {
        receipt: original, ..
    } = receive(&mut game.wizard).await
    else {
        panic!("receipt required")
    };
    assert!(matches!(
        &original,
        RequestReceipt::Immediate {
            entry_id: Some(_),
            ..
        }
    ));
    assert_eq!(original.actor(), ActorId(1));
    assert_eq!(original.branch(), &branch);
    game.wizard
        .request(
            "rewind",
            wizard_request(
                game.wizard.input_context(),
                &branch,
                0,
                WizardOperation::Rewind { target: None },
            ),
        )
        .await;
    let mut current_branch = None;
    for client in [&mut game.wizard, &mut game.player, &mut game.spectator] {
        let ServerMessage::Snapshot { snapshot, .. } = receive(client).await else {
            panic!("rewind snapshot required")
        };
        assert_ne!(snapshot.branch, branch);
        current_branch = Some(snapshot.branch.clone());
    }
    assert!(matches!(
        receive(&mut game.wizard).await,
        ServerMessage::Ack { .. }
    ));
    game.wizard.request("note-receipt", note).await;
    let ServerMessage::Ack {
        context,
        receipt: retried,
        ..
    } = receive(&mut game.wizard).await
    else {
        panic!("retry receipt required")
    };
    assert_eq!(retried, original);
    assert_eq!(Some(context.branch.clone()), current_branch);
    assert_ne!(&context.branch, retried.branch());
    game.wizard.request("after-retry", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut game.wizard).await else {
        panic!("snapshot required")
    };
    assert_eq!(Some(snapshot.branch), current_branch);
    assert_eq!(snapshot.state.observation.tick, 0);
    game.finish().await;
}
