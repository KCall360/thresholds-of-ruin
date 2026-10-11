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
            "  {} from actor {}: {} active ticks",
            condition.condition.name, condition.source.0, condition.remaining_ticks
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
        if let Some(cost) = &record.charge {
            rows.push(format!(
                "  Cost {}: start {}, resolution {}",
                cost.resource.name, cost.start, cost.resolution
            ));
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
                DiagnosticEffect::Damage { target, amount } => {
                    format!("  Actor {}: {amount} damage", target.0)
                }
                DiagnosticEffect::Heal { target, amount } => {
                    format!("  Actor {}: {amount} healing", target.0)
                }
                DiagnosticEffect::Restore {
                    target,
                    resource,
                    amount,
                } => format!("  Actor {}: {amount} {} restored", target.0, resource.name),
                DiagnosticEffect::Condition {
                    target,
                    condition,
                    source,
                    duration,
                } => format!(
                    "  Actor {}: {} from {} for {duration} ticks",
                    target.0, condition.name, source.0
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
}
