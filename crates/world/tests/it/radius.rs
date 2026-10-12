use std::collections::BTreeSet;
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 0, z: 0 },
    }
}
#[test]
fn current_3d_sight_clamps_public_radius_at_sixteen_on_every_builder() {
    // A one-cell open volume gives a literal independent oracle for every cell,
    // including the 0/1/15/16/17 boundaries, without a second visibility algorithm.
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "range".into(),
            bounds: Extent::new(35, 1, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let eye = at(17);
    for (radius, reach) in [(0, 0), (1, 1), (15, 15), (16, 16), (17, 16), (255, 16)] {
        let expected: BTreeSet<_> = ((17 - reach)..=(17 + reach)).map(at).collect();
        let uncached = world.eye_scene_uncached(eye, 0, radius);
        assert_eq!(
            uncached.iter().map(|c| c.location).collect::<BTreeSet<_>>(),
            expected,
            "radius {radius}"
        );
        assert_eq!(uncached.len(), expected.len());
        assert_eq!(world.eye_scene_reference(eye, 0, radius), uncached);
        assert_eq!(world.eye_scene(eye, 0, radius), uncached);
        assert!(world.eye_scene_cached(eye, 0, radius));
        assert_eq!(world.eye_scene(eye, 0, radius), uncached);
        assert_eq!(world.illuminated_eye_scene(eye, 0, radius), uncached);
    }
}

#[test]
fn radius_sixteen_cached_views_follow_bounded_wall_and_light_mutations() {
    let mut world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "range mutations".into(),
            bounds: Extent::new(35, 1, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let eye = at(1);
    // This small sequence adds the production radius to mutation coverage rather
    // than multiplying the exhaustive geometry matrix by another radius.
    for (wall, lit) in [(false, true), (true, true), (false, false), (false, true)] {
        world.set_wall(at(16), wall).unwrap();
        world.set_cell_light(at(17), lit).unwrap();
        let fresh = world.eye_scene_uncached(eye, 0, 16);
        assert_eq!(world.eye_scene_reference(eye, 0, 16), fresh);
        assert_eq!(world.eye_scene(eye, 0, 16), fresh);
        assert_eq!(world.eye_scene(eye, 0, 16), fresh);
        assert_eq!(fresh.iter().any(|c| c.location == at(17)), !wall);
        assert!(!fresh.iter().any(|c| c.location == at(18)));
        let illuminated = world.illuminated_eye_scene(eye, 0, 16);
        assert_eq!(
            illuminated.iter().any(|c| c.location == at(17)),
            !wall && lit
        );
        assert_eq!(
            illuminated,
            fresh
                .into_iter()
                .filter(|c| world.is_lit(c.location) == Some(true))
                .collect::<Vec<_>>()
        );
    }
}
