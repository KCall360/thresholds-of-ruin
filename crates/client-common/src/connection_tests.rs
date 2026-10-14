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
            request,
        } = receive(&mut socket).await
        else {
            panic!("expected one palette request")
        };
        let Request::Palette = *request else {
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
        assert!(
            matches!(
                receive(&mut socket).await,
                ClientMessage::Request { request, .. } if matches!(*request, Request::Snapshot)
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
        let before = client.state.snapshot();
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
            assert_eq!(client.state.snapshot(), before);
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
            value: AdvancementOverview {
                growth: AdvancementGrowth {
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
                respec: None,
            },
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
            let message =
                owner_advancement_message(context, if case == "wrong_id" { "wrong" } else { &id });
            if case == "oversized" {
                let mut text = serde_json::to_string(&message).unwrap();
                text.push_str(&" ".repeat(MAX_ADVANCEMENT_PAGE_BYTES + 1 - text.len()));
                socket.send(Message::Text(text.into())).await.unwrap();
            } else {
                send(&mut socket, message).await;
            }
            if matches!(case, "owner" | "external") {
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
        let result = client.next().await;
        if matches!(case, "owner" | "external") {
            let message = result.unwrap();
            assert_eq!(
                client.advancement_pages.overview().is_some(),
                case == "owner"
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
