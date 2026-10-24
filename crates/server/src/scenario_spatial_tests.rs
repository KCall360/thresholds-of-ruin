use super::*;
use std::{collections::BTreeSet, path::Path};
use tor_simulation::spatial_scope::{Membership, PassageId, PrimarySpace};
use tor_world::{Direction, Extent, Position, Region};

fn budget() -> Budget {
    Budget {
        scopes: 1,
        members: 2,
        rooms: 2,
        runs: 4096,
        expanded_refs: 32768,
        label_bytes: 64,
    }
}
fn at(x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z },
    }
}
fn expanded(run: &tor_simulation::spatial_scope::registry::CellRun) -> Vec<Location> {
    (run.first_x..=run.last_x)
        .map(|x| Location {
            region: run.region,
            position: Position {
                x,
                y: run.y,
                z: run.z,
            },
        })
        .collect()
}

#[test]
fn explicit_geometry_compiles_exact_volume_doorway_and_six_face_surfaces() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "compiler control".into(),
            bounds: Extent::new(7, 3, 2).unwrap(),
        })
        .unwrap();
    for z in 0..2 {
        for y in 0..3 {
            for x in 0..7 {
                if y != 1 || !(1..=5).contains(&x) {
                    world.set_wall(at(x, y, z), true).unwrap();
                }
            }
        }
    }
    let room = RoomId::new(1).unwrap();
    let room_cells: Vec<_> = (0..2)
        .flat_map(|z| (1..=2).map(move |x| at(x, 1, z)))
        .collect();
    let passages: Vec<_> = (0..2)
        .flat_map(|z| (3..=5).map(move |x| at(x, 1, z)))
        .collect();
    assert!(room_cells
        .iter()
        .chain(&passages)
        .all(|p| world.walkable(*p)));
    assert_eq!(
        world.physics_neighbor(at(2, 1, 0), Direction::East),
        Some((at(3, 1, 0), 0))
    );
    assert_eq!(
        world.physics_neighbor(at(3, 1, 0), Direction::Up),
        Some((at(3, 1, 1), 0))
    );
    assert!(world.is_wall(at(0, 1, 0)));
    let source = CanonicalSpatialSource {
        id: "explicit-control".into(),
        members: vec![RegionId(1)],
        rooms: BTreeMap::from([(room, room_cells)]),
        passages,
        doorways: vec![at(2, 1, 0), at(2, 1, 1)],
    };
    let before = world.geometry_snapshot();
    let compiled =
        compile(&world, &source, &budget()).expect("valid explicit canonical source must compile");
    assert_eq!(world.geometry_snapshot(), before, "compilation is pure");
    assert_eq!(compiled.id, source.id);
    assert_eq!(compiled.members, source.members);
    let mut actual = BTreeMap::new();
    for run in &compiled.primary {
        for p in expanded(&run.cells) {
            assert!(actual.insert(p, run.membership).is_none());
        }
    }
    let passage = PassageId::new(1).unwrap();
    let mut expected = BTreeMap::new();
    for z in 0..2 {
        for x in 1..=5 {
            expected.insert(
                at(x, 1, z),
                Membership::new(
                    if x <= 2 {
                        PrimarySpace::Room(room)
                    } else {
                        PrimarySpace::Passage(passage)
                    },
                    (x == 2).then_some(passage),
                )
                .unwrap(),
            );
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(compiled.rooms.len(), 1);
    assert_eq!(compiled.rooms[0].room, room);
    // Independent rectangle face oracle, excluding the open east mouth and
    // every diagonal. Both z layers and top/bottom shell faces are required.
    let mut surfaces = BTreeSet::new();
    for x in 1..=2 {
        for z in -1..=2 {
            surfaces.insert(at(x, 1, z));
        }
    }
    for z in 0..2 {
        surfaces.insert(at(0, 1, z));
        for x in 1..=2 {
            for y in [0, 2] {
                surfaces.insert(at(x, y, z));
            }
        }
    }
    let actual_surfaces: BTreeSet<_> = compiled.rooms[0].runs.iter().flat_map(expanded).collect();
    assert_eq!(actual_surfaces, surfaces);
    assert!(!actual_surfaces.contains(&at(3, 1, 0)));
    assert!(!actual_surfaces.contains(&at(0, 0, 0)));
}

#[test]
fn retained_generated_recipe_supplies_full_volume_not_population_floor_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
    let mut manifest = super::super::resolve_source(&root).unwrap().manifest;
    let family = manifest.procedural_worlds.get_mut("dungeon").unwrap();
    family.grid = [2, 1];
    family.size = [32, 32, 2];
    family.check().unwrap();
    let recipe = manifest.generation_recipes.get_mut("exploration").unwrap();
    assert!(matches!(
        recipe.stages[2].operation,
        crate::generation_recipe::Operation::Rooms { .. }
    ));
    recipe.stages[2].operation = crate::generation_recipe::Operation::Rooms {
        width: [30, 30],
        height: [30, 30],
    };
    crate::generation_recipe::check_layout(recipe, [32, 32, 2]).unwrap();
    let package = Package::from_parts(
        manifest,
        vec![],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    package.check().unwrap();
    let generated = package.generated_group("dungeon/1", 42).unwrap();
    assert_eq!(generated.regions.len(), 2);
    assert_eq!(generated.rooms.len(), 2);
    assert!(generated
        .rooms
        .iter()
        .all(|r| r.cells.len() == 900 && r.cells.iter().all(|p| p.position.z == 0)));
    let source = generated_source(&package, "dungeon/1", 42, &budget())
        .expect("validated recipe must supply canonical source");
    assert_eq!(source.members, vec![RegionId(1), RegionId(2)]);
    assert_eq!(source.rooms.len(), 2);
    for (slot, room) in source.rooms.values().enumerate() {
        let expected: BTreeSet<_> = (0..2)
            .flat_map(|z| {
                (1..=30).flat_map(move |y| {
                    (1..=30).map(move |x| Location {
                        region: RegionId(slot as u64 + 1),
                        position: Position { x, y, z },
                    })
                })
            })
            .collect();
        assert_eq!(room.iter().copied().collect::<BTreeSet<_>>(), expected);
        assert_eq!(room.len(), 1800);
    }
    assert!(!source.passages.is_empty());
    assert!(!source.doorways.is_empty());
}

#[test]
fn doorway_junction_joins_passage_arms_but_ordinary_room_interior_does_not() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "doorway junction".into(),
            bounds: Extent::new(5, 3, 1).unwrap(),
        })
        .unwrap();
    for y in 0..3 {
        for x in 0..5 {
            if y != 1 || !(1..=3).contains(&x) {
                world.set_wall(at(x, y, 0), true).unwrap();
            }
        }
    }
    let left = at(1, 1, 0);
    let doorway = at(2, 1, 0);
    let right = at(3, 1, 0);
    assert!([left, doorway, right].iter().any(|p| !world.walkable(*p)));
    assert_eq!(
        world.physics_neighbor(left, Direction::East),
        Some((doorway, 0))
    );
    assert_eq!(
        world.physics_neighbor(doorway, Direction::East),
        Some((right, 0))
    );
    let room = RoomId::new(1).unwrap();
    let mut source = CanonicalSpatialSource {
        id: "junction".into(),
        members: vec![RegionId(1)],
        rooms: BTreeMap::from([(room, vec![doorway])]),
        passages: vec![left, right],
        doorways: vec![doorway],
    };
    let membership = |scope: &tor_simulation::spatial_scope::registry::Scope, p| {
        let matching: Vec<_> = scope
            .primary
            .iter()
            .filter(|r| r.cells.contains(p))
            .collect();
        assert_eq!(matching.len(), 1);
        matching[0].membership
    };
    let joined =
        compile(&world, &source, &budget()).expect("explicit doorway joins both real passage arms");
    let p1 = PassageId::new(1).unwrap();
    assert_eq!(
        membership(&joined, left),
        Membership::new(PrimarySpace::Passage(p1), None).unwrap()
    );
    assert_eq!(membership(&joined, right), membership(&joined, left));
    assert_eq!(
        membership(&joined, doorway),
        Membership::new(PrimarySpace::Room(room), Some(p1)).unwrap()
    );
    source.doorways.clear();
    let separated =
        compile(&world, &source, &budget()).expect("ordinary room is not a passage component edge");
    assert_eq!(
        membership(&separated, left),
        Membership::new(PrimarySpace::Passage(p1), None).unwrap()
    );
    assert_eq!(
        membership(&separated, right),
        Membership::new(PrimarySpace::Passage(PassageId::new(2).unwrap()), None).unwrap()
    );
    assert_eq!(
        membership(&separated, doorway),
        Membership::new(PrimarySpace::Room(room), None).unwrap()
    );
}
