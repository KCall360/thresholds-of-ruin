//! Author-owned declaration schemas. Conversions into simulation definitions are
//! explicit so runtime representation changes cannot silently alter package files.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    #[default]
    Misc,
    Weapon,
    Armor,
    Potion,
    Food,
    Corpse,
    Tool,
    Amulet,
    Ring,
    Scroll,
    Spellbook,
    Wand,
    Coin,
    Gem,
}

impl From<ItemClass> for tor_simulation::ItemClass {
    fn from(value: ItemClass) -> Self {
        match value {
            ItemClass::Misc => Self::Misc,
            ItemClass::Weapon => Self::Weapon,
            ItemClass::Armor => Self::Armor,
            ItemClass::Potion => Self::Potion,
            ItemClass::Food => Self::Food,
            ItemClass::Corpse => Self::Corpse,
            ItemClass::Tool => Self::Tool,
            ItemClass::Amulet => Self::Amulet,
            ItemClass::Ring => Self::Ring,
            ItemClass::Scroll => Self::Scroll,
            ItemClass::Spellbook => Self::Spellbook,
            ItemClass::Wand => Self::Wand,
            ItemClass::Coin => Self::Coin,
            ItemClass::Gem => Self::Gem,
        }
    }
}

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
}

impl From<DamageType> for tor_simulation::combat::DamageType {
    fn from(value: DamageType) -> Self {
        match value {
            DamageType::Energy => Self::Energy,
            DamageType::Impact => Self::Impact,
            DamageType::Keen => Self::Keen,
            DamageType::Spirit => Self::Spirit,
            DamageType::Vital => Self::Vital,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentSlot {
    Weapon,
    BodyArmor,
    Shield,
    HeadArmor,
    HandsArmor,
    FeetArmor,
    Cloak,
    Ring,
    Amulet,
}
impl From<EquipmentSlot> for tor_simulation::EquipmentSlot {
    fn from(value: EquipmentSlot) -> Self {
        match value {
            EquipmentSlot::Weapon => Self::Weapon,
            EquipmentSlot::BodyArmor => Self::BodyArmor,
            EquipmentSlot::Shield => Self::Shield,
            EquipmentSlot::HeadArmor => Self::HeadArmor,
            EquipmentSlot::HandsArmor => Self::HandsArmor,
            EquipmentSlot::FeetArmor => Self::FeetArmor,
            EquipmentSlot::Cloak => Self::Cloak,
            EquipmentSlot::Ring => Self::Ring,
            EquipmentSlot::Amulet => Self::Amulet,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
}
impl From<AnatomySpec> for tor_simulation::AnatomySpec {
    fn from(value: AnatomySpec) -> Self {
        Self {
            slots: value.slots.into_iter().map(Into::into).collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSpec {
    pub slot: EquipmentSlot,
    pub attack: Option<tor_simulation::attacks::MeleeAttack>,
    #[serde(default)]
    pub defense: i32,
    #[serde(default)]
    pub reductions: BTreeMap<DamageType, u32>,
}
impl From<EquipmentSpec> for tor_simulation::EquipmentSpec {
    fn from(value: EquipmentSpec) -> Self {
        Self {
            slot: value.slot.into(),
            attack: value.attack,
            defense: value.defense,
            reductions: value
                .reductions
                .into_iter()
                .map(|(kind, amount)| (kind.into(), amount))
                .collect(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectSpec {
    Heal {
        amount: u32,
    },
    Damage {
        components: BTreeMap<DamageType, u32>,
    },
}
impl From<EffectSpec> for tor_simulation::EffectSpec {
    fn from(value: EffectSpec) -> Self {
        match value {
            EffectSpec::Heal { amount } => Self::Heal { amount },
            EffectSpec::Damage { components } => Self::Damage {
                components: components
                    .into_iter()
                    .map(|(kind, amount)| (kind.into(), amount))
                    .collect(),
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<EffectSpec>,
}
impl From<ConsumableSpec> for tor_simulation::ConsumableSpec {
    fn from(value: ConsumableSpec) -> Self {
        Self {
            effects: value.effects.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodySpec {
    pub cells: Vec<[i32; 3]>,
    pub eye: [i32; 3],
    pub mass: u32,
}

impl From<BodySpec> for tor_simulation::BodySpec {
    fn from(value: BodySpec) -> Self {
        Self {
            cells: value.cells,
            eye: value.eye,
            mass: value.mass,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiProfile {
    pub memory_ticks: u64,
    pub flee_percent: u32,
}

impl Default for AiProfile {
    fn default() -> Self {
        Self {
            memory_ticks: 1000,
            flee_percent: 25,
        }
    }
}

impl From<AiProfile> for tor_simulation::ai::AiProfile {
    fn from(value: AiProfile) -> Self {
        Self {
            memory_ticks: value.memory_ticks,
            flee_percent: value.flee_percent,
        }
    }
}
