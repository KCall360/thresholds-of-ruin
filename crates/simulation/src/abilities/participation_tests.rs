//! Closed participation evidence is semantic data, never a permissive event bit.
use super::{AttemptedParticipation, CheckParticipation, Outcome, PayloadParticipation};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn outcome() -> Value {
    json!({"actor":1,"target":2,"ability":null,"intention":null,
        "result":"unaffected","check":null,"target_rider_participated":false,"effects":[],
        "participation":{"check":"none","payload":{"kind":"empty"}}})
}
fn read(value: &Value) -> Result<Outcome, serde_json::Error> {
    serde_json::from_slice(&serde_json::to_vec(value).unwrap())
}
#[test]
fn public_plan_review_participation_outcome_consistency_matrix() {
    for (actor, opposed, evidence, valid) in [
        ("user", false, "user_only", true),
        ("user", false, "target_involved", true), // frozen target defense
        ("target", false, "target_involved", true),
        ("user", true, "target_involved", true),
        ("target", true, "target_involved", true),
        ("user", false, "none", false),
        ("target", false, "user_only", false),
        ("user", true, "user_only", false),
    ] {
        let mut value = outcome();
        value["check"] = json!({"actor":actor,"success":false,"opposed":opposed});
        value["participation"]["check"] = json!(evidence);
        value["result"] = json!(if actor == "user" {
            "miss"
        } else {
            "unaffected"
        });
        assert_eq!(read(&value).is_ok(), valid, "{actor}/{opposed}/{evidence}");
    }
    for evidence in ["user_only", "target_involved"] {
        let mut value = outcome();
        value["participation"]["check"] = json!(evidence);
        assert!(
            read(&value).is_err(),
            "selected checked branch needs an actual check"
        );
    }
    for (payload, effect, valid) in [
        ("target", json!({"kind":"heal","amount":0}), true),
        ("source", json!({"kind":"heal","amount":0}), false),
        ("empty", json!({"kind":"heal","amount":0}), false),
        (
            "source",
            json!({"kind":"observe_public_plan","observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}}}),
            true,
        ),
        (
            "target",
            json!({"kind":"observe_public_plan","observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}}}),
            false,
        ),
    ] {
        let mut value = outcome();
        value["participation"]["payload"] = json!({"kind":payload});
        if effect["kind"] == "observe_public_plan" {
            value["result"] = json!("applied");
        }
        value["effects"] = json!([effect]);
        assert_eq!(read(&value).is_ok(), valid, "surviving recipient {payload}");
    }
    for (reason, check, status) in [
        ("immune", "user_only", "unaffected"),
        ("immune", "target_involved", "unaffected"),
        ("dead", "none", "unaffected"),
        ("unavailable_target", "none", "miss"),
    ] {
        let mut value = outcome();
        value["participation"] =
            json!({"check":check,"payload":{"kind":"unselected","reason":reason}});
        value["result"] = json!(status);
        assert!(read(&value).is_ok(), "closed boundary {reason}/{check}");
        for field in ["effects", "check", "result"] {
            let mut forged = value.clone();
            forged[field] = match field {
                "effects" => json!([{"kind":"heal","amount":0}]),
                "check" => json!({"actor":"user","success":false,"opposed":false}),
                _ => json!("applied"),
            };
            assert!(
                read(&forged).is_err(),
                "blocked {reason} cannot carry {field}"
            );
        }
        value["participation"]["check"] = json!(if check == "none" {
            "target_involved"
        } else {
            "none"
        });
        assert!(
            read(&value).is_err(),
            "closed boundary check mismatch {reason}"
        );
    }
    let mut miss = outcome();
    miss["result"] = json!("miss");
    assert!(
        read(&miss).is_err(),
        "no-check miss requires unavailable-target boundary"
    );
    miss["check"] = json!({"actor":"user","success":false,"opposed":false});
    miss["participation"]["check"] = json!("user_only");
    assert!(read(&miss).is_ok());
    miss["check"]["success"] = json!(true);
    assert!(read(&miss).is_err());
    assert!(!AttemptedParticipation::source().target_participated());
    assert!(!AttemptedParticipation::empty().target_participated());
    assert!(AttemptedParticipation::target().target_participated());
    assert!(AttemptedParticipation {
        check: CheckParticipation::TargetInvolved,
        payload: PayloadParticipation::Empty
    }
    .target_participated());
}

fn ordered(value: &Value, reverse: bool) -> String {
    let fields: Vec<_> = value.as_object().unwrap().iter().collect();
    let fields: Vec<_> = if reverse {
        fields.into_iter().rev().collect()
    } else {
        fields
    };
    format!(
        "{{{}}}",
        fields
            .into_iter()
            .map(|(key, value)| format!("{}:{}", serde_json::to_string(key).unwrap(), value))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn strict<T: DeserializeOwned + Serialize>(value: Value) {
    for reverse in [false, true] {
        let raw = ordered(&value, reverse);
        let decoded: T = serde_json::from_slice(raw.as_bytes()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        for field in value.as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_slice::<T>(ordered(&missing, reverse).as_bytes()).is_err(),
                "missing {field} in {raw}"
            );
            let duplicate = format!(
                "{},{}:{}}}",
                &raw[..raw.len() - 1],
                serde_json::to_string(field).unwrap(),
                value[field]
            );
            assert!(
                serde_json::from_slice::<T>(duplicate.as_bytes()).is_err(),
                "duplicate {field}"
            );
        }
        let mut extra = value.clone();
        extra["unexpected"] = json!(0);
        assert!(serde_json::from_slice::<T>(ordered(&extra, reverse).as_bytes()).is_err());
    }
    let alias = Value::Array(value.as_object().unwrap().values().cloned().collect());
    assert!(serde_json::from_value::<T>(alias.clone()).is_err());
    assert!(serde_json::from_slice::<T>(&serde_json::to_vec(&alias).unwrap()).is_err());
}
#[test]
fn public_plan_review_participation_current_bytes_are_named_required_closed() {
    strict::<AttemptedParticipation>(json!({"check":"none","payload":{"kind":"source"}}));
    for value in [
        json!({"kind":"empty"}),
        json!({"kind":"source"}),
        json!({"kind":"target"}),
        json!({"kind":"unselected","reason":"immune"}),
        json!({"kind":"unselected","reason":"dead"}),
        json!({"kind":"unselected","reason":"unavailable_target"}),
    ] {
        strict::<PayloadParticipation>(value);
    }
    strict::<super::ResolvedCheck>(json!({"actor":"target","success":false,"opposed":true}));
    strict::<Outcome>(outcome());
    for raw in [
        r#"{"check":"future","payload":{"kind":"empty"}}"#,
        r#"{"check":"none","payload":{"kind":"unselected","reason":"future"}}"#,
        r#"{"check":"none","payload":{"kind":"future"}}"#,
    ] {
        assert!(serde_json::from_slice::<AttemptedParticipation>(raw.as_bytes()).is_err());
    }
}
#[test]
fn public_plan_review_participation_plan_bytes_are_named_required_closed() {
    let check = json!({"actor":"user","target_participates":true,"bonus":0,"threshold":{"kind":"fixed","value":10},"edge":0});
    strict::<super::PlannedCheck>(check.clone());
    strict::<super::CheckThreshold>(json!({"kind":"fixed","value":10}));
    strict::<super::CheckThreshold>(json!({"kind":"opposed","bonus":0,"edge":0}));
    let rider = json!({"trigger":"success","condition":null,"check":check.clone(),"effects":[],"descriptors":[]});
    strict::<super::FrozenRider>(rider.clone());
    strict::<super::FrozenStrike>(
        json!({"offset":0,"origin":{"kind":"ability","ordinal":0},"check":check.clone(),"on_success":[],"on_failure":[],"descriptors":[],"riders":[rider],"edge":0,"shared_check":{"phase":"primary","side":"initiating"}}),
    );
    for (actor, threshold) in [
        ("target", json!({"kind":"fixed","value":10})),
        ("user", json!({"kind":"opposed","bonus":0,"edge":0})),
    ] {
        let mut value = check.clone();
        value["actor"] = json!(actor);
        value["threshold"] = threshold;
        value["target_participates"] = json!(false);
        assert!(serde_json::from_slice::<super::PlannedCheck>(
            &serde_json::to_vec(&value).unwrap()
        )
        .is_err());
    }
}

#[test]
fn public_plan_status_review_saved_bytes_require_canonical_result() {
    // Roller success is not the status: a failure branch may apply damage.
    for actor in ["user", "target"] {
        for success in [false, true] {
            let mut value = outcome();
            value["check"] = json!({"actor":actor,"success":success,"opposed":false});
            value["participation"] = json!({"check":"target_involved","payload":{"kind":"target"}});
            value["effects"] = json!([{"kind":"damage","amount":1}]);
            value["result"] = json!("applied");
            assert!(
                read(&value).is_ok(),
                "damage may apply on either roller branch"
            );
        }
    }
    for effect in [
        json!({"kind":"damage","amount":0}),
        json!({"kind":"heal","amount":0}),
        json!({"kind":"restore","resource":"focus","amount":0}),
    ] {
        let mut value = outcome();
        value["participation"]["payload"] = json!({"kind":"target"});
        value["effects"] = json!([effect]);
        assert!(
            read(&value).is_ok(),
            "retained zero effects remain Unaffected"
        );
    }
    for actor in ["user", "target"] {
        let mut value = outcome();
        value["check"] = json!({"actor":actor,"success":false,"opposed":false});
        value["participation"]["check"] = json!("target_involved");
        value["result"] = json!(if actor == "user" {
            "miss"
        } else {
            "unaffected"
        });
        assert!(
            read(&value).is_ok(),
            "empty branch uses the initiating check role"
        );
        for payload in ["source", "target"] {
            value["participation"]["payload"] = json!({"kind":payload});
            value["result"] = json!("unaffected");
            assert!(
                read(&value).is_ok(),
                "attempted nonempty payload can lose every effect"
            );
        }
    }
    for (payload, effect) in [
        (
            "target",
            json!({"kind":"apply_condition","id":"readied","duration":1}),
        ),
        (
            "source",
            json!({"kind":"observe_public_plan","observation":{"observed_at":0,"subject_name":"foe","state":{"kind":"no_public_plan"}}}),
        ),
    ] {
        let mut value = outcome();
        value["participation"]["payload"] = json!({"kind":payload});
        value["effects"] = json!([effect]);
        value["result"] = json!("applied");
        assert!(
            read(&value).is_ok(),
            "retained condition/observation means a change"
        );
    }
    let mut invalid = Vec::new();
    let mut empty_applied = outcome();
    empty_applied["result"] = json!("applied");
    invalid.push(("empty Applied", empty_applied));
    let mut damage_unaffected = outcome();
    damage_unaffected["participation"]["payload"] = json!({"kind":"target"});
    damage_unaffected["effects"] = json!([{"kind":"damage","amount":1}]);
    invalid.push(("positive damage Unaffected", damage_unaffected));
    let mut zero_applied = outcome();
    zero_applied["participation"]["payload"] = json!({"kind":"target"});
    zero_applied["effects"] = json!([{"kind":"heal","amount":0}]);
    zero_applied["result"] = json!("applied");
    invalid.push(("zero Heal Applied", zero_applied));
    let mut failed_empty = outcome();
    failed_empty["check"] = json!({"actor":"user","success":false,"opposed":false});
    failed_empty["participation"]["check"] = json!("user_only");
    invalid.push(("failed User empty Unaffected", failed_empty.clone()));
    for payload in ["source", "target"] {
        let mut miss = failed_empty.clone();
        miss["participation"]["payload"] = json!({"kind":payload});
        miss["result"] = json!("miss");
        invalid.push((payload, miss));
    }
    let accepted: Vec<_> = invalid
        .into_iter()
        .filter_map(|(label, value)| read(&value).is_ok().then_some(label))
        .collect();
    assert!(
        accepted.is_empty(),
        "noncanonical saved status accepted: {accepted:?}"
    );
}

#[test]
fn public_plan_rider_review_summary_is_required_and_primary_only() {
    let mut primary = outcome();
    primary["ability"] = json!("basic_melee");
    primary["participation"]["payload"] = json!({"kind":"source"});
    primary["target_rider_participated"] = json!(true);
    strict::<Outcome>(primary.clone());
    let mut anonymous = primary.clone();
    anonymous["ability"] = Value::Null;
    assert!(
        read(&anonymous).is_err(),
        "an individual rider cannot claim another rider summary"
    );
    for (reason, check, status) in [
        ("dead", "none", "unaffected"),
        ("immune", "target_involved", "unaffected"),
        ("unavailable_target", "none", "miss"),
    ] {
        let mut blocked = primary.clone();
        blocked["participation"] =
            json!({"check":check,"payload":{"kind":"unselected","reason":reason}});
        blocked["result"] = json!(status);
        assert!(
            read(&blocked).is_err(),
            "blocked primary cannot have an executed rider: {reason}"
        );
    }
}
