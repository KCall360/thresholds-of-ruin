//! Strict checkpoint definitions. Derived combat values are never persisted here.
//! Catalog IDs are explicit so changing declaration order cannot reinterpret saves.

mod ids;
mod shape;

use super::{BuildError, CreatureBuild, HdChoices, Species, Subtype, Template};
use crate::attributes::{Attribute, Attributes, ManaBinding, Skill};
use crate::combat::DamageType;
use crate::grants::{Ability, Descriptor, Grant, Selector};
use crate::progression::{Class, CreatureType, HdLedger, HdSource, HitDie};
use crate::resources::{Resource, Resources};
use crate::talents::Talent;
use crate::{AnatomySpec, EquipmentSlot};
use ids::CatalogId;
use serde::{Deserialize, Serialize};
use shape::{required_option, Bounded, Identifier};
use std::collections::{BTreeMap, BTreeSet};

/// Mutable checkpoint data is independent of the runtime cache layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureRecord {
    build: BuildRecord,
    injury: u32,
    dead: bool,
    balances: [u32; 3],
    recovery: [u64; 3],
    holds: Bounded<(u64, u8, u32, u32), 64>,
    fear: Bounded<(u64, u64), 128>,
}
impl CreatureRecord {
    pub fn capture(creature: &super::CreatureState) -> Self {
        let resources = creature.costs().resources();
        Self {
            build: BuildRecord::capture(creature.build()),
            injury: creature.health().injury(),
            dead: creature.health().dead(),
            balances: Resource::ALL.map(|resource| resources.balance(resource)),
            recovery: Resource::ALL.map(|resource| resources.recovery_elapsed(resource)),
            holds: Bounded(
                creature
                    .costs()
                    .reservations()
                    .iter()
                    .map(|(id, cost)| (id.0, cost.resource.id(), cost.start, cost.resolution))
                    .collect(),
            ),
            fear: Bounded(
                creature
                    .fear()
                    .sources()
                    .iter()
                    .map(|(id, remaining)| (id.0, *remaining))
                    .collect(),
            ),
        }
    }
    pub fn restore(&self) -> Result<super::CreatureState, super::CreatureStateError> {
        use super::CreatureStateError as Error;
        let build = self.build.restore().map_err(Error::Build)?;
        let derived = build.derive().map_err(Error::Build)?;
        let resources = Resources::from_recorded(derived.resources, self.balances, self.recovery)
            .map_err(Error::Resource)?;
        let holds = self
            .holds
            .0
            .iter()
            .map(|&(id, resource, start, resolution)| {
                Ok((
                    crate::IntentionId(id),
                    crate::costs::ResourceCost {
                        resource: Resource::decode(resource).map_err(Error::Build)?,
                        start,
                        resolution,
                    },
                ))
            })
            .collect::<Result<_, Error>>()?;
        let costs =
            crate::costs::CostLedger::from_recorded(resources, holds).map_err(Error::Cost)?;
        let fear = crate::fear::FearState::from_recorded(
            self.fear
                .0
                .iter()
                .map(|&(id, remaining)| (crate::ActorId(id), remaining))
                .collect(),
        )
        .map_err(Error::Fear)?;
        super::CreatureState::from_recorded(build, self.injury, self.dead, costs, fear)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    species: SpeciesRecord,
    initial_attributes: [u8; 6],
    binding: u8,
    hd: Bounded<HdRecord, 256>,
    choices: Bounded<ChoiceRecord, 256>,
    templates: Bounded<TemplateRecord, 32>,
}

impl BuildRecord {
    pub fn capture(build: &CreatureBuild) -> Self {
        Self {
            species: SpeciesRecord::capture(build.species()),
            initial_attributes: attributes(build.initial_attributes()),
            binding: build.binding().id(),
            hd: Bounded(
                build
                    .ledger()
                    .entries()
                    .iter()
                    .map(|die| HdRecord {
                        source: match die.source() {
                            HdSource::Racial => SourceRecord::Racial,
                            HdSource::Class(class) => SourceRecord::Class(class.id()),
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
                        training: Bounded(choice.training.iter().map(|skill| skill.id()).collect()),
                        attribute: choice.attribute.map(CatalogId::id),
                        talent: choice.talent.map(CatalogId::id),
                    })
                    .collect(),
            ),
            templates: Bounded(
                build
                    .templates()
                    .iter()
                    .map(TemplateRecord::capture)
                    .collect(),
            ),
        }
    }

    /// Decode catalog IDs, then validate ownership and recompute every derived value.
    pub fn restore(&self) -> Result<CreatureBuild, BuildError> {
        let ledger = HdLedger::new(
            self.hd
                .0
                .iter()
                .map(|die| {
                    Ok(HitDie::new(
                        match die.source {
                            SourceRecord::Racial => HdSource::Racial,
                            SourceRecord::Class(id) => HdSource::Class(Class::decode(id)?),
                        },
                        die.health_seed,
                    ))
                })
                .collect::<Result<_, BuildError>>()?,
        )
        .map_err(|_| BuildError::InvalidDefinition)?;
        let choices = self
            .choices
            .0
            .iter()
            .map(|choice| {
                Ok(HdChoices {
                    training: choice
                        .training
                        .0
                        .iter()
                        .map(|&id| Skill::decode(id))
                        .collect::<Result<_, _>>()?,
                    attribute: choice.attribute.map(Attribute::decode).transpose()?,
                    talent: choice.talent.map(Talent::decode).transpose()?,
                })
            })
            .collect::<Result<_, BuildError>>()?;
        CreatureBuild::from_recorded(
            self.species.restore()?,
            Attributes::new(self.initial_attributes).map_err(|_| BuildError::InvalidDefinition)?,
            ledger,
            ManaBinding::decode(self.binding)?,
            choices,
            self.templates
                .0
                .iter()
                .map(TemplateRecord::restore)
                .collect::<Result<_, _>>()?,
        )
    }
}

fn attributes(values: Attributes) -> [u8; 6] {
    // Build definitions have already validated the ordinary attribute cap.
    Attribute::ALL.map(|attribute| values.get(attribute) as u8)
}

fn set<T: CatalogId + Ord>(values: &[u8]) -> Result<BTreeSet<T>, BuildError> {
    let decoded: BTreeSet<_> = values
        .iter()
        .map(|&id| T::decode(id))
        .collect::<Result<_, _>>()?;
    if decoded.len() != values.len() {
        return Err(BuildError::InvalidDefinition);
    }
    Ok(decoded)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpeciesRecord {
    id: Identifier,
    kind: u8,
    subtypes: Bounded<u8, 20>,
    default_attributes: [u8; 6],
    anatomy: Bounded<u8, 64>,
    melee: crate::attacks::MeleeAttackRecord,
    grants: Bounded<GrantRecord, 32>,
}
impl SpeciesRecord {
    fn capture(species: &Species) -> Self {
        Self {
            id: Identifier(species.id.clone()),
            kind: species.kind.id(),
            subtypes: Bounded(
                species
                    .subtypes
                    .iter()
                    .map(|subtype| subtype.id())
                    .collect(),
            ),
            default_attributes: attributes(species.default_attributes),
            anatomy: Bounded(species.anatomy.slots.iter().map(|slot| slot.id()).collect()),
            melee: crate::attacks::MeleeAttackRecord::capture(&species.melee),
            grants: Bounded(
                species
                    .grants
                    .iter()
                    .copied()
                    .map(GrantRecord::capture)
                    .collect(),
            ),
        }
    }
    fn restore(&self) -> Result<Species, BuildError> {
        Ok(Species {
            id: self.id.0.clone(),
            kind: CreatureType::decode(self.kind)?,
            subtypes: set::<Subtype>(&self.subtypes.0)?,
            default_attributes: Attributes::new(self.default_attributes)
                .map_err(|_| BuildError::InvalidDefinition)?,
            anatomy: AnatomySpec {
                slots: self
                    .anatomy
                    .0
                    .iter()
                    .map(|&id| EquipmentSlot::decode(id))
                    .collect::<Result<_, _>>()?,
            },
            melee: self
                .melee
                .restore()
                .map_err(|_| BuildError::InvalidDefinition)?,
            grants: self
                .grants
                .0
                .iter()
                .map(GrantRecord::restore)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SourceRecord {
    Racial,
    Class(u8),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HdRecord {
    source: SourceRecord,
    health_seed: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceRecord {
    training: Bounded<u8, 2>,
    #[serde(deserialize_with = "required_option")]
    attribute: Option<u8>,
    #[serde(deserialize_with = "required_option")]
    talent: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateRecord {
    id: Identifier,
    priority: i16,
    #[serde(deserialize_with = "required_option")]
    kind: Option<u8>,
    add_subtypes: Bounded<u8, 20>,
    remove_subtypes: Bounded<u8, 20>,
    adjustments: [i16; 6],
    overrides: Bounded<OverrideRecord, 6>,
    grants: Bounded<GrantRecord, 32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideRecord {
    attribute: u8,
    value: u16,
}
impl TemplateRecord {
    fn capture(template: &Template) -> Self {
        Self {
            id: Identifier(template.id.clone()),
            priority: template.priority,
            kind: template.kind.map(CatalogId::id),
            add_subtypes: Bounded(
                template
                    .add_subtypes
                    .iter()
                    .map(|value| value.id())
                    .collect(),
            ),
            remove_subtypes: Bounded(
                template
                    .remove_subtypes
                    .iter()
                    .map(|value| value.id())
                    .collect(),
            ),
            adjustments: template.adjustments,
            overrides: Bounded(
                template
                    .overrides
                    .iter()
                    .map(|(&attribute, &value)| OverrideRecord {
                        attribute: attribute.id(),
                        value,
                    })
                    .collect(),
            ),
            grants: Bounded(
                template
                    .grants
                    .iter()
                    .copied()
                    .map(GrantRecord::capture)
                    .collect(),
            ),
        }
    }
    fn restore(&self) -> Result<Template, BuildError> {
        let mut overrides = BTreeMap::new();
        for entry in &self.overrides.0 {
            if overrides
                .insert(Attribute::decode(entry.attribute)?, entry.value)
                .is_some()
            {
                return Err(BuildError::InvalidDefinition);
            }
        }
        Ok(Template {
            id: self.id.0.clone(),
            priority: self.priority,
            kind: self.kind.map(CreatureType::decode).transpose()?,
            add_subtypes: set(&self.add_subtypes.0)?,
            remove_subtypes: set(&self.remove_subtypes.0)?,
            adjustments: self.adjustments,
            overrides,
            grants: self
                .grants
                .0
                .iter()
                .map(GrantRecord::restore)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum SelectorRecord {
    Category(u8),
    Descriptor(u8),
}
impl SelectorRecord {
    fn capture(selector: Selector) -> Self {
        match selector {
            Selector::Category(value) => Self::Category(value.id()),
            Selector::Descriptor(value) => Self::Descriptor(value.id()),
        }
    }
    fn restore(self) -> Result<Selector, BuildError> {
        Ok(match self {
            Self::Category(id) => Selector::Category(DamageType::decode(id)?),
            Self::Descriptor(id) => Selector::Descriptor(Descriptor::decode(id)?),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum GrantRecord {
    Health(u32),
    Stamina(u32),
    Focus(u32),
    Mana(u32),
    PhysicalDefense(i32),
    MeleeFlat(i32),
    MeleeDice(u16),
    BoltFlat(i32),
    BoltDice(u16),
    FearDifficulty(i32),
    FearDuration(u64),
    Immunity(SelectorRecord),
    Reduction(SelectorRecord, u32),
    Ability(u8),
    Mindless,
    Magical,
}
impl GrantRecord {
    fn capture(grant: Grant) -> Self {
        match grant {
            Grant::Health(v) => Self::Health(v),
            Grant::Stamina(v) => Self::Stamina(v),
            Grant::Focus(v) => Self::Focus(v),
            Grant::Mana(v) => Self::Mana(v),
            Grant::PhysicalDefense(v) => Self::PhysicalDefense(v),
            Grant::MeleeFlat(v) => Self::MeleeFlat(v),
            Grant::MeleeDice(v) => Self::MeleeDice(v),
            Grant::BoltFlat(v) => Self::BoltFlat(v),
            Grant::BoltDice(v) => Self::BoltDice(v),
            Grant::FearDifficulty(v) => Self::FearDifficulty(v),
            Grant::FearDuration(v) => Self::FearDuration(v),
            Grant::Immunity(v) => Self::Immunity(SelectorRecord::capture(v)),
            Grant::Reduction(s, v) => Self::Reduction(SelectorRecord::capture(s), v),
            Grant::Ability(v) => Self::Ability(v.id()),
            Grant::Mindless => Self::Mindless,
            Grant::Magical => Self::Magical,
        }
    }
    fn restore(&self) -> Result<Grant, BuildError> {
        let grant = match *self {
            Self::Health(v) => Grant::Health(v),
            Self::Stamina(v) => Grant::Stamina(v),
            Self::Focus(v) => Grant::Focus(v),
            Self::Mana(v) => Grant::Mana(v),
            Self::PhysicalDefense(v) => Grant::PhysicalDefense(v),
            Self::MeleeFlat(v) => Grant::MeleeFlat(v),
            Self::MeleeDice(v) => Grant::MeleeDice(v),
            Self::BoltFlat(v) => Grant::BoltFlat(v),
            Self::BoltDice(v) => Grant::BoltDice(v),
            Self::FearDifficulty(v) => Grant::FearDifficulty(v),
            Self::FearDuration(v) => Grant::FearDuration(v),
            Self::Immunity(v) => Grant::Immunity(v.restore()?),
            Self::Reduction(s, v) => Grant::Reduction(s.restore()?, v),
            Self::Ability(v) => Grant::Ability(Ability::decode(v)?),
            Self::Mindless => Grant::Mindless,
            Self::Magical => Grant::Magical,
        };
        if !grant.valid() {
            return Err(BuildError::InvalidDefinition);
        }
        Ok(grant)
    }
}
