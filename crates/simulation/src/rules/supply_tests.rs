use super::*;
use serde_json::{json, Value};

fn document() -> RulesetDocument {
    toml::from_str(DEFAULT_RULESET_TOML).unwrap()
}
fn cost(supply: &str) -> Value {
    json!({"supply": supply, "quantity": 1, "access": "carried_or_ground_touch", "spend_at": "first_resolution"})
}
fn with_costs(costs: Value) -> RulesetDocument {
    let mut value = serde_json::to_value(document()).unwrap();
    value["abilities"][0]["supply_costs"] = costs;
    serde_json::from_value(value).unwrap()
}

#[test]
fn supplies_are_typed_validated_catalog_entries() {
    let mut value = serde_json::to_value(document()).unwrap();
    assert!(value["supplies"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == "bandage"));
    assert!(value["supplies"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["id"] == "ration"));
    value["supplies"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"bandage", "name":"Duplicate"}));
    assert!(Ruleset::validate(serde_json::from_value(value).unwrap()).is_err());
    let mut value = serde_json::to_value(document()).unwrap();
    value["supplies"][0]["id"] = json!("bad id");
    assert!(serde_json::from_value::<RulesetDocument>(value).is_err());
}

#[test]
fn supply_costs_require_known_unique_canonical_positive_rows() {
    for costs in [
        json!([cost("unknown")]),
        json!([cost("bandage"), cost("bandage")]),
        json!([cost("ration"), cost("bandage")]),
    ] {
        assert!(Ruleset::validate(with_costs(costs)).is_err());
    }
    for quantity in [0, crate::limits::NUMERIC as u64 + 1] {
        let mut row = cost("bandage");
        row["quantity"] = json!(quantity);
        assert!(Ruleset::validate(with_costs(json!([row]))).is_err());
    }
    let mut row = cost("bandage");
    row["spend_at"] = json!("replacement_commit");
    assert!(Ruleset::validate(with_costs(json!([row]))).is_err());
    assert!(Ruleset::validate(with_costs(json!([cost("bandage"), cost("ration")]))).is_ok());
}

#[test]
fn supply_rows_are_bounded_during_decode_and_reject_unknown_fields() {
    let mut value = serde_json::to_value(document().abilities.remove(0)).unwrap();
    value["supply_costs"] = json!(vec![cost("bandage"); 9]);
    assert!(serde_json::from_value::<AbilityDef>(value.clone()).is_err());
    let mut row = cost("bandage");
    row["extra"] = json!(true);
    value["supply_costs"] = json!([row]);
    assert!(serde_json::from_value::<AbilityDef>(value).is_err());
}

#[test]
fn eight_unique_supply_costs_are_valid_but_programmatic_ninth_is_rejected() {
    let mut doc = document();
    doc.abilities[0].supply_costs.clear();
    for index in 0..=8 {
        let id = SupplyId::new(format!("test_supply_{index}")).unwrap();
        doc.supplies.push(SupplyDef {
            id: id.clone(),
            name: format!("Test {index}"),
        });
        doc.abilities[0].supply_costs.push(SupplyCost {
            supply: id,
            quantity: crate::limits::NUMERIC as u64,
            access: SupplyAccess::CarriedOnly,
            spend_at: SupplySpendAt::FirstResolution,
        });
        if index == 7 {
            assert!(Ruleset::validate(doc.clone()).is_ok());
        }
    }
    assert!(Ruleset::validate(doc).is_err());
}

#[test]
fn authored_cooldowns_are_positive_and_numerically_bounded() {
    let mut value = serde_json::to_value(document()).unwrap();
    value["abilities"][0]["cooldown"] = json!(0);
    assert!(serde_json::from_value::<RulesetDocument>(value.clone()).is_err());
    value["abilities"][0]["cooldown"] = json!(crate::limits::NUMERIC as u64 + 1);
    assert!(Ruleset::validate(serde_json::from_value(value).unwrap()).is_err());
}

#[test]
fn ally_targeting_rejects_basic_attack_and_invalid_sight_ranges() {
    let mut value = serde_json::to_value(document()).unwrap();
    value["abilities"][0]["reach"] = json!({"kind":"self_or_ally", "range":{"kind":"touch"}});
    assert!(Ruleset::validate(serde_json::from_value(value.clone()).unwrap()).is_err());
    value["abilities"][0]["basic_attack"] = json!(false);
    assert!(Ruleset::validate(serde_json::from_value(value.clone()).unwrap()).is_ok());
    for range in [0, crate::limits::SIGHT_RANGE + 1] {
        value["abilities"][0]["reach"] =
            json!({"kind":"self_or_ally", "range":{"kind":"sight", "range":range}});
        assert!(Ruleset::validate(serde_json::from_value(value.clone()).unwrap()).is_err());
    }
}

#[test]
fn bind_wounds_is_universal_ordinary_supply_healing_content() {
    let rules = Ruleset::validate(document()).unwrap();
    let id = AbilityId::new("bind_wounds").unwrap();
    assert!(rules.constants().default_abilities.contains(&id));
    let value = serde_json::to_value(rules.ability(&id).unwrap()).unwrap();
    assert_eq!(value["cooldown"], json!(1200));
    assert_eq!(value["supply_costs"], json!([cost("bandage")]));
    assert_eq!(
        value["reach"],
        json!({"kind":"self_or_ally", "range":{"kind":"touch"}})
    );
    assert_eq!(
        value["strikes"]["strikes"][0]["on_success"],
        json!([{"kind":"heal", "amount":{"kind":"fixed", "amount":2}}])
    );
}

#[test]
fn phase5_supply_definition_requires_a_named_object() {
    assert!(serde_json::from_value::<SupplyDef>(json!(["bandage", "Bandage"])).is_err());
}
#[test]
fn phase5_frozen_supply_cost_requires_a_named_object() {
    assert!(serde_json::from_value::<SupplyCost>(json!([
        "bandage",
        1,
        "carried_only",
        "first_resolution"
    ]))
    .is_err());
}
#[test]
fn phase5_touch_range_rejects_surplus_fields() {
    assert!(serde_json::from_value::<AllyRange>(json!({"kind":"touch","range":1})).is_err());
}

fn care_reach_alias(replacement: &str) -> String {
    let named = "reach = { kind = \"self_or_ally\", range = { kind = \"touch\" } }";
    assert_eq!(DEFAULT_RULESET_TOML.matches(named).count(), 1);
    let source = DEFAULT_RULESET_TOML.replacen(named, replacement, 1);
    assert!(
        toml::from_str::<toml::Value>(&source).is_ok(),
        "alias is syntactically valid TOML"
    );
    assert!(
        Ruleset::validate(document()).is_ok(),
        "unchanged declared rules are valid"
    );
    source
}

#[test]
fn phase5_authoring_reach_rejects_positional_tagged_alias() {
    let source = care_reach_alias("reach = [\"self_or_ally\", { kind = \"touch\" }]");
    assert!(toml::from_str::<RulesetDocument>(&source).is_err());
}

#[test]
fn phase5_authoring_ally_range_rejects_positional_tagged_alias() {
    let source = care_reach_alias("reach = { kind = \"self_or_ally\", range = [\"touch\"] }");
    assert!(toml::from_str::<RulesetDocument>(&source).is_err());
}

fn authoring_field_values(value: &Value, fields: &[&str]) -> Vec<Value> {
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), fields.len());
    fields
        .iter()
        .map(|field| object.get(*field).unwrap().clone())
        .collect()
}

// Match the named adapter's Fields order so the rejected positional probe
// is otherwise structurally complete, rather than failing on misplaced fields.
const ABILITY_FIELDS: &[&str] = &[
    "id",
    "invocation",
    "tags",
    "name",
    "basic_attack",
    "outgoing_edge_policy",
    "cost",
    "cooldown",
    "supply_costs",
    "timing",
    "reach",
    "strikes",
    "ai",
];

#[test]
fn phase5_container_rules_document_rejects_positional_fields() {
    let document = document();
    Ruleset::validate(document.clone()).unwrap();
    let value = serde_json::to_value(&document).unwrap();
    assert_eq!(
        serde_json::from_slice::<RulesetDocument>(&serde_json::to_vec(&value).unwrap()).unwrap(),
        document
    );
    let fields = authoring_field_values(
        &value,
        &[
            "id",
            "constants",
            "point_pools",
            "supplies",
            "abilities",
            "forms",
            "ability_tags",
            "talents",
            "creature_types",
            "subtypes",
            "classes",
            "conditions",
            "descriptors",
            "flags",
            "resources",
            "health_recovery",
            "advancement",
            "growth",
            "equipment_slots",
        ],
    );
    assert!(
        serde_json::from_slice::<RulesetDocument>(&serde_json::to_vec(&fields).unwrap()).is_err()
    );
}

#[test]
fn phase5_container_ability_definition_rejects_positional_fields() {
    let document = document();
    Ruleset::validate(document.clone()).unwrap();
    let care = document
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "bind_wounds")
        .unwrap();
    let value = serde_json::to_value(care).unwrap();
    assert_eq!(
        &serde_json::from_value::<AbilityDef>(value.clone()).unwrap(),
        care
    );
    assert!(
        serde_json::from_value::<AbilityDef>(json!(authoring_field_values(&value, ABILITY_FIELDS)))
            .is_err()
    );
}

#[test]
fn phase5_container_rules_toml_rejects_positional_ability_definition() {
    let mut document = document();
    let index = document
        .abilities
        .iter()
        .position(|ability| ability.id.as_str() == "rattling_blow")
        .unwrap();
    // Every positional field has a TOML value: use a real paid ability and a
    // valid non-null cooldown, rather than an impossible TOML null fixture.
    document.abilities[index].cooldown = std::num::NonZeroU64::new(100);
    Ruleset::validate(document.clone()).unwrap();
    let named = toml::to_string(&document).unwrap();
    assert_eq!(toml::from_str::<RulesetDocument>(&named).unwrap(), document);
    let mut value: toml::Value = toml::from_str(&named).unwrap();
    let ability = &mut value["abilities"].as_array_mut().unwrap()[index];
    let object = ability.as_table().unwrap();
    assert_eq!(object.len(), ABILITY_FIELDS.len());
    *ability = toml::Value::Array(
        ABILITY_FIELDS
            .iter()
            .map(|field| object.get(*field).unwrap().clone())
            .collect(),
    );
    let source = toml::to_string(&value).unwrap();
    assert!(toml::from_str::<toml::Value>(&source).is_ok());
    assert!(toml::from_str::<RulesetDocument>(&source).is_err());
}
