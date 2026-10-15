//! Head-only ordered-strikes v1: real commands, frozen-plan size and bounded replay.
//! Timed commands exclude setup, projection/equality, serialization diagnostics,
//! barriers and reopen. Only scalar sizes/counts leave the frozen-plan diagnostic.
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Instant};
use tor_protocol::{AbilityOutcome, ActorId, CombatEventView};
use tor_server::{
    journal::{Action, Command},
    scenario_package::{Manifest, Package, RegionDef},
    Engine, SavePolicy, Scenario,
};
use tor_simulation::diagnostics::{work_counts, WorkCounts};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const USER: &str = "strike-benchmark";
const FRONTEND: &str = "ordered-strikes-v1";
const ATTACKER: ActorId = ActorId(1);
const TARGET: ActorId = ActorId(2);

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn elapsed(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.
}

fn strike(offset: usize, riders: usize) -> Value {
    let damage = json!({"kind":"damage","damage":{"kind":"components","components":[{
        "category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}});
    let skill = json!({"kind":"named","skill":"heavy_weaponry"});
    json!({"offset":offset,"unlock":null,
        "check":{"actor":"user","skill":skill,"against":{"kind":"defense","defense":"physical"}},
        "on_success":[damage],"on_failure":[],"descriptors":[],
        "riders":(0..riders).map(|_| json!({"trigger":"success",
            "check":{"skill":skill,"defense":"physical"},"effects":[damage],"descriptors":[]})).collect::<Vec<_>>()})
}

fn scenario(
    strikes: usize,
    riders: usize,
    actors: usize,
    regions: usize,
    seed: u64,
) -> Result<Scenario> {
    require(
        [(1, 0), (3, 2), (16, 8)].contains(&(strikes, riders)),
        "unknown strike layout",
    )?;
    require(
        [2, 256].contains(&actors) && [16, 256].contains(&regions),
        "unknown loaded dimensions",
    )?;
    let mut manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml"))?;
    manifest.id = "ordered-strike-benchmark".into();
    manifest.objective = None;
    manifest.characters[0].turn_ticks = 150;
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML)?;
    rules.abilities.push(serde_json::from_value(json!({"id":"benchmark_sequence","name":"Benchmark sequence",
        "tags":[],"basic_attack":false,"cost":null,
        "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":"speed"},
        "reach":{"kind":"melee"},
        "strikes":{"kind":"authored","strikes":(0..strikes).map(|i|strike(i*100,riders)).collect::<Vec<_>>()},
        "interaction": "hostile_to_target", "ai":{"priority":0,"hostile":true,"minimum_resource_percent":0,"range_preference":"any"}}))?);
    manifest.creatures = serde_json::from_value(json!({"species":{"benchmark_fighter":{
        "kind":"humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},"attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[strike(0,0)]},
        "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"benchmark_sequence"}},
            {"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":999000}}]
    }}}))?;
    let creature = json!({"species":"benchmark_fighter","name":"Benchmark fighter","faction":"neutral",
        "binding":"intellect","starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":[{"source":"racial"}]});
    manifest.characters[0].creature = Some(serde_json::from_value(creature.clone())?);
    let mut definitions: Vec<RegionDef> = (1..=regions)
        .map(|id| {
            serde_json::from_value(json!({"id":id,"name":format!("Loaded chamber {id}"),
            "size":[20,4,2],"chamber":true,"gravity":[0,0,-1],
            "anchors":{"start":[1,1,0]}}))
        })
        .collect::<std::result::Result<_, _>>()?;
    for id in 2..=actors {
        // Keep the adjacent target in chamber one; distribute parked actors
        // across the other loaded chambers, without overlapping occupied cells.
        let region = if id == 2 {
            0
        } else {
            1 + (id - 3) % (regions - 1)
        };
        let x = if id == 2 {
            2
        } else {
            1 + (id - 3) / (regions - 1)
        };
        definitions[region]
            .actors
            .push(serde_json::from_value(json!({"id":id,
            "at":[x,1,0],"turn_ticks":if id == 2 {150} else {100000},
            "controller":"external","creature":creature}))?);
    }
    let package = Package::from_parts(manifest, definitions, &toml::to_string(&rules)?)?;
    Ok(Scenario {
        seed,
        actors: vec![],
        regions: regions as u64,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: None,
    })
}

fn act(engine: &mut Engine, actor: ActorId, id: &str, action: Action) -> Result<()> {
    engine.command(
        USER,
        FRONTEND,
        actor,
        id,
        &engine.branch().clone(),
        Command::Act {
            expected_revision: engine.revision(actor)?,
            action,
        },
    )?;
    Ok(())
}

#[derive(Default)]
struct Measurements {
    commands: Vec<f64>,
    phases: BTreeMap<&'static str, f64>,
    counts: BTreeMap<&'static str, usize>,
    primary: BTreeMap<u16, AbilityOutcome>,
    projection_ms: f64,
}
impl Measurements {
    fn command(
        &mut self,
        engine: &mut Engine,
        actor: ActorId,
        id: &str,
        action: Action,
    ) -> Result<()> {
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: engine.revision(actor)?,
            action,
        };
        let before = work_counts();
        let started = Instant::now();
        let (_, profile) = engine.command_profiled(USER, FRONTEND, actor, id, &branch, command)?;
        let duration = elapsed(started);
        let after = work_counts();
        self.commands.push(duration);
        self.add_counts(before, after);
        for (name, value) in [
            ("simulation_ms", profile.simulation_transition),
            ("perception_ms", profile.perception),
            ("navigation_ms", profile.navigation_refresh),
            ("revision_ms", profile.revision_detection),
            ("checkpoint_capture_ms", profile.checkpoint_capture),
        ] {
            *self.phases.entry(name).or_default() += value.as_secs_f64() * 1000.;
        }
        *self.counts.entry("revision_comparisons").or_default() += profile.revision_comparisons;
        *self.counts.entry("actors_observed").or_default() += profile.actors_observed;
        // Publication projection and event counting are explicitly outside the
        // command/counter window. Deduplicate ordinals if a view retains events.
        let started = Instant::now();
        let state = engine.state(ATTACKER)?;
        for event in &state
            .observation
            .combat
            .as_ref()
            .ok_or("missing combat")?
            .events
        {
            if let CombatEventView::Ability {
                strike: Some(ordinal),
                outcome,
                ..
            } = event
            {
                self.primary.insert(*ordinal, *outcome);
            }
        }
        self.projection_ms += elapsed(started);
        Ok(())
    }

    fn add_counts(&mut self, before: WorkCounts, after: WorkCounts) {
        macro_rules! count {
            ($($field:ident),+ $(,)?) => { $(
                *self.counts.entry(stringify!($field)).or_default() += after.$field - before.$field;
            )+ };
        }
        count!(
            ability_plans,
            game_clones,
            preparation_candidates,
            actor_table_copies,
            observations,
            scenes,
            body_cells,
            actor_candidates,
            health_derivations,
            strike_resolutions,
            rider_resolutions
        );
    }
}

struct Evidence {
    states: Vec<u8>,
    journal: Vec<u8>,
}
fn evidence(engine: &Engine) -> Result<Evidence> {
    let actors = engine.actors();
    let states = actors
        .iter()
        .map(|actor| engine.state(*actor))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut history = Vec::new();
    for actor in actors {
        let mut before = None;
        loop {
            let page = engine.history(actor, USER, before.as_ref(), 64)?;
            before = page.older_before.clone();
            history.push((actor, page));
            if before.is_none() {
                break;
            }
        }
    }
    Ok(Evidence {
        states: serde_json::to_vec(&states)?,
        journal: serde_json::to_vec(&history)?,
    })
}

fn verify_evidence(engine: &Engine, expected: &Evidence) -> Result<()> {
    let actual = evidence(engine)?;
    require(
        actual.states == expected.states,
        "reopen changed an actor's disclosed state",
    )?;
    require(
        actual.journal == expected.journal,
        "reopen changed disclosed journal entries or identities",
    )
}

fn recovery(
    row: &mut Value,
    stage: &str,
    engine: &Engine,
    checkpoint: bool,
    setup: u64,
    commands: usize,
) -> Result<()> {
    let profile = engine.recovery_profile();
    require(
        profile.records_loaded == setup as usize + commands,
        "wrong reopened accepted prefix",
    )?;
    require(
        profile.checkpoint_sequence
            == if checkpoint {
                setup + commands as u64
            } else {
                setup
            },
        "reopen did not use the expected real checkpoint",
    )?;
    require(
        profile.records_replayed == if checkpoint { 0 } else { commands },
        "wrong bounded replay tail",
    )?;
    row[format!("{stage}_records_loaded")] = json!(profile.records_loaded);
    row[format!("{stage}_checkpoint_sequence")] = json!(profile.checkpoint_sequence);
    row[format!("{stage}_records_replayed")] = json!(profile.records_replayed);
    Ok(())
}

fn sample(
    strikes: usize,
    riders: usize,
    actors: usize,
    regions: usize,
    seed: u64,
    mode: &str,
) -> Result<Value> {
    require(
        ["checkpoint", "tail"].contains(&mode),
        "unknown persistence mode",
    )?;
    let checkpoint = mode == "checkpoint";
    let setup_start = Instant::now();
    let scenario = scenario(strikes, riders, actors, regions, seed)?;
    // Full package construction with streaming disabled loads every authored
    // region; all actors are additionally counted through the live Engine API.
    let loaded_regions = scenario
        .package
        .as_ref()
        .ok_or("missing package")?
        .region_defs()?
        .len();
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("strikes.db");
    let mut engine = Engine::open_with_policy(
        &path,
        scenario,
        SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )?;
    let loaded_actors = engine.actors().len();
    require(
        loaded_regions == regions && loaded_actors == actors,
        "incorrect loaded dimensions",
    )?;
    for id in 1..=actors {
        let actor = ActorId(id as u64);
        require(
            engine.next_actor() == Some(actor),
            "unexpected setup actor ordering",
        )?;
        act(&mut engine, actor, &format!("setup-{id}"), Action::Wait)?;
    }
    require(
        engine.next_actor() == Some(ATTACKER),
        "attacker is not ready after setup",
    )?;
    engine.flush()?;
    let setup_checkpoint = engine.save_status().checkpoint_sequence;
    require(
        setup_checkpoint == actors as u64,
        "setup checkpoint was not persisted",
    )?;
    drop(engine);
    let policy = SavePolicy {
        checkpoint_interval: if checkpoint { 1 } else { 4096 },
        ..Default::default()
    };
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy.clone())?;
    require(
        engine.recovery_profile().records_replayed == 0,
        "setup checkpoint unexpectedly replayed",
    )?;
    let setup_ms = elapsed(setup_start);
    let projection_start = Instant::now();
    let health_before = engine
        .state(TARGET)?
        .observation
        .combat
        .ok_or("missing target combat")?
        .hp;
    let mut measurements = Measurements {
        projection_ms: elapsed(projection_start),
        ..Default::default()
    };
    measurements.command(
        &mut engine,
        ATTACKER,
        "start",
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("benchmark_sequence")
                .expect("static ability ID"),
            invocation: tor_server::journal::AbilityInvocation::ActorTarget {
                target: tor_simulation::ActorId(TARGET.0),
            },
            supplies: Default::default(),
        },
    )?;
    if strikes > 1 {
        measurements.command(&mut engine, TARGET, "advance-0", Action::Wait)?;
    }
    let prefix_commands = measurements.commands.len();
    let projection_start = Instant::now();
    let prefix_state = engine.state(ATTACKER)?;
    let prefix_completed = prefix_state
        .observation
        .combat
        .as_ref()
        .ok_or("missing combat")?
        .sequence
        .as_ref()
        .map_or(0, |sequence| sequence.completed);
    require(
        prefix_completed == u16::from(strikes > 1),
        "wrong retained strike prefix",
    )?;
    let prefix_evidence = evidence(&engine)?;
    measurements.projection_ms += elapsed(projection_start);
    let (plan_json_bytes, plan_encoding) = engine
        .profile_frozen_plan_json_encoding(ATTACKER)?
        .ok_or("no frozen plan at save boundary")?;
    let (checkpoint_json_bytes, checkpoint_encoding) = engine.profile_checkpoint_json_encoding()?;
    let started = Instant::now();
    engine.flush()?;
    let save_ms = elapsed(started);
    drop(engine);
    let saved_bytes = std::fs::metadata(&path)?.len();
    let started = Instant::now();
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy.clone())?;
    let resume_ms = elapsed(started);
    let mut row = json!({"workload":"ordered-strikes","version":1,"strikes":strikes,"riders":riders,
        "actors":actors,"regions":regions,"mode":mode,"sample":seed-42,"seed":seed,
        "loaded_actors":loaded_actors,"loaded_regions":loaded_regions,"setup_commands":actors,
        "setup_checkpoint_sequence":setup_checkpoint,"checkpoint_interval":policy.checkpoint_interval,
        "prefix_completed":prefix_completed,"prefix_commands":prefix_commands,
        "setup_ms":setup_ms,"plan_encoding_ms":plan_encoding.as_secs_f64()*1000.,
        "checkpoint_encoding_ms":checkpoint_encoding.as_secs_f64()*1000.,
        "plan_json_bytes":plan_json_bytes,"checkpoint_json_bytes":checkpoint_json_bytes,
        "saved_bytes":saved_bytes,"save_ms":save_ms,"resume_ms":resume_ms});
    recovery(
        &mut row,
        "prefix",
        &engine,
        checkpoint,
        setup_checkpoint,
        prefix_commands,
    )?;
    let started = Instant::now();
    verify_evidence(&engine, &prefix_evidence)?;
    measurements.projection_ms += elapsed(started);
    row["prefix_state_equal"] = json!(true);
    row["prefix_journal_equal"] = json!(true);
    let waits = (strikes * 100).div_ceil(150);
    for wait in usize::from(strikes > 1)..waits {
        require(
            engine.next_actor() == Some(TARGET),
            "unexpected continuation actor",
        )?;
        measurements.command(
            &mut engine,
            TARGET,
            &format!("advance-{wait}"),
            Action::Wait,
        )?;
    }
    require(
        measurements.counts["strike_resolutions"] == strikes,
        "not every primary resolved",
    )?;
    require(
        measurements.primary.len() == strikes,
        "not every primary outcome was observed",
    )?;
    let started = Instant::now();
    let final_state = engine.state(ATTACKER)?;
    let combat = final_state
        .observation
        .combat
        .as_ref()
        .ok_or("missing combat")?;
    require(
        combat.sequence.is_none() && combat.preparation_remaining.is_none(),
        "sequence did not finish",
    )?;
    let health_after = engine
        .state(TARGET)?
        .observation
        .combat
        .ok_or("missing target combat")?
        .hp;
    require(health_after > 0, "target died")?;
    let final_evidence = evidence(&engine)?;
    measurements.projection_ms += elapsed(started);
    let started = Instant::now();
    engine.flush()?;
    row["final_save_ms"] = json!(elapsed(started));
    drop(engine);
    row["final_saved_bytes"] = json!(std::fs::metadata(&path)?.len());
    let started = Instant::now();
    let engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy)?;
    row["final_resume_ms"] = json!(elapsed(started));
    recovery(
        &mut row,
        "final",
        &engine,
        checkpoint,
        setup_checkpoint,
        measurements.commands.len(),
    )?;
    let started = Instant::now();
    verify_evidence(&engine, &final_evidence)?;
    measurements.projection_ms += elapsed(started);
    row["final_state_equal"] = json!(true);
    row["final_journal_equal"] = json!(true);
    row["disclosed_bytes"] = json!(final_evidence.states.len());
    row["command_count"] = json!(measurements.commands.len());
    row["continuation_commands"] = json!(measurements.commands.len() - prefix_commands);
    row["admission_start_ms"] = json!(measurements.commands[0]);
    row["advancement_ms"] = json!(measurements.commands[1..].iter().sum::<f64>());
    row["sequence_ms"] = json!(measurements.commands.iter().sum::<f64>());
    row["command_ms"] = json!(measurements.commands);
    row["comparison_projection_ms"] = json!(measurements.projection_ms);
    row["target_health_before"] = json!(health_before);
    row["target_health_after"] = json!(health_after);
    row["primary_hits"] = json!(measurements
        .primary
        .values()
        .filter(|outcome| **outcome == AbilityOutcome::Applied)
        .count());
    for (name, value) in measurements.phases {
        row[name] = json!(value);
    }
    for (name, value) in measurements.counts {
        row[name] = json!(value);
    }
    Ok(row)
}

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    require(
        arguments.is_empty() || arguments == ["--smoke"],
        "usage: strike_bench [--smoke]",
    )?;
    let smoke = !arguments.is_empty();
    for (strikes, riders) in [(1, 0), (3, 2), (16, 8)] {
        for actors in [2, 256] {
            for regions in [16, 256] {
                if smoke && (strikes, riders, actors, regions) != (3, 2, 2, 16) {
                    continue;
                }
                for mode in ["checkpoint", "tail"] {
                    for seed in [42, 43, 44] {
                        println!("{}", sample(strikes, riders, actors, regions, seed, mode)?);
                    }
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn ordered_strike_workload_has_the_requested_loaded_dimensions() {
        let scenario = super::scenario(16, 8, 256, 256, 42).unwrap();
        assert!(scenario.streaming.is_none());
        assert_eq!(
            scenario
                .package
                .as_ref()
                .unwrap()
                .region_defs()
                .unwrap()
                .len(),
            256
        );
        let engine = tor_server::Engine::memory(scenario).unwrap();
        assert_eq!(engine.actors().len(), 256);
        assert_eq!(engine.next_actor(), Some(tor_protocol::ActorId(1)));
    }

    #[test]
    fn ordered_strike_workload_finishes_and_reopens_only_its_bounded_journal_tail() {
        let row = super::sample(3, 2, 2, 16, 42, "tail").unwrap();
        assert_eq!(row["strike_resolutions"], 3);
        assert_eq!(row["setup_checkpoint_sequence"], 2);
        assert_eq!(row["prefix_checkpoint_sequence"], 2);
        assert_eq!(row["prefix_records_replayed"], 2);
        assert_eq!(row["final_records_replayed"], 3);
        for field in [
            "prefix_state_equal",
            "prefix_journal_equal",
            "final_state_equal",
            "final_journal_equal",
        ] {
            assert_eq!(row[field], true);
        }
    }
}
