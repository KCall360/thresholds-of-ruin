//! Owned advancement, independent of transient effects and control ownership.
use super::{BuildError, CreatureBuild};
use crate::attributes::{Attribute, Skill};
use crate::content_ids::{ExpertiseId, FormId, PointPoolId, TalentId};
use crate::progression::HdSource;
use crate::rules::PurchaseKind;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurchaseSelection {
    SkillRank(Skill),
    Expertise(ExpertiseId),
    Talent(TalentId),
    TalentForm { talent: TalentId, form: FormId },
}
impl PurchaseSelection {
    pub fn kind(&self) -> PurchaseKind {
        match self {
            Self::SkillRank(_) => PurchaseKind::SkillRank,
            Self::Expertise(_) => PurchaseKind::Expertise,
            Self::Talent(_) | Self::TalentForm { .. } => PurchaseKind::Talent,
        }
    }
    pub fn talent(&self) -> Option<&TalentId> {
        match self {
            Self::SkillRank(_) | Self::Expertise(_) => None,
            Self::Talent(talent) | Self::TalentForm { talent, .. } => Some(talent),
        }
    }
    pub fn expertise(&self) -> Option<&ExpertiseId> {
        match self {
            Self::Expertise(expertise) => Some(expertise),
            Self::SkillRank(_) | Self::Talent(_) | Self::TalentForm { .. } => None,
        }
    }
    pub fn form(&self) -> Option<&FormId> {
        match self {
            Self::TalentForm { form, .. } => Some(form),
            Self::SkillRank(_) | Self::Expertise(_) | Self::Talent(_) => None,
        }
    }
    /// Validate the owned operand even when its talent is currently dormant.
    /// Eligibility is separate; a valid choice never changes as gates change.
    pub(crate) fn validate(&self, rules: &crate::rules::Ruleset) -> Result<(), BuildError> {
        if let Some(expertise) = self.expertise() {
            return rules
                .expertise(expertise)
                .map(|_| ())
                .ok_or(BuildError::UnknownExpertise);
        }
        let Some(talent) = self.talent() else {
            return Ok(());
        };
        let definition = rules.talent(talent).ok_or(BuildError::UnknownTalent)?;
        match self.form() {
            None if definition.form_choices.is_empty() => Ok(()),
            Some(form) if definition.form_choices.contains(form) && rules.form(form).is_some() => {
                Ok(())
            }
            _ => Err(BuildError::InvalidTalentForm),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvancementPurchase {
    pub id: Option<super::composition::PurchaseId>,
    pub pool: PointPoolId,
    pub selection: PurchaseSelection,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HdChoices {
    /// Monotonic per-HD allocation watermark; removing a purchase never lowers it.
    pub purchase_highwater: u64,
    pub purchases: Vec<AdvancementPurchase>,
    pub attribute: Option<Attribute>,
}
impl HdChoices {
    /// Bind an authored row once. Recomposition retains these IDs unchanged.
    pub fn enroll_owner(&mut self, owner: &super::composition::HdOwner) -> Result<(), BuildError> {
        if self.purchases.is_empty() || self.purchases.iter().any(|purchase| purchase.id.is_some())
        {
            return self.validate_purchase_ids(Some(owner));
        }
        if self.purchase_highwater != 0 {
            return Err(BuildError::InvalidOwnership);
        }
        let mut candidate = self.clone();
        for purchase in &mut candidate.purchases {
            candidate.purchase_highwater = candidate
                .purchase_highwater
                .checked_add(1)
                .ok_or(BuildError::Arithmetic)?;
            purchase.id = Some(super::composition::PurchaseId {
                owner: owner.clone(),
                serial: std::num::NonZeroU64::new(candidate.purchase_highwater)
                    .ok_or(BuildError::Arithmetic)?,
            });
        }
        candidate.validate_purchase_ids(Some(owner))?;
        *self = candidate;
        Ok(())
    }
    pub(crate) fn validate_purchase_ids(
        &self,
        owner: Option<&super::composition::HdOwner>,
    ) -> Result<(), BuildError> {
        let mut ids = std::collections::BTreeSet::new();
        if owner.is_none() && self.purchase_highwater != 0 {
            return Err(BuildError::InvalidOwnership);
        }
        for purchase in &self.purchases {
            match (owner, &purchase.id) {
                (None, None) => {}
                (Some(owner), Some(id))
                    if &id.owner == owner
                        && id.serial.get() <= self.purchase_highwater
                        && ids.insert(id.serial) => {}
                _ => return Err(BuildError::InvalidOwnership),
            }
        }
        Ok(())
    }
    pub(super) fn allocate_purchase(
        &mut self,
        owner: Option<&super::composition::HdOwner>,
        pool: PointPoolId,
        selection: PurchaseSelection,
    ) -> Result<AdvancementPurchase, BuildError> {
        self.validate_purchase_ids(owner)?;
        let id = if let Some(owner) = owner {
            if !owner.is_personal() {
                return Err(BuildError::InvalidOwnership);
            }
            let serial = self
                .purchase_highwater
                .checked_add(1)
                .ok_or(BuildError::Arithmetic)?;
            self.purchase_highwater = serial;
            Some(super::composition::PurchaseId {
                owner: owner.clone(),
                serial: std::num::NonZeroU64::new(serial).ok_or(BuildError::Arithmetic)?,
            })
        } else {
            None
        };
        Ok(AdvancementPurchase {
            id,
            pool,
            selection,
        })
    }
    pub fn talents(&self) -> impl Iterator<Item = &TalentId> {
        self.purchases
            .iter()
            .filter_map(|purchase| purchase.selection.talent())
    }
    pub fn skills(&self) -> impl Iterator<Item = Skill> + '_ {
        self.purchases
            .iter()
            .filter_map(|purchase| match purchase.selection {
                PurchaseSelection::SkillRank(skill) => Some(skill),
                PurchaseSelection::Expertise(_)
                | PurchaseSelection::Talent(_)
                | PurchaseSelection::TalentForm { .. } => None,
            })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedAdvancement {
    pub source: HdSource,
    pub choices: HdChoices,
}
/// Explicit provenance, never inferred from whether an actor is controlled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestorationPolicy {
    ChooseAgain,
    Authored(Vec<PlannedAdvancement>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildAdvancement {
    pub starting_skills: [Skill; 2],
    pub choices: Vec<HdChoices>,
    pub restoration: RestorationPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointPoolBalance {
    pub pool: PointPoolId,
    pub granted: u16,
    pub spent: u16,
}
impl PointPoolBalance {
    pub fn unspent(&self) -> u16 {
        self.granted - self.spent
    }
}
/// A validated permanent-build projection. Actor equipment, injury, clocks and
/// reservations still belong to CreatureState and must be projected there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvancementPreview {
    candidate: CreatureBuild,
    derived: super::DerivedCreature,
}
impl AdvancementPreview {
    pub fn build(&self) -> &CreatureBuild {
        &self.candidate
    }
    pub fn derived(&self) -> &super::DerivedCreature {
        &self.derived
    }
    pub fn into_build(self) -> CreatureBuild {
        self.candidate
    }
}
/// Only automatic growth may retain an otherwise valid funded ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GrowthBuildOutcome {
    Applied,
    Obstructed,
}

impl CreatureBuild {
    pub fn advancement(&self) -> &BuildAdvancement {
        &self.advancement
    }
    pub fn starting_skills(&self) -> &[Skill; 2] {
        &self.advancement.starting_skills
    }
    /// Effective declared personal restoration policy. The recorded outer
    /// positional recipe remains normalized for explicitly composed bodies.
    pub fn uses_authored_restoration(&self) -> bool {
        self.body_build.as_ref().map_or_else(
            || matches!(self.restoration_policy(), RestorationPolicy::Authored(_)),
            |body| {
                matches!(
                    body.personal_restoration,
                    super::body::PersonalRestorationPolicy::Authored(_)
                )
            },
        )
    }
    pub(crate) fn next_authored_restoration_source(&self) -> Option<HdSource> {
        if self.body_build.is_some() {
            return self.owned_personal_restoration_source();
        }
        match self.restoration_policy() {
            RestorationPolicy::Authored(plan) => {
                plan.get(self.choices().len()).map(|row| row.source.clone())
            }
            RestorationPolicy::ChooseAgain => None,
        }
    }
    /// Raw positional record policy; use uses_authored_restoration for semantics.
    pub fn restoration_policy(&self) -> &RestorationPolicy {
        &self.advancement.restoration
    }

    /// The returned declaration-ordered ledger reconciles every purchase exactly once.
    pub fn point_balances(
        &self,
        owner: crate::HdIndex,
    ) -> Result<Vec<PointPoolBalance>, BuildError> {
        let owner = owner.to_usize().map_err(BuildError::from)?;
        let entry = self
            .ledger
            .entries()
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        let choice = self
            .choices()
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        choice.validate_purchase_ids(entry.owner())?;
        // A retained personal acquisition spends its original donor's pool,
        // not the unrelated current form's pool. Body rows remain locked.
        let retained = match (&self.body_build, entry.owner()) {
            (Some(body), Some(super::composition::HdOwner::Body { form, ordinal }))
                if form == &body.base.id =>
            {
                body.personal_body_grants.get(ordinal)
            }
            _ => None,
        };
        if retained.is_some() {
            self.body_build
                .as_ref()
                .expect("retained body receipt")
                .validate_personal_body_grants(self.content())?;
        }
        let retained_purchases = retained.map_or(&[][..], |grant| grant.purchases.as_slice());
        if choice.purchases.len() + retained_purchases.len() > crate::limits::POINTS_PER_HIT_DIE {
            return Err(BuildError::NoAdvancementPoint);
        }
        let grants = self
            .ruleset()
            .point_grants(&entry.source())
            .ok_or(BuildError::UnknownClass)?;
        let mut balances: BTreeMap<_, _> = grants
            .iter()
            .map(|grant| {
                (
                    grant.pool.clone(),
                    PointPoolBalance {
                        pool: grant.pool.clone(),
                        granted: grant.points,
                        spent: 0,
                    },
                )
            })
            .collect();
        for purchase in choice.purchases.iter().chain(retained_purchases) {
            let pool = self
                .ruleset()
                .point_pool(&purchase.pool)
                .ok_or(BuildError::UnknownPointPool)?;
            if !pool.allowed.contains(&purchase.selection.kind()) {
                return Err(BuildError::PurchaseNotAllowed);
            }
            let balance = balances
                .get_mut(&purchase.pool)
                .ok_or(BuildError::UnknownPointPool)?;
            if balance.spent >= balance.granted {
                return Err(BuildError::NoAdvancementPoint);
            }
            balance.spent += 1;
        }
        Ok(grants
            .iter()
            .map(|grant| balances.remove(&grant.pool).expect("validated grant"))
            .collect())
    }
    pub fn unspent_points(&self) -> u64 {
        // All public construction and editing paths validate the immutable build.
        (0..self.choices().len())
            .filter(|owner| {
                self.ledger().entries()[*owner]
                    .owner()
                    .is_none_or(|owner| owner.is_personal())
            })
            .flat_map(|owner| {
                self.point_balances(crate::HdIndex::from_usize(owner).expect("resident HD index"))
                    .expect("validated ledger")
            })
            .try_fold(0_u64, |sum, balance| {
                sum.checked_add(u64::from(balance.unspent()))
            })
            .expect("aggregate grants validated when compiling the build")
    }
    /// Validate explicit pool selection without mutation or randomness. This
    /// permanent projection is not an authorization to commit after state changes.
    pub fn preview_purchase(
        &self,
        owner: crate::HdIndex,
        pool: PointPoolId,
        selection: PurchaseSelection,
    ) -> Result<AdvancementPreview, BuildError> {
        selection.validate(self.ruleset())?;
        if let Some(expertise) = selection.expertise() {
            if self.derive()?.expertises.owns(expertise) {
                return Err(BuildError::DuplicateExpertise);
            }
        }
        let owner = owner.to_usize().map_err(BuildError::from)?;
        let (candidate, derived) = self.preview_intellect_advancement(|build| {
            if let Some(id) = selection.talent() {
                if build.ruleset().talent(id).is_none() {
                    return Err(BuildError::UnknownTalent);
                }
                if build
                    .choices()
                    .iter()
                    .flat_map(HdChoices::talents)
                    .any(|owned| owned == id)
                {
                    return Err(BuildError::DuplicateTalent);
                }
            }
            let ownership = build
                .ledger()
                .entries()
                .get(owner)
                .ok_or(BuildError::InvalidOwnership)?
                .owner()
                .cloned();
            let choice = build
                .advancement
                .choices
                .get_mut(owner)
                .ok_or(BuildError::InvalidOwnership)?;
            let purchase = choice.allocate_purchase(ownership.as_ref(), pool, selection)?;
            choice.purchases.push(purchase);
            Ok(())
        })?;
        Ok(AdvancementPreview { candidate, derived })
    }
    /// Commit precisely the same validation used by `preview_purchase`.
    pub fn purchase(
        &mut self,
        owner: crate::HdIndex,
        pool: PointPoolId,
        selection: PurchaseSelection,
    ) -> Result<(), BuildError> {
        *self = self.preview_purchase(owner, pool, selection)?.into_build();
        Ok(())
    }
    /// Deterministic convenience for callers which do not select a named pool.
    /// A future UI can present all balances and call `purchase` directly.
    fn purchase_from_available_pool(
        &mut self,
        owner: crate::HdIndex,
        selection: PurchaseSelection,
    ) -> Result<(), BuildError> {
        selection.validate(self.ruleset())?;
        if let Some(expertise) = selection.expertise() {
            if self.derive()?.expertises.owns(expertise) {
                return Err(BuildError::DuplicateExpertise);
            }
        }
        if let Some(id) = selection.talent() {
            if self.ruleset().talent(id).is_none() {
                return Err(BuildError::UnknownTalent);
            }
            if self
                .choices()
                .iter()
                .flat_map(HdChoices::talents)
                .any(|owned| owned == id)
            {
                return Err(BuildError::DuplicateTalent);
            }
        }
        let pool = self
            .point_balances(owner)?
            .into_iter()
            .find(|balance| {
                balance.unspent() > 0
                    && self
                        .ruleset()
                        .point_pool(&balance.pool)
                        .is_some_and(|pool| pool.allowed.contains(&selection.kind()))
            })
            .ok_or(BuildError::NoAdvancementPoint)?
            .pool;
        self.purchase(owner, pool, selection)
    }
    pub fn train(&mut self, owner: crate::HdIndex, skill: Skill) -> Result<(), BuildError> {
        self.purchase_from_available_pool(owner, PurchaseSelection::SkillRank(skill))
    }
    pub fn select_expertise(
        &mut self,
        owner: crate::HdIndex,
        expertise: ExpertiseId,
    ) -> Result<(), BuildError> {
        self.purchase_from_available_pool(owner, PurchaseSelection::Expertise(expertise))
    }
    pub fn available_expertises(&self) -> Result<Vec<ExpertiseId>, BuildError> {
        let owned = self.derive()?.expertises;
        Ok(self
            .ruleset()
            .document()
            .expertises
            .iter()
            .filter(|definition| !owned.owns(&definition.id))
            .map(|definition| definition.id.clone())
            .collect())
    }
    pub fn select_talent(
        &mut self,
        owner: crate::HdIndex,
        talent: TalentId,
    ) -> Result<(), BuildError> {
        self.purchase_from_available_pool(owner, PurchaseSelection::Talent(talent))
    }
    /// Classify the failed next ordinal at its ownership boundary. The four
    /// choice conflicts can follow ordinary retained edits to a valid frozen
    /// plan; they are not exceptions for a newly selected empty ordinal.
    pub(super) fn grow_hit_die(
        &mut self,
        source: HdSource,
        root_seed: u64,
    ) -> Result<GrowthBuildOutcome, BuildError> {
        let restores_authored = self.next_authored_restoration_source().is_some();
        match self.add_hit_die(source, root_seed) {
            Ok(()) => Ok(GrowthBuildOutcome::Applied),
            Err(
                BuildError::AttributeCap
                | BuildError::RankCap
                | BuildError::DuplicateTalent
                | BuildError::DuplicateExpertise
                | BuildError::TooManyOwnedExpertises
                | BuildError::TooManyOwnedTalents,
            ) if restores_authored => Ok(GrowthBuildOutcome::Obstructed),
            // Preserve current authored numeric/override obstructions. Ledger
            // provenance, operational capacity and invariants never enter here.
            Err(BuildError::DerivedLimit | BuildError::ConflictingOverride) => {
                Ok(GrowthBuildOutcome::Obstructed)
            }
            Err(error) => Err(error),
        }
    }

    /// Restores one authored ordinal, including its intended choices, without
    /// replacing surviving choices or borrowing combat randomness. `ChooseAgain`
    /// builds append an empty ordinal with `add_hit_die` and choose separately.
    pub fn restore_hit_die(&mut self, root_seed: u64) -> Result<(), BuildError> {
        if self.body_build.is_some() {
            let source = self
                .next_authored_restoration_source()
                .ok_or(BuildError::NoAuthoredRestoration)?;
            return self.add_personal_hit_die(source, root_seed);
        }
        let RestorationPolicy::Authored(plan) = self.restoration_policy() else {
            return Err(BuildError::NoAuthoredRestoration);
        };
        let planned = plan
            .get(self.choices().len())
            .ok_or(BuildError::NoAuthoredRestoration)?
            .clone();
        let mut candidate = self.clone();
        candidate
            .ledger
            .append_seeded(planned.source, root_seed)
            .map_err(BuildError::from)?;
        candidate.advancement.choices.push(planned.choices);
        // An intended choice can be dormant after another retained choice or
        // permanent template changed. Ownership is restored, never eligibility.
        let kind = super::derive::kind_for_templates(&candidate, &candidate.templates)?;
        candidate.bind_health_origins(&kind)?;
        let derived = candidate.derive()?;
        candidate.retain_intellect_acquisition(self, &derived)?;
        *self = candidate;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adding_and_restoring_reject_a_wrong_health_root_without_losing_its_origin() {
        let mut build = crate::test_creatures::build(crate::test_creatures::species())
            .instantiate_seeded(42)
            .unwrap();
        let before = build.clone();
        assert_eq!(
            build.add_hit_die(HdSource::Racial, 43),
            Err(BuildError::InvalidHealthRoot)
        );
        assert_eq!(build, before);
        build.add_hit_die(HdSource::Racial, 42).unwrap();
        let mut advancement = build.advancement().clone();
        advancement.restoration = RestorationPolicy::Authored(
            build
                .ledger()
                .entries()
                .iter()
                .zip(build.choices())
                .map(|(row, choices)| PlannedAdvancement {
                    source: row.source(),
                    choices: choices.clone(),
                })
                .collect(),
        );
        let mut build = CreatureBuild::from_recorded(
            build.content().clone(),
            build.species_id().clone(),
            build.initial_attributes(),
            build.ledger().clone(),
            build.binding(),
            advancement,
            vec![],
        )
        .unwrap();
        build.remove_latest().unwrap();
        let before = build.clone();
        assert_eq!(
            build.restore_hit_die(43),
            Err(BuildError::InvalidHealthRoot)
        );
        assert_eq!(build, before);
        build.restore_hit_die(42).unwrap();
        assert_eq!(build.ledger().health_root_seed(), Some(42));
    }
    #[test]
    fn explicit_starting_ranks_do_not_spend_the_hit_dies_point() {
        let build = crate::test_creatures::with_starting_skills(
            crate::test_creatures::build(crate::test_creatures::species()),
            [Skill::Athletics, Skill::Lore],
        );
        assert_eq!(build.unspent_points(), 1);
        assert_eq!(build.derive().unwrap().skills.get(Skill::Athletics), 1);
        assert_eq!(build.derive().unwrap().skills.get(Skill::Lore), 1);
        assert!(build.choices()[0].purchases.is_empty());
    }
    #[test]
    fn duplicate_starting_skills_fail_at_creation() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        assert_eq!(
            CreatureBuild::new(
                build.content().clone(),
                build.species_id().clone(),
                build.ledger().clone(),
                build.binding(),
                [Skill::Athletics, Skill::Athletics],
            ),
            Err(BuildError::InvalidStartingSkills)
        );
    }
    #[test]
    fn purchase_preview_and_commit_each_derive_only_once() {
        let mut build = crate::test_creatures::build(crate::test_creatures::species());
        let pool = PointPoolId::new("advancement").unwrap();
        let before = super::super::derive::fold_count();
        let preview = build
            .preview_purchase(
                crate::HdIndex(0),
                pool.clone(),
                PurchaseSelection::SkillRank(Skill::Athletics),
            )
            .unwrap();
        assert_eq!(super::super::derive::fold_count() - before, 1);
        let before = super::super::derive::fold_count();
        build
            .purchase(
                crate::HdIndex(0),
                pool,
                PurchaseSelection::SkillRank(Skill::Athletics),
            )
            .unwrap();
        assert_eq!(super::super::derive::fold_count() - before, 1);
        assert_eq!(preview.into_build(), build);
    }
    #[test]
    fn owned_purchase_ids_survive_removal_and_reenrollment_without_reuse() {
        use super::super::composition::{HdOwner, PersonalHdId};
        use std::num::NonZeroU64;
        let owner = HdOwner::Personal {
            id: PersonalHdId(NonZeroU64::new(7).unwrap()),
        };
        let pool = PointPoolId::new("advancement").unwrap();
        let selection = PurchaseSelection::SkillRank(Skill::Athletics);
        let mut choice = HdChoices {
            purchase_highwater: 0,
            purchases: vec![AdvancementPurchase {
                id: None,
                pool: pool.clone(),
                selection: selection.clone(),
            }],
            attribute: None,
        };
        choice.enroll_owner(&owner).unwrap();
        let first = choice.purchases[0].id.clone().unwrap();
        assert_eq!(first.serial.get(), 1);
        let retained = choice.clone();
        choice.enroll_owner(&owner).unwrap();
        assert_eq!(choice, retained);
        choice.purchases.clear();
        choice.enroll_owner(&owner).unwrap();
        let second = choice
            .allocate_purchase(Some(&owner), pool, selection)
            .unwrap();
        assert_eq!(second.id.as_ref().unwrap().serial.get(), 2);
        assert_ne!(second.id, Some(first));
        choice.purchases.push(second);
        assert_eq!(choice.purchase_highwater, 2);
        choice.validate_purchase_ids(Some(&owner)).unwrap();
        let wrong = HdOwner::Personal {
            id: PersonalHdId(NonZeroU64::new(8).unwrap()),
        };
        let before = choice.clone();
        assert_eq!(
            choice.enroll_owner(&wrong),
            Err(BuildError::InvalidOwnership)
        );
        assert_eq!(choice, before);
        choice.purchases.push(choice.purchases[0].clone());
        assert_eq!(
            choice.validate_purchase_ids(Some(&owner)),
            Err(BuildError::InvalidOwnership)
        );
    }
    #[test]
    fn package_purchase_budget_is_locked_and_ordinary_purchases_remain_unowned() {
        use super::super::composition::HdOwner;
        let owner = HdOwner::Body {
            form: crate::content_ids::BodyFormId::new("human").unwrap(),
            ordinal: 0,
        };
        let pool = PointPoolId::new("advancement").unwrap();
        let selection = PurchaseSelection::SkillRank(Skill::Athletics);
        let mut choice = HdChoices::default();
        let before = choice.clone();
        assert_eq!(
            choice.allocate_purchase(Some(&owner), pool.clone(), selection.clone()),
            Err(BuildError::InvalidOwnership)
        );
        assert_eq!(choice, before);
        let purchase = choice.allocate_purchase(None, pool, selection).unwrap();
        assert!(purchase.id.is_none());
        assert_eq!(choice.purchase_highwater, 0);
    }
}
