//! Actor-active periodic discovery. A source grants a cadence, not extra scans.
//! Earned fractions survive source changes; suppressed time never becomes credit.
use crate::discovery::FrozenDiscoveryCheck;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchClock {
    period: u64,
    elapsed: u64,
    pending: Option<FrozenDiscoveryCheck>,
}
impl Default for SearchClock {
    fn default() -> Self {
        Self {
            period: 100,
            elapsed: 0,
            pending: None,
        }
    }
}
impl<'de> Deserialize<'de> for SearchClock {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            period: u64,
            elapsed: u64,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            pending: Option<FrozenDiscoveryCheck>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if !(1..=crate::limits::NUMERIC as u64).contains(&f.period) || f.elapsed >= f.period {
            return Err(serde::de::Error::custom("invalid periodic search clock"));
        }
        Ok(Self {
            period: f.period,
            elapsed: f.elapsed,
            pending: f.pending,
        })
    }
}
impl SearchClock {
    pub(crate) fn valid_for(&self, actor: &crate::Actor) -> bool {
        !self.pending()
            && actor
                .perception()
                .search_interval
                .is_none_or(|period| period == self.period)
    }
    pub(crate) fn rephase(&mut self, period: Option<u64>) {
        if let Some(period) = period {
            debug_assert!((1..=crate::limits::NUMERIC as u64).contains(&period));
            self.elapsed =
                (u128::from(self.elapsed) * u128::from(period) / u128::from(self.period)) as u64;
            self.period = period;
        }
    }
    pub(crate) fn next_in(&self, actor: &crate::Actor) -> Option<u64> {
        let perception = actor.perception();
        (actor.alive()
            && perception.search_interval.is_some()
            && perception.sight
            && actor.combat.as_ref().is_some_and(|combat| {
                !combat
                    .creature()
                    .condition_restricts(crate::rules::ConditionRestriction::VoluntaryActions)
            }))
        .then_some(self.period - self.elapsed)
    }
    pub(crate) fn advance(
        &mut self,
        ticks: u64,
        check: FrozenDiscoveryCheck,
    ) -> Result<(), crate::GameError> {
        if ticks > self.period - self.elapsed || self.pending.is_some() {
            return Err(crate::GameError::Schedule(
                crate::ScheduleError::TimeExhausted,
            ));
        }
        self.elapsed += ticks;
        if self.elapsed == self.period {
            self.elapsed = 0;
            self.pending = Some(check);
        }
        Ok(())
    }
    pub(crate) fn take_pending(&mut self) -> Option<FrozenDiscoveryCheck> {
        self.pending.take()
    }
    pub(crate) fn pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl crate::Game {
    /// Runs only at authoritative boundaries after callers publish their RNG.
    pub(crate) fn settle_periodic_searches(&mut self) -> Result<(), crate::GameError> {
        let owners: Vec<_> = self
            .actors
            .pending_searches()
            .inspect(|_| {
                #[cfg(test)]
                SEARCH_SETTLEMENT_CANDIDATES.with(|count| count.set(count.get() + 1));
            })
            .collect();
        for id in owners {
            self.settle_periodic_search_for(id)?;
        }
        Ok(())
    }
    pub(crate) fn settle_periodic_search_for(
        &mut self,
        id: crate::ActorId,
    ) -> Result<(), crate::GameError> {
        let check = self
            .actors
            .get_mut(&id)
            .and_then(|mut owner| owner.search_clock.take_pending());
        if let Some(check) = check {
            self.run_active_discovery(id, &check)?;
        }
        Ok(())
    }
}

#[cfg(test)]
thread_local! {
    static SEARCH_SETTLEMENT_CANDIDATES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn search_settlement_candidates() -> usize {
    SEARCH_SETTLEMENT_CANDIDATES.with(|count| count.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check() -> FrozenDiscoveryCheck {
        FrozenDiscoveryCheck { bonus: 0, edge: 0 }
    }
    #[test]
    fn cadence_is_exact_and_source_rephase_never_grants_a_pulse() {
        let mut clock = SearchClock::default();
        clock.advance(99, check()).unwrap();
        assert!(!clock.pending());
        clock.rephase(Some(100));
        clock.rephase(None);
        clock.rephase(Some(100));
        assert!(!clock.pending());
        clock.advance(1, check()).unwrap();
        assert!(clock.take_pending().is_some());
        assert_eq!(clock.elapsed, 0);
        clock.advance(25, check()).unwrap();
        clock.rephase(Some(200));
        assert_eq!(clock.elapsed, 50);
        assert!(!clock.pending());
    }
    #[test]
    fn malformed_clock_and_unsettled_catchup_are_rejected() {
        for value in [
            r#"{"period":0,"elapsed":0,"pending":null}"#,
            r#"{"period":100,"elapsed":100,"pending":null}"#,
            r#"{"period":100,"elapsed":0}"#,
        ] {
            assert!(serde_json::from_str::<SearchClock>(value).is_err());
        }
        let mut clock = SearchClock::default();
        assert!(clock.advance(101, check()).is_err());
        clock.advance(100, check()).unwrap();
        assert!(clock.advance(1, check()).is_err());
    }
}
