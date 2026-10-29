//! Owner-only advancement protocol contract; host authority is tested separately.
use serde_json::{json, Value};
use tor_protocol::{decode_request, decode_response, encode_response};

fn basis() -> Value {
    json!({"actor":"1","branch":"branch","stream":"attachment","reset":"1","build_epoch":"7"})
}
fn purchase() -> Value {
    json!({"owner":"0","pool":"advancement","selection":{"kind":"skill_rank","skill":"perception"}})
}
fn replacement() -> Value {
    json!({"source":{"owner":"0","index":0},"replacement":null})
}
fn proposal() -> Value {
    json!({"kind":"replacement","replacement":replacement()})
}
fn request(command: Value) -> Value {
    json!({"type":"request","request_id":"advancement-request","request":{"type":"command",
        "context":{"stream":{"stream":"attachment","epoch":"1"},"readiness_revision":"3"},
        "branch":"branch","command":command}})
}
fn query(query: Value) -> Value {
    request(json!({"type":"advancement","expected_revision":"5","query":query}))
}
fn response(query: Value, entries: Value, total: u16, next: Option<u16>) -> Value {
    json!({"type":"advancement_page","request_id":"advancement-request","context":{
        "input":{"stream":{"stream":"attachment","epoch":"1"},"readiness_revision":"3"},
        "actor":"1","branch":"branch","cursor":{"sequence":"5","tick":"12"},"revision":"5"},
        "page":{"basis":basis(),"revision":"5","query":query,"total":total,"next":next,"entries":entries}})
}
fn overview() -> Value {
    json!({"kind":"overview","value":{"strength_acquired":0,
        "starting_skills":[{"id":"athletics","name":"Athletics"},{"id":"lore","name":"Lore"}],
        "points":{"earned":"2","spent":"1","unspent":"1"},
        "growth":{"body_package_hit_dice":null,"highest_applied_tier":"2","restorable_hit_dice":"0","restoration_blocker":null,"highest_earned_tier":"2","peak_threat":"2","held_xp":"0","earned_xp":"0","spent_xp":"0","next_threshold":"100","future":{"kind":"disabled"}},
        "timings":{"purchase_recovery":"0","respec_prepare":"600","respec_recovery":"0","received_loss_interval":"600","provisional":true},
        "ration":{"supply":{"id":"ration","name":"Ration"},"quantity":"1","access":"carried_only"},
        "received_loss_remaining":"0","perceived_threat":false,"ready":true,"other_preparation":false,"respec":null}})
}
fn summary() -> Value {
    json!({"kind":"summary","value":{"strength_acquired":0,
        "proposal":proposal(),"resulting_build_epoch":"8","removed_count":1,"refund_count":1,"removed_condition_count":0,
        "health":{"current":"8","maximum":"10","injury":"2","dead":false},
        "health_recovery":{"period_ticks":"600","elapsed_ticks":"12","stopped":false},
        "resources":{
            "stamina":{"resource":{"id":"stamina","name":"Stamina"},"balance":2,"maximum":3},
            "focus":{"resource":{"id":"focus","name":"Focus"},"balance":1,"maximum":1},
            "mana":{"resource":{"id":"mana","name":"Mana"},"balance":0,"maximum":0}},
        "canceled_resource_holds":0}})
}
#[test]
fn owner_advancement_actions_have_exact_basis_and_operation_without_client_candidates() {
    for action in [
        json!({"type":"choose_intellect_expertise","basis":basis(),"choice":{"threshold":1,"expertise":"spear"}}),
        json!({"type":"purchase","basis":basis(),"purchase":purchase()}),
        json!({"type":"respec","basis":basis(),"replacement":replacement(),"supplies":{"kind":"carried"}}),
    ] {
        let value = request(json!({"type":"act","action":action}));
        let decoded =
            decode_request(&value.to_string()).expect("ordinary advancement action must decode");
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        for field in ["candidate", "refund", "held_quantity", "prepare_ticks"] {
            let mut bad = value.clone();
            bad["request"]["command"]["action"][field] = json!(1);
            assert!(
                decode_request(&bad.to_string()).is_err(),
                "client authority field {field}"
            );
        }
    }
}
#[test]
fn owner_advancement_query_variants_roundtrip_without_arbitrary_actor_selector() {
    for page in [
        json!({"kind":"overview"}),
        json!({"kind":"ledger","basis":basis(),"offset":0}),
        json!({"kind":"intellect_expertises","basis":basis(),"offset":999}),
        json!({"kind":"balances","basis":basis(),"offset":0}),
        json!({"kind":"catalog","basis":basis(),"catalog":"talent","offset":32}),
        json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"removed","offset":0}),
    ] {
        let value = query(page);
        let decoded =
            decode_request(&value.to_string()).expect("owner advancement query must decode");
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        let mut bad = value;
        bad["request"]["command"]["actor"] = json!("2");
        assert!(decode_request(&bad.to_string()).is_err());
    }
}
#[test]
fn owner_advancement_pages_roundtrip_complete_correlated_envelope_and_explicit_nulls() {
    for (query, row) in [
        (json!({"kind":"overview"}), overview()),
        (
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"summary","offset":0}),
            summary(),
        ),
    ] {
        let value = response(query, json!([row]), 1, None);
        let decoded =
            decode_response(&value.to_string()).expect("owner advancement page must decode");
        assert_eq!(
            serde_json::to_value(decoded.reply_context().expect("correlated owner reply")).unwrap(),
            value["context"]
        );
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        let encoded = encode_response(&decoded, None, 32768).unwrap();
        assert_eq!(decode_response(&encoded.text).unwrap(), decoded);
        let mut bad = value;
        bad["page"].as_object_mut().unwrap().remove("next");
        assert!(decode_response(&bad.to_string()).is_err());
    }
}

fn ledger_row(index: u16) -> Value {
    json!({"kind":"purchase","value":{"source":{"owner":"0","index":index},
        "pool":{"id":"discipline","name":"Discipline"},
        "selection":{"kind":"talent","talent":format!("talent_{index}")},
        "name":format!("Talent {index}"),"status":"dormant"}})
}
fn ledger_page(count: u16) -> Value {
    response(
        json!({"kind":"ledger","basis":basis(),"offset":0}),
        Value::Array((0..count).map(ledger_row).collect()),
        count,
        None,
    )
}
fn overview_page() -> Value {
    response(json!({"kind":"overview"}), json!([overview()]), 1, None)
}
fn summary_page() -> Value {
    response(
        json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"summary","offset":0}),
        json!([summary()]),
        1,
        None,
    )
}

#[test]
fn owner_advancement_named_map_contract_rejects_ordered_positional_aliases() {
    let good = summary_page();
    decode_response(&good.to_string()).expect("valid complete summary");
    let outer = Value::Array(
        ["type", "context", "request_id", "page"]
            .iter()
            .map(|field| good[*field].clone())
            .collect(),
    );
    assert!(
        decode_response(&outer.to_string()).is_err(),
        "positional page reply envelope"
    );
    let cases = [
        (
            "/page",
            vec!["basis", "revision", "query", "total", "next", "entries"],
        ),
        (
            "/page/basis",
            vec!["actor", "branch", "stream", "reset", "build_epoch"],
        ),
        (
            "/page/query",
            vec!["kind", "basis", "proposal", "section", "offset"],
        ),
        ("/page/query/proposal", vec!["kind", "replacement"]),
        (
            "/page/query/proposal/replacement",
            vec!["source", "replacement"],
        ),
        (
            "/page/query/proposal/replacement/source",
            vec!["owner", "index"],
        ),
        ("/page/entries/0", vec!["kind", "value"]),
        (
            "/page/entries/0/value",
            vec![
                "strength_acquired",
                "proposal",
                "resulting_build_epoch",
                "removed_count",
                "refund_count",
                "removed_condition_count",
                "health",
                "health_recovery",
                "resources",
                "canceled_resource_holds",
            ],
        ),
        (
            "/page/entries/0/value/health",
            vec!["current", "maximum", "injury", "dead"],
        ),
        (
            "/page/entries/0/value/health_recovery",
            vec!["period_ticks", "elapsed_ticks", "stopped"],
        ),
        (
            "/page/entries/0/value/resources",
            vec!["stamina", "focus", "mana"],
        ),
        (
            "/page/entries/0/value/resources/stamina",
            vec!["resource", "balance", "maximum"],
        ),
        (
            "/page/entries/0/value/resources/stamina/resource",
            vec!["id", "name"],
        ),
    ];
    for (path, fields) in cases {
        let mut bad = good.clone();
        let record = bad.pointer_mut(path).unwrap();
        *record = Value::Array(fields.iter().map(|key| record[*key].clone()).collect());
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "positional {path}"
        );
    }
    let good = overview_page();
    decode_response(&good.to_string()).unwrap();
    for (path, fields) in [
        (
            "/page/entries/0/value",
            vec![
                "strength_acquired",
                "starting_skills",
                "points",
                "timings",
                "ration",
                "growth",
                "received_loss_remaining",
                "perceived_threat",
                "ready",
                "other_preparation",
                "respec",
            ],
        ),
        (
            "/page/entries/0/value/starting_skills/0",
            vec!["id", "name"],
        ),
        (
            "/page/entries/0/value/points",
            vec!["earned", "spent", "unspent"],
        ),
        (
            "/page/entries/0/value/timings",
            vec![
                "purchase_recovery",
                "respec_prepare",
                "respec_recovery",
                "received_loss_interval",
                "provisional",
            ],
        ),
        (
            "/page/entries/0/value/ration",
            vec!["supply", "quantity", "access"],
        ),
    ] {
        let mut bad = good.clone();
        let record = bad.pointer_mut(path).unwrap();
        *record = Value::Array(fields.iter().map(|key| record[*key].clone()).collect());
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "positional {path}"
        );
    }
}

#[test]
fn owner_advancement_request_containers_and_all_operands_require_named_maps() {
    let good = request(json!({"type":"act","action":{"type":"respec",
        "basis":basis(),"replacement":{"source":{"owner":"0","index":0},
        "replacement":{"pool":"discipline","selection":{"kind":"talent","talent":"alternate"}}},
        "supplies":{"kind":"carried"}}}));
    decode_request(&good.to_string()).unwrap();
    for (path, fields) in [
        (
            "/request/command/action",
            vec!["type", "basis", "replacement", "supplies"],
        ),
        (
            "/request/command/action/basis",
            vec!["actor", "branch", "stream", "reset", "build_epoch"],
        ),
        (
            "/request/command/action/replacement",
            vec!["source", "replacement"],
        ),
        (
            "/request/command/action/replacement/source",
            vec!["owner", "index"],
        ),
        (
            "/request/command/action/replacement/replacement",
            vec!["pool", "selection"],
        ),
        (
            "/request/command/action/replacement/replacement/selection",
            vec!["kind", "talent"],
        ),
        ("/request/command/action/supplies", vec!["kind"]),
    ] {
        let mut bad = good.clone();
        let record = bad.pointer_mut(path).unwrap();
        *record = Value::Array(fields.iter().map(|key| record[*key].clone()).collect());
        assert!(
            decode_request(&bad.to_string()).is_err(),
            "positional {path}"
        );
    }
    let good = query(json!({"kind":"overview"}));
    decode_request(&good.to_string()).unwrap();
    for path in ["/request/command", "/request/command/query"] {
        let mut bad = good.clone();
        *bad.pointer_mut(path).unwrap() = if path.ends_with("query") {
            json!(["overview"])
        } else {
            json!(["advancement","5",{"kind":"overview"}])
        };
        assert!(
            decode_request(&bad.to_string()).is_err(),
            "positional {path}"
        );
    }
}

#[test]
fn owner_advancement_new_records_reject_missing_unknown_and_duplicate_fields() {
    for good in [summary_page(), overview_page(), ledger_page(1)] {
        decode_response(&good.to_string()).unwrap();
        let mut records = vec![("/page".to_owned(), &good["page"])];
        let mut paths = Vec::new();
        while let Some((path, value)) = records.pop() {
            match value {
                Value::Object(fields) => {
                    paths.push(path.clone());
                    records.extend(
                        fields
                            .iter()
                            .map(|(key, value)| (format!("{path}/{key}"), value)),
                    );
                }
                Value::Array(values) => records.extend(
                    values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| (format!("{path}/{index}"), value)),
                ),
                _ => {}
            }
        }
        for path in paths {
            let keys: Vec<_> = good
                .pointer(&path)
                .unwrap()
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            let mut bad = good.clone();
            bad.pointer_mut(&path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("private_candidate".into(), json!(1));
            assert!(decode_response(&bad.to_string()).is_err(), "unknown {path}");
            for key in keys {
                let mut bad = good.clone();
                bad.pointer_mut(&path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(&key);
                assert!(
                    decode_response(&bad.to_string()).is_err(),
                    "missing {path}/{key}"
                );
            }
        }
        // A raw duplicate survives only in text, never through serde_json::Value.
        let text = good.to_string().replacen(
            "\"build_epoch\":\"7\"",
            "\"build_epoch\":\"7\",\"build_epoch\":\"7\"",
            1,
        );
        assert!(decode_response(&text).is_err());
    }
}

#[test]
fn owner_advancement_u64_id_and_source_bounds_are_strict_at_byte_decode() {
    let good = request(
        json!({"type":"act","action":{"type":"purchase","basis":basis(),"purchase":purchase()}}),
    );
    decode_request(&good.to_string()).unwrap();
    for field in ["actor", "reset", "build_epoch"] {
        for bad_value in [
            json!(7),
            json!("07"),
            json!("+7"),
            json!("-1"),
            json!("18446744073709551616"),
        ] {
            let mut bad = good.clone();
            bad["request"]["command"]["action"]["basis"][field] = bad_value;
            assert!(decode_request(&bad.to_string()).is_err(), "invalid {field}");
        }
    }
    for (field, value) in [
        ("actor", "0"),
        ("actor", "18446744073709551615"),
        ("build_epoch", "0"),
        ("branch", ""),
        ("stream", ""),
    ] {
        let mut bad = good.clone();
        bad["request"]["command"]["action"]["basis"][field] = json!(value);
        assert!(decode_request(&bad.to_string()).is_err(), "invalid {field}");
    }
    for field in ["branch", "stream"] {
        for value in ["x".repeat(129), "bad\nidentity".into(), "   ".into()] {
            let mut bad = good.clone();
            bad["request"]["command"]["action"]["basis"][field] = json!(value);
            assert!(decode_request(&bad.to_string()).is_err());
        }
    }
    let mut max = good.clone();
    max["request"]["command"]["action"]["basis"]["build_epoch"] = json!(u64::MAX.to_string());
    assert!(decode_request(&max.to_string()).is_ok());
    for owner in ["0", "256", "65536", "18446744073709551614"] {
        let mut valid = good.clone();
        valid["request"]["command"]["action"]["purchase"]["owner"] = json!(owner);
        let decoded = decode_request(&valid.to_string()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), valid);
    }
    for value in [
        json!(0),
        json!(256),
        json!(-1),
        json!("00"),
        json!("+1"),
        json!("-1"),
        json!("18446744073709551615"),
        json!("18446744073709551616"),
    ] {
        let mut bad = good.clone();
        bad["request"]["command"]["action"]["purchase"]["owner"] = value;
        assert!(decode_request(&bad.to_string()).is_err());
    }
    for value in ["".into(), "bad id".into(), "a".repeat(61)] {
        let mut bad = good.clone();
        bad["request"]["command"]["action"]["purchase"]["pool"] = json!(value);
        assert!(decode_request(&bad.to_string()).is_err());
    }
}

#[test]
fn owner_advancement_full_escaped_envelope_honors_page_and_negotiated_byte_caps() {
    let mut maximum = ledger_page(32);
    maximum["request_id"] = json!("\\".repeat(128));
    for row in maximum["page"]["entries"].as_array_mut().unwrap() {
        row["value"]["pool"]["name"] = json!("\\".repeat(256));
        row["value"]["name"] = json!("\\".repeat(256));
    }
    let bytes = maximum.to_string();
    assert!(bytes.len() > 32 * 1024);
    assert!(
        matches!(decode_response(&bytes),Err(tor_protocol::DecodeError::TooLarge{limit}) if limit==32*1024)
    );
    // Typed decoding alone has the same leaf/row constraints but no raw envelope;
    // encoding must independently reject this legal DTO at the full page budget.
    let typed: tor_protocol::ServerMessage = serde_json::from_value(maximum).unwrap();
    assert!(
        matches!(encode_response(&typed,None,1024*1024),Err(tor_protocol::EncodeError::TooLarge{limit}) if limit==32*1024)
    );
    let good = overview_page();
    let typed = decode_response(&good.to_string()).unwrap();
    let encoded = encode_response(&typed, None, 32 * 1024).unwrap();
    assert!(encode_response(&typed, None, encoded.text.len() - 1).is_err());
    assert_eq!(
        encode_response(&typed, None, encoded.text.len())
            .unwrap()
            .text,
        encoded.text
    );
    // Unknown or oversized fields still hit the raw page cap before DTO allocation.
    for text in [
        format!(
            "{{\"type\":\"advancement_page\",\"padding\":\"{}\"}}",
            "x".repeat(32768)
        ),
        format!(
            "{{\"padding\":\"{}\",\"type\":\"advancement_page\"}}",
            "x".repeat(32768)
        ),
    ] {
        assert!(
            matches!(decode_response(&text),Err(tor_protocol::DecodeError::TooLarge{limit}) if limit==32768)
        );
    }
}

#[test]
fn owner_advancement_all_page_entry_variants_roundtrip_with_exact_query_section() {
    let rows = [
        (
            json!({"kind":"ledger","basis":basis(),"offset":0}),
            ledger_row(0),
        ),
        (
            json!({"kind":"balances","basis":basis(),"offset":0}),
            json!({"kind":"balance","value":{
            "owner":"0","pool":{"id":"practice","name":"Practice"},"granted":4,"spent":1,"unspent":3,"allowed":["skill_rank","talent"]}}),
        ),
        (
            json!({"kind":"catalog","basis":basis(),"catalog":"skill_rank","offset":0}),
            json!({"kind":"catalog","value":{
            "selection":{"kind":"skill_rank","skill":"perception"},"name":"Perception","form_choices":[]}}),
        ),
        (
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"removed","offset":0}),
            {
                let mut value = ledger_row(0);
                value["kind"] = json!("removed");
                value
            },
        ),
        (
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"refunds","offset":0}),
            json!({"kind":"refund","value":{
            "owner":"0","pool":{"id":"discipline","name":"Discipline"},"points":1}}),
        ),
        (
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"skills","offset":0}),
            json!({"kind":"skill","value":{
            "skill":{"id":"perception","name":"Perception"},"rank":5}}),
        ),
        (
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"talents","offset":0}),
            json!({"kind":"talent","value":{
            "talent":{"id":"root","name":"Root"},"status":"active"}}),
        ),
        (
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"removed_conditions","offset":0}),
            json!({"kind":"removed_condition","value":{
            "condition":{"id":"poison","name":"Poison"},"applications":128,"stacks":256}}),
        ),
    ];
    for (query, row) in rows {
        let value = response(query, json!([row]), 1, None);
        let decoded = decode_response(&value.to_string()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        let mut bad = value;
        bad["page"]["entries"][0]["value"]["untrusted"] = json!(true);
        assert!(decode_response(&bad.to_string()).is_err());
    }
    let mut progress = overview_page();
    progress["page"]["entries"][0]["value"]["respec"] = json!({"intention":"accepted-intention",
        "basis":basis(),"replacement":replacement(),"active":false,"total_ticks":"600","remaining_ticks":"123"});
    decode_response(&progress.to_string()).unwrap();
    let path = "/page/entries/0/value/respec";
    let fields = [
        "intention",
        "basis",
        "replacement",
        "active",
        "total_ticks",
        "remaining_ticks",
    ];
    let mut bad = progress.clone();
    let record = bad.pointer_mut(path).unwrap();
    *record = Value::Array(fields.iter().map(|key| record[*key].clone()).collect());
    assert!(decode_response(&bad.to_string()).is_err());
    for key in fields {
        let mut bad = progress.clone();
        bad.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "missing progress {key}"
        );
    }
}

#[test]
fn owner_advancement_page_bounds_and_continuation_remain_structurally_exact() {
    let empty = ledger_page(0);
    assert!(decode_response(&empty.to_string()).is_ok());
    let full = ledger_page(32);
    assert!(decode_response(&full.to_string()).is_ok());
    assert!(decode_response(&ledger_page(33).to_string()).is_err());
    for (path, value) in [
        ("/page/total", json!(357)),
        ("/page/next", json!(0)),
        ("/page/query/offset", json!(u16::MAX)),
        ("/page/basis/build_epoch", json!("8")),
    ] {
        let mut bad = ledger_page(1);
        *bad.pointer_mut(path).unwrap() = value;
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "bad page {path}"
        );
    }
    let mut continuation = ledger_page(32);
    continuation["page"]["total"] = json!(356);
    continuation["page"]["next"] = json!(32);
    assert!(decode_response(&continuation.to_string()).is_ok());
    continuation["page"]["next"] = json!(31);
    assert!(decode_response(&continuation.to_string()).is_err());
    let mut empty_middle = ledger_page(0);
    empty_middle["page"]["total"] = json!(1);
    assert!(decode_response(&empty_middle.to_string()).is_err());
    let mut wrong = ledger_page(1);
    wrong["page"]["entries"][0] = overview();
    assert!(decode_response(&wrong.to_string()).is_err());
    let mut invalid = overview_page();
    invalid["page"]["entries"][0]["value"]["ration"]["quantity"] = json!("2");
    assert!(decode_response(&invalid.to_string()).is_err());
    let mut invalid = summary_page();
    invalid["page"]["entries"][0]["value"]["resources"]["focus"]["resource"]["id"] = json!("mana");
    assert!(decode_response(&invalid.to_string()).is_err());
}

#[test]
fn owner_advancement_skill_pages_accept_twenty_rows_and_reject_twenty_one() {
    for count in [20, 21] {
        for preview in [false, true] {
            let query = if preview {
                json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"skills","offset":0})
            } else {
                json!({"kind":"catalog","basis":basis(),"catalog":"skill_rank","offset":0})
            };
            let rows = (0..count)
                .map(|index| {
                    let skill = if index == 19 { "breath".into() } else { format!("skill_{index}") };
                    if preview {
                        json!({"kind":"skill","value":{"skill":{"id":skill,"name":"Skill"},"rank":2}})
                    } else {
                        json!({"kind":"catalog","value":{"selection":{"kind":"skill_rank","skill":skill},"name":"Skill","form_choices":[]}})
                    }
                })
                .collect::<Vec<_>>();
            let value = response(query, json!(rows), count, None);
            let decoded = decode_response(&value.to_string());
            if count == 20 {
                let decoded = decoded.unwrap();
                assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
                let encoded = encode_response(&decoded, None, 32768).unwrap();
                assert_eq!(decode_response(&encoded.text).unwrap(), decoded);
            } else {
                assert!(decoded.is_err());
            }
        }
    }
}

#[test]
fn owner_advancement_respec_explicit_ration_is_one_opaque_zero_ordinal_source() {
    let target = tor_protocol::ItemTarget::from_digest([7; 32]);
    let good = request(json!({"type":"act","action":{"type":"respec",
        "basis":basis(),"replacement":replacement(),
        "supplies":{"kind":"explicit","items":[{"cost_ordinal":0,"item":target}]}}}));
    decode_request(&good.to_string()).unwrap();
    for rows in [
        json!([]),
        json!([{"cost_ordinal":1,"item":target}]),
        json!([{"cost_ordinal":0,"item":"7"}]),
        json!([{"cost_ordinal":0,"item":target},{"cost_ordinal":0,"item":tor_protocol::ItemTarget::from_digest([8;32])}]),
        json!([[0, target]]),
        json!([{"cost_ordinal":0,"item":target,"quantity":"1"}]),
    ] {
        let mut bad = good.clone();
        bad["request"]["command"]["action"]["supplies"]["items"] = rows;
        assert!(decode_request(&bad.to_string()).is_err());
    }
}

#[test]
fn owner_form_operands_roundtrip_exactly_and_reject_lossy_or_positional_aliases() {
    let selection =
        json!({"kind":"talent_form","talent":"evocation_initiate","form":"spark_lance.cold"});
    let mut operation = purchase();
    operation["selection"] = selection.clone();
    let good = request(json!({"type":"act","action":{
        "type":"purchase","basis":basis(),"purchase":operation}}));
    assert_eq!(
        serde_json::to_value(decode_request(&good.to_string()).unwrap()).unwrap(),
        good
    );
    for bad in [
        json!({"kind":"talent_form","talent":"evocation_initiate"}),
        json!({"kind":"talent_form","talent":"evocation_initiate","form":null}),
        json!({"kind":"talent","talent":"evocation_initiate","form":"spark_lance.cold"}),
        json!({"kind":"talent_form","talent":"evocation_initiate","form":"spark_lance.cold","extra":0}),
        json!(["talent_form", "evocation_initiate", "spark_lance.cold"]),
    ] {
        let mut value = good.clone();
        value["request"]["command"]["action"]["purchase"]["selection"] = bad;
        assert!(decode_request(&value.to_string()).is_err());
    }
    let bytes = good.to_string().replace(
        "\"form\":\"spark_lance.cold\"",
        "\"form\":\"spark_lance.cold\",\"form\":\"spark_lance.fire\"",
    );
    assert!(decode_request(&bytes).is_err());
}

#[test]
fn owner_form_catalog_keeps_bounded_options_nested_under_4096_roots() {
    let options: Vec<_> = (0..16)
        .map(|n| json!({"id":format!("form_{n}"),"name":format!("Form {n}")}))
        .collect();
    let row = json!({"kind":"catalog","value":{"selection":{"kind":"talent","talent":"root"},"name":"Root","form_choices":options}});
    let good = response(
        json!({"kind":"catalog","basis":basis(),"catalog":"talent","offset":4095}),
        json!([row]),
        4096,
        None,
    );
    assert_eq!(
        serde_json::to_value(decode_response(&good.to_string()).unwrap()).unwrap(),
        good
    );
    for malformed in [
        Value::Null,
        json!([]),
        json!([["form_0", "Form 0"]]),
        json!([{"id":"form_0","name":"Form 0"},{"id":"form_0","name":"Other"}]),
        Value::Array(
            (0..17)
                .map(|n| json!({"id":format!("form_{n}"),"name":"Form"}))
                .collect(),
        ),
    ] {
        let mut value = good.clone();
        value["page"]["entries"][0]["value"]["form_choices"] = malformed.clone();
        // An empty option list is valid for an ordinary talent root.
        assert_eq!(
            decode_response(&value.to_string()).is_ok(),
            malformed == json!([])
        );
    }
    let mut missing = good.clone();
    missing["page"]["entries"][0]["value"]
        .as_object_mut()
        .unwrap()
        .remove("form_choices");
    assert!(decode_response(&missing.to_string()).is_err());
    let mut flattened = good;
    flattened["page"]["total"] = json!(65536);
    assert!(decode_response(&flattened.to_string()).is_err());
}

#[test]
fn future_growth_source_actions_and_previews_are_exact_and_have_no_xp_authority() {
    for source in [
        json!({"kind":"racial"}),
        json!({"kind":"class","class":"warrior"}),
    ] {
        let action = json!({"type":"choose_growth_source","basis":basis(),"source":source});
        let good = request(json!({"type":"act","action":action}));
        assert_eq!(
            serde_json::to_value(decode_request(&good.to_string()).unwrap()).unwrap(),
            good
        );
        let preview = query(
            json!({"kind":"preview","basis":basis(),"proposal":{"kind":"growth_source","source":source},"section":"summary","offset":0}),
        );
        assert_eq!(
            serde_json::to_value(decode_request(&preview.to_string()).unwrap()).unwrap(),
            preview
        );
        for field in ["xp", "seed", "threat", "candidate", "points"] {
            let mut bad = good.clone();
            bad["request"]["command"]["action"][field] = json!(1);
            assert!(decode_request(&bad.to_string()).is_err());
        }
        for bad_source in [
            json!({"kind":"racial","class":"warrior"}),
            json!({"kind":"class"}),
            json!({"kind":"class","class":"INVALID"}),
            json!({"kind":"disabled"}),
            json!(["racial"]),
        ] {
            let mut bad = good.clone();
            bad["request"]["command"]["action"]["source"] = bad_source;
            assert!(decode_request(&bad.to_string()).is_err());
        }
    }
}

#[test]
fn growth_overview_requires_complete_checked_accounting_and_future_policy() {
    let mut good = overview_page();
    good["page"]["entries"][0]["value"]["growth"] = json!({
        "body_package_hit_dice":null,"highest_applied_tier":"2","restorable_hit_dice":"0","restoration_blocker":null,"highest_earned_tier":"2","peak_threat":"3","held_xp":"60","earned_xp":"160","spent_xp":"100",
        "next_threshold":"100","future":{"kind":"selected","source":{"kind":"racial"}}
    });
    decode_response(&good.to_string()).expect("complete earned progress");
    for field in [
        "body_package_hit_dice",
        "highest_applied_tier",
        "restorable_hit_dice",
        "restoration_blocker",
        "highest_earned_tier",
        "peak_threat",
        "held_xp",
        "earned_xp",
        "spent_xp",
        "next_threshold",
        "future",
    ] {
        let mut bad = good.clone();
        bad["page"]["entries"][0]["value"]["growth"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "missing {field}"
        );
    }
    for policy in [
        json!({"kind":"disabled","source":{"kind":"racial"}}),
        json!({"kind":"selected"}),
        json!({"kind":"authored","seed":1}),
    ] {
        let mut bad = good.clone();
        bad["page"]["entries"][0]["value"]["growth"]["future"] = policy;
        assert!(decode_response(&bad.to_string()).is_err());
    }
    let mut bad = good;
    bad["page"]["entries"][0]["value"]["growth"]["held_xp"] = json!("61");
    assert!(
        decode_response(&bad.to_string()).is_err(),
        "XP ledger must conserve earned XP"
    );
}

#[test]
fn hit_die_restoration_disclosure_is_bounded_by_applied_tier_and_has_a_closed_blocker() {
    let mut good = overview_page();
    let growth = &mut good["page"]["entries"][0]["value"]["growth"];
    growth["highest_applied_tier"] = json!("2");
    growth["highest_earned_tier"] = json!("3");
    growth["restorable_hit_dice"] = json!("1");
    growth["restoration_blocker"] = json!({"kind":"missing_source"});
    decode_response(&good.to_string()).expect("owner can distinguish earned and applied ceilings");
    for (field, value) in [
        ("highest_applied_tier", json!("0")),
        ("highest_applied_tier", json!("4")),
        ("restorable_hit_dice", json!("3")),
        ("restorable_hit_dice", json!("0")),
        (
            "restoration_blocker",
            json!({"kind":"missing_source","class":"mage"}),
        ),
        ("restoration_blocker", json!({"kind":"guessed_source"})),
    ] {
        let mut invalid = good.clone();
        invalid["page"]["entries"][0]["value"]["growth"][field] = value;
        assert!(
            decode_response(&invalid.to_string()).is_err(),
            "invalid {field}"
        );
    }
    good["page"]["entries"][0]["value"]["growth"]["restoration_blocker"] =
        json!({"kind":"missing_health_root"});
    decode_response(&good.to_string()).expect("original Health provenance blocker");
}

#[test]
fn expertise_summary_is_named_required_bounded_and_compact() {
    use tor_protocol::AdvancementExpertiseRow;
    let row =
        json!({"expertise":{"id":"spear","name":"Spear"},"status":"active","source_count":257});
    let value: AdvancementExpertiseRow = serde_json::from_value(row.clone()).unwrap();
    assert_eq!(value.source_count, 257);
    assert!(serde_json::to_vec(&value).unwrap().len() < 256);
    for count in [0, 258] {
        let mut bad = row.clone();
        bad["source_count"] = json!(count);
        assert!(serde_json::from_value::<AdvancementExpertiseRow>(bad).is_err());
    }
    for field in ["expertise", "status", "source_count"] {
        let mut bad = row.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<AdvancementExpertiseRow>(bad).is_err());
    }
    let mut bad = row.clone();
    bad["origins"] = json!([]);
    assert!(serde_json::from_value::<AdvancementExpertiseRow>(bad).is_err());
    assert!(serde_json::from_value::<AdvancementExpertiseRow>(json!([
        row["expertise"],
        "active",
        257
    ]))
    .is_err());
    let text = serde_json::to_string(&row).unwrap();
    assert!(
        serde_json::from_str::<AdvancementExpertiseRow>(&text.replace(
            "\"source_count\":257",
            "\"source_count\":257,\"source_count\":1"
        ))
        .is_err()
    );
}

#[test]
fn expertise_purchase_catalog_and_preview_use_server_disclosed_rows() {
    let choice = json!({"kind":"expertise","expertise":"unknown_but_well_formed"});
    let decoded: tor_protocol::PurchaseChoice = serde_json::from_value(choice.clone()).unwrap();
    assert_eq!(decoded.kind(), tor_protocol::AdvancementKind::Expertise);
    assert_eq!(serde_json::to_value(decoded).unwrap(), choice);
    let catalog = json!({"selection":choice,"name":"Example","form_choices":[]});
    assert!(serde_json::from_value::<tor_protocol::AdvancementCatalogRow>(catalog.clone()).is_ok());
    let mut bad = catalog;
    bad["form_choices"] = json!([{"id":"form","name":"Form"}]);
    assert!(serde_json::from_value::<tor_protocol::AdvancementCatalogRow>(bad).is_err());
    let balance = json!({"owner":"0","pool":{"id":"general","name":"General"},"granted":3,"spent":0,"unspent":3,"allowed":["skill_rank","talent","expertise"]});
    assert!(serde_json::from_value::<tor_protocol::AdvancementBalanceRow>(balance.clone()).is_ok());
    let mut bad = balance;
    bad["allowed"] = json!(["expertise", "talent", "expertise"]);
    assert!(serde_json::from_value::<tor_protocol::AdvancementBalanceRow>(bad).is_err());
}

#[test]
fn expertise_preview_section_rejects_wrong_rows() {
    let row =
        json!({"expertise":{"id":"spear","name":"Spear"},"status":"dormant","source_count":257});
    let q = json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"expertises","offset":0});
    let valid = response(
        q.clone(),
        json!([{"kind":"expertise","value":row}]),
        1,
        None,
    );
    assert!(decode_response(&serde_json::to_string(&valid).unwrap()).is_ok());
    let bad = response(
        q,
        json!([{"kind":"talent","value":{"talent":{"id":"training","name":"Training"},"status":"active"}}]),
        1,
        None,
    );
    assert!(decode_response(&serde_json::to_string(&bad).unwrap()).is_err());
}

#[test]
fn intellect_expertise_action_and_preview_use_strict_distinct_operands() {
    for threshold in [1, 1000] {
        let choice = json!({"threshold":threshold,"expertise":"spear"});
        let action = request(json!({"type":"act","action":{
            "type":"choose_intellect_expertise","basis":basis(),"choice":choice}}));
        let preview = query(json!({"kind":"preview","basis":basis(),"proposal":{
            "kind":"intellect_expertise","choice":choice},"section":"summary","offset":0}));
        for value in [&action, &preview] {
            assert_eq!(
                serde_json::to_value(decode_request(&value.to_string()).unwrap()).unwrap(),
                *value
            );
        }
        for bad_choice in [
            json!({"threshold":0,"expertise":"spear"}),
            json!({"threshold":1001,"expertise":"spear"}),
            json!({"threshold":"1","expertise":"spear"}),
            json!({"threshold":1,"expertise":"bad id"}),
            json!({"threshold":1}),
            json!({"threshold":1,"expertise":"spear","pool":"general"}),
            json!({"threshold":1,"expertise":"spear","owner":"0"}),
        ] {
            let mut bad = action.clone();
            bad["request"]["command"]["action"]["choice"] = bad_choice.clone();
            assert!(decode_request(&bad.to_string()).is_err());
            let mut bad = preview.clone();
            bad["request"]["command"]["query"]["proposal"]["choice"] = bad_choice;
            assert!(decode_request(&bad.to_string()).is_err());
        }
    }
}

#[test]
fn intellect_expertise_pages_bound_threshold_rows_and_require_nullable_choice() {
    let make_page = |offset: u16, threshold, status, expertise| {
        response(
            json!({"kind":"intellect_expertises","basis":basis(),"offset":offset}),
            json!([{"kind":"intellect_expertise","value":{"threshold":threshold,"status":status,"expertise":expertise}}]),
            1000,
            if offset == 999 {
                None
            } else {
                Some(offset + 1)
            },
        )
    };
    for (offset, threshold, status, expertise) in [
        (0, 1, "active", Value::Null),
        (999, 1000, "dormant", json!({"id":"spear","name":"Spear"})),
    ] {
        let good = make_page(offset, threshold, status, expertise);
        let typed = decode_response(&good.to_string()).unwrap();
        assert_eq!(serde_json::to_value(&typed).unwrap(), good);
        let encoded = encode_response(&typed, None, 32 * 1024).unwrap();
        assert!(encode_response(&typed, None, encoded.text.len() - 1).is_err());
        for field in ["expertise", "threshold", "status"] {
            let mut bad = good.clone();
            bad["page"]["entries"][0]["value"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                decode_response(&bad.to_string()).is_err(),
                "required {field}"
            );
        }
        for threshold in [0, 1001] {
            let mut bad = good.clone();
            bad["page"]["entries"][0]["value"]["threshold"] = json!(threshold);
            assert!(decode_response(&bad.to_string()).is_err());
        }
        let mut bad = good.clone();
        bad["page"]["total"] = json!(1001);
        assert!(decode_response(&bad.to_string()).is_err());
        let mut bad = good.clone();
        bad["page"]["query"]["offset"] = json!(1000);
        assert!(decode_response(&bad.to_string()).is_err());
        let mut bad = good.clone();
        bad["page"]["entries"][0]["value"]["history"] = json!([]);
        assert!(decode_response(&bad.to_string()).is_err());
        let mut bad = good;
        bad["page"]["query"]["kind"] = json!("ledger");
        assert!(decode_response(&bad.to_string()).is_err());
    }
}

#[test]
fn intellect_expertise_pages_keep_the_shared_entry_and_full_frame_budgets() {
    let rows = |count| {
        Value::Array(
            (1..=count)
                .map(|threshold| {
                    json!({"kind":"intellect_expertise","value":{
                        "threshold":threshold,"status":"active","expertise":null
                    }})
                })
                .collect(),
        )
    };
    let page_query = json!({"kind":"intellect_expertises","basis":basis(),"offset":0});
    let good = response(page_query.clone(), rows(32), 1000, Some(32));
    assert!(decode_response(&good.to_string()).is_ok());
    let too_many = response(page_query, rows(33), 1000, Some(33));
    assert!(decode_response(&too_many.to_string()).is_err());
    let mut oversized = good;
    oversized["page"]["entries"][0]["value"]["expertise"] =
        json!({"id":"spear","name":"x".repeat(32768)});
    assert!(
        matches!(decode_response(&oversized.to_string()), Err(tor_protocol::DecodeError::TooLarge { limit }) if limit == 32 * 1024)
    );
}

#[test]
fn permanent_potion_strength_is_required_bounded_and_not_a_refund_row() {
    for mut page in [
        overview_page(),
        response(
            json!({"kind":"preview","basis":basis(),"proposal":proposal(),"section":"summary","offset":0}),
            json!([summary()]),
            1,
            None,
        ),
    ] {
        for count in [0, 3, 7] {
            page["page"]["entries"][0]["value"]["strength_acquired"] = json!(count);
            decode_response(&page.to_string()).unwrap();
        }
        for invalid in [json!(8), json!(-1), json!("1"), Value::Null] {
            let mut bad = page.clone();
            bad["page"]["entries"][0]["value"]["strength_acquired"] = invalid;
            assert!(decode_response(&bad.to_string()).is_err());
        }
        page["page"]["entries"][0]["value"]
            .as_object_mut()
            .unwrap()
            .remove("strength_acquired");
        assert!(decode_response(&page.to_string()).is_err());
    }
}

#[test]
fn owned_growth_disclosure_separates_personal_tiers_from_authored_body_loss_capacity() {
    let ordinary = overview_page();
    decode_response(&ordinary.to_string()).expect("explicit null retains ordinary domain");
    let mut owned = ordinary.clone();
    let growth = &mut owned["page"]["entries"][0]["value"]["growth"];
    growth["body_package_hit_dice"] = json!("5");
    growth["highest_applied_tier"] = json!("0");
    growth["highest_earned_tier"] = json!("0");
    decode_response(&owned.to_string()).expect("body-only actor has no personal advancement");
    for loss in [1, 5] {
        let mut drained = owned.clone();
        drained["page"]["entries"][0]["value"]["growth"]["restorable_hit_dice"] =
            json!(loss.to_string());
        decode_response(&drained.to_string())
            .expect("body losses do not require personal earned tiers");
    }
    let mut both = owned.clone();
    let growth = &mut both["page"]["entries"][0]["value"]["growth"];
    growth["highest_applied_tier"] = json!("2");
    growth["highest_earned_tier"] = json!("3");
    growth["restorable_hit_dice"] = json!("7");
    decode_response(&both.to_string())
        .expect("personal losses plus full authored package capacity");
    let mut maximum = owned.clone();
    maximum["page"]["entries"][0]["value"]["growth"]["body_package_hit_dice"] = json!("256");
    maximum["page"]["entries"][0]["value"]["growth"]["restorable_hit_dice"] = json!("256");
    decode_response(&maximum.to_string()).expect("bounded authored package maximum");
    for (field, value) in [
        ("body_package_hit_dice", Value::Null),
        ("body_package_hit_dice", json!("0")),
        ("body_package_hit_dice", json!("257")),
        ("body_package_hit_dice", json!(5)),
        ("body_package_hit_dice", json!("05")),
        ("highest_applied_tier", json!("1")),
        ("restorable_hit_dice", json!("6")),
        ("earned_xp", json!("1")),
    ] {
        let mut bad = owned.clone();
        bad["page"]["entries"][0]["value"]["growth"][field] = value;
        assert!(
            decode_response(&bad.to_string()).is_err(),
            "invalid owned {field}"
        );
    }
    let mut missing = ordinary.clone();
    missing["page"]["entries"][0]["value"]["growth"]
        .as_object_mut()
        .unwrap()
        .remove("body_package_hit_dice");
    assert!(
        decode_response(&missing.to_string()).is_err(),
        "ownership domain is required, never defaulted"
    );
    let mut overflow = owned.clone();
    let growth = &mut overflow["page"]["entries"][0]["value"]["growth"];
    growth["highest_applied_tier"] = json!(u64::MAX.to_string());
    growth["highest_earned_tier"] = json!(u64::MAX.to_string());
    assert!(
        decode_response(&overflow.to_string()).is_err(),
        "loss ceiling cannot overflow"
    );
    let mut excess = both.clone();
    excess["page"]["entries"][0]["value"]["growth"]["restorable_hit_dice"] = json!("8");
    assert!(decode_response(&excess.to_string()).is_err());
    let mut unowned = ordinary;
    unowned["page"]["entries"][0]["value"]["growth"]["restorable_hit_dice"] = json!("3");
    assert!(
        decode_response(&unowned.to_string()).is_err(),
        "unowned loss bound remains personal applied tier"
    );
}
