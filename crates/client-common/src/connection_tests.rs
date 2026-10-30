//! Deadline coverage uses an expired stored deadline, avoiding clock sleeps.
use super::*;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, WebSocketStream};

async fn next_non_map_frame(
    socket: &mut WebSocketStream<TcpStream>,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    loop {
        let frame = socket.next().await?;
        if let Ok(Message::Text(text)) = &frame {
            if let Ok(ClientMessage::Request {
                request_id,
                request,
            }) = serde_json::from_str::<ClientMessage>(text)
            {
                if let Request::Map { query } = *request {
                    assert!(request_id.starts_with("map-"));
                    query.validate().unwrap();
                    assert!(query.cursor.is_none(), "unanswered maps cannot continue");
                    continue;
                }
            }
        }
        return Some(frame);
    }
}
fn visible_model(
    state: &ClientState,
) -> (
    crate::ClientSessionState,
    Vec<crate::RememberedCell>,
    Vec<RememberedMapCell>,
) {
    (
        state.session().clone(),
        state.memory().cloned().collect(),
        state.map_memory().map(|c| c.appearance()).collect(),
    )
}

async fn receive(socket: &mut WebSocketStream<TcpStream>) -> ClientMessage {
    let frame = timeout(Duration::from_secs(2), next_non_map_frame(socket))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(frame.to_text().unwrap()).unwrap()
}
async fn send(socket: &mut WebSocketStream<TcpStream>, message: ServerMessage) {
    socket
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn an_expired_recovery_cannot_send_or_apply_queued_traffic_in_any_phase() {
    for phase in [QueryPhase::Queue, QueryPhase::Flush, QueryPhase::AwaitReply] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let initial = snapshot();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            assert!(matches!(
                receive(&mut socket).await,
                ClientMessage::Hello { .. }
            ));
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role: AccessRole::Spectator,
                },
            )
            .await;
            assert!(matches!(
                receive(&mut socket).await,
                ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
            ));
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            send(
                &mut socket,
                ServerMessage::Update {
                    update: Box::new(StreamUpdate {
                        context: initial.context,
                        actor: initial.actor,
                        branch: initial.branch,
                        cursor: StreamCursor {
                            sequence: initial.cursor.sequence + 1,
                            tick: initial.cursor.tick,
                        },
                        body: UpdateBody::Control { has_control: true },
                    }),
                },
            )
            .await;
            if let Some(Ok(Message::Text(text))) = socket.next().await {
                panic!("an expired recovery must not send a request: {text}");
            }
        });
        let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
            .await
            .unwrap();
        let before = visible_model(&client.state);
        client.require_snapshot();
        let recovery = client.recovery.as_mut().unwrap();
        recovery.phase = phase;
        recovery.deadline = tokio::time::Instant::now() - Duration::from_millis(1);
        let error = timeout(Duration::from_secs(1), client.next())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.to_string(), "Stream resynchronization timed out");
        assert_eq!(visible_model(&client.state), before);
        assert!(!client.is_synchronized());
        assert!(client.request(Request::AcquireControl).await.is_err());
        drop(client);
        server.await.unwrap();
    }
}

fn snapshot() -> Snapshot {
    let samples: serde_json::Value =
        serde_json::from_str(
            &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                format!("../protocol/tests/fixtures/wire-v{PROTOCOL_VERSION}.json"),
            ))
            .unwrap(),
        )
        .unwrap();
    let mut current = samples["server"]
        .as_array()
        .unwrap()
        .iter()
        .find(|sample| sample["type"] == "snapshot")
        .unwrap()["snapshot"]
        .clone();
    // Coordinated goldens stay untouched until the current-only format handoff.
    if current["state"]["observation"]["combat"].is_object() {
        current["state"]["observation"]["combat"]["sequence"] = serde_json::Value::Null;
    }
    serde_json::from_value(current).unwrap()
}

#[tokio::test]
async fn canceled_partial_snapshot_read_keeps_assembly_and_disables_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let mut reset = initial.clone();
    reset.context = reset.context.next_reset().unwrap();
    let expected = reset.clone();
    let (consumed, received) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Player,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        let parts = encode_snapshot(
            &ServerMessage::Snapshot {
                request_id: "staged-reset".into(),
                snapshot: Box::new(reset),
            },
            512,
            tor_protocol::MAX_SNAPSHOT_BYTES * 2,
        )
        .unwrap();
        assert!(parts.len() > 1);
        socket
            .send(Message::Text(parts[0].clone().into()))
            .await
            .unwrap();
        socket.send(Message::Ping(Vec::new().into())).await.unwrap();
        // The client's automatic pong proves its pending read consumed the part
        // and the following ping. Cancel at this barrier, without a timing race.
        let pong = timeout(Duration::from_secs(2), next_non_map_frame(&mut socket))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(pong, Message::Pong(_)));
        consumed.send(()).unwrap();
        released.await.unwrap();
        for part in parts.into_iter().skip(1) {
            socket.send(Message::Text(part.into())).await.unwrap();
        }
    });
    let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
        .await
        .unwrap();
    let before = visible_model(&client.state);
    tokio::select! {
        message = client.next() => panic!("partial transfer escaped: {message:?}"),
        received = received => received.unwrap(),
    }
    assert_eq!(
        visible_model(&client.state),
        before,
        "partial snapshot must not replace the visible model"
    );
    assert!(!client.is_synchronized());
    assert!(client.request(Request::AcquireControl).await.is_err());
    release.send(()).unwrap();
    let completed = client.next().await.unwrap();
    assert_eq!(
        completed,
        ServerMessage::Snapshot {
            request_id: "staged-reset".into(),
            snapshot: Box::new(expected.clone()),
        }
    );
    assert_eq!(client.state.reset_snapshot().unwrap(), expected);
    assert!(client.is_synchronized());
    server.await.unwrap();
}

#[tokio::test]
async fn receipts_keep_original_branch_identity_but_cannot_name_another_actor() {
    for admitted in [false, true] {
        for foreign_actor in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let initial = snapshot();
            let actor = if foreign_actor {
                ActorId(999)
            } else {
                initial.actor
            };
            let old_branch = BranchId("original-branch-before-rewind".into());
            assert_ne!(old_branch, initial.branch);
            let receipt = if admitted {
                RequestReceipt::Admitted {
                    actor,
                    branch: old_branch,
                    intention: IntentionId("original-intention".into()),
                    entry_id: EntryId("original-admission".into()),
                    phase: IntentionPhase::Resolved,
                }
            } else {
                RequestReceipt::Immediate {
                    actor,
                    branch: old_branch,
                    entry_id: Some(EntryId("original-note".into())),
                }
            };
            let expected = receipt.clone();
            let reply_context = initial.reply_context();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = accept_async(stream).await.unwrap();
                assert!(matches!(
                    receive(&mut socket).await,
                    ClientMessage::Hello { .. }
                ));
                send(
                    &mut socket,
                    ServerMessage::Welcome {
                        protocol: PROTOCOL_VERSION,
                        capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                        user: "test".into(),
                        actors: vec![initial.actor],
                        role: AccessRole::Player,
                    },
                )
                .await;
                assert!(matches!(
                    receive(&mut socket).await,
                    ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
                ));
                send(
                    &mut socket,
                    ServerMessage::Snapshot {
                        request_id: "attach".into(),
                        snapshot: Box::new(initial),
                    },
                )
                .await;
                let ClientMessage::Request {
                    request_id,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("one request required")
                };
                let Request::Save = *request else {
                    panic!("one request required")
                };
                send(
                    &mut socket,
                    ServerMessage::Ack {
                        context: reply_context,
                        request_id,
                        receipt,
                    },
                )
                .await;
                if let Some(Ok(Message::Text(text))) = next_non_map_frame(&mut socket).await {
                    panic!("receipt handling must not replay input: {text}");
                }
            });
            let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
                .await
                .unwrap();
            let before = visible_model(&client.state);
            let before_context = client.state.reply_context();
            let id = client.request(Request::Save).await.unwrap();
            let result = client.next().await;
            if foreign_actor {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    "Receipt belongs to another actor"
                );
            } else {
                assert_eq!(
                    result.unwrap(),
                    ServerMessage::Ack {
                        context: before_context,
                        request_id: id,
                        receipt: expected
                    }
                );
            }
            assert_eq!(visible_model(&client.state), before);
            drop(client);
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn palette_backpressure_cancellation_does_not_lose_the_applied_observation() {
    backpressure_observation(false).await;
}

#[tokio::test]
async fn blocked_map_flush_survives_cancellation_without_starving_or_duplicating() {
    backpressure_observation(true).await;
}

async fn backpressure_observation(block_map_first: bool) {
    use tokio::io::AsyncReadExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let mut state = Arc::unwrap_or_clone(initial.state.clone());
    state.revision += 1;
    state.snapshot = tor_protocol::SnapshotId(format!("fixture-{}", state.revision));
    state.observation.visible_cells[0].asset = Some("missing.test.asset".into());
    let update = ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: initial.context.clone(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: initial.cursor.sequence + 1,
                tick: initial.cursor.tick,
            },
            body: UpdateBody::Observation {
                state: state.clone().into(),
                event: None,
            },
        }),
    };
    let expected = update.clone();
    let map_context = initial.reply_context();
    let mut context = initial.reply_context();
    context.cursor.sequence += 1;
    context.revision = state.revision;
    context.snapshot = state.snapshot.clone();
    let (sent, arrived) = tokio::sync::oneshot::channel();
    let (release, drain) = tokio::sync::oneshot::channel::<usize>();
    let (map_blocked, allow_update) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Hello { .. }
        ));
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Spectator,
            },
        )
        .await;
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
        ));
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        if block_map_first {
            sent.send(()).unwrap();
            allow_update.await.unwrap();
            send(&mut socket, update).await;
        } else {
            send(&mut socket, update).await;
            sent.send(()).unwrap();
        }
        // Test-only raw padding fills TCP without retaining an unbounded queue.
        // Drain it before decoding the subsequent real WebSocket query.
        let mut remaining = drain.await.unwrap();
        let mut buffer = vec![0; 65536];
        while remaining > 0 {
            let size = remaining.min(buffer.len());
            socket
                .get_mut()
                .read_exact(&mut buffer[..size])
                .await
                .unwrap();
            remaining -= size;
        }
        let mut obsolete_map = None;
        let request_id = loop {
            let frame = timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let ClientMessage::Request {
                request_id,
                request,
            } = serde_json::from_str::<ClientMessage>(frame.to_text().unwrap()).unwrap()
            else {
                panic!("expected one palette request")
            };
            match *request {
                Request::Map { query } => {
                    assert!(
                        obsolete_map.is_none(),
                        "a canceled blocked Map must not be queued twice"
                    );
                    query.validate().unwrap();
                    assert_eq!(query.context, map_context.input.stream);
                    assert_eq!(query.base.revision, map_context.revision);
                    assert!(query.cursor.is_none());
                    obsolete_map = Some((request_id, query));
                }
                Request::Palette => break request_id,
                other => panic!("expected one palette request, got {other:?}"),
            }
        };
        assert!(!block_map_first || obsolete_map.is_some());
        if let Some((request_id, query)) = obsolete_map {
            // The newer observation canceled this transfer before the socket
            // could finish sending it. Its late answer must not be published.
            send(
                &mut socket,
                ServerMessage::MapPage {
                    context: map_context,
                    request_id,
                    page: Box::new(MapPage {
                        transfer: "obsolete-map".into(),
                        actor: ActorId(1),
                        branch: query.branch,
                        base: query.base.clone(),
                        window: query.window,
                        start: 0,
                        page: 0,
                        total_cells: 0,
                        complete: true,
                        ambiguous: false,
                        limited: false,
                        cells: vec![],
                        next: None,
                    }),
                },
            )
            .await;
        }
        send(
            &mut socket,
            ServerMessage::Palette {
                context,
                request_id: Some(request_id),
                palette: PaletteUpdate {
                    revision: 1,
                    body: PaletteBody::Full {
                        assets: std::collections::BTreeSet::from(["missing.test.asset".into()]),
                    },
                },
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    client.palette.apply(&PaletteUpdate {
        revision: 0,
        body: PaletteBody::Full {
            assets: Default::default(),
        },
    });
    arrived.await.unwrap();
    let MaybeTlsStream::Plain(stream) = client.socket.get_ref() else {
        panic!("loopback test must use plain TCP")
    };
    let padding = vec![0; 65536];
    let mut written = 0;
    loop {
        match stream.try_write(&padding) {
            Ok(bytes) => written += bytes,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("padding write failed: {error}"),
        }
        assert!(
            written < 32 * 1024 * 1024,
            "test pressure must stay bounded"
        );
    }
    if block_map_first {
        assert!(timeout(Duration::from_millis(50), client.next())
            .await
            .is_err());
        let pending = &client.map_request.as_ref().unwrap().0;
        assert!(
            pending.phase == QueryPhase::Flush,
            "Map must be blocked in flush"
        );
        let identity = pending.request_id.clone();
        assert!(timeout(Duration::from_millis(20), client.next())
            .await
            .is_err());
        let pending = &client.map_request.as_ref().unwrap().0;
        assert!(pending.phase == QueryPhase::Flush);
        assert_eq!(
            pending.request_id, identity,
            "cancellation retains the queued identity"
        );
        map_blocked.send(()).unwrap();
    }
    assert!(timeout(Duration::from_millis(50), client.next())
        .await
        .is_err());
    assert_eq!(
        client.state.state(),
        &state,
        "observation was applied before cancellation"
    );
    assert!(
        client.playing(),
        "retained observation must remain pending presentation"
    );
    release.send(written).unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), client.next())
            .await
            .unwrap()
            .unwrap(),
        expected,
        "an applied observation must still reach presentation exactly once"
    );
    assert!(matches!(
        client.next().await.unwrap(),
        ServerMessage::Palette { .. }
    ));
    assert!(client.palette.holds("missing.test.asset"));
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_server_capabilities_are_rejected_before_actor_attachment() {
    for capabilities in [
        ServerCapabilities {
            max_connections: 0,
            ..ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16)
        },
        ServerCapabilities {
            max_request_bytes: u32::MAX,
            ..ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16)
        },
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            assert!(matches!(
                receive(&mut socket).await,
                ClientMessage::Hello { .. }
            ));
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities,
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role: AccessRole::Spectator,
                },
            )
            .await;
            let next = timeout(Duration::from_secs(2), socket.next()).await;
            assert!(
                !matches!(next, Ok(Some(Ok(Message::Text(_))))),
                "invalid capabilities must not grant attachment"
            );
        });
        assert!(
            Connection::connect(address, "test".into(), ActorId(1), "headless")
                .await
                .is_err()
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn advertised_request_limit_rejects_locally_and_keeps_connection_usable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let base_sequence = initial.cursor.sequence;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Hello { .. }
        ));
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities {
                    max_request_bytes: 256,
                    ..ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16)
                },
                user: "test".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Wizard,
            },
        )
        .await;
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
        ));
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        for sequence in base_sequence + 1..=base_sequence + 2 {
            send(
                &mut socket,
                ServerMessage::Update {
                    update: Box::new(StreamUpdate {
                        context: initial.context.clone(),
                        actor: initial.actor,
                        branch: initial.branch.clone(),
                        cursor: StreamCursor {
                            sequence,
                            tick: initial.cursor.tick,
                        },
                        body: UpdateBody::Control {
                            has_control: initial.has_control,
                        },
                    }),
                },
            )
            .await;
        }
        // Inspect the raw packet: an oversized automatic Map request must not
        // be swallowed by the unrelated-fixture receive helper.
        let frame = timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match serde_json::from_str::<ClientMessage>(frame.to_text().unwrap()).unwrap() {
            ClientMessage::Request {
                request_id,
                request,
            } if matches!(*request, Request::Snapshot) => request_id,
            _ => panic!("neither oversized command nor automatic Map may precede the valid query"),
        }
    });
    let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
        .await
        .unwrap();
    assert_eq!(client.capabilities().max_request_bytes, 256);
    let oversized = client.state.command_request(Command::Wizard {
        expected_revision: client.state.state().revision,
        operation: "x".repeat(512),
    });
    let error = client.request(oversized).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref::<EncodeError>(),
        Some(EncodeError::TooLarge { limit: 256 })
    ));
    for sequence in base_sequence + 1..=base_sequence + 2 {
        assert!(matches!(
            client.next().await.unwrap(),
            ServerMessage::Update { .. }
        ));
        assert_eq!(client.state.cursor().sequence, sequence);
        assert!(client.is_synchronized());
        assert!(client.state.map_hydration().failed);
        assert!(!client.state.map_hydration().pending);
        assert!(client.map_request.is_none());
    }
    let request_id = client.request(Request::Snapshot).await.unwrap();
    assert_eq!(server.await.unwrap(), request_id);
}

#[tokio::test]
async fn attachment_rejects_response_exceeding_advertised_frame_limit() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(512, 16),
                user: "test".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Spectator,
            },
        )
        .await;
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
        ));
        let oversized = ServerMessage::Snapshot {
            request_id: "attach".into(),
            snapshot: Box::new(initial),
        };
        assert!(serde_json::to_string(&oversized).unwrap().len() > 512);
        send(&mut socket, oversized).await;
    });
    let result = Connection::connect(address, "test".into(), ActorId(1), "headless").await;
    assert!(matches!(
        result
            .err()
            .and_then(|e| e.downcast::<DecodeError>().ok())
            .as_deref(),
        Some(DecodeError::TooLarge { limit: 512 })
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn creature_inspection_is_validated_and_rejected_for_non_wizards() {
    for (role, corrupt, accepted) in [
        (AccessRole::Wizard, false, true),
        (AccessRole::Wizard, true, false),
        (AccessRole::Player, false, false),
        (AccessRole::Spectator, false, false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let initial = snapshot();
        let context = initial.reply_context();
        let mut report = crate::inspection::tests::report();
        if corrupt {
            report.hp += 1;
        }
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial),
                },
            )
            .await;
            send(
                &mut socket,
                ServerMessage::CreatureInspection {
                    context,
                    request_id: "inspect".into(),
                    report: Box::new(report),
                },
            )
            .await;
        });
        let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
            .await
            .unwrap();
        let result = client.next().await;
        if accepted {
            assert!(matches!(
                result.unwrap(),
                ServerMessage::CreatureInspection { .. }
            ));
        } else {
            assert!(result.is_err());
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn stale_creature_inspection_is_discarded_during_stream_repair() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let mut stale_context = initial.reply_context();
    stale_context.revision += 1;
    let mut stale = crate::inspection::tests::report();
    stale.name = "stale report".into();
    let mut fresh = stale.clone();
    fresh.name = "fresh report".into();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Wizard,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        send(
            &mut socket,
            ServerMessage::CreatureInspection {
                context: stale_context,
                request_id: "stale".into(),
                report: Box::new(stale),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("one recovery snapshot required");
        };
        let Request::Snapshot = *request else {
            panic!("one recovery snapshot required");
        };
        let mut reset = initial;
        reset.context.epoch += 1;
        let context = reset.reply_context();
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id,
                snapshot: Box::new(reset),
            },
        )
        .await;
        send(
            &mut socket,
            ServerMessage::CreatureInspection {
                context,
                request_id: "fresh".into(),
                report: Box::new(fresh),
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
        .await
        .unwrap();
    assert!(matches!(
        client.next().await.unwrap(),
        ServerMessage::Snapshot { .. }
    ));
    assert!(client.is_synchronized());
    let ServerMessage::CreatureInspection { report, .. } = client.next().await.unwrap() else {
        panic!("fresh query reply");
    };
    assert_eq!(report.name, "fresh report");
    server.await.unwrap();
}

#[tokio::test]
async fn combat_diagnostics_is_validated_and_rejected_for_non_wizards() {
    for (role, corrupt, accepted) in [
        (AccessRole::Wizard, false, true),
        (AccessRole::Wizard, true, false),
        (AccessRole::Player, false, false),
        (AccessRole::Spectator, false, false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let initial = snapshot();
        let context = initial.reply_context();
        let mut report = crate::combat_diagnostics::tests::report();
        if corrupt {
            report.retained += 1;
        }
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial),
                },
            )
            .await;
            send(
                &mut socket,
                ServerMessage::CombatDiagnostics {
                    context,
                    request_id: "inspect".into(),
                    report: Box::new(report),
                },
            )
            .await;
        });
        let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
            .await
            .unwrap();
        let result = client.next().await;
        if accepted {
            assert!(matches!(
                result.unwrap(),
                ServerMessage::CombatDiagnostics { .. }
            ));
        } else {
            assert!(result.is_err());
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn stale_combat_diagnostics_is_discarded_during_stream_repair() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let mut stale_context = initial.reply_context();
    stale_context.revision += 1;
    let mut stale = crate::combat_diagnostics::tests::report();
    stale.tick = 77;
    let mut fresh = stale.clone();
    fresh.tick = 88;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Wizard,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        send(
            &mut socket,
            ServerMessage::CombatDiagnostics {
                context: stale_context,
                request_id: "stale".into(),
                report: Box::new(stale),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("one recovery snapshot required");
        };
        let Request::Snapshot = *request else {
            panic!("one recovery snapshot required");
        };
        let mut reset = initial;
        reset.context.epoch += 1;
        let context = reset.reply_context();
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id,
                snapshot: Box::new(reset),
            },
        )
        .await;
        send(
            &mut socket,
            ServerMessage::CombatDiagnostics {
                context,
                request_id: "fresh".into(),
                report: Box::new(fresh),
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
        .await
        .unwrap();
    assert!(matches!(
        client.next().await.unwrap(),
        ServerMessage::Snapshot { .. }
    ));
    assert!(client.is_synchronized());
    let ServerMessage::CombatDiagnostics { report, .. } = client.next().await.unwrap() else {
        panic!("fresh query reply");
    };
    assert_eq!(report.tick, 88);
    server.await.unwrap();
}

fn ordered_strike_page_message(context: ReplyContext, request_id: &str) -> ServerMessage {
    serde_json::from_value(serde_json::json!({"type":"creature_strike_page","context":context,
        "request_id":request_id,"page":{"inspection":"detail-handle","actor":"2","target_revision":"5",
        "source":{"kind":"ability","id":"basic_melee"},
        "resolved":{"kind":"ability","ability":{"id":"basic_melee","name":"Attack"}},
        "strike_count":0,"start":{"kind":"selected_melee"},"entries":[{"kind":"selected_melee"}],"next":null}}))
        .expect("strict typed private source page")
}

fn ordered_strike_query(client: &Connection) -> Request {
    client.state.command_request(Command::InspectStrikeSource {
        expected_revision: client.state.state().revision,
        actor: ActorId(2),
        target_revision: 5,
        source: StrikeSourceSelector::Ability {
            id: "basic_melee".into(),
        },
        cursor: None,
    })
}

#[tokio::test]
async fn ordered_strikes_detail_reply_is_private_validated_and_completes_only_its_query() {
    for role in [
        AccessRole::Wizard,
        AccessRole::Player,
        AccessRole::Spectator,
    ] {
        let initial = snapshot();
        let reply_context = initial.reply_context();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial),
                },
            )
            .await;
            let request_id = if role == AccessRole::Wizard {
                let ClientMessage::Request {
                    request_id,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("typed detail query");
                };
                let Request::Command {
                    command: Command::InspectStrikeSource { .. },
                    ..
                } = *request
                else {
                    panic!("typed detail query");
                };
                request_id
            } else {
                "unsolicited-private".into()
            };
            send(
                &mut socket,
                ordered_strike_page_message(reply_context, &request_id),
            )
            .await;
        });
        let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
            .await
            .unwrap();
        let before = client.state.session().clone();
        let id = if role == AccessRole::Wizard {
            client.request(ordered_strike_query(&client)).await.unwrap()
        } else {
            "unsolicited-private".into()
        };
        let result = client.next().await;
        if role == AccessRole::Wizard {
            let message = result.unwrap();
            let mut pending = crate::PendingRequest::new(id);
            assert!(matches!(
                pending.observe(&client, &message),
                Some(crate::RequestCompletion::Reply(
                    crate::ConfirmedReply::Completed
                ))
            ));
            let mut unrelated = crate::PendingRequest::new("other".into());
            assert!(unrelated.observe(&client, &message).is_none());
            assert_eq!(client.state.session().clone(), before);
        } else {
            assert!(
                result.is_err(),
                "ordinary roles cannot accept a private detail reply"
            );
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn ordered_strikes_stale_detail_is_discarded_before_recovery_and_fresh_reopen() {
    let initial = snapshot();
    let mut stale_context = initial.reply_context();
    stale_context.revision += 1;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Wizard,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("first detail query");
        };
        let Request::Command {
            command: Command::InspectStrikeSource { .. },
            ..
        } = *request
        else {
            panic!("first detail query");
        };
        send(
            &mut socket,
            ordered_strike_page_message(stale_context, &request_id),
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("stale detail requires recovery instead of presentation");
        };
        let Request::Snapshot = *request else {
            panic!("stale detail requires recovery instead of presentation");
        };
        let mut reset = initial;
        reset.context.epoch += 1;
        let fresh_context = reset.reply_context();
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id,
                snapshot: Box::new(reset),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("explicit fresh reopen");
        };
        let Request::Command {
            command: Command::InspectStrikeSource { cursor: None, .. },
            ..
        } = *request
        else {
            panic!("explicit fresh reopen");
        };
        send(
            &mut socket,
            ordered_strike_page_message(fresh_context, &request_id),
        )
        .await;
    });
    let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
        .await
        .unwrap();
    client.request(ordered_strike_query(&client)).await.unwrap();
    assert!(matches!(
        client.next().await.unwrap(),
        ServerMessage::Snapshot { .. }
    ));
    assert!(client.is_synchronized());
    let id = client.request(ordered_strike_query(&client)).await.unwrap();
    let fresh = serde_json::to_value(client.next().await.unwrap()).unwrap();
    assert_eq!(fresh["type"], "creature_strike_page");
    assert_eq!(fresh["request_id"], id);
    server.await.unwrap();
}

fn owner_advancement_message(context: ReplyContext, id: &str) -> ServerMessage {
    let basis = AdvancementBasis {
        actor: context.actor,
        branch: context.branch.clone(),
        stream: context.input.stream.stream.clone(),
        reset: context.input.stream.epoch,
        build_epoch: 1,
    };
    let content = |id: &str| AdvancementContentRef {
        id: id.into(),
        name: id.into(),
    };
    let page = AdvancementPage {
        basis,
        revision: context.revision,
        query: AdvancementQuery::Overview {},
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Overview {
            value: Box::new(AdvancementOverview {
                strength_acquired: 0,
                growth: AdvancementGrowth {
                    body_package_hit_dice: None,
                    highest_applied_tier: 2,
                    restorable_hit_dice: 0,
                    restoration_blocker: None,
                    highest_earned_tier: 2,
                    peak_threat: 2,
                    held_xp: 0,
                    earned_xp: 0,
                    spent_xp: 0,
                    next_threshold: 100,
                    future: FutureGrowth::Disabled {},
                },
                starting_skills: [content("deception"), content("persuasion")],
                points: AdvancementPoints {
                    earned: 4,
                    spent: 0,
                    unspent: 4,
                },
                timings: AdvancementTimings {
                    purchase_recovery: 0,
                    respec_prepare: 600,
                    respec_recovery: 0,
                    received_loss_interval: 600,
                    provisional: true,
                },
                ration: AdvancementRation {
                    supply: content("ration"),
                    quantity: 1,
                    access: SupplyAccess::CarriedOrGroundTouch,
                },
                received_loss_remaining: 0,
                perceived_threat: false,
                ready: true,
                other_preparation: false,
                locked: false,
                respec: None,
            }),
        }],
    };
    ServerMessage::AdvancementPage {
        context,
        request_id: id.into(),
        page: Box::new(page),
    }
}

#[tokio::test]
async fn advancement_transport_installs_only_correlated_owned_bounded_pages_and_clears_on_revocation(
) {
    for case in [
        "owner",
        "owner_cursor",
        "external",
        "observer",
        "spectator",
        "unsolicited",
        "oversized",
        "wrong_id",
    ] {
        let mut initial = snapshot();
        initial.has_control = !matches!(case, "observer" | "spectator");
        initial.readiness.admission = initial.has_control;
        let role = if case == "spectator" {
            AccessRole::Spectator
        } else {
            AccessRole::Player
        };
        let mut context = initial.reply_context();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![initial.actor],
                    role,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            let id = if case == "unsolicited" {
                "unsolicited".into()
            } else {
                let ClientMessage::Request {
                    request_id,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("ordinary advancement query required")
                };
                let Request::Command {
                    command:
                        Command::Advancement {
                            query: AdvancementQuery::Overview {},
                            ..
                        },
                    ..
                } = *request
                else {
                    panic!("ordinary advancement query required")
                };
                request_id
            };
            if case == "owner_cursor" {
                context.cursor.sequence += 1;
                let facts = OutcomeFacts::capture(
                    &initial.state.observation,
                    &initial.state.observation,
                    None,
                );
                send(
                    &mut socket,
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: initial.context.clone(),
                            actor: initial.actor,
                            branch: initial.branch.clone(),
                            cursor: context.cursor,
                            body: UpdateBody::Outcome {
                                visual_changed: false,
                                facts,
                                event: None,
                            },
                        }),
                    },
                )
                .await;
            }
            let message =
                owner_advancement_message(context, if case == "wrong_id" { "wrong" } else { &id });
            if case == "oversized" {
                let mut text = serde_json::to_string(&message).unwrap();
                text.push_str(&" ".repeat(MAX_ADVANCEMENT_PAGE_BYTES + 1 - text.len()));
                socket.send(Message::Text(text.into())).await.unwrap();
            } else {
                send(&mut socket, message).await;
            }
            if matches!(case, "owner" | "owner_cursor" | "external") {
                send(
                    &mut socket,
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: initial.context,
                            actor: initial.actor,
                            branch: initial.branch,
                            cursor: StreamCursor {
                                sequence: initial.cursor.sequence
                                    + if case == "owner_cursor" { 2 } else { 1 },
                                tick: initial.cursor.tick,
                            },
                            body: UpdateBody::Control { has_control: false },
                        }),
                    },
                )
                .await;
            }
        });
        let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
            .await
            .unwrap();
        if case == "external" {
            client.use_external_advancement_presentation();
        }
        let before = client.state.state().clone();
        let id = if case != "unsolicited" {
            Some(
                client
                    .request(client.state.command_request(Command::Advancement {
                        expected_revision: client.state.state().revision,
                        query: AdvancementQuery::Overview {},
                    }))
                    .await
                    .unwrap(),
            )
        } else {
            None
        };
        if case == "owner_cursor" {
            assert!(matches!(
                client.next().await.unwrap(),
                ServerMessage::Update { .. }
            ));
            assert_eq!(client.state.state(), &before);
        }
        let result = client.next().await;
        if matches!(case, "owner" | "owner_cursor" | "external") {
            let message = result.unwrap();
            assert_eq!(
                client.advancement_pages.overview().is_some(),
                matches!(case, "owner" | "owner_cursor")
            );
            let mut pending = crate::PendingRequest::new(id.unwrap());
            assert!(matches!(
                pending.observe(&client, &message),
                Some(crate::RequestCompletion::Reply(
                    crate::ConfirmedReply::Completed
                ))
            ));
            client.next().await.unwrap();
            assert!(!client.state.has_control());
            assert!(client.advancement_pages.page().is_none());
        } else {
            assert!(result.is_err(), "{case}");
            assert!(client.advancement_pages.page().is_none());
            assert_eq!(client.state.state(), &before);
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn advancement_pending_reply_after_ordered_control_loss_is_rejected_in_both_presenters() {
    for external in [false, true] {
        let mut initial = snapshot();
        initial.has_control = true;
        initial.readiness.admission = true;
        let original_actor = initial.actor;
        let context = initial.reply_context();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "owner".into(),
                    actors: vec![initial.actor],
                    role: AccessRole::Player,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request,
            } = receive(&mut socket).await
            else {
                panic!("current controller must issue the pending query")
            };
            let Request::Command {
                command:
                    Command::Advancement {
                        query: AdvancementQuery::Overview {},
                        ..
                    },
                ..
            } = *request
            else {
                panic!("current controller must issue the pending query")
            };
            // This valid reply was prepared while the same actor was controlled.
            let authorized_reply = owner_advancement_message(context, &request_id);
            send(
                &mut socket,
                ServerMessage::Update {
                    update: Box::new(StreamUpdate {
                        context: initial.context,
                        actor: initial.actor,
                        branch: initial.branch,
                        cursor: StreamCursor {
                            sequence: initial.cursor.sequence + 1,
                            tick: initial.cursor.tick,
                        },
                        body: UpdateBody::Control { has_control: false },
                    }),
                },
            )
            .await;
            // Its old envelope must not trigger snapshot repair and then become
            // private presentation after the ordered revocation already applied.
            send(&mut socket, authorized_reply).await;
        });
        let mut client = Connection::connect(address, "token".into(), original_actor, "test")
            .await
            .unwrap();
        if external {
            client.use_external_advancement_presentation();
        }
        assert!(client.state.has_control());
        client
            .request(client.state.command_request(Command::Advancement {
                expected_revision: client.state.state().revision,
                query: AdvancementQuery::Overview {},
            }))
            .await
            .unwrap();
        assert!(matches!(
            client.next().await.unwrap(),
            ServerMessage::Update { .. }
        ));
        assert!(!client.state.has_control());
        assert_eq!(client.role(), AccessRole::Player);
        assert_eq!(client.state.state().observation.actor, original_actor);
        let error = client.next().await.unwrap_err().to_string();
        assert!(
            error.contains("without current control"),
            "{external}: {error}"
        );
        assert_eq!(client.advancement_pages.retained_pages(), 0);
        assert!(client.advancement_pages.page().is_none());
        assert!(client.advancement_pages.confirmation().is_none());
        server.await.unwrap();
    }
}

fn creature_catalog_message(context: ReplyContext, request_id: &str) -> ServerMessage {
    serde_json::from_value(serde_json::json!({"type":"creature_catalog","context":context,
        "request_id":request_id,"page":{"depth":27,"entries":[
            {"selector":"rogue:vampire","name":"Vampire","details":"Racial growth at depth 27"}],"next":null}}))
        .expect("strict typed private creature catalog")
}

fn creature_catalog_query(client: &Connection) -> Request {
    client.state.command_request(Command::Wizard {
        expected_revision: client.state.state().revision,
        operation: "creature list depth 27".into(),
    })
}

#[tokio::test]
async fn creature_catalog_reply_is_private_validated_and_completes_only_its_query() {
    for role in [
        AccessRole::Wizard,
        AccessRole::Player,
        AccessRole::Spectator,
    ] {
        let initial = snapshot();
        let reply_context = initial.reply_context();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial),
                },
            )
            .await;
            let request_id = if role == AccessRole::Wizard {
                let ClientMessage::Request {
                    request_id,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("typed catalog query");
                };
                let Request::Command {
                    command: Command::Wizard { .. },
                    ..
                } = *request
                else {
                    panic!("typed catalog query");
                };
                request_id
            } else {
                "unsolicited-private".into()
            };
            send(
                &mut socket,
                creature_catalog_message(reply_context, &request_id),
            )
            .await;
        });
        let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
            .await
            .unwrap();
        let before = client.state.session().clone();
        let id = if role == AccessRole::Wizard {
            client
                .request(creature_catalog_query(&client))
                .await
                .unwrap()
        } else {
            "unsolicited-private".into()
        };
        let result = client.next().await;
        if role == AccessRole::Wizard {
            let message = result.unwrap();
            let mut pending = crate::PendingRequest::new(id);
            assert!(matches!(
                pending.observe(&client, &message),
                Some(crate::RequestCompletion::Reply(
                    crate::ConfirmedReply::Completed
                ))
            ));
            let mut unrelated = crate::PendingRequest::new("other".into());
            assert!(unrelated.observe(&client, &message).is_none());
            assert_eq!(client.state.session().clone(), before);
        } else {
            assert!(
                result.is_err(),
                "ordinary roles cannot accept a private catalog reply"
            );
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn creature_catalog_stale_reply_is_discarded_before_recovery_and_fresh_reopen() {
    let initial = snapshot();
    let mut stale_context = initial.reply_context();
    stale_context.revision += 1;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Wizard,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("first catalog query");
        };
        let Request::Command {
            command: Command::Wizard { .. },
            ..
        } = *request
        else {
            panic!("first catalog query");
        };
        send(
            &mut socket,
            creature_catalog_message(stale_context, &request_id),
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("stale catalog requires recovery instead of presentation");
        };
        let Request::Snapshot = *request else {
            panic!("stale catalog requires recovery instead of presentation");
        };
        let mut reset = initial;
        reset.context.epoch += 1;
        let fresh_context = reset.reply_context();
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id,
                snapshot: Box::new(reset),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("explicit fresh reopen");
        };
        let Request::Command {
            command: Command::Wizard { .. },
            ..
        } = *request
        else {
            panic!("explicit fresh reopen");
        };
        send(
            &mut socket,
            creature_catalog_message(fresh_context, &request_id),
        )
        .await;
    });
    let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
        .await
        .unwrap();
    let stale_id = client
        .request(creature_catalog_query(&client))
        .await
        .unwrap();
    let mut pending = crate::PendingRequest::new(stale_id);
    let recovered = client.next().await.unwrap();
    assert!(matches!(recovered, ServerMessage::Snapshot { .. }));
    assert!(matches!(
        pending.observe(&client, &recovered),
        Some(crate::RequestCompletion::Unknown)
    ));
    assert!(client.is_synchronized());
    let id = client
        .request(creature_catalog_query(&client))
        .await
        .unwrap();
    let fresh = serde_json::to_value(client.next().await.unwrap()).unwrap();
    assert_eq!(fresh["type"], "creature_catalog");
    assert_eq!(fresh["request_id"], id);
    server.await.unwrap();
}

#[tokio::test]
async fn creature_catalog_rejects_wrong_attachment_actor_and_malformed_payload() {
    for fault in ["actor", "attachment", "entries", "stale-player"] {
        let initial = snapshot();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role: if fault == "stale-player" {
                        AccessRole::Player
                    } else {
                        AccessRole::Wizard
                    },
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            let mut value =
                serde_json::to_value(creature_catalog_message(initial.reply_context(), "private"))
                    .unwrap();
            match fault {
                "actor" => value["context"]["actor"] = serde_json::json!("2"),
                "attachment" => {
                    value["context"]["input"]["stream"]["stream"] = serde_json::json!("another")
                }
                "entries" => {
                    value["page"]["entries"][0]["selector"] = serde_json::json!("invalid selector")
                }
                "stale-player" => value["context"]["revision"] = serde_json::json!("999"),
                _ => unreachable!(),
            }
            socket
                .send(Message::Text(value.to_string().into()))
                .await
                .unwrap();
        });
        let mut client = Connection::connect(address, "test".into(), ActorId(1), "headless")
            .await
            .unwrap();
        let before = client.state.session().clone();
        let error = client.next().await.unwrap_err().to_string();
        if fault == "stale-player" {
            assert!(
                error.contains("non-wizard"),
                "privacy rejection precedes stale query recovery: {error}"
            );
        }
        assert_eq!(client.state.session().clone(), before);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn spectator_map_hydration_survives_cancellation_without_blocking_live_state() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let expected = initial.clone();
    let (queried, query_seen) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Spectator,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        // Unlike unrelated transport fixtures, this owner inspects every map packet.
        let packet = socket.next().await.unwrap().unwrap();
        let ClientMessage::Request {
            request_id,
            request,
        } = serde_json::from_str(packet.to_text().unwrap()).unwrap()
        else {
            panic!("map request")
        };
        let Request::Map { query } = *request else {
            panic!("automatic map query")
        };
        assert_eq!(query.context, initial.context);
        assert!(query.cursor.is_none());
        queried.send(()).unwrap();
        released.await.unwrap();
        let cell = |x, key: &str| RememberedMapCell {
            object_presentation: None,
            key: key.into(),
            position: Position {
                x,
                y: query.window.min.y,
                z: query.window.min.z,
            },
            wall: false,
            material: "stone".into(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            trap: None,
            door: None,
            ground_items: vec![],
        };
        let mut page = MapPage {
            transfer: "hydration".into(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            base: query.base.clone(),
            window: query.window,
            start: 0,
            page: 0,
            total_cells: 2,
            complete: true,
            ambiguous: false,
            limited: false,
            cells: vec![cell(query.window.min.x, "hydrated-first")],
            next: Some(MapPageCursor {
                transfer: "hydration".into(),
                next: 1,
                page: 1,
            }),
        };
        send(
            &mut socket,
            ServerMessage::MapPage {
                context: initial.reply_context(),
                request_id,
                page: Box::new(page.clone()),
            },
        )
        .await;
        let packet = socket.next().await.unwrap().unwrap();
        let ClientMessage::Request {
            request_id,
            request,
        } = serde_json::from_str(packet.to_text().unwrap()).unwrap()
        else {
            panic!("continuation")
        };
        let Request::Map { query: next } = *request else {
            panic!("map continuation")
        };
        assert_eq!(
            next.cursor, page.next,
            "canceled next must not resend its first query"
        );
        page.start = 1;
        page.page = 1;
        page.cells = vec![cell(query.window.min.x + 1, "hydrated-second")];
        page.next = None;
        send(
            &mut socket,
            ServerMessage::MapPage {
                context: initial.reply_context(),
                request_id,
                page: Box::new(page),
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "test".into(), expected.actor, "headless")
        .await
        .unwrap();
    tokio::select! {
        value = client.next() => panic!("no map page released yet: {value:?}"),
        seen = query_seen => seen.unwrap(),
    }
    assert!(client.is_synchronized());
    assert_eq!(client.state.reset_snapshot().unwrap(), expected);
    assert!(client.state.map_hydration().pending);
    release.send(()).unwrap();
    let first = client.next().await.unwrap();
    assert!(matches!(first,ServerMessage::MapPage{page,..} if page.next.is_some()));
    assert!(client
        .state
        .memory()
        .all(|cell| cell.key != "hydrated-first"));
    let last = client.next().await.unwrap();
    assert!(matches!(last,ServerMessage::MapPage{page,..} if page.next.is_none()));
    assert!(client.state.map_hydration().complete);
    assert!(client
        .state
        .memory()
        .any(|cell| cell.key == "hydrated-second"));
    assert_eq!(client.state.reset_snapshot().unwrap(), expected);
    server.await.unwrap();
}

#[tokio::test]
async fn manual_map_request_is_rejected_without_sending_or_replacing_automatic_state() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let actor = initial.actor;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![actor],
                role: AccessRole::Spectator,
            },
        )
        .await;
        receive(&mut socket).await;
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        assert!(
            !matches!(socket.next().await, Some(Ok(Message::Text(_)))),
            "manual request must not reach the wire"
        );
    });
    let mut client = Connection::connect(address, "test".into(), actor, "headless")
        .await
        .unwrap();
    let before = client.state.clone();
    let query = MapQuery {
        context: client.state.context().clone(),
        branch: client.state.branch().clone(),
        base: client.state.observation_base(),
        window: MapWindow::normal(),
        cursor: None,
    };
    assert_eq!(
        client
            .request(Request::Map { query })
            .await
            .unwrap_err()
            .to_string(),
        "Map hydration is managed automatically"
    );
    assert_eq!(client.state, before);
    drop(client);
    server.await.unwrap();
}

#[tokio::test]
async fn canceled_map_reply_is_discarded_and_map_error_cannot_cancel_another_context() {
    for late in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let initial = snapshot();
        let actor = initial.actor;
        let (queried, seen) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![actor],
                    role: AccessRole::Spectator,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            let packet = socket.next().await.unwrap().unwrap();
            let ClientMessage::Request {
                request_id,
                request,
            } = serde_json::from_str(packet.to_text().unwrap()).unwrap()
            else {
                panic!("query")
            };
            let Request::Map { query } = *request else {
                panic!("map")
            };
            queried.send(()).unwrap();
            released.await.unwrap();
            if late {
                send(
                    &mut socket,
                    ServerMessage::MapPage {
                        context: initial.reply_context(),
                        request_id,
                        page: Box::new(MapPage {
                            transfer: "late".into(),
                            actor,
                            branch: initial.branch.clone(),
                            base: query.base.clone(),
                            window: query.window,
                            start: 0,
                            page: 0,
                            total_cells: 0,
                            complete: true,
                            ambiguous: false,
                            limited: false,
                            cells: vec![],
                            next: None,
                        }),
                    },
                )
                .await;
                send(&mut socket, ServerMessage::Waiting { on: Waiting::You }).await;
            } else {
                let mut context = initial.reply_context();
                context.actor = ActorId(actor.0 + 1);
                send(
                    &mut socket,
                    ServerMessage::Error {
                        scope: ErrorScope::Attached { context },
                        request_id: Some(request_id),
                        code: ErrorCode::InvalidAction,
                        message: "Wrong actor".into(),
                    },
                )
                .await;
            }
        });
        let mut client = Connection::connect(address, "test".into(), actor, "headless")
            .await
            .unwrap();
        tokio::select! { message = client.next() => panic!("reply before release: {message:?}"), ready = seen => ready.unwrap(), }
        let before = visible_model(&client.state);
        if late {
            client.map_request.as_mut().unwrap().0.deadline =
                tokio::time::Instant::now() - Duration::from_millis(1);
        }
        release.send(()).unwrap();
        let result = client.next().await;
        if late {
            assert!(matches!(result.unwrap(), ServerMessage::Waiting { .. }));
            assert!(client.state.map_hydration().failed);
            assert!(client.is_synchronized());
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Invalid map error context"));
            assert!(
                client.state.map_hydration().pending,
                "foreign error cannot mutate the assembly"
            );
        }
        assert_eq!(visible_model(&client.state), before);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn projectile_transport_keeps_pages_private_bounded_and_correlated() {
    for case in [
        "owner",
        "observer",
        "spectator",
        "revoked",
        "oversized",
        "wrong-id",
    ] {
        // Independent current-shape owner fixture; historical wire goldens stay unchanged.
        let mut initial = Snapshot {
            context: StreamContext {
                stream: StreamId("projectile-fixture".into()),
                epoch: 0,
            },
            readiness: Readiness {
                revision: 0,
                admission: true,
                resume: vec![],
                cancel: vec![],
            },
            intentions: vec![],
            travel: None,
            actor: ActorId(1),
            branch: BranchId("main".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            state: Arc::new(StateView {
                snapshot: tor_protocol::SnapshotId("fixture".into()),
                wizard_game: false,
                revision: 1,
                observation: crate::test_support::observation(),
            }),
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        };
        initial.has_control = !matches!(case, "observer" | "spectator");
        initial.readiness.admission = initial.has_control;
        let role = if case == "spectator" {
            AccessRole::Spectator
        } else {
            AccessRole::Player
        };
        let context = initial.reply_context();
        let actor = initial.actor;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "owner".into(),
                    actors: vec![actor],
                    role,
                },
            )
            .await;
            receive(&mut socket).await;
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            let query = ProjectileChoiceQuery {
                ability: "fixture_projectile".into(),
                phase: ProjectileChoicePhase::Sources {},
                offset: 0,
            };
            let id = if matches!(case, "observer" | "spectator") {
                "unsolicited".into()
            } else {
                let ClientMessage::Request {
                    request_id,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("query")
                };
                assert!(matches!(
                    *request,
                    Request::Command {
                        command: Command::ProjectileChoices { .. },
                        ..
                    }
                ));
                request_id
            };
            if case == "revoked" {
                send(
                    &mut socket,
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: initial.context,
                            actor,
                            branch: initial.branch,
                            cursor: StreamCursor {
                                sequence: initial.cursor.sequence + 1,
                                tick: initial.cursor.tick,
                            },
                            body: UpdateBody::Control { has_control: false },
                        }),
                    },
                )
                .await;
            }
            let reply = ServerMessage::ProjectileChoices {
                context,
                request_id: if case == "wrong-id" {
                    "other".into()
                } else {
                    id
                },
                page: Box::new(ProjectileChoicePage {
                    query,
                    choices: vec![],
                    next_offset: Some(128),
                }),
            };
            if case == "oversized" {
                let text = format!(
                    "{}{}",
                    serde_json::to_string(&reply).unwrap(),
                    " ".repeat(MAX_PROJECTILE_CHOICE_PAGE_BYTES)
                );
                socket.send(Message::Text(text.into())).await.unwrap();
            } else {
                send(&mut socket, reply).await;
            }
        });
        let mut client = Connection::connect(address, "token".into(), actor, "text")
            .await
            .unwrap();
        let before = client.state.session().clone();
        let id = if matches!(case, "observer" | "spectator") {
            "unsolicited".into()
        } else {
            client
                .request(client.state.command_request(Command::ProjectileChoices {
                    expected_revision: client.state.state().revision,
                    query: ProjectileChoiceQuery {
                        ability: "fixture_projectile".into(),
                        phase: ProjectileChoicePhase::Sources {},
                        offset: 0,
                    },
                }))
                .await
                .unwrap()
        };
        if case == "revoked" {
            assert!(matches!(
                client.next().await.unwrap(),
                ServerMessage::Update { .. }
            ));
            assert!(!client.state.has_control());
        }
        match case {
            "owner" | "wrong-id" => {
                let reply = client.next().await.unwrap();
                assert!(matches!(reply, ServerMessage::ProjectileChoices { .. }));
                let mut pending = crate::PendingRequest::new(id);
                let completed = pending.observe(&client, &reply).is_some();
                assert_eq!(completed, case == "owner");
                assert_eq!(client.state.session().clone(), before);
            }
            "oversized" => {
                assert!(client.next().await.is_err());
                assert_eq!(client.state.session().clone(), before);
            }
            _ => {
                let error = client.next().await.unwrap_err().to_string();
                assert!(error.contains("without current control"), "{case}: {error}");
            }
        }
        server.await.unwrap();
    }
}

// Exercise the actual stderr payload constructor through JSON serialization;
// no socket, wall-clock ordering assumption, or copied diagnostic encoder.
#[test]
fn timing_request_diagnostics_serialize_actual_request_classification() {
    let baseline = timing_record(
        "client_request",
        "measured",
        ActorId(1),
        17,
        None,
        0.5,
        None,
    );
    assert_eq!(baseline["timing_version"], 1);
    assert_eq!(baseline["actor"], 1);
    assert_eq!(baseline["revision"], 17);
    assert_eq!(baseline["request_id"], "measured");
    assert_eq!(baseline["duration_ms"], serde_json::Value::Null);
    assert_eq!(baseline["previous_timing_write_ms"], 0.5);
    assert!(baseline["unix_ns"].as_u64().is_some());
    assert_eq!(baseline.as_object().unwrap().len(), 8);

    let state = ClientState::from_snapshot(snapshot()).unwrap();
    let action = Request::Command {
        context: state.input_context(),
        branch: state.branch().clone(),
        command: Command::Act {
            action: Action::Wait,
        },
    };
    for (request, expected) in [
        (action, "act"),
        (
            Request::Command {
                context: state.input_context(),
                branch: state.branch().clone(),
                command: Command::ResumeIntention {
                    intention: IntentionId("pending-intention".into()),
                },
            },
            "non_action",
        ),
        (Request::AcquireControl, "non_action"),
        (Request::Snapshot, "non_action"),
        (Request::Save, "non_action"),
    ] {
        let record = timing_record(
            "client_request",
            "measured",
            ActorId(1),
            17,
            None,
            0.5,
            Some(&request),
        );
        let encoded = serde_json::to_string(&record).unwrap();
        let decoded: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded["request_kind"], expected);
        assert_eq!(decoded["event"], "client_request");
        assert_eq!(decoded["request_id"], baseline["request_id"]);
        assert_eq!(decoded["revision"], baseline["revision"]);
        assert_eq!(decoded.as_object().unwrap().len(), 9);
    }
}

#[test]
fn timing_non_request_diagnostics_keep_existing_payload() {
    for event in ["client_request_sent", "client_ack"] {
        let record = timing_record(
            event,
            "measured",
            ActorId(1),
            17,
            Some(1.25),
            0.5,
            Some(&Request::AcquireControl),
        );
        let decoded: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(decoded["event"], event);
        assert_eq!(decoded["duration_ms"], 1.25);
        assert!(decoded.get("request_kind").is_none());
        assert_eq!(decoded.as_object().unwrap().len(), 8);
    }
}

#[tokio::test]
async fn chart_continuations_are_pure_atomic_and_old_generation_pages_do_not_restore_current_rows()
{
    use tor_protocol::chart_pages::*;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut initial = snapshot();
    Arc::make_mut(&mut initial.state).observation.charts = vec![ChartSummary {
        supported: true,
        kind: ChartKind::Monsters,
        mode: ChartMode::Live,
        status: ChartStatus::Available,
        acquired_at: Some(initial.cursor.tick),
        chart: Some("sample".into()),
        latest_attempt: None,
    }];
    initial.state.validate().unwrap();
    let expected = initial.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Hello { .. }
        ));
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Player,
            },
        )
        .await;
        assert!(
            matches!(receive(&mut socket).await, ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. }))
        );
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id: first_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("chart request")
        };
        let Request::Chart { query: first } = *request else {
            panic!("not a pure chart request")
        };
        assert_eq!(first.cursor, None);
        let prefix = ChartPage {
            context: first.context.clone(),
            branch: first.branch.clone(),
            base: first.base.clone(),
            chart: first.chart.clone(),
            kind: ChartKind::Monsters,
            mode: ChartMode::Live,
            acquired_at: initial.cursor.tick,
            total_components: 1,
            total_records: 3,
            start: 0,
            records: vec![ChartRecord::Component(ChartComponent {
                component: 0,
                alignment: ChartAlignment::Aligned,
            })],
            next: Some(1),
        };
        let old = ServerMessage::ChartPage {
            context: initial.reply_context(),
            request_id: first_id,
            page: Box::new(prefix),
        };
        send(&mut socket, old.clone()).await;
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("continuation")
        };
        let Request::Chart { query } = *request else {
            panic!("continuation must remain pure query")
        };
        assert_eq!(query.cursor, Some(1));
        assert_eq!(query.chart, first.chart);
        assert_eq!(query.base, first.base);
        send(&mut socket, old).await; // delayed duplicate must be ignored, not published
        let page = ChartPage {
            context: query.context,
            branch: query.branch,
            base: query.base.clone(),
            chart: query.chart,
            kind: ChartKind::Monsters,
            mode: ChartMode::Live,
            acquired_at: initial.cursor.tick,
            total_components: 1,
            total_records: 3,
            start: 1,
            records: vec![
                ChartRecord::Row(ChartRow {
                    component: 0,
                    position: Some([4, -2, 8]),
                    mark: ChartMark::Monster {
                        name: "".into(),
                        asset: None
                    }
                });
                2
            ],
            next: None,
        };
        send(
            &mut socket,
            ServerMessage::ChartPage {
                context: initial.reply_context(),
                request_id,
                page: Box::new(page),
            },
        )
        .await;
        let mut next = (*initial.state).clone();
        next.revision += 1;
        next.snapshot = tor_protocol::SnapshotId(format!("fixture-{}", next.revision));
        next.observation.tick += 1;
        next.observation.charts[0].status = ChartStatus::Inactive;
        next.observation.charts[0].acquired_at = None;
        next.observation.charts[0].chart = None;
        send(
            &mut socket,
            ServerMessage::Update {
                update: Box::new(StreamUpdate {
                    context: initial.context,
                    actor: initial.actor,
                    branch: initial.branch,
                    cursor: StreamCursor {
                        sequence: initial.cursor.sequence + 1,
                        tick: next.observation.tick,
                    },
                    body: UpdateBody::Observation {
                        state: Arc::new(next),
                        event: None,
                    },
                }),
            },
        )
        .await;
    });
    let mut connection = Connection::connect(address, "token".into(), expected.actor, "chart-test")
        .await
        .unwrap();
    let before = connection.state.session().clone();
    let request = connection
        .charts
        .request(
            crate::charts::ChartInput::Select(ChartKind::Monsters),
            &connection.state,
        )
        .unwrap()
        .unwrap();
    connection.request(request).await.unwrap();
    assert!(
        matches!(connection.next().await.unwrap(), ServerMessage::ChartPage { page, .. } if page.next.is_some())
    );
    assert!(connection.charts.display().is_none());
    assert_eq!(connection.state.session().clone(), before);
    assert!(
        matches!(connection.next().await.unwrap(), ServerMessage::ChartPage { page, .. } if page.next.is_none())
    );
    assert_eq!(connection.charts.display().unwrap().rows.len(), 2);
    assert!(connection
        .charts
        .lines(&connection.state)
        .join("\n")
        .contains(": creature"));
    assert_eq!(connection.state.session().clone(), before);
    assert!(matches!(
        connection.next().await.unwrap(),
        ServerMessage::Update { .. }
    ));
    assert!(connection.charts.display().is_none());
    assert!(!connection.charts.loading());
    assert!(connection
        .charts
        .lines(&connection.state)
        .join("\n")
        .contains("inactive"));
    server.await.unwrap();
}

/// Independent positive fixture: this transport regression must not wait for
/// a newly recorded protocol golden before it can exercise the write fault.
fn outcome_transport_snapshot() -> Snapshot {
    Snapshot {
        context: StreamContext {
            stream: StreamId("outcome-pump".into()),
            epoch: 0,
        },
        readiness: Readiness {
            revision: 0,
            admission: false,
            resume: vec![],
            cancel: vec![],
        },
        intentions: vec![],
        travel: None,
        actor: ActorId(1),
        branch: BranchId("main".into()),
        cursor: StreamCursor {
            sequence: 0,
            tick: 0,
        },
        state: Arc::new(StateView {
            snapshot: SnapshotId("outcome-pump-initial".into()),
            revision: 1,
            wizard_game: false,
            observation: crate::test_support::observation(),
        }),
        has_control: false,
        history: HistoryPage {
            entries: vec![],
            older_before: None,
        },
    }
}

#[tokio::test]
async fn blocked_observe_flush_keeps_ordered_outcomes_live_and_query_identity_stable() {
    use tokio::io::AsyncReadExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = outcome_transport_snapshot();
    ClientState::from_snapshot(initial.clone()).unwrap();
    let mut previous = initial.state.observation.clone();
    let mut updates = Vec::new();
    for (sequence, hp, visual_changed) in [(1, 12, false), (2, 11, true), (3, 10, true)] {
        let mut current = previous.clone();
        current.tick = sequence;
        current.combat.as_mut().unwrap().hp = hp;
        let facts = OutcomeFacts::capture(&previous, &current, Some(&Event::Waited));
        facts.validate_at(sequence).unwrap();
        updates.push(ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: initial.context.clone(),
                actor: initial.actor,
                branch: initial.branch.clone(),
                cursor: StreamCursor {
                    sequence,
                    tick: sequence,
                },
                body: UpdateBody::Outcome {
                    visual_changed,
                    facts,
                    event: None,
                },
            }),
        });
        previous = current;
    }
    let expected = updates[2].clone();
    let mut picture = initial.state.as_ref().clone();
    picture.revision += 1;
    picture.snapshot = SnapshotId("outcome-pump-final".into());
    picture.observation = previous;
    let final_picture = ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: initial.context.clone(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: 4,
                tick: 3,
            },
            body: UpdateBody::Observation {
                state: Arc::new(picture),
                event: None,
            },
        }),
    };
    let (release_outcome, await_outcome) = tokio::sync::oneshot::channel();
    let (release_padding, await_padding) = tokio::sync::oneshot::channel::<usize>();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut socket).await,
            ClientMessage::Hello { .. }
        ));
        send(
            &mut socket,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![initial.actor],
                role: AccessRole::Spectator,
            },
        )
        .await;
        assert!(
            matches!(receive(&mut socket).await, ClientMessage::Request { request, .. }
            if matches!(*request, Request::Attach { .. }))
        );
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        send(&mut socket, updates.remove(0)).await;
        send(&mut socket, updates.remove(0)).await;
        await_outcome.await.unwrap();
        send(&mut socket, updates.remove(0)).await;
        // Do not drain the client's writes until it has consumed the Outcome.
        let mut remaining = await_padding.await.unwrap();
        let mut buffer = vec![0; 65536];
        while remaining > 0 {
            let size = remaining.min(buffer.len());
            socket
                .get_mut()
                .read_exact(&mut buffer[..size])
                .await
                .unwrap();
            remaining -= size;
        }
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut socket).await
        else {
            panic!("expected the one queued Observe");
        };
        assert!(matches!(*request, Request::Observe));
        send(&mut socket, final_picture).await;
        // A later ordinary write is a barrier behind any accidentally duplicated
        // Observe frame, making exactly-once queueing an explicit assertion.
        assert!(
            matches!(receive(&mut socket).await, ClientMessage::Request { request, .. }
            if matches!(*request, Request::AcquireControl))
        );
        request_id
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    client.palette.apply(&PaletteUpdate {
        revision: 0,
        body: PaletteBody::Full {
            assets: Default::default(),
        },
    });
    // This witness isolates Observe from optional map hydration. Reject its
    // unsent query through the normal navigator path; map flush has its own test.
    client.prepare_map_request().unwrap();
    if let Some((pending, _)) = client.map_request.take() {
        assert!(client.state.reject_map_query(&pending.request_id));
    }
    let causal_only = timeout(Duration::from_secs(2), client.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(causal_only, ServerMessage::Update { update }
        if matches!(update.body, UpdateBody::Outcome { visual_changed: false, .. })));
    assert!(!client.observation_demand.dirty);
    client.prepare_observation_request().unwrap();
    assert!(
        client.observation_demand.pending.is_none(),
        "causal-only events do not request pictures"
    );
    client.next().await.unwrap();
    assert!(client.observation_demand.dirty);
    let MaybeTlsStream::Plain(stream) = client.socket.get_ref() else {
        panic!("plain loopback TCP")
    };
    let padding = vec![0; 65536];
    let mut written = 0;
    loop {
        match stream.try_write(&padding) {
            Ok(bytes) => written += bytes,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("padding write failed: {error}"),
        }
        assert!(
            written < 32 * 1024 * 1024,
            "test pressure must stay bounded"
        );
    }
    assert!(timeout(Duration::from_millis(50), client.next())
        .await
        .is_err());
    let pending = client.observation_demand.pending.as_ref().unwrap();
    assert!(
        pending.phase == QueryPhase::Flush,
        "Observe must really be blocked in flush"
    );
    let identity = pending.request_id.clone();
    // An explicit reader joins the canceled automatic flush, not a second
    // request. Its completion belongs to the picture, never a causal Outcome.
    assert_eq!(client.request(Request::Observe).await.unwrap(), identity);
    let mut explicit = crate::PendingRequest::new(identity.clone());
    let mut unrelated = crate::PendingRequest::new("another-demand".into());
    assert!(timeout(Duration::from_millis(20), client.next())
        .await
        .is_err());
    let pending = client.observation_demand.pending.as_ref().unwrap();
    assert!(pending.phase == QueryPhase::Flush);
    assert_eq!(pending.request_id, identity);
    release_outcome.send(()).unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), client.next())
            .await
            .unwrap()
            .unwrap(),
        expected
    );
    assert!(explicit.observe(&client, &expected).is_none());
    assert_eq!(client.state.cursor().sequence, 3);
    assert_eq!(client.state.outcome_status().unwrap().hp, 10);
    assert_eq!(
        client.state.state().observation.combat.as_ref().unwrap().hp,
        12,
        "semantic authority advances without inventing a new picture"
    );
    assert_eq!(
        client
            .observation_demand
            .pending
            .as_ref()
            .unwrap()
            .request_id,
        identity
    );
    release_padding.send(written).unwrap();
    let picture = timeout(Duration::from_secs(2), client.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(&picture,
        ServerMessage::Update { update } if matches!(update.body, UpdateBody::Observation { .. })));
    assert!(matches!(
        explicit.observe(&client, &picture),
        Some(crate::RequestCompletion::Reply(
            crate::ConfirmedReply::Completed
        ))
    ));
    assert!(unrelated.observe(&client, &picture).is_none());
    assert!(client.observation_demand.pending.is_none());
    assert!(!client.observation_demand.dirty);
    client.request(Request::AcquireControl).await.unwrap();
    assert_eq!(server.await.unwrap(), identity);
    client.require_snapshot();
    assert!(!client.completed_observation_request(&identity, &picture));
}
