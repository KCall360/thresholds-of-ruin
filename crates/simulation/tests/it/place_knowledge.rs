use std::num::NonZeroU64;
use tor_simulation::{checkpoint::SharedState, Game};
use tor_world::{Location, Position, RegionId};

#[test]
fn first_sight_names_are_actor_owned_stale_and_checkpointed() {
    let mut game = Game::two_room_with_place_hints(42);
    let start = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(start, NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    let known: Vec<_> = game.remembered_places(actor).collect();
    assert_eq!(known.len(), 2);
    assert!(known.iter().all(|(_, name)| !name.starts_with("Place ")));
    let location = known[0].0;
    let old_name = known[0].1.to_owned();
    assert!(game
        .rename_place(actor, location, "Hearth of Echoes")
        .is_ok());
    assert!(game.rename_place(actor, location, "\n").is_err());
    game.set_place_hint(location, false).unwrap();
    game.refresh_navigation();
    assert_eq!(
        game.remembered_places(actor)
            .find(|(cell, _)| *cell == location)
            .unwrap()
            .1,
        "Hearth of Echoes"
    );
    let mut shared = SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let recovered = Game::restore_checkpoint(checkpoint, &shared).unwrap();
    assert_eq!(game, recovered);
    assert_ne!(old_name, "Hearth of Echoes");
}

#[test]
fn unseen_places_and_other_characters_names_are_not_learned() {
    let mut game = Game::two_room_with_place_hints(42);
    let cell = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    };
    let first = game
        .spawn_actor(cell(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let second = game
        .spawn_actor(cell(0), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(game.remembered_places(first).count(), 0);
    // Observation is a read, never a knowledge mutation.
    game.observe(first).unwrap();
    assert_eq!(game.remembered_places(first).count(), 0);
    game.refresh_navigation();
    let (anchor, original) = game
        .remembered_places(first)
        .next()
        .map(|(cell, name)| (cell, name.to_owned()))
        .unwrap();
    game.rename_place(first, anchor, "My refuge").unwrap();
    assert_eq!(
        game.remembered_places(second)
            .find(|(cell, _)| *cell == anchor)
            .unwrap()
            .1,
        original
    );
    assert!(game
        .rename_place(first, cell(1), "Undiscovered anchor")
        .is_err());
    let before = game.clone();
    game.refresh_navigation();
    assert_eq!(game, before);
}
