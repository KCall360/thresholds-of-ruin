//! Captured destinations are live references even when their source is frozen.
use super::*;
fn stair_step() -> (crate::Game, crate::IntentionId, tor_world::Location) {
    let mut game = guarded_step_game(100);
    game.add_region(tor_world::Region {
        id: tor_world::RegionId(2),
        name: "step landing".into(),
        bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
    })
    .unwrap();
    let destination = tor_world::Location {
        region: tor_world::RegionId(2),
        position: tor_world::Position { x: 2, y: 2, z: 0 },
    };
    game.connect(
        tor_world::Passage {
            from: cell(2, 2, 0),
            direction: tor_world::Direction::Up,
            to: destination,
        },
        0,
    )
    .unwrap();
    let intention = start_step(&mut game, "up");
    (game, intention, destination)
}
#[test]
fn a_frozen_prepared_self_step_keeps_its_captured_destination_loaded() {
    let (mut game, intention, destination) = stair_step();
    let frozen = game.preparation(crate::ActorId(1)).unwrap().plan.clone();
    let before = game.clone();
    let mut records = crate::MemoryRecords::default();
    assert_eq!(
        game.apply_region_transition(
            &crate::RegionTransition {
                active: Default::default(),
                loaded: [tor_world::RegionId(1)].into(),
            },
            &mut records
        ),
        Err(crate::TransitionError::MustBeLoaded(destination.region))
    );
    assert_eq!(game, before, "rejected streaming is atomic");
    let all = [tor_world::RegionId(1), destination.region].into();
    let (frozen_regions, _) = game
        .transition_regions(
            &crate::RegionTransition {
                active: Default::default(),
                loaded: all,
            },
            &mut records,
        )
        .unwrap();
    assert!(frozen_regions.active.is_empty());
    game.tick = 1000;
    game.resolve_preparations().unwrap();
    assert_eq!(game.actors[&crate::ActorId(1)].location, cell(2, 2, 0));
    assert_eq!(game.preparation(crate::ActorId(1)).unwrap().plan, frozen);
    assert_eq!(
        game.preparation(crate::ActorId(1)).unwrap().intention,
        Some(intention)
    );
    game.transition_regions(
        &crate::RegionTransition {
            active: [tor_world::RegionId(1), destination.region].into(),
            loaded: [tor_world::RegionId(1), destination.region].into(),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 1100);
    game.tick = 1100;
    game.resolve_preparations().unwrap();
    assert_eq!(game.actors[&crate::ActorId(1)].location, destination);
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 1300);
}
#[test]
fn paused_self_step_keeps_the_original_destination_and_claim_through_checkpoint() {
    let (mut game, intention, destination) = stair_step();
    game.pause_preparation(crate::ActorId(1)).unwrap();
    let frozen = game.preparation(crate::ActorId(1)).unwrap().clone();
    let mut records = crate::MemoryRecords::default();
    let (sets, _) = game
        .transition_regions(
            &crate::RegionTransition {
                active: [tor_world::RegionId(1)].into(),
                loaded: [tor_world::RegionId(1)].into(),
            },
            &mut records,
        )
        .unwrap();
    assert!(sets.active.contains(&destination.region));
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let restored =
        crate::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.preparation(crate::ActorId(1)), Some(&frozen));
    assert_eq!(
        restored
            .preparation(crate::ActorId(1))
            .unwrap()
            .plan
            .as_ability()
            .unwrap()
            .step
            .unwrap()
            .destination(),
        destination
    );
    assert_eq!(
        restored
            .creature(crate::ActorId(1))
            .unwrap()
            .costs()
            .reservation(intention)
            .unwrap()
            .entries()
            .find(|cost| cost.resource == crate::resources::Resource::Stamina)
            .unwrap()
            .resolution,
        1
    );
}
