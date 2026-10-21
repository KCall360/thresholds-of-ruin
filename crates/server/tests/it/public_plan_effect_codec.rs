// Add as an independent server integration module alongside the host behavior tests.
#[test]
fn public_plan_authoring_effects_have_strict_named_payloads() {
    use tor_server::scenario_package::Effect;
    let observe = r#"{"kind":"observe_public_plan"}"#;
    let grant = r#"{"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"ability_tag":"weapon_attack"}"#;
    for raw in [observe, grant] {
        let value: Effect =
            serde_json::from_slice(raw.as_bytes()).expect("shared authoring effect schema");
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::from_str::<serde_json::Value>(raw).unwrap()
        );
    }
    for raw in [
        r#"["observe_public_plan"]"#,
        r#"{"kind":"observe_public_plan","private_actor":2}"#,
        r#"{"private_actor":2,"kind":"observe_public_plan"}"#,
        r#"["apply_qualified_condition_to_source","readied",300,"weapon_attack"]"#,
        r#"{"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"duration":301,"ability_tag":"weapon_attack"}"#,
        r#"{"condition":"readied","duration":300,"duration":301,"ability_tag":"weapon_attack","kind":"apply_qualified_condition_to_source"}"#,
    ] {
        assert!(
            serde_json::from_slice::<Effect>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
}
#[test]
fn public_plan_authoring_consumable_wrapper_requires_a_named_object() {
    use tor_server::scenario_package::ConsumableSpec;
    let canonical = serde_json::json!({"descriptors":[],"effects":[{"kind":"observe_public_plan"}],"discovery":"self_effect"});
    let value: ConsumableSpec = serde_json::from_value(canonical.clone()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), canonical);
    let mut omitted_descriptors = canonical.clone();
    omitted_descriptors
        .as_object_mut()
        .unwrap()
        .remove("descriptors");
    let value: ConsumableSpec = serde_json::from_value(omitted_descriptors).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), canonical);
    for raw in [
        r#"[[{"kind":"observe_public_plan"}]]"#,
        r#"{"effects":[{"kind":"observe_public_plan"}],"effects":[]}"#,
        r#"{"effects":[{"kind":"observe_public_plan"}],"discovery":"self_effect","discovery":"on_use"}"#,
        r#"{"effects":[{"kind":"observe_public_plan"}],"discovery":"infer_name"}"#,
        r#"{"effects":[{"kind":"observe_public_plan"}],"discovery":null}"#,
        r#"{"private_actor":2,"effects":[{"kind":"observe_public_plan"}]}"#,
        r#"{"effects":[{"kind":"observe_public_plan"}],"private_actor":2}"#,
    ] {
        assert!(
            serde_json::from_slice::<ConsumableSpec>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
}
#[test]
fn public_plan_authoring_consumable_effect_count_is_eagerly_bounded() {
    use tor_server::scenario_package::ConsumableSpec;
    for count in [16, 17] {
        let raw = serde_json::to_vec(&serde_json::json!({"effects":vec![serde_json::json!({"kind":"observe_public_plan"});count]})).unwrap();
        assert_eq!(
            serde_json::from_slice::<ConsumableSpec>(&raw).is_ok(),
            count == 16,
            "count {count}"
        );
    }
}
