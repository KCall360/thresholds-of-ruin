//! Gameplay sight starts at the eye cell a body declares; see docs/sight-3d.md.
use std::num::NonZeroU64;
use tor_simulation::{Action, BodySpec, Game};
use tor_world::{rotate_vector, Direction, Extent, Location, Passage, Position, Region, RegionId};

fn two_cell_body(eye: [i32; 3]) -> BodySpec {
    BodySpec {
        cells: vec![[0, 0, 0], [0, 0, 1]],
        eye,
        mass: 80,
    }
}

#[test]
fn a_humanoid_sees_over_a_waist_wall_only_from_its_head_and_offsets_stay_at_its_feet() {
    for (eye, sees_behind) in [([0, 0, 1], true), ([0, 0, 0], false)] {
        let mut game = Game::two_room_in_stone(42);
        let feet = Location {
            region: RegionId(1),
            position: Position { x: 0, y: 0, z: 0 },
        };
        let cell = |x, z| Location {
            position: Position { x, y: 0, z },
            ..feet
        };
        let id = game
            .spawn_actor(feet, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(id, two_cell_body(eye)).unwrap();
        // A wall one cell high in a room two cells high, along the room's
        // edge row (the fixture's items and hints are in the middle row).
        game.set_wall(cell(1, 0), true).unwrap();
        let scene = game.scene(id).unwrap();
        let seen_at = |x, y, z| {
            scene
                .iter()
                .find(|c| c.offset == Position { x, y, z })
                .map(|c| c.location)
        };
        assert_eq!(
            seen_at(0, 0, 0),
            Some(feet),
            "eye {eye:?}: offsets from the feet"
        );
        assert_eq!(
            seen_at(0, 0, 1),
            Some(cell(0, 1)),
            "eye {eye:?}: own head cell"
        );
        assert_eq!(
            seen_at(0, 0, -1),
            Some(cell(0, -1)),
            "eye {eye:?}: floor underfoot"
        );
        assert_eq!(
            seen_at(3, 0, -1).is_some(),
            sees_behind,
            "eye {eye:?}: floor behind the waist wall"
        );
    }
}

#[test]
fn the_eye_turns_with_a_body_lying_sideways_after_a_rotated_portal() {
    for (eye, sees_past_the_wall) in [([0, 0, 1], true), ([0, 0, 0], false)] {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        for id in 1..=2 {
            world
                .add_region(Region {
                    id: RegionId(id),
                    name: "shaft".into(),
                    bounds: Extent::new(8, 4, 16).unwrap(),
                })
                .unwrap();
        }
        let at = |region, x, z| Location {
            region: RegionId(region),
            position: Position { x, y: 1, z },
        };
        let mut game = Game::new(world, 42);
        let id = game
            .spawn_actor(at(1, 7, 10), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(id, two_cell_body(eye)).unwrap();
        // Region 2 is stored sideways: body +x becomes region -z, and body
        // up (+z) becomes region +x.
        let rotation = (0..24)
            .find(|r| {
                rotate_vector(*r, [1, 0, 0]) == [0, 0, -1]
                    && rotate_vector(*r, [0, 0, 1]) == [1, 0, 0]
            })
            .unwrap();
        game.connect_portal_area(
            Passage {
                from: at(1, 7, 10),
                direction: Direction::East,
                to: at(2, 2, 15),
            },
            rotation,
            1,
            2,
        )
        .unwrap();
        // A wall straight "ahead" of the feet in region 2; the head is one
        // cell to the side of it.
        game.set_wall(at(2, 2, 9), true).unwrap();
        game.set_gravity(RegionId(1), [0; 3]).unwrap();
        game.set_gravity(RegionId(2), [0; 3]).unwrap();
        game.set_actor_velocity(id, [4096, 0, 0]).unwrap();
        game.act(id, Action::Wait).unwrap();
        assert_eq!(game.observe(id).unwrap().location, at(2, 2, 10));
        let scene = game.scene(id).unwrap();
        // The head lies at region +x, still at body-frame offset (0, 0, 1).
        assert!(scene
            .iter()
            .any(|c| c.location == at(2, 3, 10) && c.offset == Position { x: 0, y: 0, z: 1 }));
        assert_eq!(
            scene.iter().any(|c| c.location == at(2, 2, 7)),
            sees_past_the_wall,
            "eye {eye:?}: the cell behind the wall"
        );
    }
}
