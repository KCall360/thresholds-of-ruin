use futures_util::SinkExt;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tor_protocol::*;
use tor_server::{serve, Account, Engine, Scenario, Service, Simulation, SimulationHandle};

use crate::wire_client::WireClient as Client;

/// Gameplay-focused tests skip permission metadata. Model/ordering tests use
/// `receive_metadata` and apply every ordered update.
async fn receive(client: &mut Client) -> ServerMessage {
    loop {
        match receive_metadata(client).await {
            ServerMessage::Update { update }
                if matches!(update.body, UpdateBody::Readiness { .. }) =>
            {
                continue
            }
            message => return message,
        }
    }
}

/// The next message past unordered waiting signals, retaining all stream updates.
async fn receive_metadata(client: &mut Client) -> ServerMessage {
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
async fn connect(address: &str, token: &str, frontend: &str) -> Client {
    let (socket, _) = connect_async(address).await.unwrap();
    let mut client = Client::new(socket);
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: token.into(),
        frontend: frontend.into(),
    };
    client
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    let ServerMessage::Welcome { role, .. } = receive(&mut client).await else {
        panic!("Expected welcome")
    };
    let expected = if matches!(token, "spectator-test-token" | "bob-test-token") {
        AccessRole::Spectator
    } else {
        AccessRole::Player
    };
    assert_eq!(role, expected);
    client
}
/// A pushed observation as a full state, expanding a delta against `base`.
fn observation(body: UpdateBody, base: &StateView) -> (StateView, Option<Box<HistoryEntry>>) {
    match body {
        UpdateBody::Observation { state, event } => (Arc::unwrap_or_clone(state), event),
        UpdateBody::ObservationDelta { state, event, .. } => (state.apply(base).unwrap(), event),
        other => panic!("{other:?}"),
    }
}

async fn attach(client: &mut Client) -> Snapshot {
    client
        .request("attach", Request::Attach { actor: ActorId(1) })
        .await;
    match receive(client).await {
        ServerMessage::Snapshot { snapshot, .. } => *snapshot,
        other => panic!("{other:?}"),
    }
}

/// Check the admission/effect lifecycle on both ordered streams.
async fn completed_action(player: &mut Client, observer: &mut Client, id: &str) -> StreamUpdate {
    let ServerMessage::Update { update: queued } = receive(player).await else {
        panic!("queued lifecycle required")
    };
    let UpdateBody::Intention { status } = &queued.body else {
        panic!("queued lifecycle required")
    };
    assert_eq!(status.phase, IntentionPhase::Queued);
    let ServerMessage::Update { update: watched } = receive(observer).await else {
        panic!("observer queued lifecycle required")
    };
    assert_ne!(watched.context.stream, queued.context.stream);
    assert_eq!(watched.actor, queued.actor);
    assert_eq!(watched.branch, queued.branch);
    assert_eq!(watched.cursor.tick, queued.cursor.tick);
    assert_eq!(watched.body, queued.body);
    let ServerMessage::Ack {
        request_id,
        receipt,
        ..
    } = receive(player).await
    else {
        panic!("admission receipt required before effect")
    };
    assert_eq!(request_id, id);
    assert!(
        matches!(receipt, RequestReceipt::Admitted { intention, entry_id, phase: IntentionPhase::Queued, .. }
        if intention == status.intention && entry_id == status.entry_id)
    );
    let ServerMessage::Update { update: effect } = receive(player).await else {
        panic!("simulation effect required")
    };
    assert!(matches!(
        effect.body,
        UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
    ));
    let ServerMessage::Update { update: watched } = receive(observer).await else {
        panic!("observer effect required")
    };
    assert_ne!(watched.context.stream, effect.context.stream);
    assert_eq!(watched.actor, effect.actor);
    assert_eq!(watched.branch, effect.branch);
    // Permission updates are controller-specific, so stream sequences and delta
    // base cursors differ while disclosed observations and events remain equal.
    assert_eq!(watched.cursor.tick, effect.cursor.tick);
    match (&watched.body, &effect.body) {
        (
            UpdateBody::ObservationDelta {
                base: watched_base,
                state: watched_state,
                event: watched_event,
            },
            UpdateBody::ObservationDelta { base, state, event },
        ) => {
            assert_eq!(watched_base.revision, base.revision);
            assert_eq!(watched_base.cursor.tick, base.cursor.tick);
            assert_eq!(watched_state, state);
            assert_eq!(watched_event, event);
        }
        _ => assert_eq!(watched.body, effect.body),
    }
    for (client, effect_cursor) in [
        (&mut *player, effect.cursor),
        (&mut *observer, watched.cursor),
    ] {
        let ServerMessage::Update { update } = receive(client).await else {
            panic!("resolution required")
        };
        let UpdateBody::Intention { status: resolved } = update.body else {
            panic!("resolution required")
        };
        assert_eq!(resolved.phase, IntentionPhase::Resolved);
        assert_eq!(resolved.intention, status.intention);
        assert_eq!(resolved.entry_id, status.entry_id);
        assert_eq!(update.cursor.sequence, effect_cursor.sequence + 1);
    }
    let ServerMessage::Update { update } = receive_metadata(player).await else {
        panic!("post-resolution readiness required")
    };
    assert!(matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission));
    *effect
}
async fn launch() -> (
    String,
    SimulationHandle,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<Service>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}", listener.local_addr().unwrap());
    let simulation = Simulation::start(Service::new(
        Engine::memory(Scenario::two_room(42)).unwrap(),
    ));
    let service = simulation.handle();
    let mut accounts = vec![
        Account {
            role: tor_protocol::AccessRole::Player,
            user: "alice".into(),
            token: "alice-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
        Account {
            role: tor_protocol::AccessRole::Spectator,
            user: "bob".into(),
            token: "bob-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
    ];
    accounts.push(Account {
        role: AccessRole::Spectator,
        user: "alice".into(),
        token: "spectator-test-token".into(),
        actors: BTreeSet::from([ActorId(1)]),
    });
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(serve(listener, simulation, accounts, async {
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
    text.request("acquire", Request::AcquireControl).await;
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
    ascii.request("denied", Request::AcquireControl).await;
    assert!(matches!(
        receive(&mut ascii).await,
        ServerMessage::Error {
            code: ErrorCode::ControlTaken,
            ..
        }
    ));
    text.request(
        "take",
        Request::Command {
            context: text.input_context(),
            branch: initial.branch.clone(),
            command: Command::Act {
                expected_revision: 0,
                action: Action::Take {
                    item: initial.state.observation.ground_items[0].item.id,
                    quantity: None,
                },
            },
        },
    )
    .await;
    let update = completed_action(&mut text, &mut ascii, "take").await;
    let (state, event) = observation(update.body, &initial.state);
    assert_eq!(state.observation.inventory.len(), 1);
    assert_eq!(state.observation.tick, 50);
    assert_eq!(state.revision, 1);
    assert!(event.is_some());
    text.request("release", Request::ReleaseControl).await;
    receive(&mut text).await;
    receive(&mut text).await;
    receive(&mut ascii).await;
    ascii.request("acquire", Request::AcquireControl).await;
    receive(&mut text).await;
    receive(&mut ascii).await;
    receive(&mut ascii).await;
    ascii
        .request(
            "stale",
            Request::Command {
                context: ascii.input_context(),
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
    text.request(
        "note",
        Request::Command {
            context: text.input_context(),
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
    bob.request("barrier", Request::Snapshot).await;
    match receive(&mut bob).await {
        ServerMessage::Snapshot { snapshot, .. } => {
            assert_eq!(snapshot.cursor.sequence, 0);
            assert!(snapshot.history.entries.is_empty());
            assert_eq!(snapshot.state.revision, 0);
        }
        other => panic!("{other:?}"),
    }
    service
        .with(|service| {
            service.annotate_backend(
                ActorId(1),
                "simulation",
                Anchor::State { revision: 0 },
                AnnotationCategory::Explanation,
                "A rare explanation.",
            )
        })
        .await
        .expect("running simulation")
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
        (1, "alice-test-token", ErrorCode::VersionMismatch),
    ] {
        let (socket, _) = connect_async(&address).await.unwrap();
        let mut socket = Client::new(socket);
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
            matches!(receive(&mut socket).await, ServerMessage::Error { scope: ErrorScope::Transport {}, code, .. } if code == expected)
        );
    }
    let mut client = connect(&address, "alice-test-token", "text").await;
    client
        .request(
            "unauthorized",
            Request::Attach {
                actor: ActorId(999),
            },
        )
        .await;
    assert!(matches!(
        receive(&mut client).await,
        ServerMessage::Error {
            scope: ErrorScope::Unattached {},
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
        context: first.input_context(),
        branch: snapshot.branch,
        command: Command::Annotate {
            anchor: Anchor::State { revision: 0 },
            text: "Keep this".into(),
            source: ClientSource::Frontend,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
        },
    };
    first.request("same-request", command.clone()).await;
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
    second.request("same-request", command).await;
    let ServerMessage::Ack {
        context: original_context,
        receipt: original_receipt,
        ..
    } = original
    else {
        panic!("original receipt required")
    };
    let ServerMessage::Ack {
        context,
        request_id,
        receipt,
    } = receive(&mut second).await
    else {
        panic!("retried receipt required")
    };
    assert_eq!(request_id, "same-request");
    assert_eq!(receipt, original_receipt);
    assert_eq!(context, resumed.reply_context());
    assert_ne!(
        context.input.stream.stream,
        original_context.input.stream.stream
    );
    second.request("barrier", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut second).await else {
        panic!("No duplicate update expected")
    };
    assert_eq!(snapshot.cursor.sequence, 0);
    assert_eq!(snapshot.history.entries.len(), 1);
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn spectator_authority_denies_all_mutations_even_same_user_receipt_retries() {
    let (address, _, stop, server) = launch().await;
    let mut player = connect(&address, "alice-test-token", "text").await;
    let initial = attach(&mut player).await;
    let note = Request::Command {
        context: player.input_context(),
        branch: initial.branch.clone(),
        command: Command::Annotate {
            anchor: Anchor::State { revision: 0 },
            text: "Private plan".into(),
            source: ClientSource::User,
            audience: Audience::Private,
            category: AnnotationCategory::Note,
        },
    };
    player.request("original", note.clone()).await;
    receive(&mut player).await;
    receive(&mut player).await;
    // Spoofing the frontend name cannot confer player authority.
    let mut spectator = connect(&address, "spectator-test-token", "text").await;
    spectator
        .request("before-attach", Request::AcquireControl)
        .await;
    assert!(matches!(
        receive(&mut spectator).await,
        ServerMessage::Error {
            code: ErrorCode::Unauthorized,
            ..
        }
    ));
    spectator
        .request(
            "wrong-actor",
            Request::Attach {
                actor: ActorId(999),
            },
        )
        .await;
    assert!(matches!(
        receive(&mut spectator).await,
        ServerMessage::Error {
            code: ErrorCode::Unauthorized,
            ..
        }
    ));
    let before = attach(&mut spectator).await;
    // Identity and annotation visibility are independent of write authority.
    assert_eq!(before.history.entries.len(), 1);
    for (id, mutation) in [
        ("acquire", Request::AcquireControl),
        ("release", Request::ReleaseControl),
        (
            "door",
            Request::Command {
                context: spectator.input_context(),
                branch: initial.branch.clone(),
                command: Command::Act {
                    expected_revision: 0,
                    action: Action::SetDoor {
                        door: tor_protocol::DoorTarget::from_digest([0; 32]),
                        open: false,
                    },
                },
            },
        ),
        ("original", note.clone()),
        ("new-note", note),
        (
            "act",
            Request::Command {
                context: spectator.input_context(),
                branch: initial.branch,
                command: Command::Act {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            },
        ),
    ] {
        spectator.request(id, mutation).await;
        assert!(matches!(
            receive(&mut spectator).await,
            ServerMessage::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
    }
    spectator.request("snapshot", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut spectator).await else {
        panic!()
    };
    assert_eq!(snapshot.context.stream, before.context.stream);
    assert_eq!(snapshot.context.epoch, before.context.epoch + 1);
    let mut expected = before.clone();
    expected.context = snapshot.context.clone();
    assert_eq!(*snapshot, expected);
    spectator
        .request(
            "history",
            Request::History {
                before: None,
                limit: 1,
            },
        )
        .await;
    let ServerMessage::History { page, .. } = receive(&mut spectator).await else {
        panic!()
    };
    assert_eq!(page, before.history);
    // Denied acquisition/release must not change the player's ability to control.
    player.request("control", Request::AcquireControl).await;
    receive(&mut player).await;
    assert!(matches!(
        receive(&mut player).await,
        ServerMessage::Ack { .. }
    ));
    let ServerMessage::Update { update } = receive(&mut spectator).await else {
        panic!()
    };
    assert_eq!(update.body, UpdateBody::Control { has_control: false });
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn reconnect_retries_a_taken_target_before_old_context_and_fresh_resolution() {
    let (address, _, stop, server) = launch().await;
    let mut player = connect(&address, "alice-test-token", "text").await;
    let initial = attach(&mut player).await;
    player.acquire_control("control").await;
    let item = initial.state.observation.ground_items[0].item.id;
    let original = Request::Command {
        context: player.input_context(),
        branch: initial.branch.clone(),
        command: Command::Act {
            expected_revision: initial.state.revision,
            action: Action::Take {
                item,
                quantity: None,
            },
        },
    };
    player.request("original-take", original.clone()).await;
    let mut state = Arc::unwrap_or_clone(initial.state.clone());
    let mut admitted = None;
    loop {
        match receive(&mut player).await {
            ServerMessage::Ack {
                request_id,
                receipt,
                ..
            } => {
                assert_eq!(request_id, "original-take");
                assert!(matches!(receipt, RequestReceipt::Admitted { .. }));
                assert!(admitted.replace(receipt).is_none());
            }
            ServerMessage::Update { update } => match update.body {
                body @ (UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }) => {
                    assert!(admitted.is_some(), "admission precedes simulation effects");
                    state = observation(body, &state).0;
                }
                UpdateBody::Intention { status } => {
                    if status.phase == IntentionPhase::Resolved {
                        break;
                    }
                    assert_eq!(status.phase, IntentionPhase::Queued);
                }
                other => panic!("unexpected update: {other:?}"),
            },
            other => panic!("unexpected message: {other:?}"),
        }
    }
    assert!(state
        .observation
        .ground_items
        .iter()
        .all(|ground| ground.item.id != item));
    assert!(state
        .observation
        .inventory
        .iter()
        .any(|held| held.id == item));
    player.close(None).await.unwrap();
    let mut resumed = connect(&address, "alice-test-token", "ascii").await;
    let snapshot = attach(&mut resumed).await;
    assert_eq!(*snapshot.state, state);
    assert_ne!(snapshot.context.stream, initial.context.stream);
    assert!(!snapshot.has_control);
    resumed.request("original-take", original).await;
    let ServerMessage::Ack {
        receipt, context, ..
    } = receive(&mut resumed).await
    else {
        panic!("retry must return the original receipt without requiring control or resolving the item")
    };
    let RequestReceipt::Admitted { phase, .. } = admitted.as_mut().unwrap() else {
        panic!("original gameplay receipt required")
    };
    assert_eq!(*phase, IntentionPhase::Queued);
    *phase = IntentionPhase::Resolved;
    assert_eq!(Some(receipt), admitted);
    assert_eq!(context, snapshot.reply_context());

    resumed.acquire_control("resume-control").await;
    resumed
        .request(
            "fresh-take",
            Request::Command {
                context: resumed.input_context(),
                branch: snapshot.branch.clone(),
                command: Command::Act {
                    expected_revision: state.revision,
                    action: Action::Take {
                        item,
                        quantity: None,
                    },
                },
            },
        )
        .await;
    assert!(matches!(
        receive(&mut resumed).await,
        ServerMessage::Error {
            code: ErrorCode::InvalidAction,
            ..
        }
    ));
    resumed.request("unchanged", Request::Snapshot).await;
    let ServerMessage::Snapshot {
        snapshot: after, ..
    } = receive(&mut resumed).await
    else {
        panic!("fresh rejection must not publish an effect")
    };
    assert_eq!(*after.state, state);
    assert_eq!(after.history, snapshot.history);
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn spectators_receive_each_accepted_action_once_with_identical_disclosed_state() {
    let (address, _, stop, server) = launch().await;
    let mut player = connect(&address, "alice-test-token", "text").await;
    let initial = attach(&mut player).await;
    let mut spectator = connect(&address, "bob-test-token", "ascii").await;
    let seen = attach(&mut spectator).await;
    assert_eq!(seen.state, initial.state);
    assert!(!serde_json::to_string(&seen)
        .unwrap()
        .contains("known_places"));
    player.request("control", Request::AcquireControl).await;
    receive(&mut player).await;
    receive(&mut player).await;
    receive(&mut spectator).await;
    let actions = [
        Action::Take {
            item: initial.state.observation.ground_items[0].item.id,
            quantity: None,
        },
        Action::Wait,
        Action::Move {
            direction: Direction::East,
        },
    ];
    let mut entries = vec![];
    let mut state = Arc::unwrap_or_clone(initial.state.clone());
    for (revision, action) in actions.into_iter().enumerate() {
        let id = format!("action-{revision}");
        let command = Request::Command {
            context: player.input_context(),
            branch: initial.branch.clone(),
            command: Command::Act {
                expected_revision: revision as u64,
                action: action.clone(),
            },
        };
        player.request(&id, command.clone()).await;
        let watched = completed_action(&mut player, &mut spectator, &id).await;
        let event;
        (state, event) = observation(watched.body, &state);
        assert_eq!(state.revision, revision as u64 + 1);
        let entry = *event.unwrap();
        assert!(
            matches!(&entry.content, HistoryContent::Action { action: recorded, .. } if recorded == &action)
        );
        entries.push(entry);
        player.request(&id, command).await;
        assert!(matches!(
            receive(&mut player).await,
            ServerMessage::Ack { .. }
        ));
    }
    spectator
        .request("release-other", Request::ReleaseControl)
        .await;
    assert!(matches!(
        receive(&mut spectator).await,
        ServerMessage::Error {
            code: ErrorCode::Unauthorized,
            ..
        }
    ));
    player
        .request(
            "invalid",
            Request::Command {
                context: player.input_context(),
                branch: initial.branch.clone(),
                command: Command::Act {
                    expected_revision: 3,
                    action: Action::Move {
                        direction: Direction::Up,
                    },
                },
            },
        )
        .await;
    assert!(matches!(
        receive(&mut player).await,
        ServerMessage::Error {
            code: ErrorCode::InvalidAction,
            ..
        }
    ));
    spectator.request("barrier", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive(&mut spectator).await else {
        panic!("No duplicate or rejected action updates")
    };
    assert_eq!(snapshot.cursor.sequence, 10);
    assert_eq!(snapshot.state.revision, 3);
    assert_eq!(snapshot.history.entries, entries);
    spectator.close(None).await.unwrap();
    let mut resumed = connect(&address, "bob-test-token", "text").await;
    let snapshot = attach(&mut resumed).await;
    assert_eq!(snapshot.cursor.sequence, 0);
    assert_eq!(snapshot.history.entries, entries);
    resumed
        .request(
            "older",
            Request::History {
                before: Some(entries[2].id.clone()),
                limit: 1,
            },
        )
        .await;
    let ServerMessage::History { page, .. } = receive(&mut resumed).await else {
        panic!()
    };
    assert_eq!(page.entries, vec![entries[1].clone()]);
    assert!(page.older_before.is_some());
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

/// The next `waiting` signal, past anything else.
async fn waiting(client: &mut Client) -> Waiting {
    loop {
        if let ServerMessage::Waiting { on } = receive_any(client).await {
            return on;
        }
    }
}

#[tokio::test]
async fn each_client_is_told_whose_move_it_is_when_play_stops() {
    let (address, _, stop, server) = launch().await;
    let mut player = connect(&address, "alice-test-token", "text").await;
    attach(&mut player).await;
    // Nobody controls the character yet.
    assert_eq!(waiting(&mut player).await, Waiting::Unclaimed);
    let mut watcher = connect(&address, "bob-test-token", "text").await;
    attach(&mut watcher).await;
    assert_eq!(waiting(&mut watcher).await, Waiting::Unclaimed);
    player.request("acquire", Request::AcquireControl).await;
    assert_eq!(waiting(&mut player).await, Waiting::You);
    assert_eq!(waiting(&mut watcher).await, Waiting::Others);
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn attachment_contexts_are_distinct_and_snapshot_resets_do_not_change_game_state() {
    let (address, _, stop, server) = launch().await;
    let mut first = connect(&address, "alice-test-token", "headless").await;
    let initial = attach(&mut first).await;
    let mut second = connect(&address, "spectator-test-token", "headless").await;
    let watched = attach(&mut second).await;
    assert_ne!(initial.context.stream, watched.context.stream);
    assert_eq!(initial.context.epoch, 1);
    assert_eq!(watched.context.epoch, 1);
    let mut model = tor_client_common::ClientState::from_snapshot(initial.clone()).unwrap();
    first.request("reset", Request::Snapshot).await;
    let ServerMessage::Snapshot { snapshot, .. } = receive_metadata(&mut first).await else {
        panic!("reset snapshot required")
    };
    assert_eq!(snapshot.context.stream, initial.context.stream);
    assert_eq!(snapshot.context.epoch, 2);
    assert_eq!(snapshot.state, initial.state);
    assert_eq!(snapshot.cursor, initial.cursor);
    assert_eq!(snapshot.history, initial.history);
    model.replace_snapshot(*snapshot).unwrap();
    first.request("acquire", Request::AcquireControl).await;
    let ServerMessage::Update { update } = receive_metadata(&mut first).await else {
        panic!("control update required")
    };
    assert_eq!(update.context, *model.context());
    model.apply(*update).unwrap();
    assert!(model.has_control());
    let ServerMessage::Update { update } = receive_metadata(&mut first).await else {
        panic!("readiness required before acknowledgement")
    };
    assert!(matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission));
    model.apply(*update).unwrap();
    assert!(matches!(
        receive_metadata(&mut first).await,
        ServerMessage::Ack { .. }
    ));
    let ServerMessage::Update { update } = receive_metadata(&mut second).await else {
        panic!("observer control update required")
    };
    assert_eq!(update.context, watched.context);
    assert!(matches!(
        update.body,
        UpdateBody::Control { has_control: false }
    ));
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn deltas_follow_each_attachments_last_observation_across_control_and_reset() {
    let (address, _, stop, server) = launch().await;
    let mut player = connect(&address, "alice-test-token", "headless").await;
    let mut player_state =
        tor_client_common::ClientState::from_snapshot(attach(&mut player).await).unwrap();
    let mut observer = connect(&address, "spectator-test-token", "headless").await;
    let mut observer_state =
        tor_client_common::ClientState::from_snapshot(attach(&mut observer).await).unwrap();
    player.request("control", Request::AcquireControl).await;
    let ServerMessage::Update { update } = receive_metadata(&mut player).await else {
        panic!("control required")
    };
    player_state.apply(*update).unwrap();
    let ServerMessage::Update { update } = receive_metadata(&mut player).await else {
        panic!("readiness required before acknowledgement")
    };
    assert!(matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission));
    player_state.apply(*update).unwrap();
    assert!(matches!(
        receive_metadata(&mut player).await,
        ServerMessage::Ack { .. }
    ));
    let ServerMessage::Update { update } = receive_metadata(&mut observer).await else {
        panic!("control required")
    };
    observer_state.apply(*update).unwrap();
    for round in 0..2 {
        player
            .request(
                &format!("wait-{round}"),
                Request::Command {
                    context: player_state.input_context(),
                    branch: player_state.branch().clone(),
                    command: Command::Act {
                        expected_revision: player_state.state().revision,
                        action: Action::Wait,
                    },
                },
            )
            .await;
        for model_and_client in [
            (&mut player_state, &mut player),
            (&mut observer_state, &mut observer),
        ] {
            let (model, client) = model_and_client;
            let mut saw_delta = false;
            loop {
                match receive_metadata(client).await {
                    ServerMessage::Ack { .. } => continue,
                    ServerMessage::Update { update } => {
                        if let UpdateBody::ObservationDelta { base, .. } = &update.body {
                            assert_eq!(*base, model.observation_base());
                            saw_delta = true;
                        }
                        let resolved = matches!(&update.body, UpdateBody::Intention { status } if status.phase == IntentionPhase::Resolved);
                        model.apply(*update).unwrap();
                        if resolved {
                            break;
                        }
                    }
                    message => panic!("unexpected message: {message:?}"),
                }
            }
            assert!(saw_delta);
        }
        let ServerMessage::Update { update } = receive_metadata(&mut player).await else {
            panic!("readiness follows resolved lifecycle")
        };
        assert!(matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission));
        player_state.apply(*update).unwrap();
        assert_eq!(player_state.state(), observer_state.state());
        if round == 0 {
            player.request("reset", Request::Snapshot).await;
            let ServerMessage::Snapshot { snapshot, .. } = receive_metadata(&mut player).await
            else {
                panic!("reset required")
            };
            player_state.replace_snapshot(*snapshot).unwrap();
            assert_ne!(
                player_state.observation_base(),
                observer_state.observation_base()
            );
        }
    }
    player.close(None).await.unwrap();
    observer.close(None).await.unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}
