use super::*;
use crate::attributes::{ManaBinding, Skill};
use crate::creatures::{Content, CreatureBuild, ScenarioCatalog};
use crate::expertise::ExpertiseDef;
use crate::progression::{HdLedger, HdSource};
use crate::rules::{Requirement, ThreatOverride, ThreatSource};
use crate::{HdCount, HdIndex};
use serde_json::json;
use std::collections::BTreeMap;

fn id() -> ExpertiseId {
    ExpertiseId::new("fixture.weapon").unwrap()
}
fn rules() -> std::sync::Arc<Ruleset> {
    let mut doc = crate::test_creatures::ruleset().document().clone();
    doc.expertises = vec![
        ExpertiseDef {
            id: id(),
            name: "Weapon".into(),
            category: ExpertiseCategory::Equipment,
        },
        ExpertiseDef {
            id: ExpertiseId::new("fixture.utility").unwrap(),
            name: "Utility".into(),
            category: ExpertiseCategory::Utility,
        },
    ];
    let mut talent = crate::test_creatures::talent("fixture_weapon_training", vec![]);
    talent.expertise_grants = vec![id()];
    talent.requirement = Requirement::TotalHd {
        minimum: HdCount(4),
    };
    doc.growth.threat.overrides.push(ThreatOverride {
        source: ThreatSource::Talent {
            id: talent.id.clone(),
        },
        points: 0,
    });
    doc.talents.push(talent);
    Ruleset::validate(doc).unwrap()
}
fn build(active: bool) -> CreatureBuild {
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let content = Content::new(
        rules(),
        ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut build = CreatureBuild::new(
        content,
        species_id,
        HdLedger::seeded(vec![HdSource::Racial; 4], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    let talent = crate::content_ids::TalentId::new("fixture_weapon_training").unwrap();
    build.select_talent(HdIndex(0), talent.clone()).unwrap();
    if !active {
        build.remove_latest().unwrap();
    }
    build
}
fn ownership(active: bool) -> ExpertiseOwnership {
    build(active).derive().unwrap().expertises
}
fn spec() -> WeaponProfileSpec {
    WeaponProfileSpec {
        expertise: id(),
        ranged: None,
        base_traits: vec![],
        expert_traits: WeaponTraitChanges {
            add: vec![],
            remove: vec![],
        },
    }
}
fn range() -> RangeBands {
    RangeBands {
        short_feet: 20,
        long_feet: 60,
    }
}

#[test]
fn references_require_known_equipment_expertise() {
    assert!(spec().validate(&rules()).is_ok());
    for value in ["fixture.missing", "fixture.utility"] {
        let mut s = spec();
        s.expertise = ExpertiseId::new(value).unwrap();
        assert!(s.validate(&rules()).is_err());
    }
}
#[test]
fn only_active_expertise_applies_typed_trait_changes() {
    let mut s = spec();
    s.base_traits = vec![WeaponTrait::TwoHanded];
    s.expert_traits.remove = vec![WeaponTraitKind::TwoHanded];
    s.expert_traits.add = vec![
        WeaponTrait::Offhand,
        WeaponTrait::Quickdraw,
        WeaponTrait::Thrown { range: range() },
    ];
    let profile = s.validate(&rules()).unwrap();
    for owner in [ExpertiseOwnership::default(), ownership(false)] {
        let effective = profile.resolve(&owner);
        assert!(effective.two_handed());
        assert_eq!(effective.grip_demand(), 2);
        assert!(!effective.offhand());
        assert!(!effective.quickdraw());
        assert_eq!(effective.thrown(), None);
    }
    let dormant = ownership(false);
    assert!(dormant.owns(&id()));
    assert!(!dormant.active(&id()));
    let effective = profile.resolve(&ownership(true));
    assert!(!effective.two_handed());
    assert_eq!(effective.grip_demand(), 1);
    assert!(effective.offhand());
    assert!(effective.quickdraw());
    assert_eq!(effective.thrown(), Some(range()));
    assert_eq!(profile.minimum_grip_demand(), 1);
}
#[test]
fn minimum_grip_demand_uses_both_actual_states() {
    let mut s = spec();
    s.base_traits = vec![WeaponTrait::TwoHanded];
    assert_eq!(s.validate(&rules()).unwrap().minimum_grip_demand(), 2);
    s.base_traits.clear();
    s.expert_traits.add = vec![WeaponTrait::TwoHanded];
    let profile = s.validate(&rules()).unwrap();
    assert_eq!(profile.minimum_grip_demand(), 1);
    assert_eq!(profile.resolve(&ownership(true)).grip_demand(), 2);
}
#[test]
fn explicit_removal_allows_same_kind_range_replacement() {
    let mut s = spec();
    s.ranged = Some(range());
    s.base_traits = vec![WeaponTrait::Thrown { range: range() }];
    let improved = RangeBands {
        short_feet: 30,
        long_feet: 90,
    };
    s.expert_traits.add = vec![WeaponTrait::Thrown { range: improved }];
    assert!(s.validate(&rules()).is_err());
    s.expert_traits.remove = vec![WeaponTraitKind::Thrown];
    let profile = s.validate(&rules()).unwrap();
    assert_eq!(profile.resolve(&ownership(true)).thrown(), Some(improved));
    assert_eq!(
        profile.resolve(&ExpertiseOwnership::default()).thrown(),
        Some(range())
    );
    assert_eq!(profile.resolve(&ownership(true)).ranged(), Some(range()));
}
#[test]
fn physical_range_classification_includes_exact_boundaries() {
    for (feet, expected) in [
        (0, RangeClassification::Normal),
        (20, RangeClassification::Normal),
        (21, RangeClassification::Disadvantage),
        (60, RangeClassification::Disadvantage),
        (61, RangeClassification::Unavailable),
        (u32::MAX, RangeClassification::Unavailable),
    ] {
        assert_eq!(range().classify(feet), expected);
    }
    assert_eq!(
        RangeBands {
            short_feet: 5,
            long_feet: 5
        }
        .classify(6),
        RangeClassification::Unavailable
    );
}
#[test]
fn invalid_ranges_and_deltas_are_rejected_in_memory_and_on_decode() {
    for (short_feet, long_feet) in [(0, 20), (5, 0), (6, 20), (5, 21), (20, 10), (5, 1_000_005)] {
        let mut s = spec();
        s.ranged = Some(RangeBands {
            short_feet,
            long_feet,
        });
        assert!(s.validate(&rules()).is_err());
        assert!(
            serde_json::from_value::<WeaponProfileSpec>(serde_json::to_value(&s).unwrap()).is_err()
        );
        s.ranged = None;
        s.base_traits = vec![WeaponTrait::Thrown {
            range: RangeBands {
                short_feet,
                long_feet,
            },
        }];
        assert!(s.validate(&rules()).is_err());
    }
    assert!(RangeBands {
        short_feet: 5,
        long_feet: 1_000_000
    }
    .validate()
    .is_ok());
    let mut bad = vec![];
    let mut s = spec();
    s.base_traits = vec![WeaponTrait::Offhand; 2];
    bad.push(s);
    let mut s = spec();
    s.expert_traits.add = vec![WeaponTrait::Offhand; 2];
    bad.push(s);
    let mut s = spec();
    s.expert_traits.remove = vec![WeaponTraitKind::Offhand];
    bad.push(s);
    let mut s = spec();
    s.base_traits = vec![WeaponTrait::Offhand];
    s.expert_traits.remove = vec![WeaponTraitKind::Offhand; 2];
    bad.push(s);
    let mut s = spec();
    s.base_traits = vec![WeaponTrait::Offhand];
    s.expert_traits.add = vec![WeaponTrait::Offhand];
    bad.push(s);
    for s in bad {
        assert!(s.validate(&rules()).is_err());
        assert!(
            serde_json::from_value::<WeaponProfileSpec>(serde_json::to_value(&s).unwrap()).is_err()
        );
    }
}
#[test]
fn named_records_required_nullable_and_bounded_supported_traits_are_strict() {
    let canonical = serde_json::to_value(spec()).unwrap();
    assert_eq!(
        serde_json::from_value::<WeaponProfileSpec>(canonical.clone()).unwrap(),
        spec()
    );
    let mut invalid = vec![json!(["fixture.weapon", null, [], {"add":[],"remove":[]}])];
    for key in ["expertise", "ranged", "base_traits", "expert_traits"] {
        let mut value = canonical.clone();
        value.as_object_mut().unwrap().remove(key);
        invalid.push(value);
    }
    for (key, value) in [
        ("unknown", json!(true)),
        ("expert_traits", json!([[], []])),
        ("ranged", json!([20, 60])),
        ("ranged", json!({"short_feet":20,"long_feet":60,"extra":0})),
        ("expert_traits", json!({"add":[],"remove":[],"extra":0})),
        ("base_traits", json!([{"kind":"discreet"}])),
        ("base_traits", json!([{"kind":"opportunity"}])),
        ("base_traits", json!([{"kind":"complication"}])),
        ("base_traits", json!([{"kind":"quickdraw","extra":true}])),
        ("base_traits", json!(["quickdraw"])),
        ("base_traits", json!([{"kind":"thrown","range":[20,60]}])),
        ("base_traits", json!(vec![json!({"kind":"quickdraw"}); 5])),
        (
            "expert_traits",
            json!({"add":vec![json!({"kind":"offhand"});5],"remove":[]}),
        ),
        (
            "expert_traits",
            json!({"add":[],"remove":vec!["offhand";5]}),
        ),
    ] {
        let mut v = canonical.clone();
        v[key] = value;
        invalid.push(v);
    }
    for value in invalid {
        assert!(
            serde_json::from_value::<WeaponProfileSpec>(value.clone()).is_err(),
            "accepted {value}"
        );
    }
}

#[test]
fn four_trait_round_trip_and_compiled_state_are_immutable() {
    let mut source = spec();
    source.base_traits = vec![
        WeaponTrait::TwoHanded,
        WeaponTrait::Offhand,
        WeaponTrait::Quickdraw,
        WeaponTrait::Thrown { range: range() },
    ];
    let encoded = serde_json::to_value(&source).unwrap();
    assert_eq!(
        serde_json::from_value::<WeaponProfileSpec>(encoded).unwrap(),
        source
    );
    let compiled = source.validate(&rules()).unwrap();
    assert_eq!(compiled.spec(), &source);
    source.base_traits.clear();
    let effective = compiled.resolve(&ExpertiseOwnership::default());
    assert_eq!(effective.traits().len(), 4);
    assert!(effective.two_handed() && effective.offhand() && effective.quickdraw());
    assert_eq!(effective.thrown(), Some(range()));
    assert_eq!(compiled.minimum_grip_demand(), 2);
}

#[test]
fn in_memory_limits_and_nested_required_fields_are_checked() {
    let mut source = spec();
    source.base_traits = vec![WeaponTrait::Quickdraw; 5];
    assert_eq!(
        source.validate(&rules()),
        Err(WeaponProfileError::TooManyTraits)
    );
    source.base_traits.clear();
    source.expert_traits.add = vec![WeaponTrait::Quickdraw; 5];
    assert_eq!(
        source.validate(&rules()),
        Err(WeaponProfileError::TooManyTraits)
    );
    source.expert_traits.add.clear();
    source.expert_traits.remove = vec![WeaponTraitKind::Quickdraw; 5];
    assert_eq!(
        source.validate(&rules()),
        Err(WeaponProfileError::TooManyTraits)
    );
    for changes in [
        json!({"add":[]}),
        json!({"remove":[]}),
        json!({"add":[], "remove":["discreet"]}),
    ] {
        let mut value = serde_json::to_value(spec()).unwrap();
        value["expert_traits"] = changes;
        assert!(serde_json::from_value::<WeaponProfileSpec>(value).is_err());
    }
    for value in [
        json!({"kind":"thrown"}),
        json!({"kind":"thrown", "range":{"short_feet":20}}),
        json!({"kind":"thrown", "range":{"short_feet":20, "long_feet":60},"extra":0}),
    ] {
        assert!(serde_json::from_value::<WeaponTrait>(value).is_err());
    }
    assert!(serde_json::from_str::<RangeBands>(
        r#"{"short_feet":20,"short_feet":25,"long_feet":60}"#
    )
    .is_err());
}

#[test]
fn compiled_equipped_profiles_follow_dormancy_without_losing_gear_or_history() {
    use crate::creatures::{CreatureState, EquipmentSource};
    use crate::{EquipmentSlotId, ItemId};
    let mut authored = spec();
    authored.base_traits = vec![WeaponTrait::TwoHanded];
    authored.expert_traits.remove = vec![WeaponTraitKind::TwoHanded];
    let profile = authored.validate(&rules()).unwrap();
    let original = build(true);
    let mut creature = CreatureState::new(original.clone()).unwrap();
    let sources = vec![
        EquipmentSource {
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            modifiers: vec![],
            attack: Some(crate::test_creatures::melee(
                3,
                40,
                crate::combat::DamageType::Keen,
                4,
            )),
            weapon_profile: Some(profile),
        },
        EquipmentSource {
            item: ItemId(2),
            socket: EquipmentSlotId(10),
            modifiers: vec![],
            attack: Some(crate::test_creatures::melee(
                7,
                80,
                crate::combat::DamageType::Impact,
                6,
            )),
            weapon_profile: None,
        },
    ];
    let mut unrelated = CreatureState::new(crate::test_creatures::build(
        crate::test_creatures::species(),
    ))
    .unwrap();
    let before = unrelated.clone();
    assert!(unrelated.set_equipment(sources.clone()).is_err());
    assert_eq!(unrelated, before);
    creature.set_equipment(sources.clone()).unwrap();
    let sheet = creature.derived();
    assert_eq!(sheet.equipped_attacks.len(), 2);
    assert_eq!(
        sheet.selected_attack.source,
        crate::creatures::SourceId::Equipment(ItemId(1))
    );
    assert!(sheet.equipment_capacity_deficits.is_empty());
    assert_eq!(
        sheet.equipped_attacks[&EquipmentSlotId(10)]
            .attack
            .attack
            .bonus(),
        7
    );
    assert!(!sheet.equipped_attacks[&EquipmentSlotId(0)]
        .profile
        .as_ref()
        .unwrap()
        .two_handed());
    let secondary_before = sheet.equipped_attacks[&EquipmentSlotId(10)].attack.clone();
    let mut stronger = original.clone();
    stronger
        .increase_attribute(HdIndex(3), secondary_before.attribute)
        .unwrap();
    creature.rebuild(stronger).unwrap();
    let secondary_after = &creature.derived().equipped_attacks[&EquipmentSlotId(10)].attack;
    assert_eq!(
        secondary_after.attribute_value,
        secondary_before.attribute_value + 1
    );
    assert_eq!(
        secondary_after.check_bonus,
        secondary_before.check_bonus + 1
    );
    assert_eq!(secondary_after.rank, secondary_before.rank);
    assert_eq!(
        creature.derived().selected_attack.source,
        crate::creatures::SourceId::Equipment(ItemId(1))
    );
    let mut drained = original.clone();
    drained.remove_latest().unwrap();
    creature.rebuild(drained).unwrap();
    let sheet = creature.derived();
    assert!(sheet.equipped_attacks[&EquipmentSlotId(0)]
        .profile
        .as_ref()
        .unwrap()
        .two_handed());
    assert_eq!(
        sheet.equipment_capacity_deficits
            [&crate::content_ids::EquipmentCapacityId::new("grip").unwrap()]
            .missing(),
        1
    );
    assert_eq!(sheet.equipped_attacks.len(), 2);
    creature.rebuild(original).unwrap();
    assert!(creature.derived().equipment_capacity_deficits.is_empty());
    assert_eq!(
        creature.derived().equipped_attacks[&EquipmentSlotId(10)]
            .attack
            .attack
            .bonus(),
        7
    );
}

#[test]
fn two_handed_states_require_an_authored_shared_capacity() {
    use crate::creatures::{CreatureState, EquipmentSource};
    use crate::equipment_capacity::EquipmentCapacityUse;
    use crate::{EquipmentSlotId, ItemId};
    let shared = rules();
    let mut document = shared.document().clone();
    document
        .equipment_slots
        .iter_mut()
        .find(|slot| slot.id.as_str() == "weapon")
        .unwrap()
        .capacity = EquipmentCapacityUse::None;
    let no_capacity = Ruleset::validate(document).unwrap();
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let content = Content::new(
        no_capacity.clone(),
        ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let build = CreatureBuild::new(
        content,
        species_id,
        HdLedger::seeded(vec![HdSource::Racial; 4], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    for (base, expert) in [
        (vec![], vec![]),
        (vec![WeaponTrait::TwoHanded], vec![]),
        (vec![], vec![WeaponTrait::TwoHanded]),
    ] {
        let needs_capacity = !base.is_empty() || !expert.is_empty();
        let mut profile = spec();
        profile.base_traits = base;
        profile.expert_traits.add = expert;
        let attack = crate::test_creatures::melee(0, 40, crate::combat::DamageType::Keen, 4);
        let equipment = crate::EquipmentSpec {
            slot: crate::EquipmentSlot::new("weapon").unwrap(),
            armor: None,
            attack: Some(attack.clone()),
            weapon_profile: Some(profile.clone()),
            modifiers: vec![],
        };
        assert!(equipment
            .validate_rules(crate::ItemClass::Weapon, &shared)
            .is_ok());
        assert_eq!(
            equipment
                .validate_rules(crate::ItemClass::Weapon, &no_capacity)
                .is_err(),
            needs_capacity
        );
        let mut creature = CreatureState::new(build.clone()).unwrap();
        let before = creature.clone();
        let result = creature.set_equipment(vec![EquipmentSource {
            item: ItemId(1),
            socket: EquipmentSlotId(0),
            modifiers: vec![],
            attack: Some(attack),
            weapon_profile: Some(profile.validate(&no_capacity).unwrap()),
        }]);
        assert_eq!(result.is_err(), needs_capacity);
        if needs_capacity {
            assert_eq!(creature, before);
        }
    }
}
