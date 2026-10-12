//! Bounded creature attributes, training and shared skill-check resolution.
//!
//! These values are independent of ownership, authoring and persistence DTOs.
//! Build derivation owns permanent choices and applies source-owned adjustments.

use crate::dice::{roll_check, CheckRoll, Edge, CHECK_EDGE_CAPACITY};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::Speed => "speed",
            Self::Intellect => "intellect",
            Self::Willpower => "willpower",
            Self::Awareness => "awareness",
            Self::Presence => "presence",
        }
    }
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

/// A named, exhaustive map over the six engine attributes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ByAttribute<T> {
    pub strength: T,
    pub speed: T,
    pub intellect: T,
    pub willpower: T,
    pub awareness: T,
    pub presence: T,
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for ByAttribute<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields<T> {
            strength: T,
            speed: T,
            intellect: T,
            willpower: T,
            awareness: T,
            presence: T,
        }
        let fields: Fields<T> = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            strength: fields.strength,
            speed: fields.speed,
            intellect: fields.intellect,
            willpower: fields.willpower,
            awareness: fields.awareness,
            presence: fields.presence,
        })
    }
}
impl<T> ByAttribute<T> {
    pub fn get(&self, attribute: Attribute) -> &T {
        match attribute {
            Attribute::Strength => &self.strength,
            Attribute::Speed => &self.speed,
            Attribute::Intellect => &self.intellect,
            Attribute::Willpower => &self.willpower,
            Attribute::Awareness => &self.awareness,
            Attribute::Presence => &self.presence,
        }
    }
    pub fn get_mut(&mut self, attribute: Attribute) -> &mut T {
        match attribute {
            Attribute::Strength => &mut self.strength,
            Attribute::Speed => &mut self.speed,
            Attribute::Intellect => &mut self.intellect,
            Attribute::Willpower => &mut self.willpower,
            Attribute::Awareness => &mut self.awareness,
            Attribute::Presence => &mut self.presence,
        }
    }
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> ByAttribute<U> {
        ByAttribute {
            strength: f(self.strength),
            speed: f(self.speed),
            intellect: f(self.intellect),
            willpower: f(self.willpower),
            awareness: f(self.awareness),
            presence: f(self.presence),
        }
    }
    pub fn from_fn(mut f: impl FnMut(Attribute) -> T) -> Self {
        Self {
            strength: f(Attribute::Strength),
            speed: f(Attribute::Speed),
            intellect: f(Attribute::Intellect),
            willpower: f(Attribute::Willpower),
            awareness: f(Attribute::Awareness),
            presence: f(Attribute::Presence),
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = (Attribute, &T)> {
        Attribute::ALL
            .into_iter()
            .map(|attribute| (attribute, self.get(attribute)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attributes(ByAttribute<u16>);

impl Attributes {
    pub(crate) fn from_derived(values: ByAttribute<u16>) -> Result<Self, BuildValueError> {
        if values
            .iter()
            .any(|(_, &value)| value > crate::limits::DERIVED_ATTRIBUTE)
        {
            return Err(BuildValueError::Attribute);
        }
        Ok(Self(values))
    }

    pub fn new(values: ByAttribute<u16>) -> Result<Self, BuildValueError> {
        if values
            .iter()
            .any(|(_, &value)| value > crate::limits::DERIVED_ATTRIBUTE)
        {
            return Err(BuildValueError::Attribute);
        }
        Ok(Self(values))
    }

    pub fn values(self) -> ByAttribute<u16> {
        self.0
    }
    pub fn get(self, attribute: Attribute) -> u16 {
        *self.0.get(attribute)
    }

    /// Aggregate a template stage before flooring, independent of declaration order.
    pub fn adjusted(self, adjustments: ByAttribute<i16>) -> Result<Self, BuildValueError> {
        let mut values = self.0;
        for attribute in Attribute::ALL {
            let adjusted =
                (i32::from(*values.get(attribute)) + i32::from(*adjustments.get(attribute))).max(0);
            if adjusted > i32::from(crate::limits::DERIVED_ATTRIBUTE) {
                return Err(BuildValueError::Attribute);
            }
            *values.get_mut(attribute) = adjusted as u16;
        }
        Ok(Self(values))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defenses {
    pub physical: i32,
    pub cognitive: i32,
    pub spiritual: i32,
}

/// Permanent magical attribute choice. Physical attributes cannot bind Mana.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Athletics => "athletics",
            Self::HeavyWeaponry => "heavy_weaponry",
            Self::Agility => "agility",
            Self::LightWeaponry => "light_weaponry",
            Self::Stealth => "stealth",
            Self::Thievery => "thievery",
            Self::Crafting => "crafting",
            Self::Deduction => "deduction",
            Self::Lore => "lore",
            Self::Medicine => "medicine",
            Self::Discipline => "discipline",
            Self::Intimidation => "intimidation",
            Self::Insight => "insight",
            Self::Perception => "perception",
            Self::Survival => "survival",
            Self::Deception => "deception",
            Self::Leadership => "leadership",
            Self::Persuasion => "persuasion",
            Self::Spellcasting => "spellcasting",
        }
    }
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
    pub fn bonus(self, attributes: Attributes, ranks: SkillRanks) -> i64 {
        i64::from(attributes.get(self.skill.attribute(self.binding)))
            + i64::from(ranks.get(self.skill))
            + i64::from(self.modifier)
    }

    /// Callers allocate edge before this check when it starts a damage bundle.
    pub fn resolve(
        self,
        state: &mut u64,
        attributes: Attributes,
        ranks: SkillRanks,
        edge: Edge,
        observer: &mut impl crate::resolution_diagnostics::ResolutionObserver,
    ) -> CheckOutcome {
        use crate::resolution_diagnostics::{ResolutionStep, TraceField as F, TraceValue as V};
        let rng_before = *state;
        let roll = roll_check(state, edge);
        let attribute = self.skill.attribute(self.binding);
        let attribute_value = attributes.get(attribute);
        let rank = ranks.get(self.skill);
        let total = i64::from(roll.kept) + self.bonus(attributes, ranks);
        let outcome = CheckOutcome {
            roll,
            total,
            success: total >= i64::from(self.threshold),
        };
        let input_edge = edge.balance();
        let allocated = input_edge.signum()
            * input_edge
                .unsigned_abs()
                .min(u64::from(CHECK_EDGE_CAPACITY)) as i64;
        observer.record(ResolutionStep {
            id: "check.resolved",
            fields: &[
                F {
                    id: "skill",
                    value: V::Text(self.skill.as_str()),
                },
                F {
                    id: "attribute",
                    value: V::Text(attribute.as_str()),
                },
                F {
                    id: "attribute_value",
                    value: V::Unsigned(u64::from(attribute_value)),
                },
                F {
                    id: "rank",
                    value: V::Unsigned(u64::from(rank)),
                },
                F {
                    id: "modifier",
                    value: V::Signed(i64::from(self.modifier)),
                },
                F {
                    id: "threshold",
                    value: V::Signed(i64::from(self.threshold)),
                },
                F {
                    id: "input_edge",
                    value: V::Signed(input_edge),
                },
                F {
                    id: "edge",
                    value: V::Signed(allocated),
                },
                F {
                    id: "unused_edge",
                    value: V::Signed(input_edge - allocated),
                },
                F {
                    id: "rng_before",
                    value: V::Unsigned(rng_before),
                },
                F {
                    id: "rng_after",
                    value: V::Unsigned(*state),
                },
                F {
                    id: "rolled",
                    value: V::Dice(roll.rolled()),
                },
                F {
                    id: "retained",
                    value: V::Dice(roll.retained()),
                },
                F {
                    id: "kept",
                    value: V::Unsigned(u64::from(roll.kept)),
                },
                F {
                    id: "total",
                    value: V::Signed(total),
                },
                F {
                    id: "success",
                    value: V::Bool(outcome.success),
                },
            ],
        });
        outcome
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckOutcome {
    pub roll: CheckRoll,
    pub total: i64,
    pub success: bool,
}
