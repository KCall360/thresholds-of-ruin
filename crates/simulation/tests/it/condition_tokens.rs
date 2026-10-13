//! Preparation-owned one-use conditions through public queue, restore and clocks.
use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::conditions::ConditionUpdate;
use tor_simulation::content_ids::{AbilityId, ConditionId};
use tor_simulation::{
    Action, ActorId, CreatureIdentity, Game, IntentionOrigin, MemoryRecords, RegionTransition,
};
use tor_world::{Location, Position, RegionId};

fn id(name: &str) -> ConditionId {
    ConditionId::new(name).unwrap()
}
fn attack(target: ActorId) -> Action {
    Action::UseAbility {
        id: AbilityId::new("basic_melee").unwrap(),
        target,
    }
}
fn plan(game: &Game, actor: ActorId, target: ActorId) -> tor_simulation::abilities::Plan {
    game.ability_plan(actor, &AbilityId::new("basic_melee").unwrap(), target)
        .unwrap()
}
fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(saved, shared)).unwrap();
    let (saved, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap()
}
fn start(game: &mut Game, actor: ActorId, target: ActorId) -> tor_simulation::IntentionId {
    let intention = game
        .admit_intention(actor, attack(target), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    intention
}

#[test]
fn token_queue_cancel_is_free_and_preparation_cancel_spends_it_across_restore() {
    for token in ["readied", "distracted", "exposed"] {
        let (mut game, actor, target) = super::abilities::duel(42);
        let holder = if token == "exposed" { target } else { actor };
        game.apply_condition(holder, &id(token), actor, 500)
            .unwrap();
        let queued = game
            .admit_intention(actor, attack(target), IntentionOrigin::Human)
            .unwrap();
        game.cancel_intention(actor, queued).unwrap();
        assert_eq!(plan(&game, actor, target).tokens().len(), 1);
        let intention = start(&mut game, actor, target);
        let prepared = game.preparation(actor).unwrap().plan.clone();
        assert_eq!(prepared.tokens().len(), 1);
        game = round_trip(&game);
        assert_eq!(game.preparation(actor).unwrap().plan, prepared);
        assert_eq!(
            game.apply_condition(holder, &id(token), target, 900)
                .unwrap(),
            ConditionUpdate::Reserved
        );
        game.cancel_intention(actor, intention).unwrap();
        assert!(game.creature(holder).unwrap().conditions().is_empty());
        assert!(plan(&game, actor, target).tokens().len() == 0);
        assert!(round_trip(&game)
            .creature(holder)
            .unwrap()
            .conditions()
            .is_empty());
    }
}

#[test]
fn exposed_is_claimed_by_only_the_first_preparation_and_retry_does_not_reclaim() {
    let (mut game, first, target) = super::abilities::duel(42);
    let build = game.creature(first).unwrap().build().clone();
    let second = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 3, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        second,
        CreatureIdentity {
            name: "competitor".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    game.apply_condition(target, &id("exposed"), target, 500)
        .unwrap();
    // Both admissions initially see the opening, but no claim exists yet.
    let first_intention = game
        .admit_intention(first, attack(target), IntentionOrigin::Human)
        .unwrap();
    game.admit_intention(second, attack(target), IntentionOrigin::Human)
        .unwrap();
    assert!(game
        .creature(target)
        .unwrap()
        .conditions()
        .instances()
        .all(|entry| entry.reserved_by.is_none()));
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.preparation(first).unwrap().plan.tokens().len(), 1);
    assert_eq!(
        plan(&game, second, target).as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .edge,
        0
    );
    assert!(plan(&game, second, target).tokens().len() == 0);
    game.pause_preparation(first).unwrap();
    let original = game.preparation(first).unwrap().plan.clone();
    // A resumed receipt is a new intention over the original frozen work.
    game.admit_intention(first, attack(target), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.preparation(first).unwrap().plan, original);
    assert_eq!(
        game.preparation(first).unwrap().origin_intention(),
        Some(first_intention)
    );
    assert_eq!(round_trip(&game).preparation(first).unwrap().plan, original);
}

#[test]
fn freezing_and_thawing_preserve_reserved_token_expiry_and_frozen_operands() {
    let (mut game, player, subject) = super::creature_time::game();
    let build = game.creature(subject).unwrap().build().clone();
    let target = game
        .spawn_actor(
            Location {
                region: RegionId(3),
                position: Position { x: 2, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        target,
        CreatureIdentity {
            name: "target".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    game.apply_condition(subject, &id("readied"), subject, 40)
        .unwrap();
    game.act(player, Action::Wait).unwrap();
    start(&mut game, subject, target);
    let prepared = game.preparation(subject).unwrap().plan.clone();
    let remaining = game
        .creature(subject)
        .unwrap()
        .conditions()
        .remaining(&id("readied"), subject);
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
        },
        &mut records,
    )
    .unwrap();
    game = round_trip(&game);
    for _ in 0..5 {
        game.act(player, Action::Wait).unwrap();
    }
    assert_eq!(
        game.creature(subject)
            .unwrap()
            .conditions()
            .remaining(&id("readied"), subject),
        remaining
    );
    assert_eq!(game.preparation(subject).unwrap().plan, prepared);
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(3)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(
        round_trip(&game).preparation(subject).unwrap().plan,
        prepared
    );
    for _ in 0..20 {
        if game.preparation(subject).is_none() {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    assert!(game.preparation(subject).is_none());
    assert!(game
        .creature(subject)
        .unwrap()
        .conditions()
        .remaining(&id("readied"), subject)
        .is_none());
}

#[test]
fn detached_token_partners_are_loaded_before_cross_region_reservations_are_revalidated() {
    let build = super::creature_state::build();
    let mut game = Game::region_corridor(42, 5, build.content().clone());
    let mut ids = Vec::new();
    for (region, x) in [(1, 1), (4, 11), (5, 0)] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(region),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject{region}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
        ids.push(actor);
    }
    let (player, owner, holder) = (ids[0], ids[1], ids[2]);
    game.apply_condition(holder, &id("exposed"), holder, 500)
        .unwrap();
    game.act(player, Action::Wait).unwrap();
    start(&mut game, owner, holder);
    let prepared = game.preparation(owner).unwrap().plan.clone();
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1)]),
        },
        &mut records,
    )
    .unwrap();
    assert!(game.creature(owner).is_none());
    assert!(game.creature(holder).is_none());
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(4)]),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.preparation(owner).unwrap().plan, prepared);
    assert!(game.creature(holder).is_some());
    assert_eq!(round_trip(&game).preparation(owner).unwrap().plan, prepared);
}

#[test]
fn a_stale_cancellation_cannot_consume_a_new_token_reservation_after_reapplication() {
    let (mut game, actor, target) = super::abilities::duel(42);
    game.apply_condition(actor, &id("readied"), actor, 500)
        .unwrap();
    let old = start(&mut game, actor, target);
    game.cancel_intention(actor, old).unwrap();
    game.apply_condition(actor, &id("readied"), actor, 500)
        .unwrap();
    let new = start(&mut game, actor, target);
    assert_ne!(old, new);
    let before = game.clone();
    assert!(game.cancel_intention(actor, old).is_err());
    assert_eq!(game, before);
    assert_eq!(
        game.preparation(actor).unwrap().origin_intention(),
        Some(new)
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .conditions()
            .instances()
            .next()
            .unwrap()
            .reserved_by,
        Some(actor)
    );
}

#[test]
fn token_partner_death_cleans_reservations_without_corrupting_the_other_plan() {
    use tor_simulation::abilities::{Outcome, OutcomeStatus, ResolvedEffect};
    for owner_dies in [false, true] {
        let (mut game, owner, holder) = super::abilities::duel(42);
        game.apply_condition(holder, &id("exposed"), holder, 500)
            .unwrap();
        let intention = start(&mut game, owner, holder);
        let original = game.preparation(owner).unwrap().plan.clone();
        let victim = if owner_dies { owner } else { holder };
        let damage = game.creature(victim).unwrap().health().maximum();
        game.apply(Outcome {
            actor: if owner_dies { holder } else { owner },
            target: victim,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            effects: vec![ResolvedEffect::Damage { amount: damage }],
        })
        .unwrap();
        assert!(game.creature(victim).unwrap().health().dead());
        assert!(game.creature(holder).unwrap().conditions().is_empty());
        game = round_trip(&game);
        if owner_dies {
            assert!(game.preparation(owner).is_none());
        } else {
            assert_eq!(game.preparation(owner).unwrap().plan, original);
            game.cancel_intention(owner, intention).unwrap();
            assert!(round_trip(&game).preparation(owner).is_none());
        }
    }
}
