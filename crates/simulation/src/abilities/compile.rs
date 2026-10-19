//! Compile all strike sites once. The live scheduler never derives operands.
use super::{
    AbilityPlan, CheckPhase, CheckSide, CheckSite, CheckThreshold, FrozenRider, FrozenStrike, Plan,
    PlannedCheck, PlannedEffect, RuleError, StrikeOrigin, TokenClaim,
};
use crate::content_ids::AbilityId;
use crate::creatures::{AbilityValues, CheckContext};
use crate::rules::{
    Against, CheckActor, CheckDef, DefenseKind, Effect, SkillSelector, StrikeDef, StrikeSource,
};
use crate::{ActorId, Game, GameError};

fn invalid() -> GameError {
    GameError::Rule(RuleError::InvalidDefinition)
}

struct Compiler<'a> {
    game: &'a Game,
    caster: &'a crate::creatures::CreatureState,
    defender: &'a crate::creatures::CreatureState,
    actor: ActorId,
    target: ActorId,
    ability: Option<&'a AbilityId>,
    modifiers: AbilityValues,
    outgoing_edge_policy: crate::rules::OutgoingEdgePolicy,
    claims: Vec<TokenClaim>,
    natural_source: bool,
    selected_attack: &'a crate::creatures::SelectedAttack,
}
impl<'a> Compiler<'a> {
    fn subject(&self, subject: ActorId) -> Result<&'a crate::creatures::CreatureState, GameError> {
        if subject == self.actor {
            Ok(self.caster)
        } else if subject == self.target {
            Ok(self.defender)
        } else {
            Err(GameError::UnknownActor)
        }
    }
    fn skill_bonus(
        &self,
        selector: &SkillSelector,
        subject: ActorId,
        role: CheckActor,
    ) -> Result<i64, GameError> {
        let creature = self.subject(subject)?;
        let skill = match selector {
            SkillSelector::Named { skill } => *skill,
            SkillSelector::NaturalAttack => creature.derived().natural_attack.attack.skill(),
            SkillSelector::SelectedWeapon => {
                if self.natural_source && role == CheckActor::User {
                    return Ok(creature.derived().natural_attack.check_bonus);
                }
                return Ok(if role == CheckActor::User {
                    self.selected_attack.check_bonus
                } else {
                    creature.derived().selected_attack.check_bonus
                });
            }
        };
        Ok(crate::attributes::SkillCheck {
            skill,
            binding: creature.derived().binding(),
            modifier: 0,
            threshold: 0,
        }
        .bonus(creature.derived().attributes, creature.derived().skills))
    }
    fn check_edge(
        &mut self,
        site: CheckSite,
        context: CheckContext<'_>,
        shared: bool,
        policy: crate::rules::OutgoingEdgePolicy,
    ) -> Result<i64, GameError> {
        let creature = self.subject(context.roller)?;
        let recipient = self.subject(context.opponent)?;
        let rules = self.game.content.rules();
        let mut selected = Vec::new();
        if site.strike == 0 {
            for token in creature
                .conditions()
                .qualifying_tokens_for(
                    rules,
                    context.roller,
                    false,
                    context
                        .ability
                        .map(|ability| (context.opponent, ability, context.roller_role)),
                )
                .into_iter()
                .filter(|token| {
                    rules
                        .condition(&token.condition)
                        .is_some_and(|condition| policy.selects_token(condition))
                })
                .chain(
                    recipient
                        .conditions()
                        .qualifying_tokens(rules, context.opponent, true),
                )
            {
                if !self.claims.iter().any(|claim| claim.reservation == token) {
                    selected.push(token.clone());
                    self.claims.push(TokenClaim {
                        reservation: token,
                        site,
                    });
                }
            }
        }
        creature
            .derived()
            .check_edge_with_selection(
                recipient.derived(),
                rules,
                context,
                shared,
                policy,
                &selected,
            )
            .map_err(|_| invalid())
    }
    fn check(
        &mut self,
        definition: &CheckDef,
        strike: u16,
        phase: CheckPhase,
        owner: Option<CheckPhase>,
    ) -> Result<PlannedCheck, GameError> {
        let (subject, opposing) = match definition.actor {
            CheckActor::User => (self.actor, self.target),
            CheckActor::Target => (self.target, self.actor),
        };
        // Initiating then opposing is the structural site order, even though no
        // random dice are rolled during this capture.
        let edge = self.check_edge(
            CheckSite {
                strike,
                check: phase,
            },
            CheckContext {
                ability: self.ability,
                roller_role: definition.actor,
                roller: subject,
                opponent: opposing,
                defense: match definition.against {
                    Against::Defense { defense } => Some(defense),
                    _ => None,
                },
            },
            definition.actor == CheckActor::Target || owner == Some(phase),
            self.outgoing_edge_policy.for_role(definition.actor),
        )?;
        let threshold = match &definition.against {
            Against::Defense { defense } => {
                let defenses = self.subject(opposing)?.derived().defenses;
                CheckThreshold::Fixed {
                    value: i64::from(match defense {
                        DefenseKind::Physical => defenses.physical,
                        DefenseKind::Cognitive => defenses.cognitive,
                        DefenseKind::Spiritual => defenses.spiritual,
                    }),
                }
            }
            Against::Difficulty { formula } => {
                let mut value = i64::from(formula.base) + i64::from(self.modifiers.difficulty);
                if let Some(skill) = &formula.skill {
                    value += self.skill_bonus(skill, self.actor, CheckActor::User)?;
                }
                if let Some(attribute) = formula.attribute {
                    value += i64::from(self.caster.derived().attributes.get(attribute));
                }
                CheckThreshold::Fixed { value }
            }
            Against::Opposed { skill } => {
                let opposing_phase = CheckPhase::Primary {
                    side: CheckSide::Opposing,
                };
                let opposing_role = match definition.actor {
                    CheckActor::User => CheckActor::Target,
                    CheckActor::Target => CheckActor::User,
                };
                CheckThreshold::Opposed {
                    bonus: self.skill_bonus(skill, opposing, opposing_role)?,
                    edge: self.check_edge(
                        CheckSite {
                            strike,
                            check: opposing_phase,
                        },
                        CheckContext {
                            ability: self.ability,
                            roller_role: opposing_role,
                            roller: opposing,
                            opponent: subject,
                            defense: None,
                        },
                        definition.actor == CheckActor::User || owner == Some(opposing_phase),
                        self.outgoing_edge_policy.for_role(opposing_role),
                    )?,
                }
            }
        };
        Ok(PlannedCheck {
            actor: definition.actor,
            target_participates: definition.target_participates(),
            bonus: self.skill_bonus(&definition.skill, subject, definition.actor)?
                + i64::from(self.modifiers.check_bonus),
            threshold,
            edge,
        })
    }
    fn strike(
        &mut self,
        definition: &StrikeDef,
        index: u16,
        origin: StrikeOrigin,
        natural: bool,
        scaled_by: Option<crate::attributes::Attribute>,
    ) -> Result<FrozenStrike, GameError> {
        let primary = CheckPhase::Primary {
            side: CheckSide::Initiating,
        };
        let owner = definition
            .check
            .as_ref()
            .and_then(|check| {
                if check.actor == CheckActor::User {
                    Some(primary)
                } else if matches!(check.against, Against::Opposed { .. }) {
                    Some(CheckPhase::Primary {
                        side: CheckSide::Opposing,
                    })
                } else {
                    None
                }
            })
            .or_else(|| {
                definition
                    .riders
                    .iter()
                    .position(|rider| rider.check.is_some())
                    .map(|ordinal| CheckPhase::Rider {
                        ordinal: ordinal as u16,
                    })
            });
        let check = definition
            .check
            .as_ref()
            .map(|check| self.check(check, index, primary, owner))
            .transpose()?;
        // Each selectable primary branch owns its first natural damage payload.
        // The unselected branch is captured, but never executed or charged twice.
        let capture_primary = |effects: &[Effect]| {
            if natural {
                self.game.compile_natural_effects(
                    self.actor,
                    effects,
                    &self.modifiers,
                    self.natural_source,
                    Some(self.selected_attack),
                )
            } else {
                self.game.compile_effects_from_source(
                    self.actor,
                    effects,
                    &self.modifiers,
                    self.natural_source,
                    Some(self.selected_attack),
                )
            }
        };
        let on_success = capture_primary(&definition.on_success)?;
        let on_failure = capture_primary(&definition.on_failure)?;
        // Permanent natural melee dice/flat belong only to primary damage.
        // Other operands, including duration, retain their normal frozen scope.
        let rider_modifiers = natural.then(|| AbilityValues {
            dice: 0,
            flat: 0,
            ..self.modifiers.clone()
        });
        let mut riders = Vec::with_capacity(definition.riders.len());
        for (ordinal, rider) in definition.riders.iter().enumerate() {
            let phase = CheckPhase::Rider {
                ordinal: ordinal as u16,
            };
            let check = rider
                .check
                .as_ref()
                .map(|check| {
                    self.check(
                        &CheckDef {
                            actor: CheckActor::User,
                            skill: check.skill.clone(),
                            against: Against::Defense {
                                defense: check.defense,
                            },
                        },
                        index,
                        phase,
                        owner,
                    )
                })
                .transpose()?;
            riders.push(FrozenRider {
                trigger: rider.trigger,
                condition: rider.condition,
                check,
                effects: self.game.compile_effects_from_source(
                    self.actor,
                    &rider.effects,
                    rider_modifiers.as_ref().unwrap_or(&self.modifiers),
                    self.natural_source,
                    Some(self.selected_attack),
                )?,
                descriptors: rider.descriptors.clone(),
            });
        }
        let creature = self.caster;
        let offset = if definition.offset == 0 {
            0
        } else {
            crate::creatures::scaled_duration(
                definition.offset,
                scaled_by.map(|attribute| creature.derived().attributes.get(attribute)),
                creature.build().ruleset(),
            )
            .map_err(|_| invalid())?
        };
        Ok(FrozenStrike {
            offset,
            origin,
            check,
            on_success,
            on_failure,
            descriptors: definition.descriptors.clone(),
            riders,
            edge: creature
                .derived()
                .effect_edge_against_with_policy(
                    creature.build().ruleset(),
                    self.target,
                    self.outgoing_edge_policy,
                )
                .map_err(|_| invalid())?,
            shared_check: owner,
        })
    }
}
impl Game {
    /// Generic checks share ordinary edge/token capture without an authored ability.
    pub(crate) fn compile_escape_check(
        &self,
        actor: ActorId,
        holder: ActorId,
    ) -> Result<(PlannedCheck, Vec<TokenClaim>), GameError> {
        let caster = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let defender = self.creature(holder).ok_or(GameError::UnknownActor)?;
        let mut compiler = Compiler {
            game: self,
            caster: &caster,
            defender: &defender,
            actor,
            target: holder,
            ability: None,
            modifiers: AbilityValues::default(),
            outgoing_edge_policy: crate::rules::OutgoingEdgePolicy::Ordinary,
            claims: Vec::new(),
            natural_source: false,
            selected_attack: &caster.derived().selected_attack,
        };
        let skill = SkillSelector::Named {
            skill: crate::attributes::Skill::Athletics,
        };
        let phase = CheckPhase::Primary {
            side: CheckSide::Initiating,
        };
        let check = compiler.check(
            &CheckDef {
                actor: CheckActor::User,
                skill: skill.clone(),
                against: Against::Opposed { skill },
            },
            0,
            phase,
            Some(phase),
        )?;
        Ok((check, compiler.claims))
    }
    pub(super) fn compile_strike_plan(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        invocation: super::AbilityInvocation,
        sight: Option<&[tor_world::SightCell]>,
        supplies: crate::supplies::SupplySelection,
    ) -> Result<Plan, GameError> {
        crate::diagnostics::ability_plan();
        if let super::AbilityAim::DirectionalRay { direction } = invocation.aim {
            if invocation.weapon != super::WeaponSelection::Default {
                return Err(invalid());
            }
            return self.compile_ray_plan(actor, ability, direction, supplies);
        }
        if let super::AbilityAim::DirectionalProjectile {
            direction,
            delivery,
        } = invocation.aim
        {
            if invocation.weapon != super::WeaponSelection::Default
                || supplies != crate::supplies::SupplySelection::Carried
            {
                return Err(invalid());
            }
            return self
                .capture_projectile_plan(actor, ability, direction, delivery)
                .map(|plan| Plan::Projectile(tor_world::Shared::new(plan)));
        }
        let target = invocation.target(actor);
        let caster = self.creature(actor).ok_or(GameError::UnknownActor)?;
        // The original reference domain is an admission operand, not a hint.
        // Validate it before touching hidden faction, health or private plans.
        let disclosure = self.actor_disclosure(actor, target)?;
        if target != actor && disclosure.reference() != Some(invocation.reference()) {
            return Err(GameError::Rule(RuleError::OutOfReach));
        }
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
        if definition.invocation != invocation.kind() {
            return Err(invalid());
        }
        if matches!(invocation.aim, super::AbilityAim::ContactTarget { .. })
            && (!matches!(definition.reach, crate::rules::Reach::Melee)
                || definition.interaction != crate::rules::Interaction::HostileToTarget
                || !definition.basic_attack)
        {
            return Err(GameError::Rule(RuleError::OutOfReach));
        }
        let step = invocation
            .step()
            .map(|direction| super::StepCommitment::capture(self, actor, direction))
            .transpose()?;
        if let Some(cooldown) = definition.cooldown {
            caster
                .validate_cooldown(ability, cooldown.get())
                .map_err(|_| GameError::Rule(RuleError::CoolingDown))?;
        }
        if !supplies.valid(definition.supply_costs.len()) {
            return Err(GameError::ItemUnavailable);
        }
        let weapon = self.capture_invocation_weapon(actor, definition, invocation)?;
        let timing = crate::creatures::ability_timing_for_attack(
            caster.derived(),
            caster.build(),
            definition,
            &caster.derived().ability_values(ability),
            &weapon.attack,
        )
        .map_err(|_| invalid())?;
        let reach = caster
            .derived()
            .ability_reach(ability)
            .map_err(|_| invalid())?;
        if !self.target_in_reach_snapshot(actor, target, &reach, sight) {
            return Err(GameError::Rule(RuleError::OutOfReach));
        }
        let mut compiler = Compiler {
            game: self,
            caster: &caster,
            defender: &defender,
            actor,
            target,
            ability: Some(ability),
            modifiers: caster.derived().ability_values(ability),
            outgoing_edge_policy: definition.outgoing_edge_policy,
            claims: vec![],
            natural_source: matches!(definition.strikes, StrikeSource::NaturalMelee { .. }),
            selected_attack: &weapon.attack,
        };
        let mut strikes = Vec::new();
        match &definition.strikes {
            StrikeSource::Projectile { .. } => return Err(invalid()),
            StrikeSource::Authored { strikes: authored } => {
                for (ordinal, strike) in authored.iter().enumerate() {
                    if strike
                        .unlock
                        .as_ref()
                        .is_some_and(|id| !caster.derived().active_talents.contains(id))
                    {
                        continue;
                    }
                    strikes.push(compiler.strike(
                        strike,
                        strikes.len() as u16,
                        StrikeOrigin::Ability {
                            ordinal: ordinal as u16,
                        },
                        false,
                        definition.timing.scaled_by,
                    )?);
                }
            }
            StrikeSource::NaturalMelee { additional_riders } => {
                let crate::attacks::SelectedMeleeAttack::Natural(natural) =
                    &caster.derived().natural_attack.attack
                else {
                    return Err(invalid());
                };
                let crate::creatures::SourceId::Species(species) =
                    &caster.derived().natural_attack.source
                else {
                    return Err(invalid());
                };
                let authored =
                    crate::rules::append_natural_riders(natural.strikes(), additional_riders)
                        .map_err(|_| invalid())?;
                for (ordinal, strike) in authored.iter().enumerate() {
                    if strike
                        .unlock
                        .as_ref()
                        .is_some_and(|id| !caster.derived().active_talents.contains(id))
                    {
                        continue;
                    }
                    strikes.push(compiler.strike(
                        strike,
                        strikes.len() as u16,
                        StrikeOrigin::NaturalMelee {
                            species: species.clone(),
                            ordinal: ordinal as u16,
                            natural_riders: natural.strikes()[ordinal].riders.len() as u16,
                        },
                        true,
                        definition.timing.scaled_by,
                    )?);
                }
            }
            StrikeSource::SelectedMelee => match &weapon.attack.attack {
                crate::attacks::SelectedMeleeAttack::Natural(natural) => {
                    let crate::creatures::SourceId::Species(species) = &weapon.attack.source else {
                        return Err(invalid());
                    };
                    for (ordinal, strike) in natural.strikes().iter().enumerate() {
                        if strike
                            .unlock
                            .as_ref()
                            .is_some_and(|id| !caster.derived().active_talents.contains(id))
                        {
                            continue;
                        }
                        strikes.push(compiler.strike(
                            strike,
                            strikes.len() as u16,
                            StrikeOrigin::Natural {
                                species: species.clone(),
                                ordinal: ordinal as u16,
                            },
                            true,
                            definition.timing.scaled_by,
                        )?);
                    }
                }
                crate::attacks::SelectedMeleeAttack::Weapon(_) => {
                    let crate::creatures::SourceId::Equipment(item) = &weapon.attack.source else {
                        return Err(invalid());
                    };
                    let strike = StrikeDef {
                        offset: 0,
                        unlock: None,
                        check: Some(CheckDef {
                            actor: CheckActor::User,
                            skill: SkillSelector::SelectedWeapon,
                            against: Against::Defense {
                                defense: DefenseKind::Physical,
                            },
                        }),
                        on_success: vec![crate::rules::Effect::Damage {
                            damage: crate::rules::DamageExpr::SelectedWeapon { impact_bonus: 0 },
                        }],
                        on_failure: vec![],
                        descriptors: vec![],
                        riders: vec![],
                    };
                    strikes.push(compiler.strike(
                        &strike,
                        0,
                        StrikeOrigin::Weapon { item: *item },
                        false,
                        definition.timing.scaled_by,
                    )?);
                }
            },
        }
        // One source-owned claim augments only the first outgoing attack check.
        // It is check-only: ordinary edge allocation still applies the three-die cap.
        let ambush = if definition.basic_attack
            && definition.interaction == crate::rules::Interaction::HostileToTarget
            && matches!(reach, crate::rules::Reach::Melee)
            && self.actors[&actor].guise.ambush_available()
            && strikes
                .first()
                .and_then(|strike| strike.check.as_ref())
                .is_some_and(|check| check.actor == CheckActor::User)
        {
            let site = CheckSite {
                strike: 0,
                check: CheckPhase::Primary {
                    side: CheckSide::Initiating,
                },
            };
            let check = strikes[0].check.as_mut().expect("first outgoing check");
            check.edge = check.edge.checked_add(1).ok_or_else(invalid)?;
            Some(site)
        } else {
            None
        };
        for strike in &strikes {
            self.validate_hit_die_effect_admission(target, &strike.on_success)?;
            self.validate_hit_die_effect_admission(target, &strike.on_failure)?;
            for rider in &strike.riders {
                self.validate_hit_die_effect_admission(target, &rider.effects)?;
            }
        }
        let plan = Plan::Ability(tor_world::Shared::new(AbilityPlan {
            ambush,
            actor,
            invocation,
            weapon: weapon.frozen.clone(),
            step,
            target,
            initial_hold: self
                .linked_holds(target)
                .into_iter()
                .find(|link| link.holder == actor)
                .map(|link| link.id),
            hold_damage_increment: compiler.modifiers.hold_damage_increment,
            public_aim: self.capture_public_aim(actor, target, sight),
            ability: ability.clone(),
            preparation: timing.preparation,
            recovery: timing.recovery,
            cost: weapon.frozen.cost(rules, self.ability_cost(ability)?)?,
            cooldown: definition.cooldown,
            supply_costs: definition.supply_costs.clone(),
            supplies,
            reach,
            strikes,
            tokens: compiler.claims,
        }));
        if !plan.valid(rules) || !self.plan_source_valid(&plan) {
            return Err(invalid());
        }
        Ok(plan)
    }
    fn compile_natural_effects(
        &self,
        actor: ActorId,
        effects: &[crate::rules::Effect],
        modifiers: &AbilityValues,
        natural_source: bool,
        selected: Option<&crate::creatures::SelectedAttack>,
    ) -> Result<Vec<PlannedEffect>, GameError> {
        let mut captured = modifiers.clone();
        captured.dice = 0;
        captured.flat = 0;
        let mut effects =
            self.compile_effects_from_source(actor, effects, &captured, natural_source, selected)?;
        if let Some(PlannedEffect::Damage { damage }) = effects
            .iter_mut()
            .find(|effect| matches!(effect, PlannedEffect::Damage { .. }))
        {
            *damage = damage
                .with_melee_modifiers(modifiers.dice, modifiers.flat, 0)
                .map_err(|_| invalid())?;
        }
        Ok(effects)
    }
}
