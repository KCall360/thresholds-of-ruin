use std::num::NonZeroU64;
use tor_simulation::{Action, Game, GameError};
use tor_world::{Direction, Extent, Location, Position, Region, RegionId, World};

fn cell(x: i32, y: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    }
}
fn game(cost: u64) -> (Game, tor_simulation::ActorId) {
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "room".into(),
            bounds: Extent::new(5, 5, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42);
    game.enable_diagonals();
    let actor = game
        .spawn_actor(cell(2, 2), NonZeroU64::new(cost).unwrap())
        .unwrap();
    (game, actor)
}

#[test]
fn diagonal_cost_and_one_clear_side() {
    for (cost, expected) in [(1, 2), (2, 3), (75, 107), (100, 142)] {
        let (mut game, actor) = game(cost);
        game.set_wall(cell(2, 1), true).unwrap();
        game.act(actor, Action::Move(Direction::NorthEast)).unwrap();
        assert_eq!(game.tick(), expected);
        assert_eq!(game.observe(actor).unwrap().location, cell(3, 1));
    }
}

#[test]
fn blocked_corners_and_overflow_are_atomic() {
    let (mut game, actor) = game(100);
    game.set_wall(cell(2, 1), true).unwrap();
    game.set_wall(cell(3, 2), true).unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::NorthEast)),
        Err(GameError::Blocked)
    );
    assert_eq!(game, before);
    let (mut game, actor) = self::game(u64::MAX);
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::NorthEast)),
        Err(GameError::TimeExhausted)
    );
    assert_eq!(game, before);
}

#[test]
fn diagonal_door_reach_uses_normal_cost_and_corner_clearance() {
    let (mut game, actor) = game(100);
    let door = game.place_door(cell(3, 1), false).unwrap();
    game.set_wall(cell(2, 1), true).unwrap();
    game.act(actor, Action::SetDoor { door, open: true })
        .unwrap();
    assert_eq!(game.tick(), 100);
    game.act(actor, Action::SetDoor { door, open: false })
        .unwrap();
    game.set_wall(cell(3, 2), true).unwrap();
    assert_eq!(
        game.act(actor, Action::SetDoor { door, open: true }),
        Err(GameError::DoorUnavailable)
    );
}

#[test]
fn travel_uses_diagonals_and_legacy_games_reject_them() {
    let (mut game, actor) = game(100);
    game.refresh_navigation();
    let route = game.travel_route(actor, cell(4, 0)).unwrap();
    assert_eq!(route.len(), 2);
    assert!(route.iter().all(|s| s.direction == Direction::NorthEast));
    let mut legacy = Game::two_room(42);
    let actor = legacy
        .spawn_actor(cell(2, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(
        legacy.act(actor, Action::Move(Direction::NorthEast)),
        Err(GameError::Blocked)
    );
}

#[test]
fn actors_and_closed_doors_block_sides_but_items_do_not() {
    let (mut game, actor) = game(100);
    game.spawn_actor(cell(2, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.place_door(cell(3, 2), false).unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::NorthEast)),
        Err(GameError::Blocked)
    );
    assert_eq!(game, before);
    let (mut game, actor) = self::game(100);
    game.set_wall(cell(2, 1), true).unwrap();
    game.place_item(cell(3, 2), "token".into()).unwrap();
    game.act(actor, Action::Move(Direction::NorthEast)).unwrap();
}

#[test]
fn weighted_travel_prefers_cardinal_route_over_equal_step_zigzag() {
    let (mut game, actor) = game(100);
    game.refresh_navigation();
    let route = game.travel_route(actor, cell(4, 2)).unwrap();
    assert_eq!(route.len(), 2);
    assert!(route.iter().all(|s| s.direction == Direction::East));
    // Hidden/current changes must not rewrite remembered plans before disclosure.
    game.set_wall(cell(3, 2), true).unwrap();
    assert_eq!(game.travel_route(actor, cell(4, 2)).unwrap(), route);
    assert_eq!(
        game.act(actor, Action::Move(route[0].direction)),
        Err(GameError::Blocked)
    );
    game.refresh_navigation();
    let route = game.travel_route(actor, cell(4, 2)).unwrap();
    assert_eq!(route.len(), 2);
    assert!(route.iter().all(|s| s.direction.components().is_some()));
}

#[test]
fn minimum_ticks_can_require_more_steps() {
    let mut world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "weighted routes".into(),
            bounds: Extent::new(7, 7, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let walls = [
        (6, 0),
        (3, 1),
        (1, 4),
        (2, 3),
        (4, 5),
        (5, 6),
        (2, 2),
        (6, 6),
    ];
    for (x, y) in walls {
        world.set_wall(cell(x, y), true).unwrap();
    }
    let mut game = Game::new(world, 42);
    game.enable_diagonals();
    let actor = game
        .spawn_actor(cell(0, 3), NonZeroU64::new(100).unwrap())
        .unwrap();
    // Explicitly disclose the map; the planner must not read undiscovered edges.
    for x in 0..7 {
        for y in 0..7 {
            if !walls.contains(&(x, y)) {
                game.teleport(actor, cell(x, y)).unwrap();
                game.refresh_navigation();
            }
        }
    }
    game.teleport(actor, cell(0, 3)).unwrap();
    let route = game.travel_route(actor, cell(6, 3)).unwrap();
    assert_eq!(route.len(), 7);
    for step in route {
        game.act(actor, Action::Move(step.direction)).unwrap();
    }
    // Six diagonal steps cost 852; seven mixed steps cost only 826.
    assert_eq!(game.tick(), 826);
}

#[test]
fn door_approaches_do_not_disclose_hidden_corner_clearance() {
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "sight edge".into(),
            bounds: Extent::new(10, 10, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42);
    game.enable_diagonals();
    let actor = game
        .spawn_actor(cell(0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.place_door(cell(4, 4), false).unwrap();
    game.set_wall(cell(4, 3), true).unwrap();
    let before = game.observe(actor).unwrap();
    assert!(before
        .visible_cells
        .iter()
        .any(|c| c.location == cell(5, 3)));
    let door = before
        .visible_cells
        .iter()
        .find(|c| c.location == cell(4, 4))
        .unwrap();
    assert!(!door.door_approaches.contains(&cell(5, 3)));
    assert!(!before
        .visible_cells
        .iter()
        .any(|c| c.location == cell(5, 4)));
    game.set_wall(cell(5, 4), true).unwrap();
    assert_eq!(game.observe(actor).unwrap(), before);
}
