use std::num::NonZeroU64;
use tor_simulation::{Action, Game, GameError};
use tor_world::{Direction, Extent, Location, Position, Region, RegionId, World};

fn cell(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 0, z: 0 },
    }
}

#[test]
fn interior_door_blocks_sight_and_movement_and_actions_are_atomic() {
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "Hall".into(),
            bounds: Extent::new(6, 1, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42);
    let actor = game
        .spawn_actor(cell(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let door = game.place_door(cell(2), false).unwrap();
    game.place_item(cell(4), "stone tablet".into()).unwrap();
    assert!(game.observe(actor).unwrap().ground_items.is_empty());
    assert!(
        !game
            .observe(actor)
            .unwrap()
            .visible_cells
            .iter()
            .find(|c| c.location == cell(2))
            .unwrap()
            .wall
    );
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::East)),
        Err(GameError::Blocked)
    );
    assert_eq!(
        game.act(
            actor,
            Action::SetDoor {
                door: 999,
                open: true
            }
        ),
        Err(GameError::DoorUnavailable)
    );
    assert_eq!(game, before);
    game.act(actor, Action::SetDoor { door, open: true })
        .unwrap();
    assert_eq!(game.tick(), 100);
    assert_eq!(game.observe(actor).unwrap().ground_items.len(), 1);
    let before = game.clone();
    assert!(game
        .act(actor, Action::SetDoor { door, open: true })
        .is_err());
    assert_eq!(game, before);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let before = game.clone();
    assert!(game
        .act(actor, Action::SetDoor { door, open: false })
        .is_err());
    assert_eq!(game, before);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.act(actor, Action::SetDoor { door, open: false })
        .unwrap();
    assert_eq!(game.tick(), 400);
    assert!(game.set_wall(cell(2), true).is_err());
    assert!(game.place_door(cell(2), true).is_err());
}

#[test]
fn ground_items_prevent_closing_and_distant_doors_cannot_be_manipulated() {
    let mut game = Game::two_room(42);
    let location = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(location(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let door = game.place_door(location(3), true).unwrap();
    let before = game.clone();
    assert!(game
        .act(actor, Action::SetDoor { door, open: false })
        .is_err());
    assert_eq!(game, before);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.place_item(location(3), "token".into()).unwrap();
    let before = game.clone();
    assert!(game
        .act(actor, Action::SetDoor { door, open: false })
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn a_rotated_join_door_is_reachable_without_crossing() {
    use tor_world::Passage;
    let at = |r, x, y, z| Location {
        region: RegionId(r),
        position: Position { x, y, z },
    };
    let mut world = World::new(
        vec![
            Region {
                id: RegionId(1),
                name: "A".into(),
                bounds: Extent::new(2, 1, 1).unwrap(),
            },
            Region {
                id: RegionId(2),
                name: "B".into(),
                bounds: Extent::new(1, 4, 1).unwrap(),
            },
        ],
        vec![],
    )
    .unwrap();
    world
        .connect(
            Passage {
                from: at(1, 1, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            1,
        )
        .unwrap();
    let mut game = Game::new(world, 0);
    let actor = game
        .spawn_actor(at(1, 1, 0, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let door = game.place_door(at(2, 0, 0, 0), false).unwrap();
    game.place_item(at(2, 0, 2, 0), "tablet".into()).unwrap();
    assert!(game.observe(actor).unwrap().ground_items.is_empty());
    game.act(actor, Action::SetDoor { door, open: true })
        .unwrap();
    assert_eq!(game.observe(actor).unwrap().ground_items.len(), 1);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.observe(actor).unwrap().location, at(2, 0, 1, 0));
}
