//! Exhaustive projections of captured facts. This never rolls dice, advances
//! time, loads actors or serializes simulation records into public messages.
use super::{attribute, binding, category, descriptor, resource, selector, skill, technique};
use s::resolution_diagnostics as d;
use tor_protocol as p;
use tor_simulation as s;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticProjectionError {
    InvalidPage,
    Inconsistent,
}

fn check(value: d::CheckDiagnostic) -> p::DiagnosticCheck {
    p::DiagnosticCheck {
        skill: skill(value.check.skill),
        binding: binding(value.check.binding),
        attribute: attribute(value.attribute),
        attribute_value: value.attribute_value,
        rank: value.rank,
        modifier: value.check.modifier,
        threshold: value.check.threshold,
        input_edge: value.input_edge,
        edge: value.edge,
        unused_edge: value.unused_edge,
        rng_before: value.rng_before,
        rng_after: value.rng_after,
        first: value.outcome.roll.first,
        second: value.outcome.roll.second,
        kept: value.outcome.roll.kept,
        total: value.outcome.total,
        success: value.outcome.success,
    }
}
fn component(value: &d::ComponentRecord<s::damage::ComponentRoll>) -> p::DiagnosticComponent {
    let (rolled, kept) = value.component.pool.as_ref().map_or_else(
        || (Vec::new(), Vec::new()),
        |pool| (pool.rolled.clone(), pool.kept.clone()),
    );
    p::DiagnosticComponent {
        category: category(value.component.key.category),
        descriptor: value.component.key.descriptor.map(descriptor),
        expression: value.expression.map(|pool| p::DiagnosticDiceExpression {
            count: pool.count(),
            sides: pool.sides(),
            bonus: pool.bonus(),
        }),
        edge: value.edge,
        rng_before: value.rng_before,
        rng_after: value.rng_after,
        rolled,
        kept,
        raw: value.component.raw,
        category_immune: value.category_immune,
        descriptor_immune: value.descriptor_immune,
        after_immunity: value.after_immunity,
    }
}
fn step(value: &d::ResolutionRecord) -> p::CombatTraceStep {
    use d::ResolutionRecord as S;
    use p::CombatTraceStep as P;
    match value {
        S::AttackStarted { net_edge } => P::AttackStarted {
            net_edge: *net_edge,
        },
        S::Check(value) => P::Check {
            check: check(*value),
        },
        S::AttackMissed { unused_edge } => P::AttackMissed {
            unused_edge: *unused_edge,
        },
        S::DamageStarted { net_edge } => P::DamageStarted {
            net_edge: *net_edge,
        },
        S::Component(value) => P::Component {
            component: component(value),
        },
        S::Reduction {
            selector: value,
            category: damage,
            before,
            capacity,
            after,
        } => P::Reduction {
            selector: selector(*value),
            category: category(*damage),
            before: *before,
            capacity: *capacity,
            after: *after,
        },
        S::DamageFinished {
            raw,
            after_immunity,
            after_descriptors,
            total,
            unused_edge,
        } => P::DamageFinished {
            raw: *raw,
            after_immunity: *after_immunity,
            after_descriptors: *after_descriptors,
            total: *total,
            unused_edge: *unused_edge,
        },
        S::FearStarted {
            difficulty,
            duration,
            net_edge,
            difficulty_attribute,
            difficulty_rank,
            difficulty_bonus,
            duration_bonus,
        } => P::FearStarted {
            difficulty: *difficulty,
            duration: *duration,
            net_edge: *net_edge,
            difficulty_attribute: *difficulty_attribute,
            difficulty_rank: *difficulty_rank,
            difficulty_bonus: *difficulty_bonus,
            duration_bonus: *duration_bonus,
        },
        S::FearImmunity {
            fear,
            mind_affecting,
        } => P::FearImmunity {
            fear: *fear,
            mind_affecting: *mind_affecting,
        },
        S::FearFinished { applied, duration } => P::FearFinished {
            applied: *applied,
            duration: *duration,
        },
    }
}
fn combatant(value: &d::CombatantDiagnostic) -> p::DiagnosticCombatant {
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
        fear: value
            .fear
            .iter()
            .map(|(causer, remaining)| p::InspectionFear {
                causer: p::ActorId(causer.0),
                remaining_ticks: *remaining,
            })
            .collect(),
    }
}
fn record(
    value: &d::CombatDiagnostic,
    sequence: u64,
) -> Result<p::CombatDiagnosticRecord, DiagnosticProjectionError> {
    let ability = match value.work {
        s::Work::Attack { .. } => p::Technique::BasicMelee,
        s::Work::UseAbility { ability, .. } => technique(ability),
        s::Work::Equip { .. } | s::Work::Unequip { .. } | s::Work::Drink { .. } => {
            return Err(DiagnosticProjectionError::Inconsistent)
        }
    };
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
        charge: value.charge.map(|cost| p::DiagnosticCost {
            resource: resource(cost.resource),
            start: cost.start,
            resolution: cost.resolution,
        }),
        actor_before: combatant(&value.actor_before),
        target_before: combatant(&value.target_before),
        actor_after: combatant(&value.actor_after),
        target_after: combatant(&value.target_after),
        applied: value.applied,
        damage: value.damage,
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
            .map(|(index, value)| record(value, dropped + start as u64 + index as u64 + 1))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    Ok(p::CombatDiagnosticsView {
        enabled: capture.is_some(),
        tick: game.tick(),
        captured,
        dropped,
        retained: retained_u8,
        through,
        records,
    })
}
