//! Physical translation, ordinary gravity and bounded-work regression controls.
use super::lifecycle::{ability_action, push_variant};
use super::*;
#[test]
fn each_primary_captures_a_fresh_contact_but_pays_the_ability_only_once() {
    let (mut game, source, target) = push_variant(true, |document| {
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        let mut second = ability["strikes"]["strikes"][0].clone();
        second["offset"] = json!(100);
        ability["strikes"]["strikes"]
            .as_array_mut()
            .unwrap()
            .push(second);
    });
    let identity = game
        .admit_intention(
            source,
            ability_action("push_probe", target),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.act(target, crate::Action::Wait).unwrap();
    game.actors.get_mut(&target).unwrap().turn_ticks = std::num::NonZeroU64::new(10).unwrap();
    game.act(target, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 110);
    assert_eq!(game.actors[&target].location, cell(5, 3, 3));
    assert_eq!(game.health(target), Some((26, 30)));
    assert!(game.preparation(source).unwrap().has_prefix());
    assert_eq!(game.preparation(source).unwrap().intention, Some(identity));
    assert!(game
        .creature(source)
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    game.actors.get_mut(&target).unwrap().turn_ticks = std::num::NonZeroU64::new(100).unwrap();
    // This ordinary action brings the target back into contact for primary two.
    game.act(target, crate::Action::Move(tor_world::Direction::West))
        .unwrap();
    assert_eq!(game.tick(), 210);
    assert_eq!(game.actors[&target].location, cell(5, 3, 3));
    assert_eq!(game.health(target), Some((22, 30)));
    assert!(game.preparation(source).is_none());
    assert_eq!(game.actors[&source].ready_at, 250);
    assert_eq!(
        game.creature(source)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        19
    );
}

#[test]
fn forced_push_preserves_existing_persistent_motion_and_fractional_remainders() {
    let facts = profile(CreatureSize::Medium, false);
    let mut pushed = push_game(facts, facts, true);
    let mut control = push_game(facts, facts, false);
    for game in [&mut pushed, &mut control] {
        game.actors.get_mut(&crate::ActorId(2)).unwrap().motion = crate::MotionState {
            velocity: [1, 2, 3],
            displacement: [7, 8, 9],
            acceleration_remainder: [4, 5, 6],
            active_fall: None,
        };
        start_push(game);
        finish_push(game);
    }
    assert_eq!(
        pushed
            .actor_motion(crate::ActorId(2))
            .unwrap()
            .without_fall(),
        control
            .actor_motion(crate::ActorId(2))
            .unwrap()
            .without_fall()
    );
    assert!(pushed
        .actor_motion(crate::ActorId(2))
        .unwrap()
        .active_fall
        .is_some());
    assert!(control
        .actor_motion(crate::ActorId(2))
        .unwrap()
        .active_fall
        .is_none());
    assert_eq!(pushed.actors[&crate::ActorId(2)].location, cell(5, 3, 3));
    assert_eq!(control.actors[&crate::ActorId(2)].location, cell(4, 3, 3));
    assert_eq!(
        pushed.health(crate::ActorId(2)),
        control.health(crate::ActorId(2))
    );
}

#[test]
fn forced_vertical_push_does_not_take_a_voluntary_stair_link() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    game.teleport(crate::ActorId(1), cell(4, 3, 4)).unwrap();
    game.connect(
        tor_world::Passage {
            from: cell(4, 3, 3),
            direction: tor_world::Direction::Down,
            to: cell(7, 7, 1),
        },
        0,
    )
    .unwrap();
    assert_eq!(
        game.actor_translation(crate::ActorId(2), tor_world::Direction::Down)
            .unwrap()
            .0,
        cell(7, 7, 1)
    );
    start_push(&mut game);
    finish_push(&mut game);
    assert_eq!(game.actors[&crate::ActorId(2)].location, cell(4, 3, 2));
    assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
}

#[test]
fn a_pushed_unsupported_body_enters_ordinary_gravity_fall_and_checkpoint_replay() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    let source = crate::ActorId(1);
    let target = crate::ActorId(2);
    game.set_wall(cell(3, 3, 2), true).unwrap();
    game.set_wall(cell(4, 3, 2), true).unwrap();
    game.set_gravity(tor_world::RegionId(1), [0, 0, -1])
        .unwrap();
    start_push(&mut game);
    game.act(target, crate::Action::Wait).unwrap();
    game.actors.get_mut(&target).unwrap().turn_ticks = std::num::NonZeroU64::new(10).unwrap();
    game.act(target, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 110);
    assert_eq!(game.actors[&target].location, cell(5, 3, 3));
    assert_eq!(game.actor_motion(target).unwrap().velocity, [0; 3]);
    assert_eq!(
        game.health(target),
        Some((26, 30)),
        "translation adds no impact damage"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let mut replay = crate::Game::restore_checkpoint(
        game.checkpoint(&mut shared),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    let mut saw_fall = false;
    let mut saw_impact = false;
    for _ in 0..100 {
        let actor = game.next_actor().unwrap();
        assert_eq!(
            game.act(actor, crate::Action::Wait),
            replay.act(actor, crate::Action::Wait)
        );
        assert_eq!(game, replay);
        saw_fall |= game.actors[&target].location.position.z < 3;
        saw_impact |= game
            .physics_impacts()
            .iter()
            .any(|impact| impact.entity == crate::PhysicsEntity::Actor(target));
        assert_eq!(game.actors[&source].location, cell(3, 3, 3));
        if game.actors[&target].location.position.z == 0
            && game.actor_motion(target).unwrap().velocity == [0; 3]
        {
            break;
        }
    }
    assert!(saw_fall && saw_impact);
    assert_eq!(game.actors[&target].location, cell(5, 3, 0));
}
#[test]
fn maximum_body_and_rider_counts_do_not_clone_games_or_recapture_plans_per_rider() {
    let mut measured = Vec::new();
    for riders in [1, crate::limits::RIDERS_PER_STRIKE] {
        let (mut game, source, target) = push_variant(true, |document| {
            let ability = document["abilities"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|ability| ability["id"] == "push_probe")
                .unwrap();
            let rider = ability["strikes"]["strikes"][0]["riders"][0].clone();
            ability["strikes"]["strikes"][0]["riders"] =
                serde_json::Value::Array(vec![rider; riders]);
        });
        game.teleport(target, cell(7, 3, 3)).unwrap();
        let cells: Vec<_> = (-2..=1)
            .flat_map(|x| (-2..=1).flat_map(move |y| (-2..=1).map(move |z| [x, y, z])))
            .collect();
        assert_eq!(cells.len(), 64);
        for actor in [source, target] {
            game.set_body(
                actor,
                crate::BodySpec {
                    cells: cells.clone(),
                    eye: [0; 3],
                    mass: 80,
                },
            )
            .unwrap();
        }
        start_push(&mut game);
        let before = crate::diagnostics::work_counts();
        finish_push(&mut game);
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            game.actors[&target].location,
            cell(7, 3, 3),
            "full-body boundary blocks each push"
        );
        assert_eq!(game.health(target), Some((26, 30)));
        assert_eq!(after.rider_resolutions - before.rider_resolutions, riders);
        assert_eq!(after.strike_resolutions - before.strike_resolutions, 1);
        assert_eq!(after.ability_plans - before.ability_plans, 0);
        measured.push((
            after.game_clones - before.game_clones,
            after.scenes - before.scenes,
            after.body_cells - before.body_cells,
        ));
    }
    assert_eq!(
        measured[0].0, measured[1].0,
        "outer ordinary action transactions only"
    );
    assert_eq!(
        measured[0].1, measured[1].1,
        "no rider-specific scene search"
    );
    assert!(measured[1].2 >= measured[0].2);
    assert!(measured[1].2-measured[0].2<=7*4*64,
        "each extra rider revalidates two bounded participant mappings and two candidate placements");
}
