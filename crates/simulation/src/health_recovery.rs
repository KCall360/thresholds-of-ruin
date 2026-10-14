//! Injury-only Health recovery. The clock owns fractional work, never a balance.
use crate::health::{Health, HealthError};

/// Backend clock context, never an observation supplied to a player. Automatic
/// growth can first acquire a contextual source while its owner is frozen; that
/// clock retains its old raw denominator until the legitimate thaw boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryContext {
    Known { sensed_hostile: bool },
    Pending { base_period: u64 },
}

/// Nutrition can slow or stop recovery without changing injury.
/// A stopped clock preserves its earned fraction, but earns no catch-up ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoveryNutrition {
    #[default]
    Normal,
    Slowed {
        multiplier: u16,
    },
    Stopped,
}
impl<'de> serde::Deserialize<'de> for RecoveryNutrition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde internally tagged unit variants accept extra fields; empty
        // struct variants enforce the persisted policy's exact named shape.
        #[derive(serde::Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Recorded {
            Normal {},
            Slowed { multiplier: u16 },
            Stopped {},
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Recorded::Normal {} => Self::Normal,
            Recorded::Slowed { multiplier } => Self::Slowed { multiplier },
            Recorded::Stopped {} => Self::Stopped,
        })
    }
}
impl RecoveryNutrition {
    pub fn valid(self) -> bool {
        match self {
            Self::Slowed { multiplier } => (2..=100).contains(&multiplier),
            Self::Normal | Self::Stopped => true,
        }
    }
    fn multiplier(self) -> u64 {
        match self {
            Self::Slowed { multiplier } => u64::from(multiplier),
            Self::Normal | Self::Stopped => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthRecovery {
    period: u64,
    elapsed: u64,
    nutrition: RecoveryNutrition,
}
impl HealthRecovery {
    pub(crate) fn new(period: u64) -> Self {
        debug_assert!((1..=crate::limits::NUMERIC as u64).contains(&period));
        Self {
            period,
            elapsed: 0,
            nutrition: RecoveryNutrition::Normal,
        }
    }
    pub(crate) fn from_recorded(
        period: u64,
        elapsed: u64,
        nutrition: RecoveryNutrition,
        health: Health,
    ) -> Result<Self, HealthError> {
        if !nutrition.valid() || !(1..=crate::limits::NUMERIC as u64).contains(&period) {
            return Err(HealthError::InvalidState);
        }
        let value = Self {
            period,
            elapsed,
            nutrition,
        };
        if elapsed >= value.period() || ((health.dead() || health.injury() == 0) && elapsed != 0) {
            return Err(HealthError::InvalidState);
        }
        Ok(value)
    }
    /// Effective ticks per Health, including nutrition. While stopped this is
    /// the base denominator used to retain an earned fractional point.
    pub fn period(self) -> u64 {
        self.period * self.nutrition.multiplier()
    }
    pub(crate) fn base_period(self) -> u64 {
        self.period
    }
    pub fn elapsed(self) -> u64 {
        self.elapsed
    }
    pub fn nutrition(self) -> RecoveryNutrition {
        self.nutrition
    }
    pub fn stopped(self) -> bool {
        self.nutrition == RecoveryNutrition::Stopped
    }
    pub(crate) fn clear_if_saturated(&mut self, health: Health) {
        if health.dead() || health.injury() == 0 {
            self.elapsed = 0;
        }
    }
    /// Rephase by flooring the earned fraction. Changing a modifier or nutrition
    /// never grants a point, and repeated toggles cannot increase earned work.
    pub(crate) fn rephase(&mut self, period: u64, nutrition: RecoveryNutrition) {
        let previous = self.period();
        self.period = period;
        self.nutrition = nutrition;
        self.elapsed =
            (u128::from(self.elapsed) * u128::from(self.period()) / u128::from(previous)) as u64;
    }
    pub(crate) fn set_period(&mut self, period: u64) {
        self.rephase(period, self.nutrition);
    }
    pub(crate) fn set_nutrition(
        &mut self,
        nutrition: RecoveryNutrition,
    ) -> Result<(), HealthError> {
        if !nutrition.valid() {
            return Err(HealthError::InvalidState);
        }
        self.rephase(self.period, nutrition);
        Ok(())
    }
    pub(crate) fn damage_activity(&mut self, reset_percent: u8) {
        debug_assert!(reset_percent <= 100);
        self.elapsed = (u128::from(self.elapsed) * u128::from(100 - reset_percent) / 100) as u64;
    }
    pub fn next_recovery_in(self, health: Health) -> Option<u64> {
        (!health.dead() && health.injury() > 0 && !self.stopped())
            .then(|| self.period() - self.elapsed)
    }
    pub(crate) fn advance_active(&mut self, health: &mut Health, ticks: u64) {
        self.clear_if_saturated(*health);
        if self.next_recovery_in(*health).is_none() {
            return;
        }
        let elapsed = u128::from(self.elapsed) + u128::from(ticks);
        let gained = (elapsed / u128::from(self.period())).min(u128::from(health.injury())) as u32;
        health.heal(gained);
        self.elapsed = if health.injury() == 0 {
            0
        } else {
            (elapsed % u128::from(self.period())) as u64
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saturation_and_dead_clocks_never_bank_recovery() {
        let mut health = Health::new(8);
        let mut clock = HealthRecovery::new(3);
        clock.advance_active(&mut health, u64::MAX);
        health.damage(2);
        clock.advance_active(&mut health, 2);
        assert_eq!((health.injury(), clock.elapsed()), (2, 2));
        clock.advance_active(&mut health, u64::MAX);
        assert_eq!((health.injury(), clock.elapsed()), (0, 0));
        health.kill();
        clock.advance_active(&mut health, u64::MAX);
        assert!(health.dead());
        assert_eq!(clock.next_recovery_in(health), None);
    }
    #[test]
    fn nutrition_and_modifier_toggles_cannot_create_fractional_work() {
        let mut health = Health::new(8);
        health.damage(2);
        let mut clock = HealthRecovery::new(101);
        clock.advance_active(&mut health, 73);
        let did_not_increase = |before: HealthRecovery, after: HealthRecovery| {
            assert!(
                u128::from(after.elapsed()) * u128::from(before.period())
                    <= u128::from(before.elapsed()) * u128::from(after.period())
            );
        };
        for _ in 0..100 {
            let before = clock;
            clock.set_period(37);
            did_not_increase(before, clock);
            let before = clock;
            clock
                .set_nutrition(RecoveryNutrition::Slowed { multiplier: 3 })
                .unwrap();
            did_not_increase(before, clock);
            let before = clock;
            clock.set_nutrition(RecoveryNutrition::Stopped).unwrap();
            did_not_increase(before, clock);
            let stopped = clock;
            clock.advance_active(&mut health, u64::MAX);
            assert_eq!(clock, stopped);
            clock.set_period(101);
            did_not_increase(stopped, clock);
            let before = clock;
            clock.set_nutrition(RecoveryNutrition::Normal).unwrap();
            did_not_increase(before, clock);
            assert_eq!(health.injury(), 2);
        }
    }
    #[test]
    fn recorded_clocks_reject_invalid_or_saturated_progress_and_policy() {
        let mut health = Health::new(8);
        assert!(HealthRecovery::from_recorded(3, 1, RecoveryNutrition::Normal, health).is_err());
        health.damage(1);
        for period in [0, crate::limits::NUMERIC as u64 + 1, u64::MAX] {
            assert!(
                HealthRecovery::from_recorded(period, 0, RecoveryNutrition::Normal, health)
                    .is_err()
            );
        }
        assert!(HealthRecovery::from_recorded(3, 3, RecoveryNutrition::Normal, health).is_err());
        for multiplier in [0, 1, 101, u16::MAX] {
            assert!(HealthRecovery::from_recorded(
                3,
                0,
                RecoveryNutrition::Slowed { multiplier },
                health
            )
            .is_err());
        }
        let restored =
            HealthRecovery::from_recorded(3, 2, RecoveryNutrition::Stopped, health).unwrap();
        assert_eq!(restored.elapsed(), 2);
        health.kill();
        assert!(HealthRecovery::from_recorded(3, 1, RecoveryNutrition::Normal, health).is_err());
    }
    #[test]
    fn damage_reset_percent_removes_progress_with_floor_rounding() {
        let health = Health::from_recorded(8, 2, false).unwrap();
        let mut clock =
            HealthRecovery::from_recorded(100, 75, RecoveryNutrition::Normal, health).unwrap();
        clock.damage_activity(0);
        assert_eq!(clock.elapsed(), 75);
        clock.damage_activity(50);
        assert_eq!(clock.elapsed(), 37);
        clock.damage_activity(100);
        assert_eq!(clock.elapsed(), 0);
    }
}
