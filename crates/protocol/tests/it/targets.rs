use std::collections::BTreeSet;
use tor_protocol::{ActorTarget, DoorTarget, ItemTarget};

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
