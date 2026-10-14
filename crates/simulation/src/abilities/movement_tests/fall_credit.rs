//! Bounded causal fall ownership is private actor state, never item momentum.
use super::*;
use crate::{Action, ActorId, Game, ItemLocation};
use std::num::NonZeroU64;
use tor_world::{Direction, RegionId};

const SOURCE: ActorId = ActorId(1);
const TARGET: ActorId = ActorId(2);

fn at_push_boundary(mut game: Game) -> Game {
    start_push(&mut game);
    game.act(TARGET, Action::Wait).unwrap();
    game.actors.get_mut(&TARGET).unwrap().turn_ticks = NonZeroU64::new(10).unwrap();
    game.act(TARGET, Action::Wait).unwrap();
    assert_eq!(game.tick(), 110);
    game
}

fn falling_target(lethal: bool) -> Game {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    game.set_wall(cell(3, 3, 2), true).unwrap();
    game.set_wall(cell(4, 3, 2), true).unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    if lethal {
        crate::test_creatures::injure(&mut game, TARGET, 25).unwrap();
    }
    let game = at_push_boundary(game);
    assert_eq!(game.actors[&TARGET].location, cell(5, 3, 3));
    assert_eq!(game.actors[&TARGET].motion.velocity, [0; 3]);
    assert_eq!(
        game.actors[&TARGET]
            .motion
            .active_fall
            .as_ref()
            .unwrap()
            .owner,
        SOURCE
    );
    game
}

fn until(game: &mut Game, finished: impl Fn(&Game) -> bool) {
    for _ in 0..200 {
        if finished(game) {
            return;
        }
        let next = game
            .next_actor()
            .expect("fixture retains an ordinary decision");
        game.act(next, Action::Wait).unwrap();
    }
    assert!(
        finished(game),
        "fixture did not reach its expected boundary"
    );
}

fn push_as(game: &mut Game, source: ActorId) {
    until(game, |game| game.next_actor() == Some(source));
    let identity = game
        .admit_intention(source, push_action(), crate::IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.preparation(source).unwrap().intention, Some(identity));
    until(game, |game| game.preparation(source).is_none());
}

fn saved(game: &Game) -> serde_json::Value {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    serde_json::to_value((snapshot, shared)).unwrap()
}

fn restore(value: serde_json::Value, game: &Game) -> Result<Game, String> {
    let bytes = serde_json::to_vec(&value).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).map_err(|e| e.to_string())
}

fn xp(game: &Game, actor: ActorId) -> u64 {
    game.creature(actor).unwrap().earned_progress().earned_xp()
}

#[test]
fn a_new_applied_push_replaces_the_fall_owner_but_a_blocked_push_preserves_it() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    // Existing nonzero momentum makes the no-gravity episode physically live.
    game.set_actor_velocity(TARGET, [1, 0, 0]).unwrap();
    push_as(&mut game, SOURCE);
    let first = game.actors[&TARGET].motion.active_fall.unwrap();
    assert_eq!(first.owner, SOURCE);
    let other = game
        .spawn_actor(cell(5, 2, 3), NonZeroU64::new(100).unwrap())
        .unwrap();
    configure_push_subject(&mut game, other, "pushing_source");
    push_as(&mut game, other);
    assert_eq!(game.actors[&TARGET].location, cell(5, 4, 3));
    let second = game.actors[&TARGET].motion.active_fall.unwrap();
    assert_eq!(second.owner, other);
    assert!(second.episode > first.episode);
    assert!(second.started_at > first.started_at);
    game.teleport(other, cell(5, 3, 3)).unwrap();
    game.set_wall(cell(5, 5, 3), true).unwrap();
    let allocator = game.physics.next_fall_episode;
    push_as(&mut game, other);
    assert_eq!(game.actors[&TARGET].location, cell(5, 4, 3));
    assert_eq!(
        game.actors[&TARGET].motion.active_fall.as_ref(),
        Some(&second)
    );
    assert_eq!(game.physics.next_fall_episode, allocator);
}

#[test]
fn a_live_fall_round_trips_real_checkpoint_bytes_and_awards_only_once() {
    let mut game = falling_target(true);
    let mut replay = restore(saved(&game), &game).unwrap();
    assert_eq!(game, replay);
    for _ in 0..200 {
        if !game.alive(TARGET) {
            break;
        }
        let next = game.next_actor().unwrap();
        assert_eq!(game.act(next, Action::Wait), replay.act(next, Action::Wait));
        assert_eq!(game, replay);
    }
    assert!(!game.alive(TARGET));
    assert_eq!(xp(&game, SOURCE), 25);
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
    for _ in 0..5 {
        let next = game.next_actor().unwrap();
        game.act(next, Action::Wait).unwrap();
    }
    assert_eq!(
        xp(&game, SOURCE),
        25,
        "diagnostic impacts cannot award again"
    );
}

#[test]
fn stationary_supported_and_no_gravity_pushes_cannot_credit_a_later_fall() {
    for initially_supported in [false, true] {
        let facts = profile(CreatureSize::Medium, false);
        let mut game = push_game(facts, facts, true);
        for x in 3..=5 {
            game.set_wall(cell(x, 3, 2), true).unwrap();
        }
        if initially_supported {
            game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        }
        let mut game = at_push_boundary(game);
        assert!(game.actors[&TARGET].motion.active_fall.is_none());
        crate::test_creatures::injure(&mut game, TARGET, 25).unwrap();
        game.set_wall(cell(5, 3, 2), false).unwrap();
        game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        until(&mut game, |game| !game.alive(TARGET));
        assert_eq!(
            xp(&game, SOURCE),
            0,
            "later environmental fall is unrelated"
        );
    }
}

#[test]
fn surviving_a_landing_retires_the_cause_before_an_unrelated_second_fall() {
    let mut game = falling_target(false);
    game.set_wall(cell(5, 3, 1), true).unwrap();
    until(&mut game, |game| {
        game.actors[&TARGET].location == cell(5, 3, 2)
            && game.actors[&TARGET].motion.velocity == [0; 3]
    });
    assert!(game.alive(TARGET));
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
    let injury = game.health(TARGET).unwrap().0 - 1;
    crate::test_creatures::injure(&mut game, TARGET, injury).unwrap();
    game.set_wall(cell(5, 3, 1), false).unwrap();
    until(&mut game, |game| !game.alive(TARGET));
    assert_eq!(xp(&game, SOURCE), 0);
}

#[test]
fn landing_settles_its_damage_before_retiring_a_cause_with_tangential_velocity() {
    for lethal in [false, true] {
        let mut game = falling_target(lethal);
        {
            let mut target = game.actors.get_mut(&TARGET).unwrap();
            target.motion.velocity = [1, 0, -4096];
            target.motion.displacement = [0, 0, -crate::physics::CELL + 4096];
        }
        // Existing incoming motion must survive newly added support until impact.
        game.set_wall(cell(5, 3, 2), true).unwrap();
        assert!(game.actors[&TARGET].motion.active_fall.is_some());
        let tick = game.tick() + 1;
        game.advance_physics(tick).unwrap();
        assert!(game.actors[&TARGET].motion.active_fall.is_none());
        if lethal {
            assert!(!game.alive(TARGET));
            assert_eq!(xp(&game, SOURCE), 25);
        } else {
            assert!(game.alive(TARGET));
            assert_eq!(game.actors[&TARGET].motion.velocity, [1, 0, 0]);
            assert!(game.health(TARGET).unwrap().0 < 26);
            assert_eq!(xp(&game, SOURCE), 0);
            let injury = game.health(TARGET).unwrap().0 - 1;
            crate::test_creatures::injure(&mut game, TARGET, injury).unwrap();
            game.set_wall(cell(5, 3, 2), false).unwrap();
            until(&mut game, |game| !game.alive(TARGET));
            assert_eq!(
                xp(&game, SOURCE),
                0,
                "tangential landing motion cannot credit a later independent fall"
            );
        }
    }
}

#[test]
fn ordinary_voluntary_displacement_retires_a_live_fall() {
    let mut game = falling_target(false);
    // A declared stair is an ordinary voluntary displacement even while airborne.
    game.connect(
        tor_world::Passage {
            from: cell(5, 3, 3),
            direction: Direction::Up,
            to: cell(7, 3, 3),
        },
        0,
    )
    .unwrap();
    assert_eq!(game.next_actor(), Some(TARGET));
    game.act(TARGET, Action::Move(Direction::Up)).unwrap();
    assert_eq!(game.actors[&TARGET].location, cell(7, 3, 3));
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
}

#[test]
fn teleport_and_body_reshape_retire_the_actor_fall_episode() {
    for teleport in [false, true] {
        let mut game = falling_target(false);
        if teleport {
            game.teleport(TARGET, cell(7, 3, 3)).unwrap();
        } else {
            game.set_body(
                TARGET,
                crate::BodySpec {
                    cells: vec![[0; 3], [0, 1, 0]],
                    eye: [0; 3],
                    mass: 80,
                },
            )
            .unwrap();
        }
        assert!(game.actors[&TARGET].motion.active_fall.is_none());
    }
}

#[test]
fn dropped_stacks_compare_kinematics_without_copying_actor_fall_ownership() {
    let mut game = falling_target(false);
    let at = game.actors[&TARGET].location;
    let mut spec = crate::ItemSpec::ordinary("falling token".into());
    spec.stackable = true;
    let carried = game
        .place_item_stack(100, at, Some(TARGET), 2, spec.clone())
        .unwrap();
    let ground = game.place_item_stack(101, at, None, 3, spec).unwrap();
    let before = game.actors[&TARGET].motion.clone();
    assert!(
        matches!(game.prepare_transfer(TARGET, carried, None, false).unwrap(),
        crate::OutcomeKind::Dropped { result, quantity: 2, .. } if result == ground)
    );
    game.apply_transfer(carried, ground, 2, ItemLocation::Ground(at))
        .unwrap();
    assert_eq!(game.items[&ground].quantity, 5);
    assert!(game.items[&ground].motion.active_fall.is_none());
    assert_eq!(game.actors[&TARGET].motion, before);
    let loose = game
        .place_authored_item(102, at, "loose token".into(), Some(TARGET))
        .unwrap();
    game.apply_transfer(loose, loose, 1, ItemLocation::Ground(at))
        .unwrap();
    let motion = &game.items[&loose].motion;
    assert!(motion.active_fall.is_none());
    assert_eq!(motion.velocity, before.velocity);
    assert_eq!(motion.displacement, before.displacement);
    assert_eq!(motion.acceleration_remainder, before.acceleration_remainder);
}

#[test]
fn death_clears_actor_ownership_before_transferring_motion_to_corpse_and_inventory() {
    let mut game = falling_target(false);
    let at = game.actors[&TARGET].location;
    let carried = game
        .place_authored_item(100, at, "keepsake".into(), Some(TARGET))
        .unwrap();
    let before = game.actors[&TARGET].motion.clone();
    crate::test_creatures::injure(&mut game, TARGET, 1000).unwrap();
    assert!(!game.alive(TARGET));
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
    let corpse = game
        .items
        .iter()
        .find(|(_, item)| item.spec.class == crate::ItemClass::Corpse)
        .unwrap()
        .1;
    for item in [&game.items[&carried], corpse] {
        assert!(item.motion.active_fall.is_none());
        assert_eq!(item.motion.velocity, before.velocity);
        assert_eq!(item.motion.displacement, before.displacement);
    }
    assert_eq!(
        xp(&game, SOURCE),
        0,
        "unrelated death is not fall impact credit"
    );
}

#[test]
fn fall_owner_load_pins_do_not_thaw_or_catch_up_the_credit_owner() {
    let mut game = falling_target(true);
    let started = game.actors[&TARGET]
        .motion
        .active_fall
        .as_ref()
        .unwrap()
        .started_at;
    game.add_region(tor_world::Region {
        id: RegionId(2),
        name: "remote owner".into(),
        bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
    })
    .unwrap();
    game.teleport(
        SOURCE,
        tor_world::Location {
            region: RegionId(2),
            ..cell(3, 3, 3)
        },
    )
    .unwrap();
    crate::test_creatures::injure(&mut game, SOURCE, 3).unwrap();
    let mut records = crate::MemoryRecords::default();
    let (sets, _) = game
        .transition_regions(
            &crate::RegionTransition {
                active: [RegionId(1)].into(),
                loaded: [RegionId(1)].into(),
            },
            &mut records,
        )
        .unwrap();
    assert!(sets.loaded.contains(&RegionId(2)));
    assert!(!sets.active.contains(&RegionId(2)));
    assert!(game.actor_frozen(SOURCE));
    let clock = game.actor_clock(SOURCE);
    let active_since = game.actors[&SOURCE].active_since;
    let ready_at = game.actors[&SOURCE].ready_at;
    let recovery = game.creature(SOURCE).unwrap().health_recovery();
    let mut game = restore(saved(&game), &game).unwrap();
    assert_eq!(
        game.actors[&TARGET]
            .motion
            .active_fall
            .as_ref()
            .unwrap()
            .started_at,
        started
    );
    until(&mut game, |game| !game.alive(TARGET));
    assert_eq!(xp(&game, SOURCE), 25);
    assert!(game.actor_frozen(SOURCE));
    assert_eq!(game.actor_clock(SOURCE), clock);
    assert_eq!(game.actors[&SOURCE].active_since, active_since);
    assert_eq!(game.actors[&SOURCE].ready_at, ready_at);
    assert_eq!(game.creature(SOURCE).unwrap().health_recovery(), recovery);
}

#[test]
fn current_motion_requires_an_explicit_nullable_fall_field_and_closed_cause_shape() {
    let game = falling_target(false);
    let value = serde_json::to_value(&game.actors[&TARGET].motion).unwrap();
    let valid: crate::MotionState =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(valid, game.actors[&TARGET].motion);
    let fields = [
        "velocity",
        "displacement",
        "acceleration_remainder",
        "active_fall",
    ];
    assert_eq!(value.as_object().unwrap().len(), fields.len());
    let positional: Vec<_> = fields.iter().map(|field| value[*field].clone()).collect();
    assert!(serde_json::from_value::<crate::MotionState>(json!(positional)).is_err());
    assert_eq!(value["active_fall"].as_object().unwrap().len(), 3);
    let mut absent = value.clone();
    absent.as_object_mut().unwrap().remove("active_fall");
    assert!(
        serde_json::from_slice::<crate::MotionState>(&serde_json::to_vec(&absent).unwrap())
            .is_err()
    );
    for field in ["owner", "episode", "started_at"] {
        let mut absent = value.clone();
        absent["active_fall"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_slice::<crate::MotionState>(
            &serde_json::to_vec(&absent).unwrap()
        )
        .is_err());
    }
    for malformed in [
        json!({"owner": 0, "episode": 1, "started_at": 110}),
        json!({"owner": 1, "episode": 0, "started_at": 110}),
        json!({"owner": 1, "episode": 1, "started_at": 110, "extra": true}),
        json!([1, 1, 110]),
    ] {
        let mut broken = value.clone();
        broken["active_fall"] = malformed;
        assert!(serde_json::from_slice::<crate::MotionState>(
            &serde_json::to_vec(&broken).unwrap()
        )
        .is_err());
    }
    let text = serde_json::to_string(&value).unwrap();
    let duplicate = text.replacen("\"owner\":1", "\"owner\":1,\"owner\":1", 1);
    assert_ne!(text, duplicate);
    assert!(serde_json::from_slice::<crate::MotionState>(duplicate.as_bytes()).is_err());
    let mut cleared = value;
    cleared["active_fall"] = serde_json::Value::Null;
    assert!(
        serde_json::from_slice::<crate::MotionState>(&serde_json::to_vec(&cleared).unwrap())
            .is_ok()
    );
}

#[test]
fn checkpoint_rejects_future_fall_stamps_and_unissued_episodes() {
    let game = falling_target(false);
    for corruption in ["future", "unissued", "allocator_zero"] {
        let mut value = saved(&game);
        match corruption {
            "future" => {
                value[1]["actors"][0]["2"]["motion"]["active_fall"]["started_at"] =
                    json!(game.tick() + 1)
            }
            "unissued" => {
                value[1]["actors"][0]["2"]["motion"]["active_fall"]["episode"] =
                    json!(game.physics.next_fall_episode.get())
            }
            "allocator_zero" => value[0]["physics"]["next_fall_episode"] = json!(0),
            _ => unreachable!(),
        }
        assert!(restore(value, &game).is_err(), "accepted {corruption}");
    }
}

#[test]
fn deleting_the_causal_owner_cannot_veto_a_later_lethal_impact() {
    let mut game = falling_target(true);
    // There is no ordinary deletion action; remove the source at the identity
    // boundary to exercise a stale historical owner independently of detach.
    game.actors.remove(&SOURCE).unwrap();
    until(&mut game, |game| !game.alive(TARGET));
    assert!(!game.alive(TARGET));
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
    assert_eq!(
        game.items
            .values()
            .filter(|item| item.spec.class == crate::ItemClass::Corpse)
            .count(),
        1
    );
}

#[test]
fn a_dead_fall_owner_receives_held_xp_without_resurrection() {
    let mut game = falling_target(true);
    crate::test_creatures::injure(&mut game, SOURCE, 1000).unwrap();
    assert!(!game.alive(SOURCE));
    let health = game.creature(SOURCE).unwrap().health();
    until(&mut game, |game| !game.alive(TARGET));
    assert_eq!(xp(&game, SOURCE), 25);
    assert_eq!(game.creature(SOURCE).unwrap().health(), health);
    assert!(!game.alive(SOURCE));
}

#[test]
fn reattaching_a_detached_fall_materializes_its_owner_without_thawing_it() {
    let mut game = falling_target(true);
    for region in [2, 3] {
        game.add_region(tor_world::Region {
            id: RegionId(region),
            name: format!("remote {region}"),
            bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
        })
        .unwrap();
    }
    game.teleport(
        SOURCE,
        tor_world::Location {
            region: RegionId(2),
            ..cell(3, 3, 3)
        },
    )
    .unwrap();
    let waiting = game
        .spawn_actor(
            tor_world::Location {
                region: RegionId(3),
                ..cell(3, 3, 3)
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    configure_push_subject(&mut game, waiting, "pushing_source");
    let cause = game.actors[&TARGET].motion.active_fall;
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(
        &crate::RegionTransition {
            active: [RegionId(3)].into(),
            loaded: [RegionId(3)].into(),
        },
        &mut records,
    )
    .unwrap();
    assert!(game.detached_actor(SOURCE));
    assert!(game.detached_actor(TARGET));
    assert!(game.detached_records_valid(&mut records));
    until(&mut game, |game| game.tick() >= 410);
    let mut restored = restore(saved(&game), &game).unwrap();
    let proposed = crate::RegionTransition {
        active: [RegionId(1)].into(),
        loaded: [RegionId(1)].into(),
    };
    let (sets, _) = game.transition_regions(&proposed, &mut records).unwrap();
    let mut replay_records = records.clone();
    restored
        .transition_regions(&proposed, &mut replay_records)
        .unwrap();
    assert_eq!(game, restored);
    assert_eq!(game.actors[&TARGET].motion.active_fall, cause);
    assert_eq!(sets.active, [RegionId(1)].into());
    assert!(sets.loaded.contains(&RegionId(2)));
    assert!(game.actor_frozen(SOURCE));
    let clock = game.actor_clock(SOURCE);
    until(&mut game, |game| !game.alive(TARGET));
    assert_eq!(xp(&game, SOURCE), 25);
    assert_eq!(game.actor_clock(SOURCE), clock);
    assert!(game.actor_frozen(SOURCE));
}

#[test]
fn fall_index_tracks_mutation_and_clone_without_entering_authoritative_equality() {
    let mut game = falling_target(false);
    let snapshot = game.clone();
    assert_eq!(
        game.actors.active_fall_owners().collect::<Vec<_>>(),
        vec![TARGET]
    );
    game.actors.get_mut(&TARGET).unwrap().motion.active_fall = None;
    assert_eq!(game.actors.active_fall_owners().count(), 0);
    assert_eq!(
        snapshot.actors.active_fall_owners().collect::<Vec<_>>(),
        vec![TARGET]
    );
    let actor = snapshot.actors[&TARGET].clone();
    game.actors.insert(TARGET, actor);
    assert_eq!(game, snapshot);
    assert_eq!(
        game.actors.active_fall_owners().collect::<Vec<_>>(),
        vec![TARGET]
    );
    let mut rebuilt =
        crate::actor_store::ActorStore::from_entries(game.actors.raw_entries().clone());
    assert_eq!(rebuilt, game.actors);
    assert_eq!(
        rebuilt.active_fall_owners().collect::<Vec<_>>(),
        vec![TARGET]
    );
    rebuilt.remove(&TARGET).unwrap();
    assert_eq!(rebuilt.active_fall_owners().count(), 0);
}
// Append these tests inside abilities/movement_tests/fall_credit.rs.
// Source-only review drafts for 0785ff67b3593547d1b5b2f1558c867dad4e1f79.
// Not compiled or run. Use the existing helpers/imports in that module.

#[test]
fn a_harmless_side_contact_preserves_the_owner_until_the_lethal_landing() {
    let mut game = falling_target(true);
    let cause = game.actors[&TARGET].motion.active_fall.unwrap();
    let health = game.health(TARGET);
    game.set_wall(cell(6, 3, 3), true).unwrap();
    {
        // Preserve the episode created by the ordinary admitted push. As in
        // landing_settles_its_damage_before_retiring_a_cause_with_tangential_velocity,
        // seed only kinematics to isolate the collision boundary.
        let mut target = game.actors.get_mut(&TARGET).unwrap();
        target.motion.velocity = [1, 0, -4096];
        target.motion.displacement = [crate::physics::CELL - 1, 0, 0];
    }
    let tick = game.tick();
    game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    assert!(game.physics_impacts().iter().any(|impact| {
        impact.entity == crate::PhysicsEntity::Actor(TARGET)
            && impact.at_tick == tick + 1
            && impact.axis == 0
            && impact.incoming_velocity == 1
    }));
    assert_eq!(game.health(TARGET), health, "side contact does no damage");
    assert!(game.actors[&TARGET].motion.velocity[2] < 0);
    assert_eq!(
        game.actors[&TARGET].motion.active_fall,
        Some(cause),
        "a side collision did not settle the downward fall"
    );
    assert_eq!(xp(&game, SOURCE), 0);
    until(&mut game, |game| !game.alive(TARGET));
    assert_eq!(
        xp(&game, SOURCE),
        25,
        "the admitted push still owns the later lethal landing"
    );
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
}

#[test]
fn support_on_one_gravity_axis_preserves_the_fall_on_another_axis() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    game.set_body(
        TARGET,
        crate::BodySpec {
            cells: vec![[0; 3], [0, 1, 0]],
            eye: [0; 3],
            mass: 80,
        },
    )
    .unwrap();
    game.set_wall(cell(3, 3, 2), true).unwrap();
    game.set_wall(cell(4, 3, 2), true).unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    crate::test_creatures::injure(&mut game, TARGET, 25).unwrap();
    let mut game = at_push_boundary(game);
    assert_eq!(game.actors[&TARGET].location, cell(5, 3, 3));
    let cause = game.actors[&TARGET].motion.active_fall.unwrap();
    game.set_wall(cell(6, 3, 3), true).unwrap();
    // Each cell has one valid gravity axis. Across the two-cell body their
    // resultant has supported X and unsupported Z components.
    game.set_cell_gravity(cell(5, 3, 3), [1, 0, 0]).unwrap();
    assert_eq!(
        game.actors[&TARGET].motion.active_fall,
        Some(cause),
        "partial support is not a settled physical chain"
    );
    game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    assert_eq!(game.actors[&TARGET].motion.velocity[0], 0);
    assert!(game.actors[&TARGET].motion.velocity[2] < 0);
    assert_eq!(
        game.actors[&TARGET].motion.active_fall,
        Some(cause),
        "a resting axis must not erase the other axis's continuing fall"
    );
    until(&mut game, |game| !game.alive(TARGET));
    assert_eq!(xp(&game, SOURCE), 25);
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
}

#[test]
fn setting_the_unchanged_velocity_preserves_the_current_fall_episode() {
    let mut game = falling_target(true);
    let before = game.clone();
    let velocity = game.actors[&TARGET].motion.velocity;
    game.set_actor_velocity(TARGET, velocity).unwrap();
    assert!(
        game == before,
        "an unchanged velocity cannot abandon causal work"
    );
}

#[test]
fn a_nonlethal_collision_preserves_credit_for_another_axis_in_the_same_tick() {
    let mut game = falling_target(false);
    let injury = game.health(TARGET).unwrap().0 - 3;
    crate::test_creatures::injure(&mut game, TARGET, injury).unwrap();
    let cause = game.actors[&TARGET].motion.active_fall;
    game.set_wall(cell(6, 3, 3), true).unwrap();
    {
        // Seed only kinematics; the episode came from the ordinary admitted push.
        let mut target = game.actors.get_mut(&TARGET).unwrap();
        target.motion.velocity = [2560, 0, -4096];
        target.motion.displacement = [crate::physics::CELL - 2560, 0, -crate::physics::CELL + 4096];
    }
    game.set_wall(cell(5, 3, 2), true).unwrap();
    assert_eq!(game.actors[&TARGET].motion.active_fall, cause);
    let tick = game.tick();
    game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    let impacts: Vec<_> = game
        .physics_impacts()
        .iter()
        .filter(|impact| {
            impact.entity == crate::PhysicsEntity::Actor(TARGET) && impact.at_tick == tick + 1
        })
        .collect();
    assert_eq!(
        impacts.iter().map(|impact| impact.axis).collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(
        crate::combat::impact_damage(impacts[0].incoming_velocity),
        1
    );
    assert_eq!(
        crate::combat::impact_damage(impacts[1].incoming_velocity),
        5
    );
    assert!(!game.alive(TARGET));
    assert_eq!(
        xp(&game, SOURCE),
        25,
        "the earlier nonlethal impact must not erase the same-step fatal cause"
    );
    assert!(game.actors[&TARGET].motion.active_fall.is_none());
}

fn remote_fall_owner(game: &mut Game) {
    game.add_region(tor_world::Region {
        id: RegionId(2),
        name: "off-floor fall owner".into(),
        bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
    })
    .unwrap();
    game.teleport(
        SOURCE,
        tor_world::Location {
            region: RegionId(2),
            ..cell(3, 3, 3)
        },
    )
    .unwrap();
}

#[test]
fn checkpoint_rejects_one_episode_claimed_by_two_loaded_actors() {
    let game = falling_target(false);
    let valid = saved(&game);
    assert_eq!(restore(valid.clone(), &game).unwrap(), game);
    let mut forged = valid;
    let mut duplicate = forged[1]["actors"][0]["2"]["motion"]["active_fall"].clone();
    duplicate["owner"] = json!(TARGET.0);
    forged[1]["actors"][0]["1"]["motion"]["active_fall"] = duplicate;
    assert!(restore(forged, &game).is_err());
}

#[test]
fn duplicate_fall_episodes_reject_across_loaded_and_detached_record_boundaries() {
    use crate::streaming::{RecordStore, RegionRecordData};
    for both_detached in [false, true] {
        let mut game = falling_target(false);
        let mut duplicate = serde_json::to_value(game.actors[&TARGET].motion.active_fall).unwrap();
        duplicate["owner"] = json!(TARGET.0);
        remote_fall_owner(&mut game);
        let mut records = crate::MemoryRecords::default();
        game.transition_regions(
            &crate::RegionTransition {
                active: Default::default(),
                loaded: if both_detached {
                    Default::default()
                } else {
                    [RegionId(1)].into()
                },
            },
            &mut records,
        )
        .unwrap();
        assert!(game.detached_actor(SOURCE));
        assert_eq!(game.detached_actor(TARGET), both_detached);
        assert!(game.detached_records_valid(&mut records));
        assert_eq!(restore(saved(&game), &game).unwrap(), game);
        let mut corrupted = crate::MemoryRecords::default();
        for (region, id) in game.detached_records() {
            let record = records.get(id).unwrap();
            let mut value = serde_json::to_value(RegionRecordData::capture(&record)).unwrap();
            if region == RegionId(2) {
                value["actors"]["1"]["motion"]["active_fall"] = duplicate.clone();
            }
            let decoded: RegionRecordData =
                serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
            corrupted.put(
                id,
                tor_world::Shared::new(decoded.restore(game.content().clone()).unwrap()),
            );
        }
        assert!(
            !game.detached_records_valid(&mut corrupted),
            "episode uniqueness crosses record boundaries"
        );
        let attach = crate::RegionTransition {
            active: Default::default(),
            loaded: [RegionId(1), RegionId(2)].into(),
        };
        let mut control = game.clone();
        control
            .apply_region_transition(&attach, &mut records)
            .unwrap();
        let before = game.clone();
        let record_ids = corrupted.ids().collect::<Vec<_>>();
        assert!(matches!(
            game.apply_region_transition(&attach, &mut corrupted),
            Err(crate::TransitionError::InvalidRecord(_))
        ));
        assert_eq!(
            game, before,
            "failed attachment publishes no partial arrival"
        );
        assert_eq!(corrupted.ids().collect::<Vec<_>>(), record_ids);
    }
}

#[test]
fn loaded_and_detached_item_and_corpse_records_reject_forged_fall_causes() {
    use crate::streaming::{RecordStore, RegionRecordData};
    for corpse in [false, true] {
        let mut game = falling_target(corpse);
        let cause = serde_json::to_value(game.actors[&TARGET].motion.active_fall).unwrap();
        let item = if corpse {
            // Let the ordinary action boundary settle the fatal landing and
            // select the next ready actor before saving the positive control.
            until(&mut game, |game| !game.alive(TARGET));
            *game
                .items
                .iter()
                .find(|(_, item)| item.spec.class == crate::ItemClass::Corpse)
                .unwrap()
                .0
        } else {
            game.place_authored_item(100, cell(7, 3, 3), "ordinary token".into(), None)
                .unwrap()
        };
        assert!(game.items[&item].motion.active_fall.is_none());
        let original = saved(&game);
        assert_eq!(restore(original.clone(), &game).unwrap(), game);
        let mut forged = original;
        let items = forged[0]["items"].as_u64().unwrap() as usize;
        let key = item.0.to_string();
        forged[1]["items"][items][&key]["motion"]["active_fall"] = cause.clone();
        assert!(
            restore(forged, &game).is_err(),
            "accepted forged loaded item, corpse={corpse}"
        );

        let mut records = crate::MemoryRecords::default();
        game.transition_regions(&crate::RegionTransition::default(), &mut records)
            .unwrap();
        assert!(game.detached_records_valid(&mut records));
        let id = game.detached_record(RegionId(1)).unwrap();
        let record = records.get(id).unwrap();
        let mut value = serde_json::to_value(RegionRecordData::capture(&record)).unwrap();
        value["items"][&key]["motion"]["active_fall"] = cause;
        let decoded: RegionRecordData =
            serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
        let mut corrupted = crate::MemoryRecords::default();
        corrupted.put(
            id,
            tor_world::Shared::new(decoded.restore(game.content().clone()).unwrap()),
        );
        assert!(!game.detached_records_valid(&mut corrupted));
        let attach = crate::RegionTransition {
            active: Default::default(),
            loaded: [RegionId(1)].into(),
        };
        let mut control = game.clone();
        control
            .apply_region_transition(&attach, &mut records)
            .unwrap();
        let before = game.clone();
        assert_eq!(
            game.apply_region_transition(&attach, &mut corrupted),
            Err(crate::TransitionError::InvalidRecord(RegionId(1)))
        );
        assert_eq!(
            game, before,
            "forged item cause cannot partially attach a region"
        );
    }
}

#[test]
fn exhausted_fall_identity_rejects_the_whole_ordinary_resolution_boundary() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    game.set_actor_velocity(TARGET, [1, 0, 0]).unwrap();
    let intention = start_push(&mut game);
    let mut expected_costs = game.creature(SOURCE).unwrap().costs().clone();
    assert_eq!(game.tick(), 0);
    assert_eq!(
        expected_costs
            .resources()
            .balance(crate::resources::Resource::Stamina),
        19,
        "the admitted start is paid before any recovery"
    );
    assert_eq!(
        expected_costs.reservation(intention),
        Some(crate::costs::ResourceCost {
            resource: crate::resources::Resource::Stamina,
            start: 1,
            resolution: 2,
        })
    );
    game.act(TARGET, Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    game.actors.get_mut(&TARGET).unwrap().turn_ticks = NonZeroU64::new(10).unwrap();
    game.physics.next_fall_episode = NonZeroU64::new(u64::MAX).unwrap();
    // Recovery is ordinary elapsed-time work, not a refund of the start cost.
    expected_costs.advance_active(100);
    assert_eq!(
        game.creature(SOURCE).unwrap().costs(),
        &expected_costs,
        "recovery preserves the unpaid resolution hold"
    );
    let before = game.clone();
    for _ in 0..2 {
        assert_eq!(
            game.act(TARGET, Action::Wait),
            Err(crate::GameError::IdentityExhausted)
        );
        assert_eq!(
            game, before,
            "roll back time, RNG, primary damage, rider movement, costs, events and allocator"
        );
        assert_eq!(
            game.creature(SOURCE).unwrap().costs(),
            &expected_costs,
            "a rejected boundary neither refunds the start nor spends the hold"
        );
        assert_eq!(game.health(TARGET), Some((30, 30)));
        assert_eq!(game.actors[&TARGET].location, cell(4, 3, 3));
        assert_eq!(game.preparation(SOURCE).unwrap().intention, Some(intention));
    }
    let mut last_issued = before;
    last_issued.physics.next_fall_episode = NonZeroU64::new(u64::MAX - 1).unwrap();
    last_issued.act(TARGET, Action::Wait).unwrap();
    assert_eq!(last_issued.tick(), 110);
    assert_eq!(last_issued.physics.next_fall_episode.get(), u64::MAX);
    assert_eq!(
        last_issued.actors[&TARGET]
            .motion
            .active_fall
            .unwrap()
            .episode
            .get(),
        u64::MAX - 1
    );
    assert_eq!(last_issued.health(TARGET), Some((26, 30)));
    assert_eq!(last_issued.actors[&TARGET].location, cell(5, 3, 3));
    expected_costs.advance_active(10);
    expected_costs.finish(intention).unwrap();
    assert_eq!(
        last_issued.creature(SOURCE).unwrap().costs(),
        &expected_costs,
        "the successful boundary pays the resolution exactly once"
    );
}

#[test]
fn landing_and_death_release_only_the_fall_owner_load_pin_without_advancing_it() {
    use crate::streaming::RecordStore;
    for lethal in [false, true] {
        let mut game = falling_target(lethal);
        if !lethal {
            game.set_wall(cell(5, 3, 1), true).unwrap();
        }
        remote_fall_owner(&mut game);
        crate::test_creatures::injure(&mut game, SOURCE, 3).unwrap();
        let mut records = crate::MemoryRecords::default();
        let proposed = crate::RegionTransition {
            active: [RegionId(1)].into(),
            loaded: [RegionId(1)].into(),
        };
        let (pinned, _) = game.transition_regions(&proposed, &mut records).unwrap();
        assert_eq!(pinned.active, proposed.active);
        assert_eq!(pinned.loaded, [RegionId(1), RegionId(2)].into());
        assert!(game.actor_frozen(SOURCE));
        let clock = game.actor_clock(SOURCE);
        let since = game.actors[&SOURCE].active_since;
        let ready = game.actors[&SOURCE].ready_at;
        let recovery = game.creature(SOURCE).unwrap().health_recovery();
        until(&mut game, |game| {
            game.actors[&TARGET].motion.active_fall.is_none()
        });
        assert_eq!(game.alive(TARGET), !lethal);
        assert_eq!(xp(&game, SOURCE), if lethal { 25 } else { 0 });
        assert!(game.actor_frozen(SOURCE));
        assert_eq!(game.actor_clock(SOURCE), clock);
        assert_eq!(game.actors[&SOURCE].active_since, since);
        assert_eq!(game.actors[&SOURCE].ready_at, ready);
        assert_eq!(game.creature(SOURCE).unwrap().health_recovery(), recovery);
        let owner = serde_json::to_value(crate::actor_record::ActorRecord::capture(
            &game.actors[&SOURCE],
        ))
        .unwrap();
        let tick = game.tick();
        let (released, report) = game.transition_regions(&proposed, &mut records).unwrap();
        assert_eq!(released, proposed);
        assert_eq!(report.detached, vec![RegionId(2)]);
        assert!(report.thawed.is_empty());
        assert_eq!(game.tick(), tick);
        assert!(game.detached_actor(SOURCE));
        assert!(game.detached_records_valid(&mut records));
        let record = records
            .get(game.detached_record(RegionId(2)).unwrap())
            .unwrap();
        let detached =
            serde_json::to_value(crate::streaming::RegionRecordData::capture(&record)).unwrap();
        assert_eq!(detached["actors"]["1"], owner);
        assert_eq!(detached["stamps"]["1"], json!(clock));
    }
}

#[test]
fn frozen_falls_do_not_expand_the_active_victims_causal_owner_working_set() {
    use crate::streaming::RecordStore;
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    for region in 2..=8 {
        game.add_region(tor_world::Region {
            id: RegionId(region),
            name: format!("isolated fall pair {region}"),
            bounds: tor_world::Extent::new(9, 9, 9).unwrap(),
        })
        .unwrap();
    }
    let mut pairs = Vec::new();
    for victim_region in [1, 3, 5, 7] {
        let at = |x| tor_world::Location {
            region: RegionId(victim_region),
            ..cell(x, 3, 3)
        };
        let (owner, victim) = if victim_region == 1 {
            (SOURCE, TARGET)
        } else {
            let owner = game
                .spawn_actor(at(3), NonZeroU64::new(100).unwrap())
                .unwrap();
            let victim = game
                .spawn_actor(at(4), NonZeroU64::new(100).unwrap())
                .unwrap();
            configure_push_subject(&mut game, owner, "pushing_source");
            configure_push_subject(&mut game, victim, "pushed_target");
            (owner, victim)
        };
        // Tiny existing zero-gravity velocity keeps these ordinary admitted
        // pushes live while the other fixture pairs take their turns.
        game.set_actor_velocity(victim, [1, 0, 0]).unwrap();
        until(&mut game, |game| game.next_actor() == Some(owner));
        let mut action = push_action();
        let Action::UseAbility { invocation, .. } = &mut action else {
            unreachable!()
        };
        *invocation = crate::abilities::AbilityInvocation::ActorTarget { target: victim };
        game.admit_intention(owner, action, crate::IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        until(&mut game, |game| game.preparation(owner).is_none());
        assert_eq!(
            game.actors[&victim].motion.active_fall.unwrap().owner,
            owner
        );
        game.teleport(
            owner,
            tor_world::Location {
                region: RegionId(victim_region + 1),
                ..at(3)
            },
        )
        .unwrap();
        pairs.push((
            owner,
            victim,
            RegionId(victim_region),
            RegionId(victim_region + 1),
        ));
    }
    assert_eq!(game.actors.active_fall_owners().count(), pairs.len());
    let victim_regions = pairs.iter().map(|(_, _, region, _)| *region).collect();
    let frozen = crate::RegionTransition {
        active: Default::default(),
        loaded: victim_regions,
    };
    let mut records = crate::MemoryRecords::default();
    let (sets, _) = game.transition_regions(&frozen, &mut records).unwrap();
    assert_eq!(
        sets, frozen,
        "frozen victims do not recursively load any causal owners"
    );
    for (owner, victim, _, _) in &pairs {
        assert!(game.detached_actor(*owner));
        assert!(game.actor_frozen(*victim));
        assert!(game.actors[victim].motion.active_fall.is_some());
    }
    let owner_record = records
        .get(game.detached_record(RegionId(2)).unwrap())
        .unwrap();
    let owner_before =
        serde_json::to_value(crate::streaming::RegionRecordData::capture(&owner_record)).unwrap();
    let proposed = crate::RegionTransition {
        active: [RegionId(1)].into(),
        loaded: frozen.loaded.clone(),
    };
    let (sets, report) = game.transition_regions(&proposed, &mut records).unwrap();
    let mut expected_loaded = frozen.loaded;
    expected_loaded.insert(RegionId(2));
    assert_eq!(sets.active, [RegionId(1)].into());
    assert_eq!(
        sets.loaded, expected_loaded,
        "one active victim adds exactly its one owner region"
    );
    assert_eq!(report.attached, vec![RegionId(2)]);
    assert!(!report.thawed.contains(&RegionId(2)));
    assert!(game.actor_frozen(SOURCE));
    let owner_after = serde_json::to_value(crate::actor_record::ActorRecord::capture(
        &game.actors[&SOURCE],
    ))
    .unwrap();
    assert_eq!(owner_after, owner_before["actors"]["1"]);
    assert_eq!(json!(game.actor_clock(SOURCE)), owner_before["stamps"]["1"]);
    for (owner, victim, _, region) in &pairs[1..] {
        assert!(game.detached_actor(*owner));
        assert!(!sets.loaded.contains(region));
        assert!(game.actor_frozen(*victim));
    }
    assert!(game.detached_records_valid(&mut records));
}
