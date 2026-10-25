//! Native physical-search controls; the paid/residency Service witnesses are separate.
use super::contact_trace::{ContactEnd, ContactTraceError};
use crate::{ActorId, Game};
use std::num::NonZeroU64;
use tor_world::{Direction, Extent, Location, Position, Region, RegionId, World};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn line() -> Game {
    let mut game = super::ray_tests::fixture_with(|_, _, _| {});
    *game.world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "physical contact".into(),
            bounds: Extent::new(130, 3, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    game.actors.get_mut(&ActorId(1)).unwrap().location = at(1);
    game.actors.get_mut(&ActorId(2)).unwrap().location = at(110);
    assert!(game.world.walkable(at(1)) && game.world.walkable(at(110)));
    game
}
#[test]
fn physical_first_body_exceeds_old_limits_and_noncombat_body_intercepts() {
    let mut game = line();
    let trace = game
        .trace_first_body(ActorId(1), at(1), Direction::East, &[at(1)], 65536)
        .unwrap();
    assert_eq!(trace.end, ContactEnd::Body(ActorId(2)));
    let body = game
        .spawn_actor(at(80), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(game.creature(body).is_none() && game.actors[&body].alive());
    let before = game.clone();
    let trace = game
        .trace_first_body(ActorId(1), at(1), Direction::East, &[at(1)], 65536)
        .unwrap();
    assert_eq!(trace.end, ContactEnd::Body(body));
    assert_eq!(game, before, "search does not publish, roll or activate");
}
#[test]
fn contact_work_limit_is_an_error_not_a_shortened_success() {
    let game = line();
    let before = game.clone();
    assert_eq!(
        game.trace_first_body(ActorId(1), at(1), Direction::East, &[at(1)], 108),
        Err(ContactTraceError::WorkLimit)
    );
    assert_eq!(
        game.trace_first_body(ActorId(1), at(1), Direction::East, &[at(1)], 109)
            .unwrap()
            .end,
        ContactEnd::Body(ActorId(2))
    );
    assert_eq!(game, before);
}
#[test]
fn contact_obstruction_never_bounces_back_to_the_source() {
    let mut game = line();
    game.world.set_wall(at(2), true).unwrap();
    assert_eq!(
        game.trace_first_body(ActorId(1), at(1), Direction::East, &[at(1)], 65536)
            .unwrap()
            .end,
        ContactEnd::Obstruction
    );
}

#[test]
fn contact_operational_ceiling_is_exact_and_does_not_claim_full_world_reach() {
    let mut game = line();
    *game.world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "native work-bound witness, not authored package".into(),
            bounds: Extent::new(65540, 3, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().location = at(65537);
    assert_eq!(
        game.trace_first_body(
            ActorId(1),
            at(1),
            Direction::East,
            &[at(1)],
            super::contact_trace::MAX_CONTACT_STATES
        )
        .unwrap()
        .end,
        ContactEnd::Body(ActorId(2))
    );
    game.actors.get_mut(&ActorId(2)).unwrap().location = at(65538);
    let before = game.clone();
    assert_eq!(
        game.trace_first_body(
            ActorId(1),
            at(1),
            Direction::East,
            &[at(1)],
            super::contact_trace::MAX_CONTACT_STATES
        ),
        Err(ContactTraceError::WorkLimit)
    );
    assert_eq!(game, before);
}
