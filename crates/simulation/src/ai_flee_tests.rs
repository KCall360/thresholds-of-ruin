use super::*;
use crate::{behavior::Engagement, perception::ActorReferenceKind, test_creatures};
use std::{collections::BTreeSet, num::NonZeroU64};
use tor_world::{Extent, Passage, Position, Region, RegionId, World};

#[derive(Clone, Copy, Debug)]
enum Corridor {
    Unsplit,
    Split,
    Rotated,
}

impl Corridor {
    fn location(self, index: i32) -> Location {
        let (region, x, y) = match self {
            Self::Unsplit => (1, index, 0),
            Self::Split if index >= 3 => (2, index - 3, 0),
            Self::Rotated if index >= 3 => (2, 0, index - 3),
            _ => (1, index, 0),
        };
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }

    fn game(self, actor_index: i32, threat_index: i32) -> (Game, ActorId, ActorId) {
        self.game_with_reverse(actor_index, threat_index, true)
    }

    fn game_with_reverse(
        self,
        actor_index: i32,
        threat_index: i32,
        reverse: bool,
    ) -> (Game, ActorId, ActorId) {
        let mut world = World::new(vec![], vec![]).unwrap();
        let extents = match self {
            Self::Unsplit => vec![(1, 6, 1)],
            Self::Split => vec![(1, 3, 1), (2, 3, 1)],
            Self::Rotated => vec![(1, 3, 1), (2, 1, 3)],
        };
        for (id, width, depth) in extents {
            world
                .add_region(Region {
                    id: RegionId(id),
                    name: "corridor".into(),
                    bounds: Extent::new(width, depth, 1).unwrap(),
                })
                .unwrap();
        }
        if !matches!(self, Self::Unsplit) {
            let turns = if matches!(self, Self::Rotated) { 1 } else { 0 };
            world
                .connect(
                    Passage {
                        from: self.location(2),
                        direction: Direction::East,
                        to: self.location(3),
                    },
                    turns,
                )
                .unwrap();
            if reverse {
                world
                    .connect(
                        Passage {
                            from: self.location(3),
                            direction: Direction::West.rotated(turns),
                            to: self.location(2),
                        },
                        tor_world::inverse_rotation(turns),
                    )
                    .unwrap();
            }
        }
        let mut species = test_creatures::species();
        species.behavior.memory_ticks = 50;
        let content = test_creatures::content_with(species.clone(), vec![]);
        let mut game = Game::new(world, 42, content);
        let threat = game
            .spawn_actor(self.location(threat_index), NonZeroU64::new(100).unwrap())
            .unwrap();
        let actor = game
            .spawn_actor(self.location(actor_index), NonZeroU64::new(100).unwrap())
            .unwrap();
        for (id, faction) in [(threat, "hero"), (actor, "foe")] {
            test_creatures::configure(&mut game, id, faction, species.clone());
        }
        game.configure_run(
            threat,
            BTreeSet::from([threat]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(
            actor,
            AiProfile {
                flee_percent: 100,
                ..AiProfile::default()
            },
        )
        .unwrap();
        game.actors.get_mut(&actor).unwrap().behavior.engagement = Some(Engagement {
            target: threat,
            reference: ActorReferenceKind::Creature,
            location: self.location(threat_index),
            last_seen: 0,
        });
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        assert!(view.visible_actors.iter().any(|other| other.id == threat));
        assert_eq!(game.actors[&actor].body.cells.len(), 1);
        assert!(game.actors[&actor].locomotion().erratic.is_none());
        assert!(game.preparation(actor).is_none());
        (game, actor, threat)
    }
}

#[test]
fn cornered_flee_does_not_approach_a_threat_across_region_partitions() {
    for corridor in [Corridor::Unsplit, Corridor::Split, Corridor::Rotated] {
        let (game, actor, _) = corridor.game(0, 5);
        let route = game.travel_route(actor, corridor.location(5)).unwrap();
        assert_eq!(route.len(), 5, "{corridor:?}");
        let legal: Vec<_> = Direction::HORIZONTAL
            .into_iter()
            .chain([Direction::Up, Direction::Down])
            .filter(|direction| game.actor_translation(actor, *direction).is_some())
            .collect();
        assert_eq!(
            legal,
            [Direction::East],
            "the sole step approaches the threat"
        );
        let before = game.clone();
        let decision = game.choose_ai(actor).unwrap();
        assert_eq!(decision.ai.state, State::Flee, "{corridor:?}");
        assert_eq!(decision.action, Action::Wait, "{corridor:?}");
        assert_eq!(game, before, "a decision must not mutate actor knowledge");
    }
}

#[test]
fn fleeing_retains_a_disclosed_retreat_through_a_rotated_portal() {
    for corridor in [Corridor::Unsplit, Corridor::Split, Corridor::Rotated] {
        let (game, actor, _) = corridor.game(2, 0);
        assert_eq!(
            game.travel_route(actor, corridor.location(0))
                .unwrap()
                .len(),
            2
        );
        let (destination, orientation) = game.actor_translation(actor, Direction::East).unwrap();
        assert_eq!(destination, corridor.location(3));
        assert_eq!(
            orientation,
            if matches!(corridor, Corridor::Rotated) {
                1
            } else {
                0
            }
        );
        let return_route = game
            .route_search_from(actor, destination, orientation)
            .unwrap()
            .route(corridor.location(0))
            .unwrap();
        assert_eq!(return_route.len(), 3);
        assert_eq!(return_route[0].direction, Direction::West);
        let before = game.clone();
        let decision = game.choose_ai(actor).unwrap();
        assert_eq!(decision.ai.state, State::Flee, "{corridor:?}");
        assert_eq!(
            decision.action,
            Action::Move(Direction::East),
            "{corridor:?}"
        );
        assert_eq!(game, before);
    }
}

#[test]
fn fleeing_can_escape_through_a_disclosed_one_way_portal() {
    let corridor = Corridor::Rotated;
    let (game, actor, _) = corridor.game_with_reverse(2, 0, false);
    assert_eq!(
        game.travel_route(actor, corridor.location(0))
            .unwrap()
            .len(),
        2
    );
    let (at, orientation) = game.actor_translation(actor, Direction::East).unwrap();
    assert_eq!(
        game.route_search_from(actor, at, orientation)
            .unwrap()
            .route(corridor.location(0)),
        Err(GameError::Blocked),
        "the remembered graph has no reverse link"
    );
    assert_eq!(
        game.choose_ai(actor).unwrap().action,
        Action::Move(Direction::East)
    );
}

#[test]
fn fleeing_does_not_invent_distance_for_an_unknown_current_route() {
    let corridor = Corridor::Split;
    let (mut game, actor, _) = corridor.game(0, 5);
    // Knowing the cells does not license an unlearned connection between them.
    game.navigation
        .get_mut(&actor)
        .unwrap()
        .edges
        .remove(&(corridor.location(2), Direction::East));
    assert_eq!(
        game.travel_route(actor, corridor.location(5)),
        Err(GameError::Blocked)
    );
    assert!(game.actor_translation(actor, Direction::East).is_some());
    let before = game.clone();
    let decision = game.choose_ai(actor).unwrap();
    assert_eq!(decision.ai.state, State::Flee);
    assert_eq!(decision.action, Action::Wait);
    assert_eq!(
        game, before,
        "a distance query must not learn the live portal"
    );
}

#[test]
fn fleeing_does_not_select_a_blocked_portal_retreat() {
    let corridor = Corridor::Rotated;
    let (mut game, actor, _) = corridor.game(2, 0);
    game.set_wall(corridor.location(3), true).unwrap();
    assert!(game.actor_translation(actor, Direction::East).is_none());
    assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
}

#[test]
fn cornered_flee_intention_recomputes_the_same_wait_after_checkpoint() {
    let corridor = Corridor::Split;
    let (mut game, actor, threat) = corridor.game(0, 5);
    game.act(threat, Action::Wait).unwrap();
    assert_eq!(game.next_ai_action(), Some((actor, Action::Wait)));
    game.admit_ai_intention(actor).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for candidate in [&mut game, &mut restored] {
        let execution = candidate.execute_next_intention().unwrap();
        assert_eq!(execution.action, Some(Action::Wait));
        execution.outcome.unwrap();
        assert_eq!(
            candidate.observe(actor).unwrap().location,
            corridor.location(0)
        );
    }
    assert_eq!(game, restored);
}
