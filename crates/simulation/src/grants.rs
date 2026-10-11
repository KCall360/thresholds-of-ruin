//! Source-owned benefits shared by species, composition and talent definitions.

use crate::combat::DamageType;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Ability {
    BasicMelee,
    PowerStrike,
    MagicBolt,
    Fear,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Descriptor {
    Fire,
    Cold,
    Fear,
    MindAffecting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Selector {
    Category(DamageType),
    Descriptor(Descriptor),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    Health(u32),
    Stamina(u32),
    Focus(u32),
    Mana(u32),
    PhysicalDefense(i32),
    MeleeFlat(i32),
    MeleeDice(u16),
    BoltFlat(i32),
    BoltDice(u16),
    FearDifficulty(i32),
    FearDuration(u64),
    Immunity(Selector),
    Reduction(Selector, u32),
    Ability(Ability),
    Mindless,
    Magical,
}

impl Grant {
    pub fn valid(self) -> bool {
        match self {
            Self::Health(value)
            | Self::Stamina(value)
            | Self::Focus(value)
            | Self::Mana(value)
            | Self::Reduction(_, value) => value <= 1_000_000,
            Self::PhysicalDefense(value)
            | Self::MeleeFlat(value)
            | Self::BoltFlat(value)
            | Self::FearDifficulty(value) => (-1_000..=1_000).contains(&value),
            Self::MeleeDice(value) | Self::BoltDice(value) => value <= 63,
            Self::FearDuration(value) => value <= 1_000_000,
            Self::Immunity(_) | Self::Ability(_) | Self::Mindless | Self::Magical => true,
        }
    }
}
