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
    pub health_recovery: HealthRecoveryView,
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

/// Personal recovery facts; no second Health balance or spendable resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthRecoveryView {
    #[serde(with = "crate::integers::unsigned")]
    pub period_ticks: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub elapsed_ticks: u64,
    pub stopped: bool,
    pub reset_percent: u8,
}
impl HealthRecoveryView {
    pub(crate) fn valid(&self) -> bool {
        (1..=100_000_000).contains(&self.period_ticks)
            && (!self.stopped || self.period_ticks <= 1_000_000)
            && self.elapsed_ticks < self.period_ticks
            && self.reset_percent <= 100
    }
}

#[cfg(test)]
mod health_recovery_tests {
    use super::*;
    #[test]
    fn stopped_recovery_disclosure_uses_the_bounded_base_period() {
        let mut view = HealthRecoveryView {
            period_ticks: 100_000_000,
            elapsed_ticks: 0,
            stopped: false,
            reset_percent: 100,
        };
        assert!(view.valid(), "maximum slowed period remains valid");
        view.stopped = true;
        assert!(
            !view.valid(),
            "stopped recovery always uses the base denominator"
        );
        view.period_ticks = 1_000_000;
        assert!(view.valid());
        view.period_ticks += 1;
        assert!(!view.valid());
    }
    #[test]
    fn health_recovery_disclosure_requires_bounded_period_progress_and_reset() {
        let valid = HealthRecoveryView {
            period_ticks: 1200,
            elapsed_ticks: 1199,
            stopped: false,
            reset_percent: 100,
        };
        assert!(valid.valid());
        for changed in [
            HealthRecoveryView {
                period_ticks: 0,
                ..valid
            },
            HealthRecoveryView {
                period_ticks: 100_000_001,
                ..valid
            },
            HealthRecoveryView {
                elapsed_ticks: 1200,
                ..valid
            },
            HealthRecoveryView {
                reset_percent: 101,
                ..valid
            },
        ] {
            assert!(!changed.valid());
        }
        let encoded = serde_json::to_value(valid).unwrap();
        assert_eq!(encoded["period_ticks"], "1200");
        assert_eq!(
            serde_json::from_value::<HealthRecoveryView>(encoded.clone()).unwrap(),
            valid
        );
        for field in ["period_ticks", "elapsed_ticks", "stopped", "reset_percent"] {
            let mut missing = encoded.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<HealthRecoveryView>(missing).is_err());
        }
    }
}
