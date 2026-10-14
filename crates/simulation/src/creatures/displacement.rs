//! Content-owned push eligibility, independent of footprint, mass and hit dice.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum CreatureSize {
    Tiny = 0,
    Small = 1,
    Medium = 2,
    Large = 3,
    Huge = 4,
    Colossal = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisplacementProfile {
    pub size: CreatureSize,
    pub anchored: bool,
}
impl<'de> Deserialize<'de> for DisplacementProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            size: CreatureSize,
            anchored: bool,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            size: fields.size,
            anchored: fields.anchored,
        })
    }
}
impl DisplacementProfile {
    pub(crate) fn permits_push(self, target: Self) -> bool {
        !target.anchored && u16::from(target.size as u8) <= u16::from(self.size as u8) + 1
    }
}
