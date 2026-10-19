//! Narrow permanent Intellect composition and source-witness contracts.
use super::intellect::{IntellectSources, current_sources, eligible_intellect};
use super::{Content, CreatureBuild, Template};
use crate::attributes::{Attribute, Attributes, ByAttribute};
use crate::content_ids::{ClassId, FlagId, TalentId, TemplateId};
use crate::rules::{FlagDef, Modifier, ModifierOp, Requirement, Ruleset, Scope, Stat};

fn add(value: i64) -> Modifier {
    crate::test_creatures::add(
        Stat::Attribute {
            attribute: Attribute::Intellect,
        },
        value,
    )
}
fn set(value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::Attribute {
                attribute: Attribute::Intellect,
            },
            value,
        },
    }
}
fn fixture(modifiers: Vec<Modifier>, templates: Vec<Template>) -> CreatureBuild {
    let mut species = crate::test_creatures::species();
    species.default_attributes = Attributes::new(ByAttribute {
        intellect: 3,
        ..ByAttribute::default()
    })
    .unwrap();
    species.modifiers = modifiers;
    crate::test_creatures::build_with(species, templates, &[crate::progression::HdSource::Racial])
}

#[test]
fn birth_templates_are_retained_while_later_polymorph_is_excluded() {
    let mut birth = Template::new(TemplateId::new("fixture_birth").unwrap(), 0);
    birth.adjustments.intellect = 2;
    birth.modifiers.push(add(1));
    let mut later = Template::new(TemplateId::new("fixture_later").unwrap(), 1);
    later.overrides.intellect = Some(1);
    let mut build = fixture(vec![], vec![birth.clone(), later.clone()]);
    // A later replacement does not alter the immutable birth composition.
    build.templates = vec![later.id.clone()];
    let sources = current_sources(&build, &[birth.id.clone()]).unwrap();
    assert_eq!(
        eligible_intellect(&build, &[birth.id], &sources).unwrap(),
        6
    );
    assert_eq!(
        eligible_intellect(&build, &[later.id], &sources).unwrap(),
        1
    );
}

#[test]
fn historical_set_masks_attribute_and_add_sources_without_stored_amounts() {
    let mut build = fixture(vec![set(7)], vec![]);
    let talent = crate::test_creatures::talent("fixture_intellect_add", vec![add(4)]);
    let mut document = build.ruleset().document().clone();
    document.talents.push(talent.clone());
    build.content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    let before = IntellectSources::default();
    let mut after = before.clone();
    after.attribute_increases.intellect = 1;
    after.talents.insert(talent.id);
    assert_eq!(eligible_intellect(&build, &[], &before).unwrap(), 7);
    assert_eq!(eligible_intellect(&build, &[], &after).unwrap(), 7);
}

#[test]
fn flag_expansion_is_deduplicated_per_source_not_across_owners() {
    let flag = FlagId::new("fixture_intellect").unwrap();
    let flag_modifier = Modifier {
        scope: Scope::Always,
        op: ModifierOp::Flag { flag: flag.clone() },
    };
    let mut build = fixture(vec![], vec![]);
    let mut document = build.ruleset().document().clone();
    document.flags.push(FlagDef {
        id: flag,
        name: "Intellect fixture".into(),
        modifiers: vec![add(2)],
    });
    let talent = crate::test_creatures::talent(
        "fixture_intellect_flag",
        vec![flag_modifier.clone(), flag_modifier.clone()],
    );
    document.talents.push(talent.clone());
    let mut catalog = build.content().catalog().clone();
    catalog
        .species
        .get_mut(build.species_id())
        .unwrap()
        .modifiers = vec![flag_modifier];
    build.content = Content::new(Ruleset::validate(document).unwrap(), catalog).unwrap();
    let mut sources = IntellectSources::default();
    assert_eq!(eligible_intellect(&build, &[], &sources).unwrap(), 5);
    sources.talents.insert(talent.id);
    assert_eq!(eligible_intellect(&build, &[], &sources).unwrap(), 7);
}

#[test]
fn permanent_requirements_control_talents_without_effective_fold_or_clone() {
    let mut build = fixture(vec![], vec![]);
    let mut talent = crate::test_creatures::talent("fixture_intellect_requirement", vec![add(2)]);
    talent.requirement = Requirement::AttributeMinimum {
        attribute: Attribute::Intellect,
        minimum: 4,
    };
    let mut document = build.ruleset().document().clone();
    document.talents.push(talent.clone());
    build.content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    build.ledger =
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial; 4], 42)
            .unwrap();
    build.advancement.choices = vec![super::HdChoices::default(); 4];
    build.advancement.choices[0]
        .purchases
        .push(super::AdvancementPurchase {
            pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: super::PurchaseSelection::Talent(talent.id.clone()),
        });
    build.advancement.choices[3].attribute = Some(Attribute::Intellect);
    let before = crate::diagnostics::work_counts();
    let active = current_sources(&build, &[]).unwrap();
    assert!(active.talents.contains(&talent.id));
    assert_eq!(active.attribute_increases.intellect, 1);
    assert_eq!(eligible_intellect(&build, &[], &active).unwrap(), 6);
    // A real permanent prerequisite choice loss makes the owned talent dormant.
    build.advancement.choices[3].attribute = None;
    let dormant = current_sources(&build, &[]).unwrap();
    assert!(dormant.talents.is_empty());
    assert_eq!(eligible_intellect(&build, &[], &dormant).unwrap(), 3);
    build.advancement.choices[3].attribute = Some(Attribute::Intellect);
    assert_eq!(current_sources(&build, &[]).unwrap(), active);
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.dynamic_creature_folds, before.dynamic_creature_folds);
    assert_eq!(
        after.permanent_creature_compiles,
        before.permanent_creature_compiles
    );
    assert_eq!(after.creature_build_copies, before.creature_build_copies);
    // Temporary losses live in effective state and do not alter this witness.
    let mut state = super::CreatureState::new(build).unwrap();
    state.lose_attribute(Attribute::Intellect, 3).unwrap();
    assert_eq!(state.derived().attributes.get(Attribute::Intellect), 3);
    let after_loss = current_sources(state.build(), &[]).unwrap();
    assert_eq!(after_loss, active);
    assert_eq!(
        eligible_intellect(state.build(), &[], &after_loss).unwrap(),
        6
    );
}

#[test]
fn class_modifiers_apply_once_and_reused_facts_do_not_rescan_choices() {
    let mut build = fixture(vec![], vec![]);
    let class = ClassId::new("warrior").unwrap();
    let mut document = build.ruleset().document().clone();
    document
        .classes
        .iter_mut()
        .find(|definition| definition.id == class)
        .unwrap()
        .modifiers
        .push(add(2));
    build.content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    build.ledger = crate::progression::HdLedger::seeded(
        vec![crate::progression::HdSource::Class(class.clone()); 2],
        42,
    )
    .unwrap();
    build.advancement.choices = vec![super::HdChoices::default(); 2];
    let witness = current_sources(&build, &[]).unwrap();
    assert_eq!(witness.classes.len(), 1);
    assert_eq!(eligible_intellect(&build, &[], &witness).unwrap(), 5);
    let (attributes, skills, selected) =
        super::derive::advancement_facts(&build, build.initial_attributes()).unwrap();
    let hit_dice = super::HitDiceSummary {
        total: crate::HdCount(2),
        racial: crate::HdCount::ZERO,
        classes: std::collections::BTreeMap::from([(class, crate::HdCount(2))]),
    };
    let before = crate::diagnostics::work_counts();
    assert_eq!(
        super::intellect::current_sources_from_facts(
            &build,
            &[],
            attributes,
            &skills,
            &selected,
            &hit_dice
        )
        .unwrap(),
        witness
    );
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.advancement_choice_visits,
        before.advancement_choice_visits
    );
    assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
    assert_eq!(after.dynamic_creature_folds, before.dynamic_creature_folds);
    assert_eq!(after.creature_build_copies, before.creature_build_copies);
}

#[test]
fn immutable_source_ids_reject_missing_definitions_and_conflicting_sets() {
    let mut build = fixture(vec![set(7)], vec![]);
    let mut sources = IntellectSources::default();
    sources
        .classes
        .insert(ClassId::new("fixture_missing").unwrap());
    assert_eq!(
        eligible_intellect(&build, &[], &sources),
        Err(super::BuildError::UnknownClass)
    );
    sources.classes.clear();
    sources
        .talents
        .insert(TalentId::new("fixture_conflicting_set").unwrap());
    let mut document = build.ruleset().document().clone();
    document.talents.push(crate::test_creatures::talent(
        "fixture_conflicting_set",
        vec![set(8)],
    ));
    build.content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    assert_eq!(
        eligible_intellect(&build, &[], &sources),
        Err(super::BuildError::ConflictingOverride)
    );
}

#[test]
fn historical_initial_attributes_do_not_follow_privileged_current_edits() {
    let mut build = fixture(vec![], vec![]);
    let initial = build.initial_attributes();
    let witness = current_sources(&build, &[]).unwrap();
    build.initial_attributes = Attributes::new(ByAttribute {
        intellect: 5,
        ..ByAttribute::default()
    })
    .unwrap();
    assert_eq!(eligible_intellect(&build, &[], &witness).unwrap(), 5);
    assert_eq!(
        super::intellect::eligible_intellect_at(&build, initial, &[], &witness).unwrap(),
        3
    );
}

#[test]
fn birth_type_and_subtype_sources_and_requirements_ignore_later_form() {
    let kind = crate::content_ids::TypeId::new("construct").unwrap();
    let subtype = crate::content_ids::SubtypeId::new("air").unwrap();
    let mut birth = Template::new(TemplateId::new("fixture_birth_kind").unwrap(), 0);
    birth.kind = Some(kind.clone());
    birth.add_subtypes.insert(subtype.clone());
    let mut later = Template::new(TemplateId::new("fixture_later_kind").unwrap(), 1);
    later.kind = Some(crate::content_ids::TypeId::new("humanoid").unwrap());
    later.remove_subtypes.insert(subtype.clone());
    let mut build = fixture(vec![add(1)], vec![birth.clone(), later.clone()]);
    let mut document = build.ruleset().document().clone();
    document
        .creature_types
        .iter_mut()
        .find(|definition| definition.id == kind)
        .unwrap()
        .modifiers
        .push(add(2));
    document
        .subtypes
        .iter_mut()
        .find(|definition| definition.id == subtype)
        .unwrap()
        .modifiers
        .push(add(3));
    let mut talent = crate::test_creatures::talent("fixture_birth_requirement", vec![add(4)]);
    talent.requirement = Requirement::All {
        requirements: vec![
            Requirement::TypeIn { types: vec![kind] },
            Requirement::SubtypeIn {
                subtypes: vec![subtype],
            },
        ],
    };
    document.talents.push(talent.clone());
    build.content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    build.advancement.choices[0]
        .purchases
        .push(super::AdvancementPurchase {
            pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: super::PurchaseSelection::Talent(talent.id.clone()),
        });
    build.templates = vec![later.id];
    let sources = current_sources(&build, &[birth.id.clone()]).unwrap();
    assert!(sources.talents.contains(&talent.id));
    assert_eq!(
        eligible_intellect(&build, &[birth.id], &sources).unwrap(),
        13
    );
    assert!(current_sources(&build, &[]).unwrap().talents.is_empty());
}

#[test]
fn historical_talent_eligibility_uses_recorded_choices_and_initial_values() {
    let mut build = fixture(vec![], vec![]);
    let historical_initial = build.initial_attributes();
    let mut talent = crate::test_creatures::talent("fixture_historical_requirement", vec![add(2)]);
    talent.requirement = Requirement::All {
        requirements: vec![
            Requirement::AttributeMinimum {
                attribute: Attribute::Intellect,
                minimum: 4,
            },
            Requirement::SkillRank {
                skill: crate::rules::SkillSelector::Named {
                    skill: crate::attributes::Skill::Deception,
                },
                minimum: 2,
            },
        ],
    };
    let mut document = build.ruleset().document().clone();
    document.talents.push(talent.clone());
    build.content = Content::new(
        Ruleset::validate(document).unwrap(),
        build.content().catalog().clone(),
    )
    .unwrap();
    let mut rows = vec![
        (
            crate::progression::HdSource::Racial,
            super::HdChoices::default()
        );
        4
    ];
    rows[0].1.purchases.push(super::AdvancementPurchase {
        pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: super::PurchaseSelection::Talent(talent.id.clone()),
    });
    rows[1].1.purchases.push(super::AdvancementPurchase {
        pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: super::PurchaseSelection::SkillRank(crate::attributes::Skill::Deception),
    });
    rows[3].1.attribute = Some(Attribute::Intellect);
    let historical =
        super::intellect::historical_sources(&build, historical_initial, &[], &rows).unwrap();
    assert!(historical.talents.contains(&talent.id));
    assert_eq!(historical.attribute_increases.intellect, 1);
    assert!(current_sources(&build, &[]).unwrap().talents.is_empty());
    // Current privileged edits cannot satisfy a missing historical choice.
    build.initial_attributes = Attributes::new(ByAttribute {
        intellect: 5,
        ..ByAttribute::default()
    })
    .unwrap();
    rows[3].1.attribute = None;
    assert!(
        super::intellect::historical_sources(&build, historical_initial, &[], &rows)
            .unwrap()
            .talents
            .is_empty()
    );
    rows[3].1.attribute = Some(Attribute::Intellect);
    rows[1].1.purchases.clear();
    assert!(
        super::intellect::historical_sources(&build, historical_initial, &[], &rows)
            .unwrap()
            .talents
            .is_empty()
    );
}
