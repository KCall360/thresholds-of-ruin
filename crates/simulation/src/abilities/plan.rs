//! Compile and validate owned action-specific plans; never resolve effects here.
use super::{FrozenStrike, ResolutionPlan, TokenClaim};
use crate::content_ids::{AbilityId, ConditionId};
use crate::costs::CostBundle;
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
    /// Valid operands exceed the shared runtime numeric budget.
    NumericBudget,
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
    Theft {
        theft: crate::theft::PlannedTheft,
    },
    AlterEquipment {
        alteration: crate::PlannedEquipmentAlteration,
    },
    CorrodeArmor {
        slot: crate::EquipmentSlot,
        amount: u32,
    },
    ApplyLinkedHold {
        policy: crate::linked_hold::HoldPolicy,
    },
    HoldDamage {
        category: crate::combat::DamageType,
        descriptor: Option<crate::content_ids::DescriptorId>,
    },
    DrainLatestHitDie,
    RestoreHitDie,
    TeleportRandom,
    TeleportToRayOrigin,
    ArrestDescent,
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
    HealPerHitDie {
        dice: Vec<DicePool>,
        bonus: i32,
        overflow: crate::rules::HealingOverflow,
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
    ApplyPersistentCondition {
        id: ConditionId,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        cancels: Option<ConditionId>,
    },
    ApplyConditionForTurns {
        id: ConditionId,
        duration: PlannedAmount,
        turn_ticks: u64,
    },
    RemoveCondition {
        id: ConditionId,
    },
    Restore {
        resource: Resource,
        amount: PlannedAmount,
    },
    SearchAdjacentGuises {
        check: crate::discovery::FrozenDiscoveryCheck,
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
            Self::AlterEquipment { .. }
            | Self::SearchAdjacentGuises { .. }
            | Self::ObservePublicPlan
            | Self::ApplyQualifiedConditionToSource { .. } => crate::rules::EffectRecipient::Source,
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
    pub(crate) fn valid_turn_duration(&self, turn_ticks: u64) -> bool {
        let maximum = match self {
            Self::Fixed { amount } => *amount,
            Self::Rolled { dice } => dice.maximum(),
        };
        self.strictly_positive()
            && turn_ticks > 0
            && u64::from(maximum)
                .checked_mul(turn_ticks)
                .is_some_and(|ticks| ticks <= crate::limits::NUMERIC as u64)
    }
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
    Ray(tor_world::Shared<super::RayPlan>),
    Projectile(tor_world::Shared<super::ProjectilePlan>),
    Zap {
        item: ItemId,
        charges: u16,
        discovery: crate::ItemDiscovery,
        ray: tor_world::Shared<super::RayPlan>,
    },
    Respec(tor_world::Shared<crate::player_respec::RespecPlan>),
    Equip {
        actor: ActorId,
        item: ItemId,
        slot: EquipmentSlotId,
        allocation: crate::equipment_capture::FrozenEquipAllocation,
        preparation: u64,
    },
    Unequip {
        actor: ActorId,
        item: ItemId,
        preparation: u64,
    },
    Read {
        #[serde(deserialize_with = "crate::bounded::required_option")]
        control: Option<crate::control_scrolls::ControlSpec>,
        #[serde(deserialize_with = "super::strikes::read_descriptors")]
        descriptors: Vec<crate::content_ids::DescriptorId>,
        actor: ActorId,
        item: ItemId,
        preparation: u64,
        #[serde(deserialize_with = "read_item_effects")]
        effects: Vec<PlannedEffect>,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        identify: Option<Vec<crate::ItemClass>>,
        discovery: crate::ItemDiscovery,
    },
    Drink {
        #[serde(deserialize_with = "super::strikes::read_descriptors")]
        descriptors: Vec<crate::content_ids::DescriptorId>,
        discovery: crate::ItemDiscovery,
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
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub ambush: Option<super::CheckSite>,
    pub actor: ActorId,
    pub invocation: super::AbilityInvocation,
    pub weapon: super::FrozenWeaponSelection,
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
    pub cost: Option<CostBundle>,
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
            #[serde(deserialize_with = "crate::bounded::required_option")]
            ambush: Option<super::CheckSite>,
            actor: ActorId,
            invocation: super::AbilityInvocation,
            weapon: super::FrozenWeaponSelection,
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
            cost: Option<CostBundle>,
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
            ambush: fields.ambush,
            actor: fields.actor,
            invocation: fields.invocation,
            weapon: fields.weapon,
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
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => plan.tokens.as_slice(),
            Self::Projectile(plan) => plan.tokens.as_slice(),
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
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => plan.actor,
            Self::Projectile(plan) => plan.actor,
            Self::Respec(plan) => plan.basis.actor,
            Self::Equip { actor, .. }
            | Self::Unequip { actor, .. }
            | Self::Read { actor, .. }
            | Self::Drink { actor, .. }
            | Self::Eat { actor, .. } => *actor,
        }
    }
    pub fn target(&self) -> ActorId {
        match self {
            Self::Escape(plan) => plan.holder,
            Self::Ability(plan) => plan.target,
            // Legacy actor-local bookkeeping only; a ray has no persisted target.
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => plan.actor,
            Self::Projectile(plan) => plan.actor,
            Self::Respec(plan) => plan.basis.actor,
            Self::Equip { actor, .. }
            | Self::Unequip { actor, .. }
            | Self::Read { actor, .. }
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
            Self::Zap { item, ray, .. } => Work::Zap {
                item: *item,
                direction: ray.direction,
            },
            Self::Projectile(plan) => Work::UseAbility {
                id: plan.ability.clone(),
                invocation: super::AbilityInvocation::directional_projectile(
                    plan.direction,
                    plan.selection,
                ),
                supplies: crate::supplies::SupplySelection::Carried,
            },
            Self::Ray(plan) => Work::UseAbility {
                id: plan.ability.clone(),
                invocation: super::AbilityInvocation::directional_ray(plan.direction),
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
            Self::Read { item, .. } => Work::Read { item: *item },
            Self::Eat { item, .. } => Work::Eat { item: *item },
        }
    }
    pub fn preparation(&self) -> u64 {
        match self {
            Self::Escape(plan) => plan.preparation,
            Self::Ability(plan) => plan.preparation,
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => plan.preparation,
            Self::Projectile(plan) => plan.preparation,
            Self::Respec(plan) => plan.preparation,
            Self::Equip { preparation, .. }
            | Self::Unequip { preparation, .. }
            | Self::Read { preparation, .. }
            | Self::Drink { preparation, .. }
            | Self::Eat { preparation, .. } => *preparation,
        }
    }
    pub fn recovery(&self) -> u64 {
        match self {
            Self::Escape(plan) => plan.recovery,
            Self::Ability(plan) => plan.recovery + plan.step.map_or(0, |step| step.duration),
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => plan.recovery,
            Self::Projectile(plan) => plan.recovery,
            Self::Respec(plan) => plan.recovery,
            Self::Equip { .. }
            | Self::Unequip { .. }
            | Self::Read { .. }
            | Self::Drink { .. }
            | Self::Eat { .. } => 0,
        }
    }
    pub fn cost(&self) -> Option<CostBundle> {
        match self {
            Self::Ability(plan) => plan.cost,
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => plan.cost,
            Self::Projectile(plan) => plan.cost,
            Self::Escape(_) | Self::Respec(_) => None,
            Self::Equip { .. }
            | Self::Unequip { .. }
            | Self::Read { .. }
            | Self::Drink { .. }
            | Self::Eat { .. } => None,
        }
    }
    pub fn on_success(&self) -> &[PlannedEffect] {
        match self {
            Self::Ability(plan) => plan
                .strikes
                .first()
                .map_or(&[], |strike| strike.on_success.as_slice()),
            Self::Read { effects, .. }
            | Self::Drink { effects, .. }
            | Self::Eat { effects, .. } => effects,
            Self::Projectile(_)
            | Self::Ray(_)
            | Self::Zap { .. }
            | Self::Escape(_)
            | Self::Respec(_)
            | Self::Equip { .. }
            | Self::Unequip { .. } => &[],
        }
    }
    pub fn as_ability(&self) -> Option<&AbilityPlan> {
        match self {
            Self::Ability(plan) => Some(plan),
            Self::Projectile(_)
            | Self::Ray(_)
            | Self::Zap { .. }
            | Self::Escape(_)
            | Self::Respec(_) => None,
            Self::Equip { .. }
            | Self::Unequip { .. }
            | Self::Read { .. }
            | Self::Drink { .. }
            | Self::Eat { .. } => None,
        }
    }
    pub fn as_projectile(&self) -> Option<&super::ProjectilePlan> {
        match self {
            Self::Projectile(plan) => Some(plan),
            _ => None,
        }
    }
    pub fn as_ray(&self) -> Option<&super::RayPlan> {
        match self {
            Self::Ray(plan) | Self::Zap { ray: plan, .. } => Some(plan),
            _ => None,
        }
    }
    pub(crate) fn ability_id(&self) -> Option<&AbilityId> {
        match self {
            Self::Ability(plan) => Some(&plan.ability),
            Self::Ray(plan) => Some(&plan.ability),
            Self::Projectile(plan) => Some(&plan.ability),
            _ => None,
        }
    }
    pub(crate) fn ability_cooldown(&self) -> Option<std::num::NonZeroU64> {
        match self {
            Self::Ability(plan) => plan.cooldown,
            Self::Ray(plan) => plan.cooldown,
            Self::Projectile(plan) => plan.cooldown,
            _ => None,
        }
    }
    /// Borrows frozen operands without constructing a second owned plan.
    pub fn resolution(&self) -> Option<ResolutionPlan<'_>> {
        match self {
            Self::Ability(plan) => Some(ResolutionPlan::Ability(plan)),
            Self::Read {
                actor,
                effects,
                descriptors,
                ..
            }
            | Self::Drink {
                actor,
                effects,
                descriptors,
                ..
            } => Some(ResolutionPlan::Exposure {
                actor: *actor,
                target: *actor,
                effects,
                descriptors,
            }),
            Self::Eat { actor, effects, .. } => Some(ResolutionPlan::Effects {
                actor: *actor,
                target: *actor,
                effects,
            }),
            Self::Projectile(_)
            | Self::Ray(_)
            | Self::Zap { .. }
            | Self::Escape(_)
            | Self::Respec(_)
            | Self::Equip { .. }
            | Self::Unequip { .. } => None,
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        valid_actor(self.actor())
            && self.preparation() <= crate::limits::NUMERIC as u64
            && (self.preparation() > 0
                || matches!(self, Self::Equip { allocation, .. } if allocation.quickdraw(rules)))
            && match self {
                Self::Escape(plan) => plan.valid(rules),
                Self::Ability(plan) => plan.valid(rules),
                Self::Ray(plan) => plan.valid(rules),
                Self::Projectile(plan) => plan.valid(rules),
                Self::Zap {
                    item,
                    ray,
                    discovery,
                    ..
                } => {
                    valid_item(*item)
                        && matches!(
                            discovery,
                            crate::ItemDiscovery::OnUse | crate::ItemDiscovery::Never
                        )
                        && ray.valid(rules)
                        && ray.cost.is_none()
                        && ray.cooldown.is_none()
                        && ray.supply_costs.is_empty()
                }
                Self::Respec(plan) => plan.valid(rules),
                Self::Equip {
                    item,
                    slot,
                    allocation,
                    ..
                } => {
                    valid_item(*item)
                        && usize::from(slot.0) < crate::limits::EQUIPMENT_SOCKETS
                        && allocation.valid_shape(rules, *item, *slot)
                        && allocation.quickdraw(rules) == (self.preparation() == 0)
                }
                Self::Unequip { item, .. } => valid_item(*item),
                Self::Read {
                    control,
                    item,
                    descriptors,
                    effects,
                    identify,
                    discovery,
                    ..
                } => {
                    valid_item(*item)
                        && rules
                            .validate_payload_recipients(
                                descriptors,
                                effects.iter().map(PlannedEffect::recipient),
                            )
                            .is_ok()
                        && effects_valid(effects, rules)
                        && control.as_ref().is_none_or(|control| {
                            control.valid(rules)
                                && effects.is_empty()
                                && descriptors.is_empty()
                                && identify.is_none()
                        })
                        && match identify {
                            Some(_) => {
                                effects.is_empty()
                                    && descriptors.is_empty()
                                    && *discovery == crate::ItemDiscovery::OnUse
                            }
                            None => control.is_some() || !effects.is_empty(),
                        }
                }
                Self::Drink {
                    item,
                    effects,
                    descriptors,
                    ..
                } => {
                    valid_item(*item)
                        && effects_valid(effects, rules)
                        && rules
                            .validate_payload_recipients(
                                descriptors,
                                effects.iter().map(PlannedEffect::recipient),
                            )
                            .is_ok()
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
/// HD scales a healing roll, not a damage expression. Chunking reuses the
/// bounded dice kernel without widening the independent damage/check limit.
pub(super) fn compile_hit_die_healing(
    count: u64,
    sides: u16,
    bonus: i32,
) -> Result<Vec<DicePool>, GameError> {
    if !(1..=crate::limits::HIT_DICE as u64).contains(&count) {
        return Err(invalid());
    }
    let mut left = count;
    let mut pools = Vec::new();
    while left > 0 {
        let chunk = left.min(u64::from(crate::limits::DICE)) as u16;
        pools.push(DicePool::new(chunk, sides, 0).map_err(|_| invalid())?);
        left -= u64::from(chunk);
    }
    if !valid_hit_die_healing(&pools, bonus) {
        return Err(invalid());
    }
    Ok(pools)
}
pub(super) fn valid_hit_die_healing(pools: &[DicePool], bonus: i32) -> bool {
    !pools.is_empty()
        && pools.len() <= crate::limits::HIT_DICE.div_ceil(usize::from(crate::limits::DICE))
        && pools.iter().enumerate().all(|(index, pool)| {
            pool.bonus() == 0
                && pool.sides() == pools[0].sides()
                && (index + 1 == pools.len() || pool.count() == crate::limits::DICE)
        })
        && pools
            .iter()
            .map(|pool| usize::from(pool.count()))
            .sum::<usize>()
            <= crate::limits::HIT_DICE
        && (-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(bonus))
        && pools
            .iter()
            .map(|pool| i64::from(pool.maximum()))
            .sum::<i64>()
            + i64::from(bonus)
            <= crate::limits::NUMERIC
}
fn read_hit_die_pools<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<DicePool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct NamedPool(#[serde(deserialize_with = "crate::bounded::named_map")] DicePool);
    Ok(crate::bounded::Bounded::<
        NamedPool,
        { crate::limits::HIT_DICE.div_ceil(crate::limits::DICE as usize) },
    >::deserialize(deserializer)?
    .0
    .into_iter()
    .map(|pool| pool.0)
    .collect())
}
pub(crate) fn effects_valid(effects: &[PlannedEffect], rules: &crate::rules::Ruleset) -> bool {
    effects.len() <= crate::limits::EFFECTS_PER_BRANCH
        && (effects.len() == 1
            || !effects.iter().any(|effect| {
                matches!(
                    effect,
                    PlannedEffect::TeleportRandom | PlannedEffect::TeleportToRayOrigin
                )
            }))
        && (effects.len() == 1
            || !effects
                .iter()
                .any(|effect| matches!(effect, PlannedEffect::CorrodeArmor { .. })))
        && effects.iter().skip(1).all(|effect| !effect.spatial())
        && effects.first().is_none_or(|first| {
            effects
                .iter()
                .all(|effect| effect.recipient() == first.recipient())
        })
        && effects.iter().all(|effect| match effect {
            PlannedEffect::Theft { theft } => theft.valid(),
            PlannedEffect::AlterEquipment { alteration } => alteration.valid(rules),
            PlannedEffect::CorrodeArmor { slot, amount } => {
                rules.equipment_slot(slot).is_some()
                    && (1..=crate::limits::NUMERIC as u32).contains(amount)
            }
            PlannedEffect::StepSelfOneCell
            | PlannedEffect::PushTargetOneCell { .. }
            | PlannedEffect::DrainLatestHitDie
            | PlannedEffect::RestoreHitDie
            | PlannedEffect::TeleportRandom
            | PlannedEffect::TeleportToRayOrigin
            | PlannedEffect::ArrestDescent => true,
            PlannedEffect::ApplyLinkedHold { policy } => {
                policy.valid() && rules.flag(&policy.sustaining).is_some()
            }
            PlannedEffect::HoldDamage { descriptor, .. } => descriptor
                .as_ref()
                .is_none_or(|id| rules.descriptor(id).is_some()),
            PlannedEffect::Damage { damage } => rules.validate_damage(damage).is_ok(),
            PlannedEffect::HealPerHitDie { dice, bonus, .. } => valid_hit_die_healing(dice, *bonus),
            PlannedEffect::ApplyConditionForTurns {
                id,
                duration,
                turn_ticks,
            } => {
                rules.condition(id).is_some_and(|definition| {
                    definition.timing != crate::rules::ConditionTiming::UntilRemoved
                }) && duration.valid_turn_duration(*turn_ticks)
            }
            PlannedEffect::ApplyPersistentCondition { id, cancels } => {
                let persistent = |condition: &ConditionId| {
                    rules.condition(condition).is_some_and(|definition| {
                        definition.timing == crate::rules::ConditionTiming::UntilRemoved
                    })
                };
                persistent(id)
                    && cancels
                        .as_ref()
                        .is_none_or(|opposite| opposite != id && persistent(opposite))
            }
            PlannedEffect::RemoveCondition { id } => rules.condition(id).is_some(),
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
                rules.condition(id).is_some_and(|definition| {
                    definition.timing != crate::rules::ConditionTiming::UntilRemoved
                }) && (1..=crate::limits::NUMERIC as u64).contains(duration)
            }
            PlannedEffect::SearchAdjacentGuises { check } => check.valid(),
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
                (Effect::TeleportRandom, PlannedEffect::TeleportRandom)
                | (Effect::TeleportToRayOrigin, PlannedEffect::TeleportToRayOrigin)
                | (Effect::ArrestDescent, PlannedEffect::ArrestDescent) => true,
                (
                    Effect::Theft {
                        selection,
                        relocation,
                    },
                    PlannedEffect::Theft { theft },
                ) => theft.matches(selection, relocation.as_ref()),
                (
                    Effect::AlterEquipment {
                        selection,
                        alteration,
                    },
                    PlannedEffect::AlterEquipment {
                        alteration: planned,
                    },
                ) => &planned.selection == selection && planned.alteration == *alteration,
                (
                    Effect::CorrodeArmor {
                        slot: expected_slot,
                        amount: expected_amount,
                    },
                    PlannedEffect::CorrodeArmor { slot, amount },
                ) => expected_slot == slot && expected_amount == amount,
                (
                    Effect::ApplyLinkedHold { policy: expected },
                    PlannedEffect::ApplyLinkedHold { policy: actual },
                ) => expected == actual,
                (
                    Effect::HoldDamage {
                        category: a,
                        descriptor: d,
                    },
                    PlannedEffect::HoldDamage {
                        category: b,
                        descriptor: e,
                    },
                ) => a == b && d == e,
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
                (
                    Effect::HealPerHitDie {
                        sides,
                        bonus,
                        overflow,
                    },
                    PlannedEffect::HealPerHitDie {
                        dice,
                        bonus: actual_bonus,
                        overflow: actual_overflow,
                    },
                ) => {
                    overflow == actual_overflow
                        && bonus == actual_bonus
                        && valid_hit_die_healing(dice, *actual_bonus)
                        && dice.iter().all(|pool| pool.sides() == *sides)
                }
                (
                    Effect::ApplyPersistentCondition { condition, cancels },
                    PlannedEffect::ApplyPersistentCondition {
                        id,
                        cancels: actual,
                    },
                ) => condition == id && cancels == actual,
                (Effect::RemoveCondition { condition }, PlannedEffect::RemoveCondition { id }) => {
                    condition == id
                }
                (
                    Effect::ApplyConditionForTurns {
                        condition,
                        duration,
                    },
                    PlannedEffect::ApplyConditionForTurns {
                        id,
                        duration: actual,
                        turn_ticks,
                    },
                ) => {
                    condition == id
                        && actual.valid_turn_duration(*turn_ticks)
                        && PlannedAmount::compile(duration)
                            .is_ok_and(|expected| &expected == actual)
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
                (Effect::SearchAdjacentGuises, PlannedEffect::SearchAdjacentGuises { .. })
                | (Effect::ObservePublicPlan, PlannedEffect::ObservePublicPlan) => true,
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
    pub fn ability_cost(&self, ability: &AbilityId) -> Result<Option<CostBundle>, GameError> {
        self.content
            .rules
            .ability(ability)
            .ok_or(GameError::Rule(RuleError::UnknownAbility))?
            .cost
            .as_ref()
            .map(CostBundle::try_from)
            .transpose()
            .map_err(|_| invalid())
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
            // Physical projectile reach is resolved along its released flight.
            Reach::Projectile => false,
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
            Reach::MutualSight { range } => {
                actor != target
                    && sight.map_or_else(
                        || self.genuinely_sees_actor(actor, target, *range),
                        |scene| self.genuinely_sees_actor_in_sight(actor, target, *range, scene),
                    )
                    && self.genuinely_sees_actor(target, actor, *range)
            }
            Reach::Sight { range } => sight.map_or_else(
                || self.genuinely_sees_actor(actor, target, *range),
                |sight| self.genuinely_sees_actor_in_sight(actor, target, *range, sight),
            ),
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
        let reference = self.actor_disclosure(actor, target)?.reference();
        let invocation = if reference == Some(crate::perception::ActorReferenceKind::Contact) {
            super::AbilityInvocation::contact_target(target)
        } else {
            super::AbilityInvocation::actor_target(target)
        };
        self.compile_strike_plan(
            actor,
            ability,
            invocation,
            Some(sight),
            crate::supplies::SupplySelection::Carried,
        )
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
            super::AbilityInvocation::actor_target(target),
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
        self.compile_effects_from_source(actor, effects, modifiers, false, None)
    }
    pub(crate) fn compile_effects_from_source(
        &self,
        actor: ActorId,
        effects: &[Effect],
        modifiers: &crate::creatures::AbilityValues,
        natural_source: bool,
        selected: Option<&crate::creatures::SelectedAttack>,
    ) -> Result<Vec<PlannedEffect>, GameError> {
        let bonus = self
            .creature(actor)
            .map_or(0, |creature| creature.derived().attack_damage_roll_bonus);
        self.compile_effects_with_roll_bonus(
            actor,
            effects,
            modifiers,
            natural_source,
            selected,
            bonus,
        )
    }
    fn compile_effects_with_roll_bonus(
        &self,
        actor: ActorId,
        effects: &[Effect],
        modifiers: &crate::creatures::AbilityValues,
        natural_source: bool,
        selected: Option<&crate::creatures::SelectedAttack>,
        roll_bonus: i32,
    ) -> Result<Vec<PlannedEffect>, GameError> {
        if effects.len() > crate::limits::EFFECTS_PER_BRANCH {
            return Err(invalid());
        }

        effects
            .iter()
            .map(|effect| match effect {
                Effect::Theft {
                    selection,
                    relocation,
                } => Ok(PlannedEffect::Theft {
                    theft: self.compile_theft(actor, selection, relocation.as_ref())?,
                }),
                Effect::AlterEquipment {
                    selection,
                    alteration,
                } => Ok(PlannedEffect::AlterEquipment {
                    alteration: self.compile_equipment_alteration(actor, selection, *alteration)?,
                }),
                Effect::CorrodeArmor { slot, amount } => Ok(PlannedEffect::CorrodeArmor {
                    slot: slot.clone(),
                    amount: *amount,
                }),
                Effect::ApplyLinkedHold { policy } => Ok(PlannedEffect::ApplyLinkedHold {
                    policy: policy.clone(),
                }),
                Effect::HoldDamage {
                    category,
                    descriptor,
                } => Ok(PlannedEffect::HoldDamage {
                    category: *category,
                    descriptor: descriptor.clone(),
                }),
                Effect::DrainLatestHitDie => Ok(PlannedEffect::DrainLatestHitDie),
                Effect::RestoreHitDie => Ok(PlannedEffect::RestoreHitDie),
                Effect::TeleportRandom => Ok(PlannedEffect::TeleportRandom),
                Effect::TeleportToRayOrigin => Ok(PlannedEffect::TeleportToRayOrigin),
                Effect::ArrestDescent => Ok(PlannedEffect::ArrestDescent),
                Effect::StepSelfOneCell => Ok(PlannedEffect::StepSelfOneCell),
                Effect::PushTargetOneCell { direction } => Ok(PlannedEffect::PushTargetOneCell {
                    direction: *direction,
                }),
                Effect::SearchAdjacentGuises => Ok(PlannedEffect::SearchAdjacentGuises {
                    check: self.capture_discovery_check(actor)?,
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
                        DamageExpr::SelectedWeapon { impact_bonus } => selected
                            .map(|selected| &selected.attack)
                            .or_else(|| self.selected_melee_attack(actor))
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
                    let damage = damage.with_roll_bonus(roll_bonus).map_err(|_| invalid())?;
                    Ok(PlannedEffect::Damage { damage })
                }
                Effect::HealPerHitDie {
                    sides,
                    bonus,
                    overflow,
                } => {
                    let count = self
                        .creature(actor)
                        .ok_or_else(invalid)?
                        .derived()
                        .total_hd()
                        .0;
                    let dice = compile_hit_die_healing(count, *sides, *bonus)?;
                    Ok(PlannedEffect::HealPerHitDie {
                        dice,
                        bonus: *bonus,
                        overflow: *overflow,
                    })
                }
                Effect::ApplyPersistentCondition { condition, cancels } => {
                    Ok(PlannedEffect::ApplyPersistentCondition {
                        id: condition.clone(),
                        cancels: cancels.clone(),
                    })
                }
                Effect::RemoveCondition { condition } => Ok(PlannedEffect::RemoveCondition {
                    id: condition.clone(),
                }),
                Effect::ApplyConditionForTurns {
                    condition,
                    duration,
                } => {
                    let duration = PlannedAmount::compile(duration)?;
                    let turn_ticks = self
                        .actors
                        .get(&actor)
                        .ok_or_else(invalid)?
                        .turn_ticks
                        .get();
                    if !duration.valid_turn_duration(turn_ticks) {
                        return Err(invalid());
                    }
                    Ok(PlannedEffect::ApplyConditionForTurns {
                        id: condition.clone(),
                        duration,
                        turn_ticks,
                    })
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
        if effects.iter().any(|effect| {
            (effect.spatial() && !matches!(effect, Effect::TeleportRandom))
                || matches!(effect, Effect::Theft { .. } | Effect::TeleportToRayOrigin)
        }) {
            return Err(invalid());
        }
        let effects = self.compile_effects_with_roll_bonus(
            actor,
            effects,
            &crate::creatures::AbilityValues::default(),
            false,
            None,
            0,
        )?;
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
            Theft {
                theft: crate::theft::PlannedTheft,
            },
            AlterEquipment {
                alteration: crate::PlannedEquipmentAlteration,
            },
            CorrodeArmor {
                slot: crate::EquipmentSlot,
                #[serde(deserialize_with = "crate::armor::read_corrosion_amount")]
                amount: u32,
            },
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
            TeleportRandom {},
            TeleportToRayOrigin {},
            ArrestDescent {},
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
            HealPerHitDie {
                #[serde(deserialize_with = "read_hit_die_pools")]
                dice: Vec<DicePool>,
                bonus: i32,
                overflow: crate::rules::HealingOverflow,
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
            ApplyPersistentCondition {
                id: ConditionId,
                #[serde(deserialize_with = "crate::bounded::required_option")]
                cancels: Option<ConditionId>,
            },
            ApplyConditionForTurns {
                id: ConditionId,
                duration: PlannedAmount,
                turn_ticks: u64,
            },
            RemoveCondition {
                id: ConditionId,
            },
            Restore {
                resource: Resource,
                amount: PlannedAmount,
            },
            SearchAdjacentGuises {
                check: crate::discovery::FrozenDiscoveryCheck,
            },
            ObservePublicPlan {},
            ApplyQualifiedConditionToSource {
                id: ConditionId,
                duration: u64,
                ability_tag: crate::content_ids::AbilityTagId,
            },
        }
        let effect = match crate::bounded::named_map(deserializer)? {
            Fields::Theft { theft } => Self::Theft { theft },
            Fields::AlterEquipment { alteration } => Self::AlterEquipment { alteration },
            Fields::CorrodeArmor { slot, amount } => Self::CorrodeArmor { slot, amount },
            Fields::ApplyLinkedHold { policy } => Self::ApplyLinkedHold { policy },
            Fields::HoldDamage {
                category,
                descriptor,
            } => Self::HoldDamage {
                category,
                descriptor,
            },
            Fields::DrainLatestHitDie {} => Self::DrainLatestHitDie,
            Fields::RestoreHitDie {} => Self::RestoreHitDie,
            Fields::TeleportRandom {} => Self::TeleportRandom,
            Fields::TeleportToRayOrigin {} => Self::TeleportToRayOrigin,
            Fields::ArrestDescent {} => Self::ArrestDescent,
            Fields::StepSelfOneCell {} => Self::StepSelfOneCell,
            Fields::PushTargetOneCell { direction } => Self::PushTargetOneCell { direction },
            Fields::Damage { damage } => Self::Damage { damage },
            Fields::Heal { amount } => Self::Heal { amount },
            Fields::HealPerHitDie {
                dice,
                bonus,
                overflow,
            } => Self::HealPerHitDie {
                dice,
                bonus,
                overflow,
            },
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
            Fields::ApplyPersistentCondition { id, cancels } => {
                Self::ApplyPersistentCondition { id, cancels }
            }
            Fields::ApplyConditionForTurns {
                id,
                duration,
                turn_ticks,
            } => Self::ApplyConditionForTurns {
                id,
                duration,
                turn_ticks,
            },
            Fields::RemoveCondition { id } => Self::RemoveCondition { id },
            Fields::Restore { resource, amount } => Self::Restore { resource, amount },
            Fields::SearchAdjacentGuises { check } => Self::SearchAdjacentGuises { check },
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
        };
        let valid = match &effect {
            Self::ApplyPersistentCondition { id, cancels } => cancels.as_ref() != Some(id),
            Self::ApplyConditionForTurns {
                duration,
                turn_ticks,
                ..
            } => duration.valid_turn_duration(*turn_ticks),
            Self::HealPerHitDie { dice, bonus, .. } => valid_hit_die_healing(dice, *bonus),
            _ => true,
        };
        if !valid {
            return Err(serde::de::Error::custom("invalid frozen effect operands"));
        }
        Ok(effect)
    }
}

fn read_item_effects<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PlannedEffect>, D::Error> {
    Ok(crate::bounded::Bounded::<PlannedEffect, {crate::limits::CONSUMABLE_EFFECTS}>::deserialize(d)?.0)
}
