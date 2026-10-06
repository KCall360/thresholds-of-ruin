use std::{
    net::SocketAddr,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::Duration,
};
use tokio::{sync::mpsc as async_mpsc, time::timeout};
use tor_client_common::Connection;
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;
pub enum Event {
    Role(AccessRole),
    Snapshot(Box<Snapshot>),
    Update(Box<StreamUpdate>),
    Status(String),
    Ready,
    History(HistoryPage),
    Fatal(String),
}

/// What the window asks of the network worker.
pub enum Command {
    Request {
        context: StreamContext,
        request: Request,
    },
    /// Show what has arrived without waiting.
    Skip,
    /// Space shown updates this far apart.
    Pace(Duration),
}

pub struct Network {
    pub commands: async_mpsc::Sender<Command>,
    pub events: Receiver<Event>,
    worker: JoinHandle<Result<(), String>>,
}

impl Network {
    pub fn start(
        address: SocketAddr,
        token: String,
        actor: ActorId,
        observe: bool,
        pace: Duration,
    ) -> Self {
        let (commands, rx) = async_mpsc::channel(4);
        let (tx, events) = mpsc::sync_channel(64);
        let worker = std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| -> Error { e.into() })
                .and_then(|runtime| {
                    runtime.block_on(run(address, token, actor, observe, pace, rx, &tx))
                });
            if let Err(error) = &result {
                let _=tx.try_send(Event::Fatal(format!("Connection ended: {error}. Relaunch to reconnect; inspect history before retrying.")));
            }
            result.map_err(|error| error.to_string())
        });
        Self {
            commands,
            events,
            worker,
        }
    }

    pub fn shutdown(self) -> Result<(), Error> {
        drop(self.commands);
        // Drain presentation while the worker performs save-and-quit.
        for _ in self.events {}
        self.worker
            .join()
            .map_err(|_| "Network worker failed")?
            .map_err(Into::into)
    }
}

// Backpressure stays on this dedicated worker, never the native event loop.
// The server retains its bounded slow-client disconnect/snapshot-on-relaunch
// contract. Do not drop intermediate observations or grow this queue.
fn publish(tx: &SyncSender<Event>, event: Event) -> Result<(), Error> {
    tx.send(event)
        .map_err(|_| "Window stopped consuming network updates".into())
}

async fn run(
    address: SocketAddr,
    token: String,
    actor: ActorId,
    observe: bool,
    pace: Duration,
    mut rx: async_mpsc::Receiver<Command>,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let mut connection = Connection::connect(address, token, actor, "ascii").await?;
    connection.set_pace(pace);
    publish(tx, Event::Role(connection.role()))?;
    publish(tx, Event::Snapshot(Box::new(connection.state.snapshot())))?;
    if connection.role() == AccessRole::Spectator {
        publish(
            tx,
            Event::Status("Spectator access is read-only. F2: history; Esc: close.".into()),
        )?;
    } else if !observe {
        transact(&mut connection, Request::AcquireControl, tx).await?;
    } else {
        publish(
            tx,
            Event::Status("Observing. Press F3 to request control.".into()),
        )?;
    }
    publish(tx, Event::Ready)?;
    loop {
        tokio::select! {
            message=connection.next()=>{
                let message = message?;
                let reset = matches!(message, ServerMessage::Snapshot { .. });
                present(message,tx)?;
                if reset { publish(tx, Event::Ready)?; }
            },
            command=rx.recv()=>match command {
                None => {connection.close().await?;return Ok(());},
                Some(Command::Request { context, request }) => {
                    if !connection.is_synchronized() {
                        publish(tx, Event::Status("Resynchronizing; request was not sent.".into()))?;
                        continue;
                    }
                    let stale_input = matches!(&request, Request::Command { context, .. }
                        if context != &connection.state.input_context());
                    if &context != connection.state.context() || stale_input {
                        publish(tx, Event::Status("State changed; request was not sent. Review the new state.".into()))?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    transact(&mut connection,request,tx).await?;
                    publish(tx,Event::Ready)?;
                },
                Some(Command::Skip) => connection.skip(),
                Some(Command::Pace(pace)) => connection.set_pace(pace),
            },
        }
    }
}

async fn transact(
    connection: &mut Connection,
    request: Request,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let id = connection.request(request).await?;
    let mut pending = tor_client_common::PendingRequest::new(id);
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            let completion = pending.observe(connection, &message);
            let complete = completion.is_some();
            let unknown = matches!(completion, Some(tor_client_common::RequestCompletion::Unknown));
            present(message, tx)?;
            if unknown {
                publish(tx, Event::Status("State resynchronized; the previous request may have completed. Inspect history before retrying.".into()))?;
            }
            if complete { return Ok::<(), Error>(()); }
        }
    })
    .await
    .map_err(|_| "Response timed out; the last request may have committed")??;
    Ok(())
}

fn present(message: ServerMessage, tx: &SyncSender<Event>) -> Result<(), Error> {
    match message {
        ServerMessage::Update { update } => publish(tx, Event::Update(update)),
        ServerMessage::Snapshot { snapshot, .. } => publish(tx, Event::Snapshot(snapshot)),
        ServerMessage::Ack {
            receipt: tor_protocol::RequestReceipt::Admitted { phase, .. },
            ..
        } => publish(tx, Event::Status(format!("Action: {phase:?}."))),
        ServerMessage::Ack {
            receipt: tor_protocol::RequestReceipt::Immediate { .. },
            ..
        } => publish(
            tx,
            Event::Status(
                "Ready. Background saving is enabled; closing normally saves pending play.".into(),
            ),
        ),
        ServerMessage::Error { code, message, .. } => {
            publish(tx, Event::Status(format!("{code:?}: {message}")))
        }
        ServerMessage::History { page, .. } => {
            publish(tx, Event::History(page))?;
            publish(tx, Event::Status("History loaded.".into()))
        }
        ServerMessage::Welcome { .. } => Err("Unexpected repeated welcome".into()),
        // Palettes are drawn with once the window resolves assets.
        ServerMessage::Palette { .. } => Ok(()),
        // Input permissions arrive through ordered readiness updates.
        ServerMessage::Waiting { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_backpressure_retains_order_and_disconnects_cleanly() {
        let (tx, rx) = mpsc::sync_channel(64);
        for n in 0..64 {
            publish(&tx, Event::Status(n.to_string())).unwrap();
        }
        assert!(matches!(
            tx.try_send(Event::Ready),
            Err(mpsc::TrySendError::Full(_))
        ));
        let worker =
            std::thread::spawn(move || publish(&tx, Event::Ready).map_err(|e| e.to_string()));
        for n in 0..64 {
            assert!(
                matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Event::Status(s) if s == n.to_string())
            );
        }
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::Ready
        ));
        assert!(worker.join().unwrap().is_ok());
        let (tx, rx) = mpsc::sync_channel(1);
        drop(rx);
        assert!(publish(&tx, Event::Ready).is_err());
    }

    use futures_util::{SinkExt, StreamExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_request_from_before_a_reset_is_refused_without_closing_the_connection() {
        queued_request_after_context_change(true).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_request_from_before_an_authority_generation_change_is_refused() {
        queued_request_after_context_change(false).await;
    }

    async fn queued_request_after_context_change(reset: bool) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let samples: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                format!("../protocol/tests/fixtures/wire-v{PROTOCOL_VERSION}.json"),
            ))
            .unwrap(),
        )
        .unwrap();
        let mut initial: Snapshot = serde_json::from_value(
            samples["server"]
                .as_array()
                .unwrap()
                .iter()
                .find(|sample| sample["type"] == "snapshot")
                .unwrap()["snapshot"]
                .clone(),
        )
        .unwrap();
        initial.has_control = true;
        let obsolete = initial.context.clone();
        let mut fresh = initial.clone();
        if reset {
            fresh.context.epoch += 1;
        } else {
            fresh.readiness.revision += 1;
        }
        let current = fresh.context.clone();
        let current_generation = fresh.readiness.revision;
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
                    user: "test".into(),
                    actors: vec![ActorId(1)],
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
            let reply_branch = fresh.branch.clone();
            let mut reply_context = fresh.reply_context();
            if !reset {
                reply_context.cursor.sequence += 1;
            }
            send(
                &mut socket,
                if reset {
                    ServerMessage::Snapshot {
                        request_id: "reset".into(),
                        snapshot: Box::new(fresh),
                    }
                } else {
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: fresh.context,
                            actor: fresh.actor,
                            branch: fresh.branch,
                            cursor: StreamCursor {
                                sequence: fresh.cursor.sequence + 1,
                                tick: fresh.cursor.tick,
                            },
                            body: UpdateBody::Readiness {
                                readiness: fresh.readiness,
                            },
                        }),
                    }
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request: Request::History { .. },
            } = receive(&mut socket).await
            else {
                panic!("stale gameplay request must not reach the server")
            };
            send(
                &mut socket,
                ServerMessage::History {
                    context: reply_context.clone(),
                    request_id,
                    page: HistoryPage {
                        entries: vec![],
                        older_before: None,
                    },
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request: Request::Save,
            } = receive(&mut socket).await
            else {
                panic!("connection remains usable and closes with its save barrier")
            };
            send(
                &mut socket,
                ServerMessage::Ack {
                    context: reply_context,
                    request_id,
                    receipt: RequestReceipt::Immediate {
                        actor: ActorId(1),
                        branch: reply_branch,
                        entry_id: None,
                    },
                },
            )
            .await;
        });
        let (commands, rx) = async_mpsc::channel(4);
        let (events, received) = mpsc::sync_channel(64);
        let worker = tokio::spawn(async move {
            run(
                address,
                "token".into(),
                ActorId(1),
                true,
                Duration::ZERO,
                rx,
                &events,
            )
            .await
        });
        loop {
            let event = received.recv_timeout(Duration::from_secs(2)).unwrap();
            let changed = match event {
                Event::Snapshot(snapshot) => reset && snapshot.context == current,
                Event::Update(update) => {
                    !reset
                        && matches!(&update.body,
                    UpdateBody::Readiness { readiness } if readiness.revision == current_generation)
                }
                _ => false,
            };
            if changed {
                break;
            }
        }
        commands
            .send(Command::Request {
                context: obsolete.clone(),
                request: Request::Command {
                    context: InputContext {
                        stream: obsolete,
                        readiness_revision: 0,
                    },
                    branch: BranchId("unused".into()),
                    command: tor_protocol::Command::Act {
                        expected_revision: 0,
                        action: Action::Wait,
                    },
                },
            })
            .await
            .unwrap();
        loop {
            if matches!(received.recv_timeout(Duration::from_secs(2)).unwrap(),
                Event::Status(status) if status.starts_with("State changed; request was not sent"))
            {
                break;
            }
        }
        commands
            .send(Command::Request {
                context: current,
                request: Request::History {
                    before: None,
                    limit: 1,
                },
            })
            .await
            .unwrap();
        loop {
            if matches!(
                received.recv_timeout(Duration::from_secs(2)).unwrap(),
                Event::History(_)
            ) {
                break;
            }
        }
        drop(commands);
        worker.await.unwrap().unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn a_confirmed_reply_during_recovery_is_retained_until_the_snapshot() {
        for rejected in [false, true] {
            let samples: serde_json::Value = serde_json::from_str(
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
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (reply_sent, sent) = tokio::sync::oneshot::channel();
            let (reset_allowed, allowed) = tokio::sync::oneshot::channel();
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
                        snapshot: Box::new(initial.clone()),
                    },
                )
                .await;
                let ClientMessage::Request {
                    request_id: pending,
                    request: Request::Save,
                } = receive(&mut socket).await
                else {
                    panic!("one original save required")
                };
                send(
                    &mut socket,
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: initial.context.clone(),
                            actor: initial.actor,
                            branch: initial.branch.clone(),
                            cursor: StreamCursor {
                                sequence: initial.cursor.sequence + 2,
                                tick: initial.cursor.tick,
                            },
                            body: UpdateBody::Control { has_control: false },
                        }),
                    },
                )
                .await;
                let ClientMessage::Request {
                    request_id: reset,
                    request: Request::Snapshot,
                } = receive(&mut socket).await
                else {
                    panic!("one recovery request required")
                };
                let reply = if rejected {
                    ServerMessage::Error {
                        scope: ErrorScope::Attached {
                            context: initial.reply_context(),
                        },
                        request_id: Some(pending),
                        code: ErrorCode::StorageFailure,
                        message: "Save rejected".into(),
                    }
                } else {
                    ServerMessage::Ack {
                        context: initial.reply_context(),
                        request_id: pending,
                        receipt: RequestReceipt::Immediate {
                            actor: initial.actor,
                            branch: initial.branch.clone(),
                            entry_id: None,
                        },
                    }
                };
                send(&mut socket, reply).await;
                reply_sent.send(()).unwrap();
                allowed.await.unwrap();
                let mut fresh = initial;
                fresh.context.epoch += 1;
                fresh.cursor.sequence += 2;
                send(
                    &mut socket,
                    ServerMessage::Snapshot {
                        request_id: reset,
                        snapshot: Box::new(fresh),
                    },
                )
                .await;
                if let Some(Ok(Message::Text(text))) = socket.next().await {
                    panic!("request must not be replayed: {text}");
                }
            });
            let connection = Connection::connect(address, "token".into(), ActorId(1), "test")
                .await
                .unwrap();
            let (events, received) = mpsc::sync_channel(64);
            let mut worker = tokio::spawn(async move {
                let mut connection = connection;
                transact(&mut connection, Request::Save, &events)
                    .await
                    .unwrap();
                assert!(connection.is_synchronized());
            });
            sent.await.unwrap();
            assert!(
                timeout(Duration::from_millis(20), &mut worker)
                    .await
                    .is_err(),
                "the confirmed reply must wait for a valid stream reset"
            );
            reset_allowed.send(()).unwrap();
            worker.await.unwrap();
            server.await.unwrap();
            let statuses: Vec<_> = received
                .try_iter()
                .filter_map(|event| match event {
                    Event::Status(status) => Some(status),
                    _ => None,
                })
                .collect();
            assert!(
                !statuses
                    .iter()
                    .any(|status| status.contains("previous request may have completed")),
                "confirmed reply became uncertain: {statuses:?}"
            );
            assert!(
                statuses.iter().any(|status| if rejected {
                    status.contains("StorageFailure: Save rejected")
                } else {
                    status.starts_with("Ready.")
                }),
                "original reply missing: {statuses:?}"
            );
        }
    }
}
