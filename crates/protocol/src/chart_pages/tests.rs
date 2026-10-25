use super::*;
use serde_json::{json, Value};

fn component(component: u64, alignment: ChartAlignment) -> ChartRecord {
    ChartRecord::Component(ChartComponent {
        component,
        alignment,
    })
}
fn food(component: u64, position: Option<[i64; 3]>) -> ChartRecord {
    ChartRecord::Row(ChartRow {
        component,
        position,
        mark: ChartMark::Food,
    })
}
fn monster(component: u64, position: Option<[i64; 3]>, name: &str) -> ChartRecord {
    ChartRecord::Row(ChartRow {
        component,
        position,
        mark: ChartMark::Monster {
            name: name.into(),
            asset: None,
        },
    })
}
fn reply(
    records: Vec<ChartRecord>,
    start: u64,
    total_components: u64,
    total_records: u64,
    kind: ChartKind,
) -> ChartPageReply {
    let end = start + records.len() as u64;
    ChartPageReply {
        message_type: ChartReplyType::ChartPage,
        context: crate::ReplyContext {
            input: crate::InputContext {
                stream: StreamContext {
                    stream: crate::StreamId("attachment-1".into()),
                    epoch: 2,
                },
                readiness_revision: 6,
            },
            actor: crate::ActorId(7),
            branch: BranchId("branch-1".into()),
            cursor: crate::StreamCursor {
                sequence: 3,
                tick: 4,
            },
            revision: 5,
        },
        request_id: "request-1".into(),
        page: ChartPage {
            context: StreamContext {
                stream: crate::StreamId("attachment-1".into()),
                epoch: 2,
            },
            branch: BranchId("branch-1".into()),
            base: ObservationBase {
                cursor: crate::StreamCursor {
                    sequence: 3,
                    tick: 4,
                },
                revision: 5,
            },
            chart: "immutable-sample-1".into(),
            kind,
            mode: ChartMode::Historical,
            acquired_at: 1,
            total_components,
            total_records,
            start,
            records,
            next: (end < total_records).then_some(end),
        },
    }
}
fn query(reply: &ChartPageReply) -> ChartQuery {
    ChartQuery {
        context: reply.page.context.clone(),
        branch: reply.page.branch.clone(),
        base: reply.page.base,
        chart: reply.page.chart.clone(),
        cursor: (reply.page.start != 0).then_some(reply.page.start),
    }
}
fn budget() -> ChartAssemblyBudget {
    ChartAssemblyBudget {
        components: 10_000,
        records: 100_000,
        encoded_bytes: 16 * 1024 * 1024,
    }
}
fn positive() -> ChartPageReply {
    let value = reply(
        vec![
            component(0, ChartAlignment::Aligned),
            monster(0, Some([i64::MIN, i64::MAX, 0]), "bat"),
        ],
        0,
        1,
        2,
        ChartKind::Monsters,
    );
    assert_eq!(value.validate(), Ok(()));
    let text = serde_json::to_string(&value).unwrap();
    assert_eq!(decode_chart_page_reply(&text).unwrap(), value);
    let mut stream = ChartStreamValidator::new(crate::ActorId(7), budget());
    stream
        .accept(&query(&value), &value.request_id, &value)
        .unwrap();
    assert!(stream.is_complete());
    value
}
fn bad(value: Value) {
    assert!(
        decode_chart_page_reply(&serde_json::to_string(&value).unwrap()).is_err(),
        "{value}"
    );
}

#[test]
fn strict_named_records_require_every_field_and_reject_unknown_null_and_positional_forms() {
    let value = serde_json::to_value(positive()).unwrap();
    for path in [
        "",
        "/page",
        "/context",
        "/context/input",
        "/context/input/stream",
        "/context/cursor",
        "/page/context",
        "/page/base",
        "/page/base/cursor",
        "/page/records/0",
        "/page/records/1",
        "/page/records/1/mark",
    ] {
        let object = value.pointer(path).unwrap().as_object().unwrap();
        for field in object.keys() {
            let mut missing = value.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            bad(missing);
            // Only these required nullable values admit an explicit JSON null.
            if !matches!(
                (path, field.as_str()),
                ("/page", "next")
                    | ("/page/records/1", "position")
                    | ("/page/records/1/mark", "asset")
            ) {
                let mut null = value.clone();
                null.pointer_mut(path).unwrap()[field] = Value::Null;
                bad(null);
            }
        }
        let mut unknown = value.clone();
        unknown.pointer_mut(path).unwrap()["actor_target"] = json!("forbidden");
        bad(unknown);
        let mut positional = value.clone();
        *positional.pointer_mut(path).unwrap() = Value::Array(object.values().cloned().collect());
        bad(positional);
    }
    let text = serde_json::to_string(&value).unwrap();
    for (single, duplicate) in [
        (
            "\"type\":\"chart_page\"",
            "\"type\":\"chart_page\",\"type\":\"chart_page\"",
        ),
        ("\"position\":[", "\"position\":null,\"position\":["),
        (
            "\"kind\":\"monster\"",
            "\"kind\":\"monster\",\"kind\":\"monster\"",
        ),
        ("\"asset\":null", "\"asset\":null,\"asset\":null"),
    ] {
        assert!(text.contains(single));
        assert!(decode_chart_page_reply(&text.replacen(single, duplicate, 1)).is_err());
    }
}

#[test]
fn query_is_named_bounded_and_requires_nullable_cursor_with_canonical_wide_integers() {
    let q = query(&positive());
    let text = serde_json::to_string(&q).unwrap();
    assert_eq!(decode_chart_query(&text).unwrap(), q);
    let value = serde_json::to_value(q).unwrap();
    for field in value.as_object().unwrap().keys() {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(decode_chart_query(&missing.to_string()).is_err());
    }
    for cursor in [
        json!(0),
        json!("0"),
        json!("01"),
        json!("-1"),
        json!("18446744073709551616"),
        json!({}),
    ] {
        let mut invalid = value.clone();
        invalid["cursor"] = cursor;
        assert!(decode_chart_query(&invalid.to_string()).is_err());
    }
    for token in ["", "with.dot", "with space", "\\", "é"] {
        let mut invalid = value.clone();
        invalid["chart"] = json!(token);
        assert!(decode_chart_query(&invalid.to_string()).is_err());
    }
    let mut valid = value.clone();
    valid["chart"] = json!("x".repeat(128));
    assert!(decode_chart_query(&valid.to_string()).is_ok());
    valid["chart"] = json!("x".repeat(129));
    assert!(decode_chart_query(&valid.to_string()).is_err());
    assert!(
        decode_chart_query(&format!("{}{}", " ".repeat(crate::MAX_REQUEST_BYTES), text)).is_err()
    );
}

#[test]
fn marks_have_no_identity_seed_or_unselected_nullable_siblings_and_keep_exact_byte_limits() {
    for mark in [
        ChartMark::Monster {
            name: "".into(),
            asset: None,
        },
        ChartMark::Monster {
            name: "é".repeat(128),
            asset: Some("a".repeat(80)),
        },
        ChartMark::Food,
        ChartMark::Magic,
    ] {
        let text = serde_json::to_string(&mark).unwrap();
        assert_eq!(serde_json::from_str::<ChartMark>(&text).unwrap(), mark);
        for field in [
            "id", "actor", "item", "seed", "class", "quantity", "health", "target",
        ] {
            let mut value = serde_json::to_value(&mark).unwrap();
            value[field] = Value::Null;
            assert!(serde_json::from_value::<ChartMark>(value).is_err());
        }
    }
    for mark in [
        json!({"kind":"monster","name":"a".repeat(257),"asset":null}),
        json!({"kind":"monster","name":"bad\nname","asset":null}),
        json!({"kind":"monster","name":"bat","asset":"a".repeat(81)}),
        json!({"kind":"monster","name":"bat","asset":"Creature.bat"}),
        json!({"kind":"monster","name":"bat","asset":"creature..bat"}),
        json!({"kind":"monster","name":"bat","asset":""}),
        json!({"kind":"food","name":null}),
        json!({"kind":"magic","asset":null}),
        json!({"kind":"mapping"}),
        json!({"kind":{"food":null}}),
        json!(["food"]),
    ] {
        assert!(serde_json::from_value::<ChartMark>(mark).is_err());
    }
    let mut value = serde_json::to_value(positive()).unwrap();
    for position in [
        json!([0, "0", "0"]),
        json!(["-0", "0", "0"]),
        json!(["9223372036854775808", "0", "0"]),
        json!(["0", "0"]),
        json!(["0", "0", "0", "0"]),
        json!({"x":"0","y":"0","z":"0"}),
    ] {
        value["page"]["records"][1]["position"] = position;
        bad(value.clone());
    }
}

#[test]
fn declarations_stream_beyond_one_page_and_repeated_occurrences_remain_counted() {
    let mut records: Vec<_> = (0..700)
        .map(|n| component(n, ChartAlignment::Aligned))
        .collect();
    records.extend((0..800).map(|_| food(699, Some([4, -2, 9]))));
    let mut stream = ChartStreamValidator::new(crate::ActorId(7), budget());
    let total = records.len() as u64;
    for (part, chunk) in records.chunks(300).enumerate() {
        let value = reply(
            chunk.to_vec(),
            (part * 300) as u64,
            700,
            total,
            ChartKind::Food,
        );
        assert_eq!(value.validate(), Ok(()));
        let decoded = decode_chart_page_reply(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(decoded.page.records, chunk);
        stream
            .accept(&query(&decoded), &decoded.request_id, &decoded)
            .unwrap();
    }
    assert!(stream.is_complete());
    assert_eq!(stream.records_seen(), 1500);
    assert_eq!(stream.components_seen(), 700);
}

#[test]
fn coordinate_presence_and_order_are_validated_across_pages_without_deduplicating() {
    let first = reply(
        vec![
            component(0, ChartAlignment::Aligned),
            component(1, ChartAlignment::Inconsistent),
        ],
        0,
        2,
        6,
        ChartKind::Food,
    );
    let second = reply(
        vec![food(0, Some([4, 2, 9])), food(0, Some([4, 2, 9]))],
        2,
        2,
        6,
        ChartKind::Food,
    );
    let final_page = reply(vec![food(1, None), food(1, None)], 4, 2, 6, ChartKind::Food);
    let mut stream = ChartStreamValidator::new(crate::ActorId(7), budget());
    for value in [&first, &second, &final_page] {
        assert_eq!(value.validate(), Ok(()));
        stream
            .accept(&query(value), &value.request_id, value)
            .unwrap();
    }
    assert!(stream.is_complete());
    assert_eq!(stream.records_seen(), 6);
    assert!(stream
        .accept(&query(&final_page), &final_page.request_id, &final_page)
        .is_err());
    for bad_row in [
        food(0, None),
        food(1, Some([0, 0, 0])),
        food(0, Some([3, 2, 9])),
    ] {
        let mut stream = ChartStreamValidator::new(crate::ActorId(7), budget());
        for value in [&first, &second] {
            stream
                .accept(&query(value), &value.request_id, value)
                .unwrap();
        }
        let bytes = stream.encoded_bytes();
        let malformed = reply(vec![bad_row, food(1, None)], 4, 2, 6, ChartKind::Food);
        assert!(stream
            .accept(&query(&malformed), &malformed.request_id, &malformed)
            .is_err());
        assert_eq!(stream.records_seen(), 4);
        assert_eq!(stream.encoded_bytes(), bytes);
        stream
            .accept(&query(&final_page), &final_page.request_id, &final_page)
            .unwrap();
        assert!(stream.is_complete());
    }
}

#[test]
fn exact_context_base_token_mode_and_request_correlation_cannot_be_rebound_mid_stream() {
    let first = reply(
        vec![component(0, ChartAlignment::Aligned)],
        0,
        1,
        2,
        ChartKind::Food,
    );
    let last = reply(vec![food(0, Some([0, 0, 0]))], 1, 1, 2, ChartKind::Food);
    let mut stream = ChartStreamValidator::new(crate::ActorId(7), budget());
    stream
        .accept(&query(&first), &first.request_id, &first)
        .unwrap();
    let expected = query(&last);
    for change in 0..12 {
        let mut changed = last.clone();
        match change {
            0 => changed.page.context.stream.0.push('x'),
            1 => changed.page.context.epoch += 1,
            2 => changed.page.branch.0.push('x'),
            3 => changed.page.base.cursor.sequence += 1,
            4 => changed.page.base.cursor.tick += 1,
            5 => changed.page.base.revision += 1,
            6 => changed.page.chart.push('x'),
            7 => changed.request_id.push('x'),
            8 => changed.page.mode = ChartMode::Live,
            9 => changed.page.acquired_at += 1,
            10 => {
                changed.page.total_records += 1;
                changed.page.next = Some(2);
            }
            11 => changed.context.actor.0 += 1,
            _ => unreachable!(),
        }
        assert!(stream
            .accept(&expected, &last.request_id, &changed)
            .is_err());
        // Even an attacker rewriting the outstanding query cannot change the
        // already accepted immutable chart header or root selection.
        if change != 7 {
            assert!(stream
                .accept(&query(&changed), &last.request_id, &changed)
                .is_err());
        }
        assert_eq!(stream.records_seen(), 1);
    }
    // General publication/readiness advances are not an observation-base change.
    let mut later_publication = last.clone();
    later_publication.context.cursor.sequence += 1;
    later_publication.context.input.readiness_revision += 1;
    stream
        .accept(&expected, &last.request_id, &later_publication)
        .unwrap();
    assert!(stream.is_complete());
}

#[test]
fn available_empty_chart_keeps_all_components_and_no_fake_rows() {
    let value = reply(
        vec![
            component(0, ChartAlignment::Aligned),
            component(1, ChartAlignment::Inconsistent),
        ],
        0,
        2,
        2,
        ChartKind::Magic,
    );
    assert_eq!(value.validate(), Ok(()));
    let mut stream = ChartStreamValidator::new(crate::ActorId(7), budget());
    stream
        .accept(&query(&value), &value.request_id, &value)
        .unwrap();
    assert!(stream.is_complete());
    assert_eq!(stream.records_seen(), 2);
    assert_eq!(stream.components_seen(), 2);
    let mut truncated = value.clone();
    truncated.page.records.pop();
    assert!(truncated.validate().is_err());
    let mut nonexistent = value;
    nonexistent.page.total_components = 0;
    assert!(nonexistent.validate().is_err());
}

#[test]
fn page_and_assembly_limits_are_checked_without_allocating_claimed_totals() {
    let records: Vec<_> = (0..512)
        .map(|n| component(n, ChartAlignment::Aligned))
        .collect();
    let value = reply(records, 0, 512, 512, ChartKind::Food);
    assert_eq!(value.validate(), Ok(()));
    assert!(decode_chart_page_reply(&serde_json::to_string(&value).unwrap()).is_ok());
    let mut excess = value.clone();
    excess
        .page
        .records
        .push(component(512, ChartAlignment::Aligned));
    excess.page.total_components = 513;
    excess.page.total_records = 513;
    assert!(decode_chart_page_reply(&serde_json::to_string(&excess).unwrap()).is_err());
    let bytes = serde_json::to_vec(&value).unwrap().len() as u64;
    for limits in [
        ChartAssemblyBudget {
            components: 511,
            ..budget()
        },
        ChartAssemblyBudget {
            records: 511,
            ..budget()
        },
        ChartAssemblyBudget {
            encoded_bytes: bytes - 1,
            ..budget()
        },
    ] {
        let mut stream = ChartStreamValidator::new(crate::ActorId(7), limits);
        assert!(stream
            .accept(&query(&value), &value.request_id, &value)
            .is_err());
        assert_eq!(stream.records_seen(), 0);
        assert_eq!(stream.components_seen(), 0);
    }
    let mut exact = ChartStreamValidator::new(
        crate::ActorId(7),
        ChartAssemblyBudget {
            components: 512,
            records: 512,
            encoded_bytes: bytes,
        },
    );
    exact
        .accept(&query(&value), &value.request_id, &value)
        .unwrap();
    assert!(exact.is_complete());
    let mut huge_claim = reply(
        vec![component(0, ChartAlignment::Aligned)],
        0,
        u64::MAX,
        u64::MAX,
        ChartKind::Food,
    );
    assert_eq!(huge_claim.validate(), Ok(()));
    let mut bounded = ChartStreamValidator::new(crate::ActorId(7), budget());
    assert!(bounded
        .accept(&query(&huge_claim), &huge_claim.request_id, &huge_claim)
        .is_err());
    assert_eq!(bounded.components_seen(), 0);
    huge_claim.context.actor = crate::ActorId(8);
    assert!(bounded
        .accept(&query(&huge_claim), &huge_claim.request_id, &huge_claim)
        .is_err());
    let text = serde_json::to_string(&value).unwrap();
    assert!(
        decode_chart_page_reply(&format!("{}{}", " ".repeat(MAX_CHART_PAGE_BYTES), text)).is_err()
    );
    let deep = format!(
        "{}0{}",
        "[".repeat(crate::MAX_JSON_DEPTH + 1),
        "]".repeat(crate::MAX_JSON_DEPTH + 1)
    );
    assert!(matches!(
        decode_chart_page_reply(&deep),
        Err(crate::DecodeError::TooDeep { .. })
    ));
    let oversized = reply(
        vec![monster(0, Some([0, 0, 0]), &"\\".repeat(256)); 512],
        1,
        1,
        513,
        ChartKind::Monsters,
    );
    assert!(oversized.page.validate().is_ok());
    assert!(serde_json::to_vec(&oversized).unwrap().len() > MAX_CHART_PAGE_BYTES);
    assert!(oversized.validate().is_err());
}

#[test]
fn declarations_row_kind_ranges_and_continuations_have_no_permissive_fallback() {
    let value = serde_json::to_value(positive()).unwrap();
    for (path, replacement) in [
        ("/page/records/0/component", json!("1")),
        ("/page/records/0/alignment", json!({"aligned":null})),
        ("/page/records/0/alignment", json!("unplaced")),
        ("/page/records/1/component", json!("1")),
        ("/page/records/1/component", json!(0)),
        ("/page/records/1/component", json!("00")),
        ("/page/records/1/component", json!("18446744073709551616")),
        ("/page/records/1/mark", json!({"kind":"food"})),
        ("/page/total_components", json!("0")),
        ("/page/total_components", json!("3")),
        ("/page/total_records", json!("1")),
        ("/page/total_records", json!("3")),
        ("/page/start", json!("18446744073709551615")),
        ("/page/next", json!("2")),
        ("/page/mode", json!("current")),
        ("/page/kind", json!("mapping")),
        ("/type", json!("observation")),
        ("/request_id", json!("   ")),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(path).unwrap() = replacement;
        bad(changed);
    }
    let mut reversed = value;
    reversed["page"]["records"]
        .as_array_mut()
        .unwrap()
        .reverse();
    bad(reversed);
}

#[test]
fn conservative_envelope_and_records_cover_actual_serde_at_all_scalar_extremes() {
    let mut value = reply(
        vec![monster(
            u64::MAX - 3,
            Some([i64::MIN, i64::MAX, -1]),
            &"\\\"".repeat(128),
        )],
        u64::MAX - 2,
        u64::MAX - 2,
        u64::MAX,
        ChartKind::Monsters,
    );
    value.request_id = "\\".repeat(128);
    value.page.context.stream.0 = "\"".repeat(128);
    value.page.context.epoch = u64::MAX;
    value.page.branch.0 = "\\".repeat(128);
    value.page.base.cursor.sequence = u64::MAX;
    value.page.base.cursor.tick = u64::MAX;
    value.page.base.revision = u64::MAX;
    value.page.chart = "x".repeat(128);
    value.page.acquired_at = u64::MAX;
    value.context.input.stream = value.page.context.clone();
    value.context.input.readiness_revision = u64::MAX;
    value.context.actor = crate::ActorId(u64::MAX);
    value.context.branch = value.page.branch.clone();
    value.context.cursor = value.page.base.cursor;
    value.context.revision = value.page.base.revision;
    let ChartRecord::Row(row) = &mut value.page.records[0] else {
        unreachable!()
    };
    let ChartMark::Monster { asset, .. } = &mut row.mark else {
        unreachable!()
    };
    *asset = Some("a".repeat(80));
    assert_eq!(value.validate(), Ok(()));
    assert!(
        serde_json::to_vec(&value).unwrap().len() <= value.page.encoded_reply_size_bound().unwrap()
    );
    assert_eq!(value.page.records[0].encoded_size_bound(), Ok(2372));
    assert!(CHART_ENVELOPE_BYTES + 2372 + 1 <= MAX_CHART_PAGE_BYTES);
    for record in [
        component(u64::MAX, ChartAlignment::Inconsistent),
        value.page.records[0].clone(),
        food(u64::MAX, None),
        food(u64::MAX, Some([i64::MIN; 3])),
    ] {
        assert!(serde_json::to_vec(&record).unwrap().len() <= record.encoded_size_bound().unwrap());
    }
    // Independent envelope shape: six empty identifiers already include quotes;
    // all fifteen u64 values are maximal canonical strings, enum strings longest.
    let max = u64::MAX.to_string();
    let skeleton = json!({"type":"chart_page","context":{
        "input":{"stream":{"stream":"","epoch":max},"readiness_revision":max},
        "actor":max,"branch":"","cursor":{"sequence":max,"tick":max},"revision":max},
        "request_id":"","page":{
        "context":{"stream":"","epoch":max},"branch":"",
        "base":{"cursor":{"sequence":max,"tick":max},"revision":max},
        "chart":"","kind":"monsters","mode":"historical","acquired_at":max,
        "total_components":max,"total_records":max,"start":max,"records":[],"next":max}});
    assert_eq!(serde_json::to_vec(&skeleton).unwrap().len(), 744);
    assert_eq!(744 + 5 * (6 * 128) + 128, 4712);
    assert!(4712 <= CHART_ENVELOPE_BYTES);
    let keys: Vec<_> = serde_json::to_value(&value).unwrap()["page"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        [
            "acquired_at",
            "base",
            "branch",
            "chart",
            "context",
            "kind",
            "mode",
            "next",
            "records",
            "start",
            "total_components",
            "total_records"
        ]
    );
}

#[test]
fn live_message_dispatch_uses_standard_reply_context_and_complete_page_byte_caps() {
    let value = positive();
    let message = crate::ServerMessage::ChartPage {
        context: value.context.clone(),
        request_id: value.request_id.clone(),
        page: Box::new(value.page.clone()),
    };
    assert_eq!(message.reply_context(), Some(&value.context));
    let encoded = crate::encode_response(&message, None, crate::MAX_RESPONSE_BYTES).unwrap();
    assert_eq!(encoded.text, serde_json::to_string(&value).unwrap());
    assert_eq!(crate::decode_response(&encoded.text).unwrap(), message);
    let request = crate::ClientMessage::Request {
        request_id: value.request_id.clone(),
        request: Box::new(crate::Request::Chart {
            query: query(&value),
        }),
    };
    assert_eq!(
        crate::decode_request(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let mut invalid = serde_json::to_value(&value).unwrap();
    invalid["unexpected"] = json!(null);
    assert!(crate::decode_response(&invalid.to_string()).is_err());
    invalid.as_object_mut().unwrap().remove("unexpected");
    invalid["context"]["cursor"]["tick"] = json!("5");
    assert!(crate::decode_response(&invalid.to_string()).is_err());
    for text in [
        encoded.text.clone(),
        encoded.text.replace("chart_page", "chart\\u005fpage"),
    ] {
        assert!(matches!(
            crate::decode_response(&format!("{}{}", " ".repeat(MAX_CHART_PAGE_BYTES), text)),
            Err(crate::DecodeError::TooLarge {
                limit: MAX_CHART_PAGE_BYTES
            })
        ));
    }
    let oversized = reply(
        vec![monster(0, Some([0, 0, 0]), &"\\".repeat(256)); 512],
        1,
        1,
        513,
        ChartKind::Monsters,
    );
    let message = crate::ServerMessage::ChartPage {
        context: oversized.context,
        request_id: oversized.request_id,
        page: Box::new(oversized.page),
    };
    assert!(matches!(
        crate::encode_response(&message, None, crate::MAX_RESPONSE_BYTES),
        Err(crate::EncodeError::TooLarge {
            limit: MAX_CHART_PAGE_BYTES
        })
    ));
}

fn inactive(kind: ChartKind) -> ChartSummary {
    ChartSummary {
        kind,
        mode: ChartMode::Historical,
        status: ChartStatus::Inactive,
        acquired_at: None,
        chart: None,
        latest_attempt: None,
    }
}
fn state() -> crate::StateView {
    crate::StateView {
        wizard_game: false,
        revision: 10,
        observation: crate::Observation {
            charts: vec![
                inactive(ChartKind::Monsters),
                inactive(ChartKind::Food),
                inactive(ChartKind::Magic),
            ],
            self_appearance: None,
            interactions: None,
            combat: None,
            motion: None,
            places: vec![],
            actor: crate::ActorId(7),
            self_target: crate::ActorTarget::from_digest([7; 32]),
            linked_holds: vec![],
            tick: 10,
            position: crate::Position { x: 0, y: 0, z: 0 },
            visible_cells: vec![],
            ground_items: vec![],
            inventory: vec![],
            visible_actors: vec![],
            ready: true,
        },
    }
}

#[test]
fn strict_catalog_separates_retained_sample_from_latest_empty_or_unavailable_attempt() {
    let mut summary = ChartSummary {
        kind: ChartKind::Food,
        mode: ChartMode::Historical,
        status: ChartStatus::Available,
        acquired_at: Some(5),
        chart: Some("captured-food".into()),
        latest_attempt: Some(ChartAttempt {
            attempted_at: 9,
            status: ChartAttemptStatus::Empty,
        }),
    };
    for attempt in [
        ChartAttemptStatus::Empty,
        ChartAttemptStatus::Positive,
        ChartAttemptStatus::Unavailable,
    ] {
        summary.latest_attempt.as_mut().unwrap().status = attempt;
        assert_eq!(summary.validate(), Ok(()));
        assert_eq!(
            serde_json::from_value::<ChartSummary>(serde_json::to_value(&summary).unwrap())
                .unwrap(),
            summary
        );
        assert_eq!(summary.acquired_at, Some(5));
        assert_eq!(summary.status, ChartStatus::Available);
    }
    let value = serde_json::to_value(&summary).unwrap();
    for path in ["", "/latest_attempt"] {
        for field in value.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<ChartSummary>(missing).is_err());
        }
        let mut unknown = value.clone();
        unknown.pointer_mut(path).unwrap()["target"] = Value::Null;
        assert!(serde_json::from_value::<ChartSummary>(unknown).is_err());
        let mut positional = value.clone();
        *positional.pointer_mut(path).unwrap() = json!([]);
        assert!(serde_json::from_value::<ChartSummary>(positional).is_err());
    }
    for (status, tick, token, valid) in [
        (ChartStatus::Inactive, None, None, true),
        (ChartStatus::Inactive, Some(1), None, false),
        (ChartStatus::Inactive, None, Some("token"), false),
        (ChartStatus::Empty, Some(1), Some("token"), true),
        (ChartStatus::Empty, None, Some("token"), false),
        (ChartStatus::Available, Some(1), None, false),
        (ChartStatus::Unavailable, None, None, true),
        (ChartStatus::Unavailable, Some(1), None, true),
        (ChartStatus::Unavailable, Some(1), Some("token"), false),
    ] {
        summary.status = status;
        summary.acquired_at = tick;
        summary.chart = token.map(str::to_owned);
        assert_eq!(summary.validate().is_ok(), valid);
        assert_eq!(
            serde_json::from_value::<ChartSummary>(serde_json::to_value(&summary).unwrap()).is_ok(),
            valid
        );
    }
    let mut invalid_attempt = value;
    invalid_attempt["latest_attempt"]["status"] = json!("inactive");
    assert!(serde_json::from_value::<ChartSummary>(invalid_attempt).is_err());
}

#[test]
fn observation_and_delta_require_bounded_canonical_catalog_and_preserve_replacement() {
    let base = state();
    assert_eq!(base.validate(), Ok(()));
    let mut encoded = serde_json::to_value(&base).unwrap();
    assert_eq!(
        serde_json::from_value::<crate::StateView>(encoded.clone()).unwrap(),
        base
    );
    encoded["observation"]
        .as_object_mut()
        .unwrap()
        .remove("charts");
    assert!(serde_json::from_value::<crate::StateView>(encoded.clone()).is_err());
    for charts in [
        Value::Null,
        json!({}),
        json!([inactive(ChartKind::Food), inactive(ChartKind::Food)]),
        json!([inactive(ChartKind::Magic), inactive(ChartKind::Food)]),
        json!(vec![inactive(ChartKind::Food); 5]),
    ] {
        encoded["observation"]["charts"] = charts;
        assert!(serde_json::from_value::<crate::StateView>(encoded.clone()).is_err());
    }
    let mut next = base.clone();
    next.revision += 1;
    next.observation.tick += 1;
    next.observation.charts[0] = ChartSummary {
        kind: ChartKind::Monsters,
        mode: ChartMode::Live,
        status: ChartStatus::Available,
        acquired_at: Some(11),
        chart: Some("monster-generation".into()),
        latest_attempt: Some(ChartAttempt {
            attempted_at: 11,
            status: ChartAttemptStatus::Positive,
        }),
    };
    assert_eq!(next.validate(), Ok(()));
    let delta = crate::StateDelta::between(&base, &next).unwrap();
    assert_eq!(delta.clone().apply(&base).unwrap(), next);
    let mut encoded_delta = serde_json::to_value(&delta).unwrap();
    encoded_delta.as_object_mut().unwrap().remove("charts");
    assert!(serde_json::from_value::<crate::StateDelta>(encoded_delta).is_err());
    let mut invalid = delta;
    invalid.charts.push(inactive(ChartKind::Monsters));
    assert_eq!(invalid.apply(&base), Err(crate::DeltaError::InvalidChange));
    let mut cured = next.clone();
    cured.revision += 1;
    cured.observation.tick += 1;
    cured.observation.charts[0] = inactive(ChartKind::Monsters);
    assert_eq!(
        crate::StateDelta::between(&next, &cured)
            .unwrap()
            .apply(&next)
            .unwrap(),
        cured
    );
    next.observation.charts.push(inactive(ChartKind::Monsters));
    assert_eq!(next.validate(), Err(crate::InvalidState::InvalidChart));
    let query = query(&positive());
    assert!(crate::AccessRole::Spectator.permits(&crate::Request::Chart { query }));
}
