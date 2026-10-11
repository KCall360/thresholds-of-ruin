//! Offline arena measurement through the ordinary Engine admission/execution path.
use super::{Engine, Failure, Scenario};
use crate::scenario_package::ArenaControl;
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tor_protocol::{ActorId, ErrorCode};
use tor_simulation::{
    combat::CombatEvent, grants::Ability, resources::Resource, ActorId as SimActor,
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
    /// Combat resolution damage only; environmental changes remain in HP state.
    #[serde(serialize_with = "decimal")]
    pub damage_dealt: u64,
    #[serde(serialize_with = "decimal")]
    pub damage_received: u64,
    pub abilities: BTreeMap<&'static str, ArenaAbility>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArenaEvaluation {
    pub format: &'static str,
    pub ruleset: &'static str,
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
        abilities: BTreeMap::new(),
    })
}

fn record_damage(
    participants: &mut BTreeMap<u64, ArenaParticipant>,
    actor: SimActor,
    target: SimActor,
    attack: bool,
    hit: bool,
    damage: u32,
) -> Result<(), Failure> {
    let source = participants
        .get_mut(&actor.0)
        .ok_or_else(|| invalid("Combat actor is outside arena participants"))?;
    source.attack_checks += u64::from(attack);
    source.hits += u64::from(attack && hit);
    source.damage_dealt += u64::from(damage);
    let defender = participants
        .get_mut(&target.0)
        .ok_or_else(|| invalid("Combat target is outside arena participants"))?;
    defender.damage_received += u64::from(damage);
    Ok(())
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
        if run.stop.is_some() {
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
                CombatEvent::Resolved {
                    actor,
                    target,
                    hit,
                    damage,
                    ..
                } => record_damage(&mut participants, *actor, *target, true, *hit, *damage)?,
                CombatEvent::AbilityResolved {
                    actor,
                    target,
                    ability,
                    applied,
                    damage,
                    ..
                } => {
                    let (name, attack) = match ability {
                        Ability::BasicMelee => ("basic_melee", true),
                        Ability::PowerStrike => ("power_strike", true),
                        Ability::MagicBolt => ("magic_bolt", true),
                        Ability::Fear => ("fear", false),
                    };
                    record_damage(
                        &mut participants,
                        *actor,
                        *target,
                        attack,
                        *applied,
                        *damage,
                    )?;
                    let counter = participants
                        .get_mut(&actor.0)
                        .expect("validated participant")
                        .abilities
                        .entry(name)
                        .or_default();
                    counter.resolutions += 1;
                    counter.applications += u64::from(*applied);
                }
                CombatEvent::ItemCompleted { .. }
                | CombatEvent::Interrupted { .. }
                | CombatEvent::Died { .. } => {}
            }
        }
    }
    let run = engine.game.arena_run().expect("validated arena runtime");
    let termination = match &run.stop {
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
    }
    Ok(ArenaEvaluation {
        format: "tor-arena-run-v1",
        ruleset: crate::scenario_package::RULESET,
        input,
        input_hash,
        tick: engine.game.tick(),
        actions: run.actions,
        termination,
        participants: participants.into_values().collect(),
    })
}
