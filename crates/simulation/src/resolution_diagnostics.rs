//! Bounded generic rule observations. Borrowed callbacks allocate nothing when
//! ignored; only the retaining observer owns a copy of diagnostic data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceValue<'a> {
    Text(&'a str),
    Signed(i64),
    Unsigned(u64),
    Bool(bool),
    Dice(&'a [u16]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceField<'a> {
    pub id: &'a str,
    pub value: TraceValue<'a>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionStep<'a> {
    pub id: &'a str,
    pub fields: &'a [TraceField<'a>],
}
pub trait ResolutionObserver {
    fn record(&mut self, step: ResolutionStep<'_>);
}
impl ResolutionObserver for () {
    #[inline]
    fn record(&mut self, _: ResolutionStep<'_>) {}
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceValueRecord {
    Text(String),
    Signed(i64),
    Unsigned(u64),
    Bool(bool),
    Dice(Vec<u16>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceFieldRecord {
    pub id: String,
    pub value: TraceValueRecord,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionRecord {
    pub id: String,
    pub fields: Vec<TraceFieldRecord>,
}
pub const MAX_RESOLUTION_STEPS: usize = 128;
pub const MAX_TRACE_FIELDS: usize = 32;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolutionTrace {
    steps: Vec<ResolutionRecord>,
    truncated: bool,
}
impl ResolutionTrace {
    pub fn steps(&self) -> &[ResolutionRecord] {
        &self.steps
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}
impl ResolutionObserver for ResolutionTrace {
    fn record(&mut self, step: ResolutionStep<'_>) {
        if self.truncated
            || self.steps.len() >= MAX_RESOLUTION_STEPS
            || step.fields.len() > MAX_TRACE_FIELDS
        {
            self.truncated = true;
            return;
        }
        // All produced fields are bounded too; an accidental oversized debug
        // observation cannot turn optional capture into unbounded allocation.
        if !crate::content_ids::valid_id(step.id)
            || step.fields.iter().any(|field| {
                !crate::content_ids::valid_id(field.id)
                    || match field.value {
                        TraceValue::Text(text) => {
                            text.len() > 4096 || text.chars().any(char::is_control)
                        }
                        TraceValue::Dice(values) => values.len() > 128,
                        _ => false,
                    }
            })
        {
            self.truncated = true;
            return;
        }
        self.steps.push(ResolutionRecord {
            id: step.id.into(),
            fields: step
                .fields
                .iter()
                .map(|field| TraceFieldRecord {
                    id: field.id.into(),
                    value: match field.value {
                        TraceValue::Text(value) => TraceValueRecord::Text(value.into()),
                        TraceValue::Signed(value) => TraceValueRecord::Signed(value),
                        TraceValue::Unsigned(value) => TraceValueRecord::Unsigned(value),
                        TraceValue::Bool(value) => TraceValueRecord::Bool(value),
                        TraceValue::Dice(value) => TraceValueRecord::Dice(value.into()),
                    },
                })
                .collect(),
        });
    }
}

/// A bounded observation window, excluded from checkpoints and normal views.
pub const MAX_COMBAT_DIAGNOSTICS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceDiagnostic {
    pub resource: crate::resources::Resource,
    pub maximum: u32,
    pub balance: u32,
    pub available: u32,
    pub recovery_elapsed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatantDiagnostic {
    pub health: u32,
    pub maximum_health: u32,
    pub injury: Option<u32>,
    pub dead: bool,
    pub resources: Vec<ResourceDiagnostic>,
    pub conditions: Vec<crate::conditions::ConditionInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatDiagnostic {
    pub tick: u64,
    pub actor: crate::ActorId,
    pub target: crate::ActorId,
    pub intention: Option<crate::IntentionId>,
    pub origin_intention: Option<crate::IntentionId>,
    pub work: crate::Work,
    pub preparation: u64,
    pub started: u64,
    pub remaining: u64,
    pub recovery: u64,
    pub charge: Option<crate::costs::ResourceCost>,
    pub actor_before: CombatantDiagnostic,
    pub target_before: CombatantDiagnostic,
    pub actor_after: CombatantDiagnostic,
    pub target_after: CombatantDiagnostic,
    pub outcome: crate::abilities::Outcome,
    pub trace: ResolutionTrace,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CombatDiagnostics {
    records: std::collections::VecDeque<tor_world::Shared<CombatDiagnostic>>,
    dropped: u64,
    pub(crate) active: Option<ResolutionTrace>,
}

impl CombatDiagnostics {
    pub fn records(&self) -> &std::collections::VecDeque<tor_world::Shared<CombatDiagnostic>> {
        &self.records
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    fn retain(&mut self, record: CombatDiagnostic) {
        if self.records.len() == MAX_COMBAT_DIAGNOSTICS {
            self.records.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.records.push_back(tor_world::Shared::new(record));
    }
}

pub(crate) struct PendingCombatDiagnostic {
    tick: u64,
    actor: crate::ActorId,
    target: crate::ActorId,
    intention: Option<crate::IntentionId>,
    origin_intention: Option<crate::IntentionId>,
    work: crate::Work,
    preparation: u64,
    started: u64,
    remaining: u64,
    recovery: u64,
    charge: Option<crate::costs::ResourceCost>,
    actor_before: CombatantDiagnostic,
    target_before: CombatantDiagnostic,
}

impl crate::Game {
    /// Trusted backend tooling only; this grants no client disclosure authority.
    /// Repeated enabling retains the window; disabling drops all retained data.
    pub fn set_combat_diagnostics(&mut self, enabled: bool) {
        if enabled {
            self.combat_diagnostics
                .get_or_insert_with(|| tor_world::Shared::new(CombatDiagnostics::default()));
        } else {
            self.combat_diagnostics = None;
        }
    }

    pub fn combat_diagnostics(&self) -> Option<&CombatDiagnostics> {
        self.combat_diagnostics.as_deref()
    }

    fn combatant_diagnostic(&self, actor: crate::ActorId) -> Option<CombatantDiagnostic> {
        let creature = self.creature(actor)?;
        let health = creature.health();
        let costs = creature.costs();
        Some(CombatantDiagnostic {
            health: health.current(),
            maximum_health: health.maximum(),
            injury: Some(health.injury()),
            dead: health.dead(),
            resources: crate::resources::Resource::ALL
                .into_iter()
                .map(|resource| ResourceDiagnostic {
                    resource,
                    maximum: costs.resources().maximum(resource),
                    balance: costs.resources().balance(resource),
                    available: costs.available(resource),
                    recovery_elapsed: costs.resources().recovery_elapsed(resource),
                })
                .collect(),
            conditions: creature.conditions().instances().cloned().collect(),
        })
    }

    pub(crate) fn begin_combat_diagnostic(
        &mut self,
        actor: crate::ActorId,
        target: crate::ActorId,
        preparation: &crate::combat::Preparation,
    ) -> Option<PendingCombatDiagnostic> {
        if self.combat_diagnostics.is_none() || self.creature(actor).is_none() {
            return None;
        }
        let pending = PendingCombatDiagnostic {
            tick: self.tick,
            actor,
            target,
            intention: preparation.intention,
            origin_intention: preparation.origin,
            work: preparation.plan.work(),
            preparation: preparation.plan.preparation(),
            started: preparation.started,
            remaining: preparation.remaining,
            recovery: preparation.plan.recovery(),
            charge: preparation.plan.cost(),
            actor_before: self.combatant_diagnostic(actor)?,
            target_before: self.combatant_diagnostic(target)?,
        };
        let diagnostics = self.combat_diagnostics.as_mut()?;
        debug_assert!(
            diagnostics.active.is_none(),
            "combat resolutions are not nested"
        );
        diagnostics.active = Some(ResolutionTrace::default());
        Some(pending)
    }

    pub(crate) fn finish_combat_diagnostic(
        &mut self,
        pending: Option<PendingCombatDiagnostic>,
        outcome: crate::abilities::Outcome,
    ) {
        let Some(pending) = pending else {
            return;
        };
        let Some(actor_after) = self.combatant_diagnostic(pending.actor) else {
            return;
        };
        let Some(target_after) = self.combatant_diagnostic(pending.target) else {
            return;
        };
        let Some(diagnostics) = self.combat_diagnostics.as_mut() else {
            return;
        };
        let Some(trace) = diagnostics.active.take() else {
            return;
        };
        diagnostics.retain(CombatDiagnostic {
            tick: pending.tick,
            actor: pending.actor,
            target: pending.target,
            intention: pending.intention,
            origin_intention: pending.origin_intention,
            work: pending.work,
            preparation: pending.preparation,
            started: pending.started,
            remaining: pending.remaining,
            recovery: pending.recovery,
            charge: pending.charge,
            actor_before: pending.actor_before,
            target_before: pending.target_before,
            actor_after,
            target_after,
            outcome,
            trace,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(tick: u64) -> CombatDiagnostic {
        let state = CombatantDiagnostic {
            health: 1,
            maximum_health: 1,
            injury: None,
            dead: false,
            resources: Vec::new(),
            conditions: vec![],
        };
        CombatDiagnostic {
            tick,
            actor: crate::ActorId(1),
            target: crate::ActorId(2),
            intention: None,
            origin_intention: None,
            work: crate::Work::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                target: crate::ActorId(2),
            },
            preparation: 1,
            started: tick - 1,
            remaining: 1,
            recovery: 1,
            charge: None,
            actor_before: state.clone(),
            target_before: state.clone(),
            actor_after: state.clone(),
            target_after: state,
            outcome: crate::abilities::Outcome {
                actor: crate::ActorId(1),
                target: crate::ActorId(2),
                ability: Some(crate::content_ids::AbilityId::new("basic_melee").unwrap()),
                intention: None,
                result: crate::abilities::OutcomeStatus::Unaffected,
                effects: vec![],
            },
            trace: ResolutionTrace::default(),
        }
    }

    #[test]
    fn combat_window_evicts_oldest_and_reports_every_dropped_resolution() {
        let mut diagnostics = CombatDiagnostics::default();
        for tick in 1..=MAX_COMBAT_DIAGNOSTICS as u64 + 3 {
            diagnostics.retain(record(tick));
        }
        assert_eq!(diagnostics.records().len(), MAX_COMBAT_DIAGNOSTICS);
        assert_eq!(diagnostics.dropped(), 3);
        assert_eq!(diagnostics.records().front().unwrap().tick, 4);
        assert_eq!(
            diagnostics.records().back().unwrap().tick,
            MAX_COMBAT_DIAGNOSTICS as u64 + 3
        );
        assert!(diagnostics
            .records()
            .iter()
            .zip(4..)
            .all(|(record, tick)| record.tick == tick));
    }
}
