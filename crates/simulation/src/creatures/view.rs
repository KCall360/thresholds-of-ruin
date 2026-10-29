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

/// Exact owner disclosure with shared effective-sheet collections. The backing
/// sheet is private: no raw rules inputs or hidden derived fields are exposed.
/// This is derived, unsaved data; equality is only the legacy OwnStats projection.
#[derive(Clone)]
pub struct OwnStatsFacts {
    rules: Arc<Ruleset>,
    derived: tor_world::Shared<super::DerivedCreature>,
    dynamic: OwnStatsDynamic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnStatsDynamic {
    restorable_losses: super::RestorableLosses,
    restorable_hit_dice: crate::HdCount,
    resources: Vec<ResourceView>,
    health_recovery: crate::health_recovery::HealthRecovery,
    nutrition: crate::nutrition::NutritionView,
    conditions: Vec<PersonalCondition>,
    ability_cooldowns: BTreeMap<AbilityId, u64>,
}

impl std::fmt::Debug for OwnStatsFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnStatsFacts").finish_non_exhaustive()
    }
}

impl PartialEq for OwnStatsFacts {
    fn eq(&self, other: &Self) -> bool {
        let a = &*self.derived;
        let b = &*other.derived;
        self.rules == other.rules
            && self.dynamic == other.dynamic
            && a.kind == b.kind
            && a.subtypes == b.subtypes
            && a.hit_dice == b.hit_dice
            && a.attributes == b.attributes
            && a.skills == b.skills
            && a.defenses == b.defenses
            && a.binding() == b.binding()
            && a.health_recovery_reset_percent == b.health_recovery_reset_percent
            && a.active_talents == b.active_talents
            && a.expertises == b.expertises
            && a.dormant_talents == b.dormant_talents
            && a.abilities == b.abilities
            && a.weapon_abilities == b.weapon_abilities
            && a.ability_reaches == b.ability_reaches
    }
}
impl Eq for OwnStatsFacts {}

impl OwnStatsFacts {
    /// Read the rules already disclosed by the owned personal view.
    pub fn rules(&self) -> &Arc<Ruleset> {
        &self.rules
    }

    /// Read compiled disclosed ability IDs without materializing personal statistics.
    pub fn abilities(&self) -> &BTreeSet<AbilityId> {
        &self.derived.abilities
    }

    /// Read the captured personal nutrition state without materializing collections.
    pub fn nutrition(&self) -> crate::nutrition::NutritionView {
        self.dynamic.nutrition
    }

    /// Return an independent owned value without consulting live creature state.
    pub fn materialize(&self) -> OwnStats {
        self.clone().into_owned()
    }

    /// Consume fresh facts without copying their already-owned dynamic rows.
    pub fn into_owned(self) -> OwnStats {
        let derived = &*self.derived;
        // Count actual copied collection entries, including both expertise-map
        // levels. Shared hit-die handles and scalar/string fields are excluded.
        let entries = derived.subtypes.len()
            + derived.active_talents.len()
            + derived.dormant_talents.len()
            + derived.abilities.len()
            + derived.weapon_abilities.len()
            + derived.ability_reaches.len()
            + derived.expertises.grants().len()
            + derived
                .expertises
                .grants()
                .values()
                .map(BTreeMap::len)
                .sum::<usize>();
        crate::diagnostics::own_stats_materialization(entries);
        OwnStats {
            rules: self.rules,
            kind: derived.kind.clone(),
            subtypes: derived.subtypes.clone(),
            hit_dice: derived.hit_dice.clone(),
            attributes: derived.attributes,
            restorable_losses: self.dynamic.restorable_losses,
            restorable_hit_dice: self.dynamic.restorable_hit_dice,
            skills: derived.skills,
            defenses: derived.defenses,
            binding: derived.binding(),
            resources: self.dynamic.resources,
            health_recovery: self.dynamic.health_recovery,
            nutrition: self.dynamic.nutrition,
            conditions: self.dynamic.conditions,
            health_recovery_reset_percent: derived.health_recovery_reset_percent,
            active_talents: derived.active_talents.clone(),
            expertises: derived.expertises.clone(),
            dormant_talents: derived.dormant_talents.clone(),
            abilities: derived.abilities.clone(),
            weapon_abilities: derived.weapon_abilities.clone(),
            ability_cooldowns: self.dynamic.ability_cooldowns,
            ability_reaches: derived.ability_reaches.clone(),
        }
    }
}

impl CreatureState {
    pub(crate) fn own_stats(&self) -> OwnStats {
        self.own_stats_facts().into_owned()
    }

    pub(crate) fn own_stats_facts(&self) -> OwnStatsFacts {
        let derived = self.derived_snapshot();
        let costs = self.costs();
        let dynamic = OwnStatsDynamic {
            restorable_losses: self.restorable_losses(),
            restorable_hit_dice: self.restorable_hit_dice(),
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
            ability_cooldowns: derived
                .abilities
                .iter()
                .map(|id| (id.clone(), self.cooldowns().remaining(id)))
                .collect(),
        };
        OwnStatsFacts {
            rules: self.build().content().rules.clone(),
            derived,
            dynamic,
        }
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
