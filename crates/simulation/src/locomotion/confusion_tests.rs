//! Confusion preserves admitted commands, physical geometry and paid work.
use super::tests::{at, fixture, fixture_with_perception, flight, modifier};
use super::*;
use crate::{Action, GameError, IntentionOrigin, OutcomeKind};
use tor_world::{Direction, Location, Position};

fn confused(seed: u64) -> (crate::Game, crate::ActorId) {
    let (mut game, actor) = fixture(&[LocomotionValue::Confused {
        chance: Some(MovementChance {
            numerator: 4,
            denominator: 5,
        }),
    }]);
    game.rules_rng = seed;
    (game, actor)
}

#[test]
fn confused_draws_once_from_fixed_actor_local_nine_cells() {
    // Pinned SplitMix fixtures: seed 1 selects South, 3 West, 4 stay, 8 NE.
    for (seed, dx, dy, after) in [
        (1, 0, 1, 0x3c6ef372fe94f82b),
        (3, -1, 0, 0x3c6ef372fe94f82d),
        (4, 0, 0, 0x3c6ef372fe94f82e),
        (8, 1, -1, 0x3c6ef372fe94f832),
    ] {
        let (mut game, actor) = confused(seed);
        let original = game.actors[&actor].location;
        let outcome = game.act(actor, Action::Move(Direction::East)).unwrap();
        assert_eq!(
            game.actors[&actor].location,
            Location {
                position: Position {
                    x: original.position.x + dx,
                    y: original.position.y + dy,
                    ..original.position
                },
                ..original
            }
        );
        assert_eq!(game.rules_rng, after);
        assert_eq!(game.actors[&actor].ready_at, 100);
        assert_eq!(
            matches!(outcome.kind, OutcomeKind::Waited),
            dx == 0 && dy == 0
        );
    }
}

#[test]
fn confused_paid_stay_and_obstruction_keep_requested_diagonal_cost() {
    for blocked in [false, true] {
        let (mut game, actor) = confused(if blocked { 3 } else { 4 });
        if blocked {
            game.set_wall(at(1, 1), true).unwrap();
        }
        let from = game.actors[&actor].location;
        let outcome = game.act(actor, Action::Move(Direction::NorthEast)).unwrap();
        assert_eq!(outcome.kind, OutcomeKind::Waited);
        assert_eq!(game.actors[&actor].location, from);
        assert_eq!(
            game.actors[&actor].ready_at,
            crate::movement_cost(100, Direction::NorthEast).unwrap()
        );
        assert_eq!(
            game.rules_rng,
            if blocked {
                0x3c6ef372fe94f82d
            } else {
                0x3c6ef372fe94f82e
            }
        );
    }
}

#[test]
fn confused_admission_and_stale_context_do_not_draw() {
    let (mut game, actor) = confused(3);
    let original_rng = game.rules_rng;
    let id = game
        .admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(game.rules_rng, original_rng);
    game.teleport(actor, at(4, 1)).unwrap();
    let mut expected = game.clone();
    expected.cancel_intention(actor, id).unwrap();
    assert_eq!(
        game.execute_next_intention().unwrap().outcome,
        Err(GameError::InvalidLocation)
    );
    assert_eq!(game, expected);
    assert_eq!(game.rules_rng, original_rng);
}

#[test]
fn confused_original_obstruction_and_time_overflow_reject_before_draw() {
    for overflow in [false, true] {
        let (mut game, actor) = confused(4);
        if overflow {
            game.tick = u64::MAX;
            game.actors.get_mut(&actor).unwrap().ready_at = u64::MAX;
        } else {
            game.set_wall(at(3, 1), true).unwrap();
        }
        let before = game.clone();
        assert!(game.act(actor, Action::Move(Direction::East)).is_err());
        assert_eq!(game, before);
    }
}

#[test]
fn confused_travel_keeps_original_context_but_commits_redirected_result_once() {
    let (mut game, actor) = confused(3);
    let intended = game.actor_translation(actor, Direction::East).unwrap().0;
    game.admit_travel_intention(
        actor,
        crate::TravelStep {
            direction: Direction::East,
            destination: intended,
        },
    )
    .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert!(execution.outcome.is_ok());
    assert_ne!(game.actors[&actor].location, intended);
    assert_eq!(game.actors[&actor].location, at(1, 1));
    let committed = game.clone();
    assert!(game.execute_next_intention().is_none());
    assert_eq!(game, committed);
}

#[test]
fn confused_probability_is_separate_from_permanent_erratic_capability() {
    let values = [
        modifier(LocomotionValue::Confused {
            chance: Some(MovementChance {
                numerator: 4,
                denominator: 5,
            }),
        }),
        modifier(LocomotionValue::Erratic {
            chance: Some(MovementChance {
                numerator: 1,
                denominator: 5,
            }),
        }),
        modifier(LocomotionValue::Confused { chance: None }),
    ];
    let result = LocomotionCapabilities::from_modifiers(values.iter());
    assert_eq!(result.confused, None);
    assert_eq!(
        result.erratic,
        Some(MovementChance {
            numerator: 1,
            denominator: 5
        })
    );
    for value in [
        serde_json::json!({"kind":"confused"}),
        serde_json::json!({"kind":"confused","chance":{"numerator":5,"denominator":4}}),
        serde_json::json!({"kind":"confused","chance":null,"enabled":true}),
    ] {
        assert!(serde_json::from_value::<LocomotionValue>(value).is_err());
    }
}

#[test]
fn confused_does_not_randomize_wait_or_explicit_vertical_movement() {
    let (mut game, actor) = fixture(&[
        LocomotionValue::Flying { enabled: true },
        LocomotionValue::Confused {
            chance: Some(MovementChance {
                numerator: 1,
                denominator: 1,
            }),
        },
    ]);
    let rng = game.rules_rng;
    game.act(actor, Action::Wait).unwrap();
    game.act(actor, Action::Move(Direction::Up)).unwrap();
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.actors[&actor].location, at(2, 2));
    game.act(actor, Action::Move(Direction::EastUp)).unwrap();
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.actors[&actor].location, at(3, 3));
}

#[test]
fn confused_failed_post_sample_commit_rolls_back_rng_and_retries_the_same_sample() {
    let (mut game, actor) = confused(3);
    let mut arena = crate::arena::ArenaRun::new(
        crate::arena::ArenaLimits {
            ticks: 1000,
            actions: 1,
        },
        0,
    )
    .unwrap();
    arena.actions = 1;
    game.combat.run = crate::run_policy::RunMode::Arena(arena);
    assert_eq!(game.next_actor(), Some(actor));
    let before = game.clone();
    assert_eq!(
        game.act(actor, Action::Move(Direction::East)),
        Err(GameError::Run(crate::run_policy::RunError::ActionLimit))
    );
    assert_eq!(game, before);
    game.combat.run.arena_mut().unwrap().actions = 0;
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.actors[&actor].location, at(1, 1));
    assert_eq!(game.rules_rng, 0x3c6ef372fe94f82d);
}

#[test]
fn confused_checkpoint_preserves_the_next_committed_draw_and_admitted_context() {
    let (mut game, actor) = confused(3);
    game.admit_intention(actor, Action::Move(Direction::East), IntentionOrigin::Human)
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let (snapshot, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(snapshot, shared)).unwrap()).unwrap();
    let mut restored =
        crate::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        game.execute_next_intention(),
        restored.execute_next_intention()
    );
    assert_eq!(game, restored);
    assert_eq!(game.actors[&actor].location, at(1, 1));
}

#[test]
fn confused_unsplit_and_rotated_physical_joins_share_samples_costs_and_frames() {
    assert_confused_partition_equivalence(false);
    assert_confused_partition_equivalence(true);
}

fn assert_confused_partition_equivalence(flying: bool) {
    use tor_world::{Extent, Passage, Region, RegionId, World};
    let (template, actor) = fixture(&[
        LocomotionValue::Confused {
            chance: Some(MovementChance {
                numerator: 4,
                denominator: 5,
            }),
        },
        LocomotionValue::Flying { enabled: flying },
    ]);
    let build = template.creature(actor).unwrap().build().clone();
    let origin = Location {
        region: RegionId(1),
        position: Position { x: 2, y: 2, z: 2 },
    };
    let make = |world| {
        let mut game = crate::Game::new(world, 1, template.content().clone());
        let actor = game
            .spawn_actor(origin, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "subject".into(),
            },
            build.clone(),
        )
        .unwrap();
        game
    };
    let flat = make(
        World::new(
            vec![Region {
                id: RegionId(1),
                name: "flat".into(),
                bounds: Extent::new(8, 5, 5).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
    );
    for rotation in 0..24 {
        // Arbitrary-axis joins change which local legs require flight. Compare
        // geometry with that capability, and retain the non-flying witnesses
        // for every rotation that preserves the local horizontal plane.
        if !flying
            && Direction::HORIZONTAL
                .into_iter()
                .any(|direction| direction.rotated(rotation).delta().2 != 0)
        {
            continue;
        }
        let normal = tor_world::rotate_vector(rotation, [1, 0, 0]);
        let mut center = [2, 2, 2];
        for axis in 0..3 {
            if normal[axis] != 0 {
                center[axis] = if normal[axis] > 0 { 0 } else { 4 };
            }
        }
        let offset = tor_world::rotate_vector(rotation, [0, 2, 2]);
        let target = Location {
            region: RegionId(2),
            position: Position {
                x: center[0] - offset[0] as i32,
                y: center[1] - offset[1] as i32,
                z: center[2] - offset[2] as i32,
            },
        };
        let mut world = World::new(
            vec![
                Region {
                    id: RegionId(1),
                    name: "left".into(),
                    bounds: Extent::new(3, 5, 5).unwrap(),
                },
                Region {
                    id: RegionId(2),
                    name: "right".into(),
                    bounds: Extent::new(5, 5, 5).unwrap(),
                },
            ],
            vec![],
        )
        .unwrap();
        world
            .connect_portal_area(
                Passage {
                    from: Location {
                        position: Position { x: 2, y: 0, z: 0 },
                        ..origin
                    },
                    direction: Direction::East,
                    to: target,
                },
                rotation,
                5,
                5,
            )
            .unwrap();
        for y in 0..5 {
            for z in 0..5 {
                let delta = tor_world::rotate_vector(rotation, [0, y, z]);
                let from = Location {
                    region: RegionId(2),
                    position: Position {
                        x: target.position.x + delta[0] as i32,
                        y: target.position.y + delta[1] as i32,
                        z: target.position.z + delta[2] as i32,
                    },
                };
                let to = Location {
                    position: Position {
                        x: 2,
                        y: y as i32,
                        z: z as i32,
                    },
                    ..origin
                };
                world
                    .connect_portal_area(
                        Passage {
                            from,
                            direction: Direction::West.rotated(rotation),
                            to,
                        },
                        tor_world::inverse_rotation(rotation),
                        1,
                        1,
                    )
                    .unwrap();
            }
        }
        let joined = make(world);
        for seed in 1..=64 {
            let (mut a, mut b) = (flat.clone(), joined.clone());
            a.rules_rng = seed;
            b.rules_rng = seed;
            for _ in 0..2 {
                a.act(actor, Action::Move(Direction::East)).unwrap();
                b.act(actor, Action::Move(Direction::East)).unwrap();
                let actual = a.actors[&actor].location;
                let expected = if actual.position.x >= 3 {
                    let delta = tor_world::rotate_vector(
                        rotation,
                        [
                            i64::from(actual.position.x - 3),
                            i64::from(actual.position.y - 2),
                            0,
                        ],
                    );
                    Location {
                        region: RegionId(2),
                        position: Position {
                            x: center[0] + delta[0] as i32,
                            y: center[1] + delta[1] as i32,
                            z: center[2] + delta[2] as i32,
                        },
                    }
                } else {
                    actual
                };
                assert_eq!(
                    b.actors[&actor].location, expected,
                    "rotation {rotation}, seed {seed}"
                );
                assert_eq!(
                    b.actors[&actor].orientation,
                    if actual.position.x >= 3 { rotation } else { 0 }
                );
                assert_eq!(a.rules_rng, b.rules_rng);
                assert_eq!(a.tick, b.tick);
                assert_eq!(a.actors[&actor].ready_at, b.actors[&actor].ready_at);
            }
        }
    }
}

#[test]
fn confused_unchanged_fifth_uses_one_gate_draw_and_zero_chance_uses_none() {
    let (mut game, actor) = confused(17);
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.actors[&actor].location, at(3, 1));
    assert_eq!(game.rules_rng, 0x9e3779b97f4a7c26);
    let (mut zero, actor) = fixture(&[LocomotionValue::Confused {
        chance: Some(MovementChance {
            numerator: 0,
            denominator: 5,
        }),
    }]);
    let before = zero.rules_rng;
    zero.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(zero.rules_rng, before);
}

#[test]
fn confused_rotated_sample_cannot_choose_abstract_stairs_but_explicit_stairs_are_exempt() {
    let (mut game, actor) = confused(3);
    let from = game.actors[&actor].location;
    let to = at(4, 1);
    game.world
        .connect(
            tor_world::Passage {
                from,
                direction: Direction::Up,
                to,
            },
            0,
        )
        .unwrap();
    let frame = (0..24)
        .find(|frame| Direction::West.rotated(*frame) == Direction::Up)
        .unwrap();
    game.actors.get_mut(&actor).unwrap().orientation = frame;
    assert!(game.actor_translation(actor, Direction::North).is_some());
    assert_eq!(
        game.actor_translation(actor, Direction::West).unwrap().0,
        to
    );
    let mut explicit = game.clone();
    let result = game.act(actor, Action::Move(Direction::North)).unwrap();
    assert_eq!(result.kind, OutcomeKind::Waited);
    assert_eq!(game.actors[&actor].location, from);
    assert_eq!(game.rules_rng, 0x3c6ef372fe94f82d);
    explicit.act(actor, Action::Move(Direction::West)).unwrap();
    assert_eq!(explicit.actors[&actor].location, to);
    assert_eq!(explicit.rules_rng, 3);
}

#[test]
fn rotated_requested_and_sampled_movement_keep_resolved_vertical_flight_gate() {
    for frame in 0..24 {
        for flying in [false, true] {
            let (mut game, actor) = confused(3);
            game.actors.get_mut(&actor).unwrap().orientation = frame;
            if flying {
                flight(&mut game, actor, 1_000);
            }
            for direction in Direction::HORIZONTAL
                .into_iter()
                .chain([Direction::Up, Direction::Down])
            {
                let allowed = flying || direction.rotated(frame).delta().2 == 0;
                assert_eq!(
                    game.actor_translation(actor, direction).is_some(),
                    allowed,
                    "requested: frame {frame}, {direction:?}, flying {flying}"
                );
                assert_eq!(
                    game.actor_physical_translation(actor, direction).is_some(),
                    allowed,
                    "sampled: frame {frame}, {direction:?}, flying {flying}"
                );
                if !allowed {
                    let mut rejected = game.clone();
                    assert!(rejected.act(actor, Action::Move(direction)).is_err());
                    assert_eq!(rejected, game, "invalid request must not draw or pay");
                }
            }
        }
    }
}

#[test]
fn confused_rotated_samples_pay_without_widening_vertical_capability() {
    // Pinned seeds exercise every fixed actor-local sample, including stay.
    let samples = [
        (30, (-1, -1), 0x3c6ef372fe94f848),
        (23, (0, -1), 0x3c6ef372fe94f841),
        (8, (1, -1), 0x3c6ef372fe94f832),
        (3, (-1, 0), 0x3c6ef372fe94f82d),
        (4, (0, 0), 0x3c6ef372fe94f82e),
        (2, (1, 0), 0x3c6ef372fe94f82c),
        (7, (-1, 1), 0x3c6ef372fe94f831),
        (1, (0, 1), 0x3c6ef372fe94f82b),
        (21, (1, 1), 0x3c6ef372fe94f83f),
    ];
    for frame in 0..24 {
        for flying in [false, true] {
            for (seed, (x, y), after) in samples {
                let (mut game, actor) = confused(seed);
                game.actors.get_mut(&actor).unwrap().orientation = frame;
                if flying {
                    flight(&mut game, actor, 1_000);
                }
                let requested = [Direction::North, Direction::East]
                    .into_iter()
                    .find(|direction| direction.rotated(frame).delta().2 == 0)
                    .unwrap();
                assert!(game.actor_translation(actor, requested).is_some());
                let from = game.actors[&actor].location;
                let [dx, dy, dz] = tor_world::rotate_vector(frame, [x, y, 0]);
                let moves = (x != 0 || y != 0) && (flying || dz == 0);
                let expected = if moves {
                    Location {
                        position: Position {
                            x: from.position.x + dx as i32,
                            y: from.position.y + dy as i32,
                            z: from.position.z + dz as i32,
                        },
                        ..from
                    }
                } else {
                    from
                };
                let outcome = game.act(actor, Action::Move(requested)).unwrap();
                assert_eq!(
                    game.actors[&actor].location, expected,
                    "frame {frame}, seed {seed}, flying {flying}"
                );
                assert_eq!(game.actors[&actor].orientation, frame);
                assert_eq!(matches!(outcome.kind, OutcomeKind::Moved { .. }), moves);
                assert_eq!(game.actors[&actor].ready_at, 100);
                assert_eq!((outcome.at_tick, outcome.next_tick), (0, 100));
                assert_eq!(
                    game.rules_rng, after,
                    "no replacement draw for a denied sample"
                );
            }
        }
    }
}

fn guised_fixture(values: &[LocomotionValue]) -> (crate::Game, crate::ActorId) {
    fixture_with_perception(
        values,
        &[crate::perception::PerceptionValue::ObjectGuise {
            appearance: Some(crate::perception::GuiseAppearance {
                name: "chest".into(),
                description: "A wooden chest.".into(),
                class: crate::ItemClass::Tool,
                asset: None,
            }),
        }],
    )
}

fn spawn_matching_creature(
    game: &mut crate::Game,
    source: crate::ActorId,
    location: Location,
) -> crate::ActorId {
    let build = game.creature(source).unwrap().build().clone();
    let subject = game
        .spawn_actor(location, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        subject,
        crate::CreatureIdentity {
            name: "subject".into(),
            faction: "subject".into(),
        },
        build,
    )
    .unwrap();
    subject
}

#[test]
fn confused_vertical_guise_samples_cannot_bypass_cardinal_or_diagonal_flight_gates() {
    for frame in 0..24 {
        for (seed, selected, after) in [
            (2, Direction::East, 0x3c6ef372fe94f82c),
            (8, Direction::NorthEast, 0x3c6ef372fe94f832),
        ] {
            let (dx, dy, dz) = selected.rotated(frame).delta();
            if dz == 0 {
                continue;
            }
            for flying in [false, true] {
                let (mut game, actor) = guised_fixture(&[
                    LocomotionValue::Confused {
                        chance: Some(MovementChance {
                            numerator: 4,
                            denominator: 5,
                        }),
                    },
                    LocomotionValue::Flying { enabled: flying },
                ]);
                game.rules_rng = seed;
                game.actors.get_mut(&actor).unwrap().orientation = frame;
                let from = game.actors[&actor].location;
                let target = Location {
                    position: Position {
                        x: from.position.x + dx,
                        y: from.position.y + dy,
                        z: from.position.z + dz,
                    },
                    ..from
                };
                let subject = spawn_matching_creature(&mut game, actor, target);
                let health = game.health(subject);
                assert_eq!(
                    game.movement_guise_contacts(actor, selected).is_some(),
                    flying,
                    "frame {frame}, {selected:?}, flying {flying}"
                );
                if !flying {
                    let mut rejected = game.clone();
                    assert!(rejected.act(actor, Action::Move(selected)).is_err());
                    assert_eq!(
                        rejected, game,
                        "guise fallback must not admit vertical work"
                    );
                }
                let requested = [Direction::North, Direction::East]
                    .into_iter()
                    .find(|direction| direction.rotated(frame).delta().2 == 0)
                    .unwrap();
                assert!(game.actor_translation(actor, requested).is_some());
                let outcome = game.act(actor, Action::Move(requested)).unwrap();
                assert_eq!(outcome.kind, OutcomeKind::Waited);
                assert_eq!(game.actors[&actor].location, from);
                assert_eq!(game.actors[&actor].orientation, frame);
                assert_eq!(game.actors[&actor].ready_at, 100);
                assert_eq!(game.rules_rng, after);
                assert_eq!(game.health(subject), health);
                assert_eq!(game.actors[&subject].guise.appearance().is_none(), flying);
            }
        }
    }
}

#[test]
fn diagonal_guise_route_allows_own_body_overlap_but_not_other_actors_or_walls() {
    for obstruction in [None, Some(false), Some(true)] {
        let (mut game, actor) = guised_fixture(&[]);
        let from = game.actors[&actor].location;
        let east = Location {
            position: Position {
                x: 3,
                ..from.position
            },
            ..from
        };
        let north = Location {
            position: Position {
                y: 1,
                ..from.position
            },
            ..from
        };
        let target = Location {
            position: Position {
                x: 3,
                y: 1,
                ..from.position
            },
            ..from
        };
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![
                    [0, 0, 0],
                    if obstruction.is_none() {
                        [1, 0, 0]
                    } else {
                        [0, 0, 1]
                    },
                ],
                ..Default::default()
            },
        )
        .unwrap();
        // The north ordering is always occupied. The east ordering must admit
        // the mover's own overlapping cell, but no other actor or wall.
        game.spawn_actor(north, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        if let Some(wall) = obstruction {
            if wall {
                game.set_wall(east, true).unwrap();
            } else {
                game.spawn_actor(east, std::num::NonZeroU64::new(100).unwrap())
                    .unwrap();
            }
        }
        let subject = spawn_matching_creature(&mut game, actor, target);
        let contacts = game.movement_guise_contacts(actor, Direction::NorthEast);
        assert_eq!(contacts.is_some(), obstruction.is_none());
        if let Some((_, contacts)) = contacts {
            assert_eq!(contacts, vec![subject]);
        }
    }
}

#[test]
fn guise_contact_keeps_vertical_physical_portals_and_rejects_abstract_stairs() {
    for stairs in [false, true] {
        let (mut game, actor) = guised_fixture(&[]);
        let from = if stairs { at(2, 1) } else { at(2, 5) };
        let to = if stairs {
            at(4, 1)
        } else {
            game.add_region(tor_world::Region {
                id: tor_world::RegionId(2),
                name: "above".into(),
                bounds: tor_world::Extent::new(8, 5, 6).unwrap(),
            })
            .unwrap();
            Location {
                region: tor_world::RegionId(2),
                position: Position { x: 2, y: 2, z: 0 },
            }
        };
        let passage = tor_world::Passage {
            from,
            direction: Direction::Up,
            to,
        };
        if stairs {
            game.connect(passage, 0).unwrap();
        } else {
            game.connect_portal_area(passage, 0, 1, 1).unwrap();
        }
        game.teleport(actor, from).unwrap();
        let subject = spawn_matching_creature(&mut game, actor, to);
        let contacts = game.movement_guise_contacts(actor, Direction::Up);
        assert_eq!(contacts.is_some(), !stairs);
        if let Some((destination, contacts)) = contacts {
            assert_eq!(destination, (to, 0));
            assert_eq!(contacts, vec![subject]);
        }
    }
}

#[test]
fn confused_held_simulation_move_rejects_before_rng_and_explicit_escape_stays_usable() {
    let mut species = crate::test_creatures::species();
    let sustaining = crate::content_ids::FlagId::new("magical").unwrap();
    species.modifiers.extend([
        modifier(LocomotionValue::Confused {
            chance: Some(MovementChance {
                numerator: 4,
                denominator: 5,
            }),
        }),
        crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Flag {
                flag: sustaining.clone(),
            },
        },
    ]);
    let build = crate::test_creatures::build(species);
    let mut game = crate::Game::two_room_in_stone(42, build.content().clone());
    let mut actors = Vec::new();
    for x in [1, 2] {
        let actor = game
            .spawn_actor(
                Location {
                    region: tor_world::RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "fixture actor".into(),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
        actors.push(actor);
    }
    let hold = game
        .apply_linked_hold(
            actors[1],
            actors[0],
            crate::linked_hold::HoldPolicy {
                sustaining,
                preparation: 100,
                recovery: 100,
                protection: 400,
            },
        )
        .unwrap()
        .unwrap();
    let before = game.clone();
    let rng = game.rules_rng;
    assert_eq!(
        game.act(actors[0], Action::Move(Direction::North)),
        Err(GameError::ActorBusy)
    );
    assert_eq!(
        game, before,
        "server-owned Move-to-Escape admission is not a simulation rewrite"
    );
    let outcome = game.act(actors[0], Action::Escape { hold }).unwrap();
    assert_eq!(outcome.kind, OutcomeKind::EscapeStarted { hold });
    assert_eq!(
        game.rules_rng, rng,
        "preparing Escape does not sample confusion"
    );
    assert!(matches!(
        game.preparation(actors[0]).unwrap().plan,
        crate::abilities::Plan::Escape(_)
    ));
    assert_eq!(
        game.actors[&actors[0]].location,
        before.actors[&actors[0]].location
    );
}
