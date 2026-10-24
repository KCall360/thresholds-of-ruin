//! Source-valid maximal generated geometry prerequisite, not a row-run budget or performance test.
use crate::scenario_package::Package;
use std::{collections::BTreeSet, path::Path, sync::Arc};

fn maximal_generated_scope_package() -> Package {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
    let source = crate::scenario_package::resolve_source(&root).unwrap();
    let mut manifest = source.manifest;
    let family = manifest.procedural_worlds.get_mut("dungeon").unwrap();
    family.grid = [8, 8];
    family.size = [32, 32, 2];
    family.check().unwrap();
    assert_eq!(family.slots(), 64);
    let recipe = manifest.generation_recipes.get_mut("exploration").unwrap();
    assert!(matches!(
        recipe.stages[2].operation,
        crate::generation_recipe::Operation::Rooms { .. }
    ));
    recipe.stages[2].operation = crate::generation_recipe::Operation::Rooms {
        width: [30, 30],
        height: [30, 30],
    };
    // Existing depth1 lighting must be deterministically lit; no omitted/maze
    // rooms or concealed entrances in this isolated maximum-room positive.
    assert!(!recipe.stages.iter().any(|s| matches!(
        s.operation,
        crate::generation_recipe::Operation::RoomOmissions { .. }
            | crate::generation_recipe::Operation::MazeRooms { .. }
    )));
    assert!(recipe.stages.iter().any(|s| matches!(
        s.operation,
        crate::generation_recipe::Operation::RoomLighting {
            darkness_start: 1,
            ..
        }
    )));
    crate::generation_recipe::check_layout(recipe, [32, 32, 2]).unwrap();
    let package = Package::from_parts(
        manifest,
        vec![],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    package.check().unwrap();
    package
}

#[test]
fn maximal_generated_scope_has_exact_room_volume_and_physical_illumination() {
    let package = maximal_generated_scope_package();
    let family = &package.manifest.procedural_worlds["dungeon"];
    let seed = 42;
    let group = family.group("dungeon", seed, 1).unwrap();
    assert_eq!(group.grid, [8, 8]);
    assert_eq!(group.members.len(), 64);
    assert!(group.members.windows(2).all(|p| p[0] < p[1]));
    let definitions = group
        .members
        .iter()
        .map(|id| family.describe("dungeon", seed, *id).unwrap().unwrap())
        .collect();
    let generated = crate::generation_recipe::materialize(
        "dungeon/1",
        &group,
        &package.manifest.generation_recipes[&group.recipe],
        definitions,
        seed,
    )
    .unwrap();
    assert_eq!(generated.regions.len(), 64);
    assert_eq!(generated.rooms.len(), 64);
    let mut room_volume = 0usize;
    let mut surface_volume = 0usize;
    let mut mouth_floor_cells = 0usize;
    let mut physical_open_floor = 0usize;
    for (slot, id) in group.members.iter().enumerate() {
        let region = &generated.regions[id];
        assert_eq!(region.size, [32, 32, 2]);
        let room = &generated.rooms[slot];
        assert_eq!(room.id, slot as u32 + 1);
        // Width/height30 with two-cell clearance forces origin[1,1]. This
        // independent rectangle is not derived from room metadata under test.
        let expected_floor: BTreeSet<_> = (1..=30)
            .flat_map(|y| {
                (1..=30).map(move |x| tor_world::Location {
                    region: tor_world::RegionId(*id),
                    position: tor_world::Position { x, y, z: 0 },
                })
            })
            .collect();
        assert_eq!(
            room.cells.iter().copied().collect::<BTreeSet<_>>(),
            expected_floor
        );
        assert_eq!(room.cells.len(), 900);
        let walls: BTreeSet<_> = region.walls.iter().copied().collect();
        for y in 0..32 {
            for x in 0..32 {
                assert_eq!(walls.contains(&[x, y, 0]), walls.contains(&[x, y, 1]));
                physical_open_floor += usize::from(!walls.contains(&[x, y, 0]));
            }
        }
        let mut expected_light = BTreeSet::new();
        for y in 1..=30 {
            for x in 1..=30 {
                for z in [-1, 0, 1, 2] {
                    expected_light.insert([x, y, z]);
                }
                assert!(!walls.contains(&[x, y, 0]) && !walls.contains(&[x, y, 1]));
                room_volume += 2;
            }
        }
        let sides: BTreeSet<_> = (1..=30)
            .flat_map(|n| [[0, n], [31, n], [n, 0], [n, 31]])
            .collect();
        let mut mouths = 0;
        for [x, y] in sides {
            if walls.contains(&[x, y, 0]) {
                for z in [0, 1] {
                    expected_light.insert([x, y, z]);
                }
            } else {
                mouths += 1;
            }
        }
        assert_eq!(expected_light.len(), 3840 - 2 * mouths);
        assert!(region.lighting.iter().all(|cell| cell.lit));
        assert_eq!(
            region
                .lighting
                .iter()
                .map(|cell| cell.at)
                .collect::<BTreeSet<_>>(),
            expected_light
        );
        assert_eq!(region.lighting.len(), expected_light.len());
        mouth_floor_cells += mouths;
        surface_volume += expected_light.len();
    }
    assert_eq!(room_volume, 115_200);
    assert_eq!(surface_volume, 245_760 - 2 * mouth_floor_cells);
    assert!(
        mouth_floor_cells > 0,
        "real corridor connections, not disconnected rectangles"
    );
    assert!(physical_open_floor >= 57_600);
    // Ordinary host construction is a separate positive from point metadata;
    // it validates real portals/start support and only uses existing authority.
    let scenario = crate::Scenario {
        seed,
        actors: vec![],
        regions: 0,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: Some(crate::regions::Streaming {
            active_radius: 0,
            load_radius: 0,
        }),
    };
    let engine = crate::Engine::memory(scenario).unwrap();
    assert!(engine.state(tor_protocol::ActorId(1)).is_ok());
}
