//! Shared catalog references, source-owned advancement and cached derivation.
mod advancement;
mod catalog;
mod derive;
pub mod record;
mod state;
mod view;
pub use advancement::{
    AdvancementPreview, AdvancementPurchase, BuildAdvancement, HdChoices, PlannedAdvancement,
    PointPoolBalance, PurchaseSelection, RestorationPolicy,
};
pub use catalog::{Content, ScenarioCatalog, SourceId, Species, Template};
pub use derive::{
    AbilityTiming, AbilityValues, CheckContext, DerivedCreature, DormantReason, SelectedAttack,
};
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
    NoAdvancementPoint,
    UnknownPointPool,
    PurchaseNotAllowed,
    InvalidStartingSkills,
    NoAuthoredRestoration,
    RankCap,
    NoAttributeOpportunity,
    AttributeCap,
    DerivedLimit,
    TooManyHitDice,
    TooManyOwnedTalents,
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
    advancement: BuildAdvancement,
    templates: Vec<TemplateId>,
}
impl CreatureBuild {
    pub fn new(
        content: Arc<Content>,
        species: SpeciesId,
        ledger: HdLedger,
        binding: ManaBinding,
        starting_skills: [Skill; 2],
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
            BuildAdvancement {
                starting_skills,
                choices,
                restoration: RestorationPolicy::ChooseAgain,
            },
            vec![],
        )
    }
    pub fn from_recorded(
        content: Arc<Content>,
        species: SpeciesId,
        initial_attributes: Attributes,
        ledger: HdLedger,
        binding: ManaBinding,
        advancement: BuildAdvancement,
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
            advancement,
            templates,
        };
        build.derive()?;
        if let RestorationPolicy::Authored(plan) = build.restoration_policy() {
            if plan.is_empty() || plan.len() > crate::limits::HIT_DICE {
                return Err(BuildError::InvalidDefinition);
            }
            if build
                .ledger()
                .entries()
                .iter()
                .zip(plan)
                .any(|(entry, intended)| entry.source() != intended.source)
            {
                return Err(BuildError::InvalidOwnership);
            }
            let mut intended = build.clone();
            intended.ledger =
                HdLedger::seeded(plan.iter().map(|entry| entry.source.clone()).collect(), 0)
                    .map_err(|_| BuildError::TooManyHitDice)?;
            intended.advancement.choices = plan.iter().map(|entry| entry.choices.clone()).collect();
            // Future authored choices are provenance, not choices currently
            // applied to this actor. Retained edits may temporarily block a
            // later restoration without making the current build unsaveable.
            derive::advancement_facts(&intended, Attributes::default())?;
        }
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
        &self.advancement.choices
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
    /// Preview and commit use one candidate clone and one permanent derivation.
    fn preview_edit(
        &self,
        f: impl FnOnce(&mut Self) -> Result<(), BuildError>,
    ) -> Result<(Self, DerivedCreature), BuildError> {
        let mut candidate = self.clone();
        f(&mut candidate)?;
        let derived = candidate.derive()?;
        let owned: BTreeSet<_> = self
            .choices()
            .iter()
            .flat_map(|choice| choice.talents().cloned())
            .collect();
        for talent in candidate.choices().iter().flat_map(HdChoices::talents) {
            if !owned.contains(talent) && !derived.active_talents.contains(talent) {
                return Err(BuildError::IneligibleTalent);
            }
        }
        Ok((candidate, derived))
    }
    fn edit(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<(), BuildError>,
    ) -> Result<DerivedCreature, BuildError> {
        let (candidate, derived) = self.preview_edit(f)?;
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
    pub fn increase_attribute(
        &mut self,
        owner: usize,
        attribute: Attribute,
    ) -> Result<(), BuildError> {
        self.edit(|build| {
            let interval = usize::from(build.ruleset().constants().attribute_opportunity_interval);
            let choice = build
                .advancement
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
    pub fn available_talents(&self) -> Result<BTreeSet<TalentId>, BuildError> {
        let derived = self.derive()?;
        let facts = derive::PermanentFacts::new(self, &derived);
        let selected: BTreeSet<_> = self
            .choices()
            .iter()
            .flat_map(|choice| choice.talents().cloned())
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
        if let RestorationPolicy::Authored(plan) = self.restoration_policy() {
            if let Some(intended) = plan.get(self.choices().len()) {
                if intended.source != source {
                    return Err(BuildError::InvalidOwnership);
                }
                return self.restore_hit_die(root_seed);
            }
        }
        self.edit(|build| {
            build
                .ledger
                .append_seeded(source, root_seed)
                .map_err(|_| BuildError::TooManyHitDice)?;
            build.advancement.choices.push(HdChoices::default());
            Ok(())
        })
        .map(|_| ())
    }
    pub fn remove_latest(&mut self) -> Option<RemovedAdvancement> {
        let hit_die = self.ledger.remove_latest()?;
        let choices = self
            .advancement
            .choices
            .pop()
            .expect("one choice per hit die in this build");
        Some(RemovedAdvancement { hit_die, choices })
    }
}
