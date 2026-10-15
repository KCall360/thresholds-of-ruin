//! Physics workload v1: falling/resting populations, footprint scaling, actual
//! disclosed client application/drawing, durable barriers and checkpoint resume.
//! Default runs three samples per shape; explicit --smoke runs one without
//! changing the shapes, commands or per-sample checks.
use std::{path::Path, time::Instant};
use tor_protocol::ActorId;
use tor_server::journal::Action;
use tor_server::{journal::Command, scenario_package, Engine, SavePolicy, Scenario};

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    profile: bool,
    smoke: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Self, String> {
        let mut options = Self::default();
        for arg in args {
            match arg.as_ref() {
                "--profile" if !options.profile => options.profile = true,
                "--smoke" if !options.smoke => options.smoke = true,
                arg => {
                    return Err(format!(
                        "unknown or repeated argument: {arg}; usage: physics_bench [--profile] [--smoke]"
                    ));
                }
            }
        }
        Ok(options)
    }

    fn sample_count(&self) -> usize {
        if self.smoke {
            1
        } else {
            3
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse(std::env::args().skip(1))?;
    let profiling = options.profile;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/physics");
    let template = scenario_package::load(&root, 42, None, false)?;
    for (actors, items, cells) in [(1, 1, 2), (8, 128, 2), (1, 1, 8), (8, 128, 8)] {
        for falling in [false, true] {
            for sample in 0..options.sample_count() {
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
                        creature: None,
                        anatomy: None,
                        known_identities: vec![],
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
                scenario_package::write_package(
                    directory.path(),
                    &package.manifest,
                    &regions,
                    &package.rules_source,
                )?;
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
                let mut profiles = Vec::new();
                let mut apply_ms = Vec::new();
                let mut draw_ms = Vec::new();
                let mut sequence = 0;
                for turn in 0..(actors * 8) {
                    let actor = ActorId(turn % actors + 1);
                    let revision = engine.revision(actor)?;
                    let start = Instant::now();
                    let command = Command::Act {
                        expected_revision: revision,
                        action: Action::Wait,
                    };
                    let profile = if profiling {
                        Some(
                            engine
                                .command_profiled(
                                    "physics-benchmark",
                                    "physics-v1",
                                    actor,
                                    &format!("{turn}"),
                                    &engine.branch().clone(),
                                    command,
                                )?
                                .1,
                        )
                    } else {
                        engine.command(
                            "physics-benchmark",
                            "physics-v1",
                            actor,
                            &format!("{turn}"),
                            &engine.branch().clone(),
                            command,
                        )?;
                        None
                    };
                    command_ms.push(start.elapsed().as_secs_f64() * 1000.);
                    if let Some(profile) = profile {
                        profiles.push(profile);
                    }
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
                let checkpoint_profile = if profiling {
                    let (bytes, elapsed) = engine.profile_checkpoint_encoding()?;
                    Some(
                        serde_json::json!({"bytes":bytes,"ms":elapsed.as_secs_f64()*1000.,"status":engine.save_status()}),
                    )
                } else {
                    None
                };
                drop(engine);
                let saved_bytes = std::fs::metadata(&path)?.len();
                let start = Instant::now();
                let resumed = Engine::open_with_policy(&path, Scenario::two_room(0), policy)?;
                let resume_ms = start.elapsed().as_secs_f64() * 1000.;
                assert_eq!(resumed.state(player)?, expected);
                println!(
                    "{}",
                    serde_json::json!({"workload":"physics","version":1,"actors":actors,"items":items,"cells":cells,"falling":falling,"sample":sample,"command_ms":command_ms,"profiles":profiles,"checkpoint_profile":checkpoint_profile,"client_apply_ms":apply_ms,"client_draw_ms":draw_ms,"save_ms":save_ms,"resume_ms":resume_ms,"saved_bytes":saved_bytes,"disclosed_bytes":disclosed_bytes,"physics_steps":after.physics_steps-before.physics_steps,"body_cells":after.body_cells-before.body_cells,"scenes":after.scenes-before.scenes})
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

#[cfg(test)]
mod tests {
    use super::Options;

    #[test]
    fn ordinary_and_profiled_workloads_keep_three_samples_per_shape() {
        let ordinary = Options::parse(std::iter::empty::<&str>()).unwrap();
        assert_eq!(ordinary.sample_count(), 3);
        assert!(!ordinary.profile);
        assert!(!ordinary.smoke);
        let profiled = Options::parse(["--profile"]).unwrap();
        assert_eq!(profiled.sample_count(), 3);
        assert!(profiled.profile);
        assert!(!profiled.smoke);
    }

    #[test]
    fn explicit_smoke_runs_one_sample_and_preserves_optional_profiling() {
        let smoke = Options::parse(["--smoke"]).unwrap();
        assert_eq!(smoke.sample_count(), 1);
        assert!(!smoke.profile);
        assert!(smoke.smoke);
        for args in [["--profile", "--smoke"], ["--smoke", "--profile"]] {
            let profiled = Options::parse(args).unwrap();
            assert_eq!(profiled.sample_count(), 1);
            assert!(profiled.profile);
            assert!(profiled.smoke);
        }
    }

    #[test]
    fn unknown_ambiguous_and_repeated_options_are_rejected() {
        for args in [
            vec!["--unknown"],
            vec!["--prof"],
            vec!["--samples", "1"],
            vec!["--smoke=true"],
            vec!["1"],
            vec!["--smoke", "--smoke"],
            vec!["--profile", "--profile"],
            vec!["--smoke", "--profile", "--unknown"],
        ] {
            assert!(Options::parse(&args).is_err(), "accepted {args:?}");
        }
    }
}
