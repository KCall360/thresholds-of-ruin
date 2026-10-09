//! Rogue generation and ordinary streaming diagnostics. Timings are not gates.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;
use tor_protocol::ActorId;
use tor_server::journal::{Action, Command, Direction};
use tor_server::{generation_recipe, scenario_package, Engine, SavePolicy, Streaming};

fn route(defs: &BTreeMap<u64, scenario_package::RegionDef>) -> Vec<Direction> {
    let mut open = BTreeSet::new();
    for (id, def) in defs {
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        let slot = (id - 1) as i32;
        for y in 0..7 {
            for x in 0..26 {
                if !walls.contains(&[x, y, 0]) {
                    open.insert((x + slot % 3 * 26, y + slot / 3 * 7));
                }
            }
        }
    }
    let down = defs[&9].anchors["down"];
    let target = (down[0] + 52, down[1] + 14);
    let mut pending = VecDeque::from([(13, 3)]);
    let mut seen = BTreeSet::from([(13, 3)]);
    let mut back = BTreeMap::new();
    while let Some(at) = pending.pop_front() {
        if at == target {
            break;
        }
        for (next, direction) in [
            ((at.0 - 1, at.1), Direction::West),
            ((at.0 + 1, at.1), Direction::East),
            ((at.0, at.1 - 1), Direction::North),
            ((at.0, at.1 + 1), Direction::South),
        ] {
            if open.contains(&next) && seen.insert(next) {
                back.insert(next, (at, direction));
                pending.push_back(next);
            }
        }
    }
    let mut at = target;
    let mut moves = Vec::new();
    while at != (13, 3) {
        let (previous, direction) = back[&at];
        moves.push(direction);
        at = previous;
    }
    moves.reverse();
    moves.push(Direction::Down);
    moves
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples = std::env::args()
        .nth(1)
        .map(|n| n.parse())
        .transpose()?
        .unwrap_or(5_u64);
    assert!(samples > 0);
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let source = scenario_package::load(&root, 42, None, false)?
        .package
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"kind":"rogue", "version":1, "samples":samples,
        "floors":26, "records_per_floor":9, "prepared_limit":32,
        "profile":if cfg!(debug_assertions) {"debug"} else {"release"}})
    );
    for sample in 0..samples {
        let seed = 42 + sample;
        let mut first = None;
        for depth in 1..=26 {
            let identity = format!("floor-{depth}");
            let group = &source.manifest.generation_groups[&identity];
            // Read sources outside the pure-generation measurement.
            let defs = group
                .members
                .iter()
                .map(|id| source.region_def(*id).map(|d| (*d).clone()))
                .collect::<Result<Vec<_>, _>>()?;
            let start = Instant::now();
            let generated = generation_recipe::materialize(
                &identity,
                group,
                &source.manifest.generation_recipes[&group.recipe],
                defs,
                seed,
            )?;
            let elapsed = start.elapsed();
            assert_eq!(generated.len(), 9);
            println!(
                "{}",
                serde_json::json!({"kind":"generation", "sample":sample,
                "depth":depth, "records":generated.len(), "elapsed":elapsed})
            );
            if depth == 1 {
                first = Some(generated);
            }
        }
        let moves = route(&first.unwrap());
        for mode in ["demand", "prepared", "racing"] {
            for durable in [false, true] {
                let directory = tempfile::tempdir()?;
                let mut scenario = scenario_package::load(&root, seed, None, false)?;
                scenario.streaming = Some(Streaming {
                    active_radius: 0,
                    load_radius: 0,
                });
                let mut engine = Engine::memory(scenario)?;
                let initial = engine.region_counts().unwrap();
                assert!(initial.active + initial.frozen < 9 && initial.detached > 0);
                if durable {
                    engine = engine.attach_profile_save_with_policy(
                        directory.path().join("run.db"),
                        SavePolicy {
                            checkpoint_interval: 0,
                            ..SavePolicy::default()
                        },
                    )?;
                }
                if mode != "demand" {
                    engine.start_preloading();
                }
                let mut groups = 0;
                for (step, direction) in moves.iter().enumerate() {
                    if mode == "prepared" {
                        engine.settle_preloading();
                    }
                    let actor = ActorId(1);
                    let revision = engine.revision(actor)?;
                    let (_, profile) = engine.command_profiled(
                        "benchmark",
                        "headless",
                        actor,
                        &format!("{step}"),
                        &engine.branch().clone(),
                        Command::Act {
                            expected_revision: revision,
                            action: Action::Move {
                                direction: *direction,
                            },
                        },
                    )?;
                    groups += profile.region_acquisition.groups_committed;
                    let counts = engine.region_counts().unwrap();
                    assert!(counts.active + counts.frozen < 18);
                    println!(
                        "{}",
                        serde_json::json!({"kind":"acquisition", "sample":sample,
                        "mode":mode, "durable":durable, "step":step, "profile":profile,
                        "residency":counts})
                    );
                }
                assert_eq!(groups, 1);
                let start = Instant::now();
                engine.flush()?;
                println!(
                    "{}",
                    serde_json::json!({"kind":"rogue_end", "sample":sample,
                    "mode":mode, "durable":durable, "groups":groups, "steps":moves.len(),
                    "barrier":start.elapsed(), "residency":engine.region_counts()})
                );
            }
        }
    }
    Ok(())
}
