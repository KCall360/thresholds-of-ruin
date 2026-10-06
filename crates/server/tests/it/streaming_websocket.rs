//! Region streaming over real WebSocket connections: clients attached to an
//! actor whose region leaves the loaded world. See docs/region-streaming.md.
use futures_util::SinkExt;
use std::collections::BTreeSet;
use std::path::Path;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tor_protocol::*;
use tor_server::{scenario_package, serve, Account, Engine, Service, Simulation, Streaming};

use crate::wire_client::WireClient as Client;

async fn receive(client: &mut Client) -> Option<ServerMessage> {
    client.receive().await
}
async fn connect(address: &str, token: &str) -> Client {
    let (socket, _) = connect_async(address).await.unwrap();
    let mut client = Client::new(socket);
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
            context: self.client.input_context(),
            branch: self.branch.clone(),
            command: Command::Act {
                expected_revision: self.state.revision,
                action: Action::Move { direction },
            },
        };
        self.client.request(&id, request).await;
        let mut admitted = None;
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
                        UpdateBody::Intention { status }
                            if admitted.as_ref() == Some(&status.intention) =>
                        {
                            match status.phase {
                                IntentionPhase::Resolved => {
                                    self.client.resolution_readiness().await;
                                    return None;
                                }
                                IntentionPhase::Failed => {
                                    self.client.resolution_readiness().await;
                                    return Some((
                                        ErrorCode::InvalidAction,
                                        "Queued move failed".into(),
                                    ));
                                }
                                _ => continue,
                            }
                        }
                        _ => continue,
                    };
                }
                ServerMessage::Ack {
                    request_id,
                    receipt,
                    ..
                } if request_id == id => {
                    let RequestReceipt::Admitted { intention, .. } = receipt else {
                        panic!("gameplay admission receipt required")
                    };
                    admitted = Some(intention);
                }
                ServerMessage::Error {
                    request_id: Some(request_id),
                    code,
                    message,
                    ..
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
    client
        .request("attach", Request::Attach { actor: ActorId(1) })
        .await;
    let Some(ServerMessage::Snapshot { snapshot, .. }) = receive(&mut client).await else {
        panic!("expected a snapshot")
    };
    client.acquire_control("control").await;
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
    spectator
        .request("attach", Request::Attach { actor: ActorId(2) })
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
