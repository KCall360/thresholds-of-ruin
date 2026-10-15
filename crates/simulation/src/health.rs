//! Injury-preserving health for rederived creature builds.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthError {
    InvalidState,
}

/// Death is a persistent fact, independent of later increases to maximum health.
/// Injury records applied damage rather than an independently clamped balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    maximum: u64,
    injury: u64,
    dead: bool,
}

impl Health {
    /// Restore injury against a freshly derived maximum. Persistent death may
    /// outlive the injury that caused it, but a living state must have health.
    pub fn from_recorded(maximum: u64, injury: u64, dead: bool) -> Result<Self, HealthError> {
        if !dead && injury >= maximum {
            return Err(HealthError::InvalidState);
        }
        Ok(Self {
            maximum,
            injury,
            dead,
        })
    }

    pub fn new(maximum: u64) -> Self {
        Self {
            maximum,
            injury: 0,
            dead: maximum == 0,
        }
    }

    pub fn maximum(self) -> u64 {
        self.maximum
    }

    pub fn injury(self) -> u64 {
        self.injury
    }

    pub fn dead(self) -> bool {
        self.dead
    }

    pub fn current(self) -> u64 {
        if self.dead {
            0
        } else {
            self.maximum - self.injury
        }
    }

    pub fn set_maximum(&mut self, maximum: u64) {
        self.maximum = maximum;
        self.dead |= self.injury >= maximum;
    }

    /// Returns applied damage, excluding overkill or damage to a dead actor.
    pub fn damage(&mut self, amount: u32) -> u32 {
        let applied = u64::from(amount).min(self.current());
        self.injury = self
            .injury
            .checked_add(applied)
            .expect("applied damage cannot exceed the maximum Health");
        self.dead |= self.injury >= self.maximum;
        u32::try_from(applied).expect("applied damage is bounded by its effect operand")
    }

    /// Healing cannot revive a dead actor. Returns injury actually removed.
    pub fn heal(&mut self, amount: u32) -> u32 {
        u32::try_from(self.recover(u64::from(amount)))
            .expect("direct healing is bounded by its effect operand")
    }

    /// Passive recovery can restore a wide balance in one active interval.
    pub(crate) fn recover(&mut self, amount: u64) -> u64 {
        if self.dead {
            return 0;
        }
        let healed = amount.min(self.injury);
        self.injury -= healed;
        healed
    }

    /// Exhaust the living balance, preserving starvation's full injury record.
    pub(crate) fn deplete(&mut self) -> u64 {
        let applied = self.current();
        if applied > 0 {
            self.injury = self.maximum;
            self.dead = true;
        }
        applied
    }

    /// Used for death independent of damage, including removal of the last HD.
    pub fn kill(&mut self) {
        self.dead = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_scalar_health_preserves_exact_balances_and_bounded_effects() {
        for maximum in [
            1_000_001_u64,
            u64::from(u32::MAX) + 1,
            (1_u64 << 53) + 1,
            u64::MAX,
        ] {
            let mut health = Health::new(maximum);
            let amount = u32::try_from(maximum.min(u64::from(u32::MAX))).unwrap();
            assert_eq!(health.maximum(), maximum);
            assert_eq!(health.damage(amount), amount);
            assert_eq!(health.injury(), u64::from(amount));
            assert_eq!(health.current(), maximum - u64::from(amount));
            if !health.dead() {
                assert_eq!(health.heal(amount), amount);
                assert_eq!(health.current(), maximum);
            }
        }
    }

    #[test]
    fn wide_scalar_direct_healing_preserves_injury_beyond_the_effect_operand() {
        let injury = 2 * u64::from(u32::MAX) + 17;
        let mut health = Health::from_recorded(u64::MAX, injury, false).unwrap();
        assert_eq!(health.heal(u32::MAX), u32::MAX);
        assert_eq!(health.injury(), u64::from(u32::MAX) + 17);
        assert_eq!(health.current(), u64::MAX - (u64::from(u32::MAX) + 17));
        assert!(!health.dead());
    }

    #[test]
    fn wide_scalar_health_lethal_damage_never_overflows_and_death_stays_persistent() {
        let mut health = Health::from_recorded(u64::MAX, u64::MAX - 7, false).unwrap();
        assert_eq!(health.damage(u32::MAX), 7);
        assert_eq!(health.injury(), u64::MAX);
        assert!(health.dead());
        assert_eq!(health.damage(u32::MAX), 0);
        assert_eq!(health.heal(u32::MAX), 0);
        health.set_maximum(1);
        health.set_maximum(u64::MAX);
        assert_eq!(health.current(), 0);
        assert_eq!(health.injury(), u64::MAX);
        assert!(Health::from_recorded(u64::MAX, u64::MAX, false).is_err());
        assert!(Health::from_recorded(1, u64::MAX, true).unwrap().dead());
    }

    #[test]
    fn wide_scalar_depletion_records_the_complete_balance_without_reviving() {
        let mut health = Health::from_recorded(u64::MAX, 7, false).unwrap();
        assert_eq!(health.deplete(), u64::MAX - 7);
        assert_eq!(health.injury(), u64::MAX);
        assert!(health.dead());
        assert_eq!(health.deplete(), 0);
        assert_eq!(health.recover(u64::MAX), 0);
        let mut already_dead = Health::from_recorded(u64::MAX, 7, true).unwrap();
        assert_eq!(already_dead.deplete(), 0);
        assert_eq!(already_dead.injury(), 7);
    }

    #[test]
    fn wide_scalar_maximum_changes_preserve_injury_and_do_not_revive() {
        let injury = u64::from(u32::MAX) + 17;
        let mut health = Health::from_recorded(u64::MAX, injury, false).unwrap();
        health.set_maximum(injury + 1);
        assert_eq!((health.current(), health.injury()), (1, injury));
        health.set_maximum(injury);
        health.set_maximum(u64::MAX);
        assert!(health.dead());
        assert_eq!((health.current(), health.injury()), (0, injury));
    }
}
