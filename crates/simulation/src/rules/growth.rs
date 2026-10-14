//! Bounded, provisional numerical policy for ordinary earned progression.
use crate::content_ids::{ClassId, SpeciesId, SubtypeId, TalentId, TemplateId, TypeId};
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
            })
    }
}

impl GrowthPolicy {
    pub fn threshold(&self, highest_earned_tier: u16) -> u32 {
        // Validation proves the complete supported tier domain fits the bound.
        self.threshold_base
            + self.threshold_per_tier * u32::from(highest_earned_tier.saturating_sub(1))
    }

    pub fn reward(&self, victim_threat: u16, recipient_highest_tier: u16) -> u32 {
        let relative = i64::from(victim_threat) - i64::from(recipient_highest_tier);
        (i64::from(self.reward_base) + relative * i64::from(self.reward_per_relative_tier)).clamp(
            i64::from(self.minimum_reward),
            i64::from(self.maximum_reward),
        ) as u32
    }

    pub(super) fn validate(&self) -> Result<(), super::RulesetError> {
        let maximum = crate::limits::NUMERIC as u32;
        let t = &self.threat;
        super::bound(
            self.threshold_base > 0
                && self.threshold_base <= maximum
                && self.threshold_per_tier <= maximum
                && u64::from(self.threshold_base)
                    + u64::from(self.threshold_per_tier) * (crate::limits::HIT_DICE as u64 - 1)
                    <= u64::from(maximum)
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
