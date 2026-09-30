use std::num::NonZeroU64;
use tor_simulation::{Action, Game, GameError};
use tor_world::{Location, Position, RegionId};

fn location(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}

#[test]
fn teleport_is_atomic_preserves_recovery_and_updates_knowledge() {
    let mut game = Game::two_room(42);
    let first = game
        .spawn_actor(location(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let second = game
        .spawn_actor(location(1, 2), NonZeroU64::new(75).unwrap())
        .unwrap();
    game.act(first, Action::Wait).unwrap();
    let before = game.clone();
    for destination in [location(1, 2), location(2, -1), location(99, 0)] {
        assert!(game.teleport(first, destination).is_err());
        assert_eq!(game, before);
    }
    game.teleport(first, location(2, 1)).unwrap();
    assert_eq!(game.tick(), 0);
    assert_eq!(game.next_actor(), Some(second));
    assert_eq!(game.act(first, Action::Wait), Err(GameError::NotActorsTurn));
    assert_eq!(game.observe(first).unwrap().known_places.len(), 2);
    game.act(second, Action::Wait).unwrap();
    assert_eq!(game.tick(), 75);
    game.act(second, Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    assert_eq!(game.next_actor(), Some(first));
}
