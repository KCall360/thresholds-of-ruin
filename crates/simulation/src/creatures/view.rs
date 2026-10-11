//! Personal inspection projection. No seeds, reservation identities, private
//! grant provenance or other actors' records cross this boundary.

use super::CreatureState;
use crate::attributes::{Attributes, Defenses, ManaBinding, SkillRanks};
use crate::content_ids::{AbilityId, SubtypeId, TalentId, TypeId};
use crate::progression::HdSource;
use crate::resources::Resource;
use crate::rules::Ruleset;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnStats {
    pub rules: Arc<Ruleset>,
    pub kind: TypeId,
    pub subtypes: BTreeSet<SubtypeId>,
    /// Advancement order, without the private per-die random seeds.
    pub hit_dice: Vec<HdSource>,
    pub attributes: Attributes,
    pub skills: SkillRanks,
    pub defenses: Defenses,
    pub binding: ManaBinding,
    pub resources: Vec<ResourceView>,
    pub active_talents: BTreeSet<TalentId>,
    pub dormant_talents: BTreeSet<TalentId>,
    /// Granted techniques; target validity and affordability are checked when
    /// invoking them, rather than promised by an inspection snapshot.
    pub abilities: BTreeSet<AbilityId>,
    pub ability_reaches: BTreeMap<AbilityId, crate::rules::Reach>,
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
            hit_dice: self
                .build()
                .ledger()
                .entries()
                .iter()
                .map(|hd| hd.source())
                .collect(),
            attributes: derived.attributes,
            skills: derived.skills,
            defenses: derived.defenses,
            binding: self.build().binding(),
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
            dormant_talents: derived.dormant_talents.clone(),
            abilities: derived.abilities.clone(),
            ability_reaches: derived.ability_reaches.clone(),
        }
    }
}
