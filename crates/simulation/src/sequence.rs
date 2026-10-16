//! Active-work progress and checked commitment debt. Frozen plans never advance.
use crate::combat::{Preparation, SequenceView};
use crate::{abilities::Plan, GameError, ScheduleError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationProgress {
    Ability {
        next_strike: u16,
        successful_primaries: u16,
        attempted_riders: [u64; 2],
    },
    Item {},
    Respec {},
}
impl<'de> Deserialize<'de> for PreparationProgress {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Ability {
                next_strike: u16,
                successful_primaries: u16,
                attempted_riders: [u64; 2],
            },
            Item {},
            Respec {},
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Ability {
                next_strike,
                successful_primaries,
                attempted_riders,
            } => {
                if usize::from(next_strike) > crate::limits::STRIKES
                    || u32::from(successful_primaries) >> next_strike != 0
                {
                    return Err(serde::de::Error::custom("invalid ability progress prefix"));
                }
                Self::Ability {
                    next_strike,
                    successful_primaries,
                    attempted_riders,
                }
            }
            Fields::Item {} => Self::Item {},
            Fields::Respec {} => Self::Respec {},
        })
    }
}
impl PreparationProgress {
    pub(crate) fn new(plan: &Plan) -> Self {
        if matches!(plan, Plan::Respec(_)) {
            Self::Respec {}
        } else if plan.as_ability().is_some() {
            Self::Ability {
                next_strike: 0,
                successful_primaries: 0,
                attempted_riders: [0; 2],
            }
        } else {
            Self::Item {}
        }
    }

    pub(crate) fn advanced(self) -> Self {
        match self {
            Self::Ability {
                next_strike,
                successful_primaries,
                attempted_riders,
            } => Self::Ability {
                next_strike: next_strike + 1,
                successful_primaries,
                attempted_riders,
            },
            other => other,
        }
    }

    pub(crate) fn record_successful_primary(&mut self, index: usize) {
        if let Self::Ability {
            successful_primaries,
            ..
        } = self
        {
            *successful_primaries |= 1 << index;
        }
    }

    /// Consume the source-qualified once receipt before its ordinary attempt.
    pub(crate) fn attempt_conditional_rider(
        &mut self,
        plan: &crate::abilities::AbilityPlan,
        strike: usize,
        rider: usize,
    ) -> bool {
        let Some(condition) = plan.strikes[strike].riders[rider].condition else {
            return true;
        };
        let Some(slot) = plan.conditional_rider_slot(strike, rider) else {
            return false;
        };
        let Self::Ability {
            successful_primaries,
            attempted_riders,
            ..
        } = self
        else {
            return false;
        };
        let crate::rules::RiderCondition::SequenceSuccessfulHits { minimum } = condition;
        if *successful_primaries & (1 << strike) == 0
            || successful_primaries.count_ones() < u32::from(minimum)
            || attempted_riders[slot / 64] & (1 << (slot % 64)) != 0
        {
            return false;
        }
        attempted_riders[slot / 64] |= 1 << (slot % 64);
        true
    }

    /// Earliest eligible completed occurrence per bounded canonical rider slot.
    fn eligible_riders(self, plan: &crate::abilities::AbilityPlan) -> Option<[Option<usize>; 128]> {
        let Self::Ability {
            next_strike,
            successful_primaries,
            ..
        } = self
        else {
            return None;
        };
        let next = usize::from(next_strike);
        if next > plan.strikes.len()
            || next > crate::limits::STRIKES
            || u32::from(successful_primaries) >> next != 0
        {
            return None;
        }
        let mut eligible = [None; 128];
        let mut hits = 0;
        for (index, strike) in plan.strikes.iter().take(next).enumerate() {
            if successful_primaries & (1 << index) == 0 {
                continue;
            }
            if !strike.qualifying_primary_check() {
                return None;
            }
            hits += 1;
            for (ordinal, rider) in strike.riders.iter().enumerate() {
                if let Some(crate::rules::RiderCondition::SequenceSuccessfulHits { minimum }) =
                    rider.condition
                {
                    let slot = plan.conditional_rider_slot(index, ordinal)?;
                    if hits >= minimum {
                        eligible[slot].get_or_insert(index);
                    }
                }
            }
        }
        Some(eligible)
    }

    fn history_valid(self, plan: &crate::abilities::AbilityPlan) -> bool {
        let Self::Ability {
            successful_primaries,
            attempted_riders,
            ..
        } = self
        else {
            return false;
        };
        let Some(eligible) = self.eligible_riders(plan) else {
            return false;
        };
        eligible.iter().enumerate().all(|(slot, site)| {
            let attempted = attempted_riders[slot / 64] & (1 << (slot % 64)) != 0;
            match site {
                None => !attempted,
                // A later hit proves the target survived the eligible attempt site.
                Some(index) => attempted || u32::from(successful_primaries) >> (index + 1) == 0,
            }
        })
    }

    /// Loaded, committed live targets cannot justify an omitted past attempt.
    pub(crate) fn conditional_receipts_complete(
        self,
        plan: &crate::abilities::AbilityPlan,
    ) -> bool {
        let Self::Ability {
            attempted_riders, ..
        } = self
        else {
            return false;
        };
        self.eligible_riders(plan).is_some_and(|eligible| {
            eligible.iter().enumerate().all(|(slot, site)| {
                site.is_none() || attempted_riders[slot / 64] & (1 << (slot % 64)) != 0
            })
        })
    }
}
impl Preparation {
    pub(crate) fn next_strike(&self) -> Option<usize> {
        match self.progress {
            PreparationProgress::Ability { next_strike, .. } => Some(usize::from(next_strike)),
            PreparationProgress::Item {} | PreparationProgress::Respec {} => None,
        }
    }
    pub(crate) fn has_prefix(&self) -> bool {
        self.next_strike().is_some_and(|index| index > 0)
    }
    pub(crate) fn remaining_at(&self, clock: u64) -> u64 {
        if self.active {
            self.remaining
                .saturating_sub(clock.saturating_sub(self.started))
        } else {
            self.remaining
        }
    }
    /// Remaining execution work through the last strike, followed by recovery.
    /// A suspended preparation accrues no elapsed execution work.
    pub(crate) fn completion_delay(&self, clock: u64) -> Result<u64, GameError> {
        let tail = match (self.plan.as_ability(), self.next_strike()) {
            (Some(plan), Some(index)) => plan.strikes.last().and_then(|last| {
                plan.strikes
                    .get(index)
                    .and_then(|next| last.offset.checked_sub(next.offset))
            }),
            (None, None) => Some(0),
            _ => None,
        }
        .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
        self.remaining_at(clock)
            .checked_add(tail)
            .and_then(|value| value.checked_add(self.plan.recovery()))
            .ok_or(GameError::Schedule(ScheduleError::TimeExhausted))
    }
    pub(crate) fn abandon_delay(&self, clock: u64) -> Result<u64, GameError> {
        if self.has_prefix() {
            self.completion_delay(clock)
        } else {
            Ok(0)
        }
    }
    pub(crate) fn progress_valid(&self) -> bool {
        let bound = match (self.plan.as_ability(), self.next_strike()) {
            (Some(plan), Some(0)) if !plan.strikes.is_empty() => Some(plan.preparation),
            (Some(plan), Some(index)) if index > 0 => plan
                .strikes
                .get(index)
                .zip(plan.strikes.get(index - 1))
                .and_then(|(next, previous)| next.offset.checked_sub(previous.offset)),
            (None, None) => Some(self.plan.preparation()),
            _ => None,
        };
        matches!(
            (&self.plan, self.progress),
            (Plan::Respec(_), PreparationProgress::Respec {})
                | (Plan::Ability(_), PreparationProgress::Ability { .. })
                | (
                    Plan::Equip { .. }
                        | Plan::Unequip { .. }
                        | Plan::Drink { .. }
                        | Plan::Eat { .. },
                    PreparationProgress::Item {}
                )
        ) && self.item_payment.shape_valid(&self.plan, self.has_prefix())
            && self
                .plan
                .as_ability()
                .is_none_or(|plan| self.progress.history_valid(plan))
            && bound.is_some_and(|bound| self.remaining <= bound)
            && if self.has_prefix() {
                self.hold.is_none()
            } else {
                self.plan.cost().is_some() == self.hold.is_some()
            }
    }
    pub(crate) fn schedule_valid(&self, clock: u64, ready_at: u64) -> bool {
        self.progress_valid()
            && self.started <= clock
            && self
                .completion_delay(clock)
                .ok()
                .and_then(|delay| clock.checked_add(delay))
                .is_some()
            && (!self.active
                || self
                    .started
                    .checked_add(self.remaining)
                    .is_some_and(|due| due >= clock && ready_at == due))
    }
    pub(crate) fn sequence_view(&self, clock: u64) -> Option<SequenceView> {
        let plan = self.plan.as_ability()?;
        if plan.strikes.len() < 2 {
            return None;
        }
        Some(SequenceView {
            completed: self.next_strike()? as u16,
            total: plan.strikes.len() as u16,
            next_strike_remaining: self.remaining_at(clock),
            cancel_recovery_remaining: self.abandon_delay(clock).ok()?,
        })
    }
}
