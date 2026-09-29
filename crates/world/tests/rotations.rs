use tor_world::{compose_rotation, inverse_rotation, rotate_vector};

#[test]
fn cube_rotations_preserve_lengths_and_have_exact_inverses() {
    let mut images = std::collections::BTreeSet::new();
    for r in 0..24 {
        let v = rotate_vector(r, [1, 2, 3]);
        assert_eq!(v.iter().map(|x| x * x).sum::<i64>(), 14);
        assert!(images.insert(v));
        assert_eq!(rotate_vector(inverse_rotation(r), v), [1, 2, 3]);
        for s in 0..24 {
            assert_eq!(
                rotate_vector(compose_rotation(r, s), [1, 2, 3]),
                rotate_vector(s, v)
            );
        }
    }
}

#[test]
fn physical_vertical_apertures_do_not_disclose_a_false_ceiling() {
    use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in 1..=2 {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: "shaft".into(),
                bounds: Extent::new(3, 3, 2).unwrap(),
            })
            .unwrap();
    }
    let at = |region, z| Location {
        region: RegionId(region),
        position: Position { x: 1, y: 1, z },
    };
    world
        .connect_portal_area(
            Passage {
                from: at(1, 1),
                direction: Direction::Up,
                to: at(2, 0),
            },
            0,
            1,
            1,
        )
        .unwrap();
    // Straight up through the portal: open space where region 1's shell would
    // be, and the real ceiling at the top of region 2.
    let above = |z: i32| {
        world
            .eye_scene(at(1, 0), 0, 8)
            .into_iter()
            .find(|c| c.offset == Position { x: 0, y: 0, z })
            .map(|c| (c.location, c.wall))
    };
    assert_eq!(above(2), Some((at(2, 0), false)));
    assert_eq!(above(3), Some((at(2, 1), false)));
    assert_eq!(above(4).map(|(_, wall)| wall), Some(true));
}

#[test]
fn volume_stair_landing_preserves_the_crossing_frame() {
    use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};
    let origin = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let target = Location {
        region: RegionId(2),
        ..origin
    };
    let mut world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: "stairs".into(),
                bounds: Extent::new(3, 3, 2).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    world
        .connect(
            Passage {
                from: origin,
                direction: Direction::Up,
                to: target,
            },
            1,
        )
        .unwrap();
    let landing = world
        .volume_scene(origin, 23, 4)
        .into_iter()
        .find(|c| c.offset.z == 5)
        .unwrap();
    assert_eq!(landing.location, target);
    assert_eq!(landing.rotation, compose_rotation(23, 1));
}

#[test]
fn volume_main_plane_retains_beveled_doorway_visibility() {
    use tor_world::{Extent, Location, Position, Region, RegionId, World};
    let mut world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "room".into(),
            bounds: Extent::new(5, 5, 3).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let at = |x, y| Location {
        region: RegionId(1),
        position: Position { x, y, z: 1 },
    };
    world.set_wall(at(3, 2), true).unwrap();
    let planar: Vec<_> = world
        .shadow_scene(at(2, 2), 0, 4)
        .into_iter()
        .filter(|c| c.offset.z == 0)
        .collect();
    assert!(planar.iter().any(|c| c.location == at(3, 1)));
    let volume: Vec<_> = world
        .volume_scene(at(2, 2), 0, 4)
        .into_iter()
        .filter(|c| c.offset.z == 0)
        .collect();
    assert_eq!(volume, planar);
}
