//! Timed source-specific fear. The same relation supplies disadvantage for a
//! hostile action or a resistance check against its causer; other targets do
//! not inherit it. This state requires no continued line of sight.

use crate::ActorId;
use std::collections::BTreeMap;

pub const BASE_FEAR_DURATION: u64 = 300;
pub const MAX_FEAR_DURATION: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FearError {
    InvalidSource,
    InvalidDuration,
    TooManySources,
    DuplicateSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FearUpdate {
    Applied,
    Refreshed,
    Immune,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FearState {
    remaining: BTreeMap<ActorId, u64>,
}

impl FearState {
    /// Restore remaining durations without refresh semantics or elapsed time.
    pub fn from_recorded(sources: Vec<(ActorId, u64)>) -> Result<Self, FearError> {
        if sources.len() > 128 {
            return Err(FearError::TooManySources);
        }
        let mut fear = Self::default();
        for (source, remaining) in sources {
            if fear.remaining.contains_key(&source) {
                return Err(FearError::DuplicateSource);
            }
            fear.apply(source, remaining, false)?;
        }
        Ok(fear)
    }

    pub fn sources(&self) -> &BTreeMap<ActorId, u64> {
        &self.remaining
    }

    pub fn remaining(&self, source: ActorId) -> Option<u64> {
        self.remaining.get(&source).copied()
    }

    /// Active ticks until the earliest source expires. Refreshing a source or
    /// gaining immunity changes this deadline without stacking applications.
    pub fn next_expiry_in(&self) -> Option<u64> {
        self.remaining.values().copied().min()
    }

    pub fn disadvantages_against(&self, source: ActorId) -> u32 {
        u32::from(self.remaining.contains_key(&source))
    }

    /// Refresh to at least a full new duration, without stacking or shortening
    /// a stronger existing application. Immunity removes all current fear.
    pub fn apply(
        &mut self,
        source: ActorId,
        duration: u64,
        immune: bool,
    ) -> Result<FearUpdate, FearError> {
        if source.0 == 0 || source.0 == u64::MAX {
            return Err(FearError::InvalidSource);
        }
        if !(1..=MAX_FEAR_DURATION).contains(&duration) {
            return Err(FearError::InvalidDuration);
        }
        if immune {
            self.remaining.clear();
            return Ok(FearUpdate::Immune);
        }
        if let Some(remaining) = self.remaining.get_mut(&source) {
            *remaining = (*remaining).max(duration);
            return Ok(FearUpdate::Refreshed);
        }
        if self.remaining.len() >= 128 {
            return Err(FearError::TooManySources);
        }
        self.remaining.insert(source, duration);
        Ok(FearUpdate::Applied)
    }

    pub fn reconcile_immunity(&mut self, immune: bool) {
        if immune {
            self.remaining.clear();
        }
    }

    /// Frozen actors receive no elapsed ticks or reactivation catch-up.
    pub fn advance_active(&mut self, ticks: u64) {
        self.remaining.retain(|_, remaining| {
            *remaining = remaining.saturating_sub(ticks);
            *remaining > 0
        });
    }
}
