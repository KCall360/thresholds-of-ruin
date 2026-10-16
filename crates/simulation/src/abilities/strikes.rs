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
    Ability {
        ordinal: u16,
    },
    Natural {
        species: SpeciesId,
        ordinal: u16,
    },
    NaturalMelee {
        species: SpeciesId,
        ordinal: u16,
        natural_riders: u16,
    },
    Weapon {
        item: ItemId,
    },
}
impl StrikeOrigin {
    pub fn ordinal(&self) -> u16 {
        match self {
            Self::Ability { ordinal }
            | Self::Natural { ordinal, .. }
            | Self::NaturalMelee { ordinal, .. } => *ordinal,
            Self::Weapon { .. } => 0,
        }
    }
}

impl super::AbilityPlan {
    /// Appended natural riders keep their source ordinal across copied rows.
    /// Natural row-local riders retain distinct slots even when row lengths differ.
    pub(crate) fn conditional_rider_slot(&self, strike: usize, rider: usize) -> Option<usize> {
        let row = self.strikes.get(strike)?;
        let occurrence = row.riders.get(rider)?;
        occurrence.condition?;
        let slot = match &row.origin {
            StrikeOrigin::NaturalMelee {
                species,
                natural_riders,
                ..
            } if rider >= usize::from(*natural_riders) => {
                let first = self.strikes.first()?;
                let StrikeOrigin::NaturalMelee {
                    species: owner,
                    natural_riders: boundary,
                    ..
                } = &first.origin
                else {
                    return None;
                };
                let ordinal = usize::from(*boundary) + rider - usize::from(*natural_riders);
                let canonical = first.riders.get(ordinal)?;
                if owner != species || !canonical.same_copied_operands(occurrence) {
                    return None;
                }
                ordinal
            }
            _ => strike * crate::limits::RIDERS_PER_STRIKE + rider,
        };
        (slot < 128).then_some(slot)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRider {
    pub trigger: crate::rules::RiderTrigger,
    pub condition: Option<crate::rules::RiderCondition>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub check: Option<PlannedCheck>,
    #[serde(deserialize_with = "read_effects")]
    pub effects: Vec<PlannedEffect>,
    #[serde(deserialize_with = "read_descriptors")]
    pub descriptors: Vec<DescriptorId>,
}
impl FrozenRider {
    /// One appended source is captured with the same operands on every row.
    /// Check Edge alone can differ through structural ownership and row-zero tokens.
    pub(super) fn same_copied_operands(&self, other: &Self) -> bool {
        self.trigger == other.trigger
            && self.condition == other.condition
            && self.effects == other.effects
            && self.descriptors == other.descriptors
            && match (&self.check, &other.check) {
                (None, None) => true,
                (Some(left), Some(right)) => {
                    left.actor == right.actor
                        && left.target_participates == right.target_participates
                        && left.bonus == right.bonus
                        && left.threshold == right.threshold
                }
                _ => false,
            }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
impl<'de> Deserialize<'de> for FrozenRider {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            trigger: crate::rules::RiderTrigger,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            condition: Option<crate::rules::RiderCondition>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            check: Option<PlannedCheck>,
            #[serde(deserialize_with = "read_effects")]
            effects: Vec<PlannedEffect>,
            #[serde(deserialize_with = "read_descriptors")]
            descriptors: Vec<DescriptorId>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            trigger: fields.trigger,
            condition: fields.condition,
            check: fields.check,
            effects: fields.effects,
            descriptors: fields.descriptors,
        })
    }
}
impl<'de> Deserialize<'de> for FrozenStrike {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            offset: u64,
            origin: StrikeOrigin,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            check: Option<PlannedCheck>,
            #[serde(deserialize_with = "read_effects")]
            on_success: Vec<PlannedEffect>,
            #[serde(deserialize_with = "read_effects")]
            on_failure: Vec<PlannedEffect>,
            #[serde(deserialize_with = "read_descriptors")]
            descriptors: Vec<DescriptorId>,
            #[serde(deserialize_with = "read_riders")]
            riders: Vec<FrozenRider>,
            edge: i64,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            shared_check: Option<CheckPhase>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            offset: fields.offset,
            origin: fields.origin,
            check: fields.check,
            on_success: fields.on_success,
            on_failure: fields.on_failure,
            descriptors: fields.descriptors,
            riders: fields.riders,
            edge: fields.edge,
            shared_check: fields.shared_check,
        })
    }
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
    pub(crate) fn qualifying_primary_check(&self) -> bool {
        self.check
            .as_ref()
            .is_some_and(|check| check.actor == CheckActor::User && check.target_participates)
    }

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

    /// The check's structural role does not collapse when actor == target.
    pub(super) fn check_role(&self, phase: CheckPhase) -> Option<CheckActor> {
        match phase {
            CheckPhase::Primary { side } => {
                let check = self.check.as_ref()?;
                match side {
                    CheckSide::Initiating => Some(check.actor),
                    CheckSide::Opposing
                        if matches!(check.threshold, CheckThreshold::Opposed { .. }) =>
                    {
                        Some(match check.actor {
                            CheckActor::User => CheckActor::Target,
                            CheckActor::Target => CheckActor::User,
                        })
                    }
                    CheckSide::Opposing => None,
                }
            }
            CheckPhase::Rider { ordinal } => self
                .riders
                .get(usize::from(ordinal))?
                .check
                .as_ref()
                .map(|check| check.actor),
        }
    }
    pub(super) fn check_participants(
        &self,
        phase: CheckPhase,
        actor: crate::ActorId,
        target: crate::ActorId,
    ) -> Option<(crate::ActorId, crate::ActorId)> {
        self.check_role(phase).map(|role| match role {
            CheckActor::User => (actor, target),
            CheckActor::Target => (target, actor),
        })
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
                    && rider.condition.is_none_or(|condition| {
                        condition.valid() && rider.trigger == crate::rules::RiderTrigger::Success
                    })
                    && rider.check.as_ref().is_none_or(|check| {
                        check_valid(check)
                            && check.actor == CheckActor::User
                            && check.target_participates
                            && matches!(check.threshold, CheckThreshold::Fixed { .. })
                    })
                    && super::plan::effects_valid(&rider.effects, rules)
                    && descriptors_valid(&rider.descriptors)
            })
    }
}
fn check_valid(check: &PlannedCheck) -> bool {
    (check.target_participates
        || (check.actor == CheckActor::User
            && matches!(check.threshold, CheckThreshold::Fixed { .. })))
        && check.bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
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
