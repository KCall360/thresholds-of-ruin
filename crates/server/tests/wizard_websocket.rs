use futures_util::{SinkExt, StreamExt};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{oneshot, Mutex},
    time::timeout,
};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::*;
use tor_server::journal::{Command, Position, RegionView, WizardItem, WizardOperation};
use tor_server::{serve, Account, Engine, Scenario, Service};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;
async fn receive(client: &mut Client) -> ServerMessage {
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

#[tokio::test]
async fn raw_wizard_requests_enforce_roles_disabled_mode_retries_and_rewind_boundaries() {
    for enabled in [false, true] {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        if enabled {
            engine.enable_wizard().unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}", listener.local_addr().unwrap());
        // Same user deliberately tests receipt-retry bypass and private-note rules.
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
            Arc::new(Mutex::new(Service::new(engine))),
            accounts,
            async {
                let _ = stopped.await;
            },
        ));
        let (mut wizard, initial) = connect(&address, AccessRole::Wizard).await;
        let (mut player, _) = connect(&address, AccessRole::Player).await;
        let (mut spectator, _) = connect(&address, AccessRole::Spectator).await;
        assert_eq!(initial.state.wizard_game, enabled);
        let operations = [
            WizardOperation::PlaceDoor {
                position: Position {
                    region: 1,
                    x: 0,
                    y: 0,
                    z: 0,
                },
                open: false,
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
        ];
        for (index, operation) in operations.into_iter().enumerate() {
            let command = Request::Command {
                branch: initial.branch.clone(),
                command: Command::Wizard {
                    expected_revision: 0,
                    operation,
                }
                .into(),
            };
            for client in [&mut player, &mut spectator] {
                request(client, &format!("denied-{index}"), command.clone()).await;
                assert!(matches!(
                    receive(client).await,
                    ServerMessage::Error {
                        code: ErrorCode::Unauthorized,
                        ..
                    }
                ));
            }
            if !enabled {
                request(&mut wizard, &format!("disabled-{index}"), command).await;
                assert!(matches!(
                    receive(&mut wizard).await,
                    ServerMessage::Error {
                        code: ErrorCode::Unauthorized,
                        ..
                    }
                ));
            }
        }
        if enabled {
            let teleport = Request::Command {
                branch: initial.branch.clone(),
                command: Command::Wizard {
                    expected_revision: 0,
                    operation: WizardOperation::Teleport {
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
                }
                .into(),
            };
            request(&mut wizard, "accepted", teleport.clone()).await;
            for client in [&mut wizard, &mut player, &mut spectator] {
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
                receive(&mut wizard).await,
                ServerMessage::Ack { .. }
            ));
            for client in [&mut player, &mut spectator] {
                request(client, "accepted", teleport.clone()).await;
                assert!(matches!(
                    receive(client).await,
                    ServerMessage::Error {
                        code: ErrorCode::Unauthorized,
                        ..
                    }
                ));
            }
            request(&mut wizard, "accepted", teleport).await;
            assert!(matches!(
                receive(&mut wizard).await,
                ServerMessage::Ack { .. }
            ));
            request(
                &mut wizard,
                "rewind",
                Request::Command {
                    branch: initial.branch.clone(),
                    command: Command::Wizard {
                        expected_revision: 1,
                        operation: WizardOperation::Rewind { target: None },
                    }
                    .into(),
                },
            )
            .await;
            for client in [&mut wizard, &mut player, &mut spectator] {
                let ServerMessage::Snapshot { snapshot, .. } = receive(client).await else {
                    panic!("rewind snapshot")
                };
                assert!(snapshot
                    .state
                    .observation
                    .ground_items
                    .iter()
                    .any(|i| i.item.name == "copper token" && i.reachable));
                assert_ne!(snapshot.branch, initial.branch);
                assert!(snapshot.state.wizard_game);
            }
            assert!(matches!(
                receive(&mut wizard).await,
                ServerMessage::Ack { .. }
            ));
            request(
                &mut wizard,
                "stale",
                Request::Command {
                    branch: initial.branch,
                    command: Command::Wizard {
                        expected_revision: 0,
                        operation: WizardOperation::Rewind { target: None },
                    }
                    .into(),
                },
            )
            .await;
            assert!(matches!(
                receive(&mut wizard).await,
                ServerMessage::Error {
                    code: ErrorCode::WrongBranch,
                    ..
                }
            ));
        }
        let _ = stop.send(());
        server.await.unwrap().unwrap();
    }
}
