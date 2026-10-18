//! Pure effect resolution, with no mutable Game access.
use super::{
    AbilityPlan, AttemptedParticipation, CheckPhase, CheckSide, CheckThreshold, FrozenRider,
    FrozenStrike, PlannedAmount, PlannedCheck, PlannedEffect, UnselectedReason,
};
use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
use crate::dice::{roll_check, Edge, CHECK_EDGE_CAPACITY};
use crate::public_plan::PublicPlanObservation;
use crate::resolution_diagnostics::{ResolutionObserver, ResolutionStep, TraceField, TraceValue};
use crate::resources::Resource;
pub use crate::rules::EffectRecipient;
use crate::rules::{CheckActor, Selector};
use crate::ActorId;
use serde::{Deserialize, Serialize};
use tor_world::Shared;

/// Bounded live facts captured once for a primary or rider payload. The selected
/// target remains the check/descriptor-protection subject; source is used only
/// when that payload applies its effects to the source actor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolutionContext {
    pub hold_damage: Option<HoldDamageExecution>,
    pub observation: Option<Shared<PublicPlanObservation>>,
    pub source: Option<ApplicationSnapshot>,
    pub spatial: Option<super::SpatialInput>,
    pub target_recovery_context: Option<crate::health_recovery::RecoveryContext>,
    pub source_recovery_context: Option<crate::health_recovery::RecoveryContext>,
}
/// The sole dynamic relationship operand. Every attack statistic stays frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoldDamageExecution {
    pub id: crate::linked_hold::HoldId,
    pub bonus: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationSnapshot {
    pub creature: crate::creatures::CreatureState,
    pub observed_plan: Option<Shared<PublicPlanObservation>>,
}
/// The same actor-local mutation kernel is used for pure preview and publication.
pub(crate) struct ActorEffectState<'a> {
    pub creature: &'a mut crate::creatures::CreatureState,
    pub observed_plan: Option<&'a mut Option<Shared<PublicPlanObservation>>>,
    pub placement: Option<&'a mut super::Placement>,
    pub recovery_context: Option<crate::health_recovery::RecoveryContext>,
}

/// Ordinary condition rejection is a soft payload result. Creature rebuild or
/// limit failures are hard errors and must reject the enclosing transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectApplicationError {
    Condition(crate::conditions::ConditionError),
    Creature(crate::creatures::CreatureStateError),
}
impl From<crate::conditions::ConditionError> for EffectApplicationError {
    fn from(error: crate::conditions::ConditionError) -> Self {
        Self::Condition(error)
    }
}
impl From<crate::creatures::CreatureStateError> for EffectApplicationError {
    fn from(error: crate::creatures::CreatureStateError) -> Self {
        Self::Creature(error)
    }
}
impl From<EffectApplicationError> for crate::GameError {
    fn from(error: EffectApplicationError) -> Self {
        match error {
            EffectApplicationError::Creature(error) => Self::Creature(error),
            EffectApplicationError::Condition(error) => Self::Rule(match error {
                crate::conditions::ConditionError::Capacity => super::RuleError::ConditionCapacity,
                _ => super::RuleError::InvalidDefinition,
            }),
        }
    }
}

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
    pub(crate) fn spatial_effect(self) -> Option<&'a PlannedEffect> {
        let input = self.input();
        input
            .on_success
            .iter()
            .chain(input.on_failure)
            .find(|effect| effect.spatial())
    }

    pub(crate) fn source_effects(self) -> bool {
        let input = self.input();
        input
            .on_success
            .iter()
            .chain(input.on_failure)
            .any(|effect| effect.recipient() == EffectRecipient::Source)
    }
    pub(crate) fn observes_public_plan(self) -> bool {
        let input = self.input();
        input
            .on_success
            .iter()
            .chain(input.on_failure)
            .any(|effect| matches!(effect, PlannedEffect::ObservePublicPlan))
    }
    pub(crate) fn contextual_conditions(
        self,
        role: EffectRecipient,
        rules: &crate::rules::Ruleset,
    ) -> bool {
        let input = self.input();
        input
            .on_success
            .iter()
            .chain(input.on_failure)
            .any(|effect| {
                if effect.recipient() != role {
                    return false;
                }
                let id = match effect {
                    PlannedEffect::ApplyCondition { id, .. }
                    | PlannedEffect::ApplyQualifiedConditionToSource { id, .. } => id,
                    _ => return false,
                };
                rules.condition(id).is_some_and(|condition| {
                    rules.modifiers_use_recovery_context(&condition.modifiers)
                })
            })
    }
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
    pub(crate) fn roll(&self, rng: &mut u64) -> u32 {
        match self {
            Self::Fixed { amount } => *amount,
            Self::Rolled { dice } => dice.roll(rng, Edge::default()).total,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub(super) creature: crate::creatures::CreatureState,
    context: ResolutionContext,
}
impl Snapshot {
    pub fn capture(creature: &crate::creatures::CreatureState) -> Self {
        Self {
            creature: creature.clone(),
            context: ResolutionContext::default(),
        }
    }
    pub fn with_context(mut self, context: ResolutionContext) -> Self {
        self.context = context;
        self
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Applied,
    Unaffected,
    Miss,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolvedEffect {
    Theft {
        quantity: u64,
        relocation: Option<super::Placement>,
    },
    ApplyLinkedHold {
        policy: crate::linked_hold::HoldPolicy,
    },
    Translate {
        mode: super::MovementMode,
        result: super::MovementResult,
    },
    Damage {
        amount: u32,
    },
    Heal {
        amount: u32,
    },
    Nourish {
        amount: u32,
    },
    LoseAttribute {
        attribute: crate::attributes::Attribute,
        amount: u32,
    },
    RestoreAttribute {
        attribute: crate::attributes::Attribute,
        amount: u32,
    },
    DrainLatestHitDie {
        amount: u32,
    },
    RestoreHitDie {
        amount: u32,
    },
    LoseMaximumHealth {
        amount: u32,
    },
    RestoreMaximumHealth {
        amount: u32,
    },
    ApplyCondition {
        id: ConditionId,
        duration: u64,
    },
    Restore {
        resource: Resource,
        amount: u32,
    },
    SearchAdjacentGuises,
    ObservePublicPlan {
        observation: Shared<PublicPlanObservation>,
    },
    ApplyQualifiedConditionToSource {
        id: ConditionId,
        duration: u64,
        qualification: crate::conditions::TokenQualification,
    },
}
impl ResolvedEffect {
    pub(crate) fn recipient(&self) -> EffectRecipient {
        match self {
            Self::SearchAdjacentGuises
            | Self::ObservePublicPlan { .. }
            | Self::ApplyQualifiedConditionToSource { .. } => EffectRecipient::Source,
            _ => EffectRecipient::Target,
        }
    }
    /// Whether a canonical applied effect changed its target. Condition effects
    /// reach this point only after the shared kernel observed a state change.
    pub(crate) fn changed(&self) -> bool {
        match self {
            Self::Theft { quantity, .. } => *quantity > 0,
            Self::Translate { result, .. } => result.changed(),
            Self::Damage { amount }
            | Self::Heal { amount }
            | Self::Nourish { amount }
            | Self::LoseAttribute { amount, .. }
            | Self::RestoreAttribute { amount, .. }
            | Self::DrainLatestHitDie { amount }
            | Self::RestoreHitDie { amount }
            | Self::LoseMaximumHealth { amount }
            | Self::RestoreMaximumHealth { amount }
            | Self::Restore { amount, .. } => *amount > 0,
            Self::SearchAdjacentGuises
            | Self::ApplyLinkedHold { .. }
            | Self::ApplyCondition { .. }
            | Self::ObservePublicPlan { .. }
            | Self::ApplyQualifiedConditionToSource { .. } => true,
        }
    }
    /// One actor-state effect kernel, shared by pure resolution and publication.
    /// The input contains resolved amounts; protection and RNG belong to resolve.
    pub(crate) fn apply_to(
        &self,
        state: &mut ActorEffectState<'_>,
        source: ActorId,
    ) -> Result<Option<Self>, EffectApplicationError> {
        let creature = &mut *state.creature;
        if creature.health().dead() {
            return Ok(None);
        }
        let applied = match self {
            Self::Theft { .. } => return Ok(None),
            Self::SearchAdjacentGuises | Self::ApplyLinkedHold { .. } => self.clone(),
            Self::Translate { mode, result } => {
                if let super::MovementResult::Moved { from, to } = result {
                    let Some(placement) = state.placement.as_deref_mut() else {
                        return Ok(None);
                    };
                    if placement != from {
                        return Ok(Some(Self::Translate {
                            mode: *mode,
                            result: super::MovementResult::Stale,
                        }));
                    }
                    *placement = *to;
                }
                self.clone()
            }
            Self::Damage { amount } => Self::Damage {
                amount: creature.damage_without_reset(*amount).applied,
            },
            Self::Heal { amount } => Self::Heal {
                amount: creature.heal(*amount),
            },
            Self::Nourish { amount } => Self::Nourish {
                amount: creature.nourish(*amount),
            },
            Self::DrainLatestHitDie { amount } | Self::RestoreHitDie { amount } => {
                if *amount > 1 {
                    return Err(crate::creatures::CreatureStateError::Inconsistent.into());
                }
                let restore = matches!(self, Self::RestoreHitDie { .. });
                let applied = if *amount == 0 {
                    0
                } else {
                    match creature.stage_hit_die_effect(restore) {
                        Ok(change) => change.applied,
                        // Admission rejects missing source/root before payment.
                        // Later ineligibility is an ordinary spent, unaffected
                        // payload; it cannot refund or replay a resolved prefix.
                        Err(crate::creatures::CreatureStateError::HitDieRestoration(_)) => 0,
                        Err(error) => return Err(error.into()),
                    }
                };
                if restore {
                    Self::RestoreHitDie { amount: applied }
                } else {
                    Self::DrainLatestHitDie { amount: applied }
                }
            }
            Self::LoseAttribute { .. }
            | Self::RestoreAttribute { .. }
            | Self::LoseMaximumHealth { .. }
            | Self::RestoreMaximumHealth { .. } => {
                let apply = |creature: &mut crate::creatures::CreatureState| match self {
                    Self::LoseAttribute { attribute, amount } => {
                        creature.lose_attribute(*attribute, *amount)
                    }
                    Self::RestoreAttribute { attribute, amount } => {
                        creature.restore_attribute(*attribute, *amount)
                    }
                    Self::LoseMaximumHealth { amount } => creature.lose_maximum_health(*amount),
                    Self::RestoreMaximumHealth { amount } => {
                        creature.restore_maximum_health(*amount)
                    }
                    _ => unreachable!("restorable loss effect"),
                };
                let change = if let Some(context) = state.recovery_context {
                    creature.with_recovery_context(context, apply)?
                } else {
                    apply(creature)?
                };
                match self {
                    Self::LoseAttribute { attribute, .. } => Self::LoseAttribute {
                        attribute: *attribute,
                        amount: change.applied,
                    },
                    Self::RestoreAttribute { attribute, .. } => Self::RestoreAttribute {
                        attribute: *attribute,
                        amount: change.applied,
                    },
                    Self::LoseMaximumHealth { .. } => Self::LoseMaximumHealth {
                        amount: change.applied,
                    },
                    Self::RestoreMaximumHealth { .. } => Self::RestoreMaximumHealth {
                        amount: change.applied,
                    },
                    _ => unreachable!("restorable loss effect"),
                }
            }
            Self::Restore { resource, amount } => Self::Restore {
                resource: *resource,
                amount: creature.restore_resource(*resource, *amount),
            },
            Self::ApplyCondition { id, duration } => {
                let before = creature.clone();
                if let Some(sensed) = state.recovery_context {
                    creature.with_recovery_context(sensed, |creature| {
                        creature.apply_condition(id, source, *duration)
                    })?;
                } else {
                    creature.apply_condition(id, source, *duration)?;
                }
                if *creature == before {
                    return Ok(None);
                }
                self.clone()
            }
            Self::ApplyQualifiedConditionToSource {
                id,
                duration,
                qualification,
            } => {
                let before = creature.clone();
                let apply = |creature: &mut crate::creatures::CreatureState| {
                    creature.apply_qualified_condition(
                        id,
                        qualification.opponent,
                        *duration,
                        Some(qualification.clone()),
                    )
                };
                if let Some(sensed) = state.recovery_context {
                    creature.with_recovery_context(sensed, apply)?;
                } else {
                    apply(creature)?;
                }
                if *creature == before {
                    return Ok(None);
                }
                self.clone()
            }
            Self::ObservePublicPlan { observation } => {
                let Some(record) = state.observed_plan.as_deref_mut() else {
                    return Ok(None);
                };
                if record.as_ref() == Some(observation) {
                    return Ok(None);
                }
                *record = Some(observation.clone());
                self.clone()
            }
        };
        Ok(Some(applied))
    }
}
/// Actual check evidence for trusted accounting; never ordinary wire disclosure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedCheck {
    pub actor: CheckActor,
    pub success: bool,
    pub opposed: bool,
}
impl<'de> Deserialize<'de> for ResolvedCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: CheckActor,
            success: bool,
            opposed: bool,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            actor: fields.actor,
            success: fields.success,
            opposed: fields.opposed,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Option<AbilityId>,
    pub intention: Option<crate::IntentionId>,
    pub result: OutcomeStatus,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub check: Option<ResolvedCheck>,
    pub participation: AttemptedParticipation,
    /// Committed target participation from this primary's executed riders only.
    /// Individual payload publication always starts with false.
    pub target_rider_participated: bool,
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
/// Resolve a generic opposed check through the same kernel as ordinary abilities.
pub(crate) fn resolve_escape_check(check: &PlannedCheck, rng: &mut u64) -> bool {
    resolve_planned_check(check, rng, None, &StrikeBudget::effects(), &mut ())
}
fn resolve_planned_check(
    check: &PlannedCheck,
    rng: &mut u64,
    shared_owner: Option<CheckSide>,
    budget: &StrikeBudget,
    observer: &mut dyn ResolutionObserver,
) -> bool {
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
}
pub(super) struct BatchResolution {
    pub deferred_theft: Option<crate::theft::PlannedTheft>,
    pub outcome: Outcome,
    /// Ordinary abilities may reject individual condition effects. Authoritative
    /// automatic effects must be able to reject their enclosing transaction.
    pub condition_error: Option<crate::conditions::ConditionError>,
    /// The primary roller's branch; whole-payload immunity selects neither.
    pub selected_branch: Option<crate::rules::RiderTrigger>,
}

/// Resolve one payload through the shared pure kernel, preserving the caller's
/// RNG on every hard failure. Ordinary condition rejection remains soft.
pub fn resolve<'a>(
    plan: impl Into<ResolutionPlan<'a>>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
) -> Result<Outcome, EffectApplicationError> {
    let mut next_rng = *rng;
    let resolved = resolve_payload(plan.into(), snapshot, &mut next_rng, observer)?;
    *rng = next_rng;
    Ok(resolved.outcome)
}

/// The common resolver with typed condition failure retained for an atomic
/// automatic transaction. Immunity and unchanged Keep applications are Ok(None)
/// in the shared actor kernel, and remain successful no-ops here.
pub(crate) fn resolve_checked<'a>(
    plan: ResolutionPlan<'a>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
) -> Result<Outcome, EffectApplicationError> {
    let mut next_rng = *rng;
    let resolved = resolve_payload(plan, snapshot, &mut next_rng, observer)?;
    if let Some(error) = resolved.condition_error {
        return Err(error.into());
    }
    *rng = next_rng;
    Ok(resolved.outcome)
}

fn resolve_payload<'a>(
    plan: ResolutionPlan<'a>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
) -> Result<BatchResolution, EffectApplicationError> {
    let mut budget = match plan {
        ResolutionPlan::Ability(ability) => StrikeBudget::new(&ability.strikes[0]),
        ResolutionPlan::Strike { strike, .. } => StrikeBudget::new(strike),
        ResolutionPlan::Rider { .. } | ResolutionPlan::Effects { .. } => StrikeBudget::effects(),
    };
    resolve_batch(plan, snapshot, rng, observer, &mut budget)
}

/// The only effect resolver. It reads no game state, writes no actor, and draws only from rng.
pub(super) fn resolve_batch<'a>(
    plan: ResolutionPlan<'a>,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
    budget: &mut StrikeBudget,
) -> Result<BatchResolution, EffectApplicationError> {
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
        participation: AttemptedParticipation::empty(),
        target_rider_participated: false,
        effects: vec![],
    };
    let dead = snapshot.creature.health().dead();
    if dead
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
        outcome.participation = if dead {
            AttemptedParticipation::blocked(None, UnselectedReason::Dead)
        } else if check.is_some() {
            AttemptedParticipation::blocked(check, UnselectedReason::Immune)
        } else {
            // With no check, the success payload is already selected. Checked
            // immunity above cannot claim either unresolved branch was attempted.
            AttemptedParticipation::selected(None, on_success)
        };
        observer.record(ResolutionStep {
            id: "effect.immune",
            fields: &[],
        });
        return Ok(BatchResolution {
            deferred_theft: None,
            outcome,
            condition_error: None,
            selected_branch: None,
        });
    }

    let success = if let Some(check) = check {
        resolve_planned_check(check, rng, shared_owner, budget, observer)
    } else {
        true
    };
    outcome.check = check.map(|check| ResolvedCheck {
        actor: check.actor,
        success,
        opposed: matches!(check.threshold, CheckThreshold::Opposed { .. }),
    });
    let effects = if success { on_success } else { on_failure };
    outcome.participation = AttemptedParticipation::selected(check, effects);
    if effects.is_empty() && !success && check.is_some_and(|check| check.actor == CheckActor::User)
    {
        outcome.result = OutcomeStatus::Miss;
    }
    let recipient = effects
        .first()
        .map_or(EffectRecipient::Target, PlannedEffect::recipient);
    if effects.iter().any(|effect| effect.recipient() != recipient)
        || (effects
            .iter()
            .any(|effect| matches!(effect, PlannedEffect::ObservePublicPlan))
            && snapshot.context.observation.is_none())
    {
        return Ok(BatchResolution {
            deferred_theft: None,
            outcome,
            condition_error: None,
            selected_branch: None,
        });
    }
    let mut application = match recipient {
        EffectRecipient::Target => ApplicationSnapshot {
            creature: snapshot.creature.clone(),
            observed_plan: None,
        },
        EffectRecipient::Source => {
            let Some(source) = &snapshot.context.source else {
                return Ok(BatchResolution {
                    deferred_theft: None,
                    outcome,
                    condition_error: None,
                    selected_branch: None,
                });
            };
            source.clone()
        }
    };
    let mut placement = snapshot.context.spatial.map(|input| input.placement);
    let mut condition_error = None;
    let mut deferred_theft = None;
    for effect in effects {
        // Once damage kills the target, later effects cannot revive or mutate it.
        if application.creature.health().dead() {
            break;
        }
        let effect = match effect {
            PlannedEffect::Theft { theft } => {
                deferred_theft = Some(theft.clone());
                continue;
            }
            PlannedEffect::StepSelfOneCell | PlannedEffect::PushTargetOneCell { .. } => {
                let mode = if matches!(effect, PlannedEffect::StepSelfOneCell) {
                    super::MovementMode::Voluntary
                } else {
                    super::MovementMode::Forced
                };
                let result = snapshot
                    .context
                    .spatial
                    .filter(|input| input.mode == mode)
                    .map_or(super::MovementResult::Blocked, |input| input.result);
                ResolvedEffect::Translate { mode, result }
            }
            PlannedEffect::ApplyLinkedHold { policy } => ResolvedEffect::ApplyLinkedHold {
                policy: policy.clone(),
            },
            PlannedEffect::HoldDamage {
                category,
                descriptor,
            } => {
                let Some(execution) = snapshot.context.hold_damage else {
                    continue;
                };
                let damage = crate::damage::DamageSpec::new(
                    vec![crate::damage::DamageComponent::fixed(
                        *category,
                        descriptor.clone(),
                        execution.bonus,
                    )],
                    None,
                )
                .map_err(|_| crate::creatures::CreatureStateError::Inconsistent)?;
                let resolved = damage.resolve(
                    rng,
                    budget.remaining,
                    &application.creature.derived().protection,
                    observer,
                );
                budget.remaining = resolved.unused_edge;
                ResolvedEffect::Damage {
                    amount: resolved.total,
                }
            }
            PlannedEffect::Damage { damage } => {
                let resolved = damage.resolve(
                    rng,
                    budget.remaining,
                    &application.creature.derived().protection,
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
            PlannedEffect::Nourish { amount } => ResolvedEffect::Nourish {
                amount: amount.roll(rng),
            },
            PlannedEffect::LoseAttribute { attribute, amount } => ResolvedEffect::LoseAttribute {
                attribute: *attribute,
                amount: amount.roll(rng),
            },
            PlannedEffect::RestoreAttribute { attribute, amount } => {
                ResolvedEffect::RestoreAttribute {
                    attribute: *attribute,
                    amount: amount.roll(rng),
                }
            }
            PlannedEffect::DrainLatestHitDie => ResolvedEffect::DrainLatestHitDie { amount: 1 },
            PlannedEffect::RestoreHitDie => ResolvedEffect::RestoreHitDie { amount: 1 },
            PlannedEffect::LoseMaximumHealth { amount } => ResolvedEffect::LoseMaximumHealth {
                amount: amount.roll(rng),
            },
            PlannedEffect::RestoreMaximumHealth { amount } => {
                ResolvedEffect::RestoreMaximumHealth {
                    amount: amount.roll(rng),
                }
            }
            PlannedEffect::ApplyCondition { id, duration } => ResolvedEffect::ApplyCondition {
                id: id.clone(),
                duration: *duration,
            },
            PlannedEffect::Restore { resource, amount } => ResolvedEffect::Restore {
                resource: *resource,
                amount: amount.roll(rng),
            },
            PlannedEffect::SearchAdjacentGuises => ResolvedEffect::SearchAdjacentGuises,
            PlannedEffect::ObservePublicPlan => ResolvedEffect::ObservePublicPlan {
                observation: snapshot
                    .context
                    .observation
                    .clone()
                    .expect("authorized observation payload"),
            },
            PlannedEffect::ApplyQualifiedConditionToSource {
                id,
                duration,
                ability_tag,
            } => ResolvedEffect::ApplyQualifiedConditionToSource {
                id: id.clone(),
                duration: *duration,
                qualification: crate::conditions::TokenQualification {
                    opponent: target,
                    ability_tag: ability_tag.clone(),
                },
            },
        };
        let nourishment_offered = match effect {
            ResolvedEffect::Nourish { amount } => Some(amount),
            _ => None,
        };
        let effect = match effect.apply_to(
            &mut ActorEffectState {
                creature: &mut application.creature,
                observed_plan: Some(&mut application.observed_plan),
                placement: placement.as_mut(),
                recovery_context: match recipient {
                    EffectRecipient::Target => snapshot.context.target_recovery_context,
                    EffectRecipient::Source => snapshot.context.source_recovery_context,
                },
            },
            actor,
        ) {
            Ok(Some(effect)) => effect,
            Ok(None) => continue,
            Err(EffectApplicationError::Condition(error)) => {
                condition_error.get_or_insert(error);
                if let ResolvedEffect::ApplyCondition { id, .. } = &effect {
                    observer.record(ResolutionStep {
                        id: "condition.rejected",
                        fields: &[text("condition", id.as_str())],
                    });
                }
                continue;
            }
            Err(error @ EffectApplicationError::Creature(_)) => return Err(error),
        };
        if let (Some(offered), ResolvedEffect::Nourish { amount }) = (nourishment_offered, &effect)
        {
            observer.nourishment(offered, *amount);
        }
        if effect.changed() {
            outcome.result = OutcomeStatus::Applied;
        }
        outcome.effects.push(effect);
    }
    Ok(BatchResolution {
        deferred_theft,
        outcome,
        condition_error,
        selected_branch: Some(if success {
            crate::rules::RiderTrigger::Success
        } else {
            crate::rules::RiderTrigger::Failure
        }),
    })
}
impl<'de> Deserialize<'de> for ResolvedEffect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Theft {
                quantity: u64,
                #[serde(deserialize_with = "crate::bounded::required_option")]
                relocation: Option<super::Placement>,
            },
            ApplyLinkedHold {
                policy: crate::linked_hold::HoldPolicy,
            },
            Translate {
                mode: super::MovementMode,
                result: super::MovementResult,
            },
            Damage {
                amount: u32,
            },
            Heal {
                amount: u32,
            },
            Nourish {
                amount: u32,
            },
            LoseAttribute {
                attribute: crate::attributes::Attribute,
                amount: u32,
            },
            RestoreAttribute {
                attribute: crate::attributes::Attribute,
                amount: u32,
            },
            DrainLatestHitDie {
                amount: u32,
            },
            RestoreHitDie {
                amount: u32,
            },
            LoseMaximumHealth {
                amount: u32,
            },
            RestoreMaximumHealth {
                amount: u32,
            },
            ApplyCondition {
                id: ConditionId,
                duration: u64,
            },
            Restore {
                resource: Resource,
                amount: u32,
            },
            SearchAdjacentGuises {},
            ObservePublicPlan {
                observation: Shared<PublicPlanObservation>,
            },
            ApplyQualifiedConditionToSource {
                id: ConditionId,
                duration: u64,
                qualification: crate::conditions::TokenQualification,
            },
        }
        let effect = match crate::bounded::named_map(deserializer)? {
            Fields::Theft {
                quantity,
                relocation,
            } => Self::Theft {
                quantity,
                relocation,
            },
            Fields::ApplyLinkedHold { policy } => Self::ApplyLinkedHold { policy },
            Fields::Translate { mode, result } => Self::Translate { mode, result },
            Fields::Damage { amount } => Self::Damage { amount },
            Fields::Heal { amount } => Self::Heal { amount },
            Fields::Nourish { amount } => Self::Nourish { amount },
            Fields::LoseAttribute { attribute, amount } => {
                Self::LoseAttribute { attribute, amount }
            }
            Fields::RestoreAttribute { attribute, amount } => {
                Self::RestoreAttribute { attribute, amount }
            }
            Fields::DrainLatestHitDie { amount } => Self::DrainLatestHitDie { amount },
            Fields::RestoreHitDie { amount } => Self::RestoreHitDie { amount },
            Fields::LoseMaximumHealth { amount } => Self::LoseMaximumHealth { amount },
            Fields::RestoreMaximumHealth { amount } => Self::RestoreMaximumHealth { amount },
            Fields::ApplyCondition { id, duration } => Self::ApplyCondition { id, duration },
            Fields::Restore { resource, amount } => Self::Restore { resource, amount },
            Fields::SearchAdjacentGuises {} => Self::SearchAdjacentGuises,
            Fields::ObservePublicPlan { observation } => Self::ObservePublicPlan { observation },
            Fields::ApplyQualifiedConditionToSource {
                id,
                duration,
                qualification,
            } => Self::ApplyQualifiedConditionToSource {
                id,
                duration,
                qualification,
            },
        };
        let valid_amount = match effect {
            Self::Theft { quantity, .. } => quantity > 0,
            Self::DrainLatestHitDie { amount } | Self::RestoreHitDie { amount } => amount <= 1,
            Self::LoseAttribute { amount, .. } | Self::RestoreAttribute { amount, .. } => {
                amount <= u32::from(crate::limits::DERIVED_ATTRIBUTE)
            }
            Self::LoseMaximumHealth { amount } | Self::RestoreMaximumHealth { amount } => {
                amount <= crate::limits::NUMERIC as u32
            }
            _ => true,
        };
        if !valid_amount {
            return Err(serde::de::Error::custom(
                "out-of-bounds restorable loss amount",
            ));
        }
        Ok(effect)
    }
}
impl<'de> Deserialize<'de> for Outcome {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            target: ActorId,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            ability: Option<AbilityId>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            intention: Option<crate::IntentionId>,
            result: OutcomeStatus,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            check: Option<ResolvedCheck>,
            participation: AttemptedParticipation,
            target_rider_participated: bool,
            effects: crate::bounded::Bounded<ResolvedEffect, { crate::limits::EFFECTS_PER_BRANCH }>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let outcome = Self {
            actor: fields.actor,
            target: fields.target,
            ability: fields.ability,
            intention: fields.intention,
            result: fields.result,
            check: fields.check,
            participation: fields.participation,
            target_rider_participated: fields.target_rider_participated,
            effects: fields.effects.0,
        };
        if !outcome.valid() {
            return Err(serde::de::Error::custom(
                "inconsistent outcome evidence or result",
            ));
        }
        Ok(outcome)
    }
}
