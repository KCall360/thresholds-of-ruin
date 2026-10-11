//! Immutable Hit Dice records and reversible, current-composition derivation.
//!
//! The ledger never consumes combat randomness. Each record owns a separate
//! health seed, interpreted against its current die by deterministic sampling.

use crate::dice::{die, random_word};

use crate::content_ids::{ClassId, TypeId};
use crate::rules::Ruleset;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HdSource {
    Racial,
    Class(ClassId),
}

/// Cached derivation facts for one owned hit die. Never part of a save record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthContribution {
    pub die: u16,
    pub health: u32,
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
    TooManyHitDice,
    UnknownSource,
}

/// Ordered advancement records. No caller can mutate a retained die's source
/// or health seed. A zero-HD ledger represents regression to persistent death;
/// actor state, rather than this definition, owns whether death has occurred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HdLedger {
    entries: Vec<HitDie>,
}

impl HdLedger {
    /// Assign independent health streams in advancement order. Class/type
    /// choices do not change the assigned stream; combat RNG is never borrowed.
    pub fn seeded(sources: Vec<HdSource>, seed: u64) -> Result<Self, HdError> {
        if sources.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        let mut state = seed;
        Self::new(
            sources
                .into_iter()
                .map(|source| HitDie::new(source, random_word(&mut state)))
                .collect(),
        )
    }

    /// Append from the actor's original health stream without changing retained
    /// records. Removing and re-adding an ordinal assigns the same health seed.
    pub fn append_seeded(&mut self, source: HdSource, root_seed: u64) -> Result<(), HdError> {
        if self.entries.len() >= crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        let mut stream = root_seed;
        for _ in 0..self.entries.len() {
            random_word(&mut stream);
        }
        let health_seed = random_word(&mut stream);
        self.entries.push(HitDie::new(source, health_seed));
        Ok(())
    }

    pub fn new(entries: Vec<HitDie>) -> Result<Self, HdError> {
        if entries.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[HitDie] {
        &self.entries
    }

    pub fn total_hd(&self) -> u16 {
        self.entries.len() as u16
    }

    pub fn racial_hd(&self) -> u16 {
        self.entries
            .iter()
            .filter(|entry| entry.source == HdSource::Racial)
            .count() as u16
    }

    pub fn class_level(&self, class: &ClassId) -> u16 {
        self.entries
            .iter()
            .filter(|entry| matches!(&entry.source, HdSource::Class(id) if id == class))
            .count() as u16
    }

    pub fn training_points(&self, rules: &Ruleset) -> Result<u16, HdError> {
        self.entries
            .iter()
            .map(|entry| match &entry.source {
                HdSource::Racial => Ok(rules.constants().racial_training_points),
                HdSource::Class(class) => rules
                    .class(class)
                    .map(|definition| definition.training_points)
                    .ok_or(HdError::UnknownSource),
            })
            .sum()
    }

    pub fn attribute_opportunities(&self, rules: &Ruleset) -> u16 {
        self.total_hd() / rules.constants().attribute_opportunity_interval
    }
    pub fn talent_slots(&self) -> u16 {
        self.total_hd()
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
