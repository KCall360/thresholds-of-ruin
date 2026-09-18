use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};
fn at(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}
fn room(id: u64, w: i32, h: i32) -> Region {
    Region {
        id: RegionId(id),
        name: "test".into(),
        bounds: Extent::new(w, h, 1).unwrap(),
    }
}
fn sees(world: &World, from: Location, to: Location) -> bool {
    world
        .shadow_scene(from, 0, 8)
        .iter()
        .any(|c| c.location == to)
}
#[test]
fn closed_door_allows_corner_vision_but_blocks_straight_ahead() {
    let mut world = World::new(vec![room(1, 7, 7)], vec![]).unwrap();
    world.place_door(at(3, 3), 1, false).unwrap();
    assert!(sees(&world, at(2, 3), at(3, 2)));
    assert!(sees(&world, at(2, 3), at(3, 3)));
    assert!(!sees(&world, at(2, 3), at(4, 3)));
    assert!(!world
        .scene(at(2, 3), 0, 8)
        .iter()
        .any(|c| c.location == at(3, 2)));
    world.set_door(at(3, 3), true);
    assert!(sees(&world, at(2, 3), at(4, 3)));
}
#[test]
fn visibility_is_reciprocal_for_every_floor_pair_in_small_obstacle_maps() {
    // Exhaust all 3x3 blocker patterns inside a 5x5 room.
    for mask in 0..512 {
        let mut world = World::new(vec![room(1, 5, 5)], vec![]).unwrap();
        for bit in 0..9 {
            if mask & (1 << bit) != 0 {
                world.set_wall(at(1 + bit % 3, 1 + bit / 3), true).unwrap();
            }
        }
        let floors: Vec<_> = (0..5)
            .flat_map(|x| (0..5).map(move |y| at(x, y)))
            .filter(|p| world.walkable(*p))
            .collect();
        let visible: Vec<_> = floors
            .iter()
            .map(|p| world.shadow_scene(*p, 0, 8))
            .collect();
        for (i, a) in floors.iter().enumerate() {
            for (j, b) in floors.iter().enumerate() {
                assert_eq!(
                    visible[i].iter().any(|c| c.location == *b),
                    visible[j].iter().any(|c| c.location == *a),
                    "mask={mask} {a:?} {b:?}"
                );
            }
        }
    }
}
#[test]
fn room_walls_are_visible_and_diagonally_touching_blockers_allow_sight() {
    let mut world = World::new(vec![room(1, 7, 7)], vec![]).unwrap();
    for x in 0..7 {
        for y in 0..7 {
            if x == 0 || y == 0 || x == 6 || y == 6 {
                world.set_wall(at(x, y), true).unwrap();
            }
        }
    }
    let scene = world.shadow_scene(at(2, 3), 0, 8);
    assert_eq!(scene.iter().filter(|c| c.wall).count(), 24);
    world.set_wall(at(3, 2), true).unwrap();
    world.set_wall(at(2, 3), true).unwrap();
    assert!(sees(&world, at(2, 2), at(3, 3)));
}
#[test]
fn broad_rotated_join_matches_one_room_with_doors_and_walls() {
    check_rotated_join(false);
}
#[test]
fn enclosed_rotated_join_matches_one_volume_from_both_sides() {
    check_rotated_join(true);
}
fn check_rotated_join(enclosed: bool) {
    let build = |rooms: Vec<Region>| {
        let mut world = World::new(vec![], vec![]).unwrap();
        for region in rooms {
            if enclosed {
                world.add_chamber(region)
            } else {
                world.add_region(region)
            }
            .unwrap();
        }
        world
    };
    let mut whole = build(vec![room(1, 10, 5)]);
    let mut split = build(vec![room(1, 5, 5), room(2, 5, 5)]);
    split
        .connect_area(
            Passage {
                from: at(4, 0),
                direction: Direction::East,
                to: Location {
                    region: RegionId(2),
                    position: Position { x: 4, y: 0, z: 0 },
                },
            },
            1,
            5,
            1,
        )
        .unwrap();
    // Connections are directed; add the inverse for views from the east room.
    split
        .connect_area(
            Passage {
                from: Location {
                    region: RegionId(2),
                    position: Position { x: 0, y: 0, z: 0 },
                },
                direction: Direction::North,
                to: at(4, 4),
            },
            3,
            5,
            1,
        )
        .unwrap();
    let remote = |x, y| Location {
        region: RegionId(2),
        position: Position {
            x: 4 - y,
            y: x - 5,
            z: 0,
        },
    };
    for (x, y) in [(5, 1), (7, 3)] {
        whole.set_wall(at(x, y), true).unwrap();
        split.set_wall(remote(x, y), true).unwrap();
    }
    whole.place_door(at(5, 2), 1, false).unwrap();
    split.place_door(remote(5, 2), 1, false).unwrap();
    for open in [false, true] {
        whole.set_door(at(5, 2), open);
        split.set_door(remote(5, 2), open);
        for x in 0..10 {
            for y in 0..5 {
                let origin = at(x, y);
                if !whole.walkable(origin) {
                    continue;
                }
                let (other, turns) = if x < 5 {
                    (origin, 0)
                } else {
                    (remote(x, y), 1)
                };
                let projection = |w: &World, p, r| {
                    w.shadow_scene(p, r, 8)
                        .iter()
                        .map(|c| (c.offset, c.wall, w.door(c.location)))
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    projection(&whole, origin, 0),
                    projection(&split, other, turns),
                    "origin {origin:?}"
                );
            }
        }
    }
}
#[test]
fn cycles_keep_distinct_occurrences_with_bounded_work() {
    let mut world = World::new(vec![room(1, 3, 3)], vec![]).unwrap();
    world
        .connect_area(
            Passage {
                from: at(2, 0),
                direction: Direction::East,
                to: at(0, 0),
            },
            0,
            3,
            1,
        )
        .unwrap();
    let scene = world.shadow_scene(at(1, 1), 0, 255);
    assert!(scene.iter().filter(|c| c.location == at(1, 1)).count() > 3);
    assert!(scene.len() <= 545);
}

#[test]
fn rotation_does_not_change_physical_visibility_and_range_is_exact() {
    let mut world = World::new(vec![room(1, 9, 9)], vec![]).unwrap();
    world.place_door(at(5, 4), 1, false).unwrap();
    world.set_wall(at(2, 3), true).unwrap();
    for radius in 0..=8 {
        let locations = |turns| {
            world
                .shadow_scene(at(4, 4), turns, radius)
                .iter()
                .map(|c| c.location)
                .collect::<std::collections::BTreeSet<_>>()
        };
        for turns in 0..4 {
            assert_eq!(locations(0), locations(turns));
        }
        assert!(world
            .shadow_scene(at(4, 4), 0, radius)
            .iter()
            .all(|c| c.offset.x.abs() + c.offset.y.abs() <= i32::from(radius)));
    }
    assert_eq!(world.shadow_scene(at(4, 4), 0, 0).len(), 1);
    assert!(world.shadow_scene(at(5, 4), 0, 8).is_empty());
}

#[test]
fn inconsistent_corner_topology_is_not_disclosed_as_a_connection() {
    let mut world = World::new(vec![room(1, 1, 1), room(2, 2, 2), room(3, 2, 2)], vec![]).unwrap();
    world
        .connect(
            Passage {
                from: at(0, 0),
                direction: Direction::East,
                to: Location {
                    region: RegionId(2),
                    position: Position { x: 0, y: 0, z: 0 },
                },
            },
            0,
        )
        .unwrap();
    world
        .connect(
            Passage {
                from: at(0, 0),
                direction: Direction::South,
                to: Location {
                    region: RegionId(3),
                    position: Position { x: 0, y: 0, z: 0 },
                },
            },
            0,
        )
        .unwrap();
    let scene = world.shadow_scene(at(0, 0), 0, 8);
    assert!(scene
        .iter()
        .any(|c| c.offset == Position { x: 1, y: 0, z: 0 }));
    assert!(scene
        .iter()
        .any(|c| c.offset == Position { x: 0, y: 1, z: 0 }));
    assert!(!scene
        .iter()
        .any(|c| c.offset == Position { x: 1, y: 1, z: 0 }));
}
