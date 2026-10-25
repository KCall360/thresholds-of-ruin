//! Validated scenario-owned definitions, shared by every actor in a game.
use super::BuildError;
use crate::attributes::{Attribute, Attributes, ByAttribute};
use crate::content_ids::{
    ClassId, ConditionId, SpeciesId, SubtypeId, TalentId, TemplateId, TypeId,
};
use crate::rules::{Modifier, Ruleset};
use crate::{ActorId, AnatomySpec, ItemId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceId {
    Species(SpeciesId),
    Type(TypeId),
    Subtype(SubtypeId),
    Class(ClassId),
    Template(TemplateId),
    Talent(TalentId),
    Equipment(ItemId),
    Condition(ConditionId, ActorId),
    QualifiedCondition {
        condition: ConditionId,
        source: ActorId,
        qualification: crate::conditions::TokenQualification,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Species {
    pub behavior: crate::behavior::BehaviorPolicy,
    pub displacement: Option<super::DisplacementProfile>,
    pub id: SpeciesId,
    pub kind: TypeId,
    pub subtypes: BTreeSet<SubtypeId>,
    pub default_attributes: Attributes,
    pub anatomy: AnatomySpec,
    pub melee: crate::attacks::NaturalAttack,
    pub modifiers: Vec<Modifier>,
}
impl Species {
    fn validate(&self, rules: &Ruleset) -> Result<(), BuildError> {
        if self.melee.strikes().iter().any(|strike| {
            [&strike.on_success, &strike.on_failure]
                .into_iter()
                .chain(strike.riders.iter().map(|rider| &rider.effects))
                .flatten()
                .any(|effect| {
                    effect.spatial()
                        || matches!(
                            effect,
                            crate::rules::Effect::RoomLight
                                | crate::rules::Effect::DrainLife
                                | crate::rules::Effect::DetectMonsters { .. }
                                | crate::rules::Effect::DetectFood
                                | crate::rules::Effect::MapTopology
                                | crate::rules::Effect::DetectMagic
                        )
                })
        }) {
            return Err(BuildError::InvalidDefinition);
        }
        if !self.behavior.valid()
            || !self.anatomy.valid(rules)
            || rules.creature_type(&self.kind).is_none()
            || self.subtypes.iter().any(|id| rules.subtype(id).is_none())
            || Attribute::ALL.into_iter().any(|attribute| {
                self.default_attributes.get(attribute) > rules.constants().attribute_cap
            })
        {
            return Err(BuildError::InvalidDefinition);
        }
        rules
            .validate_strikes(self.melee.strikes())
            .map_err(|_| BuildError::InvalidDefinition)?;
        rules
            .validate_modifiers(&self.modifiers)
            .map_err(|_| BuildError::InvalidDefinition)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pub id: TemplateId,
    pub priority: i16,
    pub kind: Option<TypeId>,
    pub add_subtypes: BTreeSet<SubtypeId>,
    pub remove_subtypes: BTreeSet<SubtypeId>,
    pub adjustments: ByAttribute<i16>,
    pub overrides: ByAttribute<Option<u16>>,
    pub modifiers: Vec<Modifier>,
}
impl Template {
    pub fn new(id: TemplateId, priority: i16) -> Self {
        Self {
            id,
            priority,
            kind: None,
            add_subtypes: BTreeSet::new(),
            remove_subtypes: BTreeSet::new(),
            adjustments: ByAttribute::default(),
            overrides: ByAttribute::default(),
            modifiers: vec![],
        }
    }
    fn validate(&self, rules: &Ruleset) -> Result<(), BuildError> {
        if self
            .kind
            .as_ref()
            .is_some_and(|id| rules.creature_type(id).is_none())
            || !self.add_subtypes.is_disjoint(&self.remove_subtypes)
            || self
                .add_subtypes
                .iter()
                .chain(&self.remove_subtypes)
                .any(|id| rules.subtype(id).is_none())
            || self.adjustments.iter().any(|(_, v)| {
                !(-crate::limits::STAT_MAGNITUDE..=crate::limits::STAT_MAGNITUDE)
                    .contains(&i64::from(*v))
            })
            || self
                .overrides
                .iter()
                .any(|(_, v)| v.is_some_and(|v| v > rules.constants().derived_attribute_cap))
        {
            return Err(BuildError::InvalidDefinition);
        }
        rules
            .validate_modifiers(&self.modifiers)
            .map_err(|_| BuildError::InvalidDefinition)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScenarioCatalog {
    pub species: BTreeMap<SpeciesId, Species>,
    pub templates: BTreeMap<TemplateId, Template>,
}
impl ScenarioCatalog {
    pub fn validate(&self, rules: &Ruleset) -> Result<(), BuildError> {
        if self.species.len() > crate::limits::CATALOG_ENTRIES
            || self.templates.len() > crate::limits::CATALOG_ENTRIES
        {
            return Err(BuildError::InvalidDefinition);
        }
        for (id, species) in &self.species {
            if id != &species.id {
                return Err(BuildError::InvalidDefinition);
            }
            species.validate(rules)?;
        }
        for (id, template) in &self.templates {
            if id != &template.id {
                return Err(BuildError::InvalidDefinition);
            }
            template.validate(rules)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content {
    presentation: Arc<crate::appearance_catalog::PresentationCatalog>,
    detection_appearances: Arc<BTreeMap<SpeciesId, crate::appearance_catalog::CreatureAppearance>>,
    pub(crate) rules: Arc<Ruleset>,
    pub(crate) catalog: Arc<ScenarioCatalog>,
    /// Immutable ordinary projectile definitions, reconstructed from the saved
    /// scenario catalog. Traps reference keys, never invent item identities.
    trap_ammunition: Arc<BTreeMap<String, crate::ItemSpec>>,
}
impl Content {
    /// Explicit species-owned labels only; no personal identity or ID prettifying.
    pub fn detection_appearance(
        &self,
        species: &SpeciesId,
    ) -> Option<&crate::appearance_catalog::CreatureAppearance> {
        self.detection_appearances.get(species)
    }
    /// Independent from Hallucination's roster: adding a species label cannot
    /// change already selected cosmetic identities or palette ordering.
    pub fn with_detection_appearances(
        content: &Arc<Self>,
        appearances: BTreeMap<SpeciesId, crate::appearance_catalog::CreatureAppearance>,
    ) -> Result<Arc<Self>, BuildError> {
        if appearances.len() > content.catalog.species.len()
            || appearances.iter().any(|(species, value)| {
                !content.catalog.species.contains_key(species) || !value.valid()
            })
        {
            return Err(BuildError::InvalidDefinition);
        }
        Ok(Arc::new(Self {
            detection_appearances: Arc::new(appearances),
            presentation: content.presentation.clone(),
            rules: content.rules.clone(),
            catalog: content.catalog.clone(),
            trap_ammunition: content.trap_ammunition.clone(),
        }))
    }
    pub fn presentation_catalog(&self) -> &Arc<crate::appearance_catalog::PresentationCatalog> {
        &self.presentation
    }
    pub fn with_presentation_catalog(
        content: &Arc<Self>,
        presentation: Arc<crate::appearance_catalog::PresentationCatalog>,
    ) -> Arc<Self> {
        Arc::new(Self {
            rules: content.rules.clone(),
            catalog: content.catalog.clone(),
            trap_ammunition: content.trap_ammunition.clone(),
            presentation,
            detection_appearances: content.detection_appearances.clone(),
        })
    }
    pub fn trap_ammunition(&self, key: &str) -> Option<&crate::ItemSpec> {
        self.trap_ammunition.get(key)
    }
    pub fn with_trap_ammunition(
        content: &Arc<Self>,
        ammunition: BTreeMap<String, crate::ItemSpec>,
    ) -> Result<Arc<Self>, BuildError> {
        if ammunition.len() > crate::limits::CATALOG_ENTRIES
            || ammunition.iter().any(|(key, spec)| {
                key.is_empty()
                    || key.len() > 80
                    || key.chars().any(char::is_control)
                    || !spec.valid()
                    || spec.projectile.is_none()
                    || !spec.stackable
                    || spec.concealed
                    || spec.magical
                    || spec.equipment_generation.is_some()
                    || spec.validate_rules(&content.rules).is_err()
            })
        {
            return Err(BuildError::InvalidDefinition);
        }
        Ok(Arc::new(Self {
            presentation: content.presentation.clone(),
            detection_appearances: content.detection_appearances.clone(),
            rules: content.rules.clone(),
            catalog: content.catalog.clone(),
            trap_ammunition: Arc::new(ammunition),
        }))
    }
    pub fn rules(&self) -> &Arc<Ruleset> {
        &self.rules
    }
    pub fn catalog(&self) -> &ScenarioCatalog {
        &self.catalog
    }
    pub fn new(rules: Arc<Ruleset>, mut catalog: ScenarioCatalog) -> Result<Arc<Self>, BuildError> {
        catalog.validate(&rules)?;
        for assessment in &rules.document().growth.threat.overrides {
            use crate::rules::ThreatSource;
            let known = match &assessment.source {
                ThreatSource::Species { id } => catalog.species.contains_key(id),
                ThreatSource::Type { id } => rules.creature_type(id).is_some(),
                ThreatSource::Subtype { id } => rules.subtype(id).is_some(),
                ThreatSource::Class { id } => rules.class(id).is_some(),
                ThreatSource::Template { id } => catalog.templates.contains_key(id),
                ThreatSource::Talent { id } => rules.talent(id).is_some(),
                ThreatSource::Expertise { id } => rules.expertise(id).is_some(),
            };
            if !known {
                return Err(BuildError::InvalidDefinition);
            }
        }
        let mut pool = crate::content_ids::Interner::default();
        catalog.species = std::mem::take(&mut catalog.species)
            .into_values()
            .map(|mut species| {
                species.id.intern(&mut pool);
                (species.id.clone(), species)
            })
            .collect();
        catalog.templates = std::mem::take(&mut catalog.templates)
            .into_values()
            .map(|mut template| {
                template.id.intern(&mut pool);
                (template.id.clone(), template)
            })
            .collect();
        Ok(Arc::new(Self {
            rules,
            catalog: Arc::new(catalog),
            presentation: crate::appearance_catalog::PresentationCatalog::new(vec![], vec![])?,
            trap_ammunition: Arc::new(BTreeMap::new()),
            detection_appearances: Arc::new(BTreeMap::new()),
        }))
    }
}

#[cfg(test)]
#[path = "catalog_detection_tests.rs"]
mod detection_tests;
