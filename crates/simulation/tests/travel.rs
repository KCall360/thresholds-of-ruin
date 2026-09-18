use std::num::NonZeroU64;
use tor_simulation::{Action, Game};
use tor_world::{Direction, Location, Position, RegionId};

fn cell(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}

#[test]
fn travel_uses_remembered_connections_and_ordinary_actions() {
    let mut game = Game::two_room(42);
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    let route = game.travel_route(actor, cell(3, 1)).unwrap();
    assert_eq!(
        route.iter().map(|s| s.direction).collect::<Vec<_>>(),
        vec![Direction::East, Direction::East]
    );
    for step in route {
        game.act(actor, Action::Move(step.direction)).unwrap();
    }
    assert_eq!(game.observe(actor).unwrap().location, cell(3, 1));
    assert_eq!(game.tick(), 200);
}

#[test]
fn unseen_changes_do_not_change_a_remembered_route() {
    let mut game = Game::two_room(42);
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    let before = game.travel_route(actor, cell(3, 1)).unwrap();
    // No perception refresh: route planning must not consult current terrain.
    game.set_wall(cell(2, 1), true).unwrap();
    assert_eq!(game.travel_route(actor, cell(3, 1)).unwrap(), before);
    assert!(game.act(actor, Action::Move(before[0].direction)).is_err());
    assert_eq!(game.tick(), 0);
    game.refresh_navigation();
    assert_ne!(game.travel_route(actor, cell(3, 1)), Ok(before));
}

#[test]
fn unknown_destinations_and_walls_are_unavailable() {
    let mut game = Game::two_room(42);
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(game.travel_route(actor, cell(3, 1)).is_err());
    game.set_wall(cell(3, 1), true).unwrap();
    game.refresh_navigation();
    assert!(game.travel_route(actor, cell(3, 1)).is_err());
    assert!(game.travel_route(actor, cell(100, 100)).is_err());
    assert!(game.travel_route(actor, cell(1, 1)).unwrap().is_empty());
}

#[test]
fn routes_cross_rotated_joins_and_preserve_view_directions() {
    use tor_world::{Extent, Passage, Region, World};
    let mut world = World::new(
        vec![
            Region {
                id: RegionId(1),
                name: "A".into(),
                bounds: Extent::new(3, 3, 1).unwrap(),
            },
            Region {
                id: RegionId(2),
                name: "B".into(),
                bounds: Extent::new(3, 3, 1).unwrap(),
            },
        ],
        vec![],
    )
    .unwrap();
    let target = Location {
        region: RegionId(2),
        position: Position { x: 1, y: 1, z: 0 },
    };
    world
        .connect(
            Passage {
                from: cell(2, 1),
                direction: Direction::East,
                to: Location {
                    position: Position { x: 1, y: 0, z: 0 },
                    ..target
                },
            },
            1,
        )
        .unwrap();
    let mut game = Game::new(world, 0);
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    let route = game.travel_route(actor, target).unwrap();
    assert_eq!(
        route.iter().map(|s| s.direction).collect::<Vec<_>>(),
        vec![Direction::East; 3]
    );
    for step in route {
        game.act(actor, Action::Move(step.direction)).unwrap();
    }
    assert_eq!(game.observe(actor).unwrap().location, target);
}

#[test]
fn explicit_stairs_are_known_connections_and_self_loops_terminate() {
    use tor_world::{Extent, Passage, Region};
    let mut game = Game::two_room(0);
    game.add_region(Region {
        id: RegionId(3),
        name: "Upstairs".into(),
        bounds: Extent::new(3, 3, 1).unwrap(),
    })
    .unwrap();
    let landing = Location {
        region: RegionId(3),
        position: Position { x: 1, y: 1, z: 0 },
    };
    game.connect(
        Passage {
            from: cell(1, 1),
            direction: Direction::Up,
            to: landing,
        },
        0,
    )
    .unwrap();
    game.connect(
        Passage {
            from: cell(2, 0),
            direction: Direction::North,
            to: cell(2, 0),
        },
        2,
    )
    .unwrap();
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    assert_eq!(
        game.travel_route(actor, landing).unwrap()[0].direction,
        Direction::Up
    );
    assert!(!game.travel_route(actor, cell(2, 0)).unwrap().is_empty());
    assert!(game.travel_route(actor, cell(100, 0)).is_err());
}

#[test]
fn hidden_shortcut_cannot_affect_a_known_route() {
    // Separate scenes can disclose two locations without disclosing a link.
    use tor_world::{Extent, Passage, Region};
    let mut game = Game::two_room(0);
    game.add_region(Region {
        id: RegionId(3),
        name: "Other".into(),
        bounds: Extent::new(3, 3, 1).unwrap(),
    })
    .unwrap();
    let elsewhere = Location {
        region: RegionId(3),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    game.teleport(actor, elsewhere).unwrap();
    game.refresh_navigation();
    game.teleport(actor, cell(1, 1)).unwrap();
    game.refresh_navigation();
    assert!(game.travel_route(actor, elsewhere).is_err());
    game.connect(
        Passage {
            from: cell(1, 1),
            direction: Direction::Up,
            to: elsewhere,
        },
        0,
    )
    .unwrap();
    assert!(game.travel_route(actor, elsewhere).is_err());
    game.refresh_navigation();
    assert_eq!(game.travel_route(actor, elsewhere).unwrap().len(), 1);
}

#[test]
fn splitting_a_known_space_across_a_wide_join_does_not_change_the_route() {
    use tor_world::{Extent, Passage, Region, World};
    let region = |id, width| Region {
        id: RegionId(id),
        name: "Internal".into(),
        bounds: Extent::new(width, 3, 1).unwrap(),
    };
    let mut split_world = World::new(vec![region(1, 3), region(2, 3)], vec![]).unwrap();
    split_world
        .connect_area(
            Passage {
                from: cell(2, 0),
                direction: Direction::East,
                to: Location {
                    region: RegionId(2),
                    position: Position { x: 0, y: 0, z: 0 },
                },
            },
            0,
            3,
            1,
        )
        .unwrap();
    let mut split = Game::new(split_world, 0);
    let mut whole = Game::new(World::new(vec![region(1, 6)], vec![]).unwrap(), 0);
    let a = split
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let b = whole
        .spawn_actor(cell(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    split.refresh_navigation();
    whole.refresh_navigation();
    let directions = |route: Vec<tor_simulation::TravelStep>| {
        route
            .into_iter()
            .map(|step| step.direction)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        directions(
            split
                .travel_route(
                    a,
                    Location {
                        region: RegionId(2),
                        position: Position { x: 1, y: 1, z: 0 }
                    }
                )
                .unwrap()
        ),
        directions(whole.travel_route(b, cell(4, 1)).unwrap())
    );
}
