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
use tokio::time::timeout;
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
        let text = std::mem::take(&mut self.pending.as_mut().unwrap().text);
        if !matches!(
            timeout(IO_TIMEOUT, self.socket.send(Message::Text(text.into()))).await,
            Ok(Ok(()))
        ) {
            return Err(());
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
    let hello = serde_json::from_str::<ClientMessage>(&text);
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
                    Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
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
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.entered.notify_one();
            if self.fail {
                Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
            } else {
                Poll::Pending
            }
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
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
        let (sender, mut receiver) = pool.channel(16);
        sender.try_send(message).unwrap();
        let frame = receiver.try_recv_frame().unwrap();
        let destroyed = Arc::new(AtomicBool::new(false));
        let entered = Arc::new(tokio::sync::Notify::new());
        let stream = Blocked {
            pool: pool.clone(),
            destroyed_while_charged: destroyed.clone(),
            entered: entered.clone(),
            fail,
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
}
