use super::websocket::assert_picture_payload;
use futures_util::SinkExt;
use std::collections::BTreeSet;
use tokio::{net::TcpListener, sync::oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tor_protocol::*;
use tor_server::journal::{Position, RegionView, WizardItem, WizardOperation};
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

/// Representative parser inputs for the common authority gate. The session
/// rejects these before parsing; this is deliberately not an enum inventory.
/// Per-operation parsing and mutation are covered by the developer/unit tests.
fn common_gate_operations() -> Vec<WizardOperation> {
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
        command: tor_protocol::Command::Wizard {
            expected_revision,
            operation: serde_json::to_string(&operation).unwrap(),
        },
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
async fn common_wizard_gate_refuses_players_and_spectators_before_parsing() {
    for wizard_mode in [false, true] {
        let mut game = Game::start(wizard_mode).await;
        for (index, operation) in common_gate_operations().into_iter().enumerate() {
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
        for operation in ["not a valid operation", "{"] {
            let command = Request::Command {
                context: game.wizard.input_context(),
                branch: game.initial.branch.clone(),
                command: Command::Wizard {
                    expected_revision: 0,
                    operation: operation.into(),
                },
            };
            for client in [&mut game.player, &mut game.spectator] {
                client
                    .request("invalid-parser-input", command.clone())
                    .await;
                assert_error(client, ErrorCode::Unauthorized).await;
            }
        }
        game.player.request("unchanged", Request::Snapshot).await;
        let ServerMessage::Snapshot { snapshot, .. } = receive(&mut game.player).await else {
            panic!("snapshot")
        };
        assert_picture_payload(&snapshot.state, &game.initial.state);
        assert_eq!(snapshot.history, game.initial.history);
        game.finish().await;
    }
}

#[tokio::test]
async fn without_wizard_mode_the_wizard_is_refused_too() {
    let mut game = Game::start(false).await;
    for (index, operation) in common_gate_operations().into_iter().enumerate() {
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
    let command = Request::Command {
        context: game.wizard.input_context(),
        branch: game.initial.branch.clone(),
        command: Command::Wizard {
            expected_revision: 0,
            operation: "{".into(),
        },
    };
    game.wizard.request("invalid-parser-input", command).await;
    assert_error(&mut game.wizard, ErrorCode::Unauthorized).await;
    game.wizard.request("unchanged", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut game.wizard).await else {
        panic!("snapshot")
    };
    assert_picture_payload(&snapshot.state, &game.initial.state);
    assert_eq!(snapshot.history, game.initial.history);
    game.finish().await;
}

#[tokio::test]
async fn an_accepted_wizard_command_reaches_every_client_and_its_retry_is_acknowledged() {
    let mut game = Game::start(true).await;
    let command = wizard_request(
        game.wizard.input_context(),
        &game.initial.branch,
        game.initial.state.revision,
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
            wizard_request(
                game.wizard.input_context(),
                &branch,
                game.initial.state.revision,
                teleport(),
            ),
        )
        .await;
    let mut teleport_revision = None;
    for client in [&mut game.wizard, &mut game.player, &mut game.spectator] {
        let ServerMessage::Snapshot { snapshot, .. } = receive(client).await else {
            panic!("teleport snapshot")
        };
        teleport_revision.get_or_insert(snapshot.state.revision);
    }
    assert!(matches!(
        receive(&mut game.wizard).await,
        ServerMessage::Ack { .. }
    ));
    let rewind = WizardOperation::Rewind { target: None };
    game.wizard
        .request(
            "rewind",
            wizard_request(
                game.wizard.input_context(),
                &branch,
                teleport_revision.unwrap(),
                rewind.clone(),
            ),
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
            anchor: Anchor::State {
                snapshot: game.initial.state.snapshot.clone(),
            },
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
                game.initial.state.revision,
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

#[tokio::test]
async fn named_creature_catalog_is_private_read_only_and_context_checked() {
    for enabled in [false, true] {
        let mut game = Game::start(enabled).await;
        let request = |client: &Client, revision, branch: BranchId| Request::Command {
            context: client.input_context(),
            branch,
            command: Command::Wizard {
                expected_revision: revision,
                operation: "creature list".into(),
            },
        };
        for client in [&mut game.player, &mut game.spectator] {
            client
                .request(
                    "catalog-denied",
                    request(client, 0, game.initial.branch.clone()),
                )
                .await;
            assert_error(client, ErrorCode::Unauthorized).await;
            client
                .request(
                    "spawn-denied",
                    Request::Command {
                        context: client.input_context(),
                        branch: game.initial.branch.clone(),
                        command: Command::Wizard {
                            expected_revision: 0,
                            operation: "creature spawn archetype:rat 1 2 1 0".into(),
                        },
                    },
                )
                .await;
            assert_error(client, ErrorCode::Unauthorized).await;
        }
        game.wizard
            .request(
                "catalog",
                request(
                    &game.wizard,
                    game.initial.state.revision,
                    game.initial.branch.clone(),
                ),
            )
            .await;
        if enabled {
            let ServerMessage::CreatureCatalog {
                request_id, page, ..
            } = receive(&mut game.wizard).await
            else {
                panic!("private catalog reply")
            };
            assert_eq!(request_id, "catalog");
            assert!(page.entries.is_empty());
            game.wizard
                .request(
                    "catalog-stale",
                    request(
                        &game.wizard,
                        game.initial.state.revision + 1,
                        game.initial.branch.clone(),
                    ),
                )
                .await;
            assert_error(&mut game.wizard, ErrorCode::StaleContext).await;
            game.wizard
                .request(
                    "catalog-branch",
                    request(&game.wizard, 0, BranchId("absent".into())),
                )
                .await;
            assert_error(&mut game.wizard, ErrorCode::WrongBranch).await;
        } else {
            assert_error(&mut game.wizard, ErrorCode::Unauthorized).await;
        }
        game.wizard
            .request("unchanged-catalog", Request::Snapshot)
            .await;
        let ServerMessage::Snapshot { snapshot, .. } = receive(&mut game.wizard).await else {
            panic!("snapshot")
        };
        assert_picture_payload(&snapshot.state, &game.initial.state);
        assert_eq!(snapshot.history, game.initial.history);
        game.finish().await;
    }
}

/// Map hydration is an attached read capability available to ordinary spectators.
#[tokio::test]
async fn explored_map_pages_round_trip_without_control_or_game_mutation() {
    let mut game = Game::start(false).await;
    game.spectator.request("map-base", Request::Snapshot).await;
    let ServerMessage::Snapshot {
        snapshot: before, ..
    } = receive(&mut game.spectator).await
    else {
        panic!("fresh spectator snapshot")
    };
    let mut query = MapQuery {
        context: before.context.clone(),
        branch: before.branch.clone(),
        base: ObservationBase {
            snapshot: before.state.snapshot.clone(),
            cursor: before.cursor,
            revision: before.state.revision,
        },
        window: MapWindow::normal()
            .checked_translate(before.state.observation.position)
            .unwrap(),
        cursor: None,
    };
    let mut cells = Vec::new();
    for index in 0..MAX_MAP_TRANSFER_PAGES {
        let id = format!("map-page-{index}");
        game.spectator
            .request(
                &id,
                Request::Map {
                    query: query.clone(),
                },
            )
            .await;
        let ServerMessage::MapPage {
            request_id,
            context,
            page,
        } = receive(&mut game.spectator).await
        else {
            panic!("correlated map page")
        };
        assert_eq!(request_id, id);
        assert_eq!(context.actor, ActorId(1));
        assert_eq!(page.base, query.base);
        assert!(page.complete && !page.limited);
        assert_eq!(page.start as usize, cells.len());
        cells.extend(page.cells);
        query.cursor = page.next;
        if query.cursor.is_none() {
            break;
        }
    }
    assert!(query.cursor.is_none() && !cells.is_empty());
    game.spectator
        .request("map-unchanged", Request::Snapshot)
        .await;
    let ServerMessage::Snapshot {
        snapshot: after, ..
    } = receive(&mut game.spectator).await
    else {
        panic!("unchanged spectator snapshot")
    };
    assert_picture_payload(&after.state, &before.state);
    assert_eq!(after.history, before.history);
    game.finish().await;
}
