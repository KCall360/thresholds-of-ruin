//! Bounded creature attributes, training and shared skill-check resolution.
//!
//! These values are independent of ownership, authoring and persistence DTOs.
//! Build derivation owns permanent choices and applies source-owned adjustments.

use crate::dice::{roll_check, CheckRoll, Edge};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Attribute {
    Strength,
    Speed,
    Intellect,
    Willpower,
    Awareness,
    Presence,
}

impl Attribute {
    pub const ALL: [Self; 6] = [
        Self::Strength,
        Self::Speed,
        Self::Intellect,
        Self::Willpower,
        Self::Awareness,
        Self::Presence,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildValueError {
    Attribute,
    Rank,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attributes([u16; 6]);

impl Attributes {
    pub(crate) fn from_derived(values: [u16; 6]) -> Result<Self, BuildValueError> {
        if values.iter().any(|&value| value > 1_000) {
            return Err(BuildValueError::Attribute);
        }
        Ok(Self(values))
    }

    /// Ordinary permanent attribute choices have a cap of five, including zero.
    pub fn new(values: [u8; 6]) -> Result<Self, BuildValueError> {
        if values.iter().any(|&value| value > 5) {
            return Err(BuildValueError::Attribute);
        }
        Ok(Self(values.map(u16::from)))
    }

    pub fn get(self, attribute: Attribute) -> u16 {
        self.0[attribute as usize]
    }

    /// Apply a derivation stage atomically. Negative adjustments floor at zero;
    /// derived values may exceed the advancement cap but remain bounded.
    pub fn adjusted(self, adjustments: [i16; 6]) -> Result<Self, BuildValueError> {
        let mut values = self.0;
        for (value, adjustment) in values.iter_mut().zip(adjustments) {
            let adjusted = (i32::from(*value) + i32::from(adjustment)).max(0);
            if adjusted > 1_000 {
                return Err(BuildValueError::Attribute);
            }
            *value = adjusted as u16;
        }
        Ok(Self(values))
    }

    pub fn defenses(self) -> Defenses {
        let pair = |a, b| 10 + i32::from(self.get(a)) + i32::from(self.get(b));
        Defenses {
            physical: pair(Attribute::Strength, Attribute::Speed),
            cognitive: pair(Attribute::Intellect, Attribute::Willpower),
            spiritual: pair(Attribute::Awareness, Attribute::Presence),
        }
    }

    /// Scale only action phases whose rules explicitly designate them physical.
    /// Wide arithmetic preserves the full scheduler duration domain.
    pub fn physical_duration(self, base: u64) -> u64 {
        let divisor = 5 + u128::from(self.get(Attribute::Speed));
        (u128::from(base) * 5).div_ceil(divisor).max(1) as u64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defenses {
    pub physical: i32,
    pub cognitive: i32,
    pub spiritual: i32,
}

/// Permanent magical attribute choice. Physical attributes cannot bind Mana.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManaBinding {
    Intellect,
    Willpower,
    Awareness,
    Presence,
}

impl ManaBinding {
    pub fn attribute(self) -> Attribute {
        match self {
            Self::Intellect => Attribute::Intellect,
            Self::Willpower => Attribute::Willpower,
            Self::Awareness => Attribute::Awareness,
            Self::Presence => Attribute::Presence,
        }
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Skill {
    Athletics,
    HeavyWeaponry,
    Agility,
    LightWeaponry,
    Stealth,
    Thievery,
    Crafting,
    Deduction,
    Lore,
    Medicine,
    Discipline,
    Intimidation,
    Insight,
    Perception,
    Survival,
    Deception,
    Leadership,
    Persuasion,
    Spellcasting,
}

impl Skill {
    pub const ALL: [Self; 19] = [
        Self::Athletics,
        Self::HeavyWeaponry,
        Self::Agility,
        Self::LightWeaponry,
        Self::Stealth,
        Self::Thievery,
        Self::Crafting,
        Self::Deduction,
        Self::Lore,
        Self::Medicine,
        Self::Discipline,
        Self::Intimidation,
        Self::Insight,
        Self::Perception,
        Self::Survival,
        Self::Deception,
        Self::Leadership,
        Self::Persuasion,
        Self::Spellcasting,
    ];

    pub fn attribute(self, binding: ManaBinding) -> Attribute {
        match self {
            Self::Athletics | Self::HeavyWeaponry => Attribute::Strength,
            Self::Agility | Self::LightWeaponry | Self::Stealth | Self::Thievery => {
                Attribute::Speed
            }
            Self::Crafting | Self::Deduction | Self::Lore | Self::Medicine => Attribute::Intellect,
            Self::Discipline | Self::Intimidation => Attribute::Willpower,
            Self::Insight | Self::Perception | Self::Survival => Attribute::Awareness,
            Self::Deception | Self::Leadership | Self::Persuasion => Attribute::Presence,
            Self::Spellcasting => binding.attribute(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillRanks([u8; 19]);

impl SkillRanks {
    pub fn get(self, skill: Skill) -> u8 {
        self.0[skill as usize]
    }

    pub fn with_rank(mut self, skill: Skill, rank: u8) -> Result<Self, BuildValueError> {
        if rank > 5 {
            return Err(BuildValueError::Rank);
        }
        self.0[skill as usize] = rank;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillCheck {
    pub skill: Skill,
    pub binding: ManaBinding,
    pub modifier: i32,
    pub threshold: i32,
}

impl SkillCheck {
    /// Callers allocate edge before this check when it starts a damage bundle.
    pub fn resolve(
        self,
        state: &mut u64,
        attributes: Attributes,
        ranks: SkillRanks,
        edge: Edge,
    ) -> CheckOutcome {
        self.resolve_with_diagnostics(state, attributes, ranks, edge, &mut ())
    }

    pub fn resolve_with_diagnostics(
        self,
        state: &mut u64,
        attributes: Attributes,
        ranks: SkillRanks,
        edge: Edge,
        observer: &mut impl crate::resolution_diagnostics::ResolutionObserver,
    ) -> CheckOutcome {
        use crate::resolution_diagnostics::{CheckDiagnostic, ResolutionStep};
        let rng_before = *state;
        let roll = roll_check(state, edge);
        let attribute = self.skill.attribute(self.binding);
        let attribute_value = attributes.get(attribute);
        let rank = ranks.get(self.skill);
        let total = i64::from(roll.kept)
            + i64::from(attribute_value)
            + i64::from(rank)
            + i64::from(self.modifier);
        let outcome = CheckOutcome {
            roll,
            total,
            success: total >= i64::from(self.threshold),
        };
        let input_edge = edge.balance();
        let allocated = input_edge.signum();
        observer.record(ResolutionStep::Check(CheckDiagnostic {
            check: self,
            attribute,
            attribute_value,
            rank,
            input_edge,
            edge: allocated,
            unused_edge: input_edge - allocated,
            rng_before,
            rng_after: *state,
            outcome,
        }));
        outcome
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckOutcome {
    pub roll: CheckRoll,
    pub total: i64,
    pub success: bool,
}
