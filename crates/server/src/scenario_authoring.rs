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

use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackSpec {
    pub bonus: i32,
    pub wind_up: u64,
    pub recovery: u64,
    pub damage: BTreeMap<DamageType, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CombatSpec {
    pub name: String,
    pub max_hp: u32,
    pub defense: i32,
    pub attack: AttackSpec,
    pub immunities: BTreeSet<DamageType>,
    pub reductions: BTreeMap<DamageType, u32>,
    pub faction: String,
}

impl Default for CombatSpec {
    fn default() -> Self {
        Self {
            name: "figure".into(),
            max_hp: 30,
            defense: 10,
            attack: AttackSpec {
                bonus: 2,
                wind_up: 60,
                recovery: 40,
                damage: BTreeMap::from([(DamageType::Impact, 4)]),
            },
            immunities: BTreeSet::new(),
            reductions: BTreeMap::new(),
            faction: "neutral".into(),
        }
    }
}

impl From<CombatSpec> for tor_simulation::combat::CombatSpec {
    fn from(value: CombatSpec) -> Self {
        Self {
            name: value.name,
            max_hp: value.max_hp,
            defense: value.defense,
            attack: tor_simulation::combat::AttackSpec {
                bonus: value.attack.bonus,
                wind_up: value.attack.wind_up,
                recovery: value.attack.recovery,
                damage: value
                    .attack
                    .damage
                    .into_iter()
                    .map(|(k, v)| (k.into(), v))
                    .collect(),
            },
            immunities: value.immunities.into_iter().map(Into::into).collect(),
            reductions: value
                .reductions
                .into_iter()
                .map(|(k, v)| (k.into(), v))
                .collect(),
            faction: value.faction,
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
    pub attack: Option<AttackSpec>,
    #[serde(default)]
    pub defense: i32,
    #[serde(default)]
    pub reductions: BTreeMap<DamageType, u32>,
}
impl From<EquipmentSpec> for tor_simulation::EquipmentSpec {
    fn from(value: EquipmentSpec) -> Self {
        Self {
            slot: value.slot.into(),
            attack: value
                .attack
                .map(|attack| tor_simulation::combat::AttackSpec {
                    bonus: attack.bonus,
                    wind_up: attack.wind_up,
                    recovery: attack.recovery,
                    damage: attack
                        .damage
                        .into_iter()
                        .map(|(kind, amount)| (kind.into(), amount))
                        .collect(),
                }),
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
