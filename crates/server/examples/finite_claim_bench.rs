//! Isolated finite-claims v1: ordinary admission, reserve, settlement and reopen.
//! Private authority is reduced to scalar diagnostics outside measured commands.
//! The claims_equal flags compare that scalar profile, not the private claim graph.
//! A 27-owner local stack and 64 owners across eight stacks are distinct cases;
//! neither is described as exercising the per-item 64-record safety ceiling.
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Instant};
use tor_protocol::ActorId;
use tor_server::{
    journal::{Action, Command, JournalContent, SelectedSupply, SupplySelection},
    scenario_package::{Manifest, Package, RegionDef},
    Engine, SavePolicy, Scenario,
};
use tor_simulation::diagnostics::{work_counts, WorkCounts};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const USER: &str = "finite-claim-benchmark";
const FRONTEND: &str = "finite-claims-v1";
const REGIONS: usize = 16;
const PLAN_BUDGET: u64 = 16 * 1024;
const CHECKPOINT_BUDGET: u64 = 64 * 1024 * 1024;

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

#[derive(Clone, Debug)]
struct Case {
    name: String,
    ground: bool,
    costs: usize,
    fragments: usize,
    owners: usize,
    successes: usize,
    maximum_holds: usize,
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for ground in [false, true] {
        for (costs, fragments) in [(1, 1), (1, 32), (8, 8), (8, 32)] {
            let access = if ground { "ground" } else { "carried" };
            cases.push(Case {
                name: format!("{access}-c{costs}-f{fragments}"),
                ground,
                costs,
                fragments,
                owners: 1,
                successes: 1,
                maximum_holds: 1,
            });
        }
    }
    for (name, owners, successes, maximum_holds) in [
        ("last-unit-2", 2, 1, 1),
        ("shared-stack-27", 27, 27, 27),
        ("shared-stacks-64", 64, 64, 8),
    ] {
        cases.push(Case {
            name: name.into(),
            ground: true,
            costs: 1,
            fragments: 1,
            owners,
            successes,
            maximum_holds,
        });
    }
    cases
}
impl Case {
    fn units(&self) -> usize {
        self.successes * self.fragments
    }
    fn smoke(&self) -> bool {
        ["ground-c8-f32", "last-unit-2"].contains(&self.name.as_str())
    }
    fn source(&self, owner: usize, fragment: usize) -> u64 {
        100 + if self.owners == 64 {
            (owner - 1) / 8
        } else {
            fragment
        } as u64
    }
    fn placement(&self, owner: usize) -> (usize, [i32; 3]) {
        if self.owners == 27 {
            let i = owner - 1;
            (
                0,
                [
                    1 + (i % 3) as i32,
                    1 + ((i / 3) % 3) as i32,
                    1 + (i / 9) as i32,
                ],
            )
        } else if self.owners == 64 {
            // The eight distinct corners all touch their region's center stack.
            let i = (owner - 1) % 8;
            (
                (owner - 1) / 8,
                [
                    1 + 2 * (i % 2) as i32,
                    1 + 2 * ((i / 2) % 2) as i32,
                    1 + 2 * (i / 4) as i32,
                ],
            )
        } else {
            (0, [if owner == 1 { 1 } else { 3 }, 2, 2])
        }
    }
    fn action(&self, owner: usize) -> Action {
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("benchmark_claim").expect("static ID"),
            invocation: tor_server::journal::AbilityInvocation {
                aim: tor_server::journal::AbilityAim::ActorTarget {
                    target: tor_simulation::ActorId(owner as u64),
                },
                weapon: tor_server::journal::WeaponSelection::Default,
            },
            supplies: if self.ground {
                SupplySelection::Explicit {
                    items: (0..self.fragments)
                        .map(|fragment| SelectedSupply {
                            cost_ordinal: (fragment / (self.fragments / self.costs)) as u8,
                            item: self.source(owner, fragment),
                        })
                        .collect(),
                }
            } else {
                SupplySelection::Carried
            },
        }
    }
}

fn scenario(case: &Case, background: usize, seed: u64) -> Result<Scenario> {
    require([0, 64].contains(&background), "unknown background density")?;
    let mut manifest: Manifest =
        toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml"))?;
    manifest.id = "finite-claim-benchmark".into();
    manifest.objective = None;
    manifest.characters.truncate(1);
    // The source scenario has a two-cell body. Every workload actor explicitly
    // uses the same one-cell body, including the selected character.
    let body = json!({"cells":[[0,0,0]],"eye":[0,0,0],"mass":80});
    manifest.characters[0].body = Some(serde_json::from_value(body.clone())?);
    manifest.characters[0].turn_ticks = 100;
    let mut document: Value =
        serde_json::to_value(toml::from_str::<tor_simulation::rules::RulesetDocument>(
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )?)?;
    let mut ability = document["abilities"]
        .as_array()
        .ok_or("abilities absent")?
        .iter()
        .find(|ability| ability["id"] == "bind_wounds")
        .ok_or("bundled Bind Wounds absent")?
        .clone();
    ability["id"] = json!("benchmark_claim");
    ability["name"] = json!("Benchmark claim");
    ability["cooldown"] = Value::Null;
    ability["cost"] = Value::Null;
    ability["timing"] =
        json!({"source":{"kind":"fixed","prepare":1000,"recover":1},"scaled_by":null});
    ability["reach"] = json!({"kind":"self_or_ally","range":{"kind":"touch"}});
    ability["strikes"] = json!({"kind":"authored","strikes":[{"offset":0,"unlock":null,"check":null,
        "on_success":[{"kind":"heal","amount":{"kind":"fixed","amount":3}}],
        "on_failure":[],"descriptors":[],"riders":[]}]});
    let costs: Vec<_> = (0..case.costs).map(|ordinal| json!({"supply":format!("bench_supply_{ordinal}"),
        "quantity":case.fragments / case.costs,"access":if case.ground {"carried_or_ground_touch"} else {"carried_only"},
        "spend_at":"first_resolution"})).collect();
    ability["supply_costs"] = json!(costs);
    document["abilities"]
        .as_array_mut()
        .ok_or("abilities absent")?
        .push(ability);
    for ordinal in 0..case.costs {
        document["supplies"].as_array_mut().ok_or("supply catalog absent")?.push(
            json!({"id":format!("bench_supply_{ordinal}"),"name":format!("Benchmark supply {ordinal}")}));
        manifest.archetypes.insert(format!("bench_supply_{ordinal}"), serde_json::from_value(json!({
            "name":format!("Benchmark supply {ordinal}"),"identity":format!("bench_supply_{ordinal}"),
            "supply":format!("bench_supply_{ordinal}"),"class":"misc","stackable":true}))?);
    }
    let rules: tor_simulation::rules::RulesetDocument = serde_json::from_value(document)?;
    let natural = json!({"offset":0,"unlock":null,"check":null,
        "on_success":[{"kind":"damage","damage":{"kind":"components","components":[{
            "category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}}],
        "on_failure":[],"descriptors":[],"riders":[]});
    manifest.creatures = serde_json::from_value(json!({"species":{"benchmark_carer":{
        "kind":"humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},"attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[natural]},
        "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"benchmark_claim"}}]}}}))?;
    let creature = json!({"species":"benchmark_carer","name":"Benchmark carer","faction":"neutral",
        "binding":"intellect","starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":[{"source":"racial"}]});
    manifest.characters[0].creature = Some(serde_json::from_value(creature.clone())?);
    let first = case.placement(1).1;
    let mut regions: Vec<RegionDef> = (1..=REGIONS)
        .map(|id| {
            serde_json::from_value(json!({
        "id":id,"name":format!("Finite claim chamber {id}"),"size":[12,12,5],"chamber":true,
        "anchors":{"start":if id == 1 {first} else {[1,1,1]}}}))
        })
        .collect::<std::result::Result<_, _>>()?;
    let actors = case.owners + 1 + background;
    for id in 2..=actors {
        let (region, at, turn_ticks) = if id <= case.owners {
            let (region, at) = case.placement(id);
            (region, at, 100)
        } else if id == case.owners + 1 {
            (0, [8, 8, 2], 1000)
        } else {
            let i = id - case.owners - 2;
            (
                8 + i % 8,
                [1 + (i / 8 % 4) as i32, 1 + (i / 32) as i32, 1],
                100000,
            )
        };
        regions[region]
            .actors
            .push(serde_json::from_value(json!({"id":id,"at":at,
            "turn_ticks":turn_ticks,"controller":"external","creature":creature,"body":body}))?);
    }
    let sources = if case.owners == 64 { 8 } else { case.fragments };
    for fragment in 0..sources {
        let (region, ordinal, quantity) = if case.owners == 64 {
            (fragment, 0, 8)
        } else {
            (
                0,
                fragment / (case.fragments / case.costs),
                if case.owners == 27 { 27 } else { 1 },
            )
        };
        regions[region]
            .items
            .push(serde_json::from_value(json!({"id":100 + fragment,
            "at":[2,2,2],"archetype":format!("bench_supply_{ordinal}"),
            "name":format!("finite-claim-source-{}",100 + fragment),"quantity":quantity,
            "carried_by":if case.ground {None} else {Some(1)}}))?);
    }
    // No gravity, portals, overlapping actors or injected holds are used. Full
    // ordinary package instantiation validates cells, bodies and identities.
    let package = Package::from_parts(manifest, regions, &toml::to_string(&rules)?)?;
    Ok(Scenario {
        seed,
        actors: vec![],
        regions: REGIONS as u64,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: None,
    })
}

fn command(
    engine: &mut Engine,
    actor: ActorId,
    request: &str,
    action: Action,
    admit: bool,
) -> Result<()> {
    let revision = engine.revision(actor)?;
    let command = if admit {
        Command::AdmitIntention {
            expected_revision: revision,
            action,
        }
    } else {
        Command::Act {
            expected_revision: revision,
            action,
        }
    };
    engine.command(
        USER,
        FRONTEND,
        actor,
        request,
        &engine.branch().clone(),
        command,
    )?;
    Ok(())
}

#[derive(Default)]
struct Measurements {
    intervals: BTreeMap<&'static str, Vec<f64>>,
    counts: BTreeMap<String, usize>,
}
impl Measurements {
    fn measure<T>(&mut self, phase: &'static str, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let before = work_counts();
        let started = Instant::now();
        let result = work()?;
        let duration = elapsed(started);
        let after = work_counts();
        self.intervals.entry(phase).or_default().push(duration);
        self.add_counts(phase, before, after);
        Ok(result)
    }
    fn add_counts(&mut self, phase: &str, before: WorkCounts, after: WorkCounts) {
        macro_rules! count { ($($field:ident),+ $(,)?) => { $(
            *self.counts.entry(format!("{phase}_{}",stringify!($field))).or_default() += after.$field - before.$field;
        )+ }; }
        count!(
            game_clones,
            scenes,
            ordinary_scenes,
            observations,
            ability_plans,
            preparation_candidates,
            strike_resolutions,
            scene_membership_lookups,
            scene_membership_cells,
            scene_cells_copied
        );
    }
    fn records(&self) -> usize {
        self.intervals.values().map(Vec::len).sum()
    }
    fn record(&self, row: &mut Value) {
        let mut total = 0.;
        for (phase, field) in [
            ("admission", "admission_ms"),
            ("start", "start_ms"),
            ("settlement", "advancement_ms"),
        ] {
            row[field] = json!(self.intervals[phase]);
            total += self.intervals[phase].iter().sum::<f64>();
        }
        row["measured_ms"] = json!(total);
        for (key, value) in &self.counts {
            row[key] = json!(value);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Claims {
    fragments: usize,
    owners: usize,
    maximum: usize,
    units: u64,
}
fn claims(engine: &Engine) -> Result<Claims> {
    let profile = engine.profile_finite_claims()?;
    require(
        profile.item_holds == profile.preparation_refs,
        "item authority/reference count disagreement",
    )?;
    Ok(Claims {
        fragments: profile.preparation_refs,
        owners: profile.claimant_actors,
        maximum: profile.max_holds_per_item,
        units: profile.held_units,
    })
}
impl Claims {
    fn record(&self, row: &mut Value, stage: &str) {
        row[format!("{stage}_held_fragments")] = json!(self.fragments);
        row[format!("{stage}_claim_owners")] = json!(self.owners);
        row[format!("{stage}_max_item_holds")] = json!(self.maximum);
        row[format!("{stage}_held_units")] = json!(self.units);
    }
}

struct Evidence {
    states: Vec<u8>,
    history: Vec<u8>,
    claims: Claims,
    physical_units: u64,
}
fn evidence(engine: &Engine) -> Result<Evidence> {
    let mut states = Vec::new();
    let mut history = Vec::new();
    let mut sources = BTreeMap::new();
    for actor in engine.actors() {
        let state = engine.state(actor)?;
        for item in state.observation.inventory.iter().chain(
            state
                .observation
                .ground_items
                .iter()
                .map(|ground| &ground.item),
        ) {
            require(
                item.name.starts_with("finite-claim-source-"),
                "unexpected workload item",
            )?;
            if let Some(previous) = sources.insert(item.name.clone(), item.quantity) {
                require(
                    previous == item.quantity,
                    "observers disagree about physical quantity",
                )?;
            }
        }
        states.push(state);
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
        history: serde_json::to_vec(&history)?,
        claims: claims(engine)?,
        physical_units: sources.values().sum(),
    })
}
fn compare_evidence(engine: &Engine, expected: &Evidence) -> Result<()> {
    let actual = evidence(engine)?;
    require(
        actual.states == expected.states,
        "reopen changed disclosed actor states",
    )?;
    require(
        actual.history == expected.history,
        "reopen changed disclosed journal content/identity",
    )?;
    require(
        actual.claims == expected.claims,
        "reopen changed private scalar claim profile",
    )?;
    require(
        actual.physical_units == expected.physical_units,
        "reopen changed physical item quantities",
    )
}
fn recovery(
    row: &mut Value,
    stage: &str,
    engine: &Engine,
    setup: usize,
    records: usize,
    checkpoint: bool,
) -> Result<()> {
    let profile = engine.recovery_profile();
    require(
        profile.records_loaded == setup + records,
        "wrong reopened accepted prefix",
    )?;
    require(
        profile.checkpoint_sequence == (if checkpoint { setup + records } else { setup }) as u64,
        "reopen did not select the expected real checkpoint",
    )?;
    require(
        profile.records_replayed == if checkpoint { 0 } else { records },
        "wrong bounded replay tail",
    )?;
    row[format!("{stage}_records_loaded")] = json!(profile.records_loaded);
    row[format!("{stage}_checkpoint_sequence")] = json!(profile.checkpoint_sequence);
    row[format!("{stage}_records_replayed")] = json!(profile.records_replayed);
    Ok(())
}
fn checkpoint_bytes(engine: &Engine) -> Result<(u64, f64)> {
    let (bytes, duration) = engine.profile_checkpoint_json_encoding()?;
    require(
        bytes <= CHECKPOINT_BUDGET,
        "logical checkpoint exceeds diagnostic 64 MiB budget",
    )?;
    Ok((bytes, duration.as_secs_f64() * 1000.))
}

fn sample(case: &Case, background: usize, seed: u64, mode: &str) -> Result<Value> {
    require(
        ["checkpoint", "tail"].contains(&mode),
        "unknown persistence mode",
    )?;
    let checkpoint = mode == "checkpoint";
    let setup_started = Instant::now();
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("finite-claims.db");
    let authored = scenario(case, background, seed)?;
    let loaded_regions = authored
        .package
        .as_ref()
        .ok_or("missing package")?
        .region_defs()?
        .len();
    require(
        authored.streaming.is_none() && loaded_regions == REGIONS,
        "fixture must load all authored regions",
    )?;
    let mut engine = Engine::open_with_policy(
        &path,
        authored,
        SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )?;
    let actors = case.owners + 1 + background;
    require(engine.actors().len() == actors, "wrong loaded actor count")?;
    for id in 1..=actors {
        let actor = ActorId(id as u64);
        require(
            engine.next_actor() == Some(actor),
            "unexpected setup actor order",
        )?;
        command(
            &mut engine,
            actor,
            &format!("setup-{id}"),
            Action::Wait,
            false,
        )?;
    }
    engine.flush()?;
    require(
        engine.save_status().checkpoint_sequence == actors as u64,
        "setup checkpoint not durable",
    )?;
    drop(engine);
    let policy = SavePolicy {
        checkpoint_interval: if checkpoint { 1 } else { 4096 },
        ..Default::default()
    };
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy.clone())?;
    require(
        engine.recovery_profile().records_replayed == 0,
        "setup unexpectedly replayed",
    )?;
    let setup_ms = elapsed(setup_started);
    let diagnostic_started = Instant::now();
    let initial = evidence(&engine)?;
    require(
        initial.physical_units == case.units() as u64 && initial.claims.fragments == 0,
        "incorrect initial ordinary supplies",
    )?;
    let mut diagnostic_ms = elapsed(diagnostic_started);
    let mut measured = Measurements::default();
    for owner in 1..=case.owners {
        measured.measure("admission", || {
            command(
                &mut engine,
                ActorId(owner as u64),
                &format!("admit-{owner}"),
                case.action(owner),
                true,
            )
        })?;
    }
    let mut successful = Vec::new();
    let mut failed = 0;
    for owner in 1..=case.owners {
        let result = measured.measure("start", || {
            Ok(engine
                .execute_next_intention()?
                .ok_or("queued owner did not execute")?)
        })?;
        require(
            result.entry.actor == ActorId(owner as u64),
            "unexpected execution actor order",
        )?;
        match result.entry.content {
            JournalContent::IntentionStarted { .. } => successful.push(ActorId(owner as u64)),
            JournalContent::IntentionFailed { .. } => failed += 1,
            _ => return Err("unexpected admission execution record".into()),
        }
    }
    require(
        successful.len() == case.successes && failed == case.owners - case.successes,
        "wrong genuine contention outcomes",
    )?;
    let prefix_records = measured.records();
    let started = Instant::now();
    let prefix = evidence(&engine)?;
    require(
        prefix.claims.fragments == case.units()
            && prefix.claims.owners == case.successes
            && prefix.claims.maximum == case.maximum_holds
            && prefix.claims.units == case.units() as u64,
        "wrong real held fragment/owner/unit counts",
    )?;
    require(
        prefix.physical_units == initial.physical_units,
        "reservation spent physical units early",
    )?;
    diagnostic_ms += elapsed(started);
    let mut plan_total = 0;
    let mut plan_max = 0;
    let mut plan_encoding_ms = 0.;
    for actor in &successful {
        let (bytes, duration) = engine
            .profile_frozen_plan_json_encoding(*actor)?
            .ok_or("missing held plan")?;
        require(
            bytes <= PLAN_BUDGET,
            "captured plan exceeds diagnostic 16 KiB budget",
        )?;
        plan_total += bytes;
        plan_max = plan_max.max(bytes);
        plan_encoding_ms += duration.as_secs_f64() * 1000.;
    }
    let (prefix_bytes, prefix_encoding_ms) = checkpoint_bytes(&engine)?;
    let started = Instant::now();
    engine.flush()?;
    let prefix_save_ms = elapsed(started);
    drop(engine);
    let prefix_file_bytes = std::fs::metadata(&path)?.len();
    let started = Instant::now();
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy.clone())?;
    let prefix_resume_ms = elapsed(started);
    let mut row = json!({"workload":"finite-claims","version":1,"case":case.name,
        "background_actors":background,"mode":mode,"sample":seed-42,"seed":seed,
        "access":if case.ground {"ground"} else {"carried"},"costs":case.costs,
        "fragments_per_use":case.fragments,"competing_owners":case.owners,
        "loaded_actors":actors,"loaded_regions":loaded_regions,"setup_records":actors,
        "setup_checkpoint_sequence":actors,"checkpoint_interval":policy.checkpoint_interval,
        "admitted_uses":case.owners,"successful_starts":successful.len(),"failed_starts":failed,
        "item_hold_limit":tor_simulation::supplies::MAX_ITEM_HOLDS,
        "fragment_limit":tor_simulation::supplies::MAX_ITEM_FRAGMENTS,"cost_limit":8,
        "physical_units_before":initial.physical_units,"physical_units_at_prefix":prefix.physical_units,
        "prefix_records":prefix_records,"plan_count":successful.len(),"plan_max_json_bytes":plan_max,
        "plan_total_json_bytes":plan_total,"plan_encoding_ms":plan_encoding_ms,"setup_ms":setup_ms,
        "prefix_checkpoint_json_bytes":prefix_bytes,"prefix_checkpoint_encoding_ms":prefix_encoding_ms,
        "prefix_sqlite_file_bytes":prefix_file_bytes,"prefix_save_ms":prefix_save_ms,"prefix_resume_ms":prefix_resume_ms});
    prefix.claims.record(&mut row, "prefix");
    recovery(
        &mut row,
        "prefix",
        &engine,
        actors,
        prefix_records,
        checkpoint,
    )?;
    let started = Instant::now();
    compare_evidence(&engine, &prefix)?;
    let mut comparison_ms = elapsed(started);
    for field in [
        "prefix_state_equal",
        "prefix_journal_equal",
        "prefix_claims_equal",
    ] {
        row[field] = json!(true);
    }
    let mut advances = 0;
    loop {
        let started = Instant::now();
        let retained = claims(&engine)?.fragments;
        diagnostic_ms += elapsed(started);
        if retained == 0 {
            break;
        }
        require(
            advances < 11 * (case.owners + 1),
            "ordinary settlement did not finish within its fixture horizon",
        )?;
        let actor = engine
            .next_actor()
            .ok_or("no actor can advance settlement")?;
        require(
            engine.profile_frozen_plan_json_encoding(actor)?.is_none(),
            "settlement waiter must not replace a preparation",
        )?;
        measured.measure("settlement", || {
            command(
                &mut engine,
                actor,
                &format!("advance-{advances}"),
                Action::Wait,
                false,
            )
        })?;
        advances += 1;
    }
    let started = Instant::now();
    let final_evidence = evidence(&engine)?;
    require(
        final_evidence.claims
            == (Claims {
                fragments: 0,
                owners: 0,
                maximum: 0,
                units: 0,
            }),
        "claims remain after settlement",
    )?;
    require(
        final_evidence.physical_units == 0,
        "successful uses did not consume exact finite supplies",
    )?;
    for actor in successful {
        require(
            engine.profile_frozen_plan_json_encoding(actor)?.is_none(),
            "preparation remains after settlement",
        )?;
    }
    diagnostic_ms += elapsed(started);
    let resolved = measured.counts["settlement_strike_resolutions"];
    require(
        resolved == case.successes,
        "wrong actual successful resolution count",
    )?;
    let (final_bytes, final_encoding_ms) = checkpoint_bytes(&engine)?;
    let started = Instant::now();
    engine.flush()?;
    row["final_save_ms"] = json!(elapsed(started));
    drop(engine);
    row["final_sqlite_file_bytes"] = json!(std::fs::metadata(&path)?.len());
    let started = Instant::now();
    let engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy)?;
    row["final_resume_ms"] = json!(elapsed(started));
    recovery(
        &mut row,
        "final",
        &engine,
        actors,
        measured.records(),
        checkpoint,
    )?;
    let started = Instant::now();
    compare_evidence(&engine, &final_evidence)?;
    comparison_ms += elapsed(started);
    for field in [
        "final_state_equal",
        "final_journal_equal",
        "final_claims_equal",
    ] {
        row[field] = json!(true);
    }
    final_evidence.claims.record(&mut row, "final");
    row["final_checkpoint_json_bytes"] = json!(final_bytes);
    row["final_checkpoint_encoding_ms"] = json!(final_encoding_ms);
    row["physical_units_after"] = json!(final_evidence.physical_units);
    row["consumed_units"] = json!(initial.physical_units - final_evidence.physical_units);
    row["successful_resolutions"] = json!(resolved);
    row["advancement_commands"] = json!(advances);
    row["final_records"] = json!(measured.records());
    row["comparison_ms"] = json!(comparison_ms);
    row["diagnostic_ms"] = json!(diagnostic_ms);
    measured.record(&mut row);
    Ok(row)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    require(
        args.is_empty() || args == ["--smoke"],
        "usage: finite_claim_bench [--smoke]",
    )?;
    let smoke = !args.is_empty();
    for case in cases() {
        for background in [0, 64] {
            if smoke && (!case.smoke() || background != 0) {
                continue;
            }
            for mode in ["checkpoint", "tail"] {
                for seed in [42, 43, 44] {
                    println!("{}", sample(&case, background, seed, mode)?);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_valid_conventional_neighborhood_has_exactly_27_claimant_cells() {
        let case = cases().into_iter().find(|case| case.owners == 27).unwrap();
        let cells: std::collections::BTreeSet<_> = (1..=case.owners)
            .map(|owner| case.placement(owner))
            .collect();
        assert_eq!(cells.len(), 27);
        assert!(cells
            .iter()
            .all(|(region, at)| *region == 0 && at.iter().all(|v| (1..=3).contains(v))));
        let engine = Engine::memory(scenario(&case, 64, 42).unwrap()).unwrap();
        assert_eq!(engine.actors().len(), 92);
        assert_eq!(claims(&engine).unwrap().fragments, 0);
    }

    #[test]
    fn ordinary_last_unit_race_reserves_once_and_replays_the_real_bounded_tail() {
        let case = cases().into_iter().find(|case| case.owners == 2).unwrap();
        let row = sample(&case, 0, 42, "tail").unwrap();
        assert_eq!(row["admitted_uses"], 2);
        assert_eq!(row["successful_starts"], 1);
        assert_eq!(row["failed_starts"], 1);
        assert_eq!(row["prefix_max_item_holds"], 1);
        assert_eq!(row["successful_resolutions"], 1);
        assert_eq!(row["prefix_records_replayed"], 4);
        assert_eq!(row["final_records_replayed"], row["final_records"]);
    }

    #[test]
    fn maximum_costs_and_fragments_reopen_at_exact_ordinary_holds() {
        let case = cases()
            .into_iter()
            .find(|case| case.name == "ground-c8-f32")
            .unwrap();
        for mode in ["checkpoint", "tail"] {
            let row = sample(&case, 0, 42, mode).unwrap();
            assert_eq!(row["prefix_held_fragments"], 32);
            assert_eq!(row["prefix_held_units"], 32);
            assert_eq!(row["successful_resolutions"], 1);
            assert_eq!(row["physical_units_after"], 0);
        }
    }

    #[test]
    fn ordinary_27_and_64_owner_cases_report_actual_per_item_capacity() {
        for (owners, maximum) in [(27, 27), (64, 8)] {
            let case = cases()
                .into_iter()
                .find(|case| case.owners == owners)
                .unwrap();
            let row = sample(&case, 0, 42, "tail").unwrap();
            assert_eq!(row["prefix_claim_owners"], owners);
            assert_eq!(row["prefix_max_item_holds"], maximum);
            assert_eq!(row["successful_resolutions"], owners);
            assert_eq!(row["consumed_units"], owners);
        }
    }

    #[test]
    fn adding_fragments_does_not_add_per_fragment_game_clones_or_scene_builds() {
        for ground in [false, true] {
            let mut rows = Vec::new();
            for fragments in [1, 32] {
                let case = cases()
                    .into_iter()
                    .find(|case| {
                        case.ground == ground && case.costs == 1 && case.fragments == fragments
                    })
                    .unwrap();
                rows.push(sample(&case, 0, 42, "tail").unwrap());
            }
            for phase in ["admission", "start", "settlement"] {
                for counter in [
                    "game_clones",
                    "scenes",
                    "scene_membership_cells",
                    "scene_cells_copied",
                ] {
                    let field = format!("{phase}_{counter}");
                    assert_eq!(rows[0][&field], rows[1][&field], "{field}");
                }
            }
        }
    }
}
