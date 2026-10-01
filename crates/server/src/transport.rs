use crate::engine::valid_label;
use crate::{Account, Service};
use futures_util::sink::SinkExt;
use futures_util::{Sink, Stream, StreamExt};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{timeout, Interval};
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

/// What the session loop does when a client frame and the autonomous pump
/// are both due. A frame already read from a socket wins: autonomous work
/// moves the revision and would reject that frame's command as stale.
#[derive(Debug, PartialEq, Eq)]
enum Scheduled {
    Command,
    Pump,
    Idle,
}

fn scheduled_step(already_read: bool, pump_due: bool) -> Scheduled {
    if already_read {
        Scheduled::Command
    } else if pump_due {
        Scheduled::Pump
    } else {
        Scheduled::Idle
    }
}

/// The 75ms autonomous pump. A tick noticed while a command is also buffered
/// stays owed, so the command is handled and the pump runs on the next step
/// instead of being skipped for a whole period.
struct Pace {
    interval: Interval,
    owed: bool,
}

impl Pace {
    fn new() -> Self {
        let mut interval = tokio::time::interval(Duration::from_millis(75));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        Self {
            interval,
            owed: false,
        }
    }

    fn poll_due(&mut self, cx: &mut Context<'_>) -> bool {
        if self.owed {
            return true;
        }
        if self.interval.poll_tick(cx).is_ready() {
            self.owed = true;
            true
        } else {
            false
        }
    }

    fn take(&mut self) {
        self.owed = false;
    }
}

enum Buffered {
    Text(String),
    Closed,
}

struct Slot {
    id: u64,
    socket: WebSocketStream<TcpStream>,
    outbound: mpsc::Receiver<ServerMessage>,
    buffered: Option<Buffered>,
    pending_out: Option<Message>,
    flushing: bool,
    ack_id: Option<String>,
    ack_started: Option<Instant>,
    _permit: OwnedSemaphorePermit,
}

enum IoEvent {
    Request { index: usize, text: String },
    Closed { index: usize },
    Pump,
    Wrote,
}

fn poll_reads(slots: &mut [Slot], cx: &mut Context<'_>) -> Option<IoEvent> {
    for slot in slots.iter_mut() {
        if slot.buffered.is_some() {
            continue;
        }
        match Pin::new(&mut slot.socket).poll_next(cx) {
            Poll::Ready(Some(Ok(Message::Text(text)))) => {
                slot.buffered = Some(Buffered::Text(text.to_string()));
            }
            Poll::Ready(Some(Ok(Message::Ping(_)))) => slot.flushing = true,
            Poll::Ready(Some(Ok(Message::Pong(_)))) => {}
            Poll::Ready(_) => slot.buffered = Some(Buffered::Closed),
            Poll::Pending => {}
        }
    }
    let index = slots.iter().position(|slot| slot.buffered.is_some())?;
    match slots[index].buffered.take() {
        Some(Buffered::Text(text)) => Some(IoEvent::Request { index, text }),
        _ => Some(IoEvent::Closed { index }),
    }
}

fn poll_writes(slots: &mut [Slot], timing: bool, cx: &mut Context<'_>) -> Poll<IoEvent> {
    let mut progress = false;
    let mut closed = None;
    for (index, slot) in slots.iter_mut().enumerate() {
        if slot.pending_out.is_none() && !slot.flushing {
            match slot.outbound.poll_recv(cx) {
                Poll::Ready(Some(message)) => {
                    slot.ack_id = None;
                    slot.ack_started = None;
                    if timing {
                        if let ServerMessage::Ack { request_id, .. } = &message {
                            slot.ack_id = Some(request_id.clone());
                            slot.ack_started = Some(Instant::now());
                        }
                    }
                    match serde_json::to_string(&message) {
                        Ok(text) => slot.pending_out = Some(Message::Text(text.into())),
                        Err(_) => {
                            closed = Some(index);
                            break;
                        }
                    }
                }
                Poll::Ready(None) => {
                    closed = Some(index);
                    break;
                }
                Poll::Pending => continue,
            }
        }
        if let Some(message) = slot.pending_out.take() {
            match Pin::new(&mut slot.socket).poll_ready(cx) {
                Poll::Ready(Ok(())) => {
                    if Pin::new(&mut slot.socket).start_send(message).is_err() {
                        closed = Some(index);
                        break;
                    }
                    slot.flushing = true;
                }
                Poll::Ready(Err(_)) => {
                    closed = Some(index);
                    break;
                }
                Poll::Pending => {
                    slot.pending_out = Some(message);
                    continue;
                }
            }
        }
        if slot.flushing {
            match Pin::new(&mut slot.socket).poll_flush(cx) {
                Poll::Ready(Ok(())) => {
                    slot.flushing = false;
                    if let Some(request_id) = slot.ack_id.take() {
                        let elapsed = slot
                            .ack_started
                            .take()
                            .map_or(0., |started| started.elapsed().as_secs_f64() * 1000.);
                        timing_event("server_ack_sent", slot.id, &request_id, 0., elapsed);
                    }
                    progress = true;
                }
                Poll::Ready(Err(_)) => {
                    closed = Some(index);
                    break;
                }
                Poll::Pending => {}
            }
        }
    }
    if let Some(index) = closed {
        Poll::Ready(IoEvent::Closed { index })
    } else if progress {
        Poll::Ready(IoEvent::Wrote)
    } else {
        Poll::Pending
    }
}

fn poll_io(
    slots: &mut [Slot],
    pace: &mut Pace,
    timing: bool,
    cx: &mut Context<'_>,
) -> Poll<IoEvent> {
    let command = poll_reads(slots, cx);
    let pump_due = pace.poll_due(cx);
    match (scheduled_step(command.is_some(), pump_due), command) {
        (Scheduled::Command, Some(event)) => Poll::Ready(event),
        (Scheduled::Pump, _) => {
            pace.take();
            Poll::Ready(IoEvent::Pump)
        }
        _ => poll_writes(slots, timing, cx),
    }
}

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
    let mut slots: Vec<Slot> = Vec::new();
    let mut pace = Pace::new();
    let timing = std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some();
    let (established_tx, mut established_rx) = mpsc::channel(16);
    tokio::pin!(shutdown);
    // Reads, the autonomous pump, and writes share this task so a command
    // already in a socket is handled before the pump can invalidate it.
    let result = loop {
        let step = tokio::select! {
            event = std::future::poll_fn(|cx| poll_io(&mut slots, &mut pace, timing, cx)) => Loop::Io(event),
            accepted = listener.accept() => Loop::Accept(accepted),
            established = established_rx.recv() => Loop::Established(established.map(Box::new)),
            _ = &mut shutdown => Loop::Shutdown,
        };
        match step {
            Loop::Shutdown => break Ok(()),
            Loop::Accept(Err(error)) => break Err(error),
            Loop::Accept(Ok((socket, _))) => {
                if let Ok(permit) = capacity.clone().try_acquire_owned() {
                    let service = service.clone();
                    let accounts = accounts.clone();
                    let tx = established_tx.clone();
                    tasks.spawn(async move {
                        match handshake(socket, service.clone(), accounts, permit).await {
                            Ok(slot) => {
                                if let Err(mpsc::error::SendError(slot)) = tx.send(slot).await {
                                    service.lock().await.disconnect(slot.id);
                                }
                            }
                            Err(Some(id)) => service.lock().await.disconnect(id),
                            Err(None) => {}
                        }
                    });
                }
            }
            Loop::Established(None) => break Ok(()),
            Loop::Established(Some(slot)) => slots.push(*slot),
            Loop::Io(IoEvent::Pump) => {
                service.lock().await.deliver_or_advance(None);
            }
            Loop::Io(IoEvent::Wrote) => {}
            Loop::Io(IoEvent::Closed { index }) => drop_slot(&service, slots.remove(index)).await,
            Loop::Io(IoEvent::Request { index, text }) => {
                let id = slots[index].id;
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Request {
                        request_id,
                        request,
                    }) => {
                        if timing {
                            let started = Instant::now();
                            let mut session = service.lock().await;
                            let lock_ms = started.elapsed().as_secs_f64() * 1000.;
                            let handle_started = Instant::now();
                            session.deliver_or_advance(Some((id, request_id.clone(), request)));
                            let handle_ms = handle_started.elapsed().as_secs_f64() * 1000.;
                            drop(session);
                            timing_event("server_handled", id, &request_id, lock_ms, handle_ms);
                        } else {
                            service
                                .lock()
                                .await
                                .deliver_or_advance(Some((id, request_id, request)));
                        }
                    }
                    _ => {
                        send_error(
                            &mut slots[index].socket,
                            ErrorCode::InvalidRequest,
                            "Invalid request message",
                        )
                        .await;
                        drop_slot(&service, slots.remove(index)).await;
                    }
                }
            }
        }
    };
    service.lock().await.shutdown();
    // Clients accepted after the slot list was last polled are still in this
    // channel. Flush both groups; shutdown already dropped their senders.
    let mut closing = Vec::new();
    while let Ok(slot) = established_rx.try_recv() {
        closing.push(slot);
    }
    closing.append(&mut slots);
    for slot in closing {
        drop_slot(&service, slot).await;
    }
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

enum Loop {
    Io(IoEvent),
    Accept(io::Result<(TcpStream, std::net::SocketAddr)>),
    // Boxed: a live socket is far larger than the other steps, and an
    // unboxed variant trips clippy's large-enum lint.
    Established(Option<Box<Slot>>),
    Shutdown,
}

async fn drop_slot(service: &Mutex<Service>, slot: Slot) {
    service.lock().await.disconnect(slot.id);
    // Detached so one slow socket cannot stall the session loop. The task
    // owns the socket exclusively; the serve loop has already removed it.
    drop(tokio::spawn(flush_slot(slot)));
}

/// Write what was already queued, then the close frame. Disconnect drops the
/// sender only after the reason is in this queue, and a half-flushed write may
/// still be sitting in `pending_out`.
async fn flush_slot(mut slot: Slot) {
    if let Some(message) = slot.pending_out.take() {
        if !matches!(
            timeout(IO_TIMEOUT, slot.socket.send(message)).await,
            Ok(Ok(()))
        ) {
            let _ = timeout(IO_TIMEOUT, slot.socket.close(None)).await;
            return;
        }
    }
    while let Ok(message) = slot.outbound.try_recv() {
        let Ok(text) = serde_json::to_string(&message) else {
            break;
        };
        if !matches!(
            timeout(IO_TIMEOUT, slot.socket.send(Message::Text(text.into()))).await,
            Ok(Ok(()))
        ) {
            break;
        }
    }
    let _ = timeout(IO_TIMEOUT, slot.socket.close(None)).await;
}

async fn handshake(
    socket: TcpStream,
    service: Arc<Mutex<Service>>,
    accounts: Arc<Vec<Account>>,
    permit: OwnedSemaphorePermit,
) -> Result<Slot, Option<u64>> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(16 * 1024))
        .max_frame_size(Some(16 * 1024));
    let upgrade = accept_hdr_async_with_config(socket, native_origin, Some(config));
    let Ok(Ok(mut socket)) = timeout(IO_TIMEOUT, upgrade).await else {
        return Err(None);
    };
    let Ok(Some(Ok(Message::Text(text)))) = timeout(IO_TIMEOUT, socket.next()).await else {
        return Err(None);
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
    match authenticated {
        Ok((account, frontend)) => {
            let connected = service.lock().await.connect(account, frontend);
            match connected {
                Ok(client) => {
                    // The watch used to wake the per-connection task. Sender
                    // drop is that signal now: writes drain the queue and then
                    // see the closed channel.
                    drop(client.close);
                    Ok(Slot {
                        id: client.id,
                        socket,
                        outbound: client.messages,
                        buffered: None,
                        pending_out: None,
                        flushing: false,
                        ack_id: None,
                        ack_started: None,
                        _permit: permit,
                    })
                }
                Err(error) => {
                    send_error(&mut socket, error.code, &error.message).await;
                    Err(None)
                }
            }
        }
        Err((code, message)) => {
            send_error(&mut socket, code, message).await;
            Err(None)
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buffered_command_runs_before_a_due_autonomous_pump() {
        assert_eq!(scheduled_step(true, true), Scheduled::Command);
        assert_eq!(scheduled_step(true, false), Scheduled::Command);
        assert_eq!(scheduled_step(false, true), Scheduled::Pump);
        assert_eq!(scheduled_step(false, false), Scheduled::Idle);
    }
}
