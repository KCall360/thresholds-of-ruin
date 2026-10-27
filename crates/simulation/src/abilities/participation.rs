//! Bounded attempted-role evidence for ordinary combat disclosure.
//!
//! This records resolution facts, not a second plan. Pending plans are validated
//! against their sources. Checkpoint restoration checks retained facts for
//! consistency; it does not reconstruct every discarded historical operand.
use super::{Outcome, OutcomeStatus, PlannedCheck, PlannedEffect};
use crate::rules::{CheckActor, EffectRecipient};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckParticipation {
    None,
    UserOnly,
    TargetInvolved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnselectedReason {
    Immune,
    Dead,
    UnavailableTarget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PayloadParticipation {
    Unselected { reason: UnselectedReason },
    Empty,
    Source,
    Target,
}
impl<'de> Deserialize<'de> for PayloadParticipation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unselected { reason: UnselectedReason },
            Empty {},
            Source {},
            Target {},
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Unselected { reason } => Self::Unselected { reason },
            Fields::Empty {} => Self::Empty,
            Fields::Source {} => Self::Source,
            Fields::Target {} => Self::Target,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptedParticipation {
    pub check: CheckParticipation,
    pub payload: PayloadParticipation,
}
impl<'de> Deserialize<'de> for AttemptedParticipation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            check: CheckParticipation,
            payload: PayloadParticipation,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            check: fields.check,
            payload: fields.payload,
        })
    }
}
impl AttemptedParticipation {
    pub const fn source() -> Self {
        Self {
            check: CheckParticipation::None,
            payload: PayloadParticipation::Source,
        }
    }
    pub const fn target() -> Self {
        Self {
            check: CheckParticipation::None,
            payload: PayloadParticipation::Target,
        }
    }
    pub const fn empty() -> Self {
        Self {
            check: CheckParticipation::None,
            payload: PayloadParticipation::Empty,
        }
    }
    pub(crate) fn blocked(check: Option<&PlannedCheck>, reason: UnselectedReason) -> Self {
        Self {
            check: Self::check(check),
            payload: PayloadParticipation::Unselected { reason },
        }
    }
    pub(crate) fn selected(check: Option<&PlannedCheck>, effects: &[PlannedEffect]) -> Self {
        Self {
            check: Self::check(check),
            payload: effects
                .first()
                .map_or(PayloadParticipation::Empty, |effect| {
                    match effect.recipient() {
                        EffectRecipient::Source => PayloadParticipation::Source,
                        EffectRecipient::Target => PayloadParticipation::Target,
                    }
                }),
        }
    }
    fn check(check: Option<&PlannedCheck>) -> CheckParticipation {
        check.map_or(CheckParticipation::None, |check| {
            if check.target_participates {
                CheckParticipation::TargetInvolved
            } else {
                CheckParticipation::UserOnly
            }
        })
    }
    pub(crate) fn target_participated(self) -> bool {
        self.check == CheckParticipation::TargetInvolved
            || matches!(self.payload, PayloadParticipation::Target)
    }
    /// Only internal consistency is provable for retained historical facts.
    pub(crate) fn valid(self, outcome: &Outcome) -> bool {
        if let PayloadParticipation::Unselected { reason } = self.payload {
            return outcome.check.is_none()
                && outcome.effects.is_empty()
                && match reason {
                    UnselectedReason::Immune => self.check != CheckParticipation::None,
                    UnselectedReason::Dead | UnselectedReason::UnavailableTarget => {
                        self.check == CheckParticipation::None
                    }
                };
        }
        let check_valid = match (self.check, &outcome.check) {
            (CheckParticipation::None, None) => true,
            (CheckParticipation::UserOnly, Some(check)) => {
                check.actor == CheckActor::User && !check.opposed
            }
            (CheckParticipation::TargetInvolved, Some(_)) => true,
            _ => false,
        };
        check_valid
            && match self.payload {
                PayloadParticipation::Empty => outcome.effects.is_empty(),
                PayloadParticipation::Source => outcome
                    .effects
                    .iter()
                    .all(|effect| effect.recipient() == EffectRecipient::Source),
                PayloadParticipation::Target => outcome
                    .effects
                    .iter()
                    .all(|effect| effect.recipient() == EffectRecipient::Target),
                PayloadParticipation::Unselected { .. } => unreachable!(),
            }
    }
}

impl Outcome {
    /// One canonical mapping shared by retained-history validation and trusted
    /// publication's normalization of caller-supplied non-Miss status hints.
    pub(crate) fn canonical_result(&self) -> OutcomeStatus {
        match self.participation.payload {
            PayloadParticipation::Unselected {
                reason: UnselectedReason::UnavailableTarget,
            } => OutcomeStatus::Miss,
            PayloadParticipation::Unselected { .. } => OutcomeStatus::Unaffected,
            PayloadParticipation::Empty
                if self
                    .check
                    .as_ref()
                    .is_some_and(|check| check.actor == CheckActor::User && !check.success) =>
            {
                OutcomeStatus::Miss
            }
            _ if self.effects.iter().any(super::ResolvedEffect::changed) => OutcomeStatus::Applied,
            _ => OutcomeStatus::Unaffected,
        }
    }
    pub(crate) fn valid(&self) -> bool {
        crate::detection_lifecycle::resolved_shape_valid(self)
            && self.participation.valid(self)
            && self.resistance.is_none_or(|check| {
                check.actor == crate::rules::CheckActor::Target
                    && !check.opposed
                    && self.participation.payload == PayloadParticipation::Target
                    && (!check.success || self.effects.is_empty())
            })
            && self.result == self.canonical_result()
            && (!self.target_rider_participated
                || (self.ability.is_some()
                    && !matches!(
                        self.participation.payload,
                        PayloadParticipation::Unselected { .. }
                    )))
    }
    pub(crate) fn target_participated(&self) -> bool {
        self.participation.target_participated() || self.target_rider_participated
    }
}
