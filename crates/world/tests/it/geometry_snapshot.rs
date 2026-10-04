use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

#[test]
fn geometry_witnesses_follow_edits_and_clones_without_entering_saved_state() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "one".into(),
            bounds: Extent::new(16, 16, 2).unwrap(),
        })
        .unwrap();
    let at = |region, x| Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    };
    let original = world.clone();
    let initial = world.geometry_snapshot();
    assert_eq!(initial, original.geometry_snapshot());
    world.set_wall(at(1, 2), true).unwrap();
    assert_ne!(world.geometry_snapshot(), initial);
    assert_eq!(original.geometry_snapshot(), initial);
    let wall = world.geometry_snapshot();
    world
        .add_region(Region {
            id: RegionId(2),
            name: "two".into(),
            bounds: Extent::new(16, 16, 2).unwrap(),
        })
        .unwrap();
    assert_ne!(world.geometry_snapshot(), wall);
    let added = world.geometry_snapshot();
    world
        .connect(
            Passage {
                from: at(1, 15),
                direction: Direction::East,
                to: at(2, 0),
            },
            1,
        )
        .unwrap();
    assert_ne!(world.geometry_snapshot(), added);
    let connected = world.geometry_snapshot();
    let slice = world.detach_region(RegionId(2)).unwrap();
    assert_ne!(world.geometry_snapshot(), connected);
    let detached = world.geometry_snapshot();
    world.attach_region(slice).unwrap();
    assert_ne!(world.geometry_snapshot(), detached);
    let restored: World = serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
    assert_eq!(restored, world);
    assert_ne!(restored.geometry_snapshot(), world.geometry_snapshot());
    assert_eq!(
        serde_json::to_vec(&restored).unwrap(),
        serde_json::to_vec(&world).unwrap()
    );
}
