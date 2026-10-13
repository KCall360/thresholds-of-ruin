//! Offline arena measurement through the ordinary Engine admission/execution path.
use super::{Engine, Failure, Scenario};
use crate::scenario_package::ArenaControl;
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tor_protocol::{ActorId, ErrorCode};
use tor_simulation::{
    abilities::{OutcomeStatus, ResolvedEffect},
    combat::CombatEvent,
    resources::Resource,
    rules::CheckActor,
    ActorId as SimActor,
};

fn decimal<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}
fn invalid(message: &str) -> Failure {
    Failure::new(ErrorCode::InvalidAction, message)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ArenaTermination {
    ActionLimit,
    TickLimit,
    Elimination { winner: Option<String> },
    Stalemate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArenaInput {
    #[serde(serialize_with = "decimal")]
    pub seed: u64,
    pub model_hash: String,
    pub ruleset: String,
    pub ruleset_hash: String,
    pub selected: ActorId,
    pub streaming: Option<crate::Streaming>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArenaResource {
    pub resource: &'static str,
    pub maximum: u32,
    pub balance: u32,
    pub available: u32,
    pub reserved: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArenaState {
    pub health: u32,
    pub maximum_health: u32,
    pub injury: u32,
    pub dead: bool,
    pub resources: Vec<ArenaResource>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ArenaAbility {
    #[serde(serialize_with = "decimal")]
    pub resolutions: u64,
    #[serde(serialize_with = "decimal")]
    pub applications: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArenaParticipant {
    pub actor: ActorId,
    pub name: String,
    pub faction: String,
    pub hit_dice: u16,
    pub initial: ArenaState,
    pub final_state: ArenaState,
    #[serde(serialize_with = "decimal")]
    pub attack_checks: u64,
    #[serde(serialize_with = "decimal")]
    pub hits: u64,
    /// Actual resolved damage, including item and environmental outcomes.
    #[serde(serialize_with = "decimal")]
    pub damage_dealt: u64,
    #[serde(serialize_with = "decimal")]
    pub damage_received: u64,
    /// Applied healing from explicit effects, excluding natural recovery.
    #[serde(serialize_with = "decimal")]
    pub healing_received: u64,
    /// Natural recovery measured by injury conservation, never HP-capacity changes.
    #[serde(serialize_with = "decimal")]
    pub health_recovered: u64,
    pub abilities: BTreeMap<String, ArenaAbility>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArenaEvaluation {
    pub format: &'static str,
    pub ruleset: String,
    pub ruleset_hash: String,
    pub input: ArenaInput,
    pub input_hash: String,
    #[serde(serialize_with = "decimal")]
    pub tick: u64,
    #[serde(serialize_with = "decimal")]
    pub actions: u64,
    pub termination: ArenaTermination,
    pub participants: Vec<ArenaParticipant>,
}

fn participant(engine: &Engine, actor: u64) -> Result<ArenaParticipant, Failure> {
    let view = engine
        .game
        .inspect_creature(SimActor(actor))
        .ok_or_else(|| invalid("Arena evaluation requires loaded creature participants"))?;
    let health = view.creature.health();
    let state = ArenaState {
        health: health.current(),
        maximum_health: health.maximum(),
        injury: health.injury(),
        dead: !engine.game.alive(SimActor(actor)),
        resources: view
            .stats
            .resources
            .iter()
            .map(|value| ArenaResource {
                resource: match value.resource {
                    Resource::Stamina => "stamina",
                    Resource::Focus => "focus",
                    Resource::Mana => "mana",
                },
                maximum: value.maximum,
                balance: value.balance,
                available: value.available,
                reserved: value.reserved,
            })
            .collect(),
    };
    Ok(ArenaParticipant {
        actor: ActorId(actor),
        name: view.identity.name,
        faction: view.identity.faction,
        hit_dice: view.stats.hit_dice.len() as u16,
        initial: state.clone(),
        final_state: state,
        attack_checks: 0,
        hits: 0,
        damage_dealt: 0,
        damage_received: 0,
        healing_received: 0,
        health_recovered: 0,
        abilities: BTreeMap::new(),
    })
}

fn record_damage(
    participants: &mut BTreeMap<u64, ArenaParticipant>,
    actor: SimActor,
    target: SimActor,
    attack: bool,
    hit: bool,
    damage: u64,
) -> Result<(), Failure> {
    let source = participants
        .get_mut(&actor.0)
        .ok_or_else(|| invalid("Combat actor is outside arena participants"))?;
    source.attack_checks += u64::from(attack);
    source.hits += u64::from(attack && hit);
    source.damage_dealt += damage;
    let defender = participants
        .get_mut(&target.0)
        .ok_or_else(|| invalid("Combat target is outside arena participants"))?;
    defender.damage_received += damage;
    Ok(())
}

fn record_resolution(
    participants: &mut BTreeMap<u64, ArenaParticipant>,
    outcome: &tor_simulation::abilities::Outcome,
    attack: bool,
) -> Result<(), Failure> {
    let damage = outcome
        .effects
        .iter()
        .map(|effect| match effect {
            ResolvedEffect::Damage { amount } => u64::from(*amount),
            _ => 0,
        })
        .sum();
    record_damage(
        participants,
        outcome.actor,
        outcome.target,
        attack,
        outcome.result != OutcomeStatus::Miss,
        damage,
    )?;
    let healing = outcome
        .effects
        .iter()
        .map(|effect| match effect {
            ResolvedEffect::Heal { amount } => u64::from(*amount),
            _ => 0,
        })
        .sum::<u64>();
    participants
        .get_mut(&outcome.target.0)
        .expect("validated participant")
        .healing_received += healing;
    if let Some(ability) = &outcome.ability {
        let counter = participants
            .get_mut(&outcome.actor.0)
            .expect("validated participant")
            .abilities
            .entry(ability.as_str().to_owned())
            .or_default();
        counter.resolutions += 1;
        counter.applications += u64::from(outcome.result == OutcomeStatus::Applied);
    }
    Ok(())
}
fn measured_recovery(value: &ArenaParticipant) -> Result<u64, Failure> {
    u64::from(value.initial.injury)
        .checked_add(value.damage_received)
        .and_then(|injury| injury.checked_sub(value.healing_received))
        .and_then(|injury| injury.checked_sub(u64::from(value.final_state.injury)))
        .ok_or_else(|| invalid("Arena Health accounting does not conserve injury"))
}

/// Run a fresh, unpaused, all-AI package arena. Errors remain explicit failures;
/// no work is synthesized when the ordinary engine has no admissible decision.
pub fn run(scenario: Scenario) -> Result<ArenaEvaluation, Failure> {
    let package = scenario
        .package
        .as_ref()
        .ok_or_else(|| invalid("Arena evaluation requires a scenario package"))?;
    let arena = package
        .manifest
        .arena
        .as_ref()
        .ok_or_else(|| invalid("Scenario is not an arena"))?;
    if arena.control != ArenaControl::AllAi || arena.start_paused {
        return Err(invalid("Arena evaluation requires unpaused all-AI control"));
    }
    let ids = arena.participants.clone();
    let input = ArenaInput {
        seed: scenario.seed,
        model_hash: package.certificate.model_hash.clone(),
        ruleset: package.certificate.ruleset.clone(),
        ruleset_hash: package.certificate.ruleset_hash.clone(),
        selected: ActorId(package.selected),
        streaming: scenario.streaming,
    };
    let bytes = serde_json::to_vec(&input).map_err(|_| invalid("Invalid arena inputs"))?;
    let input_hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut engine = Engine::memory(scenario)?;
    let mut participants = ids
        .into_iter()
        .map(|id| Ok((id, participant(&engine, id)?)))
        .collect::<Result<BTreeMap<_, _>, Failure>>()?;
    loop {
        let run = engine
            .game
            .arena_run()
            .ok_or_else(|| invalid("Missing arena runtime"))?;
        if run.stop().is_some() {
            break;
        }
        let previous = run.actions;
        let Some((actor, _)) = engine.next_ai_action() else {
            break;
        };
        engine.advance_ai(actor)?;
        let run = engine.game.arena_run().expect("validated arena runtime");
        if run.actions != previous + 1 {
            return Err(invalid("Arena decision did not commit exactly one action"));
        }
        for event in engine.game.combat_events() {
            match event {
                CombatEvent::Resolved { outcome } => {
                    let definition = outcome
                        .ability
                        .as_ref()
                        .map(|ability| {
                            engine
                                .game
                                .content()
                                .rules()
                                .ability(ability)
                                .ok_or_else(|| {
                                    invalid("Resolved ability is absent from arena ruleset")
                                })
                        })
                        .transpose()?;
                    let attack = definition
                        .and_then(|definition| definition.check.as_ref())
                        .is_some_and(|check| check.actor == CheckActor::User);
                    record_resolution(&mut participants, outcome, attack)?;
                }
                CombatEvent::ItemCompleted { .. }
                | CombatEvent::Interrupted { .. }
                | CombatEvent::Died { .. } => {}
            }
        }
    }
    let run = engine.game.arena_run().expect("validated arena runtime");
    let termination = match run.stop() {
        Some(tor_simulation::arena::ArenaStop::ActionLimit) => ArenaTermination::ActionLimit,
        Some(tor_simulation::arena::ArenaStop::TickLimit) => ArenaTermination::TickLimit,
        Some(tor_simulation::arena::ArenaStop::Elimination { winner }) => {
            ArenaTermination::Elimination {
                winner: winner.clone(),
            }
        }
        None => ArenaTermination::Stalemate,
    };
    for (id, value) in &mut participants {
        value.final_state = participant(&engine, *id)?.final_state;
        value.health_recovered = measured_recovery(value)?;
    }
    Ok(ArenaEvaluation {
        format: "tor-arena-run-v2",
        ruleset: input.ruleset.clone(),
        ruleset_hash: input.ruleset_hash.clone(),
        input,
        input_hash,
        tick: engine.game.tick(),
        actions: run.actions,
        termination,
        participants: participants.into_values().collect(),
    })
}

#[cfg(test)]
mod recovery_measurement_tests {
    use super::*;
    fn subject(id: u64) -> ArenaParticipant {
        let state = ArenaState {
            health: 12,
            maximum_health: 20,
            injury: 8,
            dead: false,
            resources: vec![],
        };
        ArenaParticipant {
            actor: ActorId(id),
            name: "subject".into(),
            faction: "neutral".into(),
            hit_dice: 1,
            initial: state.clone(),
            final_state: state,
            attack_checks: 0,
            hits: 0,
            damage_dealt: 0,
            damage_received: 0,
            healing_received: 0,
            health_recovered: 0,
            abilities: BTreeMap::new(),
        }
    }
    #[test]
    fn non_ability_damage_and_explicit_healing_conserve_injury_despite_capacity_changes() {
        let mut participants = BTreeMap::from([(1, subject(1)), (2, subject(2))]);
        record_resolution(
            &mut participants,
            &tor_simulation::abilities::Outcome {
                actor: SimActor(1),
                target: SimActor(2),
                ability: None,
                intention: None,
                result: OutcomeStatus::Applied,
                effects: vec![
                    ResolvedEffect::Damage { amount: 3 },
                    ResolvedEffect::Heal { amount: 2 },
                ],
            },
            false,
        )
        .unwrap();
        let source = &participants[&1];
        assert_eq!(
            (source.damage_dealt, source.attack_checks, source.hits),
            (3, 0, 0)
        );
        assert!(source.abilities.is_empty());
        let target = participants.get_mut(&2).unwrap();
        target.final_state = ArenaState {
            health: 193,
            maximum_health: 200,
            injury: 7,
            dead: false,
            resources: vec![],
        };
        assert_eq!((target.damage_received, target.healing_received), (3, 2));
        assert_eq!(measured_recovery(target).unwrap(), 2);
        target.final_state.injury = 10;
        assert!(measured_recovery(target).is_err());
    }
}
