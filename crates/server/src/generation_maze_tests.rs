fn maze_recipe(mut recipe: Recipe, one_in: u32) -> Recipe {
    if !recipe
        .stages
        .iter()
        .any(|s| matches!(s.operation, Operation::RoomLighting { .. }))
    {
        recipe.stages.push(Stage {
            id: "lighting".into(),
            version: 1,
            operation: Operation::RoomLighting {
                darkness_roll: 1,
                darkness_start: 0,
            },
        });
    }
    let position = 6 + usize::from(matches!(
        recipe.stages[6].operation,
        Operation::RoomOmissions { .. }
    ));
    recipe.stages.insert(
        position,
        Stage {
            id: "maze_rooms".into(),
            version: 1,
            operation: Operation::MazeRooms {
                config: MazeRooms { one_in },
            },
        },
    );
    recipe.version = 5;
    check(&recipe).unwrap();
    recipe
}

#[test]
fn lit_maze_policy_preserves_exact_legacy_features_and_omissions() {
    let (mut group, _, definitions) = fixture();
    group.depth = 1; // All rooms are lit for the unchanged hazard lighting policy.
    for recipe in [hazard_recipe(), with_omissions(hazard_recipe(), [3, 3])] {
        for seed in [0, 1, 42, u64::MAX] {
            let old =
                super::materialize("floor", &group, &recipe, definitions.clone(), seed).unwrap();
            let new = super::materialize(
                "floor",
                &group,
                &maze_recipe(recipe.clone(), 1),
                definitions.clone(),
                seed,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(old.regions).unwrap(),
                serde_json::to_value(new.regions).unwrap()
            );
            assert_eq!(old.rooms, new.rooms);
        }
    }
}

#[test]
fn forced_dark_mazes_preserve_actual_stairs_connected_clearance_and_metadata() {
    for grid in [[1, 1], [2, 3], [8, 8]] {
        let (group, mut recipe, mut definitions) = rectangular_fixture(grid);
        recipe.stages[2].operation = Operation::Rooms {
            width: [24, 24],
            height: [5, 5],
        };
        for definition in &mut definitions {
            definition.generate = Some(toml::from_str(
                "generator='group'\nversion=1\ngroup='grid'\nrooms=[1,1]\nstair_anchors=['up','down']\n"
            ).unwrap());
        }
        let maze = maze_recipe(recipe.clone(), 1);
        for seed in [0, 42, u64::MAX] {
            let old =
                super::materialize("grid", &group, &recipe, definitions.clone(), seed).unwrap();
            let new = super::materialize("grid", &group, &maze, definitions.clone(), seed).unwrap();
            let mut reversed = definitions.clone();
            reversed.reverse();
            let repeated = super::materialize("grid", &group, &maze, reversed, seed).unwrap();
            assert_eq!(
                serde_json::to_value(&new.regions).unwrap(),
                serde_json::to_value(repeated.regions).unwrap()
            );
            assert_eq!(new.rooms, repeated.rooms);
            assert_eq!(new.rooms.len(), old.rooms.len());
            let mut removed = 0;
            let mut open = BTreeSet::new();
            for (slot, id) in group.members.iter().enumerate() {
                let region = &new.regions[id];
                assert_eq!(region.anchors, old.regions[id].anchors);
                assert!(region.lighting.is_empty());
                let room = &new.rooms[slot];
                assert_eq!(room.id, slot as u32 + 1);
                removed += old.rooms[slot].cells.len() - room.cells.len();
                for cell in &old.rooms[slot].cells {
                    let p = cell.position;
                    let clear = !region.walls.contains(&[p.x, p.y, 0]);
                    assert_eq!(clear, !region.walls.contains(&[p.x, p.y, 1]));
                    assert_eq!(
                        room.cells.contains(cell),
                        clear,
                        "metadata must include all open rectangle cells"
                    );
                }
                for at in region.anchors.values() {
                    assert!((0..2).all(|z| !region.walls.contains(&[at[0], at[1], z])));
                }
                for y in 0..region.size[1] {
                    for x in 0..region.size[0] {
                        if !region.walls.contains(&[x, y, 0]) {
                            assert!(!region.walls.contains(&[x, y, 1]));
                            open.insert((
                                x + (slot as u32 % grid[0]) as i32 * region.size[0],
                                y + (slot as u32 / grid[0]) as i32 * region.size[1],
                            ));
                        }
                    }
                }
            }
            validate_carving(&open, [26 * grid[0] as i32, 7 * grid[1] as i32, 2]).unwrap();
            assert!(
                removed > 0,
                "forced dark policy must carve physical interior walls"
            );
        }
    }
}

#[test]
fn maze_holdouts_preserve_full_trap_count_and_every_slot_landing_namespace() {
    let (mut group, _, definitions) = fixture();
    group.depth = 100; // Every original lighting trial is dark, not a seed search.
    for omissions in [false, true] {
        let mut old_recipe = if omissions {
            with_omissions(hazard_recipe(), [3, 3])
        } else {
            hazard_recipe()
        };
        old_recipe.stages[2].operation = Operation::Rooms {
            width: [24, 24],
            height: [5, 5],
        };
        let maze = maze_recipe(old_recipe.clone(), 1);
        for seed in [0, 1, 42, u64::MAX] {
            let old = super::materialize("floor", &group, &old_recipe, definitions.clone(), seed)
                .unwrap();
            let new =
                super::materialize("floor", &group, &maze, definitions.clone(), seed).unwrap();
            assert_eq!(
                new.rooms.iter().map(|r| r.id).collect::<Vec<_>>(),
                old.rooms.iter().map(|r| r.id).collect::<Vec<_>>()
            );
            assert_eq!(
                new.regions.values().map(|r| r.traps.len()).sum::<usize>(),
                2
            );
            let mut smaller = 0;
            for room in &new.rooms {
                let old_room = old.rooms.iter().find(|r| r.id == room.id).unwrap();
                if room.cells.len() < old_room.cells.len() {
                    smaller += 1;
                }
                for trap in &new.regions[&group.members[room.id as usize - 1]].traps {
                    assert_eq!(
                        room.cells, old_room.cells,
                        "ground traps belong only to ordinary rooms"
                    );
                    assert!(room.cells.iter().any(|cell| [
                        cell.position.x,
                        cell.position.y,
                        cell.position.z
                    ] == trap.at));
                }
            }
            assert!(smaller > 0);
            for region in new.regions.values() {
                let landings: Vec<_> = (0..2)
                    .map(|ordinal| region.anchors[&format!("trapdoor_landing_{ordinal}")])
                    .collect();
                assert_ne!(landings[0], landings[1]);
                for at in landings {
                    assert_eq!(at[2], 1);
                    assert!((0..2).all(|z| !region.walls.contains(&[at[0], at[1], z])));
                    assert!(!region.doors.iter().any(|door| door.at[..2] == at[..2]));
                    assert!(!region.traps.iter().any(|trap| trap.at[..2] == at[..2]));
                }
            }
        }
    }
}

#[test]
fn maze_policy_rejects_insufficient_full_maximum_without_truncation() {
    let (group, _, mut definitions) = rectangular_fixture([1, 1]);
    definitions[0].size = [5, 5, 2];
    definitions[0].anchors.clear();
    let mut recipe = hazard_recipe();
    recipe.stages[2].operation = Operation::Rooms {
        width: [3, 3],
        height: [3, 3],
    };
    let trap = recipe
        .stages
        .iter_mut()
        .find(|s| matches!(s.operation, Operation::HiddenTraps { .. }))
        .unwrap();
    let Operation::HiddenTraps { config } = &mut trap.operation else {
        unreachable!()
    };
    config.hazards = None;
    config.count = [1, 10];
    // Existing policy allows its old capacity clamp. New policy must reserve
    // the full authored maximum, and reject rather than inherit that clamp.
    super::materialize("grid", &group, &recipe, definitions.clone(), 42).unwrap();
    let error = super::materialize("grid", &group, &maze_recipe(recipe, 1), definitions, 42)
        .err()
        .unwrap();
    assert!(
        error.message.contains("authored trap maximum"),
        "{}",
        error.message
    );
}

#[test]
fn dark_negative_maze_trials_preserve_exact_legacy_output() {
    let (mut group, _, definitions) = fixture();
    group.depth = 100;
    let recipe = hazard_recipe();
    let old = super::materialize("floor", &group, &recipe, definitions.clone(), 42).unwrap();
    let new = super::materialize(
        "floor",
        &group,
        &maze_recipe(recipe, 10_000),
        definitions,
        42,
    )
    .unwrap();
    // Fixed negative witness, to be established by the authorized first run;
    // never search or retry a seed inside this test.
    assert_eq!(
        serde_json::to_value(old.regions).unwrap(),
        serde_json::to_value(new.regions).unwrap()
    );
    assert_eq!(old.rooms, new.rooms);
}

#[test]
fn maze_holdouts_keep_both_authored_trap_range_endpoints() {
    let (mut group, _, definitions) = fixture();
    group.depth = 100;
    let mut recipe = hazard_recipe();
    for stage in &mut recipe.stages {
        if let Operation::HiddenTraps { config } = &mut stage.operation {
            config.count = [1, 2];
        }
    }
    let recipe = maze_recipe(recipe, 1);
    let mut counts = BTreeSet::new();
    for seed in [0, 1, 2, 3, 4, 5, 6, 7] {
        let generated =
            super::materialize("floor", &group, &recipe, definitions.clone(), seed).unwrap();
        let count = generated
            .regions
            .values()
            .map(|region| region.traps.len())
            .sum::<usize>();
        assert!((1..=2).contains(&count));
        counts.insert(count);
    }
    assert_eq!(counts, BTreeSet::from([1, 2]));
}

#[test]
fn maze_minimum_and_maximum_rectangles_preserve_every_mandatory_point() {
    for side in [3, 30] {
        let (group, mut recipe, mut definitions) = rectangular_fixture([1, 1]);
        definitions[0].size = [side + 2, side + 2, 2];
        definitions[0].anchors.insert("start".into(), [1, 1, 0]);
        definitions[0].generate = Some(toml::from_str(
            "generator='group'\nversion=1\ngroup='grid'\nrooms=[1,1]\nstair_anchors=['up','down']\n"
        ).unwrap());
        recipe.stages[2].operation = Operation::Rooms {
            width: [side as u32; 2],
            height: [side as u32; 2],
        };
        let old = super::materialize("grid", &group, &recipe, definitions.clone(), 42).unwrap();
        let new =
            super::materialize("grid", &group, &maze_recipe(recipe, 1), definitions, 42).unwrap();
        assert_eq!(new.regions[&1].anchors, old.regions[&1].anchors);
        let room = &new.rooms[0];
        let open: BTreeSet<_> = room
            .cells
            .iter()
            .map(|cell| (cell.position.x, cell.position.y))
            .collect();
        validate_carving(&open, [side + 2, side + 2, 2]).unwrap();
        for at in new.regions[&1].anchors.values() {
            assert!(open.contains(&(at[0], at[1])));
            assert!((0..2).all(|z| !new.regions[&1].walls.contains(&[at[0], at[1], z])));
        }
    }
}

#[test]
fn mixed_maze_family_preserves_reciprocal_ports_and_future_landing_owners() {
    use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
    let family = WorldFamily {
        kind: FamilyKind::GridChain,
        version: 1,
        base_region: 1,
        domain: Domain::Finite { count: 2 },
        grid: [3, 3],
        size: [26, 7, 2],
        recipe: "hazards".into(),
        zone: None,
        gravity: Some([0, 0, -1]),
    };
    let mut baseline = with_omissions(hazard_recipe(), [3, 3]);
    for stage in &mut baseline.stages {
        if let Operation::RoomLighting {
            darkness_roll,
            darkness_start,
        } = &mut stage.operation
        {
            *darkness_roll = 1;
            *darkness_start = 0;
        }
    }
    let maze = maze_recipe(baseline.clone(), 1);
    let mut trapdoors = 0;
    for seed in 0..32 {
        let build = |depth, recipe: &Recipe| {
            let group = family.group("grid", seed, depth).unwrap();
            let definitions = group
                .members
                .iter()
                .map(|id| family.describe("grid", seed, *id).unwrap().unwrap())
                .collect();
            super::materialize(&format!("grid/{depth}"), &group, recipe, definitions, seed).unwrap()
        };
        let lower = build(2, &maze); // Materialize the destination first.
        let upper = build(1, &maze);
        for (depth, generated) in [(1, &upper), (2, &lower)] {
            let old = build(depth, &baseline);
            assert_eq!(generated.rooms.len(), 6);
            for (id, region) in &generated.regions {
                let described = family.describe("grid", seed, *id).unwrap().unwrap();
                assert_eq!(
                    serde_json::to_value(&region.portals).unwrap(),
                    serde_json::to_value(&described.portals).unwrap()
                );
                for (name, at) in &old.regions[id].anchors {
                    if !name.starts_with("trapdoor_landing_") {
                        assert_eq!(region.anchors[name], *at);
                    }
                }
                for ordinal in 0..2 {
                    let at = region.anchors[&format!("trapdoor_landing_{ordinal}")];
                    assert!((0..2).all(|z| !region.walls.contains(&[at[0], at[1], z])));
                }
            }
        }
        for region in upper.regions.values() {
            for trap in &region.traps {
                if let crate::scenario_package::TrapKind::TrapDoor { destination, .. } = &trap.kind
                {
                    trapdoors += 1;
                    assert_eq!(destination.region.0, region.id + family.slots());
                    let target = &lower.regions[&destination.region.0];
                    let at = target.anchors[&destination.name];
                    assert!((0..2).all(|z| !target.walls.contains(&[at[0], at[1], z])));
                }
            }
        }
        assert!(lower
            .regions
            .values()
            .all(|region| region.trapdoor_destination.is_none()));
    }
    assert!(trapdoors > 0, "exercise a real generated source trapdoor");
}
