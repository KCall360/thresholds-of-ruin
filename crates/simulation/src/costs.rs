//! Preparation-owned reservations. Queued intentions do not pay or reserve;
//! execution starts a hold, resume retains it, completion pays the remainder.
//! Completed intention receipts own retry idempotence after a hold is removed.

use crate::resources::{Resource, ResourceMaxima, Resources};
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
            .filter(|value| *value <= 1_000_000)
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
    held: BTreeMap<IntentionId, ResourceCost>,
}

impl CostLedger {
    /// Restore reservations after their start payment. Calling `start` here
    /// would charge twice; validate IDs, costs and funding without spending.
    pub fn from_recorded(
        resources: Resources,
        reservations: Vec<(IntentionId, ResourceCost)>,
    ) -> Result<Self, CostError> {
        if reservations.len() > 64 {
            return Err(CostError::Limit);
        }
        let mut ledger = Self::new(resources);
        for (intention, cost) in reservations {
            if intention.0 == 0 || intention.0 == u64::MAX || cost.total().is_none() {
                return Err(CostError::Invalid);
            }
            if ledger.held.insert(intention, cost).is_some() {
                return Err(CostError::Conflict);
            }
        }
        if Resource::ALL
            .into_iter()
            .any(|resource| ledger.reserved(resource) > ledger.resources.balance(resource))
        {
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

    pub fn reservation(&self, intention: IntentionId) -> Option<ResourceCost> {
        self.held.get(&intention).copied()
    }

    pub fn reservations(&self) -> &BTreeMap<IntentionId, ResourceCost> {
        &self.held
    }

    fn reserved(&self, resource: Resource) -> u32 {
        self.held
            .values()
            .filter(|cost| cost.resource == resource)
            .map(|cost| cost.resolution)
            .sum()
    }

    pub fn available(&self, resource: Resource) -> u32 {
        self.resources.balance(resource) - self.reserved(resource)
    }

    /// Read-only gate for atomic action preparation. Committing must follow in
    /// the same uninterrupted mutation boundary; this is not a saved promise.
    pub fn validate_start(
        &self,
        intention: IntentionId,
        cost: ResourceCost,
    ) -> Result<StartCost, CostError> {
        if intention.0 == 0 || intention.0 == u64::MAX {
            return Err(CostError::Invalid);
        }
        let total = cost.total().ok_or(CostError::Invalid)?;
        if let Some(existing) = self.held.get(&intention) {
            return if *existing == cost {
                Ok(StartCost::Resumed)
            } else {
                Err(CostError::Conflict)
            };
        }
        if self.held.len() >= 64 {
            return Err(CostError::Limit);
        }
        if self.available(cost.resource) < total {
            return Err(CostError::Insufficient);
        }
        Ok(StartCost::Started)
    }

    pub fn start(
        &mut self,
        intention: IntentionId,
        cost: ResourceCost,
    ) -> Result<StartCost, CostError> {
        let result = self.validate_start(intention, cost)?;
        if result == StartCost::Started {
            assert!(
                self.resources.spend(cost.resource, cost.start),
                "validated start cost"
            );
            self.held.insert(intention, cost);
        }
        Ok(result)
    }

    /// Hits, misses and resisted effects all pay this execution cost. The
    /// scheduler must resolve completion once under the intention's lineage.
    pub fn finish(&mut self, intention: IntentionId) -> Result<ResourceCost, CostError> {
        let cost = self
            .held
            .get(&intention)
            .copied()
            .ok_or(CostError::Unknown)?;
        if !self.resources.spend(cost.resource, cost.resolution) {
            return Err(CostError::Insufficient);
        }
        self.held.remove(&intention);
        Ok(cost)
    }

    /// Only the unpaid portion becomes available again. No paid cost is refunded.
    pub fn cancel(&mut self, intention: IntentionId) -> Option<ResourceCost> {
        self.held.remove(&intention)
    }

    /// Preserve balances, then cancel newest intention IDs in each pool until
    /// remaining holds are funded. The engine cancels the returned preparations.
    /// Existing earlier reservations retain priority when capacity becomes scarce.
    pub fn set_maxima(&mut self, maxima: ResourceMaxima) -> Vec<IntentionId> {
        self.resources.set_maxima(maxima);
        let mut canceled = vec![];
        for resource in Resource::ALL {
            let mut reserved = self.reserved(resource);
            let balance = self.resources.balance(resource);
            if reserved <= balance {
                continue;
            }
            let newest: Vec<_> = self
                .held
                .iter()
                .rev()
                .filter(|(_, cost)| cost.resource == resource)
                .map(|(&intention, _)| intention)
                .collect();
            for intention in newest {
                if reserved <= balance {
                    break;
                }
                let cost = self
                    .held
                    .remove(&intention)
                    .expect("reservation selected from this ledger");
                reserved -= cost.resolution;
                canceled.push(intention);
            }
        }
        canceled
    }

    pub fn advance_active(&mut self, ticks: u64) {
        self.resources.advance_active(ticks);
    }
}
