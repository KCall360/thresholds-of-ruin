use tor_protocol::*;

#[test]
fn personal_stats_use_named_catalog_values_and_reject_hidden_state_fields() {
    let value = serde_json::json!({
        "kind": "humanoid", "subtypes": ["goblinoid"],
        "hit_dice": ["racial", "warrior", "mage"],
        "attributes": {"strength": 2, "speed": 1, "intellect": 3,
            "willpower": 2, "awareness": 1, "presence": 0},
        "skills": [{"skill": "spellcasting", "rank": 2}],
        "defenses": {"physical": 13, "cognitive": 15, "spiritual": 11},
        "binding": "intellect",
        "resources": [{"resource": "mana", "balance": 4, "maximum": 5,
            "available": 3, "reserved": 1}],
        "active_talents": ["magic_bolt"], "dormant_talents": ["fear"],
        "abilities": ["basic_melee", "magic_bolt"]
    });
    let stats: OwnStats = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(stats.kind, CreatureType::Humanoid);
    assert_eq!(
        stats.hit_dice,
        [
            HitDieSource::Racial,
            HitDieSource::Warrior,
            HitDieSource::Mage
        ]
    );
    assert_eq!(serde_json::to_value(&stats).unwrap(), value);
    for key in ["health_seed", "reservation_owner", "grant_sources", "actor"] {
        let mut leaked = value.clone();
        leaked[key] = serde_json::json!(7);
        assert!(serde_json::from_value::<OwnStats>(leaked).is_err());
    }
    let mut unknown = value;
    unknown["kind"] = serde_json::json!("unrecognized");
    assert!(serde_json::from_value::<OwnStats>(unknown).is_err());
    // Inspection includes basic melee, but that label is not a paid command.
    assert!(serde_json::from_str::<Ability>("\"basic_melee\"").is_err());
}
