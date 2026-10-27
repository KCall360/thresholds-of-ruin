//! Stable ownership for replaceable bodies and retained personal advancement.
//! Effective HD positions are projections, never funding or Health identities.
use super::{AdvancementPurchase, BuildError, PointPoolBalance, PurchaseSelection};
use crate::attributes::Attribute;
use crate::content_ids::{BodyFormId, PointPoolId, TalentId};
use crate::progression::{HdSource, HitDie};
use crate::rules::Ruleset;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

/// Allocated monotonically, never from the current effective row count.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct PersonalHdId(pub NonZeroU64);
/// Ownership is independent of source: an earned racial HD is personal too.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HdOwner {
    Body { form: BodyFormId, ordinal: u16 },
    Personal { id: PersonalHdId },
}
impl HdOwner {
    pub fn maximizes_health(&self) -> bool {
        matches!(self, Self::Body { ordinal: 0, .. })
    }
    pub fn is_personal(&self) -> bool {
        matches!(self, Self::Personal { .. })
    }
}
/// Serials are never reused when a purchase is replaced or removed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurchaseId {
    pub owner: HdOwner,
    pub serial: NonZeroU64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedHitDie {
    owner: HdOwner,
    hit_die: HitDie,
    purchases: Vec<AdvancementPurchase>,
    attribute: Option<Attribute>,
}
impl OwnedHitDie {
    pub fn new(
        owner: HdOwner,
        hit_die: HitDie,
        purchases: Vec<AdvancementPurchase>,
        attribute: Option<Attribute>,
        rules: &Ruleset,
    ) -> Result<Self, BuildError> {
        let hit_die = hit_die
            .with_owner(owner.clone())
            .map_err(BuildError::from)?;
        let row = Self {
            owner,
            hit_die,
            purchases,
            attribute,
        };
        if !row.owner.is_personal()
            && (row.attribute.is_some() || !matches!(row.hit_die.source(), HdSource::Racial))
        {
            return Err(BuildError::InvalidOwnership);
        }
        row.point_balances(rules)?;
        Ok(row)
    }
    pub fn owner(&self) -> &HdOwner {
        &self.owner
    }
    pub fn hit_die(&self) -> &HitDie {
        &self.hit_die
    }
    pub fn purchases(&self) -> &[AdvancementPurchase] {
        &self.purchases
    }
    pub fn attribute(&self) -> Option<Attribute> {
        self.attribute
    }
    /// Funding validation does not erase owned purchases whose current-body
    /// prerequisites fail. Eligibility belongs to the effective derivation.
    pub fn point_balances(&self, rules: &Ruleset) -> Result<Vec<PointPoolBalance>, BuildError> {
        if self.purchases.len() > crate::limits::POINTS_PER_HIT_DIE {
            return Err(BuildError::NoAdvancementPoint);
        }
        let grants = rules
            .point_grants(&self.hit_die.source())
            .ok_or(BuildError::UnknownClass)?;
        let mut balances: BTreeMap<PointPoolId, PointPoolBalance> = grants
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
        let mut ids = BTreeSet::new();
        for purchase in &self.purchases {
            let id = purchase.id.as_ref().ok_or(BuildError::InvalidOwnership)?;
            if id.owner != self.owner || !ids.insert(id) {
                return Err(BuildError::InvalidOwnership);
            }
            purchase.selection.validate(rules)?;
            let pool = rules
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
}
/// Read-only projection. The source rows remain with their respective owners.
#[derive(Debug)]
pub struct Composition<'a> {
    body: &'a [OwnedHitDie],
    personal: &'a [OwnedHitDie],
}
impl<'a> Composition<'a> {
    pub fn new(body: &'a [OwnedHitDie], personal: &'a [OwnedHitDie]) -> Result<Self, BuildError> {
        if body.is_empty() || body.len().saturating_add(personal.len()) > crate::limits::HIT_DICE {
            return Err(BuildError::InvalidOwnership);
        }
        let HdOwner::Body { form, .. } = body[0].owner() else {
            return Err(BuildError::InvalidOwnership);
        };
        for (index, row) in body.iter().enumerate() {
            if !matches!(row.owner(), HdOwner::Body { form: actual, ordinal } if actual == form && usize::from(*ordinal) == index)
            {
                return Err(BuildError::InvalidOwnership);
            }
        }
        let mut previous = None;
        for row in personal {
            let HdOwner::Personal { id } = row.owner() else {
                return Err(BuildError::InvalidOwnership);
            };
            if previous.is_some_and(|previous| previous >= *id) {
                return Err(BuildError::InvalidOwnership);
            }
            previous = Some(*id);
        }
        Ok(Self { body, personal })
    }
    pub fn rows(&self) -> impl Iterator<Item = &'a OwnedHitDie> {
        self.body.iter().chain(self.personal)
    }
    pub fn personal_count(&self) -> usize {
        self.personal.len()
    }
    pub fn combat_count(&self) -> usize {
        self.body.len() + self.personal.len()
    }
    pub fn personal_unspent_points(&self, rules: &Ruleset) -> Result<u64, BuildError> {
        self.personal.iter().try_fold(0u64, |total, row| {
            row.point_balances(rules)?
                .iter()
                .try_fold(total, |total, balance| {
                    total
                        .checked_add(u64::from(balance.unspent()))
                        .ok_or(BuildError::Arithmetic)
                })
        })
    }
    /// Identical roots grant once, retaining all funding provenance. Conflicting
    /// fixed variants are unsupported rather than selected by iteration order.
    pub fn talent_grants(&self) -> Result<BTreeMap<TalentId, &'a PurchaseSelection>, BuildError> {
        let mut grants: BTreeMap<TalentId, &'a PurchaseSelection> = BTreeMap::new();
        for purchase in self.rows().flat_map(|row| &row.purchases) {
            let selection = &purchase.selection;
            let Some(talent) = selection.talent() else {
                continue;
            };
            if let Some(previous) = grants.get(talent) {
                if *previous != selection {
                    return Err(BuildError::InvalidTalentForm);
                }
            } else {
                grants.insert(talent.clone(), selection);
            }
        }
        Ok(grants)
    }
    pub fn next_drain_owner(&self) -> &HdOwner {
        self.personal
            .last()
            .or_else(|| self.body.last())
            .expect("nonempty body")
            .owner()
    }
}
#[cfg(test)]
mod tests;
