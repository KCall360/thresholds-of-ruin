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
    // Floors and ceilings are seen solid cells in the player's column.
    let column = |view: &tor_simulation::Observation, z: i32| {
        view.visible_cells
            .iter()
            .find(|c| {
                c.location
                    == Location {
                        position: Position {
                            z,
                            ..location.position
                        },
                        ..location
                    }
            })
            .map(|c| (c.wall, c.material))
    };
    assert_eq!(column(&before, -1), Some((true, "stone")));
    assert_eq!(column(&before, 1), Some((false, "")));
    assert_eq!(column(&before, 2), Some((true, "stone")));
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
    // With a hole in the ceiling, the column above is open and nothing past
    // the chamber's storage is shown.
    let after = game.observe(actor).unwrap();
    assert_eq!(column(&after, 2), Some((false, "")));
    assert_eq!(column(&after, 3), None);
}
