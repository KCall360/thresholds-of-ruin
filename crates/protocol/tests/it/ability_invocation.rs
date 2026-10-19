//! Current, independently named wire selection boundary and ordinary controls.
use serde_json::json;

#[test]
fn selected_ability_invocations_are_named_closed_and_current_only() {
    let target = tor_protocol::ActorTarget::from_digest([7; 32]);
    for invocation in [
        json!({"kind":"actor_target","target":target}),
        json!({"kind":"self_step","direction":"north_east"}),
    ] {
        let value = json!({"type":"use_ability","id":"content_owned_name",
            "invocation":{"aim":invocation,"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}});
        let action: tor_protocol::Action =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(action).unwrap(), value);
    }
    for invocation in [
        json!(["self_step", "north"]),
        json!({"kind":"self_step"}),
        json!({"kind":"self_step","direction":"east_up"}),
        json!({"kind":"self_step","direction":"North"}),
        json!({"kind":"self_step","direction":"north","target":target}),
        json!({"kind":"self_step","direction":"north","destination":{"region":1,"position":{"x":1,"y":2,"z":0}}}),
        json!({"kind":"actor_target","target":target,"direction":"north"}),
    ] {
        let value = json!({"type":"use_ability","id":"content_owned_name",
            "invocation":{"aim":invocation,"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}});
        assert!(serde_json::from_slice::<tor_protocol::Action>(
            &serde_json::to_vec(&value).unwrap()
        )
        .is_err());
    }
    let legacy = json!({"type":"use_ability","id":"content_owned_name","target":target,"supplies":{"kind":"carried"}});
    assert!(
        serde_json::from_slice::<tor_protocol::Action>(&serde_json::to_vec(&legacy).unwrap())
            .is_err()
    );
    for raw in [
        r#"{"type":"use_ability","id":"probe","invocation":{"aim":{"kind":"self_step","direction":"north","direction":"south"},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}"#,
        r#"{"type":"use_ability","id":"probe","invocation":{"aim":["self_step",{"direction":"north"}],"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}"#,
        r#"{"type":"use_ability","id":"probe","invocation":{"aim":{"kind":"self_step","kind":"actor_target","direction":"north"},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}}"#,
    ] {
        assert!(serde_json::from_slice::<tor_protocol::Action>(raw.as_bytes()).is_err());
    }
}

#[test]
fn ordinary_movement_and_wait_remain_valid_controls_of_the_invocation_decoder() {
    for value in [
        json!({"type":"move","direction":"north"}),
        json!({"type":"wait"}),
    ] {
        let action: tor_protocol::Action =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(action).unwrap(), value);
    }
}

#[test]
fn selected_step_direction_is_a_string_token_not_an_externally_tagged_enum_object() {
    for direction in [
        serde_json::json!({"north":null}),
        serde_json::json!(["north"]),
        serde_json::json!(null),
        serde_json::json!(0),
    ] {
        let action = serde_json::json!({"type":"use_ability","id":"probe",
            "invocation":{"aim":{"kind":"self_step","direction":direction},"weapon":{"kind":"default"}},
            "supplies":{"kind":"carried"}});
        assert!(
            serde_json::from_slice::<tor_protocol::Action>(&serde_json::to_vec(&action).unwrap())
                .is_err(),
            "{action}"
        );
    }
}

#[test]
fn selected_step_invocations_round_trip_all_ten_direction_tokens() {
    for direction in [
        "north",
        "east",
        "south",
        "west",
        "north_east",
        "south_east",
        "south_west",
        "north_west",
        "up",
        "down",
    ] {
        let value =
            json!({"aim":{"kind":"self_step","direction":direction},"weapon":{"kind":"default"}});
        let invocation: tor_protocol::AbilityInvocation =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(invocation).unwrap(), value);
    }
}

#[test]
fn directional_ray_is_targetless_closed_and_accepts_exactly_ten_direction_tokens() {
    for direction in [
        "north",
        "east",
        "south",
        "west",
        "north_east",
        "south_east",
        "south_west",
        "north_west",
        "up",
        "down",
    ] {
        let value = json!({"aim":{"kind":"directional_ray","direction":direction},"weapon":{"kind":"default"}});
        let invocation: tor_protocol::AbilityInvocation =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(invocation).unwrap(), value);
    }
    for invocation in [
        json!(["directional_ray", "north"]),
        json!({"kind":"directional_ray"}),
        json!({"kind":"directional_ray","direction":"east_up"}),
        json!({"kind":"directional_ray","direction":"North"}),
        json!({"kind":"directional_ray","direction":{"north":null}}),
        json!({"kind":"directional_ray","direction":["north"]}),
        json!({"kind":"directional_ray","direction":null}),
        json!({"kind":"directional_ray","direction":0}),
        json!({"kind":"directional_ray","direction":"north","target":tor_protocol::ActorTarget::from_digest([7;32])}),
        json!({"kind":"directional_ray","direction":"north","target":null}),
        json!({"kind":"directional_ray","direction":"north","destination":{"region":1,"position":{"x":1,"y":2,"z":0}}}),
        json!({"kind":"directional_ray","direction":"north","path":[]}),
    ] {
        let value = json!({"type":"use_ability","id":"content_owned_name",
            "invocation":{"aim":invocation,"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}});
        assert!(
            serde_json::from_slice::<tor_protocol::Action>(&serde_json::to_vec(&value).unwrap())
                .is_err(),
            "{value}"
        );
    }
    for raw in [
        r#"{"kind":"directional_ray","direction":"north","direction":"south"}"#,
        r#"{"kind":"directional_ray","kind":"self_step","direction":"north"}"#,
    ] {
        let raw = format!(r#"{{"aim":{raw},"weapon":{{"kind":"default"}}}}"#);
        assert!(serde_json::from_slice::<tor_protocol::AbilityInvocation>(raw.as_bytes()).is_err());
    }
}

#[test]
fn invocation_requires_independent_named_aim_and_weapon_operands() {
    let aim = json!({"kind":"self_step", "direction":"north"});
    let weapon = json!({"kind":"default"});
    for invalid in [
        json!({"aim":aim}),
        json!({"weapon":weapon}),
        json!({}),
        json!([aim, weapon]),
        json!({"aim":aim,"weapon":weapon,"target":null}),
        json!({"aim":aim,"weapon":null}),
        json!({"aim":null,"weapon":weapon}),
        json!({"kind":"self_step","direction":"north"}),
    ] {
        assert!(
            serde_json::from_value::<tor_protocol::AbilityInvocation>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
    for raw in [
        r#"{"aim":{"kind":"self_step","direction":"north"},"aim":{"kind":"self_step","direction":"south"},"weapon":{"kind":"default"}}"#,
        r#"{"aim":{"kind":"self_step","direction":"north"},"weapon":{"kind":"default"},"weapon":{"kind":"default"}}"#,
    ] {
        assert!(serde_json::from_str::<tor_protocol::AbilityInvocation>(raw).is_err());
    }
}

#[test]
fn exact_equipped_weapon_selection_round_trips_and_rejects_ambiguous_operands() {
    let item = tor_protocol::ItemTarget::from_digest([11; 32]);
    let aim =
        json!({"kind":"actor_target","target":tor_protocol::ActorTarget::from_digest([7;32])});
    for socket in [0, 63] {
        let value = json!({"aim":aim,"weapon":{"kind":"equipped","item":item,"socket":socket}});
        let invocation: tor_protocol::AbilityInvocation =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(invocation).unwrap(), value);
    }
    for weapon in [
        json!("default"),
        json!(["default"]),
        json!({"kind":"default","item":item}),
        json!({"kind":"default","socket":0}),
        json!({"kind":"equipped"}),
        json!({"kind":"equipped","item":item}),
        json!({"kind":"equipped","socket":0}),
        json!({"kind":"equipped","item":item,"socket":0,"target":null}),
        json!({"kind":"equipped","item":1,"socket":0}),
        json!({"kind":"equipped","item":tor_protocol::ActorTarget::from_digest([11;32]),"socket":0}),
        json!({"kind":"equipped","item":item,"socket":-1}),
        json!({"kind":"equipped","item":item,"socket":64}),
        json!({"kind":"equipped","item":item,"socket":65535}),
        json!({"kind":"equipped","item":item,"socket":65536}),
        json!({"kind":"equipped","item":item,"socket":0.0}),
        json!({"kind":"equipped","item":item,"socket":"0"}),
    ] {
        let value = json!({"aim":aim,"weapon":weapon});
        assert!(
            serde_json::from_value::<tor_protocol::AbilityInvocation>(value.clone()).is_err(),
            "{value}"
        );
    }
    for weapon in [
        format!(r#"{{"kind":"equipped","item":"{item}","item":"{item}","socket":0}}"#),
        format!(r#"{{"kind":"equipped","item":"{item}","socket":0,"socket":1}}"#),
        format!(r#"{{"kind":"equipped","kind":"default","item":"{item}","socket":0}}"#),
    ] {
        let raw = format!(r#"{{"aim":{aim},"weapon":{weapon}}}"#);
        assert!(
            serde_json::from_str::<tor_protocol::AbilityInvocation>(&raw).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn projectile_invocations_keep_exact_typed_delivery_and_all_ten_directions() {
    let item = tor_protocol::ItemTarget::from_digest([11; 32]);
    let ammunition = tor_protocol::ItemTarget::from_digest([12; 32]);
    for delivery in [
        json!({"kind":"throw","source":{"kind":"equipped","item":item,"socket":0}}),
        json!({"kind":"throw","source":{"kind":"carried","item":item,"socket":63}}),
        json!({"kind":"launch","launcher":{"item":item,"socket":0},"ammunition":ammunition}),
    ] {
        for direction in [
            "north",
            "east",
            "south",
            "west",
            "north_east",
            "south_east",
            "south_west",
            "north_west",
            "up",
            "down",
        ] {
            let value = json!({"aim":{"kind":"directional_projectile","direction":direction,"delivery":delivery},"weapon":{"kind":"default"}});
            let invocation: tor_protocol::AbilityInvocation =
                serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(invocation).unwrap(), value);
        }
    }
}

#[test]
fn projectile_invocations_reject_ambiguous_delivery_direction_and_outer_weapon() {
    let item = tor_protocol::ItemTarget::from_digest([11; 32]);
    let ammunition = tor_protocol::ItemTarget::from_digest([12; 32]);
    let source = json!({"kind":"carried","item":item,"socket":0});
    let delivery = json!({"kind":"throw","source":source});
    let valid = json!({"aim":{"kind":"directional_projectile","direction":"north","delivery":delivery},"weapon":{"kind":"default"}});
    assert!(serde_json::from_value::<tor_protocol::AbilityInvocation>(valid.clone()).is_ok());
    let mut invalid = Vec::new();
    for bad in [
        json!(null),
        json!(1),
        json!({"north":null}),
        json!(["north"]),
        json!("north_up"),
    ] {
        let mut value = valid.clone();
        value["aim"]["direction"] = bad;
        invalid.push(value);
    }
    for extra in ["target", "path", "destination", "weapon"] {
        let mut value = valid.clone();
        value["aim"][extra] = json!(null);
        invalid.push(value);
    }
    let mut value = valid.clone();
    value["weapon"] = json!({"kind":"equipped","item":item,"socket":0});
    invalid.push(value);
    for bad in [
        json!(null),
        json!(["throw", source]),
        json!({"kind":"throw"}),
        json!({"kind":"throw","source":source,"ammunition":ammunition}),
        json!({"kind":"launch","launcher":{"item":item,"socket":0}}),
        json!({"kind":"launch","launcher":{"item":item,"socket":0},"ammunition":item}),
        json!({"kind":"launch","launcher":[item,0],"ammunition":ammunition}),
        json!({"kind":"launch","launcher":{"item":item,"socket":64},"ammunition":ammunition}),
        json!({"kind":"launch","launcher":{"item":item,"socket":0,"extra":null},"ammunition":ammunition}),
    ] {
        let mut value = valid.clone();
        value["aim"]["delivery"] = bad;
        invalid.push(value);
    }
    for kind in ["equipped", "carried"] {
        for bad in [
            json!([kind, item, 0]),
            json!({"kind":kind,"item":item}),
            json!({"kind":kind,"socket":0}),
            json!({"kind":kind,"item":1,"socket":0}),
            json!({"kind":kind,"item":tor_protocol::ActorTarget::from_digest([11;32]),"socket":0}),
            json!({"kind":kind,"item":item,"socket":64}),
            json!({"kind":kind,"item":item,"socket":"0"}),
            json!({"kind":kind,"item":item,"socket":0.0}),
            json!({"kind":kind,"item":item,"socket":-1}),
            json!({"kind":kind,"item":item,"socket":0,"extra":null}),
        ] {
            let mut value = valid.clone();
            value["aim"]["delivery"]["source"] = bad;
            invalid.push(value);
        }
    }
    for value in invalid {
        assert!(
            serde_json::from_value::<tor_protocol::AbilityInvocation>(value.clone()).is_err(),
            "{value}"
        );
    }
    let raw = serde_json::to_string(&valid).unwrap();
    for (from, to) in [
        (
            "\"direction\":\"north\"".to_owned(),
            "\"direction\":\"north\",\"direction\":\"south\"".to_owned(),
        ),
        (
            "\"kind\":\"throw\"".to_owned(),
            "\"kind\":\"throw\",\"kind\":\"launch\"".to_owned(),
        ),
        (
            "\"socket\":0".to_owned(),
            "\"socket\":0,\"socket\":1".to_owned(),
        ),
        (
            format!("\"item\":\"{item}\""),
            format!("\"item\":\"{item}\",\"item\":\"{item}\""),
        ),
    ] {
        assert!(
            serde_json::from_str::<tor_protocol::AbilityInvocation>(&raw.replace(&from, &to))
                .is_err()
        );
    }
}
