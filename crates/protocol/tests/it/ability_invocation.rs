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
            "invocation":invocation,"supplies":{"kind":"carried"}});
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
            "invocation":invocation,"supplies":{"kind":"carried"}});
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
        r#"{"type":"use_ability","id":"probe","invocation":{"kind":"self_step","direction":"north","direction":"south"},"supplies":{"kind":"carried"}}"#,
        r#"{"type":"use_ability","id":"probe","invocation":["self_step",{"direction":"north"}],"supplies":{"kind":"carried"}}"#,
        r#"{"type":"use_ability","id":"probe","invocation":{"kind":"self_step","kind":"actor_target","direction":"north"},"supplies":{"kind":"carried"}}"#,
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
            "invocation":{"kind":"self_step","direction":direction},
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
        let value = json!({"kind":"self_step","direction":direction});
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
        let value = json!({"kind":"directional_ray","direction":direction});
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
            "invocation":invocation,"supplies":{"kind":"carried"}});
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
        assert!(serde_json::from_slice::<tor_protocol::AbilityInvocation>(raw.as_bytes()).is_err());
    }
}
