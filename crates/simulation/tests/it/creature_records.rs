use super::creature_fixture;
use std::collections::BTreeMap;
use tor_simulation::attributes::{Attributes, ByAttribute, Skill};
use tor_simulation::content_ids::{TalentId, TemplateId};
use tor_simulation::creatures::record::{BuildRecord, CreatureRecord};
use tor_simulation::creatures::{CreatureBuild, CreatureState, Template};
use tor_simulation::progression::HdSource;
use tor_simulation::rules::{Modifier, ModifierOp, Scope, Stat};

fn build() -> CreatureBuild {
    let mut template = Template::new(TemplateId::new("armored").unwrap(), 0);
    template.modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Health,
            value: 5,
        },
    });
    let mut build = creature_fixture::build_with(
        creature_fixture::species(),
        vec![template],
        &[HdSource::Racial, HdSource::Racial],
    );
    build
        .set_initial_attributes(
            Attributes::new(ByAttribute {
                strength: 2,
                speed: 1,
                intellect: 3,
                willpower: 2,
                awareness: 1,
                presence: 4,
            })
            .unwrap(),
        )
        .unwrap();
    build = creature_fixture::with_talents(
        build,
        [creature_fixture::talent(
            "fixture_health",
            vec![creature_fixture::add(Stat::Health, 2)],
        )],
    );
    build
        .train(tor_simulation::HdIndex(1), Skill::Intimidation)
        .unwrap();
    build
        .select_talent(
            tor_simulation::HdIndex(0),
            TalentId::new("fixture_health").unwrap(),
        )
        .unwrap();
    build
        .set_templates(vec![TemplateId::new("armored").unwrap()])
        .unwrap();
    build
}
fn value() -> serde_json::Value {
    serde_json::to_value(BuildRecord::capture(&build())).unwrap()
}
fn rejected(value: serde_json::Value) -> bool {
    match serde_json::from_value::<BuildRecord>(value) {
        Err(_) => true,
        Ok(record) => record.restore(build().content().clone()).is_err(),
    }
}

#[test]
fn build_record_round_trip_preserves_seeds_choices_and_catalog_references() {
    let original = build();
    let bytes = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
    let restored = serde_json::from_slice::<BuildRecord>(&bytes)
        .unwrap()
        .restore(original.content().clone())
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.derive().unwrap(), original.derive().unwrap());
    assert!(std::sync::Arc::ptr_eq(
        original.content(),
        restored.content()
    ));
    assert_eq!(
        serde_json::to_vec(&BuildRecord::capture(&restored)).unwrap(),
        bytes
    );
}

#[test]
fn saved_actor_records_contain_named_choices_and_ids_without_definitions_or_caches() {
    let record = value();
    assert_eq!(record["initial_attributes"]["strength"], 2);
    assert_eq!(record["binding"], "intellect");
    assert_eq!(record["species"], "unit_subject");
    assert_eq!(record["templates"], serde_json::json!(["armored"]));
    assert_eq!(
        record["choices"][0]["purchases"][0]["selection"]["talent"],
        "fixture_health"
    );
    for field in [
        "maximum_health",
        "defenses",
        "active_talents",
        "protection",
        "catalog",
        "modifiers",
    ] {
        assert!(record.get(field).is_none(), "{field}");
    }
    assert_eq!(record["hd"].as_array().unwrap().len(), 2);
}

#[test]
fn unknown_fields_and_missing_choice_fields_reject() {
    let mut record = value();
    record["maximum_health"] = serde_json::json!(1_000_000);
    assert!(rejected(record));
    for field in ["purchases", "attribute"] {
        let mut record = value();
        record["choices"][1].as_object_mut().unwrap().remove(field);
        assert!(rejected(record), "missing {field} silently defaulted");
    }
}

#[test]
fn missing_catalog_references_reject_before_deriving_a_creature() {
    for (field, entry) in [
        ("species", serde_json::json!("missing")),
        ("templates", serde_json::json!(["missing"])),
    ] {
        let mut record = value();
        record[field] = entry;
        assert!(rejected(record));
    }
    let mut record = value();
    record["choices"][0]["purchases"][0]["selection"]["talent"] = serde_json::json!("missing");
    assert!(rejected(record));
    let mut record = value();
    record["hd"][0]["source"] = serde_json::json!({"kind":"class", "id":"missing"});
    assert!(rejected(record));
}

#[test]
fn positional_ids_legacy_definitions_and_invalid_mechanism_names_reject() {
    for (field, entry) in [
        ("binding", serde_json::json!(255)),
        ("species", serde_json::json!({"id":"unit_subject","kind":7})),
        ("initial_attributes", serde_json::json!([1, 1, 1, 1, 1, 1])),
    ] {
        let mut record = value();
        record[field] = entry;
        assert!(rejected(record));
    }
    let mut record = value();
    record["choices"][1]["purchases"][0]["selection"]["skill"] = serde_json::json!("not-a-skill");
    assert!(rejected(record));
    let mut record = value();
    record["initial_attributes"]["strength"] = serde_json::json!(6);
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
    let purchase = record["choices"][0]["purchases"][0].clone();
    record["choices"][0]["purchases"] = serde_json::Value::Array(vec![purchase; 257]);
    assert!(serde_json::from_value::<BuildRecord>(record).is_err());
    let mut record = value();
    record["choices"].as_array_mut().unwrap().pop();
    assert!(rejected(record));
    let mut record = value();
    record["choices"][0]["purchases"] = serde_json::json!([
        {"pool":"advancement", "selection":{"kind":"skill_rank", "skill":"athletics"}},
        {"pool":"advancement", "selection":{"kind":"skill_rank", "skill":"athletics"}}
    ]);
    assert!(rejected(record));
    let mut record = value();
    record["templates"] = serde_json::json!(["armored", "armored"]);
    assert!(rejected(record));
}

#[test]
fn empty_ledger_and_retained_templates_restore_without_revival() {
    let mut original = build();
    while original.remove_latest().is_some() {}
    let restored = BuildRecord::capture(&original)
        .restore(original.content().clone())
        .unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.derive().unwrap().maximum_health, 0);
    assert!(restored.derive().unwrap().abilities.is_empty());
}

#[test]
fn nested_record_shapes_and_duplicate_owned_talents_reject() {
    let mut record = value();
    record["hd"][0]["source"]["unexpected"] = serde_json::json!(true);
    assert!(rejected(record));
    let mut record = value();
    record["choices"][1]["purchases"] = record["choices"][0]["purchases"].clone();
    assert!(rejected(record));
    let mut record = value();
    record["choices"][0]["attribute"] = serde_json::json!(0);
    assert!(rejected(record));
}

#[test]
fn streaming_json_decoder_enforces_collection_and_identifier_limits() {
    let mut record = value();
    let die = record["hd"][0].clone();
    record["hd"] = serde_json::Value::Array(vec![die; 257]);
    assert!(serde_json::from_slice::<BuildRecord>(&serde_json::to_vec(&record).unwrap()).is_err());
    for id in ["bad id".to_owned(), "x".repeat(257)] {
        let mut record = value();
        record["species"] = serde_json::json!(id);
        assert!(
            serde_json::from_slice::<BuildRecord>(&serde_json::to_vec(&record).unwrap()).is_err()
        );
    }
}

#[test]
fn mutable_creature_records_use_named_pools_and_restore_injury_with_shared_content() {
    let mut creature = CreatureState::new(build()).unwrap();
    creature.damage(3);
    let record = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    assert!(record["pools"]["stamina"]["balance"].is_number());
    assert!(record.get("balances").is_none());
    let restored = serde_json::from_value::<CreatureRecord>(record)
        .unwrap()
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
}

#[test]
fn records_preserve_ruleset_point_grants_above_the_standard_default() {
    use tor_simulation::attributes::ManaBinding;
    use tor_simulation::creatures::{Content, ScenarioCatalog};
    use tor_simulation::progression::{HdLedger, HitDie};
    use tor_simulation::rules::Ruleset;
    let mut document = creature_fixture::ruleset().document().clone();
    document.constants.racial_point_grants[0].points = 3;
    let species = creature_fixture::species();
    let id = species.id.clone();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut original = CreatureBuild::new(
        content.clone(),
        id,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    for skill in [Skill::Athletics, Skill::Intimidation, Skill::Lore] {
        original.train(tor_simulation::HdIndex(0), skill).unwrap();
    }
    let bytes = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
    let restored = serde_json::from_slice::<BuildRecord>(&bytes)
        .unwrap()
        .restore(content)
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn named_engine_maps_reject_positional_arrays_and_unknown_fields() {
    use tor_simulation::resources::ByResource;
    assert!(serde_json::from_str::<ByAttribute<u16>>("[1,2,3,4,5,6]").is_err());
    assert!(serde_json::from_str::<ByResource<u32>>("[1,2,3]").is_err());
    assert!(serde_json::from_str::<ByResource<u32>>(
        r#"{"stamina":1,"focus":2,"mana":3,"other":4}"#
    )
    .is_err());
    assert_eq!(
        serde_json::from_str::<ByResource<u32>>(r#"{"stamina":1,"focus":2,"mana":3}"#).unwrap(),
        ByResource {
            stamina: 1,
            focus: 2,
            mana: 3
        }
    );
}

#[test]
fn public_restoration_accepts_only_structurally_valid_equipment_sources() {
    use tor_simulation::{
        creatures::{CreatureStateError, EquipmentSource},
        EquipmentSlotId, ItemId,
    };
    let original = CreatureState::new(build()).unwrap();
    for item in [ItemId(0), ItemId(u64::MAX)] {
        let equipment = vec![EquipmentSource {
            weapon_profile: None,
            item,
            socket: EquipmentSlotId(0),
            modifiers: vec![],
            attack: None,
        }];
        assert_eq!(
            CreatureState::from_recorded(
                original.build().clone(),
                original.build_epoch(),
                original.restorable_losses(),
                0,
                false,
                original.costs().clone(),
                original.conditions().clone(),
                equipment.clone()
            ),
            Err(CreatureStateError::Inconsistent)
        );
        assert_eq!(
            CreatureRecord::capture(&original)
                .restore(original.build().content().clone(), equipment),
            Err(CreatureStateError::Inconsistent)
        );
    }
}

#[test]
fn rules_attribute_cap_above_five_survives_build_and_game_restore() {
    use std::num::NonZeroU64;
    use tor_simulation::attributes::{Attribute, ManaBinding};
    use tor_simulation::creatures::{Content, ScenarioCatalog};
    use tor_simulation::progression::{HdLedger, HitDie};
    use tor_simulation::rules::Ruleset;
    use tor_simulation::{CreatureIdentity, Game};
    use tor_world::{Location, Position, RegionId};

    let six = Attributes::new(ByAttribute::from_fn(|_| 6)).unwrap();
    let mut species = creature_fixture::species();
    species.default_attributes = six;
    let catalog = ScenarioCatalog {
        species: BTreeMap::from([(species.id.clone(), species.clone())]),
        templates: BTreeMap::new(),
    };
    let baseline = creature_fixture::ruleset();
    assert!(Content::new(baseline.clone(), catalog.clone()).is_err());
    let mut document = baseline.document().clone();
    document.constants.attribute_cap = 6;
    let content = Content::new(Ruleset::validate(document).unwrap(), catalog).unwrap();
    let mut build = CreatureBuild::new(
        content.clone(),
        species.id,
        HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
        ManaBinding::Intellect,
        [
            tor_simulation::attributes::Skill::Deception,
            tor_simulation::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    build.set_initial_attributes(six).unwrap();
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        6
    );
    let record: BuildRecord =
        serde_json::from_value(serde_json::to_value(BuildRecord::capture(&build)).unwrap())
            .unwrap();
    assert_eq!(record.restore(content.clone()).unwrap(), build);

    let mut game = Game::two_room_in_stone(42, content.clone());
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "Six".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let saved = serde_json::to_vec(&(game.checkpoint(&mut shared), shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&saved).unwrap();
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, content).unwrap(),
        game
    );

    let mut capped = creature_fixture::build(creature_fixture::species());
    let before = capped.clone();
    assert!(capped.set_initial_attributes(six).is_err());
    assert_eq!(capped, before);
    let mut forged = serde_json::to_value(BuildRecord::capture(&capped)).unwrap();
    forged["initial_attributes"]["strength"] = serde_json::json!(6);
    assert!(serde_json::from_value::<BuildRecord>(forged)
        .unwrap()
        .restore(capped.content().clone())
        .is_err());
}

#[test]
fn mutable_creature_records_require_condition_generation_counter_even_when_empty() {
    let mut creature = CreatureState::new(build()).unwrap();
    for conditioned in [false, true] {
        if conditioned {
            creature
                .apply_condition(
                    &tor_simulation::content_ids::ConditionId::new("readied").unwrap(),
                    tor_simulation::ActorId(7),
                    40,
                )
                .unwrap();
        }
        let mut value = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
        assert_eq!(value["next_condition_generation"], u64::from(conditioned));
        value
            .as_object_mut()
            .unwrap()
            .remove("next_condition_generation");
        assert!(
            serde_json::from_value::<CreatureRecord>(value).is_err(),
            "conditioned={conditioned}"
        );
    }
}

#[test]
fn mutable_creature_records_reject_condition_generation_counter_reuse() {
    let mut creature = CreatureState::new(build()).unwrap();
    creature
        .apply_condition(
            &tor_simulation::content_ids::ConditionId::new("readied").unwrap(),
            tor_simulation::ActorId(7),
            40,
        )
        .unwrap();
    let mut value = serde_json::to_value(CreatureRecord::capture(&creature)).unwrap();
    value["conditions"][0]["generation"] = serde_json::json!(7);
    for counter in [0, 6, 7] {
        value["next_condition_generation"] = serde_json::json!(counter);
        let record: CreatureRecord = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            record.restore(creature.build().content().clone(), vec![]),
            Err(tor_simulation::creatures::CreatureStateError::Condition(
                tor_simulation::conditions::ConditionError::InvalidGeneration
            ))
        );
    }
    value["next_condition_generation"] = serde_json::json!(8);
    let record: CreatureRecord = serde_json::from_value(value).unwrap();
    let restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(
        restored.conditions().instances().next().unwrap().generation,
        7
    );
    assert_eq!(restored.conditions().next_generation(), 8);
}
