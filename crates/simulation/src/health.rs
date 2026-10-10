//! Injury-preserving health for rederived creature builds.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthError {
    InvalidState,
}

/// Death is a persistent fact, independent of later increases to maximum health.
/// Injury records applied damage rather than an independently clamped balance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    maximum: u32,
    injury: u32,
    dead: bool,
}

impl Health {
    /// Restore injury against a freshly derived maximum. Persistent death may
    /// outlive the injury that caused it, but a living state must have health.
    pub fn from_recorded(maximum: u32, injury: u32, dead: bool) -> Result<Self, HealthError> {
        if !dead && injury >= maximum {
            return Err(HealthError::InvalidState);
        }
        Ok(Self {
            maximum,
            injury,
            dead,
        })
    }

    pub fn new(maximum: u32) -> Self {
        Self {
            maximum,
            injury: 0,
            dead: maximum == 0,
        }
    }

    pub fn maximum(self) -> u32 {
        self.maximum
    }

    pub fn injury(self) -> u32 {
        self.injury
    }

    pub fn dead(self) -> bool {
        self.dead
    }

    pub fn current(self) -> u32 {
        if self.dead {
            0
        } else {
            self.maximum - self.injury
        }
    }

    pub fn set_maximum(&mut self, maximum: u32) {
        self.maximum = maximum;
        self.dead |= self.injury >= maximum;
    }

    /// Returns applied damage, excluding overkill or damage to a dead actor.
    pub fn damage(&mut self, amount: u32) -> u32 {
        let applied = amount.min(self.current());
        self.injury += applied;
        self.dead |= self.injury >= self.maximum;
        applied
    }

    /// Healing cannot revive a dead actor. Returns injury actually removed.
    pub fn heal(&mut self, amount: u32) -> u32 {
        if self.dead {
            return 0;
        }
        let healed = amount.min(self.injury);
        self.injury -= healed;
        healed
    }

    /// Used for death independent of damage, including removal of the last HD.
    pub fn kill(&mut self) {
        self.dead = true;
    }
}
