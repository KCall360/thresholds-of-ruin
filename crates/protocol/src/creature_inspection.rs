//! Private source inspection, using the same generic rows as rule diagnostics.
use crate::{
    AbilityStrikesView, ActorId, AttributeView, CombatTraceStep, ContentRef, NaturalAttackView,
    OwnStats, SelectedMeleeView,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionSource {
    pub owner: Option<ActorId>,
    pub category: String,
    pub source: ContentRef,
    pub rows: Vec<CombatTraceStep>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionHitDie {
    pub ordinal: u16,
    pub source: ContentRef,
    #[serde(with = "crate::integers::unsigned")]
    pub health_seed: u64,
    pub health_die: u16,
    pub base_health: u32,
    pub point_pools: Vec<InspectionPointPool>,
    pub purchases: Vec<InspectionPurchase>,
    pub attribute: Option<ContentRef>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancementKind {
    SkillRank,
    Talent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionPointPool {
    pub pool: ContentRef,
    pub granted: u16,
    pub spent: u16,
    pub allowed: Vec<AdvancementKind>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionPurchase {
    pub pool: ContentRef,
    pub selection: InspectionPurchaseSelection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectionPurchaseSelection {
    SkillRank { skill: ContentRef },
    Talent { talent: ContentRef },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionRestoration {
    ChooseAgain,
    Authored,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionDormantTalent {
    pub talent: ContentRef,
    pub reasons: Vec<String>,
}
impl InspectionHitDie {
    fn valid_points(&self) -> bool {
        let mut pools = BTreeSet::new();
        if self.point_pools.len() > 16 || self.purchases.len() > 256 {
            return false;
        }
        if !self.point_pools.iter().all(|balance| {
            balance.pool.valid()
                && pools.insert(&balance.pool.id)
                && balance.granted > 0
                && balance.spent <= balance.granted
                && !balance.allowed.is_empty()
                && balance.allowed.len() <= 2
                && balance.allowed.iter().collect::<BTreeSet<_>>().len() == balance.allowed.len()
                && self
                    .purchases
                    .iter()
                    .filter(|purchase| purchase.pool.id == balance.pool.id)
                    .count()
                    == usize::from(balance.spent)
        }) || self
            .point_pools
            .iter()
            .map(|balance| u32::from(balance.granted))
            .sum::<u32>()
            > 256
        {
            return false;
        }
        self.purchases.iter().all(|purchase| {
            let (kind, value) = match &purchase.selection {
                InspectionPurchaseSelection::SkillRank { skill } => {
                    (AdvancementKind::SkillRank, skill)
                }
                InspectionPurchaseSelection::Talent { talent } => (AdvancementKind::Talent, talent),
            };
            value.valid()
                && self
                    .point_pools
                    .iter()
                    .any(|balance| balance.pool == purchase.pool && balance.allowed.contains(&kind))
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionView {
    pub condition: ContentRef,
    pub source: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub remaining_ticks: u64,
}
impl ConditionView {
    pub(crate) fn valid(&self) -> bool {
        self.condition.valid()
            && self.source.0 != 0
            && (1..=1_000_000).contains(&self.remaining_ticks)
    }
}
/// Authoritative selected weapon operands, plus generic cached ability timing/modifier rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedAttackView {
    pub category: String,
    pub source: ContentRef,
    pub socket: Option<u16>,
    pub attack: SelectedMeleeView,
    pub attribute: ContentRef,
    pub attribute_value: u16,
    pub rank: u8,
    #[serde(with = "crate::integers::signed")]
    pub check_bonus: i64,
    pub abilities: Vec<CombatTraceStep>,
}
impl SelectedAttackView {
    fn valid(&self) -> bool {
        crate::valid_content_id(&self.category)
            && self.source.valid()
            && self
                .socket
                .is_none_or(|socket| usize::from(socket) < crate::MAX_EQUIPMENT_SLOTS)
            && self.attack.valid()
            && self.attribute.valid()
            && self.attribute_value <= 1000
            && self.rank <= 5
            && (-1_000_000..=1_000_000).contains(&self.check_bonus)
            && self.abilities.len() <= crate::MAX_CONTENT_REFERENCES
            && self.abilities.iter().all(CombatTraceStep::valid)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureInspectionView {
    pub actor: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub name: String,
    pub faction: String,
    pub species: ContentRef,
    pub starting_skills: [ContentRef; 2],
    pub restoration: InspectionRestoration,
    pub dormant_talents: Vec<InspectionDormantTalent>,
    pub natural_attack: NaturalAttackView,
    pub ability_strikes: Vec<AbilityStrikesView>,
    pub selected_attack: SelectedAttackView,
    pub initial_attributes: AttributeView,
    pub templates: Vec<ContentRef>,
    pub max_hp: u32,
    pub hp: u32,
    pub injury: u32,
    pub dead: bool,
    pub stats: OwnStats,
    pub hit_dice: Vec<InspectionHitDie>,
    pub sources: Vec<InspectionSource>,
    pub conditions: Vec<ConditionView>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCreatureInspection;
impl std::fmt::Display for InvalidCreatureInspection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid privileged creature inspection")
    }
}
impl std::error::Error for InvalidCreatureInspection {}
impl CreatureInspectionView {
    /// Count structural grants without assuming a particular catalog of skills.
    /// The caller has already validated the bounded, unique disclosed rows.
    fn valid_skill_ownership(&self) -> bool {
        let mut owned = self
            .stats
            .skills
            .iter()
            .map(|row| (row.skill.id.as_str(), (&row.skill, 0_u8)))
            .collect::<BTreeMap<_, _>>();
        let paid = self
            .hit_dice
            .iter()
            .flat_map(|die| &die.purchases)
            .filter_map(|purchase| match &purchase.selection {
                InspectionPurchaseSelection::SkillRank { skill } => Some(skill),
                InspectionPurchaseSelection::Talent { .. } => None,
            });
        for skill in self.starting_skills.iter().chain(paid) {
            let Some((disclosed, rank)) = owned.get_mut(skill.id.as_str()) else {
                return false;
            };
            if *disclosed != skill || *rank == 5 {
                return false;
            }
            *rank += 1;
        }
        self.stats
            .skills
            .iter()
            .all(|row| owned[row.skill.id.as_str()].1 == row.rank)
    }
    /// Ownership consistency is structural; clients never evaluate eligibility.
    fn valid_talent_ownership(&self) -> bool {
        let mut purchases = BTreeMap::new();
        for purchase in self.hit_dice.iter().flat_map(|die| &die.purchases) {
            if let InspectionPurchaseSelection::Talent { talent } = &purchase.selection {
                if purchases.len() >= crate::MAX_OWNED_TALENTS
                    || purchases.insert(talent.id.as_str(), talent).is_some()
                {
                    return false;
                }
            }
        }
        purchases.len() == self.stats.active_talents.len() + self.stats.dormant_talents.len()
            && self
                .stats
                .active_talents
                .iter()
                .chain(&self.stats.dormant_talents)
                .all(|talent| purchases.get(talent.id.as_str()) == Some(&talent))
    }
    pub fn validate(&self) -> Result<(), InvalidCreatureInspection> {
        let mut templates = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let valid = self.actor.0 != 0
            && crate::valid_label(&self.name, 60)
            && crate::valid_label(&self.faction, 80)
            && self.species.valid()
            && self.starting_skills.iter().all(ContentRef::valid)
            && self.starting_skills[0].id != self.starting_skills[1].id
            && self.dormant_talents.len() == self.stats.dormant_talents.len()
            && self
                .dormant_talents
                .iter()
                .zip(&self.stats.dormant_talents)
                .all(|(entry, talent)| {
                    entry.talent == *talent
                        && !entry.reasons.is_empty()
                        && entry.reasons.len() <= 129
                        && entry
                            .reasons
                            .iter()
                            .all(|reason| crate::valid_label(reason, 65536))
                })
            && self.natural_attack.valid()
            && self.natural_attack.source == self.species
            && self.ability_strikes.len() <= crate::MAX_CONTENT_REFERENCES
            && self.ability_strikes.len() == self.stats.abilities.len()
            && self
                .ability_strikes
                .iter()
                .zip(&self.stats.abilities)
                .all(|(source, ability)| source.valid() && source.ability == ability.id)
            && self.selected_attack.valid()
            && self.stats.valid()
            && (!(self.dead || self.injury == 0) || self.stats.health_recovery.elapsed_ticks == 0)
            && self.templates.len() <= 32
            && self
                .templates
                .iter()
                .all(|value| value.valid() && templates.insert(&value.id))
            && self.max_hp <= 1_000_000
            && self.injury <= 1_000_000
            && self.hp <= self.max_hp
            && if self.dead {
                self.hp == 0
            } else {
                self.hp > 0 && self.max_hp.checked_sub(self.injury) == Some(self.hp)
            }
            && self.hit_dice.len() == self.stats.hit_dice.len()
            && self.hit_dice.iter().enumerate().all(|(index, die)| {
                die.ordinal as usize == index + 1
                    && die.source == self.stats.hit_dice[index]
                    && (2..=1000).contains(&die.health_die)
                    && (1..=u32::from(die.health_die)).contains(&die.base_health)
                    && die.valid_points()
                    && die.attribute.as_ref().is_none_or(ContentRef::valid)
            })
            && self.valid_skill_ownership()
            && self.valid_talent_ownership()
            && self.sources.len() <= 4096
            && self.sources.iter().all(|source| {
                crate::valid_content_id(&source.category)
                    && source.source.valid()
                    && sources.insert((&source.category, &source.source.id, source.owner))
                    && source.owner.is_none_or(|owner| owner.0 != 0)
                    && source.rows.len() <= 160
                    && source.rows.iter().all(CombatTraceStep::valid)
            })
            && self.conditions.len() <= 128
            && self.conditions.iter().all(ConditionView::valid)
            && (!self.dead || self.conditions.is_empty());
        if valid
            && matches!(
                crate::codec::encoded_length(self, crate::MAX_RESPONSE_BYTES),
                Ok(Some(_))
            )
        {
            Ok(())
        } else {
            Err(InvalidCreatureInspection)
        }
    }
}
