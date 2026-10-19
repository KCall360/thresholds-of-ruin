//! Typed weapon traits and expertise-gated profiles. These are independent of
//! attack and damage formulas; only the explicitly supported traits are accepted.
use crate::content_ids::{ExpertiseId, LauncherFamilyId};
use crate::expertise::{ExpertiseCategory, ExpertiseOwnership};
use crate::rules::Ruleset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// There are exactly four supported trait kinds. Bound allocation before decoding.
const TRAIT_LIMIT: usize = 4;

/// Engine work bound, not an extra gameplay range. Every physical grid step
/// costs five feet; authored bands exceeding this budget are rejected.
pub const MAX_PROJECTILE_STEPS: u32 = 4096;
pub const PROJECTILE_STEP_FEET: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct RangeBands {
    pub short_feet: u32,
    pub long_feet: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeClassification {
    Normal,
    Disadvantage,
    Unavailable,
}

impl RangeBands {
    pub fn validate(&self) -> Result<(), WeaponProfileError> {
        if self.short_feet == 0
            || !self.short_feet.is_multiple_of(5)
            || !self.long_feet.is_multiple_of(5)
            || self.long_feet < self.short_feet
            || self.long_feet > MAX_PROJECTILE_STEPS * PROJECTILE_STEP_FEET
        {
            return Err(WeaponProfileError::InvalidRange);
        }
        Ok(())
    }

    /// Classify a physical distance in feet, never a count of map cells.
    pub fn classify(&self, distance_feet: u32) -> RangeClassification {
        if distance_feet <= self.short_feet {
            RangeClassification::Normal
        } else if distance_feet <= self.long_feet {
            RangeClassification::Disadvantage
        } else {
            RangeClassification::Unavailable
        }
    }
}

impl<'de> Deserialize<'de> for RangeBands {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            short_feet: u32,
            long_feet: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            short_feet: fields.short_feet,
            long_feet: fields.long_feet,
        };
        result.validate().map_err(serde::de::Error::custom)?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponTraitKind {
    TwoHanded,
    Offhand,
    Quickdraw,
    Thrown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeaponTrait {
    TwoHanded,
    Offhand,
    Quickdraw,
    Thrown { range: RangeBands },
}

impl WeaponTrait {
    pub fn kind(&self) -> WeaponTraitKind {
        match self {
            Self::TwoHanded => WeaponTraitKind::TwoHanded,
            Self::Offhand => WeaponTraitKind::Offhand,
            Self::Quickdraw => WeaponTraitKind::Quickdraw,
            Self::Thrown { .. } => WeaponTraitKind::Thrown,
        }
    }
}

impl<'de> Deserialize<'de> for WeaponTrait {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Empty struct variants deliberately reject unknown fields, including on
        // boolean traits, rather than using permissive unit-variant decoding.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            TwoHanded {},
            Offhand {},
            Quickdraw {},
            Thrown { range: RangeBands },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::TwoHanded {} => Self::TwoHanded,
            Fields::Offhand {} => Self::Offhand,
            Fields::Quickdraw {} => Self::Quickdraw,
            Fields::Thrown { range } => Self::Thrown { range },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct WeaponTraitChanges {
    pub add: Vec<WeaponTrait>,
    pub remove: Vec<WeaponTraitKind>,
}

impl WeaponTraitChanges {
    fn validate(&self) -> Result<(), WeaponProfileError> {
        validate_traits(&self.add)?;
        if self.remove.len() > TRAIT_LIMIT {
            return Err(WeaponProfileError::TooManyTraits);
        }
        let mut seen = BTreeSet::new();
        for kind in &self.remove {
            if !seen.insert(*kind) {
                return Err(WeaponProfileError::DuplicateTrait(*kind));
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for WeaponTraitChanges {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            add: crate::bounded::Bounded<WeaponTrait, TRAIT_LIMIT>,
            remove: crate::bounded::Bounded<WeaponTraitKind, TRAIT_LIMIT>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            add: fields.add.0,
            remove: fields.remove.0,
        };
        result.validate().map_err(serde::de::Error::custom)?;
        Ok(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct LauncherProfile {
    pub family: LauncherFamilyId,
    pub range: RangeBands,
}

impl<'de> Deserialize<'de> for LauncherProfile {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            family: LauncherFamilyId,
            range: RangeBands,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            family: fields.family,
            range: fields.range,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct WeaponProfileSpec {
    pub expertise: ExpertiseId,
    pub launcher: Option<LauncherProfile>,
    pub base_traits: Vec<WeaponTrait>,
    pub expert_traits: WeaponTraitChanges,
}

impl<'de> Deserialize<'de> for WeaponProfileSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            expertise: ExpertiseId,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            launcher: Option<LauncherProfile>,
            base_traits: crate::bounded::Bounded<WeaponTrait, TRAIT_LIMIT>,
            expert_traits: WeaponTraitChanges,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            expertise: fields.expertise,
            launcher: fields.launcher,
            base_traits: fields.base_traits.0,
            expert_traits: fields.expert_traits,
        };
        result
            .effective_states()
            .map_err(serde::de::Error::custom)?;
        Ok(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeaponProfileError {
    InvalidRange,
    TooManyTraits,
    DuplicateTrait(WeaponTraitKind),
    MissingBaseTrait(WeaponTraitKind),
    UnknownExpertise(ExpertiseId),
    NonEquipmentExpertise(ExpertiseId),
}
impl std::fmt::Display for WeaponProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange => f.write_str(
                "weapon ranges must be positive five-foot multiples, ordered and bounded",
            ),
            Self::TooManyTraits => f.write_str("weapon trait collection exceeds limit"),
            Self::DuplicateTrait(kind) => write!(f, "duplicate weapon trait: {kind:?}"),
            Self::MissingBaseTrait(kind) => {
                write!(f, "removed weapon trait is absent from base: {kind:?}")
            }
            Self::UnknownExpertise(id) => write!(f, "unknown weapon expertise: {id}"),
            Self::NonEquipmentExpertise(id) => {
                write!(f, "weapon expertise must have equipment category: {id}")
            }
        }
    }
}
impl std::error::Error for WeaponProfileError {}

fn validate_traits(traits: &[WeaponTrait]) -> Result<(), WeaponProfileError> {
    if traits.len() > TRAIT_LIMIT {
        return Err(WeaponProfileError::TooManyTraits);
    }
    let mut seen = BTreeSet::new();
    for trait_ in traits {
        if !seen.insert(trait_.kind()) {
            return Err(WeaponProfileError::DuplicateTrait(trait_.kind()));
        }
        if let WeaponTrait::Thrown { range } = trait_ {
            range.validate()?;
        }
    }
    Ok(())
}

impl WeaponProfileSpec {
    fn effective_states(
        &self,
    ) -> Result<(EffectiveWeaponProfile, EffectiveWeaponProfile), WeaponProfileError> {
        if let Some(launcher) = &self.launcher {
            launcher.range.validate()?;
        }
        validate_traits(&self.base_traits)?;
        self.expert_traits.validate()?;
        for removed in &self.expert_traits.remove {
            if !self
                .base_traits
                .iter()
                .any(|trait_| trait_.kind() == *removed)
            {
                return Err(WeaponProfileError::MissingBaseTrait(*removed));
            }
        }
        let mut expert_traits: Vec<_> = self
            .base_traits
            .iter()
            .copied()
            .filter(|trait_| !self.expert_traits.remove.contains(&trait_.kind()))
            .collect();
        // Removal first permits an explicit replacement, such as a new thrown range.
        expert_traits.extend_from_slice(&self.expert_traits.add);
        validate_traits(&expert_traits)?;
        Ok((
            EffectiveWeaponProfile {
                launcher: self.launcher.clone(),
                traits: self.base_traits.clone(),
            },
            EffectiveWeaponProfile {
                launcher: self.launcher.clone(),
                traits: expert_traits,
            },
        ))
    }

    /// Compile once at the content boundary. Runtime resolution cannot bypass
    /// validation and does not need a ruleset or the actor's full build.
    pub fn validate(
        &self,
        ruleset: &Ruleset,
    ) -> Result<ValidatedWeaponProfile, WeaponProfileError> {
        let definition = ruleset
            .expertise(&self.expertise)
            .ok_or_else(|| WeaponProfileError::UnknownExpertise(self.expertise.clone()))?;
        if definition.category != ExpertiseCategory::Equipment {
            return Err(WeaponProfileError::NonEquipmentExpertise(
                self.expertise.clone(),
            ));
        }
        let (base, expert) = self.effective_states()?;
        Ok(ValidatedWeaponProfile {
            spec: self.clone(),
            base,
            expert,
        })
    }
}

/// Immutable compiled profile. Independent adapters retain access to its authoring
/// specification without gaining mutation access to the validated states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedWeaponProfile {
    spec: WeaponProfileSpec,
    base: EffectiveWeaponProfile,
    expert: EffectiveWeaponProfile,
}
impl ValidatedWeaponProfile {
    pub(crate) fn state(&self, expert: bool) -> &EffectiveWeaponProfile {
        if expert {
            &self.expert
        } else {
            &self.base
        }
    }
    pub fn spec(&self) -> &WeaponProfileSpec {
        &self.spec
    }
    pub fn resolve(&self, ownership: &ExpertiseOwnership) -> &EffectiveWeaponProfile {
        if ownership.active(&self.spec.expertise) {
            &self.expert
        } else {
            &self.base
        }
    }
    pub fn compatible_with(&self, rules: &Ruleset) -> bool {
        rules
            .expertise(&self.spec.expertise)
            .is_some_and(|definition| definition.category == ExpertiseCategory::Equipment)
    }
    pub fn minimum_grip_demand(&self) -> u8 {
        self.base.grip_demand().min(self.expert.grip_demand())
    }
    pub fn maximum_grip_demand(&self) -> u8 {
        self.base.grip_demand().max(self.expert.grip_demand())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveWeaponProfile {
    launcher: Option<LauncherProfile>,
    traits: Vec<WeaponTrait>,
}
impl EffectiveWeaponProfile {
    pub fn launcher(&self) -> Option<&LauncherProfile> {
        self.launcher.as_ref()
    }
    pub fn traits(&self) -> &[WeaponTrait] {
        &self.traits
    }
    pub fn two_handed(&self) -> bool {
        self.traits.contains(&WeaponTrait::TwoHanded)
    }
    pub fn offhand(&self) -> bool {
        self.traits.contains(&WeaponTrait::Offhand)
    }
    pub fn quickdraw(&self) -> bool {
        self.traits.contains(&WeaponTrait::Quickdraw)
    }
    pub fn thrown(&self) -> Option<RangeBands> {
        self.traits.iter().find_map(|trait_| match trait_ {
            WeaponTrait::Thrown { range } => Some(*range),
            _ => None,
        })
    }
    pub fn grip_demand(&self) -> u8 {
        if self.two_handed() {
            2
        } else {
            1
        }
    }
}

#[cfg(test)]
#[path = "weapon_profile_tests.rs"]
mod tests;
