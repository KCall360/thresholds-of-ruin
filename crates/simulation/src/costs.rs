//! Preparation-owned reservations. Queued intentions do not pay or reserve;
//! execution starts a hold, resume retains it, completion pays the remainder.
//! Completed intention receipts own retry idempotence after a hold is removed.

use crate::resources::{ByResource, Resource, ResourceMaxima, Resources};
use crate::IntentionId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCost {
    pub resource: Resource,
    pub start: u32,
    pub resolution: u32,
}

impl ResourceCost {
    fn total(self) -> Option<u32> {
        self.start
            .checked_add(self.resolution)
            .filter(|value| i64::from(*value) <= crate::limits::NUMERIC)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
struct CostPhases {
    start: u32,
    resolution: u32,
}

impl<'de> serde::Deserialize<'de> for CostPhases {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            start: u32,
            resolution: u32,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            start: fields.start,
            resolution: fields.resolution,
        })
    }
}

/// One immutable, nonempty reservation across the fixed resource catalog.
/// A present zero cost still identifies a paid preparation and occupies a hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct CostBundle(ByResource<Option<CostPhases>>);

impl TryFrom<&crate::rules::Cost> for CostBundle {
    type Error = CostError;
    fn try_from(cost: &crate::rules::Cost) -> Result<Self, Self::Error> {
        Self::single(ResourceCost {
            resource: cost.resource,
            start: cost.start,
            resolution: cost.resolution,
        })
    }
}

impl CostBundle {
    /// Capture one authored scalar cost without changing its payment phases.
    pub fn single(cost: ResourceCost) -> Result<Self, CostError> {
        cost.total().ok_or(CostError::Invalid)?;
        let mut phases = ByResource::from_fn(|_| None);
        *phases.get_mut(cost.resource) = Some(CostPhases {
            start: cost.start,
            resolution: cost.resolution,
        });
        Ok(Self(phases))
    }

    /// Combine costs by resource and phase, rejecting arithmetic or domain overflow.
    /// Neither input changes when the combined cost exceeds a resource's bound.
    pub fn checked_add(self, other: Self) -> Result<Self, CostError> {
        let mut combined = self;
        for cost in other.entries() {
            let current = combined.0.get_mut(cost.resource);
            let phases = match *current {
                Some(existing) => CostPhases {
                    start: existing
                        .start
                        .checked_add(cost.start)
                        .ok_or(CostError::Invalid)?,
                    resolution: existing
                        .resolution
                        .checked_add(cost.resolution)
                        .ok_or(CostError::Invalid)?,
                },
                None => CostPhases {
                    start: cost.start,
                    resolution: cost.resolution,
                },
            };
            ResourceCost {
                resource: cost.resource,
                start: phases.start,
                resolution: phases.resolution,
            }
            .total()
            .ok_or(CostError::Invalid)?;
            *current = Some(phases);
        }
        Ok(combined)
    }

    /// Present scalar entries in `Resource::ALL` order, including explicit zeros.
    pub fn entries(self) -> impl Iterator<Item = ResourceCost> {
        Resource::ALL.into_iter().filter_map(move |resource| {
            self.0.get(resource).map(|phases| ResourceCost {
                resource,
                start: phases.start,
                resolution: phases.resolution,
            })
        })
    }

    fn validate(self) -> Result<(), CostError> {
        let mut present = false;
        for cost in self.entries() {
            cost.total().ok_or(CostError::Invalid)?;
            present = true;
        }
        if present {
            Ok(())
        } else {
            Err(CostError::Invalid)
        }
    }
}

impl<'de> serde::Deserialize<'de> for CostBundle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            stamina: Option<CostPhases>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            focus: Option<CostPhases>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            mana: Option<CostPhases>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let bundle = Self(ByResource {
            stamina: fields.stamina,
            focus: fields.focus,
            mana: fields.mana,
        });
        bundle
            .validate()
            .map_err(|_| serde::de::Error::custom("invalid resource cost bundle"))?;
        Ok(bundle)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostError {
    Invalid,
    Insufficient,
    Conflict,
    Unknown,
    Limit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartCost {
    Started,
    Resumed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostLedger {
    resources: Resources,
    held: BTreeMap<IntentionId, CostBundle>,
}

impl CostLedger {
    /// Restore reservations after their start payment. Calling `start` here
    /// would charge twice; validate IDs, costs and funding without spending.
    pub fn from_recorded(
        resources: Resources,
        reservations: Vec<(IntentionId, CostBundle)>,
    ) -> Result<Self, CostError> {
        if reservations.len() > crate::limits::COST_HOLDS {
            return Err(CostError::Limit);
        }
        let mut ledger = Self::new(resources);
        for (intention, cost) in reservations {
            if intention.0 == 0 || intention.0 == u64::MAX {
                return Err(CostError::Invalid);
            }
            cost.validate()?;
            if ledger.held.insert(intention, cost).is_some() {
                return Err(CostError::Conflict);
            }
        }
        if Resource::ALL.into_iter().any(|resource| {
            ledger.reserved(resource) > u64::from(ledger.resources.balance(resource))
        }) {
            return Err(CostError::Insufficient);
        }
        Ok(ledger)
    }

    pub fn new(resources: Resources) -> Self {
        Self {
            resources,
            held: BTreeMap::new(),
        }
    }

    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    pub fn reservation(&self, intention: IntentionId) -> Option<CostBundle> {
        self.held.get(&intention).copied()
    }

    pub fn reservations(&self) -> &BTreeMap<IntentionId, CostBundle> {
        &self.held
    }

    fn reserved(&self, resource: Resource) -> u64 {
        // Widen before summing: restored holds have not passed the funding check.
        // COST_HOLDS bounds the number of individually bounded u32 entries.
        self.held
            .values()
            .filter_map(|cost| *cost.0.get(resource))
            .map(|phases| u64::from(phases.resolution))
            .sum()
    }

    pub fn available(&self, resource: Resource) -> u32 {
        // Every published ledger funds its complete holds, so the difference
        // lies between zero and the original u32 balance.
        (u64::from(self.resources.balance(resource)) - self.reserved(resource)) as u32
    }

    /// Read-only gate for atomic action preparation. Committing must follow in
    /// the same uninterrupted mutation boundary; this is not a saved promise.
    pub fn validate_start(
        &self,
        intention: IntentionId,
        cost: CostBundle,
    ) -> Result<StartCost, CostError> {
        if intention.0 == 0 || intention.0 == u64::MAX {
            return Err(CostError::Invalid);
        }
        cost.validate()?;
        if let Some(existing) = self.held.get(&intention) {
            return if *existing == cost {
                Ok(StartCost::Resumed)
            } else {
                Err(CostError::Conflict)
            };
        }
        if self.held.len() >= crate::limits::COST_HOLDS {
            return Err(CostError::Limit);
        }
        for entry in cost.entries() {
            let total = entry.total().ok_or(CostError::Invalid)?;
            if self.available(entry.resource) < total {
                return Err(CostError::Insufficient);
            }
        }
        Ok(StartCost::Started)
    }

    pub fn start(
        &mut self,
        intention: IntentionId,
        cost: CostBundle,
    ) -> Result<StartCost, CostError> {
        let result = self.validate_start(intention, cost)?;
        if result == StartCost::Started {
            // All pools were checked before the first payment or hold mutation.
            for entry in cost.entries() {
                assert!(
                    self.resources.spend(entry.resource, entry.start),
                    "validated start cost"
                );
            }
            self.held.insert(intention, cost);
        }
        Ok(result)
    }

    /// Hits, misses and resisted effects all pay this execution cost. The
    /// scheduler must resolve completion once under the intention's lineage.
    pub fn finish(&mut self, intention: IntentionId) -> Result<CostBundle, CostError> {
        let cost = self
            .held
            .get(&intention)
            .copied()
            .ok_or(CostError::Unknown)?;
        if cost
            .entries()
            .any(|entry| self.resources.balance(entry.resource) < entry.resolution)
        {
            return Err(CostError::Insufficient);
        }
        for entry in cost.entries() {
            assert!(
                self.resources.spend(entry.resource, entry.resolution),
                "validated resolution cost"
            );
        }
        self.held.remove(&intention);
        Ok(cost)
    }

    /// Only the unpaid portion becomes available again. No paid cost is refunded.
    pub fn cancel(&mut self, intention: IntentionId) -> Option<CostBundle> {
        self.held.remove(&intention)
    }

    /// Preserve balances, then cancel newest intention IDs in each pool until
    /// remaining holds are funded. Cancellation releases the whole bundle once;
    /// each later pool recomputes its total after those releases. The engine
    /// cancels the returned preparations, retaining earlier reservations first.
    pub fn set_maxima(&mut self, maxima: ResourceMaxima) -> Vec<IntentionId> {
        self.resources.set_maxima(maxima);
        let mut canceled = vec![];
        for resource in Resource::ALL {
            let mut reserved = self.reserved(resource);
            let balance = u64::from(self.resources.balance(resource));
            if reserved <= balance {
                continue;
            }
            let newest: Vec<_> = self
                .held
                .iter()
                .rev()
                .filter_map(|(&intention, cost)| {
                    cost.0
                        .get(resource)
                        .filter(|phases| phases.resolution > 0)
                        .map(|phases| (intention, phases.resolution))
                })
                .collect();
            for (intention, resolution) in newest {
                if reserved <= balance {
                    break;
                }
                self.held.remove(&intention);
                reserved -= u64::from(resolution);
                canceled.push(intention);
            }
        }
        canceled
    }

    pub fn restore_resource(&mut self, resource: Resource, amount: u32) -> u32 {
        self.resources.restore(resource, amount)
    }
    pub fn set_recovery_rules(&mut self, rules: &crate::rules::Ruleset) {
        self.resources.set_recovery_rules(rules);
    }
    pub fn advance_active(&mut self, ticks: u64) {
        self.resources.advance_active(ticks);
    }
}

#[cfg(test)]
#[path = "costs_tests.rs"]
mod tests;
