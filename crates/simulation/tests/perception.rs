use std::num::NonZeroU64;
use tor_simulation::{Action, Game, GameError};
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId};

fn cell(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

#[test]
fn portal_sight_discloses_only_visible_locations_and_does_not_grant_reach_or_visit() {
    let mut game = Game::two_room(42);
    let actor = game
        .spawn_actor(cell(1, 3, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let view = game.observe(actor).unwrap();
    let tablet = view
        .ground_items
        .iter()
        .find(|item| item.name == "stone tablet")
        .unwrap();
    assert_eq!(tablet.location, cell(2, 2, 1, 0));
    assert!(!view
        .known_places
        .iter()
        .any(|place| place.id == RegionId(2)));
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Take(tablet.id)),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
    game.set_wall(cell(1, 4, 1, 0), true).unwrap();
    assert!(!game
        .observe(actor)
        .unwrap()
        .ground_items
        .iter()
        .any(|i| i.name == "stone tablet"));
}

#[test]
fn geometry_setup_is_atomic_and_vertical_movement_requires_an_explicit_link() {
    let mut game = Game::two_room(0);
    game.add_region(Region {
        id: RegionId(3),
        name: "Upper".into(),
        bounds: Extent::new(3, 3, 2).unwrap(),
    })
    .unwrap();
    let actor = game
        .spawn_actor(cell(3, 1, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::Up)),
        Err(GameError::Blocked)
    );
    assert!(game.set_wall(cell(3, 1, 1, 0), true).is_err());
    assert_eq!(game, before);
    game.connect(
        Passage {
            from: cell(3, 1, 1, 0),
            direction: Direction::Up,
            to: cell(3, 1, 1, 1),
        },
        0,
    )
    .unwrap();
    game.act(actor, Action::Move(Direction::Up)).unwrap();
    assert_eq!(game.observe(actor).unwrap().location, cell(3, 1, 1, 1));
    assert_eq!(game.tick(), 100);
}

#[test]
fn movement_keeps_the_observers_axes_when_crossing_a_rotated_join() {
    let mut game = Game::two_room(42);
    game.add_region(Region {
        id: RegionId(3),
        name: "Remote".into(),
        bounds: Extent::new(5, 5, 2).unwrap(),
    })
    .unwrap();
    game.connect_area(
        Passage {
            from: cell(1, 0, 0, 0),
            direction: Direction::North,
            to: cell(3, 0, 1, 1),
        },
        1,
        3,
        1,
    )
    .unwrap();
    let actor = game
        .spawn_actor(cell(1, 1, 1, 0), NonZeroU64::new(100).unwrap())
        .unwrap();
    let target = cell(3, 2, 2, 1);
    assert!(game
        .scene(actor)
        .unwrap()
        .iter()
        .any(|c| c.location == target && c.offset == Position { x: 0, y: -4, z: 0 }));
    for distance in 1..=4 {
        game.act(actor, Action::Move(Direction::North)).unwrap();
        assert!(game
            .scene(actor)
            .unwrap()
            .iter()
            .any(|c| c.location == target
                && c.offset
                    == Position {
                        x: 0,
                        y: distance - 4,
                        z: 0
                    }));
    }
    assert_eq!(game.observe(actor).unwrap().location, target);
    game.act(actor, Action::Move(Direction::South)).unwrap();
    assert_eq!(game.observe(actor).unwrap().location, cell(3, 1, 2, 1));
}
