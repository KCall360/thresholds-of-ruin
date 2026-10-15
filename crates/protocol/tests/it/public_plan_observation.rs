//! Wire contract tests that compile against the pre-observation protocol.
use serde_json::{json, Value};
use tor_protocol::{ActorTarget, CombatView, DeltaError, InvalidState, StateDelta, StateView};

fn record() -> Value {
    json!({"observed_at":"100","subject_name":"guard","state":{
        "kind":"preparing","ability":null,"remaining_work":"200",
        "aim":{"kind":"disclosed","cell":"past-cell-key","occurrences":[{"x":0,"y":0,"z":0}]}
    }})
}
fn combat(record: Value) -> Value {
    json!({"hp":"10","max_hp":"10","preparation_remaining":null,"preparation_active":false,
        "sequence":null,"recovery_remaining":"0","actors":[],"events":[],"objective":null,
        "victory":false,"dead":false,"terminal":false,"public_plan":record})
}
fn state(record: Value) -> StateView {
    serde_json::from_value(json!({"wizard_game":false,"revision":"1","observation":{
        "actor":"1","self_target":ActorTarget::from_digest([1;32]),"tick":"100",
        "position":{"x":0,"y":0,"z":0},"visible_cells":[],"ground_items":[],
        "inventory":[],"visible_actors":[],"places":[],"ready":false,
        "interactions":null,"motion":null,"combat":combat(record)}}))
    .unwrap()
}
fn rejects(record: &str) {
    let mut wrapper = combat(Value::Null).to_string();
    wrapper = wrapper.replace("\"public_plan\":null", &format!("\"public_plan\":{record}"));
    assert!(
        serde_json::from_slice::<CombatView>(wrapper.as_bytes()).is_err(),
        "{record}"
    );
    if let Ok(value) = serde_json::from_str::<Value>(&wrapper) {
        assert!(
            serde_json::from_value::<CombatView>(value).is_err(),
            "{record}"
        );
    }
}
#[test]
fn past_public_plan_combat_wire_has_required_nullable_record_and_canonical_encoding() {
    for record in [
        Value::Null,
        record(),
        json!({"observed_at":"100","subject_name":"guard","state":{"kind":"no_public_plan"}}),
        json!({"observed_at":"100","subject_name":"guard","state":{"kind":"no_public_plan"}}),
    ] {
        let value = combat(record);
        let parsed: CombatView = serde_json::from_slice(&serde_json::to_vec(&value).unwrap())
            .expect("ordinary combat view must carry the paid historical result");
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    let mut missing = combat(Value::Null);
    missing.as_object_mut().unwrap().remove("public_plan");
    assert!(serde_json::from_value::<CombatView>(missing).is_err());
    let duplicate = combat(Value::Null).to_string().replace(
        "\"public_plan\":null",
        "\"public_plan\":null,\"public_plan\":null",
    );
    assert!(serde_json::from_slice::<CombatView>(duplicate.as_bytes()).is_err());
}
#[test]
fn past_public_plan_wire_rejects_hidden_fields_arrays_and_duplicate_nested_properties() {
    serde_json::from_value::<CombatView>(combat(record())).expect("positive schema control");
    for bad in [
        r#"["100","guard",{"kind":"no_public_plan"}]"#,
        r#"{"observed_at":"100","subject_name":"guard","state":["ready"]}"#,
        r#"{"observed_at":"100","subject_name":"guard","state":{"kind":"preparing","remaining_work":"200","aim":{"kind":"unavailable"}}}"#,
        r#"{"observed_at":"100","subject_name":"guard","state":{"kind":"preparing","ability":null,"remaining_work":"200","aim":{"kind":"disclosed","cell":"x","occurrences":[[0,0,0]]}}}"#,
        r#"{"observed_at":"100","subject_name":"guard","state":{"kind":"preparing","ability":["hidden","Hidden"],"remaining_work":"200","aim":{"kind":"unavailable"}}}"#,
    ] {
        rejects(bad);
    }
    for state in [
        r#"{"kind":"no_public_plan","target_actor":"999"}"#,
        r#"{"target_actor":"999","kind":"no_public_plan"}"#,
        r#"{"kind":"no_public_plan","remaining":"42"}"#,
        r#"{"remaining":"42","kind":"no_public_plan"}"#,
        r#"{"kind":"preparing","ability":null,"ability":null,"remaining_work":"1","aim":{"kind":"unavailable"}}"#,
        r#"{"ability":null,"ability":null,"remaining_work":"1","aim":{"kind":"unavailable"},"kind":"preparing"}"#,
        r#"{"kind":"preparing","ability":null,"remaining_work":"1","aim":{"kind":"unavailable","location":{"region":999}}}"#,
        r#"{"ability":null,"remaining_work":"1","aim":{"location":{"region":999},"kind":"unavailable"},"kind":"preparing"}"#,
    ] {
        let record = format!(r#"{{"observed_at":"100","subject_name":"guard","state":{state}}}"#);
        let mut wrapper = combat(Value::Null).to_string();
        wrapper = wrapper.replace("\"public_plan\":null", &format!("\"public_plan\":{record}"));
        // Value cannot preserve duplicates; actual bytes are the authority here.
        assert!(
            serde_json::from_slice::<CombatView>(wrapper.as_bytes()).is_err(),
            "{record}"
        );
    }
}
#[test]
fn past_public_plan_invalid_delta_is_atomic_and_aim_count_is_bounded() {
    let before = state(record());
    assert_eq!(before.validate(), Ok(()));
    let original = before.clone();
    for bad in [
        json!([]),
        json!([{"x":0,"y":0,"z":0},{"x":0,"y":0,"z":0}]),
        json!([{"x":1,"y":0,"z":0},{"x":0,"y":0,"z":0}]),
    ] {
        let mut invalid = record();
        invalid["state"]["aim"]["occurrences"] = bad;
        let mut after = state(invalid);
        after.revision = 2;
        assert_eq!(after.validate(), Err(InvalidState::InvalidCombat));
        assert_eq!(
            StateDelta::between(&before, &after).unwrap().apply(&before),
            Err(DeltaError::InvalidChange)
        );
        assert_eq!(before, original);
    }
    let mut excessive = record();
    excessive["state"]["aim"]["occurrences"] = json!(vec![
        json!({"x":0,"y":0,"z":0});
        33_usize.pow(3) + 64 * 27 + 3
    ]);
    assert!(
        serde_json::from_slice::<CombatView>(&serde_json::to_vec(&combat(excessive)).unwrap())
            .is_err()
    );
    let mut future = record();
    future["observed_at"] = json!("101");
    assert_eq!(state(future).validate(), Err(InvalidState::InvalidCombat));
}
// Append to protocol integration public_plan_observation after the frozen batch.
#[test]
fn past_public_plan_maximum_escaped_record_fits_and_reassembles_negotiated_frames() {
    use std::sync::Arc;
    use tor_protocol::*;
    let mut record = record();
    record["subject_name"] = json!("\\\"".repeat(128));
    record["state"]["ability"] = json!({"id":"licensed_plan","name":"\\\"".repeat(128)});
    record["state"]["aim"]["cell"] = json!("\\\"".repeat(64));
    record["state"]["aim"]["occurrences"] = json!((0..MAX_PUBLIC_AIM_OCCURRENCES)
        .map(|x| Position {
            x: i32::MIN + x as i32,
            y: i32::MIN,
            z: i32::MIN
        })
        .collect::<Vec<_>>());
    let state = state(record);
    state.validate().unwrap();
    let message = ServerMessage::Snapshot {
        request_id: "public-plan-recovery".into(),
        snapshot: Box::new(Snapshot {
            context: StreamContext {
                stream: StreamId("dded9f7a-1fd4-47b9-8d80-a3420dc407eb".into()),
                epoch: 1,
            },
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
                tick: 100,
            },
            state: Arc::new(state),
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        }),
    };
    assert!(
        encoded_length(&message, MAX_RESPONSE_BYTES)
            .unwrap()
            .is_some(),
        "the maximal historical record fits the ordinary full envelope"
    );
    for limit in [MAX_RESPONSE_BYTES, 65_536] {
        let frames = encode_snapshot(&message, limit, 2 * MAX_SNAPSHOT_BYTES).unwrap();
        assert_eq!(frames.len() == 1, limit == MAX_RESPONSE_BYTES);
        let mut assembler = ResponseAssembler::default();
        let mut result = None;
        for frame in frames {
            assert!(frame.len() <= limit);
            assert!(
                result.is_none(),
                "only complete assembly may publish the record"
            );
            result = assembler.push(decode_response(&frame).unwrap()).unwrap();
        }
        assert_eq!(result, Some(message.clone()));
    }
}
#[test]
fn public_plan_diagnostic_effect_is_a_strict_recipient_only_record() {
    use tor_protocol::DiagnosticEffect;
    let value = serde_json::json!({"type":"observation","target":"1"});
    let decoded: DiagnosticEffect =
        serde_json::from_value(value.clone()).expect("generic diagnostic observation effect");
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    for raw in [
        r#"["observation","1"]"#,
        r#"{"type":"observation","target":"1","target":"2"}"#,
        r#"{"target":"1","target":"2","type":"observation"}"#,
        r#"{"type":"observation","target":"1","public_plan":null}"#,
        r#"{"public_plan":null,"target":"1","type":"observation"}"#,
        r#"{"type":"observation"}"#,
    ] {
        assert!(
            serde_json::from_slice::<DiagnosticEffect>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
}
#[test]
fn public_plan_diagnostic_outcome_wrapper_requires_a_named_object() {
    use tor_protocol::DiagnosticOutcome;
    let canonical =
        serde_json::json!({"result":"applied","effects":[{"type":"observation","target":"1"}]});
    let value: DiagnosticOutcome = serde_json::from_value(canonical.clone()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), canonical);
    for raw in [
        r#"["applied",[{"type":"observation","target":"1"}]]"#,
        r#"{"result":"applied","effects":[{"type":"observation","target":"1"}],"effects":[]}"#,
        r#"{"private_actor":"2","result":"applied","effects":[{"type":"observation","target":"1"}]}"#,
        r#"{"result":"applied","effects":[{"type":"observation","target":"1"}],"private_actor":"2"}"#,
    ] {
        assert!(
            serde_json::from_slice::<DiagnosticOutcome>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
}
#[test]
fn public_plan_diagnostic_effect_count_is_eagerly_bounded() {
    use tor_protocol::DiagnosticOutcome;
    for count in [128, 129] {
        let raw = serde_json::to_vec(&serde_json::json!({"result":"applied","effects":vec![serde_json::json!({"type":"observation","target":"1"});count]})).unwrap();
        assert_eq!(
            serde_json::from_slice::<DiagnosticOutcome>(&raw).is_ok(),
            count == 128,
            "count {count}"
        );
    }
}
