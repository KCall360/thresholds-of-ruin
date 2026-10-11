//! Private source inspection, using the same generic rows as rule diagnostics.
use crate::{ActorId, AttackView, AttributeView, CombatTraceStep, ContentRef, OwnStats};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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
    pub training: Vec<ContentRef>,
    pub attribute: Option<ContentRef>,
    pub talent: Option<ContentRef>,
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
    pub attack: AttackView,
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
    pub tick: u64,
    pub name: String,
    pub faction: String,
    pub species: ContentRef,
    pub natural_attack: AttackView,
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
    pub fn validate(&self) -> Result<(), InvalidCreatureInspection> {
        let mut templates = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let valid = self.actor.0 != 0
            && crate::valid_label(&self.name, 60)
            && crate::valid_label(&self.faction, 80)
            && self.species.valid()
            && self.natural_attack.valid()
            && self.selected_attack.valid()
            && self.stats.valid()
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
                    && die.training.len() <= 256
                    && die.training.iter().all(ContentRef::valid)
                    && die.attribute.as_ref().is_none_or(ContentRef::valid)
                    && die.talent.as_ref().is_none_or(ContentRef::valid)
            })
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
