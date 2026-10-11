use crate::{observation_assets, ClientState, Palette};
use futures_util::{SinkExt, StreamExt};
use std::{
    error::Error,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};
use tor_protocol::*;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type ConnectionError = Box<dyn Error + Send + Sync>;
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueryPhase {
    Queue,
    Flush,
    AwaitReply,
}

/// Stored across canceled `next` calls. A queued request is flushed again,
/// never queued again; uncertainty never causes gameplay input to be replayed.
struct PendingQuery {
    request_id: String,
    deadline: tokio::time::Instant,
    phase: QueryPhase,
}

impl PendingQuery {
    fn new() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            deadline: tokio::time::Instant::now() + DEADLINE,
            phase: QueryPhase::Queue,
        }
    }

    /// Stored phases ensure cancellation never loses or duplicates a query.
    async fn send(
        &mut self,
        socket: &mut Socket,
        request: Request,
        expired: &'static str,
        limit: usize,
    ) -> Result<(), ConnectionError> {
        if tokio::time::Instant::now() >= self.deadline {
            return Err(expired.into());
        }
        if self.phase == QueryPhase::Queue {
            let message = ClientMessage::Request {
                request_id: self.request_id.clone(),
                request,
            };
            tokio::time::timeout_at(
                self.deadline,
                socket.feed(Message::Text(encode_bounded_json(&message, limit)?.into())),
            )
            .await
            .map_err(|_| expired)??;
            self.phase = QueryPhase::Flush;
        }
        if self.phase == QueryPhase::Flush {
            tokio::time::timeout_at(self.deadline, socket.flush())
                .await
                .map_err(|_| expired)??;
            self.phase = QueryPhase::AwaitReply;
        }
        Ok(())
    }
}

/// Loopback transport for one attached actor. Disconnects are surfaced; uncertain
/// commands are never retried with a new identity. Reconnect with a fresh snapshot.
pub struct Connection {
    socket: Socket,
    responses: ResponseAssembler,
    pub state: ClientState,
    /// The asset palette as last heard; [`Connection::next`] keeps it current.
    pub palette: Palette,
    role: AccessRole,
    capabilities: ServerCapabilities,
    timing: bool,
    previous_timing_write_ms: f64,
    pace: Duration,
    /// A received update waiting for its turn on screen.
    held: Option<ServerMessage>,
    last_shown: Option<Instant>,
    skipping: bool,
    recovery: Option<PendingQuery>,
    last_recovery_snapshot: Option<String>,
    /// Applied exactly once, retained until its automatic query is flushed.
    pending_presentation: Option<ServerMessage>,
    palette_request: Option<PendingQuery>,
}

/// Whether a message shows the player a new moment of play. Only these are
/// spaced out; everything else is applied as soon as it arrives.
fn shown(message: &ServerMessage) -> bool {
    matches!(
        message,
        ServerMessage::Update { update } if matches!(
            update.body,
            UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
        )
    )
}

impl Connection {
    pub async fn connect(
        address: SocketAddr,
        token: String,
        actor: ActorId,
        frontend: &str,
    ) -> Result<Self, ConnectionError> {
        if !address.ip().is_loopback() {
            return Err("Only loopback connections are supported".into());
        }
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_RESPONSE_BYTES))
            .max_frame_size(Some(MAX_RESPONSE_BYTES));
        let (mut socket, _) = timeout(
            DEADLINE,
            connect_async_with_config(format!("ws://{address}"), Some(config), false),
        )
        .await??;
        send(
            &mut socket,
            ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                token,
                frontend: frontend.into(),
            },
            MAX_REQUEST_BYTES,
        )
        .await?;
        let mut responses = ResponseAssembler::default();
        let (role, capabilities) = match timeout(
            DEADLINE,
            receive(&mut socket, MAX_RESPONSE_BYTES, &mut responses),
        )
        .await??
        {
            ServerMessage::Welcome {
                protocol,
                actors,
                role,
                capabilities,
                ..
            } if protocol == PROTOCOL_VERSION
                && actors.contains(&actor)
                && capabilities.is_valid() =>
            {
                (role, capabilities)
            }
            ServerMessage::Error { code, .. } => {
                return Err(format!("Connection rejected: {code:?}").into())
            }
            _ => return Err("Incompatible welcome or unauthorized actor".into()),
        };
        responses = ResponseAssembler::with_limit(capabilities.max_snapshot_bytes as usize)?;
        send(
            &mut socket,
            ClientMessage::Request {
                request_id: "attach".into(),
                request: Request::Attach { actor },
            },
            capabilities.max_request_bytes as usize,
        )
        .await?;
        let snapshot = match timeout(
            DEADLINE,
            receive(
                &mut socket,
                capabilities.max_response_bytes as usize,
                &mut responses,
            ),
        )
        .await??
        {
            ServerMessage::Snapshot {
                request_id,
                snapshot,
            } if request_id == "attach" && snapshot.actor == actor => *snapshot,
            _ => return Err("Expected attachment snapshot".into()),
        };
        let state =
            ClientState::from_snapshot(snapshot).map_err(|e| format!("Invalid snapshot: {e:?}"))?;
        Ok(Self {
            socket,
            responses,
            state,
            palette: Palette::default(),
            role,
            capabilities,
            timing: std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some(),
            previous_timing_write_ms: 0.,
            pace: Duration::ZERO,
            held: None,
            last_shown: None,
            skipping: false,
            recovery: None,
            last_recovery_snapshot: None,
            pending_presentation: None,
            palette_request: None,
        })
    }

    pub fn role(&self) -> AccessRole {
        self.role
    }

    pub fn capabilities(&self) -> ServerCapabilities {
        self.capabilities
    }

    /// The least time between two updates the player sees. The server runs
    /// play until it needs input, so a journey's steps arrive together; this
    /// spaces them out on screen. Zero shows every update as it arrives.
    pub fn pace(&self) -> Duration {
        self.pace
    }

    pub fn set_pace(&mut self, pace: Duration) {
        self.pace = pace;
    }

    /// Show what has already arrived without waiting, until the next request.
    /// This only changes the display; the server isn't told.
    pub fn skip(&mut self) {
        self.skipping = true;
    }

    /// Whether an update is waiting for its turn on screen.
    pub fn playing(&self) -> bool {
        self.held.is_some() || self.pending_presentation.is_some()
    }

    /// Whether requests may use the current disclosed state. The last valid
    /// model remains available for display during recovery, never for commands.
    pub fn is_synchronized(&self) -> bool {
        self.recovery.is_none() && !self.responses.is_pending()
    }

    /// Classify a snapshot using this connection's own recovery request identity.
    /// An ordinary authoritative reset may precede a successful command receipt.
    pub fn is_recovery_snapshot(&self, message: &ServerMessage) -> bool {
        matches!(message, ServerMessage::Snapshot { request_id, .. }
            if self.last_recovery_snapshot.as_ref() == Some(request_id))
    }

    fn require_snapshot(&mut self) {
        if self.recovery.is_none() {
            self.recovery = Some(PendingQuery::new());
        }
    }

    async fn progress_recovery(&mut self) -> Result<(), ConnectionError> {
        if let Some(query) = &mut self.recovery {
            query
                .send(
                    &mut self.socket,
                    Request::Snapshot,
                    "Stream resynchronization timed out",
                    self.capabilities.max_request_bytes as usize,
                )
                .await?;
        }
        Ok(())
    }

    async fn progress_palette_request(&mut self) -> Result<(), ConnectionError> {
        if let Some(query) = &mut self.palette_request {
            query
                .send(
                    &mut self.socket,
                    Request::Palette,
                    "Asset palette request timed out",
                    self.capabilities.max_request_bytes as usize,
                )
                .await?;
            self.palette_request = None;
        }
        Ok(())
    }

    /// Send a request; any skipping ends, so what follows is paced again.
    pub async fn request(&mut self, request: Request) -> Result<String, ConnectionError> {
        if !self.is_synchronized() {
            return Err("Resynchronizing; request was not sent".into());
        }
        self.progress_palette_request().await?;
        self.skipping = false;
        self.send_request(request).await
    }

    async fn send_request(&mut self, request: Request) -> Result<String, ConnectionError> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = self.timing.then(std::time::Instant::now);
        if self.timing {
            self.timing_event("client_request", &request_id, None);
        }
        send(
            &mut self.socket,
            ClientMessage::Request {
                request_id: request_id.clone(),
                request,
            },
            self.capabilities.max_request_bytes as usize,
        )
        .await?;
        if let Some(started) = started {
            self.timing_event(
                "client_request_sent",
                &request_id,
                Some(started.elapsed().as_secs_f64() * 1000.),
            );
        }
        Ok(request_id)
    }

    /// Apply ordered updates and palettes before presentation. A palette that
    /// misses a revision, or an observation naming an asset the palette
    /// lacks, sends a `palette` request; its answer is an ordinary palette
    /// message. An update the player sees waits until [`Connection::pace`]
    /// after the previous one. Waiting is safe to cancel when terminal input
    /// becomes available: a held update is kept for the next call.
    pub async fn next(&mut self) -> Result<ServerMessage, ConnectionError> {
        loop {
            self.progress_recovery().await?;
            self.progress_palette_request().await?;
            if let Some(message) = self.pending_presentation.take() {
                return Ok(message);
            }
            let mut message = match self.held.take() {
                Some(message) => message,
                None => match &self.recovery {
                    Some(recovery) => tokio::time::timeout_at(
                        recovery.deadline,
                        receive(
                            &mut self.socket,
                            self.capabilities.max_response_bytes as usize,
                            &mut self.responses,
                        ),
                    )
                    .await
                    .map_err(|_| "Stream resynchronization timed out")??,
                    None => {
                        receive(
                            &mut self.socket,
                            self.capabilities.max_response_bytes as usize,
                            &mut self.responses,
                        )
                        .await?
                    }
                },
            };
            if self.is_synchronized() && shown(&message) {
                let due = self
                    .last_shown
                    .map(|shown| shown + self.pace)
                    .filter(|due| !self.skipping && *due > Instant::now());
                if let Some(due) = due {
                    self.held = Some(message);
                    tokio::time::sleep_until(due.into()).await;
                    message = self.held.take().expect("held update");
                }
                self.last_shown = Some(Instant::now());
            }
            if self.timing {
                if let ServerMessage::Ack { request_id, .. } = &message {
                    self.timing_event("client_ack", request_id, None);
                }
            }
            if let ServerMessage::Ack { receipt, .. } = &message {
                if receipt.actor() != self.state.state().observation.actor {
                    return Err("Receipt belongs to another actor".into());
                }
            }
            match &message {
                ServerMessage::Error {
                    scope: ErrorScope::Transport {},
                    code,
                    message,
                    ..
                } => {
                    return Err(format!("Transport error: {code:?}: {message}").into());
                }
                ServerMessage::Error {
                    scope: ErrorScope::Unattached {},
                    ..
                } => {
                    return Err("Host error has no current attachment".into());
                }
                _ => {}
            }
            if let ServerMessage::CreatureInspection { report, .. } = &message {
                if self.role != AccessRole::Wizard {
                    return Err("Privileged inspection sent to a non-wizard client".into());
                }
                report
                    .validate()
                    .map_err(|error| format!("Invalid creature inspection: {error}"))?;
            }
            if let ServerMessage::CombatDiagnostics { report, .. } = &message {
                if self.role != AccessRole::Wizard {
                    return Err("Privileged combat diagnostics sent to a non-wizard client".into());
                }
                report
                    .validate()
                    .map_err(|error| format!("Invalid combat diagnostics: {error}"))?;
            }
            let reply_context = message.reply_context();
            if let Some(context) = reply_context {
                // Even during repair, another attachment/actor cannot confirm
                // this request. Missing ordered state within this attachment
                // requires repair; a reply never installs that state itself.
                if context.input.stream.stream != self.state.context().stream {
                    return Err("Reply belongs to another attachment".into());
                }
                if context.actor != self.state.state().observation.actor {
                    return Err("Reply belongs to another actor".into());
                }
                if self.is_synchronized() && self.state.validate_reply_context(context).is_err() {
                    self.require_snapshot();
                }
                if !self.is_synchronized()
                    && matches!(
                        message,
                        ServerMessage::History { .. }
                            | ServerMessage::Palette { .. }
                            | ServerMessage::CreatureInspection { .. }
                            | ServerMessage::CombatDiagnostics { .. }
                    )
                {
                    // A receipt survives independently of the stream. Query
                    // contents cannot be presented or applied while uncertain.
                    continue;
                }
            }
            match &message {
                ServerMessage::Update { update } => {
                    if !self.is_synchronized() {
                        continue;
                    }
                    if self.state.apply(*update.clone()).is_err() {
                        self.require_snapshot();
                        continue;
                    }
                }
                ServerMessage::Snapshot {
                    request_id,
                    snapshot,
                } => {
                    let recovering = self.recovery.is_some();
                    if let Some(recovery) = &self.recovery {
                        if request_id != &recovery.request_id {
                            continue;
                        }
                    }
                    match self.state.replace_snapshot(*snapshot.clone()) {
                        Ok(()) => {
                            if recovering {
                                self.last_recovery_snapshot = Some(request_id.clone());
                            }
                            self.recovery = None;
                            self.last_shown = None;
                        }
                        Err(error) if recovering => {
                            return Err(
                                format!("Invalid resynchronization snapshot: {error:?}").into()
                            );
                        }
                        Err(_) => {
                            self.require_snapshot();
                            continue;
                        }
                    }
                }
                ServerMessage::Error {
                    request_id: Some(request_id),
                    message,
                    ..
                } if self
                    .recovery
                    .as_ref()
                    .is_some_and(|recovery| &recovery.request_id == request_id) =>
                {
                    return Err(format!("Stream resynchronization rejected: {message}").into());
                }
                _ => {}
            }
            let ask = match &message {
                ServerMessage::Update { .. } | ServerMessage::Snapshot { .. } => self
                    .palette
                    .notice(observation_assets(&self.state.state().observation)),
                // A full palette may still lack what's already in view.
                ServerMessage::Palette { palette, .. } => {
                    let gap = self.palette.apply(palette);
                    let missing = self
                        .palette
                        .notice(observation_assets(&self.state.state().observation));
                    gap || missing
                }
                _ => false,
            };
            if ask {
                self.pending_presentation = Some(message);
                self.palette_request = Some(PendingQuery::new());
                continue;
            }
            return Ok(message);
        }
    }

    // Opt-in host diagnostics own their numeric scalar schema independently of wire encoding.
    fn timing_event(&mut self, event: &str, request_id: &str, duration_ms: Option<f64>) {
        let started = std::time::Instant::now();
        eprintln!(
            "{}",
            serde_json::json!({"timing_version":1,"event":event,
            "request_id":request_id,"actor":self.state.state().observation.actor.0,
            "revision":self.state.state().revision,"duration_ms":duration_ms,
            "previous_timing_write_ms":self.previous_timing_write_ms,
            "unix_ns":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()})
        );
        self.previous_timing_write_ms = started.elapsed().as_secs_f64() * 1000.;
    }

    /// Save-and-quit is a durable barrier; abrupt disconnect is not.
    pub async fn close(&mut self) -> Result<(), ConnectionError> {
        if self.role != AccessRole::Spectator {
            let id = self.request(Request::Save).await?;
            timeout(std::time::Duration::from_secs(30), async {
                loop {
                    match self.next().await? {
                        ServerMessage::Ack { request_id, .. } if request_id == id => {
                            return Ok::<(), ConnectionError>(())
                        }
                        ServerMessage::Error {
                            request_id: Some(request_id),
                            message,
                            ..
                        } if request_id == id => return Err(message.into()),
                        _ => {}
                    }
                }
            })
            .await
            .map_err(|_| "Save timed out; recent play may not be durable")??;
        }
        timeout(DEADLINE, self.socket.close(None)).await??;
        Ok(())
    }
}

async fn send(
    socket: &mut Socket,
    message: ClientMessage,
    limit: usize,
) -> Result<(), ConnectionError> {
    timeout(
        DEADLINE,
        socket.send(Message::Text(encode_bounded_json(&message, limit)?.into())),
    )
    .await??;
    Ok(())
}

async fn receive(
    socket: &mut Socket,
    limit: usize,
    responses: &mut ResponseAssembler,
) -> Result<ServerMessage, ConnectionError> {
    loop {
        match socket.next().await.ok_or("Server disconnected")?? {
            Message::Text(text) => {
                if text.len() > limit {
                    return Err(DecodeError::TooLarge { limit }.into());
                }
                if let Some(message) = responses.push(decode_response(&text)?)? {
                    return Ok(message);
                }
            }
            Message::Close(_) => return Err("Server disconnected".into()),
            Message::Ping(_) | Message::Pong(_) => {}
            _ => return Err("Unexpected non-text server frame".into()),
        }
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
