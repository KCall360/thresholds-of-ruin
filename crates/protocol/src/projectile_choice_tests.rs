use super::*;
use crate::{
    decode_request, decode_response, encode_response, ActorTarget, ClientMessage, Command, Request,
    ServerMessage,
};
use serde_json::{json, Value};

fn query(phase: ProjectileChoicePhase) -> ProjectileChoiceQuery {
    ProjectileChoiceQuery {
        ability: "fixture.launch".into(),
        phase,
        offset: 0,
    }
}
fn launcher() -> EquippedProjectileWeapon {
    EquippedProjectileWeapon {
        item: ItemTarget::from_digest([1; 32]),
        socket: 63,
    }
}
fn context() -> ReplyContext {
    serde_json::from_value(json!({"snapshot":"fixture", "input":{"stream":{"stream":"attachment","epoch":"0"},"readiness_revision":"0"},"actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"revision":"0"})).unwrap()
}
fn page() -> ProjectileChoicePage {
    ProjectileChoicePage {
        query: query(ProjectileChoicePhase::Ammunition {
            launcher: launcher(),
        }),
        choices: vec![ProjectileChoice::Ammunition {
            item: ItemTarget::from_digest([2; 32]),
        }],
        next_offset: None,
    }
}
fn reply() -> Value {
    serde_json::to_value(ServerMessage::ProjectileChoices {
        context: context(),
        request_id: "choices".into(),
        page: Box::new(page()),
    })
    .unwrap()
}
fn assert_bad_reply(value: Value) {
    assert!(
        decode_response(&serde_json::to_string(&value).unwrap()).is_err(),
        "{value}"
    );
}
#[test]
fn projectile_choice_named_records_are_exact_at_every_nesting_level() {
    let value = reply();
    let decoded = decode_response(&value.to_string()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    for pointer in [
        "",
        "/page",
        "/page/query",
        "/page/query/phase",
        "/page/query/phase/launcher",
        "/page/choices/0",
    ] {
        let record = value.pointer(pointer).unwrap().as_object().unwrap();
        for field in record.keys() {
            let mut bad = value.clone();
            bad.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert_bad_reply(bad);
        }
        let mut bad = value.clone();
        bad.pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(0));
        assert_bad_reply(bad);
        let mut bad = value.clone();
        *bad.pointer_mut(pointer).unwrap() = Value::Array(record.values().cloned().collect());
        assert_bad_reply(bad);
        for (key, field) in record {
            let original = serde_json::to_string(&Value::Object(record.clone())).unwrap();
            let duplicate = format!(
                "{{{}:{},{}",
                serde_json::to_string(key).unwrap(),
                field,
                &original[1..]
            );
            let text = value.to_string().replacen(&original, &duplicate, 1);
            assert!(decode_response(&text).is_err(), "duplicate {pointer}/{key}");
        }
    }
}
#[test]
fn projectile_choice_shape_bounds_phase_and_cursor_are_checked() {
    for (pointer, bad) in [
        ("/page/query/ability", json!("")),
        ("/page/query/ability", json!("a".repeat(61))),
        ("/page/query/offset", json!(-1)),
        ("/page/query/offset", json!(4_294_967_296_u64)),
        ("/page/query/offset", json!("0")),
        ("/page/query/phase/launcher/socket", json!(64)),
        (
            "/page/query/phase/launcher/item",
            json!(ActorTarget::from_digest([1; 32])),
        ),
        ("/page/choices/0/item", json!(launcher().item)),
        (
            "/page/choices/0",
            json!({"kind":"launcher","launcher":launcher()}),
        ),
        ("/page/next_offset", json!(0)),
        ("/page/next_offset", json!(129)),
        ("/page/next_offset", json!("128")),
        ("/request_id", json!("")),
        ("/request_id", json!("x".repeat(129))),
        ("/request_id", json!("bad\nrequest")),
    ] {
        let mut value = reply();
        *value.pointer_mut(pointer).unwrap() = bad;
        assert_bad_reply(value);
    }
    let mut value = page();
    value.choices = vec![value.choices[0]; 2];
    assert!(!value.is_valid(), "duplicate inventory identity");
    value.choices = (2..=18)
        .map(|n| ProjectileChoice::Ammunition {
            item: ItemTarget::from_digest([n; 32]),
        })
        .collect();
    assert!(
        serde_json::from_value::<ProjectileChoicePage>(serde_json::to_value(&value).unwrap())
            .is_err(),
        "eager row bound"
    );
    value.choices.clear();
    value.next_offset = Some(128);
    assert!(
        value.is_valid(),
        "empty filtered page advances scanned inventory cursor"
    );
    value.query.offset = u32::MAX;
    value.next_offset = Some(0);
    assert!(!value.is_valid(), "cursor cannot wrap");
    value.next_offset = None;
    value.choices = page().choices;
    assert!(
        !value.is_valid(),
        "emitted rows cannot overflow inventory cursor"
    );
}
#[test]
fn projectile_choices_echo_exact_query_and_accept_only_matching_phase_rows() {
    let phases = [
        (
            ProjectileChoicePhase::Sources {},
            ProjectileChoice::Throw {
                source: ThrownSourceSelection::Carried {
                    item: ItemTarget::from_digest([2; 32]),
                    socket: 63,
                },
            },
        ),
        (
            ProjectileChoicePhase::Sources {},
            ProjectileChoice::Launcher {
                launcher: launcher(),
            },
        ),
        (
            ProjectileChoicePhase::Ammunition {
                launcher: launcher(),
            },
            page().choices[0],
        ),
    ];
    for (phase, choice) in phases {
        let page = ProjectileChoicePage {
            query: query(phase),
            choices: vec![choice],
            next_offset: Some(1),
        };
        assert!(page.valid_for(&page.query));
        assert_eq!(
            serde_json::from_value::<ProjectileChoicePage>(serde_json::to_value(&page).unwrap())
                .unwrap(),
            page
        );
        let mut other = page.query.clone();
        other.offset += 1;
        assert!(!page.valid_for(&other));
        other = page.query.clone();
        other.ability = "other.ability".into();
        assert!(!page.valid_for(&other));
        for (other_phase, _) in phases {
            if other_phase != phase {
                other = page.query.clone();
                other.phase = other_phase;
                assert!(!page.valid_for(&other));
            }
        }
    }
    let mixed = ProjectileChoicePage {
        query: query(ProjectileChoicePhase::Sources {}),
        choices: vec![phases[0].1, phases[1].1],
        next_offset: None,
    };
    assert!(
        !mixed.is_valid(),
        "one ability cannot mix throw and launcher sources"
    );
    assert!(
        serde_json::from_value::<ProjectileChoicePage>(serde_json::to_value(&mixed).unwrap())
            .is_err()
    );
    let mut other = page().query.clone();
    other.phase = ProjectileChoicePhase::Ammunition {
        launcher: EquippedProjectileWeapon {
            socket: 62,
            ..launcher()
        },
    };
    assert!(
        !page().valid_for(&other),
        "exact launcher socket is part of reply binding"
    );
}
#[test]
fn projectile_choice_commands_are_strict_and_replies_obey_complete_envelope_budget() {
    let command = Command::ProjectileChoices {
        expected_revision: u64::MAX,
        query: page().query,
    };
    let mut wire = serde_json::to_value(ClientMessage::Request {
        request_id: "choices".into(),
        request: Box::new(Request::Command {
            context: context().input,
            branch: context().branch,
            command,
        }),
    })
    .unwrap();
    assert!(decode_request(&wire.to_string()).is_ok());
    for field in ["expected_revision", "query"] {
        let mut bad = wire.clone();
        bad["request"]["command"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode_request(&bad.to_string()).is_err());
    }
    wire["request"]["command"]["expected_revision"] = json!(0);
    assert!(decode_request(&wire.to_string()).is_err());
    let message = decode_response(&reply().to_string()).unwrap();
    let encoded = encode_response(&message, None, MAX_PROJECTILE_CHOICE_PAGE_BYTES).unwrap();
    assert_eq!(decode_response(&encoded.text).unwrap(), message);
    assert!(encode_response(&message, None, encoded.text.len() - 1).is_err());
}
