use crate::engine::valid_label;
use crate::{Account, Service};
use futures_util::{SinkExt, StreamExt};
use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore};
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
pub async fn serve(
    listener: TcpListener,
    service: Arc<Mutex<Service>>,
    accounts: Vec<Account>,
    shutdown: impl Future<Output = ()> + Send,
) -> io::Result<()> {
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
    tokio::pin!(shutdown);
    // Delivery pacing only: simulation time still advances solely through actions.
    let mut travel_pump = tokio::time::interval(Duration::from_millis(75));
    travel_pump.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result = loop {
        tokio::select! {
            _ = &mut shutdown => break Ok(()),
            _ = travel_pump.tick() => { let mut session = service.lock().await; session.advance_travel(); session.poll_saves(); },
            accepted = listener.accept() => {
                let (socket, _) = match accepted { Ok(value) => value, Err(error) => break Err(error) };
                if let Ok(permit) = capacity.clone().try_acquire_owned() {
                    let service = service.clone(); let accounts = accounts.clone();
                    tasks.spawn(async move { let _permit = permit; connection(socket, service, accounts).await; });
                }
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {}
        }
    };
    service.lock().await.shutdown();
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let handle = service.lock().await.flush_handle();
    if let Some(handle) = handle {
        tokio::task::spawn_blocking(move || handle.flush())
            .await
            .map_err(io::Error::other)?
            .map_err(io::Error::other)?;
    }
    result
}

async fn connection(socket: TcpStream, service: Arc<Mutex<Service>>, accounts: Arc<Vec<Account>>) {
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
            let connected = {
                let mut service = service.lock().await;
                service.connect(account, frontend)
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
            _ = client.close.changed() => break,
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
                            if timing {
                                let started = std::time::Instant::now();
                                let mut session = service.lock().await;
                                let lock_ms = started.elapsed().as_secs_f64()*1000.;
                                let handle_started = std::time::Instant::now();
                                session.handle(client.id, request_id.clone(), request);
                                let handle_ms = handle_started.elapsed().as_secs_f64()*1000.;
                                drop(session);
                                timing_event("server_handled", client.id, &request_id, lock_ms, handle_ms);
                            } else {
                                service.lock().await.handle(client.id, request_id, request);
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
    service.lock().await.disconnect(client.id);
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

// Emit after releasing the session lock. Diagnostic stderr can itself block;
// timestamps and durations expose that boundary without affecting ordinary play.
fn timing_event(event: &str, client: u64, request_id: &str, lock_ms: f64, duration_ms: f64) {
    eprintln!(
        "{}",
        serde_json::json!({"timing_version":1,"event":event,"client":client,
        "request_id":request_id,"lock_ms":lock_ms,"duration_ms":duration_ms,
        "unix_ns":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()})
    );
}
