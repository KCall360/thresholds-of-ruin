use tor_world::{
    Direction, Extent, Location, Material, Passage, Position, Region, RegionId, Terrain, World,
};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

#[test]
fn chambers_are_voids_inside_finite_stone_including_floor_and_ceiling() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "test".into(),
            bounds: Extent::new(5, 3, 2).unwrap(),
        })
        .unwrap();
    for z in [-1, 0, 1, 2] {
        for y in -1..=3 {
            for x in -1..=5 {
                let interior = (0..5).contains(&x) && (0..3).contains(&y) && (0..2).contains(&z);
                assert_eq!(
                    world.terrain(at(1, x, y, z)),
                    Some(if interior {
                        Terrain::Empty
                    } else {
                        Terrain::Solid(Material::Stone)
                    })
                );
            }
        }
    }
    assert_eq!(world.terrain(at(1, -2, 1, 0)), None);
    assert_eq!(world.step(at(1, 0, 1, 0), Direction::West), None);
    assert_eq!(world.step(at(1, 1, 1, 0), Direction::Down), None);
    assert_eq!(world.step(at(1, 1, 1, 1), Direction::Up), None);
}

#[test]
fn vertical_surfaces_stop_at_material_doors_range_and_unallocated_space() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "tall".into(),
            bounds: Extent::new(3, 3, 4).unwrap(),
        })
        .unwrap();
    assert_eq!(
        world.vertical_surface(at(1, 1, 1, 0), Direction::Up, 3),
        None
    );
    assert_eq!(
        world.vertical_surface(at(1, 1, 1, 0), Direction::Up, 4),
        Some((Material::Stone, 4))
    );
    world.place_door(at(1, 1, 1, 1), 1, false).unwrap();
    assert_eq!(
        world.vertical_surface(at(1, 1, 1, 0), Direction::Up, 8),
        None
    );
    world.set_door(at(1, 1, 1, 1), true);
    world.set_wall(at(1, 1, 1, 4), false).unwrap();
    assert_eq!(
        world.vertical_surface(at(1, 1, 1, 0), Direction::Up, 8),
        None
    );
    assert_eq!(world.terrain(at(1, 1, 1, 4)), Some(Terrain::Empty));
    assert_eq!(world.terrain(at(1, 1, 1, 5)), None);
}

#[test]
fn joined_chambers_have_no_material_wall_at_the_storage_seam() {
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in [1, 2] {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: "test".into(),
                bounds: Extent::new(3, 3, 2).unwrap(),
            })
            .unwrap();
    }
    world
        .connect_area(
            Passage {
                from: at(1, 2, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
            3,
            2,
        )
        .unwrap();
    world
        .connect_area(
            Passage {
                from: at(2, 0, 0, 0),
                direction: Direction::West,
                to: at(1, 2, 0, 0),
            },
            0,
            3,
            2,
        )
        .unwrap();
    assert_eq!(
        world.step(at(1, 2, 1, 0), Direction::East),
        Some(at(2, 0, 1, 0))
    );
    let joined = world.shadow_scene(at(1, 1, 1, 0), 0, 8);
    let mut whole = World::new(vec![], vec![]).unwrap();
    whole
        .add_chamber(Region {
            id: RegionId(3),
            name: "whole".into(),
            bounds: Extent::new(6, 3, 2).unwrap(),
        })
        .unwrap();
    let shape = |cells: Vec<tor_world::SightCell>| {
        cells
            .into_iter()
            .map(|c| (c.offset, c.wall))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shape(joined),
        shape(whole.shadow_scene(at(3, 1, 1, 0), 0, 8))
    );
}

#[test]
fn narrow_doorway_corners_match_an_unsplit_volume_from_every_floor_cell() {
    for turns in 0..4 {
        check_narrow_doorway(turns);
    }
}

#[test]
fn rim_projection_does_not_widen_a_narrow_opening() {
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in [1, 2] {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: "room".into(),
                bounds: Extent::new(3, 3, 2).unwrap(),
            })
            .unwrap();
    }
    world
        .connect_area(
            Passage {
                from: at(1, 2, 1, 0),
                direction: Direction::East,
                to: at(2, 0, 1, 0),
            },
            0,
            1,
            2,
        )
        .unwrap();
    world
        .connect_area(
            Passage {
                from: at(2, 0, 1, 0),
                direction: Direction::West,
                to: at(1, 2, 1, 0),
            },
            0,
            1,
            2,
        )
        .unwrap();
    for y in [0, 2] {
        assert_eq!(world.step(at(1, 2, y, 0), Direction::East), None);
        assert_eq!(world.step(at(2, 0, y, 0), Direction::West), None);
        assert!(world.passage(at(1, 2, y, 0), Direction::East).is_none());
        assert!(world.passage(at(2, 0, y, 0), Direction::West).is_none());
    }
    assert_eq!(
        world.step(at(1, 2, 1, 0), Direction::East),
        Some(at(2, 0, 1, 0))
    );
}

fn check_narrow_doorway(turns: u8) {
    let remote = |x, y| {
        let (x, y) = match turns {
            0 => (x, y),
            1 => (2 - y, x),
            2 => (4 - x, 2 - y),
            _ => (y, 4 - x),
        };
        at(2, x, y, 0)
    };
    let mut split = World::new(vec![], vec![]).unwrap();
    let mut whole = World::new(vec![], vec![]).unwrap();
    for (world, id, width) in [(&mut split, 1, 6), (&mut whole, 1, 11)] {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: "test".into(),
                bounds: Extent::new(width, 3, 2).unwrap(),
            })
            .unwrap();
        for y in [0, 2] {
            for z in [0, 1] {
                world.set_wall(at(1, 5, y, z), true).unwrap();
            }
        }
        world.place_door(at(1, 5, 1, 0), 1, true).unwrap();
    }
    split
        .add_chamber(Region {
            id: RegionId(2),
            name: "far".into(),
            bounds: if matches!(turns, 0 | 2) {
                Extent::new(5, 3, 2)
            } else {
                Extent::new(3, 5, 2)
            }
            .unwrap(),
        })
        .unwrap();
    split
        .connect_area(
            Passage {
                from: at(1, 5, 1, 0),
                direction: Direction::East,
                to: remote(0, 1),
            },
            turns,
            1,
            2,
        )
        .unwrap();
    split
        .connect_area(
            Passage {
                from: remote(0, 1),
                direction: Direction::West.rotated(turns),
                to: at(1, 5, 1, 0),
            },
            (4 - turns) % 4,
            1,
            2,
        )
        .unwrap();
    let shape = |world: &World, origin: Location| {
        world
            .shadow_scene(
                origin,
                if origin.region == RegionId(2) {
                    turns
                } else {
                    0
                },
                8,
            )
            .into_iter()
            .map(|c| (c.offset, c.wall))
            .collect::<Vec<_>>()
    };
    // First reproduce the player's three viewpoints: west, on, and east of door.
    for x in [4, 5, 6] {
        let origin = if x < 6 {
            at(1, x, 1, 0)
        } else {
            remote(x - 6, 1)
        };
        assert_eq!(
            shape(&split, origin),
            shape(&whole, at(1, x, 1, 0)),
            "door viewpoint x={x}"
        );
    }
    for open in [true, false] {
        split.set_door(at(1, 5, 1, 0), open);
        whole.set_door(at(1, 5, 1, 0), open);
        for x in 0..11 {
            for y in 0..3 {
                if !whole.walkable(at(1, x, y, 0)) {
                    continue;
                }
                let origin = if x < 6 {
                    at(1, x, y, 0)
                } else {
                    remote(x - 6, y)
                };
                assert_eq!(
                    shape(&split, origin),
                    shape(&whole, at(1, x, y, 0)),
                    "x={x}, y={y}, open={open}"
                );
            }
        }
    }
}
