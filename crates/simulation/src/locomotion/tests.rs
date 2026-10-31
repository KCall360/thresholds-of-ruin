use super::*;
use crate::rules::{Modifier, ModifierOp, Scope};

pub(super) fn modifier(value: LocomotionValue) -> Modifier {
    Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Locomotion { value },
    }
}

#[test]
fn source_composition_is_exact_order_independent_and_suppression_wins() {
    let mut values = vec![
        modifier(LocomotionValue::Flying { enabled: true }),
        modifier(LocomotionValue::Stationary { enabled: true }),
        modifier(LocomotionValue::Erratic {
            chance: Some(MovementChance {
                numerator: 2,
                denominator: 10,
            }),
        }),
        modifier(LocomotionValue::Erratic {
            chance: Some(MovementChance {
                numerator: 3,
                denominator: 6,
            }),
        }),
    ];
    let folded = LocomotionCapabilities::from_modifiers(values.iter());
    assert!(folded.flying && folded.stationary);
    assert_eq!(
        folded.erratic,
        Some(MovementChance {
            numerator: 1,
            denominator: 2
        })
    );
    values.reverse();
    assert_eq!(
        LocomotionCapabilities::from_modifiers(values.iter()),
        folded
    );
    values.extend([
        modifier(LocomotionValue::Flying { enabled: false }),
        modifier(LocomotionValue::Stationary { enabled: false }),
        modifier(LocomotionValue::Erratic { chance: None }),
    ]);
    assert_eq!(
        LocomotionCapabilities::from_modifiers(values.iter()),
        LocomotionCapabilities::default()
    );
}

#[test]
fn canonical_locomotion_is_strict_bounded_and_required_nullable() {
    for value in [
        serde_json::json!({"kind":"erratic"}),
        serde_json::json!({"kind":"erratic","chance":{"numerator":2,"denominator":1}}),
        serde_json::json!({"kind":"erratic","chance":{"numerator":0,"denominator":0}}),
        serde_json::json!({"kind":"flying","enabled":true,"chance":null}),
        serde_json::json!(["flying", true]),
    ] {
        assert!(serde_json::from_value::<LocomotionValue>(value).is_err());
    }
    assert_eq!(
        serde_json::from_value::<LocomotionValue>(
            serde_json::json!({"kind":"erratic","chance":null})
        )
        .unwrap(),
        LocomotionValue::Erratic { chance: None }
    );
    let mut value = modifier(LocomotionValue::Flying { enabled: true });
    value.scope = Scope::VersusSource;
    assert!(crate::rules::validate_modifier_shapes(&[value]).is_err());
}

use crate::{Action, ActorId, Game};
use tor_world::{Direction, Extent, Location, Position, Region, RegionId, World};
pub(super) fn at(x: i32, z: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 2, z },
    }
}
pub(super) fn fixture(values: &[LocomotionValue]) -> (Game, ActorId) {
    fixture_with_perception(values, &[])
}
pub(super) fn fixture_with_perception(
    values: &[LocomotionValue],
    perception: &[crate::perception::PerceptionValue],
) -> (Game, ActorId) {
    let mut species = crate::test_creatures::species();
    species
        .modifiers
        .extend(values.iter().copied().map(modifier));
    species
        .modifiers
        .extend(perception.iter().cloned().map(|value| Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Perception { value },
        }));
    let mut rules = crate::test_creatures::ruleset().document().clone();
    rules
        .conditions
        .iter_mut()
        .find(|c| c.id.as_str() == "braced")
        .unwrap()
        .modifiers = vec![modifier(LocomotionValue::Flying { enabled: true })];
    let suppression = rules
        .conditions
        .iter_mut()
        .find(|c| c.id.as_str() == "fear")
        .unwrap();
    suppression.modifiers = vec![modifier(LocomotionValue::Flying { enabled: false })];
    suppression.restrictions.clear();
    let id = species.id.clone();
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(rules).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: std::collections::BTreeMap::from([(id.clone(), species)]),
            templates: Default::default(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content,
        id,
        crate::progression::HdLedger::new(vec![crate::progression::HitDie::new(
            crate::progression::HdSource::Racial,
            42,
        )])
        .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let world = World::new(
        vec![Region {
            id: RegionId(1),
            name: "air".into(),
            bounds: Extent::new(8, 5, 6).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, build.content().clone());
    let actor = game
        .spawn_actor(at(2, 1), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "subject".into(),
            faction: "subject".into(),
        },
        build,
    )
    .unwrap();
    (game, actor)
}

#[test]
fn stationary_rejects_voluntary_translation_without_blocking_imposed_velocity() {
    let (mut game, actor) = fixture(&[LocomotionValue::Stationary { enabled: true }]);
    let before = game.clone();
    assert!(game.actor_translation(actor, Direction::East).is_none());
    assert!(game.act(actor, Action::Move(Direction::East)).is_err());
    assert_eq!(game, before);
    game.set_actor_velocity(actor, [crate::physics::CELL / 10, 0, 0])
        .unwrap();
    game.act(actor, Action::Wait).unwrap();
    assert_ne!(game.actors[&actor].location, at(2, 1));
}

#[test]
fn genuine_flight_pays_for_open_vertical_motion_and_suppresses_gravity() {
    let (mut game, actor) = fixture(&[LocomotionValue::Flying { enabled: true }]);
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    assert_eq!(
        game.actor_translation(actor, Direction::Up).map(|v| v.0),
        Some(at(2, 2))
    );
    game.act(actor, Action::Move(Direction::Up)).unwrap();
    assert_eq!(game.actors[&actor].location, at(2, 2));
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(game.actors[&actor].location, at(2, 2));
    assert_eq!(game.actor_motion(actor).unwrap().velocity, [0; 3]);
    assert!(game.tick > 0);
}

#[test]
fn erratic_preview_is_actor_owned_and_only_committed_move_advances_rng() {
    let (mut game, actor) = fixture(&[
        LocomotionValue::Flying { enabled: true },
        LocomotionValue::Erratic {
            chance: Some(MovementChance {
                numerator: 1,
                denominator: 1,
            }),
        },
    ]);
    game.configure_ai(
        actor,
        crate::ai::AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    game.refresh_navigation();
    let before = game.clone();
    let first = game.next_ai_action().unwrap();
    assert!(matches!(first.1, Action::Move(_)));
    assert_eq!(game.next_ai_action(), Some(first.clone()));
    assert_eq!(game, before);
    game.act(actor, first.1).unwrap();
    assert_ne!(
        game.actors[&actor].behavior.rng,
        before.actors[&actor].behavior.rng
    );
}

pub(super) fn flight(game: &mut Game, actor: ActorId, duration: u64) {
    game.apply_condition(
        actor,
        &crate::content_ids::ConditionId::new("braced").unwrap(),
        actor,
        duration,
    )
    .unwrap();
}

#[test]
fn capability_loss_resumes_gravity_without_snap_or_invented_causer() {
    let (mut game, actor) = fixture(&[]);
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    flight(&mut game, actor, 50);
    game.set_actor_velocity(actor, [123, 0, 0]).unwrap();
    let mut state = game.actors.get_mut(&actor).unwrap();
    state.motion.acceleration_remainder = [1, 2, 3];
    state.motion.displacement = [4, 5, 6];
    drop(state);
    // Settle only the capability boundary, not a physical interval.
    game.tick = 50;
    game.settle_actor_time(actor).unwrap();
    let state = &game.actors[&actor];
    assert!(!state.locomotion().flying);
    assert_eq!(state.location, at(2, 1));
    assert_eq!(state.motion.velocity, [123, 0, 0]);
    assert_eq!(state.motion.displacement, [4, 5, 6]);
    assert_eq!(state.motion.acceleration_remainder, [0; 3]);
    assert!(state.motion.active_fall.is_none());
    game.act(actor, Action::Wait).unwrap();
    assert!(
        game.actors[&actor].location.position.z < 1
            || game.actor_motion(actor).unwrap().velocity[2] < 0
    );
}

#[test]
fn flight_does_not_brake_imposed_velocity_or_make_dropped_items_fly() {
    let (mut game, actor) = fixture(&[LocomotionValue::Flying { enabled: true }]);
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.set_actor_velocity(actor, [100, 0, 0]).unwrap();
    game.place_item_stack(
        1,
        at(3, 3),
        None,
        1,
        crate::ItemSpec::ordinary("stone".into()),
    )
    .unwrap();
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(game.actor_motion(actor).unwrap().velocity, [100, 0, 0]);
    assert_eq!(game.actors[&actor].location.position.z, 1);
    assert!(
        game.items[&crate::ItemId(1)].motion.velocity[2] < 0
            || game.items[&crate::ItemId(1)].location != crate::ItemLocation::Ground(at(3, 3))
    );
}

#[test]
fn flight_retains_multi_cell_clearance() {
    let (mut game, actor) = fixture(&[LocomotionValue::Flying { enabled: true }]);
    game.set_body(
        actor,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 20,
        },
    )
    .unwrap();
    assert!(game.actor_translation(actor, Direction::Up).is_some());
    game.teleport(actor, at(2, 4)).unwrap();
    assert!(game.actor_translation(actor, Direction::Up).is_none());
}

#[test]
fn flight_loss_filters_remembered_air_edges_without_erasing_knowledge_or_stairs() {
    let (mut game, actor) = fixture(&[]);
    flight(&mut game, actor, 50);
    game.refresh_navigation();
    let destination = at(2, 2);
    assert_eq!(game.travel_route(actor, destination).unwrap().len(), 1);
    let edge = *game.navigation[&actor]
        .edges
        .get(&(at(2, 1), Direction::Up))
        .unwrap();
    assert!(edge.requires_flight);
    game.act(actor, Action::Wait).unwrap();
    // An undisclosed edge stays stale. Planning filters its captured requirement.
    game.refresh_navigation_scene(actor, &[]);
    assert_eq!(
        game.navigation[&actor]
            .edges
            .get(&(at(2, 1), Direction::Up)),
        Some(&edge)
    );
    assert!(game.travel_route(actor, destination).is_err());
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        restored.navigation[&actor]
            .edges
            .get(&(at(2, 1), Direction::Up)),
        Some(&edge)
    );
    assert!(restored.travel_route(actor, destination).is_err());
}

#[test]
fn rotated_cardinal_translation_uses_local_vertical_grant_and_loss() {
    for rotation in 0..24 {
        let (mut game, actor) = fixture(&[]);
        game.actors.get_mut(&actor).unwrap().orientation = rotation;
        let direction = [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
            Direction::Up,
            Direction::Down,
        ]
        .into_iter()
        .find(|d| d.rotated(rotation) == Direction::Up)
        .unwrap();
        assert!(game.actor_translation(actor, direction).is_none());
        flight(&mut game, actor, 50);
        assert_eq!(
            game.actor_translation(actor, direction).map(|v| v.0),
            Some(at(2, 2))
        );
        game.act(actor, Action::Wait).unwrap();
        assert!(game.actor_translation(actor, direction).is_none());
    }
}

#[test]
fn rotated_abstract_stair_keeps_actor_policy_with_or_without_flight() {
    for flying in [false, true] {
        let (mut game, actor) = fixture(&[]);
        game.configure_scenario_policy(
            crate::floor_activation::ScenarioCapabilities {
                npc_stair_traversal: false,
                ..Default::default()
            },
            Some(ActorId(99)),
        )
        .unwrap();
        game.connect(
            tor_world::Passage {
                from: at(2, 1),
                direction: Direction::Up,
                to: at(6, 1),
            },
            0,
        )
        .unwrap();
        let rotation = (0..24)
            .find(|r| Direction::East.rotated(*r) == Direction::Up)
            .unwrap();
        game.actors.get_mut(&actor).unwrap().orientation = rotation;
        if flying {
            flight(&mut game, actor, 50);
        }
        assert!(game.actor_translation(actor, Direction::East).is_none());
        game.refresh_navigation();
        assert!(!game.navigation[&actor]
            .edges
            .contains_key(&(at(2, 1), Direction::Up)));
    }
}

#[test]
fn flight_loss_filters_vertical_components_of_rotated_diagonal_routes() {
    let (mut game, actor) = fixture(&[]);
    let rotation = (0..24)
        .find(|r| Direction::NorthEast.rotated(*r).delta().2 != 0)
        .unwrap();
    game.actors.get_mut(&actor).unwrap().orientation = rotation;
    flight(&mut game, actor, 50);
    game.refresh_navigation();
    let destination = game
        .actor_translation(actor, Direction::NorthEast)
        .unwrap()
        .0;
    assert!(game.travel_route(actor, destination).is_ok());
    game.act(actor, Action::Wait).unwrap();
    assert!(game.travel_route(actor, destination).is_err());
    assert!(game
        .actor_translation(actor, Direction::NorthEast)
        .is_none());
}

#[test]
fn zero_chance_and_stationary_erratic_candidates_draw_nothing() {
    for values in [
        vec![LocomotionValue::Erratic {
            chance: Some(MovementChance {
                numerator: 0,
                denominator: 5,
            }),
        }],
        vec![
            LocomotionValue::Stationary { enabled: true },
            LocomotionValue::Erratic {
                chance: Some(MovementChance {
                    numerator: 1,
                    denominator: 1,
                }),
            },
        ],
    ] {
        let (mut game, actor) = fixture(&values);
        game.configure_ai(
            actor,
            crate::ai::AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                ..Default::default()
            },
        )
        .unwrap();
        let rng = game.actors[&actor].behavior.rng;
        assert_eq!(game.next_ai_action().unwrap().1, Action::Wait);
        game.act(actor, Action::Wait).unwrap();
        assert_eq!(game.actors[&actor].behavior.rng, rng);
    }
}

#[test]
fn known_edge_requires_explicit_bounded_named_fields() {
    let edge = crate::travel::KnownEdge {
        destination: at(2, 2),
        rotation: 0,
        requires_flight: true,
    };
    let valid = serde_json::to_value(edge).unwrap();
    assert_eq!(
        serde_json::from_value::<crate::travel::KnownEdge>(valid.clone()).unwrap(),
        edge
    );
    for field in ["destination", "rotation", "requires_flight"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<crate::travel::KnownEdge>(missing).is_err());
        let mut null = valid.clone();
        null[field] = serde_json::Value::Null;
        assert!(serde_json::from_value::<crate::travel::KnownEdge>(null).is_err());
    }
    let mut bad = valid;
    bad["rotation"] = serde_json::json!(24);
    assert!(serde_json::from_value::<crate::travel::KnownEdge>(bad).is_err());
    assert!(
        serde_json::from_value::<crate::travel::KnownEdge>(serde_json::json!([at(2, 2), 0, true]))
            .is_err()
    );
}

#[test]
fn stationary_gate_also_covers_self_step_and_fresh_ai_retreat() {
    let (mut game, actor) = fixture(&[LocomotionValue::Stationary { enabled: true }]);
    let action = Action::UseAbility {
        id: crate::content_ids::AbilityId::new("guarded_step").unwrap(),
        invocation: crate::abilities::AbilityInvocation::self_step(
            crate::abilities::StepDirection::East,
        ),
        supplies: Default::default(),
    };
    assert!(game.condition_blocks_action(actor, &action));
    let before = game.clone();
    assert!(game.act(actor, action).is_err());
    assert_eq!(game, before);
    game.configure_ai(
        actor,
        crate::ai::AiProfile {
            flee_percent: 100,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!matches!(game.next_ai_action().unwrap().1, Action::Move(_)));
}

#[test]
fn erratic_candidate_survives_rejection_and_checkpoint_without_reroll() {
    let (mut game, actor) = fixture(&[LocomotionValue::Erratic {
        chance: Some(MovementChance {
            numerator: 1,
            denominator: 1,
        }),
    }]);
    game.configure_ai(actor, crate::ai::AiProfile::default())
        .unwrap();
    game.refresh_navigation();
    let chosen = game.next_ai_action();
    let before = game.clone();
    assert!(game.act(actor, Action::Wait).is_err());
    assert_eq!(game, before);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.next_ai_action(), chosen);
    assert_eq!(
        restored.actors[&actor].behavior.rng,
        game.actors[&actor].behavior.rng
    );
}

#[test]
fn learned_vertical_physical_portal_does_not_require_flight() {
    let (mut game, actor) = fixture(&[]);
    game.configure_scenario_policy(
        crate::floor_activation::ScenarioCapabilities {
            npc_stair_traversal: false,
            ..Default::default()
        },
        Some(ActorId(99)),
    )
    .unwrap();
    game.add_region(Region {
        id: RegionId(2),
        name: "above".into(),
        bounds: Extent::new(8, 5, 6).unwrap(),
    })
    .unwrap();
    let from = at(2, 5);
    let to = Location {
        region: RegionId(2),
        position: Position { x: 2, y: 2, z: 0 },
    };
    game.connect_portal_area(
        tor_world::Passage {
            from,
            direction: Direction::Up,
            to,
        },
        0,
        1,
        1,
    )
    .unwrap();
    game.teleport(actor, from).unwrap();
    assert!(!game.world.is_stair(from, Direction::Up));
    assert_eq!(
        game.actor_translation(actor, Direction::Up).map(|v| v.0),
        Some(to)
    );
    game.refresh_navigation();
    let edge = game.navigation[&actor]
        .edges
        .get(&(from, Direction::Up))
        .unwrap();
    assert!(!edge.requires_flight);
    assert_eq!(game.travel_route(actor, to).unwrap().len(), 1);
}

#[test]
fn raw_navigation_rejects_diagonal_edges_like_compact_checkpoints() {
    let (game, _) = fixture(&[]);
    let mut navigation = crate::travel::Navigation::default();
    navigation.cells.insert(at(2, 1), false);
    navigation.cells.insert(at(3, 1), false);
    navigation.edges.insert(
        (at(2, 1), Direction::East),
        crate::travel::KnownEdge::ordinary(at(3, 1), 0),
    );
    assert!(navigation.checkpoint_valid(&game.world));
    navigation.edges.insert(
        (at(2, 1), Direction::NorthEast),
        crate::travel::KnownEdge::ordinary(at(3, 1), 0),
    );
    let raw = serde_json::to_vec(&navigation).unwrap();
    let decoded: crate::travel::Navigation = serde_json::from_slice(&raw).unwrap();
    assert!(!decoded.checkpoint_valid(&game.world));
}

#[test]
fn flying_multicell_portals_keep_clearance_and_frame_in_all_24_rotations() {
    for rotation in 0..24 {
        let (mut game, actor) = fixture(&[LocomotionValue::Flying { enabled: true }]);
        game.add_region(Region {
            id: RegionId(2),
            name: "rotated".into(),
            bounds: Extent::new(8, 5, 6).unwrap(),
        })
        .unwrap();
        let from = at(7, 2);
        game.teleport(actor, from).unwrap();
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 20,
            },
        )
        .unwrap();
        let normal = tor_world::rotate_vector(rotation, [1, 0, 0]);
        let mut position = [3, 2, 2];
        for axis in 0..3 {
            if normal[axis] != 0 {
                position[axis] = if normal[axis] > 0 { 0 } else { [7, 4, 5][axis] };
            }
        }
        let to = Location {
            region: RegionId(2),
            position: Position {
                x: position[0],
                y: position[1],
                z: position[2],
            },
        };
        game.connect_portal_area(
            tor_world::Passage {
                from,
                direction: Direction::East,
                to,
            },
            rotation,
            1,
            2,
        )
        .unwrap();
        let head = tor_world::rotate_vector(rotation, [0, 0, 1]).map(|n| n as i32);
        let blocked = Location {
            region: to.region,
            position: Position {
                x: to.position.x + head[0],
                y: to.position.y + head[1],
                z: to.position.z + head[2],
            },
        };
        game.set_wall(blocked, true).unwrap();
        assert!(
            game.actor_translation(actor, Direction::East).is_none(),
            "rotation {rotation}"
        );
        game.set_wall(blocked, false).unwrap();
        assert_eq!(
            game.actor_translation(actor, Direction::East),
            Some((to, rotation)),
            "rotation {rotation}"
        );
        game.act(actor, Action::Move(Direction::East)).unwrap();
        assert_eq!(
            (
                game.actors[&actor].location,
                game.actors[&actor].orientation
            ),
            (to, rotation)
        );
        assert!(
            game.set_wall(blocked, true).is_err(),
            "rotated body occupies the head cell"
        );
    }
}

#[test]
fn erratic_with_no_disclosed_legal_move_does_not_draw() {
    let (mut game, actor) = fixture(&[LocomotionValue::Erratic {
        chance: Some(MovementChance {
            numerator: 1,
            denominator: 1,
        }),
    }]);
    for direction in Direction::HORIZONTAL {
        let (dx, dy, dz) = direction.delta();
        let origin = at(2, 1).position;
        let position = Position {
            x: origin.x + dx,
            y: origin.y + dy,
            z: origin.z + dz,
        };
        game.set_wall(
            Location {
                position,
                ..at(2, 1)
            },
            true,
        )
        .unwrap();
    }
    game.configure_ai(
        actor,
        crate::ai::AiProfile {
            idle: crate::behavior::IdleBehavior::Wait,
            ..Default::default()
        },
    )
    .unwrap();
    let before = game.actors[&actor].behavior.rng;
    assert_eq!(game.next_ai_action().unwrap().1, Action::Wait);
    game.act(actor, Action::Wait).unwrap();
    assert_eq!(game.actors[&actor].behavior.rng, before);
}

#[test]
fn levitation_hovers_without_vertical_control_and_suppression_is_independent() {
    let (mut game, actor) = fixture(&[LocomotionValue::Levitating { enabled: true }]);
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    assert!(game.actor_translation(actor, Direction::Up).is_none());
    assert!(game.actor_translation(actor, Direction::Down).is_none());
    assert_eq!(
        game.actor_translation(actor, Direction::East).map(|v| v.0),
        Some(at(3, 1))
    );
    let before = game.clone();
    assert!(game.act(actor, Action::Move(Direction::Up)).is_err());
    assert_eq!(
        game, before,
        "vertical refusal must not advance clocks or mutate state"
    );
    game.act(actor, Action::Move(Direction::East)).unwrap();
    assert_eq!(game.actors[&actor].location, at(3, 1));
    assert_eq!(game.actors[&actor].motion.velocity, [0; 3]);
    assert!(!game.can_use_abstract_stairs(actor));
    assert_eq!(
        game.prepare_transfer(actor, crate::ItemId(999), None, true),
        Err(crate::GameError::ItemUnavailable)
    );
    let folded = LocomotionCapabilities::from_modifiers(
        [
            modifier(LocomotionValue::Levitating { enabled: true }),
            modifier(LocomotionValue::Flying { enabled: true }),
            modifier(LocomotionValue::Flying { enabled: false }),
        ]
        .iter(),
    );
    assert!(folded.levitating && folded.hovers() && !folded.flying);
    let folded = LocomotionCapabilities::from_modifiers(
        [
            modifier(LocomotionValue::Levitating { enabled: true }),
            modifier(LocomotionValue::Flying { enabled: true }),
            modifier(LocomotionValue::Levitating { enabled: false }),
        ]
        .iter(),
    );
    assert!(folded.flying && folded.hovers() && !folded.levitating);
}

#[test]
fn levitation_arrest_preserves_tangential_motion_and_true_flight_does_not_brake() {
    for rotation in 0..24 {
        let (mut game, actor) = fixture(&[LocomotionValue::Levitating { enabled: true }]);
        game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        let mut outcome = game.actor_outcome(actor).unwrap();
        outcome.state.orientation = rotation;
        let inverse = tor_world::inverse_rotation(rotation);
        outcome.state.motion.velocity = tor_world::rotate_vector(inverse, [123, -77, -400]);
        outcome.state.motion.displacement = tor_world::rotate_vector(inverse, [31, -19, -50]);
        outcome.state.motion.active_fall = Some(crate::physics::ActiveFall {
            owner: actor,
            episode: std::num::NonZeroU64::MIN,
            started_at: game.tick,
        });
        game.arrest_descent_in_outcome(&mut outcome).unwrap();
        assert_eq!(
            tor_world::rotate_vector(rotation, outcome.state.motion.velocity),
            [123, -77, 0]
        );
        assert_eq!(
            tor_world::rotate_vector(rotation, outcome.state.motion.displacement),
            [31, -19, 0]
        );
        assert_eq!(outcome.state.motion.active_fall, None);
    }
    let (mut game, actor) = fixture(&[LocomotionValue::Flying { enabled: true }]);
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let mut outcome = game.actor_outcome(actor).unwrap();
    outcome.state.motion.velocity = [123, -77, -400];
    game.arrest_descent_in_outcome(&mut outcome).unwrap();
    assert_eq!(outcome.state.motion.velocity, [123, -77, -400]);
}

#[test]
fn temporary_hover_does_not_expand_supported_teleport_destination_pool() {
    let (mut walking, actor) = fixture(&[]);
    walking.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let (mut hovering, other) = fixture(&[LocomotionValue::Levitating { enabled: true }]);
    hovering.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let baseline = walking.random_relocation_candidates(actor).unwrap();
    assert!(!baseline.is_empty(), "valid supported baseline is required");
    assert!(baseline.iter().all(|p| p.location.position.z == 0));
    assert_eq!(
        hovering.random_relocation_candidates(other).unwrap(),
        baseline
    );
}

#[test]
fn impact_teleport_keeps_paid_collision_and_authoritative_destination() {
    let (mut game, actor) = fixture(&[]);
    for x in 0..8 {
        for y in 0..5 {
            game.world
                .set_wall(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y, z: 0 },
                    },
                    true,
                )
                .unwrap();
        }
    }
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.teleport(actor, at(2, 2)).unwrap();
    game.set_actor_velocity(actor, [0, 0, -crate::physics::TERMINAL])
        .unwrap();
    game.actors.get_mut(&actor).unwrap().ready_at = 1_000;
    let mut ordinary = game.clone();
    game.place_trap(
        at(2, 1),
        tor_world::Trap {
            id: 1,
            dc: 100,
            avoidance_dc: None,
            kind: tor_world::TrapKind::Teleport {},
        },
    )
    .unwrap();
    let health = game.creature(actor).unwrap().health().current();
    ordinary.advance_physics(20).unwrap();
    game.advance_physics(20).unwrap();
    assert_eq!(
        ordinary.actors[&actor].location,
        at(2, 1),
        "independent collision baseline must land"
    );
    assert!(
        ordinary.creature(actor).unwrap().health().current() < health,
        "the witnessed collision must cause damage"
    );
    assert_eq!(
        game.creature(actor).unwrap().health(),
        ordinary.creature(actor).unwrap().health(),
        "teleport cannot undo the impact which triggered it"
    );
    assert_ne!(
        game.actors[&actor].location,
        at(2, 1),
        "integration must not copy its old cursor over teleport"
    );
    assert_eq!(game.actors[&actor].location.position.z, 1);
    assert_eq!(game.actors[&actor].motion.velocity, [0; 3]);
}

#[test]
fn exact_ray_relocation_composes_every_portal_frame_and_never_searches_on_block() {
    use crate::abilities::{Placement, StepDirection};
    for rotation in 0..24 {
        let (mut game, actor) = fixture(&[]);
        *game.world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: "portal fixture".into(),
                    bounds: Extent::new(9, 9, 9).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let origin = Location {
            region: RegionId(1),
            position: Position { x: 8, y: 4, z: 4 },
        };
        let endpoint = Location {
            region: RegionId(2),
            position: Position { x: 4, y: 4, z: 4 },
        };
        game.world
            .connect_portal_area(
                tor_world::Passage {
                    from: origin,
                    direction: Direction::East,
                    to: endpoint,
                },
                rotation,
                1,
                1,
            )
            .unwrap();
        game.actors.get_mut(&actor).unwrap().orientation = 5;
        let captured = Placement {
            location: origin,
            orientation: 0,
        };
        let expected = Placement {
            location: endpoint,
            orientation: tor_world::compose_rotation(5, rotation),
        };
        assert_eq!(
            game.ray_relocation_destination(actor, captured, StepDirection::East)
                .unwrap(),
            Some(expected),
            "rotation={rotation}"
        );
        game.world.set_wall(endpoint, true).unwrap();
        assert_eq!(
            game.ray_relocation_destination(actor, captured, StepDirection::East)
                .unwrap(),
            None,
            "blocked exact cell must not find a neighbor; rotation={rotation}"
        );
    }
}

#[test]
fn levitation_pickup_refusal_has_a_valid_ground_item_baseline() {
    for (values, allowed) in [
        (vec![], true),
        (vec![LocomotionValue::Flying { enabled: true }], true),
        (vec![LocomotionValue::Levitating { enabled: true }], false),
    ] {
        let (mut game, actor) = fixture(&values);
        let item = game
            .place_item(game.actors[&actor].location, "ordinary item".into())
            .unwrap();
        let before = game.clone();
        let result = game.prepare_transfer(actor, item, None, true);
        assert_eq!(result.is_ok(), allowed);
        assert_eq!(game, before, "preparation/refusal must not mutate state");
        if !allowed {
            assert_eq!(result, Err(crate::GameError::ItemUnavailable));
        }
    }
}

#[test]
fn oblique_descent_arrest_keeps_motion_valid_without_translating_the_body() {
    use crate::physics::{CELL, TERMINAL};
    for gravity in [[-1, -2, 0], [1, 7, -3], [997, -991, 983]] {
        for velocity in [[TERMINAL, 0, 0], [0, -TERMINAL, 0], [123, -77, -400]] {
            let (mut game, actor) = fixture(&[LocomotionValue::Levitating { enabled: true }]);
            // Each authored cell has cardinal gravity; a body spanning three
            // different fields has an oblique aggregate in its own frame.
            game.set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [1, 0, 0], [2, 0, 0]],
                    eye: [0, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
            for axis in 0..3 {
                let mut field = [0; 3];
                field[axis] = gravity[axis];
                game.set_cell_gravity(at(2 + axis as i32, 1), field)
                    .unwrap();
            }
            let mut outcome = game.actor_outcome(actor).unwrap();
            outcome.state.motion.velocity = velocity;
            outcome.state.motion.displacement = [CELL - 1, -(CELL - 1), 0];
            let location = outcome.state.location;
            assert!(
                outcome.state.motion.valid(),
                "valid independent initial motion"
            );
            game.arrest_descent_in_outcome(&mut outcome).unwrap();
            assert!(
                outcome.state.motion.valid(),
                "gravity={gravity:?}, velocity={velocity:?}"
            );
            assert_eq!(outcome.state.location, location);
            for vector in [
                outcome.state.motion.velocity,
                outcome.state.motion.displacement,
            ] {
                let dot: i128 = vector
                    .into_iter()
                    .zip(gravity)
                    .map(|(v, g)| i128::from(v) * i128::from(g))
                    .sum();
                assert!(
                    dot <= 0,
                    "no remaining descent: {vector:?} with {gravity:?}"
                );
            }
        }
    }
}
