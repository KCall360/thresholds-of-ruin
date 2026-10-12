//! Rogue generation and ordinary streaming diagnostics. Timings are not gates.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;
use tor_protocol::ActorId;
use tor_server::journal::{Action, Command, Direction};
use tor_server::{generation_recipe, scenario_package, Engine, SavePolicy, Streaming};

// Chart membership and dimensions come from the descriptor, never numeric IDs.
struct FloorChart {
    offsets: BTreeMap<u64, (i32, i32)>,
    size: [i32; 3],
}

impl FloorChart {
    fn new(
        group: &generation_recipe::GenerationGroup,
        defs: &BTreeMap<u64, scenario_package::RegionDef>,
    ) -> Self {
        let [width, height, clearance] = defs[&group.members[0]].size;
        let columns = group.grid[0] as usize;
        let offsets = group
            .members
            .iter()
            .enumerate()
            .map(|(slot, id)| {
                assert_eq!(defs[id].size, [width, height, clearance]);
                (
                    *id,
                    (
                        (slot % columns) as i32 * width,
                        (slot / columns) as i32 * height,
                    ),
                )
            })
            .collect();
        Self {
            offsets,
            size: [
                width * group.grid[0] as i32,
                height * group.grid[1] as i32,
                clearance,
            ],
        }
    }

    fn anchor(
        &self,
        defs: &BTreeMap<u64, scenario_package::RegionDef>,
        endpoint: &str,
    ) -> (i32, i32) {
        let (region, name) = endpoint.split_once('/').expect("resolved numeric endpoint");
        let id = region.parse::<u64>().expect("resolved region ID");
        let [x, y, _] = defs[&id].anchors[name];
        let (dx, dy) = self.offsets[&id];
        (x + dx, y + dy)
    }
}

fn route(
    defs: &BTreeMap<u64, scenario_package::RegionDef>,
    chart: &FloorChart,
    start: &str,
    down: &str,
) -> Vec<Direction> {
    let mut open = BTreeSet::new();
    for (id, def) in defs {
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        let (dx, dy) = chart.offsets[id];
        for y in 0..def.size[1] {
            for x in 0..def.size[0] {
                if !walls.contains(&[x, y, 0]) {
                    open.insert((x + dx, y + dy));
                }
            }
        }
    }
    let target = chart.anchor(defs, down);
    let start = chart.anchor(defs, start);
    assert!(open.contains(&start) && open.contains(&target));
    let mut pending = VecDeque::from([start]);
    let mut seen = BTreeSet::from([start]);
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
    while at != start {
        let (previous, direction) = back[&at];
        moves.push(direction);
        at = previous;
    }
    moves.reverse();
    moves.push(Direction::Down);
    moves
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_and_route_follow_members_dimensions_and_actual_anchor_owners() {
        let defs = [80_u64, 20]
            .into_iter()
            .map(|id| {
                let definition = serde_json::from_value(serde_json::json!({
                    "id":id, "name":"test", "size":[4,3,2],
                    "anchors":if id == 20 {serde_json::json!({"start":[1,1,0]})}
                              else {serde_json::json!({"down":[2,1,0]})}
                }))
                .unwrap();
                (id, definition)
            })
            .collect();
        for (grid, size, start, directions) in [
            ([2, 1], [8, 3, 2], (5, 1), vec![Direction::West; 3]),
            (
                [1, 2],
                [4, 6, 2],
                (1, 4),
                vec![
                    Direction::East,
                    Direction::North,
                    Direction::North,
                    Direction::North,
                ],
            ),
        ] {
            let group = generation_recipe::GenerationGroup {
                depth: 1,
                recipe: "test".into(),
                grid,
                members: vec![80, 20],
            };
            let chart = FloorChart::new(&group, &defs);
            assert_eq!(chart.size, size);
            assert_eq!(chart.anchor(&defs, "20/start"), start);
            let mut expected = directions;
            expected.push(Direction::Down);
            assert_eq!(route(&defs, &chart, "20/start", "80/down"), expected);
        }
    }
}

fn materialized_groups(counts: tor_server::RegionCounts) -> usize {
    let generated = counts.active + counts.frozen + counts.detached;
    assert!(
        generated > 0 && generated <= 36 && generated.is_multiple_of(9),
        "{counts:?}"
    );
    generated / 9
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
    let family_id = "dungeon";
    let family = &source.manifest.procedural_worlds[family_id];
    let depths: Vec<u32> = (1..=26).collect(); // Workload sample, not the world's extent.
    assert!(matches!(
        family.domain,
        tor_server::procedural_world::Domain::Open { .. }
    ));
    assert_eq!(family.slots(), 9);
    println!(
        "{}",
        serde_json::json!({"kind":"rogue", "version":2, "workload":"rogue-descriptor-v2", "samples":samples,
        "family":family_id, "domain":"open", "sampled_depths":depths,
        "records_per_floor":family.slots(), "prepared_limit":32,
        "initial_group_limit":3, "materialized_group_limit":4,
        "profile":if cfg!(debug_assertions) {"debug"} else {"release"}})
    );
    for sample in 0..samples {
        let seed = 42 + sample;
        let mut first = None;
        for &depth in &depths {
            let identity = format!("{family_id}/{depth}");
            let group = family.group(family_id, seed, depth)?;
            // Describe only the sampled group outside the pure-generation measurement.
            let defs = group
                .members
                .iter()
                .map(|id| {
                    family
                        .describe(family_id, seed, *id)
                        .map(|d| d.expect("sampled family member"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let start = Instant::now();
            let generated = generation_recipe::materialize(
                &identity,
                &group,
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
            if sample == 0 && [1, 6, 11].contains(&depth) {
                floor_perception(&generated, &FloorChart::new(&group, &generated), depth);
            }
            if depth == 1 {
                first = Some((FloorChart::new(&group, &generated), generated));
            }
        }
        let (chart, first) = first.unwrap();
        let start = family.resolve_port(family_id, seed, "1", "start")?;
        let down = family.resolve_port(family_id, seed, "1", "down")?;
        let moves = route(&first, &chart, &start, &down);
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
                // Radius zero cannot override view/reach pins. Reach three may
                // activate a later landing; active-region links load one further
                // group, but frozen landings do not recurse. A safe non-stair
                // start needs an extra step: at most three initial groups, then
                // four over the route ending at the second floor's up anchor.
                let initial_groups = materialized_groups(initial);
                assert!((1..=3).contains(&initial_groups), "{initial:?}");
                println!(
                    "{}",
                    serde_json::json!({"kind":"rogue_initial", "sample":sample,
                    "mode":mode, "durable":durable, "groups":initial_groups, "residency":initial})
                );
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
                let mut detached = false;
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
                    assert_eq!(materialized_groups(counts), initial_groups + groups);
                    detached |= counts.detached > 0;
                    println!(
                        "{}",
                        serde_json::json!({"kind":"acquisition", "sample":sample,
                        "mode":mode, "durable":durable, "step":step, "profile":profile,
                        "residency":counts,
                        "observation_bytes":serde_json::to_vec(&engine.state(actor)?)?.len()})
                    );
                }
                let final_groups = materialized_groups(engine.region_counts().unwrap());
                assert!((2..=4).contains(&final_groups));
                assert_eq!(final_groups, initial_groups + groups);
                assert!(detached, "expedition must exercise real region detachment");
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

/// The actual generated floor's cells in one physical chart; chunk boundaries
/// introduce no geometric discontinuity. Compare fresh illumination-filtered
/// scenes at each accepted radius, with one and eight observer positions.
fn floor_perception(
    defs: &BTreeMap<u64, scenario_package::RegionDef>,
    chart: &FloorChart,
    depth: u32,
) {
    use tor_world::{Extent, Location, Position, Region, RegionId, World};
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "generated floor".into(),
            bounds: Extent::new(chart.size[0], chart.size[1], chart.size[2]).unwrap(),
        })
        .unwrap();
    world.set_region_light(RegionId(1), false).unwrap();
    let mut eyes = Vec::new();
    for (id, def) in defs {
        let (dx, dy) = chart.offsets[id];
        let at = |[x, y, z]: [i32; 3]| Location {
            region: RegionId(1),
            position: Position {
                x: x + dx,
                y: y + dy,
                z,
            },
        };
        for wall in &def.walls {
            world.set_wall(at(*wall), true).unwrap();
        }
        for light in &def.lighting {
            world.set_cell_light(at(light.at), light.lit).unwrap();
        }
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        let eye = (0..def.size[1])
            .flat_map(|y| (0..def.size[0]).map(move |x| [x, y, 1]))
            .find(|p| !walls.contains(p))
            .unwrap();
        eyes.push(at(eye));
    }
    for radius in [8u8, 12, 16] {
        for observers in [1usize, 8] {
            let mut timings = Vec::new();
            let mut cells = 0;
            for _ in 0..20 {
                let before = Instant::now();
                for eye in eyes.iter().take(observers) {
                    cells +=
                        std::hint::black_box(world.illuminated_eye_scene_uncached(*eye, 0, radius))
                            .len();
                }
                timings.push(before.elapsed().as_secs_f64() * 1000.0);
            }
            timings.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({"kind":"perception","depth":depth,"radius":radius,
            "observers":observers,"n":timings.len(),"p50_ms":timings[10],"p95_ms":timings[18],
            "max_ms":timings[19],"cells":cells})
            );
        }
    }
}
