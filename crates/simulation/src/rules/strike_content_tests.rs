//! These tests decode the real rules document, so the first red checkpoint also
//! verifies that an unsupported source schema fails at the production boundary.
use super::*;
use serde_json::{json, Value};

fn strike(offset: u64) -> Value {
    json!({"offset": offset, "unlock": null, "check": null,
        "on_success": [], "on_failure": [], "descriptors": [], "riders": []})
}
fn document_with(strikes: Vec<Value>) -> Value {
    let document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    let mut value = serde_json::to_value(document).unwrap();
    for ability in value["abilities"].as_array_mut().unwrap() {
        let object = ability.as_object_mut().unwrap();
        object.remove("check");
        object.remove("on_success");
        object.remove("on_failure");
        object.remove("descriptors");
        object.insert("strikes".into(), json!({"kind": "authored", "strikes": [strike(0)]}));
    }
    value["abilities"][0]["strikes"]["strikes"] = json!(strikes);
    value
}
fn decode(value: Value) -> Result<Arc<Ruleset>, String> {
    let document = serde_json::from_value::<RulesetDocument>(value)
        .map_err(|error| error.to_string())?;
    Ruleset::validate(document).map_err(|error| error.to_string())
}
#[test]
fn ordered_strike_content_decodes_authored_and_selected_melee_sources() {
    let mut value = document_with(vec![strike(0)]);
    assert!(decode(value.clone()).is_ok(), "a one-strike authored source must decode");
    value["abilities"][0]["strikes"] = json!({"kind": "selected_melee"});
    assert!(decode(value).is_ok(), "the full selected melee source must decode");
}
#[test]
fn ordered_strike_content_accepts_equal_offsets_and_checks_every_boundary() {
    let valid = document_with(vec![strike(0); 16]);
    assert!(decode(valid.clone()).is_ok(), "sixteen simultaneous strikes are bounded content");
    assert!(decode(document_with(vec![strike(0); 17])).is_err());
    assert!(decode(document_with(vec![])).is_err());
    assert!(decode(document_with(vec![strike(1)])).is_err());
    assert!(decode(document_with(vec![strike(0), strike(2), strike(1)])).is_err());
    assert!(decode(document_with(vec![strike(0), strike(u64::MAX)])).is_err());
    let mut first_locked = valid;
    first_locked["abilities"][0]["strikes"]["strikes"][0]["unlock"] = json!("missing");
    assert!(decode(first_locked).is_err());
}
#[test]
fn ordered_strike_content_bounds_riders_and_combined_effects() {
    let rider = json!({"check": null, "effects": [], "descriptors": []});
    let mut first = strike(0);
    first["riders"] = json!(vec![rider.clone(); 8]);
    assert!(decode(document_with(vec![first.clone()])).is_ok(), "eight riders are valid");
    first["riders"] = json!(vec![rider; 9]);
    assert!(decode(document_with(vec![first])).is_err());
    let effect = json!({"kind": "heal", "amount": {"kind": "fixed", "amount": 1}});
    let mut first = strike(0);
    first["on_success"] = json!(vec![effect.clone(); 128]);
    first["on_failure"] = json!(vec![effect.clone(); 128]);
    assert!(decode(document_with(vec![first.clone()])).is_ok(), "256 effects across both branches are valid");
    first["riders"] = json!([{"check": null, "effects": [effect], "descriptors": []}]);
    assert!(decode(document_with(vec![first])).is_err(), "rider effects count toward the aggregate cap");
}
#[test]
fn ordered_strike_content_rejects_legacy_payloads_and_recursive_riders() {
    let valid = document_with(vec![strike(0)]);
    assert!(decode(valid.clone()).is_ok());
    let mut legacy = valid.clone();
    legacy["abilities"][0]["on_success"] = json!([]);
    assert!(decode(legacy).is_err());
    let mut recursive = valid;
    recursive["abilities"][0]["strikes"]["strikes"][0]["riders"] =
        json!([{"check": null, "effects": [], "descriptors": [], "riders": []}]);
    assert!(decode(recursive).is_err());
}
