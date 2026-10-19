//! Immutable natural/weapon melee definitions use the shared damage kernel.
mod record;
pub use record::{MeleeAttackRecord, NaturalAttackRecord};
mod natural;
pub use natural::{NaturalAttack, SelectedDamageError, SelectedMeleeAttack};

use crate::attributes::Skill;
use crate::damage::{DamageError, DamageSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackError {
    InvalidDefinition,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MeleeAttack {
    skill: Skill,
    bonus: i32,
    wind_up: u64,
    recovery: u64,
    damage: DamageSpec,
    damage_enchantment: i32,
}

impl MeleeAttack {
    /// A single fixed component has an unambiguous primary.
    pub fn fixed(
        skill: Skill,
        bonus: i32,
        wind_up: u64,
        recovery: u64,
        category: crate::combat::DamageType,
        descriptor: Option<crate::content_ids::DescriptorId>,
        amount: u32,
    ) -> Result<Self, AttackError> {
        let component = crate::damage::DamageComponent::fixed(category, descriptor, amount);
        let primary = component.key();
        let damage = DamageSpec::new(vec![component], Some(primary))
            .map_err(|_| AttackError::InvalidDefinition)?;
        Self::new(skill, bonus, wind_up, recovery, damage)
    }

    pub fn new(
        skill: Skill,
        bonus: i32,
        wind_up: u64,
        recovery: u64,
        damage: DamageSpec,
    ) -> Result<Self, AttackError> {
        if !matches!(skill, Skill::HeavyWeaponry | Skill::LightWeaponry)
            || !(-crate::limits::STAT_MAGNITUDE..=crate::limits::STAT_MAGNITUDE)
                .contains(&i64::from(bonus))
            || !(1..=crate::limits::NUMERIC as u64).contains(&wind_up)
            || !(1..=crate::limits::NUMERIC as u64).contains(&recovery)
            || damage.primary().is_none()
        {
            return Err(AttackError::InvalidDefinition);
        }
        Ok(Self {
            skill,
            bonus,
            wind_up,
            recovery,
            damage,
            damage_enchantment: 0,
        })
    }

    pub fn skill(&self) -> Skill {
        self.skill
    }
    pub fn bonus(&self) -> i32 {
        self.bonus
    }
    pub fn wind_up(&self) -> u64 {
        self.wind_up
    }
    pub fn recovery(&self) -> u64 {
        self.recovery
    }
    pub fn damage(&self) -> &DamageSpec {
        &self.damage
    }

    /// Signed instance credit stays separate until all primary damage modifiers
    /// are combined, so negative fixed damage never clamps prematurely.
    pub fn damage_enchantment(&self) -> i32 {
        self.damage_enchantment
    }

    pub(crate) fn with_enchantment(&self, hit: i32, damage: i32) -> Result<Self, AttackError> {
        let bonus = self
            .bonus
            .checked_add(hit)
            .ok_or(AttackError::InvalidDefinition)?;
        let damage_enchantment = self
            .damage_enchantment
            .checked_add(damage)
            .ok_or(AttackError::InvalidDefinition)?;
        if !(-crate::limits::NUMERIC..=crate::limits::NUMERIC)
            .contains(&i64::from(damage_enchantment))
        {
            return Err(AttackError::InvalidDefinition);
        }
        let mut result = Self::new(
            self.skill,
            bonus,
            self.wind_up,
            self.recovery,
            self.damage.clone(),
        )?;
        result.damage_enchantment = damage_enchantment;
        // Validate the effective standalone contribution, preserving the signed
        // operand itself for later ability modifiers.
        result
            .damage_with_modifiers(0, 0, 0)
            .map_err(|_| AttackError::InvalidDefinition)?;
        Ok(result)
    }

    /// Fixed primary amounts have no dice to extend. Dice-count modifiers apply
    /// only to rolled primaries; signed flat modifiers apply to either kind.
    pub fn damage_with_modifiers(
        &self,
        extra_dice: u16,
        flat_bonus: i32,
        impact_bonus: u32,
    ) -> Result<DamageSpec, DamageError> {
        let flat_bonus = flat_bonus
            .checked_add(self.damage_enchantment)
            .ok_or(DamageError::InvalidSpec)?;
        self.damage
            .with_melee_modifiers(extra_dice, flat_bonus, impact_bonus)
    }
}

// Persist only bounded source records; deserialization reconstructs through the
// same constructors used by authoring and explicit checkpoint restoration.
impl serde::Serialize for MeleeAttack {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MeleeAttackRecord::capture(self).serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for MeleeAttack {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = MeleeAttackRecord::deserialize(deserializer)?;
        record
            .restore()
            .map_err(|_| serde::de::Error::custom("invalid melee attack definition"))
    }
}

#[cfg(test)]
mod tests;
