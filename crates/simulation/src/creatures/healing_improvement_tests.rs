//! Owned Health improvements survive exact restoration without changing seeds.
use super::{record::BuildRecord, CreatureBuild, CreatureState};
use crate::abilities::{
    resolve, ActorEffectState, PlannedEffect, ResolutionPlan, ResolvedEffect, Snapshot,
};
use crate::progression::{HdLedger, HdSource};
use crate::rules::HealingOverflow;
use crate::ActorId;
use serde_json::json;

fn build(count: usize) -> CreatureBuild {
    let base = crate::test_creatures::build(crate::test_creatures::species());
    CreatureBuild::from_recorded(
        base.content().clone(),
        base.species_id().clone(),
        base.initial_attributes(),
        HdLedger::seeded(vec![HdSource::Racial; count], 42).unwrap(),
        base.binding(),
        super::BuildAdvancement {
            choices: vec![Default::default(); count],
            ..base.advancement().clone()
        },
        vec![],
    )
    .unwrap()
}
fn input(effects: &[PlannedEffect]) -> ResolutionPlan<'_> {
    ResolutionPlan::Effects {
        actor: ActorId(1),
        target: ActorId(1),
        effects,
    }
}
fn effect(count: u16, overflow: HealingOverflow) -> PlannedEffect {
    PlannedEffect::HealPerHitDie {
        dice: vec![crate::dice::DicePool::new(count, 4, 0).unwrap()],
        bonus: 0,
        overflow,
    }
}
fn apply(
    creature: &mut CreatureState,
    effect: &ResolvedEffect,
) -> Result<Option<ResolvedEffect>, crate::abilities::EffectApplicationError> {
    effect.apply_to(
        &mut ActorEffectState {
            creature,
            observed_plan: None,
            placement: None,
            recovery_context: None,
        },
        ActorId(1),
    )
}
fn roundtrip(build: &CreatureBuild) -> CreatureBuild {
    serde_json::from_slice::<BuildRecord>(
        &serde_json::to_vec(&BuildRecord::capture(build)).unwrap(),
    )
    .unwrap()
    .restore(build.content().clone())
    .unwrap()
}

#[test]
fn strict_overflow_improves_one_earliest_active_die_and_exact_fill_does_not() {
    let original = CreatureState::new(build(3)).unwrap();
    let candidate = original.derived().next_health_improvement.clone().unwrap();
    assert!(candidate.ordinal.get() > 0);
    let planned = [effect(3, HealingOverflow::ImproveEarliestHitDie)];
    let mut expected_rng = 42;
    let roll = crate::dice::DicePool::new(3, 4, 0)
        .unwrap()
        .roll(&mut expected_rng, Default::default())
        .total;
    for injury in [0, roll - 1, roll, roll + 1] {
        let mut creature = original.clone();
        creature.damage(injury);
        let before = creature.clone();
        let mut rng = 42;
        let outcome = resolve(
            input(&planned),
            &Snapshot::capture(&creature),
            &mut rng,
            &mut (),
        )
        .unwrap();
        assert_eq!(rng, expected_rng);
        let grew = injury < roll;
        assert_eq!(
            matches!(
                &outcome.effects[0],
                ResolvedEffect::HealAndImproveHitDie { .. }
            ),
            grew
        );
        let persisted: ResolvedEffect =
            serde_json::from_slice(&serde_json::to_vec(&outcome.effects[0]).unwrap()).unwrap();
        apply(&mut creature, &persisted).unwrap();
        assert_eq!(
            creature.health().maximum(),
            before.health().maximum() + u64::from(grew)
        );
        assert_eq!(
            creature.build().ledger().entries()[candidate.ordinal.get() as usize]
                .health_improvement(),
            u16::from(grew)
        );
        for (a, b) in before
            .build()
            .ledger()
            .entries()
            .iter()
            .zip(creature.build().ledger().entries())
        {
            assert_eq!(a.health_seed(), b.health_seed());
            assert_eq!(a.source(), b.source());
        }
        if grew {
            let committed = creature.clone();
            assert!(apply(&mut creature, &outcome.effects[0]).is_err());
            assert_eq!(creature, committed);
        }
    }
}

#[test]
fn ordinary_healing_and_all_maximized_overflow_never_scan_or_recompile_hit_dice() {
    for count in [1, 3] {
        let mut creature = CreatureState::new(build(count)).unwrap();
        if count == 3 {
            while let Some(claim) = creature.derived().next_health_improvement.clone() {
                creature
                    .heal_and_improve_hit_die(0, &claim, creature.build_epoch())
                    .unwrap();
            }
        }
        let before = creature.clone();
        let counts = crate::diagnostics::work_counts();
        let outcome = resolve(
            input(&[effect(count as u16, HealingOverflow::ImproveEarliestHitDie)]),
            &Snapshot::capture(&creature),
            &mut 42,
            &mut (),
        )
        .unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.raw_hit_die_visits, counts.raw_hit_die_visits);
        assert_eq!(
            after.permanent_creature_compiles,
            counts.permanent_creature_compiles
        );
        assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: 0 }]);
        apply(&mut creature, &outcome.effects[0]).unwrap();
        assert_eq!(creature, before);
    }
    let creature = CreatureState::new(build(3)).unwrap();
    let counts = crate::diagnostics::work_counts();
    let outcome = resolve(
        input(&[effect(3, HealingOverflow::None)]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: 0 }]);
    assert_eq!(
        crate::diagnostics::work_counts().raw_hit_die_visits,
        counts.raw_hit_die_visits
    );
}

#[test]
fn same_owned_die_restores_improvement_and_replacement_retires_it() {
    let mut creature = CreatureState::new(build(2)).unwrap();
    let claim = creature.derived().next_health_improvement.clone().unwrap();
    assert_eq!(claim.ordinal.get(), 1);
    creature
        .heal_and_improve_hit_die(0, &claim, creature.build_epoch())
        .unwrap();
    let improved = creature.build().ledger().entries()[1].clone();
    let mut drained = creature.build().clone();
    assert_eq!(drained.remove_latest().unwrap().hit_die, improved);
    let mut restored = roundtrip(&drained);
    restored.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(restored.ledger().entries()[1], improved);
    assert_eq!(roundtrip(&restored), restored);
    let mut replacement = roundtrip(&drained);
    replacement
        .add_hit_die(
            HdSource::Class(crate::content_ids::ClassId::new("mage").unwrap()),
            42,
        )
        .unwrap();
    assert_eq!(replacement.ledger().entries()[1].health_improvement(), 0);
    replacement.remove_latest();
    replacement.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(replacement.ledger().entries()[1].health_improvement(), 0);
}

#[test]
fn acquisition_template_and_class_die_caps_survive_later_template_changes() {
    let base = build(3);
    let template_id = crate::content_ids::TemplateId::new("fixture_health_origin").unwrap();
    let mut catalog = base.content().catalog().clone();
    let mut template = super::Template::new(template_id.clone(), 0);
    let current = base.derive().unwrap().kind;
    let larger = base
        .ruleset()
        .document()
        .creature_types
        .iter()
        .find(|entry| entry.health_die > base.ruleset().creature_type(&current).unwrap().health_die)
        .unwrap();
    template.kind = Some(larger.id.clone());
    catalog.templates.insert(template_id.clone(), template);
    let content = super::Content::new(base.ruleset().clone().into(), catalog).unwrap();
    let mut original = CreatureBuild::from_recorded(
        content,
        base.species_id().clone(),
        base.initial_attributes(),
        HdLedger::seeded(
            vec![
                HdSource::Racial,
                HdSource::Racial,
                HdSource::Class(crate::content_ids::ClassId::new("mage").unwrap()),
            ],
            42,
        )
        .unwrap(),
        base.binding(),
        base.advancement().clone(),
        vec![template_id.clone()],
    )
    .unwrap();
    assert_eq!(
        original.ledger().entries()[1].health_origin().unwrap().die,
        larger.health_die
    );
    assert_eq!(
        original.ledger().entries()[1]
            .health_origin()
            .unwrap()
            .templates,
        [template_id]
    );
    let class_die = base
        .ruleset()
        .class(&crate::content_ids::ClassId::new("mage").unwrap())
        .unwrap()
        .health_die;
    assert_eq!(
        original.ledger().entries()[2].health_origin().unwrap().die,
        class_die
    );
    let before = original.ledger().clone();
    original.set_templates(vec![]).unwrap();
    assert_eq!(original.ledger(), &before);
    assert_eq!(roundtrip(&original), original);
}

#[test]
fn saved_improvements_require_complete_provenance_and_reject_forged_caps_or_archives() {
    let original = build(3);
    let value = serde_json::to_value(BuildRecord::capture(&original)).unwrap();
    let accepts = |value: &serde_json::Value| {
        serde_json::from_value::<BuildRecord>(value.clone())
            .ok()
            .and_then(|record| record.restore(original.content().clone()).ok())
            .is_some()
    };
    assert!(accepts(&value));
    for key in ["health_origin", "health_improvement"] {
        let mut bad = value.clone();
        bad["hd"][1].as_object_mut().unwrap().remove(key);
        assert!(!accepts(&bad));
    }
    let mut bad = value.clone();
    bad.as_object_mut().unwrap().remove("drained_hd");
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][1]["health_origin"]
        .as_object_mut()
        .unwrap()
        .remove("templates");
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][1] = json!([
        value["hd"][1]["source"],
        value["hd"][1]["health_seed"],
        value["hd"][1]["health_origin"],
        0
    ]);
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][1]["health_origin"] = json!([
        [],
        value["hd"][1]["health_origin"]["kind"],
        value["hd"][1]["health_origin"]["die"]
    ]);
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][1]["health_seed"] = json!(0);
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][0]["health_improvement"] = json!(1);
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][1]["health_improvement"] = json!(1000);
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["hd"][1]["health_origin"]["die"] = json!(1000);
    assert!(!accepts(&bad));
    let larger = original
        .ruleset()
        .document()
        .creature_types
        .iter()
        .find(|entry| {
            entry.health_die > original.ledger().entries()[1].health_origin().unwrap().die
        })
        .unwrap();
    let mut bad = value.clone();
    bad["hd"][1]["health_origin"]["kind"] = json!(larger.id);
    bad["hd"][1]["health_origin"]["die"] = json!(larger.health_die);
    assert!(!accepts(&bad));
    let mut bad = value.clone();
    bad["drained_hd"] = json!([{"ordinal":0,"hit_die":value["hd"][0]}]);
    assert!(!accepts(&bad));
}

#[test]
fn resolved_improvement_claims_reject_forgery_without_partial_healing() {
    let original = CreatureState::new(build(3)).unwrap();
    let claim = original.derived().next_health_improvement.clone().unwrap();
    let valid = ResolvedEffect::HealAndImproveHitDie {
        amount: 1,
        improvement: claim.clone(),
        build_epoch: original.build_epoch(),
    };
    let value = serde_json::to_value(&valid).unwrap();
    assert_eq!(
        serde_json::from_value::<ResolvedEffect>(value.clone()).unwrap(),
        valid
    );
    let mut missing_epoch = value.clone();
    missing_epoch.as_object_mut().unwrap().remove("build_epoch");
    assert!(serde_json::from_value::<ResolvedEffect>(missing_epoch).is_err());
    let mut zero_epoch = value.clone();
    zero_epoch["build_epoch"] = json!(0);
    assert!(serde_json::from_value::<ResolvedEffect>(zero_epoch).is_err());
    for key in [
        "ordinal",
        "source",
        "health_seed",
        "original_kind",
        "original_die",
        "previous",
    ] {
        let mut bad = value.clone();
        bad["improvement"].as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    }
    let mut bad = value.clone();
    bad["improvement"]["ordinal"] = json!(0);
    assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    let mut bad = value.clone();
    bad["improvement"]["previous"] = json!(1000);
    assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    let mut bad = value.clone();
    bad["improvement"]["extra"] = json!(1);
    assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    let mut creature = original.clone();
    creature.damage(1);
    let before = creature.clone();
    let mut changed = claim;
    changed.health_seed ^= 1;
    assert!(apply(
        &mut creature,
        &ResolvedEffect::HealAndImproveHitDie {
            amount: 1,
            improvement: changed,
            build_epoch: before.build_epoch(),
        }
    )
    .is_err());
    assert_eq!(creature, before);
}

#[test]
fn masked_owned_credit_is_acquisition_capped_and_returns_when_override_ends() {
    let base = build(3);
    let mut catalog = base.content().catalog().clone();
    let id = crate::content_ids::TemplateId::new("fixture_health_mask").unwrap();
    let mut mask = super::Template::new(id.clone(), 0);
    mask.modifiers.push(crate::rules::Modifier {
        scope: crate::rules::Scope::Always,
        op: crate::rules::ModifierOp::Set {
            stat: crate::rules::Stat::Health,
            value: 1000,
        },
    });
    catalog.templates.insert(id.clone(), mask);
    let content = super::Content::new(base.ruleset().clone().into(), catalog).unwrap();
    let mut masked = CreatureBuild::from_recorded(
        content,
        base.species_id().clone(),
        base.initial_attributes(),
        base.ledger().clone(),
        base.binding(),
        base.advancement().clone(),
        vec![],
    )
    .unwrap();
    let ordinary_maximum = CreatureState::new(masked.clone())
        .unwrap()
        .health()
        .maximum();
    masked.set_templates(vec![id]).unwrap();
    let mut creature = CreatureState::new(masked).unwrap();
    assert_eq!(creature.health().maximum(), 1000);
    let original = creature.build().ledger().clone();
    let mut increments = 0;
    while let Some(claim) = creature.derived().next_health_improvement.clone() {
        let outcome = resolve(
            input(&[effect(3, HealingOverflow::ImproveEarliestHitDie)]),
            &Snapshot::capture(&creature),
            &mut 42,
            &mut (),
        )
        .unwrap();
        assert!(
            matches!(&outcome.effects[0], ResolvedEffect::HealAndImproveHitDie { improvement, .. } if improvement == &claim)
        );
        apply(&mut creature, &outcome.effects[0]).unwrap();
        increments += 1;
        assert_eq!(creature.health().maximum(), 1000);
        assert!(increments <= 16);
    }
    assert!(increments > 0);
    let final_maximum = creature.health().maximum();
    let final_build = creature.build().clone();
    let outcome = resolve(
        input(&[effect(3, HealingOverflow::ImproveEarliestHitDie)]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: 0 }]);
    assert_eq!(creature.health().maximum(), final_maximum);
    assert_eq!(creature.build(), &final_build);
    for (before, after) in original
        .entries()
        .iter()
        .zip(creature.build().ledger().entries())
    {
        assert_eq!(before.health_seed(), after.health_seed());
        assert_eq!(before.health_origin(), after.health_origin());
    }
    let mut unmasked = creature.build().clone();
    unmasked.set_templates(vec![]).unwrap();
    creature.rebuild(unmasked).unwrap();
    assert_eq!(creature.health().maximum(), ordinary_maximum + increments);
    assert!(creature.derived().next_health_improvement.is_none());
}

#[test]
fn drained_creature_roundtrip_retains_credit_for_exact_owned_restoration() {
    let mut initial = build(2);
    initial
        .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
            source: crate::earned_growth::GrowthSource::Racial {},
        })
        .unwrap();
    let mut creature = CreatureState::new(initial).unwrap();
    let claim = creature.derived().next_health_improvement.clone().unwrap();
    creature
        .heal_and_improve_hit_die(0, &claim, creature.build_epoch())
        .unwrap();
    let improved = creature.build().ledger().entries()[1].clone();
    let maximum = creature.health().maximum();
    assert_eq!(creature.drain_latest_hit_die().unwrap().applied, 1);
    assert!(creature.derived().next_health_improvement.is_none());
    let before = creature.clone();
    let outcome = resolve(
        input(&[effect(1, HealingOverflow::ImproveEarliestHitDie)]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: 0 }]);
    assert_eq!(creature, before);
    let saved = serde_json::to_vec(&super::record::CreatureRecord::capture(&creature)).unwrap();
    let record: super::record::CreatureRecord = serde_json::from_slice(&saved).unwrap();
    let mut restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored, creature);
    assert_eq!(restored.restore_hit_die().unwrap().applied, 1);
    assert_eq!(restored.build().ledger().entries()[1], improved);
    assert_eq!(restored.health().maximum(), maximum);
    assert_eq!(restored.restorable_hit_dice().get(), 0);
    assert_eq!(restored.restore_hit_die().unwrap().applied, 0);
}

#[test]
fn magic_item_regression_stale_healing_claim_rejects_replaced_row() {
    let mut creature = CreatureState::new(build(2)).unwrap();
    creature.damage(1);
    let outcome = resolve(
        input(&[effect(2, HealingOverflow::ImproveEarliestHitDie)]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    let old = outcome.effects[0].clone();
    let original_claim = creature.derived().next_health_improvement.clone();
    apply(&mut creature, &old).unwrap();
    for source in [
        HdSource::Class(crate::content_ids::ClassId::new("mage").unwrap()),
        HdSource::Racial,
    ] {
        let mut replacement = creature.build().clone();
        replacement.remove_latest().unwrap();
        replacement.add_hit_die(source, 42).unwrap();
        creature.rebuild(replacement).unwrap();
    }
    assert_eq!(
        creature.derived().next_health_improvement,
        original_claim,
        "row operands repeat, so generation must reject the old claim"
    );
    creature.damage(1);
    let before = creature.clone();
    assert!(apply(&mut creature, &old).is_err());
    assert_eq!(
        creature, before,
        "stale generation must reject before healing"
    );
    let saved = serde_json::to_vec(&super::record::CreatureRecord::capture(&creature)).unwrap();
    let record: super::record::CreatureRecord = serde_json::from_slice(&saved).unwrap();
    let mut restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored.build_epoch(), creature.build_epoch());
    assert!(apply(&mut restored, &old).is_err());
    assert_eq!(restored, creature);
}

#[test]
fn healing_growth_epoch_exhaustion_rolls_back_the_prior_heal() {
    let mut creature = CreatureState::new(build(2)).unwrap();
    creature.damage(1);
    let mut saved =
        serde_json::to_value(super::record::CreatureRecord::capture(&creature)).unwrap();
    saved["build_epoch"] = json!(u64::MAX);
    let record: super::record::CreatureRecord = serde_json::from_value(saved).unwrap();
    let mut creature = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    let before = creature.clone();
    let claim = creature.derived().next_health_improvement.clone().unwrap();
    let effect = ResolvedEffect::HealAndImproveHitDie {
        amount: 1,
        improvement: claim,
        build_epoch: creature.build_epoch(),
    };
    assert_eq!(
        apply(&mut creature, &effect),
        Err(crate::abilities::EffectApplicationError::Creature(
            super::CreatureStateError::BuildEpochExhausted
        ))
    );
    assert_eq!(creature, before);
}

#[test]
fn sequential_healing_effects_own_successive_epochs() {
    let mut creature = CreatureState::new(build(3)).unwrap();
    let original_epoch = creature.build_epoch().get();
    let maximum = creature.health().maximum();
    let outcome = resolve(
        input(&[
            effect(3, HealingOverflow::ImproveEarliestHitDie),
            effect(3, HealingOverflow::ImproveEarliestHitDie),
        ]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.effects.len(), 2);
    for (index, effect) in outcome.effects.iter().enumerate() {
        let ResolvedEffect::HealAndImproveHitDie { build_epoch, .. } = effect else {
            panic!("expected record improvement")
        };
        assert_eq!(build_epoch.get(), original_epoch + index as u64);
        apply(&mut creature, effect).unwrap();
    }
    assert_eq!(creature.build_epoch().get(), original_epoch + 2);
    assert_eq!(creature.health().maximum(), maximum + 2);
    assert_eq!(creature.build(), &roundtrip(creature.build()));
}

#[test]
fn same_source_restoration_rejects_a_pre_drain_healing_claim() {
    let mut creature = CreatureState::new(build(2)).unwrap();
    let outcome = resolve(
        input(&[effect(2, HealingOverflow::ImproveEarliestHitDie)]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    let claim = creature.derived().next_health_improvement.clone();
    let mut drained = creature.build().clone();
    drained.remove_latest().unwrap();
    creature.rebuild(drained).unwrap();
    let mut restored = creature.build().clone();
    restored.add_hit_die(HdSource::Racial, 42).unwrap();
    creature.rebuild(restored).unwrap();
    assert_eq!(creature.derived().next_health_improvement, claim);
    let before = creature.clone();
    assert!(apply(&mut creature, &outcome.effects[0]).is_err());
    assert_eq!(creature, before);
}
