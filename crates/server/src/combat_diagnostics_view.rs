//! One generic projection of retained observer facts and authoritative outcomes.
use super::{condition, label, resource};
use s::{resolution_diagnostics as d, rules::Ruleset};
use tor_protocol as p;
use tor_simulation as s;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticProjectionError {
    InvalidPage,
    Inconsistent,
}
pub(crate) fn cost_bundle(value: s::costs::CostBundle) -> p::DiagnosticCostBundle {
    let mut entries = s::resources::ByResource::from_fn(|_| None);
    for cost in value.entries() {
        *entries.get_mut(cost.resource) = Some(p::DiagnosticCost {
            resource: resource(cost.resource),
            start: cost.start,
            resolution: cost.resolution,
        });
    }
    p::DiagnosticCostBundle {
        stamina: entries.stamina,
        focus: entries.focus,
        mana: entries.mana,
    }
}

fn step(value: &d::ResolutionRecord) -> p::CombatTraceStep {
    p::CombatTraceStep {
        id: value.id.clone(),
        name: label(&value.id),
        fields: value
            .fields
            .iter()
            .map(|field| p::DiagnosticField {
                id: field.id.clone(),
                name: label(&field.id),
                value: match &field.value {
                    d::TraceValueRecord::Text(value) => p::DiagnosticValue::Text {
                        value: value.clone(),
                    },
                    d::TraceValueRecord::Signed(value) => {
                        p::DiagnosticValue::Signed { value: *value }
                    }
                    d::TraceValueRecord::Unsigned(value) => {
                        p::DiagnosticValue::Unsigned { value: *value }
                    }
                    d::TraceValueRecord::Bool(value) => p::DiagnosticValue::Bool { value: *value },
                    d::TraceValueRecord::Dice(values) => p::DiagnosticValue::Dice {
                        values: values.clone(),
                    },
                },
            })
            .collect(),
    }
}
fn combatant(value: &d::CombatantDiagnostic, rules: &Ruleset) -> p::DiagnosticCombatant {
    p::DiagnosticCombatant {
        health: value.health,
        maximum_health: value.maximum_health,
        injury: value.injury,
        dead: value.dead,
        resources: value
            .resources
            .iter()
            .map(|value| p::DiagnosticResource {
                resource: resource(value.resource),
                maximum: value.maximum,
                balance: value.balance,
                available: value.available,
                recovery_elapsed: value.recovery_elapsed,
            })
            .collect(),
        conditions: value
            .conditions
            .iter()
            .filter(|entry| entry.reserved_by.is_none())
            .map(|value| p::ConditionView {
                condition: condition(&value.condition, rules),
                phase: match value.phase {
                    s::conditions::ConditionPhase::Active => p::ConditionPhase::Active,
                    s::conditions::ConditionPhase::Protection => p::ConditionPhase::Protection,
                },
                source: p::ActorId(value.source.0),
                remaining_ticks: value.remaining,
            })
            .collect(),
    }
}
fn record(
    value: &d::CombatDiagnostic,
    sequence: u64,
    rules: &Ruleset,
) -> Result<p::CombatDiagnosticRecord, DiagnosticProjectionError> {
    let ability = value
        .outcome
        .ability
        .as_ref()
        .map(|id| super::ability(id, rules));
    let target = p::ActorId(value.outcome.target.0);
    Ok(p::CombatDiagnosticRecord {
        sequence,
        tick: value.tick,
        actor: p::ActorId(value.actor.0),
        target: p::ActorId(value.target.0),
        ability,
        intention: value.intention.map(|id| id.0),
        origin_intention: value.origin_intention.map(|id| id.0),
        preparation: value.preparation,
        started: value.started,
        remaining: value.remaining,
        recovery: value.recovery,
        charge: value.charge.map(cost_bundle),
        actor_before: combatant(&value.actor_before, rules),
        target_before: combatant(&value.target_before, rules),
        actor_after: combatant(&value.actor_after, rules),
        target_after: combatant(&value.target_after, rules),
        outcome: p::DiagnosticOutcome {
            result: super::outcome_status(value.outcome.result),
            effects: value
                .outcome
                .effects
                .iter()
                .filter_map(|effect| {
                    Some(match effect {
                        s::abilities::ResolvedEffect::Theft { .. }
                        | s::abilities::ResolvedEffect::AlterEquipment { .. } => return None,
                        s::abilities::ResolvedEffect::CorrodeArmor { amount, .. } => {
                            p::DiagnosticEffect::CorrodeArmor {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::ApplyLinkedHold { .. } => {
                            p::DiagnosticEffect::LinkedHold {
                                target,
                                holder: p::ActorId(value.actor.0),
                            }
                        }
                        s::abilities::ResolvedEffect::Translate { mode, result } => {
                            p::DiagnosticEffect::Translate {
                                target,
                                mode: match mode {
                                    s::abilities::MovementMode::Voluntary => {
                                        p::DiagnosticMovementMode::Voluntary
                                    }
                                    s::abilities::MovementMode::Forced => {
                                        p::DiagnosticMovementMode::Forced
                                    }
                                },
                                result: match result {
                                    s::abilities::MovementResult::Moved { .. } => {
                                        p::DiagnosticMovementResult::Moved
                                    }
                                    s::abilities::MovementResult::Blocked => {
                                        p::DiagnosticMovementResult::Blocked
                                    }
                                    s::abilities::MovementResult::Ineligible => {
                                        p::DiagnosticMovementResult::Ineligible
                                    }
                                    s::abilities::MovementResult::Stale => {
                                        p::DiagnosticMovementResult::Stale
                                    }
                                    s::abilities::MovementResult::Ambiguous => {
                                        p::DiagnosticMovementResult::Ambiguous
                                    }
                                },
                            }
                        }
                        s::abilities::ResolvedEffect::Damage { amount } => {
                            p::DiagnosticEffect::Damage {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::Heal { amount } => {
                            p::DiagnosticEffect::Heal {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::HealAndImproveHitDice {
                            amount,
                            improvements,
                            ..
                        } => p::DiagnosticEffect::HealAndImproveHitDice {
                            target,
                            amount: *amount,
                            credits: improvements.len() as u32,
                        },
                        s::abilities::ResolvedEffect::LoseAttribute { attribute, amount } => {
                            p::DiagnosticEffect::LoseAttribute {
                                target,
                                attribute: super::attribute(*attribute),
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::RestoreAttribute { attribute, amount } => {
                            p::DiagnosticEffect::RestoreAttribute {
                                target,
                                attribute: super::attribute(*attribute),
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::DrainLatestHitDie { amount } => {
                            p::DiagnosticEffect::DrainLatestHitDie {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::RestoreHitDie { amount } => {
                            p::DiagnosticEffect::RestoreHitDie {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::LoseMaximumHealth { amount } => {
                            p::DiagnosticEffect::LoseMaximumHealth {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::RestoreMaximumHealth { amount } => {
                            p::DiagnosticEffect::RestoreMaximumHealth {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::Nourish { amount } => {
                            p::DiagnosticEffect::Nourish {
                                target,
                                amount: *amount,
                            }
                        }
                        s::abilities::ResolvedEffect::Restore {
                            resource: value,
                            amount,
                        } => p::DiagnosticEffect::Restore {
                            target,
                            resource: resource(*value),
                            amount: *amount,
                        },
                        s::abilities::ResolvedEffect::RemoveCondition { id } => {
                            p::DiagnosticEffect::ConditionRemoved {
                                target,
                                condition: condition(id, rules),
                            }
                        }
                        s::abilities::ResolvedEffect::ApplyPersistentCondition { id } => {
                            p::DiagnosticEffect::Condition {
                                target,
                                condition: condition(id, rules),
                                source: p::ActorId(value.outcome.actor.0),
                                duration: None,
                            }
                        }
                        s::abilities::ResolvedEffect::ApplyCondition { id, duration } => {
                            p::DiagnosticEffect::Condition {
                                target,
                                condition: condition(id, rules),
                                source: p::ActorId(value.outcome.actor.0),
                                duration: Some(*duration),
                            }
                        }
                        s::abilities::ResolvedEffect::SearchAdjacentGuises { .. }
                        | s::abilities::ResolvedEffect::ObservePublicPlan { .. } => {
                            p::DiagnosticEffect::Observation {
                                target: p::ActorId(value.outcome.actor.0),
                            }
                        }
                        s::abilities::ResolvedEffect::ApplyQualifiedConditionToSource {
                            id,
                            duration,
                            qualification,
                        } => p::DiagnosticEffect::Condition {
                            target: p::ActorId(value.outcome.actor.0),
                            condition: condition(id, rules),
                            source: p::ActorId(qualification.opponent.0),
                            duration: Some(*duration),
                        },
                    })
                })
                .collect(),
        },
        trace: p::CombatTraceView {
            steps: value.trace.steps().iter().map(step).collect(),
            truncated: value.trace.truncated(),
        },
    })
}
pub(crate) fn combat_diagnostics(
    game: &s::Game,
    through: Option<u64>,
) -> Result<p::CombatDiagnosticsView, DiagnosticProjectionError> {
    let capture = game.combat_diagnostics();
    let retained = capture.map_or(0, |capture| capture.records().len());
    let retained_u8 =
        u8::try_from(retained).map_err(|_| DiagnosticProjectionError::Inconsistent)?;
    let dropped = capture.map_or(0, |capture| capture.dropped());
    let captured = dropped
        .checked_add(retained as u64)
        .ok_or(DiagnosticProjectionError::Inconsistent)?;
    let through = through.unwrap_or(captured);
    if through < dropped || through > captured {
        return Err(DiagnosticProjectionError::InvalidPage);
    }
    let end = (through - dropped) as usize;
    let start = end.saturating_sub(p::MAX_COMBAT_REPORT_RECORDS);
    let records = if let Some(capture) = capture {
        capture
            .records()
            .iter()
            .skip(start)
            .take(end - start)
            .enumerate()
            .map(|(index, value)| {
                record(
                    value,
                    dropped + start as u64 + index as u64 + 1,
                    game.content().rules(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let view = p::CombatDiagnosticsView {
        enabled: capture.is_some(),
        tick: game.tick(),
        captured,
        dropped,
        retained: retained_u8,
        through,
        records,
    };
    view.validate()
        .map_err(|_| DiagnosticProjectionError::Inconsistent)?;
    Ok(view)
}

#[cfg(test)]
mod token_disclosure_tests {
    use super::*;
    #[test]
    fn spent_reservations_and_their_private_owners_are_not_disclosed_conditions() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let conditions = vec![
            s::conditions::ConditionInstance {
                phase: s::conditions::ConditionPhase::Active,
                condition: s::content_ids::ConditionId::new("readied").unwrap(),
                source: s::ActorId(1),
                remaining: Some(0),
                stacks: 1,
                order: 0,
                generation: 0,
                reserved_by: Some(s::ActorId(999)),
                qualification: None,
            },
            s::conditions::ConditionInstance {
                phase: s::conditions::ConditionPhase::Active,
                condition: s::content_ids::ConditionId::new("braced").unwrap(),
                source: s::ActorId(1),
                remaining: Some(30),
                stacks: 1,
                order: 1,
                generation: 1,
                reserved_by: None,
                qualification: None,
            },
        ];
        let disclosed = combatant(
            &d::CombatantDiagnostic {
                health: 5,
                maximum_health: 5,
                injury: Some(0),
                dead: false,
                resources: vec![],
                conditions,
            },
            &rules,
        );
        assert_eq!(disclosed.conditions.len(), 1);
        assert_eq!(disclosed.conditions[0].condition.id, "braced");
        assert_eq!(disclosed.conditions[0].remaining_ticks, Some(30));
        let text = serde_json::to_string(&disclosed).unwrap();
        assert!(!text.contains("reserved_by"));
        assert!(!text.contains("999"));
    }
}

#[cfg(test)]
mod restorable_loss_diagnostic_tests {
    use super::*;

    fn diagnostic(effects: Vec<s::abilities::ResolvedEffect>) -> d::CombatDiagnostic {
        let state = d::CombatantDiagnostic {
            health: 10,
            maximum_health: 10,
            injury: Some(0),
            dead: false,
            resources: vec![],
            conditions: vec![],
        };
        d::CombatDiagnostic {
            tick: 0,
            actor: s::ActorId(1),
            target: s::ActorId(2),
            intention: None,
            origin_intention: None,
            work: s::Work::Drink { item: s::ItemId(1) },
            preparation: 0,
            started: 0,
            remaining: 0,
            recovery: 0,
            charge: None,
            actor_before: state.clone(),
            target_before: state.clone(),
            actor_after: state.clone(),
            target_after: state,
            outcome: s::abilities::Outcome {
                actor: s::ActorId(1),
                target: s::ActorId(2),
                ability: None,
                intention: None,
                result: s::abilities::OutcomeStatus::Applied,
                check: None,
                participation: s::abilities::AttemptedParticipation::source(),
                target_rider_participated: false,
                effects,
            },
            trace: d::ResolutionTrace::default(),
        }
    }

    #[test]
    fn authoritative_loss_results_project_and_render_actual_applied_amounts() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let diagnostic = diagnostic(vec![
            s::abilities::ResolvedEffect::LoseAttribute {
                attribute: s::attributes::Attribute::Strength,
                amount: 1,
            },
            s::abilities::ResolvedEffect::RestoreAttribute {
                attribute: s::attributes::Attribute::Strength,
                amount: 0,
            },
            s::abilities::ResolvedEffect::LoseMaximumHealth { amount: 3 },
            s::abilities::ResolvedEffect::RestoreMaximumHealth { amount: 2 },
            s::abilities::ResolvedEffect::DrainLatestHitDie { amount: 1 },
            s::abilities::ResolvedEffect::RestoreHitDie { amount: 0 },
            s::abilities::ResolvedEffect::Theft {
                quantity: 987654321,
                relocation: Some(s::abilities::Placement {
                    location: tor_world::Location {
                        region: tor_world::RegionId(987654321),
                        position: tor_world::Position {
                            x: 731,
                            y: 947,
                            z: 0,
                        },
                    },
                    orientation: 3,
                }),
            },
        ]);
        let record = record(&diagnostic, 1, &rules).unwrap();
        let encoded = serde_json::to_value(&record.outcome.effects).unwrap();
        assert_eq!(
            record.outcome.effects.len(),
            6,
            "private theft receipts have no diagnostic wire variant"
        );
        let disclosure = serde_json::to_string(&record).unwrap();
        for private in ["theft", "987654321", "relocation", "orientation"] {
            assert!(
                !disclosure.contains(private),
                "private receipt leaked {private}"
            );
        }
        assert_eq!(
            encoded[0],
            serde_json::json!({
                "type":"lose_attribute","target":"2","attribute":{"id":"strength","name":"Strength"},"amount":1
            })
        );
        assert_eq!(encoded[1]["amount"], 0);
        let report = p::CombatDiagnosticsView {
            enabled: true,
            tick: 0,
            captured: 1,
            dropped: 0,
            retained: 1,
            through: 1,
            records: vec![record],
        };
        let text = tor_client_common::combat_diagnostics::lines(&report)
            .unwrap()
            .join("\n");
        for expected in [
            "Actor 2: 1 restorable Strength loss",
            "Actor 2: 0 Strength restored",
            "Actor 2: 3 restorable maximum Health loss",
            "Actor 2: 2 maximum Health restored",
            "Actor 2: 1 latest Hit Die drained",
            "Actor 2: 0 Hit Die restored",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn persistent_diagnostics_project_explicit_null_for_effect_and_active_status() {
        let mut document: s::rules::RulesetDocument =
            toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
        let mut definition = document
            .conditions
            .iter()
            .find(|condition| condition.id.as_str() == "braced")
            .unwrap()
            .clone();
        let id = s::content_ids::ConditionId::new("fixture_persistent").unwrap();
        definition.id = id.clone();
        definition.family = s::content_ids::ConditionFamilyId::new("fixture_persistent").unwrap();
        definition.name = "Fixture Persistent".into();
        definition.timing = s::rules::ConditionTiming::UntilRemoved;
        definition.stacking = s::rules::Stacking::Keep;
        document.conditions.push(definition);
        let rules = Ruleset::validate(document).unwrap();
        let mut diagnostic = diagnostic(vec![
            s::abilities::ResolvedEffect::ApplyPersistentCondition { id: id.clone() },
        ]);
        diagnostic
            .target_after
            .conditions
            .push(s::conditions::ConditionInstance {
                phase: s::conditions::ConditionPhase::Active,
                condition: id,
                source: s::ActorId(1),
                remaining: None,
                stacks: 1,
                order: 0,
                generation: 7,
                reserved_by: None,
                qualification: None,
            });
        let projected = record(&diagnostic, 1, &rules).unwrap();
        let encoded = serde_json::to_value(&projected).unwrap();
        assert_eq!(
            encoded["outcome"]["effects"],
            serde_json::json!([{
                "type":"condition", "target":"2", "source":"1", "duration":null,
                "condition":{"id":"fixture_persistent","name":"Fixture Persistent"}
            }])
        );
        assert_eq!(
            encoded["target_after"]["conditions"],
            serde_json::json!([{
                "condition":{"id":"fixture_persistent","name":"Fixture Persistent"},
                "phase":"active", "source":"1", "remaining_ticks":null
            }])
        );
        let report = p::CombatDiagnosticsView {
            enabled: true,
            tick: 0,
            captured: 1,
            dropped: 0,
            retained: 1,
            through: 1,
            records: vec![projected],
        };
        report.validate().unwrap();
        assert_eq!(
            serde_json::from_value::<p::CombatDiagnosticsView>(
                serde_json::to_value(&report).unwrap()
            )
            .unwrap(),
            report
        );
    }

    #[test]
    fn plural_healing_diagnostics_project_two_credits_without_private_claims() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let effect = s::abilities::ResolvedEffect::HealAndImproveHitDice {
            amount: 8,
            improvements: (1..=2)
                .map(|ordinal| s::progression::HitDieHealthImprovement {
                    ordinal: s::HdIndex(ordinal),
                    source: s::progression::HdSource::Racial,
                    health_seed: 42,
                    original_kind: s::content_ids::TypeId::new("humanoid").unwrap(),
                    original_die: 8,
                    previous: 0,
                })
                .collect(),
            build_epoch: s::creatures::BuildEpoch::new(987654321).unwrap(),
        };
        // The projection fixture itself must satisfy the current strict receipt shape.
        assert_eq!(
            serde_json::from_value::<s::abilities::ResolvedEffect>(
                serde_json::to_value(&effect).unwrap()
            )
            .unwrap(),
            effect
        );
        let projected = record(&diagnostic(vec![effect]), 1, &rules).unwrap();
        assert_eq!(
            serde_json::to_value(&projected.outcome.effects).unwrap(),
            serde_json::json!([{
                "type":"heal_and_improve_hit_dice", "target":"2", "amount":8, "credits":2
            }])
        );
        let report = p::CombatDiagnosticsView {
            enabled: true,
            tick: 0,
            captured: 1,
            dropped: 0,
            retained: 1,
            through: 1,
            records: vec![projected],
        };
        report.validate().unwrap();
        let encoded = serde_json::to_string(&report).unwrap();
        for private in [
            "improvements",
            "health_seed",
            "original_kind",
            "original_die",
            "previous",
            "ordinal",
            "build_epoch",
            "987654321",
        ] {
            assert!(
                !encoded.contains(private),
                "private healing claim leaked {private}"
            );
        }
    }
}
