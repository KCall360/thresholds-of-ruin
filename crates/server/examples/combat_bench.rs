//! Combat workload v1: real AI decisions, typed attacks, history, client drawing,
//! checkpoint barriers and deterministic restart. Timings exclude report output.
use std::{path::Path, time::Instant};
use tor_protocol::{Action, ActorId};
use tor_server::{journal::Command, scenario_package, Engine, SavePolicy, Scenario};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/dungeon-loop");
    let template = scenario_package::load(&root, 42, None, false)?;
    for actors in [2, 8] {
        for history in [0, 1000] {
            for sample in 0..3 {
                let directory = tempfile::tempdir()?;
                let mut package = (**template.package.as_ref().unwrap()).clone();
                package.manifest.objective = None;
                let hero = package.manifest.characters[0].combat.as_mut().unwrap();
                hero.max_hp = 1_000_000;
                hero.attack.damage.values_mut().for_each(|n| *n = 1);
                let mut regions = package.region_defs()?;
                let region = &mut regions[0];
                region.size = [16, 8, 2];
                region.items.clear();
                let mut enemy = region.actors[0].clone();
                enemy.combat.as_mut().unwrap().max_hp = 1_000_000;
                region.actors = (2..=actors)
                    .map(|id| {
                        let mut enemy = enemy.clone();
                        enemy.id = id;
                        enemy.at = [2 + (id as i32 - 2) % 4, 1 + (id as i32 - 2) / 4, 0];
                        enemy
                    })
                    .collect();
                scenario_package::write_package(directory.path(), &package.manifest, &regions)?;
                scenario_package::validate(directory.path())?;
                let scenario = scenario_package::load(directory.path(), 42, None, false)?;
                let path = directory.path().join("run.db");
                let policy = SavePolicy {
                    checkpoint_interval: 128,
                    ..Default::default()
                };
                let mut engine = Engine::open_with_policy(&path, scenario, policy.clone())?;
                let player = ActorId(1);
                let mut times = Vec::new();
                let mut apply = Vec::new();
                let mut draw = Vec::new();
                let mut ai_times = Vec::new();
                let mut app = tor_client_ascii::App::new();
                let snapshot = tor_protocol::Snapshot {
                    readiness: tor_protocol::Readiness {
                        revision: 0,
                        admission: false,
                        resume: vec![],
                        cancel: vec![],
                    },
                    context: fixture_context(),
                    intentions: Vec::new(),
                    travel: None,
                    actor: player,
                    branch: engine.branch().clone(),
                    cursor: tor_protocol::StreamCursor {
                        sequence: 0,
                        tick: 0,
                    },
                    state: engine.state(player)?,
                    has_control: true,
                    history: tor_protocol::HistoryPage {
                        entries: vec![],
                        older_before: None,
                    },
                };
                app.set_state(
                    tor_client_common::ClientState::from_snapshot(snapshot)
                        .map_err(|e| format!("{e:?}"))?,
                );
                app.ready();
                let mut canvas = tor_client_ascii::render::Canvas::default();
                let before = tor_simulation::diagnostics::work_counts();
                let mut phase_totals = std::collections::BTreeMap::<&str, f64>::new();
                let mut navigation_refreshes = 0;
                let mut sequence = 0;
                for turn in 0..history + 64 {
                    let decision_start = Instant::now();
                    let (actor, action) = if let Some(actor) =
                        engine.next_actor().filter(|id| engine.is_ai(*id))
                    {
                        (actor, None)
                    } else {
                        let observation = engine.observation(player)?;
                        let target = observation.visible_actors.iter().find(|a| {
                            a.id != player
                                && a.position.x.abs() <= 1
                                && a.position.y.abs() <= 1
                                && a.position.z == 0
                        });
                        (
                            player,
                            Some(target.map_or(Action::Wait, |a| Action::Attack { target: a.id })),
                        )
                    };
                    let decision_ms = decision_start.elapsed().as_secs_f64() * 1000.;
                    let revision = engine.revision(actor)?;
                    let start = Instant::now();
                    let (_, profile) = if let Some(action) = action {
                        engine.command_profiled(
                            "combat-benchmark",
                            "combat-v1",
                            actor,
                            &turn.to_string(),
                            &engine.branch().clone(),
                            Command::Act {
                                expected_revision: revision,
                                action,
                            },
                        )?
                    } else {
                        engine.advance_ai_profiled(actor)?
                    };
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    if turn >= history {
                        for (name, duration) in [
                            ("simulation", profile.simulation_transition),
                            ("perception", profile.perception),
                            ("navigation", profile.navigation_refresh),
                            ("revision", profile.revision_detection),
                            ("checkpoint_capture", profile.checkpoint_capture),
                        ] {
                            *phase_totals.entry(name).or_default() +=
                                duration.as_secs_f64() * 1000.;
                        }
                        navigation_refreshes += profile.navigation_refreshes;
                        times.push(elapsed);
                        ai_times.push(decision_ms);
                    }
                    let state = engine.state(player)?;
                    if state.revision == app.state.as_ref().unwrap().state().revision {
                        continue;
                    }
                    sequence += 1;
                    let update = tor_protocol::StreamUpdate {
                        context: fixture_context(),
                        actor: player,
                        branch: engine.branch().clone(),
                        cursor: tor_protocol::StreamCursor {
                            sequence,
                            tick: state.observation.tick,
                        },
                        body: tor_protocol::UpdateBody::Observation {
                            state: Box::new(state),
                            event: None,
                        },
                    };
                    let start = Instant::now();
                    app.update(update).map_err(|e| format!("{e:?}"))?;
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    if turn >= history {
                        apply.push(elapsed);
                    }
                    if turn >= history {
                        let start = Instant::now();
                        canvas.draw(std::hint::black_box(&app));
                        std::hint::black_box(&canvas.pixels);
                        draw.push(start.elapsed().as_secs_f64() * 1000.);
                    }
                }
                let after = tor_simulation::diagnostics::work_counts();
                let expected = engine.state(player)?;
                let disclosed_bytes = serde_json::to_vec(&expected)?.len();
                let start = Instant::now();
                engine.flush()?;
                let save_ms = start.elapsed().as_secs_f64() * 1000.;
                drop(engine);
                let saved_bytes = std::fs::metadata(&path)?.len();
                let start = Instant::now();
                let resumed = Engine::open_with_policy(&path, Scenario::two_room(0), policy)?;
                let resume_ms = start.elapsed().as_secs_f64() * 1000.;
                assert_eq!(resumed.state(player)?, expected);
                println!(
                    "{}",
                    serde_json::json!({"workload":"combat", "version":1, "actors":actors, "history":history, "sample":sample,
                    "phase_totals_ms":phase_totals, "navigation_refreshes":navigation_refreshes, "command_ms":times, "decision_ms":ai_times, "client_apply_ms":apply, "client_draw_ms":draw,
                    "save_ms":save_ms, "resume_ms":resume_ms, "saved_bytes":saved_bytes, "disclosed_bytes":disclosed_bytes,
                    "scenes":after.scenes-before.scenes, "body_cells":after.body_cells-before.body_cells})
                );
            }
        }
    }
    Ok(())
}

/// Context for one synthetic attachment used by this workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}
