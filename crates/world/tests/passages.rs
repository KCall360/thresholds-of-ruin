use tor_world::{
    Direction, Extent, Location, Passage, Position, Region, RegionId, World, WorldError,
};

fn cell(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

fn room(id: u64) -> Region {
    Region {
        id: RegionId(id),
        name: format!("Room {id}"),
        bounds: Extent::new(3, 3, 2).unwrap(),
    }
}

#[test]
fn crossing_a_boundary_uses_region_local_destination_coordinates() {
    let passage = Passage {
        from: cell(1, 2, 1, 0),
        direction: Direction::East,
        to: cell(2, 0, 1, 1),
    };
    let world = World::new(vec![room(1), room(2)], vec![passage]).unwrap();
    assert_eq!(
        world.step(cell(1, 1, 1, 0), Direction::East),
        Some(cell(1, 2, 1, 0))
    );
    assert_eq!(
        world.step(cell(1, 2, 1, 0), Direction::East),
        Some(cell(2, 0, 1, 1))
    );
    assert_eq!(world.step(cell(1, 2, 0, 0), Direction::East), None);
    // Connections are directed: a return path must be declared explicitly.
    assert_eq!(world.step(cell(2, 0, 1, 1), Direction::West), None);
}

#[test]
fn vertical_steps_and_extreme_invalid_coordinates_do_not_wrap() {
    let world = World::new(vec![room(1)], vec![]).unwrap();
    assert_eq!(
        world.step(cell(1, 1, 1, 0), Direction::Up),
        Some(cell(1, 1, 1, 1))
    );
    assert_eq!(
        world.step(cell(1, 1, 1, 1), Direction::Down),
        Some(cell(1, 1, 1, 0))
    );
    assert_eq!(world.step(cell(1, 1, 1, 1), Direction::Up), None);
    assert_eq!(world.step(cell(1, i32::MAX, 1, 0), Direction::East), None);
    assert_eq!(world.step(cell(1, i32::MIN, 1, 0), Direction::West), None);
    assert_eq!(world.step(cell(99, 1, 1, 0), Direction::East), None);
}

#[test]
fn invalid_or_ambiguous_topology_is_rejected_at_construction() {
    let passage = Passage {
        from: cell(1, 2, 1, 0),
        direction: Direction::East,
        to: cell(2, 0, 1, 0),
    };
    assert_eq!(
        World::new(vec![room(1), room(1)], vec![]),
        Err(WorldError::DuplicateRegion)
    );
    assert_eq!(
        World::new(vec![room(1)], vec![passage]),
        Err(WorldError::InvalidEndpoint)
    );
    assert_eq!(
        World::new(
            vec![room(1), room(2)],
            vec![Passage {
                from: cell(1, 1, 1, 0),
                ..passage
            }]
        ),
        Err(WorldError::NotBoundaryExit)
    );
    assert_eq!(
        World::new(vec![room(1), room(2)], vec![passage, passage]),
        Err(WorldError::DuplicateExit)
    );
}
