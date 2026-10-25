//! Natural attacks retain authored definitions; no derived operands are persisted.
use super::{AttackError, MeleeAttack};
use crate::attributes::Skill;
use crate::damage::{DamageAmount, DamageComponent, DamageSpec};
use crate::rules::{
    Against, AmountExpr, CheckActor, CheckDef, DamageComponentExpr, DamageExpr, DefenseKind,
    DiceExpr, Effect, SkillSelector, StrikeDef,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NaturalAttack {
    pub(super) skill: Skill,
    pub(super) bonus: i32,
    pub(super) wind_up: u64,
    pub(super) recovery: u64,
    pub(super) strikes: Vec<StrikeDef>,
}
impl NaturalAttack {
    pub fn new(
        skill: Skill,
        bonus: i32,
        wind_up: u64,
        recovery: u64,
        strikes: Vec<StrikeDef>,
    ) -> Result<Self, AttackError> {
        if !matches!(skill, Skill::HeavyWeaponry | Skill::LightWeaponry)
            || !(-crate::limits::STAT_MAGNITUDE..=crate::limits::STAT_MAGNITUDE)
                .contains(&i64::from(bonus))
            || !(1..=crate::limits::NUMERIC as u64).contains(&wind_up)
            || !(1..=crate::limits::NUMERIC as u64).contains(&recovery)
            || crate::rules::validate_strike_shapes(&strikes).is_err()
            || crate::rules::validate_strike_horizon(&strikes, wind_up, recovery).is_err()
            || strikes.iter().any(|strike| {
                [&strike.on_success, &strike.on_failure]
                    .into_iter()
                    .chain(strike.riders.iter().map(|rider| &rider.effects))
                    .flatten()
                    .any(|effect| {
                        effect.spatial()
                            || matches!(
                                effect,
                                Effect::RoomLight
                                    | Effect::DetectMonsters { .. }
                                    | Effect::DetectFood
                                    | Effect::DetectMagic
                                    | Effect::DrainLife
                                    | Effect::CancelSpecials
                                    | Effect::HasteDose { .. }
                            )
                    })
            })
        {
            return Err(AttackError::InvalidDefinition);
        }
        Ok(Self {
            skill,
            bonus,
            wind_up,
            recovery,
            strikes,
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
    pub fn strikes(&self) -> &[StrikeDef] {
        &self.strikes
    }
    /// A selected-source damage expression uses the first strike's first success-branch
    /// damage payload. Never infer a fallback from failure, riders or later strikes:
    /// the consuming ability supplies its own check and outcome branches.
    pub fn first_primary_damage(&self) -> Result<DamageSpec, SelectedDamageError> {
        let damage = self
            .strikes
            .first()
            .and_then(|strike| {
                strike.on_success.iter().find_map(|effect| match effect {
                    Effect::Damage { damage } => Some(damage),
                    _ => None,
                })
            })
            .ok_or(SelectedDamageError::NoPrimaryDamage)?;
        let DamageExpr::Components { components } = damage else {
            return Err(SelectedDamageError::NoPrimaryDamage);
        };
        let values = components
            .iter()
            .map(|component| {
                Ok(match &component.amount {
                    AmountExpr::Fixed { amount } => DamageComponent::fixed(
                        component.category,
                        component.descriptor.clone(),
                        *amount,
                    ),
                    AmountExpr::Rolled { dice } => DamageComponent::rolled(
                        component.category,
                        component.descriptor.clone(),
                        crate::dice::DicePool::new(dice.count, dice.sides, dice.bonus)
                            .map_err(|_| SelectedDamageError::InvalidDefinition)?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, SelectedDamageError>>()?;
        let primary = values.first().map(DamageComponent::key);
        if primary.is_none() {
            return Err(SelectedDamageError::NoPrimaryDamage);
        }
        DamageSpec::new(values, primary).map_err(|_| SelectedDamageError::InvalidDefinition)
    }
    /// Explicit conversion for existing Rust content builders, never a source reader fallback.
    pub fn from_melee(attack: MeleeAttack) -> Result<Self, AttackError> {
        if attack.damage_enchantment() != 0 {
            return Err(AttackError::InvalidDefinition);
        }
        let primary = attack
            .damage()
            .primary()
            .ok_or(AttackError::InvalidDefinition)?;
        let mut components: Vec<_> = attack.damage().components().iter().collect();
        // Component expressions declare primary by authored order. DamageSpec later
        // restores canonical order, including when primary sorts after a secondary.
        components.sort_by_key(|component| component.key() != primary);
        let components = components
            .into_iter()
            .map(|component| {
                let key = component.key();
                DamageComponentExpr {
                    category: key.category,
                    descriptor: key.descriptor,
                    amount: match component.amount() {
                        DamageAmount::Fixed(amount) => AmountExpr::Fixed { amount },
                        DamageAmount::Rolled(dice) => AmountExpr::Rolled {
                            dice: DiceExpr {
                                count: dice.count(),
                                sides: dice.sides(),
                                bonus: dice.bonus(),
                            },
                        },
                    },
                }
            })
            .collect();
        Self::new(
            attack.skill(),
            attack.bonus(),
            attack.wind_up(),
            attack.recovery(),
            vec![StrikeDef {
                cancellable: false,
                offset: 0,
                unlock: None,
                check: Some(CheckDef {
                    actor: CheckActor::User,
                    skill: SkillSelector::SelectedWeapon,
                    against: Against::Defense {
                        defense: DefenseKind::Physical,
                    },
                }),
                on_success: vec![Effect::Damage {
                    damage: DamageExpr::Components { components },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }],
        )
    }
}
impl serde::Serialize for NaturalAttack {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        super::NaturalAttackRecord::capture(self).serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for NaturalAttack {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = super::NaturalAttackRecord::deserialize(deserializer)?;
        record
            .restore()
            .map_err(|_| serde::de::Error::custom("invalid natural attack definition"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedDamageError {
    NoPrimaryDamage,
    InvalidDefinition,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedMeleeAttack {
    Weapon(MeleeAttack),
    Natural(NaturalAttack),
}
impl SelectedMeleeAttack {
    pub fn skill(&self) -> Skill {
        match self {
            Self::Weapon(attack) => attack.skill(),
            Self::Natural(attack) => attack.skill(),
        }
    }
    pub fn bonus(&self) -> i32 {
        match self {
            Self::Weapon(attack) => attack.bonus(),
            Self::Natural(attack) => attack.bonus(),
        }
    }
    pub fn wind_up(&self) -> u64 {
        match self {
            Self::Weapon(attack) => attack.wind_up(),
            Self::Natural(attack) => attack.wind_up(),
        }
    }
    pub fn recovery(&self) -> u64 {
        match self {
            Self::Weapon(attack) => attack.recovery(),
            Self::Natural(attack) => attack.recovery(),
        }
    }
    pub fn first_primary_damage(&self) -> Result<DamageSpec, SelectedDamageError> {
        match self {
            Self::Weapon(attack) => Ok(attack.damage().clone()),
            Self::Natural(attack) => attack.first_primary_damage(),
        }
    }
    pub fn damage_with_modifiers(
        &self,
        extra_dice: u16,
        flat_bonus: i32,
        impact_bonus: u32,
    ) -> Result<DamageSpec, SelectedDamageError> {
        match self {
            Self::Weapon(attack) => {
                attack.damage_with_modifiers(extra_dice, flat_bonus, impact_bonus)
            }
            Self::Natural(attack) => attack.first_primary_damage()?.with_melee_modifiers(
                extra_dice,
                flat_bonus,
                impact_bonus,
            ),
        }
        .map_err(|_| SelectedDamageError::InvalidDefinition)
    }
}
