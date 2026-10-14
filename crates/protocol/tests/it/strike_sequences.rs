use serde_json::{json, Value};
use tor_protocol::*;

fn sequence() -> Value {
    json!({"completed": 1, "total": 3, "next_strike_remaining": "5", "cancel_recovery_remaining": "35"})
}

fn combat(sequence: Value) -> Value {
    json!({"hp": 10, "max_hp": 10, "preparation_remaining": "5",
        "preparation_active": true, "sequence": sequence, "public_plan": null, "recovery_remaining": "0",
        "actors": [], "events": [], "objective": null,
        "victory": false, "dead": false, "terminal": false})
}

fn state(combat: Value) -> StateView {
    serde_json::from_value(json!({"wizard_game":false,"revision":"1","observation":{
        "actor":"1","self_target":ActorTarget::from_digest([1;32]),"tick":"15",
        "position":{"x":0,"y":0,"z":0},"visible_cells":[],"ground_items":[],
        "inventory":[],"visible_actors":[],"places":[],"ready":false,
        "interactions":null,"motion":null,"combat":combat}}))
    .unwrap()
}

#[test]
fn ordered_strikes_status_preserves_exact_personal_progress_and_large_tick_strings() {
    for active in [false, true] {
        let mut value = combat(sequence());
        value["preparation_active"] = json!(active);
        value["sequence"]["cancel_recovery_remaining"] = json!(u64::MAX.to_string());
        let decoded: CombatView = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
        assert_eq!(state(value).validate(), Ok(()));
    }
}

#[test]
fn ordered_strikes_required_nullable_fields_reject_omitted_or_forged_facts() {
    let mut missing = combat(Value::Null);
    missing.as_object_mut().unwrap().remove("sequence");
    assert!(serde_json::from_value::<CombatView>(missing).is_err());
    for key in [
        "completed",
        "total",
        "next_strike_remaining",
        "cancel_recovery_remaining",
    ] {
        let mut missing = combat(sequence());
        missing["sequence"].as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<CombatView>(missing).is_err(),
            "missing {key}"
        );
    }
    for key in ["plan", "tokens", "check", "actor", "future_strikes"] {
        let mut forged = combat(sequence());
        forged["sequence"][key] = json!([]);
        assert!(
            serde_json::from_value::<CombatView>(forged).is_err(),
            "forged {key}"
        );
    }
    for invalid in [
        json!(5),
        json!("05"),
        json!("-1"),
        json!("18446744073709551616"),
    ] {
        let mut value = combat(sequence());
        value["sequence"]["next_strike_remaining"] = invalid;
        assert!(serde_json::from_value::<CombatView>(value).is_err());
    }
}

#[test]
fn ordered_strikes_status_rejects_incoherent_bounds_and_delta_is_atomic() {
    let base = state(combat(sequence()));
    assert_eq!(base.validate(), Ok(()));
    let original = base.clone();
    for (key, value) in [
        ("completed", json!(3)),
        ("completed", json!(16)),
        ("total", json!(0)),
        ("total", json!(1)),
        ("total", json!(17)),
        ("next_strike_remaining", json!("6")),
        ("cancel_recovery_remaining", json!("4")),
    ] {
        let mut invalid = combat(sequence());
        invalid["sequence"][key] = value;
        let mut next = state(invalid);
        next.revision = 2;
        assert_eq!(next.validate(), Err(InvalidState::InvalidCombat), "{key}");
        let delta = StateDelta::between(&base, &next).unwrap();
        assert_eq!(delta.apply(&base), Err(DeltaError::InvalidChange));
        assert_eq!(base, original);
    }
    for (key, value) in [
        ("preparation_remaining", Value::Null),
        ("recovery_remaining", json!("1")),
    ] {
        let mut invalid = combat(sequence());
        invalid[key] = value;
        assert_eq!(state(invalid).validate(), Err(InvalidState::InvalidCombat));
    }
    let mut next = state(combat(sequence()));
    next.revision = 2;
    next.observation.combat.as_mut().unwrap().preparation_active = false;
    assert_eq!(
        StateDelta::between(&base, &next)
            .unwrap()
            .apply(&base)
            .unwrap(),
        next
    );
}

#[test]
fn ordered_strikes_events_disclose_only_actual_bounded_primary_ordinals() {
    for ordinal in [Value::Null, json!(0), json!(1), json!(15)] {
        let value = json!({"type":"ability","caster":null,"target":null,
            "ability":{"id":"triple_strike","name":"Triple strike"},
            "outcome":"miss","strike":ordinal});
        let decoded: CombatEventView = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        let mut view = combat(Value::Null);
        view["events"] = json!([value.clone()]);
        assert_eq!(state(view).validate(), Ok(()));
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("strike");
        assert!(serde_json::from_value::<CombatEventView>(missing).is_err());
        for key in ["total", "remaining", "plan", "check", "tokens"] {
            let mut forged = value.clone();
            forged[key] = json!(3);
            assert!(serde_json::from_value::<CombatEventView>(forged).is_err());
        }
    }
    let mut view = combat(Value::Null);
    view["events"] = json!([{"type":"ability","caster":null,"target":null,
        "ability":{"id":"triple_strike","name":"Triple strike"},"outcome":"miss","strike":16}]);
    assert_eq!(state(view).validate(), Err(InvalidState::InvalidCombat));
}

#[test]
fn ordered_strikes_cancel_requires_explicit_step_and_preserves_identity() {
    for step in [Value::Null, json!("north_east"), json!("down")] {
        let value = json!({"type":"cancel_intention","expected_revision":"9007199254740993",
            "intention":"original-admission","step":step});
        let command: Command = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(command).unwrap(), value);
    }
    for extra in [Value::Null, json!("sideways"), json!({"x":1,"y":0,"z":0})] {
        let mut value =
            json!({"type":"cancel_intention","expected_revision":"1","intention":"same"});
        if !extra.is_null() {
            value["step"] = extra;
        }
        assert!(serde_json::from_value::<Command>(value).is_err());
    }
}

#[test]
fn ordered_strikes_initial_preparation_has_no_tail_cancellation_debt() {
    for active in [false, true] {
        let mut initial = combat(sequence());
        initial["preparation_active"] = json!(active);
        initial["sequence"]["completed"] = json!(0);
        initial["sequence"]["cancel_recovery_remaining"] = json!("0");
        assert_eq!(state(initial.clone()).validate(), Ok(()));
        initial["sequence"]["cancel_recovery_remaining"] = json!("5");
        assert_eq!(state(initial).validate(), Err(InvalidState::InvalidCombat));
    }
}
