use serde_json::{json, Value};
use tor_protocol::*;
fn item() -> ItemTarget {
    ItemTarget::from_digest([71; 32])
}
#[test]
fn magic_commands_round_trip_only_current_named_opaque_operands() {
    for value in [
        json!({"type":"read","item":item()}),
        json!({"type":"zap","item":item(),"aim":{"kind":"direction","direction":"north_east"}}),
        json!({"type":"identify","choice":"opaque-choice","item":item()}),
        json!({"type":"identify","choice":"opaque-choice","item":null}),
    ] {
        let action: Action = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(action).unwrap(), value);
    }
    for value in [
        json!(["read", item()]),
        json!({"type":"read","item":"71"}),
        json!({"type":"read","item":item(),"ability":"secret"}),
        json!({"type":"zap","item":item()}),
        json!({"type":"zap","item":item(),"direction":"north","charges":5}),
        json!({"type":"zap","item":item(),"direction":{"north":null}}),
        json!({"type":"identify","item":item()}),
        json!({"type":"identify","choice":"opaque-choice"}),
        json!({"type":"identify","choice":"opaque-choice","item":item(),"classes":["wand"]}),
    ] {
        assert!(
            serde_json::from_value::<Action>(value.clone()).is_err(),
            "{value}"
        );
    }
    let raw = format!(
        "{{\"type\":\"identify\",\"choice\":\"one\",\"choice\":\"two\",\"item\":\"{}\"}}",
        item()
    );
    assert!(serde_json::from_str::<Action>(&raw).is_err());
}
fn affordance() -> Value {
    json!({"item":item(),"slot":null,"equipped_slot":null,"known_equipment":null,"known_projectile_bonus":null,"known_cursed":null,"known_protected":null,"drinkable":false,"edible":false,"readable":false,"zappable":true,"device_activation":"direction","charges":null})
}
#[test]
fn magic_affordances_require_explicit_flags_and_instance_count_and_reject_private_fields() {
    for count in [Value::Null, json!(0), json!(65535)] {
        let mut value = affordance();
        value["charges"] = count;
        let decoded: ItemInteractionView = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    for field in [
        "readable",
        "zappable",
        "device_activation",
        "charges",
        "known_cursed",
        "known_protected",
        "known_projectile_bonus",
    ] {
        let mut value = affordance();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ItemInteractionView>(value).is_err());
    }
    for count in [json!(-1), json!(65536), json!("5")] {
        let mut value = affordance();
        value["charges"] = count;
        assert!(serde_json::from_value::<ItemInteractionView>(value).is_err());
    }
    let mut value = affordance();
    value["source_ability"] = json!("secret");
    assert!(serde_json::from_value::<ItemInteractionView>(value).is_err());
}
#[test]
fn identification_choices_require_named_unique_typed_targets_and_no_private_metadata() {
    let good = json!({"choice":"opaque-choice","classes":["wand"],"items":[item()]});
    let decoded: IdentificationChoiceView = serde_json::from_value(good.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), good);
    let both = json!({"choice":"opaque-choice","classes":["ring","wand"],"items":[item()]});
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<IdentificationChoiceView>(both.clone()).unwrap()
        )
        .unwrap(),
        both
    );
    for value in [
        json!({"choice":"opaque-choice","class":"wand","items":[]}),
        json!({"choice":"opaque-choice","classes":[],"items":[]}),
        json!({"choice":"opaque-choice","classes":["wand","ring"],"items":[]}),
        json!({"choice":"opaque-choice","classes":["wand","wand"],"items":[]}),
        json!({"choice":"opaque-choice","classes":[{"wand":null}],"items":[]}),
        json!({"choice":"opaque-choice","classes":["missing"],"items":[]}),
        json!(["opaque-choice", "wand", [item()]]),
        json!({"choice":"","classes":["wand"],"items":[]}),
        json!({"choice":"opaque-choice","classes":["wand"],"items":[item(),item()]}),
        json!({"choice":"opaque-choice","classes":["wand"],"items":["71"]}),
        json!({"choice":"opaque-choice","class":{"wand":null},"items":[]}),
        json!({"choice":"opaque-choice","classes":["wand"],"items":(0..65).map(|n| ItemTarget::from_digest([n;32])).collect::<Vec<_>>()}),
        json!({"choice":"opaque-choice","classes":["wand"],"items":[item()],"source":"identify_wand"}),
    ] {
        assert!(
            serde_json::from_value::<IdentificationChoiceView>(value.clone()).is_err(),
            "{value}"
        );
    }
    assert!(serde_json::from_value::<InteractionView>(
        json!({"completed":[],"slots":[],"inventory":[],"preparation":null})
    )
    .is_err());
}

#[test]
fn remembered_projectile_bonus_is_named_required_nullable_and_bounded() {
    for bonus in [
        Value::Null,
        json!({"hit":-1_000_000,"damage":1_000_000}),
        json!({"hit":0,"damage":0}),
    ] {
        let mut value = affordance();
        value["known_projectile_bonus"] = bonus;
        let decoded: ItemInteractionView = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    for bonus in [
        json!([1, 2]),
        json!({"hit":1}),
        json!({"damage":2}),
        json!({"hit":null,"damage":2}),
        json!({"hit":1,"damage":2,"live":true}),
        json!({"hit":1_000_001,"damage":0}),
        json!({"hit":0,"damage":-1_000_001}),
    ] {
        let mut value = affordance();
        value["known_projectile_bonus"] = bonus;
        assert!(serde_json::from_value::<ItemInteractionView>(value).is_err());
    }
    assert!(
        serde_json::from_str::<ProjectileBonusView>(r#"{"hit":1,"hit":2,"damage":3}"#).is_err()
    );
}

#[test]
fn remembered_bonus_interaction_rejects_positional_records() {
    assert!(serde_json::from_value::<ItemInteractionView>(json!([
        item(),null,null,null,{"hit":1,"damage":2},null,null,false,false,false,true,null
    ]))
    .is_err());
}

#[test]
fn crumbled_event_uses_only_opaque_source_and_exact_requested_quantity() {
    let positive = json!({"type":"crumbled","item":item(),"quantity":"2"});
    let event: Event = serde_json::from_value(positive.clone()).unwrap();
    assert_eq!(
        event,
        Event::Crumbled {
            item: item(),
            quantity: 2
        }
    );
    assert_eq!(serde_json::to_value(event).unwrap(), positive);
    for malformed in [
        json!({"type":"crumbled","item":item()}),
        json!({"type":"crumbled","item":"71","quantity":"2"}),
        json!({"type":"crumbled","item":item(),"quantity":"2","result":item()}),
        json!({"type":"crumbled","item":item(),"quantity":"2","cursed":true}),
    ] {
        assert!(serde_json::from_value::<Event>(malformed).is_err());
    }
}

#[test]
fn device_aim_is_closed_named_and_rejects_legacy_or_foreign_operands() {
    for value in [
        json!({"kind":"self_target"}),
        json!({"kind":"direction","direction":"east"}),
    ] {
        let decoded: DeviceAim = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    for value in [
        json!({"kind":"self_target","target":"forged"}),
        json!({"kind":"self_target","direction":"east"}),
        json!({"kind":"direction"}),
        json!({"kind":"direction","direction":{"east":null}}),
        json!(["self_target"]),
    ] {
        assert!(
            serde_json::from_value::<DeviceAim>(value.clone()).is_err(),
            "{value}"
        );
    }
    assert!(serde_json::from_value::<Action>(
        json!({"type":"zap","item":item(),"direction":"east"})
    )
    .is_err());
}
