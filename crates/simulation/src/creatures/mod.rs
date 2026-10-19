//! Shared catalog references, source-owned advancement and cached derivation.
mod advancement;
mod build_epoch;
mod catalog;
mod derive;
mod displacement;
mod expertise_learning;
#[cfg(test)]
mod expertise_learning_tests;
#[cfg(test)]
mod growth_failure_tests;
#[cfg(test)]
mod hit_die_effect_tests;
mod intellect;
#[cfg(test)]
mod intellect_tests;
mod losses;
mod projectile_edges;
pub(crate) use derive::{ability_timing_for_attack, scaled_duration};
pub use displacement::{CreatureSize, DisplacementProfile};
pub use losses::{HitDieRestorationBlocker, RestorableLossOutcome, RestorableLosses};
pub mod record;
mod replacement;
mod state;
mod view;
pub use advancement::{
    AdvancementPreview, AdvancementPurchase, BuildAdvancement, HdChoices, PlannedAdvancement,
    PointPoolBalance, PurchaseSelection, RestorationPolicy,
};
pub use build_epoch::BuildEpoch;
pub use catalog::{Content, ScenarioCatalog, SourceId, Species, Template};
pub use derive::{
    AbilityTiming, AbilityValues, CheckContext, DerivedCreature, DormantReason, EquippedAttack,
    HitDiceSummary, SelectedAttack,
};
pub use projectile_edges::CapturedAttackEdges;
pub use replacement::{
    AdvancementReplacementPreview, PointRefund, PurchaseLocation, RemovedPurchase,
};
pub use state::{
    ActiveTimeOutcome, CreatureState, CreatureStateError, DamageOutcome, EquipmentSource,
    RebuildOutcome,
};
pub use view::{OwnStats, PersonalCondition, ResourceView};

use crate::attributes::{Attribute, Attributes, ManaBinding, Skill};
use crate::content_ids::{SpeciesId, TalentId, TemplateId};
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::rules::Ruleset;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    InvalidDefinition,
    /// Source-owned interaction contradiction; explicit candidates cannot evict it away.
    InvalidInteraction,
    UnknownSpecies,
    UnknownTemplate,
    UnknownTalent,
    UnknownExpertise,
    DuplicateExpertise,
    TooManyOwnedExpertises,
    InvalidTalentForm,
    UnknownClass,
    ConflictingTemplates,
    ConflictingOverride,
    InvalidOwnership,
    OccupiedSlot,
    DuplicateTalent,
    IneligibleTalent,
    NoAdvancementPoint,
    NoAdvancementChange,
    StaleAdvancementPreview,
    UnknownHealthStream,
    /// A supplied root contradicts the retained ordinal Health stream.
    InvalidHealthRoot,
    UnknownPointPool,
    PurchaseNotAllowed,
    InvalidStartingSkills,
    NoAuthoredRestoration,
    RankCap,
    NoAttributeOpportunity,
    AttributeCap,
    /// An authored numeric/operand bound, distinct from machine capacity.
    DerivedLimit,
    /// Checked arithmetic or a machine representation was exhausted.
    Arithmetic,
    Allocation,
    TooManyHitDice,
    TooManyOwnedTalents,
}
impl From<crate::progression::HdError> for BuildError {
    fn from(error: crate::progression::HdError) -> Self {
        match error {
            crate::progression::HdError::Arithmetic => Self::Arithmetic,
            crate::progression::HdError::TooManyHitDice => Self::TooManyHitDice,
            // A missing type/class in a catalog-normalized ledger is malformed.
            crate::progression::HdError::UnknownSource => Self::InvalidDefinition,
            crate::progression::HdError::InvalidHealthRoot => Self::InvalidHealthRoot,
            crate::progression::HdError::InvalidHealthImprovement => Self::InvalidDefinition,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedAdvancement {
    pub hit_die: HitDie,
    pub choices: HdChoices,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CreatureBuild {
    content: Arc<Content>,
    species: SpeciesId,
    initial_attributes: Attributes,
    ledger: HdLedger,
    binding: ManaBinding,
    advancement: BuildAdvancement,
    intellect_learning: Option<Arc<expertise_learning::IntellectLearning>>,
    future_growth: crate::earned_growth::FutureGrowth,
    templates: Vec<TemplateId>,
}
impl Clone for CreatureBuild {
    fn clone(&self) -> Self {
        crate::diagnostics::creature_build_copy();
        Self {
            content: self.content.clone(),
            species: self.species.clone(),
            initial_attributes: self.initial_attributes,
            ledger: self.ledger.clone(),
            binding: self.binding,
            advancement: self.advancement.clone(),
            intellect_learning: self.intellect_learning.clone(),
            future_growth: self.future_growth.clone(),
            templates: self.templates.clone(),
        }
    }
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
        let choices = vec![HdChoices::default(); ledger.entries().len()];
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
        let mut build = Self {
            content,
            species,
            initial_attributes,
            ledger,
            binding,
            advancement,
            intellect_learning: None,
            future_growth: crate::earned_growth::FutureGrowth::Disabled {},
            templates,
        };
        let kind = derive::kind_for_templates(&build, &build.templates)?;
        build.bind_health_origins(&kind)?;
        let derived = build.derive()?;
        build.intellect_learning = Some(Arc::new(expertise_learning::IntellectLearning::new(
            &build,
            derived.intellect_sources.clone(),
            derived.intellect_capacity,
        )?));
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
                    .map_err(BuildError::from)?;
            intended.advancement.choices = plan.iter().map(|entry| entry.choices.clone()).collect();
            // Future authored choices are provenance, not choices currently
            // applied to this actor. Retained edits may temporarily block a
            // later restoration without making the current build unsaveable.
            derive::advancement_facts(&intended, Attributes::default())?;
        }
        Ok(build)
    }
    fn bind_health_origins(&mut self, kind: &crate::content_ids::TypeId) -> Result<(), BuildError> {
        // Fresh source requests retain their public typed error. Bound rows
        // instead carry provenance, whose missing references remain malformed.
        if self
            .ledger
            .entries()
            .iter()
            .chain(self.ledger.drained().map(|(_, entry)| entry))
            .any(|entry| {
                entry.health_origin().is_none()
                    && matches!(entry.source(), HdSource::Class(id) if self.ruleset().class(&id).is_none())
            })
        {
            return Err(BuildError::UnknownClass);
        }
        self.ledger
            .bind_health_origins(kind, &self.templates, self.content.rules())
            .map_err(BuildError::from)?;
        for entry in self
            .ledger
            .entries()
            .iter()
            .chain(self.ledger.drained().map(|(_, entry)| entry))
        {
            let origin = entry.health_origin().ok_or(BuildError::InvalidDefinition)?;
            if derive::kind_for_templates(self, &origin.templates)? != origin.kind {
                return Err(BuildError::InvalidDefinition);
            }
        }
        Ok(())
    }
    pub(crate) fn stage_health_improvement(
        &mut self,
        claim: &crate::progression::HitDieHealthImprovement,
    ) -> Result<(), BuildError> {
        self.ledger.improve_health(claim).map_err(BuildError::from)
    }
    /// Only consulted at an explicit build replacement boundary. Future growth
    /// and restoration instructions do not change the currently owned sheet.
    fn same_permanent_inputs(&self, other: &Self) -> bool {
        crate::diagnostics::permanent_input_comparison();
        (Arc::ptr_eq(&self.content, &other.content) || self.content == other.content)
            && self.species == other.species
            && self.initial_attributes == other.initial_attributes
            && self.ledger == other.ledger
            && self.binding == other.binding
            && self.advancement.starting_skills == other.advancement.starting_skills
            && self.advancement.choices == other.advancement.choices
            && self.templates == other.templates
            && match (&self.intellect_learning, &other.intellect_learning) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
                (None, None) => true,
                _ => false,
            }
    }
    pub fn content(&self) -> &Arc<Content> {
        &self.content
    }
    pub fn future_growth(&self) -> &crate::earned_growth::FutureGrowth {
        &self.future_growth
    }
    pub fn set_future_growth(
        &mut self,
        policy: crate::earned_growth::FutureGrowth,
    ) -> Result<(), BuildError> {
        match &policy {
            crate::earned_growth::FutureGrowth::Selected { source }
                if !source.valid(self.ruleset()) =>
            {
                return Err(BuildError::UnknownClass);
            }
            crate::earned_growth::FutureGrowth::Authored {}
                if !matches!(self.restoration_policy(), RestorationPolicy::Authored(_)) =>
            {
                return Err(BuildError::NoAuthoredRestoration);
            }
            _ => {}
        }
        self.future_growth = policy;
        Ok(())
    }
    pub fn ruleset(&self) -> &Ruleset {
        &self.content.rules
    }
    pub fn species_id(&self) -> &SpeciesId {
        &self.species
    }
    pub fn species(&self) -> &Species {
        crate::diagnostics::raw_species_lookup();
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
        let kind = derive::kind_for_templates(&candidate, &candidate.templates)?;
        candidate.bind_health_origins(&kind)?;
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
    fn preview_intellect_advancement(
        &self,
        f: impl FnOnce(&mut Self) -> Result<(), BuildError>,
    ) -> Result<(Self, DerivedCreature), BuildError> {
        let (mut candidate, derived) = self.preview_edit(f)?;
        candidate.retain_intellect_acquisition(self, &derived)?;
        Ok((candidate, derived))
    }
    fn retain_intellect_acquisition(
        &mut self,
        before: &Self,
        derived: &DerivedCreature,
    ) -> Result<(), BuildError> {
        if self.intellect_learning.as_ref().is_some_and(|learning| {
            usize::from(derived.intellect_capacity) <= learning.thresholds.len()
        }) {
            return Ok(());
        }
        let Some(mut learning) = self.intellect_learning.take() else {
            return Ok(());
        };
        let previous = intellect::current_sources(before, &learning.birth.templates)?;
        Arc::make_mut(&mut learning).record_acquisition(
            before,
            self,
            previous,
            derived.intellect_sources.clone(),
            derived.intellect_capacity,
        )?;
        self.intellect_learning = Some(learning);
        Ok(())
    }
    fn edit_intellect_advancement(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<(), BuildError>,
    ) -> Result<DerivedCreature, BuildError> {
        let (candidate, derived) = self.preview_intellect_advancement(f)?;
        *self = candidate;
        Ok(derived)
    }
    pub fn select_intellect_expertise(
        &mut self,
        threshold: u16,
        expertise: crate::content_ids::ExpertiseId,
    ) -> Result<(), BuildError> {
        let derived = self.derive()?;
        if self.ruleset().expertise(&expertise).is_none() {
            return Err(BuildError::UnknownExpertise);
        }
        if derived.expertises.owns(&expertise) {
            return Err(BuildError::DuplicateExpertise);
        }
        if self
            .ruleset()
            .expertise(&expertise)
            .is_some_and(|definition| {
                definition.category == crate::expertise::ExpertiseCategory::Specialist
            })
        {
            return Err(BuildError::PurchaseNotAllowed);
        }
        self.edit(|build| {
            Arc::make_mut(
                build
                    .intellect_learning
                    .as_mut()
                    .ok_or(BuildError::InvalidOwnership)?,
            )
            .choose(threshold, expertise, derived.intellect_capacity)
        })
        .map(|_| ())
    }
    pub fn intellect_expertise_choices(
        &self,
    ) -> impl Iterator<Item = (u16, Option<&crate::content_ids::ExpertiseId>)> {
        self.intellect_learning
            .iter()
            .flat_map(|learning| learning.thresholds.iter().enumerate())
            .map(|(index, row)| ((index + 1) as u16, row.expertise.as_ref()))
    }
    pub fn set_initial_attributes(&mut self, attributes: Attributes) -> Result<(), BuildError> {
        if attributes == self.initial_attributes {
            return Ok(());
        }
        if self.intellect_learning.as_ref().is_some_and(|learning| {
            !learning.acquisitions.is_empty()
                || learning
                    .thresholds
                    .iter()
                    .any(|row| row.expertise.is_some())
        }) {
            // Privileged baseline rewrites cannot rewrite already learned history.
            return Err(BuildError::InvalidOwnership);
        }
        let (mut candidate, _) = self.preview_edit(|build| {
            build.initial_attributes = attributes;
            Ok(())
        })?;
        let templates = self.intellect_learning.as_ref().map_or_else(
            || self.template_ids().to_vec(),
            |learning| learning.birth.templates.clone(),
        );
        let sources = intellect::current_sources(&candidate, &templates)?;
        let capacity = intellect::eligible_intellect(&candidate, &templates, &sources)?;
        let mut learning =
            expertise_learning::IntellectLearning::new(&candidate, sources, capacity)?;
        learning.birth.templates = templates;
        candidate.intellect_learning = Some(Arc::new(learning));
        *self = candidate;
        Ok(())
    }
    pub fn increase_attribute(
        &mut self,
        owner: crate::HdIndex,
        attribute: Attribute,
    ) -> Result<(), BuildError> {
        let ordinal = owner.ordinal().map_err(BuildError::from)?.get();
        let owner = owner.to_usize().map_err(BuildError::from)?;
        self.edit_intellect_advancement(|build| {
            let interval = u64::from(build.ruleset().constants().attribute_opportunity_interval);
            let choice = build
                .advancement
                .choices
                .get_mut(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            if !ordinal.is_multiple_of(interval) {
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
        self.edit_intellect_advancement(|build| {
            build
                .ledger
                .append_seeded(source, root_seed)
                .map_err(BuildError::from)?;
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

#[cfg(test)]
mod healing_improvement_tests;
