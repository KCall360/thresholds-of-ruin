use super::*;
use crate::attributes::Skill;
use crate::combat::DamageType;
use crate::damage::{DamageComponent, DamageSpec};
use crate::rules::{DamageExpr, Effect, StrikeDef};
use serde_json::json;

fn empty(offset: u64) -> StrikeDef {
    StrikeDef {
        offset,
        unlock: None,
        check: None,
        on_success: vec![],
        on_failure: vec![],
        descriptors: vec![],
        riders: vec![],
    }
}
fn natural(strikes: Vec<StrikeDef>) -> Result<NaturalAttack, AttackError> {
    NaturalAttack::new(Skill::HeavyWeaponry, 2, 10, 20, strikes)
}
#[test]
fn ordered_strikes_natural_conversion_preserves_primary_after_secondary_keys_and_modifiers() {
    let secondary = DamageComponent::rolled(
        DamageType::Energy,
        None,
        crate::dice::DicePool::new(2, 6, 1).unwrap(),
    );
    let primary = DamageComponent::rolled(
        DamageType::Vital,
        None,
        crate::dice::DicePool::new(1, 4, 2).unwrap(),
    );
    let damage = DamageSpec::new(vec![primary.clone(), secondary], Some(primary.key())).unwrap();
    assert!(damage
        .components()
        .iter()
        .any(|component| component.key() < primary.key()));
    let weapon = MeleeAttack::new(Skill::HeavyWeaponry, 7, 60, 40, damage.clone()).unwrap();
    let converted = NaturalAttack::from_melee(weapon.clone()).unwrap();
    assert_eq!(converted.first_primary_damage().unwrap(), damage);
    let Effect::Damage {
        damage: DamageExpr::Components { components },
    } = &converted.strikes()[0].on_success[0]
    else {
        panic!("explicit damage source");
    };
    assert_eq!(components[0].category, DamageType::Vital);
    assert_eq!(
        SelectedMeleeAttack::Natural(converted.clone())
            .damage_with_modifiers(2, -1, 3)
            .unwrap(),
        weapon.damage_with_modifiers(2, -1, 3).unwrap()
    );
    assert_eq!(converted.bonus(), 7);
    assert_eq!(converted.wind_up(), 60);
    assert_eq!(converted.recovery(), 40);
    let restored: NaturalAttack =
        serde_json::from_value(serde_json::to_value(&converted).unwrap()).unwrap();
    assert_eq!(restored, converted);
    assert_eq!(
        restored.first_primary_damage().unwrap().primary(),
        Some(primary.key())
    );
}
#[test]
fn ordered_strikes_checkless_no_damage_natural_is_valid_and_has_no_fake_primary() {
    let attack = natural(vec![empty(0)]).unwrap();
    assert_eq!(
        attack.first_primary_damage(),
        Err(SelectedDamageError::NoPrimaryDamage)
    );
    let selected = SelectedMeleeAttack::Natural(attack.clone());
    assert_eq!(
        selected.damage_with_modifiers(0, 0, 0),
        Err(SelectedDamageError::NoPrimaryDamage)
    );
    let restored: NaturalAttack =
        serde_json::from_value(serde_json::to_value(&attack).unwrap()).unwrap();
    assert_eq!(restored, attack);
}
#[test]
fn ordered_strikes_natural_rejects_bad_order_unlocks_and_oversized_sequences() {
    assert!(natural(vec![empty(0); 16]).is_ok());
    assert!(natural(vec![empty(0); 17]).is_err());
    assert!(natural(vec![]).is_err());
    assert!(natural(vec![empty(1)]).is_err());
    assert!(natural(vec![empty(0), empty(5), empty(4)]).is_err());
    let mut locked = empty(0);
    locked.unlock = Some(crate::content_ids::TalentId::new("rattling_blow").unwrap());
    assert!(natural(vec![locked]).is_err());
    assert!(natural(vec![empty(0), empty(u64::MAX)]).is_err());
}
#[test]
fn ordered_strikes_natural_source_record_rejects_legacy_cache_and_missing_payload_lists() {
    let original = serde_json::to_value(natural(vec![empty(0)]).unwrap()).unwrap();
    for field in ["damage", "derived", "next_strike"] {
        let mut forged = original.clone();
        forged[field] = json!({});
        assert!(
            serde_json::from_value::<NaturalAttack>(forged).is_err(),
            "{field}"
        );
    }
    for field in ["on_success", "on_failure", "descriptors", "riders"] {
        let mut forged = original.clone();
        forged["strikes"][0].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<NaturalAttack>(forged).is_err(),
            "missing {field}"
        );
    }
    let positional = original
        .as_object()
        .unwrap()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    assert!(serde_json::from_value::<NaturalAttack>(json!(positional)).is_err());
}
#[test]
fn ordered_strikes_natural_catalog_validates_dormant_unlocks_and_all_rider_references() {
    let rules = crate::test_creatures::ruleset();
    let mut second = empty(10);
    second.unlock = Some(crate::content_ids::TalentId::new("rattling_blow").unwrap());
    let mut species = crate::test_creatures::unmodified_species("sequence_subject");
    species.melee = natural(vec![empty(0), second.clone()]).unwrap();
    let catalog = |species: crate::creatures::Species| crate::creatures::ScenarioCatalog {
        species: std::collections::BTreeMap::from([(species.id.clone(), species)]),
        templates: Default::default(),
    };
    assert!(crate::creatures::Content::new(rules.clone(), catalog(species.clone())).is_ok());
    second.unlock = Some(crate::content_ids::TalentId::new("absent_unlock").unwrap());
    species.melee = natural(vec![empty(0), second]).unwrap();
    assert!(crate::creatures::Content::new(rules.clone(), catalog(species.clone())).is_err());
    let mut first = empty(0);
    first.riders.push(crate::rules::RiderDef {
        condition: None,
        trigger: crate::rules::RiderTrigger::Success,
        check: None,
        effects: vec![Effect::ApplyCondition {
            condition: crate::content_ids::ConditionId::new("absent_condition").unwrap(),
            duration: 10,
        }],
        descriptors: vec![],
    });
    species.melee = natural(vec![first]).unwrap();
    assert!(crate::creatures::Content::new(rules, catalog(species)).is_err());
}
#[test]
fn ordered_strikes_unscaled_horizon_may_exceed_numeric_until_speed_scaling() {
    let attack = NaturalAttack::new(
        Skill::HeavyWeaponry,
        0,
        crate::limits::NUMERIC as u64,
        crate::limits::NUMERIC as u64,
        vec![empty(0), empty(crate::limits::NUMERIC as u64)],
    );
    assert!(attack.is_ok(), "each authored phase is independently bounded; final scaled horizon is checked by plan capture");
}

#[test]
fn ordered_strikes_natural_source_rejects_merged_damage_overflow_before_attachment() {
    let mut first = empty(0);
    let component = crate::rules::DamageComponentExpr {
        category: DamageType::Impact,
        descriptor: None,
        amount: crate::rules::AmountExpr::Fixed {
            amount: crate::limits::NUMERIC as u32,
        },
    };
    first.on_success = vec![Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![component.clone(), component],
        },
    }];
    assert!(natural(vec![first]).is_err());
}
