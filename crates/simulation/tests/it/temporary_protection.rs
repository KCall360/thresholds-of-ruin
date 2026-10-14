//! Timed protection is ordinary condition state and live damage authority.
use super::creature_fixture as fixture;
use std::{collections::BTreeMap, sync::Arc};
use tor_simulation::{
    abilities::{resolve, PlannedEffect, ResolutionPlan, ResolvedEffect, Snapshot},
    attributes::{ManaBinding, Skill},
    combat::DamageType,
    content_ids::{ConditionFamilyId, ConditionId, TalentId},
    creatures::{record::CreatureRecord, Content, CreatureBuild, CreatureState, ScenarioCatalog},
    damage::{DamageComponent, DamageSpec},
    progression::{HdLedger, HdSource},
    rules::{
        ConditionDef, ConditionLifecycle, Eviction, Modifier, ModifierOp, Requirement, Ruleset,
        Scope, Selector, Stacking, TalentDef,
    },
    ActorId,
};

fn reduction(amount: u32) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Reduction {
            selector: Selector::Category {
                category: DamageType::Impact,
            },
            amount,
        },
    }
}
fn ward(id: &str, amount: u32) -> ConditionDef {
    ConditionDef {
        id: ConditionId::new(id).unwrap(),
        family: ConditionFamilyId::new("test_ward").unwrap(),
        lifecycle: ConditionLifecycle::Timed,
        timing: tor_simulation::rules::ConditionTiming::expiring(),
        restrictions: vec![],
        preparation: tor_simulation::rules::ConditionPreparation::Preserve,
        name: id.into(),
        modifiers: vec![Modifier {
            scope: Scope::Always,
            op: serde_json::from_str(&format!(
                r#"{{"kind":"talent_reduction","category":"impact","amount":{amount}}}"#
            ))
            .expect("typed talent protection must decode before behavior is tested"),
        }],
        stacking: Stacking::Longest,
        source_scoped: true,
        descriptors: vec![],
        capacity: Eviction::Reject,
    }
}
fn content(with_wards: bool) -> Arc<Content> {
    let mut document = fixture::ruleset().document().clone();
    if with_wards {
        document
            .conditions
            .extend([ward("lesser_ward", 2), ward("greater_ward", 4)]);
    }
    for (id, amount) in [("lesser_protection", 1), ("greater_protection", 3)] {
        document.talents.push(TalentDef {
            form_choices: vec![],
            id: TalentId::new(id).unwrap(),
            name: id.into(),
            prerequisites: vec![],
            requirement: Requirement::All {
                requirements: vec![],
            },
            modifiers: vec![reduction(amount)],
        });
    }
    let mut species = fixture::species();
    species.modifiers.push(reduction(2));
    Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(species.id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap()
}
fn creature(with_wards: bool) -> CreatureState {
    let content = content(with_wards);
    let build = CreatureBuild::new(
        content,
        fixture::species().id,
        HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    CreatureState::new(build).unwrap()
}
fn impact_reduction(creature: &CreatureState) -> u32 {
    creature
        .derived()
        .protection
        .reduction(&Selector::Category {
            category: DamageType::Impact,
        })
}
fn apply(creature: &mut CreatureState, id: &str, source: u64, duration: u64) {
    creature
        .apply_condition(&ConditionId::new(id).unwrap(), ActorId(source), duration)
        .unwrap();
}

#[test]
fn talent_category_reduction_uses_the_strongest_value_and_adds_ordinary_protection() {
    let mut creature = creature(false);
    let mut build = creature.build().clone();
    build
        .select_talent(0, TalentId::new("lesser_protection").unwrap())
        .unwrap();
    build
        .select_talent(1, TalentId::new("greater_protection").unwrap())
        .unwrap();
    creature.rebuild(build).unwrap();
    assert_eq!(
        impact_reduction(&creature),
        5,
        "ordinary 2 plus strongest talent 3"
    );
}

#[test]
fn authored_talent_protection_never_adds_across_causers_or_definitions() {
    let mut creature = creature(true);
    apply(&mut creature, "lesser_ward", 1, 400);
    apply(&mut creature, "lesser_ward", 2, 500);
    apply(&mut creature, "greater_ward", 3, 200);
    assert_eq!(
        impact_reduction(&creature),
        6,
        "ordinary 2 plus strongest ward 4"
    );
    creature.advance_active(200);
    assert_eq!(
        impact_reduction(&creature),
        4,
        "weaker surviving ward resumes after expiry"
    );
    creature.advance_active(300);
    assert_eq!(impact_reduction(&creature), 2);
}

#[test]
fn timed_ward_is_live_for_a_frozen_payload_and_expires_without_changing_that_payload() {
    let mut creature = creature(true);
    let damage = DamageComponent::fixed(DamageType::Impact, None, 10);
    let primary = damage.key();
    let effects = vec![PlannedEffect::Damage {
        damage: DamageSpec::new(vec![damage], Some(primary)).unwrap(),
    }];
    let frozen = effects.clone();
    let dealt = |creature: &CreatureState| {
        let outcome = resolve(
            ResolutionPlan::Effects {
                actor: ActorId(1),
                target: ActorId(2),
                effects: &effects,
            },
            &Snapshot::capture(creature),
            &mut 42,
            &mut (),
        );
        match outcome.effects.as_slice() {
            [ResolvedEffect::Damage { amount }] => *amount,
            other => panic!("expected one live damage effect, got {other:?}"),
        }
    };
    assert_eq!(dealt(&creature), 8);
    apply(&mut creature, "lesser_ward", 1, 400);
    assert_eq!(dealt(&creature), 6);
    creature.advance_active(399);
    let bytes = serde_json::to_vec(&CreatureRecord::capture(&creature)).unwrap();
    let record: CreatureRecord = serde_json::from_slice(&bytes).unwrap();
    let mut restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
    restored.advance_active(1);
    assert_eq!(dealt(&restored), 8);
    assert_eq!(effects, frozen);
}

#[test]
fn typed_talent_reduction_has_a_named_category_only_operand() {
    let bytes = br#"{"kind":"talent_reduction","category":"impact","amount":2}"#;
    let decoded = serde_json::from_slice::<ModifierOp>(bytes);
    assert!(
        decoded.is_ok(),
        "missing typed talent-reduction schema: {decoded:?}"
    );
    let valid = decoded.unwrap();
    assert_eq!(serde_json::to_vec(&valid).unwrap(), bytes);
    for malformed in [
        r#"["talent_reduction","impact",2]"#,
        r#"{"kind":"talent_reduction","category":"impact"}"#,
        r#"{"kind":"talent_reduction","category":"impact","category":"keen","amount":2}"#,
        r#"{"kind":"talent_reduction","category":"impact","amount":2,"extra":1}"#,
        r#"{"kind":"talent_reduction","selector":{"kind":"descriptor","descriptor":"fire"},"amount":2}"#,
    ] {
        assert!(
            serde_json::from_slice::<ModifierOp>(malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn explicit_ward_and_passive_talent_share_one_strongest_category() {
    let mut creature = creature(true);
    let mut build = creature.build().clone();
    build
        .select_talent(0, TalentId::new("greater_protection").unwrap())
        .unwrap();
    creature.rebuild(build).unwrap();
    apply(&mut creature, "lesser_ward", 1, 400);
    assert_eq!(impact_reduction(&creature), 5);
    apply(&mut creature, "greater_ward", 2, 200);
    assert_eq!(impact_reduction(&creature), 6);
    creature.advance_active(200);
    assert_eq!(impact_reduction(&creature), 5);
}

#[test]
fn ordinary_condition_reduction_keeps_its_additive_contract() {
    let initial = creature(true);
    let mut document = initial.build().ruleset().document().clone();
    let mut mundane = ward("mundane_cover", 2);
    mundane.modifiers = vec![reduction(2)];
    mundane.stacking = Stacking::Stack { maximum: 2 };
    document.conditions.push(mundane);
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        initial.build().content().catalog().clone(),
    )
    .unwrap();
    let build = CreatureBuild::from_recorded(
        content,
        initial.build().species_id().clone(),
        initial.build().initial_attributes(),
        initial.build().ledger().clone(),
        initial.build().binding(),
        initial.build().advancement().clone(),
        vec![],
    )
    .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    apply(&mut creature, "mundane_cover", 1, 400);
    apply(&mut creature, "mundane_cover", 1, 400);
    apply(&mut creature, "mundane_cover", 2, 400);
    apply(&mut creature, "greater_ward", 3, 400);
    assert_eq!(
        impact_reduction(&creature),
        12,
        "ordinary 2 + additive stacks 6 + strongest ward 4"
    );
}

#[test]
fn descriptor_reduction_on_talents_preserves_its_existing_additive_scope() {
    use tor_simulation::content_ids::DescriptorId;
    let initial = creature(false);
    let mut document = initial.build().ruleset().document().clone();
    for talent in &mut document.talents {
        if matches!(
            talent.id.as_str(),
            "lesser_protection" | "greater_protection"
        ) {
            let ModifierOp::Reduction { selector, .. } = &mut talent.modifiers[0].op else {
                unreachable!()
            };
            *selector = Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            };
        }
    }
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        initial.build().content().catalog().clone(),
    )
    .unwrap();
    let mut build = CreatureBuild::from_recorded(
        content,
        initial.build().species_id().clone(),
        initial.build().initial_attributes(),
        initial.build().ledger().clone(),
        initial.build().binding(),
        initial.build().advancement().clone(),
        vec![],
    )
    .unwrap();
    build
        .select_talent(0, TalentId::new("lesser_protection").unwrap())
        .unwrap();
    build
        .select_talent(1, TalentId::new("greater_protection").unwrap())
        .unwrap();
    let creature = CreatureState::new(build).unwrap();
    assert_eq!(impact_reduction(&creature), 2);
    assert_eq!(
        creature
            .derived()
            .protection
            .reduction(&Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap()
            }),
        4
    );
}

#[test]
fn protection_bucket_overflow_rejects_atomically_in_either_fold_order() {
    use tor_simulation::damage::{DamageError, Protection};
    let maximum = tor_simulation::limits::NUMERIC as u32;
    let talent = ModifierOp::TalentReduction {
        category: DamageType::Impact,
        amount: maximum,
    };
    let ordinary = reduction(1).op;
    for (first, second) in [(&talent, &ordinary), (&ordinary, &talent)] {
        let mut protection = Protection::default();
        protection.apply(first).unwrap();
        let before = protection.clone();
        assert_eq!(
            protection.apply(second),
            Err(DamageError::InvalidProtection)
        );
        assert_eq!(protection, before);
    }
    let bytes = format!(
        r#"{{"kind":"talent_reduction","category":"impact","amount":{}}}"#,
        maximum + 1
    );
    assert!(serde_json::from_slice::<ModifierOp>(bytes.as_bytes()).is_err());
}

#[test]
fn stacked_talent_wards_still_contribute_only_the_strongest_single_amount() {
    let initial = creature(true);
    let mut document = initial.build().ruleset().document().clone();
    document
        .conditions
        .iter_mut()
        .find(|row| row.id.as_str() == "lesser_ward")
        .unwrap()
        .stacking = Stacking::Stack { maximum: 3 };
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        initial.build().content().catalog().clone(),
    )
    .unwrap();
    let build = CreatureBuild::from_recorded(
        content,
        initial.build().species_id().clone(),
        initial.build().initial_attributes(),
        initial.build().ledger().clone(),
        initial.build().binding(),
        initial.build().advancement().clone(),
        vec![],
    )
    .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    for _ in 0..4 {
        apply(&mut creature, "lesser_ward", 1, 400);
    }
    apply(&mut creature, "lesser_ward", 2, 500);
    assert_eq!(
        creature
            .conditions()
            .instances()
            .find(|row| row.source == ActorId(1))
            .unwrap()
            .stacks,
        3
    );
    assert_eq!(
        impact_reduction(&creature),
        4,
        "ordinary 2 plus one 2-point talent ward, regardless of four retained stacks"
    );
    apply(&mut creature, "greater_ward", 3, 200);
    assert_eq!(impact_reduction(&creature), 6);
    creature.advance_active(200);
    assert_eq!(impact_reduction(&creature), 4);
}
