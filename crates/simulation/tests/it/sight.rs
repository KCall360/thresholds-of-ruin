//! Gameplay sight starts at the eye cell a body declares; see docs/sight-3d.md.
use std::num::NonZeroU64;
use tor_simulation::{Action, BodySpec, Game};
use tor_world::{rotate_vector, Direction, Extent, Location, Passage, Position, Region, RegionId};

fn body(height: i32, eye: [i32; 3]) -> BodySpec {
    BodySpec {
        cells: (0..height).map(|z| [0, 0, z]).collect(),
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
        game.set_body(id, body(2, eye)).unwrap();
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
    for (height, eye, sees_past_the_wall) in [
        (1, [0, 0, 0], false),
        (2, [0, 0, 1], true),
        (2, [0, 0, 0], false),
        (3, [0, 0, 2], true),
    ] {
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
        game.set_body(id, body(height, eye)).unwrap();
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
            height.max(2) as u16,
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
        // Every occupied body cell rotates to region +x while keeping its
        // original body-frame offset in the disclosed scene.
        for z in 0..height {
            assert!(scene
                .iter()
                .any(|c| c.location == at(2, 2 + z, 10) && c.offset == Position { x: 0, y: 0, z }));
        }
        assert_eq!(
            scene.iter().any(|c| c.location == at(2, 2, 7)),
            sees_past_the_wall,
            "eye {eye:?}: the cell behind the wall"
        );
    }
}

#[test]
fn upright_bodies_of_every_height_see_from_their_eye_and_keep_offsets_at_the_feet() {
    for height in 1..=3 {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_chamber(Region {
                id: RegionId(1),
                name: "tall room".into(),
                bounds: Extent::new(8, 3, 3).unwrap(),
            })
            .unwrap();
        let at = |x, z| Location {
            region: RegionId(1),
            position: Position { x, y: 1, z },
        };
        world.set_wall(at(1, 0), true).unwrap();
        let mut game = Game::new(world, 42);
        let actor = game
            .spawn_actor(at(0, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(actor, body(height, [0, 0, height - 1]))
            .unwrap();
        let scene = game.scene(actor).unwrap();
        for z in 0..height {
            assert!(scene
                .iter()
                .any(|c| c.location == at(0, z) && c.offset == Position { x: 0, y: 0, z }));
        }
        assert!(scene
            .iter()
            .any(|c| c.location == at(0, -1) && c.offset == Position { x: 0, y: 0, z: -1 }));
        assert_eq!(
            scene.iter().any(|c| c.location == at(3, -1)),
            height > 1,
            "height {height}: floor beyond the waist wall"
        );
    }
}

#[test]
fn occupied_cells_win_when_eye_projection_conflicts_at_a_one_way_physical_join() {
    let mut expected_scene: Option<Vec<tor_world::SightCell>> = None;
    for reversed in [false, true] {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        for id in 1..=2 {
            world
                .add_chamber(Region {
                    id: RegionId(id),
                    name: "shaft".into(),
                    bounds: Extent::new(3, 3, 2).unwrap(),
                })
                .unwrap();
        }
        let feet = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 1 },
        };
        let eye = Location {
            region: RegionId(2),
            position: Position { x: 1, y: 1, z: 0 },
        };
        world
            .connect_portal_area(
                Passage {
                    from: feet,
                    direction: Direction::Up,
                    to: eye,
                },
                0,
                1,
                1,
            )
            .unwrap();
        let mut game = Game::new(world, 42);
        let id = game
            .spawn_actor(feet, NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut spec = body(2, [0, 0, 1]);
        if reversed {
            spec.cells.reverse();
        }
        game.set_body(id, spec).unwrap();
        let scene = game.scene(id).unwrap();
        for (offset, location) in [(0, feet), (1, eye)] {
            let cell = scene
                .iter()
                .find(|c| {
                    c.offset
                        == Position {
                            x: 0,
                            y: 0,
                            z: offset,
                        }
                })
                .unwrap();
            assert_eq!(
                cell.location, location,
                "authored order reversed: {reversed}"
            );
            assert!(!cell.wall);
        }
        if let Some(expected) = &expected_scene {
            assert_eq!(scene.len(), expected.len());
            for (cell, expected) in scene.iter().zip(expected) {
                assert_eq!(cell, expected, "body cell order must not change perception");
            }
        } else {
            expected_scene = Some(scene.clone());
        }
        let observation = game.observe(id).unwrap();
        assert!(observation
            .visible_cells
            .iter()
            .any(|c| c.location == feet && !c.wall));
        assert!(observation
            .visible_cells
            .iter()
            .any(|c| c.location == eye && !c.wall));
    }
}
