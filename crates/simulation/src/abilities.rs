//! Transient ability rules for action preparation. These values are backend
//! facts, not disclosed observations, persisted caches or execution permissions.
use crate::attributes::{CheckOutcome, Skill, SkillCheck};
use crate::combat::DamageType;
use crate::costs::ResourceCost;
use crate::creatures::CreatureState;
use crate::damage::{AttackCheck, AttackOutcome, DamageComponent, DamageSpec, Protection};
use crate::dice::Edge;
use crate::grants::Ability;
use crate::resolution_diagnostics::{ResolutionObserver, ResolutionStep};
use crate::resources::Resource;
use crate::{ActorId, Game, GameError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FearResolution {
    Immune,
    Resisted(CheckOutcome),
    Applied {
        resistance: CheckOutcome,
        duration: u64,
    },
}

/// Pure resolution only; the action engine owns reach, payment and injury.
pub fn resolve_magic_bolt(
    caster: &CreatureState,
    defense: i32,
    protection: &Protection,
    rng: &mut u64,
    edge: Edge,
) -> Result<AttackOutcome, GameError> {
    resolve_magic_bolt_with_diagnostics(caster, defense, protection, rng, edge, &mut ())
}

/// Records the actual check and damage stages without borrowing additional randomness.
pub fn resolve_magic_bolt_with_diagnostics(
    caster: &CreatureState,
    defense: i32,
    protection: &Protection,
    rng: &mut u64,
    edge: Edge,
    observer: &mut impl ResolutionObserver,
) -> Result<AttackOutcome, GameError> {
    let derived = require_ability(caster, Ability::MagicBolt)?;
    let damage = bolt_damage(derived)?;
    Ok(AttackCheck {
        check: SkillCheck {
            skill: Skill::Spellcasting,
            binding: caster.build().binding(),
            modifier: 0,
            threshold: defense,
        },
        attributes: derived.attributes,
        skills: derived.skills,
    }
    .resolve_with_diagnostics(rng, edge, &damage, protection, observer))
}

/// The edge belongs to this defender's resistance bundle. It is not copied
/// from a separate caster roll. The engine applies a failed resistance's effect.
pub fn resolve_fear(
    caster: &CreatureState,
    defender: &CreatureState,
    protection: &Protection,
    rng: &mut u64,
    edge: Edge,
) -> Result<FearResolution, GameError> {
    resolve_fear_with_diagnostics(caster, defender, protection, rng, edge, &mut ())
}

/// Immunity short-circuits the resistance check and consumes no randomness.
pub fn resolve_fear_with_diagnostics(
    caster: &CreatureState,
    defender: &CreatureState,
    protection: &Protection,
    rng: &mut u64,
    edge: Edge,
    observer: &mut impl ResolutionObserver,
) -> Result<FearResolution, GameError> {
    require_ability(caster, Ability::Fear)?;
    if defender.health().dead() {
        return Err(GameError::InvalidLocation);
    }
    let (difficulty, duration) = fear_parameters(caster)?;
    observer.record(ResolutionStep::FearStarted {
        difficulty,
        duration,
        net_edge: edge.balance(),
        difficulty_attribute: caster
            .derived()
            .attributes
            .get(Skill::Intimidation.attribute(caster.build().binding())),
        difficulty_rank: caster.derived().skills.get(Skill::Intimidation),
        difficulty_bonus: caster.derived().fear_difficulty_bonus,
        duration_bonus: caster.derived().fear_duration_bonus,
    });
    let fear = protection.has_immunity(crate::grants::Selector::Descriptor(
        crate::grants::Descriptor::Fear,
    ));
    let mind_affecting = protection.has_immunity(crate::grants::Selector::Descriptor(
        crate::grants::Descriptor::MindAffecting,
    ));
    if fear || mind_affecting {
        observer.record(ResolutionStep::FearImmunity {
            fear,
            mind_affecting,
        });
        observer.record(ResolutionStep::FearFinished {
            applied: false,
            duration: 0,
        });
        return Ok(FearResolution::Immune);
    }
    let resistance = SkillCheck {
        skill: Skill::Discipline,
        binding: defender.build().binding(),
        modifier: 0,
        threshold: difficulty,
    }
    .resolve_with_diagnostics(
        rng,
        defender.derived().attributes,
        defender.derived().skills,
        edge,
        observer,
    );
    observer.record(ResolutionStep::FearFinished {
        applied: !resistance.success,
        duration: if resistance.success { 0 } else { duration },
    });
    Ok(if resistance.success {
        FearResolution::Resisted(resistance)
    } else {
        FearResolution::Applied {
            resistance,
            duration,
        }
    })
}

fn require_ability(
    caster: &CreatureState,
    ability: Ability,
) -> Result<&crate::creatures::DerivedCreature, GameError> {
    if caster.health().dead() || !caster.derived().abilities.contains(&ability) {
        return Err(GameError::InvalidLocation);
    }
    Ok(caster.derived())
}

fn bolt_damage(derived: &crate::creatures::DerivedCreature) -> Result<DamageSpec, GameError> {
    let component = DamageComponent::rolled(DamageType::Energy, None, derived.bolt);
    let primary = component.key();
    DamageSpec::new(vec![component], Some(primary)).map_err(|_| GameError::InvalidLocation)
}

fn fear_parameters(caster: &CreatureState) -> Result<(i32, u64), GameError> {
    let derived = caster.derived();
    let difficulty =
        10 + i32::from(
            derived
                .attributes
                .get(Skill::Intimidation.attribute(caster.build().binding())),
        ) + i32::from(derived.skills.get(Skill::Intimidation))
            + derived.fear_difficulty_bonus;
    let duration = crate::fear::BASE_FEAR_DURATION
        .checked_add(derived.fear_duration_bonus)
        .filter(|duration| *duration <= crate::fear::MAX_FEAR_DURATION)
        .ok_or(GameError::InvalidLocation)?;
    Ok((difficulty, duration))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityReach {
    Melee,
    VisibleCells(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbilityEffect {
    Melee {
        impact_bonus: u32,
        damage: DamageSpec,
    },
    Bolt {
        damage: DamageSpec,
    },
    Fear {
        difficulty: i32,
        duration: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbilityPlan {
    pub ability: Ability,
    pub preparation: u64,
    pub recovery: u64,
    pub cost: Option<ResourceCost>,
    pub reach: AbilityReach,
    pub effect: AbilityEffect,
}

fn ability_reach(ability: Ability) -> AbilityReach {
    match ability {
        Ability::BasicMelee | Ability::PowerStrike => AbilityReach::Melee,
        Ability::MagicBolt | Ability::Fear => AbilityReach::VisibleCells(6),
    }
}

pub(crate) fn ability_cost(ability: Ability) -> Option<ResourceCost> {
    let resource = match ability {
        Ability::BasicMelee => return None,
        Ability::PowerStrike => Resource::Stamina,
        Ability::MagicBolt => Resource::Mana,
        Ability::Fear => Resource::Focus,
    };
    Some(ResourceCost {
        resource,
        start: 1,
        resolution: 1,
    })
}

impl Game {
    pub(crate) fn resolve_paid_ability(
        &mut self,
        actor: ActorId,
        ability: Ability,
        target: ActorId,
    ) -> (bool, u32) {
        match ability {
            Ability::PowerStrike => {
                let defense = self.effective_combat(target).unwrap().defense;
                self.resolve_creature_melee(actor, target, defense, 3)
            }
            Ability::MagicBolt => {
                let caster = self.creature(actor).expect("validated caster");
                let derived = caster.derived();
                let damage = bolt_damage(derived).expect("validated bolt");
                let check = AttackCheck {
                    check: SkillCheck {
                        skill: Skill::Spellcasting,
                        binding: caster.build().binding(),
                        modifier: 0,
                        threshold: self.effective_combat(target).unwrap().defense,
                    },
                    attributes: derived.attributes,
                    skills: derived.skills,
                };
                let edge = Edge::from_counts(0, caster.fear().disadvantages_against(target));
                let protection = self.damage_protection(target);
                let outcome = self.combat.resolve_creature_attack(
                    check,
                    edge,
                    &damage,
                    &protection,
                    self.combat_diagnostics
                        .as_mut()
                        .and_then(|diagnostics| diagnostics.active.as_mut()),
                );
                let damage = outcome
                    .damage
                    .map_or(0, |damage| self.apply_injury(target, damage.total));
                (outcome.check.success, damage)
            }
            Ability::Fear => {
                let protection = self.damage_protection(target);
                let caster = self.actors[&actor].combat.as_ref().unwrap().creature();
                let defender = self.actors[&target].combat.as_ref().unwrap().creature();
                let edge = Edge::from_counts(0, defender.fear().disadvantages_against(actor));
                let result = self.combat.resolve_creature_fear(
                    caster,
                    defender,
                    &protection,
                    edge,
                    self.combat_diagnostics
                        .as_mut()
                        .and_then(|diagnostics| diagnostics.active.as_mut()),
                );
                if let FearResolution::Applied { duration, .. } = result {
                    self.apply_fear_condition(target, actor, duration)
                        .expect("validated fear effect");
                    (true, 0)
                } else {
                    (false, 0)
                }
            }
            Ability::BasicMelee => unreachable!("basic melee uses the attack action"),
        }
    }
    /// Current grant and target geometry. Readiness and costs are separate;
    /// execution repeats this check rather than retaining a permission.
    pub fn ability_target_available(
        &self,
        actor: ActorId,
        ability: Ability,
        target: ActorId,
    ) -> bool {
        if actor == target || self.actor_frozen(actor) || self.actor_frozen(target) {
            return false;
        }
        let Some(caster) = self.creature(actor) else {
            return false;
        };
        if require_ability(caster, ability).is_err() {
            return false;
        }
        let Some(target_actor) = self.actors.get(&target) else {
            return false;
        };
        if target_actor
            .combat
            .as_ref()
            .is_none_or(|combat| combat.hp() == 0)
        {
            return false;
        }
        match ability_reach(ability) {
            AbilityReach::Melee => self.attack_available(actor, target),
            AbilityReach::VisibleCells(range) => {
                let Some(target_cells) = self.body_cells(
                    target_actor.location,
                    target_actor.orientation,
                    &target_actor.body,
                ) else {
                    return false;
                };
                let target_cells: std::collections::BTreeSet<_> = target_cells
                    .into_iter()
                    .map(|(location, _)| location)
                    .collect();
                let Some((eye, frame)) = self.eye(actor) else {
                    return false;
                };
                // Physical sight includes illumination, occlusion and portal
                // frames. Ordinary scenes also contain local awareness and
                // abstract stair disclosures, which are not casting paths.
                crate::diagnostics::scene();
                self.world
                    .illuminated_eye_scene(eye, frame, range)
                    .iter()
                    .any(|cell| target_cells.contains(&cell.location))
            }
        }
    }

    /// Current rule values only. Execution separately checks target visibility,
    /// reach and resources, then snapshots timing/cost at preparation start.
    pub fn ability_plan(&self, actor: ActorId, ability: Ability) -> Result<AbilityPlan, GameError> {
        let creature = self.creature(actor).ok_or(GameError::InvalidLocation)?;
        let derived = require_ability(creature, ability)?;
        let (preparation, recovery, cost, reach, effect) = match ability {
            Ability::BasicMelee | Ability::PowerStrike => {
                let combat = self
                    .effective_combat(actor)
                    .ok_or(GameError::InvalidLocation)?;
                let power = ability == Ability::PowerStrike;
                let impact_bonus = if power { 3 } else { 0 };
                let damage = self.creature_melee_damage_with_impact_bonus(actor, impact_bonus)?;
                (
                    combat.attack.wind_up,
                    combat.attack.recovery,
                    ability_cost(ability),
                    ability_reach(ability),
                    AbilityEffect::Melee {
                        impact_bonus,
                        damage,
                    },
                )
            }
            Ability::MagicBolt => {
                let damage = bolt_damage(derived)?;
                (
                    100,
                    100,
                    ability_cost(ability),
                    ability_reach(ability),
                    AbilityEffect::Bolt { damage },
                )
            }
            Ability::Fear => {
                let (difficulty, duration) = fear_parameters(creature)?;
                (
                    100,
                    100,
                    ability_cost(ability),
                    ability_reach(ability),
                    AbilityEffect::Fear {
                        difficulty,
                        duration,
                    },
                )
            }
        };
        Ok(AbilityPlan {
            ability,
            preparation,
            recovery,
            cost,
            reach,
            effect,
        })
    }
}
