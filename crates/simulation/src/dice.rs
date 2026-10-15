//! Bounded deterministic dice used by checks and damage pools.
//!
//! This module owns neither action scheduling nor serialization. Callers own the
//! persisted random state and allocate a resolution's edge budget in roll order.

/// Net advantage after one-for-one cancellation. Each unit affects a different
/// original die; a check has three original dice, damage retains its pool size.
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

/// Checks always retain three d6; each remaining edge unit contributes one
/// extra candidate die, up to three. Further edge belongs to later damage.
pub const CHECK_DICE: u16 = 3;
pub const CHECK_EDGE_CAPACITY: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckRoll {
    rolled: [u16; 6],
    draw_count: u8,
    retained: [u16; 3],
    /// Sum of the three retained dice, before attributes/ranks/modifiers.
    pub kept: u8,
}
impl CheckRoll {
    /// Actual deterministic draw order; unused array slots are never exposed.
    pub fn rolled(&self) -> &[u16] {
        &self.rolled[..usize::from(self.draw_count)]
    }
    pub fn retained(&self) -> &[u16; 3] {
        &self.retained
    }
}

/// Roll a bounded 3d6 check without heap allocation. Callers allocate up to
/// `CHECK_EDGE_CAPACITY` units before this call when later damage shares edge.
pub fn roll_check(state: &mut u64, edge: Edge) -> CheckRoll {
    let extra = edge.0.unsigned_abs().min(u64::from(CHECK_EDGE_CAPACITY)) as usize;
    let mut rolled = [0; 6];
    let draw_count = usize::from(CHECK_DICE) + extra;
    for value in &mut rolled[..draw_count] {
        *value = die(state, 6);
    }
    retain_check(rolled, draw_count, edge)
}

fn retain_check(rolled: [u16; 6], draw_count: usize, edge: Edge) -> CheckRoll {
    let mut ordered = rolled;
    if edge.0 > 0 {
        ordered[..draw_count].sort_unstable_by(|a, b| b.cmp(a));
    } else if edge.0 < 0 {
        ordered[..draw_count].sort_unstable();
    }
    let retained = [ordered[0], ordered[1], ordered[2]];
    CheckRoll {
        rolled,
        draw_count: draw_count as u8,
        retained,
        kept: retained.iter().sum::<u16>() as u8,
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
        if !(1..=crate::limits::DICE).contains(&count)
            || !(2..=crate::limits::DICE_SIDES).contains(&sides)
            || !(-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(bonus))
            || i64::from(count) * i64::from(sides) + i64::from(bonus) > crate::limits::NUMERIC
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

    /// Smallest clamped result, without consuming the random stream.
    pub fn minimum(self) -> u32 {
        (i64::from(self.count) + i64::from(self.bonus)).max(0) as u32
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
    // SplitMix64 rejection sampling. A check draws its d6 from the same explicit
    // stream as later damage; health and generation own independent streams.
    let range = u64::from(sides);
    let limit = u64::MAX - u64::MAX % range;
    loop {
        let value = random_word(state);
        if value < limit {
            return (value % range + 1) as u16;
        }
    }
}

const SPLITMIX64_GAMMA: u64 = 0x9e3779b97f4a7c15;

/// Read a zero-based word from an independent stream in constant work.
/// SplitMix64 advances its state by gamma before mixing each word, so skipping
/// `ordinal` advances and taking one ordinary draw preserves the exact stream.
/// Wrapping arithmetic also preserves the full u64 ordinal domain.
pub(crate) fn random_word_at(root: u64, ordinal: u64) -> u64 {
    let mut state = root.wrapping_add(SPLITMIX64_GAMMA.wrapping_mul(ordinal));
    random_word(&mut state)
}

pub(crate) fn random_word(state: &mut u64) -> u64 {
    #[cfg(test)]
    RANDOM_WORD_DRAWS.with(|draws| draws.set(draws.get() + 1));
    *state = state.wrapping_add(SPLITMIX64_GAMMA);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[cfg(test)]
thread_local! {
    static RANDOM_WORD_DRAWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn random_word_draws_for_test() -> usize {
    RANDOM_WORD_DRAWS.with(std::cell::Cell::get)
}

impl serde::Serialize for DicePool {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut record = serializer.serialize_struct("DicePool", 3)?;
        record.serialize_field("count", &self.count)?;
        record.serialize_field("sides", &self.sides)?;
        record.serialize_field("bonus", &self.bonus)?;
        record.end()
    }
}
impl<'de> serde::Deserialize<'de> for DicePool {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Record {
            count: u16,
            sides: u16,
            bonus: i32,
        }
        let record = Record::deserialize(deserializer)?;
        Self::new(record.count, record.sides, record.bonus)
            .map_err(|_| serde::de::Error::custom("invalid dice pool"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinal_words_match_the_existing_sequential_stream_through_large_ordinals() {
        for root in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
            let mut stream = root;
            // Includes every reachable HD ordinal and practical large offsets.
            for ordinal in 0..=1_000_000 {
                assert_eq!(
                    random_word_at(root, ordinal),
                    random_word(&mut stream),
                    "root {root:#x}, zero-based ordinal {ordinal}"
                );
            }
        }
    }

    #[test]
    fn ordinal_words_match_sequential_windows_across_full_width_wraparound() {
        for root in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
            for first in [(1_u64 << 32) - 2, (1_u64 << 63) - 2, u64::MAX - 3] {
                // Wide integer arithmetic independently positions the existing
                // sequential generator. Its following eight draws cross both
                // large boundaries and the end of the full-period stream.
                let mut stream =
                    (u128::from(root) + u128::from(first) * 0x9e3779b97f4a7c15_u128) as u64;
                for offset in 0..8 {
                    let ordinal = first.wrapping_add(offset);
                    let before = random_word_draws_for_test();
                    let actual = random_word_at(root, ordinal);
                    assert_eq!(random_word_draws_for_test() - before, 1);
                    assert_eq!(actual, random_word(&mut stream));
                }
            }
        }
    }

    #[test]
    fn ordinal_words_retain_pinned_large_offset_vectors() {
        // Independently evaluated with arbitrary-width integer intermediates,
        // reducing modulo 2^64 after each state/multiplication operation.
        let ordinals = [1_u64 << 32, 1_u64 << 63, u64::MAX - 1, u64::MAX];
        for (root, expected) in [
            (
                0,
                [
                    0x46093cf9861ec2e4,
                    0x481ec0a212a9f3db,
                    0x336503c6b835bec0,
                    0x0000000000000000,
                ],
            ),
            (
                u64::MAX,
                [
                    0xc5aa1d1d7e827744,
                    0x2a67d7552e039ea7,
                    0xde0a564cbcd060c4,
                    0xb4d055fcf2cbbd7b,
                ],
            ),
            (
                0x0123_4567_89ab_cdef,
                [
                    0xa6fb94d5ae1fc3f3,
                    0xbc37bb40fb7f4a51,
                    0x20bec7299668a13f,
                    0xb2c058e4ebb5112c,
                ],
            ),
        ] {
            for (ordinal, expected) in ordinals.into_iter().zip(expected) {
                assert_eq!(random_word_at(root, ordinal), expected);
            }
        }
    }

    #[test]
    fn exhaustive_check_distributions_match_independent_integer_counts() {
        // Enumerated independently over all 6^(3+k) ordered outcomes. These
        // exact counts cover every natural total, not a probabilistic tolerance.
        let expected: [[u32; 16]; 4] = [
            [1, 3, 6, 10, 15, 21, 25, 27, 27, 25, 21, 15, 10, 6, 3, 1],
            [
                1, 4, 10, 21, 38, 62, 91, 122, 148, 167, 172, 160, 131, 94, 54, 21,
            ],
            [
                1, 5, 15, 41, 90, 170, 296, 470, 665, 881, 1055, 1155, 1111, 935, 610, 276,
            ],
            [
                1, 6, 21, 78, 207, 447, 914, 1677, 2706, 4135, 5646, 7056, 7770, 7551, 5535, 2906,
            ],
        ];
        for extra in 0..=3 {
            for advantage in [false, true] {
                let edge = if advantage {
                    Edge::from_counts(extra, 0)
                } else {
                    Edge::from_counts(0, extra)
                };
                let count = 3 + extra as usize;
                let mut actual = [0_u32; 16];
                for outcome in 0..6_u32.pow(count as u32) {
                    let mut encoded = outcome;
                    let mut dice = [0; 6];
                    for value in &mut dice[..count] {
                        *value = (encoded % 6 + 1) as u16;
                        encoded /= 6;
                    }
                    let result = retain_check(dice, count, edge);
                    actual[usize::from(result.kept) - 3] += 1;
                }
                let mut reference = expected[extra as usize];
                if !advantage {
                    reference.reverse();
                }
                assert_eq!(actual, reference, "edge {}", edge.balance());
            }
        }
    }

    #[test]
    fn check_draw_count_and_retention_are_bounded_for_extreme_edge() {
        for (positive, negative) in [(u32::MAX, 0), (0, u32::MAX)] {
            let mut state = 42;
            let check = roll_check(&mut state, Edge::from_counts(positive, negative));
            assert_eq!(check.rolled().len(), 6);
            assert_eq!(check.retained().len(), 3);
            assert!((3..=18).contains(&check.kept));
        }
    }
}
