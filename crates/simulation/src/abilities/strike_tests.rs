//! Ordered-strike contracts. These tests are added before the implementation.
use super::AbilityPlan;
use serde_json::{json, Value};

fn frozen_strike(ordinal: u16, offset: u64) -> Value {
    json!({
        "offset": offset,
        "origin": {"kind": "ability", "ordinal": ordinal},
        "check": null,
        "on_success": [],
        "on_failure": [],
        "descriptors": [],
        "riders": [],
        "edge": 0,
        "shared_check": null
    })
}

fn frozen_plan(strikes: Vec<Value>) -> Value {
    json!({
        "actor": 1,
        "target": 2,
        "ability": "basic_melee",
        "preparation": 10,
        "recovery": 20,
        "cost": null,
        "reach": {"kind": "melee"},
        "strikes": strikes,
        "tokens": []
    })
}

#[test]
fn ordered_strikes_decode_one_immutable_plan_without_a_cursor() {
    let value = frozen_plan(vec![
        frozen_strike(0, 0),
        frozen_strike(1, 10),
        frozen_strike(2, 20),
    ]);
    let decoded: AbilityPlan = serde_json::from_value(value.clone())
        .expect("ordered strike plans must decode through the production plan type");
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);

    let mut mutated = value;
    mutated["next_strike"] = json!(1);
    assert!(serde_json::from_value::<AbilityPlan>(mutated).is_err());
}

#[test]
fn ordered_strikes_decode_bounded_zero_offset_strikes_and_riders() {
    let mut first = frozen_strike(0, 0);
    first["riders"] = json!([{
        "check": {
            "actor": "user", "bonus": 0,
            "threshold": {"kind": "fixed", "value": 10}, "edge": -1
        },
        "effects": [], "descriptors": []
    }]);
    first["edge"] = json!(5);
    first["shared_check"] = json!({"phase": "rider", "ordinal": 0});
    let value = frozen_plan(vec![first, frozen_strike(1, 0)]);
    let decoded: AbilityPlan = serde_json::from_value(value.clone())
        .expect("a checkless primary may reserve shared edge for an authored checked rider");
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
}

#[test]
fn ordered_strikes_reject_unbounded_or_missing_frozen_lists_on_decode() {
    let mut missing = frozen_plan(vec![frozen_strike(0, 0)]);
    missing.as_object_mut().unwrap().remove("strikes");
    assert!(serde_json::from_value::<AbilityPlan>(missing).is_err());

    let over_bound = frozen_plan((0..17).map(|ordinal| frozen_strike(ordinal, 0)).collect());
    assert!(serde_json::from_value::<AbilityPlan>(over_bound).is_err());

    let mut too_many_riders = frozen_strike(0, 0);
    too_many_riders["riders"] = json!((0..9).map(|_| json!({
        "check": null, "effects": [], "descriptors": []
    })).collect::<Vec<_>>());
    assert!(serde_json::from_value::<AbilityPlan>(frozen_plan(vec![too_many_riders])).is_err());
}
