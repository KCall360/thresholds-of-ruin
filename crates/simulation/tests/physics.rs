use std::num::NonZeroU64;
use tor_simulation::{Action, BodySpec, Game};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn at(x: i32, z: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z },
    }
}
fn game() -> (Game, tor_simulation::ActorId) {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "shaft".into(),
            bounds: Extent::new(8, 4, 16).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42);
    let id = game
        .spawn_actor(at(2, 10), NonZeroU64::new(100).unwrap())
        .unwrap();
    (game, id)
}

#[test]
fn bodies_fall_land_and_resume_identically() {
    let (mut game, id) = game();
    game.set_body(
        id,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert!(game.observe(id).unwrap().location.position.z < 10);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let mut restored = Game::restore_checkpoint(game.checkpoint(&mut shared), &shared).unwrap();
    for _ in 0..10 {
        assert_eq!(game.act(id, Action::Wait), restored.act(id, Action::Wait));
        assert_eq!(game, restored);
    }
    assert_eq!(game.observe(id).unwrap().location.position.z, 0);
    assert_eq!(game.actor_motion(id).unwrap().velocity, [0; 3]);
    assert!(
        game.physics_impacts().is_empty(),
        "resting contact must not cause repeated impacts"
    );
}

#[test]
fn equal_opposing_fields_cancel_without_erasing_drift() {
    let (mut game, id) = game();
    game.set_body(
        id,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.set_cell_gravity(at(2, 11), [0, 0, 1]).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert_eq!(game.observe(id).unwrap().location, at(2, 10));
    assert_eq!(game.actor_motion(id).unwrap().velocity, [0; 3]);
}

#[test]
fn zero_gravity_preserves_drift_and_walls_remove_only_normal_velocity() {
    let (mut game, id) = game();
    game.teleport(id, at(6, 2)).unwrap();
    game.set_gravity(RegionId(1), [0; 3]).unwrap();
    game.set_actor_velocity(id, [4096, 0, 4096]).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert_eq!(game.observe(id).unwrap().location.position.x, 7);
    assert_eq!(game.actor_motion(id).unwrap().velocity[0], 0);
    assert_eq!(game.actor_motion(id).unwrap().velocity[2], 4096);
    assert!(game.physics_impacts().iter().any(|i| i.axis == 0));
}

#[test]
fn whole_body_blocks_low_passages_and_wizard_terrain_edits() {
    let (mut game, id) = game();
    game.set_body(
        id,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    assert!(game.set_wall(at(2, 11), true).is_err());
    game.set_wall(at(3, 11), true).unwrap();
    assert!(game
        .act(id, Action::Move(tor_world::Direction::East))
        .is_err());
    assert_eq!(game.tick(), 0);
    assert!(game.teleport(id, at(3, 10)).is_err());
}

#[test]
fn sideways_portal_rotates_body_and_velocity_without_changing_speed() {
    use tor_world::{rotate_vector, Direction, Passage};
    let (mut game, id) = game();
    let region = RegionId(2);
    game.add_region(Region {
        id: region,
        name: "sideways".into(),
        bounds: Extent::new(8, 4, 16).unwrap(),
    })
    .unwrap();
    game.teleport(id, at(7, 10)).unwrap();
    game.set_body(
        id,
        BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    let rotation = (0..24)
        .find(|r| {
            rotate_vector(*r, [1, 0, 0]) == [0, 0, -1] && rotate_vector(*r, [0, 0, 1]) == [1, 0, 0]
        })
        .unwrap();
    game.connect_portal_area(
        Passage {
            from: at(7, 10),
            direction: Direction::East,
            to: Location {
                region,
                position: Position { x: 2, y: 1, z: 15 },
            },
        },
        rotation,
        1,
        2,
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0; 3]).unwrap();
    game.set_gravity(region, [0; 3]).unwrap();
    game.set_actor_velocity(id, [4096, 0, 0]).unwrap();
    game.act(id, Action::Wait).unwrap();
    let location = game.observe(id).unwrap().location;
    assert_eq!(location.region, region);
    assert_eq!(location.position.x, 2);
    assert_eq!(location.position.z, 10);
    assert_eq!(game.actor_motion(id).unwrap().velocity, [4096, 0, 0]);
    let mut head = location;
    head.position.x += 1;
    assert!(
        game.set_wall(head, true).is_err(),
        "transformed head occupies +x"
    );
}

#[test]
fn loose_items_fall_and_other_actors_keep_their_turns() {
    let (mut game, id) = game();
    let other = game
        .spawn_actor(at(5, 10), NonZeroU64::new(50).unwrap())
        .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.place_item(at(2, 10), "weight".into()).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.next_actor(), Some(other));
    game.act(other, Action::Wait).unwrap();
    assert_eq!(game.tick(), 50);
    game.act(other, Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    assert_eq!(game.next_actor(), Some(id));
    assert_eq!(game.observe(id).unwrap().ground_items.len(), 1);
    assert!(
        game.observe(id).unwrap().ground_items[0]
            .location
            .position
            .z
            < 10
    );
}

#[test]
fn gravity_does_not_use_stair_links() {
    let (mut game, id) = game();
    game.connect(
        tor_world::Passage {
            from: at(2, 10),
            direction: tor_world::Direction::Down,
            to: at(6, 3),
        },
        0,
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert_eq!(game.observe(id).unwrap().location.position.x, 2);
    assert_eq!(game.observe(id).unwrap().location.position.z, 9);
}

#[test]
fn straddling_body_averages_fields_in_both_regions() {
    use tor_world::{Direction, Passage};
    let (mut game, id) = game();
    game.add_region(Region {
        id: RegionId(2),
        name: "opposite".into(),
        bounds: Extent::new(8, 4, 16).unwrap(),
    })
    .unwrap();
    let destination = Location {
        region: RegionId(2),
        position: Position { x: 0, y: 1, z: 10 },
    };
    game.connect_portal_area(
        Passage {
            from: at(7, 10),
            direction: Direction::East,
            to: destination,
        },
        0,
        1,
        1,
    )
    .unwrap();
    game.teleport(id, at(7, 10)).unwrap();
    game.set_body(
        id,
        BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.set_gravity(RegionId(2), [0, 0, 1]).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert_eq!(game.actor_motion(id).unwrap().velocity, [0; 3]);
    assert!(game.set_wall(destination, true).is_err());
}

#[test]
fn dropped_items_inherit_carrier_motion() {
    let (mut game, id) = game();
    game.set_gravity(RegionId(1), [0; 3]).unwrap();
    let item = game.place_item(at(2, 10), "weight".into()).unwrap();
    game.act(
        id,
        Action::Take {
            item,
            quantity: None,
        },
    )
    .unwrap();
    game.set_actor_velocity(id, [2048, 0, 0]).unwrap();
    game.act(
        id,
        Action::Drop {
            item,
            quantity: None,
        },
    )
    .unwrap();
    let view = game.observe(id).unwrap();
    assert_eq!(view.ground_items[0].location, view.location);
    assert_ne!(view.location, at(2, 10));
}

#[test]
fn resting_population_does_not_integrate_every_tick() {
    let (mut game, id) = game();
    game.teleport(id, at(2, 0)).unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let before = tor_simulation::diagnostics::work_counts();
    game.act(id, Action::Wait).unwrap();
    let after = tor_simulation::diagnostics::work_counts();
    assert_eq!(before.physics_steps, after.physics_steps);
    assert_eq!(game.tick(), 100);
}

#[test]
fn resting_waits_and_same_tick_handoffs_do_not_rebuild_physics_scenes() {
    let (mut game, id) = game();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    assert!(game.wait_changes_perception(id));
    let other = game
        .spawn_actor(at(5, 10), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(!game.wait_changes_perception(id));
    game.act(id, Action::Wait).unwrap();
    assert!(game.wait_changes_perception(other));
    game.act(other, Action::Wait).unwrap();
    assert!(
        game.wait_changes_perception(id),
        "prior displacement sensation must clear"
    );
    for _ in 0..20 {
        let next = game.next_actor().unwrap();
        game.act(next, Action::Wait).unwrap();
    }
    assert!(!game.wait_changes_perception(game.next_actor().unwrap()));
}

#[test]
fn wall_impact_damages_only_the_moving_actor_and_can_end_the_run_at_contact() {
    use std::collections::{BTreeMap, BTreeSet};
    use tor_simulation::combat::CombatSpec;
    let (mut game, id) = game();
    game.teleport(id, at(6, 2)).unwrap();
    game.set_gravity(RegionId(1), [0; 3]).unwrap();
    game.configure_combat(
        id,
        CombatSpec {
            max_hp: 1,
            ..Default::default()
        },
    )
    .unwrap();
    game.configure_run(id, BTreeSet::from([id]), None, BTreeMap::new())
        .unwrap();
    game.set_actor_velocity(id, [4096, 0, 0]).unwrap();
    game.act(id, Action::Wait).unwrap();
    assert_eq!(game.health(id), Some((0, 1)));
    assert!(game.tick() < 100);
    assert_eq!(game.next_actor(), None);
    assert_eq!(game.observe(id).unwrap().location, at(7, 2));
    assert!(game
        .physics_impacts()
        .iter()
        .any(|impact| impact.at_tick == game.tick()));
}
