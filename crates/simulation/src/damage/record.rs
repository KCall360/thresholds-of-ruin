//! Definition records reconstruct through the canonical damage constructor.
use super::{DamageAmount, DamageComponent, DamageError, DamageKey, DamageSpec};
use crate::bounded::{required_option, Bounded};
use crate::combat::DamageType;
use crate::dice::DicePool;
use crate::grants::Descriptor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DamageRecord {
    components: Bounded<ComponentRecord, 32>,
    #[serde(deserialize_with = "required_option")]
    primary: Option<KeyRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyRecord {
    category: DamageType,
    descriptor: Option<Descriptor>,
    sides: Option<u16>,
}
impl KeyRecord {
    fn capture(key: DamageKey) -> Self {
        Self {
            category: key.category,
            descriptor: key.descriptor,
            sides: key.sides,
        }
    }
    fn restore(&self) -> DamageKey {
        DamageKey {
            category: self.category,
            descriptor: self.descriptor,
            sides: self.sides,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentRecord {
    category: DamageType,
    descriptor: Option<Descriptor>,
    amount: AmountRecord,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AmountRecord {
    Fixed { value: u32 },
    Rolled { count: u16, sides: u16, bonus: i32 },
}

impl DamageRecord {
    pub(crate) fn capture(damage: &DamageSpec) -> Self {
        Self {
            components: Bounded(
                damage
                    .components
                    .iter()
                    .map(|component| ComponentRecord {
                        category: component.category,
                        descriptor: component.descriptor,
                        amount: match component.amount {
                            DamageAmount::Fixed(value) => AmountRecord::Fixed { value },
                            DamageAmount::Rolled(pool) => AmountRecord::Rolled {
                                count: pool.count(),
                                sides: pool.sides(),
                                bonus: pool.bonus(),
                            },
                        },
                    })
                    .collect(),
            ),
            primary: damage.primary.map(KeyRecord::capture),
        }
    }

    pub(crate) fn restore(&self) -> Result<DamageSpec, DamageError> {
        let components = self
            .components
            .0
            .iter()
            .map(|component| {
                Ok(match component.amount {
                    AmountRecord::Fixed { value } => {
                        DamageComponent::fixed(component.category, component.descriptor, value)
                    }
                    AmountRecord::Rolled {
                        count,
                        sides,
                        bonus,
                    } => DamageComponent::rolled(
                        component.category,
                        component.descriptor,
                        DicePool::new(count, sides, bonus).map_err(|_| DamageError::InvalidSpec)?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, DamageError>>()?;
        DamageSpec::new(components, self.primary.as_ref().map(KeyRecord::restore))
    }
}
