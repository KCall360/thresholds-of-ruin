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
        game.act(
            actor,
            Action::Take {
                item: tablet.id,
                quantity: None
            }
        ),
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

/// An actor's asset is in its own observation (for views of itself from
/// another angle) and in every view of it others get.
#[test]
fn an_actors_asset_is_disclosed_with_it_and_to_itself() {
    let mut game = Game::two_room_in_stone(1);
    let ticks = NonZeroU64::new(100).unwrap();
    let me = game.spawn_actor(cell(1, 1, 1, 0), ticks).unwrap();
    let other = game.spawn_actor(cell(1, 3, 1, 0), ticks).unwrap();
    game.set_actor_asset(me, Some("creature.delver".into()))
        .unwrap();
    game.set_actor_asset(other, Some("creature.rat".into()))
        .unwrap();
    let mine = game.observe(me).unwrap();
    assert_eq!(mine.asset.as_deref(), Some("creature.delver"));
    let seen = mine.visible_actors.iter().find(|a| a.id == other).unwrap();
    assert_eq!(seen.asset.as_deref(), Some("creature.rat"));
    assert_eq!(
        game.set_actor_asset(tor_simulation::ActorId(999), None),
        Err(GameError::UnknownActor)
    );
}

#[test]
fn lighting_combines_body_awareness_with_distant_visual_perception() {
    let mut game = Game::new(
        tor_world::World::new(
            vec![Region {
                id: RegionId(1),
                name: "dark hall".into(),
                bounds: Extent::new(40, 7, 4).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
    );
    let actor = game
        .spawn_actor(cell(1, 2, 3, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.set_body(
        actor,
        tor_simulation::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.set_region_light(RegionId(1), false).unwrap();
    game.set_cell_light(cell(1, 14, 3, 2), true).unwrap();
    let seen = game.scene(actor).unwrap();
    for z in 0..=3 {
        for y in 2..=4 {
            for x in 1..=3 {
                assert!(
                    seen.iter().any(|c| c.location == cell(1, x, y, z)),
                    "local {x},{y},{z}"
                );
            }
        }
    }
    assert!(seen.iter().any(|c| c.location == cell(1, 14, 3, 2)));
    assert!(!seen.iter().any(|c| c.location == cell(1, 8, 3, 2)));
    game.refresh_navigation();
    assert!(game.known_cells(actor).any(|c| c == cell(1, 14, 3, 2)));
    assert!(!game.known_cells(actor).any(|c| c == cell(1, 8, 3, 2)));
}
