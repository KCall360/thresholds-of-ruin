//! Generic diagnostic rendering uses disclosed labels and captured values only.
use tor_protocol::*;
pub fn trace_lines(step: &CombatTraceStep) -> Vec<String> {
    let mut rows = vec![step.name.clone()];
    rows.extend(step.fields.iter().map(|field| {
        let value = match &field.value {
            DiagnosticValue::Text { value } => value.clone(),
            DiagnosticValue::Signed { value } => value.to_string(),
            DiagnosticValue::Unsigned { value } => value.to_string(),
            DiagnosticValue::Bool { value } => value.to_string(),
            DiagnosticValue::Dice { values } => values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            DiagnosticValue::Content { value } => format!("{} ({})", value.name, value.id),
        };
        format!("  {}: {value}", field.name)
    }));
    rows
}
fn state(rows: &mut Vec<String>, label: &str, value: &DiagnosticCombatant) {
    rows.push(format!(
        "{label}: HP {}/{}, injury {}, dead {}",
        value.health,
        value.maximum_health,
        value
            .injury
            .map_or_else(|| "unavailable".into(), |v| v.to_string()),
        value.dead
    ));
    for pool in &value.resources {
        rows.push(format!(
            "  {}: balance {}/{}, available {}, recovery elapsed {}",
            pool.resource.name, pool.balance, pool.maximum, pool.available, pool.recovery_elapsed
        ));
    }
    for condition in &value.conditions {
        rows.push(format!(
            "  {} from actor {}: {}",
            crate::inspection::condition_label(condition),
            condition.source.0,
            crate::inspection::condition_lifetime(condition.remaining_ticks)
        ));
    }
}
pub fn cost_lines(rows: &mut Vec<String>, bundle: &DiagnosticCostBundle) {
    for cost in bundle.entries() {
        rows.push(format!(
            "  Cost {}: start {}, resolution {}",
            cost.resource.name, cost.start, cost.resolution
        ));
    }
}
pub fn lines(report: &CombatDiagnosticsView) -> Result<Vec<String>, InvalidCombatDiagnostics> {
    report.validate()?;
    let mut rows = vec![format!(
        "Combat diagnostics: {}; captured {}, dropped {}, retained {}",
        if report.enabled {
            "enabled"
        } else {
            "disabled"
        },
        report.captured,
        report.dropped,
        report.retained
    )];
    for record in &report.records {
        rows.push(format!(
            "Record {} at tick {}: actor {} -> {}; {}: {:?}",
            record.sequence,
            record.tick,
            record.actor.0,
            record.target.0,
            record
                .ability
                .as_ref()
                .map_or("Effect", |ability| ability.name.as_str()),
            record.outcome.result
        ));
        rows.push(format!(
            "  Intention {:?}, origin {:?}; preparation {}, started {}, remaining {}, recovery {}",
            record.intention,
            record.origin_intention,
            record.preparation,
            record.started,
            record.remaining,
            record.recovery
        ));
        if let Some(bundle) = &record.charge {
            cost_lines(&mut rows, bundle);
        }
        state(&mut rows, "Actor before", &record.actor_before);
        state(&mut rows, "Target before", &record.target_before);
        for step in &record.trace.steps {
            rows.extend(trace_lines(step));
        }
        if record.trace.truncated {
            rows.push("Trace truncated.".into());
        }
        for effect in &record.outcome.effects {
            rows.push(match effect {
                DiagnosticEffect::LinkedHold { target, holder } => {
                    format!("  Actor {}: linked hold from actor {}", target.0, holder.0)
                }
                DiagnosticEffect::Observation { target } => {
                    format!("  Actor {}: public-plan observation recorded", target.0)
                }
                DiagnosticEffect::Translate {
                    target,
                    mode,
                    result,
                } => format!("  Actor {}: {:?} movement, {:?}", target.0, mode, result),
                DiagnosticEffect::Relocate { target, mode, result } => format!("  Actor {}: {:?} relocation, {:?}", target.0, mode, result),
                DiagnosticEffect::HasteDose { target, condition, rest, outcome } => match outcome {
                    HasteDoseOutcome::Hasted { duration } => format!("  Actor {}: {} for {duration} active ticks", target.0, condition.name),
                    HasteDoseOutcome::Overdose { rest: duration } => format!("  Actor {}: haste overdose; {} for {duration} active ticks", target.0, rest.name),
                },
                DiagnosticEffect::DrainLife { target, changed } => format!("  Actor {}: drain life {}", target.0, if *changed { "changed" } else { "unchanged" }),
                DiagnosticEffect::DirectHealthLoss { target, amount } => format!("  Actor {}: {amount} direct health loss", target.0),
                DiagnosticEffect::CancelSpecials { target, changed } => format!("  Actor {}: special capabilities {}", target.0, if *changed { "cancelled" } else { "unchanged" }),
                DiagnosticEffect::RoomLight { target, changed } => format!("  Actor {}: room light {}", target.0, if *changed { "changed" } else { "unchanged" }),
                DiagnosticEffect::ArrestDescent { target } => format!("  Actor {}: descent arrested", target.0),
                DiagnosticEffect::CorrodeArmor { target, amount } => {
                    format!("  Actor {}: armor protection reduced by {amount}", target.0)
                }
                DiagnosticEffect::Damage { target, amount } => {
                    format!("  Actor {}: {amount} damage", target.0)
                }
                DiagnosticEffect::Heal { target, amount } => {
                    format!("  Actor {}: {amount} healing", target.0)
                }
                DiagnosticEffect::HealAndImproveHitDice { target, amount, credits } => {
                    format!(
                        "  Actor {}: {amount} healing; owned Hit Die Health rolls improved by {credits}",
                        target.0
                    )
                }
                DiagnosticEffect::LoseAttribute {
                    target,
                    attribute,
                    amount,
                } => {
                    format!(
                        "  Actor {}: {amount} restorable {} loss",
                        target.0, attribute.name
                    )
                }
                DiagnosticEffect::RestoreAttribute {
                    target,
                    attribute,
                    amount,
                } => {
                    format!("  Actor {}: {amount} {} restored", target.0, attribute.name)
                }
                DiagnosticEffect::DrainLatestHitDie { target, amount } => {
                    format!("  Actor {}: {amount} latest Hit Die drained", target.0)
                }
                DiagnosticEffect::RestoreHitDie { target, amount } => {
                    format!("  Actor {}: {amount} Hit Die restored", target.0)
                }
                DiagnosticEffect::LoseMaximumHealth { target, amount } => {
                    format!(
                        "  Actor {}: {amount} restorable maximum Health loss",
                        target.0
                    )
                }
                DiagnosticEffect::RestoreMaximumHealth { target, amount } => {
                    format!("  Actor {}: {amount} maximum Health restored", target.0)
                }
                DiagnosticEffect::Nourish { target, amount } => {
                    format!("  Actor {}: {amount} nutrition", target.0)
                }
                DiagnosticEffect::Restore {
                    target,
                    resource,
                    amount,
                } => format!("  Actor {}: {amount} {} restored", target.0, resource.name),
                DiagnosticEffect::ConditionRemoved { target, condition } => {
                    format!("  Actor {}: {} removed", target.0, condition.name)
                }
                DiagnosticEffect::Condition {
                    target,
                    condition,
                    source,
                    duration,
                } => format!(
                    "  Actor {}: {} from {}: {}",
                    target.0, condition.name, source.0, crate::inspection::condition_lifetime(*duration)
                ),
            });
        }
        state(&mut rows, "Actor after", &record.actor_after);
        state(&mut rows, "Target after", &record.target_after);
    }
    if let Some(first) = report
        .records
        .first()
        .filter(|r| r.sequence > report.dropped + 1)
    {
        rows.push(format!(
            "Older records: wizard combat inspect {}",
            first.sequence - 1
        ));
    }
    Ok(rows)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn report() -> CombatDiagnosticsView {
        CombatDiagnosticsView {
            enabled: false,
            tick: 0,
            captured: 0,
            dropped: 0,
            retained: 0,
            through: 0,
            records: vec![],
        }
    }
    #[test]
    fn validates_before_formatting() {
        let mut report = report();
        assert!(lines(&report).unwrap().join("\n").contains("disabled"));
        report.retained = 65;
        assert!(lines(&report).is_err());
    }
    #[test]
    fn arbitrary_trace_labels_and_large_integers_render_without_catalogs() {
        let step = CombatTraceStep {
            id: "custom".into(),
            name: "Custom mechanism".into(),
            fields: vec![DiagnosticField {
                id: "seed".into(),
                name: "Seed".into(),
                value: DiagnosticValue::Unsigned { value: u64::MAX },
            }],
        };
        assert_eq!(
            trace_lines(&step),
            ["Custom mechanism", "  Seed: 18446744073709551615"]
        );
    }
    #[test]
    fn mixed_cost_bundle_renders_every_captured_resource_once() {
        let cost = |id: &str, name: &str, start, resolution| DiagnosticCost {
            resource: ContentRef {
                id: id.into(),
                name: name.into(),
            },
            start,
            resolution,
        };
        let bundle = DiagnosticCostBundle {
            stamina: Some(cost("stamina", "Stamina", 1, 2)),
            focus: Some(cost("focus", "Focus", 0, 1)),
            mana: None,
        };
        let mut rows = vec![];
        cost_lines(&mut rows, &bundle);
        assert_eq!(
            rows,
            [
                "  Cost Stamina: start 1, resolution 2",
                "  Cost Focus: start 0, resolution 1"
            ]
        );
    }

    fn report_with_effects(effects: Vec<DiagnosticEffect>) -> CombatDiagnosticsView {
        let combatant = DiagnosticCombatant {
            health: 10,
            maximum_health: 10,
            injury: Some(0),
            dead: false,
            resources: vec![],
            conditions: vec![],
        };
        let mut report = report();
        report.enabled = true;
        report.tick = 10;
        report.captured = 1;
        report.retained = 1;
        report.through = 1;
        report.records.push(CombatDiagnosticRecord {
            sequence: 1,
            tick: 10,
            actor: ActorId(1),
            target: ActorId(2),
            ability: None,
            intention: None,
            origin_intention: None,
            preparation: 5,
            started: 5,
            remaining: 5,
            recovery: 2,
            charge: None,
            actor_before: combatant.clone(),
            target_before: combatant.clone(),
            actor_after: combatant.clone(),
            target_after: combatant,
            outcome: DiagnosticOutcome {
                result: AbilityOutcome::Applied,
                effects,
            },
            trace: CombatTraceView {
                steps: vec![],
                truncated: false,
            },
        });
        report
    }

    #[test]
    fn condition_diagnostics_render_persistent_and_timed_lifetimes_and_captured_timing() {
        for (remaining, lifetime) in [
            (None, "until removed"),
            (Some(35), "35 active ticks remaining"),
        ] {
            let condition = ContentRef {
                id: "fixture_condition".into(),
                name: "Fixture condition".into(),
            };
            let mut report = report_with_effects(vec![DiagnosticEffect::Condition {
                target: ActorId(2),
                condition: condition.clone(),
                source: ActorId(1),
                duration: remaining,
            }]);
            report.records[0]
                .target_after
                .conditions
                .push(ConditionView {
                    condition,
                    phase: ConditionPhase::Active,
                    source: ActorId(1),
                    remaining_ticks: remaining,
                });
            let rows = lines(&report).unwrap();
            assert!(rows.contains(&format!("  Fixture condition from actor 1: {lifetime}")));
            assert!(rows.contains(&format!("  Actor 2: Fixture condition from 1: {lifetime}")));
            assert!(rows
                .iter()
                .any(|row| row.contains("preparation 5, started 5, remaining 5, recovery 2")));
        }
    }

    #[test]
    fn cancellation_diagnostic_targets_recipient_not_source() {
        for changed in [false, true] {
            let report = report_with_effects(vec![DiagnosticEffect::CancelSpecials {
                target: ActorId(2),
                changed,
            }]);
            assert!(lines(&report).is_ok());
            let mut forged = report.clone();
            forged.records[0].outcome.effects = vec![DiagnosticEffect::CancelSpecials {
                target: ActorId(1),
                changed,
            }];
            assert!(lines(&forged).is_err());
        }
    }

    #[test]
    fn drain_diagnostics_keep_full_width_and_source_target_roles() {
        let report = report_with_effects(vec![
            DiagnosticEffect::DrainLife {
                target: ActorId(1),
                changed: true,
            },
            DiagnosticEffect::DirectHealthLoss {
                target: ActorId(2),
                amount: u64::MAX,
            },
        ]);
        let rows = lines(&report).unwrap();
        assert!(rows.contains(&"  Actor 2: 18446744073709551615 direct health loss".to_owned()));
        let mut forged = report.clone();
        forged.records[0].outcome.effects = vec![DiagnosticEffect::DrainLife {
            target: ActorId(2),
            changed: true,
        }];
        assert!(lines(&forged).is_err());
        forged.records[0].outcome.effects = vec![DiagnosticEffect::DirectHealthLoss {
            target: ActorId(1),
            amount: 0,
        }];
        assert!(lines(&forged).is_err());
    }

    #[test]
    fn haste_dose_diagnostics_render_both_branches_and_reject_forged_recipients() {
        let condition = ContentRef {
            id: "rogue.hasted".into(),
            name: "Hasted".into(),
        };
        let rest = ContentRef {
            id: "rogue.haste_exhaustion".into(),
            name: "Exhausted".into(),
        };
        for (outcome, expected) in [
            (
                HasteDoseOutcome::Hasted { duration: 400 },
                "  Actor 2: Hasted for 400 active ticks",
            ),
            (
                HasteDoseOutcome::Overdose { rest: 0 },
                "  Actor 2: haste overdose; Exhausted for 0 active ticks",
            ),
            (
                HasteDoseOutcome::Overdose { rest: 700 },
                "  Actor 2: haste overdose; Exhausted for 700 active ticks",
            ),
        ] {
            let report = report_with_effects(vec![DiagnosticEffect::HasteDose {
                target: ActorId(2),
                condition: condition.clone(),
                rest: rest.clone(),
                outcome,
            }]);
            assert!(lines(&report).unwrap().contains(&expected.to_string()));
            for invalid in 0..3 {
                let mut forged = report.clone();
                let DiagnosticEffect::HasteDose {
                    target,
                    condition,
                    rest,
                    outcome,
                } = &mut forged.records[0].outcome.effects[0]
                else {
                    unreachable!()
                };
                match invalid {
                    0 => *target = ActorId(1),
                    1 => *rest = condition.clone(),
                    _ => *outcome = HasteDoseOutcome::Hasted { duration: 0 },
                }
                assert!(lines(&forged).is_err());
            }
        }
    }

    #[test]
    fn room_light_diagnostics_distinguish_changed_from_paid_noop() {
        for (changed, expected) in [(true, "changed"), (false, "unchanged")] {
            let report = report_with_effects(vec![DiagnosticEffect::RoomLight {
                target: ActorId(1),
                changed,
            }]);
            let rows = lines(&report).unwrap();
            assert!(rows.contains(&format!("  Actor 1: room light {expected}")));
            let mut forged = report.clone();
            forged.records[0].outcome.effects = vec![DiagnosticEffect::RoomLight {
                target: ActorId(2),
                changed,
            }];
            assert!(
                lines(&forged).is_err(),
                "room light cannot claim a nominal recipient instead of its source"
            );
        }
    }

    #[test]
    fn healing_diagnostics_render_the_disclosed_one_or_two_credits() {
        for credits in [1, 2] {
            let report = report_with_effects(vec![DiagnosticEffect::HealAndImproveHitDice {
                target: ActorId(2),
                amount: 8,
                credits,
            }]);
            let rows = lines(&report).unwrap();
            assert_eq!(
                rows.iter()
                    .filter(|row| row.contains("healing"))
                    .collect::<Vec<_>>(),
                vec![&format!(
                    "  Actor 2: 8 healing; owned Hit Die Health rolls improved by {credits}"
                )]
            );
        }
    }
}
