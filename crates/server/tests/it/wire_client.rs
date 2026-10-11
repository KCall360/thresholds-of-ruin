//! Raw protocol test transport. It records disclosed input generations without
//! repairing or validating server traffic; model/recovery tests do that explicitly.
use futures_util::{SinkExt, StreamExt};
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::{ClientMessage, InputContext, Request, ServerMessage, UpdateBody};

/// Do not collapse a silent/malformed peer into a successful transport rejection.
#[derive(Debug)]
#[allow(dead_code)] // Error/frame payloads are retained in assertion diagnostics.
pub(super) enum Received {
    Message(ServerMessage),
    Disconnected,
    Deadline,
    UnexpectedFrame(Message),
    InvalidJson(serde_json::Error),
    TransportError(tokio_tungstenite::tungstenite::Error),
}

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
        match self.receive_event(Duration::from_secs(5)).await {
            Received::Message(message) => Some(message),
            _ => None,
        }
    }

    pub async fn receive_event(&mut self, deadline: Duration) -> Received {
        use tokio_tungstenite::tungstenite::{error::ProtocolError, Error};
        let frame = match timeout(deadline, self.socket.next()).await {
            Err(_) => return Received::Deadline,
            Ok(None) | Ok(Some(Ok(Message::Close(_)))) => return Received::Disconnected,
            Ok(Some(Err(
                Error::ConnectionClosed
                | Error::AlreadyClosed
                | Error::Protocol(ProtocolError::ResetWithoutClosingHandshake),
            ))) => return Received::Disconnected,
            Ok(Some(Err(Error::Io(error))))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::BrokenPipe
                ) =>
            {
                return Received::Disconnected
            }
            Ok(Some(Err(error))) => return Received::TransportError(error),
            Ok(Some(Ok(frame))) => frame,
        };
        let Message::Text(text) = frame else {
            return Received::UnexpectedFrame(frame);
        };
        let message: ServerMessage = match serde_json::from_str(&text) {
            Ok(message) => message,
            Err(error) => return Received::InvalidJson(error),
        };
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
        Received::Message(message)
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

#[tokio::test]
async fn transport_evidence_distinguishes_deadline_close_and_malformed_traffic() {
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        accept_async(stream).await.unwrap()
    });
    let (socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let mut peer = peer.await.unwrap();
    let mut client = WireClient::new(socket);
    assert!(matches!(
        client.receive_event(Duration::ZERO).await,
        Received::Deadline
    ));
    peer.send(Message::Text("not JSON".into())).await.unwrap();
    match client.receive_event(Duration::from_secs(5)).await {
        Received::InvalidJson(error) => assert!(error.is_syntax()),
        other => panic!("malformed response is not a disconnect: {other:?}"),
    }
    peer.send(Message::Ping(Vec::new().into())).await.unwrap();
    assert!(matches!(
        client.receive_event(Duration::from_secs(5)).await,
        Received::UnexpectedFrame(Message::Ping(_))
    ));
    peer.close(None).await.unwrap();
    assert!(matches!(
        client.receive_event(Duration::from_secs(5)).await,
        Received::Disconnected
    ));
}
