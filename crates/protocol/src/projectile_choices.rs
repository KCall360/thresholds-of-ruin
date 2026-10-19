//! Bounded owner-only structural projectile choices. These are recommendations,
//! never affordability, numeric terms, or a guarantee that capture will succeed.
use crate::{EquippedProjectileWeapon, ItemTarget, ReplyContext, ThrownSourceSelection};
use serde::{Deserialize, Serialize};

pub const MAX_PROJECTILE_CHOICES: usize = 16;
/// Candidate evaluations per page; exact owner-token resolution is separate.
pub const MAX_PROJECTILE_CHOICE_SCAN: usize = 128;
pub const MAX_PROJECTILE_CHOICE_PAGE_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectileChoicePhase {
    Sources {},
    Ammunition { launcher: EquippedProjectileWeapon },
}
impl<'de> Deserialize<'de> for ProjectileChoicePhase {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Sources {},
            Ammunition { launcher: EquippedProjectileWeapon },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Sources {} => Self::Sources {},
            Fields::Ammunition { launcher } => Self::Ammunition { launcher },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectileChoiceQuery {
    pub ability: String,
    pub phase: ProjectileChoicePhase,
    /// Index in the current disclosed inventory, not the filtered matches.
    pub offset: u32,
}
impl ProjectileChoiceQuery {
    pub fn is_valid(&self) -> bool {
        crate::valid_content_id(&self.ability)
            && match self.phase {
                ProjectileChoicePhase::Ammunition { launcher } => launcher.valid(),
                _ => true,
            }
    }
}
impl<'de> Deserialize<'de> for ProjectileChoiceQuery {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ability: String,
            phase: ProjectileChoicePhase,
            offset: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            ability: fields.ability,
            phase: fields.phase,
            offset: fields.offset,
        };
        if !value.is_valid() {
            return Err(serde::de::Error::custom("invalid projectile choice query"));
        }
        Ok(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectileChoice {
    Throw { source: ThrownSourceSelection },
    Launcher { launcher: EquippedProjectileWeapon },
    Ammunition { item: ItemTarget },
}
impl ProjectileChoice {
    pub fn item(self) -> ItemTarget {
        match self {
            Self::Throw {
                source:
                    ThrownSourceSelection::Equipped { item, .. }
                    | ThrownSourceSelection::Carried { item, .. },
            }
            | Self::Ammunition { item } => item,
            Self::Launcher { launcher } => launcher.item,
        }
    }
    fn valid(self) -> bool {
        match self {
            Self::Throw { source } => source.valid(),
            Self::Launcher { launcher } => launcher.valid(),
            Self::Ammunition { .. } => true,
        }
    }
}
impl<'de> Deserialize<'de> for ProjectileChoice {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Throw { source: ThrownSourceSelection },
            Launcher { launcher: EquippedProjectileWeapon },
            Ammunition { item: ItemTarget },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Throw { source } => Self::Throw { source },
            Fields::Launcher { launcher } => Self::Launcher { launcher },
            Fields::Ammunition { item } => Self::Ammunition { item },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectileChoicePage {
    pub query: ProjectileChoiceQuery,
    pub choices: Vec<ProjectileChoice>,
    /// Required nullable continuation. Empty filtered pages may still continue.
    pub next_offset: Option<u32>,
}
impl ProjectileChoicePage {
    /// Clients must additionally bind the envelope's request ID and ReplyContext
    /// to the pending request and its current attachment/observation generation.
    pub fn valid_for(&self, query: &ProjectileChoiceQuery) -> bool {
        &self.query == query && self.is_valid()
    }
    pub fn is_valid(&self) -> bool {
        if !self.query.is_valid()
            || self.choices.len() > MAX_PROJECTILE_CHOICES
            || self
                .query
                .offset
                .checked_add(self.choices.len() as u32)
                .is_none()
        {
            return false;
        }
        if matches!(self.query.phase, ProjectileChoicePhase::Sources {})
            && self.choices.first().is_some_and(|first| {
                self.choices
                    .iter()
                    .any(|row| std::mem::discriminant(row) != std::mem::discriminant(first))
            })
        {
            return false;
        }
        let mut items = std::collections::BTreeSet::new();
        if !self.choices.iter().copied().all(|choice| {
            choice.valid()
                && items.insert(choice.item())
                && match (self.query.phase, choice) {
                    (ProjectileChoicePhase::Sources {}, ProjectileChoice::Throw { .. })
                    | (ProjectileChoicePhase::Sources {}, ProjectileChoice::Launcher { .. }) => {
                        true
                    }
                    (
                        ProjectileChoicePhase::Ammunition { launcher },
                        ProjectileChoice::Ammunition { item },
                    ) => launcher.item != item,
                    _ => false,
                }
        }) {
            return false;
        }
        self.next_offset.is_none_or(|next| {
            next.checked_sub(self.query.offset).is_some_and(|scanned| {
                (1..=MAX_PROJECTILE_CHOICE_SCAN as u32).contains(&scanned)
                    && scanned as usize >= self.choices.len()
            })
        })
    }
}
impl<'de> Deserialize<'de> for ProjectileChoicePage {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        fn choices<'de, D: serde::Deserializer<'de>>(
            d: D,
        ) -> Result<Vec<ProjectileChoice>, D::Error> {
            crate::bounded::bounded::<D, _, MAX_PROJECTILE_CHOICES>(d)
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            query: ProjectileChoiceQuery,
            #[serde(deserialize_with = "choices")]
            choices: Vec<ProjectileChoice>,
            #[serde(deserialize_with = "crate::wire::required_option")]
            next_offset: Option<u32>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let page = Self {
            query: fields.query,
            choices: fields.choices,
            next_offset: fields.next_offset,
        };
        if !page.is_valid() {
            return Err(serde::de::Error::custom("invalid projectile choice page"));
        }
        Ok(page)
    }
}
pub(crate) fn read_command<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(u64, ProjectileChoiceQuery), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        #[serde(with = "crate::integers::unsigned")]
        expected_revision: u64,
        query: ProjectileChoiceQuery,
    }
    let fields: Fields = crate::bounded::named_map(d)?;
    Ok((fields.expected_revision, fields.query))
}
pub(crate) fn read_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(ReplyContext, String, Box<ProjectileChoicePage>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        context: ReplyContext,
        request_id: String,
        page: Box<ProjectileChoicePage>,
    }
    let fields: Fields = crate::bounded::named_map(d)?;
    if fields.request_id.is_empty()
        || fields.request_id.len() > 128
        || fields.request_id.chars().any(char::is_control)
    {
        return Err(serde::de::Error::custom(
            "invalid projectile choice request identity",
        ));
    }
    Ok((fields.context, fields.request_id, fields.page))
}

#[cfg(test)]
#[path = "projectile_choice_tests.rs"]
mod tests;
