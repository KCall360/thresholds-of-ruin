use std::collections::BTreeSet;
use tor_simulation::attributes::{Attributes, ManaBinding, Skill};
use tor_simulation::combat::DamageType;
use tor_simulation::creatures::record::BuildRecord;
use tor_simulation::creatures::{CreatureBuild, Species, Subtype, Template};
use tor_simulation::dice::DicePool;
use tor_simulation::grants::{Ability, Descriptor, Grant, Selector};
use tor_simulation::progression::{Class, CreatureType, HdLedger, HdSource};
use tor_simulation::talents::Talent;
use tor_simulation::AnatomySpec;

fn build() -> CreatureBuild {
    let species = Species {
        id: "record_subject".into(),
        kind: CreatureType::Humanoid,
        subtypes: BTreeSet::from([Subtype::Goblinoid, Subtype::Fire]),
        default_attributes: Attributes::new([1; 6]).unwrap(),
        anatomy: AnatomySpec::humanoid(),
        melee: tor_simulation::attacks::MeleeAttack::new(Skill::HeavyWeaponry, 0, 60, 40, {
            let component = tor_simulation::damage::DamageComponent::rolled(
                tor_simulation::combat::DamageType::Impact,
                None,
                DicePool::new(1, 6, -1).unwrap(),
            );
            let primary = component.key();
            tor_simulation::damage::DamageSpec::new(vec![component], Some(primary)).unwrap()
        })
        .unwrap(),
        grants: vec![Grant::Immunity(Selector::Descriptor(Descriptor::Cold))],
    };
    let ledger = HdLedger::seeded(
        vec![
            HdSource::Class(Class::Warrior),
            HdSource::Class(Class::Mage),
            HdSource::Class(Class::Mage),
            HdSource::Class(Class::Mage),
            HdSource::Class(Class::Mage),
        ],
        42,
    )
    .unwrap();
    let mut build = CreatureBuild::new(species, ledger, ManaBinding::Presence).unwrap();
    build
        .set_initial_attributes(Attributes::new([2, 1, 3, 2, 1, 4]).unwrap())
        .unwrap();
    build.train(0, Skill::Intimidation).unwrap();
    build.train(1, Skill::Spellcasting).unwrap();
    build.select_talent(0, Talent::Fear).unwrap();
    build.select_talent(1, Talent::MagicBolt).unwrap();
    build.select_talent(2, Talent::FearMastery).unwrap();
    build.set_templates(vec![Template::zombified(0)]).unwrap();
    build
}

fn value() -> serde_json::Value {
    serde_json::to_value(BuildRecord::capture(&build())).unwrap()
}

fn rejected(value: serde_json::Value) -> bool {
    match serde_json::from_value::<BuildRecord>(value) {
        Err(_) => true,
        Ok(record) => record.restore().is_err(),
    }
}

#[test]
fn build_record_round_trip_preserves_seeds_choices_and_dormancy() {
    let original = build();
    let data = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
    let restored = serde_json::from_slice::<BuildRecord>(&data)
        .unwrap()
        .restore()
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.derive().unwrap(), original.derive().unwrap());
    assert_eq!(restored.unspent_talent_slots(), 2);
    assert_eq!(
        serde_json::to_vec(&BuildRecord::capture(&restored)).unwrap(),
        data
    );
}

#[test]
fn saved_records_contain_choices_without_derived_combat_caches() {
    let record = value();
    assert_eq!(
        record["initial_attributes"],
        serde_json::json!([2, 1, 3, 2, 1, 4])
    );
    assert_eq!(record["binding"], 3);
    assert_eq!(record["species"]["kind"], 7);
    assert!(record.get("maximum_health").is_none());
    assert!(record.get("defenses").is_none());
    assert!(record.get("active_talents").is_none());
    assert!(record.get("protection").is_none());
    assert_eq!(record["hd"].as_array().unwrap().len(), 5);
}

#[test]
fn unknown_fields_and_missing_nullable_choice_fields_reject() {
    let mut record = value();
    record["maximum_health"] = serde_json::json!(1_000_000);
    assert!(rejected(record));
    for field in ["talent", "attribute"] {
        let mut record = value();
        record["choices"][4].as_object_mut().unwrap().remove(field);
        assert!(rejected(record), "missing {field} silently defaulted");
    }
    let mut record = value();
    record["templates"][0]
        .as_object_mut()
        .unwrap()
        .remove("kind");
    assert!(rejected(record));
}

#[test]
fn unknown_catalog_ids_invalid_ranks_and_duplicate_set_members_reject() {
    let mut record = value();
    record["species"]["kind"] = serde_json::json!(255);
    assert!(rejected(record));
    for skill in ["not-a-skill", "spellcasting"] {
        let mut record = value();
        record["species"]["melee"]["skill"] = serde_json::json!(skill);
        assert!(rejected(record));
    }
    let mut record = value();
    record["binding"] = serde_json::json!(255);
    assert!(rejected(record));
    let mut record = value();
    record["choices"][0]["training"] = serde_json::json!([255]);
    assert!(rejected(record));
    let mut record = value();
    let duplicate = record["species"]["subtypes"][0].clone();
    record["species"]["subtypes"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(rejected(record));
    let mut record = value();
    record["initial_attributes"][0] = serde_json::json!(6);
    assert!(rejected(record));
}

#[test]
fn record_collection_bounds_and_owned_slot_counts_reject() {
    for (field, maximum) in [("hd", 256), ("choices", 256), ("templates", 32)] {
        let mut record = value();
        let entry = record[field][0].clone();
        record[field] = serde_json::Value::Array(vec![entry; maximum + 1]);
        assert!(serde_json::from_value::<BuildRecord>(record).is_err());
    }
    let mut record = value();
    record["choices"].as_array_mut().unwrap().pop();
    assert!(rejected(record));
    let mut record = value();
    record["choices"][0]["training"] = serde_json::json!([0, 0, 0]);
    assert!(serde_json::from_value::<BuildRecord>(record).is_err());
    let mut record = value();
    record["species"]["id"] = serde_json::json!("x".repeat(61));
    assert!(serde_json::from_value::<BuildRecord>(record).is_err());
}

#[test]
fn all_grant_variants_and_catalog_types_round_trip() {
    let grants = vec![
        Grant::Health(2),
        Grant::Stamina(2),
        Grant::Focus(2),
        Grant::Mana(2),
        Grant::PhysicalDefense(1),
        Grant::MeleeFlat(-1),
        Grant::MeleeDice(1),
        Grant::BoltFlat(1),
        Grant::BoltDice(1),
        Grant::FearDifficulty(1),
        Grant::FearDuration(10),
        Grant::Immunity(Selector::Category(DamageType::Vital)),
        Grant::Reduction(Selector::Descriptor(Descriptor::Cold), 2),
        Grant::Ability(Ability::PowerStrike),
        Grant::Mindless,
        Grant::Magical,
    ];
    for kind in CreatureType::ALL {
        let mut species = build().species().clone();
        species.kind = kind;
        species.subtypes = Subtype::ALL.into_iter().collect();
        species.grants = grants.clone();
        let original = CreatureBuild::new(
            species,
            HdLedger::seeded(vec![HdSource::Racial], 7).unwrap(),
            ManaBinding::Awareness,
        )
        .unwrap();
        let data = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
        let restored = serde_json::from_slice::<BuildRecord>(&data)
            .unwrap()
            .restore()
            .unwrap();
        assert_eq!(restored, original);
        assert_eq!(restored.derive().unwrap(), original.derive().unwrap());
    }
}

#[test]
fn empty_ledger_and_retained_templates_restore_without_revival() {
    let mut original = build();
    while original.remove_latest().is_some() {}
    let data = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
    let restored = serde_json::from_slice::<BuildRecord>(&data)
        .unwrap()
        .restore()
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.derive().unwrap().maximum_health, 0);
    assert!(restored.derive().unwrap().abilities.is_empty());
}

#[test]
fn nested_record_shapes_ids_and_definition_limits_reject() {
    let mut record = value();
    record["hd"][0]["source"]["unexpected"] = serde_json::json!(true);
    assert!(rejected(record));
    let mut record = value();
    record["species"]["grants"][0]["value"]["id"] = serde_json::json!(255);
    assert!(rejected(record));
    let mut record = value();
    record["species"]["melee"]["damage"]["components"][0]["amount"]["count"] =
        serde_json::json!(65);
    assert!(rejected(record));
    let mut record = value();
    record["templates"][0]["overrides"] = serde_json::json!([
        {"attribute": 2, "value": 0}, {"attribute": 2, "value": 1}
    ]);
    assert!(rejected(record));
    let mut record = value();
    record["species"]["grants"] = serde_json::json!([
        {"kind": "health", "value": 1_000_001}
    ]);
    assert!(rejected(record));
    let mut record = value();
    record["templates"][0]["adjustments"] = serde_json::json!([1001, 0, 0, 0, 0, 0]);
    assert!(rejected(record));
    let mut record = value();
    record["choices"][0]["attribute"] = serde_json::json!(0);
    assert!(rejected(record));
    let mut record = value();
    record["choices"][3]["talent"] = record["choices"][0]["talent"].clone();
    assert!(rejected(record));
}

#[test]
fn streaming_json_decoder_enforces_collection_and_identifier_limits() {
    let mut record = value();
    let die = record["hd"][0].clone();
    record["hd"] = serde_json::Value::Array(vec![die; 257]);
    let data = serde_json::to_vec(&record).unwrap();
    assert!(serde_json::from_slice::<BuildRecord>(&data).is_err());
    record = value();
    record["species"]["id"] = serde_json::json!("bad id");
    let data = serde_json::to_vec(&record).unwrap();
    assert!(serde_json::from_slice::<BuildRecord>(&data).is_err());
}
