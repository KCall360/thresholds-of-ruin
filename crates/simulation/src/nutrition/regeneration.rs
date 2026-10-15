//! Exact authored nutrition rates and transient gross periodic-gain diagnostics.
use crate::resources::{ByResource, Resource};

/// Numerators share one denominator, so gains from different pools contribute
/// to the same exact fractional nutrition debt. Zero is an explicit free rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct RegenerationCost {
    pub denominator: u32,
    pub health: u32,
    pub stamina: u32,
    pub focus: u32,
    pub mana: u32,
}
impl<'de> serde::Deserialize<'de> for RegenerationCost {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            denominator: u32,
            health: u32,
            stamina: u32,
            focus: u32,
            mana: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            denominator: fields.denominator,
            health: fields.health,
            stamina: fields.stamina,
            focus: fields.focus,
            mana: fields.mana,
        })
    }
}
impl RegenerationCost {
    pub(crate) fn valid(self) -> bool {
        (1..=crate::limits::NUMERIC as u32).contains(&self.denominator)
            && [self.health, self.stamina, self.focus, self.mana]
                .into_iter()
                .all(|value| value <= crate::limits::NUMERIC as u32)
    }

    pub(crate) fn numerator(self, gained: Regeneration) -> u128 {
        // Each Health gain is bounded by u64 injury, each resource remains
        // bounded, and at most CONDITIONS expiries split their capacities.
        // Gross gains times the authored rates therefore fit u128 exactly.
        [
            (gained.health, self.health),
            (u128::from(gained.resources.stamina), self.stamina),
            (u128::from(gained.resources.focus), self.focus),
            (u128::from(gained.resources.mana), self.mana),
        ]
        .into_iter()
        .try_fold(0_u128, |total, (amount, rate)| {
            total.checked_add(amount.checked_mul(u128::from(rate))?)
        })
        .expect("bounded periodic gains and authored rates fit a u128 numerator")
    }
}

/// Gross actual periodic restoration, before any simultaneous capacity change.
/// Transient backend diagnostics only: neither a save ledger nor public protocol.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regeneration {
    pub health: u128,
    pub resources: ByResource<u32>,
}
impl Regeneration {
    pub(crate) fn add(&mut self, other: Self) {
        self.health = self
            .health
            .checked_add(other.health)
            .expect("bounded active-interval Health recovery fits u128");
        // No spending occurs inside an active interval. At most CONDITIONS
        // expiries can change a resource capacity, so its gross gains fit
        // (CONDITIONS + 1) * NUMERIC, well below u32::MAX.
        for resource in Resource::ALL {
            let current = self.resources.get_mut(resource);
            *current = current
                .checked_add(*other.resources.get(resource))
                .expect("bounded active-interval resource recovery fits u32");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_scalar_gross_health_and_nutrition_products_remain_exact() {
        let gain = Regeneration {
            health: u128::from(u64::MAX),
            resources: ByResource {
                stamina: 1_000_000,
                focus: 1_000_000,
                mana: 1_000_000,
            },
        };
        let mut gross = gain;
        gross.add(gain);
        assert_eq!(gross.health, 2 * u128::from(u64::MAX));
        let cost = RegenerationCost {
            denominator: 1_000_000,
            health: 1_000_000,
            stamina: 1_000_000,
            focus: 1_000_000,
            mana: 1_000_000,
        };
        assert!(cost.valid());
        assert_eq!(
            cost.numerator(gross),
            2 * u128::from(u64::MAX) * 1_000_000 + 6_000_000_000_000
        );
    }
}
