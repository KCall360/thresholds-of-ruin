use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::*;
use tor_server::{serve, Account, Engine, Scenario, Service};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn receive(client: &mut Client) -> ServerMessage {
    let frame = timeout(Duration::from_secs(5), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(frame.to_text().unwrap()).unwrap()
}
async fn request(client: &mut Client, id: &str, request: Request) {
    let msg = ClientMessage::Request {
        request_id: id.into(),
        request,
    };
    client
        .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .unwrap();
}
async fn connect(address: &str, token: &str, frontend: &str) -> Client {
    let (mut client, _) = connect_async(address).await.unwrap();
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: token.into(),
        frontend: frontend.into(),
    };
    client
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Welcome { .. }
    ));
    client
}
async fn attach(client: &mut Client) -> Snapshot {
    request(client, "attach", Request::Attach { actor: ActorId(1) }).await;
    match receive(client).await {
        ServerMessage::Snapshot { snapshot, .. } => *snapshot,
        other => panic!("{other:?}"),
    }
}
async fn launch() -> (
    String,
    Arc<Mutex<Service>>,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}", listener.local_addr().unwrap());
    let service = Arc::new(Mutex::new(Service::new(
        Engine::memory(Scenario::two_room(42)).unwrap(),
    )));
    let accounts = vec![
        Account {
            user: "alice".into(),
            token: "alice-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
        Account {
            user: "bob".into(),
            token: "bob-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
    ];
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(serve(listener, service.clone(), accounts, async {
        let _ = stopped.await;
    }));
    (address, service, stop, server)
}

#[tokio::test]
async fn clients_receive_updates_without_polling_and_can_transfer_control() {
    let (address, _, stop, server) = launch().await;
    let mut text = connect(&address, "alice-test-token", "text").await;
    let initial = attach(&mut text).await;
    let mut ascii = connect(&address, "alice-test-token", "ascii").await;
    attach(&mut ascii).await;
    request(&mut text, "acquire", Request::AcquireControl).await;
    assert!(matches!(
        receive(&mut text).await,
        ServerMessage::Update { .. }
    ));
    assert!(matches!(
        receive(&mut text).await,
        ServerMessage::Ack { .. }
    ));
    assert!(matches!(
        receive(&mut ascii).await,
        ServerMessage::Update { .. }
    ));
    request(&mut ascii, "denied", Request::AcquireControl).await;
    assert!(matches!(
        receive(&mut ascii).await,
        ServerMessage::Error {
            code: ErrorCode::ControlTaken,
            ..
        }
    ));
    request(
        &mut text,
        "take",
        Request::Command {
            branch: initial.branch.clone(),
            command: Command::Act {
                expected_revision: 0,
                action: Action::Take {
                    item: initial.state.observation.ground_items[0].item.id,
                },
            },
        },
    )
    .await;
    for client in [&mut text, &mut ascii] {
        match receive(client).await {
            ServerMessage::Update { update } => match update.body {
                UpdateBody::Observation { state, event } => {
                    assert_eq!(state.observation.inventory.len(), 1);
                    assert_eq!(state.observation.tick, 50);
                    assert_eq!(state.revision, 1);
                    assert!(event.is_some());
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
    assert!(matches!(
        receive(&mut text).await,
        ServerMessage::Ack { .. }
    ));
    request(&mut text, "release", Request::ReleaseControl).await;
    receive(&mut text).await;
    receive(&mut text).await;
    receive(&mut ascii).await;
    request(&mut ascii, "acquire", Request::AcquireControl).await;
    receive(&mut text).await;
    receive(&mut ascii).await;
    receive(&mut ascii).await;
    request(
        &mut ascii,
        "stale",
        Request::Command {
            branch: initial.branch,
            command: Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        },
    )
    .await;
    assert!(matches!(
        receive(&mut ascii).await,
        ServerMessage::Error {
            code: ErrorCode::StaleRevision,
            ..
        }
    ));
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn private_annotations_stream_to_same_user_across_frontends_but_not_other_users() {
    let (address, service, stop, server) = launch().await;
    let mut text = connect(&address, "alice-test-token", "text").await;
    let initial = attach(&mut text).await;
    let mut ascii = connect(&address, "alice-test-token", "ascii").await;
    attach(&mut ascii).await;
    let mut bob = connect(&address, "bob-test-token", "ascii").await;
    attach(&mut bob).await;
    request(
        &mut text,
        "note",
        Request::Command {
            branch: initial.branch.clone(),
            command: Command::Annotate {
                anchor: Anchor::State { revision: 0 },
                text: "My private plan".into(),
                source: ClientSource::User,
                audience: Audience::Private,
                category: AnnotationCategory::Bookmark,
            },
        },
    )
    .await;
    for client in [&mut text, &mut ascii] {
        match receive(client).await {
            ServerMessage::Update { update } => {
                assert_eq!(update.cursor.sequence, 1);
                assert_eq!(update.cursor.tick, 0);
                assert!(matches!(update.body, UpdateBody::Annotation { .. }));
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(matches!(
        receive(&mut text).await,
        ServerMessage::Ack { .. }
    ));
    // Request a snapshot as a deterministic barrier: no private update may precede it.
    request(&mut bob, "barrier", Request::Snapshot).await;
    match receive(&mut bob).await {
        ServerMessage::Snapshot { snapshot, .. } => {
            assert_eq!(snapshot.cursor.sequence, 0);
            assert!(snapshot.history.entries.is_empty());
            assert_eq!(snapshot.state.revision, 0);
        }
        other => panic!("{other:?}"),
    }
    service
        .lock()
        .await
        .annotate_backend(
            ActorId(1),
            "simulation",
            Anchor::State { revision: 0 },
            AnnotationCategory::Explanation,
            "A rare explanation.",
        )
        .unwrap();
    for client in [&mut text, &mut ascii, &mut bob] {
        match receive(client).await {
            ServerMessage::Update { update } => match update.body {
                UpdateBody::Annotation { entry } => assert_eq!(
                    entry.author,
                    Author::Backend {
                        component: "simulation".into()
                    }
                ),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn authentication_version_and_actor_permissions_are_checked_before_disclosure() {
    let (address, _, stop, server) = launch().await;
    for (protocol, token, expected) in [
        (PROTOCOL_VERSION, "wrong", ErrorCode::Unauthorized),
        (999, "alice-test-token", ErrorCode::VersionMismatch),
    ] {
        let (mut socket, _) = connect_async(&address).await.unwrap();
        let hello = ClientMessage::Hello {
            protocol,
            token: token.into(),
            frontend: "text".into(),
        };
        socket
            .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
            .await
            .unwrap();
        assert!(
            matches!(receive(&mut socket).await, ServerMessage::Error { code, .. } if code == expected)
        );
    }
    let mut client = connect(&address, "alice-test-token", "text").await;
    request(
        &mut client,
        "unauthorized",
        Request::Attach {
            actor: ActorId(999),
        },
    )
    .await;
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Error {
            code: ErrorCode::Unauthorized,
            ..
        }
    ));
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn reconnect_recovers_history_and_duplicate_receipts_without_rebroadcast() {
    let (address, _, stop, server) = launch().await;
    let mut first = connect(&address, "alice-test-token", "text").await;
    let snapshot = attach(&mut first).await;
    let command = Request::Command {
        branch: snapshot.branch,
        command: Command::Annotate {
            anchor: Anchor::State { revision: 0 },
            text: "Keep this".into(),
            source: ClientSource::Frontend,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
        },
    };
    request(&mut first, "same-request", command.clone()).await;
    receive(&mut first).await;
    let original = receive(&mut first).await;
    first.close(None).await.unwrap();
    let mut second = connect(&address, "alice-test-token", "ascii").await;
    let resumed = attach(&mut second).await;
    assert_eq!(resumed.cursor.sequence, 0);
    assert_eq!(resumed.history.entries.len(), 1);
    assert_eq!(
        resumed.history.entries[0].author,
        Author::Frontend {
            user: "alice".into(),
            component: "text".into()
        }
    );
    request(&mut second, "same-request", command).await;
    assert_eq!(receive(&mut second).await, original);
    request(&mut second, "barrier", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut second).await else {
        panic!("No duplicate update expected")
    };
    assert_eq!(snapshot.cursor.sequence, 0);
    assert_eq!(snapshot.history.entries.len(), 1);
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}
