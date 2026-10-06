//! Shared transport recovery against a deliberately inconsistent server stream.
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    time::timeout,
};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tor_client_common::Connection;
use tor_protocol::*;

type Server = WebSocketStream<TcpStream>;
async fn receive(server: &mut Server) -> ClientMessage {
    let message = timeout(Duration::from_secs(2), server.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
async fn send(server: &mut Server, message: ServerMessage) {
    server
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

async fn attach_scripted(server: &mut Server, initial: &Snapshot) {
    assert!(matches!(receive(server).await, ClientMessage::Hello { .. }));
    send(
        server,
        ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            user: "test".into(),
            actors: vec![initial.actor],
            role: AccessRole::Player,
        },
    )
    .await;
    assert!(matches!(
        receive(server).await,
        ClientMessage::Request {
            request: Request::Attach { .. },
            ..
        }
    ));
    send(
        server,
        ServerMessage::Snapshot {
            request_id: "attach".into(),
            snapshot: Box::new(initial.clone()),
        },
    )
    .await;
}

#[tokio::test]
async fn an_attached_connection_does_not_treat_unscoped_errors_as_host_outcomes() {
    for transport in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut server = accept_async(stream).await.unwrap();
            attach_scripted(&mut server, &super::validation::snapshot(0)).await;
            send(
                &mut server,
                ServerMessage::Error {
                    scope: if transport {
                        ErrorScope::Transport {}
                    } else {
                        ErrorScope::Unattached {}
                    },
                    request_id: None,
                    code: ErrorCode::InvalidRequest,
                    message: "Invalid frame".into(),
                },
            )
            .await;
            if let Some(Ok(Message::Text(text))) = server.next().await {
                panic!("unscoped error must not send host requests: {text}")
            }
        });
        let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
            .await
            .unwrap();
        let before = client.state.clone();
        let error = client.next().await.unwrap_err();
        assert_eq!(
            error.to_string(),
            if transport {
                "Transport error: InvalidRequest: Invalid frame"
            } else {
                "Host error has no current attachment"
            }
        );
        assert_eq!(client.state, before);
        drop(client);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn foreign_reply_identity_fails_even_while_recovering_and_never_confirms_a_receipt() {
    for foreign_actor in [false, true] {
        for recovering in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut server = accept_async(stream).await.unwrap();
                let initial = super::validation::snapshot(0);
                attach_scripted(&mut server, &initial).await;
                if recovering {
                    send(
                        &mut server,
                        ServerMessage::Update {
                            update: Box::new(StreamUpdate {
                                context: initial.context.clone(),
                                actor: initial.actor,
                                branch: initial.branch.clone(),
                                cursor: StreamCursor {
                                    sequence: 2,
                                    tick: 0,
                                },
                                body: UpdateBody::Control { has_control: false },
                            }),
                        },
                    )
                    .await;
                    assert!(matches!(
                        receive(&mut server).await,
                        ClientMessage::Request {
                            request: Request::Snapshot,
                            ..
                        }
                    ));
                }
                let mut context = initial.reply_context();
                if foreign_actor {
                    context.actor = ActorId(99);
                } else {
                    context.input.stream.stream = StreamId("another-attachment".into());
                }
                send(
                    &mut server,
                    ServerMessage::Ack {
                        context,
                        request_id: "unrelated".into(),
                        receipt: RequestReceipt::Immediate {
                            actor: initial.actor,
                            branch: initial.branch,
                            entry_id: None,
                        },
                    },
                )
                .await;
                if let Some(Ok(Message::Text(text))) = server.next().await {
                    panic!("foreign identity must not cause another request: {text}")
                }
            });
            let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
                .await
                .unwrap();
            let before = client.state.clone();
            let error = client.next().await.unwrap_err();
            assert_eq!(
                error.to_string(),
                if foreign_actor {
                    "Reply belongs to another actor"
                } else {
                    "Reply belongs to another attachment"
                }
            );
            assert_eq!(client.state, before);
            drop(client);
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn a_reply_naming_missing_permissions_repairs_before_completing_but_keeps_its_receipt() {
    for rejected in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut server = accept_async(stream).await.unwrap();
            let mut initial = super::validation::snapshot(0);
            attach_scripted(&mut server, &initial).await;
            let ClientMessage::Request {
                request_id,
                request: Request::Save,
            } = receive(&mut server).await
            else {
                panic!("one original save required")
            };
            let mut context = initial.reply_context();
            context.input.readiness_revision += 1;
            send(
                &mut server,
                if rejected {
                    ServerMessage::Error {
                        scope: ErrorScope::Attached { context },
                        request_id: Some(request_id),
                        code: ErrorCode::StorageFailure,
                        message: "Original rejection".into(),
                    }
                } else {
                    ServerMessage::Ack {
                        context,
                        request_id,
                        receipt: RequestReceipt::Immediate {
                            actor: initial.actor,
                            branch: BranchId("original-operation".into()),
                            entry_id: Some(EntryId("original-record".into())),
                        },
                    }
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request: Request::Snapshot,
            } = receive(&mut server).await
            else {
                panic!("one repair snapshot required, never another save")
            };
            initial.context.epoch += 1;
            initial.readiness.revision += 1;
            send(
                &mut server,
                ServerMessage::Snapshot {
                    request_id,
                    snapshot: Box::new(initial),
                },
            )
            .await;
            if let Some(Ok(Message::Text(text))) = server.next().await {
                panic!("input must not be replayed: {text}")
            }
        });
        let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
            .await
            .unwrap();
        let before = client.state.clone();
        let id = client.request(Request::Save).await.unwrap();
        let mut pending = tor_client_common::PendingRequest::new(id);
        let reply = client.next().await.unwrap();
        assert!(
            !client.is_synchronized(),
            "a reply must not install an unseen permission generation"
        );
        assert_eq!(client.state, before);
        assert!(pending.observe(&client, &reply).is_none());
        let reset = client.next().await.unwrap();
        assert!(client.is_recovery_snapshot(&reset));
        let completion = pending.observe(&client, &reset);
        if rejected {
            assert!(
                matches!(completion, Some(tor_client_common::RequestCompletion::Reply(
                tor_client_common::ConfirmedReply::Rejected { code: ErrorCode::StorageFailure, message }
            )) if message == "Original rejection")
            );
        } else {
            assert!(
                matches!(completion, Some(tor_client_common::RequestCompletion::Reply(
                tor_client_common::ConfirmedReply::Receipt(RequestReceipt::Immediate { branch, entry_id: Some(entry), .. })
            )) if branch.0 == "original-operation" && entry.0 == "original-record")
            );
        }
        drop(client);
        server.await.unwrap();
    }
}
#[tokio::test]
async fn stale_query_payloads_are_quarantined_through_repair_even_if_later_headers_match_old_state()
{
    for history in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut server = accept_async(stream).await.unwrap();
            let mut initial = super::validation::snapshot(0);
            attach_scripted(&mut server, &initial).await;
            let mut stale = initial.reply_context();
            stale.cursor.sequence += 1;
            let query = |context| {
                if history {
                    ServerMessage::History {
                        context,
                        request_id: "stale-history".into(),
                        page: HistoryPage {
                            entries: vec![],
                            older_before: None,
                        },
                    }
                } else {
                    ServerMessage::Palette {
                        context,
                        request_id: None,
                        palette: PaletteUpdate {
                            revision: 1,
                            body: PaletteBody::Full {
                                assets: std::collections::BTreeSet::from(
                                    ["untrusted.asset".into()],
                                ),
                            },
                        },
                    }
                }
            };
            send(&mut server, query(stale)).await;
            let ClientMessage::Request {
                request_id,
                request: Request::Snapshot,
            } = receive(&mut server).await
            else {
                panic!("one repair required; query payload must not initiate asset work")
            };
            send(&mut server, query(initial.reply_context())).await;
            initial.context.epoch += 1;
            send(
                &mut server,
                ServerMessage::Snapshot {
                    request_id,
                    snapshot: Box::new(initial),
                },
            )
            .await;
            if let Some(Ok(Message::Text(text))) = server.next().await {
                panic!("quarantined payload must not send work: {text}")
            }
        });
        let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
            .await
            .unwrap();
        let before = client.state.state().clone();
        let message = client.next().await.unwrap();
        assert!(
            client.is_recovery_snapshot(&message),
            "query payload must not be published before recovery: {message:?}"
        );
        assert_eq!(client.state.state(), &before);
        assert_eq!(client.palette.revision(), None);
        assert!(!client.palette.holds("untrusted.asset"));
        drop(client);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn a_sequence_gap_requests_a_snapshot_without_replaying_input_or_losing_pending_receipts() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut server).await,
            ClientMessage::Hello { .. }
        ));
        send(
            &mut server,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                user: "player".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Player,
            },
        )
        .await;
        assert!(matches!(
            receive(&mut server).await,
            ClientMessage::Request {
                request: Request::Attach { actor: ActorId(1) },
                ..
            }
        ));
        let mut initial = super::validation::snapshot(0);
        initial.has_control = true;
        send(
            &mut server,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial.clone()),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id: pending,
            request: Request::Save,
        } = receive(&mut server).await
        else {
            panic!("one original request required")
        };
        send(
            &mut server,
            ServerMessage::Update {
                update: Box::new(StreamUpdate {
                    context: initial.context.clone(),
                    actor: initial.actor,
                    branch: initial.branch.clone(),
                    cursor: StreamCursor {
                        sequence: 2,
                        tick: 0,
                    },
                    body: UpdateBody::Control { has_control: false },
                }),
            },
        )
        .await;
        let ClientMessage::Request {
            request_id: reset,
            request: Request::Snapshot,
        } = receive(&mut server).await
        else {
            panic!("gap must request an explicit snapshot")
        };
        send(
            &mut server,
            ServerMessage::Ack {
                context: initial.reply_context(),
                request_id: pending,
                receipt: RequestReceipt::Immediate {
                    actor: initial.actor,
                    branch: initial.branch.clone(),
                    entry_id: None,
                },
            },
        )
        .await;
        released.await.unwrap();
        // Already queued old-epoch traffic cannot repair or mutate the uncertain model.
        send(
            &mut server,
            ServerMessage::Update {
                update: Box::new(StreamUpdate {
                    context: initial.context.clone(),
                    actor: initial.actor,
                    branch: initial.branch.clone(),
                    cursor: StreamCursor {
                        sequence: 1,
                        tick: 0,
                    },
                    body: UpdateBody::Control { has_control: false },
                }),
            },
        )
        .await;
        let mut fresh = initial;
        fresh.context.epoch += 1;
        fresh.cursor.sequence = 5;
        fresh.has_control = false;
        send(
            &mut server,
            ServerMessage::Snapshot {
                request_id: reset,
                snapshot: Box::new(fresh),
            },
        )
        .await;
        if let Some(Ok(Message::Text(text))) = server.next().await {
            panic!("recovery must not send a duplicate snapshot or gameplay input: {text}");
        }
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    let before = client.state.clone();
    let pending = client.request(Request::Save).await.unwrap();
    let message = client.next().await.unwrap();
    assert!(matches!(message, ServerMessage::Ack { request_id, .. } if request_id == pending));
    assert_eq!(client.state, before);
    // Cancellation while waiting for the reset must preserve the same pending recovery.
    assert!(timeout(Duration::from_millis(20), client.next())
        .await
        .is_err());
    assert!(client
        .request(Request::Command {
            context: client.state.input_context(),
            branch: client.state.branch().clone(),
            command: Command::Act {
                expected_revision: 0,
                action: Action::Wait
            }
        })
        .await
        .is_err());
    assert_eq!(client.state, before);
    release.send(()).unwrap();
    let message = client.next().await.unwrap();
    assert!(client.is_recovery_snapshot(&message));
    assert!(matches!(message, ServerMessage::Snapshot { .. }));
    assert_eq!(client.state.context().epoch, 1);
    assert_eq!(client.state.cursor().sequence, 5);
    assert!(!client.state.has_control());
    drop(client);
    server.await.unwrap();
}

#[tokio::test]
async fn an_invalid_correlated_recovery_snapshot_fails_without_publishing_or_retrying() {
    for mode in [
        "old epoch",
        "foreign attachment",
        "invalid observation",
        "server denial",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut server = accept_async(stream).await.unwrap();
            assert!(matches!(
                receive(&mut server).await,
                ClientMessage::Hello { .. }
            ));
            send(
                &mut server,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    user: "observer".into(),
                    actors: vec![ActorId(1)],
                    role: AccessRole::Spectator,
                },
            )
            .await;
            assert!(matches!(
                receive(&mut server).await,
                ClientMessage::Request {
                    request: Request::Attach { .. },
                    ..
                }
            ));
            let initial = super::validation::snapshot(0);
            send(
                &mut server,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            send(
                &mut server,
                ServerMessage::Update {
                    update: Box::new(StreamUpdate {
                        context: initial.context.clone(),
                        actor: initial.actor,
                        branch: initial.branch.clone(),
                        cursor: StreamCursor {
                            sequence: 2,
                            tick: 0,
                        },
                        body: UpdateBody::Control { has_control: true },
                    }),
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request: Request::Snapshot,
            } = receive(&mut server).await
            else {
                panic!("one recovery snapshot required")
            };
            let mut invalid = initial;
            invalid.context.epoch = 1;
            match mode {
                "old epoch" => invalid.context.epoch = 0,
                "foreign attachment" => {
                    invalid.context.stream = StreamId("foreign-attachment".into())
                }
                "invalid observation" => invalid.state.observation.inventory[0].quantity = 0,
                "server denial" => {}
                _ => unreachable!(),
            }
            if mode == "server denial" {
                send(
                    &mut server,
                    ServerMessage::Error {
                        scope: ErrorScope::Attached {
                            context: invalid.reply_context(),
                        },
                        request_id: Some(request_id),
                        code: ErrorCode::InvalidRequest,
                        message: "Snapshot unavailable".into(),
                    },
                )
                .await;
            } else {
                send(
                    &mut server,
                    ServerMessage::Snapshot {
                        request_id,
                        snapshot: Box::new(invalid),
                    },
                )
                .await;
            }
            if let Some(Ok(Message::Text(text))) = server.next().await {
                panic!("invalid recovery must not create a retry loop: {text}");
            }
        });
        let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
            .await
            .unwrap();
        let before = client.state.clone();
        let error = client.next().await.unwrap_err().to_string();
        assert!(
            error.starts_with(if mode == "server denial" {
                "Stream resynchronization rejected:"
            } else {
                "Invalid resynchronization snapshot:"
            }),
            "{mode}: {error}"
        );
        assert_eq!(client.state, before, "{mode}");
        assert!(!client.is_synchronized());
        assert!(client.request(Request::Snapshot).await.is_err());
        drop(client);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn oversized_outgoing_requests_are_rejected_without_poisoning_the_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        let ClientMessage::Request {
            request_id,
            request: Request::Save,
        } = receive(&mut server).await
        else {
            panic!("oversized request must never reach the peer")
        };
        send(
            &mut server,
            ServerMessage::Ack {
                context: initial.reply_context(),
                request_id,
                receipt: RequestReceipt::Immediate {
                    actor: initial.actor,
                    branch: initial.branch,
                    entry_id: None,
                },
            },
        )
        .await;
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    let before = client.state.clone();
    let request = client.state.command_request(Command::Annotate {
        anchor: Anchor::State { revision: 0 },
        text: "x".repeat(16 * 1024),
        audience: Audience::Actor,
        source: ClientSource::User,
        category: AnnotationCategory::Note,
    });
    assert!(
        client.request(request).await.is_err(),
        "oversized request must fail locally"
    );
    assert!(client.is_synchronized());
    assert_eq!(client.state.state(), before.state());
    let id = client.request(Request::Save).await.unwrap();
    assert!(
        matches!(client.next().await.unwrap(), ServerMessage::Ack { request_id, .. } if request_id == id)
    );
    server.await.unwrap();
}

#[tokio::test]
async fn oversized_fragmented_response_fails_before_deserialization_or_stream_repair() {
    use tokio_tungstenite::tungstenite::protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        let initial = super::validation::snapshot(0);
        attach_scripted(&mut server, &initial).await;
        let text = serde_json::to_vec(&ServerMessage::Error {
            scope: ErrorScope::Attached {
                context: initial.reply_context(),
            },
            request_id: None,
            code: ErrorCode::InvalidAction,
            message: "x".repeat(16 * 1024 * 1024),
        })
        .unwrap();
        let split = 8 * 1024 * 1024;
        server
            .send(Message::Frame(Frame::message(
                text[..split].to_vec(),
                OpCode::Data(Data::Text),
                false,
            )))
            .await
            .unwrap();
        server
            .send(Message::Frame(Frame::message(
                text[split..].to_vec(),
                OpCode::Data(Data::Continue),
                true,
            )))
            .await
            .unwrap();
        // Any snapshot request would turn a capacity violation into unnecessary repair.
        while let Some(Ok(message)) = server.next().await {
            if let Message::Text(text) = message {
                panic!("oversized input must not cause repair: {text}");
            }
            if matches!(message, Message::Close(_)) {
                break;
            }
        }
    });
    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    let before = client.state.clone();
    assert!(
        client.next().await.is_err(),
        "fragmented messages must use the response byte ceiling"
    );
    assert_eq!(client.state.state(), before.state());
    assert_eq!(client.state.context(), before.context());
    assert!(client.is_synchronized());
    drop(client);
    server.await.unwrap();
}
