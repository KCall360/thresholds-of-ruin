//! Nonrefundable consumable provenance versus ordinary HD ownership.
use super::*;
use crate::attributes::{Attribute, Attributes, ByAttribute};
use crate::content_ids::{PointPoolId, TalentId};
use crate::rules::{Requirement, Ruleset, Stat};
fn fixture(strength: u16, innate: i64) -> CreatureBuild {
    let mut species = crate::test_creatures::species();
    species.default_attributes = Attributes::new(ByAttribute {
        strength,
        intellect: 1,
        ..ByAttribute::default()
    })
    .unwrap();
    species.modifiers.push(crate::test_creatures::add(
        Stat::Attribute {
            attribute: Attribute::Strength,
        },
        innate,
    ));
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.constants.attribute_opportunity_interval = 1;
    let mut talent = crate::test_creatures::talent(
        "fixture_strength_intellect",
        vec![crate::test_creatures::add(
            Stat::Attribute {
                attribute: Attribute::Intellect,
            },
            1,
        )],
    );
    talent.requirement = Requirement::AttributeMinimum {
        attribute: Attribute::Strength,
        minimum: 5,
    };
    document.talents.push(talent);
    let id = species.id.clone();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: [(id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    CreatureBuild::new(
        content,
        id,
        HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
}
fn restore(build: &CreatureBuild) -> CreatureBuild {
    let value = serde_json::to_value(record::BuildRecord::capture(build)).unwrap();
    let record: record::BuildRecord = serde_json::from_value(value).unwrap();
    record.restore(build.content().clone()).unwrap()
}
#[test]
fn strength_dual_caps_allow_ordinary_purchase_below_seven_and_reject_eight_atomically() {
    let mut allowed = fixture(4, 0);
    assert!(allowed.acquire_strength().unwrap());
    assert_eq!(
        allowed
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        5
    );
    allowed
        .increase_attribute(crate::HdIndex(0), Attribute::Strength)
        .unwrap();
    assert_eq!(
        allowed
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        6
    );
    assert_eq!(allowed.strength_acquired(), 1);
    assert_eq!(restore(&allowed), allowed);
    let before = allowed.clone();
    assert!(allowed
        .increase_attribute(crate::HdIndex(1), Attribute::Strength)
        .is_err());
    assert_eq!(allowed, before);
    let mut capped = fixture(4, 0);
    for _ in 0..3 {
        assert!(capped.acquire_strength().unwrap());
    }
    let before = capped.clone();
    assert!(capped
        .increase_attribute(crate::HdIndex(0), Attribute::Strength)
        .is_err());
    assert_eq!(capped, before);
    assert!(!capped.acquire_strength().unwrap());
}
#[test]
fn strength_innate_folds_after_magic_and_baseline_rewrite_cannot_rebase_birth() {
    let mut build = fixture(4, 2);
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        6
    );
    for _ in 0..3 {
        assert!(build.acquire_strength().unwrap());
    }
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        9
    );
    assert!(!build.acquire_strength().unwrap());
    build
        .set_initial_attributes(build.initial_attributes())
        .unwrap();
    let before = build.clone();
    let mut changed = build.initial_attributes().values();
    changed.strength = 3;
    assert!(build
        .set_initial_attributes(Attributes::new(changed).unwrap())
        .is_err());
    assert_eq!(build, before);
    assert_eq!(restore(&build), build);
}
#[test]
fn strength_acquisition_receipts_preserve_history_and_refund_only_owned_purchase() {
    let mut build = fixture(4, 0);
    let talent = TalentId::new("fixture_strength_intellect").unwrap();
    build
        .purchase(
            crate::HdIndex(0),
            PointPoolId::new("advancement").unwrap(),
            PurchaseSelection::Talent(talent.clone()),
        )
        .unwrap();
    assert!(!build.derive().unwrap().active_talents.contains(&talent));
    assert_eq!(build.derive().unwrap().intellect_capacity, 1);
    assert!(build.acquire_strength().unwrap());
    assert!(build.derive().unwrap().active_talents.contains(&talent));
    assert_eq!(build.derive().unwrap().intellect_capacity, 2);
    assert_eq!(restore(&build), build);
    let value = serde_json::to_value(record::BuildRecord::capture(&build)).unwrap();
    assert_eq!(value["strength_acquired"], 1);
    let receipt = &value["intellect_learning"]["acquisitions"][0];
    assert_eq!(receipt["before"]["strength_acquired"], 0);
    assert_eq!(receipt["after"]["strength_acquired"], 1);
    assert_eq!(receipt["before_choices"], receipt["after_choices"]);
    let mut forged = value.clone();
    // The birth row is an independently valid empty purchase version of the
    // SAME owner/source/Health seed. At Strength four its Intellect sources
    // match the dormant-talent version; only the new no-rewrite rule rejects
    // pretending that the dose also purchased the talent.
    let original_row = value["intellect_learning"]["birth"]["choices"][0].clone();
    forged["intellect_learning"]["acquisitions"][0]["before_choices"][0] = original_row;
    assert_ne!(forged, value);
    let decoded: record::BuildRecord = serde_json::from_value(forged).unwrap();
    assert!(decoded.restore(build.content().clone()).is_err());
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: crate::HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    assert_eq!(
        preview.refunds().iter().map(|row| row.points).sum::<u16>(),
        1
    );
    assert_eq!(preview.build().strength_acquired(), 1);
    build.commit_replacement(preview).unwrap();
    assert_eq!(build.strength_acquired(), 1);
    assert_eq!(
        build.derive().unwrap().attributes.get(Attribute::Strength),
        5
    );
    assert_eq!(restore(&build), build);
}
#[test]
fn strength_saved_provenance_is_required_bounded_and_not_birth_seeded() {
    let mut build = fixture(5, 0);
    assert!(build.acquire_strength().unwrap());
    let value = serde_json::to_value(record::BuildRecord::capture(&build)).unwrap();
    assert_eq!(restore(&build), build);
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("strength_acquired");
    assert!(serde_json::from_value::<record::BuildRecord>(missing).is_err());
    for count in [3, 8] {
        let mut invalid = value.clone();
        invalid["strength_acquired"] = count.into();
        let decoded: record::BuildRecord = serde_json::from_value(invalid).unwrap();
        assert!(decoded.restore(build.content().clone()).is_err());
    }
    let mut invalid = value;
    invalid["intellect_learning"]["birth"]["sources"]["strength_acquired"] = 1.into();
    let decoded: record::BuildRecord = serde_json::from_value(invalid).unwrap();
    assert!(decoded.restore(build.content().clone()).is_err());
}
#[test]
fn strength_equipment_bonus_and_penalty_never_seed_or_bank_permanent_points() {
    let mut state = CreatureState::new(fixture(5, 0)).unwrap();
    let equipment = |bonus| {
        vec![EquipmentSource {
            item: crate::ItemId(7),
            socket: crate::EquipmentSlotId(0),
            attack: None,
            weapon_profile: None,
            modifiers: vec![crate::test_creatures::add(
                Stat::Attribute {
                    attribute: Attribute::Strength,
                },
                bonus,
            )],
        }]
    };
    state.set_equipment(equipment(10)).unwrap();
    assert_eq!(state.derived().attributes.get(Attribute::Strength), 15);
    assert_eq!(
        state.stage_gain_strength().unwrap(),
        crate::abilities::StrengthGain::Acquired
    );
    assert_eq!(state.derived().attributes.get(Attribute::Strength), 16);
    assert_eq!(
        state.stage_gain_strength().unwrap(),
        crate::abilities::StrengthGain::Acquired
    );
    state.set_equipment(equipment(-5)).unwrap();
    assert_eq!(state.derived().attributes.get(Attribute::Strength), 2);
    assert_eq!(state.restorable_losses().attributes.strength, 0);
    assert_eq!(
        state.stage_gain_strength().unwrap(),
        crate::abilities::StrengthGain::Unchanged
    );
    state.set_equipment(vec![]).unwrap();
    assert_eq!(state.derived().attributes.get(Attribute::Strength), 7);
    assert_eq!(state.build().strength_acquired(), 2);
}
#[test]
fn strength_survives_drain_regrowth_and_raise_uses_ordinary_finalization() {
    use crate::earned_growth::{FutureGrowth, GrowthSource};
    let mut build = fixture(4, 0);
    build
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    assert!(build.acquire_strength().unwrap());
    let mut state = CreatureState::new(build).unwrap();
    let original = state.build().ledger().total_hd();
    assert_eq!(state.drain_latest_hit_die().unwrap().applied, 1);
    assert_eq!(state.build().strength_acquired(), 1);
    let price = state
        .earned_progress()
        .next_threshold(&state.build().ruleset().document().growth)
        .unwrap();
    assert_eq!(state.stage_raise_level().unwrap(), price);
    assert_eq!(
        state.build().ledger().total_hd(),
        original.checked_sub(crate::HdCount(1)).unwrap()
    );
    // Public rebuild owns ordinary convergence, rather than a direct HD insertion.
    state.rebuild(state.build().clone()).unwrap();
    assert_eq!(state.build().ledger().total_hd(), original);
    assert_eq!(state.build().strength_acquired(), 1);
    assert_eq!(state.earned_progress().held_xp(), 0);
    assert_eq!(state.earned_progress().spent_xp(), price);
    assert_eq!(restore(state.build()), *state.build());
}
