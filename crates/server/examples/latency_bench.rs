//! Performance diagnostics; JSONL raw samples and distributions, no latency gates.
use serde_json::json;
use std::{collections::BTreeMap, hint::black_box, time::Instant};
use tor_client_ascii::{render::Canvas, App};
use tor_client_common::ClientState;
use tor_protocol::{ActorId, HistoryPage, Snapshot, StreamCursor, StreamUpdate, UpdateBody};
use tor_server::{journal::Command, CommandProfile, Engine, Scenario};
use tor_test_support::performance::{Step, Trace, TraceAction};
type Distributions = BTreeMap<(String, String), Vec<f64>>;
fn phases(p: &CommandProfile) -> BTreeMap<String, f64> {
    [
        ("candidate_capture", p.rollback_capture),
        ("simulation", p.simulation_transition),
        ("navigation", p.navigation_refresh),
        ("perception", p.perception),
        ("revision_comparison", p.revision_detection),
        ("rewind_snapshot", p.rollback_snapshot),
        ("serialization", p.journal_serialization),
        ("write_flush", p.journal_write),
        ("sync", p.journal_sync),
        ("replacement", p.journal_replace),
        ("publication", p.publication),
        ("authoritative_total", p.authoritative_total),
        (
            "unattributed",
            p.authoritative_total
                .checked_sub(p.exclusive_duration())
                .expect("exclusive phases"),
        ),
    ]
    .into_iter()
    .map(|(name, d)| (name.into(), d.as_secs_f64() * 1000.))
    .collect()
}
fn summarize(case: &str, distributions: Distributions) {
    for ((label, phase), mut values) in distributions {
        values.sort_by(f64::total_cmp);
        let percentile = |p: usize| values[(values.len() * p).div_ceil(100).saturating_sub(1)];
        println!(
            "{}",
            json!({"kind":"summary","case":case,"label":label,"phase":phase,
            "n":values.len(),"mean_ms":values.iter().sum::<f64>()/values.len() as f64,
            "p50_ms":percentile(50),"p95_ms":percentile(95),"max_ms":values.last()})
        );
    }
}
struct Runner {
    engine: Engine,
    app: App,
    canvas: Canvas,
    secondary: usize,
    sequence: u64,
    attempt: usize,
    case: String,
    distributions: Distributions,
}
impl Runner {
    fn new(engine: Engine, case: String) -> Self {
        let state = engine.state(ActorId(1)).unwrap();
        let snapshot = Snapshot {
            actor: ActorId(1),
            branch: engine.branch().clone(),
            cursor: StreamCursor {
                sequence: 0,
                tick: state.observation.tick,
            },
            state,
            has_control: false,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
            travel: None,
        };
        let mut app = App::new();
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        Self {
            engine,
            app,
            canvas: Canvas::default(),
            secondary: 0,
            sequence: 0,
            attempt: 0,
            case,
            distributions: BTreeMap::new(),
        }
    }
    fn perform(&mut self, actor: ActorId, step: &Step, cycle: usize, index: usize) {
        let before = self.engine.state(actor).unwrap();
        let action = step.resolve(&before);
        let request = format!("sample-{}", self.attempt);
        self.attempt += 1;
        let command = Command::Act {
            expected_revision: before.revision,
            action: action.clone(),
        };
        let branch = self.engine.branch().clone();
        let history_start = self.engine.profile_counts().0;
        let start = Instant::now();
        let result = self
            .engine
            .command_profiled("bench", "headless", actor, &request, &branch, command);
        let command_call_ms = start.elapsed().as_secs_f64() * 1000.;
        let after = self.engine.state(actor).unwrap();
        step.verify(&before, &after, result.is_ok());
        let mut timings = BTreeMap::new();
        let mut profile = None;
        let mut event = None;
        if let Ok((result, p)) = result {
            timings = phases(&p);
            profile = Some(p);
            event = Some(result.entry);
        }
        // Construct outside timing; apply sequentially to the same growing client.
        if let Some(entry) = event.as_ref().filter(|_| {
            self.engine.revision(ActorId(1)).unwrap()
                > self.app.state.as_ref().unwrap().state().revision
        }) {
            let state = self.engine.state(ActorId(1)).unwrap();
            self.sequence += 1;
            let update = StreamUpdate {
                actor: ActorId(1),
                branch: self.engine.branch().clone(),
                cursor: StreamCursor {
                    sequence: self.sequence,
                    tick: state.observation.tick,
                },
                body: UpdateBody::Observation {
                    state: Box::new(state),
                    event: (actor == ActorId(1)).then(|| Box::new(entry.disclosed())),
                },
            };
            let start = Instant::now();
            self.app.state.as_mut().unwrap().apply(update).unwrap();
            timings.insert(
                "client_application".into(),
                start.elapsed().as_secs_f64() * 1000.,
            );
            let start = Instant::now();
            self.canvas.draw(black_box(&self.app));
            black_box(&self.canvas.pixels);
            timings.insert("rendering".into(), start.elapsed().as_secs_f64() * 1000.);
        }
        timings.insert("command_call".into(), command_call_ms);
        for (phase, value) in &timings {
            self.distributions
                .entry((step.label.clone(), phase.clone()))
                .or_default()
                .push(*value);
            if step.expected != "blocked" {
                self.distributions
                    .entry(("mixed".into(), phase.clone()))
                    .or_default()
                    .push(*value);
            }
        }
        println!(
            "{}",
            json!({"kind":"sample","case":self.case,"cycle":cycle,"step":index,"attempt":self.attempt,
            "actor":actor,"label":step.label,"action":action,"expected":step.expected,
            "history_start":history_start,"history_end":self.engine.profile_counts().0,"rewind_count":self.engine.profile_counts().1,
            "client_memory":self.app.state.as_ref().unwrap().memory().count(),"phases_ms":timings,
            "save_status":self.engine.save_status(),"profile":profile,"event":event.map(|e|e.content)})
        );
    }
    fn scheduled(&mut self, trace: &Trace, step: &Step, cycle: usize, index: usize) {
        while !self.engine.state(ActorId(1)).unwrap().observation.ready {
            let actor = self
                .engine
                .actors()
                .into_iter()
                .find(|&a| self.engine.state(a).unwrap().observation.ready)
                .unwrap();
            let wait = Step {
                label: "wait_same_region".into(),
                action: TraceAction::Wait,
                expected: "waited".into(),
                min_regions: 1,
            };
            let other = if actor == ActorId(2) {
                let other = &trace.secondary[self.secondary % trace.secondary.len()];
                self.secondary += 1;
                other
            } else {
                &wait
            };
            self.perform(actor, other, cycle, index);
        }
        self.perform(ActorId(1), step, cycle, index);
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let quick = args.iter().any(|s| s == "--quick");
    let cycles = args
        .windows(2)
        .find(|a| a[0] == "--cycles")
        .map(|a| a[1].parse::<usize>().unwrap())
        .unwrap_or(5);
    assert!(cycles > 0);
    let focused = args.iter().any(|arg| arg == "--phase-b");
    let save_ms = |flag: &str, fallback| {
        args.windows(2)
            .find(|a| a[0] == flag)
            .map(|a| a[1].parse::<u64>().unwrap())
            .unwrap_or(fallback)
    };
    let save_policy = tor_server::SavePolicy {
        target_interval: std::time::Duration::from_millis(save_ms("--save-target-ms", 30000)),
        max_unsaved_age: std::time::Duration::from_millis(save_ms("--save-max-ms", 60000)),
        idle_interval: std::time::Duration::from_millis(save_ms("--save-idle-ms", 750)),
        ..tor_server::SavePolicy::default()
    };
    let trace = Trace::load();
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let dirty = std::process::Command::new("git")
        .args(["diff", "--quiet"])
        .status()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let regions: &[u64] = if focused {
        &[8, 256]
    } else if quick {
        &[1, 8]
    } else {
        &[1, 8, 64, 256]
    };
    let histories: &[usize] = if focused {
        &[100, 10000]
    } else if quick {
        &[0, 100]
    } else {
        &[0, 100, 1000, 10000]
    };
    for &regions in regions {
        for actors in [1, 8] {
            for &history in histories {
                for durable in [false, true] {
                    if focused && regions == 256 && (actors != 8 || history != 10000 || !durable) {
                        continue;
                    }
                    let case = format!(
                        "r{regions}-a{actors}-h{history}-{}",
                        if durable { "durable" } else { "memory" }
                    );
                    eprintln!("Starting {case}");
                    let scenario = Scenario::performance(trace.seed, regions, actors).unwrap();
                    let mut engine = Engine::memory(scenario.clone()).unwrap();
                    engine.seed_profile_history(history).unwrap();
                    let path = directory.path().join(format!("{case}.json"));
                    if durable {
                        engine = engine
                            .attach_profile_save_with_policy(&path, save_policy.clone())
                            .unwrap();
                    }
                    println!(
                        "{}",
                        json!({"kind":"case","case":case,"regions":regions,"actors":actors,"history_start":history,
            "seed":trace.seed,"trace_version":trace.version,"commit":String::from_utf8_lossy(&commit.stdout).trim(),
            "dirty":!dirty.success(),"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,
            "build_profile":if cfg!(debug_assertions){"debug"}else{"release"},"cycles":cycles,"warmup":0,
            "storage":if durable{"background_sqlite_journal"}else{"memory"},"client_observer":1,
            "save_policy":{"target_ms":save_policy.target_interval.as_millis(),"max_ms":save_policy.max_unsaved_age.as_millis(),"idle_ms":save_policy.idle_interval.as_millis(),"queue_bytes":save_policy.max_pending_bytes},
            "not_applicable":[if regions==1{Some("boundary")}else{None},if actors==1{Some("multi_actor")}else{None}]})
                    );
                    let mut runner = Runner::new(engine, case.clone());
                    for cycle in 0..cycles {
                        for (index, step) in trace.steps(regions).enumerate() {
                            runner.scheduled(&trace, step, cycle, index);
                        }
                    }
                    let (history_end, rewind_count) = runner.engine.profile_counts();
                    if !durable {
                        runner.engine.profile_persistence(&path).unwrap();
                    }
                    let state = runner.engine.state(ActorId(1)).unwrap();
                    let flush_started = Instant::now();
                    runner.engine.flush().unwrap();
                    let flush_ms = flush_started.elapsed().as_secs_f64() * 1000.;
                    let save_status = runner.engine.save_status();
                    let final_save_bytes = std::fs::metadata(&path).unwrap().len();
                    drop(runner.engine);
                    let start = Instant::now();
                    let resumed = Engine::open(&path, scenario).unwrap();
                    let restart_replay_ms = start.elapsed().as_secs_f64() * 1000.;
                    assert_eq!(resumed.state(ActorId(1)).unwrap(), state);
                    println!(
                        "{}",
                        json!({"kind":"case_end","case":case,"history_end":history_end,"rewind_count":rewind_count,
            "final_save_bytes":final_save_bytes,"final_flush_ms":flush_ms,"save_status":save_status,"restart_replay_ms":restart_replay_ms})
                    );
                    summarize(&case, runner.distributions);
                }
            }
        }
    }
    // Separate growing-discovery trace; local cycles cannot establish its scaling.
    if focused {
        return;
    }
    for regions in [8, 64, 256] {
        let case = format!("traversal-r{regions}");
        let engine =
            Engine::memory(Scenario::performance(trace.seed, regions, 1).unwrap()).unwrap();
        let mut runner = Runner::new(engine, case.clone());
        for cycle in 0..if quick { 2 } else { regions - 1 } {
            for (index, step) in trace.traversal.iter().enumerate() {
                runner.scheduled(&trace, step, cycle as usize, index);
            }
        }
        println!(
            "{}",
            json!({"kind":"traversal_end","case":case,"history_end":runner.engine.profile_counts().0,
            "client_memory":runner.app.state.as_ref().unwrap().memory().count()})
        );
        summarize(&case, runner.distributions);
    }
}
