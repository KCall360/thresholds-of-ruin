use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

fn cell(r: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(r),
        position: Position { x, y, z },
    }
}

fn room(id: u64) -> Region {
    Region {
        id: RegionId(id),
        name: format!("Room {id}"),
        bounds: Extent::new(5, 5, 2).unwrap(),
    }
}

#[test]
fn rays_follow_rotated_portals_and_elevation_offsets_without_revealing_whole_rooms() {
    let mut world = World::new(vec![room(1), room(2)], vec![]).unwrap();
    world
        .connect(
            Passage {
                from: cell(1, 4, 2, 0),
                direction: Direction::East,
                to: cell(2, 2, 0, 1),
            },
            1,
        )
        .unwrap();
    assert_eq!(
        world.step(cell(1, 4, 2, 0), Direction::East),
        Some(cell(2, 2, 0, 1))
    );
    let seen = world.visible_cells(cell(1, 3, 2, 0), 4);
    assert!(seen.contains(&cell(2, 2, 2, 1)));
    assert!(!seen.contains(&cell(2, 4, 4, 1)));
    assert!(!seen.contains(&cell(2, 2, 2, 0)));
    assert!(!seen.contains(&cell(2, 2, 3, 1)));
    assert_eq!(world.visible_cells(cell(1, 3, 2, 0), 4), seen);
}

#[test]
fn walls_occlude_and_diagonal_rays_cannot_cut_blocked_corners() {
    let mut world = World::new(vec![room(1)], vec![]).unwrap();
    world.set_wall(cell(1, 2, 1, 0), true).unwrap();
    world.set_wall(cell(1, 1, 2, 0), true).unwrap();
    let seen = world.visible_cells(cell(1, 1, 1, 0), 4);
    assert!(seen.contains(&cell(1, 2, 1, 0)));
    assert!(!seen.contains(&cell(1, 2, 2, 0)));
    assert!(!seen.contains(&cell(1, 3, 1, 0)));
    assert_eq!(world.step(cell(1, 1, 1, 0), Direction::East), None);
    world.set_wall(cell(1, 2, 1, 0), false).unwrap();
    assert!(world
        .visible_cells(cell(1, 1, 1, 0), 4)
        .contains(&cell(1, 3, 1, 0)));
}

#[test]
fn cycles_are_bounded_and_invalid_setup_is_atomic() {
    let mut world = World::new(vec![room(1)], vec![]).unwrap();
    world
        .connect(
            Passage {
                from: cell(1, 4, 2, 0),
                direction: Direction::East,
                to: cell(1, 4, 2, 0),
            },
            0,
        )
        .unwrap();
    assert!(world.visible_cells(cell(1, 4, 2, 0), 4).len() <= 41);
    let before = world.clone();
    assert!(world
        .connect(
            Passage {
                from: cell(1, 4, 2, 0),
                direction: Direction::East,
                to: cell(1, 0, 2, 0)
            },
            0
        )
        .is_err());
    assert!(world
        .connect(
            Passage {
                from: cell(1, 0, 2, 0),
                direction: Direction::West,
                to: cell(1, 4, 2, 0)
            },
            4
        )
        .is_err());
    assert!(world.add_region(room(1)).is_err());
    assert_eq!(world, before);
}

#[test]
fn explicit_vertical_connections_can_start_inside_a_room() {
    let mut world = World::new(vec![room(1), room(2)], vec![]).unwrap();
    world
        .connect(
            Passage {
                from: cell(1, 2, 2, 0),
                direction: Direction::Up,
                to: cell(2, 2, 2, 1),
            },
            0,
        )
        .unwrap();
    assert_eq!(
        world.step(cell(1, 2, 2, 0), Direction::Up),
        Some(cell(2, 2, 2, 1))
    );
    assert!(world
        .visible_cells(cell(1, 2, 2, 0), 4)
        .contains(&cell(2, 2, 2, 1)));
}

#[test]
fn all_quarter_turns_and_reverse_links_preserve_sight_directions() {
    for (turns, landing, beyond, reverse) in [
        (0, cell(2, 0, 2, 0), cell(2, 1, 2, 0), Direction::West),
        (1, cell(2, 2, 0, 0), cell(2, 2, 1, 0), Direction::North),
        (2, cell(2, 4, 2, 0), cell(2, 3, 2, 0), Direction::East),
        (3, cell(2, 2, 4, 0), cell(2, 2, 3, 0), Direction::South),
    ] {
        let mut world = World::new(vec![room(1), room(2)], vec![]).unwrap();
        world
            .connect(
                Passage {
                    from: cell(1, 4, 2, 0),
                    direction: Direction::East,
                    to: landing,
                },
                turns,
            )
            .unwrap();
        world
            .connect(
                Passage {
                    from: landing,
                    direction: reverse,
                    to: cell(1, 4, 2, 0),
                },
                (4 - turns) % 4,
            )
            .unwrap();
        assert!(world.visible_cells(cell(1, 3, 2, 0), 3).contains(&beyond));
        assert!(world.visible_cells(landing, 2).contains(&cell(1, 3, 2, 0)));
    }
}
