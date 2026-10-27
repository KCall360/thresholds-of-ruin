//! Explicit opt-in marker, not a general modifier policy.
use super::{
    bound, ConditionDef, ConditionExpiry, ConditionLifecycle, ConditionPreparation,
    ConditionTiming, RulesetError, Stacking,
};

/// Called once during Ruleset validation, before definitions become immutable.
/// Ordinary unmarked conditions retain their existing validation contracts.
pub(super) fn validate_hallucination_conditions(
    definitions: &[ConditionDef],
) -> Result<(), RulesetError> {
    let mut marked = false;
    for definition in definitions
        .iter()
        .filter(|definition| definition.hallucinating)
    {
        bound(!marked, "hallucination_condition_unique")?;
        marked = true;
        bound(
            definition.lifecycle == ConditionLifecycle::Timed
                && definition.timing
                    == (ConditionTiming::ActorActive {
                        on_expiry: ConditionExpiry::Remove,
                    })
                && definition.stacking == Stacking::Extend
                && !definition.source_scoped
                && definition.modifiers.is_empty()
                && definition.restrictions.is_empty()
                && definition.descriptors.is_empty()
                && definition.preparation == ConditionPreparation::Preserve,
            "hallucination_condition_shape",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::*;

    fn definition() -> ConditionDef {
        serde_json::from_value(serde_json::json!({
            "id":"fixture_hallucination", "family":"fixture_hallucination",
            "name":"Disoriented", "hallucinating":true, "lifecycle":"timed",
            "modifiers":[], "stacking":{"kind":"extend"}, "source_scoped":false,
            "descriptors":[], "capacity":"reject",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
            "restrictions":[], "preparation":"preserve"
        }))
        .unwrap()
    }

    #[test]
    fn one_pure_marked_condition_is_valid_and_unmarked_authoring_defaults_false() {
        let mut document = crate::test_creatures::ruleset().document().clone();
        document.conditions.push(definition());
        Ruleset::validate(document.clone()).unwrap();
        let mut value = serde_json::to_value(definition()).unwrap();
        value.as_object_mut().unwrap().remove("hallucinating");
        assert!(
            !serde_json::from_value::<ConditionDef>(value)
                .unwrap()
                .hallucinating
        );
        let mut duplicate = definition();
        duplicate.id = crate::content_ids::ConditionId::new("other_hallucination").unwrap();
        duplicate.family =
            crate::content_ids::ConditionFamilyId::new("other_hallucination").unwrap();
        document.conditions.push(duplicate);
        assert!(
            matches!(Ruleset::validate(document), Err(RulesetError::Bound { field }) if field == "hallucination_condition_unique")
        );
    }

    #[test]
    fn marked_condition_rejects_each_mixed_lifecycle_shape_independently() {
        let baseline = definition();
        validate_hallucination_conditions(std::slice::from_ref(&baseline)).unwrap();
        let mut variants = Vec::new();
        for lifecycle in [
            ConditionLifecycle::NextCheck,
            ConditionLifecycle::NextCheckAgainst,
        ] {
            let mut row = baseline.clone();
            row.lifecycle = lifecycle;
            variants.push(row);
        }
        for timing in [
            ConditionTiming::UntilRemoved,
            ConditionTiming::ActorActiveOrUntilRemoved,
            ConditionTiming::ActorActive {
                on_expiry: ConditionExpiry::Protect {
                    duration: ConditionTicks::new(10).unwrap(),
                },
            },
        ] {
            let mut row = baseline.clone();
            row.timing = timing;
            variants.push(row);
        }
        for stacking in [
            Stacking::Keep,
            Stacking::Refresh,
            Stacking::Longest,
            Stacking::Stack { maximum: 2 },
        ] {
            let mut row = baseline.clone();
            row.stacking = stacking;
            variants.push(row);
        }
        let mut row = baseline.clone();
        row.source_scoped = true;
        variants.push(row);
        let mut row = baseline.clone();
        row.preparation = ConditionPreparation::Pause;
        variants.push(row);
        let mut row = baseline.clone();
        row.restrictions
            .push(ConditionRestriction::VoluntaryActions);
        variants.push(row);
        let mut row = baseline.clone();
        row.descriptors
            .push(crate::content_ids::DescriptorId::new("unrelated").unwrap());
        variants.push(row);
        let mut row = baseline;
        row.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Perception {
                value: crate::perception::PerceptionValue::SeeInvisible { enabled: true },
            },
        });
        variants.push(row);
        for row in variants {
            assert!(
                matches!(validate_hallucination_conditions(&[row]), Err(RulesetError::Bound { field }) if field == "hallucination_condition_shape")
            );
        }
    }

    #[test]
    fn marker_is_direct_harm_in_targeted_riders_and_frozen_planned_effects() {
        let mut document = crate::test_creatures::ruleset().document().clone();
        let condition = definition();
        let id = condition.id.clone();
        document.conditions.push(condition);
        let rules = Ruleset::validate(document).unwrap();
        let effect = Effect::ApplyConditionForTurns {
            condition: id.clone(),
            duration: AmountExpr::Fixed { amount: 10 },
        };
        let mut rows = crate::test_creatures::species().melee.strikes().to_vec();
        rows[0].on_success = vec![effect.clone()];
        rows[0].on_failure.clear();
        rows[0].riders.clear();
        rules
            .validate_interaction(Interaction::HostileToTarget, &rows)
            .unwrap();
        assert!(
            matches!(rules.validate_interaction(Interaction::NonHostile, &rows), Err(RulesetError::Bound { field }) if field == "non_hostile_direct_harm")
        );
        rows[0].on_success.clear();
        rows[0].riders.push(RiderDef {
            cancellable: false,
            condition: None,
            trigger: RiderTrigger::Success,
            check: None,
            effects: vec![effect],
            descriptors: vec![],
        });
        rules
            .validate_interaction(Interaction::HostileToTarget, &rows)
            .unwrap();
        assert!(
            matches!(rules.validate_interaction(Interaction::NonHostile, &rows), Err(RulesetError::Bound { field }) if field == "non_hostile_direct_harm")
        );
        let effects = [crate::abilities::PlannedEffect::ApplyConditionForTurns {
            id,
            duration: crate::abilities::PlannedAmount::Fixed { amount: 10 },
            turn_ticks: 100,
        }];
        rules
            .validate_planned_interaction(Interaction::HostileToTarget, effects.iter())
            .unwrap();
        assert!(
            matches!(rules.validate_planned_interaction(Interaction::NonHostile, effects.iter()), Err(RulesetError::Bound { field }) if field == "non_hostile_direct_harm")
        );
    }
}
