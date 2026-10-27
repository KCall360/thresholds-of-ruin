//! Valid authored ownership conflicts bank XP until a retained edit clears them.
use super::record::{BuildRecord, CreatureRecord};
use super::{
    AdvancementPurchase, BuildAdvancement, BuildError, Content, CreatureBuild, CreatureState,
    CreatureStateError, HdChoices, PlannedAdvancement, PurchaseLocation, PurchaseSelection,
    RestorationPolicy,
};
use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use crate::content_ids::{ClassId, PointPoolId, TalentId};
use crate::earned_growth::{FutureGrowth, GrowthSource};
use crate::progression::{HdLedger, HdSource};
use crate::rules::{Ruleset, TalentDef};

const HEALTH_ROOT: u64 = 42;

fn purchase(selection: PurchaseSelection) -> AdvancementPurchase {
    AdvancementPurchase {
        id: None,
        pool: PointPoolId::new("advancement").unwrap(),
        selection,
    }
}

fn strength(value: u16) -> Attributes {
    Attributes::new(ByAttribute {
        strength: value,
        ..ByAttribute::default()
    })
    .unwrap()
}

fn talent(index: usize) -> TalentId {
    TalentId::new(format!("fixture_growth_conflict_{index:03}")).unwrap()
}

fn authored(
    choices: Vec<HdChoices>,
    attributes: Attributes,
    racial_points: u16,
    talents: Vec<TalentDef>,
) -> CreatureBuild {
    let fixture = crate::test_creatures::build(crate::test_creatures::species());
    let mut document = fixture.ruleset().document().clone();
    document.constants.racial_point_grants[0].points = racial_points;
    document.talents.extend(talents);
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        fixture.content().catalog().clone(),
    )
    .unwrap();
    let original = CreatureBuild::from_recorded(
        content,
        fixture.species_id().clone(),
        attributes,
        HdLedger::seeded(vec![HdSource::Racial; choices.len()], HEALTH_ROOT).unwrap(),
        ManaBinding::Intellect,
        BuildAdvancement {
            starting_skills: [Skill::HeavyWeaponry, Skill::Spellcasting],
            choices,
            restoration: RestorationPolicy::ChooseAgain,
        },
        vec![],
    )
    .unwrap();
    // Freeze only a complete, valid build, before any drain or retained edit.
    let restoration = RestorationPolicy::Authored(
        original
            .ledger()
            .entries()
            .iter()
            .zip(original.choices())
            .map(|(die, choices)| PlannedAdvancement {
                source: die.source(),
                choices: choices.clone(),
            })
            .collect(),
    );
    CreatureBuild::from_recorded(
        original.content().clone(),
        original.species_id().clone(),
        original.initial_attributes(),
        original.ledger().clone(),
        original.binding(),
        BuildAdvancement {
            restoration,
            ..original.advancement().clone()
        },
        vec![],
    )
    .unwrap()
}

fn refund(build: &mut CreatureBuild, index: usize) {
    let preview = build
        .preview_replacement(
            PurchaseLocation {
                owner: crate::HdIndex(0),
                index,
            },
            None,
        )
        .unwrap();
    assert_eq!(preview.removed().len(), 1);
    assert_eq!(preview.refunds().len(), 1);
    assert_eq!(preview.refunds()[0].points, 1);
    build.commit_replacement(preview).unwrap();
}

#[derive(Clone, Copy)]
enum BirthLearningExpectation {
    Unchanged,
    RebasedByInitialAttributes,
}

fn obstruction_banks_and_retries(
    original: CreatureBuild,
    error: BuildError,
    birth_learning: BirthLearningExpectation,
    obstruct: impl Fn(&mut CreatureBuild),
    clear: impl Fn(&mut CreatureBuild),
) {
    for policy in [
        FutureGrowth::Authored {},
        // An existing authored Racial ordinal takes precedence over this choice.
        FutureGrowth::Selected {
            source: GrowthSource::Class {
                class: ClassId::new("warrior").unwrap(),
            },
        },
    ] {
        let mut original = original.clone();
        original.set_future_growth(policy).unwrap();
        let original_derived = original.derive().unwrap();
        let retained = original.choices().len() - 1;
        let removed_die = original.ledger().entries()[retained].clone();
        let removed_choices = original.choices()[retained].clone();
        let mut state = CreatureState::new(original.clone()).unwrap();
        assert_eq!(state.drain_latest_hit_die().unwrap().applied, 1);
        assert_eq!(
            state.build().ledger().entries(),
            &original.ledger().entries()[..retained]
        );
        assert_eq!(state.build().choices(), &original.choices()[..retained]);
        assert_eq!(state.build().ledger().health_root_seed(), Some(HEALTH_ROOT));
        assert_eq!(
            state.derived().health_contributions.as_slice(),
            &original_derived.health_contributions[..retained]
        );

        let mut blocked = state.build().clone();
        obstruct(&mut blocked);
        assert_eq!(blocked.ledger(), state.build().ledger());
        assert_eq!(blocked.restoration_policy(), original.restoration_policy());
        state.rebuild(blocked.clone()).unwrap();

        // Explicit edits still return the precise error and stay atomic.
        let mut attempted = blocked.clone();
        assert_eq!(attempted.restore_hit_die(HEALTH_ROOT), Err(error));
        assert_eq!(attempted, blocked);
        assert_eq!(
            attempted.add_hit_die(removed_die.source(), HEALTH_ROOT),
            Err(error)
        );
        assert_eq!(attempted, blocked);
        let before_explicit = CreatureRecord::capture(&state);
        assert_eq!(
            state.restore_hit_die(),
            Err(CreatureStateError::Build(error))
        );
        assert_eq!(CreatureRecord::capture(&state), before_explicit);

        let price = state
            .earned_progress()
            .next_threshold(&state.build().ruleset().document().growth)
            .unwrap();
        let blocked_derived = state.derived().clone();
        state.award_xp(u32::try_from(price).unwrap()).unwrap();
        assert_eq!(state.build(), &blocked);
        assert_eq!(state.derived(), &blocked_derived);
        assert_eq!(state.earned_progress().earned_xp(), price);
        assert_eq!(state.earned_progress().held_xp(), price);
        assert_eq!(state.earned_progress().spent_xp(), 0);
        assert_eq!(state.restorable_hit_dice().get(), 1);

        // Both strict persisted forms retain the blocked build and its bank.
        let build_record = BuildRecord::capture(state.build());
        let decoded_build: BuildRecord =
            serde_json::from_slice(&serde_json::to_vec(&build_record).unwrap()).unwrap();
        assert_eq!(decoded_build, build_record);
        assert_eq!(
            decoded_build.restore(original.content().clone()).unwrap(),
            blocked
        );
        let record = CreatureRecord::capture(&state);
        let decoded: CreatureRecord =
            serde_json::from_slice(&serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(decoded, record);
        state = decoded.restore(original.content().clone(), vec![]).unwrap();
        assert_eq!(CreatureRecord::capture(&state), record);
        assert_eq!(state.derived(), &blocked_derived);

        // A retry with the same obstruction neither spends nor rewrites rows.
        state.rebuild(state.build().clone()).unwrap();
        assert_eq!(CreatureRecord::capture(&state), record);
        assert_eq!(state.build().ledger(), blocked.ledger());
        assert_eq!(state.build().choices(), blocked.choices());

        let mut cleared = state.build().clone();
        clear(&mut cleared);
        assert_eq!(cleared.ledger(), blocked.ledger());
        assert_eq!(cleared.choices(), &original.choices()[..retained]);
        let mut expected = original.clone();
        if matches!(
            birth_learning,
            BirthLearningExpectation::RebasedByInitialAttributes
        ) {
            // The privileged initial-attribute rewrite rebases unchosen birth
            // learning to the retained three-HD build. Restoring Strength must
            // retain that exact receipt, not mint a new Intellect entitlement.
            assert_ne!(cleared.intellect_learning, original.intellect_learning);
            expected.intellect_learning = cleared.intellect_learning.clone();
        }
        state.rebuild(cleared).unwrap();
        assert_eq!(state.build(), &expected);
        assert_eq!(state.build().ledger().entries()[retained], removed_die);
        assert_eq!(state.build().choices()[retained], removed_choices);
        assert_eq!(state.derived(), &original_derived);
        assert_eq!(state.earned_progress().earned_xp(), price);
        assert_eq!(state.earned_progress().held_xp(), 0);
        assert_eq!(state.earned_progress().spent_xp(), price);
        assert_eq!(state.restorable_hit_dice().get(), 0);

        let restored = CreatureRecord::capture(&state);
        let decoded: CreatureRecord =
            serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
        let roundtrip = decoded.restore(original.content().clone(), vec![]).unwrap();
        assert_eq!(roundtrip, state);
        assert_eq!(roundtrip.build(), &expected);
        state.rebuild(state.build().clone()).unwrap();
        state.award_xp(0).unwrap();
        assert_eq!(CreatureRecord::capture(&state), restored);
    }
}

#[test]
fn authored_attribute_cap_banks_xp_until_retained_initial_attributes_allow_restoration() {
    let original = authored(
        vec![
            HdChoices::default(),
            HdChoices::default(),
            HdChoices::default(),
            HdChoices {
                purchase_highwater: 0,
                attribute: Some(Attribute::Strength),
                ..HdChoices::default()
            },
        ],
        strength(4),
        1,
        vec![],
    );
    obstruction_banks_and_retries(
        original,
        BuildError::AttributeCap,
        BirthLearningExpectation::RebasedByInitialAttributes,
        |build| build.set_initial_attributes(strength(5)).unwrap(),
        |build| build.set_initial_attributes(strength(4)).unwrap(),
    );
}

#[test]
fn authored_rank_cap_banks_xp_until_the_conflicting_retained_rank_is_refunded() {
    let rank = purchase(PurchaseSelection::SkillRank(Skill::HeavyWeaponry));
    let original = authored(
        vec![
            HdChoices {
                purchase_highwater: 0,
                purchases: vec![rank.clone(); 3],
                attribute: None,
            },
            HdChoices {
                purchase_highwater: 0,
                purchases: vec![rank],
                attribute: None,
            },
        ],
        Attributes::default(),
        4,
        vec![],
    );
    assert_eq!(
        original.derive().unwrap().skills.get(Skill::HeavyWeaponry),
        5
    );
    obstruction_banks_and_retries(
        original,
        BuildError::RankCap,
        BirthLearningExpectation::Unchanged,
        |build| {
            build
                .train(crate::HdIndex(0), Skill::HeavyWeaponry)
                .unwrap()
        },
        |build| refund(build, 3),
    );
}

#[test]
fn authored_duplicate_talent_banks_xp_until_the_retained_duplicate_is_refunded() {
    let original = authored(
        vec![
            HdChoices::default(),
            HdChoices {
                purchase_highwater: 0,
                purchases: vec![purchase(PurchaseSelection::Talent(talent(0)))],
                attribute: None,
            },
        ],
        Attributes::default(),
        1,
        vec![crate::test_creatures::talent(
            "fixture_growth_conflict_000",
            vec![],
        )],
    );
    obstruction_banks_and_retries(
        original,
        BuildError::DuplicateTalent,
        BirthLearningExpectation::Unchanged,
        |build| build.select_talent(crate::HdIndex(0), talent(0)).unwrap(),
        |build| refund(build, 0),
    );
}

#[test]
fn authored_talent_ownership_cap_banks_xp_until_a_retained_talent_is_refunded() {
    let original = authored(
        vec![
            HdChoices {
                purchase_highwater: 0,
                purchases: (0..255)
                    .map(|index| purchase(PurchaseSelection::Talent(talent(index))))
                    .collect(),
                attribute: None,
            },
            HdChoices {
                purchase_highwater: 0,
                purchases: vec![purchase(PurchaseSelection::Talent(talent(255)))],
                attribute: None,
            },
        ],
        Attributes::default(),
        256,
        (0..257)
            .map(|index| {
                crate::test_creatures::talent(
                    &format!("fixture_growth_conflict_{index:03}"),
                    vec![],
                )
            })
            .collect(),
    );
    assert_eq!(original.derive().unwrap().active_talents.len(), 256);
    obstruction_banks_and_retries(
        original,
        BuildError::TooManyOwnedTalents,
        BirthLearningExpectation::Unchanged,
        |build| build.select_talent(crate::HdIndex(0), talent(256)).unwrap(),
        |build| refund(build, 255),
    );
}
