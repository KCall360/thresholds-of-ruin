//! A directional release owns one frozen source and discovers recipients only
//! at execution. No actor target or future collision path is a plan operand.
use super::contact_policy::{RayResolution, RayTraversal};
use super::{
    CheckPhase, CheckSide, CheckSite, Placement, Plan, PlannedEffect, RuleError, StepDirection,
    TokenClaim,
};
use crate::content_ids::{AbilityId, ConditionId, DescriptorId, TalentId};
use crate::rules::{
    Against, CheckActor, Effect, Modifier, ModifierOp, Reach, Scope, SkillSelector, Stat,
    StrikeSource,
};
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Direction, Location, RayStep};

pub const MAX_RAY_VISITS: u16 = crate::limits::SIGHT_RANGE;
pub const MAX_RAY_PROBES: u16 = 64;
pub const MAX_RAY_REVERSALS: u16 = 32;
const MAX_INCOMING_SOURCES: usize = 1024;
const MAX_RAY_ENCOUNTERS: usize = 1024;

/// Pinned source behavior, frozen before any future collision is inspected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RayDelivery {
    Bouncing,
    FirstContact,
}
impl<'de> Deserialize<'de> for RayDelivery {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "bouncing" => Ok(Self::Bouncing),
            "first_contact" => Ok(Self::FirstContact),
            _ => Err(serde::de::Error::custom("invalid directional delivery")),
        }
    }
}
impl RayDelivery {
    fn from_invocation(kind: super::InvocationKind) -> Option<Self> {
        match kind {
            super::InvocationKind::DirectionalRay => Some(Self::Bouncing),
            super::InvocationKind::DirectionalContactRay => Some(Self::FirstContact),
            _ => None,
        }
    }
}

fn invalid() -> GameError {
    GameError::Rule(RuleError::InvalidPlan)
}

/// Only source categories which affect edge family/talent cancellation survive.
/// Ordinary ordinals distinguish sources, never identify runtime entities.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RayIncomingOwner {
    Ordinary {
        ordinal: u16,
    },
    Talent {
        talent: TalentId,
    },
    Condition {
        condition: ConditionId,
        source: ActorId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RayIncomingSource {
    pub owner: RayIncomingOwner,
    pub modifiers: Vec<Modifier>,
}
impl<'de> Deserialize<'de> for RayIncomingSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: RayIncomingOwner,
            #[serde(deserialize_with = "read_modifiers")]
            modifiers: Vec<Modifier>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            owner: f.owner,
            modifiers: f.modifiers,
        })
    }
}
fn read_modifiers<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Modifier>, D::Error> {
    Ok(
        crate::bounded::Bounded::<Modifier, { crate::limits::MODIFIERS_PER_SOURCE }>::deserialize(
            d,
        )?
        .0,
    )
}
pub(super) fn read_incoming<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<RayIncomingSource>, D::Error> {
    Ok(crate::bounded::Bounded::<RayIncomingSource, MAX_INCOMING_SOURCES>::deserialize(d)?.0)
}
fn read_footprint<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Location>, D::Error> {
    struct Cell(Location);
    impl<'de> Deserialize<'de> for Cell {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::bounded::location(d).map(Self)
        }
    }
    Ok(crate::bounded::Bounded::<Cell, 64>::deserialize(d)?
        .0
        .into_iter()
        .map(|v| v.0)
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RayPlan {
    pub actor: ActorId,
    pub ability: AbilityId,
    pub delivery: RayDelivery,
    pub origin: Placement,
    pub direction: StepDirection,
    pub launch_footprint: Vec<Location>,
    pub traversal: RayTraversal,
    pub resolution: RayResolution,
    pub effects: Vec<PlannedEffect>,
    pub descriptors: Vec<DescriptorId>,
    pub preparation: u64,
    pub recovery: u64,
    pub cost: Option<crate::costs::CostBundle>,
    pub cooldown: Option<std::num::NonZeroU64>,
    pub supply_costs: Vec<crate::rules::SupplyCost>,
    pub supplies: crate::supplies::SupplySelection,
}
impl<'de> Deserialize<'de> for RayPlan {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            ability: AbilityId,
            delivery: RayDelivery,
            origin: Placement,
            direction: StepDirection,
            #[serde(deserialize_with = "read_footprint")]
            launch_footprint: Vec<Location>,
            traversal: RayTraversal,
            resolution: RayResolution,
            #[serde(deserialize_with = "super::strikes::read_effects")]
            effects: Vec<PlannedEffect>,
            #[serde(deserialize_with = "super::strikes::read_descriptors")]
            descriptors: Vec<DescriptorId>,
            preparation: u64,
            recovery: u64,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            cost: Option<crate::costs::CostBundle>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            cooldown: Option<std::num::NonZeroU64>,
            #[serde(deserialize_with = "crate::rules::read_supply_costs")]
            supply_costs: Vec<crate::rules::SupplyCost>,
            supplies: crate::supplies::SupplySelection,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            actor: f.actor,
            ability: f.ability,
            delivery: f.delivery,
            origin: f.origin,
            direction: f.direction,
            launch_footprint: f.launch_footprint,
            traversal: f.traversal,
            resolution: f.resolution,
            effects: f.effects,
            descriptors: f.descriptors,
            preparation: f.preparation,
            recovery: f.recovery,
            cost: f.cost,
            cooldown: f.cooldown,
            supply_costs: f.supply_costs,
            supplies: f.supplies,
        })
    }
}

pub(crate) fn definition_shape_valid(definition: &crate::rules::AbilityDef) -> bool {
    let Some(delivery) = RayDelivery::from_invocation(definition.invocation) else {
        return false;
    };
    let StrikeSource::Authored { strikes } = &definition.strikes else {
        return false;
    };
    let [strike] = strikes.as_slice() else {
        return false;
    };
    let bounded =
        matches!(definition.reach, Reach::Sight { range } if (1..=MAX_RAY_VISITS).contains(&range));
    let until = definition.reach == Reach::UntilObstruction;
    if until && delivery != RayDelivery::FirstContact {
        return false;
    }
    if until && strike.check.is_none() {
        return !definition.basic_attack
            && strike.offset == 0
            && strike.unlock.is_none()
            && strike.riders.is_empty()
            && strike.descriptors.is_empty()
            && strike.on_failure.is_empty()
            && strike.on_success == [Effect::CancelSpecials];
    }
    if (until
        || strike
            .on_failure
            .iter()
            .any(|effect| matches!(effect, Effect::BalefulPolymorph { .. })))
        && !matches!(
            &strike.check,
            Some(crate::rules::CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named {
                    skill: crate::attributes::Skill::Agility
                },
                against: Against::Opposed {
                    skill: SkillSelector::Named {
                        skill: crate::attributes::Skill::Lore
                    }
                },
            })
        )
    {
        return false;
    }
    (bounded || until)
        && !definition.basic_attack
        && strike.offset == 0
        && strike.unlock.is_none()
        && strike.riders.is_empty()
        && matches!(
            &strike.check,
            Some(crate::rules::CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named { .. },
                against: Against::Opposed {
                    skill: SkillSelector::Named { .. }
                },
            })
        )
        && strike.on_success.is_empty()
        && !strike.on_failure.is_empty()
        && (!until
            || strike
                .on_failure
                .iter()
                .all(|effect| matches!(effect, Effect::Damage { .. }))
            || matches!(
                strike.on_failure.as_slice(),
                [Effect::BalefulPolymorph { .. }]
            ))
        && strike.on_failure.iter().all(|effect| match effect {
            Effect::Damage {
                damage: crate::rules::DamageExpr::Components { .. },
            } => true,
            Effect::ApplyCondition { .. }
            | Effect::ApplyConditionForTurns { .. }
            | Effect::ApplyPersistentCondition { .. }
            | Effect::RemoveCondition { .. } => delivery == RayDelivery::FirstContact,
            Effect::BalefulPolymorph { forms } => {
                delivery == RayDelivery::FirstContact
                    && strike.on_failure.len() == 1
                    && strike.descriptors.is_empty()
                    && crate::rules::valid_body_forms(forms)
                    && matches!(
                        &strike.check,
                        Some(crate::rules::CheckDef {
                            actor: CheckActor::Target,
                            skill: SkillSelector::Named {
                                skill: crate::attributes::Skill::Agility
                            },
                            against: Against::Opposed {
                                skill: SkillSelector::Named {
                                    skill: crate::attributes::Skill::Lore
                                }
                            },
                        })
                    )
            }
            Effect::TeleportRandom | Effect::TeleportToRayOrigin => {
                delivery == RayDelivery::FirstContact && strike.on_failure.len() == 1
            }
            _ => false,
        })
}
impl RayPlan {
    pub fn tokens(&self) -> &[TokenClaim] {
        self.resolution.tokens()
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let Some(definition) = rules.ability(&self.ability) else {
            return false;
        };
        if RayDelivery::from_invocation(definition.invocation) != Some(self.delivery)
            || !definition_shape_valid(definition)
        {
            return false;
        }
        let StrikeSource::Authored { strikes } = &definition.strikes else {
            return false;
        };
        let strike = &strikes[0];
        let traversal = match (self.traversal, &definition.reach) {
            (RayTraversal::FirstBody, Reach::UntilObstruction) => {
                self.delivery == RayDelivery::FirstContact
            }
            (RayTraversal::Bounded { clear_visits }, Reach::Sight { .. }) => {
                (1..=MAX_RAY_VISITS).contains(&clear_visits)
            }
            _ => false,
        };
        let payload = if matches!(self.resolution, RayResolution::Checkless) {
            &strike.on_success
        } else {
            &strike.on_failure
        };
        traversal
            && super::plan::valid_actor(self.actor)
            && self.origin.orientation < 24
            && self.origin.location.region.0 > 0
            && !self.launch_footprint.is_empty()
            && self.launch_footprint.len() <= 64
            && self.launch_footprint.windows(2).all(|p| p[0] < p[1])
            && self.launch_footprint.contains(&self.origin.location)
            && (1..=crate::limits::NUMERIC as u64).contains(&self.preparation)
            && self
                .preparation
                .checked_add(self.recovery)
                .is_some_and(|v| v <= crate::limits::NUMERIC as u64)
            && definition
                .cost
                .as_ref()
                .map(crate::costs::CostBundle::try_from)
                .transpose()
                .is_ok_and(|cost| self.cost == cost)
            && self.cooldown == definition.cooldown
            && self.supply_costs == definition.supply_costs
            && self.supplies.valid(self.supply_costs.len())
            && super::plan::effects_valid(&self.effects, rules)
            && super::plan::effects_match_definition(payload, &self.effects)
            && self.descriptors == strike.descriptors
            && self.resolution_valid(definition, strike, rules)
    }
    fn resolution_valid(
        &self,
        definition: &crate::rules::AbilityDef,
        strike: &crate::rules::StrikeDef,
        rules: &crate::rules::Ruleset,
    ) -> bool {
        let RayResolution::Opposed {
            source_skill,
            target_skill,
            source_bonus,
            source_edge,
            effect_edge,
            incoming,
            tokens,
        } = &self.resolution
        else {
            return self.traversal == RayTraversal::FirstBody
                && strike.check.is_none()
                && strike.on_success == [Effect::CancelSpecials]
                && strike.on_failure.is_empty();
        };
        let Some(crate::rules::CheckDef {
            actor: CheckActor::Target,
            skill: SkillSelector::Named { skill: target },
            against:
                Against::Opposed {
                    skill: SkillSelector::Named { skill: source },
                },
        }) = &strike.check
        else {
            return false;
        };
        source_skill == source && target_skill == target
            && [source_bonus,source_edge,effect_edge].iter().all(|v| v.unsigned_abs() <= crate::limits::NUMERIC as u64)
            && incoming.len() <= MAX_INCOMING_SOURCES
            && incoming.windows(2).all(|p| p[0].owner < p[1].owner)
            && incoming.iter().all(|row| row.valid(rules))
            && tokens.len() <= crate::limits::CONDITIONS
            && tokens.iter().all(|claim| {
                let token = &claim.reservation;
                token.holder == self.actor && token.qualification.is_none() && claim.site.strike == 0
                    && super::plan::valid_actor(token.source)
                    && rules.condition(&token.condition).is_some_and(|condition| match condition.lifecycle {
                        crate::rules::ConditionLifecycle::NextCheck => claim.site.check == (CheckPhase::Primary {side:CheckSide::Opposing})
                            && definition.outgoing_edge_policy.selects_token(condition)
                            && condition.modifiers.iter().any(|m| m.scope == Scope::Always),
                        crate::rules::ConditionLifecycle::NextCheckAgainst => claim.site.check == (CheckPhase::Primary {side:CheckSide::Initiating})
                            && incoming.iter().any(|row| matches!(&row.owner,RayIncomingOwner::Condition {condition,source} if condition == &token.condition && *source == token.source)),
                        crate::rules::ConditionLifecycle::Timed => false,
                    })
            })
            && tokens.iter().enumerate().all(|(i,claim)| !tokens[..i].iter().any(|old| old.reservation == claim.reservation))
    }
}
impl RayIncomingSource {
    fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        match &self.owner {
            RayIncomingOwner::Ordinary { ordinal }
                if usize::from(*ordinal) >= MAX_INCOMING_SOURCES =>
            {
                return false
            }
            RayIncomingOwner::Talent { talent } if rules.talent(talent).is_none() => return false,
            RayIncomingOwner::Condition { condition, source }
                if source.0 == u64::MAX || rules.condition(condition).is_none() =>
            {
                return false
            }
            _ => {}
        }
        !self.modifiers.is_empty() && self.modifiers.len() <= crate::limits::MODIFIERS_PER_SOURCE
            && self.modifiers.iter().all(|modifier| {
                matches!(modifier.scope, Scope::Always | Scope::VersusSource)
                    && matches!(&modifier.op, ModifierOp::Add { stat: Stat::IncomingEdge { defense: None }, value } | ModifierOp::Set { stat: Stat::IncomingEdge { defense: None }, value } if value.unsigned_abs() <= crate::limits::NUMERIC as u64)
            })
    }
}

impl Game {
    pub(super) fn compile_ray_plan(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        direction: StepDirection,
        supplies: crate::supplies::SupplySelection,
    ) -> Result<Plan, GameError> {
        self.compile_ray_source(actor, ability, direction, supplies, None)
    }

    pub(crate) fn compile_device_plan(
        &self,
        actor: ActorId,
        item: crate::ItemId,
        aim: crate::DeviceAim,
        preparation: u64,
    ) -> Result<Plan, GameError> {
        let source = self
            .items
            .get(&item)
            .filter(|i| i.location == crate::ItemLocation::Carried(actor))
            .ok_or(GameError::ItemUnavailable)?;
        let device = source
            .spec
            .device
            .as_ref()
            .ok_or(GameError::ItemUnavailable)?;
        let definition = self
            .content
            .rules
            .ability(&device.ability)
            .ok_or_else(invalid)?;
        let is_self = super::self_device_definition_valid(definition);
        let polymorph_self = super::self_polymorph_device_definition_valid(definition);
        // Knowledge gates only the submitted convenience aim. Once captured,
        // neither later identification nor its absence can change delivery.
        if matches!(aim, crate::DeviceAim::SelfTarget {}) {
            let known = !source.spec.concealed
                || self
                    .actors
                    .get(&actor)
                    .is_some_and(|a| a.knowledge.contains(&source.spec.identity));
            if !known || !(is_self || polymorph_self) {
                return Err(GameError::ItemUnavailable);
            }
        }
        if !is_self && !matches!(aim, crate::DeviceAim::SelfTarget {}) {
            let crate::DeviceAim::Direction { direction } = aim else {
                return Err(GameError::ItemUnavailable);
            };
            return self.compile_device_ray_plan(actor, item, direction, preparation);
        }
        let crate::rules::StrikeSource::Authored { strikes } = &definition.strikes else {
            return Err(invalid());
        };
        let plan = Plan::Zap {
            item,
            charges: source.charges.ok_or(GameError::ItemUnavailable)?,
            discovery: device.discovery,
            aim,
            delivery: super::DeviceDeliveryPlan::SelfEffect {
                plan: tor_world::Shared::new(super::SelfDevicePlan {
                    actor,
                    ability: device.ability.clone(),
                    preparation,
                    descriptors: strikes[0].descriptors.clone(),
                    effects: self.effect_plan(
                        actor,
                        if polymorph_self {
                            &strikes[0].on_failure
                        } else {
                            &strikes[0].on_success
                        },
                    )?,
                }),
            },
        };
        if !plan.valid(&self.content.rules) {
            return Err(invalid());
        }
        Ok(plan)
    }
    pub(crate) fn compile_device_ray_plan(
        &self,
        actor: ActorId,
        item: crate::ItemId,
        direction: StepDirection,
        preparation: u64,
    ) -> Result<Plan, GameError> {
        let source = self
            .items
            .get(&item)
            .filter(|i| i.location == crate::ItemLocation::Carried(actor))
            .ok_or(GameError::ItemUnavailable)?;
        let device = source
            .spec
            .device
            .as_ref()
            .ok_or(GameError::ItemUnavailable)?;
        let charges = source.charges.ok_or(GameError::ItemUnavailable)?;
        let plan = self.compile_ray_source(
            actor,
            &device.ability,
            direction,
            crate::supplies::SupplySelection::Carried,
            Some(preparation),
        )?;
        let Plan::Ray(ray) = plan else {
            return Err(invalid());
        };
        Ok(Plan::Zap {
            item,
            charges,
            discovery: device.discovery,
            aim: crate::DeviceAim::Direction { direction },
            delivery: super::DeviceDeliveryPlan::Ray { plan: ray },
        })
    }
    fn compile_ray_source(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        direction: StepDirection,
        supplies: crate::supplies::SupplySelection,
        item_preparation: Option<u64>,
    ) -> Result<Plan, GameError> {
        let caster = self.creature(actor).ok_or(GameError::UnknownActor)?;
        if caster.health().dead() {
            return Err(GameError::Rule(RuleError::Dead));
        }
        let rules = caster.build().ruleset();
        let definition = rules
            .ability(ability)
            .ok_or(GameError::Rule(RuleError::UnknownAbility))?;
        if item_preparation.is_none() && !caster.derived().abilities.contains(ability) {
            return Err(GameError::Rule(RuleError::UnknownAbility));
        }
        if !definition.invocation.is_directional_ray() || !definition_shape_valid(definition) {
            return Err(invalid());
        }
        if let Some(cooldown) = definition.cooldown {
            caster
                .validate_cooldown(ability, cooldown.get())
                .map_err(|_| GameError::Rule(RuleError::CoolingDown))?;
        }
        let StrikeSource::Authored { strikes } = &definition.strikes else {
            return Err(invalid());
        };
        let strike = &strikes[0];
        let source_check = match &strike.check {
            Some(crate::rules::CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named { skill: target },
                against:
                    Against::Opposed {
                        skill: SkillSelector::Named { skill: source },
                    },
            }) => Some((*source, *target)),
            None if definition.reach == Reach::UntilObstruction => None,
            _ => return Err(invalid()),
        };
        let effective_reach = if item_preparation.is_some() {
            definition.reach.clone()
        } else {
            caster
                .derived()
                .ability_reach(ability)
                .map_err(|_| invalid())?
        };
        let traversal = match effective_reach {
            Reach::Sight { range } => RayTraversal::Bounded {
                clear_visits: range,
            },
            Reach::UntilObstruction => RayTraversal::FirstBody,
            _ => return Err(invalid()),
        };
        let origin = Placement::capture(self, actor).ok_or(GameError::UnknownActor)?;
        let mut launch_footprint: Vec<_> = self
            .body_cells(
                origin.location,
                origin.orientation,
                &self.actors[&actor].body,
            )
            .ok_or(GameError::Blocked)?
            .into_iter()
            .map(|(cell, _)| cell)
            .collect();
        launch_footprint.sort();
        launch_footprint.dedup();
        let modifiers = caster.derived().ability_values(ability);
        let resolution = if let Some((source_skill, target_skill)) = source_check {
            let mut selected = caster.conditions().qualifying_tokens(rules, actor, false);
            selected.retain(|token| {
                rules.condition(&token.condition).is_some_and(|def| {
                    definition.outgoing_edge_policy.selects_token(def)
                        && def.modifiers.iter().any(|m| m.scope == Scope::Always)
                })
            });
            let source_edge = caster
                .derived()
                .targetless_ray_edge(
                    rules,
                    actor,
                    ability,
                    definition.outgoing_edge_policy,
                    &selected,
                )
                .map_err(|_| invalid())?;
            let effect_edge = caster
                .derived()
                .targetless_ray_effect_edge(rules, definition.outgoing_edge_policy)
                .map_err(|_| invalid())?;
            let mut incoming_tokens = caster.conditions().qualifying_tokens(rules, actor, true);
            incoming_tokens.retain(|token| {
                rules.condition(&token.condition).is_some_and(|def| {
                    def.modifiers.iter().any(|m| {
                        matches!(
                            m.op,
                            ModifierOp::Add {
                                stat: Stat::IncomingEdge { defense: None },
                                ..
                            } | ModifierOp::Set {
                                stat: Stat::IncomingEdge { defense: None },
                                ..
                            }
                        )
                    })
                })
            });
            let incoming = caster
                .derived()
                .ray_incoming_sources(rules, &incoming_tokens);
            let tokens = selected
                .into_iter()
                .map(|reservation| TokenClaim {
                    reservation,
                    site: CheckSite {
                        strike: 0,
                        check: CheckPhase::Primary {
                            side: CheckSide::Opposing,
                        },
                    },
                })
                .chain(incoming_tokens.into_iter().map(|reservation| TokenClaim {
                    reservation,
                    site: CheckSite {
                        strike: 0,
                        check: CheckPhase::Primary {
                            side: CheckSide::Initiating,
                        },
                    },
                }))
                .collect();
            let source_bonus = named_bonus(caster.derived(), source_skill)
                .checked_add(i64::from(modifiers.check_bonus))
                .ok_or_else(invalid)?;
            RayResolution::Opposed {
                source_skill,
                target_skill,
                source_bonus,
                source_edge,
                effect_edge,
                incoming,
                tokens,
            }
        } else {
            RayResolution::Checkless
        };
        let timing = match item_preparation {
            Some(preparation) => crate::creatures::AbilityTiming {
                preparation,
                recovery: 0,
            },
            None => self.ability_timing(actor, ability)?,
        };
        let plan = RayPlan {
            actor,
            ability: ability.clone(),
            delivery: RayDelivery::from_invocation(definition.invocation).ok_or_else(invalid)?,
            origin,
            direction,
            launch_footprint,
            traversal,
            resolution,
            effects: self.compile_effects(
                actor,
                if source_check.is_none() {
                    &strike.on_success
                } else {
                    &strike.on_failure
                },
                &modifiers,
            )?,
            descriptors: strike.descriptors.clone(),
            preparation: timing.preparation,
            recovery: timing.recovery,
            cost: self.ability_cost(ability)?,
            cooldown: definition.cooldown,
            supply_costs: definition.supply_costs.clone(),
            supplies,
        };
        if !plan.valid(rules) {
            return Err(invalid());
        }
        Ok(Plan::Ray(tor_world::Shared::new(plan)))
    }
}
pub(super) fn named_bonus(
    creature: &crate::creatures::DerivedCreature,
    skill: crate::attributes::Skill,
) -> i64 {
    crate::attributes::SkillCheck {
        skill,
        binding: creature.binding(),
        modifier: 0,
        threshold: 0,
    }
    .bonus(creature.attributes, creature.skills)
}

/// Release-local state is bounded by visits and probes and is never serialized.
/// Reversals do not create an exit from the current contiguous body pass.
#[derive(Default)]
pub(super) struct RayEncounters {
    previous: BTreeSet<ActorId>,
    checks: BTreeMap<ActorId, u16>,
    exited_source: bool,
}
impl RayEncounters {
    pub(super) fn enter(
        &mut self,
        occupants: BTreeSet<ActorId>,
        source: ActorId,
        in_launch_footprint: bool,
    ) -> Vec<(ActorId, u16)> {
        self.exited_source |= !in_launch_footprint;
        let checks = occupants
            .iter()
            .filter(|id| !self.previous.contains(id) && (**id != source || self.exited_source))
            .map(|id| (*id, self.checks.get(id).copied().unwrap_or(0)))
            .collect();
        self.previous = occupants;
        checks
    }
    pub(super) fn record(&mut self, target: ActorId) -> Result<(), GameError> {
        if !self.checks.contains_key(&target) && self.checks.len() >= MAX_RAY_ENCOUNTERS {
            return Err(invalid());
        }
        let count = self.checks.entry(target).or_default();
        *count = count
            .checked_add(1)
            .filter(|v| *v <= MAX_RAY_VISITS)
            .ok_or_else(invalid)?;
        Ok(())
    }
}

impl Game {
    /// The ordinary preparation transaction already owns payment and recovery.
    /// Every probe, roll, token and effect below is part of that same release.
    #[cfg(test)]
    pub(crate) fn resolve_ray(
        &mut self,
        ray: &RayPlan,
        intention: Option<crate::IntentionId>,
    ) -> Result<(), GameError> {
        self.resolve_ray_release(
            ray,
            intention,
            None,
            crate::event_history::SourceAbilityDisclosure::Public,
        )
    }

    pub(crate) fn resolve_prepared_ray(
        &mut self,
        preparation: &crate::combat::Preparation,
    ) -> Result<(), GameError> {
        let ray = preparation.plan.as_ray().ok_or_else(invalid)?;
        use crate::event_history::SourceAbilityDisclosure;
        let disclosure = match &preparation.plan {
            Plan::Zap {
                item,
                charges,
                discovery,
                ..
            } => {
                // The enclosing release transaction has already spent the exact
                // held charge. Discovery and these receipts commit together; an
                // empty device never enters this release path.
                if *charges == 0 {
                    return Err(invalid());
                }
                let source = self.items.get(item).ok_or(GameError::ItemUnavailable)?;
                let actor = self.actors.get(&ray.actor).ok_or(GameError::UnknownActor)?;
                if !source.spec.concealed
                    || actor.knowledge.contains(&source.spec.identity)
                    || *discovery == crate::ItemDiscovery::OnUse
                {
                    SourceAbilityDisclosure::InitiatorOnly
                } else {
                    SourceAbilityDisclosure::Concealed
                }
            }
            _ => SourceAbilityDisclosure::Public,
        };
        self.resolve_ray_release(ray, preparation.intention, Some(preparation), disclosure)
    }

    fn resolve_ray_release(
        &mut self,
        ray: &RayPlan,
        intention: Option<crate::IntentionId>,
        preparation: Option<&crate::combat::Preparation>,
        disclosure: crate::event_history::SourceAbilityDisclosure,
    ) -> Result<(), GameError> {
        if ray.traversal == RayTraversal::FirstBody {
            return self.resolve_contact_release(ray, intention, preparation, disclosure);
        }
        let RayTraversal::Bounded { clear_visits } = ray.traversal else {
            return Err(invalid());
        };
        let mut rng = self.rules_rng;
        // A launch has no actor recipient. Keep its optional receipt locally and
        // attach it only to diagnostics for real, live encounters.
        let mut launch_trace = self
            .combat_diagnostics
            .as_ref()
            .map(|_| crate::resolution_diagnostics::ResolutionTrace::default());
        let threshold = if let Some(observer) = launch_trace.as_mut() {
            super::resolve::resolve_ray_launch(ray, &mut rng, observer)
        } else {
            super::resolve::resolve_ray_launch(ray, &mut rng, &mut ())
        };
        self.rules_rng = rng;
        let mut at = ray.origin.location;
        let mut direction = Direction::from(ray.direction).rotated(ray.origin.orientation);
        let mut visits = 0;
        let mut reversals = 0;
        let mut encounters = RayEncounters::default();
        let mut first_actual_check = true;
        let mut budget = super::resolve::StrikeBudget::ray(ray);
        let mut activity = crate::lifecycle::DamageActivityGuard::default();
        for _ in 0..MAX_RAY_PROBES {
            if visits == clear_visits
                || reversals == MAX_RAY_REVERSALS
                || self.region_frozen(at.region)
            {
                break;
            }
            match self.world.ray_step(at, direction) {
                RayStep::Solid => {
                    if ray.delivery == RayDelivery::FirstContact {
                        break;
                    }
                    let (x, y, z) = direction.delta();
                    direction =
                        Direction::from_delta([-i64::from(x), -i64::from(y), -i64::from(z)])
                            .ok_or_else(invalid)?;
                    reversals += 1;
                }
                RayStep::Unavailable | RayStep::Ambiguous => break,
                RayStep::Clear { to, rotation } => {
                    if self.region_frozen(to.region) {
                        break;
                    }
                    at = to;
                    direction = direction.rotated(rotation);
                    visits += 1;
                    let occupants: BTreeSet<_> = self
                        .actors
                        .at(&self.world, at)
                        .keys()
                        .copied()
                        .filter(|id| {
                            self.actors[id].alive()
                                && !self.actor_frozen(*id)
                                && self.creature(*id).is_some()
                        })
                        .collect();
                    for (target, previous) in encounters.enter(
                        occupants,
                        ray.actor,
                        ray.launch_footprint.binary_search(&at).is_ok(),
                    ) {
                        self.settle_actor_time(target)?;
                        let diagnostic = preparation.and_then(|preparation| {
                            self.begin_combat_diagnostic(ray.actor, target, preparation)
                        });
                        if diagnostic.is_some() {
                            if let Some(diagnostics) = self.combat_diagnostics.as_mut() {
                                diagnostics.active = launch_trace.clone();
                            }
                        }
                        let outcome = self.resolve_ray_encounter(
                            ray,
                            target,
                            previous,
                            threshold,
                            first_actual_check,
                            intention,
                            disclosure,
                            &mut budget,
                            &mut activity,
                        )?;
                        let check = outcome.check;
                        self.finish_combat_diagnostic(diagnostic, outcome);
                        // Contact consumes this directional release even when
                        // the target dodges or its whole-payload immunity wins.
                        if ray.delivery == RayDelivery::FirstContact {
                            return Ok(());
                        }
                        if let Some(check) = check {
                            encounters.record(target)?;
                            first_actual_check = false;
                            if !check.success {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Game {
    /// Load-only probe for bounded first-contact body effects. This follows the
    /// same visit/source-exclusion/frozen-region rules as bounded release, but
    /// makes no checks, payments, form selections or state changes.
    pub(crate) fn bounded_body_effect_regions(
        &self,
        ray: &RayPlan,
    ) -> Result<BTreeSet<tor_world::RegionId>, GameError> {
        let mut regions = BTreeSet::new();
        let RayTraversal::Bounded { clear_visits } = ray.traversal else {
            return Ok(regions);
        };
        if ray.delivery != RayDelivery::FirstContact
            || !ray.effects.iter().any(|effect| {
                matches!(
                    effect,
                    PlannedEffect::BalefulPolymorph { .. } | PlannedEffect::CancelSpecials
                )
            })
        {
            return Ok(regions);
        }
        let mut at = ray.origin.location;
        let mut direction = Direction::from(ray.direction).rotated(ray.origin.orientation);
        for _ in 0..clear_visits.min(MAX_RAY_PROBES) {
            if self.region_frozen(at.region) {
                break;
            }
            let (step, dependencies) = self.world.ray_step_dependencies(at, direction);
            let missing = dependencies
                .iter()
                .any(|id| self.world.region(*id).is_none());
            regions.extend(dependencies);
            if missing {
                break;
            }
            let RayStep::Clear { to, rotation } = step else {
                break;
            };
            if self.region_frozen(to.region) {
                break;
            }
            at = to;
            direction = direction.rotated(rotation);
            let target = self.actors.at(&self.world, at).keys().copied().find(|id| {
                self.actors[id].alive()
                    && !self.actor_frozen(*id)
                    && self.creature(*id).is_some()
                    && (*id != ray.actor || ray.launch_footprint.binary_search(&at).is_err())
            });
            if let Some(target) = target {
                regions.extend(self.body_effect_required_regions(target, &ray.effects)?);
                break;
            }
        }
        Ok(regions)
    }
}
