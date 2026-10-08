//! Physics workload v1: falling/resting populations, footprint scaling, actual
//! disclosed client application/drawing, durable barriers and checkpoint resume.
use std::{path::Path, time::Instant};
use tor_protocol::ActorId;
use tor_server::journal::Action;
use tor_server::{journal::Command, scenario_package, Engine, SavePolicy, Scenario};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    let template = scenario_package::load(&root, 42, None, false)?;
    for (actors, items, cells) in [(1, 1, 2), (8, 128, 2), (1, 1, 8), (8, 128, 8)] {
        for falling in [false, true] {
            for sample in 0..3 {
                let directory = tempfile::tempdir()?;
                let mut package = (**template.package.as_ref().unwrap()).clone();
                let z = if falling { 6 } else { 0 };
                let body = scenario_package::BodySpec {
                    cells: if cells == 2 {
                        vec![[0, 0, 0], [0, 0, 1]]
                    } else {
                        (0..2)
                            .flat_map(|x| (0..2).flat_map(move |y| (0..2).map(move |z| [x, y, z])))
                            .collect()
                    },
                    // Humanoid eye, or the top of the 2x2x2 block.
                    eye: [0, 0, 1],
                    mass: 80,
                };
                package.manifest.characters[0].body = Some(body.clone());
                let mut regions = package.region_defs()?;
                let region = &mut regions[0];
                region.size = [32, 8, 8];
                region.anchors.insert("start".into(), [2, 2, z]);
                for id in 2..=actors {
                    region.actors.push(scenario_package::Actor {
                        anatomy: None,
                        known_identities: vec![],
                        combat: None,
                        id,
                        at: [2 + 3 * (id as i32 - 1), 2, z],
                        archetype: None,
                        turn_ticks: Some(100),
                        controller: "external".into(),
                        ai: None,
                        body: Some(body.clone()),
                        velocity: None,
                    });
                }
                let item = region.items[0].clone();
                region.items = (1..=items)
                    .map(|id| {
                        let mut item = item.clone();
                        item.id = id;
                        item.at = [(id % 30) as i32 + 1, 5, z];
                        item
                    })
                    .collect();
                scenario_package::write_package(directory.path(), &package.manifest, &regions)?;
                scenario_package::validate(directory.path())?;
                let scenario = scenario_package::load(directory.path(), 42, None, false)?;
                let path = directory.path().join("game.db");
                let policy = SavePolicy {
                    checkpoint_interval: 8,
                    ..Default::default()
                };
                let mut engine = Engine::open_with_policy(&path, scenario, policy.clone())?;
                let player = ActorId(1);
                let mut app = tor_client_ascii::App::new();
                app.role = tor_protocol::AccessRole::Player;
                app.set_state(
                    tor_client_common::ClientState::from_snapshot(tor_protocol::Snapshot {
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
                        state: engine.state(player)?.into(),
                        has_control: true,
                        history: tor_protocol::HistoryPage {
                            entries: vec![],
                            older_before: None,
                        },
                    })
                    .map_err(|e| format!("snapshot {e:?}"))?,
                );
                app.ready();
                let mut canvas = tor_client_ascii::render::Canvas::default();
                let before = tor_simulation::diagnostics::work_counts();
                let mut command_ms = Vec::new();
                let mut apply_ms = Vec::new();
                let mut draw_ms = Vec::new();
                let mut sequence = 0;
                for turn in 0..(actors * 8) {
                    let actor = ActorId(turn % actors + 1);
                    let revision = engine.revision(actor)?;
                    let start = Instant::now();
                    engine.command(
                        "physics-benchmark",
                        "physics-v1",
                        actor,
                        &format!("{turn}"),
                        &engine.branch().clone(),
                        Command::Act {
                            expected_revision: revision,
                            action: Action::Wait,
                        },
                    )?;
                    command_ms.push(start.elapsed().as_secs_f64() * 1000.);
                    let state = engine.state(player)?;
                    // Like live delivery, do not emit unchanged observer states
                    // when another actor acts at the same simulation timestamp.
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
                            state: state.into(),
                            event: None,
                        },
                    };
                    let start = Instant::now();
                    app.update(update).map_err(|e| format!("update {e:?}"))?;
                    apply_ms.push(start.elapsed().as_secs_f64() * 1000.);
                    let start = Instant::now();
                    canvas.draw(std::hint::black_box(&app));
                    std::hint::black_box(&canvas.pixels);
                    draw_ms.push(start.elapsed().as_secs_f64() * 1000.);
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
                    serde_json::json!({"workload":"physics","version":1,"actors":actors,"items":items,"cells":cells,"falling":falling,"sample":sample,"command_ms":command_ms,"client_apply_ms":apply_ms,"client_draw_ms":draw_ms,"save_ms":save_ms,"resume_ms":resume_ms,"saved_bytes":saved_bytes,"disclosed_bytes":disclosed_bytes,"physics_steps":after.physics_steps-before.physics_steps,"body_cells":after.body_cells-before.body_cells,"scenes":after.scenes-before.scenes})
                );
            }
        }
    }
    Ok(())
}

/// Context for one synthetic attachment used by this fixture/workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}
