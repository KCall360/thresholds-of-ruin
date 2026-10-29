use serde_json::json;
use tor_protocol::*;

fn observation() -> serde_json::Value {
    json!({"actor":"1", "self_target": ActorTarget::from_digest([1; 32]),
        "tick":"0", "position":{"x":0,"y":0,"z":0}, "places":[],
        "visible_cells":[], "ground_items":[], "inventory":[],
        "visible_actors":[], "ready":true, "charts":[],"linked_holds":[]})
}

#[test]
fn linked_holds_is_required_in_current_observation_without_historical_defaults() {
    let mut current = observation();
    assert!(serde_json::from_value::<Observation>(current.clone()).is_ok());
    current.as_object_mut().unwrap().remove("linked_holds");
    assert!(
        serde_json::from_value::<Observation>(current).is_err(),
        "current observation must explicitly disclose its own incoming hold offers"
    );
}

#[test]
fn escape_accepts_only_a_typed_exact_hold_without_actor_authority() {
    let token = format!("h_{}", "09".repeat(32));
    let action = json!({"type":"escape", "hold": token});
    let decoded = serde_json::from_value::<Action>(action.clone());
    assert!(
        decoded.is_ok(),
        "first-class Escape must accept a current opaque hold target"
    );
    assert_eq!(serde_json::to_value(decoded.unwrap()).unwrap(), action);
    for invalid in [
        json!({"type":"escape"}),
        json!({"type":"escape","hold":9}),
        json!({"type":"escape","hold":ActorTarget::from_digest([9;32])}),
        json!({"type":"escape","hold":AppearanceSeed::from_digest([9;32])}),
        json!({"type":"escape","hold":token,"holder":"1"}),
        json!({"type":"escape","hold":token,"success":true}),
    ] {
        assert!(serde_json::from_value::<Action>(invalid).is_err());
    }
}

#[test]
fn linked_hold_offers_require_explicit_nullable_holder_and_reject_extra_authority() {
    let token = format!("h_{}", "09".repeat(32));
    let mut current = observation();
    current["linked_holds"] = json!([{"target":token,"holder":null}]);
    assert!(serde_json::from_value::<Observation>(current.clone()).is_ok());
    for offer in [
        json!({"target":token}),
        json!({"target":token,"holder":null,"location":1}),
        json!({"target":token,"holder":9}),
        json!({"target":ActorTarget::from_digest([9;32]),"holder":null}),
    ] {
        current["linked_holds"] = json!([offer]);
        assert!(serde_json::from_value::<Observation>(current.clone()).is_err());
    }
    current["linked_holds"] = json!(vec![json!({"target":token,"holder":null}); 129]);
    assert!(serde_json::from_value::<Observation>(current).is_err());
}

#[test]
fn escape_lifecycle_wire_records_cannot_carry_hidden_holder_details() {
    let token = format!("h_{}", "09".repeat(32));
    let started = json!({"type":"escape_started","hold":token});
    let decoded: Event = serde_json::from_value(started.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), started);
    let mut forged = started;
    forged["holder"] = json!(ActorTarget::from_digest([2; 32]));
    assert!(serde_json::from_value::<Event>(forged).is_err());
    for success in [false, true] {
        let resolved = json!({"type":"escape_resolved","hold":token,"success":success});
        let decoded: CombatEventView = serde_json::from_value(resolved.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), resolved);
        let mut forged = resolved;
        forged["target"] = json!(ActorTarget::from_digest([2; 32]));
        assert!(serde_json::from_value::<CombatEventView>(forged).is_err());
    }
}

#[test]
fn linked_hold_delta_replaces_exact_offers_and_validation_rejects_forged_disclosure() {
    let base = StateView {
        snapshot: tor_protocol::SnapshotId("fixture".into()),
        wizard_game: false,
        revision: 1,
        observation: serde_json::from_value(observation()).unwrap(),
    };
    base.validate().unwrap();
    let mut next = base.clone();
    next.revision = 2;
    next.observation.linked_holds = vec![LinkedHold {
        target: HoldTarget::from_digest([9; 32]),
        holder: None,
    }];
    next.validate().unwrap();
    let delta = StateDelta::between(&base, &next).unwrap();
    let mut encoded = serde_json::to_value(&delta).unwrap();
    assert_eq!(delta.apply(&base).unwrap(), next);
    encoded.as_object_mut().unwrap().remove("linked_holds");
    assert!(serde_json::from_value::<StateDelta>(encoded).is_err());
    let mut duplicate = next.clone();
    duplicate
        .observation
        .linked_holds
        .push(next.observation.linked_holds[0].clone());
    assert_eq!(duplicate.validate(), Err(InvalidState::InvalidLinkedHold));
    next.observation.linked_holds[0].holder = Some(ActorTarget::from_digest([2; 32]));
    assert_eq!(next.validate(), Err(InvalidState::InvalidLinkedHold));
}
