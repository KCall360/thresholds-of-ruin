//! Immutable Hit Dice records and reversible, current-composition derivation.
//!
//! The ledger never consumes combat randomness. Each record owns a separate
//! health seed, interpreted against its current die by deterministic sampling.

use crate::dice::{die, random_word, random_word_at};

use crate::content_ids::{ClassId, TypeId};
use crate::rules::Ruleset;
use crate::HdCount;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HdSource {
    Racial,
    Class(ClassId),
}

/// Cached derivation facts for one owned hit die. Never part of a save record.
#[derive(Debug, PartialEq, Eq)]
pub struct HealthContribution {
    pub die: u16,
    pub health: u32,
}

impl Clone for HealthContribution {
    fn clone(&self) -> Self {
        crate::diagnostics::health_provenance_copy();
        Self {
            die: self.die,
            health: self.health,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitDie {
    source: HdSource,
    health_seed: u64,
}

impl HitDie {
    pub fn new(source: HdSource, health_seed: u64) -> Self {
        Self {
            source,
            health_seed,
        }
    }

    pub fn source(&self) -> HdSource {
        self.source.clone()
    }

    pub fn health_seed(&self) -> u64 {
        self.health_seed
    }

    fn health(
        &self,
        kind: &TypeId,
        rules: &Ruleset,
        first: bool,
    ) -> Result<HealthContribution, HdError> {
        crate::diagnostics::raw_hit_die_visit();
        crate::diagnostics::health_derivation();
        let sides = match &self.source {
            HdSource::Racial => rules
                .creature_type(kind)
                .map(|definition| definition.health_die),
            HdSource::Class(class) => rules.class(class).map(|definition| definition.health_die),
        }
        .ok_or(HdError::UnknownSource)?;
        let mut state = self.health_seed;
        Ok(HealthContribution {
            die: sides,
            health: if first {
                u32::from(sides)
            } else {
                u32::from(die(&mut state, sides))
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdError {
    Arithmetic,
    TooManyHitDice,
    UnknownSource,
    InvalidHealthRoot,
}

/// Ordered advancement records. No caller can mutate a retained die's source
/// or health seed. A zero-HD ledger represents regression to persistent death;
/// actor state, rather than this definition, owns whether death has occurred.
///
/// Seeded ledgers retain their original ordinal Health stream for earned growth.
/// Recorded-only ledgers retain explicit per-die seeds without a known root;
/// they support manual advancement but cannot perform ordinary earned growth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HdLedger {
    entries: Vec<HitDie>,
    health_root_seed: Option<u64>,
}

impl HdLedger {
    /// Assign independent health streams in advancement order. Class/type
    /// choices do not change the assigned stream; combat RNG is never borrowed.
    pub fn seeded(sources: Vec<HdSource>, seed: u64) -> Result<Self, HdError> {
        if sources.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        let mut state = seed;
        Ok(Self {
            entries: sources
                .into_iter()
                .map(|source| HitDie::new(source, random_word(&mut state)))
                .collect(),
            health_root_seed: Some(seed),
        })
    }

    /// Append from the actor's original health stream without changing retained
    /// records. Removing and re-adding an ordinal assigns the same health seed.
    /// Seeded ledgers reject a different root without mutation. Recorded-only
    /// ledgers accept explicit manual roots without claiming stream provenance.
    /// Seed lookup does constant work without replaying the root's prior words;
    /// the actual per-HD records remain authoritative.
    pub fn append_seeded(&mut self, source: HdSource, root_seed: u64) -> Result<(), HdError> {
        if self.entries.len() >= crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        if self.health_root_seed.is_some_and(|root| root != root_seed) {
            return Err(HdError::InvalidHealthRoot);
        }
        let health_seed = random_word_at(root_seed, HdCount::from_usize(self.entries.len())?.get());
        self.entries.push(HitDie::new(source, health_seed));
        Ok(())
    }

    /// Construct a recorded-only ledger with explicit per-die Health seeds.
    /// Manual advancement remains available; ordinary earned growth requires
    /// an original Health stream and is unavailable for this ledger domain.
    pub fn new(entries: Vec<HitDie>) -> Result<Self, HdError> {
        Self::from_recorded(entries, None)
    }

    pub(crate) fn from_recorded(
        entries: Vec<HitDie>,
        health_root_seed: Option<u64>,
    ) -> Result<Self, HdError> {
        if entries.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        if let Some(mut stream) = health_root_seed {
            for entry in &entries {
                if entry.health_seed != random_word(&mut stream) {
                    return Err(HdError::InvalidHealthRoot);
                }
            }
        }
        Ok(Self {
            entries,
            health_root_seed,
        })
    }

    /// Root of the ordinal Health stream, retained through drain and regrowth.
    /// `None` identifies a recorded-only ledger, ineligible for earned growth.
    pub fn health_root_seed(&self) -> Option<u64> {
        self.health_root_seed
    }

    pub fn entries(&self) -> &[HitDie] {
        &self.entries
    }

    pub fn total_hd(&self) -> HdCount {
        HdCount::from_usize(self.entries.len()).expect("admitted resident ledger count")
    }

    pub fn racial_hd(&self) -> HdCount {
        HdCount::from_usize(
            self.entries
                .iter()
                .filter(|entry| {
                    crate::diagnostics::raw_hit_die_visit();
                    entry.source == HdSource::Racial
                })
                .count(),
        )
        .expect("admitted resident ledger count")
    }

    pub fn class_level(&self, class: &ClassId) -> HdCount {
        HdCount::from_usize(
            self.entries
                .iter()
                .filter(|entry| {
                    crate::diagnostics::raw_hit_die_visit();
                    matches!(&entry.source, HdSource::Class(id) if id == class)
                })
                .count(),
        )
        .expect("admitted resident ledger count")
    }

    /// Checked aggregate grants across independent pools and actual HD sources.
    pub fn advancement_points(&self, rules: &Ruleset) -> Result<u64, HdError> {
        self.entries
            .iter()
            .map(|entry| {
                crate::diagnostics::raw_hit_die_visit();
                rules
                    .point_grants(&entry.source)
                    .map(|grants| {
                        grants
                            .iter()
                            .try_fold(0_u64, |sum, grant| sum.checked_add(u64::from(grant.points)))
                            .ok_or(HdError::Arithmetic)
                    })
                    .ok_or(HdError::UnknownSource)?
            })
            .try_fold(0_u64, |sum, points| {
                sum.checked_add(points?).ok_or(HdError::Arithmetic)
            })
    }

    pub fn attribute_opportunities(&self, rules: &Ruleset) -> HdCount {
        HdCount(self.total_hd().get() / u64::from(rules.constants().attribute_opportunity_interval))
    }
    pub fn health_provenance(
        &self,
        kind: &TypeId,
        rules: &Ruleset,
    ) -> Result<Vec<HealthContribution>, HdError> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| entry.health(kind, rules, index == 0))
            .collect()
    }
    pub fn remove_latest(&mut self) -> Option<HitDie> {
        self.entries.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_append_matches_every_owned_ordinal_without_revisiting_the_root_prefix() {
        for root in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
            let expected =
                HdLedger::seeded(vec![HdSource::Racial; crate::limits::HIT_DICE], root).unwrap();
            let mut ledger = HdLedger::seeded(vec![], root).unwrap();
            for ordinal in 0..crate::limits::HIT_DICE {
                let draws_before = crate::dice::random_word_draws_for_test();
                ledger.append_seeded(HdSource::Racial, root).unwrap();
                let draws = crate::dice::random_word_draws_for_test() - draws_before;
                assert_eq!(ledger.entries(), &expected.entries()[..=ordinal]);
                assert_eq!(draws, 1, "root {root:#x}, zero-based ordinal {ordinal}");
            }
            assert_eq!(ledger, expected);
        }
    }

    #[test]
    fn recorded_health_roots_validate_every_owned_ordinal() {
        let ledger = HdLedger::seeded(vec![HdSource::Racial; 3], 42).unwrap();
        assert_eq!(
            HdLedger::from_recorded(ledger.entries().to_vec(), Some(42)),
            Ok(ledger.clone())
        );
        for ordinal in 0..3 {
            let mut entries = ledger.entries().to_vec();
            entries[ordinal].health_seed ^= 1;
            assert_eq!(
                HdLedger::from_recorded(entries, Some(42)),
                Err(HdError::InvalidHealthRoot)
            );
        }
    }

    #[test]
    fn empty_seeded_ledgers_preserve_zero_and_maximum_roots() {
        for root in [0, u64::MAX] {
            let mut ledger = HdLedger::seeded(vec![], root).unwrap();
            assert_eq!(ledger.health_root_seed(), Some(root));
            assert_eq!(
                HdLedger::from_recorded(vec![], Some(root)),
                Ok(ledger.clone())
            );
            ledger.append_seeded(HdSource::Racial, root).unwrap();
            assert_eq!(
                ledger,
                HdLedger::seeded(vec![HdSource::Racial], root).unwrap()
            );
        }
    }

    #[test]
    fn manual_ledgers_do_not_infer_a_root_from_later_seeded_appends() {
        for entries in [vec![], vec![HitDie::new(HdSource::Racial, 42)]] {
            let mut ledger = HdLedger::new(entries).unwrap();
            ledger.append_seeded(HdSource::Racial, 42).unwrap();
            assert_eq!(ledger.health_root_seed(), None);
            assert_eq!(
                HdLedger::from_recorded(ledger.entries().to_vec(), None),
                Ok(ledger)
            );
        }
    }
}
