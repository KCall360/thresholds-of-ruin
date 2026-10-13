//! Owned advancement, independent of transient effects and control ownership.
use super::{BuildError, CreatureBuild};
use crate::attributes::{Attribute, Skill};
use crate::content_ids::{PointPoolId, TalentId};
use crate::progression::HdSource;
use crate::rules::PurchaseKind;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PurchaseSelection {
    SkillRank(Skill),
    Talent(TalentId),
}
impl PurchaseSelection {
    pub fn kind(&self) -> PurchaseKind {
        match self {
            Self::SkillRank(_) => PurchaseKind::SkillRank,
            Self::Talent(_) => PurchaseKind::Talent,
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
            .filter_map(|purchase| match &purchase.selection {
                PurchaseSelection::Talent(id) => Some(id),
                PurchaseSelection::SkillRank(_) => None,
            })
    }
    pub fn skills(&self) -> impl Iterator<Item = Skill> + '_ {
        self.purchases
            .iter()
            .filter_map(|purchase| match purchase.selection {
                PurchaseSelection::SkillRank(skill) => Some(skill),
                PurchaseSelection::Talent(_) => None,
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
            if let PurchaseSelection::Talent(id) = &selection {
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
        if let PurchaseSelection::Talent(id) = &selection {
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
            .map_err(|_| BuildError::TooManyHitDice)?;
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
