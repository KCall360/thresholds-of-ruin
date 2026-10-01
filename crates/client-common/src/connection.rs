use crate::{observation_assets, ClientState, Palette};
use futures_util::{SinkExt, StreamExt};
use std::{error::Error, net::SocketAddr, time::Duration};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::*;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type ConnectionError = Box<dyn Error + Send + Sync>;
const DEADLINE: Duration = Duration::from_secs(10);

/// Loopback transport for one attached actor. Disconnects are surfaced; uncertain
/// commands are never retried with a new identity. Reconnect with a fresh snapshot.
pub struct Connection {
    socket: Socket,
    pub state: ClientState,
    /// The asset palette as last heard; [`Connection::next`] keeps it current.
    pub palette: Palette,
    role: AccessRole,
    timing: bool,
    previous_timing_write_ms: f64,
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
        let (mut socket, _) = timeout(DEADLINE, connect_async(format!("ws://{address}"))).await??;
        send(
            &mut socket,
            ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                token,
                frontend: frontend.into(),
            },
        )
        .await?;
        let role = match timeout(DEADLINE, receive(&mut socket)).await?? {
            ServerMessage::Welcome {
                protocol,
                actors,
                role,
                ..
            } if protocol == PROTOCOL_VERSION && actors.contains(&actor) => role,
            ServerMessage::Error { code, .. } => {
                return Err(format!("Authentication failed: {code:?}").into())
            }
            _ => return Err("Incompatible welcome or unauthorized actor".into()),
        };
        send(
            &mut socket,
            ClientMessage::Request {
                request_id: "attach".into(),
                request: Request::Attach { actor },
            },
        )
        .await?;
        let snapshot = match timeout(DEADLINE, receive(&mut socket)).await?? {
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
            state,
            palette: Palette::default(),
            role,
            timing: std::env::var_os("TOR_TIMING_DIAGNOSTICS").is_some(),
            previous_timing_write_ms: 0.,
        })
    }

    pub fn role(&self) -> AccessRole {
        self.role
    }

    pub async fn request(&mut self, request: Request) -> Result<String, ConnectionError> {
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
    /// message. Waiting for a frame is safe to cancel when terminal input
    /// becomes available.
    pub async fn next(&mut self) -> Result<ServerMessage, ConnectionError> {
        let message = receive(&mut self.socket).await?;
        if self.timing {
            if let ServerMessage::Ack { request_id, .. } = &message {
                self.timing_event("client_ack", request_id, None);
            }
        }
        match &message {
            ServerMessage::Update { update } => self
                .state
                .apply(*update.clone())
                .map_err(|e| format!("Invalid stream: {e:?}"))?,
            ServerMessage::Snapshot { snapshot, .. } => {
                if snapshot.actor != self.state.state().observation.actor {
                    return Err("Snapshot changed attached actor".into());
                }
                self.state
                    .replace_snapshot(*snapshot.clone())
                    .map_err(|e| format!("Invalid snapshot: {e:?}"))?;
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
            self.request(Request::Palette).await?;
        }
        Ok(message)
    }

    // Explicitly opt-in host diagnostics; no protocol or simulation-state fields.
    fn timing_event(&mut self, event: &str, request_id: &str, duration_ms: Option<f64>) {
        let started = std::time::Instant::now();
        eprintln!(
            "{}",
            serde_json::json!({"timing_version":1,"event":event,
            "request_id":request_id,"actor":self.state.state().observation.actor,
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

async fn send(socket: &mut Socket, message: ClientMessage) -> Result<(), ConnectionError> {
    timeout(
        DEADLINE,
        socket.send(Message::Text(serde_json::to_string(&message)?.into())),
    )
    .await??;
    Ok(())
}

async fn receive(socket: &mut Socket) -> Result<ServerMessage, ConnectionError> {
    loop {
        match socket.next().await.ok_or("Server disconnected")?? {
            Message::Text(text) => return Ok(serde_json::from_str(&text)?),
            Message::Close(_) => return Err("Server disconnected".into()),
            Message::Ping(_) | Message::Pong(_) => {}
            _ => return Err("Unexpected non-text server frame".into()),
        }
    }
}
