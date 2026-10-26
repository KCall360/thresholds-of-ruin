//! A connection keeps its palette current against a scripted server: it
//! applies the attach palette and deltas, and asks for the whole palette
//! again after a missed revision or for an asset the palette lacks, once.
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeSet;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tor_client_common::Connection;
use tor_protocol::*;

type Server = WebSocketStream<TcpStream>;

fn observation(revision: u64, assets: &[&str]) -> StateView {
    let cells: Vec<_> = assets
        .iter()
        .enumerate()
        .map(|(x, asset)| {
            serde_json::json!({"key":format!("cell-{x}"),"trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,
                "position":{"x":x,"y":0,"z":0},"wall":false,"lit":true,"place_hint":false,"asset":asset})
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":revision.to_string(),"observation":{
            "actor":"1","self_target":tor_protocol::ActorTarget::from_digest([1; 32]),"tick":revision.to_string(),"position":{"x":0,"y":0,"z":0},"places":[],
            "visible_cells":cells,"ground_items":[],"inventory":[],"charts":[],"linked_holds":[],"visible_actors":[],"ready":true
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
            Message::Text(text) => {
                let message: ClientMessage = serde_json::from_str(&text).unwrap();
                if let ClientMessage::Request {
                    request_id,
                    request,
                } = &message
                {
                    if let Request::Map { query } = request.as_ref() {
                        assert!(request_id.starts_with("map-"));
                        query.validate().unwrap();
                        assert!(query.cursor.is_none());
                        assert_eq!(query.context, super::stream_context(0));
                        assert_eq!(query.branch, BranchId("branch-1".into()));
                        // This healthy palette peer has no older remembered
                        // cells. Answer its independent query exactly once.
                        send(
                            server,
                            ServerMessage::MapPage {
                                context: ReplyContext {
                                    input: InputContext {
                                        stream: query.context.clone(),
                                        readiness_revision: 0,
                                    },
                                    actor: ActorId(1),
                                    branch: query.branch.clone(),
                                    cursor: query.base.cursor,
                                    revision: query.base.revision,
                                },
                                request_id: request_id.clone(),
                                page: Box::new(MapPage {
                                    transfer: request_id.clone(),
                                    actor: ActorId(1),
                                    branch: query.branch.clone(),
                                    base: query.base,
                                    window: query.window,
                                    start: 0,
                                    page: 0,
                                    total_cells: 0,
                                    complete: true,
                                    ambiguous: false,
                                    limited: false,
                                    cells: vec![],
                                    next: None,
                                }),
                            },
                        )
                        .await;
                        continue;
                    }
                }
                return Some(message);
            }
            Message::Close(_) => return None,
            _ => {}
        }
    }
}

/// Observe palette/live messages without treating a completed independent map
/// query as another palette revision. Unexpected message kinds remain visible.
async fn next_palette_message(client: &mut Connection) -> ServerMessage {
    loop {
        let message = client.next().await.unwrap();
        if let ServerMessage::MapPage { page, .. } = &message {
            assert!(page.complete && page.next.is_none() && page.cells.is_empty());
            assert!(client.state.map_hydration().complete);
            continue;
        }
        return message;
    }
}

/// The id of the palette request the client sends next.
async fn palette_request(server: &mut Server) -> String {
    match receive(server).await {
        Some(ClientMessage::Request {
            request_id,
            request,
        }) if matches!(*request, Request::Palette) => request_id,
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

#[derive(Clone, Copy, Debug)]
enum Carrier {
    Cells,
    SelfBody,
    GroundCorpse,
    CarriedCorpse,
}

fn update(sequence: u64, names: &[&str], carrier: Carrier) -> ServerMessage {
    let mut state = observation(sequence, names);
    if !matches!(carrier, Carrier::Cells) {
        state.observation.visible_cells.clear();
        let body = CreatureAppearance {
            seed: AppearanceSeed::from_digest([9; 32]),
            worn: Vec::new(),
            asset: Some(names[0].into()),
        };
        if matches!(carrier, Carrier::SelfBody) {
            state.observation.self_appearance = Some(body);
        } else {
            let item = ItemView {
                corpse: Some(body),
                class: ItemClass::Corpse,
                quantity: 1,
                appearance: "remains".into(),
                identified: true,
                description: String::new(),
                id: ItemTarget::from_digest([3; 32]),
                name: "remains".into(),
                asset: None,
                known_supply: None,
                known_currency: None,
            };
            if matches!(carrier, Carrier::CarriedCorpse) {
                state.observation.inventory.push(item);
            } else {
                state.observation.ground_items.push(GroundItemView {
                    reachable: true,
                    supply_reachable: false,
                    item,
                    position: Position { x: 0, y: 0, z: 0 },
                });
            }
        }
    }
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
                state: state.into(),
                event: None,
            },
        }),
    }
}

#[tokio::test]
async fn a_connection_repairs_its_palette_after_a_gap_and_a_missing_asset() {
    // Production ordinary traffic intentionally has no universal deadline.
    // The entire scripted exchange still has a test liveness contract: a lost
    // palette request/reply must fail this test instead of hanging the suite.
    timeout(
        Duration::from_secs(15),
        palette_repair_scenario(Carrier::Cells),
    )
    .await
    .expect("scripted palette exchange exceeded its 15-second watchdog");
}

#[tokio::test]
async fn nested_appearance_assets_trigger_one_palette_repair_per_missing_asset() {
    for carrier in [
        Carrier::SelfBody,
        Carrier::GroundCorpse,
        Carrier::CarriedCorpse,
    ] {
        timeout(Duration::from_secs(15), palette_repair_scenario(carrier))
            .await
            .unwrap_or_else(|_| panic!("nested palette repair timed out for {carrier:?}"));
    }
}

async fn palette_repair_scenario(carrier: Carrier) {
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
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
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
            state: observation(0, &["terrain.floor.cave"]).into(),
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
        send(&mut server, update(1, &["creature.rat"], carrier)).await;
        let id = palette_request(&mut server).await;
        send(
            &mut server,
            palette(1, Some(id), 6, full(&["terrain.floor.cave"])),
        )
        .await;
        send(&mut server, update(2, &["creature.rat"], carrier)).await;
        // Nothing more is asked before the client closes.
        assert!(receive(&mut server).await.is_none());
    });

    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    assert_eq!(client.palette.revision(), None);
    next_palette_message(&mut client).await;
    assert_eq!(client.palette.revision(), Some(1));
    assert!(client.palette.holds("terrain.floor.cave"));
    next_palette_message(&mut client).await;
    assert!(client.palette.holds("item.coin"));
    next_palette_message(&mut client).await;
    assert!(client.palette.stale());
    assert!(!client.palette.holds("item.coin"));
    next_palette_message(&mut client).await;
    assert_eq!(client.palette.revision(), Some(5));
    assert!(!client.palette.stale());
    next_palette_message(&mut client).await;
    next_palette_message(&mut client).await;
    assert_eq!(client.palette.revision(), Some(6));
    assert!(!client.palette.holds("creature.rat"));
    assert!(!client.palette.holds("item.coin"));
    next_palette_message(&mut client).await;
    client.close().await.unwrap();
    server.await.unwrap();
}
