//! Bounded deterministic dice used by checks and damage pools.
//!
//! This module owns neither action scheduling nor serialization. Callers own the
//! persisted random state and allocate a resolution's edge budget in roll order.

/// Net advantage after one-for-one cancellation. Each unit affects a different
/// original die, never a third roll of the same check die.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edge(i64);

impl Edge {
    pub fn from_counts(advantages: u32, disadvantages: u32) -> Self {
        Self(i64::from(advantages) - i64::from(disadvantages))
    }

    /// Signed units still available after cancellation and prior allocations.
    pub fn balance(self) -> i64 {
        self.0
    }

    pub fn is_neutral(self) -> bool {
        self.0 == 0
    }

    /// Allocate at most `maximum` units, preserving any remainder for the next
    /// eligible pool in this resolution. A zero-capacity roll consumes nothing.
    pub fn take(&mut self, maximum: u16) -> Self {
        let magnitude = self.0.unsigned_abs().min(u64::from(maximum)) as i64;
        let allocated = if self.0 < 0 { -magnitude } else { magnitude };
        self.0 -= allocated;
        Self(allocated)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckRoll {
    pub first: u8,
    pub second: Option<u8>,
    pub kept: u8,
}

/// Roll a d20 check. Any nonneutral edge rerolls once; callers allocate one
/// unit before this call when further dice belong to the same resolution.
pub fn roll_check(state: &mut u64, edge: Edge) -> CheckRoll {
    let first = die(state, 20) as u8;
    let second = (!edge.is_neutral()).then(|| die(state, 20) as u8);
    let kept = match second {
        None => first,
        Some(other) if edge.0 > 0 => first.max(other),
        Some(other) => first.min(other),
    };
    CheckRoll {
        first,
        second,
        kept,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiceError {
    InvalidPool,
}

/// One homogeneous damage pool. Construction bounds both allocation and the
/// maximum damage before a random state can be consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DicePool {
    count: u16,
    sides: u16,
    bonus: i32,
}

impl DicePool {
    pub fn new(count: u16, sides: u16, bonus: i32) -> Result<Self, DiceError> {
        if !(1..=64).contains(&count)
            || !(2..=1000).contains(&sides)
            || !(-1_000_000..=1_000_000).contains(&bonus)
            || i64::from(count) * i64::from(sides) + i64::from(bonus) > 1_000_000
        {
            return Err(DiceError::InvalidPool);
        }
        Ok(Self {
            count,
            sides,
            bonus,
        })
    }

    pub fn count(self) -> u16 {
        self.count
    }

    pub fn sides(self) -> u16 {
        self.sides
    }

    pub fn bonus(self) -> i32 {
        self.bonus
    }

    pub fn maximum(self) -> u32 {
        (i64::from(self.count) * i64::from(self.sides) + i64::from(self.bonus)).max(0) as u32
    }

    pub fn merged(self, other: Self) -> Result<Self, DiceError> {
        if self.sides != other.sides {
            return Err(DiceError::InvalidPool);
        }
        self.augmented(other.count, other.bonus)
    }

    pub fn augmented(self, extra_dice: u16, flat_bonus: i32) -> Result<Self, DiceError> {
        let count = self
            .count
            .checked_add(extra_dice)
            .ok_or(DiceError::InvalidPool)?;
        let bonus = self
            .bonus
            .checked_add(flat_bonus)
            .ok_or(DiceError::InvalidPool)?;
        Self::new(count, self.sides, bonus)
    }

    pub fn roll(self, state: &mut u64, edge: Edge) -> PoolRoll {
        let original = usize::from(self.count);
        let extra = edge.0.unsigned_abs().min(u64::from(self.count)) as usize;
        let rolled: Vec<_> = (0..original + extra)
            .map(|_| die(state, self.sides))
            .collect();
        let mut kept = rolled.clone();
        if extra != 0 {
            if edge.0 > 0 {
                kept.sort_unstable_by(|a, b| b.cmp(a));
            } else {
                kept.sort_unstable();
            }
            kept.truncate(original);
        }
        let sum: u32 = kept.iter().map(|&value| u32::from(value)).sum();
        let total = (i64::from(sum) + i64::from(self.bonus)).max(0) as u32;
        PoolRoll {
            rolled,
            kept,
            total,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolRoll {
    /// Actual draw order, before retention.
    pub rolled: Vec<u16>,
    /// Retained dice, sorted best-first when an edge was applied.
    pub kept: Vec<u16>,
    /// Retained sum plus the flat bonus, clamped to zero.
    pub total: u32,
}

// Internal callers supply a statically validated, nonzero catalog die size.
pub(crate) fn die(state: &mut u64, sides: u16) -> u16 {
    // SplitMix64 rejection sampling. The d20 threshold and state advancement
    // deliberately preserve the existing combat sequence byte for byte.
    let range = u64::from(sides);
    let limit = u64::MAX - u64::MAX % range;
    loop {
        let value = random_word(state);
        if value < limit {
            return (value % range + 1) as u16;
        }
    }
}

pub(crate) fn random_word(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}
