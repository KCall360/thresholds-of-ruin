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
    let candidate = original
        .derived()
        .next_health_improvements
        .first()
        .cloned()
        .unwrap();
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
                ResolvedEffect::HealAndImproveHitDice { .. }
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
            while let Some(claim) = creature.derived().next_health_improvements.first().cloned() {
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
    let claim = creature
        .derived()
        .next_health_improvements
        .first()
        .cloned()
        .unwrap();
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
    let claim = original
        .derived()
        .next_health_improvements
        .first()
        .cloned()
        .unwrap();
    let valid = ResolvedEffect::HealAndImproveHitDice {
        amount: 1,
        improvements: vec![claim.clone()],
        build_epoch: original.build_epoch(),
    };
    let value = serde_json::to_value(&valid).unwrap();
    assert_eq!(
        serde_json::from_value::<ResolvedEffect>(value.clone()).unwrap(),
        valid
    );
    let fields = ["kind", "amount", "improvements", "build_epoch"];
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        fields.into_iter().collect()
    );
    for field in fields {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ResolvedEffect>(missing).is_err(),
            "{field}"
        );
    }
    let mut unknown = value.clone();
    unknown["extra"] = json!(null);
    assert!(serde_json::from_value::<ResolvedEffect>(unknown).is_err());
    let positional =
        serde_json::Value::Array(fields.iter().map(|field| value[field].clone()).collect());
    assert!(serde_json::from_value::<ResolvedEffect>(positional).is_err());
    let mut singular_key = value.clone();
    singular_key.as_object_mut().unwrap().remove("improvements");
    singular_key["improvement"] = value["improvements"][0].clone();
    assert!(serde_json::from_value::<ResolvedEffect>(singular_key).is_err());
    let mut singular_tag = value.clone();
    singular_tag["kind"] = json!("heal_and_improve_hit_die");
    assert!(serde_json::from_value::<ResolvedEffect>(singular_tag).is_err());
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
        bad["improvements"][0].as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    }
    let mut bad = value.clone();
    bad["improvements"][0]["ordinal"] = json!(0);
    assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    let mut bad = value.clone();
    bad["improvements"][0]["previous"] = json!(1000);
    assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    let mut bad = value.clone();
    bad["improvements"][0]["extra"] = json!(1);
    assert!(serde_json::from_value::<ResolvedEffect>(bad).is_err());
    let mut creature = original.clone();
    creature.damage(1);
    let before = creature.clone();
    let mut changed = claim;
    changed.health_seed ^= 1;
    assert!(apply(
        &mut creature,
        &ResolvedEffect::HealAndImproveHitDice {
            amount: 1,
            improvements: vec![changed],
            build_epoch: before.build_epoch(),
        }
    )
    .is_err());
    assert_eq!(creature, before);
}

#[test]
fn masked_owned_credit_is_acquisition_capped_and_returns_when_override_ends() {
    for overflow in [
        HealingOverflow::ImproveEarliestHitDie,
        HealingOverflow::ImproveEarliestHitDiceByExcess,
    ] {
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
        while !creature.derived().next_health_improvements.is_empty() {
            let count = if overflow == HealingOverflow::ImproveEarliestHitDie {
                1
            } else {
                2
            };
            let claims = creature
                .derived()
                .next_health_improvements
                .iter()
                .take(count)
                .cloned()
                .collect::<Vec<_>>();
            let outcome = resolve(
                input(&[PlannedEffect::HealPerHitDie {
                    dice: vec![crate::dice::DicePool::new(3, 8, 0).unwrap()],
                    bonus: 100,
                    overflow,
                }]),
                &Snapshot::capture(&creature),
                &mut 42,
                &mut (),
            )
            .unwrap();
            assert!(
                matches!(&outcome.effects[0], ResolvedEffect::HealAndImproveHitDice { improvements, .. } if improvements == &claims)
            );
            apply(&mut creature, &outcome.effects[0]).unwrap();
            increments += claims.len() as u32;
            assert_eq!(creature.health().maximum(), 1000);
            assert!(increments <= 16);
        }
        assert!(increments > 0);
        let final_maximum = creature.health().maximum();
        let final_build = creature.build().clone();
        let outcome = resolve(
            input(&[PlannedEffect::HealPerHitDie {
                dice: vec![crate::dice::DicePool::new(3, 8, 0).unwrap()],
                bonus: 100,
                overflow,
            }]),
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
        assert_eq!(
            creature.health().maximum(),
            ordinary_maximum + u64::from(increments)
        );
        assert!(creature.derived().next_health_improvements.is_empty());
    }
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
    let claim = creature
        .derived()
        .next_health_improvements
        .first()
        .cloned()
        .unwrap();
    creature
        .heal_and_improve_hit_die(0, &claim, creature.build_epoch())
        .unwrap();
    let improved = creature.build().ledger().entries()[1].clone();
    let maximum = creature.health().maximum();
    assert_eq!(creature.drain_latest_hit_die().unwrap().applied, 1);
    assert!(creature.derived().next_health_improvements.is_empty());
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
    let original_claim = creature.derived().next_health_improvements.first().cloned();
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
        creature.derived().next_health_improvements.first().cloned(),
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
    let claim = creature
        .derived()
        .next_health_improvements
        .first()
        .cloned()
        .unwrap();
    let effect = ResolvedEffect::HealAndImproveHitDice {
        amount: 1,
        improvements: vec![claim],
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
        let ResolvedEffect::HealAndImproveHitDice { build_epoch, .. } = effect else {
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
    let claim = creature.derived().next_health_improvements.first().cloned();
    let mut drained = creature.build().clone();
    drained.remove_latest().unwrap();
    creature.rebuild(drained).unwrap();
    let mut restored = creature.build().clone();
    restored.add_hit_die(HdSource::Racial, 42).unwrap();
    creature.rebuild(restored).unwrap();
    assert_eq!(
        creature.derived().next_health_improvements.first().cloned(),
        claim
    );
    let before = creature.clone();
    assert!(apply(&mut creature, &outcome.effects[0]).is_err());
    assert_eq!(creature, before);
}

#[test]
fn extra_healing_strict_excess_selects_one_or_two_credits_in_one_epoch() {
    let original = CreatureState::new(build(3)).unwrap();
    let pool = crate::dice::DicePool::new(3, 8, 0).unwrap();
    let mut expected_rng = 42;
    let roll = pool.roll(&mut expected_rng, Default::default()).total;
    let healing = PlannedEffect::HealPerHitDie {
        dice: vec![pool],
        bonus: 7 - roll as i32,
        overflow: HealingOverflow::ImproveEarliestHitDiceByExcess,
    };
    assert_eq!(original.derived().next_health_improvements.len(), 2);
    for (injury, count) in [(7, 0), (6, 1), (3, 1), (2, 2)] {
        let mut creature = original.clone();
        creature.damage(injury);
        assert!(!creature.health().dead());
        let before = creature.clone();
        let mut rng = 42;
        let counters = crate::diagnostics::work_counts();
        let outcome = resolve(
            input(std::slice::from_ref(&healing)),
            &Snapshot::capture(&creature),
            &mut rng,
            &mut (),
        )
        .unwrap();
        assert_eq!(rng, expected_rng);
        assert_eq!(
            crate::diagnostics::work_counts().permanent_creature_compiles
                - counters.permanent_creature_compiles,
            usize::from(count > 0)
        );
        if count == 0 {
            assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: injury }]);
        } else {
            let ResolvedEffect::HealAndImproveHitDice {
                improvements,
                build_epoch,
                ..
            } = &outcome.effects[0]
            else {
                panic!("expected owned improvement")
            };
            assert_eq!(
                improvements,
                &before.derived().next_health_improvements[..count]
            );
            assert_eq!(*build_epoch, before.build_epoch());
        }
        let effect: ResolvedEffect =
            serde_json::from_slice(&serde_json::to_vec(&outcome.effects[0]).unwrap()).unwrap();
        let counters = crate::diagnostics::work_counts();
        apply(&mut creature, &effect).unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().permanent_creature_compiles
                - counters.permanent_creature_compiles,
            usize::from(count > 0)
        );
        assert_eq!(
            creature.health().maximum(),
            before.health().maximum() + count as u64
        );
        assert_eq!(
            creature.build_epoch().get(),
            before.build_epoch().get() + u64::from(count > 0)
        );
        assert_eq!(creature.health().injury(), 0);
        assert_eq!(creature.build(), &roundtrip(creature.build()));
        if count > 0 {
            let committed = creature.clone();
            assert!(apply(&mut creature, &effect).is_err());
            assert_eq!(creature, committed);
        }
    }
}

#[test]
fn two_credit_prefix_handles_same_and_next_row_then_caps_without_raw_scan() {
    let mut creature = CreatureState::new(build(12)).unwrap();
    let mut same_row = false;
    let mut next_row = false;
    let mut total = 0;
    while !creature.derived().next_health_improvements.is_empty() {
        let claims = creature.derived().next_health_improvements.clone();
        if let [a, b] = claims.as_slice() {
            same_row |= a.ordinal == b.ordinal;
            next_row |= a.ordinal != b.ordinal;
            if a.ordinal == b.ordinal {
                assert_eq!(a.previous + 1, b.previous);
            }
        }
        let original = creature.build().ledger().clone();
        creature
            .heal_and_improve_hit_dice(0, &claims, creature.build_epoch())
            .unwrap();
        total += claims.len();
        for (before, after) in original
            .entries()
            .iter()
            .zip(creature.build().ledger().entries())
        {
            assert_eq!(before.health_seed(), after.health_seed());
            assert_eq!(before.health_origin(), after.health_origin());
        }
        assert!(total <= 12 * 8);
        // One ordinary increment between double batches exercises cross-row
        // prefixes independently of the parity of each die's remaining budget.
        if let Some(claim) = creature.derived().next_health_improvements.first().cloned() {
            creature
                .heal_and_improve_hit_die(0, &claim, creature.build_epoch())
                .unwrap();
            total += 1;
        }
    }
    assert!(same_row && next_row);
    let counters = crate::diagnostics::work_counts();
    let outcome = resolve(
        input(&[effect(12, HealingOverflow::ImproveEarliestHitDiceByExcess)]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: 0 }]);
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.raw_hit_die_visits, counters.raw_hit_die_visits);
    assert_eq!(
        after.permanent_creature_compiles,
        counters.permanent_creature_compiles
    );
}

#[test]
fn two_credit_claim_rejects_nonprefix_reorder_duplicates_and_partial_application() {
    let mut creature = CreatureState::new(build(3)).unwrap();
    creature.damage(1);
    let claims = creature.derived().next_health_improvements.clone();
    assert_eq!(claims.len(), 2);
    let good = ResolvedEffect::HealAndImproveHitDice {
        amount: 1,
        improvements: claims.clone(),
        build_epoch: creature.build_epoch(),
    };
    let encoded = serde_json::to_value(&good).unwrap();
    for invalid in [
        json!([]),
        json!([claims[0].clone(), claims[0].clone()]),
        json!([claims[1].clone(), claims[0].clone()]),
        json!([claims[0].clone(), claims[1].clone(), claims[1].clone()]),
    ] {
        let mut value = encoded.clone();
        value["improvements"] = invalid;
        assert!(serde_json::from_value::<ResolvedEffect>(value).is_err());
    }
    for invalid in [
        vec![],
        vec![claims[1].clone()],
        vec![claims[0].clone(), claims[0].clone()],
        vec![claims[1].clone(), claims[0].clone()],
    ] {
        let before = creature.clone();
        let effect = ResolvedEffect::HealAndImproveHitDice {
            amount: 1,
            improvements: invalid,
            build_epoch: creature.build_epoch(),
        };
        assert!(apply(&mut creature, &effect).is_err());
        assert_eq!(creature, before);
    }
}

#[test]
fn double_credit_survives_drain_and_exact_restore_without_reusing_stale_epoch() {
    let mut initial = build(3);
    initial
        .set_future_growth(crate::earned_growth::FutureGrowth::Selected {
            source: crate::earned_growth::GrowthSource::Racial {},
        })
        .unwrap();
    let mut creature = CreatureState::new(initial).unwrap();
    let claims = creature.derived().next_health_improvements.clone();
    assert_eq!(claims.len(), 2);
    assert_eq!(
        claims
            .iter()
            .map(|claim| claim.ordinal.get())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let rows = creature.build().ledger().entries();
    let headroom = |index: usize| {
        let row = &rows[index];
        let die = row.health_origin().unwrap().die;
        let roll = crate::dice::die(&mut row.health_seed(), die);
        die - roll - row.health_improvement()
    };
    assert_eq!(
        headroom(1),
        1,
        "seed42's second acquired die owns exactly one credit"
    );
    assert!(
        headroom(2) >= 1,
        "third acquired die owns the second prefix credit"
    );

    let effect = ResolvedEffect::HealAndImproveHitDice {
        amount: 0,
        improvements: claims,
        build_epoch: creature.build_epoch(),
    };
    apply(&mut creature, &effect).unwrap();
    let improved = creature.build().ledger().entries()[2].clone();
    let maximum = creature.health().maximum();
    creature.drain_latest_hit_die().unwrap();
    let record: super::record::CreatureRecord = serde_json::from_slice(
        &serde_json::to_vec(&super::record::CreatureRecord::capture(&creature)).unwrap(),
    )
    .unwrap();
    let mut restored = record
        .restore(creature.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(restored.restore_hit_die().unwrap().applied, 1);
    assert_eq!(restored.build().ledger().entries()[2], improved);
    assert_eq!(restored.health().maximum(), maximum);
    let before = restored.clone();
    assert!(apply(&mut restored, &effect).is_err());
    assert_eq!(restored, before);
}

#[test]
fn extra_healing_uses_only_one_credit_when_the_acquisition_prefix_has_one_left() {
    let mut creature = CreatureState::new(build(2)).unwrap();
    let claims = creature.derived().next_health_improvements.clone();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].ordinal.get(), 1);
    let row = &creature.build().ledger().entries()[1];
    let die = row.health_origin().unwrap().die;
    assert_eq!(die - crate::dice::die(&mut row.health_seed(), die), 1);
    let maximum = creature.health().maximum();
    let epoch = creature.build_epoch();
    let planned = PlannedEffect::HealPerHitDie {
        dice: vec![crate::dice::DicePool::new(2, 8, 0).unwrap()],
        bonus: 100,
        overflow: HealingOverflow::ImproveEarliestHitDiceByExcess,
    };
    let outcome = resolve(
        input(std::slice::from_ref(&planned)),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert!(
        matches!(&outcome.effects[0], ResolvedEffect::HealAndImproveHitDice { improvements, .. } if improvements == &claims)
    );
    apply(&mut creature, &outcome.effects[0]).unwrap();
    assert_eq!(creature.health().maximum(), maximum + 1);
    assert_eq!(creature.build_epoch().get(), epoch.get() + 1);
    assert!(creature.derived().next_health_improvements.is_empty());
    let outcome = resolve(
        input(&[planned]),
        &Snapshot::capture(&creature),
        &mut 42,
        &mut (),
    )
    .unwrap();
    assert_eq!(outcome.effects, [ResolvedEffect::Heal { amount: 0 }]);
    assert_eq!(creature.health().maximum(), maximum + 1);
}
