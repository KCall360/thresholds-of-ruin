//! Bounded actor-active countdowns keyed by stable ability identity.
//! Readiness here concerns only elapsed cooldown; action authority, grants and
//! atomic preparation remain the caller's responsibility.
use crate::{content_ids::AbilityId, limits, rules::Ruleset};
use std::{collections::BTreeMap, num::NonZeroU64};
use tor_world::Shared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CooldownError {
    UnknownAbility,
    InvalidDuration,
    AlreadyCoolingDown,
    Limit,
    Dead,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Entries(BTreeMap<AbilityId, NonZeroU64>);

impl Clone for Entries {
    fn clone(&self) -> Self {
        crate::diagnostics::cooldown_entries_copied(self.0.len());
        Self(self.0.clone())
    }
}

/// One authoritative entry per cooling-down ability. Entries survive grant
/// changes and disappear only on expiry; snapshots share untouched storage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cooldowns {
    entries: Shared<Entries>,
}

impl Cooldowns {
    pub(crate) fn from_recorded(
        rows: Vec<(AbilityId, u64)>,
        rules: &Ruleset,
    ) -> Result<Self, CooldownError> {
        if rows.len() > limits::CATALOG_ENTRIES {
            return Err(CooldownError::Limit);
        }
        let mut result = Self::default();
        for (ability, remaining) in rows {
            result.start(&ability, remaining, rules)?;
        }
        Ok(result)
    }

    /// Zero means no cooldown is retained. This does not establish that the
    /// ability exists, is granted, or can otherwise be prepared.
    pub fn remaining(&self, ability: &AbilityId) -> u64 {
        self.entries.0.get(ability).map_or(0, |ticks| ticks.get())
    }

    pub fn is_ready(&self, ability: &AbilityId) -> bool {
        !self.entries.0.contains_key(ability)
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&AbilityId, NonZeroU64)> {
        self.entries
            .0
            .iter()
            .map(|(ability, ticks)| (ability, *ticks))
    }

    pub(crate) fn validate_ruleset(&self, rules: &Ruleset) -> Result<(), CooldownError> {
        if self
            .entries
            .0
            .keys()
            .any(|ability| rules.ability(ability).is_none())
        {
            return Err(CooldownError::UnknownAbility);
        }
        Ok(())
    }

    pub(crate) fn validate_start(
        &self,
        ability: &AbilityId,
        duration: u64,
        rules: &Ruleset,
    ) -> Result<NonZeroU64, CooldownError> {
        let duration = NonZeroU64::new(duration)
            .filter(|ticks| ticks.get() <= limits::NUMERIC as u64)
            .ok_or(CooldownError::InvalidDuration)?;
        if rules.ability(ability).is_none() {
            return Err(CooldownError::UnknownAbility);
        }
        if !self.is_ready(ability) {
            return Err(CooldownError::AlreadyCoolingDown);
        }
        if self.entries.0.len() >= limits::CATALOG_ENTRIES {
            return Err(CooldownError::Limit);
        }
        Ok(duration)
    }

    pub(crate) fn start(
        &mut self,
        ability: &AbilityId,
        duration: u64,
        rules: &Ruleset,
    ) -> Result<(), CooldownError> {
        let duration = self.validate_start(ability, duration, rules)?;
        let id = &rules.ability(ability).expect("validated ability").id;
        self.entries.0.insert(id.clone(), duration);
        Ok(())
    }

    /// Earliest relative expiry. No absolute world clock or frozen-time offset
    /// belongs in this store; the existing actor timer index owns deadlines.
    pub fn next_expiry_in(&self) -> Option<u64> {
        crate::diagnostics::cooldown_deadline_candidates(self.entries.0.len());
        self.entries.0.values().map(|ticks| ticks.get()).min()
    }

    pub(crate) fn advance_active(&mut self, ticks: u64) {
        if ticks == 0 || self.entries.0.is_empty() {
            return;
        }
        crate::diagnostics::cooldown_entries_advanced(self.entries.0.len());
        self.entries.0.retain(|_, remaining| {
            let Some(next) = NonZeroU64::new(remaining.get().saturating_sub(ticks)) else {
                return false;
            };
            *remaining = next;
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdowns_expire_independently_and_do_not_renew() {
        let rules = crate::test_creatures::ruleset();
        let first = AbilityId::new("basic_melee").unwrap();
        let second = AbilityId::new("magic_bolt").unwrap();
        let mut state = Cooldowns::default();
        state.start(&first, 3, &rules).unwrap();
        state.start(&second, 7, &rules).unwrap();
        state.advance_active(2);
        let before = state.clone();
        assert_eq!(
            state.start(&first, 99, &rules),
            Err(CooldownError::AlreadyCoolingDown)
        );
        assert_eq!(state, before);
        assert_eq!(state.next_expiry_in(), Some(1));
        state.advance_active(1);
        assert!(state.is_ready(&first));
        assert_eq!(state.remaining(&second), 4);
        state.start(&first, 1, &rules).unwrap();
        state.advance_active(u64::MAX);
        assert_eq!(state, Cooldowns::default());
        assert_eq!(state.next_expiry_in(), None);
    }

    #[test]
    fn recorded_rows_reject_duplicates_unknown_ids_and_out_of_bounds_intervals() {
        let rules = crate::test_creatures::ruleset();
        let ability = AbilityId::new("magic_bolt").unwrap();
        for duration in [0, limits::NUMERIC as u64 + 1, u64::MAX] {
            assert_eq!(
                Cooldowns::from_recorded(vec![(ability.clone(), duration)], &rules),
                Err(CooldownError::InvalidDuration)
            );
        }
        assert_eq!(
            Cooldowns::from_recorded(vec![(AbilityId::new("unknown").unwrap(), 1)], &rules),
            Err(CooldownError::UnknownAbility)
        );
        assert_eq!(
            Cooldowns::from_recorded(vec![(ability.clone(), 1), (ability.clone(), 2)], &rules),
            Err(CooldownError::AlreadyCoolingDown)
        );
        assert_eq!(
            Cooldowns::from_recorded(
                vec![(ability.clone(), 1); limits::CATALOG_ENTRIES + 1],
                &rules
            ),
            Err(CooldownError::Limit)
        );
        let state =
            Cooldowns::from_recorded(vec![(ability.clone(), limits::NUMERIC as u64)], &rules)
                .unwrap();
        assert_eq!(state.remaining(&ability), limits::NUMERIC as u64);
    }

    #[test]
    fn untouched_snapshots_share_storage_and_failed_starts_do_not_copy_it() {
        let rules = crate::test_creatures::ruleset();
        let ability = AbilityId::new("magic_bolt").unwrap();
        let mut state = Cooldowns::default();
        state.start(&ability, 10, &rules).unwrap();
        let snapshot = state.clone();
        assert!(state.entries.shares_storage(&snapshot.entries));
        state.advance_active(0);
        assert_eq!(
            state.start(&ability, 1, &rules),
            Err(CooldownError::AlreadyCoolingDown)
        );
        assert!(state.entries.shares_storage(&snapshot.entries));
        state.advance_active(1);
        assert!(!state.entries.shares_storage(&snapshot.entries));
        assert_eq!(snapshot.remaining(&ability), 10);
        assert_eq!(state.remaining(&ability), 9);
    }
}
