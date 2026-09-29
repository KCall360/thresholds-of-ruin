//! Region streaming on small and large maps: a chain of 20x3x1 regions with a
//! character walking through it, with streaming (default radii) and with
//! every region built and active. Reports startup, per-command timings and
//! how many regions end up loaded. Timings are diagnostic; see
//! docs/region-streaming.md#performance.
use std::path::Path;
use std::time::Instant;
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{journal::Command, scenario_package, Engine, Streaming};

fn package(root: &Path, regions: u64) {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("scenario.toml"),
        r#"format = 1
id = "streaming-profile"
version = "1.0"
ruleset = "dungeon-v17"
files = ["regions.toml"]
default_character = 1
characters = [{ "id" = 1, "anchor" = "1/start", "turn_ticks" = 100 }]
"#,
    )
    .unwrap();
    let mut text = String::new();
    for id in 1..=regions {
        let mut portals = Vec::new();
        if id < regions {
            portals.push(format!(
                r#"{{ "at" = [19, 0, 0], "direction" = "east", "to" = "{}/west", "width" = 3 }}"#,
                id + 1
            ));
        }
        if id > 1 {
            portals.push(format!(
                r#"{{ "at" = [0, 0, 0], "direction" = "west", "to" = "{}/east", "width" = 3 }}"#,
                id - 1
            ));
        }
        text.push_str(&format!(
            "[[regions]]\nid = {id}\nname = \"Hall {id}\"\nsize = [20, 3, 1]\n\
             anchors = {{ \"west\" = [0, 0, 0], \"east\" = [19, 0, 0], \"start\" = [1, 1, 0] }}\n\
             portals = [{}]\n\n",
            portals.join(", ")
        ));
    }
    std::fs::write(root.join("regions.toml"), text).unwrap();
}

fn main() {
    for regions in [16, 256] {
        let directory = tempfile::tempdir().unwrap();
        package(directory.path(), regions);
        for streaming in [Some(Streaming::default()), None] {
            let mut scenario = scenario_package::load(directory.path(), 3, None, true).unwrap();
            scenario.streaming = streaming;
            let started = Instant::now();
            let mut engine = Engine::memory(scenario).unwrap();
            let startup = started.elapsed().as_secs_f64() * 1000.0;
            let mut samples = Vec::new();
            let mut sequence = 0;
            // Walk through six regions and back, twice.
            for _ in 0..2 {
                for direction in [Direction::East, Direction::West] {
                    for _ in 0..120 {
                        let revision = engine.revision(ActorId(1)).unwrap();
                        let started = Instant::now();
                        engine
                            .command(
                                "player",
                                "profile",
                                ActorId(1),
                                &format!("step-{sequence}"),
                                &engine.branch().clone(),
                                Command::Act {
                                    expected_revision: revision,
                                    action: Action::Move { direction },
                                },
                            )
                            .unwrap();
                        samples.push(started.elapsed().as_secs_f64() * 1000.0);
                        sequence += 1;
                    }
                }
            }
            samples.sort_by(f64::total_cmp);
            let n = samples.len();
            let loaded = engine
                .region_counts()
                .map_or(regions as usize, |c| c.active + c.frozen);
            println!(
                "regions={regions:<4} streaming={:<5} startup_ms={startup:8.2} n={n} \
                 p50_ms={:.3} p95_ms={:.3} max_ms={:.3} loaded_regions={loaded}",
                streaming.is_some(),
                samples[n / 2],
                samples[n * 95 / 100],
                samples[n - 1],
            );
        }
    }
}
