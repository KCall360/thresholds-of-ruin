//! Private, bounded pages of numerical combat records. These DTOs contain no
//! simulation/save types and grant no authority to receive another actor's data.
use crate::{
    ActorId, CombatTraceStep, CombatTraceView, InspectionFear, InvalidCombatDiagnostics, Resource,
    Skill, Technique,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_COMBAT_REPORT_RECORDS: usize = 8;
pub const MAX_RETAINED_COMBAT_RECORDS: u8 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticResource {
    pub resource: Resource,
    pub maximum: u32,
    pub balance: u32,
    pub available: u32,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery_elapsed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticCombatant {
    pub health: u32,
    pub maximum_health: u32,
    #[serde(deserialize_with = "Option::deserialize")]
    pub injury: Option<u32>,
    pub dead: bool,
    pub resources: Vec<DiagnosticResource>,
    pub fear: Vec<InspectionFear>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticCost {
    pub resource: Resource,
    pub start: u32,
    pub resolution: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatDiagnosticRecord {
    #[serde(with = "crate::integers::unsigned")]
    pub sequence: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Technique,
    #[serde(with = "crate::integers::optional_unsigned")]
    pub intention: Option<u64>,
    #[serde(with = "crate::integers::optional_unsigned")]
    pub origin_intention: Option<u64>,
    #[serde(with = "crate::integers::unsigned")]
    pub preparation: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub started: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub remaining: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery: u64,
    #[serde(deserialize_with = "Option::deserialize")]
    pub charge: Option<DiagnosticCost>,
    pub actor_before: DiagnosticCombatant,
    pub target_before: DiagnosticCombatant,
    pub actor_after: DiagnosticCombatant,
    pub target_after: DiagnosticCombatant,
    pub applied: bool,
    pub damage: u32,
    pub trace: CombatTraceView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatDiagnosticsView {
    pub enabled: bool,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub captured: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub dropped: u64,
    pub retained: u8,
    /// Inclusive end of this page; older pages can precede its first sequence.
    #[serde(with = "crate::integers::unsigned")]
    pub through: u64,
    pub records: Vec<CombatDiagnosticRecord>,
}

impl DiagnosticCombatant {
    fn valid(&self) -> bool {
        if self.maximum_health > 1_000_000
            || self.health > self.maximum_health
            || (self.dead && self.health != 0)
            || (!self.dead && self.health == 0)
        {
            return false;
        }
        if let Some(injury) = self.injury {
            if injury > 1_000_000
                || (!self.dead && self.maximum_health.checked_sub(injury) != Some(self.health))
            {
                return false;
            }
            if self.resources.len() != 3 {
                return false;
            }
            for (value, resource) in
                self.resources
                    .iter()
                    .zip([Resource::Stamina, Resource::Focus, Resource::Mana])
            {
                let period = match resource {
                    Resource::Stamina => 100,
                    Resource::Focus => 300,
                    Resource::Mana => 1000,
                };
                if value.resource != resource
                    || value.maximum > 1_000_000
                    || value.balance > value.maximum
                    || value.available > value.balance
                    || value.recovery_elapsed >= period
                    || (value.balance == value.maximum && value.recovery_elapsed != 0)
                {
                    return false;
                }
            }
        } else if !self.resources.is_empty() || !self.fear.is_empty() {
            return false;
        }
        if self.fear.len() > 128 || (self.dead && !self.fear.is_empty()) {
            return false;
        }
        self.fear.iter().enumerate().all(|(index, source)| {
            source.causer.0 != 0
                && (1..=1_000_000).contains(&source.remaining_ticks)
                && (index == 0 || self.fear[index - 1].causer < source.causer)
        })
    }

    fn fear_map(&self) -> BTreeMap<ActorId, u64> {
        self.fear
            .iter()
            .map(|source| (source.causer, source.remaining_ticks))
            .collect()
    }
}

impl CombatDiagnosticRecord {
    fn valid(&self) -> bool {
        let before = &self.actor_before;
        let after = &self.actor_after;
        let target_before = &self.target_before;
        let target_after = &self.target_after;
        if self.actor.0 == 0
            || self.target.0 == 0
            || self.actor == self.target
            || self.sequence == 0
            || [self.intention, self.origin_intention]
                .into_iter()
                .flatten()
                .any(|id| id == 0 || id == u64::MAX)
            || self.remaining > self.preparation
            || self
                .started
                .checked_add(self.remaining)
                .is_none_or(|due| due > self.tick)
            || self.tick.checked_add(self.recovery).is_none()
            || !before.valid()
            || !after.valid()
            || !target_before.valid()
            || !target_after.valid()
            || before.dead
            || target_before.dead
            || before.injury.is_none()
            || before.health != after.health
            || before.injury != after.injury
            || before.dead != after.dead
            || before.maximum_health != after.maximum_health
            || before.fear != after.fear
            || target_before.maximum_health != target_after.maximum_health
            || target_before.health.checked_sub(target_after.health) != Some(self.damage)
            || target_before.resources.len() != target_after.resources.len()
            || before.resources.len() != after.resources.len()
        {
            return false;
        }
        if let Some(injury) = target_before.injury {
            if injury.checked_add(self.damage) != target_after.injury {
                return false;
            }
        } else if target_after.injury.is_some() {
            return false;
        }
        let charge = match self.ability {
            Technique::BasicMelee => None,
            Technique::PowerStrike => Some(Resource::Stamina),
            Technique::MagicBolt => Some(Resource::Mana),
            Technique::Fear => Some(Resource::Focus),
        };
        match (charge, self.charge) {
            (None, None) => {}
            (Some(resource), Some(cost))
                if cost.resource == resource
                    && cost.start == 1
                    && cost.resolution == 1
                    && self.intention.is_some()
                    && self.origin_intention.is_some() => {}
            _ => return false,
        }
        for (before, after) in before.resources.iter().zip(&after.resources) {
            let spent = u32::from(charge == Some(before.resource));
            if before.resource != after.resource
                || before.maximum != after.maximum
                || before.balance.checked_sub(spent) != Some(after.balance)
                || before.available != after.available
                || before.recovery_elapsed != after.recovery_elapsed
                || before.balance - before.available < spent
            {
                return false;
            }
        }
        for (before, after) in target_before.resources.iter().zip(&target_after.resources) {
            if before.resource != after.resource
                || before.maximum != after.maximum
                || before.balance != after.balance
                || before.recovery_elapsed != after.recovery_elapsed
                || after.available
                    != if target_after.dead {
                        after.balance
                    } else {
                        before.available
                    }
            {
                return false;
            }
        }
        let result = match self.trace.validated_result() {
            Ok(Some(result)) => result,
            _ => return false,
        };
        if result.0 != self.applied || result.1.min(target_before.health) != self.damage {
            return false;
        }
        let mut expected_fear = target_before.fear_map();
        match (self.ability, self.trace.steps.first()) {
            (Technique::Fear, Some(CombatTraceStep::FearStarted { .. })) => {
                if self.applied {
                    let duration = expected_fear.entry(self.actor).or_default();
                    *duration = (*duration).max(result.2);
                }
            }
            (
                Technique::BasicMelee | Technique::PowerStrike | Technique::MagicBolt,
                Some(CombatTraceStep::AttackStarted { .. }),
            ) => {
                let Some(CombatTraceStep::Check { check }) = self.trace.steps.get(1) else {
                    return false;
                };
                if self.ability == Technique::MagicBolt {
                    if check.skill != Skill::Spellcasting {
                        return false;
                    }
                } else if !matches!(check.skill, Skill::HeavyWeaponry | Skill::LightWeaponry) {
                    return false;
                }
            }
            _ => return false,
        }
        if target_after.dead {
            expected_fear.clear();
        }
        expected_fear == target_after.fear_map()
    }
}

impl CombatDiagnosticsView {
    pub fn validate(&self) -> Result<(), InvalidCombatDiagnostics> {
        let valid = self.retained <= MAX_RETAINED_COMBAT_RECORDS
            && self.captured.checked_sub(u64::from(self.retained)) == Some(self.dropped)
            && (self.dropped == 0 || self.retained == MAX_RETAINED_COMBAT_RECORDS)
            && self.through >= self.dropped
            && self.through <= self.captured
            && self.records.len()
                == (self.through - self.dropped).min(MAX_COMBAT_REPORT_RECORDS as u64) as usize
            && (self.enabled || self.captured == 0)
            && self.records.iter().enumerate().all(|(index, record)| {
                record.sequence == self.through - self.records.len() as u64 + index as u64 + 1
                    && record.tick <= self.tick
                    && (index == 0 || self.records[index - 1].tick <= record.tick)
                    && record.valid()
            });
        if valid
            && matches!(
                crate::codec::encoded_length(self, crate::MAX_RESPONSE_BYTES),
                Ok(Some(_))
            )
        {
            Ok(())
        } else {
            Err(InvalidCombatDiagnostics)
        }
    }
}
