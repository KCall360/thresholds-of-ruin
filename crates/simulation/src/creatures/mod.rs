//! Shared creature composition and source-owned advancement choices.
//! Derivation is pure; actor injury, timers, equipment and conditions live apart.

mod catalog;
mod derive;
pub mod record;
mod state;
mod view;
pub use catalog::{GrantSource, Species, Subtype, Template};
pub use derive::DerivedCreature;
pub use state::{CreatureState, CreatureStateError, DamageOutcome, RebuildOutcome};
pub use view::{OwnStats, ResourceView};

use crate::attributes::{Attribute, Attributes, ManaBinding, Skill};
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::talents::{Talent, TalentFacts};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    InvalidDefinition,
    ConflictingTemplates,
    InvalidOwnership,
    OccupiedSlot,
    DuplicateTalent,
    IneligibleTalent,
    NoTrainingPoint,
    RankCap,
    NoAttributeOpportunity,
    AttributeCap,
    DerivedLimit,
    TooManyHitDice,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HdChoices {
    pub training: Vec<Skill>,
    pub attribute: Option<Attribute>,
    pub talent: Option<Talent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedAdvancement {
    pub hit_die: HitDie,
    pub choices: HdChoices,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureBuild {
    species: Species,
    initial_attributes: Attributes,
    ledger: HdLedger,
    binding: ManaBinding,
    choices: Vec<HdChoices>,
    templates: Vec<Template>,
}

impl CreatureBuild {
    pub fn new(
        species: Species,
        ledger: HdLedger,
        binding: ManaBinding,
    ) -> Result<Self, BuildError> {
        let choices = vec![HdChoices::default(); usize::from(ledger.total_hd())];
        let initial_attributes = species.default_attributes;
        Self::from_recorded(
            species,
            initial_attributes,
            ledger,
            binding,
            choices,
            vec![],
        )
    }

    /// Restore recorded choices, including dormant talents. Authoring can
    /// additionally require no dormant choices; transformation must preserve them.
    pub fn from_recorded(
        species: Species,
        initial_attributes: Attributes,
        ledger: HdLedger,
        binding: ManaBinding,
        choices: Vec<HdChoices>,
        templates: Vec<Template>,
    ) -> Result<Self, BuildError> {
        let build = Self {
            species,
            initial_attributes,
            ledger,
            binding,
            choices,
            templates,
        };
        build.derive()?;
        Ok(build)
    }

    pub fn species(&self) -> &Species {
        &self.species
    }

    pub fn initial_attributes(&self) -> Attributes {
        self.initial_attributes
    }

    pub fn set_initial_attributes(&mut self, attributes: Attributes) -> Result<(), BuildError> {
        let mut candidate = self.clone();
        candidate.initial_attributes = attributes;
        candidate.derive()?;
        *self = candidate;
        Ok(())
    }
    pub fn ledger(&self) -> &HdLedger {
        &self.ledger
    }
    pub fn binding(&self) -> ManaBinding {
        self.binding
    }
    pub fn choices(&self) -> &[HdChoices] {
        &self.choices
    }
    pub fn templates(&self) -> &[Template] {
        &self.templates
    }

    pub fn derive(&self) -> Result<DerivedCreature, BuildError> {
        derive::derive(self)
    }

    pub fn unspent_talent_slots(&self) -> u16 {
        self.choices
            .iter()
            .filter(|choice| choice.talent.is_none())
            .count() as u16
    }

    pub fn train(&mut self, owner: usize, skill: Skill) -> Result<(), BuildError> {
        let entry = self
            .ledger
            .entries()
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        let budget = match entry.source() {
            HdSource::Racial => 1,
            HdSource::Class(_) => 2,
        };
        if self.choices[owner].training.len() >= budget {
            return Err(BuildError::NoTrainingPoint);
        }
        let mut candidate = self.clone();
        candidate.choices[owner].training.push(skill);
        candidate.derive()?;
        *self = candidate;
        Ok(())
    }

    pub fn increase_attribute(
        &mut self,
        owner: usize,
        attribute: Attribute,
    ) -> Result<(), BuildError> {
        let choice = self
            .choices
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        if !(owner + 1).is_multiple_of(4) {
            return Err(BuildError::NoAttributeOpportunity);
        }
        if choice.attribute.is_some() {
            return Err(BuildError::OccupiedSlot);
        }
        let mut candidate = self.clone();
        candidate.choices[owner].attribute = Some(attribute);
        candidate.derive()?;
        *self = candidate;
        Ok(())
    }

    pub fn select_talent(&mut self, owner: usize, talent: Talent) -> Result<(), BuildError> {
        let choice = self
            .choices
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        if choice.talent.is_some() {
            return Err(BuildError::OccupiedSlot);
        }
        if self
            .choices
            .iter()
            .any(|choice| choice.talent == Some(talent))
        {
            return Err(BuildError::DuplicateTalent);
        }
        let derived = self.derive()?;
        if !talent.eligible(&self.talent_facts(&derived), &derived.active_talents) {
            return Err(BuildError::IneligibleTalent);
        }
        let mut candidate = self.clone();
        candidate.choices[owner].talent = Some(talent);
        candidate.derive()?;
        *self = candidate;
        Ok(())
    }

    pub fn available_talents(&self) -> Result<BTreeSet<Talent>, BuildError> {
        let derived = self.derive()?;
        let facts = self.talent_facts(&derived);
        let selected: BTreeSet<_> = self
            .choices
            .iter()
            .filter_map(|choice| choice.talent)
            .collect();
        Ok(Talent::ALL
            .into_iter()
            .filter(|talent| {
                !selected.contains(talent) && talent.eligible(&facts, &derived.active_talents)
            })
            .collect())
    }

    fn talent_facts<'a>(&'a self, derived: &DerivedCreature) -> TalentFacts<'a> {
        TalentFacts {
            ledger: &self.ledger,
            kind: derived.kind,
            attributes: derived.attributes,
            skills: derived.skills,
            melee_skill: self.species.melee.skill(),
            magical: derived.magical,
            mindless: derived.mindless,
        }
    }

    pub fn set_templates(&mut self, templates: Vec<Template>) -> Result<(), BuildError> {
        let mut candidate = self.clone();
        candidate.templates = templates;
        candidate.derive()?;
        *self = candidate;
        Ok(())
    }

    /// Add one source-owned advancement slot using the actor's original health
    /// stream. The candidate preserves all retained seeds and recorded choices.
    pub fn add_hit_die(&mut self, source: HdSource, root_seed: u64) -> Result<(), BuildError> {
        let mut candidate = self.clone();
        candidate
            .ledger
            .append_seeded(source, root_seed)
            .map_err(|_| BuildError::TooManyHitDice)?;
        candidate.choices.push(HdChoices::default());
        candidate.derive()?;
        *self = candidate;
        Ok(())
    }

    pub fn remove_latest(&mut self) -> Option<RemovedAdvancement> {
        let hit_die = self.ledger.remove_latest()?;
        let choices = self
            .choices
            .pop()
            .expect("choices track validated ledger ownership");
        Some(RemovedAdvancement { hit_die, choices })
    }
}
