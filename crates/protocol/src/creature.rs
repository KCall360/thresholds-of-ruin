//! Disclosed content identity and personal creature facts. Names come from the
//! server's selected ruleset; clients never carry their own content catalog.
use serde::{Deserialize, Serialize};

pub const MAX_CONTENT_REFERENCES: usize = 4096;
pub fn valid_content_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 60
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.')
        })
}
pub(crate) fn valid_label(text: &str, maximum: usize) -> bool {
    !text.trim().is_empty() && text.len() <= maximum && !text.chars().any(char::is_control)
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentRef {
    pub id: String,
    pub name: String,
}
impl<'de> Deserialize<'de> for ContentRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: String,
            name: String,
        }
        let fields = Fields::deserialize(deserializer)?;
        let value = Self {
            id: fields.id,
            name: fields.name,
        };
        if value.valid() {
            Ok(value)
        } else {
            Err(serde::de::Error::custom(
                "invalid disclosed content identity",
            ))
        }
    }
}
impl ContentRef {
    pub fn valid(&self) -> bool {
        valid_content_id(&self.id) && valid_label(&self.name, 256)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityCost {
    pub resource: ContentRef,
    pub start: u32,
    pub resolution: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityTargetKind {
    SelfOnly,
    Actor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AbilityReach {
    Melee,
    Sight { range: u16 },
    SelfOnly,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityView {
    pub id: String,
    pub name: String,
    pub cost: Option<AbilityCost>,
    pub reach: AbilityReach,
    pub target_kind: AbilityTargetKind,
    pub basic_attack: bool,
}
impl AbilityView {
    pub fn valid(&self) -> bool {
        valid_content_id(&self.id)
            && valid_label(&self.name, 256)
            && self.cost.as_ref().is_none_or(|cost| {
                cost.resource.valid()
                    && u64::from(cost.start) + u64::from(cost.resolution) <= 1_000_000
            })
            && match (self.reach, self.target_kind) {
                (AbilityReach::SelfOnly, AbilityTargetKind::SelfOnly) => !self.basic_attack,
                (AbilityReach::Melee, AbilityTargetKind::Actor) => true,
                (AbilityReach::Sight { range }, AbilityTargetKind::Actor) => {
                    (1..=1000).contains(&range)
                }
                _ => false,
            }
    }
    pub fn content(&self) -> ContentRef {
        ContentRef {
            id: self.id.clone(),
            name: self.name.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnStats {
    pub kind: ContentRef,
    pub subtypes: Vec<ContentRef>,
    pub hit_dice: Vec<ContentRef>,
    pub attributes: AttributeView,
    pub skills: Vec<SkillView>,
    pub defenses: DefenseView,
    pub binding: ContentRef,
    pub resources: Vec<ResourceView>,
    pub active_talents: Vec<ContentRef>,
    pub dormant_talents: Vec<ContentRef>,
    pub abilities: Vec<AbilityView>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeView {
    pub strength: u16,
    pub speed: u16,
    pub intellect: u16,
    pub willpower: u16,
    pub awareness: u16,
    pub presence: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseView {
    pub physical: i32,
    pub cognitive: i32,
    pub spiritual: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillView {
    pub skill: ContentRef,
    pub rank: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceView {
    pub resource: ContentRef,
    pub balance: u32,
    pub maximum: u32,
    pub available: u32,
    pub reserved: u32,
}
