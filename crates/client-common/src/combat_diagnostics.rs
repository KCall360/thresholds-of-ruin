//! Validated private combat reports shared by Text and ASCII presentation.
use tor_protocol::{
    CombatDiagnosticsView, CombatTraceStep, DiagnosticCombatant, InvalidCombatDiagnostics,
};

fn state(rows: &mut Vec<String>, label: &str, value: &DiagnosticCombatant) {
    rows.push(format!(
        "{label}: HP {}/{}, injury {}, dead {}",
        value.health,
        value.maximum_health,
        value
            .injury
            .map_or_else(|| "legacy".into(), |v| v.to_string()),
        value.dead
    ));
    for resource in &value.resources {
        rows.push(format!(
            "  {:?}: balance {}/{}, available {}, recovery elapsed {}",
            resource.resource,
            resource.balance,
            resource.maximum,
            resource.available,
            resource.recovery_elapsed
        ));
    }
    for fear in &value.fear {
        rows.push(format!(
            "  Fear from actor {}: {} active ticks",
            fear.causer.0, fear.remaining_ticks
        ));
    }
}

fn step(rows: &mut Vec<String>, value: &CombatTraceStep) {
    match value {
        CombatTraceStep::AttackStarted { net_edge } => rows.push(format!("Attack: net edge {net_edge}")),
        CombatTraceStep::Check { check } => {
            rows.push(format!("Check {:?} ({:?}, {:?}): attribute {} + rank {} + modifier {} vs {}",
                check.skill, check.attribute, check.binding, check.attribute_value, check.rank, check.modifier, check.threshold));
            rows.push(format!("  d20 {}, second {}, kept {}, total {}, success {}", check.first,
                check.second.map_or_else(|| "none".into(), |v| v.to_string()), check.kept, check.total, check.success));
            rows.push(format!("  Edge input {}, allocated {}, unused {}; RNG {} -> {}",
                check.input_edge, check.edge, check.unused_edge, check.rng_before, check.rng_after));
        }
        CombatTraceStep::AttackMissed { unused_edge } => rows.push(format!("Miss: unused edge {unused_edge}")),
        CombatTraceStep::DamageStarted { net_edge } => rows.push(format!("Damage: net edge {net_edge}")),
        CombatTraceStep::Component { component } => {
            let dice = component.expression.map_or_else(|| "fixed".into(), |value|
                format!("{}d{}{:+}", value.count, value.sides, value.bonus));
            let descriptor = component.descriptor.map_or_else(|| "none".into(), |value| format!("{value:?}"));
            rows.push(format!("Component {:?}, descriptor {}, dice {}: rolled {:?}, kept {:?}, raw {}",
                component.category, descriptor, dice, component.rolled, component.kept, component.raw));
            rows.push(format!("  Edge {}; RNG {} -> {}; immunity category {}, descriptor {}; after immunity {}",
                component.edge, component.rng_before, component.rng_after, component.category_immune,
                component.descriptor_immune, component.after_immunity));
        }
        CombatTraceStep::Reduction { selector, category, before, capacity, after } => rows.push(format!(
            "Reduction {selector:?}/{category:?}: {before}, capacity {capacity}, remaining {after}")),
        CombatTraceStep::DamageFinished { raw, after_immunity, after_descriptors, total, unused_edge } => rows.push(format!(
            "Damage totals: raw {raw}, after immunity {after_immunity}, after descriptors {after_descriptors}, protected {total}; unused edge {unused_edge}")),
        CombatTraceStep::FearStarted { difficulty, duration, net_edge, difficulty_attribute, difficulty_rank, difficulty_bonus, duration_bonus } => rows.push(format!(
            "Fear: difficulty {difficulty} = 10 + Willpower {difficulty_attribute} + rank {difficulty_rank} + bonus {difficulty_bonus}; duration {duration} = 300 + bonus {duration_bonus}; edge {net_edge}")),
        CombatTraceStep::FearImmunity { fear, mind_affecting } => rows.push(format!(
            "Fear immunity: fear {fear}, mind affecting {mind_affecting}")),
        CombatTraceStep::FearFinished { applied, duration } => rows.push(format!("Fear applied {applied}, duration {duration}")),
    }
}

pub fn lines(report: &CombatDiagnosticsView) -> Result<Vec<String>, InvalidCombatDiagnostics> {
    report.validate()?;
    let mut rows = vec![format!(
        "Combat diagnostics: capture {}, tick {}, captured {}, retained {}, dropped {}, through {}",
        if report.enabled { "on" } else { "off" },
        report.tick,
        report.captured,
        report.retained,
        report.dropped,
        report.through
    )];
    for record in &report.records {
        rows.push(format!(
            "Record {}: tick {}, actor {} -> actor {}, {:?}, applied {}, HP lost {}",
            record.sequence,
            record.tick,
            record.actor.0,
            record.target.0,
            record.ability,
            record.applied,
            record.damage
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
        if let Some(cost) = record.charge {
            rows.push(format!(
                "  Cost {:?}: start {}, resolution {}",
                cost.resource, cost.start, cost.resolution
            ));
        }
        state(&mut rows, "Actor before", &record.actor_before);
        state(&mut rows, "Target before", &record.target_before);
        for value in &record.trace.steps {
            step(&mut rows, value);
        }
        if record.trace.truncated {
            rows.push("Trace truncated.".into());
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
    fn validates_before_formatting_and_shows_empty_capture_state() {
        let mut report = CombatDiagnosticsView {
            enabled: false,
            tick: 0,
            captured: 0,
            dropped: 0,
            retained: 0,
            through: 0,
            records: vec![],
        };
        assert!(lines(&report).unwrap()[0].contains("capture off"));
        report.enabled = true;
        assert!(lines(&report).unwrap()[0].contains("capture on"));
        report.retained = 1;
        assert!(lines(&report).is_err());
    }
    #[test]
    fn check_rows_preserve_draws_operands_edge_and_rng() {
        let mut rows = vec![];
        step(
            &mut rows,
            &CombatTraceStep::Check {
                check: tor_protocol::DiagnosticCheck {
                    skill: tor_protocol::Skill::HeavyWeaponry,
                    binding: tor_protocol::ManaBinding::Intellect,
                    attribute: tor_protocol::InspectionAttribute::Strength,
                    attribute_value: 5,
                    rank: 2,
                    modifier: -1,
                    threshold: 15,
                    input_edge: 2,
                    edge: 1,
                    unused_edge: 1,
                    rng_before: 123,
                    rng_after: 456,
                    first: 4,
                    second: Some(17),
                    kept: 17,
                    total: 23,
                    success: true,
                },
            },
        );
        let text = rows.join("\n");
        for expected in [
            "attribute 5 + rank 2 + modifier -1 vs 15",
            "d20 4, second 17, kept 17",
            "total 23, success true",
            "Edge input 2, allocated 1, unused 1",
            "RNG 123 -> 456",
        ] {
            assert!(text.contains(expected), "{text}");
        }
    }
}
