use tor_simulation::attacks::MeleeAttack;
use tor_simulation::attributes::Skill;
use tor_simulation::combat::DamageType;
use tor_simulation::damage::{DamageComponent, DamageSpec, Protection};
use tor_simulation::dice::{DicePool, Edge};
use tor_simulation::grants::Descriptor;

fn rolled() -> DamageSpec {
    let primary = DamageComponent::rolled(DamageType::Keen, None, DicePool::new(2, 6, -1).unwrap());
    let key = primary.key();
    DamageSpec::new(
        vec![
            DamageComponent::fixed(DamageType::Energy, Some(Descriptor::Fire), 3),
            primary,
        ],
        Some(key),
    )
    .unwrap()
}

#[test]
fn skill_timing_and_primary_are_explicit_and_validated() {
    let damage = rolled();
    let definition = MeleeAttack::new(Skill::LightWeaponry, 2, 80, 50, damage.clone()).unwrap();
    assert_eq!(definition.skill(), Skill::LightWeaponry);
    assert_eq!(definition.bonus(), 2);
    assert_eq!((definition.wind_up(), definition.recovery()), (80, 50));
    assert_eq!(definition.damage(), &damage);
    for (skill, bonus, wind_up, recovery) in [
        (Skill::Spellcasting, 0, 80, 50),
        (Skill::LightWeaponry, 1001, 80, 50),
        (Skill::LightWeaponry, 0, 0, 50),
        (Skill::LightWeaponry, 0, 80, 1_000_001),
    ] {
        assert!(MeleeAttack::new(skill, bonus, wind_up, recovery, damage.clone()).is_err());
    }
    let no_primary = DamageSpec::new(damage.components().to_vec(), None).unwrap();
    assert!(MeleeAttack::new(Skill::HeavyWeaponry, 0, 80, 50, no_primary).is_err());
}

#[test]
fn primary_dice_and_signed_flat_modifiers_keep_categories_descriptors_and_other_components() {
    let definition = MeleeAttack::new(Skill::LightWeaponry, 0, 80, 50, rolled()).unwrap();
    let damage = definition.damage_with_modifiers(1, -2, 3).unwrap();
    assert_eq!(damage.primary(), definition.damage().primary());
    let secondary = &definition.damage().components()[1];
    assert_eq!(
        damage
            .components()
            .iter()
            .find(|c| c.key() == secondary.key())
            .unwrap(),
        secondary
    );
    let mut rng = 42;
    let result = damage.resolve(&mut rng, Edge::from_counts(1, 0), &Protection::default());
    let primary = &result.components[0];
    let pool = primary.pool.as_ref().unwrap();
    assert_eq!(
        pool.rolled.len(),
        4,
        "three original dice plus one advantage roll"
    );
    assert_eq!(pool.kept.len(), 3);
    assert_eq!(
        primary.raw,
        (pool.kept.iter().map(|&n| i32::from(n)).sum::<i32>() - 3).max(0) as u32
    );
    assert_eq!(
        result
            .components
            .iter()
            .find(|c| c.key.category == DamageType::Energy)
            .unwrap()
            .raw,
        3
    );
    assert_eq!(
        result
            .components
            .iter()
            .find(|c| c.key.category == DamageType::Impact)
            .unwrap()
            .raw,
        3
    );
    assert_eq!(
        definition.damage().components().len(),
        2,
        "source definition stays immutable"
    );
}

#[test]
fn fixed_primary_combines_signed_melee_and_power_bonus_before_clamping() {
    let primary = DamageComponent::fixed(DamageType::Impact, Some(Descriptor::Cold), 2);
    let key = primary.key();
    let damage = DamageSpec::new(vec![primary], Some(key)).unwrap();
    let definition = MeleeAttack::new(Skill::HeavyWeaponry, 0, 100, 100, damage).unwrap();
    let modified = definition.damage_with_modifiers(9, -4, 3).unwrap();
    let mut rng = 42;
    let result = modified.resolve(&mut rng, Edge::from_counts(2, 0), &Protection::default());
    assert_eq!(result.total, 1, "2 - 4 + 3, with one final clamp");
    assert_eq!(rng, 42, "a fixed amount has no die to extend or reroll");
    assert_eq!(result.components[0].key, key);
    assert!(result.components[0].pool.is_none());
    assert!(definition.damage_with_modifiers(0, 1_000_001, 0).is_err());
    assert!(definition.damage_with_modifiers(0, 0, 1_000_001).is_err());
}

#[test]
fn rolled_primary_combines_modifiers_without_refreshing_dice_or_changing_roll_order() {
    let primary = DamageComponent::rolled(
        DamageType::Impact,
        Some(Descriptor::Cold),
        DicePool::new(1, 6, -5).unwrap(),
    );
    let key = primary.key();
    let definition = MeleeAttack::new(
        Skill::HeavyWeaponry,
        0,
        100,
        100,
        DamageSpec::new(vec![primary], Some(key)).unwrap(),
    )
    .unwrap();
    let modified = definition.damage_with_modifiers(0, -2, 3).unwrap();
    let mut rng = 42;
    let result = modified.resolve(&mut rng, Edge::default(), &Protection::default());
    let pool = result.components[0].pool.as_ref().unwrap();
    assert_eq!(
        result.components[0].raw,
        (i32::from(pool.kept[0]) - 4).max(0) as u32
    );
    assert_eq!(result.components[0].key, key);
    assert!(definition.damage_with_modifiers(64, 0, 0).is_err());
}

#[test]
fn source_record_round_trip_preserves_modifiers_edges_rng_and_protection() {
    use tor_simulation::attacks::MeleeAttackRecord;
    let original = MeleeAttack::new(Skill::LightWeaponry, 2, 80, 50, rolled()).unwrap();
    let encoded = serde_json::to_value(MeleeAttackRecord::capture(&original)).unwrap();
    assert_eq!(encoded["skill"], "light_weaponry");
    let record: MeleeAttackRecord = serde_json::from_value(encoded).unwrap();
    let restored = record.restore().unwrap();
    assert_eq!(restored, original);
    let protection = Protection::from_grants([tor_simulation::grants::Grant::Reduction(
        tor_simulation::grants::Selector::Descriptor(Descriptor::Fire),
        2,
    )])
    .unwrap();
    for seed in 0..32 {
        for edge in [
            Edge::default(),
            Edge::from_counts(2, 0),
            Edge::from_counts(0, 2),
        ] {
            let mut expected_rng = seed;
            let mut actual_rng = seed;
            let expected = original.damage_with_modifiers(1, -2, 3).unwrap().resolve(
                &mut expected_rng,
                edge,
                &protection,
            );
            let actual = restored.damage_with_modifiers(1, -2, 3).unwrap().resolve(
                &mut actual_rng,
                edge,
                &protection,
            );
            assert_eq!(actual, expected);
            assert_eq!(actual_rng, expected_rng);
        }
    }
}

#[test]
fn source_record_rejects_unknown_fields_bounded_collections_and_invalid_mechanics() {
    use tor_simulation::attacks::MeleeAttackRecord;
    let original = MeleeAttack::new(Skill::LightWeaponry, 0, 80, 50, rolled()).unwrap();
    let encoded = serde_json::to_value(MeleeAttackRecord::capture(&original)).unwrap();
    for path in [vec![], vec!["damage"], vec!["damage", "primary"]] {
        let mut invalid = encoded.clone();
        let mut parent = &mut invalid;
        for key in path {
            parent = &mut parent[key];
        }
        parent
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::json!(1));
        assert!(serde_json::from_value::<MeleeAttackRecord>(invalid).is_err());
    }
    let mut too_many = encoded.clone();
    let component = too_many["damage"]["components"][0].clone();
    too_many["damage"]["components"] = serde_json::json!(vec![component; 33]);
    assert!(serde_json::from_value::<MeleeAttackRecord>(too_many).is_err());
    for (path, value) in [
        (vec!["skill"], serde_json::json!("spellcasting")),
        (vec!["wind_up"], serde_json::json!(0)),
        (vec!["bonus"], serde_json::json!(1001)),
        (vec!["damage", "primary"], serde_json::Value::Null),
        (vec!["damage", "components"], serde_json::json!([])),
    ] {
        let mut invalid = encoded.clone();
        let mut target = &mut invalid;
        for key in path {
            target = &mut target[key];
        }
        *target = value;
        let record: MeleeAttackRecord = serde_json::from_value(invalid).unwrap();
        assert!(record.restore().is_err());
    }
    let mut invalid_dice = encoded.clone();
    invalid_dice["damage"]["components"][0]["amount"]["count"] = serde_json::json!(65);
    let record: MeleeAttackRecord = serde_json::from_value(invalid_dice).unwrap();
    assert!(record.restore().is_err());
    let mut missing_primary = encoded;
    missing_primary["damage"]
        .as_object_mut()
        .unwrap()
        .remove("primary");
    assert!(serde_json::from_value::<MeleeAttackRecord>(missing_primary).is_err());
}

#[test]
fn fixed_primary_record_keeps_descriptor_and_never_invents_damage_dice() {
    use tor_simulation::attacks::MeleeAttackRecord;
    let primary = DamageComponent::fixed(DamageType::Impact, Some(Descriptor::Cold), 2);
    let key = primary.key();
    let original = MeleeAttack::new(
        Skill::HeavyWeaponry,
        -1,
        60,
        40,
        DamageSpec::new(vec![primary], Some(key)).unwrap(),
    )
    .unwrap();
    let encoded = serde_json::to_value(MeleeAttackRecord::capture(&original)).unwrap();
    assert_eq!(
        encoded["damage"]["components"][0]["amount"],
        serde_json::json!({"type":"fixed","value":2})
    );
    let restored: MeleeAttackRecord = serde_json::from_value(encoded).unwrap();
    let restored = restored.restore().unwrap();
    assert_eq!(restored, original);
    let mut rng = 42;
    assert_eq!(
        restored
            .damage_with_modifiers(9, -4, 3)
            .unwrap()
            .resolve(&mut rng, Edge::from_counts(2, 0), &Protection::default())
            .total,
        1
    );
    assert_eq!(rng, 42);
}

#[test]
fn nested_record_fields_and_non_nested_descriptor_groups_are_rejected() {
    use tor_simulation::attacks::MeleeAttackRecord;
    let original = MeleeAttack::new(Skill::LightWeaponry, 0, 80, 50, rolled()).unwrap();
    let encoded = serde_json::to_value(MeleeAttackRecord::capture(&original)).unwrap();
    for amount in [false, true] {
        let mut invalid = encoded.clone();
        let component = &mut invalid["damage"]["components"][0];
        let target = if amount {
            &mut component["amount"]
        } else {
            component
        };
        target
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), serde_json::json!(1));
        assert!(serde_json::from_value::<MeleeAttackRecord>(invalid).is_err());
    }
    let mut invalid = encoded.clone();
    // Fire would now belong to both Keen and Energy; a once-per-group reduction
    // cannot be unambiguously nested within both categories.
    invalid["damage"]["components"][0]["descriptor"] = serde_json::json!("fire");
    invalid["damage"]["primary"]["descriptor"] = serde_json::json!("fire");
    let record: MeleeAttackRecord = serde_json::from_value(invalid).unwrap();
    assert!(record.restore().is_err());
    let mut missing_key = encoded;
    missing_key["damage"]["primary"]["sides"] = serde_json::json!(8);
    let record: MeleeAttackRecord = serde_json::from_value(missing_key).unwrap();
    assert!(record.restore().is_err());
}

#[test]
fn attack_values_deserialize_through_validation_and_preserve_stack_key_equivalence() {
    let source = serde_json::json!({
        "skill":"light_weaponry", "bonus":2, "wind_up":90, "recovery":70,
        "damage": {"primary":{"category":"energy","descriptor":"fire","sides":6},
            "components":[
                {"category":"energy","descriptor":"fire","amount":{"type":"rolled","count":1,"sides":6,"bonus":-1}},
                {"category":"energy","descriptor":"fire","amount":{"type":"rolled","count":2,"sides":6,"bonus":2}},
                {"category":"keen","amount":{"type":"fixed","value":3}}
            ]}
    });
    let attack: MeleeAttack = serde_json::from_value(source.clone()).unwrap();
    let encoded = serde_json::to_value(&attack).unwrap();
    assert_eq!(encoded["damage"]["components"].as_array().unwrap().len(), 2);
    let restored: MeleeAttack = serde_json::from_value(encoded).unwrap();
    assert_eq!(restored, attack);
    let mut reordered = source.clone();
    reordered["damage"]["components"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let reordered: MeleeAttack = serde_json::from_value(reordered).unwrap();
    let keys = std::collections::BTreeSet::from([attack, restored, reordered]);
    assert_eq!(
        keys.len(),
        1,
        "canonical equivalents must share one item stack key"
    );
    for case in 0..6 {
        let mut invalid = source.clone();
        match case {
            0 => invalid["skill"] = serde_json::json!("spellcasting"),
            1 => invalid["wind_up"] = serde_json::json!(0),
            2 => invalid["bonus"] = serde_json::json!(1001),
            3 => invalid["damage"]["primary"] = serde_json::Value::Null,
            4 => invalid["damage"]["components"][0]["amount"]["count"] = serde_json::json!(65),
            5 => invalid["damage"]["components"][0]["amount"]["unexpected"] = serde_json::json!(1),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<MeleeAttack>(invalid).is_err(),
            "accepted invalid attack {case}"
        );
    }
}

#[test]
fn single_fixed_attack_factory_uses_the_declared_primary_and_validates_bounds() {
    let attack = MeleeAttack::fixed(
        Skill::LightWeaponry,
        2,
        90,
        70,
        DamageType::Energy,
        Some(Descriptor::Fire),
        3,
    )
    .unwrap();
    assert_eq!(
        attack.damage().primary().unwrap().category,
        DamageType::Energy
    );
    assert_eq!(
        attack.damage().primary().unwrap().descriptor,
        Some(Descriptor::Fire)
    );
    let mut rng = 42;
    assert_eq!(
        attack
            .damage()
            .resolve(&mut rng, Edge::from_counts(3, 0), &Protection::default())
            .total,
        3
    );
    assert_eq!(rng, 42);
    assert!(MeleeAttack::fixed(
        Skill::LightWeaponry,
        2,
        90,
        70,
        DamageType::Energy,
        None,
        1_000_001
    )
    .is_err());
    assert!(
        MeleeAttack::fixed(Skill::Spellcasting, 2, 90, 70, DamageType::Energy, None, 3).is_err()
    );
}
