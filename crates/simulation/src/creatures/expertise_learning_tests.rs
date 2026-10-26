use super::*;
use crate::attributes::{Attribute, Attributes, ByAttribute};
use crate::content_ids::{ClassId, ExpertiseId, TalentId};
use crate::expertise::{ExpertiseCategory, ExpertiseDef};
use crate::rules::{Ruleset, Stat};

fn id(value: &str) -> ExpertiseId {
    ExpertiseId::new(value).unwrap()
}
fn fixture(count: usize) -> CreatureBuild {
    let mut species = crate::test_creatures::species();
    species.default_attributes = Attributes::new(ByAttribute {
        intellect: 1,
        ..ByAttribute::default()
    })
    .unwrap();
    let species_id = species.id.clone();
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.constants.attribute_opportunity_interval = 1;
    document.expertises = ["knife", "navigation", "bow"]
        .into_iter()
        .map(|name| ExpertiseDef {
            id: id(name),
            name: name.into(),
            category: ExpertiseCategory::Equipment,
        })
        .collect();
    document
        .classes
        .iter_mut()
        .find(|class| class.id.as_str() == "warrior")
        .unwrap()
        .modifiers
        .push(crate::test_creatures::add(
            Stat::Attribute {
                attribute: Attribute::Intellect,
            },
            1,
        ));
    let mut talent = crate::test_creatures::talent("fixture_knife_training", vec![]);
    talent.expertise_grants.push(id("knife"));
    document.talents.push(talent);
    document
        .growth
        .threat
        .overrides
        .push(crate::rules::ThreatOverride {
            source: crate::rules::ThreatSource::Expertise { id: id("knife") },
            points: 10,
        });
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: std::collections::BTreeMap::from([(species_id.clone(), species)]),
            templates: Default::default(),
        },
    )
    .unwrap();
    CreatureBuild::new(
        content,
        species_id,
        HdLedger::seeded(vec![HdSource::Racial; count], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
}

#[test]
fn birth_intellect_choice_is_entitled_but_never_spends_an_hd_point() {
    let mut build = fixture(2);
    assert_eq!(build.intellect_expertise_choices().count(), 1);
    let points = build.unspent_points();
    build.select_intellect_expertise(1, id("knife")).unwrap();
    assert_eq!(build.unspent_points(), points);
    assert!(build.derive().unwrap().expertises.active(&id("knife")));
    let original = build.clone();
    assert_eq!(
        build.select_intellect_expertise(1, id("bow")),
        Err(BuildError::OccupiedSlot)
    );
    assert_eq!(
        build.select_intellect_expertise(2, id("bow")),
        Err(BuildError::InvalidOwnership)
    );
    assert_eq!(build, original);
}

#[test]
fn drain_changed_regrowth_and_restore_keep_original_threshold_choice() {
    let mut build = fixture(2);
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    build
        .select_intellect_expertise(2, id("navigation"))
        .unwrap();
    build.remove_latest().unwrap();
    assert!(build.derive().unwrap().expertises.owns(&id("navigation")));
    assert!(!build.derive().unwrap().expertises.active(&id("navigation")));
    assert_eq!(
        build.select_expertise(crate::HdIndex(0), id("navigation")),
        Err(BuildError::DuplicateExpertise)
    );
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Speed)
        .unwrap();
    assert!(!build.derive().unwrap().expertises.active(&id("navigation")));
    build.remove_latest().unwrap();
    let record = record::BuildRecord::capture(&build);
    let mut build = record.restore(build.content().clone()).unwrap();
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    assert!(build.derive().unwrap().expertises.active(&id("navigation")));
    assert_eq!(build.intellect_expertise_choices().count(), 2);
    assert_eq!(
        build.intellect_expertise_choices().nth(1).unwrap().1,
        Some(&id("navigation"))
    );
}

#[test]
fn recombined_sources_earn_only_genuinely_new_capacity() {
    let mut build = fixture(1);
    let warrior = HdSource::Class(ClassId::new("warrior").unwrap());
    build.add_hit_die(warrior.clone(), 42).unwrap();
    build.select_intellect_expertise(2, id("bow")).unwrap();
    build.remove_latest().unwrap();
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    assert_eq!(build.intellect_expertise_choices().count(), 2);
    assert_eq!(
        build
            .intellect_learning
            .as_ref()
            .unwrap()
            .acquisitions
            .len(),
        1
    );
    build.add_hit_die(warrior, 42).unwrap();
    assert_eq!(build.intellect_expertise_choices().count(), 3);
    assert_eq!(
        build.intellect_expertise_choices().nth(1).unwrap().1,
        Some(&id("bow"))
    );
    assert_eq!(build.intellect_expertise_choices().nth(2).unwrap().1, None);
    let record = record::BuildRecord::capture(&build);
    assert_eq!(record.restore(build.content().clone()).unwrap(), build);
}

#[test]
fn independent_fixed_grant_survives_bonus_dormancy_and_attribute_damage_is_excluded() {
    let mut build = fixture(2);
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    build.select_intellect_expertise(2, id("knife")).unwrap();
    build
        .select_talent(
            crate::HdIndex(0),
            TalentId::new("fixture_knife_training").unwrap(),
        )
        .unwrap();
    build.remove_latest().unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.expertises.active(&id("knife")));
    assert_eq!(derived.expertises.grants()[&id("knife")].len(), 2);
    let mut creature = CreatureState::new(build).unwrap();
    creature.lose_attribute(Attribute::Intellect, 1).unwrap();
    assert!(creature.derived().expertises.active(&id("knife")));
}

#[test]
fn baseline_rewrites_after_learning_reject_atomically() {
    let mut build = fixture(1);
    build.select_intellect_expertise(1, id("knife")).unwrap();
    let original = build.clone();
    assert_eq!(
        build.set_initial_attributes(
            Attributes::new(ByAttribute {
                intellect: 4,
                ..ByAttribute::default()
            })
            .unwrap()
        ),
        Err(BuildError::InvalidOwnership)
    );
    assert_eq!(build, original);
}

#[test]
fn retained_learning_records_reject_forged_thresholds_sources_and_history() {
    let mut build = fixture(2);
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    build
        .select_intellect_expertise(2, id("navigation"))
        .unwrap();
    build.remove_latest().unwrap();
    let good = serde_json::to_value(record::BuildRecord::capture(&build)).unwrap();
    let decode = |value| {
        serde_json::from_value::<record::BuildRecord>(value).and_then(|record| {
            record
                .restore(build.content().clone())
                .map_err(|_| <serde_json::Error as serde::de::Error>::custom("invalid learning"))
        })
    };
    assert!(decode(good.clone()).is_ok());
    for field in ["birth", "history", "acquisitions", "thresholds"] {
        let mut bad = good.clone();
        bad["intellect_learning"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode(bad).is_err());
    }
    let mut bad = good.clone();
    bad["intellect_learning"]["thresholds"][1]["acquisition"] = serde_json::json!(0);
    assert!(decode(bad).is_err());
    let mut bad = good.clone();
    bad["intellect_learning"]["acquisitions"][0]["after"]["attribute_increases"]["intellect"] =
        serde_json::json!(7);
    assert!(decode(bad).is_err());
    let mut bad = good.clone();
    let row = bad["intellect_learning"]["history"][0].clone();
    bad["intellect_learning"]["history"]
        .as_array_mut()
        .unwrap()
        .push(row);
    assert!(decode(bad).is_err());
    let mut bad = good;
    bad["intellect_learning"]["birth"] = serde_json::json!([]);
    assert!(decode(bad).is_err());
}

#[test]
fn later_polymorph_changes_neither_bonus_capacity_nor_retained_choice() {
    let mut build = fixture(1);
    let mut catalog = build.content().catalog().clone();
    let template_id = crate::content_ids::TemplateId::new("later_intellect").unwrap();
    let mut template = Template::new(template_id.clone(), 1);
    template.adjustments.intellect = 5;
    catalog.templates.insert(template_id.clone(), template);
    build.content = Content::new(build.content().rules().clone(), catalog).unwrap();
    build.select_intellect_expertise(1, id("knife")).unwrap();
    build.set_templates(vec![template_id]).unwrap();
    assert!(build.derive().unwrap().attributes.get(Attribute::Intellect) > 1);
    assert_eq!(build.derive().unwrap().intellect_capacity, 1);
    assert_eq!(build.intellect_expertise_choices().count(), 1);
    assert!(build.derive().unwrap().expertises.active(&id("knife")));
    let record = record::BuildRecord::capture(&build);
    assert_eq!(record.restore(build.content().clone()).unwrap(), build);
}

#[test]
fn ordinary_purchase_keeps_shared_learning_when_capacity_does_not_grow() {
    let mut build = fixture(2);
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    let learning = build.intellect_learning.as_ref().unwrap().clone();
    build.train(crate::HdIndex(0), Skill::Deception).unwrap();
    assert!(Arc::ptr_eq(
        &learning,
        build.intellect_learning.as_ref().unwrap()
    ));
}

#[test]
fn intellect_choice_rebuild_invalidates_the_compiled_sheet() {
    let mut creature = CreatureState::new(fixture(1)).unwrap();
    let epoch = creature.build_epoch();
    let before = crate::earned_growth::assess(creature.build(), creature.derived())
        .unwrap()
        .total;
    let mut build = creature.build().clone();
    build.select_intellect_expertise(1, id("knife")).unwrap();
    creature.rebuild(build).unwrap();
    assert!(creature.derived().expertises.active(&id("knife")));
    assert_ne!(creature.build_epoch(), epoch);
    assert_eq!(
        crate::earned_growth::assess(creature.build(), creature.derived())
            .unwrap()
            .total,
        before + 10
    );
}

#[test]
fn restored_history_cannot_omit_current_earned_capacity() {
    let mut build = fixture(2);
    let birth = serde_json::to_value(record::BuildRecord::capture(&build)).unwrap()
        ["intellect_learning"]
        .clone();
    build
        .increase_attribute(crate::HdIndex(1), Attribute::Intellect)
        .unwrap();
    let mut bad = serde_json::to_value(record::BuildRecord::capture(&build)).unwrap();
    bad["intellect_learning"] = birth;
    let record: record::BuildRecord = serde_json::from_value(bad).unwrap();
    assert!(record.restore(build.content().clone()).is_err());
}

#[test]
fn learned_baseline_allows_identical_values_but_rejects_unrelated_attribute_rewrite() {
    let mut build = fixture(1);
    build.select_intellect_expertise(1, id("knife")).unwrap();
    let attributes = build.initial_attributes();
    build.set_initial_attributes(attributes).unwrap();
    let original = build.clone();
    let mut values = attributes.values();
    values.strength += 1;
    assert_eq!(
        build.set_initial_attributes(Attributes::new(values).unwrap()),
        Err(BuildError::InvalidOwnership)
    );
    assert_eq!(build, original);
}

fn body_learning_fixture() -> (CreatureBuild, Arc<BodyPackage>, Arc<BodyPackage>) {
    let mut build = fixture(2);
    let mut catalog = build.content().catalog().clone();
    let mut other = build.species().clone();
    other.id = crate::content_ids::SpeciesId::new("learning_other_body").unwrap();
    other.default_attributes = Attributes::new(ByAttribute {
        strength: 3,
        speed: 2,
        intellect: 4,
        ..ByAttribute::default()
    })
    .unwrap();
    other.modifiers.push(crate::test_creatures::add(
        Stat::Attribute {
            attribute: Attribute::Intellect,
        },
        4,
    ));
    let other_species = other.id.clone();
    catalog.species.insert(other.id.clone(), other);
    build.content = Content::new(build.content().rules().clone(), catalog).unwrap();
    let base = Arc::new(BodyPackage {
        id: crate::content_ids::BodyFormId::new("learning_base").unwrap(),
        species: build.species_id().clone(),
        body: crate::BodySpec::default(),
        advancement: vec![PlannedAdvancement {
            source: HdSource::Racial,
            choices: HdChoices::default(),
        }],
    });
    let other = Arc::new(BodyPackage {
        id: crate::content_ids::BodyFormId::new("learning_other").unwrap(),
        species: other_species,
        body: crate::BodySpec::default(),
        advancement: vec![
            PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default()
            };
            3
        ],
    });
    (
        build.with_body_package(base.clone(), 42).unwrap(),
        base,
        other,
    )
}

#[test]
fn body_package_intellect_does_not_create_personal_learning_or_replace_birth_identity() {
    let (mut build, base, other) = body_learning_fixture();
    build.select_intellect_expertise(1, id("knife")).unwrap();
    let original_learning = build.intellect_learning.clone();
    let transformed = build.replace_body_package(other).unwrap();
    assert_eq!(transformed.initial_attributes().get(Attribute::Strength), 3);
    assert_eq!(
        transformed.initial_attributes().get(Attribute::Intellect),
        1
    );
    let derived = transformed.derive().unwrap();
    assert_eq!(derived.attributes.get(Attribute::Intellect), 5);
    assert_eq!(derived.intellect_capacity, 1);
    assert_eq!(transformed.intellect_learning, original_learning);
    transformed
        .intellect_learning
        .as_ref()
        .unwrap()
        .validate(&transformed)
        .unwrap();
    let encoded = serde_json::to_value(record::BuildRecord::capture(&transformed)).unwrap();
    let restored = serde_json::from_value::<record::BuildRecord>(encoded.clone())
        .unwrap()
        .restore(transformed.content().clone())
        .unwrap();
    assert_eq!(restored, transformed);
    let mut forged = encoded;
    forged["intellect_learning"]["birth"]["attributes"]["strength"] = serde_json::json!(3);
    assert!(serde_json::from_value::<record::BuildRecord>(forged)
        .unwrap()
        .restore(transformed.content().clone())
        .is_err());
    let returned = transformed.replace_body_package(base).unwrap();
    assert_eq!(returned.intellect_learning, original_learning);
}

#[test]
fn personal_intellect_earned_in_another_body_retains_one_threshold_and_stable_receipts() {
    let (build, base, other) = body_learning_fixture();
    let mut transformed = build.replace_body_package(other).unwrap();
    // Three package rows precede the same original personal HD.
    transformed
        .increase_attribute(crate::HdIndex(3), Attribute::Intellect)
        .unwrap();
    assert_eq!(transformed.derive().unwrap().intellect_capacity, 2);
    transformed
        .select_intellect_expertise(2, id("navigation"))
        .unwrap();
    transformed
        .intellect_learning
        .as_ref()
        .unwrap()
        .validate(&transformed)
        .unwrap();
    let returned = transformed.replace_body_package(base).unwrap();
    assert_eq!(returned.derive().unwrap().intellect_capacity, 2);
    assert_eq!(returned.intellect_expertise_choices().count(), 2);
    assert!(returned
        .derive()
        .unwrap()
        .expertises
        .active(&id("navigation")));
    let record = record::BuildRecord::capture(&transformed);
    assert_eq!(
        record.restore(transformed.content().clone()).unwrap(),
        transformed
    );
    let mut forged = serde_json::to_value(record).unwrap();
    let history = forged["intellect_learning"]["history"]
        .as_array_mut()
        .unwrap();
    let personal = history
        .iter_mut()
        .find(|row| row["ownership"]["kind"] == "personal")
        .unwrap();
    personal["ownership"]["id"] = serde_json::json!(99);
    assert!(serde_json::from_value::<record::BuildRecord>(forged)
        .unwrap()
        .restore(transformed.content().clone())
        .is_err());
}
