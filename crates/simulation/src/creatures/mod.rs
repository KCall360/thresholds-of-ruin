//! Shared catalog references, source-owned advancement and cached derivation.
mod catalog;
mod derive;
pub mod record;
mod state;
mod view;
pub use catalog::{Content, ScenarioCatalog, SourceId, Species, Template};
pub use derive::{AbilityTiming, AbilityValues, CheckContext, DerivedCreature, SelectedAttack};
pub use state::{
    ActiveTimeOutcome, CreatureState, CreatureStateError, DamageOutcome, EquipmentSource,
    RebuildOutcome,
};
pub use view::{OwnStats, ResourceView};

use crate::attributes::{Attribute, Attributes, ManaBinding, Skill};
use crate::content_ids::{SpeciesId, TalentId, TemplateId};
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::rules::Ruleset;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    InvalidDefinition,
    UnknownSpecies,
    UnknownTemplate,
    UnknownTalent,
    UnknownClass,
    ConflictingTemplates,
    ConflictingOverride,
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
    pub talent: Option<TalentId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedAdvancement {
    pub hit_die: HitDie,
    pub choices: HdChoices,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureBuild {
    content: Arc<Content>,
    species: SpeciesId,
    initial_attributes: Attributes,
    ledger: HdLedger,
    binding: ManaBinding,
    choices: Vec<HdChoices>,
    templates: Vec<TemplateId>,
}
impl CreatureBuild {
    pub fn new(
        content: Arc<Content>,
        species: SpeciesId,
        ledger: HdLedger,
        binding: ManaBinding,
    ) -> Result<Self, BuildError> {
        let initial_attributes = content
            .catalog
            .species
            .get(&species)
            .ok_or(BuildError::UnknownSpecies)?
            .default_attributes;
        let choices = vec![HdChoices::default(); usize::from(ledger.total_hd())];
        Self::from_recorded(
            content,
            species,
            initial_attributes,
            ledger,
            binding,
            choices,
            vec![],
        )
    }
    pub fn from_recorded(
        content: Arc<Content>,
        species: SpeciesId,
        initial_attributes: Attributes,
        ledger: HdLedger,
        binding: ManaBinding,
        choices: Vec<HdChoices>,
        templates: Vec<TemplateId>,
    ) -> Result<Self, BuildError> {
        let species = content
            .catalog
            .species
            .get_key_value(&species)
            .ok_or(BuildError::UnknownSpecies)?
            .0
            .clone();
        let templates = templates
            .into_iter()
            .map(|id| {
                content
                    .catalog
                    .templates
                    .get_key_value(&id)
                    .map(|(id, _)| id.clone())
                    .ok_or(BuildError::UnknownTemplate)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let build = Self {
            content,
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
    pub fn content(&self) -> &Arc<Content> {
        &self.content
    }
    pub fn ruleset(&self) -> &Ruleset {
        &self.content.rules
    }
    pub fn species_id(&self) -> &SpeciesId {
        &self.species
    }
    pub fn species(&self) -> &Species {
        // Construction validates this immutable catalog reference in the same object.
        &self.content.catalog.species[&self.species]
    }
    pub fn initial_attributes(&self) -> Attributes {
        self.initial_attributes
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
    pub fn template_ids(&self) -> &[TemplateId] {
        &self.templates
    }
    pub fn templates(&self) -> impl Iterator<Item = &Template> {
        self.templates
            .iter()
            .filter_map(|id| self.content.catalog.templates.get(id))
    }
    pub fn derive(&self) -> Result<DerivedCreature, BuildError> {
        derive::derive(self)
    }
    /// Every edit validates a candidate once, then atomically replaces the choices.
    fn edit(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<(), BuildError>,
    ) -> Result<DerivedCreature, BuildError> {
        let mut candidate = self.clone();
        f(&mut candidate)?;
        let derived = candidate.derive()?;
        for choice in &candidate.choices {
            if let Some(talent) = &choice.talent {
                if !self
                    .choices
                    .iter()
                    .any(|old| old.talent.as_ref() == Some(talent))
                    && !derived.active_talents.contains(talent)
                {
                    return Err(BuildError::IneligibleTalent);
                }
            }
        }
        *self = candidate;
        Ok(derived)
    }
    pub fn set_initial_attributes(&mut self, attributes: Attributes) -> Result<(), BuildError> {
        self.edit(|build| {
            build.initial_attributes = attributes;
            Ok(())
        })
        .map(|_| ())
    }
    pub fn unspent_talent_slots(&self) -> u16 {
        self.choices
            .iter()
            .filter(|choice| choice.talent.is_none())
            .count() as u16
    }
    pub fn train(&mut self, owner: usize, skill: Skill) -> Result<(), BuildError> {
        self.edit(|build| {
            let entry = build
                .ledger
                .entries()
                .get(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            let budget = match entry.source() {
                HdSource::Racial => build.ruleset().constants().racial_training_points,
                HdSource::Class(class) => {
                    build
                        .ruleset()
                        .class(&class)
                        .ok_or(BuildError::UnknownClass)?
                        .training_points
                }
            };
            let choice = build
                .choices
                .get_mut(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            if choice.training.len() >= usize::from(budget) {
                return Err(BuildError::NoTrainingPoint);
            }
            choice.training.push(skill);
            Ok(())
        })
        .map(|_| ())
    }
    pub fn increase_attribute(
        &mut self,
        owner: usize,
        attribute: Attribute,
    ) -> Result<(), BuildError> {
        self.edit(|build| {
            let interval = usize::from(build.ruleset().constants().attribute_opportunity_interval);
            let choice = build
                .choices
                .get_mut(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            if !(owner + 1).is_multiple_of(interval) {
                return Err(BuildError::NoAttributeOpportunity);
            }
            if choice.attribute.is_some() {
                return Err(BuildError::OccupiedSlot);
            }
            choice.attribute = Some(attribute);
            Ok(())
        })
        .map(|_| ())
    }
    pub fn select_talent(&mut self, owner: usize, talent: TalentId) -> Result<(), BuildError> {
        self.edit(|build| {
            if build.ruleset().talent(&talent).is_none() {
                return Err(BuildError::UnknownTalent);
            }
            if build
                .choices
                .iter()
                .any(|choice| choice.talent.as_ref() == Some(&talent))
            {
                return Err(BuildError::DuplicateTalent);
            }
            let choice = build
                .choices
                .get_mut(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            if choice.talent.is_some() {
                return Err(BuildError::OccupiedSlot);
            }
            choice.talent = Some(talent);
            Ok(())
        })
        .map(|_| ())
    }
    pub fn available_talents(&self) -> Result<BTreeSet<TalentId>, BuildError> {
        let derived = self.derive()?;
        let facts = derive::PermanentFacts::new(self, &derived);
        let selected: BTreeSet<_> = self
            .choices
            .iter()
            .filter_map(|choice| choice.talent.clone())
            .collect();
        Ok(self
            .ruleset()
            .talent_order()
            .iter()
            .filter(|id| {
                !selected.contains(*id)
                    && self.ruleset().talent(id).is_some_and(|definition| {
                        definition
                            .requirement
                            .satisfied(&facts, &derived.active_talents)
                            && definition
                                .prerequisites
                                .iter()
                                .all(|p| derived.active_talents.contains(p))
                    })
            })
            .cloned()
            .collect())
    }
    pub fn set_templates(&mut self, templates: Vec<TemplateId>) -> Result<(), BuildError> {
        self.edit(|build| {
            build.templates = templates
                .into_iter()
                .map(|id| {
                    build
                        .content
                        .catalog
                        .templates
                        .get_key_value(&id)
                        .map(|(id, _)| id.clone())
                        .ok_or(BuildError::UnknownTemplate)
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(())
        })
        .map(|_| ())
    }
    pub fn add_hit_die(&mut self, source: HdSource, root_seed: u64) -> Result<(), BuildError> {
        self.edit(|build| {
            build
                .ledger
                .append_seeded(source, root_seed)
                .map_err(|_| BuildError::TooManyHitDice)?;
            build.choices.push(HdChoices::default());
            Ok(())
        })
        .map(|_| ())
    }
    pub fn remove_latest(&mut self) -> Option<RemovedAdvancement> {
        let hit_die = self.ledger.remove_latest()?;
        let choices = self
            .choices
            .pop()
            .expect("one choice per hit die in this build");
        Some(RemovedAdvancement { hit_die, choices })
    }
}
