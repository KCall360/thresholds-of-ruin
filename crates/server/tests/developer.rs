use tor_server::journal::WizardOperation;
#[test]
fn invalid_archetypes_and_forged_properties_are_rejected() {
    for operation in [
        serde_json::json!({"type":"place_item","kind":"sword","position":{"region":1,"x":1,"y":1,"z":0}}),
        serde_json::json!({"type":"spawn_actor","position":{"region":1,"x":1,"y":1,"z":0},"turn_ticks":100,"god":true}),
    ] {
        assert!(serde_json::from_value::<WizardOperation>(operation).is_err());
    }
}
