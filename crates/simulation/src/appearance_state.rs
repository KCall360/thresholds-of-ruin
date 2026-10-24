//! Pure observer-presentation selection. No world queries, clocks, RNG mutation,
//! identity disclosure or saved-state integration live in this primitive.

pub const MAX_PRESENTATION_CHOICES: usize = crate::limits::CATALOG_ENTRIES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum AppearanceChannel {
    Creature = 1,
    Item = 2,
    Terrain = 3,
    MonsterSense = 4,
}

/// Private stable inputs, never a public target handle or a source of sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearanceKey {
    pub seed: u64,
    pub observer: u64,
    pub subject: u64,
    pub channel: AppearanceChannel,
    pub phase: u64,
}

/// Select in a caller-provided canonical catalog. The version-one recipe folds
/// seed, observer, subject, channel and phase in that order from domain
/// 0x6170706561727631. For each operand v, replace x with mix(x XOR v), where
/// mix adds 0x9e3779b97f4a7c15, multiplies (x XOR (x >> 30)) by
/// 0xbf58476d1ce4e5b9, multiplies (x XOR (x >> 27)) by 0x94d049bb133111eb,
/// then returns x XOR (x >> 31). Addition/multiplication wrap at 64 bits.
/// Reduce the final unsigned value modulo count. Catalog order is authority;
/// this deterministic presentation choice is not a cryptographic primitive.
pub fn presentation_index(key: AppearanceKey, count: usize) -> Option<usize> {
    if !(1..=MAX_PRESENTATION_CHOICES).contains(&count) {
        return None;
    }
    let mut mixed = 0x6170706561727631;
    for operand in [
        key.seed,
        key.observer,
        key.subject,
        key.channel as u64,
        key.phase,
    ] {
        mixed = mix(mixed ^ operand);
    }
    Some((mixed % count as u64) as usize)
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> AppearanceKey {
        AppearanceKey {
            seed: 42,
            observer: 1,
            subject: 2,
            channel: AppearanceChannel::Creature,
            phase: 0,
        }
    }

    #[test]
    fn selection_rejects_empty_and_oversized_catalogs_without_clamping() {
        assert_eq!(presentation_index(key(), 0), None);
        assert_eq!(
            presentation_index(key(), MAX_PRESENTATION_CHOICES + 1),
            None
        );
        assert_eq!(presentation_index(key(), usize::MAX), None);
        assert_eq!(presentation_index(key(), 1), Some(0));
    }

    #[test]
    fn appearance_selection_matches_frozen_unsigned_domain_vectors() {
        // Literal values independently evaluated from the documented integer
        // recipe, not recomputed by the production selector under test.
        let cases = [
            (AppearanceKey { seed: 0, ..key() }, 257, 165),
            (key(), 257, 183),
            (
                AppearanceKey {
                    observer: 2,
                    ..key()
                },
                257,
                173,
            ),
            (
                AppearanceKey {
                    subject: 3,
                    ..key()
                },
                257,
                94,
            ),
            (
                AppearanceKey {
                    channel: AppearanceChannel::Item,
                    ..key()
                },
                257,
                63,
            ),
            (
                AppearanceKey {
                    channel: AppearanceChannel::Terrain,
                    ..key()
                },
                257,
                192,
            ),
            (
                AppearanceKey {
                    channel: AppearanceChannel::MonsterSense,
                    ..key()
                },
                257,
                28,
            ),
            (AppearanceKey { phase: 1, ..key() }, 257, 21),
            (
                AppearanceKey {
                    seed: u64::MAX,
                    observer: u64::MAX,
                    subject: u64::MAX,
                    channel: AppearanceChannel::MonsterSense,
                    phase: u64::MAX,
                },
                4096,
                1668,
            ),
        ];
        for (key, count, expected) in cases {
            assert_eq!(presentation_index(key, count), Some(expected), "{key:?}");
        }
    }

    #[test]
    fn repeated_selection_is_pure_and_within_the_declared_catalog() {
        let key = key();
        let before = key;
        for count in [1, 2, 257, MAX_PRESENTATION_CHOICES] {
            let first = presentation_index(key, count).unwrap();
            assert!(first < count);
            for _ in 0..16 {
                assert_eq!(presentation_index(key, count), Some(first));
            }
        }
        assert_eq!(key, before);
    }
}
