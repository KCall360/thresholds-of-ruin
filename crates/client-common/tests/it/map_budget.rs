//! Optional incremental map work must not poison a healthy live connection.
use futures_util::{SinkExt, StreamExt};
use std::future::{poll_fn, Future};
use std::task::Poll;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};
use tor_client_common::Connection;
use tor_protocol::*;

type Server = WebSocketStream<TcpStream>;

fn snapshot(revision: u64, shift: i32) -> Snapshot {
    let visible_cells = [("a", 0, 0), ("b", 1, 0), ("c", 0, 1)]
        .into_iter()
        .map(|(key, x, y)| {
            serde_json::json!({"key":key,"position":{"x":x+shift,"y":y+shift,"z":0},
                "wall":false,"lit":true,"trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,"place_hint":false})
        })
        .collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},
        "context":super::stream_context(0), "actor":"1", "branch":"map-budget",
        "cursor":{"sequence":revision.to_string(),"tick":revision.to_string()},
        "has_control":true,"intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision.to_string(),"observation":{
            "actor":"1","self_target":ActorTarget::from_digest([1;32]),"tick":revision.to_string(),
            "position":{"x":0,"y":0,"z":0},"places":[],
            "visible_cells":visible_cells,
            "ground_items":[],"inventory":[],"linked_holds":[],"visible_actors":[],"ready":true
        }}
    }))
    .unwrap()
}

async fn receive(server: &mut Server) -> ClientMessage {
    let frame = server.next().await.unwrap().unwrap();
    serde_json::from_str(frame.to_text().unwrap()).unwrap()
}

async fn send(server: &mut Server, message: ServerMessage) {
    server
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

async fn map_request(server: &mut Server) -> (String, MapQuery) {
    let ClientMessage::Request {
        request_id,
        request,
    } = receive(server).await
    else {
        panic!("expected an automatic map request")
    };
    let Request::Map { query } = *request else {
        panic!("expected an automatic map query")
    };
    query.validate().unwrap();
    assert!(request_id.starts_with("map-"));
    (request_id, query)
}

fn cell(position: Position, key: &str) -> RememberedMapCell {
    RememberedMapCell {
        object_presentation: None,
        key: key.into(),
        position,
        wall: false,
        material: "m".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: vec![],
    }
}

async fn map_reply(
    server: &mut Server,
    request_id: String,
    query: &MapQuery,
    transfer: &str,
    total: u32,
    cells: Vec<RememberedMapCell>,
    encoded_bytes: Option<usize>,
) -> usize {
    let start = query.cursor.as_ref().map_or(0, |cursor| cursor.next);
    let page = query.cursor.as_ref().map_or(0, |cursor| cursor.page);
    let end = start + cells.len() as u32;
    let mut message = ServerMessage::MapPage {
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
        request_id,
        page: Box::new(MapPage {
            transfer: transfer.into(),
            actor: ActorId(1),
            branch: query.branch.clone(),
            base: query.base,
            window: query.window,
            start,
            page,
            total_cells: total,
            complete: true,
            ambiguous: false,
            limited: false,
            cells,
            next: (end < total).then(|| MapPageCursor {
                transfer: transfer.into(),
                next: end,
                page: page + 1,
            }),
        }),
    };
    if let Some(target) = encoded_bytes {
        let current = serde_json::to_vec(&message).unwrap().len();
        let ServerMessage::MapPage { page, .. } = &mut message else {
            unreachable!()
        };
        assert_eq!(page.cells.len(), 1);
        page.cells[0].material = "m".repeat(1 + target.checked_sub(current).unwrap());
    }
    let ServerMessage::MapPage { page, .. } = &message else {
        unreachable!()
    };
    page.validate().unwrap();
    let text = encode_bounded_json(&message, MAX_MAP_PAGE_BYTES).unwrap();
    let bytes = text.len();
    if let Some(expected) = encoded_bytes {
        assert_eq!(
            bytes, expected,
            "use actual encoded envelope bytes, not synthetic counters"
        );
    }
    server.send(Message::Text(text.into())).await.unwrap();
    bytes
}

async fn observation(server: &mut Server, snapshot: &Snapshot) {
    send(
        server,
        ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: snapshot.context.clone(),
                actor: snapshot.actor,
                branch: snapshot.branch.clone(),
                cursor: snapshot.cursor,
                body: UpdateBody::Observation {
                    state: snapshot.state.clone(),
                    event: None,
                },
            }),
        },
    )
    .await;
}

#[tokio::test]
async fn aggregate_map_headroom_stops_continuations_without_losing_live_updates() {
    timeout(Duration::from_secs(15), aggregate_map_headroom_scenario())
        .await
        .expect("map byte-budget exchange exceeded its 15-second watchdog");
}

async fn aggregate_map_headroom_scenario() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut server = accept_async(stream).await.unwrap();
        assert!(matches!(
            receive(&mut server).await,
            ClientMessage::Hello { .. }
        ));
        send(
            &mut server,
            ServerMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                user: "test".into(),
                actors: vec![ActorId(1)],
                role: AccessRole::Player,
            },
        )
        .await;
        assert!(matches!(receive(&mut server).await,
            ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { actor: ActorId(1) })));
        let initial = snapshot(0, 0);
        send(
            &mut server,
            ServerMessage::Snapshot {
                request_id: "attach".into(),
                snapshot: Box::new(initial),
            },
        )
        .await;
        let (id, query) = map_request(&mut server).await;
        assert_eq!(query.window, MapWindow::normal());
        map_reply(&mut server, id, &query, "baseline", 0, vec![], None).await;
        observation(&mut server, &snapshot(1, -1)).await;

        let (id, x_query) = map_request(&mut server).await;
        assert_eq!(
            x_query.window,
            MapWindow {
                min: Position {
                    x: 31,
                    y: -16,
                    z: -2
                },
                max: Position { x: 31, y: 15, z: 3 },
            }
        );
        let first_bytes = map_reply(
            &mut server,
            id,
            &x_query,
            "x-edge",
            1,
            vec![cell(Position { x: 31, y: 0, z: 0 }, "committed-x-edge")],
            Some(MAX_MAP_PAGE_BYTES / 2),
        )
        .await;
        let mut second_bytes = 0;
        for index in 0..63 {
            let (id, query) = map_request(&mut server).await;
            assert_eq!(
                query.base, x_query.base,
                "slabs must share an observation budget"
            );
            assert_eq!(
                query.window,
                MapWindow {
                    min: Position {
                        x: -31,
                        y: 16,
                        z: -2
                    },
                    max: Position { x: 31, y: 16, z: 3 },
                }
            );
            assert_eq!(query.cursor.as_ref().map_or(0, |cursor| cursor.next), index);
            second_bytes += map_reply(
                &mut server,
                id,
                &query,
                "y-edge",
                65,
                vec![cell(
                    Position {
                        x: index as i32 - 31,
                        y: 16,
                        z: 0,
                    },
                    &format!("staged-y-{index}"),
                )],
                Some(MAX_MAP_PAGE_BYTES),
            )
            .await;
        }
        let aggregate = first_bytes + second_bytes;
        assert!(
            second_bytes < MAX_MAP_TRANSFER_BYTES,
            "each individual transfer is valid"
        );
        assert_eq!(MAX_MAP_TRANSFER_BYTES - aggregate, MAX_MAP_PAGE_BYTES / 2);
        let ClientMessage::Request {
            request_id,
            request,
        } = receive(&mut server).await
        else {
            panic!("the ordinary Save barrier must remain usable")
        };
        assert!(matches!(*request, Request::Save),
            "insufficient aggregate headroom must stop the old continuation before it reaches the wire: {request:?}");
        let live = snapshot(2, -1);
        observation(&mut server, &live).await;
        send(
            &mut server,
            ServerMessage::Ack {
                context: live.reply_context(),
                request_id,
                receipt: RequestReceipt::Immediate {
                    actor: live.actor,
                    branch: live.branch,
                    entry_id: None,
                },
            },
        )
        .await;
        aggregate
    });

    let mut client = Connection::connect(address, "token".into(), ActorId(1), "test")
        .await
        .unwrap();
    let mut pages = 0;
    let mut actual_bytes = 0;
    while pages < 63 {
        let message = client.next().await.unwrap();
        if let ServerMessage::MapPage { page, .. } = &message {
            if page.base.revision == 1 {
                actual_bytes += serde_json::to_vec(&message).unwrap().len();
                if page.transfer == "y-edge" {
                    pages += 1;
                }
            }
        }
    }
    assert_eq!(client.state.map_hydration().bytes, actual_bytes);
    assert_eq!(
        MAX_MAP_TRANSFER_BYTES - actual_bytes,
        MAX_MAP_PAGE_BYTES / 2
    );
    assert!(
        client.state.map_hydration().pending,
        "fixture leaves a valid unfinished continuation"
    );
    // Poll the real automatic request path without waiting for a nonexistent
    // reply. A following ordinary write flushes any wrongly queued map request,
    // so the peer's FIFO Save assertion proves what actually reached the wire.
    {
        let mut next = std::pin::pin!(client.next());
        poll_fn(|cx| {
            assert!(next.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
    }
    let abandoned = client.state.map_hydration().clone();
    let id = client.request(Request::Save).await.unwrap();
    let message = client
        .next()
        .await
        .expect("optional map exhaustion must not disconnect live play");
    assert!(matches!(message, ServerMessage::Update { update }
        if matches!(&update.body, UpdateBody::Observation { state, .. } if state.revision == 2)));
    assert!(abandoned.failed && !abandoned.pending && !abandoned.complete);
    assert!(client.is_synchronized());
    assert_eq!(client.state.state().revision, 2);
    assert_eq!(
        client
            .state
            .map_cell(Position { x: 31, y: 0, z: 0 })
            .unwrap()
            .key,
        "committed-x-edge"
    );
    assert!(
        client
            .state
            .map_memory()
            .all(|cell| !cell.key.starts_with("staged-y-")),
        "an unfinished second slab must not replace committed memory"
    );
    assert!(
        matches!(client.next().await.unwrap(), ServerMessage::Ack { request_id, .. } if request_id == id)
    );
    assert_eq!(server.await.unwrap(), actual_bytes);
}
