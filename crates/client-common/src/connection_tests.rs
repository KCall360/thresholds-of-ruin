//! Deadline coverage uses an expired stored deadline, avoiding clock sleeps.
use super::*;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, WebSocketStream};

async fn receive(socket: &mut WebSocketStream<TcpStream>) -> ClientMessage {
    let frame = timeout(Duration::from_secs(2), socket.next())
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
                ClientMessage::Request {
                    request: Request::Attach { .. },
                    ..
                }
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
        let before = client.state.clone();
        client.require_snapshot();
        let recovery = client.recovery.as_mut().unwrap();
        recovery.phase = phase;
        recovery.deadline = tokio::time::Instant::now() - Duration::from_millis(1);
        let error = timeout(Duration::from_secs(1), client.next())
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.to_string(), "Stream resynchronization timed out");
        assert_eq!(client.state, before);
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
    let initial: Snapshot = serde_json::from_value(
        samples["server"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sample| sample["type"] == "snapshot")
            .unwrap()["snapshot"]
            .clone(),
    )
    .unwrap();
    initial
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
        let pong = timeout(Duration::from_secs(2), socket.next())
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
    let before = client.state.clone();
    tokio::select! {
        message = client.next() => panic!("partial transfer escaped: {message:?}"),
        received = received => received.unwrap(),
    }
    assert_eq!(
        client.state, before,
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
    assert_eq!(client.state.snapshot(), expected);
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
                    ClientMessage::Request {
                        request: Request::Attach { .. },
                        ..
                    }
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
                    request: Request::Save,
                } = receive(&mut socket).await
                else {
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
                if let Some(Ok(Message::Text(text))) = socket.next().await {
                    panic!("receipt handling must not replay input: {text}");
                }
            });
            let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
                .await
                .unwrap();
            let before = client.state.clone();
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
                        context: before.reply_context(),
                        request_id: id,
                        receipt: expected
                    }
                );
            }
            assert_eq!(client.state, before);
            drop(client);
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn palette_backpressure_cancellation_does_not_lose_the_applied_observation() {
    use tokio::io::AsyncReadExt;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let initial = snapshot();
    let mut state = Arc::unwrap_or_clone(initial.state.clone());
    state.revision += 1;
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
    let mut context = initial.reply_context();
    context.cursor.sequence += 1;
    context.revision = state.revision;
    let (sent, arrived) = tokio::sync::oneshot::channel();
    let (release, drain) = tokio::sync::oneshot::channel::<usize>();
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
            ClientMessage::Request {
                request: Request::Attach { .. },
                ..
            }
        ));
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        send(&mut socket, update).await;
        sent.send(()).unwrap();
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
        let ClientMessage::Request {
            request_id,
            request: Request::Palette,
        } = receive(&mut socket).await
        else {
            panic!("expected one palette request")
        };
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
            ClientMessage::Request {
                request: Request::Attach { .. },
                ..
            }
        ));
        send(
            &mut socket,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        assert!(
            matches!(
                receive(&mut socket).await,
                ClientMessage::Request {
                    request: Request::Snapshot,
                    ..
                }
            ),
            "oversized command must not be sent before the valid query"
        );
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
    client.request(Request::Snapshot).await.unwrap();
    server.await.unwrap();
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
            ClientMessage::Request {
                request: Request::Attach { .. },
                ..
            }
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
