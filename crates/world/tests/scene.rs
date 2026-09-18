use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}
fn room(id: u64, width: i32, depth: i32, height: i32) -> Region {
    Region {
        id: RegionId(id),
        name: "Internal partition".into(),
        bounds: Extent::new(width, depth, height).unwrap(),
    }
}

#[test]
fn dividing_a_space_into_regions_does_not_change_the_visible_scene() {
    let whole = World::new(vec![room(1, 10, 5, 2)], vec![]).unwrap();
    let mut split = World::new(vec![room(1, 5, 5, 2), room(2, 5, 5, 2)], vec![]).unwrap();
    split
        .connect_area(
            Passage {
                from: at(1, 4, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
            5,
            2,
        )
        .unwrap();
    let expected = whole.scene(at(1, 3, 2, 0), 0, 8);
    let actual = split.scene(at(1, 3, 2, 0), 0, 8);
    assert_eq!(
        actual
            .iter()
            .map(|c| (c.offset, c.wall))
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|c| (c.offset, c.wall))
            .collect::<Vec<_>>()
    );
    assert!(actual
        .iter()
        .any(|c| c.location.region == RegionId(2) && c.offset.x > 0));
    for y in 0..5 {
        assert_eq!(
            split.step(at(1, 4, y, 1), Direction::East),
            Some(at(2, 0, y, 1))
        );
    }
}

#[test]
fn wide_rotated_joins_project_remote_cells_and_validate_the_entire_area_atomically() {
    let mut world = World::new(vec![room(1, 5, 5, 2), room(2, 5, 5, 2)], vec![]).unwrap();
    let join = Passage {
        from: at(1, 4, 1, 0),
        direction: Direction::East,
        to: at(2, 3, 0, 1),
    };
    let before = world.clone();
    assert!(world.connect_area(join, 1, 5, 1).is_err());
    assert_eq!(world, before);
    world.connect_area(join, 1, 3, 1).unwrap();
    assert_eq!(
        world.step(at(1, 4, 2, 0), Direction::East),
        Some(at(2, 2, 0, 1))
    );
    let seen = world.scene(at(1, 3, 2, 0), 0, 8);
    assert!(seen
        .iter()
        .any(|c| c.location == at(2, 2, 2, 1) && c.offset == Position { x: 4, y: 0, z: 0 }));
}

#[test]
fn the_same_cell_can_have_multiple_visible_appearances_without_unbounded_cycles() {
    let mut world = World::new(vec![room(1, 3, 3, 1)], vec![]).unwrap();
    world
        .connect_area(
            Passage {
                from: at(1, 2, 0, 0),
                direction: Direction::East,
                to: at(1, 0, 0, 0),
            },
            0,
            3,
            1,
        )
        .unwrap();
    let seen = world.scene(at(1, 1, 1, 0), 0, 8);
    let appearances: Vec<_> = seen
        .iter()
        .filter(|c| c.location == at(1, 1, 1, 0))
        .collect();
    assert!(appearances.len() >= 3);
    assert!(seen.len() <= 145 + 16);
}
