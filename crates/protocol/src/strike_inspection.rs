//! Lightweight private source summaries. Authored bodies require typed detail pages.
use crate::{AttackView, ContentRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaturalAttackView {
    pub source: ContentRef,
    pub skill: ContentRef,
    pub bonus: i32,
    pub wind_up: u32,
    pub recovery: u32,
    pub strike_count: u16,
}
impl NaturalAttackView {
    pub(crate) fn valid(&self) -> bool {
        self.source.valid()
            && self.skill.valid()
            && (-1000..=1000).contains(&self.bonus)
            && (1..=1_000_000).contains(&self.wind_up)
            && (1..=1_000_000).contains(&self.recovery)
            && (1..=crate::MAX_SEQUENCE_STRIKES).contains(&self.strike_count)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectedMeleeView {
    Weapon { attack: AttackView },
    Natural { attack: NaturalAttackView },
}
impl SelectedMeleeView {
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Weapon { attack } => attack.valid(),
            Self::Natural { attack } => attack.valid(),
        }
    }
    pub fn skill(&self) -> &ContentRef {
        match self {
            Self::Weapon { attack } => &attack.skill,
            Self::Natural { attack } => &attack.skill,
        }
    }
    pub fn bonus(&self) -> i32 {
        match self {
            Self::Weapon { attack } => attack.bonus,
            Self::Natural { attack } => attack.bonus,
        }
    }
    pub fn wind_up(&self) -> u32 {
        match self {
            Self::Weapon { attack } => attack.wind_up,
            Self::Natural { attack } => attack.wind_up,
        }
    }
    pub fn recovery(&self) -> u32 {
        match self {
            Self::Weapon { attack } => attack.recovery,
            Self::Natural { attack } => attack.recovery,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StrikeSourceInspectionView {
    Projectile {
        delivery: ProjectileDeliveryKind,
    },
    Authored {
        strike_count: u16,
    },
    SelectedMelee {},
    NaturalMelee {
        strike_count: u16,
        additional_riders: u16,
    },
}
impl<'de> Deserialize<'de> for StrikeSourceInspectionView {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Projectile {
                delivery: ProjectileDeliveryKind,
            },
            Authored {
                strike_count: u16,
            },
            SelectedMelee {},
            NaturalMelee {
                strike_count: u16,
                additional_riders: u16,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Projectile { delivery } => Self::Projectile { delivery },
            Fields::Authored { strike_count } => Self::Authored { strike_count },
            Fields::SelectedMelee {} => Self::SelectedMelee {},
            Fields::NaturalMelee {
                strike_count,
                additional_riders,
            } => Self::NaturalMelee {
                strike_count,
                additional_riders,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityStrikesView {
    pub ability: String,
    pub source: StrikeSourceInspectionView,
}
impl AbilityStrikesView {
    pub(crate) fn valid(&self) -> bool {
        crate::valid_content_id(&self.ability)
            && match &self.source {
                StrikeSourceInspectionView::Authored { strike_count } => {
                    (1..=crate::MAX_SEQUENCE_STRIKES).contains(strike_count)
                }
                StrikeSourceInspectionView::SelectedMelee {}
                | StrikeSourceInspectionView::Projectile { .. } => true,
                StrikeSourceInspectionView::NaturalMelee {
                    strike_count,
                    additional_riders,
                } => {
                    (1..=crate::MAX_SEQUENCE_STRIKES).contains(strike_count)
                        && *additional_riders <= 8
                }
            }
    }
}

/// Which exact physical profile supplies an ability's projectile strike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectileDeliveryKind {
    Throw,
    Launch,
}
impl<'de> Deserialize<'de> for ProjectileDeliveryKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "throw" => Ok(Self::Throw),
            "launch" => Ok(Self::Launch),
            _ => Err(serde::de::Error::custom("invalid projectile delivery kind")),
        }
    }
}
