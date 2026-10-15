use serde_json::{json, Value};
use tor_protocol::*;

fn catalog() -> Value {
    json!({"type":"creature_catalog", "request_id":"catalog-query",
        "context":{"input":{"stream":{"stream":"catalog","epoch":"0"},"readiness_revision":"0"},
            "actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"revision":"0"},
        "page":{"depth":27,"entries":[
            {"selector":"rogue:imp","name":"Imp","details":"Authored imp; racial growth at depth 27"},
            {"selector":"rogue:vampire","name":"Vampire","details":"Authored vampire; finite birth stock"}],
            "next":"rogue:vampire"}})
}

#[test]
fn creature_catalog_reply_round_trips_and_exposes_private_reply_context() {
    for depth in [Value::Null, json!(1), json!(u32::MAX)] {
        let mut value = catalog();
        value["page"]["depth"] = depth;
        let decoded = decode_response(&value.to_string()).expect("named creature catalog reply");
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        assert_eq!(decoded.reply_context().unwrap().actor, ActorId(1));
    }
    let mut empty = catalog();
    empty["page"]["entries"] = json!([]);
    empty["page"]["next"] = Value::Null;
    assert!(decode_response(&empty.to_string()).is_ok());
}

#[test]
fn creature_catalog_rejects_invalid_bounds_order_cursor_and_record_shapes() {
    let valid = catalog();
    let mut invalid = Vec::new();
    for depth in [
        json!(0),
        json!(-1),
        json!(4294967296_u64),
        json!(1.5),
        json!("1"),
    ] {
        let mut value = valid.clone();
        value["page"]["depth"] = depth;
        invalid.push(value);
    }
    for (field, limit) in [("selector", 768), ("name", 80), ("details", 400)] {
        for text in [
            String::new(),
            "x".repeat(limit + 1),
            "é".repeat(limit / 2 + 1),
            "bad\ntext".into(),
        ] {
            let mut value = valid.clone();
            value["page"]["entries"][0][field] = json!(text);
            invalid.push(value);
        }
    }
    for selector in ["two words", "two\twords", "two\u{a0}words"] {
        let mut value = valid.clone();
        value["page"]["entries"][0]["selector"] = json!(selector);
        invalid.push(value);
    }
    for next in [json!("rogue:imp"), json!("missing"), json!("")] {
        let mut value = valid.clone();
        value["page"]["next"] = next;
        invalid.push(value);
    }
    let mut value = valid.clone();
    value["page"]["entries"] = json!([]);
    invalid.push(value);
    let mut value = valid.clone();
    value["page"]["entries"].as_array_mut().unwrap().reverse();
    value["page"]["next"] = Value::Null;
    invalid.push(value);
    let mut value = valid.clone();
    value["page"]["entries"][1] = value["page"]["entries"][0].clone();
    value["page"]["next"] = Value::Null;
    invalid.push(value);
    let mut value = valid.clone();
    value["page"]["entries"] = json!((0..33)
        .map(|i| json!({"selector":format!("a{i:02}"),"name":"A","details":"B"}))
        .collect::<Vec<_>>());
    value["page"]["next"] = Value::Null;
    invalid.push(value);
    for path in ["", "/page", "/page/entries/0"] {
        let mut value = valid.clone();
        value.pointer_mut(path).unwrap()["unexpected"] = json!(true);
        invalid.push(value);
    }
    for field in ["depth", "entries", "next"] {
        let mut value = valid.clone();
        value["page"].as_object_mut().unwrap().remove(field);
        invalid.push(value);
    }
    let mut value = valid.clone();
    value["page"] = json!([27, valid["page"]["entries"], valid["page"]["next"]]);
    invalid.push(value);
    let mut value = valid;
    value["page"]["entries"][0] = json!(["rogue:imp", "Imp", "Details"]);
    invalid.push(value);
    for value in invalid {
        assert!(
            decode_response(&value.to_string()).is_err(),
            "accepted invalid catalog: {value}"
        );
        assert!(
            serde_json::from_value::<ServerMessage>(value.clone()).is_err(),
            "accepted invalid typed catalog: {value}"
        );
    }
}

#[test]
fn creature_catalog_accepts_exact_utf8_byte_and_page_limits() {
    let mut value = catalog();
    value["page"]["entries"] = json!((0..32)
        .map(|i| json!({
        "selector":format!("a{i:02}{}", "é".repeat(382)) + "x",
        "name":"é".repeat(40),"details":"é".repeat(200)}))
        .collect::<Vec<_>>());
    value["page"]["next"] = value["page"]["entries"][31]["selector"].clone();
    assert!(decode_response(&value.to_string()).is_ok());
}

#[test]
fn creature_catalog_collection_bound_precedes_decoding_an_extra_typed_entry() {
    let entries = (0..32)
        .map(|i| json!({"selector":format!("a{i:02}"),"name":"A","details":"B"}))
        .chain(std::iter::once(json!(false)))
        .collect::<Vec<_>>();
    let error = serde_json::from_value::<CreatureCatalogPage>(
        json!({"depth":null,"entries":entries,"next":null}),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("collection exceeds bound"), "{error}");
}

#[test]
fn creature_catalog_direct_validation_rejects_mutated_private_pages() {
    let page: CreatureCatalogPage = serde_json::from_value(catalog()["page"].clone()).unwrap();
    assert!(page.validate().is_ok());
    let mut variants = Vec::new();
    let mut value = page.clone();
    value.depth = Some(0);
    variants.push(value);
    let mut value = page.clone();
    value.entries[0].selector = "bad selector".into();
    variants.push(value);
    let mut value = page.clone();
    value.entries[0].name.clear();
    variants.push(value);
    let mut value = page.clone();
    value.entries[0].details = "x".repeat(401);
    variants.push(value);
    let mut value = page.clone();
    value.entries.swap(0, 1);
    value.next = None;
    variants.push(value);
    let mut value = page.clone();
    value.entries[1] = value.entries[0].clone();
    value.next = None;
    variants.push(value);
    let mut value = page.clone();
    value.entries.clear();
    variants.push(value);
    let mut value = page.clone();
    value.entries = vec![page.entries[0].clone(); 33];
    value.next = None;
    variants.push(value);
    let mut value = page;
    value.next = Some("wrong".into());
    variants.push(value);
    assert!(variants.iter().all(|page| page.validate().is_err()));
}

#[test]
fn creature_catalog_rejects_duplicate_wire_fields() {
    let valid = catalog().to_string();
    for value in [
        valid.replacen("\"depth\":27", "\"depth\":27,\"depth\":28", 1),
        valid.replacen("\"name\":\"Imp\"", "\"name\":\"Imp\",\"name\":\"Other\"", 1),
        valid.replacen(
            "\"request_id\":\"catalog-query\"",
            "\"request_id\":\"catalog-query\",\"request_id\":\"other\"",
            1,
        ),
    ] {
        assert_ne!(value, valid);
        assert!(decode_response(&value).is_err());
    }
}
