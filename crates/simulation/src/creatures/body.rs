//! Explicit authored body packages and composition through the ordinary builder.
use super::composition::{HdOwner, PersonalHdId};
use super::{BuildError, CreatureBuild, HdChoices, PlannedAdvancement, RemovedAdvancement};
use crate::attributes::{Attribute, Attributes};
use crate::content_ids::{BodyFormId, SpeciesId};
use crate::progression::{HdLedger, HdSource, HitDie};
use crate::{BodySpec, HdCount};
use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::Arc;

/// No actor identity, possessions, faction, AI or personal training is imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyPackage {
    pub id: BodyFormId,
    pub species: SpeciesId,
    pub body: BodySpec,
    pub advancement: Vec<PlannedAdvancement>,
}
impl BodyPackage {
    pub fn validate(&self, content: &super::Content) -> Result<(), BuildError> {
        if !self.body.valid()
            || !content.catalog().species.contains_key(&self.species)
            || self.advancement.is_empty()
            || self.advancement.len() > crate::limits::HIT_DICE
        {
            return Err(BuildError::InvalidDefinition);
        }
        for (ordinal, row) in self.advancement.iter().enumerate() {
            if row.source != HdSource::Racial || row.choices.attribute.is_some() {
                return Err(BuildError::InvalidOwnership);
            }
            let owner = HdOwner::Body {
                form: self.id.clone(),
                ordinal: ordinal as u16,
            };
            let mut choices = row.choices.clone();
            choices.enroll_owner(&owner)?;
            super::composition::OwnedHitDie::new(
                owner,
                HitDie::new(row.source.clone(), 0),
                choices.purchases,
                None,
                content.rules(),
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OwnedBodySnapshot {
    pub package: Arc<BodyPackage>,
    pub rows: Vec<RemovedAdvancement>,
}
/// Original personal restoration receipts, independent of effective body slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PersonalRestorationPolicy {
    ChooseAgain,
    Authored(BTreeMap<PersonalHdId, PlannedAdvancement>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BodyBuild {
    pub base: Arc<BodyPackage>,
    pub active: Arc<BodyPackage>,
    pub health_root: u64,
    pub base_initial_attributes: Attributes,
    pub personal_seed_offset: u64,
    pub next_personal_id: u64,
    pub birth_personal_count: u64,
    pub personal_restoration: PersonalRestorationPolicy,
    /// Personal grants retain their original base-body funding ordinal. No HD is created.
    pub personal_body_grants: BTreeMap<u16, HdChoices>,
    pub acquisition_species: BTreeMap<HdOwner, SpeciesId>,
    /// Preserve each package's paid improvements across repeated visits.
    pub inactive: BTreeMap<BodyFormId, OwnedBodySnapshot>,
    /// Actual lost ownership is not restored by a change of form.
    pub drained: Vec<RemovedAdvancement>,
}
impl BodyBuild {
    pub(super) fn personal_body_grant_active(&self, ordinal: u16) -> bool {
        let donor = HdOwner::Body {
            form: self.base.id.clone(),
            ordinal,
        };
        !self
            .drained
            .iter()
            .any(|row| row.hit_die.owner() == Some(&donor))
    }

    /// Original funding proof remains available even while its body is inactive/lost.
    pub(super) fn validate_personal_body_grants(
        &self,
        content: &super::Content,
    ) -> Result<(), BuildError> {
        for (&ordinal, grant) in &self.personal_body_grants {
            let original = self
                .base
                .advancement
                .get(usize::from(ordinal))
                .ok_or(BuildError::InvalidOwnership)?;
            if (grant.attribute.is_none() && grant.purchases.is_empty())
                || grant
                    .attribute
                    .is_some_and(|attribute| attribute != Attribute::Willpower)
                || grant.purchases.iter().any(|purchase| {
                    !matches!(
                        purchase.selection,
                        super::PurchaseSelection::SkillRank(crate::attributes::Skill::Breath)
                    )
                })
                || grant.attribute.is_some_and(|_| {
                    !(u64::from(ordinal) + 1).is_multiple_of(u64::from(
                        content.rules().constants().attribute_opportunity_interval,
                    ))
                })
            {
                return Err(BuildError::InvalidOwnership);
            }
            let owner = HdOwner::Body {
                form: self.base.id.clone(),
                ordinal,
            };
            grant.validate_purchase_ids(Some(&owner))?;
            let mut combined = original.choices.clone();
            combined.enroll_owner(&owner)?;
            let offset = combined.purchases.len() as u64;
            if grant.purchase_highwater != offset + grant.purchases.len() as u64
                || grant.purchases.iter().enumerate().any(|(index, purchase)| {
                    purchase
                        .id
                        .as_ref()
                        .is_none_or(|id| id.serial.get() != offset + index as u64 + 1)
                })
            {
                return Err(BuildError::InvalidOwnership);
            }
            combined.purchases.extend(grant.purchases.clone());
            combined.purchase_highwater = grant.purchase_highwater;
            combined.validate_purchase_ids(Some(&owner))?;
            // Validate the original shared pool once, never two independent budgets.
            super::composition::OwnedHitDie::new(
                owner,
                HitDie::new(HdSource::Racial, 0),
                combined.purchases,
                None,
                content.rules(),
            )?;
        }
        Ok(())
    }
}
impl CreatureBuild {
    /// Game admission requires every retained immutable definition to belong to
    /// the exact sealed catalog. Standalone builders may project uncatalogued
    /// packages, but those projections are not executable actor state.
    pub(crate) fn authored_body_packages_valid(&self) -> bool {
        let Some(body) = &self.body_build else {
            return true;
        };
        std::iter::once(&body.base)
            .chain(std::iter::once(&body.active))
            .chain(body.inactive.values().map(|snapshot| &snapshot.package))
            .all(|package| {
                self.content
                    .body_package(&package.id)
                    .is_some_and(|authored| authored.as_ref() == package.as_ref())
            })
    }
    pub fn personal_hd_count(&self) -> HdCount {
        if self.body_build.is_none() {
            return self.ledger.total_hd();
        }
        HdCount::from_usize(
            self.ledger
                .entries()
                .iter()
                .filter(|row| row.owner().is_some_and(HdOwner::is_personal))
                .count(),
        )
        .expect("bounded resident ledger")
    }
    pub fn personal_health_root_seed(&self) -> Option<u64> {
        self.body_build
            .as_ref()
            .map(|body| body.health_root)
            .or_else(|| self.ledger.health_root_seed())
    }
    pub(crate) fn body_birth_personal_grants(&self) -> BTreeMap<u16, HdChoices> {
        self.body_build.as_ref().map_or_else(BTreeMap::new, |body| {
            body.personal_body_grants
                .iter()
                .map(|(&ordinal, grant)| {
                    let mut authored = grant.clone();
                    authored.purchase_highwater = 0;
                    for purchase in &mut authored.purchases {
                        purchase.id = None;
                    }
                    (ordinal, authored)
                })
                .collect()
        })
    }

    pub fn active_body_package(&self) -> Option<&Arc<BodyPackage>> {
        self.body_build.as_ref().map(|body| &body.active)
    }
    pub fn base_body_package(&self) -> Option<&Arc<BodyPackage>> {
        self.body_build.as_ref().map(|body| &body.base)
    }

    /// Explicit birth partition: the package prefix is authored, never inferred
    /// from racial-vs-class source. Later earned racial rows remain personal.
    fn enroll_body_package(
        &self,
        package: Arc<BodyPackage>,
        health_root: u64,
        mut personal_body_grants: BTreeMap<u16, HdChoices>,
    ) -> Result<Self, BuildError> {
        if self.body_build.is_some()
            || !self.templates.is_empty()
            || self.ledger.drained().next().is_some()
            || self.species != package.species
            || self.ledger.health_root_seed() != Some(health_root)
            || self.ledger.entries().len() < package.advancement.len()
        {
            return Err(BuildError::InvalidOwnership);
        }
        package.validate(self.content())?;
        if let super::RestorationPolicy::Authored(plan) = self.restoration_policy() {
            if plan.len() != self.ledger.entries().len()
                || plan
                    .iter()
                    .zip(self.ledger.entries().iter().zip(self.choices()))
                    .any(|(row, (die, choices))| {
                        row.source != die.source() || &row.choices != choices
                    })
            {
                return Err(BuildError::InvalidOwnership);
            }
        }
        let body_count = package.advancement.len();
        let mut package_choices = Vec::with_capacity(body_count);
        for (index, authored) in package.advancement.iter().enumerate() {
            let original = &self.choices()[index];
            let grant = personal_body_grants.get_mut(&(index as u16));
            let mut combined = authored.choices.clone();
            if let Some(grant) = grant.as_ref() {
                grant.validate_purchase_ids(None)?;
                combined.purchases.extend(grant.purchases.clone());
                combined.attribute = grant.attribute;
            }
            if self.ledger.entries()[index].source() != authored.source || original != &combined {
                return Err(BuildError::InvalidOwnership);
            }
            let owner = HdOwner::Body {
                form: package.id.clone(),
                ordinal: index as u16,
            };
            combined.enroll_owner(&owner)?;
            if let Some(grant) = grant {
                grant.purchases = combined.purchases[authored.choices.purchases.len()..].to_vec();
                grant.purchase_highwater = combined.purchase_highwater;
            }
            package_choices.push(authored.choices.clone());
        }
        let mut candidate = self.clone();
        candidate.advancement.choices[..body_count].clone_from_slice(&package_choices);
        let mut rows = Vec::with_capacity(self.ledger.entries().len());
        let mut acquisition_species = BTreeMap::new();
        for (index, row) in self.ledger.entries().iter().enumerate() {
            let owner = if index < body_count {
                HdOwner::Body {
                    form: package.id.clone(),
                    ordinal: index as u16,
                }
            } else {
                HdOwner::Personal {
                    id: PersonalHdId(
                        NonZeroU64::new((index - body_count + 1) as u64)
                            .expect("positive personal ordinal"),
                    ),
                }
            };
            rows.push(
                row.clone()
                    .with_owner(owner.clone())
                    .map_err(BuildError::from)?,
            );
            candidate.advancement.choices[index].enroll_owner(&owner)?;
            acquisition_species.insert(owner, self.species.clone());
        }
        candidate.ledger = HdLedger::composed(rows).map_err(BuildError::from)?;
        let personal_restoration = if matches!(
            self.restoration_policy(),
            super::RestorationPolicy::Authored(_)
        ) {
            PersonalRestorationPolicy::Authored(candidate.personal_restoration_receipts())
        } else {
            PersonalRestorationPolicy::ChooseAgain
        };
        candidate.body_build = Some(BodyBuild {
            base: package.clone(),
            active: package,
            health_root,
            base_initial_attributes: self.initial_attributes(),
            personal_seed_offset: body_count as u64,
            next_personal_id: (self.ledger.entries().len() - body_count + 1) as u64,
            birth_personal_count: (self.ledger.entries().len() - body_count) as u64,
            acquisition_species,
            personal_restoration,
            personal_body_grants,
            inactive: BTreeMap::new(),
            drained: Vec::new(),
        });
        candidate.advancement.restoration = super::RestorationPolicy::ChooseAgain;
        candidate
            .body_build
            .as_ref()
            .expect("just enrolled")
            .validate_personal_body_grants(candidate.content())?;
        // Enrollment is fresh admission, not restoration of a saved receipt.
        // Retained form projections may saturate physical attributes, but an
        // authored birth cannot use that projection to hide an over-cap gain.
        candidate.validate_initial_attribute_admission()?;
        Ok(candidate)
    }

    pub fn with_body_package(
        &self,
        package: Arc<BodyPackage>,
        health_root: u64,
    ) -> Result<Self, BuildError> {
        self.with_body_package_personal_grants(package, health_root, BTreeMap::new())
    }

    /// Explicit partition of original funded Willpower/Breath acquisitions.
    pub fn with_body_package_personal_grants(
        &self,
        package: Arc<BodyPackage>,
        health_root: u64,
        personal_body_grants: BTreeMap<u16, HdChoices>,
    ) -> Result<Self, BuildError> {
        let candidate = self.enroll_body_package(package, health_root, personal_body_grants)?;
        candidate.derive()?;
        if let Some(learning) = &candidate.intellect_learning {
            learning.validate(&candidate)?;
        }
        Ok(candidate)
    }

    /// Interpret personal milestones only after explicit birth ownership exists.
    pub fn from_recorded_body_package(
        content: Arc<super::Content>,
        package: Arc<BodyPackage>,
        initial_attributes: Attributes,
        ledger: HdLedger,
        binding: crate::attributes::ManaBinding,
        advancement: super::BuildAdvancement,
        health_root: u64,
    ) -> Result<Self, BuildError> {
        Self::from_recorded_body_package_personal_grants(
            content,
            package,
            initial_attributes,
            ledger,
            binding,
            advancement,
            health_root,
            BTreeMap::new(),
        )
    }

    /// Reconstruct an explicit birth partition before validating personal milestones.
    #[allow(clippy::too_many_arguments)]
    pub fn from_recorded_body_package_personal_grants(
        content: Arc<super::Content>,
        package: Arc<BodyPackage>,
        initial_attributes: Attributes,
        ledger: HdLedger,
        binding: crate::attributes::ManaBinding,
        mut advancement: super::BuildAdvancement,
        health_root: u64,
        personal_body_grants: BTreeMap<u16, HdChoices>,
    ) -> Result<Self, BuildError> {
        let authored = matches!(
            &advancement.restoration,
            super::RestorationPolicy::Authored(_)
        );
        let mut future = Vec::new();
        if let super::RestorationPolicy::Authored(plan) = &mut advancement.restoration {
            // Rows beyond the birth ledger schedule later personal growth.
            if plan.len() < ledger.entries().len() {
                return Err(BuildError::InvalidOwnership);
            }
            future = plan.split_off(ledger.entries().len());
        }
        if let super::RestorationPolicy::Authored(plan) = &advancement.restoration {
            if plan.len() != ledger.entries().len()
                || plan
                    .iter()
                    .zip(ledger.entries().iter().zip(&advancement.choices))
                    .any(|(row, (die, choices))| {
                        row.source != die.source() || &row.choices != choices
                    })
            {
                return Err(BuildError::InvalidOwnership);
            }
            advancement.restoration = super::RestorationPolicy::ChooseAgain;
        }
        if !future.is_empty() {
            // The complete schedule must be admissible before any row is owned.
            let mut sources: Vec<_> = ledger.entries().iter().map(|row| row.source()).collect();
            sources.extend(future.iter().map(|row| row.source.clone()));
            let mut intended = advancement.clone();
            intended
                .choices
                .extend(future.iter().map(|row| row.choices.clone()));
            Self::from_recorded_body_package_personal_grants(
                content.clone(),
                package.clone(),
                initial_attributes,
                HdLedger::seeded(sources, health_root).map_err(BuildError::from)?,
                binding,
                intended,
                health_root,
                personal_body_grants.clone(),
            )?;
        }
        let raw = Self::recorded_structure(
            content,
            package.species.clone(),
            initial_attributes,
            ledger,
            binding,
            advancement,
            vec![],
            None,
        )?;
        let mut build = raw.enroll_body_package(package, health_root, personal_body_grants)?;
        if authored {
            let mut receipts = build.personal_restoration_receipts();
            let first = build
                .body_build
                .as_ref()
                .expect("just enrolled")
                .next_personal_id;
            for (offset, mut row) in future.into_iter().enumerate() {
                let id = PersonalHdId(
                    NonZeroU64::new(
                        first
                            .checked_add(offset as u64)
                            .ok_or(BuildError::Arithmetic)?,
                    )
                    .ok_or(BuildError::InvalidOwnership)?,
                );
                row.choices.enroll_owner(&HdOwner::Personal { id })?;
                receipts.insert(id, row);
            }
            build
                .body_build
                .as_mut()
                .expect("just enrolled")
                .personal_restoration = PersonalRestorationPolicy::Authored(receipts);
        }
        let derived = build.derive()?;
        build.intellect_learning =
            Some(Arc::new(super::expertise_learning::IntellectLearning::new(
                &build,
                derived.intellect_sources.clone(),
                derived.intellect_capacity,
            )?));
        Ok(build)
    }

    /// Builder-only projection. Game owns geometry, equipment and paid actions.
    pub fn replace_body_package(&self, package: Arc<BodyPackage>) -> Result<Self, BuildError> {
        package.validate(self.content())?;
        let state = self
            .body_build
            .as_ref()
            .ok_or(BuildError::InvalidOwnership)?;
        if !self.templates.is_empty() {
            return Err(BuildError::InvalidDefinition);
        }
        if state.active.id == package.id {
            return if state.active == package {
                Ok(self.clone())
            } else {
                Err(BuildError::InvalidDefinition)
            };
        }
        // A drained package needs an explicitly selected cross-package loss
        // policy. Reject atomically rather than silently laundering the loss.
        if state.drained.iter().any(|row| {
            row.hit_die
                .owner()
                .is_some_and(|owner| !owner.is_personal())
        }) {
            return Err(BuildError::InvalidOwnership);
        }
        let mut candidate = self.clone();
        let mut state = state.clone();
        let (mut current_body, mut personal) = (Vec::new(), Vec::new());
        for (die, choices) in self.ledger.entries().iter().zip(self.choices()) {
            let row = RemovedAdvancement {
                hit_die: die.clone(),
                choices: choices.clone(),
            };
            match die.owner() {
                Some(HdOwner::Body { form, .. }) if form == &state.active.id => {
                    current_body.push(row)
                }
                Some(HdOwner::Personal { .. }) => personal.push(row),
                _ => return Err(BuildError::InvalidOwnership),
            }
        }
        if state.inactive.len() >= crate::limits::CATALOG_ENTRIES
            && !state.inactive.contains_key(&package.id)
        {
            return Err(BuildError::DerivedLimit);
        }
        state.inactive.insert(
            state.active.id.clone(),
            OwnedBodySnapshot {
                package: state.active.clone(),
                rows: current_body,
            },
        );
        let body_rows = if let Some(snapshot) = state.inactive.remove(&package.id) {
            if snapshot.package != package {
                return Err(BuildError::InvalidDefinition);
            }
            snapshot.rows
        } else {
            // Stable actor/form stream, separate from combat randomness.
            let seed = package
                .id
                .as_str()
                .bytes()
                .fold(state.health_root ^ 0x626f64795f68645f, |seed, byte| {
                    seed.wrapping_mul(0x100000001b3) ^ u64::from(byte)
                });
            package
                .advancement
                .iter()
                .enumerate()
                .map(|(ordinal, row)| {
                    let owner = HdOwner::Body {
                        form: package.id.clone(),
                        ordinal: ordinal as u16,
                    };
                    let die = HitDie::new(
                        row.source.clone(),
                        crate::dice::random_word_at(seed, ordinal as u64),
                    )
                    .with_owner(owner.clone())
                    .map_err(BuildError::from)?;
                    let mut choices = row.choices.clone();
                    choices.enroll_owner(&owner)?;
                    state
                        .acquisition_species
                        .insert(owner, package.species.clone());
                    Ok(RemovedAdvancement {
                        hit_die: die,
                        choices,
                    })
                })
                .collect::<Result<Vec<_>, BuildError>>()?
        };
        let mut rows = body_rows;
        rows.append(&mut personal);
        candidate.ledger = HdLedger::composed(rows.iter().map(|row| row.hit_die.clone()).collect())
            .map_err(BuildError::from)?;
        candidate.advancement.choices = rows.into_iter().map(|row| row.choices).collect();
        candidate.species = package.species.clone();
        let form_attributes = if package.id == state.base.id {
            state.base_initial_attributes
        } else {
            candidate.species().default_attributes
        };
        let mut attributes = self.initial_attributes.values();
        attributes.strength = form_attributes.get(Attribute::Strength);
        attributes.speed = form_attributes.get(Attribute::Speed);
        candidate.initial_attributes =
            Attributes::new(attributes).map_err(|_| BuildError::AttributeCap)?;
        if state.acquisition_species.len()
            > crate::limits::CATALOG_ENTRIES * crate::limits::HIT_DICE + crate::limits::HIT_DICE
        {
            return Err(BuildError::DerivedLimit);
        }
        state.active = package;
        candidate.body_build = Some(state);
        let kind = super::derive::kind_for_templates(&candidate, &candidate.templates)?;
        candidate.bind_health_origins(&kind)?;
        candidate.derive()?;
        Ok(candidate)
    }
    pub(super) fn personal_attribute_ordinal(&self, effective: usize) -> Option<u64> {
        if self.body_build.is_none() {
            return Some(effective as u64 + 1);
        }
        self.ledger
            .entries()
            .get(effective)?
            .owner()
            .filter(|owner| owner.is_personal())?;
        Some(
            self.ledger.entries()[..=effective]
                .iter()
                .filter(|row| row.owner().is_some_and(HdOwner::is_personal))
                .count() as u64,
        )
    }
}

impl CreatureBuild {
    /// Canonical personal-learning/reward projection. No effects, Health changes,
    /// fit checks or recursive compilation: this is never installed on the actor.
    pub(super) fn intellect_build(&self) -> Result<Option<Self>, BuildError> {
        let Some(body) = &self.body_build else {
            return Ok(None);
        };
        if body.active.id == body.base.id {
            return Ok(None);
        }
        let base_rows = body
            .inactive
            .get(&body.base.id)
            .ok_or(BuildError::InvalidOwnership)?;
        if base_rows.package != body.base {
            return Err(BuildError::InvalidDefinition);
        }
        let mut rows = base_rows.rows.clone();
        let mut active_rows = Vec::new();
        for (die, choices) in self.ledger.entries().iter().zip(self.choices()) {
            let row = RemovedAdvancement {
                hit_die: die.clone(),
                choices: choices.clone(),
            };
            if die.owner().is_some_and(HdOwner::is_personal) {
                rows.push(row);
            } else {
                active_rows.push(row);
            }
        }
        let mut candidate = self.clone();
        candidate.ledger = HdLedger::composed(rows.iter().map(|row| row.hit_die.clone()).collect())
            .map_err(BuildError::from)?;
        candidate.advancement.choices = rows.into_iter().map(|row| row.choices).collect();
        candidate.species = body.base.species.clone();
        candidate.initial_attributes = body.base_initial_attributes;
        let projected = candidate.body_build.as_mut().expect("copied owned body");
        projected.inactive.remove(&body.base.id);
        projected.inactive.insert(
            body.active.id.clone(),
            OwnedBodySnapshot {
                package: body.active.clone(),
                rows: active_rows,
            },
        );
        projected.active = body.base.clone();
        Ok(Some(candidate))
    }
    pub(crate) fn reward_threat_tier(&self, active_threat: u64) -> Result<u64, BuildError> {
        let Some(original) = self.intellect_build()? else {
            return Ok(active_threat);
        };
        Ok(super::derive::PermanentCreature::compile(&original)?.threat_tier())
    }
    pub(crate) fn body_package_loss_count(&self) -> HdCount {
        HdCount::from_usize(self.body_build.as_ref().map_or(0, |body| {
            body.drained
                .iter()
                .filter(|row| {
                    row.hit_die
                        .owner()
                        .is_some_and(|owner| !owner.is_personal())
                })
                .count()
        }))
        .expect("bounded drained rows")
    }
    pub(super) fn remove_owned_hit_die(&mut self) -> Option<RemovedAdvancement> {
        let mut rows = self.ledger.entries().to_vec();
        let hit_die = rows.pop()?;
        let choices = self.advancement.choices.pop().expect("one choice per HD");
        let removed = RemovedAdvancement { hit_die, choices };
        self.body_build
            .as_mut()
            .expect("owned build")
            .drained
            .push(removed.clone());
        self.ledger = HdLedger::composed(rows).expect("retained bounded owned rows");
        Some(removed)
    }
    pub(super) fn add_personal_hit_die(
        &mut self,
        source: HdSource,
        root_seed: u64,
    ) -> Result<(), BuildError> {
        self.edit_intellect_advancement(|candidate| {
            let body = candidate
                .body_build
                .as_mut()
                .ok_or(BuildError::InvalidOwnership)?;
            if root_seed != body.health_root {
                return Err(BuildError::InvalidHealthRoot);
            }
            let mut rows = candidate.ledger.entries().to_vec();
            let restored = body
                .drained
                .iter()
                .rposition(|row| row.hit_die.owner().is_some_and(HdOwner::is_personal))
                .map(|index| body.drained.remove(index));
            let row = if let Some(mut row) = restored {
                let Some(HdOwner::Personal { id }) = row.hit_die.owner() else {
                    return Err(BuildError::InvalidOwnership);
                };
                let highwater = row.choices.purchase_highwater;
                row.choices = match &body.personal_restoration {
                    PersonalRestorationPolicy::ChooseAgain => HdChoices::default(),
                    PersonalRestorationPolicy::Authored(plan) => match plan.get(id) {
                        Some(authored) if authored.source == source => authored.choices.clone(),
                        Some(_) => return Err(BuildError::InvalidOwnership),
                        None => HdChoices::default(),
                    },
                };
                // Returning a declared receipt may restore its original serial,
                // but no subsequent new purchase can reuse any retired serial.
                row.choices.purchase_highwater = row.choices.purchase_highwater.max(highwater);
                if row.hit_die.source() != source {
                    // ChooseAgain (including exhausted authored recipes) may
                    // select a different future source. Keep the same random
                    // stream and owner, but bind the new source's legitimate
                    // Health origin just as ordinary unowned regrowth does.
                    let owner = row
                        .hit_die
                        .owner()
                        .cloned()
                        .ok_or(BuildError::InvalidOwnership)?;
                    row.hit_die = HitDie::new(source, row.hit_die.health_seed())
                        .with_owner(owner.clone())
                        .map_err(BuildError::from)?;
                    body.acquisition_species
                        .insert(owner, candidate.species.clone());
                }
                row
            } else {
                let id = PersonalHdId(
                    NonZeroU64::new(body.next_personal_id).ok_or(BuildError::InvalidOwnership)?,
                );
                body.next_personal_id = body
                    .next_personal_id
                    .checked_add(1)
                    .ok_or(BuildError::Arithmetic)?;
                let owner = HdOwner::Personal { id };
                let stream_ordinal = body
                    .personal_seed_offset
                    .checked_add(id.0.get() - 1)
                    .ok_or(BuildError::Arithmetic)?;
                let die = HitDie::new(
                    source.clone(),
                    crate::dice::random_word_at(root_seed, stream_ordinal),
                )
                .with_owner(owner.clone())
                .map_err(BuildError::from)?;
                body.acquisition_species
                    .insert(owner.clone(), candidate.species.clone());
                let choices = match &body.personal_restoration {
                    PersonalRestorationPolicy::Authored(plan) => match plan.get(&id) {
                        Some(authored) if authored.source == source => authored.choices.clone(),
                        Some(_) => return Err(BuildError::InvalidOwnership),
                        None => {
                            let mut choices = HdChoices::default();
                            choices.enroll_owner(&owner)?;
                            choices
                        }
                    },
                    PersonalRestorationPolicy::ChooseAgain => {
                        let mut choices = HdChoices::default();
                        choices.enroll_owner(&owner)?;
                        choices
                    }
                };
                RemovedAdvancement {
                    hit_die: die,
                    choices,
                }
            };
            rows.push(row.hit_die);
            candidate.advancement.choices.push(row.choices);
            candidate.ledger = HdLedger::composed(rows).map_err(BuildError::from)?;
            Ok(())
        })
        .map(|_| ())
    }
    pub(crate) fn restore_owned_package_hit_die(&mut self) -> Result<bool, BuildError> {
        let Some(body) = &self.body_build else {
            return Ok(false);
        };
        let Some(index) = body.drained.iter().rposition(|row| {
            row.hit_die
                .owner()
                .is_some_and(|owner| !owner.is_personal())
        }) else {
            return Ok(false);
        };
        self.edit_intellect_advancement(|candidate| {
            let body = candidate.body_build.as_mut().ok_or(BuildError::InvalidOwnership)?;
            let row = body.drained.remove(index);
            if !matches!(row.hit_die.owner(), Some(HdOwner::Body { form, .. }) if form == &body.active.id) { return Err(BuildError::InvalidOwnership); }
            let mut rows = candidate.ledger.entries().to_vec();
            let index = rows.iter().position(|row| row.owner().is_some_and(HdOwner::is_personal)).unwrap_or(rows.len());
            rows.insert(index, row.hit_die);
            candidate.advancement.choices.insert(index, row.choices);
            candidate.ledger = HdLedger::composed(rows).map_err(BuildError::from)?;
            Ok(())
        })?;
        Ok(true)
    }
}

impl CreatureBuild {
    pub(super) fn owned_personal_restoration_source(&self) -> Option<HdSource> {
        let body = self.body_build.as_ref()?;
        let PersonalRestorationPolicy::Authored(plan) = &body.personal_restoration else {
            return None;
        };
        // Regrowth restores the latest drained row; otherwise an authored
        // future row may schedule the next new personal Hit Die.
        let id = body
            .drained
            .iter()
            .rev()
            .find_map(|row| match row.hit_die.owner() {
                Some(HdOwner::Personal { id }) => Some(*id),
                _ => None,
            })
            .or_else(|| NonZeroU64::new(body.next_personal_id).map(PersonalHdId))?;
        plan.get(&id).map(|row| row.source.clone())
    }
    fn personal_restoration_receipts(&self) -> BTreeMap<PersonalHdId, PlannedAdvancement> {
        self.ledger
            .entries()
            .iter()
            .zip(self.choices())
            .filter_map(|(die, choices)| {
                let Some(HdOwner::Personal { id }) = die.owner() else {
                    return None;
                };
                Some((
                    *id,
                    PlannedAdvancement {
                        source: die.source(),
                        choices: choices.clone(),
                    },
                ))
            })
            .collect()
    }
    /// Authored future personal rows after the birth rows, as unowned
    /// declarations; a new birth assigns their owners and purchase serials.
    pub(crate) fn body_birth_growth_schedule(&self) -> Vec<PlannedAdvancement> {
        let Some(body) = &self.body_build else {
            return Vec::new();
        };
        let PersonalRestorationPolicy::Authored(plan) = &body.personal_restoration else {
            return Vec::new();
        };
        plan.iter()
            .filter(|(id, _)| id.0.get() > body.birth_personal_count)
            .map(|(_, row)| {
                let mut row = row.clone();
                row.choices.purchase_highwater = 0;
                for purchase in &mut row.choices.purchases {
                    purchase.id = None;
                }
                row
            })
            .collect()
    }
    pub(crate) fn body_birth_uses_authored_restoration(&self) -> bool {
        self.body_build.as_ref().is_some_and(|body| {
            matches!(
                body.personal_restoration,
                PersonalRestorationPolicy::Authored(_)
            )
        })
    }
    /// Only an original authored body build can serve as a fresh-birth recipe.
    /// Runtime swaps, retained losses and acquired progression are not cloned.
    pub(crate) fn body_birth_package(&self) -> Result<Option<Arc<BodyPackage>>, BuildError> {
        let Some(body) = &self.body_build else {
            return Ok(None);
        };
        if body.active != body.base
            || !body.inactive.is_empty()
            || !body.drained.is_empty()
            || !self.templates.is_empty()
            || self.strength_acquired != 0
            || self.ledger.drained().next().is_some()
            || self
                .ledger
                .entries()
                .iter()
                .any(|row| row.health_improvement() != 0)
            || body.birth_personal_count != self.personal_hd_count().get()
            || body.birth_personal_count.checked_add(1) != Some(body.next_personal_id)
        {
            return Err(BuildError::InvalidOwnership);
        }
        if let PersonalRestorationPolicy::Authored(plan) = &body.personal_restoration {
            let birth: BTreeMap<_, _> = plan
                .iter()
                .filter(|(id, _)| id.0.get() <= body.birth_personal_count)
                .map(|(id, row)| (*id, row.clone()))
                .collect();
            if self.personal_restoration_receipts() != birth {
                return Err(BuildError::InvalidOwnership);
            }
        }
        Ok(Some(body.base.clone()))
    }
}
