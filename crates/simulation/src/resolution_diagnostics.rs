//! Transient observations of actual resolution work. Observers never receive
//! mutable random state; ordinary resolution uses the allocation-free unit observer.
//! Storage, actor identity, authorization and wire projection belong to callers.
use crate::attributes::{Attribute, CheckOutcome, SkillCheck};
use crate::combat::DamageType;
use crate::damage::ComponentRoll;
use crate::dice::DicePool;
use crate::grants::Selector;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckDiagnostic {
    pub check: SkillCheck,
    pub attribute: Attribute,
    pub attribute_value: u16,
    pub rank: u8,
    pub input_edge: i64,
    pub edge: i64,
    pub unused_edge: i64,
    pub rng_before: u64,
    pub rng_after: u64,
    pub outcome: CheckOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentRecord<C> {
    pub component: C,
    pub expression: Option<DicePool>,
    pub edge: i64,
    pub rng_before: u64,
    pub rng_after: u64,
    pub category_immune: bool,
    pub descriptor_immune: bool,
    pub after_immunity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionEvent<C> {
    FearStarted {
        difficulty: i32,
        duration: u64,
        net_edge: i64,
        difficulty_attribute: u16,
        difficulty_rank: u8,
        difficulty_bonus: i32,
        duration_bonus: u64,
    },
    FearImmunity {
        fear: bool,
        mind_affecting: bool,
    },
    FearFinished {
        applied: bool,
        duration: u64,
    },
    AttackStarted {
        net_edge: i64,
    },
    Check(CheckDiagnostic),
    AttackMissed {
        unused_edge: i64,
    },
    DamageStarted {
        net_edge: i64,
    },
    Component(C),
    Reduction {
        selector: Selector,
        category: DamageType,
        before: u32,
        capacity: u32,
        after: u32,
    },
    DamageFinished {
        raw: u32,
        after_immunity: u32,
        after_descriptors: u32,
        total: u32,
        unused_edge: i64,
    },
}

pub trait ResolutionObserver {
    /// Borrowed dice remain owned by the outcome. A retaining observer must copy
    /// explicitly; no observer allocation is required by the resolver itself.
    fn record(&mut self, step: ResolutionStep<'_>);
}

impl ResolutionObserver for () {
    #[inline]
    fn record(&mut self, _: ResolutionStep<'_>) {}
}

/// Borrowed callbacks keep dice owned by the resolver; retained records own copies.
pub type ComponentDiagnostic<'a> = ComponentRecord<&'a ComponentRoll>;
pub type ResolutionStep<'a> = ResolutionEvent<ComponentDiagnostic<'a>>;
pub type ResolutionRecord = ResolutionEvent<ComponentRecord<ComponentRoll>>;

impl<C> ResolutionEvent<C> {
    fn map_component<T>(self, convert: impl FnOnce(C) -> T) -> ResolutionEvent<T> {
        match self {
            Self::FearStarted {
                difficulty,
                duration,
                net_edge,
                difficulty_attribute,
                difficulty_rank,
                difficulty_bonus,
                duration_bonus,
            } => ResolutionEvent::FearStarted {
                difficulty,
                duration,
                net_edge,
                difficulty_attribute,
                difficulty_rank,
                difficulty_bonus,
                duration_bonus,
            },
            Self::FearImmunity {
                fear,
                mind_affecting,
            } => ResolutionEvent::FearImmunity {
                fear,
                mind_affecting,
            },
            Self::FearFinished { applied, duration } => {
                ResolutionEvent::FearFinished { applied, duration }
            }
            Self::AttackStarted { net_edge } => ResolutionEvent::AttackStarted { net_edge },
            Self::Check(check) => ResolutionEvent::Check(check),
            Self::AttackMissed { unused_edge } => ResolutionEvent::AttackMissed { unused_edge },
            Self::DamageStarted { net_edge } => ResolutionEvent::DamageStarted { net_edge },
            Self::Component(component) => ResolutionEvent::Component(convert(component)),
            Self::Reduction {
                selector,
                category,
                before,
                capacity,
                after,
            } => ResolutionEvent::Reduction {
                selector,
                category,
                before,
                capacity,
                after,
            },
            Self::DamageFinished {
                raw,
                after_immunity,
                after_descriptors,
                total,
                unused_edge,
            } => ResolutionEvent::DamageFinished {
                raw,
                after_immunity,
                after_descriptors,
                total,
                unused_edge,
            },
        }
    }
}

/// Covers every stage of the current maximum 32-component damage bundle.
/// Future larger resolvers still stop retaining here and report truncation.
pub const MAX_RESOLUTION_STEPS: usize = 128;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolutionTrace {
    steps: Vec<ResolutionRecord>,
    truncated: bool,
}

impl ResolutionTrace {
    pub fn steps(&self) -> &[ResolutionRecord] {
        &self.steps
    }

    /// An incomplete trace must never be presented as a complete explanation.
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

impl ResolutionObserver for ResolutionTrace {
    fn record(&mut self, step: ResolutionStep<'_>) {
        if self.steps.len() == MAX_RESOLUTION_STEPS {
            self.truncated = true;
            return;
        }
        self.steps.push(step.map_component(|value| ComponentRecord {
            component: value.component.clone(),
            expression: value.expression,
            edge: value.edge,
            rng_before: value.rng_before,
            rng_after: value.rng_after,
            category_immune: value.category_immune,
            descriptor_immune: value.descriptor_immune,
            after_immunity: value.after_immunity,
        }));
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
    pub fear: std::collections::BTreeMap<crate::ActorId, u64>,
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
    pub applied: bool,
    pub damage: u32,
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

    fn combatant_diagnostic(&self, actor: crate::ActorId) -> CombatantDiagnostic {
        let (health, maximum_health) = self.health(actor).expect("resolved combat participant");
        let creature = self.creature(actor);
        CombatantDiagnostic {
            health,
            maximum_health,
            injury: creature.map(|creature| creature.health().injury()),
            dead: creature.map_or(health == 0, |creature| creature.health().dead()),
            resources: creature.map_or_else(Vec::new, |creature| {
                let costs = creature.costs();
                crate::resources::Resource::ALL
                    .into_iter()
                    .map(|resource| ResourceDiagnostic {
                        resource,
                        maximum: costs.resources().maximum(resource),
                        balance: costs.resources().balance(resource),
                        available: costs.available(resource),
                        recovery_elapsed: costs.resources().recovery_elapsed(resource),
                    })
                    .collect()
            }),
            fear: creature.map_or_else(Default::default, |creature| {
                creature.fear().sources().clone()
            }),
        }
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
            origin_intention: preparation.origin_intention,
            work: preparation.work,
            preparation: preparation.duration,
            started: preparation.started,
            remaining: preparation.remaining,
            recovery: preparation.recovery,
            charge: preparation.charge,
            actor_before: self.combatant_diagnostic(actor),
            target_before: self.combatant_diagnostic(target),
        };
        let diagnostics = self.combat_diagnostics.as_mut().unwrap();
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
        applied: bool,
        damage: u32,
    ) {
        let Some(pending) = pending else {
            return;
        };
        let actor_after = self.combatant_diagnostic(pending.actor);
        let target_after = self.combatant_diagnostic(pending.target);
        let diagnostics = self.combat_diagnostics.as_mut().expect("active capture");
        let trace = diagnostics.active.take().expect("started capture");
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
            applied,
            damage,
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
            fear: Default::default(),
        };
        CombatDiagnostic {
            tick,
            actor: crate::ActorId(1),
            target: crate::ActorId(2),
            intention: None,
            origin_intention: None,
            work: crate::Work::Attack {
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
            applied: false,
            damage: 0,
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
