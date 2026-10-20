use serde_json::{json, Value};
use tor_protocol::*;

fn item() -> Value {
    json!({
        "id": ItemTarget::from_digest([83; 32]),
        "class": "coin", "quantity": "9007199254740993",
        "appearance": "unmarked discs", "identified": true,
        "description": "", "name": "unrelated label",
        "known_supply": null, "known_currency": "gold"
    })
}

#[test]
fn known_currency_is_required_nullable_typed_and_preserves_exact_quantity() {
    for currency in [Value::Null, json!("gold")] {
        let mut value = item();
        value["known_currency"] = currency;
        let decoded: ItemView = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decoded.quantity, 9_007_199_254_740_993);
        assert_eq!(
            decoded.known_currency,
            (value["known_currency"] == "gold").then_some(CurrencyKind::Gold)
        );
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
    let mut missing = item();
    missing.as_object_mut().unwrap().remove("known_currency");
    assert!(serde_json::from_value::<ItemView>(missing).is_err());
    for currency in [
        json!("silver"),
        json!({"gold": null}),
        json!(1),
        json!(["gold"]),
    ] {
        let mut value = item();
        value["known_currency"] = currency;
        assert!(serde_json::from_value::<ItemView>(value).is_err());
    }
    let source = serde_json::to_string(&item()).unwrap();
    let duplicate = source.replacen(
        "\"known_currency\":\"gold\"",
        "\"known_currency\":\"gold\",\"known_currency\":null",
        1,
    );
    assert_ne!(duplicate, source);
    assert!(serde_json::from_str::<ItemView>(&duplicate).is_err());
}

#[test]
fn known_currency_requires_an_identified_coin_without_name_or_asset_inference() {
    for (field, invalid) in [("identified", json!(false)), ("class", json!("gem"))] {
        let mut value = item();
        value[field] = invalid;
        assert!(serde_json::from_value::<ItemView>(value).is_err());
    }
    for identified in [false, true] {
        let mut value = item();
        value["identified"] = identified.into();
        value["known_currency"] = Value::Null;
        value["name"] = "gold".into();
        value["asset"] = "item.gold".into();
        let decoded: ItemView = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.known_currency, None);
    }
}
