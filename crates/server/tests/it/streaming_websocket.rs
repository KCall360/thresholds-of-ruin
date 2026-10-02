//! Region streaming over real WebSocket connections: clients attached to an
//! actor whose region leaves the loaded world. See docs/region-streaming.md.
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeSet;
use std::path::Path;

use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::*;
use tor_server::{scenario_package, serve, Account, Engine, Service, Simulation, Streaming};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn receive(client: &mut Client) -> Option<ServerMessage> {
    let frame = timeout(Duration::from_secs(5), client.next())
        .await
        .ok()??
        .ok()?;
    match frame {
        Message::Text(text) => serde_json::from_str(&text).ok(),
        _ => None,
    }
}

async fn send(client: &mut Client, id: &str, request: Request) {
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

async fn connect(address: &str, token: &str) -> Client {
    let (mut client, _) = connect_async(address).await.unwrap();
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: token.into(),
        frontend: "test".into(),
    };
    client
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(
        receive(&mut client).await,
        Some(ServerMessage::Welcome { .. })
    ));
    client
}

/// The player's view, kept current from snapshots and updates.
struct Player {
    client: Client,
    state: StateView,
    branch: BranchId,
    step: usize,
}

impl Player {
    /// Move once; returns the error the command got, if any.
    async fn walk(&mut self, direction: Direction) -> Option<(ErrorCode, String)> {
        self.step += 1;
        let id = format!("step-{}", self.step);
        let request = Request::Command {
            branch: self.branch.clone(),
            command: Command::Act {
                expected_revision: self.state.revision,
                action: Action::Move { direction },
            },
        };
        send(&mut self.client, &id, request).await;
        loop {
            match receive(&mut self.client)
                .await
                .expect("the player stays connected")
            {
                ServerMessage::Update { update, .. } => {
                    self.state = match update.body {
                        UpdateBody::Observation { state, .. } => *state,
                        UpdateBody::ObservationDelta { state, .. } => {
                            state.apply(&self.state).unwrap()
                        }
                        _ => continue,
                    };
                }
                ServerMessage::Ack { request_id, .. } if request_id == id => return None,
                ServerMessage::Error {
                    request_id: Some(request_id),
                    code,
                    message,
                } if request_id == id => return Some((code, message)),
                _ => {}
            }
        }
    }
}

/// Steps east from the start to the middle of hall 5, where hall 6 (and the
/// guard in it) is loaded but out of sight.
const TO_HALL_5: usize = 88;

#[tokio::test]
async fn a_spectator_whose_actor_leaves_the_loaded_world_is_detached_not_the_player() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
    let mut scenario = scenario_package::load(&root, 5, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}", listener.local_addr().unwrap());
    let service = Simulation::start(Service::new(Engine::memory(scenario).unwrap()));
    let accounts = vec![
        Account {
            role: AccessRole::Player,
            user: "alice".into(),
            token: "alice-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
        Account {
            role: AccessRole::Spectator,
            user: "bob".into(),
            token: "bob-test-token".into(),
            actors: BTreeSet::from([ActorId(1), ActorId(2)]),
        },
    ];
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(serve(listener, service, accounts, async {
        let _ = stopped.await;
    }));

    let mut client = connect(&address, "alice-test-token").await;
    send(&mut client, "attach", Request::Attach { actor: ActorId(1) }).await;
    let Some(ServerMessage::Snapshot { snapshot, .. }) = receive(&mut client).await else {
        panic!("expected a snapshot")
    };
    send(&mut client, "control", Request::AcquireControl).await;
    let mut player = Player {
        client,
        state: snapshot.state,
        branch: snapshot.branch,
        step: 0,
    };
    for _ in 0..TO_HALL_5 {
        assert_eq!(player.walk(Direction::East).await, None);
    }

    // The guard's hall is loaded now, so a spectator can watch the guard.
    let mut spectator = connect(&address, "bob-test-token").await;
    send(
        &mut spectator,
        "attach",
        Request::Attach { actor: ActorId(2) },
    )
    .await;
    assert!(matches!(
        receive(&mut spectator).await,
        Some(ServerMessage::Snapshot { .. })
    ));

    // Walking away unloads the guard's hall. The player's commands still
    // succeed; the spectator is told and detached.
    for _ in 0..20 {
        assert_eq!(player.walk(Direction::West).await, None);
    }
    let mut notified = false;
    while let Some(message) = receive(&mut spectator).await {
        if let ServerMessage::Error {
            request_id: None,
            code: ErrorCode::NotAttached,
            ..
        } = message
        {
            notified = true;
        }
    }
    assert!(
        notified,
        "the spectator must learn its actor left the loaded world"
    );
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}
