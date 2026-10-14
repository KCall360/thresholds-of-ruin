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

    pub(crate) fn numerator(self, gained: Regeneration) -> u64 {
        // Four bounded products fit u64 (at most 4 * 10^12).
        u64::from(self.health) * u64::from(gained.health)
            + u64::from(self.stamina) * u64::from(gained.resources.stamina)
            + u64::from(self.focus) * u64::from(gained.resources.focus)
            + u64::from(self.mana) * u64::from(gained.resources.mana)
    }
}

/// Gross actual periodic restoration, before any simultaneous capacity change.
/// Transient backend diagnostics only: neither a save ledger nor public protocol.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regeneration {
    pub health: u32,
    pub resources: ByResource<u32>,
}
impl Regeneration {
    pub(crate) fn add(&mut self, other: Self) {
        // No spending occurs inside an active interval. At most CONDITIONS
        // expiries can change a bounded pool capacity, so all gross gains fit
        // (CONDITIONS + 1) * NUMERIC, well below u32::MAX.
        self.health += other.health;
        for resource in Resource::ALL {
            *self.resources.get_mut(resource) += *other.resources.get(resource);
        }
    }
}
