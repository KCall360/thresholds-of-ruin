//! One bounded authored random event at an eligible empty-depletion deadline.
//! This is immutable rules content; reserve and condition clocks remain canonical.
use crate::{
    abilities::{PlannedAmount, PlannedEffect},
    content_ids::ConditionId,
    rules::{AmountExpr, ConditionLifecycle, ConditionTicks, RulesetDocument},
};
use serde::{Deserialize, Serialize};

/// Structural work bound per uneaten empty episode, independent of tick duration.
/// Each eligible deadline draws one chance and at most one bounded amount pool.
pub const MAX_EMPTY_DEPLETION_CHECKS: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct NutritionChance {
    pub one_in: u16,
}
impl<'de> Deserialize<'de> for NutritionChance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            one_in: u16,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            one_in: fields.one_in,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NutritionConditionTrigger {
    Disabled,
    OnEmptyDepletion {
        condition: ConditionId,
        chance: NutritionChance,
        duration: AmountExpr,
        ticks_per_unit: u64,
    },
}
impl<'de> Deserialize<'de> for NutritionConditionTrigger {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Disabled {},
            OnEmptyDepletion {
                condition: ConditionId,
                chance: NutritionChance,
                duration: AmountExpr,
                ticks_per_unit: u64,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Disabled {} => Self::Disabled,
            Fields::OnEmptyDepletion {
                condition,
                chance,
                duration,
                ticks_per_unit,
            } => Self::OnEmptyDepletion {
                condition,
                chance,
                duration,
                ticks_per_unit,
            },
        })
    }
}
impl NutritionConditionTrigger {
    pub(crate) fn enabled(&self) -> bool {
        matches!(self, Self::OnEmptyDepletion { .. })
    }

    pub(crate) fn valid(&self, document: &RulesetDocument) -> bool {
        let Self::OnEmptyDepletion {
            condition,
            chance,
            duration,
            ticks_per_unit,
        } = self
        else {
            return true;
        };
        let super::NutritionPolicy::Enabled { starve_at, .. } = document.nutrition else {
            return false;
        };
        let Ok(amount) = PlannedAmount::compile(duration) else {
            return false;
        };
        let maximum = match amount {
            PlannedAmount::Fixed { amount } => amount,
            PlannedAmount::Rolled { dice } => dice.maximum(),
        };
        let bounded_duration = u64::from(maximum)
            .checked_mul(*ticks_per_unit)
            .is_some_and(|ticks| ConditionTicks::new(ticks).is_ok());
        starve_at < 0
            && i64::from(starve_at).unsigned_abs() - 1 <= u64::from(MAX_EMPTY_DEPLETION_CHECKS)
            && (1..=crate::limits::DICE_SIDES).contains(&chance.one_in)
            && *ticks_per_unit > 0
            && amount.strictly_positive()
            && bounded_duration
            && document.conditions.iter().any(|definition| {
                definition.id == *condition && definition.lifecycle == ConditionLifecycle::Timed
            })
    }

    /// Only authoritative settlement calls this, after death and denial checks.
    /// Failure draws no duration; success reuses the ordinary frozen amount kernel.
    pub(crate) fn sample(&self, rng: &mut u64) -> Option<PlannedEffect> {
        let Self::OnEmptyDepletion {
            condition,
            chance,
            duration,
            ticks_per_unit,
        } = self
        else {
            return None;
        };
        if crate::dice::die(rng, chance.one_in) != 1 {
            return None;
        }
        let amount = PlannedAmount::compile(duration).expect("validated depletion duration");
        let duration = u64::from(amount.roll(rng))
            .checked_mul(*ticks_per_unit)
            .expect("validated depletion scale");
        Some(PlannedEffect::ApplyCondition {
            id: condition.clone(),
            duration,
        })
    }
}
