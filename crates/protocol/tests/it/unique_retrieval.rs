use tor_protocol::{Event, ItemTarget, ObjectiveKind};

#[test]
fn intentional_retrieval_exit_has_distinct_objective_and_opaque_item_event() {
    let objective = serde_json::json!("retrieve_and_exit");
    let decoded: ObjectiveKind = serde_json::from_value(objective.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), objective);

    let item = ItemTarget::from_digest([7; 32]);
    let event = serde_json::json!({"type": "escaped", "item": item});
    let decoded: Event = serde_json::from_value(event.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), event);

    for invalid in [
        serde_json::json!({"type": "escaped"}),
        serde_json::json!({"type": "escaped", "item": 7}),
        serde_json::json!({"type": "escaped", "item": "7"}),
        serde_json::json!({"type": "escaped", "item": null}),
    ] {
        assert!(serde_json::from_value::<Event>(invalid).is_err());
    }
}
