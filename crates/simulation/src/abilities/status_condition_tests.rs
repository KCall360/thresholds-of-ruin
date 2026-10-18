//! Status denial shares the existing preparation lifecycle and actor clock.
use crate::{content_ids::ConditionId, Action, ActorId, Game, IntentionOrigin};
use serde_json::json;

fn sequence(protection: bool) -> (Game, crate::IntentionId) {
    sequence_restricting(protection, &["voluntary_actions"], "pause")
}
fn sequence_restricting(
    protection: bool,
    restrictions: &[&str],
    preparation: &str,
) -> (Game, crate::IntentionId) {
    super::strike_tests::sequence_game_custom(
        &[0, 100, 200],
        |ability| {
            ability.cooldown = std::num::NonZeroU64::new(700);
            ability.cost = Some(crate::rules::Cost {
                resource: crate::resources::Resource::Stamina,
                start: 1,
                resolution: 1,
            });
        },
        |rules, _| {
            rules.conditions.push(
                serde_json::from_value(json!({
                    "id":"fixture_stillness","family":"fixture_stillness",
                    "lifecycle":"timed","name":"Stillness","modifiers":[],
                    "stacking":{"kind":"keep"},"source_scoped":false,
                    "descriptors":[],"capacity":"reject",
                    "timing":{"kind":"actor_active","on_expiry":if protection {
                        json!({"kind":"protect","duration":400})
                    } else {json!({"kind":"remove"})}},
                    "restrictions":restrictions,"preparation":preparation
                }))
                .expect("generic status definition"),
            );
        },
    )
}

#[test]
fn held_movement_and_stronger_movement_ability_restrictions_are_distinct_operations() {
    for strong in [false, true] {
        let restrictions = if strong {
            vec!["ordinary_movement", "movement_abilities"]
        } else {
            vec!["ordinary_movement"]
        };
        let (mut game, intention) = sequence_restricting(false, &restrictions, "pause");
        game.tick = 5;
        apply(&mut game, 200);
        assert!(
            game.preparation(ActorId(1)).unwrap().active,
            "Held never pauses an ordinary attack"
        );
        let ordinary = Action::Move(tor_world::Direction::West);
        let attack = game.preparation(ActorId(1)).unwrap().plan.work().action();
        let movement = Action::UseAbility {
            id: crate::content_ids::AbilityId::new("guarded_step").unwrap(),
            invocation: super::AbilityInvocation::SelfStep {
                direction: super::StepDirection::West,
            },
            supplies: Default::default(),
        };
        assert!(game.condition_blocks_action(ActorId(1), &ordinary));
        assert!(!game.condition_blocks_action(ActorId(1), &attack));
        assert!(!game.condition_blocks_action(ActorId(1), &Action::Wait));
        assert_eq!(game.condition_blocks_action(ActorId(1), &movement), strong);
        let before = game.clone();
        assert!(game
            .cancel_intention_with_step(ActorId(1), intention, Some(tor_world::Direction::West))
            .is_err());
        assert_eq!(game, before);
        game.tick = 10;
        game.resolve_preparations().unwrap();
        assert!(
            game.preparation(ActorId(1)).unwrap().has_prefix(),
            "attacks still resolve"
        );
    }
}

fn apply(game: &mut Game, duration: u64) {
    game.apply_condition(
        ActorId(1),
        &ConditionId::new("fixture_stillness").unwrap(),
        ActorId(2),
        duration,
    )
    .unwrap();
}

fn round_trip(game: &Game) -> Game {
    assert!(
        game.next_actor()
            .is_none_or(|actor| game.actors[&actor].ready_at == game.tick()),
        "fixture saves must be at an actual scheduler input boundary"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let (saved, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(saved, shared)).unwrap()).unwrap();
    Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap()
}

fn ready_peer_for_snapshot(game: &mut Game) {
    // sequence_game_custom parks the peer at tick 1000 to test automatic
    // continuation. A mid-denial save instead needs an ordinary ready peer.
    let tick = game.tick();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = tick;
    assert_eq!(game.next_actor(), Some(ActorId(2)));
}

fn park_peer_for_automatic_thaw(game: &mut Game) {
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 1000;
}

#[test]
fn public_condition_application_preserves_own_interruption_across_immediate_reopen() {
    let (mut game, intention) = sequence(false);
    game.tick = 5;
    ready_peer_for_snapshot(&mut game);
    assert_eq!(
        game.apply_condition(
            ActorId(1),
            &ConditionId::new("fixture_stillness").unwrap(),
            ActorId(2),
            200,
        ),
        Ok(crate::conditions::ConditionUpdate::Applied)
    );
    assert!(matches!(
        game.combat_events().last(),
        Some(crate::combat::CombatEvent::Interrupted {
            actor: ActorId(1),
            intention: Some(id),
        }) if *id == intention
    ));
    let feedback = game.observe(ActorId(1)).unwrap().combat.unwrap().events;
    assert!(matches!(
        feedback.as_slice(),
        [crate::combat::DisclosedCombatEvent::Interrupted { actor: ActorId(1) }]
    ));
    let reopened = round_trip(&game);
    assert_eq!(reopened, game);
    assert_eq!(
        reopened.observe(ActorId(1)).unwrap().combat.unwrap().events,
        feedback
    );
}

#[test]
fn host_condition_application_preserves_unpublished_observer_extension() {
    let (mut game, _) = sequence(false);
    game.tick = 5;
    ready_peer_for_snapshot(&mut game);
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    game.apply(super::Outcome {
        actor: ActorId(1),
        target: ActorId(1),
        ability: Some(crate::content_ids::AbilityId::new("basic_melee").unwrap()),
        intention: None,
        result: super::OutcomeStatus::Applied,
        check: None,
        participation: super::AttemptedParticipation::empty(),
        target_rider_participated: false,
        effects: vec![],
    })
    .unwrap();
    apply(&mut game, 200);
    assert!(game.event_observations_open());
    assert!(game
        .observe(ActorId(2))
        .unwrap()
        .combat
        .unwrap()
        .events
        .is_empty());
    game.host_region_continuation = false;
    let witness = game.event_observation_witness();
    let (mut observation, _, proof) = game.observe_event_scene(ActorId(2)).unwrap();
    assert!(game.event_observation_pending(&proof));
    game.record_event_observation(&witness, &proof, &mut observation)
        .unwrap();
    assert!(matches!(
        observation.combat.unwrap().events.as_slice(),
        [crate::combat::DisclosedCombatEvent::Ability { .. }]
    ));
    game.seal_event_observations().unwrap();
    assert_eq!(round_trip(&game), game);
}

#[test]
fn public_condition_application_rolls_back_failed_event_publication() {
    let (mut game, _) = sequence(false);
    game.tick = 5;
    game.host_region_continuation = true;
    game.begin_event_batch().unwrap();
    for _ in 0..crate::limits::EVENTS_PER_BATCH {
        game.push_event(
            crate::combat::CombatEvent::Interrupted {
                actor: ActorId(2),
                intention: None,
            },
            None,
        )
        .unwrap();
    }
    game.seal_participant_events().unwrap();
    for host in [false, true] {
        game.host_region_continuation = host;
        let before = game.clone();
        assert_eq!(
            game.apply_condition(
                ActorId(1),
                &ConditionId::new("fixture_stillness").unwrap(),
                ActorId(2),
                200,
            ),
            Err(crate::GameError::Rule(super::RuleError::InvalidPlan))
        );
        assert_eq!(game, before);
        assert!(game.event_observations_open());
        assert!(game.preparation(ActorId(1)).unwrap().active);
    }
}

#[test]
fn status_pauses_unpaid_preparation_without_replacing_operands_holds_or_cooldown() {
    let (mut game, intention) = sequence(true);
    game.tick = 5;
    let before = game.preparation(ActorId(1)).unwrap().clone();
    apply(&mut game, 200);
    let paused = game.preparation(ActorId(1)).unwrap();
    assert!(!paused.active);
    assert_eq!(paused.remaining, 5);
    assert_eq!(paused.plan, before.plan);
    assert_eq!(paused.hold, before.hold);
    assert_eq!(paused.item_payment, before.item_payment);
    assert_eq!(paused.progress, before.progress);
    assert_eq!(paused.intention, Some(intention));
    let denied = game.clone();
    assert!(game.resume_intention(ActorId(1), intention).is_err());
    assert_eq!(game, denied);
    assert!(game
        .admit_intention(ActorId(1), Action::Wait, IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, denied);
    assert!(game
        .cancel_intention_with_step(ActorId(1), intention, Some(tor_world::Direction::West))
        .is_err());
    assert_eq!(game, denied);
    ready_peer_for_snapshot(&mut game);
    let mut restored = round_trip(&game);
    park_peer_for_automatic_thaw(&mut restored);
    restored.tick = 205;
    restored.settle_creature_time().unwrap();
    let thawed = restored.preparation(ActorId(1)).unwrap();
    assert!(!thawed.active, "expiry never resumes automatically");
    assert_eq!(thawed.remaining, 5);
    assert_eq!(thawed.plan, before.plan);
    restored.resume_intention(ActorId(1), intention).unwrap();
}

#[test]
fn status_retains_paid_prefix_and_cancel_pays_unresolved_commitment_after_thaw() {
    let (mut game, intention) = sequence(true);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    let frozen = game.preparation(ActorId(1)).unwrap().clone();
    assert!(frozen.has_prefix());
    apply(&mut game, 200);
    let paid_health = game.health(ActorId(2));
    let paused = game.preparation(ActorId(1)).unwrap().clone();
    let debt = paused.abandon_delay(215).unwrap();
    assert_eq!(paused.plan, frozen.plan);
    assert_eq!(paused.progress, frozen.progress);
    assert_eq!(paused.remaining, 95);
    ready_peer_for_snapshot(&mut game);
    game = round_trip(&game);
    park_peer_for_automatic_thaw(&mut game);
    game.tick = 215;
    game.settle_creature_time().unwrap();
    game.resolve_preparations().unwrap();
    assert_eq!(
        game.health(ActorId(2)),
        paid_health,
        "resolved prefix never repeats"
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 95);
    game.cancel_intention(ActorId(1), intention).unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.actors[&ActorId(1)].ready_at, 215 + debt);
}

#[test]
fn in_place_cancellation_remains_available_while_voluntary_work_is_denied() {
    let (mut game, intention) = sequence(false);
    game.tick = 5;
    apply(&mut game, 75);
    game.cancel_intention(ActorId(1), intention).unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.actors[&ActorId(1)].ready_at, 5);
    assert!(!game.creature(ActorId(1)).unwrap().conditions().is_empty());
}

#[test]
fn denied_human_input_does_not_stall_status_expiry_or_resume_its_paused_plan() {
    let (mut game, _) = sequence(true);
    game.tick = 5;
    apply(&mut game, 200);
    game.advance_to_next_decision().unwrap();
    assert_eq!(game.tick(), 205);
    assert_eq!(game.next_actor(), Some(ActorId(1)));
    let pending = game.preparation(ActorId(1)).unwrap();
    assert!(!pending.active);
    assert_eq!(pending.remaining, 5);
}

#[test]
fn status_does_not_replace_or_restart_existing_recovery_deadlines() {
    let (mut game, intention) = sequence(true);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    game.cancel_intention(ActorId(1), intention).unwrap();
    let recovery = game.actors[&ActorId(1)].ready_at;
    assert!(recovery > game.tick);
    apply(&mut game, 200);
    assert_eq!(game.actors[&ActorId(1)].ready_at, recovery);
    game.tick = 215;
    game.settle_creature_time().unwrap();
    assert_eq!(game.actors[&ActorId(1)].ready_at, recovery);
    assert!(game.preparation(ActorId(1)).is_none());
}

#[test]
fn past_readiness_restore_requires_current_active_denial_and_known_provenance() {
    let (mut game, _) = sequence(true);
    game.tick = 5;
    apply(&mut game, 200);
    game.tick = 50;
    ready_peer_for_snapshot(&mut game);
    assert_eq!(
        round_trip(&game),
        game,
        "lazy mid-denial readiness is grounded by its active condition"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let pool = serde_json::to_value(shared).unwrap();
    let actor_table = saved["actors"].as_u64().unwrap() as usize;
    for (field, value) in [
        ("phase", json!("protection")),
        ("remaining", json!(1)),
        ("source", json!(999)),
    ] {
        let mut invalid = pool.clone();
        invalid["actors"][actor_table]["1"]["combat"]["creature"]["conditions"][0][field] = value;
        let decoded = serde_json::from_value(invalid);
        assert!(
            decoded.is_err()
                || Game::restore_checkpoint(
                    serde_json::from_value(saved.clone()).unwrap(),
                    &decoded.unwrap(),
                    game.content().clone()
                )
                .is_err(),
            "{field}"
        );
    }
}

#[test]
fn a_queued_resume_is_suspended_by_new_denial_and_needs_a_fresh_resume_control() {
    let (mut game, intention) = sequence(true);
    game.tick = 5;
    game.pause_preparation(ActorId(1)).unwrap();
    game.resume_intention(ActorId(1), intention).unwrap();
    apply(&mut game, 200);
    assert_eq!(
        game.pending_intention(ActorId(1)).unwrap().state,
        crate::IntentionState::Suspended
    );
    let before = game.clone();
    assert!(game.resume_intention(ActorId(1), intention).is_err());
    assert_eq!(game, before);
    game.advance_to_next_decision().unwrap();
    assert_eq!(game.tick(), 205);
    assert!(game.next_intention_actor().is_none());
    assert!(!game.preparation(ActorId(1)).unwrap().active);
    assert_eq!(round_trip(&game), game);
    game.resume_intention(ActorId(1), intention).unwrap();
    assert_eq!(game.next_intention_actor(), Some(ActorId(1)));
}

fn forged_queued_resume_checkpoint(
    game: &Game,
) -> (crate::checkpoint::Snapshot, crate::checkpoint::SharedState) {
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut pool = serde_json::to_value(shared).unwrap();
    let entry = saved["intentions"]["entries"][0].as_u64().unwrap() as usize;
    assert_eq!(pool["intentions"]["entries"][entry]["state"], "suspended");
    pool["intentions"]["entries"][entry]["state"] = json!("queued");
    (
        serde_json::from_value(saved).unwrap(),
        serde_json::from_value(pool).unwrap(),
    )
}

#[test]
fn restore_rejects_a_status_suspended_resume_forged_back_to_queued() {
    let (mut game, intention) = sequence(true);
    game.tick = 5;
    game.pause_preparation(ActorId(1)).unwrap();
    game.resume_intention(ActorId(1), intention).unwrap();
    // No intermediate timer publication may incidentally repair the bad queue
    // before this denial expires and its phase becomes protection.
    apply(&mut game, 1);
    assert_eq!(
        game.pending_intention(ActorId(1)).unwrap().state,
        crate::IntentionState::Suspended
    );
    ready_peer_for_snapshot(&mut game);
    assert_eq!(round_trip(&game), game);
    let (saved, shared) = forged_queued_resume_checkpoint(&game);
    assert_eq!(
        Game::restore_checkpoint(saved, &shared, game.content().clone()).map(|_| ()),
        Err(crate::checkpoint::PersistError::Intentions)
    );

    game = round_trip(&game);
    park_peer_for_automatic_thaw(&mut game);
    game.advance_to_next_decision().unwrap();
    assert_eq!(game.tick(), 6);
    assert!(game.next_intention_actor().is_none());
    assert!(!game.preparation(ActorId(1)).unwrap().active);
    game.resume_intention(ActorId(1), intention).unwrap();
    assert_eq!(
        round_trip(&game),
        game,
        "a fresh explicit Resume after release remains valid"
    );

    let (mut fresh, intention) = sequence(true);
    fresh.cancel_intention(ActorId(1), intention).unwrap();
    fresh
        .admit_intention(ActorId(1), Action::Wait, IntentionOrigin::Human)
        .unwrap();
    apply(&mut fresh, 1);
    assert_eq!(
        fresh.pending_intention(ActorId(1)).unwrap().state,
        crate::IntentionState::Queued
    );
    ready_peer_for_snapshot(&mut fresh);
    assert_eq!(
        round_trip(&fresh),
        fresh,
        "fresh queued work is not a preparation Resume"
    );
}

#[test]
fn record_arrival_rejects_a_forged_queued_resume_before_status_reconciliation() {
    use std::collections::BTreeSet;
    use tor_world::RegionId;
    let (mut game, intention) = sequence(true);
    game.tick = 5;
    game.pause_preparation(ActorId(1)).unwrap();
    game.resume_intention(ActorId(1), intention).unwrap();
    apply(&mut game, 1);
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(
        &crate::RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::new(),
        },
        &mut records,
    )
    .unwrap();
    assert!(!game.actors.contains_key(&ActorId(1)));
    assert_eq!(round_trip(&game), game);
    let (saved, shared) = forged_queued_resume_checkpoint(&game);
    let mut forged = Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap();
    let before = forged.clone();
    assert!(forged
        .transition_regions(
            &crate::RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records
        )
        .is_err());
    assert_eq!(
        forged, before,
        "record admission rejects instead of repairing forged work"
    );
}

#[test]
fn matching_queued_actions_are_preparation_continuations_even_with_a_new_queue_identity() {
    for prefix in [false, true] {
        let (mut game, original_id) = sequence(true);
        if prefix {
            game.tick = 10;
            game.resolve_preparations().unwrap();
        }
        game.tick = if prefix { 15 } else { 5 };
        let prepared = game.preparation(ActorId(1)).unwrap().clone();
        let queued_id = game
            .admit_intention(
                ActorId(1),
                prepared.plan.work().action(),
                IntentionOrigin::Human,
            )
            .unwrap();
        assert_ne!(queued_id, original_id);
        apply(&mut game, 1);
        assert_eq!(
            game.pending_intention(ActorId(1)).unwrap().state,
            crate::IntentionState::Suspended
        );
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().intention,
            Some(original_id)
        );
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, prepared.plan);
        ready_peer_for_snapshot(&mut game);
        assert_eq!(round_trip(&game), game);
        let (saved, shared) = forged_queued_resume_checkpoint(&game);
        assert_eq!(
            Game::restore_checkpoint(saved, &shared, game.content().clone()).map(|_| ()),
            Err(crate::checkpoint::PersistError::Intentions)
        );
        game = round_trip(&game);
        park_peer_for_automatic_thaw(&mut game);
        let target_health = game.health(ActorId(2));
        game.advance_to_next_decision().unwrap();
        assert_eq!(game.tick(), if prefix { 16 } else { 6 });
        assert!(game.next_intention_actor().is_none());
        assert!(!game.preparation(ActorId(1)).unwrap().active);
        assert_eq!(game.health(ActorId(2)), target_health);
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            prepared.progress
        );
        // Stop at a real peer decision before either remaining strike can
        // resolve, so the resumed frozen plan and hold remain observable.
        let peer_decision = game.tick() + 1;
        game.actors.get_mut(&ActorId(2)).unwrap().ready_at = peer_decision;
        game.resume_intention(ActorId(1), queued_id).unwrap();
        assert_eq!(round_trip(&game), game);
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, queued_id);
        execution.outcome.unwrap();
        assert_eq!(game.tick(), peer_decision);
        assert_eq!(game.next_actor(), Some(ActorId(2)));
        assert!(game.preparation(ActorId(1)).unwrap().active);
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, prepared.plan);
        assert_eq!(game.preparation(ActorId(1)).unwrap().hold, prepared.hold);
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            prepared.progress
        );
        assert_eq!(game.health(ActorId(2)), target_health);
    }
}

#[test]
fn detached_admission_checks_matching_action_continuations_against_the_paid_plan() {
    use std::collections::BTreeSet;
    use tor_world::RegionId;
    let (mut game, original_id) = sequence(true);
    let queued_id = game
        .admit_intention(
            ActorId(1),
            game.preparation(ActorId(1)).unwrap().plan.work().action(),
            IntentionOrigin::Human,
        )
        .unwrap();
    assert_ne!(queued_id, original_id);
    apply(&mut game, 1);
    assert_eq!(
        game.pending_intention(ActorId(1)).unwrap().state,
        crate::IntentionState::Suspended
    );
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(
        &crate::RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::new(),
        },
        &mut records,
    )
    .unwrap();
    assert!(!game.actors.contains_key(&ActorId(1)));
    assert_eq!(round_trip(&game), game);
    let (saved, shared) = forged_queued_resume_checkpoint(&game);
    let mut forged = Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap();
    let before = forged.clone();
    assert!(forged
        .transition_regions(
            &crate::RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records
        )
        .is_err());
    assert_eq!(forged, before);
}

#[test]
fn suspended_concrete_autonomous_continuations_require_cancellation_and_redecision() {
    let (mut game, original_id) = sequence(true);
    let prepared = game.preparation(ActorId(1)).unwrap().clone();
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    let queued_id = game
        .admit_intention(
            ActorId(1),
            prepared.plan.work().action(),
            IntentionOrigin::Autonomous,
        )
        .unwrap();
    apply(&mut game, 1);
    assert_eq!(
        game.pending_intention(ActorId(1)).unwrap().state,
        crate::IntentionState::Suspended
    );
    ready_peer_for_snapshot(&mut game);
    game = round_trip(&game);
    park_peer_for_automatic_thaw(&mut game);
    game.advance_to_next_decision().unwrap();
    assert_eq!(game.tick(), 1);
    assert!(game.next_intention_actor().is_none());
    assert!(!game.preparation(ActorId(1)).unwrap().active);
    // A controller change does not rewrite the admitted queue's origin or
    // broaden the existing Human-only explicit Resume authorization.
    game.combat.ai.remove(&ActorId(1));
    let before = game.clone();
    assert!(game.resume_intention(ActorId(1), queued_id).is_err());
    assert_eq!(game, before);
    game.cancel_intention(ActorId(1), queued_id).unwrap();
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().intention,
        Some(original_id)
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, prepared.plan);
    assert_eq!(game.preparation(ActorId(1)).unwrap().hold, prepared.hold);
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    let decision = game.admit_ai_intention(ActorId(1)).unwrap();
    assert!(decision > queued_id);
    assert_eq!(
        game.pending_intention(ActorId(1)).unwrap().work,
        crate::intention::IntentionWork::AiDecision
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn applying_a_status_rejects_a_protection_deadline_overflow_atomically() {
    let (mut game, intention) = sequence(true);
    game.cancel_intention(ActorId(1), intention).unwrap();
    game.advance_physics(1_000).unwrap();
    game.tick = u64::MAX - 300;
    for id in game.actors.keys().copied().collect::<Vec<_>>() {
        let mut actor = game.actors.get_mut(&id).unwrap();
        actor.active_since = game.tick;
        actor.ready_at = game.tick;
    }
    let before = game.clone();
    assert_eq!(
        game.apply_condition(
            ActorId(1),
            &ConditionId::new("fixture_stillness").unwrap(),
            ActorId(2),
            200
        ),
        Err(crate::GameError::Schedule(
            crate::ScheduleError::TimeExhausted
        ))
    );
    assert_eq!(game, before);
}

#[test]
fn fresh_queued_work_waits_for_denial_expiry_without_becoming_prepared_work() {
    let (mut game, old) = sequence(true);
    game.cancel_intention(ActorId(1), old).unwrap();
    let queued = game
        .admit_intention(ActorId(1), Action::Wait, IntentionOrigin::Human)
        .unwrap();
    apply(&mut game, 200);
    assert_eq!(
        game.pending_intention(ActorId(1)).unwrap().state,
        crate::IntentionState::Queued
    );
    assert!(game.preparation(ActorId(1)).is_none());
    assert!(
        game.execute_next_intention().is_none(),
        "denied actors cannot execute their queued work"
    );
    game.advance_to_next_decision().unwrap();
    assert_eq!(game.tick(), 200);
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, queued);
    assert!(execution.outcome.is_ok());
}

#[test]
fn frozen_region_preserves_status_and_rejects_overflowing_thaw_atomically() {
    use std::collections::BTreeSet;
    use tor_world::RegionId;
    let (mut game, intention) = sequence(true);
    game.cancel_intention(ActorId(1), intention).unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 0;
    apply(&mut game, 200);
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(
        &crate::RegionTransition {
            active: BTreeSet::from([RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert!(
        game.actor_frozen(ActorId(1)),
        "denied input alone cannot pin an unobserved region"
    );
    let condition = game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .instances()
        .cloned()
        .collect::<Vec<_>>();
    game.tick = u64::MAX - 300;
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .cloned()
            .collect::<Vec<_>>(),
        condition
    );
    let before = game.clone();
    assert!(game
        .transition_regions(
            &crate::RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(2)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records
        )
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn independent_region_pins_preserve_active_clocks_and_thaw_reenables_input_pin() {
    use std::collections::BTreeSet;
    use tor_world::RegionId;
    let (mut game, intention) = sequence(true);
    game.cancel_intention(ActorId(1), intention).unwrap();
    apply(&mut game, 200);
    let point = game
        .add_reference_point(crate::ReferencePoint {
            target: crate::ReferenceTarget::Actor(ActorId(1)),
            active_radius: Some(0),
            load_radius: Some(0),
            observes: false,
        })
        .unwrap();
    let proposal = crate::RegionTransition {
        active: BTreeSet::from([RegionId(2)]),
        loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
    };
    let mut records = crate::MemoryRecords::default();
    let (actual, _) = game.transition_regions(&proposal, &mut records).unwrap();
    assert!(
        actual.active.contains(&RegionId(1)),
        "an independent point still pins the denied actor"
    );
    assert!(!game.condition_allows_voluntary_input(ActorId(1)));
    game.advance_physics(100).unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .conditions()
            .instances()
            .next()
            .unwrap()
            .remaining,
        100
    );
    game.advance_physics(200).unwrap();
    assert!(game.condition_allows_voluntary_input(ActorId(1)));
    assert!(game.remove_reference_point(point));
    let (actual, _) = game.transition_regions(&proposal, &mut records).unwrap();
    assert!(
        actual.active.contains(&RegionId(1)),
        "released actor again owns its ordinary input pin"
    );
}

#[test]
fn denied_input_marker_does_not_reattach_a_detached_actor_or_earn_catchup_time() {
    use std::collections::BTreeSet;
    use tor_world::RegionId;
    let (mut game, intention) = sequence(true);
    game.cancel_intention(ActorId(1), intention).unwrap();
    apply(&mut game, 200);
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&crate::combat::InputEligibility::Denied)
    );
    let expected = game.creature(ActorId(1)).unwrap().into_owned();
    assert!(
        game.region_requirements(
            &BTreeSet::from([RegionId(2)]),
            &BTreeSet::from([RegionId(2)])
        )
        .loaded
        .contains(&RegionId(1)),
        "an independently active neighboring region still pins its ordinary linked frontier"
    );
    let mut records = crate::MemoryRecords::default();
    let (actual, _) = game
        .transition_regions(
            &crate::RegionTransition {
                active: BTreeSet::new(),
                loaded: BTreeSet::new(),
            },
            &mut records,
        )
        .unwrap();
    assert!(!actual.loaded.contains(&RegionId(1)));
    assert!(!game.actors.contains_key(&ActorId(1)));
    game.tick = 2_000;
    game = round_trip(&game);
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&crate::combat::InputEligibility::Denied)
    );
    let (still_inactive, report) = game
        .transition_regions(&crate::RegionTransition::default(), &mut records)
        .unwrap();
    assert!(still_inactive.loaded.is_empty());
    assert!(
        report.attached.is_empty(),
        "denied saved input cannot silently reattach its actor"
    );
    game.transition_regions(
        &crate::RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.creature(ActorId(1)).unwrap().as_ref(), &expected);
    assert!(!game.condition_allows_voluntary_input(ActorId(1)));
    game.advance_physics(2_200).unwrap();
    assert!(game.condition_allows_voluntary_input(ActorId(1)));
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&crate::combat::InputEligibility::Permitted)
    );
    assert_eq!(round_trip(&game), game);
}

#[test]
fn boundary_membership_retains_explicit_ai_require_and_does_not_invent_prior_input() {
    use crate::combat::InputEligibility;
    let (mut game, _) = sequence(true);
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    assert!(!game.combat.input_boundaries.contains_key(&ActorId(1)));
    apply(&mut game, 200);
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&InputEligibility::Denied),
        "common preparation pause requires a boundary even for AI"
    );
    game.combat.ai.remove(&ActorId(1));
    ready_peer_for_snapshot(&mut game);
    game = round_trip(&game);
    park_peer_for_automatic_thaw(&mut game);
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    game.advance_physics(200).unwrap();
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&InputEligibility::Permitted)
    );
    assert!(!game.preparation(ActorId(1)).unwrap().active);

    let (mut game, intention) = sequence(true);
    game.cancel_intention(ActorId(1), intention).unwrap();
    game.begin_action_boundary(ActorId(1)).unwrap();
    apply(&mut game, 200);
    assert!(!game.combat.input_boundaries.contains_key(&ActorId(1)));
    game.configure_ai(ActorId(1), crate::ai::AiProfile::general())
        .unwrap();
    ready_peer_for_snapshot(&mut game);
    game = round_trip(&game);
    park_peer_for_automatic_thaw(&mut game);
    game.combat.ai.remove(&ActorId(1));
    game.advance_physics(200).unwrap();
    assert!(
        !game.combat.input_boundaries.contains_key(&ActorId(1)),
        "condition expiry cannot invent historical input membership"
    );
}

#[test]
fn input_eligibility_saved_map_rejects_unknown_shapes_duplicates_and_loaded_mismatches() {
    let (mut game, _) = sequence(true);
    apply(&mut game, 200);
    ready_peer_for_snapshot(&mut game);
    assert_eq!(round_trip(&game), game);
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    for value in [json!("permitted"), json!("pending")] {
        let mut invalid = saved.clone();
        invalid["combat"]["input_boundaries"]["1"] = value;
        assert!(Game::restore_checkpoint(
            serde_json::from_value(invalid).unwrap(),
            &shared,
            game.content().clone()
        )
        .is_err());
    }
    for value in [
        json!([1]),
        json!({"1":"unknown"}),
        json!({"1":{"denied":null}}),
    ] {
        let mut invalid = saved.clone();
        invalid["combat"]["input_boundaries"] = value;
        assert!(serde_json::from_value::<crate::checkpoint::Snapshot>(invalid).is_err());
    }
    let mut missing = saved.clone();
    missing["combat"]
        .as_object_mut()
        .unwrap()
        .remove("input_boundaries");
    assert!(serde_json::from_value::<crate::checkpoint::Snapshot>(missing).is_err());
    let raw = serde_json::to_string(&saved).unwrap();
    let duplicate = raw.replace(
        "\"input_boundaries\":{\"1\":\"denied\"}",
        "\"input_boundaries\":{\"1\":\"denied\",\"1\":\"denied\"}",
    );
    assert_ne!(raw, duplicate);
    assert!(serde_json::from_str::<crate::checkpoint::Snapshot>(&duplicate).is_err());
}

#[test]
fn detached_selected_configuration_resolves_pending_eligibility_before_live_admission() {
    use crate::combat::InputEligibility;
    use std::collections::{BTreeMap, BTreeSet};
    use tor_world::RegionId;
    let (mut game, intention) = sequence(true);
    game.cancel_intention(ActorId(1), intention).unwrap();
    apply(&mut game, 200);
    let mut records = crate::MemoryRecords::default();
    game.transition_regions(
        &crate::RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::new(),
        },
        &mut records,
    )
    .unwrap();
    assert!(!game.actors.contains_key(&ActorId(1)));
    assert_eq!(round_trip(&game), game);
    let proposal = crate::RegionTransition {
        active: BTreeSet::from([RegionId(1), RegionId(2)]),
        loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
    };
    let mut corrupt = game.clone();
    corrupt
        .combat
        .input_boundaries
        .insert(ActorId(1), InputEligibility::Permitted);
    let before = corrupt.clone();
    assert!(
        corrupt.transition_regions(&proposal, &mut records).is_err(),
        "known detached eligibility must match its immutable actor record"
    );
    assert_eq!(corrupt, before);
    game.configure_run(
        ActorId(1),
        BTreeSet::from([ActorId(1)]),
        None,
        BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&InputEligibility::Pending)
    );
    game = round_trip(&game);
    assert!(game.next_scheduled_actor().is_none());
    game.transition_regions(&proposal, &mut records).unwrap();
    assert_eq!(
        game.combat.input_boundaries.get(&ActorId(1)),
        Some(&InputEligibility::Denied)
    );
    assert!(!game.condition_allows_voluntary_input(ActorId(1)));
    ready_peer_for_snapshot(&mut game);
    assert_eq!(round_trip(&game), game);
}

#[test]
fn movement_only_denial_suspends_queued_resume_without_discarding_paid_step() {
    for pre_suspended in [false, true] {
        let mut game = super::movement_tests::guarded_step_game_with(100, |document| {
            document["conditions"].as_array_mut().unwrap().push(json!({
                "id":"fixture_anchor","family":"fixture_anchor","lifecycle":"timed",
                "name":"Anchor","modifiers":[],"source_scoped":false,
                "stacking":{"kind":"keep"},"descriptors":[],"capacity":"reject",
                "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                "restrictions":["movement_abilities"],"preparation":"pause"
            }));
        });
        let action = Action::UseAbility {
            id: crate::content_ids::AbilityId::new("guarded_step_probe").unwrap(),
            invocation: super::AbilityInvocation::SelfStep {
                direction: super::StepDirection::North,
            },
            supplies: Default::default(),
        };
        let intention = game
            .admit_intention(ActorId(1), action, IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        game.pause_preparation(ActorId(1)).unwrap();
        let paused = game.preparation(ActorId(1)).unwrap().clone();
        game.resume_intention(ActorId(1), intention).unwrap();
        if pre_suspended {
            game.suspend_intention(ActorId(1), intention).unwrap();
        }
        game.apply_condition(
            ActorId(1),
            &ConditionId::new("fixture_anchor").unwrap(),
            ActorId(2),
            200,
        )
        .unwrap();
        assert_eq!(
            game.pending_intention(ActorId(1)).unwrap().state,
            crate::IntentionState::Suspended
        );
        let before = game.clone();
        assert!(game.resume_intention(ActorId(1), intention).is_err());
        assert_eq!(game, before);
        assert!(game.execute_next_intention().is_none());
        assert_eq!(game.preparation(ActorId(1)), Some(&paused));
        assert_eq!(round_trip(&game), game);
    }
}
