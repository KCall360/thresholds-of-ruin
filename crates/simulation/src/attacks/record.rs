//! Source definitions validate again when restored, never trusting a cache.
use super::{AttackError, MeleeAttack};
use crate::attributes::Skill;
use crate::damage::record::DamageRecord;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeAttackRecord {
    skill: Skill,
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    damage: DamageRecord,
}

impl MeleeAttackRecord {
    pub fn capture(attack: &MeleeAttack) -> Self {
        Self {
            skill: attack.skill,
            bonus: attack.bonus,
            wind_up: attack.wind_up,
            recovery: attack.recovery,
            damage: DamageRecord::capture(&attack.damage),
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
        )
    }
}
