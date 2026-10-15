//! Fixed creature-owned deficits. Losses are mutable state, never source modifiers
//! or a history of their causers, and do not participate in permanent eligibility.
use crate::attributes::{Attribute, ByAttribute};
use serde::{Deserialize, Serialize};

/// Eligibility is derived from the current ordinal, never a second lost-HD log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitDieRestorationBlocker {
    MissingSource,
    MissingHealthRoot,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RestorableLosses {
    pub attributes: ByAttribute<u16>,
    pub maximum_health: u32,
}
impl RestorableLosses {
    pub(crate) fn valid(self) -> bool {
        self.attributes
            .iter()
            .all(|(_, value)| *value <= crate::limits::DERIVED_ATTRIBUTE)
            && i64::from(self.maximum_health) <= crate::limits::NUMERIC
    }
    pub(crate) fn has_attribute_loss(self) -> bool {
        Attribute::ALL
            .into_iter()
            .any(|attribute| *self.attributes.get(attribute) != 0)
    }
}
impl<'de> Deserialize<'de> for RestorableLosses {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            attributes: ByAttribute<u16>,
            maximum_health: u32,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let losses = Self {
            attributes: fields.attributes,
            maximum_health: fields.maximum_health,
        };
        if !losses.valid() {
            return Err(serde::de::Error::custom("invalid restorable losses"));
        }
        Ok(losses)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestorableLossOutcome {
    /// Actual aggregate change, which may differ from the immediate stat change
    /// when a later source change lowered the undrained capacity.
    pub applied: u32,
    pub rebuild: super::RebuildOutcome,
}
