//! Capacity-preserving resource balances and deterministic active-time recovery.

use crate::attributes::{Attribute, Attributes, ManaBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    InvalidState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Stamina,
    Focus,
    Mana,
}

impl Resource {
    pub const ALL: [Self; 3] = [Self::Stamina, Self::Focus, Self::Mana];

    fn recovery_period(self) -> u64 {
        match self {
            Self::Stamina => 100,
            Self::Focus => 300,
            Self::Mana => 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceMaxima {
    pub stamina: u32,
    pub focus: u32,
    pub mana: u32,
}

impl ResourceMaxima {
    /// Build derivation supplies magical capability; owning a binding alone
    /// does not grant Mana. Additional source-owned capacity grants apply later.
    pub fn derived(attributes: Attributes, binding: ManaBinding, magical: bool) -> Self {
        Self {
            stamina: 2 + u32::from(attributes.get(Attribute::Strength)),
            focus: 2 + u32::from(attributes.get(Attribute::Willpower)),
            mana: if magical {
                2 + u32::from(attributes.get(binding.attribute()))
            } else {
                0
            },
        }
    }

    fn values(self) -> [u32; 3] {
        [self.stamina, self.focus, self.mana]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pool {
    maximum: u32,
    balance: u32,
    remainder: u64,
}

impl Pool {
    fn new(maximum: u32) -> Self {
        Self {
            maximum,
            balance: maximum,
            remainder: 0,
        }
    }

    fn set_maximum(&mut self, maximum: u32) {
        self.maximum = maximum;
        self.balance = self.balance.min(maximum);
        if self.balance == maximum {
            self.remainder = 0;
        }
    }

    fn recover(&mut self, ticks: u64, period: u64) {
        let capacity = self.maximum - self.balance;
        if capacity == 0 {
            self.remainder = 0;
            return;
        }
        let elapsed = u128::from(self.remainder) + u128::from(ticks);
        let gained = (elapsed / u128::from(period)).min(u128::from(capacity)) as u32;
        self.balance += gained;
        self.remainder = if self.balance == self.maximum {
            0
        } else {
            (elapsed % u128::from(period)) as u64
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resources {
    pools: [Pool; 3],
}

impl Resources {
    /// Capacity is freshly derived; checkpoints contain only balances and
    /// fractional active recovery. Invalid state rejects rather than clamps.
    pub fn from_recorded(
        maxima: ResourceMaxima,
        balances: [u32; 3],
        elapsed: [u64; 3],
    ) -> Result<Self, ResourceError> {
        let mut resources = Self::new(maxima);
        for resource in Resource::ALL {
            let index = resource as usize;
            let pool = &mut resources.pools[index];
            if balances[index] > pool.maximum
                || elapsed[index] >= resource.recovery_period()
                || (balances[index] == pool.maximum && elapsed[index] != 0)
            {
                return Err(ResourceError::InvalidState);
            }
            pool.balance = balances[index];
            pool.remainder = elapsed[index];
        }
        Ok(resources)
    }

    pub fn new(maxima: ResourceMaxima) -> Self {
        Self {
            pools: maxima.values().map(Pool::new),
        }
    }

    pub fn balance(&self, resource: Resource) -> u32 {
        self.pools[resource as usize].balance
    }

    pub fn maximum(&self, resource: Resource) -> u32 {
        self.pools[resource as usize].maximum
    }

    pub fn recovery_elapsed(&self, resource: Resource) -> u64 {
        self.pools[resource as usize].remainder
    }

    /// Active ticks until the next balance change. Full pools have no event;
    /// partially recovered pools retain their phase when scheduling resumes.
    pub fn next_recovery_in(&self) -> Option<u64> {
        Resource::ALL
            .into_iter()
            .filter_map(|resource| {
                let pool = self.pools[resource as usize];
                (pool.balance < pool.maximum).then(|| resource.recovery_period() - pool.remainder)
            })
            .min()
    }

    pub fn set_maxima(&mut self, maxima: ResourceMaxima) {
        for (pool, maximum) in self.pools.iter_mut().zip(maxima.values()) {
            pool.set_maximum(maximum);
        }
    }

    /// Direct spending primitive. Action execution additionally owns reservations
    /// and paid-cost history; queue admission must not invoke this method.
    pub fn spend(&mut self, resource: Resource, amount: u32) -> bool {
        let pool = &mut self.pools[resource as usize];
        if amount > pool.balance {
            return false;
        }
        pool.balance -= amount;
        true
    }

    /// Pass only elapsed active simulation ticks. Frozen regions do not call
    /// this method and receive no catch-up recovery on reactivation.
    pub fn advance_active(&mut self, ticks: u64) {
        for resource in Resource::ALL {
            self.pools[resource as usize].recover(ticks, resource.recovery_period());
        }
    }
}
