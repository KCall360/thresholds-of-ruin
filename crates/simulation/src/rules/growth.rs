//! Bounded, provisional numerical policy for ordinary earned progression.
use crate::content_ids::{
    ClassId, ExpertiseId, SpeciesId, SubtypeId, TalentId, TemplateId, TypeId,
};
use serde::{Deserialize, Serialize};

/// Permanent source identities only. Equipment, conditions and actor identities
/// deliberately cannot appear in this catalog assessment policy.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThreatSource {
    Species { id: SpeciesId },
    Type { id: TypeId },
    Subtype { id: SubtypeId },
    Class { id: ClassId },
    Template { id: TemplateId },
    Talent { id: TalentId },
    Expertise { id: ExpertiseId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreatOverride {
    pub source: ThreatSource,
    pub points: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreatPolicy {
    pub points_per_tier: u32,
    pub hit_die: u32,
    pub trained_rank: u32,
    pub species: u32,
    pub creature_type: u32,
    pub subtype: u32,
    pub class: u32,
    pub template: u32,
    pub talent: u32,
    pub expertise: u32,
    #[serde(
        deserialize_with = "super::strikes::bounded_vec::<_, _, { crate::limits::CATALOG_ENTRIES }>"
    )]
    pub overrides: Vec<ThreatOverride>,
}

/// All amounts are integer rules content. These values are tuning inputs, not
/// claims that a source's mechanical complexity or arena strength is balanced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthPolicy {
    pub threshold_base: u32,
    pub threshold_per_tier: u32,
    pub reward_base: u32,
    pub reward_per_relative_tier: u32,
    pub minimum_reward: u32,
    pub maximum_reward: u32,
    pub recovery: u64,
    pub threat: ThreatPolicy,
}

impl ThreatPolicy {
    pub fn contribution(&self, source: &ThreatSource) -> u32 {
        self.overrides
            .iter()
            .find(|value| &value.source == source)
            .map(|value| value.points)
            .unwrap_or(match source {
                ThreatSource::Species { .. } => self.species,
                ThreatSource::Type { .. } => self.creature_type,
                ThreatSource::Subtype { .. } => self.subtype,
                ThreatSource::Class { .. } => self.class,
                ThreatSource::Template { .. } => self.template,
                ThreatSource::Talent { .. } => self.talent,
                ThreatSource::Expertise { .. } => self.expertise,
            })
    }
}

impl GrowthPolicy {
    /// Price for one ordinal at the highest applied tier. Tiers zero and one
    /// intentionally share the base price. A price is never clipped to fit.
    pub fn threshold(&self, tier: crate::HdCount) -> Option<u64> {
        u128::from(self.threshold_per_tier)
            .checked_mul(u128::from(tier.get().saturating_sub(1)))?
            .checked_add(u128::from(self.threshold_base))?
            .try_into()
            .ok()
    }

    /// Exact prices paid while growing from `first` to `end` (exclusive).
    /// None means invalid order or a sum beyond the u64 XP account. All terms
    /// are positive after validation, so an overflowing sum exceeds any budget.
    pub fn threshold_sum(&self, first: crate::HdCount, end: crate::HdCount) -> Option<u64> {
        let mut first = first.get();
        let end = end.get();
        if end < first {
            return None;
        }
        if end == first {
            return Some(0);
        }
        let zero_price = if first == 0 {
            first = 1;
            u128::from(self.threshold_base)
        } else {
            0
        };
        let count = u128::from(end - first);
        let bases = count.checked_mul(u128::from(self.threshold_base))?;
        let slope = if count == 0 {
            0
        } else {
            // Sum (tier - 1). Divide an even factor before multiplication.
            let endpoints = u128::from(first - 1) + u128::from(end - 2);
            let (left, right) = if count % 2 == 0 {
                (count / 2, endpoints)
            } else {
                (count, endpoints / 2)
            };
            left.checked_mul(right)?
                .checked_mul(u128::from(self.threshold_per_tier))?
        };
        zero_price
            .checked_add(bases)?
            .checked_add(slope)?
            .try_into()
            .ok()
    }

    pub fn reward(&self, victim_threat: u64, recipient_highest_tier: crate::HdCount) -> u32 {
        let relative = i128::from(victim_threat) - i128::from(recipient_highest_tier.get());
        let amount = (i128::from(self.reward_base)
            + relative * i128::from(self.reward_per_relative_tier))
        .clamp(
            i128::from(self.minimum_reward),
            i128::from(self.maximum_reward),
        );
        u32::try_from(amount).expect("reward is clamped to authored u32 bounds")
    }

    pub(super) fn validate(&self) -> Result<(), super::RulesetError> {
        let maximum = crate::limits::NUMERIC as u32;
        let t = &self.threat;
        super::bound(
            self.threshold_base > 0
                && self.threshold_base <= maximum
                && self.threshold_per_tier <= maximum
                && self.minimum_reward <= self.maximum_reward
                && self.reward_base <= maximum
                && self.reward_per_relative_tier <= maximum
                && self.maximum_reward <= maximum
                && self.recovery <= u64::from(maximum)
                && t.points_per_tier > 0
                && [
                    t.points_per_tier,
                    t.hit_die,
                    t.trained_rank,
                    t.species,
                    t.creature_type,
                    t.subtype,
                    t.class,
                    t.template,
                    t.talent,
                    t.expertise,
                ]
                .iter()
                .all(|value| *value <= maximum)
                && t.overrides.len() <= crate::limits::CATALOG_ENTRIES
                && t.overrides.iter().all(|value| value.points <= maximum)
                && t.overrides
                    .iter()
                    .map(|value| &value.source)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == t.overrides.len(),
            "growth_policy",
        )
    }
}
