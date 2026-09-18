use tor_world::{Direction as D, Extent, Location, Passage, Position, Region, RegionId, World};
fn cell(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}
fn world() -> World {
    World::new(
        (1..=3)
            .map(|id| Region {
                id: RegionId(id),
                name: "room".into(),
                bounds: Extent::new(3, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap()
}

#[test]
fn diagonal_rotation_and_all_corner_masks() {
    for d in [D::NorthEast, D::SouthEast, D::SouthWest, D::NorthWest] {
        assert_eq!(d.rotated(4), d);
        for mask in 0..8 {
            let mut w = world();
            let (a, b) = d.components().unwrap();
            let start = cell(1, 1, 1);
            let side_a = w.movement_neighbor(start, a).unwrap().0;
            let side_b = w.movement_neighbor(start, b).unwrap().0;
            let end = w.movement_neighbor(side_a, b).unwrap().0;
            for (bit, location) in [(1, side_a), (2, side_b), (4, end)] {
                if mask & bit != 0 {
                    w.set_wall(location, true).unwrap();
                }
            }
            assert_eq!(
                w.diagonal_reach(start, d, |_| true).is_some(),
                mask & 4 == 0 && mask & 3 != 3
            );
        }
    }
}

#[test]
fn rotated_aperture_combines_rotation_and_rejects_ambiguous_paths() {
    let mut w = world();
    // East becomes south after crossing. Both paths land at (2,0) in region 2.
    w.connect_area(
        Passage {
            from: cell(1, 2, 0),
            direction: D::East,
            to: cell(2, 2, 0),
        },
        1,
        3,
        1,
    )
    .unwrap();
    assert_eq!(
        w.diagonal_reach(cell(1, 2, 1), D::NorthEast, |_| true),
        Some((cell(2, 2, 0), 1))
    );
    assert_eq!(w.crossing_rotation(cell(1, 2, 1), D::NorthEast), 1);
    assert_eq!(w.step(cell(1, 2, 1), D::NorthEast), Some(cell(2, 2, 0)));
    w.set_wall(cell(1, 2, 0), true).unwrap();
    assert_eq!(
        w.diagonal_reach(cell(1, 2, 1), D::NorthEast, |_| true),
        Some((cell(2, 2, 0), 1))
    );
    let mut w = world();
    w.connect(
        Passage {
            from: cell(1, 2, 0),
            direction: D::North,
            to: cell(2, 0, 2),
        },
        0,
    )
    .unwrap();
    w.connect(
        Passage {
            from: cell(1, 2, 0),
            direction: D::East,
            to: cell(3, 0, 2),
        },
        0,
    )
    .unwrap();
    assert_eq!(
        w.diagonal_reach(cell(1, 2, 0), D::NorthEast, |_| true),
        None
    );
    assert!(w
        .connect(
            Passage {
                from: cell(1, 2, 0),
                direction: D::NorthEast,
                to: cell(2, 0, 0)
            },
            0
        )
        .is_err());
}
