//! Privileged creature reports. These are never ordinary actor observations or
//! saved creature records; the server separately authorizes their recipients.
use crate::{
    ActorId, AttackView, AttributeView, CreatureSubtype, CreatureType, DamageType, HitDieSource,
    OwnStats, Skill, Talent, Technique,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionAttribute {
    Strength,
    Speed,
    Intellect,
    Willpower,
    Awareness,
    Presence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionClass {
    Warrior,
    Mage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionDescriptor {
    Fire,
    Cold,
    Fear,
    MindAffecting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectionSelector {
    Category { category: DamageType },
    Descriptor { descriptor: InspectionDescriptor },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectionGrant {
    Health {
        amount: u32,
    },
    Stamina {
        amount: u32,
    },
    Focus {
        amount: u32,
    },
    Mana {
        amount: u32,
    },
    PhysicalDefense {
        modifier: i32,
    },
    MeleeFlat {
        modifier: i32,
    },
    MeleeDice {
        count: u16,
    },
    BoltFlat {
        modifier: i32,
    },
    BoltDice {
        count: u16,
    },
    FearDifficulty {
        modifier: i32,
    },
    FearDuration {
        #[serde(with = "crate::integers::unsigned")]
        ticks: u64,
    },
    Immunity {
        selector: InspectionSelector,
    },
    Reduction {
        selector: InspectionSelector,
        amount: u32,
    },
    Ability {
        ability: Technique,
    },
    Mindless,
    Magical,
}

impl InspectionGrant {
    fn valid(&self) -> bool {
        match *self {
            Self::Health { amount }
            | Self::Stamina { amount }
            | Self::Focus { amount }
            | Self::Mana { amount }
            | Self::Reduction { amount, .. } => amount <= 1_000_000,
            Self::PhysicalDefense { modifier }
            | Self::MeleeFlat { modifier }
            | Self::BoltFlat { modifier }
            | Self::FearDifficulty { modifier } => (-1_000..=1_000).contains(&modifier),
            Self::MeleeDice { count } | Self::BoltDice { count } => count <= 63,
            Self::FearDuration { ticks } => ticks <= 1_000_000,
            Self::Immunity { .. } | Self::Ability { .. } | Self::Mindless | Self::Magical => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectionGrantSource {
    Species { id: String },
    Type { kind: CreatureType },
    Subtype { subtype: CreatureSubtype },
    Class { class: InspectionClass },
    Template { id: String },
    Talent { talent: Talent },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionGrantGroup {
    pub source: InspectionGrantSource,
    pub grants: Vec<InspectionGrant>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionSpecies {
    pub id: String,
    pub kind: CreatureType,
    pub subtypes: Vec<CreatureSubtype>,
    pub attributes: AttributeView,
    pub melee: AttackView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionAttributeOverride {
    pub attribute: InspectionAttribute,
    pub value: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionTemplate {
    pub id: String,
    pub priority: i16,
    pub kind: Option<CreatureType>,
    pub add_subtypes: Vec<CreatureSubtype>,
    pub remove_subtypes: Vec<CreatureSubtype>,
    /// Strength, Speed, Intellect, Willpower, Awareness and Presence, in order.
    pub adjustments: [i16; 6],
    pub overrides: Vec<InspectionAttributeOverride>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionHitDie {
    pub ordinal: u16,
    pub source: HitDieSource,
    #[serde(with = "crate::integers::unsigned")]
    pub health_seed: u64,
    pub health_die: u16,
    pub base_health: u32,
    pub training: Vec<Skill>,
    pub attribute: Option<InspectionAttribute>,
    pub talent: Option<Talent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionFear {
    pub causer: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub remaining_ticks: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureInspectionView {
    pub actor: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub name: String,
    pub faction: String,
    pub species: InspectionSpecies,
    pub initial_attributes: AttributeView,
    pub templates: Vec<InspectionTemplate>,
    pub max_hp: u32,
    pub hp: u32,
    pub injury: u32,
    pub dead: bool,
    pub stats: OwnStats,
    pub hit_dice: Vec<InspectionHitDie>,
    pub grants: Vec<InspectionGrantGroup>,
    pub fear: Vec<InspectionFear>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCreatureInspection;
impl std::fmt::Display for InvalidCreatureInspection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid privileged creature inspection")
    }
}
impl std::error::Error for InvalidCreatureInspection {}

fn text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
fn id(value: &str) -> bool {
    text(value, 60)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
fn unique<T: Ord>(values: impl IntoIterator<Item = T>) -> bool {
    let mut seen = BTreeSet::new();
    values.into_iter().all(|value| seen.insert(value))
}
fn attributes(value: &AttributeView) -> [u16; 6] {
    [
        value.strength,
        value.speed,
        value.intellect,
        value.willpower,
        value.awareness,
        value.presence,
    ]
}

impl CreatureInspectionView {
    pub fn validate(&self) -> Result<(), InvalidCreatureInspection> {
        let valid = self.valid()
            && matches!(
                crate::codec::encoded_length(self, crate::MAX_RESPONSE_BYTES),
                Ok(Some(_))
            );
        if valid {
            Ok(())
        } else {
            Err(InvalidCreatureInspection)
        }
    }
    fn valid(&self) -> bool {
        if self.actor.0 == 0
            || !text(&self.name, 60)
            || !text(&self.faction, 80)
            || !id(&self.species.id)
            || self.species.subtypes.len() > 20
            || !unique(self.species.subtypes.iter())
            || !self.species.melee.valid()
            || attributes(&self.species.attributes)
                .into_iter()
                .any(|value| value > 5)
            || attributes(&self.initial_attributes)
                .into_iter()
                .any(|value| value > 5)
            || !self.stats.valid()
            || self.hit_dice.len() != self.stats.hit_dice.len()
            || self.max_hp > 1_000_000
            || self.injury > 1_000_000
            || (self.hit_dice.is_empty() != (self.max_hp == 0))
            || if self.dead {
                self.hp != 0
            } else {
                self.injury >= self.max_hp || self.hp != self.max_hp - self.injury
            }
        {
            return false;
        }
        if self.templates.len() > 32
            || !unique(self.templates.iter().map(|value| &value.id))
            || self.templates.iter().any(|value| {
                !id(&value.id)
                    || value.add_subtypes.len() > 20
                    || value.remove_subtypes.len() > 20
                    || !unique(value.add_subtypes.iter())
                    || !unique(value.remove_subtypes.iter())
                    || value
                        .add_subtypes
                        .iter()
                        .any(|subtype| value.remove_subtypes.contains(subtype))
                    || value
                        .adjustments
                        .iter()
                        .any(|value| !(-1_000..=1_000).contains(value))
                    || value.overrides.len() > 6
                    || !unique(value.overrides.iter().map(|value| value.attribute))
                    || value.overrides.iter().any(|value| value.value > 1_000)
            })
        {
            return false;
        }
        let mut training = BTreeMap::<Skill, u16>::new();
        let mut talents = BTreeSet::new();
        let mut ordinary = attributes(&self.initial_attributes);
        for (index, (die, source)) in self.hit_dice.iter().zip(&self.stats.hit_dice).enumerate() {
            let budget = if *source == HitDieSource::Racial {
                1
            } else {
                2
            };
            if usize::from(die.ordinal) != index + 1
                || die.source != *source
                || !(2..=1000).contains(&die.health_die)
                || die.base_health == 0
                || die.base_health > u32::from(die.health_die)
                || die.training.len() > budget
            {
                return false;
            }
            for &skill in &die.training {
                *training.entry(skill).or_default() += 1;
            }
            if let Some(attribute) = die.attribute {
                if !die.ordinal.is_multiple_of(4) {
                    return false;
                }
                ordinary[attribute as usize] += 1;
            }
            if die.talent.is_some_and(|talent| !talents.insert(talent)) {
                return false;
            }
        }
        if ordinary.into_iter().any(|value| value > 5)
            || self.stats.skills.iter().any(|value| {
                training.get(&value.skill).copied().unwrap_or(0) != u16::from(value.rank)
            })
            || talents
                != self
                    .stats
                    .active_talents
                    .iter()
                    .chain(&self.stats.dormant_talents)
                    .copied()
                    .collect()
        {
            return false;
        }
        if self.grants.len() > 83
            || !unique(self.grants.iter().map(|group| &group.source))
            || self.grants.iter().any(|group| {
                group.grants.len() > 128
                    || group.grants.iter().any(|grant| !grant.valid())
                    || !match &group.source {
                        InspectionGrantSource::Species { id } => id == &self.species.id,
                        InspectionGrantSource::Type { kind } => kind == &self.stats.kind,
                        InspectionGrantSource::Subtype { subtype } => {
                            self.stats.subtypes.contains(subtype)
                        }
                        InspectionGrantSource::Class { class } => {
                            self.stats.hit_dice.contains(&match class {
                                InspectionClass::Warrior => HitDieSource::Warrior,
                                InspectionClass::Mage => HitDieSource::Mage,
                            })
                        }
                        InspectionGrantSource::Template { id } => {
                            self.templates.iter().any(|value| &value.id == id)
                        }
                        InspectionGrantSource::Talent { talent } => {
                            self.stats.active_talents.contains(talent)
                        }
                    }
            })
            || self.fear.len() > 128
            || !unique(self.fear.iter().map(|value| value.causer))
            || self.fear.iter().any(|value| {
                value.causer.0 == 0 || !(1..=1_000_000).contains(&value.remaining_ticks)
            })
            || (self.dead && !self.fear.is_empty())
            || (!self.fear.is_empty()
                && self
                    .grants
                    .iter()
                    .flat_map(|group| &group.grants)
                    .any(|grant| {
                        matches!(
                            grant,
                            InspectionGrant::Immunity {
                                selector: InspectionSelector::Descriptor {
                                    descriptor: InspectionDescriptor::Fear
                                        | InspectionDescriptor::MindAffecting
                                }
                            }
                        )
                    }))
        {
            return false;
        }
        true
    }
}
