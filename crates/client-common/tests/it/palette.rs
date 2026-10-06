//! A connection keeps its palette current against a scripted server: it
//! applies the attach palette and deltas, and asks for the whole palette
//! again after a missed revision or for an asset the palette lacks, once.
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeSet;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tor_client_common::Connection;
use tor_protocol::*;

type Server = WebSocketStream<TcpStream>;

fn observation(revision: u64, assets: &[&str]) -> StateView {
    let cells: Vec<_> = assets
        .iter()
        .enumerate()
        .map(|(x, asset)| {
            serde_json::json!({"key":format!("cell-{x}"),"stairs_up":false,"stairs_down":false,
                "position":{"x":x,"y":0,"z":0},"wall":false,"place_hint":false,"asset":asset})
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":revision,"observation":{
            "actor":1,"tick":revision,"position":{"x":0,"y":0,"z":0},"places":[],
            "visible_cells":cells,"ground_items":[],"inventory":[],"visible_actors":[],"ready":true
        }
    }))
    .unwrap()
}

fn assets(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

async fn send(server: &mut Server, message: ServerMessage) {
    let text = serde_json::to_string(&message).unwrap();
    server.send(Message::Text(text.into())).await.unwrap();
}

async fn receive(server: &mut Server) -> Option<ClientMessage> {
    loop {
        match server.next().await?.unwrap() {
            Message::Text(text) => return Some(serde_json::from_str(&text).unwrap()),
            Message::Close(_) => return None,
            _ => {}
        }
    }
}

/// The id of the palette request the client sends next.
async fn palette_request(server: &mut Server) -> String {
    match receive(server).await {
        Some(ClientMessage::Request {
            request_id,
            request: Request::Palette,
        }) => request_id,
        other => panic!("expected a palette request, got {other:?}"),
    }
}

fn palette(
    disclosed: u64,
    request_id: Option<String>,
    revision: u64,
    body: PaletteBody,
) -> ServerMessage {
    ServerMessage::Palette {
        context: ReplyContext {
            input: InputContext {
                stream: super::stream_context(0),
                readiness_revision: 0,
            },
            actor: ActorId(1),
            branch: BranchId("branch-1".into()),
            cursor: StreamCursor {
                sequence: disclosed,
                tick: disclosed,
            },
            revision: disclosed,
        },
        request_id,
        palette: PaletteUpdate { revision, body },
    }
}

fn full(names: &[&str]) -> PaletteBody {
    PaletteBody::Full {
        assets: assets(names),
    }
}

fn update(sequence: u64, names: &[&str]) -> ServerMessage {
    ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: super::stream_context(0),
            actor: ActorId(1),
            branch: BranchId("branch-1".into()),
            cursor: StreamCursor {
                sequence,
                tick: sequence,
            },
            body: UpdateBody::Observation {
                state: Box::new(observation(sequence, names)),
                event: None,
            },
        }),
    }
}

#[tokio::test]
async fn a_connection_repairs_its_palette_after_a_gap_and_a_missing_asset() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut server).await,
            Some(ClientMessage::Hello { .. })
        ));
        send(
            &mut server,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                user: "spectator".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Spectator,
            },
        )
        .await;
        assert!(matches!(
            receive(&mut server).await,
            Some(ClientMessage::Request { .. })
        ));
        let snapshot = Snapshot {
            readiness: tor_protocol::Readiness {
                revision: 0,
                admission: false,
                resume: vec![],
                cancel: vec![],
            },
            context: super::stream_context(0),
            intentions: Vec::new(),
            actor: ActorId(1),
            branch: BranchId("branch-1".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            has_control: false,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
            state: observation(0, &["terrain.floor.cave"]),
            travel: None,
        };
        send(
            &mut server,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(snapshot),
            },
        )
        .await;
        // The attach palette, then a delta that follows it.
        send(
            &mut server,
            palette(0, None, 1, full(&["terrain.floor.cave"])),
        )
        .await;
        send(
            &mut server,
            palette(
                0,
                None,
                2,
                PaletteBody::Delta {
                    base: 1,
                    added: assets(&["item.coin"]),
                    removed: BTreeSet::new(),
                },
            ),
        )
        .await;
        // Revision 3 is lost: the next delta doesn't follow.
        send(
            &mut server,
            palette(
                0,
                None,
                4,
                PaletteBody::Delta {
                    base: 3,
                    added: BTreeSet::new(),
                    removed: BTreeSet::new(),
                },
            ),
        )
        .await;
        let id = palette_request(&mut server).await;
        send(
            &mut server,
            palette(0, Some(id), 5, full(&["terrain.floor.cave", "item.coin"])),
        )
        .await;
        // An asset the palette lacks: one request, even when it's still
        // missing from the answer and seen again.
        send(&mut server, update(1, &["creature.rat"])).await;
        let id = palette_request(&mut server).await;
        send(
            &mut server,
            palette(1, Some(id), 6, full(&["terrain.floor.cave"])),
        )
        .await;
        send(&mut server, update(2, &["creature.rat"])).await;
        // Nothing more is asked before the client closes.
        assert!(receive(&mut server).await.is_none());
    });

    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    assert_eq!(client.palette.revision(), None);
    client.next().await.unwrap();
    assert_eq!(client.palette.revision(), Some(1));
    assert!(client.palette.holds("terrain.floor.cave"));
    client.next().await.unwrap();
    assert!(client.palette.holds("item.coin"));
    client.next().await.unwrap();
    assert!(client.palette.stale());
    assert!(!client.palette.holds("item.coin"));
    client.next().await.unwrap();
    assert_eq!(client.palette.revision(), Some(5));
    assert!(!client.palette.stale());
    client.next().await.unwrap();
    client.next().await.unwrap();
    assert_eq!(client.palette.revision(), Some(6));
    assert!(!client.palette.holds("creature.rat"));
    assert!(!client.palette.holds("item.coin"));
    client.next().await.unwrap();
    client.close().await.unwrap();
    server.await.unwrap();
}
