use crate::engine::valid_label;
use crate::runner::{Mail, Simulation};
use crate::{Account, Service};
use futures_util::{SinkExt, StreamExt};
use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{timeout, timeout_at, Instant};
use tokio_tungstenite::{
    accept_hdr_async_with_config,
    tungstenite::{
        handshake::server::{
            ErrorResponse, Request as UpgradeRequest, Response as UpgradeResponse,
        },
        protocol::WebSocketConfig,
        Message,
    },
    WebSocketStream,
};
use tor_protocol::*;

const IO_TIMEOUT: Duration = Duration::from_secs(5);

// Field order matters on cancellation: destroy the socket's write buffer before
// releasing the in-flight frame's byte leases.
struct Output<S> {
    socket: WebSocketStream<S>,
    pending: Option<crate::outbound::Frame>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Output<S> {
    async fn send_frame(&mut self, frame: crate::outbound::Frame) -> Result<Option<String>, ()> {
        debug_assert!(self.pending.is_none());
        self.pending = Some(frame);
        // A logical transfer owns one lease and one write deadline. Following
        // parts must not renew the time a slow reader can retain that lease.
        let deadline = Instant::now() + IO_TIMEOUT;
        loop {
            let text = std::mem::take(&mut self.pending.as_mut().unwrap().text);
            if !matches!(
                timeout_at(deadline, self.socket.send(Message::Text(text.into()))).await,
                Ok(Ok(()))
            ) {
                return Err(());
            }
            let pending = self.pending.as_mut().unwrap();
            match pending.following.pop_front() {
                Some(text) => pending.text = text,
                None => break,
            }
        }
        // SinkExt::send completes flush; the write buffer no longer owns this
        // frame. On failure/cancellation, Output retains it until socket drop.
        Ok(self.pending.take().unwrap().ack_id.take())
    }
}

/// Native clients authenticate over a loopback-only WebSocket listener.
/// Remote deployments and browser origins are deliberately not enabled yet.
/// Returns the service once shut down and flushed.
pub async fn serve(
    listener: TcpListener,
    simulation: Simulation,
    accounts: Vec<Account>,
    shutdown: impl Future<Output = ()> + Send,
) -> io::Result<Service> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Only loopback listeners are supported",
        ));
    }
    if accounts.is_empty()
        || accounts
            .iter()
            .any(|a| !valid_label(&a.user) || a.token.is_empty())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid account configuration",
        ));
    }
    for (index, account) in accounts.iter().enumerate() {
        if accounts[..index]
            .iter()
            .any(|prior| prior.token == account.token)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Duplicate authentication token",
            ));
        }
    }
    let accounts = Arc::new(accounts);
    let capacity = Arc::new(Semaphore::new(128));
    let mut tasks = JoinSet::new();
    let Simulation {
        mail,
        thread,
        diagnostics,
    } = simulation;
    let timing = diagnostics.filter(|_| std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some());
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break Ok(()),
            accepted = listener.accept() => {
                let (socket, _) = match accepted { Ok(value) => value, Err(error) => break Err(error) };
                if let Ok(permit) = capacity.clone().try_acquire_owned() {
                    let mail = mail.clone();
                    let accounts = accounts.clone();
                    let timing = timing.clone();
                    tasks.spawn(async move {
                        let _permit = permit;
                        connection(socket, mail, accounts, timing).await;
                    });
                }
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {}
        }
    };
    let (reply, stopped) = oneshot::channel();
    let handle = if mail.send(Mail::Shutdown(reply)).await.is_ok() {
        stopped.await.ok().flatten()
    } else {
        None
    };
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    drop(mail);
    let service = tokio::task::spawn_blocking(move || thread.join())
        .await
        .map_err(io::Error::other)?
        .map_err(|_| io::Error::other("the simulation thread panicked"))?;
    if let Some(handle) = handle {
        tokio::task::spawn_blocking(move || handle.flush())
            .await
            .map_err(io::Error::other)?
            .map_err(io::Error::other)?;
    }
    result.map(|()| service)
}

async fn connection(
    socket: TcpStream,
    mail: mpsc::Sender<Mail>,
    accounts: Arc<Vec<Account>>,
    timing: Option<crate::diagnostics::Diagnostics>,
) {
    let config = WebSocketConfig::default()
        .write_buffer_size(0)
        .max_write_buffer_size(MAX_RESPONSE_BYTES + 64 * 1024)
        .max_message_size(Some(MAX_REQUEST_BYTES))
        .max_frame_size(Some(MAX_REQUEST_BYTES));
    let upgrade = accept_hdr_async_with_config(socket, native_origin, Some(config));
    let Ok(Ok(mut socket)) = timeout(IO_TIMEOUT, upgrade).await else {
        return;
    };
    let Ok(Some(Ok(Message::Text(text)))) = timeout(IO_TIMEOUT, socket.next()).await else {
        return;
    };
    let hello = decode_request(&text);
    let authenticated = match hello {
        Ok(ClientMessage::Hello {
            protocol,
            token,
            frontend,
        }) if protocol == PROTOCOL_VERSION => accounts
            .iter()
            .find(|account| account.token == token)
            .map(|account| (account, frontend))
            .ok_or((ErrorCode::Unauthorized, "Authentication failed")),
        Ok(ClientMessage::Hello { .. }) => {
            Err((ErrorCode::VersionMismatch, "Unsupported protocol version"))
        }
        _ => Err((ErrorCode::InvalidRequest, "Expected a hello message")),
    };
    let mut client = match authenticated {
        Ok((account, frontend)) => {
            let (reply, answer) = oneshot::channel();
            let connect = Mail::Connect {
                account: account.clone(),
                frontend,
                reply,
            };
            if mail.send(connect).await.is_err() {
                return;
            }
            let Ok(connected) = answer.await else {
                return;
            };
            match connected {
                Ok(client) => client,
                Err(error) => {
                    send_error(&mut socket, error.code, &error.message).await;
                    return;
                }
            }
        }
        Err((code, message)) => {
            send_error(&mut socket, code, message).await;
            return;
        }
    };
    let mut output = Output {
        socket,
        pending: None,
    };
    loop {
        tokio::select! {
            biased;
            _ = client.close.changed() => {
                // Deliver what was queued before the disconnect, such as the
                // reason for it, without waiting for anything new.
                while let Ok(frame) = client.messages.try_recv_frame() {
                    if output.send_frame(frame).await.is_err() { break; }
                }
                break;
            },
            outgoing = client.messages.recv_frame() => {
                let Some(frame) = outgoing else { break; };
                let started = timing.as_ref().map(|_| std::time::Instant::now());
                let Ok(ack_id) = output.send_frame(frame).await else { break; };
                if let (Some(started), Some(timing), Some(request_id)) = (started, &timing, &ack_id) {
                    timing.timing("server_ack_sent", client.id, request_id, 0., started.elapsed().as_secs_f64()*1000.);
                }
            }
            incoming = output.socket.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => match decode_request(&text) {
                        Ok(ClientMessage::Request { request_id, request }) => {
                            let timing = timing.as_ref().map(|diagnostics| crate::diagnostics::RequestTiming {
                                started: std::time::Instant::now(), diagnostics: diagnostics.clone(),
                            });
                            let request = Mail::Request { client: client.id, request_id, request, timing };
                            if mail.send(request).await.is_err() {
                                break;
                            }
                        },
                        _ => { send_error(&mut output.socket, ErrorCode::InvalidRequest, "Invalid request message").await; break; }
                    },
                    Some(Ok(Message::Ping(_))) => {
                        if !matches!(timeout(IO_TIMEOUT, output.socket.flush()).await, Ok(Ok(()))) { break; }
                    },
                    Some(Ok(Message::Pong(_))) => {},
                    _ => break,
                }
            }
        }
    }
    drop(client.messages);
    let _ = mail.send(Mail::Disconnect(client.id)).await;
    let _ = timeout(IO_TIMEOUT, output.socket.close(None)).await;
    // A failed send can leave its bytes in Tungstenite's write buffer. Keep the
    // lease until that buffer is gone, including close errors and timeouts.
    drop(output);
}

// Tungstenite's handshake callback requires this concrete (unboxed) error type.
#[allow(clippy::result_large_err)]
fn native_origin(
    request: &UpgradeRequest,
    response: UpgradeResponse,
) -> Result<UpgradeResponse, ErrorResponse> {
    if request.headers().contains_key("origin") {
        Err(tokio_tungstenite::tungstenite::http::Response::builder()
            .status(403)
            .body(Some("Native clients only".into()))
            .expect("valid response"))
    } else {
        Ok(response)
    }
}

async fn send_error(
    socket: &mut tokio_tungstenite::WebSocketStream<TcpStream>,
    code: ErrorCode,
    message: &str,
) {
    let response = ServerMessage::Error {
        scope: tor_protocol::ErrorScope::Transport {},
        request_id: None,
        code,
        message: message.into(),
    };
    if let Ok(text) = serde_json::to_string(&response) {
        let _ = timeout(IO_TIMEOUT, socket.send(Message::Text(text.into()))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll};
    use tokio::io::ReadBuf;
    use tokio_tungstenite::tungstenite::protocol::Role;

    struct Blocked {
        pool: Arc<crate::outbound::Pool>,
        destroyed_while_charged: Arc<AtomicBool>,
        entered: Arc<tokio::sync::Notify>,
        fail: bool,
        writes_before_block: usize,
    }

    impl Drop for Blocked {
        fn drop(&mut self) {
            self.destroyed_while_charged
                .store(self.pool.available_bytes() == 0, Ordering::SeqCst);
        }
    }

    impl AsyncRead for Blocked {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    impl AsyncWrite for Blocked {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.writes_before_block > 0 {
                self.writes_before_block -= 1;
                return Poll::Ready(Ok(bytes.len()));
            }
            self.entered.notify_one();
            if self.fail {
                Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
            } else {
                Poll::Pending
            }
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    async fn blocked(
        fail: bool,
    ) -> (
        Output<Blocked>,
        crate::outbound::Frame,
        Arc<crate::outbound::Pool>,
        Arc<AtomicBool>,
        Arc<tokio::sync::Notify>,
    ) {
        let message = ServerMessage::Ack {
            context: ReplyContext {
                input: InputContext {
                    stream: StreamContext {
                        stream: StreamId("transport-test".into()),
                        epoch: 0,
                    },
                    readiness_revision: 0,
                },
                actor: ActorId(1),
                branch: BranchId("transport-test".into()),
                cursor: StreamCursor {
                    sequence: 0,
                    tick: 0,
                },
                revision: 0,
            },
            request_id: "inflight".into(),
            receipt: tor_protocol::RequestReceipt::Immediate {
                actor: tor_protocol::ActorId(1),
                branch: tor_protocol::BranchId("transport-test".into()),
                entry_id: None,
            },
        };
        let bytes = serde_json::to_vec(&message).unwrap().len();
        let pool = Arc::new(crate::outbound::Pool::new(crate::OutboundLimits {
            frame_bytes: bytes,
            client_bytes: bytes,
            total_bytes: bytes,
        }));
        let (sender, mut receiver) = pool.channel(16).unwrap();
        sender.try_send(message).unwrap();
        let frame = receiver.try_recv_frame().unwrap();
        let destroyed = Arc::new(AtomicBool::new(false));
        let entered = Arc::new(tokio::sync::Notify::new());
        let stream = Blocked {
            pool: pool.clone(),
            destroyed_while_charged: destroyed.clone(),
            entered: entered.clone(),
            fail,
            writes_before_block: 0,
        };
        let socket = WebSocketStream::from_raw_socket(
            stream,
            Role::Server,
            Some(WebSocketConfig::default().write_buffer_size(0)),
        )
        .await;
        (
            Output {
                socket,
                pending: None,
            },
            frame,
            pool,
            destroyed,
            entered,
        )
    }

    #[tokio::test]
    async fn failed_and_canceled_writes_keep_charge_until_socket_destruction() {
        for fail in [false, true] {
            let (mut output, frame, pool, destroyed, _) = blocked(fail).await;
            let total = pool.available_bytes() + frame.text.len();
            let result = timeout(Duration::from_millis(10), output.send_frame(frame)).await;
            if fail {
                assert!(result.unwrap().is_err());
            } else {
                assert!(result.is_err());
            }
            assert_eq!(pool.available_bytes(), 0);
            drop(output);
            assert!(destroyed.load(Ordering::SeqCst));
            assert_eq!(pool.available_bytes(), total);
        }
    }

    #[tokio::test]
    async fn aborting_connection_destroys_socket_before_releasing_inflight_bytes() {
        let (mut output, frame, pool, destroyed, entered) = blocked(false).await;
        let total = frame.text.len();
        let task = tokio::spawn(async move { output.send_frame(frame).await });
        timeout(Duration::from_secs(1), entered.notified())
            .await
            .unwrap();
        assert_eq!(pool.available_bytes(), 0);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(destroyed.load(Ordering::SeqCst));
        assert_eq!(pool.available_bytes(), total);
    }

    #[tokio::test]
    async fn partial_snapshot_writes_keep_the_entire_lease_until_socket_destruction() {
        let message = snapshot_message();
        let parts = encode_snapshot(&message, 512, tor_protocol::MAX_SNAPSHOT_BYTES * 2).unwrap();
        assert!(parts.len() > 2);
        let count = parts.len();
        let bytes: usize = parts.iter().map(String::len).sum();
        for fail in [false, true] {
            let pool = Arc::new(crate::outbound::Pool::new(crate::OutboundLimits {
                frame_bytes: 512,
                client_bytes: bytes,
                total_bytes: bytes,
            }));
            let (sender, mut receiver) = pool.channel(1).unwrap();
            sender.try_send_snapshot(&message).unwrap();
            let frame = receiver.try_recv_frame().unwrap();
            let destroyed = Arc::new(AtomicBool::new(false));
            let stream = Blocked {
                pool: pool.clone(),
                destroyed_while_charged: destroyed.clone(),
                entered: Arc::new(tokio::sync::Notify::new()),
                fail,
                writes_before_block: 1,
            };
            let socket = WebSocketStream::from_raw_socket(
                stream,
                Role::Server,
                Some(WebSocketConfig::default().write_buffer_size(0)),
            )
            .await;
            let mut output = Output {
                socket,
                pending: None,
            };
            let result = timeout(Duration::from_millis(10), output.send_frame(frame)).await;
            if fail {
                assert!(result.unwrap().is_err());
            } else {
                assert!(result.is_err());
            }
            assert_eq!(
                output.pending.as_ref().unwrap().following.len(),
                count - 2,
                "the first part flushed before failure/cancellation on the second"
            );
            assert_eq!(
                pool.available_bytes(),
                0,
                "all admitted parts remain charged"
            );
            drop(output);
            assert!(
                destroyed.load(Ordering::SeqCst),
                "destroy socket before releasing the lease"
            );
            assert_eq!(pool.available_bytes(), bytes);
        }
    }

    fn snapshot_message() -> ServerMessage {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../../protocol/tests/fixtures/wire-v30.json"))
                .unwrap();
        serde_json::from_value(
            samples["server"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["type"] == "snapshot")
                .unwrap()
                .clone(),
        )
        .unwrap()
    }

    struct DelayedWrites {
        delay: Pin<Box<tokio::time::Sleep>>,
    }

    impl AsyncRead for DelayedWrites {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    impl AsyncWrite for DelayedWrites {
        fn poll_write(
            mut self: Pin<&mut Self>,
            context: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.delay.as_mut().poll(context).is_pending() {
                return Poll::Pending;
            }
            self.delay
                .as_mut()
                .reset(tokio::time::Instant::now() + Duration::from_secs(2));
            Poll::Ready(Ok(bytes.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test(start_paused = true)]
    async fn snapshot_parts_share_one_write_deadline() {
        let pool = crate::outbound::Pool::new(crate::OutboundLimits {
            frame_bytes: 512,
            client_bytes: 65536,
            total_bytes: 65536,
        });
        let (sender, mut receiver) = pool.channel(1).unwrap();
        sender.try_send_snapshot(&snapshot_message()).unwrap();
        let frame = receiver.try_recv_frame().unwrap();
        assert!(frame.following.len() >= 2);
        let charged = pool.available_bytes();
        let socket = WebSocketStream::from_raw_socket(
            DelayedWrites {
                delay: Box::pin(tokio::time::sleep(Duration::from_secs(2))),
            },
            Role::Server,
            Some(WebSocketConfig::default().write_buffer_size(0)),
        )
        .await;
        let mut output = Output {
            socket,
            pending: None,
        };
        let started = tokio::time::Instant::now();
        assert!(
            output.send_frame(frame).await.is_err(),
            "parts must not renew the transfer deadline"
        );
        assert_eq!(started.elapsed(), IO_TIMEOUT);
        assert_eq!(
            pool.available_bytes(),
            charged,
            "timed-out transfer remains leased until socket destruction"
        );
        drop(output);
        assert_eq!(pool.available_bytes(), 65536);
    }
}
