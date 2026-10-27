use super::ordered_strike_authoring_tests::{rules, source};
use super::*;
#[test]
fn explicit_detection_species_label_compiles_without_personal_name_or_cosmetic_roster_inference() {
    let mut value = source();
    let base: Catalog = serde_json::from_value(value.clone()).unwrap();
    let id = SpeciesId::new("sequence_test").unwrap();
    assert!(base
        .compile(rules())
        .unwrap()
        .content
        .detection_appearance(&id)
        .is_none());
    value["species"]["sequence_test"]["detection_appearance"] =
        serde_json::json!({"name":"a bat","asset":"creature.bat"});
    let compiled: Catalog = serde_json::from_value(value.clone()).unwrap();
    let label = compiled
        .compile(rules())
        .unwrap()
        .content
        .detection_appearance(&id)
        .unwrap()
        .clone();
    assert_eq!(label.name.as_ref(), "a bat");
    assert_eq!(label.asset.as_deref(), Some("creature.bat"));
    value["species"]["sequence_test"]["detection_appearance"] = serde_json::json!({"name":""});
    assert!(serde_json::from_value::<Catalog>(value)
        .unwrap()
        .compile(rules())
        .is_ok());
}
#[test]
fn malformed_species_detection_source_rejects_without_changing_existing_species_catalog() {
    let mut value = source();
    value["species"]["sequence_test"]["detection_appearance"] =
        serde_json::json!({"name":"bat","asset":null});
    assert!(serde_json::from_value::<Catalog>(value.clone())
        .unwrap()
        .compile(rules())
        .is_ok());
    for label in [
        serde_json::json!({"name":"x".repeat(257)}),
        serde_json::json!({"name":"bad\nname"}),
        serde_json::json!({"name":"bat","asset":"bad..asset"}),
    ] {
        let mut bad = value.clone();
        bad["species"]["sequence_test"]["detection_appearance"] = label;
        assert!(serde_json::from_value::<Catalog>(bad)
            .unwrap()
            .compile(rules())
            .is_err());
    }
    for label in [
        serde_json::json!({"name":"bat","hidden_actor":2}),
        serde_json::json!(["bat", null]),
        serde_json::json!({"asset":null}),
    ] {
        let mut bad = value.clone();
        bad["species"]["sequence_test"]["detection_appearance"] = label;
        assert!(serde_json::from_value::<Catalog>(bad).is_err());
    }
}
