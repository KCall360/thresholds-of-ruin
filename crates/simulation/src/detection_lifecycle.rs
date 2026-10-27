//! Sole-payload detection lifetime and paid-completion semantics.
//! Charts/history/census are owned by detection; this module owns no saved clock.
use crate::abilities::{Outcome, OutcomeStatus, PlannedEffect, ResolvedEffect};
use crate::rules::{
    ConditionDef, ConditionLifecycle, ConditionPreparation, ConditionTiming, Effect, RulesetError,
    Stacking,
};
use crate::{ActorId, Game, GameError};
use std::collections::BTreeSet;
use tor_world::RegionId;

pub(crate) const MONSTER_TURNS: u64 = 20;
pub(crate) fn duration(turn_ticks: u64) -> Option<u64> {
    turn_ticks
        .checked_mul(MONSTER_TURNS)
        .filter(|ticks| (1..=crate::limits::NUMERIC as u64).contains(ticks))
}
pub(crate) fn valid_resolved_duration(ticks: u64) -> bool {
    (MONSTER_TURNS..=crate::limits::NUMERIC as u64).contains(&ticks)
        && ticks.is_multiple_of(MONSTER_TURNS)
}
pub(crate) fn validate_conditions(definitions: &[ConditionDef]) -> Result<(), RulesetError> {
    let mut marked = false;
    for definition in definitions
        .iter()
        .filter(|definition| definition.detecting_monsters)
    {
        if marked {
            return Err(RulesetError::Bound {
                field: "monster_detection_condition_unique".into(),
            });
        }
        marked = true;
        if definition.lifecycle != ConditionLifecycle::Timed
            || definition.timing != ConditionTiming::expiring()
            || definition.stacking != Stacking::Refresh
            || definition.source_scoped
            || definition.hallucinating
            || !definition.modifiers.is_empty()
            || !definition.restrictions.is_empty()
            || !definition.descriptors.is_empty()
            || definition.preparation != ConditionPreparation::Preserve
        {
            return Err(RulesetError::Bound {
                field: "monster_detection_condition_shape".into(),
            });
        }
    }
    Ok(())
}
pub(crate) fn is_effect(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::DetectMonsters { .. }
            | Effect::DetectFood
            | Effect::DetectMagic
            | Effect::MapTopology
    )
}
pub(crate) fn validate_sole_effects(effects: &[Effect]) -> Result<(), RulesetError> {
    if effects.iter().any(is_effect) && effects.len() != 1 {
        Err(RulesetError::Bound {
            field: "detection_sole_effect".into(),
        })
    } else {
        Ok(())
    }
}
pub(crate) fn planned_kind(effect: &PlannedEffect) -> Option<crate::chart::Kind> {
    match effect {
        PlannedEffect::DetectMonsters { .. } => Some(crate::chart::Kind::Monsters),
        PlannedEffect::DetectFood => Some(crate::chart::Kind::Food),
        PlannedEffect::MapTopology => Some(crate::chart::Kind::Mapping),
        PlannedEffect::DetectMagic => Some(crate::chart::Kind::Magic),
        _ => None,
    }
}
pub(crate) fn resolved_kind(effect: &ResolvedEffect) -> Option<crate::chart::Kind> {
    match effect {
        ResolvedEffect::DetectMonsters { .. } => Some(crate::chart::Kind::Monsters),
        ResolvedEffect::DetectFood { .. } => Some(crate::chart::Kind::Food),
        ResolvedEffect::MapTopology { .. } => Some(crate::chart::Kind::Mapping),
        ResolvedEffect::DetectMagic { .. } => Some(crate::chart::Kind::Magic),
        _ => None,
    }
}

/// Retained outcomes must keep the same sole, checkless self shape as plans.
/// Marker reference validation belongs to the admitted source/live applier.
pub(crate) fn resolved_shape_valid(outcome: &Outcome) -> bool {
    !outcome
        .effects
        .iter()
        .any(|effect| resolved_kind(effect).is_some())
        || (outcome.actor == outcome.target
            && outcome.effects.len() == 1
            && outcome.check.is_none()
            && outcome.resistance.is_none()
            && outcome.participation == crate::abilities::AttemptedParticipation::source()
            && !outcome.target_rider_participated
            && outcome.effects.iter().all(|effect| match effect {
                ResolvedEffect::DetectMonsters { duration, .. } => {
                    valid_resolved_duration(*duration)
                }
                _ => true,
            }))
}

/// Final-only transaction receipt, never a saved pending-learning instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompletionReceipt {
    kind: crate::chart::Kind,
    status: CompletionStatus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionStatus {
    Empty,
    Positive,
    Unavailable(crate::chart::Unavailable),
}
impl CompletionReceipt {
    /// Caller is the final producer admission boundary, not an early census.
    pub(crate) fn from_admitted(
        kind: crate::chart::Kind,
        candidate: &crate::chart::Candidate,
    ) -> Result<Self, GameError> {
        let status = if let Some(chart) = candidate.chart() {
            if chart.kind() != kind {
                return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
            }
            if chart.rows().is_empty() {
                CompletionStatus::Empty
            } else {
                CompletionStatus::Positive
            }
        } else {
            CompletionStatus::Unavailable(
                candidate
                    .unavailable_reason()
                    .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?,
            )
        };
        Ok(Self { kind, status })
    }
    pub(crate) fn identifies(self) -> bool {
        self.status == CompletionStatus::Positive
    }
    pub(crate) fn matches(self, outcome: &Outcome) -> bool {
        outcome.actor == outcome.target
            && outcome.effects.len() == 1
            && outcome.effects.first().and_then(resolved_kind) == Some(self.kind)
    }
}

pub(crate) struct ItemCompletion {
    pub(crate) outcome: Outcome,
    receipt: Option<CompletionReceipt>,
}
impl ItemCompletion {
    pub(crate) fn new(
        outcome: Outcome,
        receipt: Option<CompletionReceipt>,
    ) -> Result<Self, GameError> {
        let chart = outcome
            .effects
            .iter()
            .any(|effect| resolved_kind(effect).is_some());
        if receipt.is_some_and(|receipt| !receipt.matches(&outcome)) || chart != receipt.is_some() {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        Ok(Self { outcome, receipt })
    }
    pub(crate) fn identifies(&self) -> bool {
        self.receipt.map_or(
            self.outcome.result == OutcomeStatus::Applied,
            CompletionReceipt::identifies,
        )
    }
}

impl Game {
    /// Whether this actor currently owns the live detection capability; no census or mutation.
    pub fn monster_detection_active(&self, actor: ActorId) -> bool {
        let Some(state) = self.actors.get(&actor).filter(|state| state.alive()) else {
            return false;
        };
        let Some(creature) = state.combat.as_ref().map(|combat| combat.creature()) else {
            return false;
        };
        let elapsed = self.actor_clock(actor).saturating_sub(state.active_since);
        creature.conditions().instances().any(|instance| {
            instance.phase == crate::conditions::ConditionPhase::Active
                && instance
                    .remaining
                    .is_some_and(|remaining| remaining > elapsed)
                && creature
                    .build()
                    .ruleset()
                    .condition(&instance.condition)
                    .is_some_and(|definition| definition.detecting_monsters)
        })
    }
    /// Whole canonical floor via exclusive region ownership, not Room/Passage
    /// membership or residency. Missing metadata is an unavailable chart, not an
    /// invented one-region scope; producer separately requires a real scope.
    pub(crate) fn detection_required_regions(
        &self,
        actor: ActorId,
    ) -> Result<BTreeSet<RegionId>, GameError> {
        let state = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        Ok(self
            .spatial_registry
            .scopes()
            .find(|scope| scope.members.binary_search(&state.location.region).is_ok())
            .map(|scope| scope.members.iter().copied().collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
#[path = "detection_lifecycle_tests.rs"]
pub(crate) mod tests;
