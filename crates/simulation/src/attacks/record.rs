//! Source definitions validate again when restored, never trusting a cache.
use super::{AttackError, MeleeAttack};
use crate::attributes::Skill;
use crate::damage::record::DamageRecord;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeAttackRecord {
    skill: Skill,
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    damage: DamageRecord,
    damage_enchantment: i32,
}

impl<'de> Deserialize<'de> for MeleeAttackRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            skill: Skill,
            bonus: i32,
            wind_up: u64,
            recovery: u64,
            damage: DamageRecord,
            damage_enchantment: i32,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let record = Self {
            skill: f.skill,
            bonus: f.bonus,
            wind_up: f.wind_up,
            recovery: f.recovery,
            damage: f.damage,
            damage_enchantment: f.damage_enchantment,
        };
        record
            .restore()
            .map_err(|_| serde::de::Error::custom("invalid melee attack record"))?;
        Ok(record)
    }
}

impl MeleeAttackRecord {
    pub fn capture(attack: &MeleeAttack) -> Self {
        Self {
            skill: attack.skill,
            bonus: attack.bonus,
            wind_up: attack.wind_up,
            recovery: attack.recovery,
            damage: DamageRecord::capture(&attack.damage),
            damage_enchantment: attack.damage_enchantment,
        }
    }

    pub fn restore(&self) -> Result<MeleeAttack, AttackError> {
        MeleeAttack::new(
            self.skill,
            self.bonus,
            self.wind_up,
            self.recovery,
            self.damage
                .restore()
                .map_err(|_| AttackError::InvalidDefinition)?,
        )?
        .with_enchantment(0, self.damage_enchantment)
    }
}

/// Definitions only. Restoration never trusts cached selected-source operands.
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalAttackRecord {
    skill: Skill,
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    strikes: crate::bounded::Bounded<crate::rules::StrikeDef, { crate::limits::STRIKES }>,
}
impl NaturalAttackRecord {
    pub fn capture(attack: &super::NaturalAttack) -> Self {
        Self {
            skill: attack.skill,
            bonus: attack.bonus,
            wind_up: attack.wind_up,
            recovery: attack.recovery,
            strikes: crate::bounded::Bounded(attack.strikes.clone()),
        }
    }
    pub fn restore(&self) -> Result<super::NaturalAttack, AttackError> {
        super::NaturalAttack::new(
            self.skill,
            self.bonus,
            self.wind_up,
            self.recovery,
            self.strikes.0.clone(),
        )
    }
}

impl<'de> Deserialize<'de> for NaturalAttackRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Record {
            skill: Skill,
            bonus: i32,
            wind_up: u64,
            recovery: u64,
            strikes: crate::bounded::Bounded<crate::rules::StrikeDef, { crate::limits::STRIKES }>,
        }
        let record: Record = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            skill: record.skill,
            bonus: record.bonus,
            wind_up: record.wind_up,
            recovery: record.recovery,
            strikes: record.strikes,
        })
    }
}
