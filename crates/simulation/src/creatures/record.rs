//! Strict mutable creature records. Definitions live once in the save catalog.
use super::SourceId;
use super::{BuildError, Content, CreatureBuild, HdChoices};
use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
use crate::bounded::{named_map, required_option, Bounded};
use crate::content_ids::{ClassId, SpeciesId, TalentId, TemplateId};
use crate::limits;
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::resources::{ByResource, Resource, Resources};
use crate::rules::Modifier;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureRecord {
    #[serde(deserialize_with = "named_map")]
    build: BuildRecord,
    injury: u32,
    dead: bool,
    pools: ByResource<PoolRecord>,
    holds: Bounded<HoldRecord, { limits::COST_HOLDS }>,
    conditions: Bounded<crate::conditions::ConditionInstance, { limits::CONDITIONS }>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PoolRecord {
    balance: u32,
    recovery_elapsed: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldRecord {
    owner: u64,
    resource: Resource,
    start: u32,
    resolution: u32,
}

impl CreatureRecord {
    pub fn capture(creature: &super::CreatureState) -> Self {
        let resources = creature.costs().resources();
        let pool = |resource| PoolRecord {
            balance: resources.balance(resource),
            recovery_elapsed: resources.recovery_elapsed(resource),
        };
        Self {
            build: BuildRecord::capture(creature.build()),
            injury: creature.health().injury(),
            dead: creature.health().dead(),
            pools: ByResource {
                stamina: pool(Resource::Stamina),
                focus: pool(Resource::Focus),
                mana: pool(Resource::Mana),
            },
            holds: Bounded(
                creature
                    .costs()
                    .reservations()
                    .iter()
                    .map(|(id, cost)| HoldRecord {
                        owner: id.0,
                        resource: cost.resource,
                        start: cost.start,
                        resolution: cost.resolution,
                    })
                    .collect(),
            ),
            conditions: Bounded(creature.conditions().instances().cloned().collect()),
        }
    }
    pub fn restore(
        &self,
        content: Arc<Content>,
        mut extra_sources: BTreeMap<SourceId, Vec<Modifier>>,
    ) -> Result<super::CreatureState, super::CreatureStateError> {
        use super::CreatureStateError as Error;
        if extra_sources.keys().any(|source| !matches!(source, SourceId::Equipment(item) if item.0 != 0 && item.0 != u64::MAX)) {
            return Err(Error::Inconsistent);
        }
        let build = self.build.restore(content).map_err(Error::Build)?;
        let conditions = crate::conditions::ConditionState::from_recorded(
            self.conditions.0.clone(),
            build.ruleset(),
        )
        .map_err(Error::Condition)?;
        extra_sources.extend(conditions.sources(build.ruleset()));
        let derived =
            super::derive::derive_with_sources(&build, &extra_sources).map_err(Error::Build)?;
        let pools = ByResource {
            stamina: &self.pools.stamina,
            focus: &self.pools.focus,
            mana: &self.pools.mana,
        };
        let resources = Resources::from_recorded(
            derived.resources,
            pools.map(|pool| pool.balance),
            pools.map(|pool| pool.recovery_elapsed),
            build.ruleset(),
        )
        .map_err(Error::Resource)?;
        let holds = self
            .holds
            .0
            .iter()
            .map(|hold| {
                (
                    crate::IntentionId(hold.owner),
                    crate::costs::ResourceCost {
                        resource: hold.resource,
                        start: hold.start,
                        resolution: hold.resolution,
                    },
                )
            })
            .collect();
        let costs =
            crate::costs::CostLedger::from_recorded(resources, holds).map_err(Error::Cost)?;
        super::CreatureState::from_recorded(
            build,
            self.injury,
            self.dead,
            costs,
            conditions,
            extra_sources,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    species: SpeciesId,
    initial_attributes: ByAttribute<u16>,
    binding: BindingRecord,
    hd: Bounded<HdRecord, { limits::HIT_DICE }>,
    choices: Bounded<ChoiceRecord, { limits::HIT_DICE }>,
    templates: Bounded<TemplateId, { limits::TEMPLATES }>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum BindingRecord {
    Intellect,
    Willpower,
    Awareness,
    Presence,
}
impl BindingRecord {
    fn capture(value: ManaBinding) -> Self {
        match value {
            ManaBinding::Intellect => Self::Intellect,
            ManaBinding::Willpower => Self::Willpower,
            ManaBinding::Awareness => Self::Awareness,
            ManaBinding::Presence => Self::Presence,
        }
    }
    fn restore(self) -> ManaBinding {
        match self {
            Self::Intellect => ManaBinding::Intellect,
            Self::Willpower => ManaBinding::Willpower,
            Self::Awareness => ManaBinding::Awareness,
            Self::Presence => ManaBinding::Presence,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SourceRecord {
    Racial,
    Class(ClassId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HdRecord {
    source: SourceRecord,
    health_seed: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceRecord {
    training: Bounded<Skill, { limits::TRAINING_PER_HIT_DIE }>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<Attribute>,
    #[serde(deserialize_with = "required_option")]
    talent: Option<TalentId>,
}
impl BuildRecord {
    pub fn capture(build: &CreatureBuild) -> Self {
        Self {
            species: build.species().id.clone(),
            initial_attributes: ByAttribute::from_fn(|attribute| {
                build.initial_attributes().get(attribute)
            }),
            binding: BindingRecord::capture(build.binding()),
            hd: Bounded(
                build
                    .ledger()
                    .entries()
                    .iter()
                    .map(|die| HdRecord {
                        source: match die.source() {
                            HdSource::Racial => SourceRecord::Racial,
                            HdSource::Class(class) => SourceRecord::Class(class.clone()),
                        },
                        health_seed: die.health_seed(),
                    })
                    .collect(),
            ),
            choices: Bounded(
                build
                    .choices()
                    .iter()
                    .map(|choice| ChoiceRecord {
                        training: Bounded(choice.training.clone()),
                        attribute: choice.attribute,
                        talent: choice.talent.clone(),
                    })
                    .collect(),
            ),
            templates: Bounded(build.template_ids().to_vec()),
        }
    }
    pub fn restore(&self, content: Arc<Content>) -> Result<CreatureBuild, BuildError> {
        let ledger = HdLedger::new(
            self.hd
                .0
                .iter()
                .map(|die| {
                    HitDie::new(
                        match &die.source {
                            SourceRecord::Racial => HdSource::Racial,
                            SourceRecord::Class(id) => HdSource::Class(id.clone()),
                        },
                        die.health_seed,
                    )
                })
                .collect(),
        )
        .map_err(|_| BuildError::InvalidDefinition)?;
        let choices = self
            .choices
            .0
            .iter()
            .map(|choice| HdChoices {
                training: choice.training.0.clone(),
                attribute: choice.attribute,
                talent: choice.talent.clone(),
            })
            .collect();
        CreatureBuild::from_recorded(
            content,
            self.species.clone(),
            Attributes::new(self.initial_attributes).map_err(|_| BuildError::InvalidDefinition)?,
            ledger,
            self.binding.restore(),
            choices,
            self.templates.0.clone(),
        )
    }
}
