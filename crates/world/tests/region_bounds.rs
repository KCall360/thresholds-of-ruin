use tor_world::{Extent, Position};

#[test]
fn region_bounds_include_all_three_axes() {
    let room = Extent::new(8, 5, 3).unwrap();
    assert!(room.contains(Position { x: 0, y: 0, z: 0 }));
    assert!(room.contains(Position { x: 7, y: 4, z: 2 }));
    for outside in [
        Position { x: -1, y: 0, z: 0 },
        Position { x: 0, y: -1, z: 0 },
        Position { x: 0, y: 0, z: -1 },
        Position { x: 8, y: 0, z: 0 },
        Position { x: 0, y: 5, z: 0 },
        Position { x: 0, y: 0, z: 3 },
    ] {
        assert!(!room.contains(outside), "accepted {outside:?}");
    }
}
