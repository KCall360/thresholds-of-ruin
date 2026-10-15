//! Strict typed private source-page contract, independent of backend records.
use serde_json::{json, Value};
use tor_protocol::*;

fn source() -> Value {
    json!({"kind":"ability","id":"page_ability_000"})
}
fn position() -> Value {
    json!({"kind":"strike_header","strike":0})
}
fn cursor() -> Value {
    json!({"inspection":"page-handle","position":{"kind":"strike_descriptor","strike":0,"index":0}})
}
fn request() -> Value {
    json!({"type":"request","request_id":"detail","request":{"type":"command",
        "context":{"stream":{"stream":"attachment","epoch":"1"},"readiness_revision":"1"},
        "branch":"branch","command":{"type":"inspect_strike_source","expected_revision":"1",
            "actor":"2","target_revision":"3","source":source(),"cursor":null}}})
}
fn response() -> Value {
    json!({"type":"creature_strike_page","request_id":"detail","context":{
        "input":{"stream":{"stream":"attachment","epoch":"1"},"readiness_revision":"1"},
        "actor":"1","branch":"branch","cursor":{"sequence":"1","tick":"5"},"revision":"1"},
        "page":{"inspection":"page-handle","actor":"2","target_revision":"3","source":source(),
        "resolved":{"kind":"ability","ability":{"id":"page_ability_000","name":"Technique"}},
        "strike_count":1,"start":position(),"entries":[{"kind":"strike_header","strike":0,"offset":"0",
            "unlock":null,"check":null,"success_count":0,"failure_count":0,"descriptor_count":1,"rider_count":0}],
        "next":cursor()}})
}

#[test]
fn ordered_strikes_detail_request_has_strict_source_target_and_required_nullable_cursor() {
    let good = request();
    let decoded = decode_request(&good.to_string()).expect("typed source page request must decode");
    assert_eq!(serde_json::to_value(decoded).unwrap(), good);
    for field in ["actor", "target_revision", "source", "cursor"] {
        let mut bad = good.clone();
        bad["request"]["command"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode_request(&bad.to_string()).is_err(), "missing {field}");
    }
    for selector in [
        json!({"kind":"natural","id":"unexpected"}),
        json!({"kind":"selected_melee","plan":{}}),
        json!({"kind":"ability","id":"unknown","private_actor":2}),
    ] {
        let mut bad = good.clone();
        bad["request"]["command"]["source"] = selector;
        assert!(decode_request(&bad.to_string()).is_err());
    }
    let mut resumed = good.clone();
    resumed["request"]["command"]["cursor"] = cursor();
    assert!(decode_request(&resumed.to_string()).is_ok());
    resumed["request"]["command"]["cursor"]["position"]["index"] = json!(128);
    assert!(decode_request(&resumed.to_string()).is_err());
    resumed["request"]["command"]["cursor"]["position"]["index"] = json!("0");
    assert!(decode_request(&resumed.to_string()).is_err());
}

#[test]
fn ordered_strikes_detail_page_roundtrips_complete_envelope_and_explicit_continuation() {
    let good = response();
    let decoded = decode_response(&good.to_string()).expect("typed source page reply must decode");
    let encoded = encode_response(&decoded, None, 196_608).unwrap();
    assert_eq!(decode_response(&encoded.text).unwrap(), decoded);
    assert_eq!(serde_json::to_value(decoded).unwrap(), good);
    for field in [
        "inspection",
        "actor",
        "target_revision",
        "source",
        "resolved",
        "strike_count",
        "start",
        "entries",
        "next",
    ] {
        let mut bad = good.clone();
        bad["page"].as_object_mut().unwrap().remove(field);
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "missing {field}"
        );
    }
    let mut last = good.clone();
    last["page"]["start"] = cursor()["position"].clone();
    last["page"]["entries"] = json!([{"kind":"strike_descriptor","strike":0,"index":0,
        "descriptor":{"id":"escaped","name":"\\".repeat(256)}}]);
    last["page"]["next"] = Value::Null;
    assert!(decode_response(&last.to_string()).is_ok());
    let mut bad = last;
    bad["page"]["entries"][0]["future_total"] = json!(9);
    assert!(decode_response(&bad.to_string()).is_err());
}

#[test]
fn ordered_strikes_detail_page_rejects_empty_oversized_or_unbounded_nested_payloads() {
    let good = response();
    decode_response(&good.to_string()).expect("valid initial page");
    for entries in [
        json!([]),
        json!(vec![good["page"]["entries"][0].clone(); 17]),
    ] {
        let mut bad = good.clone();
        bad["page"]["entries"] = entries;
        assert!(decode_response(&bad.to_string()).is_err());
    }
    let field =
        json!({"id":"component","name":"Component","value":{"type":"text","value":"damage"}});
    let mut bad = good.clone();
    bad["page"]["entries"][0]["check"] =
        json!({"id":"check","name":"Check","fields":vec![field;33]});
    assert!(decode_response(&bad.to_string()).is_err());
    bad["page"]["entries"][0]["check"] = json!({"id":"check","name":"Check","fields":[{
        "id":"oversized","name":"Oversized","value":{"type":"text","value":"x".repeat(1024)}}]});
    assert!(
        decode_response(&bad.to_string()).is_err(),
        "field envelope exceeds 1024 bytes"
    );
    let mut bad = good;
    bad["page"]["next"]["inspection"] = json!("different-handle");
    assert!(decode_response(&bad.to_string()).is_err());
}

#[test]
fn ordered_strikes_detail_weapon_page_budget_includes_all_metadata_and_primary_refs() {
    let mut value = response();
    value["request_id"] = json!("\\".repeat(128));
    value["context"]["input"]["stream"]["stream"] = json!("s".repeat(36));
    for path in [
        "/context/input/stream/epoch",
        "/context/input/readiness_revision",
        "/context/cursor/sequence",
        "/context/cursor/tick",
        "/context/revision",
    ] {
        *value.pointer_mut(path).unwrap() = json!(u64::MAX.to_string());
    }
    value["context"]["branch"] = json!("b".repeat(36));
    let content = |id: &str| json!({"id":id,"name":"\\".repeat(256)});
    value["page"]["source"] = json!({"kind":"selected_melee"});
    value["page"]["resolved"] = json!({"kind":"weapon","source":content("equipment"),"socket":63});
    value["page"]["strike_count"] = json!(1);
    value["page"]["start"] = json!({"kind":"weapon"});
    value["page"]["next"] = Value::Null;
    let descriptor = |index| ContentRef {
        id: format!("descriptor_{index:03}_{}", "d".repeat(45)),
        name: "\\".repeat(256),
    };
    let attack = AttackView {
        skill: ContentRef {
            id: "heavy_weaponry".into(),
            name: "\\".repeat(256),
        },
        bonus: 1000,
        wind_up: 1_000_000,
        recovery: 1_000_000,
        damage: DamageView {
            primary: DamageKeyView {
                category: DamageType::Energy,
                descriptor: Some(descriptor(0)),
                sides: None,
            },
            components: (0..32)
                .map(|index| DamageComponentView {
                    category: DamageType::Energy,
                    descriptor: Some(descriptor(index)),
                    amount: DamageAmountView::Fixed { value: 1000 },
                })
                .collect(),
        },
    };
    let existing_attack_json = serde_json::to_value(&attack).unwrap();
    assert_eq!(
        serde_json::from_value::<AttackView>(existing_attack_json.clone()).unwrap(),
        attack
    );
    value["page"]["entries"] = json!([{"kind":"weapon","attack":existing_attack_json}]);
    let decoded =
        decode_response(&value.to_string()).expect("maximum weapon references fit one typed page");
    let encoded = encode_response(&decoded, None, 196_608).unwrap();
    assert!(
        encoded.text.len() <= 172_032,
        "complete envelope exceeds proved bound"
    );
    assert!(encode_response(&decoded, None, encoded.text.len() - 1).is_err());
    assert_eq!(decode_response(&encoded.text).unwrap(), decoded);
}

#[test]
fn ordered_strikes_selected_melee_ability_detail_is_one_explicit_link_not_authored_strikes() {
    let mut value = response();
    value["page"]["source"] = json!({"kind":"ability","id":"basic_melee"});
    value["page"]["resolved"] =
        json!({"kind":"ability","ability":{"id":"basic_melee","name":"Attack"}});
    value["page"]["strike_count"] = json!(0);
    value["page"]["start"] = json!({"kind":"selected_melee"});
    value["page"]["entries"] = json!([{"kind":"selected_melee"}]);
    value["page"]["next"] = Value::Null;
    let decoded =
        decode_response(&value.to_string()).expect("selection marker has its own typed section");
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    value["page"]["entries"][0]["strikes"] = json!([]);
    assert!(decode_response(&value.to_string()).is_err());
}

#[test]
fn ordered_strikes_full_page_budget_checks_128_fields_and_the_entire_response() {
    let mut value = response();
    value["request_id"] = json!("\\".repeat(128));
    value["context"]["input"]["stream"]["stream"] = json!("\\".repeat(128));
    value["context"]["branch"] = json!("\\".repeat(128));
    for path in [
        "/context/input/stream/epoch",
        "/context/input/readiness_revision",
        "/context/cursor/sequence",
        "/context/cursor/tick",
        "/context/revision",
        "/page/actor",
        "/page/target_revision",
    ] {
        *value.pointer_mut(path).unwrap() = json!(u64::MAX.to_string());
    }
    value["page"]["resolved"]["ability"]["name"] = json!("\\".repeat(256));
    value["page"]["start"] = json!({"kind":"success_effect","strike":0,"index":0});
    let fields: Vec<_> = (0..32)
        .map(|index| {
            json!({"id":format!("component_{index}"),
        "name":"n".repeat(64),"value":{"type":"text","value":"\\".repeat(400)}})
        })
        .collect();
    assert!(fields.iter().all(|field| field.to_string().len() <= 1024));
    let mut entries: Vec<_> = (0..4)
        .map(|index| {
            json!({"kind":"success_effect","strike":0,"index":index,
        "row":{"id":"damage","name":"Damage","fields":fields}})
        })
        .collect();
    entries.extend((0..12).map(|index| json!({"kind":"strike_descriptor","strike":0,"index":index,
        "descriptor":{"id":format!("descriptor_{index:03}_{}", "d".repeat(45)),"name":"\\".repeat(256)}})));
    value["page"]["entries"] = json!(entries);
    value["page"]["next"]["position"]["index"] = json!(12);
    let decoded = decode_response(&value.to_string()).expect("all 128 bounded fields fit");
    let encoded = encode_response(&decoded, None, 196_608).unwrap();
    assert!(encoded.text.len() <= 172_032);
    assert_eq!(decode_response(&encoded.text).unwrap(), decoded);
    // Seventeen entries and 129 fields each fail before installation, independently.
    value["page"]["entries"].as_array_mut().unwrap().insert(
        4,
        json!({"kind":"success_effect","strike":0,"index":4,
        "row":{"id":"damage","name":"Damage","fields":[fields[0].clone()]}}),
    );
    value["page"]["entries"].as_array_mut().unwrap().pop();
    assert!(decode_response(&value.to_string()).is_err());
}

#[test]
fn ordered_strikes_weapon_page_requires_nullable_primary_sides() {
    let mut value = response();
    let mut attack = super::creature_inspection::attack();
    attack.damage.primary.sides = None;
    attack.damage.components[0].amount = DamageAmountView::Fixed { value: 4 };
    value["page"]["source"] = json!({"kind":"selected_melee"});
    value["page"]["resolved"] =
        json!({"kind":"weapon","source":{"id":"weapon","name":"Weapon"},"socket":0});
    value["page"]["start"] = json!({"kind":"weapon"});
    value["page"]["entries"] = json!([{"kind":"weapon","attack":attack}]);
    value["page"]["next"] = Value::Null;
    decode_response(&value.to_string())
        .expect("the fixed-damage key requires an explicit null sides");
    value["page"]["entries"][0]["attack"]["damage"]["primary"]
        .as_object_mut()
        .unwrap()
        .remove("sides");
    assert!(
        decode_response(&value.to_string()).is_err(),
        "omission is not an explicit null"
    );
}

#[test]
fn ordered_strikes_detail_fragment_preserves_unlock_primary_branches_and_rider_boundaries() {
    let mut value = response();
    value["page"]["strike_count"] = json!(3);
    let header = |strike, offset| {
        json!({"kind":"strike_header","strike":strike,"offset":offset,
        "unlock":null,"check":null,"success_count":0,"failure_count":0,"descriptor_count":0,"rider_count":0})
    };
    let mut middle = header(1, "10");
    middle["unlock"] = json!({"id":"second_claw","name":"Second Claw"});
    middle["rider_count"] = json!(1);
    value["page"]["entries"] = json!([header(0,"0"),middle,
        {"kind":"rider_header","strike":1,"rider":0,"trigger":"success","check":null,"effect_count":0,"descriptor_count":1},
        {"kind":"rider_descriptor","strike":1,"rider":0,"index":0,"descriptor":{"id":"energy","name":"Energy"}},
        header(2,"20")]);
    value["page"]["next"] = Value::Null;
    let decoded = decode_response(&value.to_string()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    for case in 0..9 {
        let mut bad = value.clone();
        match case {
            0 => bad["page"]["entries"][0]["offset"] = json!("1"),
            1 => bad["page"]["entries"][4]["offset"] = json!("5"),
            2 => bad["page"]["entries"][1]["strike"] = json!(0),
            3 => bad["page"]["entries"][0]["unlock"] = json!({"id":"first_unlock","name":"First"}),
            4 => bad["page"]["entries"][1]["rider_count"] = json!(9),
            5 => bad["page"]["entries"][2]["rider"] = json!(1),
            6 => {
                bad["page"]["entries"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("unlock");
            }
            7 => bad["page"]["entries"][0]["future_rolls"] = json!([]),
            8 => bad["page"]["entries"][2]["descriptor_count"] = json!(129),
            _ => unreachable!(),
        }
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "noncanonical fragment case {case}"
        );
    }
}

#[test]
fn ordered_strikes_detail_rider_trigger_is_required_typed_primary_branch_evidence() {
    let mut value = response();
    value["page"]["entries"][0]["check"] = json!({
        "id":"primary_check","name":"Primary check","fields":[]
    });
    value["page"]["entries"][0]["rider_count"] = json!(1);
    value["page"]["entries"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "kind":"rider_header","strike":0,"rider":0,"trigger":"success",
            "check":null,"effect_count":0,"descriptor_count":0
        }));
    value["page"]["next"] = Value::Null;
    for trigger in ["success", "failure"] {
        value["page"]["entries"][1]["trigger"] = json!(trigger);
        let decoded = decode_response(&value.to_string())
            .expect("typed rider trigger must be disclosed losslessly");
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    let mut checkless_failure = value.clone();
    checkless_failure["page"]["entries"][0]["check"] = Value::Null;
    assert!(
        decode_response(&checkless_failure.to_string()).is_err(),
        "a local checkless primary cannot trigger a failure rider"
    );
    for trigger in [Value::Null, json!("target"), json!({"kind":"success"})] {
        let mut invalid = value.clone();
        invalid["page"]["entries"][1]["trigger"] = trigger;
        assert!(decode_response(&invalid.to_string()).is_err());
    }
    value["page"]["entries"][1]
        .as_object_mut()
        .unwrap()
        .remove("trigger");
    assert!(
        decode_response(&value.to_string()).is_err(),
        "omission cannot default to success"
    );
}

fn ordered_page_envelope(value: &Value, kind: &str, order: usize) -> String {
    let fields = [
        format!("\"context\":{}", value["context"]),
        format!("\"request_id\":{}", value["request_id"]),
        format!("\"page\":{}", value["page"]),
    ];
    match order {
        0 => format!("{{{kind},{}}}", fields.join(",")),
        1 => format!("{{{},{kind}}}", fields.join(",")),
        _ => format!("{{{},{kind},{},{}}}", fields[2], fields[0], fields[1]),
    }
}

#[test]
fn ordered_strikes_detail_decode_caps_the_complete_envelope_before_typed_allocations() {
    let mut value = response();
    value["request_id"] = json!("x".repeat(8 * 1024 * 1024));
    let valid = ordered_page_envelope(&value, "\"type\":\"creature_strike_page\"", 0);
    assert!(valid.len() < MAX_RESPONSE_BYTES);
    assert!(
        matches!(decode_response(&valid), Err(DecodeError::TooLarge { limit })
        if limit == MAX_STRIKE_PAGE_BYTES)
    );
    // A typed-invalid nested body makes admission order observable. The private
    // byte policy must win before its field deserializers or huge owned ID run.
    value["page"]["entries"][0]["check"] = json!("invalid typed row");
    for order in 0..3 {
        let text = ordered_page_envelope(&value, "\"type\":\"creature_strike_page\"", order);
        assert!(
            matches!(decode_response(&text), Err(DecodeError::TooLarge { limit })
            if limit == MAX_STRIKE_PAGE_BYTES),
            "top-level order {order} must admit bytes before typed fields"
        );
    }
    value["request_id"] = json!("detail");
    value["page"]["entries"] = json!(["x".repeat(8 * 1024 * 1024)]);
    let giant_entries = ordered_page_envelope(&value, "\"type\":\"creature_strike_page\"", 1);
    assert!(
        matches!(decode_response(&giant_entries), Err(DecodeError::TooLarge { limit })
        if limit == MAX_STRIKE_PAGE_BYTES),
        "giant invalid entry payload is byte-rejected before typed entry decoding"
    );
}

#[test]
fn ordered_strikes_detail_predecode_recognizes_escaped_root_discriminator_only() {
    let mut value = response();
    value["request_id"] = json!("x".repeat(MAX_STRIKE_PAGE_BYTES));
    value["page"]["entries"] = json!({"invalid":"not a typed entry list"});
    for kind in [
        r#""ty\u0070e":"creature_strike_page""#,
        r#""type":"creature_strike_p\u0061ge""#,
        r#""ty\u0070e":"creature_strike_p\u0061ge""#,
    ] {
        for order in 0..3 {
            let text = ordered_page_envelope(&value, kind, order);
            assert!(
                matches!(decode_response(&text), Err(DecodeError::TooLarge { limit })
                if limit == MAX_STRIKE_PAGE_BYTES),
                "escaped top-level discriminator must keep its cap"
            );
        }
    }
    let ordinary = json!({"type":"error","scope":{"type":"transport"},"request_id":null,
        "code":"invalid_request","message":"x".repeat(MAX_STRIKE_PAGE_BYTES),
        "ignored":{"type":"creature_strike_page"}});
    assert!(
        matches!(
            decode_response(&ordinary.to_string()),
            Ok(ServerMessage::Error { .. })
        ),
        "a nested discriminator cannot lower an ordinary envelope's cap"
    );
    let mut malformed = ordinary;
    malformed["request_id"] = json!([]);
    assert!(
        matches!(
            decode_response(&malformed.to_string()),
            Err(DecodeError::Json(_))
        ),
        "ordinary typed errors retain ordinary classification despite nested discriminator text"
    );
}

#[test]
fn ordered_strikes_detail_predecode_keeps_malformed_and_duplicate_discriminators_closed() {
    let value = response();
    for discriminator in [
        r#""type":null"#,
        r#""type":["creature_strike_page"]"#,
        r#""type":"unknown_response""#,
        r#""type":"error","type":"creature_strike_page""#,
        r#""type":"creature_strike_page","ty\u0070e":"error""#,
    ] {
        assert!(decode_response(&ordered_page_envelope(&value, discriminator, 1)).is_err());
    }
    let missing = format!(
        "{{\"page\":{},\"context\":{},\"request_id\":\"detail\"}}",
        value["page"], value["context"]
    );
    assert!(decode_response(&missing).is_err());
}

#[test]
fn ordered_strikes_detail_predecode_skips_giant_escaped_keys_and_values_without_root_confusion() {
    let mut value = response();
    value["page"]["entries"][0]["check"] = json!("invalid typed row");
    let envelope = ordered_page_envelope(&value, "\"type\":\"creature_strike_page\"", 1);
    for ignored in [
        format!("\"{}\":0", r"\u0078".repeat(128 * 1024)),
        format!("\"ignored\":\"{}\"", r"\u0078".repeat(128 * 1024)),
        format!(
            "\"ignored\":{{\"type\":\"error\",\"text\":\"{}\"}}",
            r"\u0078".repeat(128 * 1024)
        ),
    ] {
        let text = format!("{{{ignored},{}", &envelope[1..]);
        assert!(
            matches!(decode_response(&text), Err(DecodeError::TooLarge { limit })
            if limit == MAX_STRIKE_PAGE_BYTES)
        );
    }
    let unknown = format!("{{\"type\":\"{}\"}}", r"\u0078".repeat(128 * 1024));
    assert!(
        matches!(decode_response(&unknown), Err(DecodeError::Json(_))),
        "an enormous escaped unknown discriminator still fails closed"
    );
}

#[test]
fn ordered_strikes_detail_predecode_retains_depth_syntax_and_outer_frame_guards() {
    let padding = "x".repeat(MAX_STRIKE_PAGE_BYTES);
    for invalid in [
        r#""\q""#,
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\uD800\u0041""#,
        r#""unterminated"#,
        "[{]}",
    ] {
        let text =
            format!("{{\"ignored\":{invalid},\"padding\":\"{padding}\",\"type\":\"error\"}}");
        assert!(
            matches!(decode_response(&text), Err(DecodeError::Json(_))),
            "{invalid}"
        );
    }
    let nested = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_DEPTH),
        "]".repeat(MAX_JSON_DEPTH)
    );
    let text = format!("{{\"ignored\":{nested},\"padding\":\"{padding}\",\"type\":\"error\"}}");
    assert!(
        matches!(decode_response(&text), Err(DecodeError::TooDeep { limit }) if limit == MAX_JSON_DEPTH)
    );
    let ordinary = format!(
        r#"{{"type":"error","scope":{{"type":"transport"}},"request_id":null,"code":"invalid_request","message":"{padding}","ignored":"\uD83D\uDE00"}}"#
    );
    assert!(
        matches!(decode_response(&ordinary), Ok(ServerMessage::Error { .. })),
        "valid ordinary escaped Unicode remains under the ordinary limit"
    );
    assert!(
        matches!(decode_response(&"x".repeat(MAX_RESPONSE_BYTES + 1)),
        Err(DecodeError::TooLarge { limit }) if limit == MAX_RESPONSE_BYTES)
    );
}

fn recovery_context() -> StreamContext {
    StreamContext {
        stream: StreamId("dded9f7a-1fd4-47b9-8d80-a3420dc407eb".into()),
        epoch: 1,
    }
}

fn assemble_encoded_parts(text: &str) -> Result<Option<ServerMessage>, SnapshotAssemblyError> {
    let mut assembler = ResponseAssembler::default();
    let mut result = Ok(None);
    for (index, data) in text.as_bytes().chunks(32 * 1024).enumerate() {
        let part = ServerMessage::SnapshotPart {
            part: Box::new(SnapshotPart {
                request_id: "recovery".into(),
                context: recovery_context(),
                offset: (index * 32 * 1024) as u32,
                total_bytes: text.len() as u32,
                data: std::str::from_utf8(data).unwrap().into(),
            }),
        };
        let frame = encode_response(&part, None, 65_536).unwrap();
        assert!(frame.text.len() <= 65_536);
        result = assembler.push(decode_response(&frame.text).unwrap());
        if result.is_err() {
            break;
        }
        if (index + 1) * 32 * 1024 < text.len() {
            assert!(
                result.as_ref().unwrap().is_none(),
                "partial assembly must never escape"
            );
        }
    }
    assert!(!assembler.is_pending());
    result
}

#[test]
fn ordered_strikes_snapshot_assembly_rejects_non_snapshot_before_inner_typed_body_decode() {
    let mut private = response();
    private["request_id"] = json!("x".repeat(8 * 1024 * 1024));
    private["page"]["entries"][0]["check"] = json!("invalid typed row");
    let error = json!({"type":"error","scope":"invalid typed scope","request_id":null,
        "code":"invalid_request","message":"x".repeat(8 * 1024 * 1024)});
    for text in [
        private.to_string(),
        error.to_string(),
        ordered_page_envelope(&private, r#""ty\u0070e":"creature_strike_p\u0061ge""#, 1),
    ] {
        let result = assemble_encoded_parts(&text);
        assert!(
            matches!(result, Err(SnapshotAssemblyError::InvalidPart)),
            "root-kind admission must reject before typed body errors: {result:?}"
        );
    }
}

#[test]
fn ordered_strikes_snapshot_assembly_rejects_conflicting_duplicate_root_before_typed_body() {
    let body = format!(
        r#""request_id":"{}","snapshot":{{"state":"invalid typed state"}}"#,
        "x".repeat(8 * 1024 * 1024)
    );
    for (first, last) in [
        (r#""type":"snapshot""#, r#""type":"creature_strike_page""#),
        (r#""type":"creature_strike_page""#, r#""type":"snapshot""#),
        (r#""ty\u0070e":"snap\u0073hot""#, r#""type":"error""#),
        (r#""type":"snapshot""#, r#""ty\u0070e":"snapshot""#),
    ] {
        let result = assemble_encoded_parts(&format!("{{{first},{body},{last}}}"));
        assert!(
            matches!(result, Err(SnapshotAssemblyError::InvalidPart)),
            "duplicate root kinds cannot authorize typed Snapshot allocation: {result:?}"
        );
    }
}

#[test]
fn ordered_strikes_legitimate_large_snapshot_keeps_its_independent_32_mib_assembly_budget() {
    use std::sync::Arc;
    let mut state = StateView {
        wizard_game: false,
        revision: 1,
        observation: Observation {
            actor: ActorId(1),
            self_target: ActorTarget::from_digest([1; 32]),
            tick: 1,
            position: Position { x: 0, y: 0, z: 0 },
            ready: true,
            visible_cells: vec![CellView {
                asset: None,
                door: None,
                material: "stone".into(),
                key: String::new(),
                stairs_up: false,
                stairs_down: false,
                position: Position { x: 0, y: 0, z: 0 },
                wall: false,
                place_hint: false,
            }],
            ground_items: vec![],
            inventory: vec![],
            visible_actors: vec![],
            places: vec![],
            interactions: None,
            motion: None,
            combat: None,
        },
    };
    let overhead = encoded_length(&state, MAX_STATE_BYTES).unwrap().unwrap();
    state.observation.visible_cells[0].key = "k".repeat(MAX_STATE_BYTES - overhead);
    state.validate().unwrap();
    let mut message = ServerMessage::Snapshot {
        request_id: "recovery".into(),
        snapshot: Box::new(Snapshot {
            context: recovery_context(),
            readiness: Readiness {
                revision: 1,
                admission: true,
                resume: vec![],
                cancel: vec![],
            },
            intentions: vec![],
            travel: None,
            actor: ActorId(1),
            branch: BranchId("c51be8f1-63e0-4383-b0eb-de605c77ac42".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 1,
            },
            state: Arc::new(state),
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        }),
    };
    assert!(encoded_length(&message, MAX_RESPONSE_BYTES)
        .unwrap()
        .is_none());
    assert!(encoded_length(&message, MAX_SNAPSHOT_BYTES)
        .unwrap()
        .is_some());
    let mut assembler = ResponseAssembler::default();
    let mut result = None;
    for frame in encode_snapshot(&message, 65_536, 2 * MAX_SNAPSHOT_BYTES).unwrap() {
        assert!(frame.len() <= 65_536);
        result = assembler.push(decode_response(&frame).unwrap()).unwrap();
    }
    assert_eq!(result.as_ref(), Some(&message));
    drop(result);
    let ServerMessage::Snapshot {
        request_id,
        snapshot,
    } = &mut message
    else {
        unreachable!()
    };
    Arc::make_mut(&mut snapshot.state).observation.visible_cells[0].key = "origin".into();
    let reordered = format!(
        r#"{{"snapshot":{},"request_id":{},"ty\u0070e":"snap\u0073hot"}}"#,
        serde_json::to_string(snapshot).unwrap(),
        serde_json::to_string(request_id).unwrap()
    );
    assert_eq!(
        assemble_encoded_parts(&reordered).unwrap(),
        Some(message),
        "valid escaped/reordered Snapshot root remains accepted"
    );
}

#[test]
fn ordered_strikes_detail_oversized_duplicate_root_kinds_are_rejected_before_body_decode() {
    let mut value = response();
    value["request_id"] = json!("x".repeat(MAX_STRIKE_PAGE_BYTES));
    value["page"]["entries"][0]["check"] = json!("invalid typed row");
    for kind in [
        r#""type":"creature_strike_page","type":"error""#,
        r#""type":"error","type":"creature_strike_page""#,
        r#""ty\u0070e":"error","type":"creature_strike_p\u0061ge""#,
    ] {
        let text = ordered_page_envelope(&value, kind, 0);
        let rejected = decode_response(&text);
        assert!(
            matches!(&rejected, Err(DecodeError::TooLarge { limit }) if *limit == MAX_STRIKE_PAGE_BYTES)
                || matches!(&rejected, Err(DecodeError::Json(error)) if error.to_string() == "Invalid response discriminator envelope"),
            "the root probe must reject before the typed-invalid nested body: {rejected:?}"
        );
    }
}

#[test]
fn ordered_strikes_detail_root_probe_counts_ordinary_depth_64_and_65_exactly() {
    let base = json!({"type":"error","scope":{"type":"transport"},"request_id":null,
        "code":"invalid_request","message":"x".repeat(MAX_STRIKE_PAGE_BYTES)})
    .to_string();
    for depth in [MAX_JSON_DEPTH, MAX_JSON_DEPTH + 1] {
        let nested = format!("{}0{}", "[".repeat(depth - 1), "]".repeat(depth - 1));
        let text = format!("{{\"ignored\":{nested},{}", &base[1..]);
        if depth == MAX_JSON_DEPTH {
            assert!(matches!(
                decode_response(&text),
                Ok(ServerMessage::Error { .. })
            ));
        } else {
            assert!(
                matches!(decode_response(&text), Err(DecodeError::TooDeep { limit }) if limit == MAX_JSON_DEPTH)
            );
        }
    }
}

#[test]
fn natural_melee_page_preserves_bounded_species_and_ability_rider_provenance() {
    let mut good = response();
    good["page"]["resolved"] = json!({"kind":"natural_melee",
        "species":{"id":"human","name":"Human"},
        "ability":{"id":"page_ability_000","name":"Technique"},
        "additional_riders":1,"natural_riders":[1]});
    good["page"]["entries"][0]["rider_count"] = json!(2);
    let decoded = decode_response(&good.to_string()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), good);
    for case in 0..9 {
        let mut bad = good.clone();
        match case {
            0 => bad["page"]["resolved"]["natural_riders"] = json!([]),
            1 => bad["page"]["resolved"]["natural_riders"] = json!([8]),
            2 => bad["page"]["resolved"]["natural_riders"] = json!(vec![0; 17]),
            3 => bad["page"]["resolved"]["additional_riders"] = json!(9),
            4 => bad["page"]["resolved"]["ability"]["id"] = json!("wrong"),
            5 => bad["page"]["resolved"]["species"]["hidden"] = json!(true),
            6 => {
                bad["page"]["resolved"]
                    .as_object_mut()
                    .unwrap()
                    .remove("natural_riders");
            }
            7 => bad["page"]["entries"][0]["rider_count"] = json!(1),
            8 => {
                bad["page"]["next"]["position"] =
                    json!({"kind":"rider_effect","strike":0,"rider":2,"index":0})
            }
            _ => unreachable!(),
        }
        assert!(decode_response(&bad.to_string()).is_err(), "case {case}");
    }
    good["page"]["entries"] = json!([{"kind":"rider_header","strike":0,"rider":1,
        "trigger":"success","check":null,"effect_count":0,"descriptor_count":0}]);
    good["page"]["start"] = json!({"kind":"rider_header","strike":0,"rider":1});
    good["page"]["next"] = Value::Null;
    assert!(
        decode_response(&good.to_string()).is_ok(),
        "a fragment retains its rider provenance"
    );
}
