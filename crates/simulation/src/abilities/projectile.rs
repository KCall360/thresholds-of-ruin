//! One paid physical release. Its path and recipient are discovered in the real
//! world at resolution; source operands never borrow a wand's reflection rules.
use super::{
    CapturedWeaponProfile, FrozenWeaponSelection, ProjectileDeliverySelection,
    ThrownSourceSelection,
};
use crate::projectile_profiles::{ProjectileDeliveryKind, ProjectileSpec};
use crate::weapon_profiles::RangeBands;
use crate::{ActorId, EquipmentSlotId, ItemId};
use serde::{Deserialize, Serialize};

pub(super) fn definition_shape_valid(definition: &crate::rules::AbilityDef) -> bool {
    definition.invocation == super::InvocationKind::DirectionalProjectile
        && matches!(
            definition.strikes,
            crate::rules::StrikeSource::Projectile { .. }
        )
        && matches!(definition.reach, crate::rules::Reach::Projectile)
        && matches!(
            definition.timing.source,
            crate::rules::TimingSource::SelectedWeapon
                | crate::rules::TimingSource::SelectedWeaponAdjusted { .. }
        )
        && definition.interaction == crate::rules::Interaction::HostileToTarget
        && !definition.basic_attack
        && definition.supply_costs.is_empty()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectileSource {
    ThrowEquipped {
        weapon: FrozenWeaponSelection,
    },
    ThrowCarried {
        item: ItemId,
        socket: EquipmentSlotId,
        secondary: bool,
        allocation: crate::equipment_capture::FrozenEquipAllocation,
    },
    Launch {
        weapon: FrozenWeaponSelection,
        ammunition: ItemId,
        payload: ProjectileSpec,
    },
}
impl<'de> Deserialize<'de> for ProjectileSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ThrowEquipped {
                weapon: FrozenWeaponSelection,
            },
            ThrowCarried {
                item: ItemId,
                socket: EquipmentSlotId,
                secondary: bool,
                allocation: crate::equipment_capture::FrozenEquipAllocation,
            },
            Launch {
                weapon: FrozenWeaponSelection,
                ammunition: ItemId,
                payload: ProjectileSpec,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::ThrowEquipped { weapon } => Self::ThrowEquipped { weapon },
            Fields::ThrowCarried {
                item,
                socket,
                secondary,
                allocation,
            } => Self::ThrowCarried {
                item,
                socket,
                secondary,
                allocation,
            },
            Fields::Launch {
                weapon,
                ammunition,
                payload,
            } => Self::Launch {
                weapon,
                ammunition,
                payload,
            },
        })
    }
}
impl ProjectileSource {
    pub(crate) fn item(&self) -> Option<ItemId> {
        match self {
            Self::ThrowEquipped {
                weapon: FrozenWeaponSelection::Equipped { item, .. },
            } => Some(*item),
            Self::ThrowCarried { item, .. } => Some(*item),
            Self::Launch { ammunition, .. } => Some(*ammunition),
            _ => None,
        }
    }
    fn profile(&self) -> Option<&CapturedWeaponProfile> {
        match self {
            Self::ThrowEquipped { weapon } | Self::Launch { weapon, .. } => match weapon {
                FrozenWeaponSelection::Equipped { profile, .. } => Some(profile),
                _ => None,
            },
            Self::ThrowCarried { allocation, .. } => Some(&allocation.target),
        }
    }
    pub(crate) fn range(&self, rules: &crate::rules::Ruleset) -> Option<RangeBands> {
        let profile = self.profile()?.resolve(rules).ok()??;
        match self {
            Self::ThrowEquipped { .. } | Self::ThrowCarried { .. } => profile.thrown(),
            Self::Launch { payload, .. } => {
                let launcher = profile.launcher()?;
                payload.compatible_with(launcher).then_some(launcher.range)
            }
        }
    }
    pub(crate) fn cost(
        &self,
        rules: &crate::rules::Ruleset,
        base: Option<crate::costs::CostBundle>,
    ) -> Result<Option<crate::costs::CostBundle>, crate::GameError> {
        match self {
            Self::ThrowEquipped { weapon } | Self::Launch { weapon, .. } => {
                weapon.cost(rules, base)
            }
            Self::ThrowCarried {
                allocation,
                secondary,
                ..
            } => super::weapon_selection::selected_weapon_cost(
                rules,
                &allocation.target,
                *secondary,
                base,
            ),
        }
    }
    fn matches(&self, selection: ProjectileDeliverySelection) -> bool {
        match (self, selection) {
            (
                Self::ThrowEquipped {
                    weapon: FrozenWeaponSelection::Equipped { item, socket, .. },
                },
                ProjectileDeliverySelection::Throw {
                    source:
                        ThrownSourceSelection::Equipped {
                            item: wanted,
                            socket: selected,
                        },
                },
            ) => *item == wanted && *socket == selected,
            (
                Self::ThrowCarried { item, socket, .. },
                ProjectileDeliverySelection::Throw {
                    source:
                        ThrownSourceSelection::Carried {
                            item: wanted,
                            socket: selected,
                        },
                },
            ) => *item == wanted && *socket == selected,
            (
                Self::Launch {
                    weapon: FrozenWeaponSelection::Equipped { item, socket, .. },
                    ammunition,
                    ..
                },
                ProjectileDeliverySelection::Launch {
                    launcher,
                    ammunition: wanted,
                },
            ) => {
                *item == launcher.item
                    && *socket == launcher.socket
                    && *ammunition == wanted
                    && *item != wanted
            }
            _ => false,
        }
    }
    fn kind(&self) -> ProjectileDeliveryKind {
        match self {
            Self::Launch { .. } => ProjectileDeliveryKind::Launch,
            _ => ProjectileDeliveryKind::Throw,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProjectilePlan {
    pub actor: ActorId,
    pub ability: crate::content_ids::AbilityId,
    pub origin: super::Placement,
    pub direction: super::StepDirection,
    pub selection: ProjectileDeliverySelection,
    pub source: ProjectileSource,
    pub launch_footprint: Vec<tor_world::Location>,
    pub range: RangeBands,
    pub bonus: i64,
    /// Own perceived nearby threats produce at most one distraction penalty.
    pub threatened: bool,
    pub outgoing: crate::creatures::CapturedAttackEdges,
    pub effects: Vec<super::PlannedEffect>,
    pub tokens: Vec<super::TokenClaim>,
    /// These are unclaimed conditional candidates, not paid token guarantees.
    pub conditional_tokens: Vec<crate::conditions::TokenReservation>,
    pub preparation: u64,
    pub recovery: u64,
    pub cost: Option<crate::costs::CostBundle>,
    pub cooldown: Option<std::num::NonZeroU64>,
}
fn read_footprint<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<tor_world::Location>, D::Error> {
    struct Cell(tor_world::Location);
    impl<'de> Deserialize<'de> for Cell {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::bounded::location(d).map(Self)
        }
    }
    Ok(crate::bounded::Bounded::<Cell, 64>::deserialize(d)?
        .0
        .into_iter()
        .map(|cell| cell.0)
        .collect())
}
impl<'de> Deserialize<'de> for ProjectilePlan {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            actor: ActorId,
            ability: crate::content_ids::AbilityId,
            origin: super::Placement,
            direction: super::StepDirection,
            selection: ProjectileDeliverySelection,
            source: ProjectileSource,
            #[serde(deserialize_with = "read_footprint")]
            launch_footprint: Vec<tor_world::Location>,
            range: RangeBands,
            bonus: i64,
            threatened: bool,
            outgoing: crate::creatures::CapturedAttackEdges,
            #[serde(deserialize_with = "super::strikes::read_effects")]
            effects: Vec<super::PlannedEffect>,
            #[serde(deserialize_with = "super::strikes::read_claims")]
            tokens: Vec<super::TokenClaim>,
            conditional_tokens: crate::bounded::Bounded<
                crate::conditions::TokenReservation,
                { crate::limits::CONDITIONS },
            >,
            preparation: u64,
            recovery: u64,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            cost: Option<crate::costs::CostBundle>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            cooldown: Option<std::num::NonZeroU64>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            actor: f.actor,
            ability: f.ability,
            origin: f.origin,
            direction: f.direction,
            selection: f.selection,
            source: f.source,
            launch_footprint: f.launch_footprint,
            range: f.range,
            bonus: f.bonus,
            threatened: f.threatened,
            outgoing: f.outgoing,
            effects: f.effects,
            tokens: f.tokens,
            conditional_tokens: f.conditional_tokens.0,
            preparation: f.preparation,
            recovery: f.recovery,
            cost: f.cost,
            cooldown: f.cooldown,
        })
    }
}
impl ProjectilePlan {
    pub(crate) fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let Some(definition) = rules.ability(&self.ability) else {
            return false;
        };
        let crate::rules::StrikeSource::Projectile { delivery } = definition.strikes else {
            return false;
        };
        definition_shape_valid(definition)
            && delivery == self.source.kind()
            && self.actor.0 > 0
            && self.actor.0 < u64::MAX
            && self.selection.valid()
            && self.source.matches(self.selection)
            && self
                .source
                .item()
                .is_some_and(|item| item.0 > 0 && item.0 < u64::MAX)
            && self.range.validate().is_ok()
            && self.source.range(rules) == Some(self.range)
            && self.bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
            && self.outgoing.valid(rules, &self.ability)
            && matches!(self.effects.as_slice(), [super::PlannedEffect::Damage { damage }] if rules.validate_damage(damage).is_ok())
            && (1..=crate::limits::NUMERIC as u64).contains(&self.preparation)
            && (1..=crate::limits::NUMERIC as u64).contains(&self.recovery)
            && self.cooldown == definition.cooldown
            && definition
                .cost
                .as_ref()
                .map(crate::costs::CostBundle::try_from)
                .transpose()
                .ok()
                .and_then(|base| self.source.cost(rules, base).ok())
                == Some(self.cost)
            && self
                .preparation
                .checked_add(self.recovery)
                .is_some_and(|time| time <= crate::limits::NUMERIC as u64)
            && match &self.source {
                ProjectileSource::Launch { payload, .. } => {
                    payload.validate_shape().is_ok()
                        && rules.validate_damage(&payload.damage).is_ok()
                }
                ProjectileSource::ThrowCarried {
                    item,
                    socket,
                    allocation,
                    ..
                } => allocation.valid_shape(rules, *item, *socket),
                ProjectileSource::ThrowEquipped { .. } => true,
            }
            && self.tokens.len() + self.conditional_tokens.len() <= crate::limits::CONDITIONS
            && self.tokens.iter().all(|claim| {
                claim.site.strike == 0
                    && claim.site.check
                        == super::CheckPhase::Primary {
                            side: super::CheckSide::Initiating,
                        }
                    && claim.reservation.qualification.is_none()
            })
            && self.conditional_tokens.iter().all(|token| {
                token
                    .qualification
                    .as_ref()
                    .is_some_and(|scope| scope.matches(scope.opponent, Some(&self.ability), rules))
            })
            && self
                .tokens
                .iter()
                .map(|claim| &claim.reservation)
                .chain(&self.conditional_tokens)
                .all(|token| {
                    token.holder == self.actor
                        && super::plan::valid_actor(token.source)
                        && self.outgoing.contains_token(token)
                        && rules.condition(&token.condition).is_some_and(|condition| {
                            condition.lifecycle == crate::rules::ConditionLifecycle::NextCheck
                                && definition.outgoing_edge_policy.selects_token(condition)
                                && token
                                    .qualification
                                    .as_ref()
                                    .is_none_or(|scope| scope.valid(condition, rules))
                        })
                })
            && {
                let tokens: Vec<_> = self
                    .tokens
                    .iter()
                    .map(|claim| &claim.reservation)
                    .chain(&self.conditional_tokens)
                    .collect();
                tokens.iter().enumerate().all(|(i, token)| {
                    tokens[..i]
                        .iter()
                        .all(|prior| prior.generation != token.generation)
                })
            }
            && !self.launch_footprint.is_empty()
            && self.launch_footprint.len() <= 64
            && self
                .launch_footprint
                .windows(2)
                .all(|rows| rows[0] < rows[1])
    }
}

fn invalid() -> crate::GameError {
    crate::GameError::Rule(super::RuleError::InvalidPlan)
}
impl crate::Game {
    /// Resolves only the actor's exact owned delivery operands. No flight or
    /// recipient lookup occurs while choosing a weapon or ammunition.
    fn capture_projectile_source(
        &self,
        actor: ActorId,
        selection: ProjectileDeliverySelection,
    ) -> Result<(ProjectileSource, crate::creatures::SelectedAttack), crate::GameError> {
        if !selection.valid() {
            return Err(invalid());
        }
        if !self.projectile_source_eligible(actor, selection) {
            return Err(crate::GameError::ItemUnavailable);
        }
        let creature = self.creature(actor).ok_or(crate::GameError::UnknownActor)?;
        let (source, mut attack, payload_hit) = match selection {
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Equipped { item, socket },
            } => {
                let selected = self.capture_selected_weapon(
                    actor,
                    super::WeaponSelection::Equipped { item, socket },
                )?;
                (
                    ProjectileSource::ThrowEquipped {
                        weapon: selected.frozen,
                    },
                    selected.attack,
                    0,
                )
            }
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Carried { item, socket },
            } => {
                let allocation = self.capture_equip_allocation(actor, item, socket)?;
                let gear = self
                    .items
                    .get(&item)
                    .and_then(|item| item.equipment_view())
                    .ok_or(crate::GameError::ItemUnavailable)?;
                let weapon = gear.attack.ok_or(crate::GameError::ItemUnavailable)?;
                let mut selected = creature.derived().natural_attack.clone();
                selected.attack = crate::attacks::SelectedMeleeAttack::Weapon(weapon);
                selected.source = crate::creatures::SourceId::Equipment(item);
                selected.socket = Some(socket);
                (
                    ProjectileSource::ThrowCarried {
                        item,
                        socket,
                        secondary: self.actors[&actor].anatomy.primary_attack_socket
                            != crate::equipment_capacity::PrimaryAttackSocket::Socket {
                                slot: socket,
                            },
                        allocation,
                    },
                    selected,
                    0,
                )
            }
            ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => {
                let selected = self.capture_selected_weapon(
                    actor,
                    super::WeaponSelection::Equipped {
                        item: launcher.item,
                        socket: launcher.socket,
                    },
                )?;
                let ammo = self
                    .items
                    .get(&ammunition)
                    .filter(|item| item.location == crate::ItemLocation::Carried(actor))
                    .ok_or(crate::GameError::ItemUnavailable)?;
                let payload = ammo
                    .spec
                    .projectile
                    .clone()
                    .ok_or(crate::GameError::ItemUnavailable)?;
                let crate::attacks::SelectedMeleeAttack::Weapon(weapon) = &selected.attack.attack
                else {
                    return Err(invalid());
                };
                // The payload replaces the launcher base expression. Each
                // instance's signed enchantment enters exactly once.
                let damage_enchantment = weapon
                    .damage_enchantment()
                    .checked_add(ammo.condition.damage_enchantment)
                    .ok_or_else(invalid)?;
                let released = crate::attacks::MeleeAttack::new(
                    weapon.skill(),
                    weapon.bonus(),
                    weapon.wind_up(),
                    weapon.recovery(),
                    payload.damage.clone(),
                )
                .map_err(|_| invalid())?
                .with_enchantment(0, damage_enchantment)
                .map_err(|_| crate::GameError::Rule(super::RuleError::NumericBudget))?;
                let mut attack = selected.attack;
                attack.attack = crate::attacks::SelectedMeleeAttack::Weapon(released);
                (
                    ProjectileSource::Launch {
                        weapon: selected.frozen,
                        ammunition,
                        payload,
                    },
                    attack,
                    ammo.condition.hit_enchantment,
                )
            }
        };
        if source.range(self.content.rules()).is_none() || !source.matches(selection) {
            return Err(crate::GameError::ItemUnavailable);
        }
        let item = source.item().ok_or_else(invalid)?;
        if self
            .items
            .get(&item)
            .and_then(|item| item.free_quantity(None))
            .is_none_or(|free| free == 0)
        {
            return Err(crate::GameError::ItemUnavailable);
        }
        attack.refresh_operands(
            creature.derived().binding(),
            creature.derived().attributes,
            creature.derived().skills,
        );
        // Ammunition is a second instance contribution, not an authored weapon
        // bonus. Keep it in the wide compiled check lane rather than reapplying
        // MeleeAttack's narrower per-weapon definition bound.
        attack.check_bonus = attack
            .check_bonus
            .checked_add(i64::from(payload_hit))
            .ok_or_else(invalid)?;
        Ok((source, attack))
    }
}

impl ProjectileSource {
    fn source_valid(
        &self,
        game: &crate::Game,
        actor: ActorId,
        sources: super::PlanSourceContext<'_>,
    ) -> bool {
        match self {
            Self::ThrowEquipped { weapon } => {
                matches!(weapon, FrozenWeaponSelection::Equipped { .. })
                    && game.frozen_weapon_source_valid_with_context(weapon, sources)
            }
            Self::ThrowCarried {
                item,
                socket,
                secondary,
                allocation,
            } => {
                allocation.source_valid_with_context(game, actor, *item, *socket, sources)
                    && allocation
                        .anatomy_with_context(game, actor, sources)
                        .is_some_and(|anatomy| {
                            *secondary
                                == (anatomy.primary_attack_socket
                                    != crate::equipment_capacity::PrimaryAttackSocket::Socket {
                                        slot: *socket,
                                    })
                        })
            }
            Self::Launch {
                weapon,
                ammunition,
                payload,
            } => {
                matches!(weapon, FrozenWeaponSelection::Equipped { .. })
                    && game.frozen_weapon_source_valid_with_context(weapon, sources)
                    && ammunition.0 > 0
                    && ammunition.0 < game.next_item_id
                    && sources
                        .item(game, *ammunition)
                        .is_none_or(|item| item.spec.projectile.as_ref() == Some(payload))
            }
        }
    }
    pub(crate) fn available(&self, game: &crate::Game, actor: ActorId) -> bool {
        if !self.source_valid(game, actor, super::PlanSourceContext::Loaded) {
            return false;
        }
        match self {
            Self::ThrowEquipped { weapon } => game.frozen_weapon_available(actor, weapon),
            Self::ThrowCarried {
                item,
                socket,
                allocation,
                ..
            } => game.frozen_equip_available(actor, *item, *socket, allocation),
            Self::Launch {
                weapon, ammunition, ..
            } => {
                game.frozen_weapon_available(actor, weapon)
                    && game
                        .items
                        .get(ammunition)
                        .is_some_and(|item| item.location == crate::ItemLocation::Carried(actor))
            }
        }
    }
}

impl crate::Game {
    pub(super) fn capture_projectile_plan(
        &self,
        actor: ActorId,
        ability: &crate::content_ids::AbilityId,
        direction: super::StepDirection,
        selection: ProjectileDeliverySelection,
    ) -> Result<ProjectilePlan, crate::GameError> {
        let caster = self.creature(actor).ok_or(crate::GameError::UnknownActor)?;
        if caster.health().dead() {
            return Err(crate::GameError::Rule(super::RuleError::Dead));
        }
        let rules = caster.build().ruleset();
        let definition = rules
            .ability(ability)
            .filter(|_| caster.derived().abilities.contains(ability))
            .ok_or(crate::GameError::Rule(super::RuleError::UnknownAbility))?;
        if !definition_shape_valid(definition) {
            return Err(invalid());
        }
        if let Some(cooldown) = definition.cooldown {
            caster
                .validate_cooldown(ability, cooldown.get())
                .map_err(|_| crate::GameError::Rule(super::RuleError::CoolingDown))?;
        }
        let (source, attack) = self.capture_projectile_source(actor, selection)?;
        let crate::rules::StrikeSource::Projectile { delivery } = definition.strikes else {
            return Err(invalid());
        };
        if source.kind() != delivery {
            return Err(invalid());
        }
        let modifiers = caster.derived().ability_values(ability);
        let timing = crate::creatures::ability_timing_for_attack(
            caster.derived(),
            caster.build(),
            definition,
            &modifiers,
            &attack,
        )
        .map_err(|_| invalid())?;
        let outgoing =
            crate::creatures::CapturedAttackEdges::capture(caster.derived(), rules, ability);
        let candidates = caster
            .conditions()
            .targetless_attack_tokens(rules, actor, ability)
            .into_iter()
            .filter(|token| {
                outgoing.contains_token(token)
                    && rules.condition(&token.condition).is_some_and(|condition| {
                        definition.outgoing_edge_policy.selects_token(condition)
                    })
            });
        let mut tokens = Vec::new();
        let mut conditional_tokens = Vec::new();
        for reservation in candidates {
            if reservation.qualification.is_some() {
                conditional_tokens.push(reservation);
            } else {
                tokens.push(super::TokenClaim {
                    reservation,
                    site: super::CheckSite {
                        strike: 0,
                        check: super::CheckPhase::Primary {
                            side: super::CheckSide::Initiating,
                        },
                    },
                });
            }
        }
        let origin =
            super::Placement::capture(self, actor).ok_or(crate::GameError::UnknownActor)?;
        let mut launch_footprint: Vec<_> = self
            .body_cells(
                origin.location,
                origin.orientation,
                &self.actors[&actor].body,
            )
            .ok_or(crate::GameError::Blocked)?
            .into_iter()
            .map(|(cell, _)| cell)
            .collect();
        launch_footprint.sort();
        launch_footprint.dedup();
        // Initial content has ordinary melee reach. This uses only the shooter's
        // disclosed scene; future extended reach needs a lawful remembered fact.
        let threatened = self.scene_snapshot(actor).is_ok_and(|(scene, _)| {
            self.visible_threats(actor)
                .iter()
                .any(|threat| self.melee_reachable_in_scene(actor, threat.actor, &scene))
        });
        // This closed delivery has exactly one selected-weapon damage payload.
        // Reuse the ordinary damage arithmetic, distinguishing its numeric
        // composition limit from malformed authored or captured structure.
        let effects = vec![super::PlannedEffect::Damage {
            damage: attack
                .attack
                .damage_with_modifiers(modifiers.dice, modifiers.flat, 0)
                .map_err(|_| crate::GameError::Rule(super::RuleError::NumericBudget))?,
        }];
        let bonus = attack
            .check_bonus
            .checked_add(i64::from(modifiers.check_bonus))
            .filter(|bonus| (-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(bonus))
            .ok_or(crate::GameError::Rule(super::RuleError::NumericBudget))?;
        let cost = source
            .cost(
                rules,
                definition
                    .cost
                    .as_ref()
                    .map(crate::costs::CostBundle::try_from)
                    .transpose()
                    .map_err(|_| invalid())?,
            )
            .map_err(|_| invalid())?;
        let plan = ProjectilePlan {
            actor,
            ability: ability.clone(),
            origin,
            direction,
            selection,
            range: source.range(rules).ok_or_else(invalid)?,
            source,
            launch_footprint,
            bonus,
            threatened,
            outgoing,
            effects,
            tokens,
            conditional_tokens,
            preparation: timing.preparation,
            recovery: timing.recovery,
            cost,
            cooldown: definition.cooldown,
        };
        if !plan.valid(rules) || !self.projectile_plan_source_valid(&plan) {
            return Err(invalid());
        }
        Ok(plan)
    }
}

impl crate::Game {
    pub(crate) fn projectile_plan_source_valid(&self, plan: &ProjectilePlan) -> bool {
        self.projectile_plan_source_valid_with_context(plan, super::PlanSourceContext::Loaded)
    }
    pub(crate) fn projectile_plan_source_valid_with_context(
        &self,
        plan: &ProjectilePlan,
        sources: super::PlanSourceContext<'_>,
    ) -> bool {
        let Some(next_generation) = sources.condition_generation(self, plan.actor) else {
            return false;
        };
        plan.source.source_valid(self, plan.actor, sources)
            && self.world.knows(plan.origin.location)
            && plan
                .launch_footprint
                .iter()
                .all(|cell| self.world.knows(*cell))
            && plan
                .outgoing
                .catalog_valid(&self.content, self.next_item_id, self.next_actor_id)
            && plan
                .tokens
                .iter()
                .map(|claim| &claim.reservation)
                .chain(&plan.conditional_tokens)
                .all(|token| {
                    token.generation < next_generation
                        && token.source.0 < self.next_actor_id
                        && token
                            .qualification
                            .as_ref()
                            .is_none_or(|scope| scope.opponent.0 < self.next_actor_id)
                })
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    use crate::rules::*;
    #[test]
    fn projectile_reach_accepts_only_its_named_empty_shape() {
        let good = serde_json::json!({"kind": "projectile"});
        assert_eq!(
            serde_json::from_value::<Reach>(good.clone()).unwrap(),
            Reach::Projectile
        );
        assert_eq!(serde_json::to_value(Reach::Projectile).unwrap(), good);
        for bad in [
            serde_json::json!({"kind": "projectile", "range": 6}),
            serde_json::json!({"kind": "projectile", "delivery": "throw"}),
            serde_json::json!(["projectile"]),
        ] {
            assert!(serde_json::from_value::<Reach>(bad).is_err());
        }
        assert!(
            serde_json::from_str::<Reach>(r#"{"kind":"projectile","kind":"projectile"}"#).is_err()
        );
    }

    #[test]
    fn projectile_definition_rejects_sight_cell_range_modifiers_before_play() {
        let mut document = crate::test_creatures::ruleset().document().clone();
        let mut projectile = document
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .clone();
        projectile.id = crate::content_ids::AbilityId::new("fixture.projectile").unwrap();
        projectile.basic_attack = false;
        projectile.invocation = super::super::InvocationKind::DirectionalProjectile;
        projectile.reach = Reach::Projectile;
        projectile.strikes = StrikeSource::Projectile {
            delivery: ProjectileDeliveryKind::Throw,
        };
        projectile.timing.source = TimingSource::SelectedWeapon;
        projectile.supply_costs.clear();
        let id = projectile.id.clone();
        let tag = projectile.tags[0].clone();
        document.abilities.push(projectile);
        let rules = Ruleset::validate(document.clone()).unwrap();
        for stat in [
            Stat::Ability {
                ability: id,
                stat: AbilityStat::Range,
            },
            Stat::AbilityTag {
                tag,
                stat: AbilityStat::Range,
            },
        ] {
            let modifier = Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add { stat, value: 1 },
            };
            assert!(rules
                .validate_modifiers(std::slice::from_ref(&modifier))
                .is_err());
            let mut malformed = document.clone();
            malformed.classes[0].modifiers.push(modifier);
            assert!(
                matches!(Ruleset::validate(malformed), Err(RulesetError::Bound { field }) if field == "projectile_range_adjustment_unsupported")
            );
        }
    }
}
