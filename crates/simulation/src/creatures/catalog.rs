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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Species {
    pub id: SpeciesId,
    pub kind: TypeId,
    pub subtypes: BTreeSet<SubtypeId>,
    pub default_attributes: Attributes,
    pub anatomy: AnatomySpec,
    pub melee: crate::attacks::MeleeAttack,
    pub modifiers: Vec<Modifier>,
}
impl Species {
    fn validate(&self, rules: &Ruleset) -> Result<(), BuildError> {
        if !self.anatomy.valid()
            || rules.creature_type(&self.kind).is_none()
            || self.subtypes.iter().any(|id| rules.subtype(id).is_none())
            || Attribute::ALL.into_iter().any(|attribute| {
                self.default_attributes.get(attribute) > rules.constants().attribute_cap
            })
        {
            return Err(BuildError::InvalidDefinition);
        }
        rules
            .validate_damage(self.melee.damage())
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
            || self
                .adjustments
                .iter()
                .any(|(_, v)| !(-1_000..=1_000).contains(v))
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
    pub(crate) rules: Arc<Ruleset>,
    pub(crate) catalog: Arc<ScenarioCatalog>,
}
impl Content {
    pub fn rules(&self) -> &Arc<Ruleset> {
        &self.rules
    }
    pub fn catalog(&self) -> &ScenarioCatalog {
        &self.catalog
    }
    pub fn new(rules: Arc<Ruleset>, mut catalog: ScenarioCatalog) -> Result<Arc<Self>, BuildError> {
        catalog.validate(&rules)?;
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
        }))
    }
}
