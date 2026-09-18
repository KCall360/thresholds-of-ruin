use std::num::NonZeroU64;

use tor_simulation::{Action, ActorId, Game, GameError, ItemId, OutcomeKind};
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

fn start() -> Location {
    Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    }
}

#[test]
fn seeded_actor_can_observe_take_and_cross_between_rooms() {
    let mut game = Game::two_room(42);
    let actor = game
        .spawn_actor(start(), NonZeroU64::new(100).unwrap())
        .unwrap();
    let initial = game.observe(actor).unwrap();
    assert_eq!(initial.region.name, "Entry chamber");
    assert_eq!(initial.ground_items.len(), 2);
    assert!(initial.inventory.is_empty());
    assert_eq!(initial.known_places.len(), 1);
    let item = initial.ground_items[0].id;
    let taken = game.act(actor, Action::Take(item)).unwrap();
    assert_eq!(taken.kind, OutcomeKind::Taken { item });
    assert_eq!((taken.at_tick, taken.next_tick), (0, 50));
    assert_eq!(game.observe(actor).unwrap().inventory[0].id, item);
    assert!(game
        .observe(actor)
        .unwrap()
        .ground_items
        .iter()
        .all(|i| i.id != item));

    let mut event_times = vec![taken.at_tick];
    for _ in 0..4 {
        let outcome = game.act(actor, Action::Move(Direction::East)).unwrap();
        event_times.push(outcome.at_tick);
    }
    let arrived = game.observe(actor).unwrap();
    assert_eq!(arrived.region.name, "Gallery");
    assert_eq!(arrived.location.region, RegionId(2));
    assert_eq!(arrived.location.position, Position { x: 0, y: 1, z: 0 });
    assert_eq!(arrived.tick, 450);
    assert_eq!(arrived.known_places.len(), 2);
    assert_eq!(arrived.inventory[0].id, item);
    assert_eq!(event_times, [0, 50, 150, 250, 350]);
    game.act(actor, Action::Move(Direction::West)).unwrap();
    assert_eq!(game.observe(actor).unwrap().location.position.x, 4);
}

#[test]
fn queries_and_invalid_actions_leave_the_entire_simulation_unchanged() {
    let mut game = Game::two_room(1);
    let actor = game
        .spawn_actor(start(), NonZeroU64::new(100).unwrap())
        .unwrap();
    let original = game.clone();
    game.observe(actor).unwrap();
    game.observe(actor).unwrap();
    assert_eq!(
        game.act(actor, Action::Move(Direction::Down)),
        Err(GameError::Blocked)
    );
    assert_eq!(
        game.act(actor, Action::Take(ItemId(999))),
        Err(GameError::ItemUnavailable)
    );
    // A real but undisclosed item must produce the same error as a guessed ID.
    assert_eq!(
        game.act(actor, Action::Take(ItemId(2))),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, original);
}

#[test]
fn identical_seed_and_actions_reproduce_state_and_event_trace() {
    fn run(seed: u64) -> (Game, Vec<tor_simulation::ActionOutcome>) {
        let mut game = Game::two_room(seed);
        let actor = game
            .spawn_actor(start(), NonZeroU64::new(100).unwrap())
            .unwrap();
        let item = game.observe(actor).unwrap().ground_items[0].id;
        let trace = [
            Action::Take(item),
            Action::Move(Direction::East),
            Action::Wait,
        ]
        .into_iter()
        .map(|action| game.act(actor, action).unwrap())
        .collect();
        (game, trace)
    }
    assert_eq!(run(42), run(42));
    assert_ne!(run(42).0, run(43).0);
}

#[test]
fn actors_have_independent_inventories_and_deterministic_variable_timing() {
    let mut game = Game::two_room(42);
    let first = game
        .spawn_actor(start(), NonZeroU64::new(100).unwrap())
        .unwrap();
    let second = game
        .spawn_actor(
            Location {
                position: Position { x: 2, y: 1, z: 0 },
                ..start()
            },
            NonZeroU64::new(40).unwrap(),
        )
        .unwrap();
    assert_eq!(game.next_actor(), Some(first));
    let before = game.clone();
    assert_eq!(
        game.act(second, Action::Wait),
        Err(GameError::NotActorsTurn)
    );
    assert_eq!(game, before);
    let item = game.observe(first).unwrap().ground_items[0].id;
    let pickup = game.act(first, Action::Take(item)).unwrap();
    assert_eq!((pickup.next_actor, pickup.next_tick), (second, 0));
    assert!(game.observe(second).unwrap().inventory.is_empty());
    assert!(game
        .observe(second)
        .unwrap()
        .ground_items
        .iter()
        .all(|i| i.id != item));
    let wait = game.act(second, Action::Wait).unwrap();
    assert_eq!((wait.next_actor, wait.next_tick), (second, 40));
    let wait = game.act(second, Action::Wait).unwrap();
    assert_eq!((wait.next_actor, wait.next_tick), (first, 50));
}

#[test]
fn actors_cannot_spawn_or_move_into_an_occupied_cell() {
    let mut game = Game::two_room(0);
    let pace = NonZeroU64::new(100).unwrap();
    let first = game.spawn_actor(start(), pace).unwrap();
    assert_eq!(game.spawn_actor(start(), pace), Err(GameError::Occupied));
    game.spawn_actor(
        Location {
            position: Position { x: 2, y: 1, z: 0 },
            ..start()
        },
        pace,
    )
    .unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(first, Action::Move(Direction::East)),
        Err(GameError::Occupied)
    );
    assert_eq!(game, before);
}

#[test]
fn time_overflow_is_rejected_without_partially_moving_an_actor() {
    let mut game = Game::two_room(u64::MAX);
    let actor = game
        .spawn_actor(start(), NonZeroU64::new(u64::MAX).unwrap())
        .unwrap();
    game.act(actor, Action::Wait).unwrap();
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::East)),
        Err(GameError::TimeExhausted)
    );
    assert_eq!(game, before);
}

#[test]
fn visible_items_still_require_reach_and_cannot_be_taken_twice() {
    let mut game = Game::two_room(0);
    let actor = game
        .spawn_actor(
            Location {
                position: Position { x: 0, y: 1, z: 0 },
                ..start()
            },
            NonZeroU64::new(1).unwrap(),
        )
        .unwrap();
    let item = game.observe(actor).unwrap().ground_items[0].id;
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Take(item)),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    let taken = game.act(actor, Action::Take(item)).unwrap();
    assert_eq!((taken.at_tick, taken.next_tick), (1, 2));
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Take(item)),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
    assert_eq!(game.observe(actor).unwrap().inventory.len(), 1);
}

#[test]
fn actors_can_see_across_boundaries_without_counting_them_as_visits() {
    let mut game = Game::two_room(0);
    let pace = NonZeroU64::new(100).unwrap();
    let entry = game.spawn_actor(start(), pace).unwrap();
    let gallery = game
        .spawn_actor(
            Location {
                region: RegionId(2),
                ..start()
            },
            pace,
        )
        .unwrap();
    let left = game.observe(entry).unwrap();
    let right = game.observe(gallery).unwrap();
    assert_eq!(left.visible_actors[0].id, gallery);
    assert_eq!(right.visible_actors[0].id, entry);
    assert_eq!(
        left.known_places
            .iter()
            .map(|place| place.id)
            .collect::<Vec<_>>(),
        [RegionId(1)]
    );
    assert_eq!(
        right
            .known_places
            .iter()
            .map(|place| place.id)
            .collect::<Vec<_>>(),
        [RegionId(2)]
    );
    assert_eq!(
        left.ground_items
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["copper token", "stone tablet"]
    );
    assert_eq!(
        right
            .ground_items
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["copper token", "stone tablet"]
    );
    let mut other_seed = Game::two_room(1);
    let actor = other_seed.spawn_actor(start(), pace).unwrap();
    assert_eq!(
        other_seed.observe(actor).unwrap().ground_items[0].name,
        "silver token"
    );
}

#[test]
fn equal_time_ties_are_stable_and_new_actors_join_at_the_current_tick() {
    let mut game = Game::two_room(0);
    let pace = NonZeroU64::new(100).unwrap();
    let first = game.spawn_actor(start(), pace).unwrap();
    game.act(first, Action::Wait).unwrap();
    let second = game
        .spawn_actor(
            Location {
                position: Position { x: 2, y: 1, z: 0 },
                ..start()
            },
            pace,
        )
        .unwrap();
    assert_eq!(game.next_actor(), Some(first));
    assert_eq!(game.act(first, Action::Wait).unwrap().next_actor, second);
    let outcome = game.act(second, Action::Wait).unwrap();
    assert_eq!(
        (outcome.at_tick, outcome.next_tick, outcome.next_actor),
        (100, 200, first)
    );
}

#[test]
fn invalid_actor_and_spawn_requests_leave_state_unchanged() {
    let mut game = Game::two_room(0);
    let before = game.clone();
    assert_eq!(game.next_actor(), None);
    assert_eq!(game.observe(ActorId(99)), Err(GameError::UnknownActor));
    assert_eq!(
        game.act(ActorId(99), Action::Wait),
        Err(GameError::UnknownActor)
    );
    assert_eq!(
        game.spawn_actor(
            Location {
                region: RegionId(99),
                ..start()
            },
            NonZeroU64::new(100).unwrap()
        ),
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, before);
}

#[test]
fn a_passage_can_return_to_the_actors_own_cell() {
    let location = Location {
        region: RegionId(1),
        position: Position { x: 0, y: 0, z: 0 },
    };
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "Loop".into(),
            bounds: Extent::new(1, 1, 1).unwrap(),
        }],
        vec![Passage {
            from: location,
            direction: Direction::East,
            to: location,
        }],
    )
    .unwrap();
    let mut game = Game::new(world, 0);
    let actor = game
        .spawn_actor(location, NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(
        game.act(actor, Action::Move(Direction::East)).unwrap().kind,
        OutcomeKind::Moved {
            from: location,
            to: location
        }
    );
    assert_eq!(game.tick(), 100);
}
