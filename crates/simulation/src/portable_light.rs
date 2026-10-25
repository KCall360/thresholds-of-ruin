//! Item-owned finite fuel and physical portable illumination.
//! Ambient World lighting and item identity remain independent authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeldLightSpec {
    pub radius: u8,
    pub fuel_ticks: u64,
}
impl HeldLightSpec {
    pub fn valid(self) -> bool {
        (1..=crate::limits::SIGHT_RANGE as u8).contains(&self.radius)
            && (1..=crate::limits::NUMERIC as u64).contains(&self.fuel_ticks)
    }
}
impl<'de> Deserialize<'de> for HeldLightSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields { radius: u8, fuel_ticks: u64 }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self { radius: fields.radius, fuel_ticks: fields.fuel_ticks };
        if !value.valid() { return Err(serde::de::Error::custom("invalid held light policy")); }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FuelState {
    pub remaining: u64,
    pub lit: bool,
}
impl<'de> Deserialize<'de> for FuelState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields { remaining: u64, lit: bool }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.remaining > crate::limits::NUMERIC as u64 || (fields.lit && fields.remaining == 0) {
            return Err(serde::de::Error::custom("invalid portable fuel"));
        }
        Ok(Self { remaining: fields.remaining, lit: fields.lit })
    }
}

/// Controlled inventory projection; no source radius or concealed identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortableLightView {
    pub remaining: u64,
    pub lit: bool,
    pub ignitable: bool,
    pub extinguishable: bool,
}

pub(crate) fn initial_fuel(spec: &crate::ItemSpec) -> Option<FuelState> {
    spec.held_light.map(|policy| FuelState { remaining: policy.fuel_ticks, lit: false })
}
pub(crate) fn valid_item(item: &crate::Item) -> bool {
    match (item.spec.held_light, item.fuel) {
        (None, None) => true,
        (Some(policy), Some(fuel)) => policy.valid() && item.quantity == 1
            && fuel.remaining <= policy.fuel_ticks && (!fuel.lit || fuel.remaining > 0),
        _ => false,
    }
}
