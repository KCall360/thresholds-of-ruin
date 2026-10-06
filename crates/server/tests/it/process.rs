use crate::wire_client::WireClient;
use futures_util::{SinkExt, StreamExt};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tor_protocol::*;

struct ChildGuard(Child);

#[test]
fn performance_region_cli_documents_bounds_and_rejects_invalid_counts() {
    let help = ProcessCommand::new(env!("CARGO_BIN_EXE_tor-server"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(text.contains("--regions") && text.contains("1..=256") && text.contains("--actors"));
    for regions in ["0", "257"] {
        let output = ProcessCommand::new(env!("CARGO_BIN_EXE_tor-server"))
            .args(["--regions", regions])
            .env_remove("TOR_WIZARD_TOKEN")
            .env("TOR_SERVER_TOKEN", "performance-cli-test-token")
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn launch(path: &Path) -> (ChildGuard, String) {
    let mut child = ChildGuard(
        ProcessCommand::new(env!("CARGO_BIN_EXE_tor-server"))
            .args(["--listen", "127.0.0.1:0", "--seed", "42", "--save"])
            .arg(path)
            .arg("--scenario")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room"))
            .env_remove("TOR_SPECTATOR_TOKEN")
            .env("TOR_SERVER_TOKEN", "process-test-token-not-a-real-secret")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = tx.send(result);
    });
    let ready = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("server ready deadline")
        .unwrap();
    assert!(!ready.contains("process-test-token"));
    let ready: serde_json::Value = serde_json::from_str(&ready).unwrap();
    let address = format!("ws://{}", ready["address"].as_str().unwrap());
    (child, address)
}

#[tokio::test]
async fn actual_server_connection_remains_usable_after_local_request_byte_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let (_child, address) = launch(&dir.path().join("game.json"));
    let address = address.strip_prefix("ws://").unwrap().parse().unwrap();
    let mut client = tor_client_common::Connection::connect(
        address,
        "process-test-token-not-a-real-secret".into(),
        ActorId(1),
        "bounded-wire-test",
    )
    .await
    .unwrap();
    let before = client.state.state().clone();
    let request = client.state.command_request(Command::Annotate {
        anchor: Anchor::State {
            revision: before.revision,
        },
        text: "\\".repeat(MAX_REQUEST_BYTES),
        source: ClientSource::User,
        audience: Audience::Actor,
        category: AnnotationCategory::Note,
    });
    assert!(client.request(request).await.is_err());
    assert!(client.is_synchronized());
    let id = client.request(Request::Save).await.unwrap();
    loop {
        match client.next().await.unwrap() {
            ServerMessage::Ack { request_id, .. } if request_id == id => break,
            ServerMessage::Error { code, message, .. } => {
                panic!("connection must remain usable: {code:?}: {message}")
            }
            _ => {}
        }
    }
    let id = client.request(Request::Snapshot).await.unwrap();
    loop {
        if let ServerMessage::Snapshot { request_id, .. } = client.next().await.unwrap() {
            if request_id == id {
                break;
            }
        }
    }
    assert_eq!(client.state.state(), &before);
    assert!(client.state.history().is_empty());
    client.close().await.unwrap();
}

#[tokio::test]
async fn actual_server_rejects_new_work_while_recovered_queue_disables_admission() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.json");
    {
        let mut engine =
            tor_server::Engine::open(&path, tor_server::Scenario::two_room(42)).unwrap();
        engine
            .command(
                "p",
                "test",
                ActorId(1),
                "original",
                &engine.branch().clone(),
                tor_server::journal::Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            )
            .unwrap();
        engine.flush().unwrap();
    }
    let (_child, address) = launch(&path);
    let (socket, _) = connect_async(&address).await.unwrap();
    let mut client = WireClient::new(socket);
    client
        .send(Message::Text(
            serde_json::to_string(&ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                token: "process-test-token-not-a-real-secret".into(),
                frontend: "raw-admission-test".into(),
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        client.receive().await,
        Some(ServerMessage::Welcome { .. })
    ));
    client
        .request("attach", Request::Attach { actor: ActorId(1) })
        .await;
    let before = loop {
        if let ServerMessage::Snapshot { snapshot, .. } = client.receive().await.unwrap() {
            break snapshot;
        }
    };
    client.acquire_control("control").await;
    client.request("snapshot", Request::Snapshot).await;
    let controlled = loop {
        if let ServerMessage::Snapshot { snapshot, .. } = client.receive().await.unwrap() {
            break snapshot;
        }
    };
    assert!(!controlled.readiness.admission);
    assert_eq!(controlled.intentions.len(), 1);
    assert_eq!(controlled.intentions[0].phase, IntentionPhase::Suspended);
    client
        .request(
            "disabled",
            Request::Command {
                context: client.input_context(),
                branch: controlled.branch.clone(),
                command: Command::Act {
                    expected_revision: controlled.state.revision,
                    action: Action::Wait,
                },
            },
        )
        .await;
    loop {
        match client.receive().await.unwrap() {
            ServerMessage::Error {
                request_id: Some(id),
                code,
                ..
            } if id == "disabled" => {
                assert_eq!(code, ErrorCode::ActorBusy);
                break;
            }
            ServerMessage::Ack { request_id, .. } if request_id == "disabled" => {
                panic!("disabled action accepted")
            }
            _ => {}
        }
    }
    client.request("after", Request::Snapshot).await;
    let after = loop {
        if let ServerMessage::Snapshot { snapshot, .. } = client.receive().await.unwrap() {
            break snapshot;
        }
    };
    assert_eq!(after.state, before.state);
    assert_eq!(after.intentions, controlled.intentions);
    assert_eq!(after.cursor.tick, controlled.cursor.tick);
    assert_eq!(after.branch, controlled.branch);
}

#[tokio::test]
async fn actual_server_process_persists_an_action_and_annotation_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("game.json");
    let (child, address) = launch(&path);
    let (socket, _) = timeout(Duration::from_secs(5), connect_async(&address))
        .await
        .unwrap()
        .unwrap();
    let mut socket = WireClient::new(socket);
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: "process-test-token-not-a-real-secret".into(),
        frontend: "test-text".into(),
    };
    socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let attach = ClientMessage::Request {
        request_id: "attach".into(),
        request: Request::Attach { actor: ActorId(1) },
    };
    socket
        .send(Message::Text(
            serde_json::to_string(&attach).unwrap().into(),
        ))
        .await
        .unwrap();
    let message = next_message(&mut socket).await;
    let ServerMessage::Snapshot { snapshot, .. } = message else {
        panic!("snapshot required")
    };
    let branch = snapshot.branch;
    let commands = [
        ("acquire", None),
        (
            "wait",
            Some(Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            }),
        ),
        (
            "note",
            Some(Command::Annotate {
                anchor: Anchor::State { revision: 1 },
                text: "Remember this after restarting.".into(),
                source: ClientSource::User,
                audience: Audience::Private,
                category: AnnotationCategory::Note,
            }),
        ),
        ("save", None),
    ];
    let mut wait_context = None;
    let mut note_context = None;
    let mut wait_admission = None;
    let mut note_receipt = None;
    for (id, command) in commands {
        let request = if let Some(command) = command {
            let context = socket.input_context();
            if id == "wait" {
                wait_context = Some(context.clone());
            }
            if id == "note" {
                note_context = Some(context.clone());
            }
            Request::Command {
                context,
                branch: branch.clone(),
                command,
            }
        } else if id == "acquire" {
            Request::AcquireControl
        } else {
            Request::Save
        };
        let request = ClientMessage::Request {
            request_id: id.into(),
            request,
        };
        socket
            .send(Message::Text(
                serde_json::to_string(&request).unwrap().into(),
            ))
            .await
            .unwrap();
        loop {
            match next_message(&mut socket).await {
                ServerMessage::Ack {
                    request_id,
                    receipt,
                    ..
                } if request_id == id => {
                    assert_eq!(receipt.actor(), ActorId(1));
                    assert_eq!(receipt.branch(), &branch);
                    if id == "note" {
                        note_receipt = Some(receipt.clone());
                    }
                    if id == "wait" {
                        let RequestReceipt::Admitted {
                            intention,
                            entry_id,
                            phase,
                            ..
                        } = receipt
                        else {
                            panic!("wait must acknowledge admission")
                        };
                        assert_eq!(phase, IntentionPhase::Queued);
                        wait_admission = Some((intention, entry_id));
                    } else {
                        break;
                    }
                }
                ServerMessage::Update { update }
                    if id == "wait"
                        && matches!(&update.body, UpdateBody::Intention { status }
                        if status.phase == IntentionPhase::Resolved
                            && wait_admission.as_ref().is_some_and(|(intention, entry)|
                                &status.intention == intention && &status.entry_id == entry)) =>
                {
                    break
                }
                ServerMessage::Update { .. } => {}
                other => panic!("{other:?}"),
            }
        }
        if id == "wait" {
            let ServerMessage::Update { update } = next_message(&mut socket).await else {
                panic!("readiness follows resolution")
            };
            assert!(
                matches!(&update.body, UpdateBody::Readiness { readiness } if readiness.admission)
            );
        }
    }
    drop(socket);
    drop(child); // Kill after the explicit durable save acknowledgement.
    let (_resumed, address) = launch(&path);
    let (socket, _) = timeout(Duration::from_secs(5), connect_async(&address))
        .await
        .unwrap()
        .unwrap();
    let mut socket = WireClient::new(socket);
    socket
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    socket
        .send(Message::Text(
            serde_json::to_string(&attach).unwrap().into(),
        ))
        .await
        .unwrap();
    let message = next_message(&mut socket).await;
    let ServerMessage::Snapshot { snapshot, .. } = message else {
        panic!("snapshot required")
    };
    assert_eq!(snapshot.branch, branch);
    assert_eq!(snapshot.state.observation.tick, 100);
    assert_eq!(snapshot.state.revision, 1);
    assert_eq!(snapshot.history.entries.len(), 2);
    assert!(
        matches!(&snapshot.history.entries[1].content, HistoryContent::Annotation { text, .. } if text == "Remember this after restarting.")
    );
    // A lost acknowledgement can be recovered after restart without reacquiring
    // control. The original action must not execute for a second time.
    assert!(!snapshot.has_control);
    let retry = ClientMessage::Request {
        request_id: "wait".into(),
        request: Request::Command {
            context: wait_context.unwrap(),
            branch,
            command: Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        },
    };
    socket
        .send(Message::Text(serde_json::to_string(&retry).unwrap().into()))
        .await
        .unwrap();
    let message = next_message(&mut socket).await;
    let (intention, entry_id) = wait_admission.expect("durable admission identity");
    assert_ne!(entry_id, snapshot.history.entries[0].id);
    assert_eq!(
        message,
        ServerMessage::Ack {
            context: snapshot.reply_context(),
            request_id: "wait".into(),
            receipt: RequestReceipt::Admitted {
                actor: ActorId(1),
                branch: snapshot.branch.clone(),
                intention,
                entry_id,
                phase: IntentionPhase::Resolved,
            },
        }
    );
    socket
        .send(Message::Text(
            serde_json::to_string(&ClientMessage::Request {
                request_id: "note".into(),
                request: Request::Command {
                    context: note_context.unwrap(),
                    branch: snapshot.branch.clone(),
                    command: Command::Annotate {
                        anchor: Anchor::State { revision: 1 },
                        text: "Remember this after restarting.".into(),
                        source: ClientSource::User,
                        audience: Audience::Private,
                        category: AnnotationCategory::Note,
                    },
                },
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        next_message(&mut socket).await,
        ServerMessage::Ack {
            context: snapshot.reply_context(),
            request_id: "note".into(),
            receipt: note_receipt.expect("original immediate receipt"),
        }
    );
    socket
        .send(Message::Text(
            serde_json::to_string(&ClientMessage::Request {
                request_id: "after-retry".into(),
                request: Request::Snapshot,
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    let ServerMessage::Snapshot {
        snapshot: after, ..
    } = next_message(&mut socket).await
    else {
        panic!("snapshot required")
    };
    assert_eq!(after.state, snapshot.state);
    assert_eq!(after.history, snapshot.history);
    assert_eq!(after.cursor, snapshot.cursor);
}

/// The next message, past `waiting` signals, which these tests don't watch.
async fn next_message(socket: &mut WireClient) -> ServerMessage {
    loop {
        match socket.receive().await.expect("connected actual server") {
            ServerMessage::Waiting { .. } => continue,
            message => return message,
        }
    }
}

#[tokio::test]
async fn actual_server_recovers_a_client_gap_without_repeating_an_admitted_action() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("recovered-gap.db");
    let (child, upstream_address) = launch(&path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let proxy = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut downstream = tokio_tungstenite::accept_async(stream).await.unwrap();
        let (mut upstream, _) = connect_async(upstream_address).await.unwrap();
        let mut dropped_readiness = false;
        let mut actions = 0;
        let mut resets = 0;
        loop {
            tokio::select! {
                frame = downstream.next() => {
                    let Some(Ok(frame)) = frame else { break; };
                    if let Message::Text(text) = &frame {
                        let message: ClientMessage = serde_json::from_str(text).unwrap();
                        if let ClientMessage::Request { request, .. } = message {
                            match request {
                                Request::Command { command: Command::Act { .. }, .. } => actions += 1,
                                Request::Snapshot => resets += 1,
                                _ => {},
                            }
                        }
                    }
                    if upstream.send(frame).await.is_err() { break; }
                },
                frame = upstream.next() => {
                    let Some(Ok(frame)) = frame else { break; };
                    if let Message::Text(text) = &frame {
                        let message: ServerMessage = serde_json::from_str(text).unwrap();
                        if !dropped_readiness && matches!(message, ServerMessage::Update { update }
                            if matches!(&update.body, UpdateBody::Readiness { readiness } if !readiness.admission)) {
                            dropped_readiness = true;
                            continue;
                        }
                    }
                    if downstream.send(frame).await.is_err() { break; }
                },
            }
        }
        assert!(dropped_readiness);
        assert_eq!(
            actions, 1,
            "accepted gameplay is never replayed during recovery"
        );
        assert_eq!(resets, 1, "one recovery owns one snapshot request");
    });
    let mut client = tor_client_common::Connection::connect(
        address,
        "process-test-token-not-a-real-secret".into(),
        ActorId(1),
        "test-recovery",
    )
    .await
    .unwrap();
    let original_context = client.state.context().clone();
    let control = client.request(Request::AcquireControl).await.unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            if matches!(client.next().await.unwrap(), ServerMessage::Ack { request_id, .. } if request_id == control) { break; }
        }
    }).await.unwrap();
    // Submit once under confirmed control. Dropping the admission's readiness
    // update creates a gap while its original acknowledgement remains in flight.
    let action = client
        .request(Request::Command {
            context: client.state.input_context(),
            branch: client.state.branch().clone(),
            command: Command::Act {
                expected_revision: client.state.state().revision,
                action: Action::Wait,
            },
        })
        .await
        .unwrap();
    let mut pending = tor_client_common::PendingRequest::new(action);
    timeout(Duration::from_secs(5), async {
        loop {
            let message = client.next().await.unwrap();
            let completion = pending.observe(&client, &message);
            if matches!(
                &message,
                ServerMessage::Ack {
                    receipt: RequestReceipt::Admitted { .. },
                    ..
                }
            ) {
                assert!(
                    !client.is_synchronized(),
                    "the acknowledgement itself must detect the missing permission update"
                );
                assert!(
                    completion.is_none(),
                    "admission cannot complete input while its context is missing"
                );
            }
            if client.is_recovery_snapshot(&message) {
                assert!(
                    matches!(
                        completion,
                        Some(tor_client_common::RequestCompletion::Reply(
                            tor_client_common::ConfirmedReply::Receipt(RequestReceipt::Admitted {
                                actor: ActorId(1),
                                ..
                            })
                        ))
                    ),
                    "original admission acknowledgement must survive stream repair"
                );
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(client.is_synchronized());
    assert_eq!(client.state.context().stream, original_context.stream);
    assert_eq!(client.state.context().epoch, original_context.epoch + 1);
    assert!(client.state.has_control());
    assert_eq!(client.state.state().observation.tick, 100);
    assert!(client.state.intentions().is_empty());
    let expected = client.state.state().clone();
    client.close().await.unwrap();
    timeout(Duration::from_secs(5), proxy)
        .await
        .unwrap()
        .unwrap();
    drop(child);
    let (restored_server, restored_address) = launch(&path);
    let mut restored = tor_client_common::Connection::connect(
        restored_address
            .trim_start_matches("ws://")
            .parse()
            .unwrap(),
        "process-test-token-not-a-real-secret".into(),
        ActorId(1),
        "test-recovery",
    )
    .await
    .unwrap();
    assert_eq!(restored.state.state(), &expected);
    assert_eq!(
        restored
            .state
            .history()
            .iter()
            .filter(|entry| matches!(entry.content, HistoryContent::Action { .. }))
            .count(),
        1
    );
    restored.close().await.unwrap();
    drop(restored_server);
}

#[tokio::test]
async fn actual_server_malformed_requests_do_not_publish_or_persist_actions() {
    use tokio_tungstenite::tungstenite::protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("malformed-request.db");
    let (child, address) = launch(&path);
    let mut healthy = tor_client_common::Connection::connect(
        address.strip_prefix("ws://").unwrap().parse().unwrap(),
        "process-test-token-not-a-real-secret".into(),
        ActorId(1),
        "healthy-decoder-peer",
    )
    .await
    .unwrap();
    let before = healthy.state.state().clone();
    assert!(healthy.state.history().is_empty());
    for case in [
        "deep_hello",
        "deep_request",
        "trailing_request",
        "fragmented_oversize",
    ] {
        let (socket, _) = connect_async(&address).await.unwrap();
        let mut bad = WireClient::new(socket);
        let hello = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            token: "process-test-token-not-a-real-secret".into(),
            frontend: "malformed-request-peer".into(),
        };
        let mut ignored = serde_json::Value::Null;
        for _ in 0..MAX_JSON_DEPTH + 1 {
            ignored = serde_json::Value::Array(vec![ignored]);
        }
        let text = if case == "deep_hello" {
            let mut wire = serde_json::to_value(hello).unwrap();
            wire["ignored"] = ignored;
            serde_json::to_string(&wire).unwrap()
        } else {
            bad.send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
                .await
                .unwrap();
            assert!(matches!(
                bad.receive().await,
                Some(ServerMessage::Welcome { .. })
            ));
            bad.request("attach", Request::Attach { actor: ActorId(1) })
                .await;
            loop {
                if matches!(
                    bad.receive().await.expect("attached peer"),
                    ServerMessage::Snapshot { .. }
                ) {
                    break;
                }
            }
            let mut request = serde_json::to_value(ClientMessage::Request {
                request_id: "rejected-input".into(),
                request: Request::Snapshot,
            })
            .unwrap();
            match case {
                "deep_request" => request["ignored"] = ignored,
                "fragmented_oversize" => request["ignored"] = "x".repeat(MAX_REQUEST_BYTES).into(),
                "trailing_request" => {}
                _ => unreachable!(),
            }
            let mut text = serde_json::to_string(&request).unwrap();
            if case == "trailing_request" {
                text.push_str(" {}");
            }
            text
        };
        if case == "fragmented_oversize" {
            assert!(text.len() > MAX_REQUEST_BYTES);
            let split = text.len() / 2;
            assert!(split < MAX_REQUEST_BYTES && text.len() - split < MAX_REQUEST_BYTES);
            for (bytes, opcode, finished) in [
                (&text.as_bytes()[..split], OpCode::Data(Data::Text), false),
                (
                    &text.as_bytes()[split..],
                    OpCode::Data(Data::Continue),
                    true,
                ),
            ] {
                bad.send(Message::Frame(Frame::message(
                    bytes.to_vec(),
                    opcode,
                    finished,
                )))
                .await
                .unwrap();
            }
        } else {
            assert!(text.len() < MAX_REQUEST_BYTES);
            bad.send(Message::Text(text.into())).await.unwrap();
        }
        loop {
            match bad.receive().await {
                Some(ServerMessage::Waiting { .. }) => {}
                Some(ServerMessage::Error {
                    scope: ErrorScope::Transport {},
                    code: ErrorCode::InvalidRequest,
                    request_id: None,
                    ..
                }) => break,
                None if case == "fragmented_oversize" => break,
                other => {
                    panic!("malformed input must not publish an action/result: {case}: {other:?}")
                }
            }
        }
        drop(bad);
        let id = healthy.request(Request::Snapshot).await.unwrap();
        loop {
            if let ServerMessage::Snapshot { request_id, .. } = healthy.next().await.unwrap() {
                if request_id == id {
                    break;
                }
            }
        }
        assert_eq!(healthy.state.state(), &before, "{case}");
        assert!(healthy.state.history().is_empty(), "{case}");
        assert!(healthy.is_synchronized());
    }
    let id = healthy.request(Request::Save).await.unwrap();
    loop {
        if let ServerMessage::Ack { request_id, .. } = healthy.next().await.unwrap() {
            if request_id == id {
                break;
            }
        }
    }
    healthy.close().await.unwrap();
    drop(child);
    let reopened = tor_server::Engine::open(&path, tor_server::Scenario::two_room(42)).unwrap();
    assert_eq!(reopened.state(ActorId(1)).unwrap(), before);
}
