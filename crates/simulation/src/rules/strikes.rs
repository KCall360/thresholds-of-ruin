//! One bounded authoring shape is shared by abilities and species natural attacks.
use super::*;
use crate::bounded::Bounded;

pub(super) fn bounded_vec<'de, D, T, const MAXIMUM: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Bounded::<T, MAXIMUM>::deserialize(deserializer).map(|values| values.0)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikeSource {
    Authored {
        strikes: Vec<StrikeDef>,
    },
    SelectedMelee,
    Projectile {
        delivery: crate::projectile_profiles::ProjectileDeliveryKind,
    },
    /// Reconstructed reusable source: the invoking species, then declared riders.
    NaturalMelee {
        additional_riders: Vec<RiderDef>,
    },
}
impl<'de> Deserialize<'de> for StrikeSource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            Authored {
                strikes: Bounded<StrikeDef, { crate::limits::STRIKES }>,
            },
            SelectedMelee {},
            Projectile {
                delivery: crate::projectile_profiles::ProjectileDeliveryKind,
            },
            NaturalMelee {
                additional_riders: Bounded<RiderDef, { crate::limits::RIDERS_PER_STRIKE }>,
            },
        }
        match crate::bounded::named_map::<_, Record>(deserializer)? {
            Record::Authored { strikes } => {
                validate_strike_shapes(&strikes.0).map_err(serde::de::Error::custom)?;
                Ok(Self::Authored { strikes: strikes.0 })
            }
            Record::SelectedMelee {} => Ok(Self::SelectedMelee),
            Record::Projectile { delivery } => Ok(Self::Projectile { delivery }),
            Record::NaturalMelee { additional_riders } => {
                validate_rider_shapes(&additional_riders.0).map_err(serde::de::Error::custom)?;
                Ok(Self::NaturalMelee {
                    additional_riders: additional_riders.0,
                })
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StrikeDef {
    pub offset: u64,
    pub unlock: Option<TalentId>,
    pub check: Option<CheckDef>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { crate::limits::EFFECTS_PER_BRANCH }>")]
    pub on_success: Vec<Effect>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { crate::limits::EFFECTS_PER_BRANCH }>")]
    pub on_failure: Vec<Effect>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { crate::limits::DESCRIPTORS_PER_EFFECT }>")]
    pub descriptors: Vec<DescriptorId>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { crate::limits::RIDERS_PER_STRIKE }>")]
    pub riders: Vec<RiderDef>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RiderDef {
    pub trigger: RiderTrigger,
    pub condition: Option<RiderCondition>,
    pub check: Option<RiderCheckDef>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { crate::limits::EFFECTS_PER_BRANCH }>")]
    pub effects: Vec<Effect>,
    #[serde(deserialize_with = "bounded_vec::<_, _, { crate::limits::DESCRIPTORS_PER_EFFECT }>")]
    pub descriptors: Vec<DescriptorId>,
}
impl<'de> Deserialize<'de> for StrikeDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            offset: u64,
            unlock: Option<TalentId>,
            check: Option<CheckDef>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { crate::limits::EFFECTS_PER_BRANCH }>"
            )]
            on_success: Vec<Effect>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { crate::limits::EFFECTS_PER_BRANCH }>"
            )]
            on_failure: Vec<Effect>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { crate::limits::DESCRIPTORS_PER_EFFECT }>"
            )]
            descriptors: Vec<DescriptorId>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { crate::limits::RIDERS_PER_STRIKE }>"
            )]
            riders: Vec<RiderDef>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            offset: fields.offset,
            unlock: fields.unlock,
            check: fields.check,
            on_success: fields.on_success,
            on_failure: fields.on_failure,
            descriptors: fields.descriptors,
            riders: fields.riders,
        })
    }
}
impl<'de> Deserialize<'de> for RiderDef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            trigger: RiderTrigger,
            condition: Option<RiderCondition>,
            check: Option<RiderCheckDef>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { crate::limits::EFFECTS_PER_BRANCH }>"
            )]
            effects: Vec<Effect>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { crate::limits::DESCRIPTORS_PER_EFFECT }>"
            )]
            descriptors: Vec<DescriptorId>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            trigger: fields.trigger,
            condition: fields.condition,
            check: fields.check,
            effects: fields.effects,
            descriptors: fields.descriptors,
        })
    }
}
/// Compose the complete source before filtering unlocks. Dormant rows must obey
/// the same combined budgets and trigger constraints as immediately enabled rows.
pub fn append_natural_riders(
    natural: &[StrikeDef],
    additional_riders: &[RiderDef],
) -> Result<Vec<StrikeDef>, RulesetError> {
    validate_strike_shapes(natural)?;
    validate_rider_shapes(additional_riders)?;
    let mut strikes = natural.to_vec();
    for strike in &mut strikes {
        strike.riders.extend_from_slice(additional_riders);
    }
    validate_strike_shapes(&strikes)?;
    Ok(strikes)
}
fn rider_shape(riders: &[RiderDef]) -> StrikeDef {
    StrikeDef {
        offset: 0,
        unlock: None,
        // A generic rider may depend on either primary branch. Its concrete
        // natural row is checked separately during composition.
        check: Some(CheckDef {
            actor: CheckActor::User,
            skill: SkillSelector::NaturalAttack,
            against: Against::Defense {
                defense: DefenseKind::Physical,
            },
        }),
        on_success: vec![],
        on_failure: vec![],
        descriptors: vec![],
        riders: riders.to_vec(),
    }
}
/// Validate reusable riders before they are composed with a concrete source.
pub fn validate_rider_shapes(riders: &[RiderDef]) -> Result<(), RulesetError> {
    bound(riders.len() <= crate::limits::RIDERS_PER_STRIKE, "riders")?;
    validate_strike_shapes(&[rider_shape(riders)])
}
/// The primary roller's selected branch, independent of who rolled the check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiderTrigger {
    Success,
    Failure,
}
/// A closed sequence-local predicate, evaluated only on a qualifying primary.
/// Its once receipt belongs to preparation progress, never to this definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RiderCondition {
    SequenceSuccessfulHits {
        #[serde(deserialize_with = "sequence_minimum")]
        minimum: u16,
    },
}
fn sequence_minimum<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    let minimum = u16::deserialize(d)?;
    if !(1..=crate::limits::STRIKES as u16).contains(&minimum) {
        return Err(serde::de::Error::custom("invalid sequence hit minimum"));
    }
    Ok(minimum)
}
impl RiderCondition {
    pub fn valid(self) -> bool {
        match self {
            Self::SequenceSuccessfulHits { minimum } => {
                (1..=crate::limits::STRIKES as u16).contains(&minimum)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiderCheckDef {
    pub skill: SkillSelector,
    pub defense: DefenseKind,
}

/// Context-free shape validation also applies to constructor-built definitions.
/// Reference validation is performed by `Ruleset::validate_strikes`.
pub fn validate_strike_shapes(strikes: &[StrikeDef]) -> Result<(), RulesetError> {
    bound(
        !strikes.is_empty() && strikes.len() <= crate::limits::STRIKES,
        "strikes",
    )?;
    bound(
        strikes[0].offset == 0 && strikes[0].unlock.is_none(),
        "first_strike",
    )?;
    let mut previous = 0;
    let mut effects = 0usize;
    let mut descriptors = 0usize;
    for strike in strikes {
        bound(
            !strike
                .on_success
                .iter()
                .chain(&strike.on_failure)
                .any(|effect| matches!(effect, Effect::Theft { .. }))
                && strike.riders.iter().all(|rider| {
                    !rider
                        .effects
                        .iter()
                        .any(|effect| matches!(effect, Effect::Theft { .. }))
                        || (rider.effects.len() == 1
                            && rider.check.is_some()
                            && rider.trigger == RiderTrigger::Success)
                }),
            "theft_checked_rider",
        )?;
        bound(
            strike.offset >= previous && strike.offset <= crate::limits::NUMERIC as u64,
            "strike_offset",
        )?;
        previous = strike.offset;
        let tightening = |effects: &[Effect]| {
            effects
                .iter()
                .filter(|effect| matches!(effect, Effect::HoldDamage { .. }))
                .count()
        };
        bound(
            tightening(&strike.on_success) <= 1
                && tightening(&strike.on_failure) == 0
                && strike
                    .riders
                    .iter()
                    .all(|rider| tightening(&rider.effects) == 0)
                && (tightening(&strike.on_success) == 0
                    || strike.check.as_ref().is_some_and(|check| {
                        check.actor == CheckActor::User
                            && matches!(
                                check.against,
                                Against::Defense {
                                    defense: DefenseKind::Physical
                                }
                            )
                    })),
            "hold_damage_primary",
        )?;
        bound(
            strike.riders.len() <= crate::limits::RIDERS_PER_STRIKE,
            "riders",
        )?;
        bound(
            strike.descriptors.len() <= crate::limits::DESCRIPTORS_PER_EFFECT,
            "descriptors",
        )?;
        descriptors = descriptors
            .checked_add(strike.descriptors.len())
            .ok_or_else(|| sequence_bound("sequence_descriptors"))?;
        if let Some(CheckDef {
            against: Against::Difficulty { formula },
            ..
        }) = &strike.check
        {
            bound(
                i64::from(formula.base).abs() <= crate::limits::NUMERIC,
                "difficulty",
            )?;
        }
        for branch in [&strike.on_success, &strike.on_failure]
            .into_iter()
            .chain(strike.riders.iter().map(|rider| &rider.effects))
        {
            bound(branch.len() <= crate::limits::EFFECTS_PER_BRANCH, "effects")?;
            effects = effects
                .checked_add(branch.len())
                .ok_or_else(|| sequence_bound("sequence_effects"))?;
            for effect in branch {
                bound(effect.valid_shape(), "effect")?;
                if let Effect::HoldDamage { descriptor, .. } = effect {
                    descriptors = descriptors
                        .checked_add(usize::from(descriptor.is_some()))
                        .ok_or_else(|| sequence_bound("sequence_descriptors"))?;
                }
                if let Effect::Damage {
                    damage: DamageExpr::Components { components },
                } = effect
                {
                    let checked = components
                        .iter()
                        .map(|component| {
                            Ok(match &component.amount {
                                AmountExpr::Fixed { amount } => {
                                    crate::damage::DamageComponent::fixed(
                                        component.category,
                                        component.descriptor.clone(),
                                        *amount,
                                    )
                                }
                                AmountExpr::Rolled { dice } => {
                                    crate::damage::DamageComponent::rolled(
                                        component.category,
                                        component.descriptor.clone(),
                                        DicePool::new(dice.count, dice.sides, dice.bonus)
                                            .map_err(|_| sequence_bound("dice"))?,
                                    )
                                }
                            })
                        })
                        .collect::<Result<Vec<_>, RulesetError>>()?;
                    crate::damage::DamageSpec::new(checked, None)
                        .map_err(|_| sequence_bound("damage_components"))?;
                    descriptors = descriptors
                        .checked_add(
                            components
                                .iter()
                                .filter(|component| component.descriptor.is_some())
                                .count(),
                        )
                        .ok_or_else(|| sequence_bound("sequence_descriptors"))?;
                }
            }
        }
        for rider in &strike.riders {
            bound(
                rider.condition.is_none_or(|condition| {
                    condition.valid() && rider.trigger == RiderTrigger::Success
                }),
                "rider_condition",
            )?;
            bound(
                strike.check.is_some() || rider.trigger == RiderTrigger::Success,
                "rider_trigger",
            )?;
            bound(
                rider.descriptors.len() <= crate::limits::DESCRIPTORS_PER_EFFECT,
                "descriptors",
            )?;
            descriptors = descriptors
                .checked_add(rider.descriptors.len())
                .ok_or_else(|| sequence_bound("sequence_descriptors"))?;
        }
        bound(
            effects <= crate::limits::EFFECTS_PER_SEQUENCE,
            "sequence_effects",
        )?;
        bound(
            descriptors <= crate::limits::DESCRIPTORS_PER_SEQUENCE,
            "sequence_descriptors",
        )?;
    }
    Ok(())
}
fn sequence_bound(field: &str) -> RulesetError {
    RulesetError::Bound {
        field: field.into(),
    }
}

/// Authored phase values are individually bounded; the final scaled horizon is
/// checked by plan capture, because Speed may shorten the complete sequence.
pub fn validate_strike_horizon(
    strikes: &[StrikeDef],
    preparation: u64,
    recovery: u64,
) -> Result<(), RulesetError> {
    let horizon = preparation
        .checked_add(strikes.last().map_or(0, |strike| strike.offset))
        .and_then(|value| value.checked_add(recovery));
    bound(
        preparation <= crate::limits::NUMERIC as u64
            && recovery <= crate::limits::NUMERIC as u64
            && strikes
                .iter()
                .all(|strike| strike.offset <= crate::limits::NUMERIC as u64)
            && horizon.is_some(),
        "strike_horizon",
    )
}

impl References<'_> {
    pub(super) fn riders(
        &self,
        values: &mut Vec<RiderDef>,
        pool: &mut Interner,
    ) -> Result<(), RulesetError> {
        let mut shape = rider_shape(values);
        self.strikes(std::slice::from_mut(&mut shape), pool)?;
        *values = shape.riders;
        Ok(())
    }
    pub(super) fn strikes(
        &self,
        values: &mut [StrikeDef],
        pool: &mut Interner,
    ) -> Result<(), RulesetError> {
        validate_strike_shapes(values)?;
        for strike in values {
            if let Some(unlock) = &mut strike.unlock {
                reference(self.talents, unlock, "talent")?;
                unlock.intern(pool);
            }
            self.descriptors(&mut strike.descriptors, pool)?;
            self.effects(&mut strike.on_success, pool)?;
            self.effects(&mut strike.on_failure, pool)?;
            for rider in &mut strike.riders {
                self.descriptors(&mut rider.descriptors, pool)?;
                self.effects(&mut rider.effects, pool)?;
            }
        }
        Ok(())
    }
}
