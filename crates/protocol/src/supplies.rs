//! Disclosed finite supply requirements and opaque exact-source selections.
use crate::{valid_content_id, ItemTarget};
use serde::{Deserialize, Serialize};

pub const MAX_SUPPLY_COSTS: usize = 8;
pub const MAX_SUPPLY_FRAGMENTS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyView {
    pub id: String,
    pub name: String,
}
impl<'de> Deserialize<'de> for SupplyView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: String,
            name: String,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            id: f.id,
            name: f.name,
        })
    }
}
impl SupplyView {
    pub fn valid(&self) -> bool {
        valid_content_id(&self.id) && crate::creature::valid_label(&self.name, 256)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplyAccess {
    CarriedOnly,
    CarriedOrGroundTouch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilitySupplyCost {
    pub supply: String,
    pub name: String,
    #[serde(with = "crate::integers::unsigned")]
    pub quantity: u64,
    pub access: SupplyAccess,
}
impl<'de> Deserialize<'de> for AbilitySupplyCost {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            supply: String,
            name: String,
            #[serde(with = "crate::integers::unsigned")]
            quantity: u64,
            access: SupplyAccess,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            supply: f.supply,
            name: f.name,
            quantity: f.quantity,
            access: f.access,
        })
    }
}
impl AbilitySupplyCost {
    pub fn valid(&self) -> bool {
        valid_content_id(&self.supply)
            && crate::creature::valid_label(&self.name, 256)
            && (1..=1_000_000).contains(&self.quantity)
    }
}
pub(crate) fn read_supply_costs<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<AbilitySupplyCost>, D::Error> {
    crate::bounded::bounded::<D, _, MAX_SUPPLY_COSTS>(d)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSupply {
    pub cost_ordinal: u8,
    pub item: ItemTarget,
}
impl<'de> Deserialize<'de> for SelectedSupply {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cost_ordinal: u8,
            item: ItemTarget,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            cost_ordinal: f.cost_ordinal,
            item: f.item,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SupplySelection {
    #[default]
    Carried,
    Explicit {
        items: Vec<SelectedSupply>,
    },
}
impl SupplySelection {
    pub fn valid(&self) -> bool {
        match self {
            Self::Carried => true,
            Self::Explicit { items } => {
                !items.is_empty()
                    && items.len() <= MAX_SUPPLY_FRAGMENTS
                    && items.windows(2).all(|pair| pair[0] < pair[1])
                    && items
                        .iter()
                        .map(|row| row.item)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == items.len()
                    && items
                        .iter()
                        .all(|row| usize::from(row.cost_ordinal) < MAX_SUPPLY_COSTS)
            }
        }
    }
}
impl<'de> Deserialize<'de> for SupplySelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        fn rows<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<SelectedSupply>, D::Error> {
            crate::bounded::bounded::<D, _, MAX_SUPPLY_FRAGMENTS>(d)
        }
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            // Empty records reject surplus keys; tagged unit variants ignore them.
            Carried {},
            Explicit {
                #[serde(deserialize_with = "rows")]
                items: Vec<SelectedSupply>,
            },
        }
        let value = match crate::bounded::named_map(d)? {
            Fields::Carried {} => Self::Carried,
            Fields::Explicit { items } => Self::Explicit { items },
        };
        if value.valid() {
            Ok(value)
        } else {
            Err(serde::de::Error::custom("invalid supply selection"))
        }
    }
}
