//! Shared reserve bookkeeping; actors supply active time through the existing clock.
//! No wall clock, scheduling queue, item quantity, random stream, or scenario identity.
mod regeneration;
mod trigger;
pub use regeneration::{Regeneration, RegenerationCost};
pub use trigger::{NutritionChance, NutritionConditionTrigger, MAX_EMPTY_DEPLETION_CHECKS};

use crate::health_recovery::RecoveryNutrition;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NutritionPolicy {
    Disabled,
    Enabled {
        initial: u32,
        maximum: u32,
        hungry_below: u32,
        weak_below: u32,
        starve_at: i32,
        unit_ticks: u64,
        hungry_recovery: RecoveryNutrition,
        weak_recovery: RecoveryNutrition,
        empty_recovery: RecoveryNutrition,
        regeneration_cost: RegenerationCost,
    },
}
impl<'de> Deserialize<'de> for NutritionPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Disabled {},
            Enabled {
                initial: u32,
                maximum: u32,
                hungry_below: u32,
                weak_below: u32,
                starve_at: i32,
                unit_ticks: u64,
                hungry_recovery: RecoveryNutrition,
                weak_recovery: RecoveryNutrition,
                empty_recovery: RecoveryNutrition,
                regeneration_cost: RegenerationCost,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Disabled {} => Self::Disabled,
            Fields::Enabled {
                initial,
                maximum,
                hungry_below,
                weak_below,
                starve_at,
                unit_ticks,
                hungry_recovery,
                weak_recovery,
                empty_recovery,
                regeneration_cost,
            } => Self::Enabled {
                initial,
                maximum,
                hungry_below,
                weak_below,
                starve_at,
                unit_ticks,
                hungry_recovery,
                weak_recovery,
                empty_recovery,
                regeneration_cost,
            },
        })
    }
}
impl NutritionPolicy {
    pub fn valid(self) -> bool {
        match self {
            Self::Disabled => true,
            Self::Enabled {
                initial,
                maximum,
                hungry_below,
                weak_below,
                starve_at,
                unit_ticks,
                hungry_recovery,
                weak_recovery,
                empty_recovery,
                regeneration_cost,
            } => {
                (1..=crate::limits::NUMERIC as u32).contains(&maximum)
                    && initial <= maximum
                    && i64::from(initial) > i64::from(starve_at)
                    && 0 < weak_below
                    && weak_below < hungry_below
                    && hungry_below <= maximum
                    && (-(crate::limits::NUMERIC as i32)..=0).contains(&starve_at)
                    && (1..=crate::limits::NUMERIC as u64).contains(&unit_ticks)
                    && hungry_recovery.valid()
                    && weak_recovery.valid()
                    && empty_recovery.valid()
                    && regeneration_cost.valid()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NutritionRequirement {
    Exempt,
    Required,
}
impl<'de> Deserialize<'de> for NutritionRequirement {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Exempt {},
            Required {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Exempt {} => Self::Exempt,
            Fields::Required {} => Self::Required,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Nutrition {
    Exempt,
    Required {
        reserve: i32,
        elapsed: u64,
        regeneration_remainder: u32,
    },
}
impl<'de> Deserialize<'de> for Nutrition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Exempt {},
            Required {
                reserve: i32,
                elapsed: u64,
                regeneration_remainder: u32,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Exempt {} => Self::Exempt,
            Fields::Required {
                reserve,
                elapsed,
                regeneration_remainder,
            } => Self::Required {
                reserve,
                elapsed,
                regeneration_remainder,
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hunger {
    Nourished,
    Hungry,
    Weak,
    Empty,
}

impl Nutrition {
    pub(crate) fn initial(
        requirement: NutritionRequirement,
        policy: NutritionPolicy,
    ) -> Option<Self> {
        match (requirement, policy) {
            (NutritionRequirement::Exempt, _) => Some(Self::Exempt),
            (NutritionRequirement::Required, NutritionPolicy::Enabled { initial, .. }) => {
                Some(Self::Required {
                    reserve: i32::try_from(initial).ok()?,
                    elapsed: 0,
                    regeneration_remainder: 0,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn valid(self, policy: NutritionPolicy, dead: bool) -> bool {
        match (self, policy) {
            (Self::Exempt, _) => true,
            (
                Self::Required {
                    reserve,
                    elapsed,
                    regeneration_remainder,
                },
                NutritionPolicy::Enabled {
                    maximum,
                    starve_at,
                    unit_ticks,
                    regeneration_cost,
                    ..
                },
            ) => {
                i64::from(reserve) <= i64::from(maximum)
                    && reserve >= starve_at
                    && (dead || reserve > starve_at)
                    && elapsed < unit_ticks
                    && (!dead || elapsed == 0)
                    && regeneration_remainder < regeneration_cost.denominator
                    && (!dead || regeneration_remainder == 0)
            }
            _ => false,
        }
    }

    pub fn hunger(self, policy: NutritionPolicy) -> Option<Hunger> {
        let (
            Self::Required { reserve, .. },
            NutritionPolicy::Enabled {
                hungry_below,
                weak_below,
                ..
            },
        ) = (self, policy)
        else {
            return None;
        };
        Some(if reserve <= 0 {
            Hunger::Empty
        } else if i64::from(reserve) < i64::from(weak_below) {
            Hunger::Weak
        } else if i64::from(reserve) < i64::from(hungry_below) {
            Hunger::Hungry
        } else {
            Hunger::Nourished
        })
    }

    pub(crate) fn recovery(self, policy: NutritionPolicy) -> Option<RecoveryNutrition> {
        let NutritionPolicy::Enabled {
            hungry_recovery,
            weak_recovery,
            empty_recovery,
            ..
        } = policy
        else {
            return None;
        };
        Some(match self.hunger(policy)? {
            Hunger::Nourished => RecoveryNutrition::Normal,
            Hunger::Hungry => hungry_recovery,
            Hunger::Weak => weak_recovery,
            Hunger::Empty => empty_recovery,
        })
    }

    /// Reserve arithmetic is lazy. Only a hunger-band change, empty warning,
    /// or death needs to wake the existing actor deadline index. At most four
    /// nutrition boundaries occur between eating actions, independent of ticks
    /// or reserve magnitude.
    pub(crate) fn next_change_in(self, policy: NutritionPolicy) -> Option<u64> {
        let (
            Self::Required {
                reserve, elapsed, ..
            },
            NutritionPolicy::Enabled {
                hungry_below,
                weak_below,
                starve_at,
                unit_ticks,
                ..
            },
        ) = (self, policy)
        else {
            return None;
        };
        let target = match self.hunger(policy)? {
            Hunger::Nourished => i64::from(hungry_below) - 1,
            Hunger::Hungry => i64::from(weak_below) - 1,
            Hunger::Weak => 0,
            Hunger::Empty => i64::from(starve_at),
        };
        let units = u64::try_from(i64::from(reserve).checked_sub(target)?).ok()?;
        units
            .checked_mul(unit_ticks)?
            .checked_sub(elapsed)
            .filter(|value| *value > 0)
    }

    /// The first stochastic check follows 0 -> -1, never 1 -> 0. Denied
    /// intervals are skipped by the creature caller; no owed-roll state exists.
    pub(crate) fn next_empty_depletion_in(self, policy: NutritionPolicy) -> Option<u64> {
        let (
            Self::Required {
                reserve, elapsed, ..
            },
            NutritionPolicy::Enabled { unit_ticks, .. },
        ) = (self, policy)
        else {
            return None;
        };
        let units = u64::try_from(i64::from(reserve).max(0) + 1).ok()?;
        units
            .checked_mul(unit_ticks)?
            .checked_sub(elapsed)
            .filter(|ticks| *ticks > 0)
    }

    pub(crate) fn at_empty_depletion(self) -> bool {
        matches!(self, Self::Required { reserve, elapsed: 0, .. } if reserve < 0)
    }

    /// Pure analytical settlement; true means the starvation deadline was met.
    /// The creature caller splits at policy changes to settle other clocks under
    /// their old denominator, but no per-point or per-tick loop exists here.
    pub(crate) fn advance_active(&mut self, policy: NutritionPolicy, ticks: u64) -> bool {
        self.advance_with_regeneration(policy, ticks, Regeneration::default())
    }

    /// The first combined crossing is located by the creature's bounded search.
    pub(crate) fn crosses_change(
        self,
        policy: NutritionPolicy,
        ticks: u64,
        gained: Regeneration,
    ) -> bool {
        let Self::Required { reserve, .. } = self else {
            return false;
        };
        let NutritionPolicy::Enabled {
            regeneration_cost, ..
        } = policy
        else {
            return false;
        };
        // Time-only crossings already have an exact analytical deadline. Full
        // pools and explicit zero prices need no additional boundary search.
        if regeneration_cost.numerator(gained) == 0 {
            return false;
        }
        let before = self.hunger(policy);
        let mut projected = self;
        projected.advance_with_regeneration(policy, ticks, gained)
            || (projected.hunger(policy) != before && reserve > 0)
    }

    pub(crate) fn advance_with_regeneration(
        &mut self,
        policy: NutritionPolicy,
        ticks: u64,
        gained: Regeneration,
    ) -> bool {
        let (
            Self::Required {
                reserve,
                elapsed,
                regeneration_remainder,
            },
            NutritionPolicy::Enabled {
                starve_at,
                unit_ticks,
                regeneration_cost,
                ..
            },
        ) = (self, policy)
        else {
            return false;
        };
        let total = u128::from(*elapsed) + u128::from(ticks);
        let regeneration = u64::from(*regeneration_remainder) + regeneration_cost.numerator(gained);
        let units = total / u128::from(unit_ticks)
            + u128::from(regeneration / u64::from(regeneration_cost.denominator));
        let until_death = (i64::from(*reserve) - i64::from(starve_at)) as u128;
        if units >= until_death {
            *reserve = starve_at;
            *elapsed = 0;
            *regeneration_remainder = 0;
            true
        } else {
            // The validated reserve envelope bounds this conversion by 2e6.
            *reserve = (i64::from(*reserve) - units as i64) as i32;
            *elapsed = (total % u128::from(unit_ticks)) as u64;
            *regeneration_remainder =
                (regeneration % u64::from(regeneration_cost.denominator)) as u32;
            false
        }
    }

    /// Uses widened addition, saturates only at the authored stomach capacity,
    /// and preserves the digestion fraction. Returns the nutrition actually added
    /// after clearing debt, so replaying the canonical resolved amount reaches
    /// the same capped reserve rather than adding the cleared debt a second time.
    pub(crate) fn nourish(&mut self, policy: NutritionPolicy, amount: u32) -> u32 {
        let (Self::Required { reserve, .. }, NutritionPolicy::Enabled { maximum, .. }) =
            (self, policy)
        else {
            return 0;
        };
        if amount == 0 {
            return 0;
        }
        let before = (*reserve).max(0) as u32;
        let next =
            (u64::from((*reserve).max(0) as u32) + u64::from(amount)).min(u64::from(maximum));
        *reserve = next as i32;
        *reserve as u32 - before
    }

    pub(crate) fn clear_dead_fraction(&mut self) {
        if let Self::Required {
            elapsed,
            regeneration_remainder,
            ..
        } = self
        {
            *elapsed = 0;
            *regeneration_remainder = 0;
        }
    }
}

/// Personal/privileged disclosure only. Ordinary visible actors never expose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NutritionView {
    Exempt,
    Required {
        reserve: i32,
        maximum: u32,
        hunger: Hunger,
        unit_ticks: u64,
        elapsed: u64,
        starve_at: i32,
    },
}
impl Nutrition {
    pub fn view(self, policy: NutritionPolicy) -> NutritionView {
        match (self, policy) {
            (
                Self::Required {
                    reserve, elapsed, ..
                },
                NutritionPolicy::Enabled {
                    maximum,
                    unit_ticks,
                    starve_at,
                    ..
                },
            ) => NutritionView::Required {
                reserve,
                maximum,
                unit_ticks,
                elapsed,
                starve_at,
                hunger: self
                    .hunger(policy)
                    .expect("required nutrition has an enabled policy"),
            },
            _ => NutritionView::Exempt,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> NutritionPolicy {
        NutritionPolicy::Enabled {
            initial: 8,
            maximum: 12,
            hungry_below: 7,
            weak_below: 3,
            starve_at: -3,
            unit_ticks: 100,
            hungry_recovery: RecoveryNutrition::Slowed { multiplier: 2 },
            weak_recovery: RecoveryNutrition::Stopped,
            empty_recovery: RecoveryNutrition::Stopped,
            regeneration_cost: crate::nutrition::RegenerationCost {
                denominator: 1,
                health: 0,
                stamina: 0,
                focus: 0,
                mana: 0,
            },
        }
    }
    #[test]
    fn analytical_settlement_is_bounded_even_for_maximum_tick_input() {
        let mut value = Nutrition::initial(NutritionRequirement::Required, policy()).unwrap();
        assert_eq!(value.next_change_in(policy()), Some(200));
        assert!(value.advance_active(policy(), u64::MAX));
        assert_eq!(
            value,
            Nutrition::Required {
                reserve: -3,
                elapsed: 0,
                regeneration_remainder: 0
            }
        );
    }
    #[test]
    fn canonical_nourishment_clears_debt_without_double_counting_and_preserves_fraction() {
        for reserve in [-2, 0, 8, 12] {
            let before = Nutrition::Required {
                reserve,
                elapsed: 57,
                regeneration_remainder: 0,
            };
            let mut preview = before;
            let resolved = preview.nourish(policy(), 6);
            let mut published = before;
            published.nourish(policy(), resolved);
            assert_eq!(preview, published);
            assert_eq!(
                preview,
                Nutrition::Required {
                    reserve: (reserve.max(0) + 6).min(12),
                    elapsed: 57,
                    regeneration_remainder: 0
                }
            );
        }
    }
    #[test]
    fn all_nutrition_records_require_named_exact_shapes() {
        for raw in [
            r#"{"kind":"disabled","extra":0}"#,
            "[]",
            r#"{"kind":"enabled"}"#,
        ] {
            assert!(serde_json::from_str::<NutritionPolicy>(raw).is_err());
        }
        for raw in [
            r#"{"kind":"exempt","reserve":0}"#,
            "[]",
            r#"{"kind":"required","reserve":2}"#,
        ] {
            assert!(serde_json::from_str::<Nutrition>(raw).is_err());
        }
        for raw in [r#"{"kind":"exempt","extra":0}"#, "[]"] {
            assert!(serde_json::from_str::<NutritionRequirement>(raw).is_err());
        }
        for raw in ["[6]", r#"{"nutrition":6,"extra":0}"#, "{}"] {
            assert!(serde_json::from_str::<crate::EdibleSpec>(raw).is_err());
        }
    }
}
