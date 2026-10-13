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
    claims: Vec<TokenClaim>,
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
    fn skill_bonus(&self, selector: &SkillSelector, subject: ActorId) -> Result<i64, GameError> {
        let creature = self.subject(subject)?;
        let skill = match selector {
            SkillSelector::Named { skill } => *skill,
            SkillSelector::NaturalAttack => creature.build().species().melee.skill(),
            SkillSelector::SelectedWeapon => {
                return Ok(creature.derived().selected_attack.check_bonus);
            }
        };
        Ok(crate::attributes::SkillCheck {
            skill,
            binding: creature.build().binding(),
            modifier: 0,
            threshold: 0,
        }
        .bonus(creature.derived().attributes, creature.derived().skills))
    }
    fn check_edge(
        &mut self,
        site: CheckSite,
        roller: ActorId,
        opponent: ActorId,
        defense: Option<DefenseKind>,
        shared: bool,
    ) -> Result<i64, GameError> {
        let creature = self.subject(roller)?;
        let recipient = self.subject(opponent)?;
        let rules = self.game.content.rules();
        let mut selected = Vec::new();
        if site.strike == 0 {
            for token in creature
                .conditions()
                .qualifying_tokens(rules, roller, false)
                .into_iter()
                .chain(
                    recipient
                        .conditions()
                        .qualifying_tokens(rules, opponent, true),
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
                CheckContext {
                    ability: self.ability,
                    roller,
                    opponent,
                    defense,
                },
                shared,
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
            subject,
            opposing,
            match definition.against {
                Against::Defense { defense } => Some(defense),
                _ => None,
            },
            subject != self.actor || owner == Some(phase),
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
                    value += self.skill_bonus(skill, self.actor)?;
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
                CheckThreshold::Opposed {
                    bonus: self.skill_bonus(skill, opposing)?,
                    edge: self.check_edge(
                        CheckSite {
                            strike,
                            check: opposing_phase,
                        },
                        opposing,
                        subject,
                        None,
                        opposing != self.actor || owner == Some(opposing_phase),
                    )?,
                }
            }
        };
        Ok(PlannedCheck {
            actor: definition.actor,
            bonus: self.skill_bonus(&definition.skill, subject)?
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
                self.game
                    .compile_natural_effects(self.actor, effects, &self.modifiers)
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
                check,
                effects: self.game.compile_effects(
                    self.actor,
                    &rider.effects,
                    rider_modifiers.as_ref().unwrap_or(&self.modifiers),
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
                .effect_edge_against(creature.build().ruleset(), self.target)
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
            claims: vec![],
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
            StrikeSource::SelectedMelee => match &caster.derived().selected_attack.attack {
                crate::attacks::SelectedMeleeAttack::Natural(natural) => {
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
                                species: caster.build().species().id.clone(),
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
        let plan = Plan::Ability(tor_world::Shared::new(AbilityPlan {
            actor,
            target,
            ability: ability.clone(),
            preparation: timing.preparation,
            recovery: timing.recovery,
            cost: self.ability_cost(ability)?,
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
    ) -> Result<Vec<PlannedEffect>, GameError> {
        let mut captured = modifiers.clone();
        captured.dice = 0;
        captured.flat = 0;
        let mut effects = self.compile_effects(actor, effects, &captured)?;
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
