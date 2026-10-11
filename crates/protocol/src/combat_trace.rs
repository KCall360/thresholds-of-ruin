//! Privileged numerical resolution observations, independent of simulation/save
//! types. Recipient authorization belongs to the server and client connection.
use crate::{
    DamageType, InspectionAttribute, InspectionDescriptor, InspectionSelector, ManaBinding, Skill,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_COMBAT_TRACE_STEPS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCombatDiagnostics;

impl std::fmt::Display for InvalidCombatDiagnostics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Invalid combat diagnostics")
    }
}
impl std::error::Error for InvalidCombatDiagnostics {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticDiceExpression {
    pub count: u16,
    pub sides: u16,
    pub bonus: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticCheck {
    pub skill: Skill,
    pub binding: ManaBinding,
    pub attribute: InspectionAttribute,
    pub attribute_value: u16,
    pub rank: u8,
    pub modifier: i32,
    pub threshold: i32,
    #[serde(with = "crate::integers::signed")]
    pub input_edge: i64,
    #[serde(with = "crate::integers::signed")]
    pub edge: i64,
    #[serde(with = "crate::integers::signed")]
    pub unused_edge: i64,
    #[serde(with = "crate::integers::unsigned")]
    pub rng_before: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub rng_after: u64,
    pub first: u8,
    #[serde(deserialize_with = "Option::deserialize")]
    pub second: Option<u8>,
    pub kept: u8,
    #[serde(with = "crate::integers::signed")]
    pub total: i64,
    pub success: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticComponent {
    pub category: DamageType,
    #[serde(deserialize_with = "Option::deserialize")]
    pub descriptor: Option<InspectionDescriptor>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub expression: Option<DiagnosticDiceExpression>,
    #[serde(with = "crate::integers::signed")]
    pub edge: i64,
    #[serde(with = "crate::integers::unsigned")]
    pub rng_before: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub rng_after: u64,
    pub rolled: Vec<u16>,
    pub kept: Vec<u16>,
    pub raw: u32,
    pub category_immune: bool,
    pub descriptor_immune: bool,
    pub after_immunity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatTraceStep {
    AttackStarted {
        #[serde(with = "crate::integers::signed")]
        net_edge: i64,
    },
    Check {
        check: DiagnosticCheck,
    },
    AttackMissed {
        #[serde(with = "crate::integers::signed")]
        unused_edge: i64,
    },
    DamageStarted {
        #[serde(with = "crate::integers::signed")]
        net_edge: i64,
    },
    Component {
        component: DiagnosticComponent,
    },
    Reduction {
        selector: InspectionSelector,
        category: DamageType,
        before: u32,
        capacity: u32,
        after: u32,
    },
    DamageFinished {
        raw: u32,
        after_immunity: u32,
        after_descriptors: u32,
        total: u32,
        #[serde(with = "crate::integers::signed")]
        unused_edge: i64,
    },
    FearStarted {
        difficulty: i32,
        #[serde(with = "crate::integers::unsigned")]
        duration: u64,
        #[serde(with = "crate::integers::signed")]
        net_edge: i64,
        difficulty_attribute: u16,
        difficulty_rank: u8,
        difficulty_bonus: i32,
        #[serde(with = "crate::integers::unsigned")]
        duration_bonus: u64,
    },
    FearImmunity {
        fear: bool,
        mind_affecting: bool,
    },
    FearFinished {
        applied: bool,
        #[serde(with = "crate::integers::unsigned")]
        duration: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatTraceView {
    pub steps: Vec<CombatTraceStep>,
    pub truncated: bool,
}

fn bounded_edge(edge: i64) -> bool {
    edge.unsigned_abs() <= u64::from(u32::MAX)
}
fn allocation(edge: i64, capacity: u16) -> i64 {
    edge.signum() * edge.unsigned_abs().min(u64::from(capacity)) as i64
}

fn governing_attribute(skill: Skill, binding: ManaBinding) -> InspectionAttribute {
    use InspectionAttribute as A;
    match skill {
        Skill::Athletics | Skill::HeavyWeaponry => A::Strength,
        Skill::Agility | Skill::LightWeaponry | Skill::Stealth | Skill::Thievery => A::Speed,
        Skill::Crafting | Skill::Deduction | Skill::Lore | Skill::Medicine => A::Intellect,
        Skill::Discipline | Skill::Intimidation => A::Willpower,
        Skill::Insight | Skill::Perception | Skill::Survival => A::Awareness,
        Skill::Deception | Skill::Leadership | Skill::Persuasion => A::Presence,
        Skill::Spellcasting => match binding {
            ManaBinding::Intellect => A::Intellect,
            ManaBinding::Willpower => A::Willpower,
            ManaBinding::Awareness => A::Awareness,
            ManaBinding::Presence => A::Presence,
        },
    }
}

impl DiagnosticCheck {
    fn valid(&self) -> bool {
        if !bounded_edge(self.input_edge)
            || self.edge != self.input_edge.signum()
            || self.unused_edge != self.input_edge - self.edge
            || self.attribute != governing_attribute(self.skill, self.binding)
            || self.attribute_value > 1000
            || self.rank > 5
            || !(1..=20).contains(&self.first)
            || self.rng_before == self.rng_after
        {
            return false;
        }
        let kept = match (self.edge, self.second) {
            (0, None) => self.first,
            (1, Some(second)) if (1..=20).contains(&second) => self.first.max(second),
            (-1, Some(second)) if (1..=20).contains(&second) => self.first.min(second),
            _ => return false,
        };
        let total = i64::from(kept)
            + i64::from(self.attribute_value)
            + i64::from(self.rank)
            + i64::from(self.modifier);
        self.kept == kept
            && self.total == total
            && self.success == (total >= i64::from(self.threshold))
    }
}

impl DiagnosticComponent {
    fn valid(&self) -> bool {
        if self.raw > 1_000_000
            || (self.descriptor.is_none() && self.descriptor_immune)
            || self.after_immunity
                != if self.category_immune || self.descriptor_immune {
                    0
                } else {
                    self.raw
                }
        {
            return false;
        }
        let Some(pool) = self.expression else {
            return self.edge == 0
                && self.rng_before == self.rng_after
                && self.rolled.is_empty()
                && self.kept.is_empty();
        };
        if !(1..=64).contains(&pool.count)
            || !(2..=1000).contains(&pool.sides)
            || !(-1_000_000..=1_000_000).contains(&pool.bonus)
            || i64::from(pool.count) * i64::from(pool.sides) + i64::from(pool.bonus) > 1_000_000
            || self.edge.unsigned_abs() > u64::from(pool.count)
            || self.rolled.len() != usize::from(pool.count) + self.edge.unsigned_abs() as usize
            || self.kept.len() != usize::from(pool.count)
            || self
                .rolled
                .iter()
                .any(|value| !(1..=pool.sides).contains(value))
            || self.rng_before == self.rng_after
        {
            return false;
        }
        let mut kept = self.rolled.clone();
        if self.edge > 0 {
            kept.sort_unstable_by(|a, b| b.cmp(a));
        }
        if self.edge < 0 {
            kept.sort_unstable();
        }
        kept.truncate(usize::from(pool.count));
        let total = (kept.iter().map(|value| i64::from(*value)).sum::<i64>()
            + i64::from(pool.bonus))
        .max(0) as u32;
        self.kept == kept && self.raw == total
    }
}

impl CombatTraceView {
    pub fn validate(&self) -> Result<(), InvalidCombatDiagnostics> {
        self.validated_result()?;
        if !matches!(
            crate::codec::encoded_length(self, crate::MAX_RESPONSE_BYTES),
            Ok(Some(_))
        ) {
            return Err(InvalidCombatDiagnostics);
        }
        Ok(())
    }

    pub(crate) fn validated_result(
        &self,
    ) -> Result<Option<(bool, u32, u64)>, InvalidCombatDiagnostics> {
        if self.steps.is_empty()
            || self.steps.len() > MAX_COMBAT_TRACE_STEPS
            || (self.truncated && self.steps.len() != MAX_COMBAT_TRACE_STEPS)
        {
            return Err(InvalidCombatDiagnostics);
        }
        let result = self.result()?;
        if self.truncated == result.is_some() {
            return Err(InvalidCombatDiagnostics);
        }
        Ok(result)
    }

    /// Complete traces yield applied/damage/effect-duration; truncated prefixes
    /// never yield an apparently complete resolution result.
    fn result(&self) -> Result<Option<(bool, u32, u64)>, InvalidCombatDiagnostics> {
        use CombatTraceStep as S;
        let mut steps = self.steps.iter().peekable();
        macro_rules! required {
            () => {
                match steps.next() {
                    Some(step) => step,
                    None if self.truncated => return Ok(None),
                    None => return Err(InvalidCombatDiagnostics),
                }
            };
        }
        macro_rules! ensure {
            ($condition:expr) => {
                if !$condition {
                    return Err(InvalidCombatDiagnostics);
                }
            };
        }
        let outcome;
        match required!() {
            S::Check { check } => {
                ensure!(check.valid());
                outcome = (check.success, 0, 0);
            }
            S::FearStarted {
                difficulty,
                duration,
                net_edge,
                difficulty_attribute,
                difficulty_rank,
                difficulty_bonus,
                duration_bonus,
            } => {
                ensure!(bounded_edge(*net_edge) && (1..=1_000_000).contains(duration));
                ensure!(
                    *difficulty_attribute <= 1000
                        && *difficulty_rank <= 5
                        && (-1_000_000..=1_000_000).contains(difficulty_bonus)
                );
                ensure!(
                    i64::from(*difficulty)
                        == 10
                            + i64::from(*difficulty_attribute)
                            + i64::from(*difficulty_rank)
                            + i64::from(*difficulty_bonus)
                );
                ensure!(duration_bonus.checked_add(300) == Some(*duration));
                let applied = match required!() {
                    S::FearImmunity {
                        fear,
                        mind_affecting,
                    } => {
                        ensure!(*fear || *mind_affecting);
                        false
                    }
                    S::Check { check } => {
                        ensure!(
                            check.valid()
                                && check.skill == Skill::Discipline
                                && check.threshold == *difficulty
                                && check.input_edge == *net_edge
                        );
                        !check.success
                    }
                    _ => return Err(InvalidCombatDiagnostics),
                };
                match required!() {
                    S::FearFinished {
                        applied: actual,
                        duration: actual_duration,
                    } => ensure!(
                        *actual == applied
                            && *actual_duration == if applied { *duration } else { 0 }
                    ),
                    _ => return Err(InvalidCombatDiagnostics),
                }
                outcome = (applied, 0, if applied { *duration } else { 0 });
            }
            S::AttackStarted { net_edge } => {
                ensure!(bounded_edge(*net_edge));
                let check = match required!() {
                    S::Check { check } => check,
                    _ => return Err(InvalidCombatDiagnostics),
                };
                let allocated = allocation(*net_edge, 1);
                ensure!(check.valid() && check.input_edge == allocated);
                let mut edge = *net_edge - allocated;
                if !check.success {
                    match required!() {
                        S::AttackMissed { unused_edge } => ensure!(*unused_edge == edge),
                        _ => return Err(InvalidCombatDiagnostics),
                    }
                    outcome = (false, 0, 0);
                } else {
                    match required!() {
                        S::DamageStarted { net_edge } => ensure!(*net_edge == edge),
                        _ => return Err(InvalidCombatDiagnostics),
                    }
                    let mut rng = check.rng_after;
                    let mut groups = BTreeMap::new();
                    let mut keys = BTreeSet::new();
                    let mut descriptors = BTreeMap::new();
                    let mut category_immunity = BTreeMap::new();
                    let mut descriptor_immunity = BTreeMap::new();
                    let mut raw = 0u32;
                    let mut previous = None;
                    while matches!(steps.peek(), Some(S::Component { .. })) {
                        let S::Component { component } = required!() else {
                            unreachable!()
                        };
                        ensure!(component.valid() && component.rng_before == rng);
                        let key = (
                            component.category,
                            component.descriptor,
                            component.expression.map(|pool| pool.sides),
                        );
                        ensure!(keys.len() < 32 && keys.insert(key));
                        // The primary may lead; all subsequent components are canonical.
                        if keys.len() > 2 {
                            ensure!(previous.is_none_or(|previous| previous < key));
                        }
                        previous = Some(key);
                        let capacity = component.expression.map_or(0, |pool| pool.count);
                        let allocated = allocation(edge, capacity);
                        ensure!(component.edge == allocated);
                        edge -= allocated;
                        rng = component.rng_after;
                        raw += component.raw;
                        ensure!(raw <= 1_000_000);
                        if let Some(old) =
                            category_immunity.insert(component.category, component.category_immune)
                        {
                            ensure!(old == component.category_immune);
                        }
                        if let Some(descriptor) = component.descriptor {
                            if let Some(old) = descriptors.insert(descriptor, component.category) {
                                ensure!(old == component.category);
                            }
                            if let Some(old) =
                                descriptor_immunity.insert(descriptor, component.descriptor_immune)
                            {
                                ensure!(old == component.descriptor_immune);
                            }
                        }
                        *groups
                            .entry((component.category, component.descriptor))
                            .or_insert(0u32) += component.after_immunity;
                    }
                    // A truncated prefix may end before its first component.
                    if groups.is_empty() {
                        let _ = required!();
                        return Err(InvalidCombatDiagnostics);
                    }
                    let after_immunity: u32 = groups.values().sum();
                    for ((category, descriptor), amount) in &mut groups {
                        if let Some(descriptor) = descriptor {
                            match required!() {
                                S::Reduction {
                                    selector,
                                    category: actual_category,
                                    before,
                                    capacity,
                                    after,
                                } => {
                                    ensure!(
                                        *selector
                                            == InspectionSelector::Descriptor {
                                                descriptor: *descriptor
                                            }
                                            && *actual_category == *category
                                            && *before == *amount
                                            && *capacity <= 1_000_000
                                            && *after == amount.saturating_sub(*capacity)
                                    );
                                    *amount = *after;
                                }
                                _ => return Err(InvalidCombatDiagnostics),
                            }
                        }
                    }
                    let after_descriptors: u32 = groups.values().sum();
                    let mut categories = BTreeMap::new();
                    for ((category, _), amount) in groups {
                        *categories.entry(category).or_insert(0u32) += amount;
                    }
                    for (category, amount) in &mut categories {
                        match required!() {
                            S::Reduction {
                                selector,
                                category: actual_category,
                                before,
                                capacity,
                                after,
                            } => {
                                ensure!(
                                    *selector
                                        == InspectionSelector::Category {
                                            category: *category
                                        }
                                        && *actual_category == *category
                                        && *before == *amount
                                        && *capacity <= 1_000_000
                                        && *after == amount.saturating_sub(*capacity)
                                );
                                *amount = *after;
                            }
                            _ => return Err(InvalidCombatDiagnostics),
                        }
                    }
                    let total: u32 = categories.values().sum();
                    match required!() {
                        S::DamageFinished {
                            raw: actual_raw,
                            after_immunity: actual_immunity,
                            after_descriptors: actual_descriptors,
                            total: actual_total,
                            unused_edge,
                        } => ensure!(
                            *actual_raw == raw
                                && *actual_immunity == after_immunity
                                && *actual_descriptors == after_descriptors
                                && *actual_total == total
                                && *unused_edge == edge
                        ),
                        _ => return Err(InvalidCombatDiagnostics),
                    }
                    outcome = (true, total, 0);
                }
            }
            _ => return Err(InvalidCombatDiagnostics),
        }
        ensure!(steps.next().is_none());
        Ok(Some(outcome))
    }
}
