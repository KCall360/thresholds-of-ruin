use super::*;
use std::{collections::BTreeSet, path::Path};
use tor_simulation::spatial_scope::{Membership, PassageId, PrimarySpace};
use tor_world::{Direction, Extent, Position, Region};

fn budget() -> Budget {
    Budget {
        scopes: 1,
        members: 2,
        rooms: 2,
        passages: 8,
        runs: 4096,
        expanded_refs: 32768,
        label_bytes: 64,
    }
}
fn work_budget() -> CompilerWorkBudget {
    CompilerWorkBudget {
        source_entries: 100_000,
        terrain_cells: 100_000,
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
        mapping: None,
        id: "explicit-control".into(),
        members: vec![RegionId(1)],
        rooms: BTreeMap::from([(room, room_cells)]),
        passages,
        doorways: BTreeMap::from([(at(2, 1, 0), room), (at(2, 1, 1), room)]),
        closure_faces: BTreeSet::new(),
    };
    let before = world.geometry_snapshot();
    let compiled = compile(&world, &source, &budget(), &work_budget())
        .expect("valid explicit canonical source must compile");
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

fn generated_pair_package() -> Package {
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
    package
}

#[test]
fn retained_generated_recipe_supplies_full_volume_not_population_floor_only() {
    let package = generated_pair_package();
    let generated = package.generated_group("dungeon/1", 42).unwrap();
    assert_eq!(generated.regions.len(), 2);
    assert_eq!(generated.rooms.len(), 2);
    assert!(generated
        .rooms
        .iter()
        .all(|r| r.cells.len() == 900 && r.cells.iter().all(|p| p.position.z == 0)));
    let source = generated_source(&package, "dungeon/1", 42, &budget(), &work_budget())
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
    // Two maximal adjacent rooms leave exactly two outer gap cells per z,
    // both room-owned doorways. No fabricated passage-primary cell is needed.
    let mut expected_doorways = BTreeMap::new();
    for z in 0..2 {
        let a = Location {
            region: RegionId(1),
            position: Position { x: 31, y: 16, z },
        };
        let b = Location {
            region: RegionId(2),
            position: Position { x: 0, y: 16, z },
        };
        assert!(!generated.regions[&1].walls.contains(&[31, 16, z]));
        assert!(!generated.regions[&2].walls.contains(&[0, 16, z]));
        expected_doorways.insert(a, RoomId::new(1).unwrap());
        expected_doorways.insert(b, RoomId::new(2).unwrap());
    }
    assert_eq!(source.doorways, expected_doorways);
    assert!(
        source.passages.is_empty(),
        "actual two-cell gap is entirely room-owned doorways"
    );
    assert!(source.closure_faces.is_empty());
    let compiled = package
        .canonical_generated_spatial("dungeon/1", 42, &generated)
        .unwrap();
    assert_eq!(compiled.members, source.members);
    assert_eq!(compiled.rooms.len(), 2);
    for (at, room) in &source.doorways {
        let membership = compiled
            .primary
            .iter()
            .find(|run| run.cells.contains(*at))
            .unwrap()
            .membership;
        assert_eq!(
            membership,
            Membership::new(PrimarySpace::Room(*room), Some(PassageId::new(1).unwrap())).unwrap()
        );
    }
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
    assert!([left, doorway, right].iter().all(|p| world.walkable(*p)));
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
        mapping: None,
        id: "junction".into(),
        members: vec![RegionId(1)],
        rooms: BTreeMap::from([(room, vec![doorway])]),
        passages: vec![left, right],
        doorways: BTreeMap::from([(doorway, room)]),
        closure_faces: BTreeSet::new(),
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
    let joined = compile(&world, &source, &budget(), &work_budget())
        .expect("explicit doorway joins both real passage arms");
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
    let separated = compile(&world, &source, &budget(), &work_budget())
        .expect("ordinary room is not a passage component edge");
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

fn exterior_fixture() -> (World, CanonicalSpatialSource) {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "external mouth".into(),
            bounds: Extent::new(5, 3, 2).unwrap(),
        })
        .unwrap();
    for z in 0..2 {
        for y in 0..3 {
            for x in 0..5 {
                if y != 1 || !(1..=3).contains(&x) {
                    world.set_wall(at(x, y, z), true).unwrap();
                }
            }
        }
    }
    let room = RoomId::new(1).unwrap();
    let source = CanonicalSpatialSource {
        mapping: None,
        id: "outer-mouth".into(),
        members: vec![RegionId(1)],
        rooms: BTreeMap::from([(room, vec![at(1, 1, 0), at(1, 1, 1)])]),
        passages: vec![at(3, 1, 0), at(3, 1, 1)],
        doorways: BTreeMap::from([(at(2, 1, 0), room), (at(2, 1, 1), room)]),
        closure_faces: BTreeSet::from([at(2, 1, 0), at(2, 1, 1)]),
    };
    assert!(source
        .rooms
        .values()
        .flatten()
        .chain(&source.passages)
        .chain(source.doorways.keys())
        .all(|p| world.walkable(*p)));
    assert!(source
        .doorways
        .keys()
        .all(|p| !source.rooms[&room].contains(p)));
    assert_eq!(
        world.physics_neighbor(at(1, 1, 0), Direction::East),
        Some((at(2, 1, 0), 0))
    );
    (world, source)
}

#[test]
fn external_doorway_retains_room_ownership_without_dilating_its_light_faces() {
    let (world, source) = exterior_fixture();
    let compiled =
        compile(&world, &source, &budget(), &work_budget()).expect("canonical external mouth");
    let room = RoomId::new(1).unwrap();
    let passage = PassageId::new(1).unwrap();
    for z in 0..2 {
        let run = compiled
            .primary
            .iter()
            .find(|r| r.cells.contains(at(2, 1, z)))
            .unwrap();
        assert_eq!(
            run.membership,
            Membership::new(PrimarySpace::Room(room), Some(passage)).unwrap()
        );
    }
    let actual: BTreeSet<_> = compiled.rooms[0].runs.iter().flat_map(expanded).collect();
    let mut expected = BTreeSet::new();
    for z in -1..=2 {
        expected.insert(at(1, 1, z));
    }
    for z in 0..2 {
        for (x, y) in [(0, 1), (1, 0), (1, 2), (2, 1)] {
            expected.insert(at(x, y, z));
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 12);
    for p in [
        at(2, 0, 0),
        at(2, 2, 1),
        at(2, 1, -1),
        at(2, 1, 2),
        at(3, 1, 0),
    ] {
        assert!(
            !actual.contains(&p),
            "external mouth must not create its own light shell"
        );
    }
    let mut open_source = source.clone();
    open_source.closure_faces.clear();
    let open = compile(&world, &open_source, &budget(), &work_budget()).unwrap();
    assert_eq!(
        open.primary, compiled.primary,
        "closure eligibility does not repartition mouths"
    );
    let open_faces: BTreeSet<_> = open.rooms[0].runs.iter().flat_map(expanded).collect();
    assert_eq!(
        open_faces,
        expected
            .difference(&source.closure_faces)
            .copied()
            .collect()
    );
}

#[test]
fn compiler_source_work_and_exact_output_limits_are_independent() {
    let (world, source) = exterior_fixture();
    let mut output = budget();
    output.expanded_refs = 18;
    output.runs = 14;
    let exact_work = CompilerWorkBudget {
        source_entries: 8,
        terrain_cells: 140,
    };
    let result =
        compile(&world, &source, &output, &exact_work).expect("exact output and source bounds");
    assert_eq!(
        result
            .primary
            .iter()
            .map(|r| r.cells.expanded_len().unwrap())
            .sum::<u64>(),
        6
    );
    assert_eq!(
        result
            .rooms
            .iter()
            .flat_map(|r| &r.runs)
            .map(|r| r.expanded_len().unwrap())
            .sum::<u64>(),
        12
    );
    assert_eq!(
        result.primary.len() + result.rooms.iter().map(|r| r.runs.len()).sum::<usize>(),
        14
    );
    let mut short_output = output;
    short_output.expanded_refs = 17;
    assert!(compile(&world, &source, &short_output, &exact_work).is_err());
    assert!(compile(
        &world,
        &source,
        &output,
        &CompilerWorkBudget {
            source_entries: 7,
            ..exact_work
        }
    )
    .is_err());
    assert!(compile(
        &world,
        &source,
        &output,
        &CompilerWorkBudget {
            terrain_cells: 139,
            ..exact_work
        }
    )
    .is_err());
    let mut foreign = source.clone();
    foreign.doorways.insert(
        Location {
            region: RegionId(9),
            position: Position { x: 2, y: 1, z: 0 },
        },
        RoomId::new(1).unwrap(),
    );
    assert!(compile(&world, &foreign, &budget(), &work_budget()).is_err());
    let mut solid = source.clone();
    solid.doorways.insert(at(0, 0, 0), RoomId::new(1).unwrap());
    assert!(compile(&world, &solid, &budget(), &work_budget()).is_err());
}

#[test]
fn single_exterior_doorway_is_a_valid_passage_component_without_degree_rule() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(0),
            name: "valid region zero".into(),
            bounds: Extent::new(3, 1, 1).unwrap(),
        })
        .unwrap();
    let p = |x| Location {
        region: RegionId(0),
        position: Position { x, y: 0, z: 0 },
    };
    world.set_wall(p(0), true).unwrap();
    assert!(world.walkable(p(1)) && world.walkable(p(2)));
    assert_eq!(
        world.physics_neighbor(p(2), Direction::West),
        Some((p(1), 0))
    );
    let room = RoomId::new(1).unwrap();
    let source = CanonicalSpatialSource {
        mapping: None,
        id: "singleton".into(),
        members: vec![RegionId(0)],
        rooms: BTreeMap::from([(room, vec![p(1)])]),
        passages: vec![],
        doorways: BTreeMap::from([(p(2), room)]),
        closure_faces: BTreeSet::new(),
    };
    let output = compile(&world, &source, &budget(), &work_budget())
        .expect("canonical singleton doorway, no invented degree minimum");
    assert_eq!(
        output
            .primary
            .iter()
            .find(|r| r.cells.contains(p(2)))
            .unwrap()
            .membership,
        Membership::new(PrimarySpace::Room(room), Some(PassageId::new(1).unwrap())).unwrap()
    );
    assert_eq!(output.validate(&budget()).unwrap().passages, 1);
}

fn generated_variant(secret_chance: Option<u32>, maze: bool) -> Package {
    use crate::generation_recipe::{MazeRooms, Operation, SecretEntrances, Stage};
    let baseline = generated_pair_package();
    let mut manifest = baseline.manifest.clone();
    let recipe = manifest.generation_recipes.get_mut("exploration").unwrap();
    if maze {
        recipe.version = 5;
        let lighting = recipe
            .stages
            .iter_mut()
            .find(|s| matches!(s.operation, Operation::RoomLighting { .. }))
            .unwrap();
        lighting.operation = Operation::RoomLighting {
            darkness_roll: 1,
            darkness_start: 0,
        };
        recipe.stages.insert(
            6,
            Stage {
                id: "maze_rooms".into(),
                version: 1,
                operation: Operation::MazeRooms {
                    config: MazeRooms { one_in: 1 },
                },
            },
        );
    }
    if let Some(chance) = secret_chance {
        if !maze {
            recipe.version = 3;
        }
        recipe.stages.push(Stage {
            id: "secret_entrances".into(),
            version: 1,
            operation: Operation::SecretEntrances {
                config: SecretEntrances {
                    chance_per_mille: chance,
                    maximum: 64,
                    dc: 10,
                },
            },
        });
    }
    crate::generation_recipe::check_layout(recipe, [32, 32, 2]).unwrap();
    let result = Package::from_parts(manifest, vec![], &baseline.rules_source).unwrap();
    result.check().unwrap();
    result
}

#[test]
fn generated_chance_zero_mouth_ownership_matches_real_secret_closures_without_repartition() {
    let none = generated_variant(Some(0), false);
    let all = generated_variant(Some(1000), false);
    let geometry_none = none.generated_group("dungeon/1", 42).unwrap();
    let geometry_all = all.generated_group("dungeon/1", 42).unwrap();
    assert!(geometry_none.regions.values().all(|r| r.doors.is_empty()));
    let doors: Vec<_> = geometry_all
        .regions
        .values()
        .flat_map(|r| r.doors.iter().map(move |d| (r.id, d)))
        .collect();
    // Exactly the generator's reservation domain: boundary anchors do not
    // reserve secret entrances; all other anchors remain in actual interiors.
    for r in geometry_all.regions.values() {
        let boundary = &r.generate.as_ref().unwrap().boundary_anchors;
        for (name, p) in &r.anchors {
            if !boundary.contains(name) {
                assert!((1..=30).contains(&p[0]) && (1..=30).contains(&p[1]));
                assert!(r.doors.iter().all(|d| d.at != *p));
            }
        }
    }
    assert_eq!(
        doors.len(),
        2,
        "both actual narrow outer mouths have closures"
    );
    for id in [1, 2] {
        assert_eq!(
            geometry_none.regions[&id].walls,
            geometry_all.regions[&id].walls
        );
        assert_eq!(
            geometry_none.regions[&id].anchors,
            geometry_all.regions[&id].anchors
        );
    }
    assert_eq!(geometry_none.rooms, geometry_all.rooms);
    let open = generated_source(&none, "dungeon/1", 42, &budget(), &work_budget())
        .expect("chance-zero still owns ordinary mouths");
    let closed = generated_source(&all, "dungeon/1", 42, &budget(), &work_budget()).unwrap();
    assert_eq!(open.rooms, closed.rooms);
    assert_eq!(open.passages, closed.passages);
    assert_eq!(open.doorways, closed.doorways);
    assert_eq!(open.doorways.len(), 4);
    assert!(open.closure_faces.is_empty());
    let expected: BTreeSet<_> = doors
        .into_iter()
        .flat_map(|(region, d)| {
            assert!(!d.open && d.height == 2 && d.concealment.is_some());
            (0..2).map(move |z| Location {
                region: RegionId(region),
                position: Position {
                    x: d.at[0],
                    y: d.at[1],
                    z,
                },
            })
        })
        .collect();
    assert_eq!(closed.closure_faces, expected);
    for (p, owner) in &closed.doorways {
        assert!(!closed.rooms[owner].contains(p));
    }
}

#[test]
fn generated_selected_maze_uses_final_open_cells_as_passages_not_population_room_ids() {
    let package = generated_variant(None, true);
    let generated = package.generated_group("dungeon/1", 42).unwrap();
    // Valid recipe has guaranteed darkness/trial and no trap reservation to
    // exempt rooms. Population metadata deliberately still carries room IDs.
    assert_eq!(generated.rooms.len(), 2);
    assert!(generated
        .rooms
        .iter()
        .all(|r| !r.cells.is_empty() && r.cells.len() < 900));
    assert!(generated
        .regions
        .values()
        .all(|r| r.lighting.is_empty() && !r.lit));
    let mut expected = BTreeSet::new();
    for (id, r) in &generated.regions {
        let walls: BTreeSet<_> = r.walls.iter().copied().collect();
        for z in 0..2 {
            for y in 0..32 {
                for x in 0..32 {
                    if !walls.contains(&[x, y, z]) {
                        expected.insert(Location {
                            region: RegionId(*id),
                            position: Position { x, y, z },
                        });
                    }
                }
            }
        }
    }
    assert!(expected.len() > 4 && expected.len() < 3604);
    let source = generated_source(&package, "dungeon/1", 42, &budget(), &work_budget())
        .expect("actual selected maze provenance excludes ordinary Room primary");
    assert!(source.rooms.is_empty());
    assert_eq!(
        source.mapping,
        Some(vec![]),
        "generated maze is explicitly certified, not absent"
    );
    assert!(source.doorways.is_empty());
    assert_eq!(
        source.passages.iter().copied().collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(source.passages.len(), expected.len());
    let compiled = package
        .canonical_generated_spatial("dungeon/1", 42, &generated)
        .unwrap();
    assert!(compiled.rooms.is_empty());
    let actual: BTreeSet<_> = compiled
        .primary
        .iter()
        .flat_map(|run| {
            (run.cells.first_x..=run.cells.last_x).map(move |x| Location {
                region: run.cells.region,
                position: Position {
                    x,
                    y: run.cells.y,
                    z: run.cells.z,
                },
            })
        })
        .collect();
    assert_eq!(actual, expected);
}

fn exterior_owner_fixture() -> (World, CanonicalSpatialSource) {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "exterior ownership".into(),
            bounds: Extent::new(5, 5, 1).unwrap(),
        })
        .unwrap();
    // One connected ordinary U-shaped room surrounds an exterior mouth on
    // three planar sides. Multiple adjacent CELLS are not multiple owners.
    let interior = vec![
        at(1, 1, 0),
        at(2, 1, 0),
        at(3, 1, 0),
        at(1, 2, 0),
        at(3, 2, 0),
    ];
    let mouth = at(2, 2, 0);
    for y in 0..5 {
        for x in 0..5 {
            let p = at(x, y, 0);
            if p != mouth && !interior.contains(&p) {
                world.set_wall(p, true).unwrap();
            }
        }
    }
    assert!(interior.iter().chain([&mouth]).all(|p| world.walkable(*p)));
    assert_eq!(
        world.physics_neighbor(mouth, Direction::West),
        Some((at(1, 2, 0), 0))
    );
    assert_eq!(
        world.physics_neighbor(mouth, Direction::East),
        Some((at(3, 2, 0), 0))
    );
    assert_eq!(
        world.physics_neighbor(mouth, Direction::North),
        Some((at(2, 1, 0), 0))
    );
    let room = RoomId::new(1).unwrap();
    (
        world,
        CanonicalSpatialSource {
            mapping: None,
            id: "exterior-unique-owner".into(),
            members: vec![RegionId(1)],
            rooms: BTreeMap::from([(room, interior)]),
            passages: vec![],
            doorways: BTreeMap::from([(mouth, room)]),
            closure_faces: BTreeSet::new(),
        },
    )
}

#[test]
fn exterior_doorway_multiple_adjacent_cells_of_one_room_are_valid() {
    let (world, source) = exterior_owner_fixture();
    let before = world.geometry_snapshot();
    let scope = compile(&world, &source, &budget(), &work_budget()).unwrap();
    assert_eq!(world.geometry_snapshot(), before);
    let expected = Membership::new(
        PrimarySpace::Room(RoomId::new(1).unwrap()),
        Some(PassageId::new(1).unwrap()),
    )
    .unwrap();
    assert_eq!(
        scope
            .primary
            .iter()
            .find(|run| run.cells.contains(at(2, 2, 0)))
            .unwrap()
            .membership,
        expected
    );
    assert_eq!(scope.validate(&budget()).unwrap().passages, 1);
}

#[test]
fn exterior_doorway_rejects_two_physical_room_owners_even_when_declared_owner_matches() {
    let (world, mut source) = exterior_owner_fixture();
    // Valid same-World/source baseline precedes the competing-owner mutation.
    compile(&world, &source, &budget(), &work_budget()).unwrap();
    let room1 = RoomId::new(1).unwrap();
    let room2 = RoomId::new(2).unwrap();
    source
        .rooms
        .insert(room1, vec![at(1, 1, 0), at(2, 1, 0), at(1, 2, 0)]);
    source.rooms.insert(room2, vec![at(3, 1, 0), at(3, 2, 0)]);
    let mouth = at(2, 2, 0);
    // The split-room geometry is valid when mouth is ordinary passage-primary:
    // only giving that exterior cell an arbitrary Room owner is ambiguous.
    source.doorways.clear();
    source.passages.push(mouth);
    compile(&world, &source, &budget(), &work_budget()).unwrap();
    source.passages.clear();
    for declared in [room1, room2] {
        source.doorways.insert(mouth, declared);
        let before = world.geometry_snapshot();
        let failure = compile(&world, &source, &budget(), &work_budget())
            .expect_err("an authored matching owner cannot hide a distinct physical owner");
        assert_eq!(failure.code, ErrorCode::InvalidAction);
        assert_eq!(failure.message, "ambiguous exterior doorway owner");
        assert_eq!(world.geometry_snapshot(), before);
    }
    // Existing interior-marker rule is deliberately unchanged: true interior
    // ownership, unlike an exterior claim, already identifies the primary room.
    source.rooms.get_mut(&room1).unwrap().push(mouth);
    source
        .rooms
        .get_mut(&room1)
        .unwrap()
        .sort_by_key(|p| (p.region, p.position.z, p.position.y, p.position.x));
    source.doorways.insert(mouth, room1);
    compile(&world, &source, &budget(), &work_budget()).unwrap();
}

// Narrow source envelope + trusted typed compiler witness only.
// No authored spatial metadata parser, automatic scope inference, Game registry
// install, archive fit, production cap or maximum-world claim.
#[test]
fn validated_authored_package_supports_explicit_typed_scope_above_sixty_four_members() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let resolved = super::super::resolve_source(&root).unwrap();
    let rules = std::fs::read_to_string(root.join(&resolved.manifest.ruleset)).unwrap();
    let rules = super::super::resolve_rules_source(&rules, "rules.toml")
        .unwrap()
        .source;
    let mut manifest = serde_json::to_value(resolved.manifest).unwrap();
    manifest["id"] = serde_json::json!("authored-sixty-five-members");
    assert_eq!(manifest["characters"].as_array().unwrap().len(), 1);
    manifest["characters"][0]["anchor"] = serde_json::json!("1/start");
    let definitions: Vec<super::super::RegionDef> = (1..=65u64)
        .map(|id| {
            serde_json::from_value(serde_json::json!({
                "id":id,"name":format!("Small authored room {id}"),
                "size":[3,3,2],"chamber":true,"anchors":{"start":[1,1,0]}
            }))
            .unwrap()
        })
        .collect();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        definitions,
        &rules,
    )
    .unwrap();
    package.check().unwrap(); // actual authored source validator, not count algebra
    assert_eq!(package.manifest.characters.len(), 1);
    let definitions = package.region_defs().unwrap();
    assert_eq!(definitions.len(), 65);
    // Actual ordinary source construction remains a separate prerequisite; no
    // assumption that Package::start eagerly materializes every actor/region.
    package.start(42).unwrap();

    let mut world = World::new(vec![], vec![]).unwrap();
    let mut rooms = BTreeMap::new();
    let mut expected_primary = BTreeMap::new();
    let mut expected_surfaces = BTreeMap::new();
    for definition in &definitions {
        assert!(
            definition.chamber && definition.walls.is_empty() && definition.openings.is_empty()
        );
        assert!(
            definition.portals.is_empty()
                && definition.doors.is_empty()
                && definition.traps.is_empty()
        );
        assert_eq!(definition.size, [3, 3, 2]);
        // Same checked Region descriptor as Package::add_geometry. This fixture
        // has no portals, overlays or mutable entities requiring a new adapter.
        world
            .add_chamber(
                super::super::region_of(definition.id, &definition.name, definition.size).unwrap(),
            )
            .unwrap();
        let region = RegionId(definition.id);
        let room = RoomId::new(definition.id.try_into().unwrap()).unwrap();
        let point = |x, y, z| Location {
            region,
            position: Position { x, y, z },
        };
        let mut interior = Vec::new();
        for z in 0..2 {
            for y in 0..3 {
                for x in 0..3 {
                    let p = point(x, y, z);
                    assert!(world.walkable(p));
                    interior.push(p);
                    expected_primary
                        .insert(p, Membership::new(PrimarySpace::Room(room), None).unwrap());
                }
            }
        }
        assert_eq!(interior.len(), 18);
        rooms.insert(room, interior);
        // Independent box+six-face oracle; no edge/corner dilation.
        let mut surface = BTreeSet::new();
        for z in -1..=2 {
            for y in -1..=3 {
                for x in -1..=3 {
                    let outside = usize::from(!(0..3).contains(&x))
                        + usize::from(!(0..3).contains(&y))
                        + usize::from(!(0..2).contains(&z));
                    if outside <= 1 {
                        surface.insert(point(x, y, z));
                    }
                }
            }
        }
        assert_eq!(surface.len(), 60);
        expected_surfaces.insert(room, surface);
    }
    let members: Vec<_> = (1..=65).map(RegionId).collect();
    let source = CanonicalSpatialSource {
        mapping: None,
        id: "explicit-authored-test-scope".into(),
        members: members.clone(),
        rooms,
        passages: vec![],
        doorways: BTreeMap::new(),
        closure_faces: BTreeSet::new(),
    };
    let budget = Budget {
        scopes: 1,
        members: 65,
        rooms: 65,
        passages: 0,
        runs: 1430,
        expanded_refs: 5070,
        label_bytes: 64,
    };
    let work = CompilerWorkBudget {
        source_entries: 1170,
        terrain_cells: 6500,
    };
    let before = world.geometry_snapshot();
    let scope = compile(&world, &source, &budget, &work).unwrap();
    assert_eq!(world.geometry_snapshot(), before);
    assert_eq!(scope.members, members);
    let actual_primary: BTreeMap<_, _> = scope
        .primary
        .iter()
        .flat_map(|run| {
            expanded(&run.cells)
                .into_iter()
                .map(move |p| (p, run.membership))
        })
        .collect();
    assert_eq!(actual_primary, expected_primary);
    let actual_surfaces: BTreeMap<_, BTreeSet<_>> = scope
        .rooms
        .iter()
        .map(|room| (room.room, room.runs.iter().flat_map(expanded).collect()))
        .collect();
    assert_eq!(actual_surfaces, expected_surfaces);
    let usage = scope.validate(&budget).unwrap();
    assert_eq!(usage.members, 65);
    assert_eq!(usage.rooms, 65);
    assert_eq!(usage.passages, 0);
    assert_eq!(usage.runs, 1430);
    assert_eq!(usage.expanded_refs, 5070);
    let smaller = Budget {
        members: 64,
        ..budget
    };
    assert_eq!(
        compile(&world, &source, &smaller, &work).unwrap_err().code,
        ErrorCode::ResourceLimit
    );
}
