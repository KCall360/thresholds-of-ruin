//! Capacity-preserving resource balances and deterministic active-time recovery.

use crate::rules::Ruleset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceError {
    InvalidState,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    Stamina,
    Focus,
    Mana,
}

impl Resource {
    pub const ALL: [Self; 3] = [Self::Stamina, Self::Focus, Self::Mana];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ByResource<T> {
    pub stamina: T,
    pub focus: T,
    pub mana: T,
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for ByResource<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields<T> {
            stamina: T,
            focus: T,
            mana: T,
        }
        let fields: Fields<T> = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            stamina: fields.stamina,
            focus: fields.focus,
            mana: fields.mana,
        })
    }
}
impl<T> ByResource<T> {
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> ByResource<U> {
        ByResource {
            stamina: f(self.stamina),
            focus: f(self.focus),
            mana: f(self.mana),
        }
    }

    pub fn from_fn(mut f: impl FnMut(Resource) -> T) -> Self {
        Self {
            stamina: f(Resource::Stamina),
            focus: f(Resource::Focus),
            mana: f(Resource::Mana),
        }
    }
    pub fn get(&self, resource: Resource) -> &T {
        match resource {
            Resource::Stamina => &self.stamina,
            Resource::Focus => &self.focus,
            Resource::Mana => &self.mana,
        }
    }
    pub fn get_mut(&mut self, resource: Resource) -> &mut T {
        match resource {
            Resource::Stamina => &mut self.stamina,
            Resource::Focus => &mut self.focus,
            Resource::Mana => &mut self.mana,
        }
    }
}
pub type ResourceMaxima = ByResource<u32>;

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
    pools: ByResource<Pool>,
    periods: ByResource<u64>,
}

impl Resources {
    /// Capacity is freshly derived; checkpoints contain only balances and
    /// fractional active recovery. Invalid state rejects rather than clamps.
    pub fn from_recorded(
        maxima: ResourceMaxima,
        balances: ByResource<u32>,
        elapsed: ByResource<u64>,
        rules: &Ruleset,
    ) -> Result<Self, ResourceError> {
        let mut resources = Self::new(maxima, rules);
        for resource in Resource::ALL {
            let pool = resources.pools.get_mut(resource);
            if *balances.get(resource) > pool.maximum
                || *elapsed.get(resource) >= *resources.periods.get(resource)
                || (*balances.get(resource) == pool.maximum && *elapsed.get(resource) != 0)
            {
                return Err(ResourceError::InvalidState);
            }
            pool.balance = *balances.get(resource);
            pool.remainder = *elapsed.get(resource);
        }
        Ok(resources)
    }

    pub fn new(maxima: ResourceMaxima, rules: &Ruleset) -> Self {
        let mut periods = ByResource {
            stamina: 1,
            focus: 1,
            mana: 1,
        };
        for definition in &rules.document().resources {
            *periods.get_mut(definition.resource) = definition.recovery_period;
        }
        Self {
            pools: ByResource::from_fn(|resource| Pool::new(*maxima.get(resource))),
            periods,
        }
    }
    pub fn set_recovery_rules(&mut self, rules: &Ruleset) {
        for definition in &rules.document().resources {
            *self.periods.get_mut(definition.resource) = definition.recovery_period;
            self.pools.get_mut(definition.resource).remainder %= definition.recovery_period;
        }
    }
    pub fn restore(&mut self, resource: Resource, amount: u32) -> u32 {
        let pool = self.pools.get_mut(resource);
        let restored = amount.min(pool.maximum - pool.balance);
        pool.balance += restored;
        if pool.balance == pool.maximum {
            pool.remainder = 0;
        }
        restored
    }

    pub fn balance(&self, resource: Resource) -> u32 {
        self.pools.get(resource).balance
    }

    pub fn maximum(&self, resource: Resource) -> u32 {
        self.pools.get(resource).maximum
    }

    pub fn recovery_elapsed(&self, resource: Resource) -> u64 {
        self.pools.get(resource).remainder
    }

    /// Active ticks until the next balance change. Full pools have no event;
    /// partially recovered pools retain their phase when scheduling resumes.
    pub fn next_recovery_in(&self) -> Option<u64> {
        Resource::ALL
            .into_iter()
            .filter_map(|resource| {
                let pool = *self.pools.get(resource);
                (pool.balance < pool.maximum).then(|| *self.periods.get(resource) - pool.remainder)
            })
            .min()
    }

    pub fn set_maxima(&mut self, maxima: ResourceMaxima) {
        for resource in Resource::ALL {
            self.pools
                .get_mut(resource)
                .set_maximum(*maxima.get(resource));
        }
    }

    /// Direct spending primitive. Action execution additionally owns reservations
    /// and paid-cost history; queue admission must not invoke this method.
    pub fn spend(&mut self, resource: Resource, amount: u32) -> bool {
        let pool = self.pools.get_mut(resource);
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
            self.pools
                .get_mut(resource)
                .recover(ticks, *self.periods.get(resource));
        }
    }

    /// Pure, bounded projection of actual periodic gains, excluding capped excess.
    pub(crate) fn regeneration_after(&self, ticks: u64) -> ByResource<u32> {
        ByResource::from_fn(|resource| {
            let pool = self.pools.get(resource);
            ((u128::from(pool.remainder) + u128::from(ticks))
                / u128::from(*self.periods.get(resource)))
            .min(u128::from(pool.maximum - pool.balance)) as u32
        })
    }
}
