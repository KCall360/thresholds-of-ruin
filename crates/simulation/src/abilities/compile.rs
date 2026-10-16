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
    ability: &'a AbilityId,
    modifiers: AbilityValues,
    outgoing_edge_policy: crate::rules::OutgoingEdgePolicy,
    claims: Vec<TokenClaim>,
    natural_source: bool,
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
                return Ok(creature.derived().selected_attack.check_bonus);
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
                    Some((context.opponent, context.ability, context.roller_role)),
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
                )
            } else {
                self.game
                    .compile_effects(self.actor, effects, &self.modifiers)
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
    pub(super) fn compile_strike_plan(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        invocation: super::AbilityInvocation,
        sight: Option<&[tor_world::SightCell]>,
        supplies: crate::supplies::SupplySelection,
    ) -> Result<Plan, GameError> {
        crate::diagnostics::ability_plan();
        let target = invocation.target(actor);
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
        if definition.invocation != invocation.kind() {
            return Err(invalid());
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
        let timing = self.ability_timing(actor, ability)?;
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
            ability,
            modifiers: caster.derived().ability_values(ability),
            outgoing_edge_policy: definition.outgoing_edge_policy,
            claims: vec![],
            natural_source: matches!(definition.strikes, StrikeSource::NaturalMelee { .. }),
        };
        let mut strikes = Vec::new();
        match &definition.strikes {
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
            StrikeSource::SelectedMelee => match &caster.derived().selected_attack.attack {
                crate::attacks::SelectedMeleeAttack::Natural(natural) => {
                    let crate::creatures::SourceId::Species(species) =
                        &caster.derived().selected_attack.source
                    else {
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
                    let crate::creatures::SourceId::Equipment(item) =
                        &caster.derived().selected_attack.source
                    else {
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
        for strike in &strikes {
            self.validate_hit_die_effect_admission(target, &strike.on_success)?;
            self.validate_hit_die_effect_admission(target, &strike.on_failure)?;
            for rider in &strike.riders {
                self.validate_hit_die_effect_admission(target, &rider.effects)?;
            }
        }
        let plan = Plan::Ability(tor_world::Shared::new(AbilityPlan {
            actor,
            invocation,
            step,
            target,
            public_aim: self.capture_public_aim(actor, target, sight),
            ability: ability.clone(),
            preparation: timing.preparation,
            recovery: timing.recovery,
            cost: self.ability_cost(ability)?,
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
    ) -> Result<Vec<PlannedEffect>, GameError> {
        let mut captured = modifiers.clone();
        captured.dice = 0;
        captured.flat = 0;
        let mut effects =
            self.compile_effects_from_source(actor, effects, &captured, natural_source)?;
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
