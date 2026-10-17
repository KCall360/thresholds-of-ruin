//! Compile and validate owned action-specific plans; never resolve effects here.
use super::{FrozenStrike, ResolutionPlan, TokenClaim};
use crate::content_ids::{AbilityId, ConditionId};
use crate::costs::ResourceCost;
use crate::damage::{DamageComponent, DamageSpec};
use crate::dice::DicePool;
use crate::resources::Resource;
use crate::rules::{AllyRange, AmountExpr, CheckActor, DamageExpr, Effect, Reach};
use crate::{ActorId, EquipmentSlotId, Game, GameError, ItemId, Work};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    UnknownAbility,
    Dead,
    OutOfReach,
    Unfunded,
    CoolingDown,
    InvalidDefinition,
    InvalidPlan,
    ConditionCapacity,
}
fn invalid() -> GameError {
    GameError::Rule(RuleError::InvalidDefinition)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckThreshold {
    Fixed { value: i64 },
    Opposed { bonus: i64, edge: i64 },
}
impl<'de> Deserialize<'de> for CheckThreshold {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Fixed { value: i64 },
            Opposed { bonus: i64, edge: i64 },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Fixed { value } => Self::Fixed { value },
            Fields::Opposed { bonus, edge } => Self::Opposed { bonus, edge },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedCheck {
    pub actor: CheckActor,
    pub target_participates: bool,
    pub bonus: i64,
    pub threshold: CheckThreshold,
    pub edge: i64,
}
impl<'de> Deserialize<'de> for PlannedCheck {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: CheckActor,
            target_participates: bool,
            bonus: i64,
            threshold: CheckThreshold,
            edge: i64,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if !fields.target_participates
            && (fields.actor == CheckActor::Target
                || matches!(fields.threshold, CheckThreshold::Opposed { .. }))
        {
            return Err(serde::de::Error::custom("inconsistent check participation"));
        }
        Ok(Self {
            actor: fields.actor,
            target_participates: fields.target_participates,
            bonus: fields.bonus,
            threshold: fields.threshold,
            edge: fields.edge,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedEffect {
    ApplyLinkedHold {
        policy: crate::linked_hold::HoldPolicy,
    },
    HoldDamage {
        category: crate::combat::DamageType,
        descriptor: Option<crate::content_ids::DescriptorId>,
    },
    DrainLatestHitDie,
    RestoreHitDie,
    StepSelfOneCell,
    PushTargetOneCell {
        direction: super::PushDirection,
    },
    Damage {
        damage: DamageSpec,
    },
    Heal {
        amount: PlannedAmount,
    },
    Nourish {
        amount: PlannedAmount,
    },
    LoseAttribute {
        attribute: crate::attributes::Attribute,
        amount: PlannedAmount,
    },
    RestoreAttribute {
        attribute: crate::attributes::Attribute,
        amount: PlannedAmount,
    },
    LoseMaximumHealth {
        amount: PlannedAmount,
    },
    RestoreMaximumHealth {
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
    ObservePublicPlan,
    ApplyQualifiedConditionToSource {
        id: ConditionId,
        duration: u64,
        ability_tag: crate::content_ids::AbilityTagId,
    },
}
impl PlannedEffect {
    pub(crate) fn recipient(&self) -> crate::rules::EffectRecipient {
        match self {
            Self::ObservePublicPlan | Self::ApplyQualifiedConditionToSource { .. } => {
                crate::rules::EffectRecipient::Source
            }
            _ => crate::rules::EffectRecipient::Target,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlannedAmount {
    Fixed { amount: u32 },
    Rolled { dice: DicePool },
}
impl<'de> Deserialize<'de> for PlannedAmount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Fixed {
                amount: u32,
            },
            Rolled {
                #[serde(deserialize_with = "crate::bounded::named_map")]
                dice: DicePool,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Fixed { amount } => Self::Fixed { amount },
            Fields::Rolled { dice } => Self::Rolled { dice },
        })
    }
}
impl PlannedAmount {
    pub(crate) fn strictly_positive(&self) -> bool {
        match self {
            Self::Fixed { amount } => *amount > 0,
            Self::Rolled { dice } => dice.minimum() > 0,
        }
    }

    pub(crate) fn compile(amount: &AmountExpr) -> Result<Self, GameError> {
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
    Escape(tor_world::Shared<crate::escape::EscapePlan>),
    Ability(tor_world::Shared<AbilityPlan>),
    Respec(tor_world::Shared<crate::player_respec::RespecPlan>),
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
    Eat {
        actor: ActorId,
        item: ItemId,
        preparation: u64,
        effects: Vec<PlannedEffect>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityPlan {
    pub actor: ActorId,
    pub invocation: super::AbilityInvocation,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub step: Option<super::StepCommitment>,
    pub target: ActorId,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub initial_hold: Option<crate::linked_hold::HoldId>,
    pub hold_damage_increment: u32,
    pub public_aim: crate::public_plan::PublicAim,
    pub ability: AbilityId,
    pub preparation: u64,
    pub recovery: u64,
    pub cost: Option<ResourceCost>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub cooldown: Option<std::num::NonZeroU64>,
    #[serde(deserialize_with = "crate::rules::read_supply_costs")]
    pub supply_costs: Vec<crate::rules::SupplyCost>,
    pub supplies: crate::supplies::SupplySelection,
    pub reach: Reach,
    #[serde(deserialize_with = "super::strikes::read_strikes")]
    pub strikes: Vec<FrozenStrike>,
    #[serde(deserialize_with = "super::strikes::read_claims")]
    pub tokens: Vec<TokenClaim>,
}
impl<'de> serde::Deserialize<'de> for AbilityPlan {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            invocation: super::AbilityInvocation,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            step: Option<super::StepCommitment>,
            target: ActorId,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            initial_hold: Option<crate::linked_hold::HoldId>,
            hold_damage_increment: u32,
            public_aim: crate::public_plan::PublicAim,
            ability: AbilityId,
            preparation: u64,
            recovery: u64,
            cost: Option<ResourceCost>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            cooldown: Option<std::num::NonZeroU64>,
            #[serde(deserialize_with = "crate::rules::read_supply_costs")]
            supply_costs: Vec<crate::rules::SupplyCost>,
            supplies: crate::supplies::SupplySelection,
            reach: Reach,
            #[serde(deserialize_with = "super::strikes::read_strikes")]
            strikes: Vec<FrozenStrike>,
            #[serde(deserialize_with = "super::strikes::read_claims")]
            tokens: Vec<TokenClaim>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            actor: fields.actor,
            invocation: fields.invocation,
            step: fields.step,
            target: fields.target,
            initial_hold: fields.initial_hold,
            hold_damage_increment: fields.hold_damage_increment,
            public_aim: fields.public_aim,
            ability: fields.ability,
            preparation: fields.preparation,
            recovery: fields.recovery,
            cost: fields.cost,
            cooldown: fields.cooldown,
            supply_costs: fields.supply_costs,
            supplies: fields.supplies,
            reach: fields.reach,
            strikes: fields.strikes,
            tokens: fields.tokens,
        })
    }
}

impl Plan {
    pub fn tokens(
        &self,
    ) -> impl ExactSizeIterator<Item = &crate::conditions::TokenReservation> + Clone {
        match self {
            Self::Ability(plan) => plan.tokens.as_slice(),
            Self::Escape(plan) => plan.tokens.as_slice(),
            _ => &[],
        }
            .iter()
            .map(|claim| &claim.reservation)
    }
    pub fn actor(&self) -> ActorId {
        match self {
            Self::Escape(plan) => plan.actor,
            Self::Ability(plan) => plan.actor,
            Self::Respec(plan) => plan.basis.actor,
            Self::Equip { actor, .. }
            | Self::Unequip { actor, .. }
            | Self::Drink { actor, .. }
            | Self::Eat { actor, .. } => *actor,
        }
    }
    pub fn target(&self) -> ActorId {
        match self {
            Self::Escape(plan) => plan.holder,
            Self::Ability(plan) => plan.target,
            Self::Respec(plan) => plan.basis.actor,
            Self::Equip { actor, .. }
            | Self::Unequip { actor, .. }
            | Self::Drink { actor, .. }
            | Self::Eat { actor, .. } => *actor,
        }
    }
    pub fn work(&self) -> Work {
        match self {
            Self::Escape(plan) => Work::Escape { hold: plan.hold },
            Self::Respec(plan) => Work::Respec {
                basis: plan.basis.clone(),
                replacement: plan.replacement.clone(),
                supplies: plan.supplies.clone(),
            },
            Self::Ability(plan) => Work::UseAbility {
                id: plan.ability.clone(),
                invocation: plan.invocation,
                supplies: plan.supplies.clone(),
            },
            Self::Equip { item, slot, .. } => Work::Equip {
                item: *item,
                slot: *slot,
            },
            Self::Unequip { item, .. } => Work::Unequip { item: *item },
            Self::Drink { item, .. } => Work::Drink { item: *item },
            Self::Eat { item, .. } => Work::Eat { item: *item },
        }
    }
    pub fn preparation(&self) -> u64 {
        match self {
            Self::Escape(plan) => plan.preparation,
            Self::Ability(plan) => plan.preparation,
            Self::Respec(plan) => plan.preparation,
            Self::Equip { preparation, .. }
            | Self::Unequip { preparation, .. }
            | Self::Drink { preparation, .. }
            | Self::Eat { preparation, .. } => *preparation,
        }
    }
    pub fn recovery(&self) -> u64 {
        match self {
            Self::Escape(plan) => plan.recovery,
            Self::Ability(plan) => plan.recovery + plan.step.map_or(0, |step| step.duration),
            Self::Respec(plan) => plan.recovery,
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } | Self::Eat { .. } => 0,
        }
    }
    pub fn cost(&self) -> Option<ResourceCost> {
        match self {
            Self::Ability(plan) => plan.cost,
            Self::Escape(_) | Self::Respec(_) => None,
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } | Self::Eat { .. } => {
                None
            }
        }
    }
    pub fn on_success(&self) -> &[PlannedEffect] {
        match self {
            Self::Ability(plan) => plan
                .strikes
                .first()
                .map_or(&[], |strike| strike.on_success.as_slice()),
            Self::Drink { effects, .. } | Self::Eat { effects, .. } => effects,
            Self::Escape(_) | Self::Respec(_) | Self::Equip { .. } | Self::Unequip { .. } => &[],
        }
    }
    pub fn as_ability(&self) -> Option<&AbilityPlan> {
        match self {
            Self::Ability(plan) => Some(plan),
            Self::Escape(_) | Self::Respec(_) => None,
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } | Self::Eat { .. } => {
                None
            }
        }
    }
    /// Borrows frozen operands without constructing a second owned plan.
    pub fn resolution(&self) -> Option<ResolutionPlan<'_>> {
        match self {
            Self::Ability(plan) => Some(ResolutionPlan::Ability(plan)),
            Self::Drink { actor, effects, .. } | Self::Eat { actor, effects, .. } => {
                Some(ResolutionPlan::Effects {
                    actor: *actor,
                    target: *actor,
                    effects,
                })
            }
            Self::Escape(_) | Self::Respec(_) | Self::Equip { .. } | Self::Unequip { .. } => None,
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        valid_actor(self.actor())
            && (1..=crate::limits::NUMERIC as u64).contains(&self.preparation())
            && match self {
                Self::Escape(plan) => plan.valid(rules),
                Self::Ability(plan) => plan.valid(rules),
                Self::Respec(plan) => plan.valid(rules),
                Self::Equip { item, slot, .. } => {
                    valid_item(*item) && usize::from(slot.0) < crate::limits::EQUIPMENT_SOCKETS
                }
                Self::Unequip { item, .. } => valid_item(*item),
                Self::Drink { item, effects, .. } => {
                    valid_item(*item) && effects_valid(effects, rules)
                }
                Self::Eat { item, effects, .. } => {
                    valid_item(*item)
                        && effects_valid(effects, rules)
                        && matches!(effects.as_slice(), [PlannedEffect::Nourish { amount }]
                            if amount.strictly_positive())
                }
            }
    }
}
pub(super) fn valid_actor(id: ActorId) -> bool {
    id.0 > 0 && id.0 != u64::MAX
}
fn valid_item(id: ItemId) -> bool {
    id.0 > 0 && id.0 != u64::MAX
}
pub(crate) fn effects_valid(effects: &[PlannedEffect], rules: &crate::rules::Ruleset) -> bool {
    effects.len() <= crate::limits::EFFECTS_PER_BRANCH
        && effects.iter().skip(1).all(|effect| !effect.spatial())
        && effects.first().is_none_or(|first| {
            effects
                .iter()
                .all(|effect| effect.recipient() == first.recipient())
        })
        && effects.iter().all(|effect| match effect {
            PlannedEffect::StepSelfOneCell
            | PlannedEffect::PushTargetOneCell { .. }
            | PlannedEffect::DrainLatestHitDie
            | PlannedEffect::RestoreHitDie => true,
            PlannedEffect::ApplyLinkedHold { policy } => policy.valid() && rules.flag(&policy.sustaining).is_some(),
            PlannedEffect::HoldDamage { descriptor, .. } => descriptor.as_ref().is_none_or(|id| rules.descriptor(id).is_some()),
            PlannedEffect::Damage { damage } => rules.validate_damage(damage).is_ok(),
            PlannedEffect::Heal { amount }
            | PlannedEffect::Nourish { amount }
            | PlannedEffect::LoseAttribute { amount, .. }
            | PlannedEffect::RestoreAttribute { amount, .. }
            | PlannedEffect::LoseMaximumHealth { amount }
            | PlannedEffect::RestoreMaximumHealth { amount }
            | PlannedEffect::Restore { amount, .. } => match amount {
                PlannedAmount::Fixed { amount } => *amount <= crate::limits::NUMERIC as u32,
                PlannedAmount::Rolled { .. } => true,
            },
            PlannedEffect::ApplyCondition { id, duration } => {
                rules.condition(id).is_some()
                    && (1..=crate::limits::NUMERIC as u64).contains(duration)
            }
            PlannedEffect::ObservePublicPlan => true,
            PlannedEffect::ApplyQualifiedConditionToSource {
                id,
                duration,
                ability_tag,
            } => {
                rules
                    .condition(id)
                    .is_some_and(crate::rules::ConditionDef::supports_qualification)
                    && (1..=crate::limits::NUMERIC as u64).contains(duration)
                    && rules.ability_tag(ability_tag).is_some()
            }
        })
}
fn damage_matches_definition(
    expected: &DamageExpr,
    actual: &DamageSpec,
    melee_primary: bool,
) -> bool {
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
    let actual_primary = actual.primary();
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
                    (melee_primary && Some(component.key()) == actual_primary)
                        || *expected == u64::from(actual)
                }
                (Some(None), crate::damage::DamageAmount::Rolled(_)) => true,
                _ => false,
            }
        })
}

/// Compare immutable effect shapes without re-evaluating captured derived operands.
pub(crate) fn effects_match_definition(expected: &[Effect], actual: &[PlannedEffect]) -> bool {
    effects_match_definition_scoped(expected, actual, false)
}

/// Natural melee modifies only the first damage effect's primary component.
/// The captured modifier cannot be re-derived from later equipment or conditions.
pub(super) fn effects_match_definition_scoped(
    expected: &[Effect],
    actual: &[PlannedEffect],
    natural: bool,
) -> bool {
    let primary_effect = natural
        .then(|| {
            expected
                .iter()
                .position(|effect| matches!(effect, Effect::Damage { .. }))
        })
        .flatten();
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .enumerate()
            .all(|(index, (expected, actual))| match (expected, actual) {
                (Effect::ApplyLinkedHold { policy: expected }, PlannedEffect::ApplyLinkedHold { policy: actual }) => expected == actual,
                (Effect::HoldDamage { category: a, descriptor: d }, PlannedEffect::HoldDamage { category: b, descriptor: e }) => a == b && d == e,
                (Effect::StepSelfOneCell, PlannedEffect::StepSelfOneCell)
                | (Effect::DrainLatestHitDie, PlannedEffect::DrainLatestHitDie)
                | (Effect::RestoreHitDie, PlannedEffect::RestoreHitDie) => true,
                (
                    Effect::PushTargetOneCell {
                        direction: expected,
                    },
                    PlannedEffect::PushTargetOneCell { direction: actual },
                ) => expected == actual,
                (Effect::Damage { damage: expected }, PlannedEffect::Damage { damage: actual }) => {
                    damage_matches_definition(expected, actual, primary_effect == Some(index))
                }
                (Effect::Heal { amount: expected }, PlannedEffect::Heal { amount: actual })
                | (
                    Effect::Nourish { amount: expected },
                    PlannedEffect::Nourish { amount: actual },
                )
                | (
                    Effect::LoseMaximumHealth { amount: expected },
                    PlannedEffect::LoseMaximumHealth { amount: actual },
                )
                | (
                    Effect::RestoreMaximumHealth { amount: expected },
                    PlannedEffect::RestoreMaximumHealth { amount: actual },
                ) => PlannedAmount::compile(expected).is_ok_and(|expected| &expected == actual),
                (
                    Effect::LoseAttribute {
                        attribute: expected_attribute,
                        amount: expected,
                    },
                    PlannedEffect::LoseAttribute {
                        attribute: actual_attribute,
                        amount: actual,
                    },
                )
                | (
                    Effect::RestoreAttribute {
                        attribute: expected_attribute,
                        amount: expected,
                    },
                    PlannedEffect::RestoreAttribute {
                        attribute: actual_attribute,
                        amount: actual,
                    },
                ) => {
                    expected_attribute == actual_attribute
                        && PlannedAmount::compile(expected)
                            .is_ok_and(|expected| &expected == actual)
                }
                (Effect::ObservePublicPlan, PlannedEffect::ObservePublicPlan) => true,
                (
                    Effect::ApplyQualifiedConditionToSource {
                        condition,
                        ability_tag,
                        ..
                    },
                    PlannedEffect::ApplyQualifiedConditionToSource {
                        id,
                        ability_tag: actual_tag,
                        ..
                    },
                ) => condition == id && ability_tag == actual_tag,
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
                )
                | (
                    Effect::ApplyQualifiedConditionToSource {
                        duration: expected, ..
                    },
                    PlannedEffect::ApplyQualifiedConditionToSource {
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
            Reach::SelfOrAlly { range } => {
                actor == target
                    || (self.allied(actor, target)
                        && self.target_in_reach_snapshot(
                            actor,
                            target,
                            &match range {
                                AllyRange::Touch => Reach::Melee,
                                AllyRange::Sight { range } => Reach::Sight { range: *range },
                            },
                            sight,
                        ))
            }
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
        self.compile_strike_plan(
            actor,
            ability,
            super::AbilityInvocation::ActorTarget { target },
            sight,
            crate::supplies::SupplySelection::Carried,
        )
    }
    pub(crate) fn ability_plan_with_supplies(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        invocation: super::AbilityInvocation,
        supplies: crate::supplies::SupplySelection,
    ) -> Result<Plan, GameError> {
        self.compile_strike_plan(actor, ability, invocation, None, supplies)
    }
    pub(crate) fn ability_invocation_plan_in_scene(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        invocation: super::AbilityInvocation,
        sight: &[tor_world::SightCell],
    ) -> Result<Plan, GameError> {
        self.compile_strike_plan(
            actor,
            ability,
            invocation,
            Some(sight),
            crate::supplies::SupplySelection::Carried,
        )
    }
    pub(crate) fn compile_effects(
        &self,
        actor: ActorId,
        effects: &[Effect],
        modifiers: &crate::creatures::AbilityValues,
    ) -> Result<Vec<PlannedEffect>, GameError> {
        self.compile_effects_from_source(actor, effects, modifiers, false)
    }
    pub(crate) fn compile_effects_from_source(
        &self,
        actor: ActorId,
        effects: &[Effect],
        modifiers: &crate::creatures::AbilityValues,
        natural_source: bool,
    ) -> Result<Vec<PlannedEffect>, GameError> {
        if effects.len() > crate::limits::EFFECTS_PER_BRANCH {
            return Err(invalid());
        }

        effects
            .iter()
            .map(|effect| match effect {
                Effect::ApplyLinkedHold { policy } => Ok(PlannedEffect::ApplyLinkedHold { policy: policy.clone() }),
                Effect::HoldDamage { category, descriptor } => Ok(PlannedEffect::HoldDamage { category: *category, descriptor: descriptor.clone() }),
                Effect::DrainLatestHitDie => Ok(PlannedEffect::DrainLatestHitDie),
                Effect::RestoreHitDie => Ok(PlannedEffect::RestoreHitDie),
                Effect::StepSelfOneCell => Ok(PlannedEffect::StepSelfOneCell),
                Effect::PushTargetOneCell { direction } => Ok(PlannedEffect::PushTargetOneCell {
                    direction: *direction,
                }),
                Effect::ObservePublicPlan => Ok(PlannedEffect::ObservePublicPlan),
                Effect::ApplyQualifiedConditionToSource {
                    condition,
                    duration,
                    ability_tag,
                } => Ok(PlannedEffect::ApplyQualifiedConditionToSource {
                    id: condition.clone(),
                    ability_tag: ability_tag.clone(),
                    duration: duration
                        .checked_add(modifiers.duration)
                        .filter(|duration| *duration <= crate::limits::NUMERIC as u64)
                        .ok_or_else(invalid)?,
                }),
                Effect::Damage { damage } => {
                    let damage = match damage {
                        DamageExpr::SelectedWeapon { impact_bonus } if natural_source => self
                            .creature(actor)
                            .ok_or_else(invalid)?
                            .derived()
                            .natural_attack
                            .attack
                            .first_primary_damage()
                            .map_err(|_| invalid())?
                            .with_melee_modifiers(modifiers.dice, modifiers.flat, *impact_bonus)
                            .map_err(|_| invalid())?,
                        DamageExpr::SelectedWeapon { impact_bonus } => self
                            .selected_melee_attack(actor)
                            .ok_or_else(invalid)?
                            .damage_with_modifiers(modifiers.dice, modifiers.flat, *impact_bonus)
                            .map_err(|_| invalid())?,
                        DamageExpr::Components { components } => {
                            crate::rules::compile_damage_components(
                                components,
                                modifiers.dice,
                                modifiers.flat,
                            )
                            .map_err(|_| invalid())?
                        }
                    };
                    Ok(PlannedEffect::Damage { damage })
                }
                Effect::Heal { amount } => Ok(PlannedEffect::Heal {
                    amount: PlannedAmount::compile(amount)?,
                }),
                Effect::Nourish { amount } => Ok(PlannedEffect::Nourish {
                    amount: PlannedAmount::compile(amount)?,
                }),
                Effect::LoseAttribute { attribute, amount } => Ok(PlannedEffect::LoseAttribute {
                    attribute: *attribute,
                    amount: PlannedAmount::compile(amount)?,
                }),
                Effect::RestoreAttribute { attribute, amount } => {
                    Ok(PlannedEffect::RestoreAttribute {
                        attribute: *attribute,
                        amount: PlannedAmount::compile(amount)?,
                    })
                }
                Effect::LoseMaximumHealth { amount } => Ok(PlannedEffect::LoseMaximumHealth {
                    amount: PlannedAmount::compile(amount)?,
                }),
                Effect::RestoreMaximumHealth { amount } => {
                    Ok(PlannedEffect::RestoreMaximumHealth {
                        amount: PlannedAmount::compile(amount)?,
                    })
                }
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
    /// New admission may reject restoration without spending anything. A paid
    /// frozen plan rechecks through the effect kernel and keeps ordinary debt.
    pub(crate) fn validate_hit_die_effect_admission(
        &self,
        target: ActorId,
        effects: &[PlannedEffect],
    ) -> Result<(), GameError> {
        if effects
            .iter()
            .any(|effect| matches!(effect, PlannedEffect::RestoreHitDie))
        {
            let creature = self.creature(target).ok_or(GameError::UnknownActor)?;
            if let Some(blocker) = creature.hit_die_restoration_blocker() {
                return Err(GameError::Creature(
                    crate::creatures::CreatureStateError::HitDieRestoration(blocker),
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn effect_plan(
        &self,
        actor: ActorId,
        effects: &[Effect],
    ) -> Result<Vec<PlannedEffect>, GameError> {
        if effects.iter().any(Effect::spatial) {
            return Err(invalid());
        }
        let effects =
            self.compile_effects(actor, effects, &crate::creatures::AbilityValues::default())?;
        if !effects_valid(&effects, &self.content.rules) {
            return Err(invalid());
        }
        self.validate_hit_die_effect_admission(actor, &effects)?;
        Ok(effects)
    }
}
impl<'de> Deserialize<'de> for PlannedEffect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ApplyLinkedHold {
        policy: crate::linked_hold::HoldPolicy,
    },
    HoldDamage {
        category: crate::combat::DamageType,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        descriptor: Option<crate::content_ids::DescriptorId>,
    },
            DrainLatestHitDie {},
            RestoreHitDie {},
            StepSelfOneCell {},
            PushTargetOneCell {
                direction: super::PushDirection,
            },
            Damage {
                damage: DamageSpec,
            },
            Heal {
                amount: PlannedAmount,
            },
            Nourish {
                amount: PlannedAmount,
            },
            LoseAttribute {
                attribute: crate::attributes::Attribute,
                amount: PlannedAmount,
            },
            RestoreAttribute {
                attribute: crate::attributes::Attribute,
                amount: PlannedAmount,
            },
            LoseMaximumHealth {
                amount: PlannedAmount,
            },
            RestoreMaximumHealth {
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
            ObservePublicPlan {},
            ApplyQualifiedConditionToSource {
                id: ConditionId,
                duration: u64,
                ability_tag: crate::content_ids::AbilityTagId,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::ApplyLinkedHold { policy } => Self::ApplyLinkedHold { policy },
            Fields::HoldDamage { category, descriptor } => Self::HoldDamage { category, descriptor },
            Fields::DrainLatestHitDie {} => Self::DrainLatestHitDie,
            Fields::RestoreHitDie {} => Self::RestoreHitDie,
            Fields::StepSelfOneCell {} => Self::StepSelfOneCell,
            Fields::PushTargetOneCell { direction } => Self::PushTargetOneCell { direction },
            Fields::Damage { damage } => Self::Damage { damage },
            Fields::Heal { amount } => Self::Heal { amount },
            Fields::Nourish { amount } => Self::Nourish { amount },
            Fields::LoseAttribute { attribute, amount } => {
                Self::LoseAttribute { attribute, amount }
            }
            Fields::RestoreAttribute { attribute, amount } => {
                Self::RestoreAttribute { attribute, amount }
            }
            Fields::LoseMaximumHealth { amount } => Self::LoseMaximumHealth { amount },
            Fields::RestoreMaximumHealth { amount } => Self::RestoreMaximumHealth { amount },
            Fields::ApplyCondition { id, duration } => Self::ApplyCondition { id, duration },
            Fields::Restore { resource, amount } => Self::Restore { resource, amount },
            Fields::ObservePublicPlan {} => Self::ObservePublicPlan,
            Fields::ApplyQualifiedConditionToSource {
                id,
                duration,
                ability_tag,
            } => Self::ApplyQualifiedConditionToSource {
                id,
                duration,
                ability_tag,
            },
        })
    }
}
