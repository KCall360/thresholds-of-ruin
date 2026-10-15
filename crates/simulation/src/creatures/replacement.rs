//! Atomic ownership replacement; actor timing and supply authority live elsewhere.
use super::{AdvancementPurchase, BuildError, CreatureBuild, DerivedCreature};
use crate::content_ids::{PointPoolId, TalentId};
use crate::rules::Requirement;
use std::collections::{BTreeMap, BTreeSet};

/// A purchase's zero-based position in the unchanged source build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PurchaseLocation {
    pub owner: crate::HdIndex,
    pub index: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedPurchase {
    pub location: PurchaseLocation,
    pub purchase: AdvancementPurchase,
}
/// Gross refunds for removed purchases. An optional replacement spends one point
/// separately; the preview's final balances disclose the net result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointRefund {
    pub owner: crate::HdIndex,
    pub pool: PointPoolId,
    pub points: u16,
}
/// A validated transaction tied to the entire immutable source build. Its private
/// fields prevent callers from substituting an unvalidated candidate or refund.
/// Actor revisions, safety, time and supply payment remain action-layer concerns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdvancementReplacementPreview {
    base: CreatureBuild,
    candidate: CreatureBuild,
    derived: DerivedCreature,
    target: PurchaseLocation,
    replacement: Option<AdvancementPurchase>,
    removed: Vec<RemovedPurchase>,
    refunds: Vec<PointRefund>,
}
impl AdvancementReplacementPreview {
    pub fn build(&self) -> &CreatureBuild {
        &self.candidate
    }
    pub fn derived(&self) -> &DerivedCreature {
        &self.derived
    }
    pub fn target(&self) -> PurchaseLocation {
        self.target
    }
    pub fn replacement(&self) -> Option<&AdvancementPurchase> {
        self.replacement.as_ref()
    }
    pub fn removed(&self) -> &[RemovedPurchase] {
        &self.removed
    }
    pub fn refunds(&self) -> &[PointRefund] {
        &self.refunds
    }
}

impl CreatureBuild {
    /// Preview one explicit replacement/refund and its owned-parent subtree.
    /// Dormant owned alternatives preserve OR branches. Rank/HD/attribute gates
    /// affect activation only and never silently refund a supported purchase.
    pub fn preview_replacement(
        &self,
        target: PurchaseLocation,
        replacement: Option<AdvancementPurchase>,
    ) -> Result<AdvancementReplacementPreview, BuildError> {
        let owner = target.owner.to_usize().map_err(BuildError::from)?;
        let original = self
            .choices()
            .get(owner)
            .and_then(|choices| choices.purchases.get(target.index))
            .ok_or(BuildError::InvalidOwnership)?
            .clone();
        if replacement.as_ref() == Some(&original) {
            return Err(BuildError::NoAdvancementChange);
        }
        let previously_owned: BTreeSet<_> = self
            .choices()
            .iter()
            .flat_map(|choice| choice.talents().cloned())
            .collect();
        let mut removed = vec![RemovedPurchase {
            location: target,
            purchase: original.clone(),
        }];
        let (candidate, derived) = self.preview_edit(|build| {
            let purchases = &mut build.advancement.choices[owner].purchases;
            if let Some(purchase) = replacement.clone() {
                purchases[target.index] = purchase;
            } else {
                purchases.remove(target.index);
            }
            let mut owned = BTreeSet::new();
            for talent in build.choices().iter().flat_map(|choice| choice.talents()) {
                if build.ruleset().talent(talent).is_none() {
                    return Err(BuildError::UnknownTalent);
                }
                if !owned.insert(talent.clone()) {
                    return Err(BuildError::DuplicateTalent);
                }
            }
            let mut lost = BTreeSet::new();
            if let Some(id) = original.selection.talent() {
                if !owned.contains(id) {
                    lost.insert(id.clone());
                }
            }
            // Validation gives a DAG including references nested in Any. One
            // topological sweep reaches the ownership-removal fixed point:
            // every alternative is decided before its dependent. The causal
            // test preserves unrelated existing dormant/orphaned purchases.
            for id in build.ruleset().talent_order() {
                if !owned.contains(id) || !previously_owned.contains(id) {
                    continue;
                }
                let definition = build.ruleset().talent(id).expect("validated talent order");
                if definition
                    .prerequisites
                    .iter()
                    .any(|parent| lost.contains(parent))
                    || lost_parent_route(&definition.requirement, &owned, &lost)
                {
                    owned.remove(id);
                    lost.insert(id.clone());
                }
            }
            for (owner, choice) in self.choices().iter().enumerate() {
                for (index, purchase) in choice.purchases.iter().enumerate() {
                    let location = PurchaseLocation {
                        owner: crate::HdIndex::from_usize(owner).map_err(BuildError::from)?,
                        index,
                    };
                    if location != target
                        && purchase
                            .selection
                            .talent()
                            .is_some_and(|id| lost.contains(id))
                    {
                        removed.push(RemovedPurchase {
                            location,
                            purchase: purchase.clone(),
                        });
                    }
                }
            }
            for choice in &mut build.advancement.choices {
                choice.purchases.retain(|purchase| {
                    !purchase
                        .selection
                        .talent()
                        .is_some_and(|id| lost.contains(id))
                });
            }
            Ok(())
        })?;
        removed.sort_by_key(|entry| entry.location);
        let mut refunds = BTreeMap::<(crate::HdIndex, PointPoolId), u16>::new();
        for entry in &removed {
            // A validated HD owns at most 256 purchases, so each sum fits u16.
            *refunds
                .entry((entry.location.owner, entry.purchase.pool.clone()))
                .or_default() += 1;
        }
        Ok(AdvancementReplacementPreview {
            base: self.clone(),
            candidate,
            derived,
            target,
            replacement,
            removed,
            refunds: refunds
                .into_iter()
                .map(|((owner, pool), points)| PointRefund {
                    owner,
                    pool,
                    points,
                })
                .collect(),
        })
    }
    /// Apply exactly the validated preview, or leave the build untouched if any
    /// source-build fact has changed. No random stream or runtime resource is read.
    pub fn commit_replacement(
        &mut self,
        preview: AdvancementReplacementPreview,
    ) -> Result<(), BuildError> {
        if *self != preview.base {
            return Err(BuildError::StaleAdvancementPreview);
        }
        *self = preview.candidate;
        Ok(())
    }
}

fn owned_parent_routes(requirement: &Requirement, owned: &BTreeSet<TalentId>) -> bool {
    match requirement {
        Requirement::All { requirements } => requirements
            .iter()
            .all(|value| owned_parent_routes(value, owned)),
        Requirement::Any { requirements } => requirements
            .iter()
            .any(|value| owned_parent_routes(value, owned)),
        Requirement::OwnedTalent { talent } => owned.contains(talent),
        Requirement::TotalHd { .. }
        | Requirement::RacialHd { .. }
        | Requirement::ClassLevel { .. }
        | Requirement::TypeIn { .. }
        | Requirement::SubtypeIn { .. }
        | Requirement::SpeciesFlag { .. }
        | Requirement::SkillRank { .. }
        | Requirement::AttributeMinimum { .. }
        | Requirement::Flag { .. } => true,
    }
}
fn lost_parent_route(
    requirement: &Requirement,
    owned: &BTreeSet<TalentId>,
    lost: &BTreeSet<TalentId>,
) -> bool {
    match requirement {
        Requirement::All { requirements } => requirements
            .iter()
            .any(|value| lost_parent_route(value, owned, lost)),
        Requirement::Any { requirements } => {
            !owned_parent_routes(requirement, owned)
                && requirements
                    .iter()
                    .any(|value| lost_parent_route(value, owned, lost))
        }
        Requirement::OwnedTalent { talent } => lost.contains(talent),
        Requirement::TotalHd { .. }
        | Requirement::RacialHd { .. }
        | Requirement::ClassLevel { .. }
        | Requirement::TypeIn { .. }
        | Requirement::SubtypeIn { .. }
        | Requirement::SpeciesFlag { .. }
        | Requirement::SkillRank { .. }
        | Requirement::AttributeMinimum { .. }
        | Requirement::Flag { .. } => false,
    }
}
