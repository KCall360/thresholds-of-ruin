//! Canonical attack damage and nested protection groups. This module returns
//! private resolution facts; disclosure and death remain actor/engine concerns.

pub(crate) mod record;

use crate::attributes::{Attributes, CheckOutcome, SkillCheck, SkillRanks};
use crate::combat::DamageType;
use crate::dice::{DicePool, Edge, PoolRoll};
use crate::grants::{Descriptor, Grant, Selector};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageError {
    InvalidSpec,
    NonNestedDescriptors,
    InvalidProtection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DamageKey {
    pub category: DamageType,
    pub descriptor: Option<Descriptor>,
    pub sides: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DamageAmount {
    Fixed(u32),
    Rolled(DicePool),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DamageComponent {
    category: DamageType,
    descriptor: Option<Descriptor>,
    amount: DamageAmount,
}

impl DamageComponent {
    pub fn amount(&self) -> DamageAmount {
        self.amount
    }

    pub fn fixed(category: DamageType, descriptor: Option<Descriptor>, amount: u32) -> Self {
        Self {
            category,
            descriptor,
            amount: DamageAmount::Fixed(amount),
        }
    }

    pub fn rolled(category: DamageType, descriptor: Option<Descriptor>, pool: DicePool) -> Self {
        Self {
            category,
            descriptor,
            amount: DamageAmount::Rolled(pool),
        }
    }

    pub fn key(&self) -> DamageKey {
        DamageKey {
            category: self.category,
            descriptor: self.descriptor,
            sides: match self.amount {
                DamageAmount::Fixed(_) => None,
                DamageAmount::Rolled(pool) => Some(pool.sides()),
            },
        }
    }

    fn maximum(&self) -> u32 {
        match self.amount {
            DamageAmount::Fixed(amount) => amount,
            DamageAmount::Rolled(pool) => pool.maximum(),
        }
    }

    fn merge(&mut self, other: Self) -> Result<(), DamageError> {
        self.amount = match (self.amount, other.amount) {
            (DamageAmount::Fixed(a), DamageAmount::Fixed(b)) => {
                DamageAmount::Fixed(a.checked_add(b).ok_or(DamageError::InvalidSpec)?)
            }
            (DamageAmount::Rolled(a), DamageAmount::Rolled(b)) => {
                DamageAmount::Rolled(a.merged(b).map_err(|_| DamageError::InvalidSpec)?)
            }
            _ => return Err(DamageError::InvalidSpec),
        };
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DamageSpec {
    components: Vec<DamageComponent>,
    primary: Option<DamageKey>,
}

impl DamageSpec {
    /// Homogeneous pools with the same category and descriptor merge before
    /// rolling. A declared primary key comes first; all other keys sort canonically.
    /// Descriptor groups must sit wholly within a single damage category.
    pub fn new(
        components: Vec<DamageComponent>,
        primary: Option<DamageKey>,
    ) -> Result<Self, DamageError> {
        if components.is_empty() || components.len() > 32 {
            return Err(DamageError::InvalidSpec);
        }
        let mut canonical: BTreeMap<DamageKey, DamageComponent> = BTreeMap::new();
        let mut descriptor_categories = BTreeMap::new();
        for component in components {
            if component.maximum() > 1_000_000 {
                return Err(DamageError::InvalidSpec);
            }
            if let Some(descriptor) = component.descriptor {
                if descriptor_categories
                    .insert(descriptor, component.category)
                    .is_some_and(|previous| previous != component.category)
                {
                    return Err(DamageError::NonNestedDescriptors);
                }
            }
            match canonical.entry(component.key()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(component);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().merge(component)?
                }
            }
        }
        if primary.is_some_and(|key| !canonical.contains_key(&key)) {
            return Err(DamageError::InvalidSpec);
        }
        let mut components: Vec<_> = canonical.into_values().collect();
        if components
            .iter()
            .map(|component| u64::from(component.maximum()))
            .sum::<u64>()
            > 1_000_000
        {
            return Err(DamageError::InvalidSpec);
        }
        components.sort_by_key(|component| (Some(component.key()) != primary, component.key()));
        Ok(Self {
            components,
            primary,
        })
    }

    pub fn components(&self) -> &[DamageComponent] {
        &self.components
    }
    pub fn primary(&self) -> Option<DamageKey> {
        self.primary
    }

    /// Apply melee modifiers to the declared primary. Power Strike affects the
    /// first Impact component (the primary if Impact), or adds a fixed component.
    /// Combine signed/positive changes before the amount's final zero clamp.
    pub fn with_melee_modifiers(
        &self,
        extra_dice: u16,
        flat_bonus: i32,
        impact_bonus: u32,
    ) -> Result<Self, DamageError> {
        if extra_dice > 64
            || !(-1_000_000..=1_000_000).contains(&flat_bonus)
            || impact_bonus > 1_000_000
        {
            return Err(DamageError::InvalidSpec);
        }
        let key = self.primary.ok_or(DamageError::InvalidSpec)?;
        let mut components = self.components.clone();
        let primary = components
            .iter_mut()
            .find(|component| component.key() == key)
            .ok_or(DamageError::InvalidSpec)?;
        let primary_impact = key.category == DamageType::Impact;
        let flat = flat_bonus
            + if primary_impact {
                impact_bonus as i32
            } else {
                0
            };
        primary.amount = match primary.amount {
            DamageAmount::Fixed(amount) => DamageAmount::Fixed(
                (i64::from(amount) + i64::from(flat))
                    .max(0)
                    .try_into()
                    .map_err(|_| DamageError::InvalidSpec)?,
            ),
            DamageAmount::Rolled(pool) => DamageAmount::Rolled(
                pool.augmented(extra_dice, flat)
                    .map_err(|_| DamageError::InvalidSpec)?,
            ),
        };
        let result = Self::new(components, self.primary)?;
        if primary_impact || impact_bonus == 0 {
            Ok(result)
        } else {
            result.with_category_bonus(DamageType::Impact, impact_bonus)
        }
    }

    /// Add to the primary pool of a category without changing bundle order.
    pub fn with_category_bonus(
        &self,
        category: DamageType,
        bonus: u32,
    ) -> Result<Self, DamageError> {
        if bonus > 1_000_000 {
            return Err(DamageError::InvalidSpec);
        }
        if bonus == 0 {
            return Ok(self.clone());
        }
        let mut components = self.components.clone();
        // Canonical order puts the declared primary first. Preserve its key
        // and descriptor; augmenting a pool adds no dice or edge allocations.
        if let Some(component) = components.iter_mut().find(|c| c.category == category) {
            component.amount = match component.amount {
                DamageAmount::Fixed(amount) => {
                    DamageAmount::Fixed(amount.checked_add(bonus).ok_or(DamageError::InvalidSpec)?)
                }
                DamageAmount::Rolled(pool) => DamageAmount::Rolled(
                    pool.augmented(0, bonus as i32)
                        .map_err(|_| DamageError::InvalidSpec)?,
                ),
            };
        } else {
            components.push(DamageComponent::fixed(category, None, bonus));
        }
        Self::new(components, self.primary)
    }

    pub fn resolve(&self, state: &mut u64, edge: Edge, protection: &Protection) -> DamageOutcome {
        self.resolve_with_diagnostics(state, edge, protection, &mut ())
    }

    pub fn resolve_with_diagnostics(
        &self,
        state: &mut u64,
        mut edge: Edge,
        protection: &Protection,
        observer: &mut impl crate::resolution_diagnostics::ResolutionObserver,
    ) -> DamageOutcome {
        use crate::resolution_diagnostics::{ComponentDiagnostic, ResolutionStep};
        observer.record(ResolutionStep::DamageStarted {
            net_edge: edge.balance(),
        });
        let mut components = Vec::with_capacity(self.components.len());
        let mut groups: BTreeMap<(DamageType, Option<Descriptor>), u32> = BTreeMap::new();
        let mut raw = 0;
        for component in &self.components {
            let rng_before = *state;
            let (amount, pool, expression, allocated) = match component.amount {
                DamageAmount::Fixed(amount) => (amount, None, None, Edge::default()),
                DamageAmount::Rolled(pool) => {
                    let allocated = edge.take(pool.count());
                    let rolled = pool.roll(state, allocated);
                    (rolled.total, Some(rolled), Some(pool), allocated)
                }
            };
            raw += amount;
            let category_immune = protection.has_immunity(Selector::Category(component.category));
            let descriptor_immune = component
                .descriptor
                .is_some_and(|value| protection.has_immunity(Selector::Descriptor(value)));
            let protected = if category_immune || descriptor_immune {
                0
            } else {
                amount
            };
            *groups
                .entry((component.category, component.descriptor))
                .or_default() += protected;
            components.push(ComponentRoll {
                key: component.key(),
                raw: amount,
                pool,
            });
            observer.record(ResolutionStep::Component(ComponentDiagnostic {
                component: components.last().expect("component just pushed"),
                expression,
                edge: allocated.balance(),
                rng_before,
                rng_after: *state,
                category_immune,
                descriptor_immune,
                after_immunity: protected,
            }));
        }
        let after_immunity = groups.values().sum();
        for ((category, descriptor), amount) in &mut groups {
            if let Some(descriptor) = descriptor {
                let selector = Selector::Descriptor(*descriptor);
                let before = *amount;
                let capacity = protection.reduction(selector);
                *amount = amount.saturating_sub(capacity);
                observer.record(ResolutionStep::Reduction {
                    selector,
                    category: *category,
                    before,
                    capacity,
                    after: *amount,
                });
            }
        }
        let after_descriptors = groups.values().sum();
        let mut by_category: BTreeMap<DamageType, u32> = BTreeMap::new();
        for ((category, _), amount) in groups {
            *by_category.entry(category).or_default() += amount;
        }
        for (&category, amount) in &mut by_category {
            let selector = Selector::Category(category);
            let before = *amount;
            let capacity = protection.reduction(selector);
            *amount = amount.saturating_sub(capacity);
            observer.record(ResolutionStep::Reduction {
                selector,
                category,
                before,
                capacity,
                after: *amount,
            });
        }
        let total = by_category.values().sum();
        observer.record(ResolutionStep::DamageFinished {
            raw,
            after_immunity,
            after_descriptors,
            total,
            unused_edge: edge.balance(),
        });
        DamageOutcome {
            components,
            raw,
            after_immunity,
            after_descriptors,
            by_category,
            total,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Protection {
    immunities: BTreeSet<Selector>,
    reductions: BTreeMap<Selector, u32>,
}

impl Protection {
    /// Combine exact selectors across independent grants. Other build benefits
    /// are irrelevant here; their source attribution remains in build derivation.
    pub fn from_grants(grants: impl IntoIterator<Item = Grant>) -> Result<Self, DamageError> {
        let mut protection = Self::default();
        for grant in grants {
            match grant {
                Grant::Immunity(selector) => {
                    protection.immunities.insert(selector);
                }
                Grant::Reduction(selector, amount) => {
                    let total = protection.reductions.entry(selector).or_default();
                    *total = total
                        .checked_add(amount)
                        .filter(|value| *value <= 1_000_000)
                        .ok_or(DamageError::InvalidProtection)?;
                }
                _ => {}
            }
        }
        Ok(protection)
    }

    pub fn immune(&self, category: DamageType, descriptor: Option<Descriptor>) -> bool {
        self.has_immunity(Selector::Category(category))
            || descriptor
                .is_some_and(|descriptor| self.has_immunity(Selector::Descriptor(descriptor)))
    }

    pub fn has_immunity(&self, selector: Selector) -> bool {
        self.immunities.contains(&selector)
    }

    pub fn reduction(&self, selector: Selector) -> u32 {
        self.reductions.get(&selector).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRoll {
    pub key: DamageKey,
    pub raw: u32,
    pub pool: Option<PoolRoll>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageOutcome {
    pub components: Vec<ComponentRoll>,
    pub raw: u32,
    pub after_immunity: u32,
    pub after_descriptors: u32,
    pub by_category: BTreeMap<DamageType, u32>,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackCheck {
    pub check: SkillCheck,
    pub attributes: Attributes,
    pub skills: SkillRanks,
}

impl AttackCheck {
    pub fn resolve(
        self,
        state: &mut u64,
        edge: Edge,
        damage: &DamageSpec,
        protection: &Protection,
    ) -> AttackOutcome {
        self.resolve_with_diagnostics(state, edge, damage, protection, &mut ())
    }

    pub fn resolve_with_diagnostics(
        self,
        state: &mut u64,
        mut edge: Edge,
        damage: &DamageSpec,
        protection: &Protection,
        observer: &mut impl crate::resolution_diagnostics::ResolutionObserver,
    ) -> AttackOutcome {
        use crate::resolution_diagnostics::ResolutionStep;
        observer.record(ResolutionStep::AttackStarted {
            net_edge: edge.balance(),
        });
        let check = self.check.resolve_with_diagnostics(
            state,
            self.attributes,
            self.skills,
            edge.take(1),
            observer,
        );
        let damage = if check.success {
            Some(damage.resolve_with_diagnostics(state, edge, protection, observer))
        } else {
            observer.record(ResolutionStep::AttackMissed {
                unused_edge: edge.balance(),
            });
            None
        };
        AttackOutcome { check, damage }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackOutcome {
    pub check: CheckOutcome,
    pub damage: Option<DamageOutcome>,
}
