use serde_json::json;
use tor_protocol::*;

#[test]
fn nutrition_and_eat_wire_shapes_are_strict_and_personal() {
    let good = json!({"kind":"required","reserve":6,"maximum":12,"hunger":"hungry","unit_ticks":"100","elapsed":"57","starve_at":-3});
    let parsed: NutritionView = serde_json::from_value(good.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), good);
    for field in good.as_object().unwrap().keys() {
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<NutritionView>(bad).is_err(),
            "{field}"
        );
    }
    for (key, value) in [
        ("unit_ticks", json!(100)),
        ("elapsed", json!("100")),
        ("maximum", json!(0)),
        ("reserve", json!(-4)),
        ("hunger", json!("empty")),
        ("private", json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(
            serde_json::from_value::<NutritionView>(bad).is_err(),
            "{key}"
        );
    }
    assert!(serde_json::from_value::<NutritionView>(json!({"kind":"exempt","reserve":0})).is_err());
    let item = ItemTarget::from_digest([7; 32]);
    let action = json!({"type":"eat","item":item});
    assert_eq!(
        serde_json::from_value::<Action>(action.clone()).unwrap(),
        Action::Eat { item }
    );
    for bad in [
        json!({"type":"eat"}),
        json!({"type":"eat","item":item,"quantity":"2"}),
        json!({"type":"eat","item":"7"}),
    ] {
        assert!(serde_json::from_value::<Action>(bad).is_err());
    }
    // Visible actors have no nutrition field to carry someone else's reserve.
    assert!(serde_json::from_value::<CombatActorView>(json!({"actor":ActorTarget::from_digest([1;32]),"hostile":false,"allied":true,"injury":"healthy","nutrition":good})).is_err());
}

fn personal_state(stats: OwnStats) -> StateView {
    serde_json::from_value(json!({"wizard_game":false,"revision":"1","observation":{
        "linked_holds":[],"actor":"1","self_target":ActorTarget::from_digest([1;32]),"tick":"15",
        "position":{"x":0,"y":0,"z":0},"visible_cells":[],"ground_items":[],
        "inventory":[],"visible_actors":[],"places":[],"ready":false,
        "interactions":null,"motion":null,"combat":{
            "hp":"9","max_hp":"10","own_stats":stats,"preparation_remaining":null,
            "preparation_active":false,"sequence":null,"public_plan":null,
            "recovery_remaining":"0","actors":[],"events":[],"objective":null,
            "victory":false,"dead":false,"terminal":false}}}))
    .unwrap()
}

#[test]
fn hunger_labels_do_not_infer_the_disclosed_health_recovery_policy() {
    for (reserve, hunger) in [(8, "nourished"), (6, "hungry"), (2, "weak"), (0, "empty")] {
        for stopped in [false, true] {
            let mut value = serde_json::to_value(super::creature::stats()).unwrap();
            value["nutrition"] = json!({"kind":"required", "reserve":reserve,
                "maximum":12, "hunger":hunger, "unit_ticks":"100", "elapsed":"57", "starve_at":-3});
            value["health_recovery"]["stopped"] = json!(stopped);
            let parsed = serde_json::from_value::<OwnStats>(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
            assert_eq!(personal_state(parsed).validate(), Ok(()));
            for (field, invalid) in [
                ("period_ticks", json!("0")),
                ("elapsed_ticks", json!("1200")),
                ("reset_percent", json!(101)),
            ] {
                let mut malformed = value.clone();
                malformed["health_recovery"][field] = invalid;
                let malformed: OwnStats = serde_json::from_value(malformed).unwrap();
                assert_eq!(
                    personal_state(malformed).validate(),
                    Err(InvalidState::InvalidCombat),
                    "{field}"
                );
            }
            let mut leaked = value;
            leaked["nutrition"]["weak_recovery"] = json!({"kind":"normal"});
            assert!(serde_json::from_value::<OwnStats>(leaked).is_err());
        }
    }
}
