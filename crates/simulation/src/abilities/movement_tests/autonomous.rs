//! Generic autonomous retreat uses the same typed selected operand.
use super::*;
#[test]
fn autonomous_retreat_selects_a_typed_self_step_and_keeps_ordinary_turn_identity() {
    let mut game = guarded_step_game(100);
    let idle = game
        .spawn_actor(cell(6, 6, 0), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    for (actor, faction) in [(crate::ActorId(1), "hero"), (crate::ActorId(2), "foe")] {
        game.actors
            .get_mut(&actor)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .identity = crate::CreatureIdentity {
            name: faction.into(),
            faction: faction.into(),
        };
    }
    game.configure_run(
        crate::ActorId(1),
        [crate::ActorId(1)].into(),
        None,
        [("foe".into(), ["hero".into()].into())].into(),
    )
    .unwrap();
    game.configure_ai(
        crate::ActorId(2),
        crate::ai::AiProfile {
            care: true,
            equipment: true,
            loot: true,
            gold: false,
            flee_percent: 100,
            idle: crate::behavior::IdleBehavior::Wait,
            doors: false,
        },
    )
    .unwrap();
    game.act(crate::ActorId(1), crate::Action::Wait).unwrap();
    assert_eq!(
        game.tick(),
        0,
        "the NPC must retain its normal decision boundary"
    );
    let (actor, action) = game.next_ai_action().unwrap();
    assert_eq!(actor, crate::ActorId(2));
    let crate::Action::UseAbility {
        invocation:
            crate::abilities::AbilityInvocation {
                aim: crate::abilities::AbilityAim::SelfStep { direction },
                ..
            },
        ..
    } = &action
    else {
        panic!("eligible self-step definitions must share the autonomous invocation pipeline");
    };
    let direction = *direction;
    let expected = game.actor_translation(actor, direction.into()).unwrap();
    let old = game.actors[&actor].location;
    let identity = game.admit_ai_intention(actor).unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, identity);
    assert_eq!(execution.action, Some(action));
    execution.outcome.unwrap();
    assert_eq!(game.preparation(actor).unwrap().intention, Some(identity));
    assert_eq!(game.actors[&actor].location, old);
    assert_eq!(game.next_actor(), Some(idle));
    game.act(idle, crate::Action::Wait).unwrap();
    assert_eq!(
        (
            game.actors[&actor].location,
            game.actors[&actor].orientation
        ),
        expected
    );
    assert_eq!(
        game.actors[&actor].ready_at,
        200 + crate::movement_cost(100, direction.into()).unwrap()
    );
    assert!(game.preparation(actor).is_none());
}
fn retreat_fixture() -> (crate::Game, crate::ActorId, crate::ActorId) {
    let mut game = guarded_step_game(100);
    let idle = game
        .spawn_actor(cell(6, 6, 0), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    for (actor, faction) in [(crate::ActorId(1), "hero"), (crate::ActorId(2), "foe")] {
        game.actors
            .get_mut(&actor)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .identity = crate::CreatureIdentity {
            name: faction.into(),
            faction: faction.into(),
        };
    }
    game.configure_run(
        crate::ActorId(1),
        [crate::ActorId(1)].into(),
        None,
        [("foe".into(), ["hero".into()].into())].into(),
    )
    .unwrap();
    game.configure_ai(
        crate::ActorId(2),
        crate::ai::AiProfile {
            care: true,
            equipment: true,
            loot: true,
            gold: false,
            flee_percent: 100,
            idle: crate::behavior::IdleBehavior::Wait,
            doors: false,
        },
    )
    .unwrap();
    game.act(crate::ActorId(1), crate::Action::Wait).unwrap();
    (game, crate::ActorId(2), idle)
}
#[test]
fn self_step_retreat_is_identical_across_undisclosed_world_twins_and_read_only() {
    let (a, actor, _) = retreat_fixture();
    let mut b = a.clone();
    b.add_region(tor_world::Region {
        id: tor_world::RegionId(2),
        name: "unseen".into(),
        bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
    })
    .unwrap();
    b.set_wall(
        tor_world::Location {
            region: tor_world::RegionId(2),
            position: tor_world::Position { x: 3, y: 3, z: 0 },
        },
        true,
    )
    .unwrap();
    let hidden = b
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(2),
                position: tor_world::Position { x: 6, y: 6, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    assert!(b
        .observe(actor)
        .unwrap()
        .visible_actors
        .iter()
        .all(|other| other.id != hidden));
    let a_before = a.clone();
    let b_before = b.clone();
    assert_eq!(a.next_ai_action(), b.next_ai_action());
    assert!(
        matches!(a.next_ai_action(),Some((id,crate::Action::UseAbility {
        invocation:crate::abilities::AbilityInvocation { aim: crate::abilities::AbilityAim::SelfStep { .. }, .. },..})) if id==actor)
    );
    assert_eq!(a, a_before);
    assert_eq!(b, b_before);
}
#[test]
fn canceled_self_steps_consume_start_cost_and_fall_back_to_an_ordinary_retreat() {
    let (mut game, actor, _) = retreat_fixture();
    for remaining in (2..=20).rev() {
        let identity = game.admit_ai_intention(actor).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert!(matches!(
            execution.action,
            Some(crate::Action::UseAbility {
                invocation: crate::abilities::AbilityInvocation {
                    aim: crate::abilities::AbilityAim::SelfStep { .. },
                    ..
                },
                ..
            })
        ));
        execution.outcome.unwrap();
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .costs()
                .resources()
                .balance(crate::resources::Resource::Stamina),
            remaining - 1
        );
        game.cancel_intention(actor, identity).unwrap();
        assert!(game.preparation(actor).is_none());
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        assert_eq!(game.tick(), 0);
    }
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .available(crate::resources::Resource::Stamina),
        1
    );
    let Some((id, crate::Action::Move(direction))) = game.next_ai_action() else {
        panic!("ordinary retreat remains available");
    };
    assert_eq!(id, actor);
    let old = game.actors[&actor].location;
    let expected = game.actor_translation(actor, direction).unwrap();
    game.admit_ai_intention(actor).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_ne!(old, expected.0);
    assert_eq!(
        (
            game.actors[&actor].location,
            game.actors[&actor].orientation
        ),
        expected
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        1
    );
}
#[test]
fn autonomous_late_block_retains_paid_self_step_and_braced_without_retargeting() {
    let (mut game, actor, idle) = retreat_fixture();
    let before = game.actors[&actor].location;
    let identity = game.admit_ai_intention(actor).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let frozen = game.preparation(actor).unwrap().plan.clone();
    let commitment = frozen.as_ability().unwrap().step.unwrap();
    game.set_wall(commitment.destination(), true).unwrap();
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        19
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .reservation(identity)
            .unwrap()
            .entries()
            .find(|cost| cost.resource == crate::resources::Resource::Stamina)
            .unwrap()
            .resolution,
        1
    );
    game.act(idle, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    assert_eq!(game.actors[&actor].location, before);
    assert_eq!(game.actors[&actor].ready_at, 200 + commitment.duration);
    assert!(game
        .creature(actor)
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        19
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .conditions()
            .instances()
            .find(|c| c.condition.as_str() == "braced")
            .unwrap()
            .remaining,
        Some(200)
    );
}
#[test]
fn a_paused_exact_self_step_resumes_its_original_hold_without_repayment() {
    let (mut game, actor, _) = retreat_fixture();
    let origin = game.admit_ai_intention(actor).unwrap();
    let first = game.execute_next_intention().unwrap();
    first.outcome.unwrap();
    let frozen = game.preparation(actor).unwrap().plan.clone();
    game.pause_preparation(actor).unwrap();
    let costs = game.creature(actor).unwrap().costs().clone();
    assert_eq!(game.next_ai_action().unwrap().1, first.action.unwrap());
    let resumed = game.admit_ai_intention(actor).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let pending = game.preparation(actor).unwrap();
    assert_eq!(pending.origin_intention(), Some(origin));
    assert_eq!(pending.intention, Some(resumed));
    assert_eq!(pending.plan, frozen);
    assert_eq!(game.creature(actor).unwrap().costs(), &costs);
}
