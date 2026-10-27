//! Bounded last-seen appearances, with no live handles or implicit context.
use serde_json::{json, Value};
use tor_protocol::*;

fn base() -> ObservationBase {
    ObservationBase {
        cursor: StreamCursor {
            sequence: 5,
            tick: 12,
        },
        revision: 7,
    }
}
fn query() -> MapQuery {
    MapQuery {
        context: StreamContext {
            stream: StreamId("attachment".into()),
            epoch: 2,
        },
        branch: BranchId("branch".into()),
        base: base(),
        window: MapWindow::normal(),
        cursor: None,
    }
}
fn cell(index: i32) -> RememberedMapCell {
    RememberedMapCell {
        object_presentation: None,
        key: format!("remembered-{index}"),
        position: Position {
            x: index % 63 - 31,
            y: index / 63 - 16,
            z: 0,
        },
        wall: false,
        material: String::new(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: Vec::new(),
    }
}
fn page() -> MapPage {
    MapPage {
        transfer: "transfer".into(),
        actor: ActorId(1),
        branch: query().branch,
        base: base(),
        window: MapWindow::normal(),
        start: 0,
        page: 0,
        total_cells: 1,
        complete: true,
        ambiguous: false,
        limited: false,
        cells: vec![cell(0)],
        next: None,
    }
}
fn response(page: MapPage) -> ServerMessage {
    ServerMessage::MapPage {
        context: ReplyContext {
            input: InputContext {
                stream: query().context,
                readiness_revision: 9,
            },
            actor: ActorId(1),
            branch: query().branch,
            // A readiness event may have advanced the stream since observation.
            cursor: StreamCursor {
                sequence: 6,
                tick: 12,
            },
            revision: 7,
        },
        request_id: "map-read".into(),
        page: Box::new(page),
    }
}
fn request() -> ClientMessage {
    ClientMessage::Request {
        request_id: "map-read".into(),
        request: Box::new(Request::Map { query: query() }),
    }
}
fn appearance_response() -> Value {
    let mut p = page();
    p.cells[0].trap = Some(MapTrapAppearance {
        family: Some(TrapFamily::Rust),
    });
    p.cells[0].door = Some(MapDoorAppearance {
        name: "Wooden door".into(),
        description: "Last seen closed".into(),
        open: false,
        asset: None,
    });
    p.cells[0].ground_items.push(MapItemAppearance {
        presentation: None,
        class: ItemClass::Potion,
        description: "A red flask".into(),
        name: "red potion".into(),
        appearance: "red potion".into(),
        identified: false,
        quantity: u64::MAX,
        asset: None,
    });
    serde_json::to_value(response(p)).unwrap()
}

#[test]
fn map_query_is_read_only_attachment_bound_and_has_required_explicit_cursor() {
    let value = serde_json::to_value(request()).unwrap();
    assert_eq!(
        serde_json::to_value(decode_request(&value.to_string()).unwrap()).unwrap(),
        value
    );
    let req = Request::Map { query: query() };
    for role in [
        AccessRole::Player,
        AccessRole::Spectator,
        AccessRole::Wizard,
    ] {
        assert!(role.permits(&req));
    }
    for field in ["context", "branch", "base", "window", "cursor"] {
        let mut bad = value.clone();
        bad["request"]["query"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode_request(&bad.to_string()).is_err(), "missing {field}");
    }
    for field in ["actor", "world", "region", "readiness_revision", "learn"] {
        let mut bad = value.clone();
        bad["request"]["query"][field] = json!("1");
        assert!(decode_request(&bad.to_string()).is_err(), "extra {field}");
    }
    let mut continued = value;
    continued["request"]["query"]["cursor"] = json!({"transfer":"transfer","next":512,"page":1});
    assert!(decode_request(&continued.to_string()).is_ok());
    for (field, invalid) in [
        ("next", json!(0)),
        ("next", json!(12474)),
        ("page", json!(0)),
        ("page", json!(128)),
        ("transfer", json!("")),
    ] {
        let mut bad = continued.clone();
        bad["request"]["query"]["cursor"][field] = invalid;
        assert!(decode_request(&bad.to_string()).is_err(), "invalid {field}");
    }
}

#[test]
fn map_window_keeps_all_normal_planes_and_checks_volume_and_translation() {
    let normal = MapWindow::normal();
    assert_eq!(normal.volume(), Some(12_474));
    for position in [normal.min, normal.max, Position { x: 0, y: 0, z: 3 }] {
        assert!(normal.contains(position));
    }
    assert!(!normal.contains(Position { x: 0, y: 0, z: 4 }));
    let by = Position {
        x: 10,
        y: -20,
        z: 3,
    };
    assert_eq!(
        normal.checked_translate(by).unwrap().min,
        Position {
            x: -21,
            y: -36,
            z: 1
        }
    );
    assert!(normal
        .checked_translate(Position {
            x: i32::MAX,
            y: 0,
            z: 0
        })
        .is_none());
    assert!(normal
        .checked_translate(Position {
            x: 0,
            y: i32::MIN,
            z: 0
        })
        .is_none());
    for window in [
        MapWindow {
            min: normal.max,
            max: normal.min,
        },
        MapWindow {
            min: normal.min,
            max: Position {
                x: 32,
                ..normal.max
            },
        },
        MapWindow {
            min: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
            max: Position {
                x: i32::MAX,
                y: i32::MAX,
                z: i32::MAX,
            },
        },
    ] {
        assert!(window.validate().is_err());
        assert!(
            serde_json::from_value::<MapWindow>(serde_json::to_value(window).unwrap()).is_err()
        );
    }
}

#[test]
fn map_appearances_roundtrip_without_live_handles_and_with_canonical_wide_counts() {
    let value = appearance_response();
    let decoded = decode_response(&value.to_string()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
    assert_eq!(decoded.reply_context().unwrap().cursor.sequence, 6);
    assert_eq!(
        decode_response(
            &encode_response(&decoded, None, MAX_MAP_PAGE_BYTES)
                .unwrap()
                .text
        )
        .unwrap(),
        decoded
    );
    for path in [
        "/page/cells/0",
        "/page/cells/0/door",
        "/page/cells/0/trap",
        "/page/cells/0/ground_items/0",
    ] {
        for field in [
            "id",
            "target",
            "reachable",
            "approaches",
            "known_supply",
            "known_currency",
            "actor",
            "age",
        ] {
            let mut bad = value.clone();
            bad.pointer_mut(path).unwrap()[field] = json!("forged");
            assert!(decode_response(&bad.to_string()).is_err(), "{path}/{field}");
        }
    }
    for invalid in [
        json!(1),
        json!("01"),
        json!("+1"),
        json!("-1"),
        json!("18446744073709551616"),
        json!("0"),
    ] {
        let mut bad = value.clone();
        bad["page"]["cells"][0]["ground_items"][0]["quantity"] = invalid;
        assert!(decode_response(&bad.to_string()).is_err());
    }
    let mut noncanonical = value.clone();
    noncanonical["page"]["cells"][0]["ground_items"][0]["class"] = json!({"potion":null});
    assert!(
        decode_response(&noncanonical.to_string()).is_err(),
        "item class is a canonical string"
    );
    for field in ["name", "appearance"] {
        let mut bad = value.clone();
        bad["page"]["cells"][0]["ground_items"][0][field] = json!("x".repeat(81));
        assert!(decode_response(&bad.to_string()).is_err());
    }
}

#[test]
fn map_page_allows_same_cell_key_at_distinct_offsets_and_complete_ambiguity() {
    let mut p = page();
    let mut duplicate_key = cell(1);
    duplicate_key.key = p.cells[0].key.clone();
    p.cells.push(duplicate_key);
    p.total_cells = 2;
    p.ambiguous = true;
    assert!(p.validate().is_ok());
    assert!(decode_response(&serde_json::to_string(&response(p.clone())).unwrap()).is_ok());
    p.cells[1].position = p.cells[0].position;
    assert!(
        p.validate().is_err(),
        "one offset cannot carry rival appearances"
    );
    p.cells[1].position = Position { x: 32, y: 0, z: 0 };
    assert!(p.validate().is_err(), "no cell outside requested window");
}

#[test]
fn map_pages_enforce_record_counts_ordinals_and_exact_continuations() {
    let mut first = page();
    first.cells = (0..512).map(cell).collect();
    first.total_cells = 513;
    first.next = Some(MapPageCursor {
        transfer: first.transfer.clone(),
        next: 512,
        page: 1,
    });
    assert!(first.validate().is_ok());
    let message = response(first.clone());
    assert!(decode_response(&serde_json::to_string(&message).unwrap()).is_ok());
    let mut last = first.clone();
    last.page = 1;
    last.start = 512;
    last.cells = vec![cell(512)];
    last.next = None;
    assert!(last.validate().is_ok());
    let mut too_many = first.clone();
    too_many.cells.push(cell(512));
    too_many.next = None;
    assert!(too_many.validate().is_err());
    assert!(decode_response(&serde_json::to_string(&response(too_many)).unwrap()).is_err());
    let value = serde_json::to_value(message).unwrap();
    for (path, invalid) in [
        ("/page/start", json!(1)),
        ("/page/page", json!(128)),
        ("/page/total_cells", json!(12475)),
        ("/page/total_cells", json!(511)),
        ("/page/next", Value::Null),
        ("/page/next/next", json!(511)),
        ("/page/next/page", json!(2)),
        ("/page/next/transfer", json!("other")),
        ("/page/cells", json!([])),
    ] {
        let mut bad = value.clone();
        *bad.pointer_mut(path).unwrap() = invalid;
        assert!(decode_response(&bad.to_string()).is_err(), "invalid {path}");
    }
    let mut impossible = last;
    impossible.page = 127;
    impossible.start = 127;
    impossible.total_cells = 129;
    impossible.next = Some(MapPageCursor {
        transfer: "transfer".into(),
        next: 128,
        page: 128,
    });
    assert!(impossible.validate().is_err());
}

#[test]
fn empty_complete_and_limited_pages_are_explicit_terminal_results() {
    let mut uncertified = page();
    uncertified.complete = false;
    uncertified.limited = true;
    assert!(
        uncertified.validate().is_err(),
        "a limited query cannot publish a prefix"
    );
    assert!(decode_response(&serde_json::to_string(&response(uncertified)).unwrap()).is_err());
    for limited in [false, true] {
        let mut p = page();
        p.cells.clear();
        p.total_cells = 0;
        p.complete = !limited;
        p.limited = limited;
        assert!(p.validate().is_ok());
        assert!(decode_response(&serde_json::to_string(&response(p.clone())).unwrap()).is_ok());
        p.complete = limited;
        assert!(
            p.validate().is_err(),
            "completeness and limited cannot agree"
        );
    }
}

#[test]
fn map_reply_cross_checks_actor_branch_revision_and_nonretrograde_context() {
    let value = serde_json::to_value(response(page())).unwrap();
    for (path, invalid) in [
        ("/context/actor", json!("2")),
        ("/context/branch", json!("other")),
        ("/context/revision", json!("8")),
        ("/context/cursor/sequence", json!("4")),
        ("/context/cursor/tick", json!("11")),
        ("/context/input/stream/stream", json!("")),
        ("/request_id", json!("")),
        ("/page/transfer", json!("x".repeat(129))),
    ] {
        let mut bad = value.clone();
        *bad.pointer_mut(path).unwrap() = invalid;
        assert!(decode_response(&bad.to_string()).is_err(), "invalid {path}");
    }
}

#[test]
fn map_records_reject_positional_missing_duplicate_and_unknown_fields_recursively() {
    let value = appearance_response();
    let records = [
        ("", vec!["type", "context", "request_id", "page"]),
        (
            "/page",
            vec![
                "transfer",
                "actor",
                "branch",
                "base",
                "window",
                "start",
                "page",
                "total_cells",
                "complete",
                "ambiguous",
                "limited",
                "cells",
                "next",
            ],
        ),
        ("/page/base", vec!["cursor", "revision"]),
        ("/page/base/cursor", vec!["sequence", "tick"]),
        ("/page/window", vec!["min", "max"]),
        ("/page/window/min", vec!["x", "y", "z"]),
        (
            "/page/cells/0",
            vec![
                "key",
                "position",
                "wall",
                "material",
                "asset",
                "place_hint",
                "stairs_up",
                "stairs_down",
                "object_presentation",
                "trap",
                "door",
                "ground_items",
            ],
        ),
        ("/page/cells/0/position", vec!["x", "y", "z"]),
        ("/page/cells/0/trap", vec!["family"]),
        (
            "/page/cells/0/door",
            vec!["name", "description", "open", "asset"],
        ),
        (
            "/page/cells/0/ground_items/0",
            vec![
                "class",
                "presentation",
                "description",
                "name",
                "appearance",
                "identified",
                "quantity",
                "asset",
            ],
        ),
        (
            "/context",
            vec!["input", "actor", "branch", "cursor", "revision"],
        ),
        ("/context/input", vec!["stream", "readiness_revision"]),
        ("/context/input/stream", vec!["stream", "epoch"]),
        ("/context/cursor", vec!["sequence", "tick"]),
    ];
    for (path, fields) in records {
        let record = value.pointer(path).unwrap();
        let mut positional = value.clone();
        *positional.pointer_mut(path).unwrap() =
            Value::Array(fields.iter().map(|field| record[*field].clone()).collect());
        assert!(
            decode_response(&positional.to_string()).is_err(),
            "positional {path}"
        );
        let mut unknown = value.clone();
        unknown.pointer_mut(path).unwrap()["extra"] = Value::Null;
        assert!(
            decode_response(&unknown.to_string()).is_err(),
            "unknown at {path}"
        );
        for field in fields {
            let mut missing = value.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                decode_response(&missing.to_string()).is_err(),
                "missing {path}/{field}"
            );
        }
    }
    let encoded = value.to_string();
    for token in [
        "\"next\":null",
        "\"quantity\":\"18446744073709551615\"",
        "\"request_id\":\"map-read\"",
    ] {
        let duplicate = encoded.replacen(token, &format!("{token},{token}"), 1);
        assert!(decode_response(&duplicate).is_err(), "duplicate {token}");
    }
    let value = serde_json::to_value(request()).unwrap();
    for (path, fields) in [
        (
            "/request/query",
            vec!["context", "branch", "base", "window", "cursor"],
        ),
        ("/request/query/context", vec!["stream", "epoch"]),
        ("/request/query/base", vec!["cursor", "revision"]),
    ] {
        let record = value.pointer(path).unwrap();
        let mut bad = value.clone();
        *bad.pointer_mut(path).unwrap() =
            Value::Array(fields.iter().map(|field| record[*field].clone()).collect());
        assert!(
            decode_request(&bad.to_string()).is_err(),
            "positional {path}"
        );
    }
}

#[test]
fn map_byte_cap_counts_entire_correlated_escaped_envelope_and_smaller_host_limit() {
    let mut message = response(page());
    if let ServerMessage::MapPage { request_id, .. } = &mut message {
        *request_id = "\\".repeat(128);
    }
    let overhead = serde_json::to_string(&message).unwrap().len();
    if let ServerMessage::MapPage { page, .. } = &mut message {
        page.cells[0].material = "x".repeat(MAX_MAP_PAGE_BYTES - overhead);
    }
    let encoded = encode_response(&message, None, MAX_RESPONSE_BYTES)
        .unwrap()
        .text;
    assert_eq!(encoded.len(), MAX_MAP_PAGE_BYTES);
    assert_eq!(decode_response(&encoded).unwrap(), message);
    assert!(
        matches!(encode_response(&message, None, MAX_MAP_PAGE_BYTES - 1), Err(EncodeError::TooLarge { limit }) if limit == MAX_MAP_PAGE_BYTES - 1)
    );
    assert!(matches!(
        decode_response(&(encoded + " ")),
        Err(DecodeError::TooLarge {
            limit: MAX_MAP_PAGE_BYTES
        })
    ));
    if let ServerMessage::MapPage { page, .. } = &mut message {
        page.cells[0].material.push('x');
        assert!(page.validate().is_ok(), "the page body alone still fits");
    }
    assert!(matches!(
        encode_response(&message, None, usize::MAX),
        Err(EncodeError::TooLarge {
            limit: MAX_MAP_PAGE_BYTES
        })
    ));
    assert!(
        serde_json::from_value::<ServerMessage>(serde_json::to_value(&message).unwrap()).is_err(),
        "typed envelope also counts correlation metadata"
    );
}

#[test]
fn map_byte_probe_handles_reordered_escaped_discriminators_before_typed_payloads() {
    let text = format!(
        r#"{{"page":{{"unexpected":"{}"}},"ty\u0070e":"map\u005fpage"}}"#,
        "x".repeat(MAX_MAP_PAGE_BYTES)
    );
    assert!(matches!(
        decode_response(&text),
        Err(DecodeError::TooLarge {
            limit: MAX_MAP_PAGE_BYTES
        })
    ));
    let ordinary = json!({"type":"error","scope":{"type":"transport"},"request_id":null,"code":"invalid_request","message":"x".repeat(MAX_MAP_PAGE_BYTES)}).to_string();
    assert!(
        matches!(decode_response(&ordinary), Ok(ServerMessage::Error { .. })),
        "map cap does not narrow unrelated messages"
    );
}

#[test]
fn map_continuations_and_nested_appearances_remain_bounded_named_records() {
    let continuation = json!({"transfer":"transfer","next":1,"page":1});
    assert!(serde_json::from_value::<MapPageCursor>(continuation.clone()).is_ok());
    for invalid in [
        json!(["transfer", 1, 1]),
        json!({"transfer":"transfer","next":1,"page":1,"actor":"1"}),
        json!({"next":1,"page":1}),
    ] {
        assert!(serde_json::from_value::<MapPageCursor>(invalid).is_err());
    }
    let mut query = query();
    query.window = MapWindow {
        min: Position { x: 0, y: 0, z: 0 },
        max: Position { x: 0, y: 0, z: 0 },
    };
    query.cursor = Some(serde_json::from_value(continuation).unwrap());
    assert!(
        query.validate().is_err(),
        "no continuation can exceed the window capacity"
    );
    let response = appearance_response();
    let mut cell = response["page"]["cells"][0].clone();
    cell["ground_items"] = Value::Array(vec![
        cell["ground_items"][0].clone();
        MAX_MAP_PAGE_BYTES / 64 + 1
    ]);
    assert!(
        serde_json::from_value::<RememberedMapCell>(cell).is_err(),
        "nested item collection has an eager finite bound"
    );
    let mut item = response["page"]["cells"][0]["ground_items"][0].clone();
    item["description"] = json!("x".repeat(MAX_MAP_PAGE_BYTES + 1));
    assert!(serde_json::from_value::<MapItemAppearance>(item).is_err());
    for path in [
        "/page/base/revision",
        "/page/base/cursor/sequence",
        "/page/base/cursor/tick",
        "/context/input/stream/epoch",
    ] {
        let mut invalid = response.clone();
        *invalid.pointer_mut(path).unwrap() = json!("01");
        assert!(
            decode_response(&invalid.to_string()).is_err(),
            "noncanonical {path}"
        );
    }
}

#[test]
fn remembered_trap_requires_named_nullable_family_and_never_carries_live_operands() {
    for family in [
        None,
        Some(TrapFamily::Rust),
        Some(TrapFamily::SleepingGas),
        Some(TrapFamily::TrapDoor),
        Some(TrapFamily::Arrow),
        Some(TrapFamily::BearTrap),
        Some(TrapFamily::Teleport),
        Some(TrapFamily::PoisonDart),
        Some(TrapFamily::Mysterious),
    ] {
        let value = MapTrapAppearance { family };
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 1);
        assert_eq!(
            serde_json::from_value::<MapTrapAppearance>(json).unwrap(),
            value
        );
    }
    for invalid in [
        json!({}),
        json!([null]),
        json!({"family":"poison"}),
        json!({"family":{"rust":null}}),
        json!({"family":null,"id":"t_00"}),
        json!({"family":null,"dc":12}),
    ] {
        assert!(serde_json::from_value::<MapTrapAppearance>(invalid).is_err());
    }
    let mut value = appearance_response();
    value["page"]["cells"][0]
        .as_object_mut()
        .unwrap()
        .remove("trap");
    assert!(decode_response(&value.to_string()).is_err());
}
