//! Pure effect resolution, with no mutable Game access.
use super::{
    AbilityPlan, CheckPhase, CheckSide, CheckThreshold, FrozenRider, FrozenStrike, PlannedAmount,
    PlannedCheck, PlannedEffect,
};
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
    Strike {
        ability: &'a AbilityPlan,
        strike: &'a FrozenStrike,
    },
    Rider {
        ability: &'a AbilityPlan,
        rider: &'a FrozenRider,
        ordinal: u16,
    },
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
    shared_owner: Option<CheckSide>,
}
impl<'a> ResolutionPlan<'a> {
    fn input(self) -> ResolutionInput<'a> {
        match self {
            Self::Ability(plan) => Self::Strike {
                ability: plan,
                strike: &plan.strikes[0],
            }
            .input(),
            Self::Strike { ability, strike } => ResolutionInput {
                actor: ability.actor,
                target: ability.target,
                ability: Some(&ability.ability),
                check: strike.check.as_ref(),
                on_success: &strike.on_success,
                on_failure: &strike.on_failure,
                descriptors: &strike.descriptors,
                shared_owner: match strike.shared_check {
                    Some(CheckPhase::Primary { side }) => Some(side),
                    _ => None,
                },
            },
            Self::Rider { ability, rider, .. } => ResolutionInput {
                actor: ability.actor,
                target: ability.target,
                // A rider is part of its strike, not another public ability event.
                ability: None,
                check: rider.check.as_ref(),
                on_success: &rider.effects,
                on_failure: &[],
                descriptors: &rider.descriptors,
                shared_owner: None,
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
                shared_owner: None,
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
/// Actual check evidence for trusted accounting; never ordinary wire disclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedCheck {
    pub actor: CheckActor,
    pub success: bool,
    pub opposed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Option<AbilityId>,
    pub intention: Option<crate::IntentionId>,
    pub result: OutcomeStatus,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub check: Option<ResolvedCheck>,
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
/// One shared allocation per strike. Check-only sources never enter this value.
pub(super) struct StrikeBudget {
    check_debit: i64,
    owner: Option<CheckPhase>,
    remaining: Edge,
}
impl StrikeBudget {
    pub(super) fn new(strike: &FrozenStrike) -> Self {
        let check_debit = if strike.shared_check.is_some() {
            strike.edge.clamp(
                -i64::from(CHECK_EDGE_CAPACITY),
                i64::from(CHECK_EDGE_CAPACITY),
            )
        } else {
            0
        };
        Self {
            check_debit,
            owner: strike.shared_check,
            remaining: edge(strike.edge - check_debit),
        }
    }
    fn effects() -> Self {
        Self {
            check_debit: 0,
            owner: None,
            remaining: Edge::default(),
        }
    }
}
pub(super) struct BatchResolution {
    pub outcome: Outcome,
    /// The primary roller's branch; whole-payload immunity selects neither.
    pub selected_branch: Option<crate::rules::RiderTrigger>,
}

/// Resolve one payload through the shared pure kernel. Ability callers selecting
/// several strike boundaries own their cursor and reuse this exact batch path.
pub fn resolve<'a>(
    plan: impl Into<ResolutionPlan<'a>>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
) -> Outcome {
    let plan = plan.into();
    let mut budget = match plan {
        ResolutionPlan::Ability(ability) => StrikeBudget::new(&ability.strikes[0]),
        ResolutionPlan::Strike { strike, .. } => StrikeBudget::new(strike),
        ResolutionPlan::Rider { .. } | ResolutionPlan::Effects { .. } => StrikeBudget::effects(),
    };
    resolve_batch(plan, snapshot, rng, observer, &mut budget).outcome
}

/// The only effect resolver. It reads no game state, writes no actor, and draws only from rng.
pub(super) fn resolve_batch<'a>(
    plan: ResolutionPlan<'a>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
    budget: &mut StrikeBudget,
) -> BatchResolution {
    let ResolutionInput {
        actor,
        target,
        ability,
        check,
        on_success,
        on_failure,
        descriptors,
        mut shared_owner,
    } = plan.input();
    if let ResolutionPlan::Rider { ordinal, .. } = plan {
        shared_owner =
            (budget.owner == Some(CheckPhase::Rider { ordinal })).then_some(CheckSide::Initiating);
    }
    let mut outcome = Outcome {
        actor,
        target,
        ability: ability.cloned(),
        intention: None,
        result: OutcomeStatus::Unaffected,
        check: None,
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
        return BatchResolution {
            outcome,
            selected_branch: None,
        };
    }

    let success = if let Some(check) = check {
        let mut check_edge = edge(check.edge);
        let allocated = check_edge.take(CHECK_EDGE_CAPACITY);
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
                let opposed = roll_check(rng, opposed_allocated);
                let total = i64::from(opposed.kept) + bonus;
                observer.record(ResolutionStep {
                    id: "opposed_check",
                    fields: &[
                        dice("rolled", opposed.rolled()),
                        dice("retained", opposed.retained()),
                        unsigned("kept", u64::from(opposed.kept)),
                        signed("input_edge", opposed_edge),
                        signed(
                            "shared_edge_spent",
                            if shared_owner == Some(CheckSide::Opposing) {
                                budget.check_debit
                            } else {
                                0
                            },
                        ),
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
                signed(
                    "shared_edge_spent",
                    if shared_owner == Some(CheckSide::Initiating) {
                        budget.check_debit
                    } else {
                        0
                    },
                ),
                signed("shared_damage_edge", budget.remaining.balance()),
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
    outcome.check = check.map(|check| ResolvedCheck {
        actor: check.actor,
        success,
        opposed: matches!(check.threshold, CheckThreshold::Opposed { .. }),
    });
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
                    budget.remaining,
                    &creature.derived().protection,
                    observer,
                );
                budget.remaining = resolved.unused_edge;
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
    BatchResolution {
        outcome,
        selected_branch: Some(if success {
            crate::rules::RiderTrigger::Success
        } else {
            crate::rules::RiderTrigger::Failure
        }),
    }
}
