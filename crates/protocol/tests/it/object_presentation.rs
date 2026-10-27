//! Independently authored cosmetic-wire records; capability fields stay factual.
use serde_json::{json, Value};
use tor_protocol::*;

fn presentation() -> Value {
    json!({"name":"painted stairs","asset":null,"kind":{"kind":"stairs"}})
}
fn item() -> Value {
    json!({"id":ItemTarget::from_digest([47;32]),"class":"coin","quantity":"3",
        "appearance":"object","identified":true,"description":"","name":"object",
        "known_supply":null,"known_currency":"gold"})
}
fn ground() -> Value {
    json!({"reachable":true,"supply_reachable":false,"item":item(),
        "position":{"x":1,"y":0,"z":0},"presentation":null})
}
fn cell() -> Value {
    json!({"lit":true,"door":null,"trap":null,"material":"stone","key":"cell",
        "stairs_up":false,"stairs_down":false,"position":{"x":1,"y":0,"z":0},
        "wall":false,"place_hint":false,"object_presentation":null})
}
fn remembered() -> Value {
    json!({"key":"cell","position":{"x":1,"y":0,"z":0},"wall":false,
        "material":"stone","asset":null,"place_hint":false,"stairs_up":false,"stairs_down":false,
        "door":null,"trap":null,"object_presentation":null,"ground_items":[{
            "class":"coin","description":"","name":"object","appearance":"object",
            "identified":true,"quantity":"3","asset":null,"presentation":null}]})
}

#[test]
fn object_presentation_is_strict_named_bounded_and_required_nullable() {
    for kind in [
        json!({"kind":"stairs"}),
        json!({"kind":"item","class":"potion"}),
    ] {
        for name in ["".to_owned(), "painted stairs".into(), "n".repeat(256)] {
            let value = json!({"name":name,"asset":null,"kind":kind});
            let decoded: ObjectPresentation = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
    }
    let valid = presentation();
    assert!(serde_json::from_value::<ObjectPresentation>(valid.clone()).is_ok());
    for field in ["name", "asset", "kind"] {
        let mut bad = valid.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ObjectPresentation>(bad).is_err(),
            "missing {field}"
        );
    }
    for (field, invalid) in [
        ("name", json!("x".repeat(257))),
        ("name", json!("bad\nname")),
        ("name", Value::Null),
        ("asset", json!("")),
        ("asset", json!("item..wand")),
        ("asset", json!("Item.wand")),
        ("asset", json!("a".repeat(81))),
        ("kind", Value::Null),
        ("kind", json!("stairs")),
        ("kind", json!(["stairs"])),
        ("kind", json!({"kind":"actor"})),
        ("kind", json!({"kind":"stairs","class":"potion"})),
        ("kind", json!({"kind":"item"})),
        ("kind", json!({"kind":"item","class":"actor"})),
        ("kind", json!({"kind":"item","class":{"potion":null}})),
        (
            "kind",
            json!({"kind":"item","class":"potion","target":"hidden"}),
        ),
        ("seed", json!(3)),
        ("true_name", json!("coin")),
    ] {
        let mut bad = valid.clone();
        bad[field] = invalid;
        assert!(
            serde_json::from_value::<ObjectPresentation>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    assert!(
        serde_json::from_value::<ObjectPresentation>(json!(["stairs",null,{"kind":"stairs"}]))
            .is_err()
    );
    assert!(serde_json::from_str::<ObjectPresentation>(
        r#"{"name":"stairs","name":"wand","asset":null,"kind":{"kind":"stairs"}}"#
    )
    .is_err());
    let mut with_asset = valid;
    with_asset["asset"] = json!("item.false-stairs");
    assert!(serde_json::from_value::<ObjectPresentation>(with_asset).is_ok());
}

#[test]
fn current_and_remembered_cosmetics_require_explicit_null_and_keep_true_authority() {
    let mut g = ground();
    let mut c = cell();
    let mut m = remembered();
    assert!(serde_json::from_value::<GroundItemView>(g.clone()).is_ok());
    assert!(serde_json::from_value::<CellView>(c.clone()).is_ok());
    assert!(serde_json::from_value::<RememberedMapCell>(m.clone()).is_ok());
    g["presentation"] = presentation();
    c["stairs_down"] = json!(true);
    c["object_presentation"] = presentation();
    m["stairs_down"] = json!(true);
    m["object_presentation"] = presentation();
    m["ground_items"][0]["presentation"] = presentation();
    let decoded: GroundItemView = serde_json::from_value(g.clone()).unwrap();
    assert_eq!(decoded.item.class, ItemClass::Coin);
    assert_eq!(decoded.item.known_currency, Some(CurrencyKind::Gold));
    assert_eq!(decoded.item.quantity, 3);
    assert_eq!(decoded.item.id, ItemTarget::from_digest([47; 32]));
    let decoded: CellView = serde_json::from_value(c.clone()).unwrap();
    assert!(!decoded.stairs_up && decoded.stairs_down);
    let decoded: RememberedMapCell = serde_json::from_value(m.clone()).unwrap();
    assert!(!decoded.stairs_up && decoded.stairs_down);
    assert_eq!(serde_json::to_value(decoded).unwrap(), m);
    g.as_object_mut().unwrap().remove("presentation");
    c.as_object_mut().unwrap().remove("object_presentation");
    m.as_object_mut().unwrap().remove("object_presentation");
    assert!(serde_json::from_value::<GroundItemView>(g).is_err());
    assert!(serde_json::from_value::<CellView>(c).is_err());
    assert!(serde_json::from_value::<RememberedMapCell>(m).is_err());
    let mut m = remembered();
    m["ground_items"][0]
        .as_object_mut()
        .unwrap()
        .remove("presentation");
    assert!(serde_json::from_value::<RememberedMapCell>(m).is_err());
    let mut c = cell();
    c["true_asset"] = json!("stairs");
    assert!(serde_json::from_value::<CellView>(c).is_err());
    let mut g = ground();
    g["phase"] = json!(2);
    assert!(serde_json::from_value::<GroundItemView>(g).is_err());
    assert!(serde_json::from_value::<CellView>(
        json!([null,true,null,null,"stone","cell",false,false,{"x":1,"y":0,"z":0},false,false])
    )
    .is_err());
}

#[test]
fn cosmetic_outer_fields_reject_impossible_cells_and_true_identity_siblings() {
    let mut g = ground();
    g["presentation"] = presentation();
    assert!(serde_json::from_value::<GroundItemView>(g.clone()).is_ok());
    for (field, value) in [
        ("name", json!("real coin")),
        ("appearance", json!("real coin")),
        ("description", json!("secret description")),
        ("asset", json!("item.coin")),
    ] {
        let mut bad = g.clone();
        bad["item"][field] = value;
        assert!(
            serde_json::from_value::<GroundItemView>(bad).is_err(),
            "{field}"
        );
    }
    let mut m = remembered();
    m["ground_items"][0]["presentation"] = presentation();
    let valid: RememberedMapCell = serde_json::from_value(m.clone()).unwrap();
    assert!(
        !valid.stairs_up && !valid.stairs_down,
        "fake ground stairs grant no navigation"
    );
    for (field, value) in [
        ("name", json!("coin")),
        ("appearance", json!("coin")),
        ("description", json!("secret")),
        ("asset", json!("item.coin")),
    ] {
        let mut bad = m.clone();
        bad["ground_items"][0][field] = value;
        assert!(
            serde_json::from_value::<RememberedMapCell>(bad).is_err(),
            "{field}"
        );
    }
    let mut c = cell();
    c["stairs_down"] = json!(true);
    c["object_presentation"] = presentation();
    assert!(serde_json::from_value::<CellView>(c.clone()).is_ok());
    for (field, value) in [("wall", json!(true)), ("stairs_down", json!(false))] {
        let mut bad = c.clone();
        bad[field] = value;
        assert!(serde_json::from_value::<CellView>(bad).is_err(), "{field}");
    }
    m["stairs_down"] = json!(true);
    m["object_presentation"] = presentation();
    assert!(serde_json::from_value::<RememberedMapCell>(m.clone()).is_ok());
    for (field, value) in [("wall", json!(true)), ("stairs_down", json!(false))] {
        let mut bad = m.clone();
        bad[field] = value;
        assert!(
            serde_json::from_value::<RememberedMapCell>(bad).is_err(),
            "{field}"
        );
    }
}
