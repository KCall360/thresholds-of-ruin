use std::num::NonZeroU64;
use tor_simulation::{Action, Game};
use tor_world::{Direction, Location, Position, RegionId};

#[test]
fn real_surfaces_are_disclosed_and_empty_headroom_does_not_allow_flying() {
    let mut game = Game::two_room_in_stone(42);
    let location = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(location, NonZeroU64::new(100).unwrap())
        .unwrap();
    let before = game.observe(actor).unwrap();
    let here = before
        .visible_cells
        .iter()
        .find(|c| c.location == location)
        .unwrap();
    assert_eq!(here.floor, Some(("stone", 1)));
    assert_eq!(here.ceiling, Some(("stone", 2)));
    assert!(before
        .visible_cells
        .iter()
        .any(|c| c.wall && c.location.position.x == -1));
    assert!(game.act(actor, Action::Move(Direction::Up)).is_err());
    assert!(game.act(actor, Action::Move(Direction::Down)).is_err());
    assert_eq!(game.observe(actor).unwrap(), before);
    game.set_wall(
        Location {
            position: Position { x: 1, y: 1, z: 2 },
            ..location
        },
        false,
    )
    .unwrap();
    let after = game.observe(actor).unwrap();
    assert_eq!(
        after
            .visible_cells
            .iter()
            .find(|c| c.location == location)
            .unwrap()
            .ceiling,
        None
    );
}
