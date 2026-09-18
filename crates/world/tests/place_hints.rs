use tor_world::*;

#[test]
fn anchors_are_independent_of_topology_and_terrain_edits() {
    let mut world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "Internal".into(),
            bounds: Extent::new(5, 3, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let at = Location {
        region: RegionId(1),
        position: Position { x: 2, y: 1, z: 0 },
    };
    let scene = world.scene(at, 0, 8);
    world.set_place_hint(at, true).unwrap();
    world.set_place_hint(at, true).unwrap();
    assert!(world.has_place_hint(at));
    assert_eq!(world.scene(at, 0, 8), scene);
    let before = world.clone();
    assert!(world
        .set_place_hint(
            Location {
                region: RegionId(9),
                ..at
            },
            true
        )
        .is_err());
    assert_eq!(world, before);
    world.set_wall(at, true).unwrap();
    assert!(!world.has_place_hint(at));
    world.set_wall(at, false).unwrap();
    assert!(world.has_place_hint(at));
    world.set_place_hint(at, false).unwrap();
    assert!(!world.has_place_hint(at));
}
