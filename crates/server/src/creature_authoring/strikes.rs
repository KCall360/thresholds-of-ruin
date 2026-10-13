//! Author-owned strike syntax is independent from simulation and save records.
use super::*;
use crate::scenario_package::{AmountExpr, DamageExpr, Effect};

pub(crate) fn bounded_vec<'de, D, T, const MAXIMUM: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Sequence<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Sequence<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "at most {N} authored entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if sequence.size_hint().is_some_and(|size| size > N) {
                return Err(A::Error::custom("authored sequence exceeds limit"));
            }
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(N));
            while values.len() < N {
                let Some(value) = sequence.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("authored sequence exceeds limit"));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Sequence::<T, MAXIMUM>(std::marker::PhantomData))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Natural {
    pub skill: String,
    pub bonus: i32,
    pub wind_up: u64,
    pub recovery: u64,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::STRIKES }>")]
    pub strikes: Vec<Strike>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strike {
    pub offset: u64,
    pub unlock: Option<String>,
    pub check: Option<Check>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::EFFECTS_PER_BRANCH }>")]
    pub on_success: Vec<Effect>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::EFFECTS_PER_BRANCH }>")]
    pub on_failure: Vec<Effect>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::DESCRIPTORS_PER_EFFECT }>")]
    pub descriptors: Vec<String>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::RIDERS_PER_STRIKE }>")]
    pub riders: Vec<Rider>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rider {
    pub trigger: RiderTrigger,
    pub check: Option<RiderCheck>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::EFFECTS_PER_BRANCH }>")]
    pub effects: Vec<Effect>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { s::limits::DESCRIPTORS_PER_EFFECT }>")]
    pub descriptors: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiderTrigger {
    Success,
    Failure,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiderCheck {
    pub skill: SkillSelector,
    pub defense: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub actor: CheckActor,
    pub skill: SkillSelector,
    pub against: Against,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckActor {
    User,
    Target,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SkillSelector {
    Named { skill: String },
    NaturalAttack,
    SelectedWeapon,
}
impl<'de> Deserialize<'de> for SkillSelector {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Empty struct records reject unknown fields that internally tagged unit
        // variants would otherwise discard before source validation.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            Named { skill: String },
            NaturalAttack {},
            SelectedWeapon {},
        }
        Ok(match Record::deserialize(deserializer)? {
            Record::Named { skill } => Self::Named { skill },
            Record::NaturalAttack {} => Self::NaturalAttack,
            Record::SelectedWeapon {} => Self::SelectedWeapon,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Against {
    Defense { defense: String },
    Difficulty { formula: Formula },
    Opposed { skill: SkillSelector },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Formula {
    pub base: i32,
    pub skill: Option<SkillSelector>,
    pub attribute: Option<String>,
}
impl SkillSelector {
    fn compile(&self) -> Result<s::rules::SkillSelector, Failure> {
        Ok(match self {
            Self::Named { skill: name } => s::rules::SkillSelector::Named {
                skill: skill(name)?,
            },
            Self::NaturalAttack => s::rules::SkillSelector::NaturalAttack,
            Self::SelectedWeapon => s::rules::SkillSelector::SelectedWeapon,
        })
    }
}
impl Natural {
    pub(crate) fn compile(
        &self,
        rules: &s::rules::Ruleset,
    ) -> Result<s::attacks::NaturalAttack, Failure> {
        if !self.valid_layout() {
            return Err(invalid("Invalid natural strike layout"));
        }
        let strikes = self
            .strikes
            .iter()
            .map(|strike| strike.compile(rules))
            .collect::<Result<Vec<_>, _>>()?;
        rules
            .validate_strikes(&strikes)
            .map_err(|error| invalid(format!("Invalid natural strikes: {error}")))?;
        s::attacks::NaturalAttack::new(
            skill(&self.skill)?,
            self.bonus,
            self.wind_up,
            self.recovery,
            strikes,
        )
        .map_err(|_| invalid("Invalid natural attack"))
    }
}
impl Check {
    fn compile(&self) -> Result<s::rules::CheckDef, Failure> {
        Ok(s::rules::CheckDef {
            actor: match self.actor {
                CheckActor::User => s::rules::CheckActor::User,
                CheckActor::Target => s::rules::CheckActor::Target,
            },
            skill: self.skill.compile()?,
            against: match &self.against {
                Against::Defense { defense } => s::rules::Against::Defense {
                    defense: primitive(defense, "defense")?,
                },
                Against::Difficulty { formula } => s::rules::Against::Difficulty {
                    formula: s::rules::Formula {
                        base: formula.base,
                        skill: formula
                            .skill
                            .as_ref()
                            .map(SkillSelector::compile)
                            .transpose()?,
                        attribute: formula
                            .attribute
                            .as_ref()
                            .map(|name| attribute(name))
                            .transpose()?,
                    },
                },
                Against::Opposed { skill } => s::rules::Against::Opposed {
                    skill: skill.compile()?,
                },
            },
        })
    }
}
fn descriptors(values: &[String], rules: &s::rules::Ruleset) -> Result<Vec<DescriptorId>, Failure> {
    values
        .iter()
        .map(|value| {
            descriptor(&Some(value.clone()), rules).map(|value| value.expect("named descriptor"))
        })
        .collect()
}
impl Strike {
    fn compile(&self, rules: &s::rules::Ruleset) -> Result<s::rules::StrikeDef, Failure> {
        Ok(s::rules::StrikeDef {
            offset: self.offset,
            unlock: self
                .unlock
                .as_ref()
                .map(|name| talent(name, rules))
                .transpose()?,
            check: self.check.as_ref().map(Check::compile).transpose()?,
            on_success: effects(&self.on_success, rules)?,
            on_failure: effects(&self.on_failure, rules)?,
            descriptors: descriptors(&self.descriptors, rules)?,
            riders: self
                .riders
                .iter()
                .map(|rider| {
                    Ok(s::rules::RiderDef {
                        trigger: match rider.trigger {
                            RiderTrigger::Success => s::rules::RiderTrigger::Success,
                            RiderTrigger::Failure => s::rules::RiderTrigger::Failure,
                        },
                        check: rider
                            .check
                            .as_ref()
                            .map(|check| {
                                Ok::<_, Failure>(s::rules::RiderCheckDef {
                                    skill: check.skill.compile()?,
                                    defense: primitive(&check.defense, "defense")?,
                                })
                            })
                            .transpose()?,
                        effects: effects(&rider.effects, rules)?,
                        descriptors: descriptors(&rider.descriptors, rules)?,
                    })
                })
                .collect::<Result<Vec<_>, Failure>>()?,
        })
    }
}
fn amount(value: &AmountExpr) -> s::rules::AmountExpr {
    match value {
        AmountExpr::Fixed { amount } => s::rules::AmountExpr::Fixed { amount: *amount },
        AmountExpr::Rolled { dice } => s::rules::AmountExpr::Rolled {
            dice: s::rules::DiceExpr {
                count: dice.count,
                sides: dice.sides,
                bonus: dice.bonus,
            },
        },
    }
}
fn effects(values: &[Effect], rules: &s::rules::Ruleset) -> Result<Vec<s::rules::Effect>, Failure> {
    values
        .iter()
        .map(|value| {
            Ok(match value {
                Effect::Damage { damage } => s::rules::Effect::Damage {
                    damage: match damage {
                        DamageExpr::SelectedWeapon { impact_bonus } => {
                            s::rules::DamageExpr::SelectedWeapon {
                                impact_bonus: *impact_bonus,
                            }
                        }
                        DamageExpr::Components { components } => s::rules::DamageExpr::Components {
                            components: components
                                .iter()
                                .map(|component| {
                                    Ok(s::rules::DamageComponentExpr {
                                        category: component.category.into(),
                                        descriptor: descriptor(&component.descriptor, rules)?,
                                        amount: amount(&component.amount),
                                    })
                                })
                                .collect::<Result<Vec<_>, Failure>>()?,
                        },
                    },
                },
                Effect::Heal { amount: value } => s::rules::Effect::Heal {
                    amount: amount(value),
                },
                Effect::Restore {
                    resource,
                    amount: value,
                } => s::rules::Effect::Restore {
                    resource: primitive(resource, "resource")?,
                    amount: amount(value),
                },
                Effect::ApplyCondition {
                    condition,
                    duration,
                } => s::rules::Effect::ApplyCondition {
                    condition: ConditionId::new(condition)
                        .map_err(|_| invalid("Invalid condition ID"))?,
                    duration: *duration,
                },
            })
        })
        .collect()
}
impl Natural {
    /// Bound allocation and layout without resolving names against a catalog.
    pub(crate) fn valid_layout(&self) -> bool {
        if !matches!(self.skill.as_str(), "heavy_weaponry" | "light_weaponry")
            || !(-s::limits::STAT_MAGNITUDE..=s::limits::STAT_MAGNITUDE)
                .contains(&i64::from(self.bonus))
            || self.strikes.is_empty()
            || self.strikes.len() > s::limits::STRIKES
            || self.strikes[0].offset != 0
            || self.strikes[0].unlock.is_some()
            || !(1..=s::limits::NUMERIC as u64).contains(&self.wind_up)
            || !(1..=s::limits::NUMERIC as u64).contains(&self.recovery)
        {
            return false;
        }
        let mut previous = 0;
        let mut effects = 0usize;
        let mut descriptors = 0usize;
        for strike in &self.strikes {
            if strike.offset < previous
                || strike.offset > s::limits::NUMERIC as u64
                || strike.riders.len() > s::limits::RIDERS_PER_STRIKE
                || strike.descriptors.len() > s::limits::DESCRIPTORS_PER_EFFECT
            {
                return false;
            }
            previous = strike.offset;
            descriptors += strike.descriptors.len();
            for rider in &strike.riders {
                if rider.descriptors.len() > s::limits::DESCRIPTORS_PER_EFFECT
                    || (strike.check.is_none() && rider.trigger == RiderTrigger::Failure)
                {
                    return false;
                }
                descriptors += rider.descriptors.len();
            }
            for branch in [&strike.on_success, &strike.on_failure]
                .into_iter()
                .chain(strike.riders.iter().map(|rider| &rider.effects))
            {
                if branch.len() > s::limits::EFFECTS_PER_BRANCH {
                    return false;
                }
                effects += branch.len();
                for effect in branch {
                    if let Effect::Damage {
                        damage: DamageExpr::Components { components },
                    } = effect
                    {
                        if components.len() > s::limits::DAMAGE_COMPONENTS {
                            return false;
                        }
                        descriptors += components
                            .iter()
                            .filter(|component| component.descriptor.is_some())
                            .count();
                    }
                }
            }
            if effects > s::limits::EFFECTS_PER_SEQUENCE
                || descriptors > s::limits::DESCRIPTORS_PER_SEQUENCE
            {
                return false;
            }
        }
        true
    }
}
