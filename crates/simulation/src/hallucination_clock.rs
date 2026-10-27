//! Presentation cadence only; ordinary ConditionState owns lifetime.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HallucinationCadence {
    generation: u64,
    phase: u64,
    period: u64,
    remaining_to_shift: u64,
}
impl<'de> Deserialize<'de> for HallucinationCadence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            generation: u64,
            phase: u64,
            period: u64,
            remaining_to_shift: u64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.period == 0 || f.remaining_to_shift == 0 || f.remaining_to_shift > f.period {
            return Err(serde::de::Error::custom("invalid hallucination cadence"));
        }
        Ok(Self {
            generation: f.generation,
            phase: f.phase,
            period: f.period,
            remaining_to_shift: f.remaining_to_shift,
        })
    }
}
impl HallucinationCadence {
    pub(crate) fn new(generation: u64, period: std::num::NonZeroU64) -> Self {
        Self {
            generation,
            phase: 0,
            period: period.get(),
            remaining_to_shift: period.get(),
        }
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    pub(crate) fn phase(&self) -> u64 {
        self.phase
    }
    pub(crate) fn next_in(&self) -> u64 {
        self.remaining_to_shift
    }

    /// Conservative impossibility check, not historical authenticity. The
    /// owner's settlement timestamp includes frozen gaps after thaw, so it is
    /// an upper bound on elapsed cadence age, never its exact start history.
    /// Call only for a record settled to this same timestamp, not a lazily
    /// projected presentation paired with the old persisted timestamp.
    pub(crate) fn valid_at(&self, settled_at: u64) -> bool {
        let elapsed = u128::from(self.phase) * u128::from(self.period)
            + u128::from(self.period - self.remaining_to_shift);
        elapsed <= u128::from(settled_at)
    }

    /// Pure bounded arithmetic; no per-shift loop, random draw or duration state.
    pub(crate) fn advance(&mut self, ticks: u64) -> Result<(), crate::GameError> {
        if ticks < self.remaining_to_shift {
            self.remaining_to_shift -= ticks;
            return Ok(());
        }
        let after_first = ticks - self.remaining_to_shift;
        let shifts =
            (after_first / self.period)
                .checked_add(1)
                .ok_or(crate::GameError::Schedule(
                    crate::ScheduleError::TimeExhausted,
                ))?;
        let phase = self
            .phase
            .checked_add(shifts)
            .ok_or(crate::GameError::Schedule(
                crate::ScheduleError::TimeExhausted,
            ))?;
        let remaining = self.period - after_first % self.period;
        self.phase = phase;
        self.remaining_to_shift = remaining;
        Ok(())
    }
    pub(crate) fn projected(&self, ticks: u64) -> Result<Self, crate::GameError> {
        let mut result = self.clone();
        result.advance(ticks)?;
        Ok(result)
    }
}

/// Settle only the OLD instance's interval. Lifetime is borrowed from ordinary
/// ConditionState; it is never copied into the saved cadence. Expiry wins ties
/// and clears before irrelevant post-expiry phase arithmetic can overflow.
pub(crate) fn settle_before_expiry(
    cadence: &mut Option<HallucinationCadence>,
    condition_remaining: u64,
    elapsed: u64,
) -> Result<(), crate::GameError> {
    if elapsed >= condition_remaining {
        *cadence = None;
    } else if let Some(clock) = cadence {
        clock.advance(elapsed)?;
    }
    Ok(())
}

/// Invoked only after old actor time settled and the final condition state is
/// known. `active_generation` comes from the one validated marked condition,
/// excluding protected/inactive/dead entries, never from presentation queries.
pub(crate) fn reconcile(
    cadence: &mut Option<HallucinationCadence>,
    active_generation: Option<u64>,
    period: std::num::NonZeroU64,
) {
    match active_generation {
        None => *cadence = None,
        Some(generation)
            if cadence
                .as_ref()
                .is_some_and(|clock| clock.generation() == generation) => {}
        Some(generation) => *cadence = Some(HallucinationCadence::new(generation, period)),
    }
}

/// The validated marked condition is target-wide and finite. Its existing
/// lifetime generation is the only cadence identity; ordinary actors have none.
pub(crate) fn hallucination_instance(actor: &crate::Actor) -> Option<(u64, u64)> {
    if !actor.alive() {
        return None;
    }
    let creature = actor.combat.as_ref()?.creature();
    creature.conditions().instances().find_map(|instance| {
        (instance.phase == crate::conditions::ConditionPhase::Active
            && instance.remaining.is_some_and(|remaining| remaining > 0)
            && creature
                .build()
                .ruleset()
                .condition(&instance.condition)
                .is_some_and(|definition| definition.hallucinating))
        .then(|| {
            (
                instance.generation,
                instance.remaining.expect("finite marked condition"),
            )
        })
    })
}

/// Earliest executable boundary, never a phantom shift after condition expiry.
pub(crate) fn next_in(actor: &crate::Actor) -> Option<u64> {
    let (generation, remaining) = hallucination_instance(actor)?;
    let clock = actor.hallucination.as_ref()?;
    (clock.generation() == generation).then(|| clock.next_in().min(remaining))
}

/// Shared by current, history and detached actor record restoration. Period is
/// captured unspeeded timing, not today's modified action duration or a new cap.
pub(crate) fn valid_for(actor: &crate::Actor) -> bool {
    match (hallucination_instance(actor), actor.hallucination.as_ref()) {
        (None, None) => true,
        (Some((generation, remaining)), Some(clock)) => {
            clock.generation == generation
                && clock.period > 0
                && clock.remaining_to_shift > 0
                && clock.remaining_to_shift <= clock.period
                && clock.valid_at(actor.active_since)
                && actor
                    .active_since
                    .checked_add(clock.next_in().min(remaining))
                    .is_some()
        }
        _ => false,
    }
}

impl crate::Game {
    /// Pure lazy visual projection. It reads only the observer's old active
    /// interval and marked lifetime; it neither settles nor samples any RNG.
    pub(crate) fn projected_hallucination(
        &self,
        id: crate::ActorId,
    ) -> Result<Option<HallucinationCadence>, crate::GameError> {
        let actor = self.actors.get(&id).ok_or(crate::GameError::UnknownActor)?;
        let Some((generation, remaining)) = hallucination_instance(actor) else {
            return Ok(None);
        };
        let elapsed = self.actor_clock(id).saturating_sub(actor.active_since);
        if elapsed >= remaining {
            return Ok(None);
        }
        actor
            .hallucination
            .as_ref()
            .filter(|clock| clock.generation() == generation)
            .map(|clock| clock.projected(elapsed))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cadence_phase_zero_full_period_and_exact_boundaries_are_pure() {
        let clock = HallucinationCadence::new(4, std::num::NonZeroU64::new(100).unwrap());
        assert_eq!((clock.phase(), clock.next_in()), (0, 100));
        for (ticks, phase, remaining) in [
            (0, 0, 100),
            (99, 0, 1),
            (100, 1, 100),
            (101, 1, 99),
            (1234, 12, 66),
        ] {
            let projected = clock.projected(ticks).unwrap();
            assert_eq!((projected.phase(), projected.next_in()), (phase, remaining));
            assert_eq!((clock.phase(), clock.next_in()), (0, 100));
        }
    }
    #[test]
    fn same_condition_generation_keeps_cadence_new_generation_resets_and_cure_clears() {
        let period = std::num::NonZeroU64::new(100).unwrap();
        let mut clock = Some(HallucinationCadence::new(4, period));
        clock.as_mut().unwrap().advance(150).unwrap();
        let before = clock.clone();
        reconcile(&mut clock, Some(4), std::num::NonZeroU64::new(200).unwrap());
        assert_eq!(clock, before); // Extend preserves the captured period/deadline.
        reconcile(&mut clock, Some(5), period);
        assert_eq!(clock, Some(HallucinationCadence::new(5, period)));
        reconcile(&mut clock, None, period);
        assert_eq!(clock, None);
    }
    #[test]
    fn expiry_wins_shift_ties_and_discards_irrelevant_post_expiry_arithmetic() {
        let period = std::num::NonZeroU64::new(100).unwrap();
        for elapsed in [100, u64::MAX] {
            let mut clock = Some(HallucinationCadence::new(4, period));
            settle_before_expiry(&mut clock, 100, elapsed).unwrap();
            assert_eq!(clock, None);
        }
        let mut clock = Some(HallucinationCadence::new(4, period));
        settle_before_expiry(&mut clock, 200, 100).unwrap();
        assert_eq!(
            clock.as_ref().map(|c| (c.phase(), c.next_in())),
            Some((1, 100))
        );
    }

    #[test]
    fn record_phase_bound_is_conservative_and_projection_needs_its_own_timestamp() {
        let clock = HallucinationCadence::new(4, std::num::NonZeroU64::new(100).unwrap());
        assert!(clock.valid_at(0));
        let projected = clock.projected(150).unwrap();
        assert!(!projected.valid_at(0));
        assert!(projected.valid_at(150));
        // A frozen gap shifts the owner's absolute settlement time but does not
        // add presentation phase. The conservative bound must allow this.
        assert!(projected.valid_at(100_150));
        let mut impossible = clock.clone();
        impossible.phase = u64::MAX;
        assert!(!impossible.valid_at(100));
        assert!(clock.valid_at(u64::MAX)); // actual next-event horizon is a separate check
    }

    #[test]
    fn phase_overflow_is_atomic_and_expiry_prevents_irrelevant_overflow() {
        let mut clock = HallucinationCadence::new(4, std::num::NonZeroU64::new(100).unwrap());
        clock.phase = u64::MAX;
        let before = clock.clone();
        assert_eq!(
            clock.advance(100),
            Err(crate::GameError::Schedule(
                crate::ScheduleError::TimeExhausted
            ))
        );
        assert_eq!(clock, before);
        let mut cadence = Some(clock);
        settle_before_expiry(&mut cadence, 100, 100).unwrap();
        assert_eq!(cadence, None);
    }

    #[test]
    fn cadence_record_is_named_required_and_checked_without_a_lifetime_field() {
        let clock = HallucinationCadence::new(0, std::num::NonZeroU64::new(100).unwrap());
        let value = serde_json::to_value(&clock).unwrap();
        assert_eq!(
            serde_json::from_value::<HallucinationCadence>(value.clone()).unwrap(),
            clock
        );
        for field in ["generation", "phase", "period", "remaining_to_shift"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<HallucinationCadence>(missing).is_err());
        }
        for (field, number) in [
            ("period", 0),
            ("remaining_to_shift", 0),
            ("remaining_to_shift", 101),
        ] {
            let mut invalid = value.clone();
            invalid[field] = number.into();
            assert!(serde_json::from_value::<HallucinationCadence>(invalid).is_err());
        }
        assert!(
            serde_json::from_value::<HallucinationCadence>(serde_json::json!([0, 0, 100, 100]))
                .is_err()
        );
        let bytes = serde_json::to_string(&clock).unwrap();
        let duplicate = bytes.replace("\"phase\":0", "\"phase\":0,\"phase\":0");
        assert!(serde_json::from_str::<HallucinationCadence>(&duplicate).is_err());
        let mut extra = value;
        extra["duration"] = 1000.into();
        assert!(serde_json::from_value::<HallucinationCadence>(extra).is_err());
    }
}

#[cfg(test)]
#[path = "hallucination_clock_tests.rs"]
pub(crate) mod integration_tests;
