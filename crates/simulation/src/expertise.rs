//! Permanent expertise ownership, distinct from weapon skill ranks and temporary
//! equipment modifiers. Compiled grants are reconstructed from owned build data.
use crate::content_ids::{ExpertiseId, PointPoolId, TalentId};
use crate::creatures::{BuildError, CreatureBuild};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpertiseCategory {
    Cultural,
    Equipment,
    Utility,
    Specialist,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertiseDef {
    pub id: ExpertiseId,
    pub name: String,
    pub category: ExpertiseCategory,
}

impl<'de> Deserialize<'de> for ExpertiseDef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: ExpertiseId,
            name: String,
            category: ExpertiseCategory,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.name.trim().is_empty()
            || fields.name.len() > crate::limits::DISPLAY_NAME_BYTES
            || fields.name.chars().any(char::is_control)
        {
            return Err(serde::de::Error::custom("invalid expertise display name"));
        }
        Ok(Self {
            id: fields.id,
            name: fields.name,
            category: fields.category,
        })
    }
}

/// The expertise ID is the enclosing map key. A paid expertise is unique in a
/// build, so owner and pool identify it without unstable purchase-list indices.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExpertiseOrigin {
    Intellect {
        threshold: u16,
    },
    HdPurchase {
        owner: crate::HdIndex,
        pool: PointPoolId,
    },
    Talent {
        talent: TalentId,
    },
}

/// All owned sources, including dormant talent grants, remain available for
/// acquisition validation. Only active sources unlock runtime weapon profiles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpertiseOwnership {
    grants: BTreeMap<ExpertiseId, BTreeMap<ExpertiseOrigin, bool>>,
}
impl ExpertiseOwnership {
    pub fn owns(&self, expertise: &ExpertiseId) -> bool {
        self.grants.contains_key(expertise)
    }

    pub fn active(&self, expertise: &ExpertiseId) -> bool {
        self.grants
            .get(expertise)
            .is_some_and(|origins| origins.values().any(|active| *active))
    }

    pub fn grants(&self) -> &BTreeMap<ExpertiseId, BTreeMap<ExpertiseOrigin, bool>> {
        &self.grants
    }

    pub fn active_ids(&self) -> impl Iterator<Item = &ExpertiseId> {
        self.grants
            .iter()
            .filter(|(_, origins)| origins.values().any(|active| *active))
            .map(|(id, _)| id)
    }

    pub(crate) fn compile(
        build: &CreatureBuild,
        active_talents: &BTreeSet<TalentId>,
        intellect_capacity: u16,
    ) -> Result<Self, BuildError> {
        let mut result = Self::default();
        for (threshold, expertise) in build.intellect_expertise_choices() {
            if let Some(expertise) = expertise {
                result.insert(
                    expertise.clone(),
                    ExpertiseOrigin::Intellect { threshold },
                    threshold <= intellect_capacity,
                )?;
            }
        }
        let bonus: BTreeSet<_> = build
            .intellect_expertise_choices()
            .filter_map(|(_, id)| id)
            .collect();
        let mut purchased = BTreeSet::new();
        for (owner, choices) in build.choices().iter().enumerate() {
            let owner = crate::HdIndex::from_usize(owner).map_err(BuildError::from)?;
            for purchase in &choices.purchases {
                purchase.selection.validate(build.ruleset())?;
                if let Some(id) = purchase.selection.expertise() {
                    if bonus.contains(id) || !purchased.insert(id.clone()) {
                        return Err(BuildError::DuplicateExpertise);
                    }
                    result.insert(
                        id.clone(),
                        ExpertiseOrigin::HdPurchase {
                            owner,
                            pool: purchase.pool.clone(),
                        },
                        true,
                    )?;
                }
                if let Some(talent) = purchase.selection.talent() {
                    let definition = build
                        .ruleset()
                        .talent(talent)
                        .ok_or(BuildError::UnknownTalent)?;
                    for expertise in &definition.expertise_grants {
                        result.insert(
                            expertise.clone(),
                            ExpertiseOrigin::Talent {
                                talent: talent.clone(),
                            },
                            active_talents.contains(talent),
                        )?;
                    }
                }
            }
        }
        Ok(result)
    }

    fn insert(
        &mut self,
        expertise: ExpertiseId,
        origin: ExpertiseOrigin,
        active: bool,
    ) -> Result<(), BuildError> {
        if !self.grants.contains_key(&expertise)
            && self.grants.len() >= crate::limits::OWNED_EXPERTISES
        {
            return Err(BuildError::TooManyOwnedExpertises);
        }
        let origins = self.grants.entry(expertise).or_default();
        if origins.insert(origin, active).is_some() {
            return Err(BuildError::InvalidOwnership);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "expertise_tests.rs"]
mod tests;
