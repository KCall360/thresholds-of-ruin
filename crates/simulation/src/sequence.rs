//! Active-work progress and checked commitment debt. Frozen plans never advance.
use crate::combat::{Preparation, SequenceView};
use crate::{abilities::Plan, GameError, ScheduleError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparationProgress {
    Ability { next_strike: u16 },
    Item {},
}
impl PreparationProgress {
    pub(crate) fn new(plan: &Plan) -> Self {
        if plan.as_ability().is_some() {
            Self::Ability { next_strike: 0 }
        } else {
            Self::Item {}
        }
    }
}
impl Preparation {
    pub(crate) fn next_strike(&self) -> Option<usize> {
        match self.progress {
            PreparationProgress::Ability { next_strike } => Some(usize::from(next_strike)),
            PreparationProgress::Item {} => None,
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
        self.item_payment.shape_valid(&self.plan, self.has_prefix())
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
