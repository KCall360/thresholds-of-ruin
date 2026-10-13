//! Frozen strike operands and bounded identities. Progress never lives here.
use super::{CheckThreshold, PlannedCheck, PlannedEffect};
use crate::content_ids::{DescriptorId, SpeciesId};
use crate::rules::CheckActor;
use crate::ItemId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSide {
    Initiating,
    Opposing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckPhase {
    Primary { side: CheckSide },
    Rider { ordinal: u16 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSite {
    pub strike: u16,
    pub check: CheckPhase,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenClaim {
    pub reservation: crate::conditions::TokenReservation,
    pub site: CheckSite,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikeOrigin {
    Ability { ordinal: u16 },
    Natural { species: SpeciesId, ordinal: u16 },
    Weapon { item: ItemId },
}
impl StrikeOrigin {
    pub fn ordinal(&self) -> u16 {
        match self {
            Self::Ability { ordinal } | Self::Natural { ordinal, .. } => *ordinal,
            Self::Weapon { .. } => 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRider {
    pub trigger: crate::rules::RiderTrigger,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub check: Option<PlannedCheck>,
    #[serde(deserialize_with = "read_effects")]
    pub effects: Vec<PlannedEffect>,
    #[serde(deserialize_with = "read_descriptors")]
    pub descriptors: Vec<DescriptorId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenStrike {
    pub offset: u64,
    pub origin: StrikeOrigin,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub check: Option<PlannedCheck>,
    #[serde(deserialize_with = "read_effects")]
    pub on_success: Vec<PlannedEffect>,
    #[serde(deserialize_with = "read_effects")]
    pub on_failure: Vec<PlannedEffect>,
    #[serde(deserialize_with = "read_descriptors")]
    pub descriptors: Vec<DescriptorId>,
    #[serde(deserialize_with = "read_riders")]
    pub riders: Vec<FrozenRider>,
    pub edge: i64,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub shared_check: Option<CheckPhase>,
}
macro_rules! bounded_reader {
    ($name:ident, $type:ty, $limit:expr) => {
        pub(super) fn $name<'de, D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Vec<$type>, D::Error> {
            crate::bounded::Bounded::<$type, { $limit }>::deserialize(deserializer)
                .map(|bounded| bounded.0)
        }
    };
}
bounded_reader!(read_strikes, FrozenStrike, crate::limits::STRIKES);
bounded_reader!(read_riders, FrozenRider, crate::limits::RIDERS_PER_STRIKE);
bounded_reader!(
    read_effects,
    PlannedEffect,
    crate::limits::EFFECTS_PER_BRANCH
);
bounded_reader!(
    read_descriptors,
    DescriptorId,
    crate::limits::DESCRIPTORS_PER_EFFECT
);
bounded_reader!(read_claims, TokenClaim, 2 * crate::limits::CONDITIONS);

impl FrozenStrike {
    /// Structural ownership is independent of checks actually reached at runtime.
    pub fn first_attacker_check(&self) -> Option<CheckPhase> {
        if let Some(check) = &self.check {
            if check.actor == CheckActor::User {
                return Some(CheckPhase::Primary {
                    side: CheckSide::Initiating,
                });
            }
            if matches!(check.threshold, CheckThreshold::Opposed { .. }) {
                return Some(CheckPhase::Primary {
                    side: CheckSide::Opposing,
                });
            }
        }
        self.riders
            .iter()
            .position(|rider| rider.check.is_some())
            .map(|ordinal| CheckPhase::Rider {
                ordinal: ordinal as u16,
            })
    }

    pub(super) fn check_participants(
        &self,
        phase: CheckPhase,
        actor: crate::ActorId,
        target: crate::ActorId,
    ) -> Option<(crate::ActorId, crate::ActorId)> {
        match phase {
            CheckPhase::Primary { side } => {
                let check = self.check.as_ref()?;
                let (roller, opponent) = match check.actor {
                    CheckActor::User => (actor, target),
                    CheckActor::Target => (target, actor),
                };
                match side {
                    CheckSide::Initiating => Some((roller, opponent)),
                    CheckSide::Opposing
                        if matches!(check.threshold, CheckThreshold::Opposed { .. }) =>
                    {
                        Some((opponent, roller))
                    }
                    CheckSide::Opposing => None,
                }
            }
            CheckPhase::Rider { ordinal } => self
                .riders
                .get(usize::from(ordinal))?
                .check
                .as_ref()
                .map(|_| (actor, target)),
        }
    }

    pub(super) fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let descriptors_valid = |values: &[DescriptorId]| {
            values.len() <= crate::limits::DESCRIPTORS_PER_EFFECT
                && values
                    .iter()
                    .all(|id| rules.document().descriptors.iter().any(|def| &def.id == id))
        };
        self.offset <= crate::limits::NUMERIC as u64
            && usize::from(self.origin.ordinal()) < crate::limits::STRIKES
            && self.shared_check == self.first_attacker_check()
            && self.edge.unsigned_abs() <= crate::limits::NUMERIC as u64
            && self.check.as_ref().is_none_or(check_valid)
            && super::plan::effects_valid(&self.on_success, rules)
            && super::plan::effects_valid(&self.on_failure, rules)
            && descriptors_valid(&self.descriptors)
            && self.riders.len() <= crate::limits::RIDERS_PER_STRIKE
            && self.riders.iter().all(|rider| {
                (self.check.is_some() || rider.trigger == crate::rules::RiderTrigger::Success)
                    && rider.check.as_ref().is_none_or(|check| {
                        check_valid(check)
                            && check.actor == CheckActor::User
                            && matches!(check.threshold, CheckThreshold::Fixed { .. })
                    })
                    && super::plan::effects_valid(&rider.effects, rules)
                    && descriptors_valid(&rider.descriptors)
            })
    }
}
fn check_valid(check: &PlannedCheck) -> bool {
    check.bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
        && check.edge.unsigned_abs() <= crate::limits::NUMERIC as u64
        && match check.threshold {
            CheckThreshold::Fixed { value } => {
                value.unsigned_abs() <= crate::limits::NUMERIC as u64
            }
            CheckThreshold::Opposed { bonus, edge } => {
                bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
                    && edge.unsigned_abs() <= crate::limits::NUMERIC as u64
            }
        }
}
