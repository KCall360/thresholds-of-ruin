use crate::engine::valid_label;
use crate::runner::{Mail, Simulation};
use crate::{Account, Service};
use futures_util::{SinkExt, StreamExt};
use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;
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
};
use tor_protocol::*;

const IO_TIMEOUT: Duration = Duration::from_secs(5);

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
    let Simulation { mail, thread } = simulation;
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
                    tasks.spawn(async move {
                        let _permit = permit;
                        connection(socket, mail, accounts).await;
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

async fn connection(socket: TcpStream, mail: mpsc::Sender<Mail>, accounts: Arc<Vec<Account>>) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(16 * 1024))
        .max_frame_size(Some(16 * 1024));
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
    let timing = std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some();
    loop {
        tokio::select! {
            biased;
            _ = client.close.changed() => {
                // Deliver what was queued before the disconnect, such as the
                // reason for it, without waiting for anything new.
                while let Ok(message) = client.messages.try_recv() {
                    let Ok(text) = serde_json::to_string(&message) else { break; };
                    if !matches!(timeout(IO_TIMEOUT, socket.send(Message::Text(text.into()))).await, Ok(Ok(()))) { break; }
                }
                break;
            },
            outgoing = client.messages.recv() => {
                let Some(message) = outgoing else { break; };
                let started = timing.then(std::time::Instant::now);
                let Ok(text) = serde_json::to_string(&message) else { break; };
                if !matches!(timeout(IO_TIMEOUT, socket.send(Message::Text(text.into()))).await, Ok(Ok(()))) { break; }
                if let (Some(started), ServerMessage::Ack { request_id, .. }) = (started, &message) {
                    timing_event("server_ack_sent", client.id, request_id, 0., started.elapsed().as_secs_f64()*1000.);
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(ClientMessage::Request { request_id, request }) => {
                            let started = timing.then(std::time::Instant::now);
                            let request = Mail::Request { client: client.id, request_id, request, started };
                            if mail.send(request).await.is_err() {
                                break;
                            }
                        },
                        _ => { send_error(&mut socket, ErrorCode::InvalidRequest, "Invalid request message").await; break; }
                    },
                    Some(Ok(Message::Ping(_))) => {
                        if !matches!(timeout(IO_TIMEOUT, socket.flush()).await, Ok(Ok(()))) { break; }
                    },
                    Some(Ok(Message::Pong(_))) => {},
                    _ => break,
                }
            }
        }
    }
    let _ = mail.send(Mail::Disconnect(client.id)).await;
    let _ = timeout(IO_TIMEOUT, socket.close(None)).await;
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
        request_id: None,
        code,
        message: message.into(),
    };
    if let Ok(text) = serde_json::to_string(&response) {
        let _ = timeout(IO_TIMEOUT, socket.send(Message::Text(text.into()))).await;
    }
}

// Diagnostic stderr can itself block; timestamps and durations expose that
// boundary without affecting ordinary play. For `server_handled`, `lock_ms` is
// how long the request waited in the simulation's mailbox.
pub(crate) fn timing_event(
    event: &str,
    client: u64,
    request_id: &str,
    lock_ms: f64,
    duration_ms: f64,
) {
    eprintln!(
        "{}",
        serde_json::json!({"timing_version":1,"event":event,"client":client,
        "request_id":request_id,"lock_ms":lock_ms,"duration_ms":duration_ms,
        "unix_ns":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()})
    );
}
