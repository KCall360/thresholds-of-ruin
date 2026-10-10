//! Immutable Hit Dice records and reversible, current-composition derivation.
//!
//! The ledger never consumes combat randomness. Each record owns a separate
//! health seed, interpreted against its current die by deterministic sampling.

use crate::dice::{die, random_word};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CreatureType {
    Aberration,
    Animal,
    Construct,
    Dragon,
    Elemental,
    Fey,
    Giant,
    Humanoid,
    MagicalBeast,
    MonstrousHumanoid,
    Ooze,
    Outsider,
    Plant,
    Undead,
    Vermin,
}

impl CreatureType {
    pub const ALL: [Self; 15] = [
        Self::Aberration,
        Self::Animal,
        Self::Construct,
        Self::Dragon,
        Self::Elemental,
        Self::Fey,
        Self::Giant,
        Self::Humanoid,
        Self::MagicalBeast,
        Self::MonstrousHumanoid,
        Self::Ooze,
        Self::Outsider,
        Self::Plant,
        Self::Undead,
        Self::Vermin,
    ];

    pub fn health_die(self) -> u16 {
        match self {
            Self::Fey => 6,
            Self::Construct | Self::MagicalBeast | Self::Ooze => 10,
            Self::Dragon | Self::Undead => 12,
            _ => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Warrior,
    Mage,
}

impl Class {
    pub fn health_die(self) -> u16 {
        match self {
            Self::Warrior => 10,
            Self::Mage => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdSource {
    Racial,
    Class(Class),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    pub fn source(self) -> HdSource {
        self.source
    }

    pub fn health_seed(self) -> u64 {
        self.health_seed
    }

    fn health(self, kind: CreatureType, first: bool) -> u32 {
        let sides = match self.source {
            HdSource::Racial => kind.health_die(),
            HdSource::Class(class) => class.health_die(),
        };
        if first {
            return u32::from(sides);
        }
        let mut state = self.health_seed;
        u32::from(die(&mut state, sides))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdError {
    TooManyHitDice,
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
        if sources.len() > 256 {
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
        if self.entries.len() >= 256 {
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
        if entries.len() > 256 {
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

    pub fn class_level(&self, class: Class) -> u16 {
        self.entries
            .iter()
            .filter(|entry| entry.source == HdSource::Class(class))
            .count() as u16
    }

    pub fn training_points(&self) -> u16 {
        self.entries
            .iter()
            .map(|entry| match entry.source {
                HdSource::Racial => 1,
                HdSource::Class(_) => 2,
            })
            .sum()
    }

    pub fn attribute_opportunities(&self) -> u16 {
        self.total_hd() / 4
    }

    pub fn talent_slots(&self) -> u16 {
        self.total_hd()
    }

    pub fn health_contributions(&self, kind: CreatureType) -> Vec<u32> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| entry.health(kind, index == 0))
            .collect()
    }

    pub fn health(&self, kind: CreatureType) -> u32 {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| entry.health(kind, index == 0))
            .sum()
    }

    pub fn remove_latest(&mut self) -> Option<HitDie> {
        self.entries.pop()
    }
}
