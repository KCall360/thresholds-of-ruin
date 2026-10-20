//! Personal inspection projection. No seeds, reservation identities, private
//! grant provenance or other actors' records cross this boundary.

use super::CreatureState;
use crate::attributes::{Attributes, Defenses, ManaBinding, SkillRanks};
use crate::content_ids::{AbilityId, SubtypeId, TalentId, TypeId};
use crate::resources::Resource;
use crate::rules::Ruleset;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnStats {
    pub rules: Arc<Ruleset>,
    pub kind: TypeId,
    pub subtypes: BTreeSet<SubtypeId>,
    /// Cached source counts only; ordered records belong to explicit inspection.
    pub hit_dice: tor_world::Shared<super::HitDiceSummary>,
    pub attributes: Attributes,
    pub restorable_losses: super::RestorableLosses,
    pub restorable_hit_dice: crate::HdCount,
    pub skills: SkillRanks,
    pub defenses: Defenses,
    pub binding: ManaBinding,
    pub resources: Vec<ResourceView>,
    pub health_recovery: crate::health_recovery::HealthRecovery,
    pub nutrition: crate::nutrition::NutritionView,
    pub conditions: Vec<PersonalCondition>,
    pub health_recovery_reset_percent: u8,
    pub active_talents: BTreeSet<TalentId>,
    pub expertises: crate::expertise::ExpertiseOwnership,
    pub dormant_talents: BTreeSet<TalentId>,
    /// Granted techniques; target validity and affordability are checked when
    /// invoking them, rather than promised by an inspection snapshot.
    pub abilities: BTreeSet<AbilityId>,
    pub weapon_abilities: BTreeSet<AbilityId>,
    pub ability_cooldowns: BTreeMap<AbilityId, u64>,
    pub ability_reaches: BTreeMap<AbilityId, crate::rules::Reach>,
}

/// Personal condition status facts omit causers and private preparation state.
/// One-use tokens retain their separate authorized disclosure contracts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonalCondition {
    pub condition: crate::content_ids::ConditionId,
    pub phase: crate::conditions::ConditionPhase,
    pub remaining_ticks: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceView {
    pub resource: Resource,
    pub balance: u32,
    pub maximum: u32,
    pub available: u32,
    pub reserved: u32,
}

impl CreatureState {
    pub(crate) fn own_stats(&self) -> OwnStats {
        let derived = self.derived();
        let costs = self.costs();
        OwnStats {
            rules: self.build().content().rules.clone(),
            kind: derived.kind.clone(),
            subtypes: derived.subtypes.clone(),
            hit_dice: derived.hit_dice.clone(),
            attributes: derived.attributes,
            restorable_losses: self.restorable_losses(),
            restorable_hit_dice: self.restorable_hit_dice(),
            skills: derived.skills,
            defenses: derived.defenses,
            binding: derived.binding(),
            health_recovery: self.health_recovery(),
            nutrition: self
                .nutrition()
                .view(self.build().ruleset().document().nutrition),
            conditions: self
                .conditions()
                .instances()
                .filter(|entry| {
                    self.build()
                        .ruleset()
                        .condition(&entry.condition)
                        .is_some_and(|definition| {
                            definition.lifecycle == crate::rules::ConditionLifecycle::Timed
                        })
                })
                .map(|entry| PersonalCondition {
                    condition: entry.condition.clone(),
                    phase: entry.phase,
                    remaining_ticks: entry.remaining,
                })
                .collect(),
            health_recovery_reset_percent: derived.health_recovery_reset_percent,
            resources: Resource::ALL
                .into_iter()
                .map(|resource| {
                    let balance = costs.resources().balance(resource);
                    let available = costs.available(resource);
                    ResourceView {
                        resource,
                        balance,
                        maximum: costs.resources().maximum(resource),
                        available,
                        reserved: balance - available,
                    }
                })
                .collect(),
            active_talents: derived.active_talents.clone(),
            expertises: derived.expertises.clone(),
            dormant_talents: derived.dormant_talents.clone(),
            abilities: derived.abilities.clone(),
            weapon_abilities: derived.weapon_abilities.clone(),
            ability_cooldowns: derived
                .abilities
                .iter()
                .map(|id| (id.clone(), self.cooldowns().remaining(id)))
                .collect(),
            ability_reaches: derived.ability_reaches.clone(),
        }
    }
}
