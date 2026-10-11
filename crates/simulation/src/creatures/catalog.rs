use crate::attributes::{Attribute, Attributes};
use crate::grants::{Descriptor, Grant, Selector};
use crate::progression::{Class, CreatureType};
use crate::talents::Talent;
use crate::AnatomySpec;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subtype {
    Air,
    Angel,
    Aquatic,
    Archon,
    Augmented,
    Chaotic,
    Cold,
    Earth,
    Evil,
    Extraplanar,
    Fire,
    Goblinoid,
    Good,
    Incorporeal,
    Lawful,
    Native,
    Reptilian,
    Shapechanger,
    Swarm,
    Water,
}

impl Subtype {
    pub const ALL: [Self; 20] = [
        Self::Air,
        Self::Angel,
        Self::Aquatic,
        Self::Archon,
        Self::Augmented,
        Self::Chaotic,
        Self::Cold,
        Self::Earth,
        Self::Evil,
        Self::Extraplanar,
        Self::Fire,
        Self::Goblinoid,
        Self::Good,
        Self::Incorporeal,
        Self::Lawful,
        Self::Native,
        Self::Reptilian,
        Self::Shapechanger,
        Self::Swarm,
        Self::Water,
    ];

    /// Only fire/cold immunity and ancestry identity are currently adapted.
    /// A catalog label must not imply unsupported planar, movement or body rules.
    pub fn mechanics_deferred(self) -> bool {
        !matches!(
            self,
            Self::Fire | Self::Cold | Self::Goblinoid | Self::Reptilian
        )
    }

    pub(super) fn grants(self) -> &'static [Grant] {
        match self {
            Self::Fire => &[Grant::Immunity(Selector::Descriptor(Descriptor::Fire))],
            Self::Cold => &[Grant::Immunity(Selector::Descriptor(Descriptor::Cold))],
            _ => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GrantSource {
    Species(String),
    Type(CreatureType),
    Subtype(Subtype),
    Class(Class),
    Template(String),
    Talent(Talent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Species {
    pub id: String,
    pub kind: CreatureType,
    pub subtypes: BTreeSet<Subtype>,
    pub default_attributes: Attributes,
    pub anatomy: AnatomySpec,
    pub melee: crate::attacks::MeleeAttack,
    pub grants: Vec<Grant>,
}

impl Species {
    pub(super) fn valid(&self) -> bool {
        valid_id(&self.id)
            && self.anatomy.valid()
            && Attribute::ALL
                .into_iter()
                .all(|attribute| self.default_attributes.get(attribute) <= 5)
            && valid_grants(&self.grants)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pub id: String,
    pub priority: i16,
    pub kind: Option<CreatureType>,
    pub add_subtypes: BTreeSet<Subtype>,
    pub remove_subtypes: BTreeSet<Subtype>,
    pub adjustments: [i16; 6],
    pub overrides: BTreeMap<Attribute, u16>,
    pub grants: Vec<Grant>,
}

impl Template {
    pub fn new(id: &str, priority: i16) -> Self {
        Self {
            id: id.into(),
            priority,
            kind: None,
            add_subtypes: BTreeSet::new(),
            remove_subtypes: BTreeSet::new(),
            adjustments: [0; 6],
            overrides: BTreeMap::new(),
            grants: vec![],
        }
    }

    pub fn zombified(priority: i16) -> Self {
        Self {
            kind: Some(CreatureType::Undead),
            adjustments: [2, -1, 0, 0, 0, 0],
            overrides: BTreeMap::from([(Attribute::Intellect, 0)]),
            grants: vec![Grant::Mindless],
            ..Self::new("zombified", priority)
        }
    }

    pub(super) fn valid(&self) -> bool {
        valid_id(&self.id)
            && self.add_subtypes.is_disjoint(&self.remove_subtypes)
            && self
                .adjustments
                .iter()
                .all(|value| (-1_000..=1_000).contains(value))
            && self.overrides.values().all(|&value| value <= 1_000)
            && valid_grants(&self.grants)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 60
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_grants(grants: &[Grant]) -> bool {
    grants.len() <= 32
        && grants.iter().copied().all(Grant::valid)
        && grants
            .iter()
            .enumerate()
            .all(|(index, grant)| !grants[..index].contains(grant))
}

pub(super) fn type_grants(kind: CreatureType) -> &'static [Grant] {
    match kind {
        CreatureType::Undead | CreatureType::Construct | CreatureType::Plant => &[
            Grant::Immunity(Selector::Descriptor(Descriptor::Fear)),
            Grant::Immunity(Selector::Descriptor(Descriptor::MindAffecting)),
        ],
        CreatureType::Ooze | CreatureType::Vermin => &[Grant::Mindless],
        _ => &[],
    }
}
