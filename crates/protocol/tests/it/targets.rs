use std::collections::BTreeSet;
use tor_protocol::{ActorTarget, DoorTarget, ItemTarget};

#[test]
fn paid_ability_commands_and_events_round_trip_with_opaque_targets() {
    let target = ActorTarget::from_digest([9; 32]);
    for ability in ["power_strike", "magic_bolt", "fear"] {
        let action = serde_json::json!({"type":"use_ability", "ability":ability, "target":target});
        let decoded = serde_json::from_value::<tor_protocol::Action>(action.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), action);
        let event =
            serde_json::json!({"type":"ability_started", "ability":ability, "target":target});
        let decoded = serde_json::from_value::<tor_protocol::Event>(event.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), event);
        for outcome in ["applied", "unaffected", "miss"] {
            let event = serde_json::json!({"type":"ability", "ability":ability, "caster":target, "target":null, "outcome":outcome});
            let decoded =
                serde_json::from_value::<tor_protocol::CombatEventView>(event.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), event);
        }
        for bad_target in [
            serde_json::json!(9),
            serde_json::json!(ItemTarget::from_digest([9; 32])),
        ] {
            let action =
                serde_json::json!({"type":"use_ability", "ability":ability, "target":bad_target});
            assert!(serde_json::from_value::<tor_protocol::Action>(action).is_err());
        }
    }
    for ability in ["basic_melee", "unknown", "MagicBolt"] {
        assert!(serde_json::from_value::<tor_protocol::Action>(
            serde_json::json!({"type":"use_ability", "ability":ability, "target":target})
        )
        .is_err());
    }
    assert!(serde_json::from_value::<tor_protocol::Action>(
        serde_json::json!({"type":"use_ability", "ability":"fear", "target":target, "cost":0})
    )
    .is_err());
}

#[test]
fn target_tokens_preserve_all_bits_and_keep_entity_kinds_distinct() {
    let digest = std::array::from_fn(|index| index as u8);
    let actor = ActorTarget::from_digest(digest);
    let item = ItemTarget::from_digest(digest);
    let door = DoorTarget::from_digest(digest);
    for token in [actor.to_string(), item.to_string(), door.to_string()] {
        assert_eq!(token.len(), 66);
        assert_eq!(
            &token[2..],
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
        );
    }
    let json = serde_json::to_string(&actor).unwrap();
    assert_eq!(serde_json::from_str::<ActorTarget>(&json).unwrap(), actor);
    assert!(serde_json::from_str::<ItemTarget>(&json).is_err());
    assert!(serde_json::from_str::<DoorTarget>(&json).is_err());
    assert_eq!(actor.to_string().parse::<ActorTarget>().unwrap(), actor);
    let mut set = BTreeSet::new();
    set.insert(actor);
    set.insert(ActorTarget::from_digest([255; 32]));
    assert_eq!(set.len(), 2);
}

#[test]
fn target_tokens_reject_numbers_aliases_truncation_and_non_ascii() {
    let canonical = ActorTarget::from_digest([0xab; 32]).to_string();
    for value in [
        "1".to_owned(),
        canonical.to_uppercase(),
        format!(" {canonical}"),
        format!("{canonical} "),
        canonical[..65].to_owned(),
        format!("{canonical}0"),
        canonical.replacen("ab", "zz", 1),
        canonical.replacen("ab", "🌒", 1),
        canonical.replacen("a_", "i_", 1),
    ] {
        assert!(value.parse::<ActorTarget>().is_err(), "accepted {value:?}");
        assert!(serde_json::from_value::<ActorTarget>(serde_json::json!(value)).is_err());
    }
    assert!(serde_json::from_value::<ActorTarget>(serde_json::json!(1)).is_err());
}
