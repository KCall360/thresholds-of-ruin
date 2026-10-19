//! Authored shared equipment resources. Current deficits are derived; they are
//! never trusted saved authority or a reason to discard equipped items.
use crate::content_ids::EquipmentCapacityId;
use crate::EquipmentSlotId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrimaryAttackSocket {
    #[default]
    None,
    Socket {
        slot: EquipmentSlotId,
    },
}
impl PrimaryAttackSocket {
    pub fn socket(&self) -> Option<EquipmentSlotId> {
        match self {
            Self::None => None,
            Self::Socket { slot } => Some(*slot),
        }
    }
}
impl<'de> Deserialize<'de> for PrimaryAttackSocket {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            None {},
            Socket { slot: EquipmentSlotId },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::None {} => Self::None,
            Fields::Socket { slot } if usize::from(slot.0) < crate::limits::EQUIPMENT_SOCKETS => {
                Self::Socket { slot }
            }
            Fields::Socket { .. } => {
                return Err(serde::de::Error::custom("invalid primary socket"))
            }
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EquipmentCapacityUse {
    #[default]
    None,
    Shared {
        resource: EquipmentCapacityId,
        units: u16,
    },
}
impl EquipmentCapacityUse {
    pub fn valid(&self) -> bool {
        match self {
            Self::None => true,
            Self::Shared { units, .. } => {
                (1..=crate::limits::EQUIPMENT_SOCKETS).contains(&usize::from(*units))
            }
        }
    }
    pub fn resource(&self) -> Option<&EquipmentCapacityId> {
        match self {
            Self::None => None,
            Self::Shared { resource, .. } => Some(resource),
        }
    }
}
impl<'de> Deserialize<'de> for EquipmentCapacityUse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            None {},
            Shared {
                resource: EquipmentCapacityId,
                units: u16,
            },
        }
        let value = match crate::bounded::named_map(d)? {
            Fields::None {} => Self::None,
            Fields::Shared { resource, units } => Self::Shared { resource, units },
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid equipment capacity use"));
        }
        Ok(value)
    }
}

/// Bounded unique authored resource capacities, with no implicit humanoid pool.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct EquipmentCapacities(BTreeMap<EquipmentCapacityId, u16>);
impl EquipmentCapacities {
    pub fn new(values: BTreeMap<EquipmentCapacityId, u16>) -> Option<Self> {
        (values.len() <= crate::limits::EQUIPMENT_SOCKETS
            && values
                .values()
                .all(|units| usize::from(*units) <= crate::limits::EQUIPMENT_SOCKETS))
        .then_some(Self(values))
    }
    pub fn get(&self, id: &EquipmentCapacityId) -> Option<u16> {
        self.0.get(id).copied()
    }
    pub(crate) fn intern(&mut self, pool: &mut crate::content_ids::Interner) {
        self.0 = std::mem::take(&mut self.0)
            .into_iter()
            .map(|(mut id, units)| {
                id.intern(pool);
                (id, units)
            })
            .collect();
    }
    pub fn entries(&self) -> impl Iterator<Item = (&EquipmentCapacityId, u16)> {
        self.0.iter().map(|(id, units)| (id, *units))
    }
}
impl<'de> Deserialize<'de> for EquipmentCapacities {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Map;
        impl<'de> serde::de::Visitor<'de> for Map {
            type Value = EquipmentCapacities;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded unique equipment capacities")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((id, units)) = map.next_entry::<EquipmentCapacityId, u16>()? {
                    if values.len() >= crate::limits::EQUIPMENT_SOCKETS
                        || usize::from(units) > crate::limits::EQUIPMENT_SOCKETS
                        || values.insert(id, units).is_some()
                    {
                        return Err(serde::de::Error::custom("invalid equipment capacities"));
                    }
                }
                Ok(EquipmentCapacities(values))
            }
        }
        d.deserialize_map(Map)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EquipmentCapacityDeficit {
    pub available: u16,
    pub required: u32,
}
impl EquipmentCapacityDeficit {
    pub fn missing(self) -> u32 {
        self.required.saturating_sub(u32::from(self.available))
    }
}

/// Minimum demands prove structural feasibility only. They do not prove that a
/// reducing expertise was historically held; actual chronology belongs to replay.
pub fn capacity_deficits(
    capacities: &EquipmentCapacities,
    demands: impl IntoIterator<Item = (EquipmentCapacityId, u16, u16)>,
) -> Option<BTreeMap<EquipmentCapacityId, EquipmentCapacityDeficit>> {
    let mut totals = BTreeMap::<EquipmentCapacityId, (u32, u32)>::new();
    for (index, (resource, minimum, current)) in demands.into_iter().enumerate() {
        if index >= crate::limits::EQUIPMENT_SOCKETS
            || minimum == 0
            || minimum > current
            || capacities.get(&resource).is_none()
        {
            return None;
        }
        let total = totals.entry(resource).or_default();
        total.0 = total.0.checked_add(u32::from(minimum))?;
        total.1 = total.1.checked_add(u32::from(current))?;
    }
    let mut deficits = BTreeMap::new();
    for (resource, (minimum, current)) in totals {
        let available = capacities.get(&resource)?;
        if minimum > u32::from(available) {
            return None;
        }
        if current > u32::from(available) {
            deficits.insert(
                resource,
                EquipmentCapacityDeficit {
                    available,
                    required: current,
                },
            );
        }
    }
    Some(deficits)
}

#[cfg(test)]
#[path = "equipment_capacity_tests.rs"]
mod tests;
