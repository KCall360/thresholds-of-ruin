//! Independent attack definitions, disclosed only through authorized reports
//! or the existing known-item boundary; never simulation/save serialization.
use crate::{DamageType, InspectionDescriptor, Skill};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageKeyView {
    pub category: DamageType,
    #[serde(deserialize_with = "Option::deserialize")]
    pub descriptor: Option<InspectionDescriptor>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub sides: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DamageAmountView {
    Fixed { value: u32 },
    Rolled { count: u16, sides: u16, bonus: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageComponentView {
    pub category: DamageType,
    #[serde(deserialize_with = "Option::deserialize")]
    pub descriptor: Option<InspectionDescriptor>,
    pub amount: DamageAmountView,
}
impl DamageComponentView {
    pub fn key(&self) -> DamageKeyView {
        DamageKeyView {
            category: self.category,
            descriptor: self.descriptor,
            sides: match self.amount {
                DamageAmountView::Fixed { .. } => None,
                DamageAmountView::Rolled { sides, .. } => Some(sides),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageView {
    pub primary: DamageKeyView,
    pub components: Vec<DamageComponentView>,
}
impl DamageView {
    pub(crate) fn valid(&self) -> bool {
        if self.components.is_empty()
            || self.components.len() > 32
            || self.components[0].key() != self.primary
        {
            return false;
        }
        let mut keys = BTreeSet::new();
        let mut descriptors = BTreeMap::new();
        let mut total = 0u64;
        for component in &self.components {
            if !keys.insert(component.key()) {
                return false;
            }
            if let Some(descriptor) = component.descriptor {
                if descriptors
                    .insert(descriptor, component.category)
                    .is_some_and(|previous| previous != component.category)
                {
                    return false;
                }
            }
            let maximum = match component.amount {
                DamageAmountView::Fixed { value } => u64::from(value),
                DamageAmountView::Rolled {
                    count,
                    sides,
                    bonus,
                } => {
                    if !(1..=64).contains(&count)
                        || !(2..=1000).contains(&sides)
                        || !(-1_000_000..=1_000_000).contains(&bonus)
                    {
                        return false;
                    }
                    (i64::from(count) * i64::from(sides) + i64::from(bonus)).max(0) as u64
                }
            };
            total += maximum;
            if total > 1_000_000 {
                return false;
            }
        }
        self.components[1..]
            .windows(2)
            .all(|pair| pair[0].key() < pair[1].key())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackView {
    pub skill: Skill,
    pub bonus: i32,
    pub wind_up: u32,
    pub recovery: u32,
    pub damage: DamageView,
}
impl AttackView {
    pub(crate) fn valid(&self) -> bool {
        matches!(self.skill, Skill::HeavyWeaponry | Skill::LightWeaponry)
            && (-1000..=1000).contains(&self.bonus)
            && (1..=1_000_000).contains(&self.wind_up)
            && (1..=1_000_000).contains(&self.recovery)
            && self.damage.valid()
    }
}
