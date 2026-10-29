//! Release diagnostic for 100 executing AI across 20 portal-connected regions.
//! Direct Engine timing is not a substitute for the matching real-client run.
#[path = "../example_support/active_mob_options.rs"]
mod active_mob_options;
#[path = "../example_support/active_mob_trace.rs"]
mod active_mob_trace;
#[path = "../example_support/diagnostic_spans.rs"]
mod diagnostic_spans;
#[path = "../example_support/active_mob_fixture.rs"]
mod fixture;
use active_mob_options::{Mode, Options};
use active_mob_trace::{InspectionTrace, SustainedActivity};
use tor_server as host;

use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tor_protocol::ActorId;
use tor_server::journal::{Action, Command, Direction, Event, JournalContent, JournalEntry};
use tor_server::{CommandProfile, Engine, SavePolicy, Scenario};
use tor_simulation::diagnostics::{work_counts, WorkCounts};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PLAYER: ActorId = ActorId(1);
const FORMAT: &str = "active-mobs-v1";

#[derive(Default, Serialize)]
struct Activity {
    executions: u64,
    moves: u64,
    combat_starts: u64,
    waits: u64,
    portal_crossings: u64,
    first_tick: Option<u64>,
    last_tick: u64,
}

#[derive(Default, Serialize)]
struct ActivityProof {
    actors: BTreeMap<u64, Activity>,
    regions: BTreeMap<u64, Activity>,
    player_moves: u64,
    player_crossings: u64,
}

impl ActivityProof {
    fn observe(&mut self, entry: &JournalEntry) {
        let event = match &entry.content {
            JournalContent::Action { event, .. }
            | JournalContent::IntentionStarted { event, .. }
            | JournalContent::IntentionContinued { event, .. } => event,
            _ => return,
        };
        let crossing = matches!(event, Event::Moved { from, to } if from.region != to.region);
        if entry.actor == PLAYER {
            self.player_moves += u64::from(matches!(event, Event::Moved { .. }));
            self.player_crossings += u64::from(crossing);
            return;
        }
        let Some(region) = fixture::region_of(entry.actor.0) else {
            return;
        };
        for activity in [
            self.actors.entry(entry.actor.0).or_default(),
            self.regions.entry(region).or_default(),
        ] {
            activity.executions += 1;
            activity.moves += u64::from(matches!(event, Event::Moved { .. }));
            activity.combat_starts += u64::from(matches!(event, Event::AbilityStarted { .. }));
            activity.waits += u64::from(matches!(event, Event::Waited));
            activity.portal_crossings += u64::from(crossing);
            activity.first_tick.get_or_insert(entry.tick);
            activity.last_tick = entry.tick;
        }
    }

    fn require_complete(&self) -> Result<()> {
        require(
            self.actors.len() == fixture::MOBS as usize,
            "not every AI executed",
        )?;
        require(
            self.regions.len() == fixture::REGIONS as usize,
            "not every region executed AI",
        )?;
        require(
            self.regions
                .values()
                .all(|a| a.moves > 0 && a.combat_starts > 0),
            "each region must contain pursuit movement and combat",
        )?;
        require(
            self.actors
                .values()
                .all(|a| a.executions > 1 && a.last_tick > a.first_tick.unwrap_or(a.last_tick)),
            "AI did not continue across simulation ticks",
        )?;
        require(
            self.actors
                .values()
                .all(|actor| actor.moves > 0 || actor.combat_starts > 0),
            "every AI must move or start combat, not only wait",
        )?;
        require(
            self.player_crossings > 0
                && self
                    .player_moves
                    .checked_add(1)
                    .map(|moves| moves / fixture::WIDTH as u64)
                    == Some(self.player_crossings),
            "player portal crossings do not match the exact corridor route",
        )
    }
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.
}
fn residency(engine: &Engine) -> Result<()> {
    let counts = engine.region_counts().ok_or("streaming is disabled")?;
    require(
        counts.active == fixture::REGIONS as usize
            && counts.frozen == 0
            && counts.detached == 0
            && counts.unbuilt == 0,
        "not all 20 regions are active",
    )?;
    require(
        engine.actors().len() == fixture::MOBS as usize + 1,
        "actor population changed",
    )?;
    for id in 2..=fixture::MOBS + 1 {
        require(
            engine.is_ai(ActorId(id)) && engine.alive(ActorId(id)),
            "AI died or stopped being AI",
        )?;
    }
    Ok(())
}

#[derive(Default, Serialize)]
struct Samples {
    command_ms: Vec<f64>,
    ai_command_ms: Vec<f64>,
    player_command_ms: Vec<f64>,
    player_cycle_ms: Vec<f64>,
    projection_ms: Vec<f64>,
    serialization_ms: Vec<f64>,
    phase_totals_ms: BTreeMap<&'static str, f64>,
    work: BTreeMap<&'static str, usize>,
    pending_bytes: Vec<usize>,
    rss_bytes: Vec<Option<u64>>,
}
impl Samples {
    fn command(
        &mut self,
        profile: &CommandProfile,
        duration: f64,
        ai: bool,
        before: WorkCounts,
        after: WorkCounts,
    ) {
        self.command_ms.push(duration);
        if ai {
            self.ai_command_ms.push(duration);
        } else {
            self.player_command_ms.push(duration);
        }
        for (name, value) in [
            ("candidate_capture", profile.rollback_capture),
            ("checkpoint_capture", profile.checkpoint_capture),
            ("simulation", profile.simulation_transition),
            ("intention_derivation", profile.intention_derivation),
            ("request_validation", profile.request_validation),
            ("map_recording", profile.map_recording),
            ("commit_metadata", profile.commit_metadata),
            ("input_generation", profile.input_generation),
            ("navigation", profile.navigation_refresh),
            ("perception", profile.perception),
            ("revision_comparison", profile.revision_detection),
            ("rewind_snapshot", profile.rollback_snapshot),
            ("journal_serialization", profile.journal_serialization),
            ("journal_write", profile.journal_write),
            ("journal_sync", profile.journal_sync),
            ("journal_replace", profile.journal_replace),
            ("publication", profile.publication),
            ("region_transition", profile.region_transition),
        ] {
            *self.phase_totals_ms.entry(name).or_default() += ms(value);
        }
        for (name, count) in [
            ("preload_jobs", profile.preload_jobs),
            ("preload_regions_expanded", profile.preload_regions_expanded),
            ("preload_links_examined", profile.preload_links_examined),
        ] {
            *self.work.entry(name).or_default() += count;
        }
        macro_rules! count { ($($field:ident),+ $(,)?) => { $(
            *self.work.entry(stringify!($field)).or_default() += after.$field - before.$field;
        )+ }; }
        count!(
            observations,
            actor_reference_vectors,
            actor_reference_rows,
            own_stats_materializations,
            own_stats_static_entries_copied,
            raw_world_fact_builds,
            raw_world_fact_cells,
            perception_projections,
            projection_location_indexes,
            projection_location_cells,
            hearing_scene_builds,
            hearing_scene_cells,
            scenes,
            actor_candidates,
            actor_disclosures,
            body_cells,
            contact_neighborhoods,
            contact_route_probes,
            item_ground_probes,
            control_contact_neighbor_probes,
            route_searches,
            strike_resolutions,
            ability_plans,
            game_clones,
            actor_table_copies,
            actor_table_entries_copied,
            preparation_candidates,
            preparation_owner_visits,
            preparation_validation_evaluations,
            preparation_validations_due,
            preparation_validations_not_due,
            preparation_validations_paused,
            growth_retry_decisions,
            growth_retry_required,
            growth_retry_shared_noops,
            preparation_owner_projection_requests,
            preparation_owner_projections,
            preparation_owner_owned_projections,
            awareness_subject_candidates,
            preparation_residency_proof_hits,
            region_contribution_actor_rebuilds,
            region_contribution_item_rebuilds,
            region_contribution_bucket_visits,
            region_contribution_cold_refreshes,
            mutation_publications,
            mutation_frames,
            mutation_frame_unknowns,
            preparation_residency_evaluations,
            preparation_snapshots,
            decision_scenes,
            ordinary_scenes,
            scene_membership_cells,
            scene_cells_copied,
            observation_cells_copied,
            ai_precommit_ordinary_scenes,
            ai_visit_entries_copied
        );
        for (name, count) in [
            ("intention_actor_visits", profile.intention_actor_visits),
            (
                "intention_candidate_visits",
                profile.intention_candidate_visits,
            ),
            ("intention_event_visits", profile.intention_event_visits),
        ] {
            *self.work.entry(name).or_default() += count;
        }
        for (name, count) in [
            (
                "action_observer_boundaries",
                profile.action_observer_boundaries,
            ),
            (
                "action_event_receipts_recorded",
                profile.action_event_receipts_recorded,
            ),
            (
                "action_projection_validated",
                profile.action_projection_validated,
            ),
            (
                "action_navigation_reviewed",
                profile.action_navigation_reviewed,
            ),
        ] {
            *self.work.entry(name).or_default() += count;
        }
        *self.work.entry("actors_observed").or_default() += profile.actors_observed;
        *self.work.entry("map_rich_views_reused").or_default() += profile.map_rich_views_reused;
        *self.work.entry("map_appearance_projections").or_default() +=
            profile.map_appearance_projections;
        *self
            .work
            .entry("previous_observer_scene_reads")
            .or_default() += profile.previous_observer_scene_reads;
        *self.work.entry("revision_comparisons").or_default() += profile.revision_comparisons;
        *self.work.entry("observation_changes_proven").or_default() +=
            profile.observation_changes_proven;
        *self
            .work
            .entry("observations_unchanged_proven")
            .or_default() += profile.observations_unchanged_proven;
        *self
            .work
            .entry("observation_projection_fallbacks")
            .or_default() += profile.observation_projection_fallbacks;
        *self.work.entry("navigation_refreshes").or_default() += profile.navigation_refreshes;
    }
}

fn rss() -> Option<u64> {
    // Host-only, best effort, outside command timing. Other OSes report null.
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .map(|kib| kib * 1024)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn evidence(engine: &Engine) -> Result<Vec<u8>> {
    let states = engine
        .actors()
        .into_iter()
        .map(|id| engine.state(id))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(serde_json::to_vec(&states)?)
}

fn bootstrap(path: &Path) -> Result<()> {
    require(!path.exists(), "refusing to replace an existing save")?;
    let package = path.with_extension("package");
    require(!package.exists(), "refusing to replace an existing package")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let start = Instant::now();
    let scenario = fixture::make(&package)?;
    let package_ms = ms(start.elapsed());
    let start = Instant::now();
    let engine = Engine::open(path, scenario)?;
    let cold_activation_ms = ms(start.elapsed());
    residency(&engine)?;
    engine.flush()?;
    let state_hash = hex_digest(&evidence(&engine)?);
    println!(
        "{}",
        json!({"workload":FORMAT,"version":1,"profile_version":3,"observer_work_contract":"causal_facts_demand_display_v1","mode":"bootstrap","seed":fixture::SEED,
        "regions":fixture::REGIONS,"ai":fixture::MOBS,"save":path,"package":package,
        "package_ms":package_ms,"cold_activation_ms":cold_activation_ms,"all_active":true,
        "initial_records":engine.profile_counts().0,"state_sha256":state_hash})
    );
    Ok(())
}

fn inspect(path: &Path, trace_executions: Option<usize>) -> Result<()> {
    let archive = tor_server::inspect_save(path)?;
    require(
        archive["scenario"]["package"]["manifest"]["id"] == "active-mobs-v1",
        "wrong workload save",
    )?;
    require(
        archive["scenario"]["seed"] == fixture::SEED
            && archive["scenario"]["regions"] == fixture::REGIONS
            && archive["scenario"]["streaming"]["active_radius"] == fixture::REGIONS
            && archive["scenario"]["streaming"]["load_radius"] == fixture::REGIONS,
        "wrong workload settings",
    )?;
    let records = archive["records"].as_array().ok_or("missing records")?;
    let mut proof = ActivityProof::default();
    let mut trace = InspectionTrace::new(trace_executions)?;
    let mut sustained = SustainedActivity::default();
    for record in records {
        let entry = serde_json::from_value::<JournalEntry>(record["entry"].clone())?;
        proof.observe(&entry);
        trace.observe(&entry)?;
        sustained.observe(&entry)?;
    }
    let trace = trace.finish()?;
    let sustained = sustained.finish()?;
    proof.require_complete()?;
    let start = Instant::now();
    let engine = Engine::open(path, Scenario::two_room(0))?;
    let resume_ms = ms(start.elapsed());
    residency(&engine)?;
    require(
        engine.profile_counts().0 == records.len(),
        "replay changed the inspected journal boundary",
    )?;
    let state_hash = hex_digest(&evidence(&engine)?);
    println!(
        "{}",
        json!({"workload":FORMAT,"version":1,"profile_version":3,"observer_work_contract":"causal_facts_demand_display_v1","mode":"inspect","seed":fixture::SEED,
        "regions":fixture::REGIONS,"ai":fixture::MOBS,"all_active":true,"all_alive":true,
        "activity":proof,"records":records.len(),"resume_ms":resume_ms,"actor_ids":engine.actors(),
        "execution_trace_count":trace.count,"execution_trace_available":trace.available,
        "execution_trace_sha256":trace.sha256,"sustained_activity":sustained,
        "recovery":engine.recovery_profile(),"state_sha256":state_hash})
    );
    Ok(())
}

fn measure(steps: usize) -> Result<()> {
    require(steps > 0, "steps must be positive")?;
    let directory = tempfile::tempdir()?;
    let start = Instant::now();
    let scenario = fixture::make(&directory.path().join("package"))?;
    let package_ms = ms(start.elapsed());
    let path = directory.path().join("game.db");
    let policy = SavePolicy::default();
    let start = Instant::now();
    let mut engine = Engine::open_with_policy(&path, scenario, policy.clone())?;
    let cold_activation_ms = ms(start.elapsed());
    // Opt-in attribution only: historical direct runs leave speculative preloading
    // off. This separately labeled mode must never silently replace that baseline.
    let diagnostic_preloading =
        std::env::var_os("TOR_PROFILE_PRELOADING").is_some_and(|value| value == "1");
    if diagnostic_preloading {
        require(
            std::env::var_os("TOR_PROFILE_SPANS").is_some_and(|value| value == "1"),
            "diagnostic preloading requires span profiling",
        )?;
        engine.start_preloading();
    }
    residency(&engine)?;
    let initial_history = engine.profile_counts().0;
    let mut samples = Samples::default();
    let mut activity = ActivityProof::default();
    samples.rss_bytes.push(rss());
    let start = Instant::now();
    let span_observer = std::env::var_os("TOR_PROFILE_SPANS")
        .filter(|value| value == "1")
        .map(|_| {
            diagnostic_spans::reset();
            tor_simulation::diagnostics::spans::observe(diagnostic_spans::event)
        });
    for step in 0..steps {
        let cycle = Instant::now();
        require(
            engine.next_actor() == Some(PLAYER),
            "player is not the due input boundary",
        )?;
        let command = Command::Act {
            expected_boundary: engine.causal_boundary(PLAYER)?,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        let before = work_counts();
        let started = Instant::now();
        let (result, profile) = engine.command_profiled(
            "active-mobs",
            FORMAT,
            PLAYER,
            &format!("move-{step}"),
            &engine.branch().clone(),
            command,
        )?;
        let duration = ms(started.elapsed());
        let after = work_counts();
        require(
            matches!(
                &result.entry.content,
                JournalContent::Action {
                    event: Event::Moved { .. },
                    ..
                }
            ),
            "player did not move",
        )?;
        activity.observe(&result.entry);
        samples.command(&profile, duration, false, before, after);
        residency(&engine)?;
        let mut decisions = 0;
        while let Some(actor) = engine.next_actor().filter(|actor| engine.is_ai(*actor)) {
            decisions += 1;
            require(
                decisions <= fixture::MOBS as usize * 10,
                "AI failed to yield to ordinary player input",
            )?;
            let before = work_counts();
            let started = Instant::now();
            let (result, profile) = engine.advance_ai_profiled(actor)?;
            let duration = ms(started.elapsed());
            let after = work_counts();
            activity.observe(&result.entry);
            samples.command(&profile, duration, true, before, after);
            residency(&engine)?;
        }
        samples.player_cycle_ms.push(ms(cycle.elapsed()));
        let started = Instant::now();
        let state = engine.state(PLAYER)?;
        samples.projection_ms.push(ms(started.elapsed()));
        let started = Instant::now();
        std::hint::black_box(serde_json::to_vec(&state)?);
        samples.serialization_ms.push(ms(started.elapsed()));
        samples
            .pending_bytes
            .push(engine.save_status().pending_bytes);
        samples.rss_bytes.push(rss());
    }
    let workload_ms = ms(start.elapsed());
    let spans_enabled = span_observer.is_some();
    drop(span_observer);
    let nested_spans = spans_enabled.then(diagnostic_spans::results);
    // Short probes expose catastrophic costs without claiming complete coverage.
    let activity_complete = activity.require_complete().is_ok();
    if steps >= 8 {
        activity.require_complete()?;
    }
    require(
        activity.player_moves == steps as u64,
        "missing player movement",
    )?;
    if steps >= 8 {
        require(
            samples.work["route_searches"] > 0 && samples.work["strike_resolutions"] > 0,
            "pursuit or combat was not exercised",
        )?;
    }
    let expected = evidence(&engine)?;
    let state_hash = hex_digest(&expected);
    let retained_history = engine.profile_counts().0;
    let started = Instant::now();
    engine.flush()?;
    let save_ms = ms(started.elapsed());
    let status = engine.save_status();
    require(
        status.pending_bytes == 0
            && status.accepted_sequence == status.durable_sequence
            && status.error.is_none(),
        "save did not reach durable boundary",
    )?;
    drop(engine);
    let started = Instant::now();
    let resumed = Engine::open_with_policy(&path, Scenario::two_room(0), policy)?;
    let resume_ms = ms(started.elapsed());
    residency(&resumed)?;
    require(
        evidence(&resumed)? == expected,
        "replay changed actor state",
    )?;
    require(
        resumed.profile_counts().0 == retained_history,
        "replay changed retained history length",
    )?;
    let mut report = json!({"workload":FORMAT,"version":1,"profile_version":3,"observer_work_contract":"causal_facts_demand_display_v1","mode":"direct","seed":fixture::SEED,
        "regions":fixture::REGIONS,"ai":fixture::MOBS,"steps":steps,"initial_history":initial_history,
        "retained_history":retained_history,"checkpoint_interval":1024,"all_active":true,"all_alive":true,
        "activity_complete":activity_complete,
        "activity":activity,"package_ms":package_ms,"cold_activation_ms":cold_activation_ms,
        "diagnostic_preloading":diagnostic_preloading,"workload_ms":workload_ms,"save_ms":save_ms,"resume_ms":resume_ms,"save_status":status,
        "saved_bytes":std::fs::metadata(path)?.len(),"recovery":resumed.recovery_profile(),
        "replay_equal":true,"state_sha256":state_hash,
        "limits":["all-active stress, not cold traversal","direct Engine command path, not transport latency",
            "player cycle includes diagnostic assertions","serialization is StateView JSON, not a transport envelope",
            "replay equality compares all actor disclosures under the strict production reader, not an independent private-state oracle"]});
    let serde_json::Value::Object(measurements) = serde_json::to_value(samples)? else {
        unreachable!()
    };
    report
        .as_object_mut()
        .expect("report object")
        .extend(measurements);
    if let Some(spans) = nested_spans {
        report["diagnostic_spans"] = json!({
            "scope": "measured main-thread workload only; inclusive values overlap; exclusive values exclude instrumented children; profiling overhead included",
            "rows": spans.into_iter().map(|(name, row)| (name, json!({"calls": row.calls, "inclusive_ms": row.inclusive_ns as f64 / 1e6, "exclusive_ms": row.exclusive_ns as f64 / 1e6}))).collect::<BTreeMap<_, _>>()
        });
    }
    println!("{report}");
    Ok(())
}

fn main() -> Result<()> {
    let options = Options::parse(std::env::args().skip(1))?;
    if options.help {
        println!("active_mob_bench [--smoke | --steps N] | --bootstrap NEW_SAVE | --inspect CLOSED_SAVE [--trace-executions N]");
        return Ok(());
    }
    match options.mode {
        Some((Mode::Bootstrap, path)) => bootstrap(&path),
        Some((Mode::Inspect, path)) => inspect(&path, options.trace_executions),
        None => measure(options.steps),
    }
}
