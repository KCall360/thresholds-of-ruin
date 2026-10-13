//! Pure effect resolution, with no mutable Game access.
use super::{AbilityPlan, CheckThreshold, PlannedAmount, PlannedCheck, PlannedEffect};
use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
use crate::dice::{roll_check, Edge, CHECK_EDGE_CAPACITY};
use crate::resolution_diagnostics::{ResolutionObserver, ResolutionStep, TraceField, TraceValue};
use crate::resources::Resource;
use crate::rules::{CheckActor, Selector};
use crate::ActorId;
use serde::{Deserialize, Serialize};

/// Borrowed resolver input. Item and environmental effects cannot carry ability
/// checks, costs, timing, or a fabricated ability identity.
#[derive(Clone, Copy, Debug)]
pub enum ResolutionPlan<'a> {
    Ability(&'a AbilityPlan),
    Effects {
        actor: ActorId,
        target: ActorId,
        effects: &'a [PlannedEffect],
    },
}
impl<'a> From<&'a AbilityPlan> for ResolutionPlan<'a> {
    fn from(plan: &'a AbilityPlan) -> Self {
        Self::Ability(plan)
    }
}
/// Common borrowed operands of the two resolver inputs; never persisted.
struct ResolutionInput<'a> {
    actor: ActorId,
    target: ActorId,
    ability: Option<&'a AbilityId>,
    check: Option<&'a PlannedCheck>,
    on_success: &'a [PlannedEffect],
    on_failure: &'a [PlannedEffect],
    descriptors: &'a [DescriptorId],
    edge: i64,
}
impl<'a> ResolutionPlan<'a> {
    fn input(self) -> ResolutionInput<'a> {
        match self {
            Self::Ability(plan) => ResolutionInput {
                actor: plan.actor,
                target: plan.target,
                ability: Some(&plan.ability),
                check: plan.check.as_ref(),
                on_success: &plan.on_success,
                on_failure: &plan.on_failure,
                descriptors: &plan.descriptors,
                edge: plan.edge,
            },
            Self::Effects {
                actor,
                target,
                effects,
            } => ResolutionInput {
                actor,
                target,
                ability: None,
                check: None,
                on_success: effects,
                on_failure: &[],
                descriptors: &[],
                edge: 0,
            },
        }
    }
}
impl PlannedAmount {
    fn roll(&self, rng: &mut u64) -> u32 {
        match self {
            Self::Fixed { amount } => *amount,
            Self::Rolled { dice } => dice.roll(rng, Edge::default()).total,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub(super) creature: crate::creatures::CreatureState,
}
impl Snapshot {
    pub fn capture(creature: &crate::creatures::CreatureState) -> Self {
        Self {
            creature: creature.clone(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Applied,
    Unaffected,
    Miss,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolvedEffect {
    Damage { amount: u32 },
    Heal { amount: u32 },
    ApplyCondition { id: ConditionId, duration: u64 },
    Restore { resource: Resource, amount: u32 },
}
impl ResolvedEffect {
    /// Whether a canonical applied effect changed its target. Condition effects
    /// reach this point only after the shared kernel observed a state change.
    pub(crate) fn changed(&self) -> bool {
        match self {
            Self::Damage { amount } | Self::Heal { amount } | Self::Restore { amount, .. } => {
                *amount > 0
            }
            Self::ApplyCondition { .. } => true,
        }
    }
    /// One actor-state effect kernel, shared by pure resolution and publication.
    /// The input contains resolved amounts; protection and RNG belong to resolve.
    pub(crate) fn apply_to(
        &self,
        creature: &mut crate::creatures::CreatureState,
        source: ActorId,
    ) -> Result<Option<Self>, crate::conditions::ConditionError> {
        if creature.health().dead() {
            return Ok(None);
        }
        let applied = match self {
            Self::Damage { amount } => Self::Damage {
                amount: creature.damage_without_reset(*amount).applied,
            },
            Self::Heal { amount } => Self::Heal {
                amount: creature.heal(*amount),
            },
            Self::Restore { resource, amount } => Self::Restore {
                resource: *resource,
                amount: creature.restore_resource(*resource, *amount),
            },
            Self::ApplyCondition { id, duration } => {
                let before = creature.clone();
                creature.apply_condition(id, source, *duration)?;
                if *creature == before {
                    return Ok(None);
                }
                self.clone()
            }
        };
        Ok(Some(applied))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Option<AbilityId>,
    pub intention: Option<crate::IntentionId>,
    pub result: OutcomeStatus,
    pub effects: Vec<ResolvedEffect>,
}
// Keep trace keys and value types visible without repeating field construction.
fn signed(id: &str, value: i64) -> TraceField<'_> {
    TraceField {
        id,
        value: TraceValue::Signed(value),
    }
}
fn unsigned(id: &str, value: u64) -> TraceField<'_> {
    TraceField {
        id,
        value: TraceValue::Unsigned(value),
    }
}
fn boolean(id: &str, value: bool) -> TraceField<'_> {
    TraceField {
        id,
        value: TraceValue::Bool(value),
    }
}
fn text<'a>(id: &'a str, value: &'a str) -> TraceField<'a> {
    TraceField {
        id,
        value: TraceValue::Text(value),
    }
}
fn dice<'a>(id: &'a str, value: &'a [u16]) -> TraceField<'a> {
    TraceField {
        id,
        value: TraceValue::Dice(value),
    }
}
fn edge(balance: i64) -> Edge {
    Edge::from_counts(
        balance.max(0).min(i64::from(u32::MAX)) as u32,
        balance.saturating_neg().max(0).min(i64::from(u32::MAX)) as u32,
    )
}
/// The only effect resolver. It reads no game state, writes no actor, and draws only from rng.
pub fn resolve<'a>(
    plan: impl Into<ResolutionPlan<'a>>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
) -> Outcome {
    let ResolutionInput {
        actor,
        target,
        ability,
        check,
        on_success,
        on_failure,
        descriptors,
        edge: input_edge,
    } = plan.into().input();
    let mut outcome = Outcome {
        actor,
        target,
        ability: ability.cloned(),
        intention: None,
        result: OutcomeStatus::Unaffected,
        effects: vec![],
    };
    if snapshot.creature.health().dead()
        || descriptors.iter().any(|descriptor| {
            snapshot
                .creature
                .derived()
                .protection
                .has_immunity(&Selector::Descriptor {
                    descriptor: descriptor.clone(),
                })
        })
    {
        observer.record(ResolutionStep {
            id: "effect.immune",
            fields: &[],
        });
        return outcome;
    }
    let mut remaining_edge = edge(input_edge);
    let success = if let Some(check) = check {
        let mut check_edge = edge(check.edge);
        let allocated = check_edge.take(CHECK_EDGE_CAPACITY);
        // Combined check-only sources determine this roll. Shared edge pays
        // its own first three units independently, so a next-check token never
        // creates, removes or frees a later damage die.
        if check.actor == CheckActor::User {
            remaining_edge.take(CHECK_EDGE_CAPACITY);
        }
        let rng_before = *rng;
        let rolled = roll_check(rng, allocated);
        let total = i64::from(rolled.kept) + check.bonus;
        let threshold = match check.threshold {
            CheckThreshold::Fixed { value } => value,
            CheckThreshold::Opposed {
                bonus,
                edge: opposed_edge,
            } => {
                let rng_before = *rng;
                let mut opposed_remaining = edge(opposed_edge);
                let opposed_allocated = opposed_remaining.take(CHECK_EDGE_CAPACITY);
                if check.actor == CheckActor::Target {
                    // The opposing roller is the caster, whose remaining budget
                    // also owns later damage effects.
                    remaining_edge.take(CHECK_EDGE_CAPACITY);
                }
                let opposed = roll_check(rng, opposed_allocated);
                let total = i64::from(opposed.kept) + bonus;
                observer.record(ResolutionStep {
                    id: "opposed_check",
                    fields: &[
                        dice("rolled", opposed.rolled()),
                        dice("retained", opposed.retained()),
                        unsigned("kept", u64::from(opposed.kept)),
                        signed("input_edge", opposed_edge),
                        signed("edge", opposed_allocated.balance()),
                        signed("unused_edge", opposed_remaining.balance()),
                        signed("bonus", bonus),
                        signed("total", total),
                        unsigned("rng_before", rng_before),
                        unsigned("rng_after", *rng),
                    ],
                });
                total
            }
        };
        observer.record(ResolutionStep {
            id: "check",
            fields: &[
                dice("rolled", rolled.rolled()),
                dice("retained", rolled.retained()),
                unsigned("kept", u64::from(rolled.kept)),
                signed("input_edge", check.edge),
                signed("edge", allocated.balance()),
                signed("unused_edge", check_edge.balance()),
                signed("shared_edge_spent", input_edge - remaining_edge.balance()),
                signed("shared_damage_edge", remaining_edge.balance()),
                unsigned("rng_before", rng_before),
                unsigned("rng_after", *rng),
                signed("total", total),
                unsigned("roll", u64::from(rolled.kept)),
                signed("bonus", check.bonus),
                signed("threshold", threshold),
                boolean("success", total >= threshold),
            ],
        });
        total >= threshold
    } else {
        true
    };
    let effects = if success { on_success } else { on_failure };
    if effects.is_empty() && !success && check.is_some_and(|check| check.actor == CheckActor::User)
    {
        outcome.result = OutcomeStatus::Miss;
    }
    let mut creature = snapshot.creature.clone();
    for effect in effects {
        // Once damage kills the target, later effects cannot revive or mutate it.
        if creature.health().dead() {
            break;
        }
        let effect = match effect {
            PlannedEffect::Damage { damage } => {
                let resolved = damage.resolve(
                    rng,
                    remaining_edge,
                    &creature.derived().protection,
                    observer,
                );
                remaining_edge = resolved.unused_edge;
                ResolvedEffect::Damage {
                    amount: resolved.total,
                }
            }
            PlannedEffect::Heal { amount } => ResolvedEffect::Heal {
                amount: amount.roll(rng),
            },
            PlannedEffect::ApplyCondition { id, duration } => ResolvedEffect::ApplyCondition {
                id: id.clone(),
                duration: *duration,
            },
            PlannedEffect::Restore { resource, amount } => ResolvedEffect::Restore {
                resource: *resource,
                amount: amount.roll(rng),
            },
        };
        let effect = match effect.apply_to(&mut creature, actor) {
            Ok(Some(effect)) => effect,
            Ok(None) => continue,
            Err(_) => {
                if let ResolvedEffect::ApplyCondition { id, .. } = &effect {
                    observer.record(ResolutionStep {
                        id: "condition.rejected",
                        fields: &[text("condition", id.as_str())],
                    });
                }
                continue;
            }
        };
        if effect.changed() {
            outcome.result = OutcomeStatus::Applied;
        }
        outcome.effects.push(effect);
    }
    outcome
}
