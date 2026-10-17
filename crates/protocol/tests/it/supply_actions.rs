use tor_protocol::*;

fn ability() -> serde_json::Value {
    serde_json::json!({"id":"mend","name":"Mend","cost":null,
        "reach":{"kind":"self_only"},"target_kind":"self_only","basic_attack":false,
        "cooldown_remaining":"25","supply_costs":[{"supply":"bandage","name":"Bandage",
            "quantity":"1","access":"carried_or_ground_touch"}]})
}
fn action(items: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({"type":"use_ability","id":"mend",
        "invocation":{"kind":"actor_target","target":ActorTarget::from_digest([1;32])},"supplies":{"kind":"explicit","items":items}})
}
fn item(index: u8) -> serde_json::Value {
    serde_json::json!({"cost_ordinal":0,"item":ItemTarget::from_digest([index;32])})
}

fn disclosed_item() -> serde_json::Value {
    serde_json::json!({"id":ItemTarget::from_digest([1;32]),"class":"misc","quantity":"1",
        "appearance":"bandage","identified":true,"description":"","name":"Bandage",
        "known_supply":{"id":"bandage","name":"Bandage"}})
}

#[test]
fn phase5_container_item_requires_a_named_object() {
    assert!(serde_json::from_value::<ItemView>(disclosed_item()).is_ok());
    assert!(serde_json::from_value::<ItemView>(serde_json::json!([
        "misc", "1", "bandage", true, "", ItemTarget::from_digest([1;32]), "Bandage", null,
        {"id":"bandage","name":"Bandage"}
    ]))
    .is_err());
}

#[test]
fn phase5_container_item_rejects_unknown_fields() {
    let mut value = disclosed_item();
    value["held_quantity"] = "1".into();
    assert!(serde_json::from_value::<ItemView>(value).is_err());
}

#[test]
fn phase5_container_ground_item_requires_a_named_object() {
    assert!(serde_json::from_value::<GroundItemView>(serde_json::json!([
        false, true, disclosed_item(), {"x":1,"y":0,"z":0}
    ]))
    .is_err());
}

#[test]
fn phase5_container_ground_item_rejects_unknown_fields() {
    let value = serde_json::json!({"reachable":false,"supply_reachable":true,
        "item":disclosed_item(),"position":{"x":1,"y":0,"z":0},"claimant":"private"});
    assert!(serde_json::from_value::<GroundItemView>(value).is_err());
}

#[test]
fn phase5_container_ability_requires_a_named_object() {
    let value = ability();
    let fields: Vec<_> = [
        "id",
        "name",
        "cost",
        "reach",
        "target_kind",
        "basic_attack",
        "cooldown_remaining",
        "supply_costs",
    ]
    .into_iter()
    .map(|field| value[field].clone())
    .collect();
    assert!(serde_json::from_value::<AbilityView>(serde_json::Value::Array(fields)).is_err());
}

#[test]
fn phase5_container_combat_actor_requires_a_named_object() {
    let actor = ActorTarget::from_digest([1; 32]);
    assert!(
        serde_json::from_value::<CombatActorView>(serde_json::json!({
            "actor":actor,"hostile":false,"allied":true,"injury":"healthy"
        }))
        .is_ok()
    );
    assert!(
        serde_json::from_value::<CombatActorView>(serde_json::json!([
            actor, false, true, "healthy"
        ]))
        .is_err()
    );
}

#[test]
fn phase5_ability_discloses_required_cooldown_and_finite_costs() {
    let value = ability();
    let decoded: AbilityView = serde_json::from_value(value.clone()).unwrap();
    assert!(decoded.valid());
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
}

#[test]
fn phase5_current_ability_rejects_missing_cooldown_and_supply_fields() {
    for field in ["cooldown_remaining", "supply_costs"] {
        let mut value = ability();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<AbilityView>(value).is_err());
    }
}

#[test]
fn phase5_supply_costs_are_bounded_typed_positive_and_unique() {
    let original = ability();
    for count in [0, 8] {
        let mut value = original.clone();
        value["supply_costs"] = (0..count).map(|i| serde_json::json!({"supply":format!("supply_{i}"), "name":format!("Supply {i}"), "quantity":"1", "access":"carried_only"})).collect();
        assert!(serde_json::from_value::<AbilityView>(value)
            .unwrap()
            .valid());
    }
    let mut over = original.clone();
    over["supply_costs"] = vec![original["supply_costs"][0].clone(); 9].into();
    assert!(serde_json::from_value::<AbilityView>(over).is_err());
    for field in ["quantity", "supply", "access"] {
        let mut value = original.clone();
        value["supply_costs"][0][field] = match field {
            "quantity" => "0".into(),
            "supply" => "bad id".into(),
            _ => "remote".into(),
        };
        assert!(serde_json::from_value::<AbilityView>(value).map_or(true, |a| !a.valid()));
    }
    let mut duplicate = original.clone();
    duplicate["supply_costs"] = vec![original["supply_costs"][0].clone(); 2].into();
    assert!(serde_json::from_value::<AbilityView>(duplicate).map_or(true, |a| !a.valid()));
}

#[test]
fn phase5_explicit_opaque_supply_selection_round_trips_at_fragment_bound() {
    for count in [1, 32] {
        let value = action((1..=count).map(item).collect());
        let decoded: Action = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    let mut carried = action(vec![]);
    carried["supplies"] = serde_json::json!({"kind":"carried"});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<Action>(carried.clone()).unwrap()).unwrap(),
        carried
    );
}

#[test]
fn phase5_supply_selection_rejects_missing_unbounded_duplicate_and_quantity_rows() {
    let mut missing = action(vec![item(1)]);
    missing.as_object_mut().unwrap().remove("supplies");
    assert!(serde_json::from_value::<Action>(missing).is_err());
    for rows in [
        vec![],
        vec![item(1), item(1)],
        (1..=33).map(item).collect(),
        vec![serde_json::json!({"cost_ordinal":8,"item":ItemTarget::from_digest([1;32])})],
        vec![serde_json::json!({"cost_ordinal":0,"item":"1"})],
        vec![
            serde_json::json!({"cost_ordinal":0,"item":ItemTarget::from_digest([1;32]),"quantity":"1"}),
        ],
    ] {
        assert!(serde_json::from_value::<Action>(action(rows)).is_err());
    }
}

#[test]
fn phase5_known_supply_is_explicit_and_unknown_items_cannot_expose_it() {
    let value = serde_json::json!({"id":ItemTarget::from_digest([1;32]),"class":"misc","quantity":"1","appearance":"bandage","identified":true,"description":"","name":"Bandage","known_supply":{"id":"bandage","name":"Bandage"}});
    let decoded: ItemView = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("known_supply");
    assert!(serde_json::from_value::<ItemView>(missing).is_err());
    let mut hidden = value;
    hidden["identified"] = false.into();
    assert!(serde_json::from_value::<ItemView>(hidden).is_err());
}

#[test]
fn phase5_ground_supply_touch_is_explicit_and_does_not_change_pickup() {
    let item = serde_json::json!({"id":ItemTarget::from_digest([1;32]),"class":"misc","quantity":"1","appearance":"bandage","identified":true,"description":"","name":"Bandage","known_supply":{"id":"bandage","name":"Bandage"}});
    let value = serde_json::json!({"reachable":false,"supply_reachable":true,"item":item,"position":{"x":1,"y":0,"z":0}});
    let decoded: GroundItemView = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("supply_reachable");
    assert!(serde_json::from_value::<GroundItemView>(missing).is_err());
    let mut unknown = value;
    unknown["item"]["known_supply"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<GroundItemView>(unknown).is_err());
}

#[test]
fn phase5_selected_supply_requires_a_named_object() {
    let target = ItemTarget::from_digest([1; 32]);
    assert!(serde_json::from_value::<SelectedSupply>(serde_json::json!({
        "cost_ordinal": 0, "item": target
    }))
    .is_ok());
    assert!(serde_json::from_value::<SelectedSupply>(serde_json::json!([0, target])).is_err());
}

#[test]
fn phase5_supply_view_requires_a_named_object() {
    assert!(serde_json::from_value::<SupplyView>(serde_json::json!({
        "id": "bandage", "name": "Bandage"
    }))
    .is_ok());
    assert!(
        serde_json::from_value::<SupplyView>(serde_json::json!(["bandage", "Bandage"])).is_err()
    );
}

#[test]
fn phase5_ability_supply_cost_requires_a_named_object() {
    assert!(
        serde_json::from_value::<AbilitySupplyCost>(ability()["supply_costs"][0].clone()).is_ok()
    );
    assert!(
        serde_json::from_value::<AbilitySupplyCost>(serde_json::json!([
            "bandage",
            "Bandage",
            "1",
            "carried_or_ground_touch"
        ]))
        .is_err()
    );
}

#[test]
fn phase5_supply_selection_rejects_one_item_reused_across_cost_ordinals() {
    let target = ItemTarget::from_digest([1; 32]);
    let value = action(vec![
        serde_json::json!({"cost_ordinal": 0, "item": target}),
        serde_json::json!({"cost_ordinal": 1, "item": target}),
    ]);
    assert!(serde_json::from_value::<Action>(value).is_err());
}

#[test]
fn phase5_carried_selection_rejects_explicit_items_key() {
    assert!(
        serde_json::from_value::<SupplySelection>(serde_json::json!({
            "kind": "carried", "items": []
        }))
        .is_err()
    );
}

fn recorded_care_request() -> serde_json::Value {
    let samples: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/wire-v46.json")).unwrap();
    let value = samples["client"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["request"]["command"]["action"]["id"] == "bind_wounds")
        .unwrap()
        .clone();
    assert!(decode_request(&serde_json::to_string(&value).unwrap()).is_ok());
    value
}

#[test]
fn phase5_codec_carried_selection_rejects_positional_tagged_alias() {
    let mut value = recorded_care_request();
    value["request"]["command"]["action"]["supplies"] = serde_json::json!(["carried"]);
    assert!(decode_request(&serde_json::to_string(&value).unwrap()).is_err());
}

#[test]
fn phase5_codec_explicit_selection_rejects_positional_tagged_alias() {
    let mut value = recorded_care_request();
    let rows = value["request"]["command"]["action"]["supplies"]["items"].clone();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    value["request"]["command"]["action"]["supplies"] = serde_json::json!(["explicit", rows]);
    assert!(decode_request(&serde_json::to_string(&value).unwrap()).is_err());
}

#[test]
fn phase5_codec_ability_action_rejects_outer_positional_tagged_alias() {
    let mut value = recorded_care_request();
    let action = &value["request"]["command"]["action"];
    value["request"]["command"]["action"] = serde_json::json!([
        "use_ability",
        action["id"],
        action["invocation"],
        action["supplies"]
    ]);
    assert!(decode_request(&serde_json::to_string(&value).unwrap()).is_err());
}

fn recorded_ability_response(id: &str) -> (serde_json::Value, usize) {
    let samples: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/wire-v46.json")).unwrap();
    let (value, index) = samples["server"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|message| {
            let abilities = message["snapshot"]["state"]["observation"]["combat"]["own_stats"]
                ["abilities"]
                .as_array()?;
            abilities
                .iter()
                .position(|ability| ability["id"] == id)
                .map(|index| (message.clone(), index))
        })
        .unwrap();
    assert!(decode_response(&serde_json::to_string(&value).unwrap()).is_ok());
    (value, index)
}

#[test]
fn phase5_codec_supportive_reach_rejects_positional_tagged_alias() {
    let (mut value, index) = recorded_ability_response("bind_wounds");
    let reach = &mut value["snapshot"]["state"]["observation"]["combat"]["own_stats"]["abilities"]
        [index]["reach"];
    assert_eq!(reach["kind"], "self_or_ally");
    *reach = serde_json::json!(["self_or_ally", {"kind":"touch"}]);
    assert!(decode_response(&serde_json::to_string(&value).unwrap()).is_err());
}

#[test]
fn phase5_codec_supportive_range_rejects_positional_tagged_alias() {
    // The inner range already has a named-map gate; retain the complete-message control.
    let (mut value, index) = recorded_ability_response("bind_wounds");
    value["snapshot"]["state"]["observation"]["combat"]["own_stats"]["abilities"][index]["reach"]
        ["range"] = serde_json::json!(["touch"]);
    assert!(decode_response(&serde_json::to_string(&value).unwrap()).is_err());
}
