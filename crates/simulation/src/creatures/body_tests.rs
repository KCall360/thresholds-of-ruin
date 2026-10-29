use super::*;
use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use crate::content_ids::{BodyFormId, ClassId, SpeciesId, TalentId};
use crate::progression::{HdLedger, HdSource};
use crate::rules::{Requirement, Ruleset};
use std::{collections::BTreeMap, sync::Arc};

fn fixture() -> (
    CreatureBuild,
    Arc<BodyPackage>,
    Arc<BodyPackage>,
    Arc<BodyPackage>,
) {
    let mut human = crate::test_creatures::unmodified_species("body_human");
    human.default_attributes = Attributes::new(ByAttribute {
        strength: 2,
        speed: 2,
        intellect: 2,
        willpower: 3,
        awareness: 1,
        presence: 2,
    })
    .unwrap();
    let mut large = crate::test_creatures::unmodified_species("body_large");
    large.default_attributes = Attributes::new(ByAttribute {
        strength: 4,
        speed: 1,
        intellect: 0,
        willpower: 0,
        awareness: 0,
        presence: 0,
    })
    .unwrap();
    let mut other = large.clone();
    other.id = SpeciesId::new("body_other").unwrap();
    let mut document = crate::test_creatures::ruleset().document().clone();
    let mut talent = crate::test_creatures::talent(
        "fixture_body_strength",
        vec![crate::test_creatures::add(crate::rules::Stat::Health, 3)],
    );
    talent.requirement = Requirement::AttributeMinimum {
        attribute: Attribute::Strength,
        minimum: 4,
    };
    document.talents.push(talent);
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([
                (human.id.clone(), human.clone()),
                (large.id.clone(), large.clone()),
                (other.id.clone(), other.clone()),
            ]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let package = |id: &str, species: SpeciesId, count: usize| {
        Arc::new(BodyPackage {
            id: BodyFormId::new(id).unwrap(),
            species,
            body: crate::BodySpec::default(),
            advancement: vec![
                PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default()
                };
                count
            ],
        })
    };
    let base = package("human_body", human.id.clone(), 1);
    let large = package("large_body", large.id, 3);
    let other = package("other_body", other.id, 2);
    let build = CreatureBuild::new(
        content,
        human.id,
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            42,
        )
        .unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
    .with_body_package(base.clone(), 42)
    .unwrap();
    (build, base, large, other)
}

#[test]
fn body_replacement_retains_personal_class_attributes_and_locked_budget() {
    let (build, base, large, _) = fixture();
    let personal = build.ledger().entries()[1].clone();
    assert_eq!(build.unspent_points(), 1);
    let changed = build.replace_body_package(large).unwrap();
    assert_eq!(changed.personal_hd_count(), crate::HdCount(1));
    assert_eq!(changed.ledger().total_hd(), crate::HdCount(4));
    assert_eq!(changed.ledger().entries()[3], personal);
    assert_eq!(changed.initial_attributes().get(Attribute::Strength), 4);
    assert_eq!(changed.initial_attributes().get(Attribute::Speed), 1);
    for attribute in [
        Attribute::Intellect,
        Attribute::Willpower,
        Attribute::Awareness,
        Attribute::Presence,
    ] {
        assert_eq!(
            changed.initial_attributes().get(attribute),
            build.initial_attributes().get(attribute)
        );
    }
    assert_eq!(changed.starting_skills(), build.starting_skills());
    assert_eq!(changed.unspent_points(), 1);
    let returned = changed.replace_body_package(base).unwrap();
    assert_eq!(returned.ledger(), build.ledger());
    assert_eq!(returned.choices(), build.choices());
    assert_eq!(
        returned.derive().unwrap().maximum_health,
        build.derive().unwrap().maximum_health
    );
}

#[test]
fn personally_owned_talent_is_dormant_then_reactivates_in_another_qualifying_form() {
    let (build, base, large, other) = fixture();
    let talent = TalentId::new("fixture_body_strength").unwrap();
    let mut changed = build.replace_body_package(large).unwrap();
    changed
        .select_talent(crate::HdIndex(3), talent.clone())
        .unwrap();
    let purchase = changed.choices()[3].purchases[0].clone();
    let dormant = changed.replace_body_package(base).unwrap();
    assert!(!dormant.derive().unwrap().active_talents.contains(&talent));
    assert_eq!(dormant.choices()[1].purchases[0], purchase);
    assert_eq!(dormant.unspent_points(), 0);
    let reactivated = dormant.replace_body_package(other).unwrap();
    assert!(reactivated
        .derive()
        .unwrap()
        .active_talents
        .contains(&talent));
    assert_eq!(reactivated.choices()[2].purchases[0], purchase);
}

#[test]
fn body_swaps_preserve_personal_physical_and_magical_growth_exactly_once_within_caps() {
    let (mut build, base, large, _) = fixture();
    let warrior = HdSource::Class(ClassId::new("warrior").unwrap());
    for _ in 1..8 {
        build.add_hit_die(warrior.clone(), 42).unwrap();
    }
    // Personal milestones four/eight occupy effective slots four/eight behind body HD0.
    build
        .increase_attribute(crate::HdIndex(4), Attribute::Strength)
        .unwrap();
    build
        .increase_attribute(crate::HdIndex(8), Attribute::Speed)
        .unwrap();
    assert!(build.acquire_strength().unwrap());
    let personal_dice = build.ledger().entries()[1..].to_vec();
    let personal_choices = build.choices()[1..].to_vec();
    assert_eq!(build.personal_hd_count(), crate::HdCount(8));
    assert_eq!(build.strength_acquired(), 1);
    assert_eq!(build.initial_attributes().get(Attribute::Strength), 2);
    assert_eq!(build.initial_attributes().get(Attribute::Speed), 2);
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        4
    );
    assert_eq!(build.derive().unwrap().attributes.get(Attribute::Speed), 3);
    for (index, die) in personal_dice.iter().enumerate() {
        assert_eq!(
            die.owner(),
            Some(&super::composition::HdOwner::Personal {
                id: super::composition::PersonalHdId(
                    std::num::NonZeroU64::new(index as u64 + 1).unwrap()
                ),
            })
        );
    }
    let changed = build.replace_body_package(large.clone()).unwrap();
    assert_eq!(changed.ledger().total_hd(), crate::HdCount(11));
    assert_eq!(changed.personal_hd_count(), crate::HdCount(8));
    assert_eq!(changed.strength_acquired(), 1);
    assert_eq!(&changed.ledger().entries()[3..], personal_dice.as_slice());
    assert_eq!(&changed.choices()[3..], personal_choices.as_slice());
    // New base4 + retained ordinary1 + magic1; Speed base1 + ordinary1.
    assert_eq!(changed.initial_attributes().get(Attribute::Strength), 4);
    assert_eq!(changed.initial_attributes().get(Attribute::Speed), 1);
    assert_eq!(
        changed
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        6
    );
    assert_eq!(
        changed.derive().unwrap().attributes.get(Attribute::Speed),
        2
    );
    let encoded = serde_json::to_vec(&super::record::BuildRecord::capture(&changed)).unwrap();
    let decoded: super::record::BuildRecord = serde_json::from_slice(&encoded).unwrap();
    let restored = decoded.restore(changed.content().clone()).unwrap();
    assert_eq!(restored, changed);
    let returned = restored.replace_body_package(base).unwrap();
    assert_eq!(returned.ledger(), build.ledger());
    assert_eq!(returned.choices(), build.choices());
    assert_eq!(returned.initial_attributes(), build.initial_attributes());
    assert_eq!(returned.strength_acquired(), 1);
    assert_eq!(
        returned
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        4
    );
    assert_eq!(
        returned.derive().unwrap().attributes.get(Attribute::Speed),
        3
    );
    let revisited = returned.replace_body_package(large).unwrap();
    assert_eq!(revisited.ledger(), changed.ledger());
    assert_eq!(revisited.choices(), changed.choices());
    assert_eq!(revisited.strength_acquired(), 1);
    assert_eq!(
        revisited
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        6
    );
    assert_eq!(
        revisited.derive().unwrap().attributes.get(Attribute::Speed),
        2
    );
}

#[test]
fn repeated_visits_and_personal_growth_do_not_reroll_owned_health() {
    let (build, base, large, _) = fixture();
    let mut changed = build.replace_body_package(large.clone()).unwrap();
    let package_rows = changed.ledger().entries()[..3].to_vec();
    changed.add_hit_die(HdSource::Racial, 42).unwrap();
    let earned = changed.ledger().entries().last().unwrap().clone();
    assert!(earned.owner().unwrap().is_personal());
    let returned = changed.replace_body_package(base).unwrap();
    assert_eq!(returned.ledger().entries().last().unwrap(), &earned);
    let revisited = returned.replace_body_package(large).unwrap();
    assert_eq!(&revisited.ledger().entries()[..3], package_rows.as_slice());
    assert_eq!(revisited.ledger().entries().last().unwrap(), &earned);
    assert_eq!(revisited.personal_hd_count(), crate::HdCount(2));
}

#[test]
fn conflicting_retained_personal_restore_is_a_growth_obstruction_not_hard_failure() {
    let (build, _, large, _) = fixture();
    let talent = TalentId::new("fixture_body_strength").unwrap();
    let mut changed = build.replace_body_package(large).unwrap();
    changed.add_hit_die(HdSource::Racial, 42).unwrap();
    changed
        .select_talent(crate::HdIndex(4), talent.clone())
        .unwrap();
    // This is explicitly Authored: ChooseAgain must not reinstate the lost
    // talent or manufacture an obstruction from a discarded choice.
    let mut choices = changed.choices().to_vec();
    for row in &mut choices {
        row.purchase_highwater = 0;
        for purchase in &mut row.purchases {
            purchase.id = None;
        }
    }
    let sources: Vec<_> = changed
        .ledger()
        .entries()
        .iter()
        .map(|row| row.source())
        .collect();
    let restoration = RestorationPolicy::Authored(
        sources
            .iter()
            .cloned()
            .zip(choices.iter().cloned())
            .map(|(source, choices)| PlannedAdvancement { source, choices })
            .collect(),
    );
    changed = CreatureBuild::from_recorded_body_package(
        changed.content().clone(),
        changed.active_body_package().unwrap().clone(),
        changed.initial_attributes(),
        HdLedger::seeded(sources, 42).unwrap(),
        changed.binding(),
        BuildAdvancement {
            starting_skills: *changed.starting_skills(),
            choices,
            restoration,
        },
        42,
    )
    .unwrap();
    changed.remove_latest().unwrap();
    changed.select_talent(crate::HdIndex(3), talent).unwrap();
    let before = changed.clone();
    assert_eq!(
        changed.grow_hit_die(HdSource::Racial, 42).unwrap(),
        super::advancement::GrowthBuildOutcome::Obstructed
    );
    assert_eq!(changed, before);
}

#[test]
fn return_uses_original_custom_physical_base_not_species_defaults() {
    let (build, base, large, _) = fixture();
    // Explicit privileged setup before the ownership enrollment, not a form
    // change resetting an earned attribute or its personal provenance.
    let mut original = CreatureBuild::new(
        build.content().clone(),
        build.species_id().clone(),
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            42,
        )
        .unwrap(),
        build.binding(),
        *build.starting_skills(),
    )
    .unwrap();
    let mut attributes = original.initial_attributes().values();
    attributes.strength = 3;
    attributes.speed = 4;
    original
        .set_initial_attributes(Attributes::new(attributes).unwrap())
        .unwrap();
    let original = original.with_body_package(base.clone(), 42).unwrap();
    let changed = original.replace_body_package(large).unwrap();
    assert_eq!(changed.initial_attributes().get(Attribute::Strength), 4);
    let returned = changed.replace_body_package(base).unwrap();
    assert_eq!(returned.initial_attributes(), original.initial_attributes());
}

#[test]
fn package_and_personal_talent_keep_two_funding_records_but_grant_once() {
    let (build, _, large, _) = fixture();
    let talent = TalentId::new("fixture_body_strength").unwrap();
    let mut personal = build.replace_body_package(large.clone()).unwrap();
    personal
        .select_talent(crate::HdIndex(3), talent.clone())
        .unwrap();
    let personal_purchase = personal.choices()[3].purchases[0].clone();
    let mut innate = (*large).clone();
    innate.id = BodyFormId::new("innate_large").unwrap();
    innate.advancement[0]
        .choices
        .purchases
        .push(AdvancementPurchase {
            id: None,
            pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: PurchaseSelection::Talent(talent.clone()),
        });
    let innate = Arc::new(innate);
    let package_only = build.replace_body_package(innate.clone()).unwrap();
    let both = personal.replace_body_package(innate).unwrap();
    assert_eq!(
        both.derive().unwrap().maximum_health,
        package_only.derive().unwrap().maximum_health
    );
    assert_eq!(both.choices()[3].purchases[0], personal_purchase);
    assert_eq!(both.choices()[0].purchases.len(), 1);
    assert_ne!(
        both.choices()[0].purchases[0].id,
        both.choices()[3].purchases[0].id
    );
    assert_eq!(both.unspent_points(), 0);
    assert!(both
        .preview_purchase(
            crate::HdIndex(0),
            crate::content_ids::PointPoolId::new("advancement").unwrap(),
            PurchaseSelection::SkillRank(Skill::Discipline)
        )
        .is_err());
}

fn personal_restoration_fixture(authored: bool) -> (CreatureBuild, Arc<BodyPackage>) {
    let (seed, base, large, _) = fixture();
    let ledger = HdLedger::seeded(
        vec![
            HdSource::Racial,
            HdSource::Class(ClassId::new("warrior").unwrap()),
        ],
        42,
    )
    .unwrap();
    let choices = vec![
        HdChoices::default(),
        HdChoices {
            purchase_highwater: 0,
            purchases: vec![AdvancementPurchase {
                id: None,
                pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
                selection: PurchaseSelection::SkillRank(Skill::Lore),
            }],
            attribute: None,
        },
    ];
    let restoration = if authored {
        RestorationPolicy::Authored(
            ledger
                .entries()
                .iter()
                .zip(&choices)
                .map(|(die, choices)| PlannedAdvancement {
                    source: die.source(),
                    choices: choices.clone(),
                })
                .collect(),
        )
    } else {
        RestorationPolicy::ChooseAgain
    };
    let build = CreatureBuild::from_recorded_body_package(
        seed.content().clone(),
        base,
        seed.initial_attributes(),
        ledger,
        seed.binding(),
        BuildAdvancement {
            starting_skills: *seed.starting_skills(),
            choices,
            restoration,
        },
        42,
    )
    .unwrap();
    (build, large)
}

#[test]
fn owned_choose_again_preserves_health_and_funding_not_removed_choices() {
    let (mut build, large) = personal_restoration_fixture(false);
    let original_die = build.ledger().entries()[1].clone();
    let original_purchase = build.choices()[1].purchases[0].clone();
    assert_eq!(build.unspent_points(), 0);
    assert_eq!(build.derive().unwrap().skills.get(Skill::Lore), 1);
    build.remove_latest().unwrap();
    build = build.replace_body_package(large).unwrap();
    let package_rows = build.ledger().entries().to_vec();
    build.add_hit_die(original_die.source(), 42).unwrap();
    assert_eq!(&build.ledger().entries()[..3], package_rows.as_slice());
    assert_eq!(build.ledger().entries()[3], original_die);
    assert_eq!(build.personal_health_root_seed(), Some(42));
    assert_eq!(
        build.choices()[3],
        HdChoices {
            purchase_highwater: 1,
            ..HdChoices::default()
        }
    );
    assert_eq!(build.derive().unwrap().skills.get(Skill::Lore), 0);
    assert_eq!(build.unspent_points(), 1);
    build.train(crate::HdIndex(3), Skill::Lore).unwrap();
    let replacement = build.choices()[3].purchases[0].id.as_ref().unwrap();
    assert_eq!(
        replacement.owner,
        original_purchase.id.as_ref().unwrap().owner
    );
    assert_eq!(replacement.serial.get(), 2);
    assert_eq!(build.unspent_points(), 0);
    build.remove_latest().unwrap();
    build.add_hit_die(original_die.source(), 42).unwrap();
    build.train(crate::HdIndex(3), Skill::Lore).unwrap();
    assert_eq!(
        build.choices()[3].purchases[0]
            .id
            .as_ref()
            .unwrap()
            .serial
            .get(),
        3
    );
    assert_eq!(build.ledger().entries()[3], original_die);
    assert_eq!(build.unspent_points(), 0);
}

#[test]
fn owned_authored_restoration_uses_original_receipt_after_respec_and_reseed() {
    let (prototype, large) = personal_restoration_fixture(true);
    let mut build = prototype.instantiate_seeded(73).unwrap();
    assert!(build.body_birth_uses_authored_restoration());
    assert_eq!(build.personal_health_root_seed(), Some(73));
    let original_die = build.ledger().entries()[1].clone();
    let original_choices = build.choices()[1].clone();
    let mut replacement = original_choices.purchases[0].clone();
    replacement.id = None;
    replacement.selection = PurchaseSelection::SkillRank(Skill::Athletics);
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: crate::HdIndex(1),
                index: 0,
            },
            Some(replacement),
        )
        .unwrap();
    build.commit_replacement(preview).unwrap();
    assert_eq!(build.choices()[1].purchase_highwater, 2);
    assert_eq!(
        build.choices()[1].purchases[0]
            .id
            .as_ref()
            .unwrap()
            .serial
            .get(),
        2
    );
    assert!(
        build.instantiate_seeded(81).is_err(),
        "edited runtime receipts are not fresh birth recipes"
    );
    build.remove_latest().unwrap();
    build = build.replace_body_package(large).unwrap();
    build.add_hit_die(original_die.source(), 73).unwrap();
    assert_eq!(build.ledger().entries()[3], original_die);
    assert_eq!(build.choices()[3].purchases, original_choices.purchases);
    assert_eq!(build.choices()[3].attribute, original_choices.attribute);
    assert_eq!(build.choices()[3].purchase_highwater, 2);
    assert_eq!(build.derive().unwrap().skills.get(Skill::Lore), 1);
    assert_eq!(build.derive().unwrap().skills.get(Skill::Athletics), 0);
    assert_eq!(build.unspent_points(), 0);
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: crate::HdIndex(3),
                index: 0,
            },
            None,
        )
        .unwrap();
    build.commit_replacement(preview).unwrap();
    build.train(crate::HdIndex(3), Skill::Lore).unwrap();
    assert_eq!(
        build.choices()[3].purchases[0]
            .id
            .as_ref()
            .unwrap()
            .serial
            .get(),
        3
    );
    assert_eq!(build.unspent_points(), 0);
}

#[test]
fn enrollment_rejects_ambiguous_unowned_authored_respec_without_inventing_serials() {
    let (seed, base, _, _) = fixture();
    let mut raw = CreatureBuild::new(
        seed.content().clone(),
        seed.species_id().clone(),
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            42,
        )
        .unwrap(),
        seed.binding(),
        *seed.starting_skills(),
    )
    .unwrap();
    raw.train(crate::HdIndex(1), Skill::Lore).unwrap();
    raw.advancement.restoration = RestorationPolicy::Authored(
        raw.ledger()
            .entries()
            .iter()
            .zip(raw.choices())
            .map(|(die, choices)| PlannedAdvancement {
                source: die.source(),
                choices: choices.clone(),
            })
            .collect(),
    );
    raw.set_future_growth(crate::earned_growth::FutureGrowth::Authored {})
        .unwrap();
    let enrolled = raw.with_body_package(base.clone(), 42).unwrap();
    assert!(enrolled.uses_authored_restoration());
    assert!(matches!(
        enrolled.future_growth(),
        crate::earned_growth::FutureGrowth::Authored {}
    ));
    let preview = raw
        .preview_replacement(
            PurchaseLocation {
                owner: crate::HdIndex(1),
                index: 0,
            },
            None,
        )
        .unwrap();
    raw.commit_replacement(preview).unwrap();
    raw.train(crate::HdIndex(1), Skill::Athletics).unwrap();
    assert_eq!(
        raw.with_body_package(base, 42),
        Err(BuildError::InvalidOwnership)
    );
}

#[test]
fn owned_authored_restore_rejects_wrong_class_without_consuming_loss_or_health_identity() {
    let (mut build, _) = personal_restoration_fixture(true);
    let removed = build.remove_latest().unwrap();
    let before = build.clone();
    assert_eq!(
        build.add_hit_die(HdSource::Racial, 42),
        Err(BuildError::InvalidOwnership)
    );
    assert_eq!(build, before);
    build.add_hit_die(removed.hit_die.source(), 42).unwrap();
    assert_eq!(build.ledger().entries()[1], removed.hit_die);
}

#[test]
fn authored_personal_plan_exhaustion_restores_earned_row_without_new_authored_choices() {
    let (mut build, _) = personal_restoration_fixture(true);
    build
        .add_hit_die(HdSource::Class(ClassId::new("warrior").unwrap()), 42)
        .unwrap();
    build.train(crate::HdIndex(2), Skill::Athletics).unwrap();
    let earned = build.remove_latest().unwrap();
    build.add_hit_die(earned.hit_die.source(), 42).unwrap();
    assert_eq!(build.ledger().entries()[2], earned.hit_die);
    assert_eq!(
        build.choices()[2],
        HdChoices {
            purchase_highwater: 1,
            ..HdChoices::default()
        }
    );
    assert_eq!(
        build.choices()[1].purchases[0].selection,
        PurchaseSelection::SkillRank(Skill::Lore)
    );
    assert_eq!(build.unspent_points(), 1);
}

#[test]
fn owned_choose_again_can_select_a_new_source_without_rerolling_or_reusing_purchase_ids() {
    let (mut build, _) = personal_restoration_fixture(false);
    let removed = build.remove_latest().unwrap();
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    let restored = &build.ledger().entries()[1];
    assert_eq!(restored.source(), HdSource::Racial);
    assert_eq!(restored.owner(), removed.hit_die.owner());
    assert_eq!(restored.health_seed(), removed.hit_die.health_seed());
    assert_eq!(restored.health_policy(), removed.hit_die.health_policy());
    assert_eq!(build.personal_health_root_seed(), Some(42));
    assert_eq!(
        build.choices()[1],
        HdChoices {
            purchase_highwater: 1,
            ..HdChoices::default()
        }
    );
    assert_eq!(build.unspent_points(), 1);
    build.train(crate::HdIndex(1), Skill::Lore).unwrap();
    assert_eq!(
        build.choices()[1].purchases[0]
            .id
            .as_ref()
            .unwrap()
            .serial
            .get(),
        2
    );
}

#[test]
fn owned_restoration_effect_uses_selected_career_only_for_choose_again() {
    for authored in [false, true] {
        let (mut build, large) = personal_restoration_fixture(authored);
        let original = build.ledger().entries()[1].clone();
        build
            .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
                source: crate::earned_growth::GrowthSource::Racial {},
            })
            .unwrap();
        let mut state = CreatureState::new(build).unwrap();
        state.drain_latest_hit_die().unwrap();
        assert_eq!(state.build().personal_hd_count(), crate::HdCount(0));
        let changed = state.build().replace_body_package(large).unwrap();
        state.rebuild(changed).unwrap();
        state.restore_hit_die().unwrap();
        let restored = &state.build().ledger().entries()[3];
        assert_eq!(
            restored.source(),
            if authored {
                original.source()
            } else {
                HdSource::Racial
            }
        );
        assert_eq!(restored.owner(), original.owner());
        assert_eq!(restored.health_seed(), original.health_seed());
        assert_eq!(
            state.build().choices()[3].purchases.len(),
            usize::from(authored)
        );
        assert_eq!(state.build().choices()[3].purchase_highwater, 1);
        let owner = restored.owner().unwrap();
        let acquisition = &state
            .build()
            .body_build
            .as_ref()
            .unwrap()
            .acquisition_species[owner];
        assert_eq!(
            acquisition,
            &SpeciesId::new(if authored { "body_human" } else { "body_large" }).unwrap()
        );
    }
}

#[test]
fn paid_owned_authored_regrowth_restores_declared_class_despite_selected_future_racial() {
    let (mut build, _) = personal_restoration_fixture(true);
    build
        .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
            source: crate::earned_growth::GrowthSource::Racial {},
        })
        .unwrap();
    let original = build.ledger().entries()[1].clone();
    let choices = build.choices()[1].clone();
    let mut state = CreatureState::new(build).unwrap();
    state.drain_latest_hit_die().unwrap();
    state.award_xp(100).unwrap();
    assert_eq!(state.build().ledger().entries()[1], original);
    assert_eq!(state.build().choices()[1], choices);
    assert_eq!(state.build().unspent_points(), 0);
}

#[test]
fn owned_authored_policy_admits_and_restores_only_its_remaining_stable_receipts() {
    let (mut build, _) = personal_restoration_fixture(true);
    assert!(build.uses_authored_restoration());
    assert_eq!(build.restoration_policy(), &RestorationPolicy::ChooseAgain);
    assert_eq!(build.next_authored_restoration_source(), None);
    build
        .set_future_growth(crate::earned_growth::FutureGrowth::Authored {})
        .unwrap();
    let born = build.instantiate_seeded(73).unwrap();
    assert!(matches!(
        born.future_growth(),
        crate::earned_growth::FutureGrowth::Authored {}
    ));
    crate::prepared_spawn::DepthGrowth::RacialAfter { depth: 1 }
        .validate_build(Some(&born))
        .unwrap();
    let original = build.remove_latest().unwrap();
    assert_eq!(
        build.next_authored_restoration_source(),
        Some(original.hit_die.source())
    );
    assert_eq!(
        crate::prepared_spawn::DepthGrowth::RacialAfter { depth: 1 }.validate_build(Some(&build)),
        Err(BuildError::InvalidOwnership)
    );
    build.restore_hit_die(42).unwrap();
    assert_eq!(build.ledger().entries()[1], original.hit_die);
    assert_eq!(build.choices()[1], original.choices);
    assert_eq!(
        build.restore_hit_die(42),
        Err(BuildError::NoAuthoredRestoration)
    );
    let mut state = CreatureState::new(build).unwrap();
    state.drain_latest_hit_die().unwrap();
    state.award_xp(100).unwrap();
    assert_eq!(state.build().ledger().entries()[1], original.hit_die);
    assert_eq!(state.build().choices()[1], original.choices);
    let (mut choose, _) = personal_restoration_fixture(false);
    assert!(!choose.uses_authored_restoration());
    assert_eq!(
        choose.set_future_growth(crate::earned_growth::FutureGrowth::Authored {}),
        Err(BuildError::NoAuthoredRestoration)
    );
    choose.remove_latest().unwrap();
    assert_eq!(
        choose.restore_hit_die(42),
        Err(BuildError::NoAuthoredRestoration)
    );
}

#[test]
fn original_body_funded_personal_grants_survive_forms_without_becoming_visitor_grants() {
    let (visitor, _, small, _) = fixture();
    let base = Arc::new(BodyPackage {
        id: BodyFormId::new("funded_personal_birth").unwrap(),
        species: visitor.species_id().clone(),
        body: crate::BodySpec::default(),
        advancement: vec![
            PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            };
            10
        ],
    });
    let mut native = CreatureBuild::new(
        visitor.content().clone(),
        base.species.clone(),
        HdLedger::seeded(vec![HdSource::Racial; 10], 77).unwrap(),
        ManaBinding::Willpower,
        [Skill::HeavyWeaponry, Skill::Breath],
    )
    .unwrap();
    native
        .increase_attribute(crate::HdIndex(3), Attribute::Willpower)
        .unwrap();
    native
        .increase_attribute(crate::HdIndex(7), Attribute::Willpower)
        .unwrap();
    native.train(crate::HdIndex(7), Skill::Breath).unwrap();
    let expected = native.derive().unwrap();
    assert_eq!(expected.attributes.get(Attribute::Willpower), 5);
    assert_eq!(expected.skills.get(Skill::Breath), 2);
    let grants = BTreeMap::from([
        (3, native.choices()[3].clone()),
        (7, native.choices()[7].clone()),
    ]);
    // This is a fresh authored partition, not a migration of the ordinary
    // comparison actor's already-recorded advancement history.
    let enroll = |grants| {
        CreatureBuild::from_recorded_body_package_personal_grants(
            native.content().clone(),
            base.clone(),
            native.initial_attributes(),
            native.ledger().clone(),
            native.binding(),
            native.advancement().clone(),
            77,
            grants,
        )
    };
    let owned = enroll(grants.clone()).unwrap();
    let birth_sources = &owned.intellect_learning.as_ref().unwrap().birth.sources;
    assert_eq!(birth_sources.attribute_increases.willpower, 2);
    let birth_record = super::record::BuildRecord::capture(&owned);
    assert_eq!(
        birth_record.restore(owned.content().clone()).unwrap(),
        owned
    );
    let historical_sources: Vec<_> = owned
        .ledger()
        .entries()
        .iter()
        .map(|die| die.source())
        .collect();
    let historical = |build: &CreatureBuild, skip_first: bool| {
        super::intellect::historical_sources_refs(
            build,
            owned.initial_attributes(),
            &[],
            0,
            historical_sources
                .iter()
                .zip(owned.ledger().entries())
                .zip(owned.choices())
                .skip(usize::from(skip_first))
                .map(|((source, die), choices)| (source, choices, die.owner())),
        )
    };

    let mut omitted = grants.clone();
    omitted.remove(&7);
    assert!(enroll(omitted).is_err());
    let mut wrong_ordinal = grants.clone();
    let moved = wrong_ordinal.remove(&3).unwrap();
    wrong_ordinal.insert(2, moved);
    assert!(enroll(wrong_ordinal).is_err());
    let mut forged = owned.clone();
    forged
        .body_build
        .as_mut()
        .unwrap()
        .personal_body_grants
        .get_mut(&7)
        .unwrap()
        .purchases[0]
        .id
        .as_mut()
        .unwrap()
        .serial = std::num::NonZeroU64::new(2).unwrap();
    assert_eq!(forged.derive().unwrap_err(), BuildError::InvalidOwnership);
    let mut overfunded = owned.clone();
    let row = overfunded
        .body_build
        .as_mut()
        .unwrap()
        .personal_body_grants
        .get_mut(&7)
        .unwrap();
    let mut extra = row.purchases[0].clone();
    extra.id.as_mut().unwrap().serial = std::num::NonZeroU64::new(2).unwrap();
    row.purchases.push(extra);
    row.purchase_highwater = 2;
    assert_eq!(
        overfunded.derive().unwrap_err(),
        BuildError::NoAdvancementPoint
    );
    assert_eq!(owned.ledger().total_hd(), crate::HdCount(10));
    assert_eq!(owned.personal_hd_count(), crate::HdCount(0));
    assert_eq!(owned.unspent_points(), 0);
    let funded_balance = owned.point_balances(crate::HdIndex(7)).unwrap();
    assert_eq!(funded_balance.len(), 1);
    assert_eq!((funded_balance[0].granted, funded_balance[0].spent), (1, 1));
    assert_eq!(
        funded_balance,
        native.point_balances(crate::HdIndex(7)).unwrap()
    );
    let mut locked = owned.clone();
    assert!(
        locked.train(crate::HdIndex(0), Skill::Breath).is_err(),
        "unspent body-pool reporting does not unlock personal purchases"
    );
    assert_eq!(locked, owned);
    let receipts = owned
        .body_build
        .as_ref()
        .unwrap()
        .personal_body_grants
        .clone();
    let born = owned.instantiate_seeded(88).unwrap();
    assert_ne!(
        born.ledger().entries()[0].health_seed(),
        owned.ledger().entries()[0].health_seed()
    );
    assert_eq!(born.ledger().total_hd(), crate::HdCount(10));
    assert_eq!(born.personal_hd_count(), crate::HdCount(0));
    assert_eq!(
        born.body_build.as_ref().unwrap().personal_body_grants,
        receipts
    );
    assert_eq!(
        born.derive().unwrap().attributes.get(Attribute::Willpower),
        5
    );
    assert_eq!(born.derive().unwrap().skills.get(Skill::Breath), 2);
    assert_eq!(born.unspent_points(), 0);
    assert_eq!(
        born.point_balances(crate::HdIndex(7)).unwrap(),
        funded_balance
    );
    assert_eq!(
        receipts[&7].purchases[0].id.as_ref().unwrap().serial.get(),
        1
    );
    assert_eq!(
        receipts[&7].purchases[0].id.as_ref().unwrap().owner,
        super::composition::HdOwner::Body {
            form: base.id.clone(),
            ordinal: 7
        }
    );
    let mut changed = owned.clone();
    for _ in 0..2 {
        changed = changed.replace_body_package(small.clone()).unwrap();
        assert_eq!(changed.ledger().total_hd(), crate::HdCount(3));
        for index in 0..3 {
            assert_eq!(
                changed.point_balances(crate::HdIndex(index)).unwrap()[0].spent,
                0,
                "retained acquisition never debits a different form"
            );
        }
        assert_eq!(
            changed
                .derive()
                .unwrap()
                .attributes
                .get(Attribute::Willpower),
            5
        );
        assert_eq!(changed.derive().unwrap().skills.get(Skill::Breath), 2);
        let lower_record = super::record::BuildRecord::capture(&changed);
        let lower_reopened = lower_record.restore(changed.content().clone()).unwrap();
        assert_eq!(lower_reopened, changed);
        assert_eq!(
            lower_reopened
                .intellect_learning
                .as_ref()
                .unwrap()
                .birth
                .sources
                .attribute_increases
                .willpower,
            2
        );
        assert_eq!(
            historical(&lower_reopened, false).unwrap(),
            *birth_sources,
            "lower active HD cannot erase the historical original donor"
        );
        changed = lower_reopened.replace_body_package(base.clone()).unwrap();
        assert_eq!(changed.ledger(), owned.ledger());
        assert_eq!(
            changed.point_balances(crate::HdIndex(7)).unwrap(),
            funded_balance,
            "repeated visits never duplicate the original debit"
        );
        assert_eq!(
            changed.body_build.as_ref().unwrap().personal_body_grants,
            receipts
        );
    }
    let mut lost = owned.clone();
    lost.remove_latest().unwrap(); // Original donor 9, no retained grant.
    lost.remove_latest().unwrap(); // Original donor 8, no retained grant.
    assert_eq!(
        lost.derive().unwrap().attributes.get(Attribute::Willpower),
        5
    );
    lost.remove_latest().unwrap(); // Original donor 7 funded Willpower and Breath.
    assert_eq!(
        lost.derive().unwrap().attributes.get(Attribute::Willpower),
        4
    );
    assert_eq!(lost.derive().unwrap().skills.get(Skill::Breath), 1);
    assert_eq!(
        lost.body_build.as_ref().unwrap().personal_body_grants,
        receipts
    );
    let encoded = serde_json::to_value(super::record::BuildRecord::capture(&lost)).unwrap();
    let recorded: super::record::BuildRecord = serde_json::from_value(encoded).unwrap();
    let restored = recorded.restore(lost.content().clone()).unwrap();
    assert_eq!(restored, lost);
    assert_eq!(
        historical(&restored, false)
            .unwrap()
            .attribute_increases
            .willpower,
        2
    );
    assert_eq!(
        historical(&restored, true).unwrap_err(),
        BuildError::InvalidOwnership
    );
    let drained_sources: Vec<_> = restored
        .ledger()
        .entries()
        .iter()
        .map(|die| die.source())
        .collect();
    let drained_history = super::intellect::historical_sources_refs(
        &restored,
        restored.initial_attributes(),
        &[],
        0,
        drained_sources
            .iter()
            .zip(restored.ledger().entries())
            .zip(restored.choices())
            .map(|((source, die), choices)| (source, choices, die.owner())),
    )
    .unwrap();
    assert_eq!(drained_history.attribute_increases.willpower, 1);

    // Birth retains both original receipts, while current donor loss suppresses
    // only the second. Reopening validates each historical snapshot separately.
    assert_eq!(
        restored
            .intellect_learning
            .as_ref()
            .unwrap()
            .birth
            .sources
            .attribute_increases
            .willpower,
        2
    );
    assert_eq!(
        super::intellect::current_sources(&restored, &[])
            .unwrap()
            .attribute_increases
            .willpower,
        1
    );
    restored
        .intellect_learning
        .as_ref()
        .unwrap()
        .validate(&restored)
        .unwrap();

    assert_eq!(
        restored
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Willpower),
        4
    );
    assert_eq!(restored.derive().unwrap().skills.get(Skill::Breath), 1);
    let before = lost.clone();
    assert_eq!(
        lost.replace_body_package(small),
        Err(BuildError::InvalidOwnership),
        "existing cross-package loss boundary must not launder donor loss"
    );
    assert_eq!(lost, before);
    assert!(lost.restore_owned_package_hit_die().unwrap());
    assert_eq!(
        lost.derive().unwrap().attributes.get(Attribute::Willpower),
        5
    );
    assert_eq!(lost.derive().unwrap().skills.get(Skill::Breath), 2);
    assert_eq!(
        lost.body_build.as_ref().unwrap().personal_body_grants,
        receipts
    );
    let reopened = super::record::BuildRecord::capture(&lost)
        .restore(lost.content().clone())
        .unwrap();
    assert_eq!(reopened, lost);
    assert_eq!(
        super::intellect::current_sources(&reopened, &[])
            .unwrap()
            .attribute_increases
            .willpower,
        2
    );
    let visitor_before = visitor.derive().unwrap();
    let visiting = visitor
        .replace_body_package(base)
        .unwrap()
        .derive()
        .unwrap();
    assert_eq!(
        visiting.attributes.get(Attribute::Willpower),
        visitor_before.attributes.get(Attribute::Willpower)
    );
    assert_eq!(
        visiting.skills.get(Skill::Breath),
        visitor_before.skills.get(Skill::Breath)
    );
}

fn retained_physical_cap_fixture() -> (
    CreatureBuild,
    Arc<BodyPackage>,
    Arc<BodyPackage>,
    Arc<BodyPackage>,
) {
    let mut high = crate::test_creatures::unmodified_species("cap_high");
    high.default_attributes = Attributes::new(ByAttribute {
        strength: 4,
        speed: 4,
        intellect: 1,
        willpower: 1,
        awareness: 1,
        presence: 1,
    })
    .unwrap();
    let mut weak = high.clone();
    weak.id = SpeciesId::new("cap_weak").unwrap();
    let mut values = weak.default_attributes.values();
    values.strength = 1;
    values.speed = 1;
    weak.default_attributes = Attributes::new(values).unwrap();
    let mut innate = high.clone();
    innate.id = SpeciesId::new("cap_innate").unwrap();
    innate.modifiers.push(crate::test_creatures::add(
        crate::rules::Stat::Attribute {
            attribute: Attribute::Strength,
        },
        2,
    ));
    let content = Content::new(
        crate::test_creatures::ruleset(),
        ScenarioCatalog {
            species: [high.clone(), weak.clone(), innate.clone()]
                .into_iter()
                .map(|s| (s.id.clone(), s))
                .collect(),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let package = |species: SpeciesId| {
        Arc::new(BodyPackage {
            id: BodyFormId::new(species.as_str()).unwrap(),
            species,
            body: crate::BodySpec::default(),
            advancement: vec![PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            }],
        })
    };
    let high = package(high.id);
    let weak = package(weak.id);
    let innate = package(innate.id);
    let content = Content::with_body_packages(
        &content,
        [high.clone(), weak.clone(), innate.clone()]
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect(),
    )
    .unwrap();
    let build = CreatureBuild::new(
        content,
        high.species.clone(),
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            42,
        )
        .unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
    .with_body_package(high.clone(), 42)
    .unwrap();
    (build, high, weak, innate)
}

#[test]
fn retained_personal_physical_gains_cap_without_erasing_ownership_or_allowing_hidden_purchases() {
    let (build, high, weak, _) = retained_physical_cap_fixture();
    let warrior = HdSource::Class(ClassId::new("warrior").unwrap());
    let mut trained = build.replace_body_package(weak.clone()).unwrap();
    for _ in 1..16 {
        trained.add_hit_die(warrior.clone(), 42).unwrap();
    }
    for (ordinal, attribute) in [
        (4, Attribute::Strength),
        (8, Attribute::Strength),
        (12, Attribute::Speed),
        (16, Attribute::Speed),
    ] {
        trained
            .increase_attribute(crate::HdIndex(ordinal), attribute)
            .unwrap();
    }
    trained.train(crate::HdIndex(4), Skill::Lore).unwrap();
    assert_eq!(
        trained
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        3
    );
    assert_eq!(
        trained.derive().unwrap().attributes.get(Attribute::Speed),
        3
    );
    let personal = trained.ledger().entries()[1..].to_vec();
    let choices = trained.choices()[1..].to_vec();
    let capped = trained.replace_body_package(high.clone()).unwrap();
    assert_eq!(
        capped.derive().unwrap().attributes.get(Attribute::Strength),
        5
    );
    assert_eq!(capped.derive().unwrap().attributes.get(Attribute::Speed), 5);
    assert_eq!(&capped.ledger().entries()[1..], personal.as_slice());
    assert_eq!(&capped.choices()[1..], choices.as_slice());
    let bytes = serde_json::to_vec(&super::record::BuildRecord::capture(&capped)).unwrap();
    let record: super::record::BuildRecord = serde_json::from_slice(&bytes).unwrap();
    let reopened = record.restore(capped.content().clone()).unwrap();
    assert_eq!(reopened, capped);
    let mut exposed = reopened.replace_body_package(weak.clone()).unwrap();
    assert_eq!(
        exposed
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        3
    );
    assert_eq!(
        exposed.derive().unwrap().attributes.get(Attribute::Speed),
        3
    );
    assert_eq!(&exposed.choices()[1..], choices.as_slice());
    let removed = exposed.remove_latest().unwrap();
    assert_eq!(removed.choices.attribute, Some(Attribute::Speed));
    assert_eq!(
        exposed.derive().unwrap().attributes.get(Attribute::Speed),
        2
    );
    exposed.add_hit_die(warrior.clone(), 42).unwrap();
    assert_eq!(exposed.ledger().entries()[16], removed.hit_die);
    assert_eq!(
        exposed.choices()[16].attribute,
        None,
        "ChooseAgain really removes the old gain"
    );
    exposed
        .increase_attribute(crate::HdIndex(16), Attribute::Speed)
        .unwrap();
    assert_eq!(
        exposed.derive().unwrap().attributes.get(Attribute::Speed),
        3
    );
    assert_eq!(exposed.choices()[4].purchases, choices[3].purchases);
    let mut saturated = exposed.replace_body_package(high).unwrap();
    for _ in 16..20 {
        saturated.add_hit_die(warrior.clone(), 42).unwrap();
    }
    for attribute in [Attribute::Strength, Attribute::Speed] {
        let before = saturated.clone();
        assert_eq!(
            saturated.increase_attribute(crate::HdIndex(20), attribute),
            Err(BuildError::AttributeCap)
        );
        assert_eq!(
            saturated, before,
            "no hidden purchase beyond the current natural cap"
        );
    }
    let lower = saturated.replace_body_package(weak).unwrap();
    assert_eq!(
        lower.derive().unwrap().attributes.get(Attribute::Strength),
        3
    );
    assert_eq!(lower.derive().unwrap().attributes.get(Attribute::Speed), 3);
}

#[test]
fn retained_magic_strength_caps_before_innate_and_survives_reopen_drain_and_revisit() {
    let (build, high, weak, innate) = retained_physical_cap_fixture();
    let mut changed = build.replace_body_package(weak.clone()).unwrap();
    for _ in 0..6 {
        assert!(changed.acquire_strength().unwrap());
    }
    assert_eq!(changed.strength_acquired(), 6);
    assert_eq!(
        changed
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        7
    );
    let before = changed.clone();
    assert!(!changed.acquire_strength().unwrap());
    assert_eq!(changed, before);
    let mut capped = changed.replace_body_package(high.clone()).unwrap();
    assert_eq!(capped.strength_acquired(), 6);
    assert_eq!(
        capped.derive().unwrap().attributes.get(Attribute::Strength),
        7
    );
    let before = capped.clone();
    assert!(!capped.acquire_strength().unwrap());
    assert_eq!(capped, before);
    let record = super::record::BuildRecord::capture(&capped);
    let mut reopened = record.restore(capped.content().clone()).unwrap();
    assert_eq!(reopened, capped);
    let removed = reopened.remove_latest().unwrap();
    assert_eq!(reopened.strength_acquired(), 6);
    reopened.add_hit_die(removed.hit_die.source(), 42).unwrap();
    assert_eq!(reopened.ledger().entries()[1], removed.hit_die);
    assert_eq!(reopened.strength_acquired(), 6);
    let innate = reopened.replace_body_package(innate).unwrap();
    assert_eq!(
        innate.derive().unwrap().attributes.get(Attribute::Strength),
        9,
        "innate +2 is applied after the natural Strength7 cap"
    );
    let weak_again = innate.replace_body_package(weak).unwrap();
    assert_eq!(
        weak_again
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        7
    );
    assert_eq!(weak_again.strength_acquired(), 6);
    let returned = weak_again.replace_body_package(high).unwrap();
    assert_eq!(
        returned
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        7
    );
    assert_eq!(returned.strength_acquired(), 6);
}

#[test]
fn unowned_overcap_attribute_and_magic_records_remain_invalid() {
    let (owned, high, _, _) = retained_physical_cap_fixture();
    let mut ordinary = CreatureBuild::new(
        owned.content().clone(),
        high.species.clone(),
        HdLedger::seeded(
            vec![HdSource::Class(ClassId::new("warrior").unwrap()); 8],
            91,
        )
        .unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    ordinary
        .increase_attribute(crate::HdIndex(3), Attribute::Strength)
        .unwrap();
    assert_eq!(
        ordinary
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        5
    );
    let valid = super::record::BuildRecord::capture(&ordinary);
    assert_eq!(valid.restore(ordinary.content().clone()).unwrap(), ordinary);
    let before = ordinary.clone();
    assert_eq!(
        ordinary.increase_attribute(crate::HdIndex(7), Attribute::Strength),
        Err(BuildError::AttributeCap)
    );
    assert_eq!(ordinary, before);
    let mut encoded = serde_json::to_value(valid).unwrap();
    encoded["choices"][7]["attribute"] = serde_json::json!("strength");
    let forged: super::record::BuildRecord = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        forged.restore(ordinary.content().clone()).unwrap_err(),
        BuildError::AttributeCap
    );
    assert!(ordinary.acquire_strength().unwrap());
    assert!(ordinary.acquire_strength().unwrap());
    assert_eq!(
        ordinary
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        7
    );
    let valid = super::record::BuildRecord::capture(&ordinary);
    assert_eq!(valid.restore(ordinary.content().clone()).unwrap(), ordinary);
    let before = ordinary.clone();
    assert!(!ordinary.acquire_strength().unwrap());
    assert_eq!(ordinary, before);
    let mut encoded = serde_json::to_value(valid).unwrap();
    encoded["strength_acquired"] = serde_json::json!(3);
    let forged: super::record::BuildRecord = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        forged.restore(ordinary.content().clone()).unwrap_err(),
        BuildError::AttributeCap
    );
}

#[test]
fn game_cancellation_returns_with_retained_magic_cap_without_healing_or_pending_return() {
    let (build, high, weak, _) = retained_physical_cap_fixture();
    let content = build.content().clone();
    let mut game = crate::Game::two_room_in_stone(42, content.clone());
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "cap subject".into(),
            faction: "fixture".into(),
        },
        build,
    )
    .unwrap();
    assert_eq!(
        game.polymorph_creature(actor, weak.clone()).unwrap(),
        crate::BodyTransitionResult::Changed
    );
    for _ in 0..6 {
        assert!(game
            .apply_effects(actor, &[crate::rules::Effect::GainStrength])
            .unwrap());
    }
    assert!(!game
        .apply_effects(actor, &[crate::rules::Effect::GainStrength])
        .unwrap());
    assert_eq!(game.creature(actor).unwrap().build().strength_acquired(), 6);
    crate::test_creatures::injure(&mut game, actor, 1).unwrap();
    assert!(game
        .apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
        .unwrap());
    let creature = game.creature(actor).unwrap();
    assert_eq!(creature.build().active_body_package().unwrap().id, high.id);
    assert_eq!(creature.derived().attributes.get(Attribute::Strength), 7);
    assert_eq!(creature.build().strength_acquired(), 6);
    assert_eq!(creature.health().injury(), 1);
    assert!(creature.cancelled());
    assert!(!creature.polymorph_return_pending());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    let mut game = crate::Game::restore_checkpoint(snapshot, &shared, content).unwrap();
    assert_eq!(game.creature(actor).unwrap().build().strength_acquired(), 6);
    assert_eq!(
        game.polymorph_creature(actor, weak).unwrap(),
        crate::BodyTransitionResult::Changed
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .derived()
            .attributes
            .get(Attribute::Strength),
        7
    );
    assert!(game
        .apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
        .unwrap());
    assert_eq!(game.creature(actor).unwrap().health().injury(), 1);
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .build()
            .active_body_package()
            .unwrap()
            .id,
        high.id
    );
    assert!(!game.creature(actor).unwrap().polymorph_return_pending());
}

#[test]
fn owned_custom_ordinary_cap_without_magic_preserves_unowned_strength_above_seven() {
    let (original, base, low, _) = fixture();
    let mut document = original.ruleset().document().clone();
    document.constants.attribute_cap = 10;
    document.constants.derived_attribute_cap = document.constants.derived_attribute_cap.max(10);
    document.constants.attribute_opportunity_interval = 1;
    let mut catalog = original.content().catalog().clone();
    let species = catalog.species.get_mut(original.species_id()).unwrap();
    let mut attributes = species.default_attributes.values();
    attributes.strength = 8;
    species.default_attributes = Attributes::new(attributes).unwrap();
    let content = Content::new(Ruleset::validate(document).unwrap(), catalog).unwrap();
    let mut ordinary = CreatureBuild::new(
        content.clone(),
        original.species_id().clone(),
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            42,
        )
        .unwrap(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    let mut owned = ordinary.with_body_package(base.clone(), 42).unwrap();
    for build in [&mut ordinary, &mut owned] {
        assert_eq!(build.strength_acquired(), 0);
        assert_eq!(
            build.derive().unwrap().attributes.get(Attribute::Strength),
            8
        );
        build
            .increase_attribute(crate::HdIndex(1), Attribute::Strength)
            .unwrap();
        assert_eq!(
            build.derive().unwrap().attributes.get(Attribute::Strength),
            9
        );
        let before = build.clone();
        assert!(!build.acquire_strength().unwrap());
        assert_eq!(*build, before);
        let encoded = serde_json::to_vec(&super::record::BuildRecord::capture(build)).unwrap();
        let decoded: super::record::BuildRecord = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.restore(build.content().clone()).unwrap(), *build);
    }

    // A lawful magical receipt from a weaker body must not debuff a custom
    // ordinary subtotal above seven. Its contribution returns with headroom.
    let mut low_package = low.as_ref().clone();
    low_package.advancement.truncate(1);
    let low_package = Arc::new(low_package);
    let mut native = CreatureBuild::new(
        content,
        low_package.species.clone(),
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Class(ClassId::new("warrior").unwrap()),
            ],
            91,
        )
        .unwrap(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap()
    .with_body_package(low_package.clone(), 91)
    .unwrap();
    assert!(native.acquire_strength().unwrap());
    assert!(native.acquire_strength().unwrap());
    assert_eq!(native.strength_acquired(), 2);
    assert_eq!(
        native.derive().unwrap().attributes.get(Attribute::Strength),
        6
    );
    let mut high = native.replace_body_package(base).unwrap();
    assert_eq!(high.strength_acquired(), 2);
    assert_eq!(
        high.derive().unwrap().attributes.get(Attribute::Strength),
        8
    );
    assert_eq!(high.ledger().entries()[1], native.ledger().entries()[1]);
    let before = high.clone();
    assert!(!high.acquire_strength().unwrap());
    assert_eq!(high, before);
    assert_eq!(
        high.increase_attribute(crate::HdIndex(1), Attribute::Strength),
        Err(BuildError::AttributeCap)
    );
    assert_eq!(high, before);
    let encoded = serde_json::to_vec(&super::record::BuildRecord::capture(&high)).unwrap();
    let decoded: super::record::BuildRecord = serde_json::from_slice(&encoded).unwrap();
    let restored = decoded.restore(high.content().clone()).unwrap();
    assert_eq!(restored, high);
    let returned = restored.replace_body_package(low_package).unwrap();
    assert_eq!(returned.strength_acquired(), 2);
    assert_eq!(
        returned
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        6
    );
    assert_eq!(returned.ledger(), native.ledger());
    assert_eq!(returned.choices(), native.choices());
}

#[test]
fn authored_restoration_reunites_six_lawful_physical_receipts_without_forging_birth() {
    let (original, base, _, _) = fixture();
    let mut catalog = original.content().catalog().clone();
    let species = catalog.species.get_mut(original.species_id()).unwrap();
    let mut attributes = species.default_attributes.values();
    attributes.strength = 0;
    species.default_attributes = Attributes::new(attributes).unwrap();
    let content = Content::new(original.content().rules().clone(), catalog).unwrap();
    let mut sources = vec![HdSource::Racial];
    sources.extend(vec![HdSource::Class(ClassId::new("warrior").unwrap()); 24]);
    let ledger = HdLedger::seeded(sources.clone(), 42).unwrap();
    let mut choices = vec![HdChoices::default(); 25];
    for index in [4, 12, 16, 20, 24] {
        choices[index].attribute = Some(Attribute::Strength);
    }
    let authored = |choices: Vec<HdChoices>| {
        let plan = sources
            .iter()
            .cloned()
            .zip(choices.iter().cloned())
            .map(|(source, choices)| PlannedAdvancement { source, choices })
            .collect();
        CreatureBuild::from_recorded_body_package(
            content.clone(),
            base.clone(),
            Attributes::new(attributes).unwrap(),
            ledger.clone(),
            original.binding(),
            BuildAdvancement {
                starting_skills: *original.starting_skills(),
                restoration: RestorationPolicy::Authored(plan),
                choices,
            },
            42,
        )
    };
    let mut build = authored(choices.clone()).unwrap();
    assert_eq!(build.personal_hd_count(), crate::HdCount(24));
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        5
    );
    let birth_record = serde_json::to_value(super::record::BuildRecord::capture(&build)).unwrap();
    let positive: super::record::BuildRecord =
        serde_json::from_value(birth_record.clone()).unwrap();
    assert_eq!(positive.restore(content.clone()).unwrap(), build);
    choices[8].attribute = Some(Attribute::Strength);
    assert_eq!(authored(choices).unwrap_err(), BuildError::AttributeCap);
    let donor = build.ledger().entries()[24].clone();
    let donor_choices = build.choices()[24].clone();
    build.remove_latest().unwrap();
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        4
    );
    build
        .increase_attribute(crate::HdIndex(8), Attribute::Strength)
        .unwrap();
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        5
    );
    build.restore_hit_die(42).unwrap();
    assert_eq!(build.ledger().entries()[24], donor);
    assert_eq!(build.choices()[24], donor_choices);
    assert_eq!(
        build
            .choices()
            .iter()
            .filter(|row| row.attribute == Some(Attribute::Strength))
            .count(),
        6
    );
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        5
    );
    assert_eq!(build.strength_acquired(), 0);
    assert_eq!(
        build
            .intellect_learning
            .as_ref()
            .unwrap()
            .birth
            .sources
            .attribute_increases
            .strength,
        5
    );
    assert_eq!(
        super::intellect::current_sources(&build, &[])
            .unwrap()
            .attribute_increases
            .strength,
        6
    );
    let record = super::record::BuildRecord::capture(&build);
    let restored = record.restore(content.clone()).unwrap();
    assert_eq!(restored, build);
    // Reopening retained state is legal; recasting that same six-receipt
    // subtotal as a brand-new birth is not, even outside source enrollment.
    assert_eq!(
        super::expertise_learning::IntellectLearning::new(
            &restored,
            super::intellect::current_sources(&restored, &[]).unwrap(),
            restored.derive().unwrap().intellect_capacity,
        )
        .unwrap_err(),
        BuildError::AttributeCap
    );
    build.remove_latest().unwrap();
    assert_eq!(
        build
            .choices()
            .iter()
            .filter(|row| row.attribute == Some(Attribute::Strength))
            .count(),
        5
    );
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        5
    );

    // This is a coherent forged six-at-birth record, not a mismatched counter:
    // original choice, authored receipt and historical row all agree. Fresh
    // birth admission must still reject it despite retained effective clipping.
    let mut forged = birth_record.clone();
    forged["choices"][8]["attribute"] = serde_json::json!("strength");
    for row in forged["body_build"]["personal_restoration"]["entries"]
        .as_array_mut()
        .unwrap()
    {
        if row["id"] == serde_json::json!(8) {
            row["choices"]["attribute"] = serde_json::json!("strength");
        }
    }
    let history_index = forged["intellect_learning"]["birth"]["choices"][8]
        .as_u64()
        .unwrap() as usize;
    forged["intellect_learning"]["history"][history_index]["choices"]["attribute"] =
        serde_json::json!("strength");
    forged["intellect_learning"]["birth"]["sources"]["attribute_increases"]["strength"] =
        serde_json::json!(6);
    let forged: super::record::BuildRecord = serde_json::from_value(forged).unwrap();
    assert_eq!(
        forged.restore(content).unwrap_err(),
        BuildError::AttributeCap
    );
    for missing in [false, true] {
        let mut bad = birth_record.clone();
        if missing {
            bad.as_object_mut().unwrap().remove("intellect_learning");
        } else {
            bad["intellect_learning"] = serde_json::Value::Null;
        }
        assert!(serde_json::from_value::<super::record::BuildRecord>(bad).is_err());
    }
}

fn scheduled_fixture() -> CreatureBuild {
    let (seed, base, _, _) = fixture();
    let warrior = HdSource::Class(ClassId::new("warrior").unwrap());
    let ledger = HdLedger::seeded(vec![HdSource::Racial, warrior.clone()], 42).unwrap();
    let train = |skill| HdChoices {
        purchase_highwater: 0,
        purchases: vec![AdvancementPurchase {
            id: None,
            pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: PurchaseSelection::SkillRank(skill),
        }],
        attribute: None,
    };
    let choices = vec![HdChoices::default(), train(Skill::Lore)];
    let mut plan: Vec<_> = ledger
        .entries()
        .iter()
        .zip(&choices)
        .map(|(die, choices)| PlannedAdvancement {
            source: die.source(),
            choices: choices.clone(),
        })
        .collect();
    for skill in [Skill::Athletics, Skill::Perception] {
        plan.push(PlannedAdvancement {
            source: warrior.clone(),
            choices: train(skill),
        });
    }
    let mut build = CreatureBuild::from_recorded_body_package(
        seed.content().clone(),
        base,
        seed.initial_attributes(),
        ledger,
        seed.binding(),
        BuildAdvancement {
            starting_skills: *seed.starting_skills(),
            choices,
            restoration: RestorationPolicy::Authored(plan),
        },
        42,
    )
    .unwrap();
    build
        .set_future_growth(crate::earned_growth::FutureGrowth::Authored {})
        .unwrap();
    build
}

fn trained(build: &CreatureBuild, index: usize) -> Vec<PurchaseSelection> {
    build.choices()[index]
        .purchases
        .iter()
        .map(|purchase| purchase.selection.clone())
        .collect()
}

#[test]
fn authored_growth_schedule_grows_scheduled_rows_in_order_then_holds_xp() {
    let build = scheduled_fixture();
    let warrior = HdSource::Class(ClassId::new("warrior").unwrap());
    assert_eq!(build.ledger().entries().len(), 2);
    assert_eq!(
        build.next_authored_restoration_source(),
        Some(warrior.clone())
    );
    let encoded = serde_json::to_vec(&super::record::BuildRecord::capture(&build)).unwrap();
    let decoded: super::record::BuildRecord = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.restore(build.content().clone()).unwrap(), build);

    let mut state = CreatureState::new(build).unwrap();
    state.award_xp(100).unwrap();
    assert_eq!(state.build().ledger().entries().len(), 3);
    assert_eq!(state.build().ledger().entries()[2].source(), warrior);
    assert_eq!(
        trained(state.build(), 2),
        [PurchaseSelection::SkillRank(Skill::Athletics)]
    );
    assert_eq!(state.build().unspent_points(), 0);

    // A grown scheduled row and a not-yet-grown row both survive a saved record.
    let encoded = serde_json::to_vec(&super::record::BuildRecord::capture(state.build())).unwrap();
    let decoded: super::record::BuildRecord = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        &decoded.restore(state.build().content().clone()).unwrap(),
        state.build()
    );

    state.award_xp(10_000).unwrap();
    assert_eq!(state.build().ledger().entries().len(), 4);
    assert_eq!(
        trained(state.build(), 3),
        [PurchaseSelection::SkillRank(Skill::Perception)]
    );
    assert_eq!(state.build().next_authored_restoration_source(), None);
    assert!(
        state.earned_progress().held_xp() > 0,
        "an exhausted schedule retains XP"
    );
}

#[test]
fn drained_scheduled_row_regrows_with_its_scheduled_purchase() {
    let mut state = CreatureState::new(scheduled_fixture()).unwrap();
    state.award_xp(100).unwrap();
    let grown = state.build().ledger().entries()[2].clone();
    state.drain_latest_hit_die().unwrap();
    assert_eq!(state.build().ledger().entries().len(), 2);
    state.award_xp(10_000).unwrap();
    assert_eq!(state.build().ledger().entries()[2], grown);
    assert_eq!(
        trained(state.build(), 2),
        [PurchaseSelection::SkillRank(Skill::Athletics)]
    );
    assert_eq!(
        trained(state.build(), 3),
        [PurchaseSelection::SkillRank(Skill::Perception)]
    );
}

#[test]
fn inadmissible_growth_schedule_rejects_the_whole_build() {
    let (seed, base, _, _) = fixture();
    let warrior = HdSource::Class(ClassId::new("warrior").unwrap());
    let ledger = HdLedger::seeded(vec![HdSource::Racial, warrior.clone()], 42).unwrap();
    let mut plan = vec![
        PlannedAdvancement {
            source: HdSource::Racial,
            choices: HdChoices::default(),
        },
        PlannedAdvancement {
            source: warrior.clone(),
            choices: HdChoices::default(),
        },
    ];
    // Two purchases cannot be funded by one scheduled class point.
    let purchase = |skill| AdvancementPurchase {
        id: None,
        pool: crate::content_ids::PointPoolId::new("advancement").unwrap(),
        selection: PurchaseSelection::SkillRank(skill),
    };
    plan.push(PlannedAdvancement {
        source: warrior,
        choices: HdChoices {
            purchase_highwater: 0,
            purchases: vec![purchase(Skill::Athletics), purchase(Skill::Perception)],
            attribute: None,
        },
    });
    assert!(CreatureBuild::from_recorded_body_package(
        seed.content().clone(),
        base,
        seed.initial_attributes(),
        ledger,
        seed.binding(),
        BuildAdvancement {
            starting_skills: *seed.starting_skills(),
            choices: vec![HdChoices::default(), HdChoices::default()],
            restoration: RestorationPolicy::Authored(plan),
        },
        42,
    )
    .is_err());
}
