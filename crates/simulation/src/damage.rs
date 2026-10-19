//! Canonical attack damage and nested protection groups. This module returns
//! private resolution facts; disclosure and death remain actor/engine concerns.

pub(crate) mod record;

use crate::attributes::{Attributes, CheckOutcome, SkillCheck, SkillRanks};
use crate::combat::DamageType;
use crate::content_ids::DescriptorId;
use crate::dice::{DicePool, Edge, PoolRoll};
use crate::rules::{ModifierOp, Selector};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn category_id(category: DamageType) -> &'static str {
    match category {
        DamageType::Energy => "energy",
        DamageType::Impact => "impact",
        DamageType::Keen => "keen",
        DamageType::Spirit => "spirit",
        DamageType::Vital => "vital",
        DamageType::Psychic => "psychic",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageError {
    InvalidSpec,
    NonNestedDescriptors,
    InvalidProtection,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DamageKey {
    pub category: DamageType,
    pub descriptor: Option<DescriptorId>,
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
    descriptor: Option<DescriptorId>,
    amount: DamageAmount,
}

impl DamageComponent {
    pub fn amount(&self) -> DamageAmount {
        self.amount
    }

    pub fn fixed(category: DamageType, descriptor: Option<DescriptorId>, amount: u32) -> Self {
        Self {
            category,
            descriptor,
            amount: DamageAmount::Fixed(amount),
        }
    }

    pub fn rolled(category: DamageType, descriptor: Option<DescriptorId>, pool: DicePool) -> Self {
        Self {
            category,
            descriptor,
            amount: DamageAmount::Rolled(pool),
        }
    }

    pub fn key(&self) -> DamageKey {
        DamageKey {
            category: self.category,
            descriptor: self.descriptor.clone(),
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
        if components.is_empty() || components.len() > crate::limits::DAMAGE_COMPONENTS {
            return Err(DamageError::InvalidSpec);
        }
        let mut canonical: BTreeMap<DamageKey, DamageComponent> = BTreeMap::new();
        let mut descriptor_categories = BTreeMap::new();
        for component in components {
            if i64::from(component.maximum()) > crate::limits::NUMERIC {
                return Err(DamageError::InvalidSpec);
            }
            if let Some(descriptor) = &component.descriptor {
                if descriptor_categories
                    .insert(descriptor.clone(), component.category)
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
        if primary
            .as_ref()
            .is_some_and(|key| !canonical.contains_key(key))
        {
            return Err(DamageError::InvalidSpec);
        }
        let mut components: Vec<_> = canonical.into_values().collect();
        if components
            .iter()
            .map(|component| u64::from(component.maximum()))
            .sum::<u64>()
            > crate::limits::NUMERIC as u64
        {
            return Err(DamageError::InvalidSpec);
        }
        components.sort_by_key(|component| (Some(component.key()) != primary, component.key()));
        Ok(Self {
            components,
            primary,
        })
    }

    /// Whether these bounded captured operands can produce any direct damage.
    pub(crate) fn can_harm(&self) -> bool {
        self.components
            .iter()
            .any(|component| component.maximum() > 0)
    }

    pub fn components(&self) -> &[DamageComponent] {
        &self.components
    }
    pub fn primary(&self) -> Option<DamageKey> {
        self.primary.clone()
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
        if extra_dice > crate::limits::DICE
            || !(-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(flat_bonus))
            || i64::from(impact_bonus) > crate::limits::NUMERIC
        {
            return Err(DamageError::InvalidSpec);
        }
        let key = self.primary.clone().ok_or(DamageError::InvalidSpec)?;
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
        let result = Self::new(components, self.primary.clone())?;
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
        if i64::from(bonus) > crate::limits::NUMERIC {
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
        Self::new(components, self.primary.clone())
    }

    pub fn resolve(
        &self,
        state: &mut u64,
        mut edge: Edge,
        protection: &Protection,
        observer: &mut impl crate::resolution_diagnostics::ResolutionObserver,
    ) -> DamageOutcome {
        use crate::resolution_diagnostics::{ResolutionStep, TraceField as F, TraceValue as V};
        observer.record(ResolutionStep {
            id: "damage.started",
            fields: &[F {
                id: "net_edge",
                value: V::Signed(edge.balance()),
            }],
        });
        let mut components = Vec::with_capacity(self.components.len());
        let mut groups: BTreeMap<(DamageType, Option<DescriptorId>), u32> = BTreeMap::new();
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
            let category_immune = protection.has_immunity(&Selector::Category {
                category: component.category,
            });
            let descriptor_immune = component.descriptor.as_ref().is_some_and(|value| {
                protection.has_immunity(&Selector::Descriptor {
                    descriptor: value.clone(),
                })
            });
            let protected = if category_immune || descriptor_immune {
                0
            } else {
                amount
            };
            *groups
                .entry((component.category, component.descriptor.clone()))
                .or_default() += protected;
            components.push(ComponentRoll {
                key: component.key(),
                raw: amount,
                pool,
            });
            let rolled = components
                .last()
                .and_then(|component| component.pool.as_ref());
            observer.record(ResolutionStep {
                id: "damage.component",
                fields: &[
                    F {
                        id: "category",
                        value: V::Text(category_id(component.category)),
                    },
                    F {
                        id: "descriptor",
                        value: V::Text(
                            component
                                .descriptor
                                .as_ref()
                                .map_or("", DescriptorId::as_str),
                        ),
                    },
                    F {
                        id: "amount_kind",
                        value: V::Text(if expression.is_some() {
                            "rolled"
                        } else {
                            "fixed"
                        }),
                    },
                    F {
                        id: "count",
                        value: V::Unsigned(u64::from(expression.map_or(0, |pool| pool.count()))),
                    },
                    F {
                        id: "sides",
                        value: V::Unsigned(u64::from(expression.map_or(0, |pool| pool.sides()))),
                    },
                    F {
                        id: "bonus",
                        value: V::Signed(i64::from(expression.map_or(0, |pool| pool.bonus()))),
                    },
                    F {
                        id: "raw",
                        value: V::Unsigned(u64::from(amount)),
                    },
                    F {
                        id: "edge",
                        value: V::Signed(allocated.balance()),
                    },
                    F {
                        id: "rng_before",
                        value: V::Unsigned(rng_before),
                    },
                    F {
                        id: "rng_after",
                        value: V::Unsigned(*state),
                    },
                    F {
                        id: "category_immune",
                        value: V::Bool(category_immune),
                    },
                    F {
                        id: "descriptor_immune",
                        value: V::Bool(descriptor_immune),
                    },
                    F {
                        id: "after_immunity",
                        value: V::Unsigned(u64::from(protected)),
                    },
                    F {
                        id: "rolled",
                        value: V::Dice(rolled.map_or(&[], |roll| &roll.rolled)),
                    },
                    F {
                        id: "kept",
                        value: V::Dice(rolled.map_or(&[], |roll| &roll.kept)),
                    },
                ],
            });
        }
        let after_immunity = groups.values().sum();
        for ((category, descriptor), amount) in &mut groups {
            if let Some(descriptor) = descriptor {
                let selector = Selector::Descriptor {
                    descriptor: descriptor.clone(),
                };
                let before = *amount;
                let capacity = protection.reduction(&selector);
                *amount = amount.saturating_sub(capacity);
                observer.record(ResolutionStep {
                    id: "damage.reduction",
                    fields: &[
                        F {
                            id: "selector_kind",
                            value: V::Text("descriptor"),
                        },
                        F {
                            id: "selector",
                            value: V::Text(descriptor.as_str()),
                        },
                        F {
                            id: "category",
                            value: V::Text(category_id(*category)),
                        },
                        F {
                            id: "before",
                            value: V::Unsigned(u64::from(before)),
                        },
                        F {
                            id: "capacity",
                            value: V::Unsigned(u64::from(capacity)),
                        },
                        F {
                            id: "after",
                            value: V::Unsigned(u64::from(*amount)),
                        },
                    ],
                });
            }
        }
        let after_descriptors = groups.values().sum();
        let mut by_category: BTreeMap<DamageType, u32> = BTreeMap::new();
        for ((category, _), amount) in groups {
            *by_category.entry(category).or_default() += amount;
        }
        for (&category, amount) in &mut by_category {
            let selector = Selector::Category { category };
            let before = *amount;
            let capacity = protection.reduction(&selector);
            *amount = amount.saturating_sub(capacity);
            observer.record(ResolutionStep {
                id: "damage.reduction",
                fields: &[
                    F {
                        id: "selector_kind",
                        value: V::Text("category"),
                    },
                    F {
                        id: "selector",
                        value: V::Text(category_id(category)),
                    },
                    F {
                        id: "category",
                        value: V::Text(category_id(category)),
                    },
                    F {
                        id: "before",
                        value: V::Unsigned(u64::from(before)),
                    },
                    F {
                        id: "capacity",
                        value: V::Unsigned(u64::from(capacity)),
                    },
                    F {
                        id: "after",
                        value: V::Unsigned(u64::from(*amount)),
                    },
                ],
            });
        }
        let total = by_category.values().sum();
        observer.record(ResolutionStep {
            id: "damage.finished",
            fields: &[
                F {
                    id: "raw",
                    value: V::Unsigned(u64::from(raw)),
                },
                F {
                    id: "after_immunity",
                    value: V::Unsigned(u64::from(after_immunity)),
                },
                F {
                    id: "after_descriptors",
                    value: V::Unsigned(u64::from(after_descriptors)),
                },
                F {
                    id: "total",
                    value: V::Unsigned(u64::from(total)),
                },
                F {
                    id: "unused_edge",
                    value: V::Signed(edge.balance()),
                },
            ],
        });
        DamageOutcome {
            unused_edge: edge,
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
    talent_reductions: BTreeMap<DamageType, u32>,
}

impl Protection {
    /// Fold one source operation into protection. Other operations are handled
    /// by the same derivation fold's stat, flag and ability accumulators.
    pub fn apply(&mut self, operation: &ModifierOp) -> Result<(), DamageError> {
        match operation {
            ModifierOp::Immunity { selector } => {
                self.immunities.insert(selector.clone());
            }
            ModifierOp::Reduction { selector, amount } => {
                let before = self.reductions.get(selector).copied().unwrap_or(0);
                let total = before
                    .checked_add(*amount)
                    .filter(|value| i64::from(*value) <= crate::limits::NUMERIC)
                    .ok_or(DamageError::InvalidProtection)?;
                Self::checked_total(total, self.talent_reduction(selector))?;
                self.reductions.insert(selector.clone(), total);
            }
            ModifierOp::TalentReduction { category, amount } => {
                let selector = Selector::Category {
                    category: *category,
                };
                let strongest = self.talent_reduction(&selector).max(*amount);
                Self::checked_total(
                    self.reductions.get(&selector).copied().unwrap_or(0),
                    strongest,
                )?;
                self.talent_reductions.insert(*category, strongest);
            }
            ModifierOp::Add { .. }
            | ModifierOp::Set { .. }
            | ModifierOp::Flag { .. }
            | ModifierOp::GrantAbility { .. }
            | ModifierOp::Locomotion { .. }
            | ModifierOp::Perception { .. } => {}
        }
        Ok(())
    }

    pub fn immune(&self, category: DamageType, descriptor: Option<&DescriptorId>) -> bool {
        self.has_immunity(&Selector::Category { category })
            || descriptor.is_some_and(|descriptor| {
                self.has_immunity(&Selector::Descriptor {
                    descriptor: descriptor.clone(),
                })
            })
    }

    pub fn has_immunity(&self, selector: &Selector) -> bool {
        self.immunities.contains(selector)
    }

    pub(crate) fn talent_reduction(&self, selector: &Selector) -> u32 {
        match selector {
            Selector::Category { category } => {
                self.talent_reductions.get(category).copied().unwrap_or(0)
            }
            Selector::Descriptor { .. } => 0,
        }
    }

    fn checked_total(ordinary: u32, talent: u32) -> Result<u32, DamageError> {
        ordinary
            .checked_add(talent)
            .filter(|total| i64::from(*total) <= crate::limits::NUMERIC)
            .ok_or(DamageError::InvalidProtection)
    }

    pub fn reduction(&self, selector: &Selector) -> u32 {
        // Each operation validates the combined total before mutating either bucket.
        self.reductions.get(selector).copied().unwrap_or(0) + self.talent_reduction(selector)
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
    pub unused_edge: Edge,
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
        mut edge: Edge,
        damage: &DamageSpec,
        protection: &Protection,
        observer: &mut impl crate::resolution_diagnostics::ResolutionObserver,
    ) -> AttackOutcome {
        use crate::resolution_diagnostics::{ResolutionStep, TraceField as F, TraceValue as V};
        observer.record(ResolutionStep {
            id: "attack.started",
            fields: &[F {
                id: "net_edge",
                value: V::Signed(edge.balance()),
            }],
        });
        let check = self.check.resolve(
            state,
            self.attributes,
            self.skills,
            edge.take(crate::dice::CHECK_EDGE_CAPACITY),
            observer,
        );
        let damage = if check.success {
            Some(damage.resolve(state, edge, protection, observer))
        } else {
            observer.record(ResolutionStep {
                id: "attack.missed",
                fields: &[F {
                    id: "unused_edge",
                    value: V::Signed(edge.balance()),
                }],
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

impl serde::Serialize for DamageSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        record::DamageRecord::capture(self).serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for DamageSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = <record::DamageRecord as serde::Deserialize>::deserialize(deserializer)?;
        record
            .restore()
            .map_err(|_| serde::de::Error::custom("invalid damage specification"))
    }
}

#[cfg(test)]
mod psychic_tests {
    use super::*;
    #[test]
    fn psychic_uses_generic_explicit_reduction_and_immunity_without_other_categories() {
        let component = DamageComponent::fixed(DamageType::Psychic, None, 9);
        let damage = DamageSpec::new(vec![component.clone()], Some(component.key())).unwrap();
        let mut protection = Protection::default();
        for category in [
            DamageType::Energy,
            DamageType::Impact,
            DamageType::Keen,
            DamageType::Spirit,
            DamageType::Vital,
        ] {
            protection
                .apply(&ModifierOp::Reduction {
                    selector: Selector::Category { category },
                    amount: 100,
                })
                .unwrap();
        }
        let resolve = |protection: &Protection| {
            damage
                .resolve(&mut 17, Edge::default(), protection, &mut ())
                .total
        };
        assert_eq!(resolve(&protection), 9);
        protection
            .apply(&ModifierOp::Reduction {
                selector: Selector::Category {
                    category: DamageType::Psychic,
                },
                amount: 3,
            })
            .unwrap();
        assert_eq!(resolve(&protection), 6);
        protection
            .apply(&ModifierOp::Immunity {
                selector: Selector::Category {
                    category: DamageType::Psychic,
                },
            })
            .unwrap();
        assert_eq!(resolve(&protection), 0);
        let encoded = serde_json::to_value(&damage).unwrap();
        let restored: DamageSpec = serde_json::from_value(encoded).unwrap();
        assert_eq!(restored, damage);
        assert_eq!(category_id(DamageType::Psychic), "psychic");
    }
}
