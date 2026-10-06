//! Raw protocol test transport. It records disclosed input generations without
//! repairing or validating server traffic; model/recovery tests do that explicitly.
use futures_util::{SinkExt, StreamExt};
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::{ClientMessage, InputContext, Request, ServerMessage, UpdateBody};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub(super) struct WireClient {
    socket: Socket,
    context: Option<InputContext>,
}

impl WireClient {
    pub fn new(socket: Socket) -> Self {
        Self {
            socket,
            context: None,
        }
    }

    pub fn input_context(&self) -> InputContext {
        self.context
            .clone()
            .expect("an attached client has disclosed input context")
    }

    pub async fn receive(&mut self) -> Option<ServerMessage> {
        let frame = timeout(Duration::from_secs(5), self.socket.next())
            .await
            .ok()??
            .ok()?;
        let Message::Text(text) = frame else {
            return None;
        };
        let message: ServerMessage = serde_json::from_str(&text).ok()?;
        match &message {
            ServerMessage::Snapshot { snapshot, .. } => {
                self.context = Some(InputContext {
                    stream: snapshot.context.clone(),
                    readiness_revision: snapshot.readiness.revision,
                });
            }
            ServerMessage::Update { update } => {
                if let UpdateBody::Readiness { readiness } = &update.body {
                    self.context = Some(InputContext {
                        stream: update.context.clone(),
                        readiness_revision: readiness.revision,
                    });
                }
            }
            _ => {}
        }
        Some(message)
    }

    /// Establish control before constructing the first gameplay request.
    pub async fn acquire_control(&mut self, id: &str) {
        self.request(id, Request::AcquireControl).await;
        loop {
            match self.receive().await.expect("connected acquiring client") {
                ServerMessage::Ack { request_id, .. } if request_id == id => return,
                ServerMessage::Error { code, message, .. } => {
                    panic!("control failed: {code:?}: {message}")
                }
                _ => {}
            }
        }
    }

    /// Resolution is followed by its permission boundary before the next action.
    pub async fn resolution_readiness(&mut self) {
        loop {
            match self.receive().await.expect("connected resolving client") {
                ServerMessage::Waiting { .. } => continue,
                ServerMessage::Update { update }
                    if matches!(update.body, UpdateBody::Readiness { .. }) =>
                {
                    return
                }
                message => panic!("expected post-resolution readiness: {message:?}"),
            }
        }
    }

    /// Send precisely the supplied request; never restamp an abandoned context.
    pub async fn request(&mut self, id: &str, request: Request) {
        let message = ClientMessage::Request {
            request_id: id.into(),
            request,
        };
        self.socket
            .send(Message::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await
            .unwrap();
    }
}

impl Deref for WireClient {
    type Target = Socket;
    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}
impl DerefMut for WireClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.socket
    }
}
