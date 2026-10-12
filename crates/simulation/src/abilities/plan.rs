//! Compile and validate owned action-specific plans; never resolve effects here.
use super::ResolutionPlan;
use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
use crate::costs::ResourceCost;
use crate::damage::{DamageComponent, DamageSpec};
use crate::dice::DicePool;
use crate::resources::Resource;
use crate::rules::{
    Against, AmountExpr, CheckActor, DamageExpr, DefenseKind, Effect, Reach, SkillSelector,
};
use crate::{ActorId, EquipmentSlotId, Game, GameError, ItemId, Work};
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
/// A prepared action carries only the data its action kind can use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Plan {
    Ability(AbilityPlan),
    Equip {
        actor: ActorId,
        item: ItemId,
        slot: EquipmentSlotId,
        preparation: u64,
    },
    Unequip {
        actor: ActorId,
        item: ItemId,
        preparation: u64,
    },
    Drink {
        actor: ActorId,
        item: ItemId,
        preparation: u64,
        effects: Vec<PlannedEffect>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityPlan {
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: AbilityId,
    pub preparation: u64,
    pub recovery: u64,
    pub cost: Option<ResourceCost>,
    pub reach: Reach,
    pub check: Option<PlannedCheck>,
    pub on_success: Vec<PlannedEffect>,
    pub on_failure: Vec<PlannedEffect>,
    pub descriptors: Vec<DescriptorId>,
    pub edge: i64,
    #[serde(deserialize_with = "read_tokens")]
    pub tokens: Vec<crate::conditions::TokenReservation>,
}
fn read_tokens<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<crate::conditions::TokenReservation>, D::Error> {
    crate::bounded::Bounded::<crate::conditions::TokenReservation, { 2 * crate::limits::CONDITIONS }>::deserialize(deserializer).map(|bounded| bounded.0)
}
impl Plan {
    pub fn tokens(&self) -> &[crate::conditions::TokenReservation] {
        match self {
            Self::Ability(plan) => &plan.tokens,
            _ => &[],
        }
    }
    pub fn actor(&self) -> ActorId {
        match self {
            Self::Ability(plan) => plan.actor,
            Self::Equip { actor, .. } | Self::Unequip { actor, .. } | Self::Drink { actor, .. } => {
                *actor
            }
        }
    }
    pub fn target(&self) -> ActorId {
        match self {
            Self::Ability(plan) => plan.target,
            Self::Equip { actor, .. } | Self::Unequip { actor, .. } | Self::Drink { actor, .. } => {
                *actor
            }
        }
    }
    pub fn work(&self) -> Work {
        match self {
            Self::Ability(plan) => Work::UseAbility {
                id: plan.ability.clone(),
                target: plan.target,
            },
            Self::Equip { item, slot, .. } => Work::Equip {
                item: *item,
                slot: *slot,
            },
            Self::Unequip { item, .. } => Work::Unequip { item: *item },
            Self::Drink { item, .. } => Work::Drink { item: *item },
        }
    }
    pub fn preparation(&self) -> u64 {
        match self {
            Self::Ability(plan) => plan.preparation,
            Self::Equip { preparation, .. }
            | Self::Unequip { preparation, .. }
            | Self::Drink { preparation, .. } => *preparation,
        }
    }
    pub fn recovery(&self) -> u64 {
        match self {
            Self::Ability(plan) => plan.recovery,
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } => 0,
        }
    }
    pub fn cost(&self) -> Option<ResourceCost> {
        match self {
            Self::Ability(plan) => plan.cost,
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } => None,
        }
    }
    pub fn on_success(&self) -> &[PlannedEffect] {
        match self {
            Self::Ability(plan) => &plan.on_success,
            Self::Drink { effects, .. } => effects,
            Self::Equip { .. } | Self::Unequip { .. } => &[],
        }
    }
    pub fn as_ability(&self) -> Option<&AbilityPlan> {
        match self {
            Self::Ability(plan) => Some(plan),
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } => None,
        }
    }
    /// Borrows frozen operands without constructing a second owned plan.
    pub fn resolution(&self) -> Option<ResolutionPlan<'_>> {
        match self {
            Self::Ability(plan) => Some(ResolutionPlan::Ability(plan)),
            Self::Drink { actor, effects, .. } => Some(ResolutionPlan::Effects {
                actor: *actor,
                target: *actor,
                effects,
            }),
            Self::Equip { .. } | Self::Unequip { .. } => None,
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        valid_actor(self.actor())
            && (1..=crate::limits::NUMERIC as u64).contains(&self.preparation())
            && match self {
                Self::Ability(plan) => plan.valid(rules),
                Self::Equip { item, slot, .. } => {
                    valid_item(*item) && usize::from(slot.0) < crate::limits::EQUIPMENT_SOCKETS
                }
                Self::Unequip { item, .. } => valid_item(*item),
                Self::Drink { item, effects, .. } => {
                    valid_item(*item) && effects_valid(effects, rules)
                }
            }
    }
}
fn valid_actor(id: ActorId) -> bool {
    id.0 > 0 && id.0 != u64::MAX
}
fn valid_item(id: ItemId) -> bool {
    id.0 > 0 && id.0 != u64::MAX
}
pub(crate) fn effects_valid(effects: &[PlannedEffect], rules: &crate::rules::Ruleset) -> bool {
    effects.len() <= crate::limits::EFFECTS_PER_BRANCH
        && effects.iter().all(|effect| match effect {
            PlannedEffect::Damage { damage } => rules.validate_damage(damage).is_ok(),
            PlannedEffect::Heal { amount } | PlannedEffect::Restore { amount, .. } => {
                match amount {
                    PlannedAmount::Fixed { amount } => *amount <= crate::limits::NUMERIC as u32,
                    PlannedAmount::Rolled { .. } => true,
                }
            }
            PlannedEffect::ApplyCondition { id, duration } => {
                rules.condition(id).is_some()
                    && (1..=crate::limits::NUMERIC as u64).contains(duration)
            }
        })
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

impl AbilityPlan {
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
        (self.check.is_some() || self.tokens.is_empty())
            && self.tokens.len() <= 2 * crate::limits::CONDITIONS
            && self
                .tokens
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.tokens.len()
            && self.tokens.iter().all(|token| {
                (token.holder == self.actor || token.holder == self.target)
                    && valid_actor(token.source)
                    && rules.condition(&token.condition).is_some_and(|def| {
                        self.check.as_ref().is_some_and(|check| {
                            let roller = if check.actor == CheckActor::User {
                                self.actor
                            } else {
                                self.target
                            };
                            let opponent = if check.actor == CheckActor::User {
                                self.target
                            } else {
                                self.actor
                            };
                            let opposed = matches!(check.threshold, CheckThreshold::Opposed { .. });
                            match def.lifecycle {
                                crate::rules::ConditionLifecycle::Timed => false,
                                crate::rules::ConditionLifecycle::NextCheck => {
                                    token.holder == roller || opposed
                                }
                                crate::rules::ConditionLifecycle::NextCheckAgainst => {
                                    token.holder == opponent || opposed
                                }
                            }
                        })
                    })
            })
            && valid_actor(self.actor)
            && valid_actor(self.target)
            && (1..=crate::limits::NUMERIC as u64).contains(&self.preparation)
            && self.recovery <= crate::limits::NUMERIC as u64
            && rules.ability(&self.ability).is_some_and(|definition| {
                self.matches_definition(definition)
                    && match definition.reach {
                        Reach::SelfOnly => self.actor == self.target,
                        _ => self.actor != self.target,
                    }
                    && matches!(
                        (&definition.reach, &self.reach),
                        (Reach::Melee, Reach::Melee)
                            | (Reach::Sight { .. }, Reach::Sight { .. })
                            | (Reach::SelfOnly, Reach::SelfOnly)
                    )
            })
            && self.cost.is_none_or(|cost| {
                cost.start <= crate::limits::NUMERIC as u32
                    && cost.resolution <= crate::limits::NUMERIC as u32
            })
            && match self.reach {
                Reach::Sight { range } => (1..=crate::limits::SIGHT_RANGE).contains(&range),
                Reach::Melee | Reach::SelfOnly => true,
            }
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
            && effects_valid(&self.on_success, rules)
            && effects_valid(&self.on_failure, rules)
    }
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
    pub(super) fn target_in_reach_snapshot(
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
    ) -> Result<crate::creatures::AbilityTiming, GameError> {
        self.creature(actor)
            .ok_or(GameError::UnknownActor)?
            .derived()
            .ability_timings
            .get(ability)
            .copied()
            .ok_or(GameError::Rule(RuleError::UnknownAbility))
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
        let crate::creatures::AbilityTiming {
            preparation,
            recovery,
        } = self.ability_timing(actor, ability)?;
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
            if matches!(selector, SkillSelector::SelectedWeapon) {
                return Ok(creature.derived().selected_attack.check_bonus);
            }
            Ok(crate::attributes::SkillCheck {
                skill,
                binding: creature.build().binding(),
                modifier: 0,
                threshold: 0,
            }
            .bonus(creature.derived().attributes, creature.derived().skills))
        };
        let mut tokens = std::collections::BTreeSet::new();
        let mut check_edge = |subject: ActorId,
                              opposing: ActorId,
                              defense: Option<DefenseKind>|
         -> Result<i64, GameError> {
            let roller = self.creature(subject).ok_or(GameError::UnknownActor)?;
            let recipient = self.creature(opposing).ok_or(GameError::UnknownActor)?;
            tokens.extend(roller.conditions().qualifying_tokens(rules, subject, false));
            tokens.extend(
                recipient
                    .conditions()
                    .qualifying_tokens(rules, opposing, true),
            );
            roller
                .derived()
                .check_edge_with(
                    recipient.derived(),
                    rules,
                    crate::creatures::CheckContext {
                        ability,
                        roller: subject,
                        opponent: opposing,
                        defense,
                    },
                )
                .map_err(|_| invalid())
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
                        edge: check_edge(opposing, subject, None)?,
                    },
                };
                Ok(PlannedCheck {
                    actor: check.actor,
                    bonus: skill_bonus(&check.skill, subject)? + i64::from(modifiers.check_bonus),
                    threshold,
                    edge: check_edge(
                        subject,
                        opposing,
                        match check.against {
                            Against::Defense { defense } => Some(defense),
                            _ => None,
                        },
                    )?,
                })
            })
            .transpose()?;
        let reach = caster
            .derived()
            .ability_reach(ability)
            .map_err(|_| invalid())?;
        if !self.target_in_reach_snapshot(actor, target, &reach, sight) {
            return Err(GameError::Rule(RuleError::OutOfReach));
        }
        let plan = Plan::Ability(AbilityPlan {
            actor,
            target,
            ability: ability.clone(),
            preparation,
            recovery,
            cost: self.ability_cost(ability)?,
            reach,
            check,
            on_success: self.compile_effects(actor, &definition.on_success, &modifiers)?,
            on_failure: self.compile_effects(actor, &definition.on_failure, &modifiers)?,
            descriptors: definition.descriptors.clone(),
            edge: caster
                .derived()
                .effect_edge_against(rules, target)
                .map_err(|_| invalid())?,
            tokens: tokens.into_iter().collect(),
        });
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
    ) -> Result<Vec<PlannedEffect>, GameError> {
        let effects =
            self.compile_effects(actor, effects, &crate::creatures::AbilityValues::default())?;
        if !effects_valid(&effects, &self.content.rules) {
            return Err(invalid());
        }
        Ok(effects)
    }
}
