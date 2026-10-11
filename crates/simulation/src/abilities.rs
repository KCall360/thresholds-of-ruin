//! Owned plans, pure resolution and a single outcome application boundary.
use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
use crate::costs::ResourceCost;
use crate::damage::{DamageComponent, DamageSpec};
use crate::dice::{roll_check, DicePool, Edge};
use crate::resolution_diagnostics::{ResolutionObserver, ResolutionStep, TraceField, TraceValue};
use crate::resources::Resource;
use crate::rules::{
    Against, AmountExpr, CheckActor, DamageExpr, DefenseKind, Effect, Reach, Selector,
    SkillSelector, TimingSource,
};
use crate::{ActorId, Game, GameError, Work};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    UnknownAbility,
    Dead,
    OutOfReach,
    Unfunded,
    InvalidDefinition,
    InvalidPlan,
    ConditionCapacity,
}
fn invalid() -> GameError {
    GameError::Rule(RuleError::InvalidDefinition)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckThreshold {
    Fixed { value: i64 },
    Opposed { bonus: i64, edge: i64 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedCheck {
    pub actor: CheckActor,
    pub bonus: i64,
    pub threshold: CheckThreshold,
    pub edge: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedEffect {
    Damage {
        damage: DamageSpec,
    },
    Heal {
        amount: PlannedAmount,
    },
    ApplyCondition {
        id: ConditionId,
        duration: u64,
    },
    Restore {
        resource: Resource,
        amount: PlannedAmount,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedAmount {
    Fixed { amount: u32 },
    Rolled { dice: DicePool },
}
impl PlannedAmount {
    fn roll(&self, rng: &mut u64) -> u32 {
        match self {
            Self::Fixed { amount } => *amount,
            Self::Rolled { dice } => dice.roll(rng, Edge::default()).total,
        }
    }
    fn compile(amount: &AmountExpr) -> Result<Self, GameError> {
        match amount {
            AmountExpr::Fixed { amount } if *amount <= crate::limits::NUMERIC as u32 => {
                Ok(Self::Fixed { amount: *amount })
            }
            AmountExpr::Fixed { .. } => Err(invalid()),
            AmountExpr::Rolled { dice } => Ok(Self::Rolled {
                dice: DicePool::new(dice.count, dice.sides, dice.bonus).map_err(|_| invalid())?,
            }),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub work: Work,
    pub resolution: ResolutionPlan,
}
impl std::ops::Deref for Plan {
    type Target = ResolutionPlan;
    fn deref(&self) -> &ResolutionPlan {
        &self.resolution
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionPlan {
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Option<AbilityId>,
    pub preparation: u64,
    pub recovery: u64,
    pub cost: Option<ResourceCost>,
    pub reach: Option<Reach>,
    pub check: Option<PlannedCheck>,
    pub on_success: Vec<PlannedEffect>,
    pub on_failure: Vec<PlannedEffect>,
    pub descriptors: Vec<DescriptorId>,
    pub edge: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    creature: crate::creatures::CreatureState,
}
impl Snapshot {
    pub fn capture(creature: &crate::creatures::CreatureState) -> Self {
        Self {
            creature: creature.clone(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Applied,
    Unaffected,
    Miss,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResolvedEffect {
    Damage { amount: u32 },
    Heal { amount: u32 },
    ApplyCondition { id: ConditionId, duration: u64 },
    Restore { resource: Resource, amount: u32 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Option<AbilityId>,
    pub intention: Option<crate::IntentionId>,
    pub result: OutcomeStatus,
    pub effects: Vec<ResolvedEffect>,
}
impl Plan {
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        self.work.structural_valid(self.actor)
            && self.resolution.valid(rules)
            && match &self.work {
                Work::UseAbility { id, target } => {
                    self.ability.as_ref() == Some(id)
                        && *target == self.target
                        && rules.ability(id).is_some_and(|definition| {
                            self.resolution.matches_definition(definition)
                                && match definition.reach {
                                    Reach::SelfOnly => self.actor == self.target,
                                    _ => self.actor != self.target,
                                }
                                && matches!(
                                    (&definition.reach, self.reach.as_ref()),
                                    (Reach::Melee, Some(Reach::Melee))
                                        | (Reach::Sight { .. }, Some(Reach::Sight { .. }))
                                        | (Reach::SelfOnly, Some(Reach::SelfOnly))
                                )
                        })
                }
                Work::Equip { .. } | Work::Unequip { .. } | Work::Drink { .. } => {
                    self.ability.is_none()
                        && self.target == self.actor
                        && self.cost.is_none()
                        && self.check.is_none()
                        && self.descriptors.is_empty()
                        && self.recovery == 0
                        && self.edge == 0
                        && self.on_failure.is_empty()
                        && match self.work {
                            Work::Equip { .. } | Work::Unequip { .. } => {
                                self.on_success.is_empty() && self.reach.is_none()
                            }
                            Work::Drink { .. } => {
                                matches!(self.reach, Some(Reach::SelfOnly))
                                    || (self.on_success.is_empty() && self.reach.is_none())
                            }
                            Work::UseAbility { .. } => false,
                        }
                }
            }
    }
}
fn damage_matches_definition(expected: &DamageExpr, actual: &DamageSpec) -> bool {
    let DamageExpr::Components { components } = expected else {
        // Selected weapon keys belonged to the captured equipment state.
        return true;
    };
    let key = |component: &crate::rules::DamageComponentExpr| crate::damage::DamageKey {
        category: component.category,
        descriptor: component.descriptor.clone(),
        sides: match &component.amount {
            AmountExpr::Fixed { .. } => None,
            AmountExpr::Rolled { dice } => Some(dice.sides),
        },
    };
    if actual.primary() != components.first().map(key) {
        return false;
    }
    let mut canonical = std::collections::BTreeMap::new();
    for component in components {
        let amount = canonical.entry(key(component)).or_insert(None::<u64>);
        if let AmountExpr::Fixed { amount: value } = component.amount {
            *amount = Some(amount.unwrap_or(0) + u64::from(value));
        }
    }
    canonical.len() == actual.components().len()
        && actual.components().iter().all(|component| {
            match (canonical.get(&component.key()), component.amount()) {
                (Some(Some(expected)), crate::damage::DamageAmount::Fixed(actual)) => {
                    *expected == u64::from(actual)
                }
                (Some(None), crate::damage::DamageAmount::Rolled(_)) => true,
                _ => false,
            }
        })
}

/// Compare immutable effect shapes without re-evaluating captured derived operands.
pub(crate) fn effects_match_definition(expected: &[Effect], actual: &[PlannedEffect]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| match (expected, actual) {
                (Effect::Damage { damage: expected }, PlannedEffect::Damage { damage: actual }) => {
                    damage_matches_definition(expected, actual)
                }
                (Effect::Heal { amount: expected }, PlannedEffect::Heal { amount: actual }) => {
                    PlannedAmount::compile(expected).is_ok_and(|expected| &expected == actual)
                }
                (
                    Effect::ApplyCondition { condition, .. },
                    PlannedEffect::ApplyCondition { id, .. },
                ) => condition == id,
                (
                    Effect::Restore {
                        resource: expected_resource,
                        amount: expected,
                    },
                    PlannedEffect::Restore {
                        resource: actual_resource,
                        amount: actual,
                    },
                ) => {
                    expected_resource == actual_resource
                        && PlannedAmount::compile(expected)
                            .is_ok_and(|expected| &expected == actual)
                }
                _ => false,
            })
}

/// Item component pools and condition durations have no derived modifiers.
pub(crate) fn item_effects_match_definition(expected: &[Effect], actual: &[PlannedEffect]) -> bool {
    effects_match_definition(expected, actual)
        && expected
            .iter()
            .zip(actual)
            .all(|(expected, actual)| match (expected, actual) {
                (
                    Effect::ApplyCondition {
                        duration: expected, ..
                    },
                    PlannedEffect::ApplyCondition {
                        duration: actual, ..
                    },
                ) => expected == actual,
                (
                    Effect::Damage {
                        damage: DamageExpr::Components { components },
                    },
                    PlannedEffect::Damage { damage },
                ) => {
                    let values = components
                        .iter()
                        .map(|component| match &component.amount {
                            AmountExpr::Fixed { amount } => Ok(DamageComponent::fixed(
                                component.category,
                                component.descriptor.clone(),
                                *amount,
                            )),
                            AmountExpr::Rolled { dice } => {
                                DicePool::new(dice.count, dice.sides, dice.bonus).map(|pool| {
                                    DamageComponent::rolled(
                                        component.category,
                                        component.descriptor.clone(),
                                        pool,
                                    )
                                })
                            }
                        })
                        .collect::<Result<Vec<_>, _>>();
                    values.is_ok_and(|values| {
                        let primary = values.first().map(DamageComponent::key);
                        DamageSpec::new(values, primary).is_ok_and(|expected| &expected == damage)
                    })
                }
                _ => true,
            })
}

impl ResolutionPlan {
    /// Immutable content contract; captured numeric operands are deliberately
    /// not recomputed from the actor's possibly changed build.
    fn matches_definition(&self, definition: &crate::rules::AbilityDef) -> bool {
        let cost = definition.cost.as_ref().map(|cost| ResourceCost {
            resource: cost.resource,
            start: cost.start,
            resolution: cost.resolution,
        });
        let check = match (&definition.check, &self.check) {
            (None, None) => true,
            (Some(expected), Some(actual)) => {
                expected.actor == actual.actor
                    && matches!(
                        (&expected.against, &actual.threshold),
                        (
                            Against::Defense { .. } | Against::Difficulty { .. },
                            CheckThreshold::Fixed { .. }
                        ) | (Against::Opposed { .. }, CheckThreshold::Opposed { .. })
                    )
            }
            _ => false,
        };
        self.cost == cost
            && self.descriptors == definition.descriptors
            && check
            && effects_match_definition(&definition.on_success, &self.on_success)
            && effects_match_definition(&definition.on_failure, &self.on_failure)
    }

    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let amount = |value: &PlannedAmount| match value {
            PlannedAmount::Fixed { amount } => *amount <= crate::limits::NUMERIC as u32,
            PlannedAmount::Rolled { .. } => true,
        };
        self.actor.0 > 0
            && self.target.0 > 0
            && self.actor.0 != u64::MAX
            && self.target.0 != u64::MAX
            && self.preparation > 0
            && self.preparation <= crate::limits::NUMERIC as u64
            && self.recovery <= crate::limits::NUMERIC as u64
            && self
                .ability
                .as_ref()
                .is_none_or(|id| rules.ability(id).is_some())
            && self.cost.is_none_or(|cost| {
                cost.start <= crate::limits::NUMERIC as u32
                    && cost.resolution <= crate::limits::NUMERIC as u32
            })
            && self.reach.as_ref().is_none_or(|reach| match reach {
                Reach::Sight { range } => (1..=crate::limits::SIGHT_RANGE).contains(range),
                Reach::Melee | Reach::SelfOnly => true,
            })
            && self.descriptors.len() <= crate::limits::DESCRIPTORS_PER_EFFECT
            && self
                .descriptors
                .iter()
                .all(|id| rules.document().descriptors.iter().any(|def| &def.id == id))
            && self.edge.unsigned_abs() <= crate::limits::NUMERIC as u64
            && self.check.as_ref().is_none_or(|check| {
                check.bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
                    && check.edge.unsigned_abs() <= crate::limits::NUMERIC as u64
                    && match check.threshold {
                        CheckThreshold::Fixed { value } => {
                            value.unsigned_abs() <= crate::limits::NUMERIC as u64
                        }
                        CheckThreshold::Opposed { bonus, edge } => {
                            bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
                                && edge.unsigned_abs() <= crate::limits::NUMERIC as u64
                        }
                    }
            })
            && self.on_success.len() <= crate::limits::EFFECTS_PER_BRANCH
            && self.on_failure.len() <= crate::limits::EFFECTS_PER_BRANCH
            && self
                .on_success
                .iter()
                .chain(&self.on_failure)
                .all(|effect| match effect {
                    PlannedEffect::Damage { damage } => rules.validate_damage(damage).is_ok(),
                    PlannedEffect::Heal { amount: value }
                    | PlannedEffect::Restore { amount: value, .. } => amount(value),
                    PlannedEffect::ApplyCondition { id, duration } => {
                        rules.condition(id).is_some()
                            && *duration > 0
                            && *duration <= crate::limits::NUMERIC as u64
                    }
                })
    }
}
fn edge(balance: i64) -> Edge {
    Edge::from_counts(
        balance.max(0).min(i64::from(u32::MAX)) as u32,
        balance.saturating_neg().max(0).min(i64::from(u32::MAX)) as u32,
    )
}
/// The only effect resolver. It reads no game state, writes no actor, and draws only from rng.
pub fn resolve(
    plan: &ResolutionPlan,
    snapshot: &Snapshot,
    rng: &mut u64,
    observer: &mut impl ResolutionObserver,
) -> Outcome {
    let mut outcome = Outcome {
        actor: plan.actor,
        target: plan.target,
        ability: plan.ability.clone(),
        intention: None,
        result: OutcomeStatus::Unaffected,
        effects: vec![],
    };
    if snapshot.creature.health().dead()
        || plan.descriptors.iter().any(|descriptor| {
            snapshot
                .creature
                .derived()
                .protection
                .has_immunity(&Selector::Descriptor {
                    descriptor: descriptor.clone(),
                })
        })
    {
        observer.record(ResolutionStep {
            id: "effect.immune",
            fields: &[],
        });
        return outcome;
    }
    let mut remaining_edge = edge(plan.edge);
    let success = if let Some(check) = &plan.check {
        let mut check_edge = edge(check.edge);
        let allocated = check_edge.take(1);
        if check.actor == CheckActor::User {
            remaining_edge = check_edge;
        }
        let rng_before = *rng;
        let rolled = roll_check(rng, allocated);
        let total = i64::from(rolled.kept) + check.bonus;
        let threshold = match check.threshold {
            CheckThreshold::Fixed { value } => value,
            CheckThreshold::Opposed {
                bonus,
                edge: opposed_edge,
            } => {
                let rng_before = *rng;
                let mut opposed_remaining = edge(opposed_edge);
                let opposed_allocated = opposed_remaining.take(1);
                if check.actor == CheckActor::Target {
                    // The opposing roller is the caster, whose remaining budget
                    // also owns later damage effects.
                    remaining_edge = opposed_remaining;
                }
                let opposed = roll_check(rng, opposed_allocated);
                let total = i64::from(opposed.kept) + bonus;
                let draws = [
                    u16::from(opposed.first),
                    u16::from(opposed.second.unwrap_or(opposed.first)),
                ];
                observer.record(ResolutionStep {
                    id: "opposed_check",
                    fields: &[
                        TraceField {
                            id: "rolled",
                            value: TraceValue::Dice(
                                &draws[..if opposed.second.is_some() { 2 } else { 1 }],
                            ),
                        },
                        TraceField {
                            id: "kept",
                            value: TraceValue::Unsigned(u64::from(opposed.kept)),
                        },
                        TraceField {
                            id: "input_edge",
                            value: TraceValue::Signed(opposed_edge),
                        },
                        TraceField {
                            id: "edge",
                            value: TraceValue::Signed(opposed_allocated.balance()),
                        },
                        TraceField {
                            id: "unused_edge",
                            value: TraceValue::Signed(opposed_remaining.balance()),
                        },
                        TraceField {
                            id: "bonus",
                            value: TraceValue::Signed(bonus),
                        },
                        TraceField {
                            id: "total",
                            value: TraceValue::Signed(total),
                        },
                        TraceField {
                            id: "rng_before",
                            value: TraceValue::Unsigned(rng_before),
                        },
                        TraceField {
                            id: "rng_after",
                            value: TraceValue::Unsigned(*rng),
                        },
                    ],
                });
                total
            }
        };
        let draws = [
            u16::from(rolled.first),
            u16::from(rolled.second.unwrap_or(rolled.first)),
        ];
        observer.record(ResolutionStep {
            id: "check",
            fields: &[
                TraceField {
                    id: "rolled",
                    value: TraceValue::Dice(&draws[..if rolled.second.is_some() { 2 } else { 1 }]),
                },
                TraceField {
                    id: "kept",
                    value: TraceValue::Unsigned(u64::from(rolled.kept)),
                },
                TraceField {
                    id: "input_edge",
                    value: TraceValue::Signed(check.edge),
                },
                TraceField {
                    id: "edge",
                    value: TraceValue::Signed(allocated.balance()),
                },
                TraceField {
                    id: "unused_edge",
                    value: TraceValue::Signed(check_edge.balance()),
                },
                TraceField {
                    id: "rng_before",
                    value: TraceValue::Unsigned(rng_before),
                },
                TraceField {
                    id: "rng_after",
                    value: TraceValue::Unsigned(*rng),
                },
                TraceField {
                    id: "total",
                    value: TraceValue::Signed(total),
                },
                TraceField {
                    id: "roll",
                    value: TraceValue::Unsigned(u64::from(rolled.kept)),
                },
                TraceField {
                    id: "bonus",
                    value: TraceValue::Signed(check.bonus),
                },
                TraceField {
                    id: "threshold",
                    value: TraceValue::Signed(threshold),
                },
                TraceField {
                    id: "success",
                    value: TraceValue::Bool(total >= threshold),
                },
            ],
        });
        total >= threshold
    } else {
        true
    };
    let effects = if success {
        &plan.on_success
    } else {
        &plan.on_failure
    };
    if effects.is_empty()
        && !success
        && plan
            .check
            .as_ref()
            .is_some_and(|check| check.actor == CheckActor::User)
    {
        outcome.result = OutcomeStatus::Miss;
    }
    let mut creature = snapshot.creature.clone();
    for effect in effects {
        // Once damage kills the target, later effects cannot revive or mutate it.
        if creature.health().dead() {
            break;
        }
        let effect = match effect {
            PlannedEffect::Damage { damage } => {
                let resolved = damage.resolve(
                    rng,
                    remaining_edge,
                    &creature.derived().protection,
                    observer,
                );
                remaining_edge = resolved.unused_edge;
                ResolvedEffect::Damage {
                    amount: creature.damage(resolved.total).applied,
                }
            }
            PlannedEffect::Heal { amount } => ResolvedEffect::Heal {
                amount: creature.heal(amount.roll(rng)),
            },
            PlannedEffect::ApplyCondition { id, duration } => {
                let before = creature.clone();
                if creature.apply_condition(id, plan.actor, *duration).is_err() {
                    observer.record(ResolutionStep {
                        id: "condition.rejected",
                        fields: &[TraceField {
                            id: "condition",
                            value: TraceValue::Text(id.as_str()),
                        }],
                    });
                    continue;
                }
                if creature == before {
                    continue;
                }
                ResolvedEffect::ApplyCondition {
                    id: id.clone(),
                    duration: *duration,
                }
            }
            PlannedEffect::Restore { resource, amount } => ResolvedEffect::Restore {
                resource: *resource,
                amount: creature.restore_resource(*resource, amount.roll(rng)),
            },
        };
        let changed = match &effect {
            ResolvedEffect::Damage { amount }
            | ResolvedEffect::Heal { amount }
            | ResolvedEffect::Restore { amount, .. } => *amount > 0,
            ResolvedEffect::ApplyCondition { .. } => true,
        };
        if changed {
            outcome.result = OutcomeStatus::Applied;
        }
        outcome.effects.push(effect);
    }
    outcome
}

impl Game {
    pub fn basic_attack(&self, actor: ActorId) -> Option<AbilityId> {
        let creature = self.creature(actor)?;
        creature
            .derived()
            .abilities
            .iter()
            .find(|id| {
                creature
                    .build()
                    .ruleset()
                    .ability(id)
                    .is_some_and(|def| def.basic_attack)
            })
            .cloned()
    }
    pub fn ability_cost(&self, ability: &AbilityId) -> Result<Option<ResourceCost>, GameError> {
        Ok(self
            .content
            .rules
            .ability(ability)
            .ok_or(GameError::Rule(RuleError::UnknownAbility))?
            .cost
            .as_ref()
            .map(|cost| ResourceCost {
                resource: cost.resource,
                start: cost.start,
                resolution: cost.resolution,
            }))
    }
    fn target_in_reach(&self, actor: ActorId, target: ActorId, reach: &Reach) -> bool {
        self.target_in_reach_snapshot(actor, target, reach, None)
    }
    fn target_in_reach_snapshot(
        &self,
        actor: ActorId,
        target: ActorId,
        reach: &Reach,
        sight: Option<&[tor_world::SightCell]>,
    ) -> bool {
        if self.actor_frozen(actor)
            || self.actor_frozen(target)
            || self
                .creature(target)
                .is_none_or(|creature| creature.health().dead())
        {
            return false;
        }
        match reach {
            Reach::SelfOnly => actor == target,
            Reach::Melee => {
                actor != target
                    && sight.map_or_else(
                        || self.attack_available(actor, target),
                        |scene| self.attack_available_in_scene(actor, target, scene),
                    )
            }
            Reach::Sight { range } => {
                if actor == target {
                    return false;
                }
                let Some(target_actor) = self.actors.get(&target) else {
                    return false;
                };
                let Some(cells) = self.body_cells(
                    target_actor.location,
                    target_actor.orientation,
                    &target_actor.body,
                ) else {
                    return false;
                };
                let targets: std::collections::BTreeSet<_> =
                    cells.into_iter().map(|(at, _)| at).collect();
                let Some((eye, frame)) = self.eye(actor) else {
                    return false;
                };
                let Ok(range) = u8::try_from(*range) else {
                    return false;
                };
                if let Some(scene) = sight {
                    scene.iter().any(|cell| {
                        targets.contains(&cell.location)
                            && cell.offset.x.unsigned_abs()
                                + cell.offset.y.unsigned_abs()
                                + cell.offset.z.unsigned_abs()
                                <= u32::from(range)
                    })
                } else {
                    crate::diagnostics::ordinary_scene();
                    crate::diagnostics::scene();
                    self.world
                        .illuminated_eye_scene(eye, frame, range)
                        .iter()
                        .any(|cell| targets.contains(&cell.location))
                }
            }
        }
    }
    pub fn ability_timing(
        &self,
        actor: ActorId,
        ability: &AbilityId,
    ) -> Result<(u64, u64), GameError> {
        let caster = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let rules = caster.build().ruleset();
        let definition = rules
            .ability(ability)
            .ok_or(GameError::Rule(RuleError::UnknownAbility))?;
        let modifiers = caster.derived().ability_values(ability);
        let weapon = self.selected_melee_attack(actor).ok_or_else(invalid)?;
        let (base_prepare, base_recover) = match definition.timing.source {
            TimingSource::Fixed { prepare, recover } => (prepare, recover),
            TimingSource::SelectedWeapon => (weapon.wind_up(), weapon.recovery()),
        };
        let duration = |base: u64, adjustment: i64| -> Result<u64, GameError> {
            let base = i128::from(base) + i128::from(adjustment);
            let base = u64::try_from(base.max(1)).map_err(|_| invalid())?;
            let duration = if let Some(attribute) = definition.timing.scaled_by {
                let numerator = u128::from(rules.constants().duration_scale_numerator);
                let divisor = u128::from(rules.constants().duration_scale_offset)
                    + u128::from(caster.derived().attributes.get(attribute));
                u64::try_from((u128::from(base) * numerator).div_ceil(divisor).max(1))
                    .map_err(|_| invalid())?
            } else {
                base
            };
            if !(1..=crate::limits::NUMERIC as u64).contains(&duration) {
                return Err(invalid());
            }
            Ok(duration)
        };
        Ok((
            duration(base_prepare, modifiers.preparation)?,
            duration(base_recover, modifiers.recovery)?,
        ))
    }
    /// Evaluates every immutable operand once. Resume retains this value.
    pub fn ability_plan(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        target: ActorId,
    ) -> Result<Plan, GameError> {
        self.plan_ability(actor, ability, target, None)
    }
    pub(crate) fn ability_plan_in_scene(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        target: ActorId,
        sight: &[tor_world::SightCell],
    ) -> Result<Plan, GameError> {
        self.plan_ability(actor, ability, target, Some(sight))
    }
    fn plan_ability(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        target: ActorId,
        sight: Option<&[tor_world::SightCell]>,
    ) -> Result<Plan, GameError> {
        crate::diagnostics::ability_plan();
        let caster = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let defender = self.creature(target).ok_or(GameError::UnknownActor)?;
        if caster.health().dead() || defender.health().dead() {
            return Err(GameError::Rule(RuleError::Dead));
        }
        let rules = caster.build().ruleset();
        let definition = rules
            .ability(ability)
            .ok_or(GameError::Rule(RuleError::UnknownAbility))?;
        if !caster.derived().abilities.contains(ability) {
            return Err(GameError::Rule(RuleError::UnknownAbility));
        }
        let modifiers = caster.derived().ability_values(ability);
        let (preparation, recovery) = self.ability_timing(actor, ability)?;
        let select_skill = |selector: &SkillSelector,
                            subject: ActorId|
         -> Result<crate::attributes::Skill, GameError> {
            match selector {
                SkillSelector::Named { skill } => Ok(*skill),
                SkillSelector::NaturalAttack => Ok(self
                    .creature(subject)
                    .ok_or(GameError::UnknownActor)?
                    .build()
                    .species()
                    .melee
                    .skill()),
                SkillSelector::SelectedWeapon => self
                    .selected_melee_attack(subject)
                    .map(|attack| attack.skill())
                    .ok_or_else(invalid),
            }
        };
        let skill_bonus = |selector: &SkillSelector, subject: ActorId| -> Result<i64, GameError> {
            let creature = self.creature(subject).ok_or(GameError::UnknownActor)?;
            let skill = select_skill(selector, subject)?;
            let modifier = if matches!(selector, SkillSelector::SelectedWeapon) {
                self.selected_melee_attack(subject)
                    .ok_or_else(invalid)?
                    .bonus()
            } else {
                0
            };
            Ok(crate::attributes::SkillCheck {
                skill,
                binding: creature.build().binding(),
                modifier,
                threshold: 0,
            }
            .bonus(creature.derived().attributes, creature.derived().skills))
        };
        let check = definition
            .check
            .as_ref()
            .map(|check| -> Result<PlannedCheck, GameError> {
                let subject = if check.actor == CheckActor::User {
                    actor
                } else {
                    target
                };
                let opposing = if check.actor == CheckActor::User {
                    target
                } else {
                    actor
                };
                let threshold = match &check.against {
                    Against::Defense { defense } => {
                        let defenses = self
                            .creature(opposing)
                            .ok_or(GameError::UnknownActor)?
                            .derived()
                            .defenses;
                        CheckThreshold::Fixed {
                            value: i64::from(match defense {
                                DefenseKind::Physical => defenses.physical,
                                DefenseKind::Cognitive => defenses.cognitive,
                                DefenseKind::Spiritual => defenses.spiritual,
                            }),
                        }
                    }
                    Against::Difficulty { formula } => {
                        let mut value = i64::from(formula.base) + i64::from(modifiers.difficulty);
                        if let Some(skill) = &formula.skill {
                            value += skill_bonus(skill, actor)?;
                        }
                        if let Some(attribute) = formula.attribute {
                            value += i64::from(caster.derived().attributes.get(attribute));
                        }
                        CheckThreshold::Fixed { value }
                    }
                    Against::Opposed { skill } => CheckThreshold::Opposed {
                        bonus: skill_bonus(skill, opposing)?,
                        edge: self
                            .creature(opposing)
                            .ok_or(GameError::UnknownActor)?
                            .derived()
                            .edge_against(subject)
                            .balance(),
                    },
                };
                Ok(PlannedCheck {
                    actor: check.actor,
                    bonus: skill_bonus(&check.skill, subject)? + i64::from(modifiers.check_bonus),
                    threshold,
                    edge: self
                        .creature(subject)
                        .ok_or(GameError::UnknownActor)?
                        .derived()
                        .edge_against(opposing)
                        .balance(),
                })
            })
            .transpose()?;
        let reach = caster
            .derived()
            .ability_reach(ability, rules)
            .map_err(|_| invalid())?;
        if !self.target_in_reach_snapshot(actor, target, &reach, sight) {
            return Err(GameError::Rule(RuleError::OutOfReach));
        }
        let plan = Plan {
            work: Work::UseAbility {
                id: ability.clone(),
                target,
            },
            resolution: ResolutionPlan {
                actor,
                target,
                ability: Some(ability.clone()),
                preparation,
                recovery,
                cost: self.ability_cost(ability)?,
                reach: Some(reach),
                check,
                on_success: self.compile_effects(actor, &definition.on_success, &modifiers)?,
                on_failure: self.compile_effects(actor, &definition.on_failure, &modifiers)?,
                descriptors: definition.descriptors.clone(),
                edge: caster.derived().edge_against(target).balance(),
            },
        };
        if !plan.valid(rules) {
            return Err(invalid());
        }
        Ok(plan)
    }
    pub(crate) fn compile_effects(
        &self,
        actor: ActorId,
        effects: &[Effect],
        modifiers: &crate::creatures::AbilityValues,
    ) -> Result<Vec<PlannedEffect>, GameError> {
        if effects.len() > crate::limits::EFFECTS_PER_BRANCH {
            return Err(invalid());
        }

        effects
            .iter()
            .map(|effect| match effect {
                Effect::Damage { damage } => {
                    let damage = match damage {
                        DamageExpr::SelectedWeapon { impact_bonus } => self
                            .selected_melee_attack(actor)
                            .ok_or_else(invalid)?
                            .damage_with_modifiers(modifiers.dice, modifiers.flat, *impact_bonus)
                            .map_err(|_| invalid())?,
                        DamageExpr::Components { components } => {
                            let mut values = Vec::new();
                            for component in components {
                                values.push(match &component.amount {
                                    AmountExpr::Fixed { amount } => DamageComponent::fixed(
                                        component.category,
                                        component.descriptor.clone(),
                                        *amount,
                                    ),
                                    AmountExpr::Rolled { dice } => DamageComponent::rolled(
                                        component.category,
                                        component.descriptor.clone(),
                                        DicePool::new(dice.count, dice.sides, dice.bonus)
                                            .and_then(|pool| {
                                                pool.augmented(modifiers.dice, modifiers.flat)
                                            })
                                            .map_err(|_| invalid())?,
                                    ),
                                });
                            }
                            let primary = values.first().map(|component| component.key());
                            DamageSpec::new(values, primary).map_err(|_| invalid())?
                        }
                    };
                    Ok(PlannedEffect::Damage { damage })
                }
                Effect::Heal { amount } => Ok(PlannedEffect::Heal {
                    amount: PlannedAmount::compile(amount)?,
                }),
                Effect::Restore { resource, amount } => Ok(PlannedEffect::Restore {
                    resource: *resource,
                    amount: PlannedAmount::compile(amount)?,
                }),
                Effect::ApplyCondition {
                    condition,
                    duration,
                } => Ok(PlannedEffect::ApplyCondition {
                    id: condition.clone(),
                    duration: duration
                        .checked_add(modifiers.duration)
                        .filter(|duration| *duration <= crate::limits::NUMERIC as u64)
                        .ok_or_else(invalid)?,
                }),
            })
            .collect()
    }
    pub(crate) fn effect_plan(
        &self,
        actor: ActorId,
        effects: &[Effect],
    ) -> Result<ResolutionPlan, GameError> {
        let plan = ResolutionPlan {
            actor,
            target: actor,
            ability: None,
            preparation: 1,
            recovery: 0,
            cost: None,
            reach: Some(Reach::SelfOnly),
            check: None,
            on_success: self.compile_effects(
                actor,
                effects,
                &crate::creatures::AbilityValues::default(),
            )?,
            on_failure: vec![],
            descriptors: vec![],
            edge: 0,
        };
        if !plan.valid(&self.content.rules) {
            return Err(invalid());
        }
        Ok(plan)
    }
    fn plan_actor_available(&self, plan: &Plan) -> bool {
        self.actors
            .get(&plan.actor)
            .is_some_and(|actor| actor.alive())
            && self.creature(plan.actor).map_or_else(
                || {
                    matches!(
                        plan.work,
                        Work::Equip { .. } | Work::Unequip { .. } | Work::Drink { .. }
                    ) && plan.actor == plan.target
                        && plan.on_success.is_empty()
                        && plan.on_failure.is_empty()
                },
                |creature| !creature.health().dead(),
            )
    }
    pub(crate) fn plan_still_available_in_scene(
        &self,
        plan: &Plan,
        sight: &[tor_world::SightCell],
    ) -> bool {
        (plan.work.item().is_none() || self.item_work_available(plan.actor, &plan.work))
            && self.plan_actor_available(plan)
            && plan.reach.as_ref().is_none_or(|reach| {
                self.target_in_reach_snapshot(plan.actor, plan.target, reach, Some(sight))
            })
    }
    pub(crate) fn plan_still_available(&self, plan: &Plan) -> bool {
        (plan.work.item().is_none() || self.item_work_available(plan.actor, &plan.work))
            && self.plan_actor_available(plan)
            && plan
                .reach
                .as_ref()
                .is_none_or(|reach| self.target_in_reach(plan.actor, plan.target, reach))
    }
    pub(crate) fn resolve_plan(
        &mut self,
        plan: &Plan,
        intention: Option<crate::IntentionId>,
    ) -> Result<Outcome, GameError> {
        if !self.plan_still_available(plan) {
            return Err(GameError::Rule(RuleError::OutOfReach));
        }
        let target = self.creature(plan.target).ok_or(GameError::UnknownActor)?;
        let snapshot = Snapshot::capture(&target);
        let mut rng = self.rules_rng;
        let mut outcome = if let Some(observer) = self
            .combat_diagnostics
            .as_mut()
            .and_then(|d| d.active.as_mut())
        {
            resolve(plan, &snapshot, &mut rng, observer)
        } else {
            resolve(plan, &snapshot, &mut rng, &mut ())
        };
        outcome.intention = intention;
        self.apply_outcome(outcome.clone())?;
        self.rules_rng = rng;
        Ok(outcome)
    }
    /// Validate all mutations on a candidate before committing this outcome and event.
    pub fn apply(&mut self, outcome: Outcome) -> Result<(), GameError> {
        let mut candidate = self.clone();
        candidate.apply_outcome(outcome)?;
        *self = candidate;
        Ok(())
    }

    /// Apply inside an enclosing transaction. Callers discard that transaction on error.
    pub(crate) fn apply_outcome(&mut self, outcome: Outcome) -> Result<(), GameError> {
        self.event_log.push(crate::combat::CombatEvent::Resolved {
            outcome: outcome.clone(),
        });
        for effect in &outcome.effects {
            match effect {
                ResolvedEffect::Damage { amount } => {
                    self.apply_injury(outcome.target, *amount)?;
                }
                ResolvedEffect::Heal { amount } => {
                    self.settle_actor_time(outcome.target)?;
                    self.actors
                        .get_mut(&outcome.target)
                        .and_then(|mut actor| {
                            actor.combat.as_mut().map(|combat| combat.heal(*amount))
                        })
                        .ok_or(GameError::UnknownActor)?;
                }
                ResolvedEffect::ApplyCondition { id, duration } => {
                    self.apply_condition(outcome.target, id, outcome.actor, *duration)
                        .map_err(|_| GameError::Rule(RuleError::ConditionCapacity))?;
                }
                ResolvedEffect::Restore { resource, amount } => {
                    self.restore_resource(outcome.target, *resource, *amount)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    fn snapshot() -> Snapshot {
        use crate::rules::{Modifier, ModifierOp, Scope, Stat};
        let mut species = crate::test_creatures::species();
        species.modifiers = vec![
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::Health,
                    value: 10,
                },
            },
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::ResourceMax {
                        resource: Resource::Stamina,
                    },
                    value: 10,
                },
            },
        ];
        Snapshot::capture(
            &crate::creatures::CreatureState::new(crate::test_creatures::build(species)).unwrap(),
        )
    }
    fn plan(effects: Vec<PlannedEffect>) -> ResolutionPlan {
        ResolutionPlan {
            actor: ActorId(1),
            target: ActorId(2),
            ability: None,
            preparation: 1,
            recovery: 0,
            cost: None,
            reach: None,
            check: None,
            on_success: effects,
            on_failure: vec![],
            descriptors: vec![],
            edge: 0,
        }
    }
    fn captured_plan(name: &str) -> (Game, Plan) {
        use crate::rules::{Modifier, ModifierOp, Scope};
        let id = AbilityId::new(name).unwrap();
        let mut species = crate::test_creatures::species();
        species.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: id.clone(),
            },
        });
        let build = crate::test_creatures::build(species);
        let mut game = Game::two_room_in_stone(42, build.content().clone());
        let mut actors = vec![];
        for x in [1, 2] {
            let actor = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    std::num::NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
            actors.push(actor);
        }
        let plan = game.ability_plan(actors[0], &id, actors[1]).unwrap();
        (game, plan)
    }

    fn fixed(amount: u32) -> PlannedAmount {
        PlannedAmount::Fixed { amount }
    }

    #[test]
    fn public_application_rolls_back_partial_effects_and_failed_death_after_settlement() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        game.apply_effects(
            actor,
            &[Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![crate::rules::DamageComponentExpr {
                        category: crate::combat::DamageType::Vital,
                        descriptor: None,
                        amount: AmountExpr::Fixed { amount: 1 },
                    }],
                },
            }],
        )
        .unwrap();
        let before = game.clone();
        assert!(game
            .apply(Outcome {
                actor,
                target: actor,
                ability: None,
                intention: None,
                result: OutcomeStatus::Applied,
                effects: vec![
                    ResolvedEffect::Heal { amount: 1 },
                    ResolvedEffect::ApplyCondition {
                        id: ConditionId::new("unknown_condition").unwrap(),
                        duration: 1
                    }
                ],
            })
            .is_err());
        assert_eq!(
            game, before,
            "failed second effect must roll back the first effect and event"
        );
        game.apply_condition(actor, &ConditionId::new("fear").unwrap(), actor, 1)
            .unwrap();
        game.tick += 2;
        game.next_item_id = u64::MAX;
        let before = game.clone();
        let lethal = Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![crate::rules::DamageComponentExpr {
                    category: crate::combat::DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Rolled {
                        dice: crate::rules::DiceExpr {
                            count: 1,
                            sides: 6,
                            bonus: 1000,
                        },
                    },
                }],
            },
        };
        assert_eq!(
            game.apply_effects(actor, &[lethal]),
            Err(GameError::IdentityExhausted)
        );
        assert_eq!(
            game, before,
            "failed death must roll back settlement, RNG, injury and events"
        );
    }

    #[test]
    fn final_plan_and_timing_bounds_reject_plus_one_without_mutation() {
        use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding};
        use crate::creatures::{Content, CreatureBuild, ScenarioCatalog};
        use crate::rules::{AbilityStat, Formula, Modifier, ModifierOp, Scope, Stat, Timing};
        for (preparation_adjustment, strength, timing_ok, plan_ok) in [
            (0, 0, true, true),
            (1, 0, false, false),
            (0, 1, true, false),
        ] {
            let mut document = crate::test_creatures::ruleset().document().clone();
            let mut definition = document
                .abilities
                .iter()
                .find(|value| value.basic_attack)
                .unwrap()
                .clone();
            definition.id = AbilityId::new("fixture_limits").unwrap();
            definition.basic_attack = false;
            definition.timing = Timing {
                source: TimingSource::Fixed {
                    prepare: crate::limits::NUMERIC as u64,
                    recover: 1,
                },
                scaled_by: None,
            };
            definition.check.as_mut().unwrap().against = Against::Difficulty {
                formula: Formula {
                    base: crate::limits::NUMERIC as i32,
                    skill: None,
                    attribute: Some(Attribute::Strength),
                },
            };
            let ability = definition.id.clone();
            document.abilities.push(definition);
            let rules = crate::rules::Ruleset::validate(document).unwrap();
            let mut species = crate::test_creatures::species();
            species.default_attributes = Attributes::new(ByAttribute {
                strength,
                ..ByAttribute::default()
            })
            .unwrap();
            species.modifiers.extend([
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::GrantAbility {
                        ability: ability.clone(),
                    },
                },
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Ability {
                            ability: ability.clone(),
                            stat: AbilityStat::Preparation,
                        },
                        value: preparation_adjustment,
                    },
                },
            ]);
            let species_id = species.id.clone();
            let content = Content::new(
                rules,
                ScenarioCatalog {
                    species: std::collections::BTreeMap::from([(species_id.clone(), species)]),
                    templates: Default::default(),
                },
            )
            .unwrap();
            let ledger = crate::test_creatures::build(crate::test_creatures::species())
                .ledger()
                .clone();
            let build =
                CreatureBuild::new(content.clone(), species_id, ledger, ManaBinding::Intellect)
                    .unwrap();
            let mut game = Game::two_room_in_stone(42, content);
            let mut actors = vec![];
            for x in [1, 2] {
                let actor = game
                    .spawn_actor(
                        tor_world::Location {
                            region: tor_world::RegionId(1),
                            position: tor_world::Position { x, y: 1, z: 0 },
                        },
                        std::num::NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                game.configure_creature(
                    actor,
                    crate::CreatureIdentity {
                        name: format!("subject {x}"),
                        faction: "neutral".into(),
                    },
                    build.clone(),
                )
                .unwrap();
                actors.push(actor);
            }
            let before = game.clone();
            assert_eq!(game.ability_timing(actors[0], &ability).is_ok(), timing_ok);
            let result = game.ability_plan(actors[0], &ability, actors[1]);
            assert_eq!(result.is_ok(), plan_ok);
            if let Ok(plan) = result {
                assert!(plan.valid(game.content().rules()));
            }
            assert_eq!(game, before);
        }
    }

    #[test]
    fn saved_ability_plan_requires_matching_bounded_reach() {
        let rules = crate::test_creatures::ruleset();
        for name in ["basic_melee", "magic_bolt"] {
            let (_, original) = captured_plan(name);
            assert!(original.valid(&rules));
            for reach in [
                serde_json::Value::Null,
                serde_json::json!({"kind":"sight","range":0}),
                serde_json::json!({"kind":"sight","range":u16::MAX}),
                serde_json::json!({"kind":"self_only"}),
            ] {
                let mut record = serde_json::to_value(&original).unwrap();
                record["resolution"]["reach"] = reach;
                let restored: Plan = serde_json::from_value(record).unwrap();
                assert!(!restored.valid(&rules));
            }
        }
        assert!(
            plan(vec![]).valid(&rules),
            "environmental effects do not require player reach"
        );
    }

    #[test]
    fn saved_paid_plan_cannot_remove_or_change_immutable_rule_contract() {
        let (game, original) = captured_plan("fear");
        let rules = game.content().rules();
        assert!(original.valid(rules));
        for case in 0..8 {
            let mut forged = original.clone();
            match case {
                0 => forged.resolution.cost = None,
                1 => forged.resolution.cost.as_mut().unwrap().resource = Resource::Stamina,
                2 => forged
                    .resolution
                    .descriptors
                    .push(DescriptorId::new("fire").unwrap()),
                3 => forged.resolution.check = None,
                4 => {
                    let check = forged.resolution.check.as_mut().unwrap();
                    check.actor = match check.actor {
                        CheckActor::User => CheckActor::Target,
                        CheckActor::Target => CheckActor::User,
                    };
                }
                5 => {
                    forged.resolution.check.as_mut().unwrap().threshold =
                        CheckThreshold::Opposed { bonus: 0, edge: 0 }
                }
                6 => forged.resolution.on_success.clear(),
                7 => forged.resolution.on_success[0] = PlannedEffect::Heal { amount: fixed(1) },
                _ => unreachable!(),
            }
            let restored: Plan =
                serde_json::from_value(serde_json::to_value(forged).unwrap()).unwrap();
            assert!(!restored.valid(rules), "forged contract case {case}");
        }
    }

    #[test]
    fn captured_component_damage_keeps_immutable_keys_and_primary_but_allows_derived_dice() {
        use crate::combat::DamageType;
        let fire = DescriptorId::new("fire").unwrap();
        let expected = vec![Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![
                    crate::rules::DamageComponentExpr {
                        category: DamageType::Energy,
                        descriptor: Some(fire.clone()),
                        amount: AmountExpr::Rolled {
                            dice: crate::rules::DiceExpr {
                                count: 1,
                                sides: 6,
                                bonus: 1,
                            },
                        },
                    },
                    crate::rules::DamageComponentExpr {
                        category: DamageType::Impact,
                        descriptor: None,
                        amount: AmountExpr::Fixed { amount: 3 },
                    },
                ],
            },
        }];
        for case in 0..7 {
            let first = if case == 4 {
                DamageComponent::fixed(DamageType::Energy, Some(fire.clone()), 3)
            } else {
                DamageComponent::rolled(
                    if case == 2 {
                        DamageType::Vital
                    } else {
                        DamageType::Energy
                    },
                    Some(if case == 1 {
                        DescriptorId::new("cold").unwrap()
                    } else {
                        fire.clone()
                    }),
                    DicePool::new(3, if case == 3 { 8 } else { 6 }, 5).unwrap(),
                )
            };
            let second =
                DamageComponent::fixed(DamageType::Impact, None, if case == 6 { 4 } else { 3 });
            let primary = if case == 5 { second.key() } else { first.key() };
            let damage = DamageSpec::new(vec![first, second], Some(primary)).unwrap();
            let saved = serde_json::to_value(PlannedEffect::Damage { damage }).unwrap();
            let restored: PlannedEffect = serde_json::from_value(saved).unwrap();
            assert_eq!(
                effects_match_definition(&expected, &[restored]),
                case == 0,
                "component contract case {case}"
            );
        }
    }

    #[test]
    fn captured_item_effects_reject_changed_duration_and_dice_operands() {
        use crate::combat::DamageType;
        let id = ConditionId::new("fear").unwrap();
        let expected = vec![
            Effect::ApplyCondition {
                condition: id.clone(),
                duration: 5,
            },
            Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![crate::rules::DamageComponentExpr {
                        category: DamageType::Vital,
                        descriptor: None,
                        amount: AmountExpr::Rolled {
                            dice: crate::rules::DiceExpr {
                                count: 1,
                                sides: 6,
                                bonus: 2,
                            },
                        },
                    }],
                },
            },
        ];
        for case in 0..4 {
            let component = DamageComponent::rolled(
                DamageType::Vital,
                None,
                DicePool::new(
                    if case == 2 { 2 } else { 1 },
                    6,
                    if case == 3 { 100 } else { 2 },
                )
                .unwrap(),
            );
            let primary = component.key();
            let effects = vec![
                PlannedEffect::ApplyCondition {
                    id: id.clone(),
                    duration: if case == 1 { 1_000_000 } else { 5 },
                },
                PlannedEffect::Damage {
                    damage: DamageSpec::new(vec![component], Some(primary)).unwrap(),
                },
            ];
            let restored: Vec<PlannedEffect> =
                serde_json::from_value(serde_json::to_value(effects).unwrap()).unwrap();
            assert!(effects_match_definition(&expected, &restored));
            assert_eq!(
                item_effects_match_definition(&expected, &restored),
                case == 0,
                "item numeric contract case {case}"
            );
        }
    }

    #[test]
    fn captured_damage_rejects_undeclared_descriptor_after_record_round_trip() {
        let snapshot = snapshot();
        let rules = snapshot.creature.build().ruleset();
        let damage = DamageSpec::new(
            vec![DamageComponent::fixed(
                crate::combat::DamageType::Energy,
                Some(DescriptorId::new("fire").unwrap()),
                3,
            )],
            None,
        )
        .unwrap();
        let original = plan(vec![PlannedEffect::Damage { damage }]);
        assert!(original.valid(rules));
        let mut saved = serde_json::to_value(&original).unwrap();
        let damage = &mut saved["on_success"][0]["damage"];
        damage["components"][0]["descriptor"] = serde_json::json!("undeclared_energy");
        let restored: ResolutionPlan = serde_json::from_value(saved).unwrap();
        assert!(!restored.valid(rules));
    }

    #[test]
    fn pure_resolver_threshold_has_no_automatic_extremes() {
        for (roll, bonus, threshold, expected) in [
            (1, 9, 10, OutcomeStatus::Applied),
            (20, 0, 21, OutcomeStatus::Miss),
            (9, 0, 10, OutcomeStatus::Miss),
            (10, 0, 10, OutcomeStatus::Applied),
        ] {
            let mut rng = (0..10_000_u64)
                .find(|seed| {
                    let mut state = *seed;
                    roll_check(&mut state, Edge::default()).kept == roll
                })
                .expect("bounded deterministic seeds cover each d20 result");
            let mut snapshot = snapshot();
            snapshot.creature.damage(1);
            let mut plan = plan(vec![PlannedEffect::Heal { amount: fixed(1) }]);
            plan.check = Some(PlannedCheck {
                actor: CheckActor::User,
                bonus,
                threshold: CheckThreshold::Fixed { value: threshold },
                edge: 0,
            });
            let mut trace = crate::resolution_diagnostics::ResolutionTrace::default();
            let outcome = resolve(&plan, &snapshot, &mut rng, &mut trace);
            assert_eq!(outcome.result, expected, "natural roll {roll}");
            assert!(trace.steps().iter().any(|step| step.id == "check"
                && step.fields.iter().any(|field| field.id == "roll"
                    && field.value
                        == crate::resolution_diagnostics::TraceValueRecord::Unsigned(u64::from(
                            roll
                        )))));
        }
    }

    #[test]
    fn opposed_check_trace_retains_actual_dice_and_does_not_change_resolution() {
        use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
        let mut snapshot = snapshot();
        snapshot.creature.damage(1);
        let mut plan = plan(vec![PlannedEffect::Heal { amount: fixed(1) }]);
        plan.check = Some(PlannedCheck {
            actor: CheckActor::User,
            bonus: 20,
            threshold: CheckThreshold::Opposed { bonus: 3, edge: 1 },
            edge: 0,
        });
        let (mut ordinary_rng, mut observed_rng) = (7, 7);
        let ordinary = resolve(&plan, &snapshot, &mut ordinary_rng, &mut ());
        let mut trace = ResolutionTrace::default();
        let observed = resolve(&plan, &snapshot, &mut observed_rng, &mut trace);
        assert_eq!((ordinary, ordinary_rng), (observed, observed_rng));
        let opposed = trace
            .steps()
            .iter()
            .find(|step| step.id == "opposed_check")
            .unwrap();
        let field = |id| {
            &opposed
                .fields
                .iter()
                .find(|field| field.id == id)
                .unwrap()
                .value
        };
        let TraceValueRecord::Dice(draws) = field("rolled") else {
            panic!("actual opposed draws")
        };
        assert_eq!(draws.len(), 2);
        assert_eq!(
            field("kept"),
            &TraceValueRecord::Unsigned(u64::from(*draws.iter().max().unwrap()))
        );
        let total = field("total");
        assert!(trace
            .steps()
            .iter()
            .find(|step| step.id == "check")
            .unwrap()
            .fields
            .iter()
            .any(|field| field.id == "threshold" && &field.value == total));
    }

    #[test]
    fn target_opposed_check_spends_caster_edge_before_either_damage_branch() {
        use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
        for bonus in [-100, 100] {
            let damage = DamageSpec::new(
                vec![DamageComponent::rolled(
                    crate::combat::DamageType::Impact,
                    None,
                    DicePool::new(1, 6, 0).unwrap(),
                )],
                None,
            )
            .unwrap();
            let mut plan = plan(vec![PlannedEffect::Damage {
                damage: damage.clone(),
            }]);
            plan.on_failure = vec![PlannedEffect::Damage { damage }];
            plan.edge = 1;
            plan.check = Some(PlannedCheck {
                actor: CheckActor::Target,
                bonus,
                threshold: CheckThreshold::Opposed { bonus: 0, edge: 1 },
                edge: 0,
            });
            let snapshot = snapshot();
            let (mut ordinary_rng, mut observed_rng) = (7, 7);
            let ordinary = resolve(&plan, &snapshot, &mut ordinary_rng, &mut ());
            let mut trace = ResolutionTrace::default();
            let observed = resolve(&plan, &snapshot, &mut observed_rng, &mut trace);
            assert_eq!((ordinary, ordinary_rng), (observed, observed_rng));
            let opposed = trace
                .steps()
                .iter()
                .find(|step| step.id == "opposed_check")
                .unwrap();
            assert!(opposed.fields.iter().any(|field| field.id == "rolled"
                && matches!(&field.value, TraceValueRecord::Dice(values) if values.len() == 2)));
            let damage = trace
                .steps()
                .iter()
                .find(|step| step.id == "damage.component")
                .unwrap();
            assert!(damage.fields.iter().any(|field| field.id == "rolled"
                && matches!(&field.value, TraceValueRecord::Dice(values) if values.len() == 1)));
            assert!(damage
                .fields
                .iter()
                .any(|field| field.id == "edge" && field.value == TraceValueRecord::Signed(0)));
        }
    }

    #[test]
    fn fixed_damage_preserves_edge_and_rolled_pools_return_the_unused_budget() {
        use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
        let rolled = DamageSpec::new(
            vec![DamageComponent::rolled(
                crate::combat::DamageType::Impact,
                None,
                DicePool::new(1, 6, 0).unwrap(),
            )],
            None,
        )
        .unwrap();
        let fixed_zero = DamageSpec::new(
            vec![DamageComponent::fixed(
                crate::combat::DamageType::Impact,
                None,
                0,
            )],
            None,
        )
        .unwrap();
        for edge in [-1, 1] {
            let mut plain = plan(vec![PlannedEffect::Damage {
                damage: rolled.clone(),
            }]);
            plain.edge = edge;
            let mut with_zero = plain.clone();
            with_zero.on_success.insert(
                0,
                PlannedEffect::Damage {
                    damage: fixed_zero.clone(),
                },
            );
            let (mut ordinary_rng, mut observed_rng) = (7, 7);
            let baseline = resolve(&plain, &snapshot(), &mut ordinary_rng, &mut ());
            let observed = resolve(
                &with_zero,
                &snapshot(),
                &mut observed_rng,
                &mut ResolutionTrace::default(),
            );
            assert_eq!(ordinary_rng, observed_rng);
            assert_eq!(baseline.result, observed.result);
            assert_eq!(observed.effects[0], ResolvedEffect::Damage { amount: 0 });
            assert_eq!(baseline.effects, observed.effects[1..]);
        }
        for edge in [-2, 2] {
            let mut plan = plan(vec![
                PlannedEffect::Damage {
                    damage: rolled.clone(),
                },
                PlannedEffect::Damage {
                    damage: rolled.clone(),
                },
            ]);
            plan.edge = edge;
            let mut trace = ResolutionTrace::default();
            let (mut ordinary_rng, mut observed_rng) = (7, 7);
            let ordinary = resolve(&plan, &snapshot(), &mut ordinary_rng, &mut ());
            let observed = resolve(&plan, &snapshot(), &mut observed_rng, &mut trace);
            assert_eq!((ordinary, ordinary_rng), (observed, observed_rng));
            let draw_counts: Vec<_> = trace
                .steps()
                .iter()
                .filter(|step| step.id == "damage.component")
                .map(|step| {
                    match &step
                        .fields
                        .iter()
                        .find(|field| field.id == "rolled")
                        .unwrap()
                        .value
                    {
                        TraceValueRecord::Dice(values) => values.len(),
                        _ => panic!("damage draws"),
                    }
                })
                .collect();
            assert_eq!(
                draw_counts,
                [2, 2],
                "leftover edge {edge} reaches the second one-die pool"
            );
        }
    }

    #[test]
    fn nonweapon_effects_compile_without_a_creature_or_selected_weapon() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        assert!(game.creature(actor).is_none());
        let effects = game
            .compile_effects(
                actor,
                &[
                    Effect::Heal {
                        amount: AmountExpr::Fixed { amount: 3 },
                    },
                    Effect::Restore {
                        resource: Resource::Stamina,
                        amount: AmountExpr::Fixed { amount: 4 },
                    },
                ],
                &crate::creatures::AbilityValues::default(),
            )
            .unwrap();
        assert_eq!(
            effects,
            vec![
                PlannedEffect::Heal { amount: fixed(3) },
                PlannedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: fixed(4)
                }
            ]
        );
    }

    #[test]
    fn full_health_and_resources_report_no_applied_change() {
        let result = resolve(
            &plan(vec![
                PlannedEffect::Heal { amount: fixed(100) },
                PlannedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: fixed(100),
                },
            ]),
            &snapshot(),
            &mut 7,
            &mut (),
        );
        assert_eq!(result.result, OutcomeStatus::Unaffected);
        assert_eq!(
            result.effects,
            vec![
                ResolvedEffect::Heal { amount: 0 },
                ResolvedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: 0
                }
            ]
        );
    }

    #[test]
    fn repeated_effects_report_sequential_actual_healing_and_restoration() {
        let mut snapshot = snapshot();
        snapshot.creature.damage(7);
        snapshot
            .creature
            .start_cost(
                crate::IntentionId(1),
                ResourceCost {
                    resource: Resource::Stamina,
                    start: 3,
                    resolution: 0,
                },
            )
            .unwrap();
        let result = resolve(
            &plan(vec![
                PlannedEffect::Heal { amount: fixed(5) },
                PlannedEffect::Heal { amount: fixed(5) },
                PlannedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: fixed(2),
                },
                PlannedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: fixed(2),
                },
            ]),
            &snapshot,
            &mut 7,
            &mut (),
        );
        assert_eq!(result.result, OutcomeStatus::Applied);
        assert_eq!(
            result.effects,
            vec![
                ResolvedEffect::Heal { amount: 5 },
                ResolvedEffect::Heal { amount: 2 },
                ResolvedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: 2
                },
                ResolvedEffect::Restore {
                    resource: Resource::Stamina,
                    amount: 1
                }
            ]
        );
    }

    #[test]
    fn unchanged_condition_refresh_reports_no_applied_change() {
        let mut snapshot = snapshot();
        let id = ConditionId::new("fear").unwrap();
        snapshot
            .creature
            .apply_condition(&id, ActorId(1), 100)
            .unwrap();
        let before = snapshot.clone();
        let result = resolve(
            &plan(vec![PlannedEffect::ApplyCondition { id, duration: 100 }]),
            &snapshot,
            &mut 7,
            &mut (),
        );
        assert_eq!(result.result, OutcomeStatus::Unaffected);
        assert!(result.effects.is_empty());
        assert_eq!(snapshot, before);
    }

    #[test]
    fn lethal_damage_clamps_overkill_and_stops_later_effects_and_rng_draws() {
        let mut snapshot = snapshot();
        snapshot.creature.damage(7);
        let damage = DamageSpec::new(
            vec![DamageComponent::fixed(
                crate::combat::DamageType::Impact,
                None,
                100,
            )],
            None,
        )
        .unwrap();
        let mut rng = 7;
        let result = resolve(
            &plan(vec![
                PlannedEffect::Damage { damage },
                PlannedEffect::Heal {
                    amount: PlannedAmount::Rolled {
                        dice: DicePool::new(1, 6, 0).unwrap(),
                    },
                },
            ]),
            &snapshot,
            &mut rng,
            &mut (),
        );
        assert_eq!(result.result, OutcomeStatus::Applied);
        assert_eq!(result.effects, vec![ResolvedEffect::Damage { amount: 3 }]);
        assert_eq!(rng, 7);
    }
}
