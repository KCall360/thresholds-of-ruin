//! Actor-owned permanent-build identity, independent of transient combat state.
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

/// A permanent-build conflict counter. A command must additionally bind it to
/// the actor and current branch/reset context; this value is not authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildEpoch(NonZeroU64);

impl BuildEpoch {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(super) fn next(self) -> Option<Self> {
        Self::new(self.get().checked_add(1)?)
    }
}
