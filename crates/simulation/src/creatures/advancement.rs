//! Owned advancement, independent of transient effects and control ownership.
use super::{BuildError, CreatureBuild};
use crate::attributes::{Attribute, Skill};
use crate::content_ids::{FormId, PointPoolId, TalentId};
use crate::progression::HdSource;
use crate::rules::PurchaseKind;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurchaseSelection {
    SkillRank(Skill),
    Talent(TalentId),
    TalentForm { talent: TalentId, form: FormId },
}
impl PurchaseSelection {
    pub fn kind(&self) -> PurchaseKind {
        match self {
            Self::SkillRank(_) => PurchaseKind::SkillRank,
            Self::Talent(_) | Self::TalentForm { .. } => PurchaseKind::Talent,
        }
    }
    pub fn talent(&self) -> Option<&TalentId> {
        match self {
            Self::SkillRank(_) => None,
            Self::Talent(talent) | Self::TalentForm { talent, .. } => Some(talent),
        }
    }
    pub fn form(&self) -> Option<&FormId> {
        match self {
            Self::TalentForm { form, .. } => Some(form),
            Self::SkillRank(_) | Self::Talent(_) => None,
        }
    }
    /// Validate the owned operand even when its talent is currently dormant.
    /// Eligibility is separate; a valid choice never changes as gates change.
    pub(crate) fn validate(&self, rules: &crate::rules::Ruleset) -> Result<(), BuildError> {
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
    pub pool: PointPoolId,
    pub selection: PurchaseSelection,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HdChoices {
    pub purchases: Vec<AdvancementPurchase>,
    pub attribute: Option<Attribute>,
}
impl HdChoices {
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
                PurchaseSelection::Talent(_) | PurchaseSelection::TalentForm { .. } => None,
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
    pub fn restoration_policy(&self) -> &RestorationPolicy {
        &self.advancement.restoration
    }

    /// The returned declaration-ordered ledger reconciles every purchase exactly once.
    pub fn point_balances(&self, owner: usize) -> Result<Vec<PointPoolBalance>, BuildError> {
        let entry = self
            .ledger
            .entries()
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        let choice = self
            .choices()
            .get(owner)
            .ok_or(BuildError::InvalidOwnership)?;
        if choice.purchases.len() > crate::limits::POINTS_PER_HIT_DIE {
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
        for purchase in &choice.purchases {
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
    pub fn unspent_points(&self) -> u32 {
        // All public construction and editing paths validate the immutable build.
        (0..self.choices().len())
            .flat_map(|owner| self.point_balances(owner).expect("validated ledger"))
            .map(|balance| u32::from(balance.unspent()))
            .sum()
    }
    /// Validate explicit pool selection without mutation or randomness. This
    /// permanent projection is not an authorization to commit after state changes.
    pub fn preview_purchase(
        &self,
        owner: usize,
        pool: PointPoolId,
        selection: PurchaseSelection,
    ) -> Result<AdvancementPreview, BuildError> {
        let (candidate, derived) = self.preview_edit(|build| {
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
            build
                .advancement
                .choices
                .get_mut(owner)
                .ok_or(BuildError::InvalidOwnership)?
                .purchases
                .push(AdvancementPurchase { pool, selection });
            Ok(())
        })?;
        Ok(AdvancementPreview { candidate, derived })
    }
    /// Commit precisely the same validation used by `preview_purchase`.
    pub fn purchase(
        &mut self,
        owner: usize,
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
        owner: usize,
        selection: PurchaseSelection,
    ) -> Result<(), BuildError> {
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
    pub fn train(&mut self, owner: usize, skill: Skill) -> Result<(), BuildError> {
        self.purchase_from_available_pool(owner, PurchaseSelection::SkillRank(skill))
    }
    pub fn select_talent(&mut self, owner: usize, talent: TalentId) -> Result<(), BuildError> {
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
        let restores_authored = matches!(self.restoration_policy(),
            RestorationPolicy::Authored(plan) if plan.get(self.choices().len()).is_some());
        match self.add_hit_die(source, root_seed) {
            Ok(()) => Ok(GrowthBuildOutcome::Applied),
            Err(
                BuildError::AttributeCap
                | BuildError::RankCap
                | BuildError::DuplicateTalent
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
        candidate.derive()?;
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
                0,
                pool.clone(),
                PurchaseSelection::SkillRank(Skill::Athletics),
            )
            .unwrap();
        assert_eq!(super::super::derive::fold_count() - before, 1);
        let before = super::super::derive::fold_count();
        build
            .purchase(0, pool, PurchaseSelection::SkillRank(Skill::Athletics))
            .unwrap();
        assert_eq!(super::super::derive::fold_count() - before, 1);
        assert_eq!(preview.into_build(), build);
    }
}
