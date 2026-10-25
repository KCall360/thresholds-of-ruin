//! Native paid-item, active-clock and strict persistence witnesses. Visual
//! palette/disclosure and production package acceptance have independent owners.
use super::*;
use crate::perception::contract_fixtures::{at, OWNER, SUBJECT};
use crate::rules::*;
use crate::{Action, Game, ItemId, Work};
use std::num::NonZeroU64;
use tor_world::RegionId;

pub(crate) fn condition() -> crate::content_ids::ConditionId {
    crate::content_ids::ConditionId::new("fixture_hallucination").unwrap()
}
pub(crate) fn fixture() -> Game {
    crate::perception::contract_fixtures::fixture_with(&[], &[], |document, _, _| {
        document.conditions.push(ConditionDef {
            hallucinating: true,
            id: condition(),
            family: crate::content_ids::ConditionFamilyId::new("fixture_hallucination").unwrap(),
            name: "Disoriented".into(),
            lifecycle: ConditionLifecycle::Timed,
            modifiers: vec![],
            stacking: Stacking::Extend,
            source_scoped: false,
            descriptors: vec![],
            capacity: Eviction::Reject,
            timing: ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: ConditionPreparation::Preserve,
        });
    })
}
pub(crate) fn dose() -> Effect {
    Effect::ApplyConditionForTurns {
        condition: condition(),
        duration: AmountExpr::Rolled {
            dice: DiceExpr {
                count: 1,
                sides: 85,
                bonus: 807,
            },
        },
    }
}
pub(crate) fn potion(game: &mut Game, identity: &str, effects: Vec<Effect>) -> ItemId {
    let mut spec = crate::ItemSpec::ordinary("unmarked dose".into());
    spec.identity = identity.into();
    spec.concealed = true;
    spec.class = crate::ItemClass::Potion;
    spec.stackable = true;
    spec.consumable = Some(crate::ConsumableSpec {
        descriptors: vec![],
        discovery: crate::ItemDiscovery::OnUse,
        effects,
    });
    game.place_item_stack(70, game.actors[&OWNER].location, Some(OWNER), 3, spec)
        .unwrap()
}
pub(crate) fn start(game: &mut Game, item: ItemId) -> u64 {
    let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
    let due = game.tick() + plan.preparation();
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = game.tick();
    let id = game
        .admit_intention(OWNER, Action::Drink { item }, crate::IntentionOrigin::Human)
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert_eq!(game.preparation(OWNER).unwrap().plan, plan);
    assert_eq!(game.items[&item].quantity, 3);
    due
}
pub(crate) fn finish(game: &mut Game, due: u64) {
    // The real peer action advances the scheduler through paid completion. A
    // raw tick assignment would strand this ready peer in the past, creating
    // an invalid checkpoint before any cadence validation could run.
    assert_eq!(game.next_actor(), Some(SUBJECT));
    assert_eq!(game.actors[&SUBJECT].ready_at, game.tick());
    game.actors.get_mut(&SUBJECT).unwrap().turn_ticks = NonZeroU64::new(due - game.tick()).unwrap();
    game.act(SUBJECT, Action::Wait).unwrap();
    assert_eq!(game.tick(), due);
    assert!(game.preparation(OWNER).is_none());
}

fn advance_to(game: &mut Game, until: u64) {
    // Drive fractional active time with ordinary Wait work, preserving the
    // owner's captured base period and the scheduler's readiness invariants.
    // Only a wholly frozen/detached scene may advance without a ready actor.
    while game.tick() < until {
        let Some(actor) = game.next_actor() else {
            assert!(game.actors.keys().all(|id| game.actor_frozen(*id)));
            assert_eq!(game.advance_physics(until).unwrap(), until);
            break;
        };
        assert!(actor == OWNER || actor == SUBJECT);
        assert_eq!(game.actors[&actor].ready_at, game.tick());
        if actor == SUBJECT {
            game.actors.get_mut(&actor).unwrap().turn_ticks =
                NonZeroU64::new(until - game.tick()).unwrap();
        }
        game.act(actor, Action::Wait).unwrap();
        assert!(
            game.tick() <= until,
            "peer Wait must retain the requested decision boundary"
        );
    }
}
pub(crate) fn clock(game: &Game) -> (u64, u64, u64) {
    let cadence = game
        .projected_hallucination(OWNER)
        .unwrap()
        .expect("active presentation cadence");
    (cadence.generation(), cadence.phase(), cadence.next_in())
}
pub(crate) fn apply(game: &mut Game, duration: u64) {
    game.apply_condition(OWNER, &condition(), SUBJECT, duration)
        .unwrap();
}
pub(crate) fn roundtrip(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
}

#[test]
fn paid_hallucination_starts_phase_zero_at_completion_not_old_interval() {
    let mut game = fixture();
    let item = potion(&mut game, "fixture.hallucination_dose", vec![dose()]);
    let plan = game.plan_work(OWNER, Work::Drink { item }).unwrap();
    assert!(matches!(
        &plan.on_success()[0],
        crate::abilities::PlannedEffect::ApplyConditionForTurns {
            turn_ticks: 100,
            ..
        }
    ));
    let initial_rng = game.rules_rng;
    let mut expected_rng = initial_rng;
    let turns = crate::dice::DicePool::new(1, 85, 807)
        .unwrap()
        .roll(&mut expected_rng, crate::dice::Edge::default())
        .total;
    let due = start(&mut game, item);
    assert_eq!(due, 100);
    assert!(game.projected_hallucination(OWNER).unwrap().is_none());
    assert_eq!(game.rules_rng, initial_rng);
    finish(&mut game, due);
    assert_eq!(game.items[&item].quantity, 2);
    assert!(game.actors[&OWNER]
        .knowledge
        .contains("fixture.hallucination_dose"));
    assert_eq!(game.rules_rng, expected_rng);
    assert_eq!(
        hallucination_instance(&game.actors[&OWNER]).unwrap().1,
        u64::from(turns) * 100
    );
    assert_eq!(
        clock(&game),
        (0, 0, 100),
        "new condition must not inherit the paid preparation interval"
    );
    let before = game.clone();
    for _ in 0..3 {
        assert_eq!(clock(&game), (0, 0, 100));
        game.observe(OWNER).unwrap();
    }
    assert_eq!(game, before, "queries may not settle or draw randomness");
    assert_eq!(roundtrip(&game), game);
}

#[test]
fn ordinary_peer_drives_exact_shift_without_owner_action_or_random_draw() {
    let mut game = fixture();
    apply(&mut game, 1000);
    let peer = game
        .spawn_actor(at(1, 5, 4), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(
        game.actors[&peer].combat.is_none(),
        "ordinary actor is the external clock driver"
    );
    game.actors.get_mut(&OWNER).unwrap().ready_at = 10_000;
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = 10_000;
    let rng = game.rules_rng;
    assert_eq!(game.next_creature_event_tick(), Some(100));
    game.act(peer, Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    assert_eq!(clock(&game), (0, 1, 100));
    assert_eq!(
        game.actors[&OWNER].hallucination.as_ref().unwrap().phase(),
        1,
        "peer time must materialize the due owner, not only query-project it"
    );
    assert_eq!(game.rules_rng, rng);
    assert!(game.actors[&peer].hallucination.is_none());
}

#[test]
fn extension_keeps_deadline_and_same_transaction_reapplication_resets_generation() {
    let mut game = fixture();
    apply(&mut game, 1000);
    advance_to(&mut game, 50);
    apply(&mut game, 200);
    assert_eq!(clock(&game), (0, 0, 50));
    assert_eq!(
        hallucination_instance(&game.actors[&OWNER]),
        Some((0, 1150))
    );
    assert_eq!(game.next_creature_event_tick(), Some(100));
    advance_to(&mut game, 100);
    advance_to(&mut game, 150);
    assert_eq!(clock(&game), (0, 1, 50));
    game.apply_effects(
        OWNER,
        &[
            Effect::RemoveCondition {
                condition: condition(),
            },
            Effect::ApplyConditionForTurns {
                condition: condition(),
                duration: AmountExpr::Fixed { amount: 3 },
            },
        ],
    )
    .unwrap();
    assert_eq!(clock(&game), (1, 0, 100));
    assert_eq!(hallucination_instance(&game.actors[&OWNER]), Some((1, 300)));
    assert_eq!(game.next_creature_event_tick(), Some(250));
}

#[test]
fn blind_paid_cure_clears_only_marker_while_ordinary_healing_keeps_it() {
    let mut game = fixture();
    apply(&mut game, 1000);
    crate::perception::contract_fixtures::apply(&mut game, OWNER, "fixture_blind", 1000);
    game.apply_effects(
        OWNER,
        &[Effect::Heal {
            amount: AmountExpr::Fixed { amount: 1 },
        }],
    )
    .unwrap();
    assert_eq!(clock(&game), (0, 0, 100));
    let item = potion(
        &mut game,
        "fixture.hallucination_cure",
        vec![Effect::RemoveCondition {
            condition: condition(),
        }],
    );
    let due = start(&mut game, item);
    finish(&mut game, due);
    assert!(game.projected_hallucination(OWNER).unwrap().is_none());
    assert!(game.actors[&OWNER].hallucination.is_none());
    assert!(game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&crate::content_ids::ConditionId::new("fixture_blind").unwrap()));
    assert!(!game.actors[&OWNER].perception().sight);
    assert!(game.actors[&OWNER]
        .knowledge
        .contains("fixture.hallucination_cure"));
    assert_eq!(game.items[&item].quantity, 2);
}

#[test]
fn expiry_precedes_tied_shift_and_bounds_next_event_before_long_period() {
    for duration in [50, 100] {
        let mut game = fixture();
        apply(&mut game, duration);
        assert_eq!(game.next_creature_event_tick(), Some(duration));
        advance_to(&mut game, duration);
        assert!(game.actors[&OWNER].hallucination.is_none());
        assert!(game.projected_hallucination(OWNER).unwrap().is_none());
        assert!(hallucination_instance(&game.actors[&OWNER]).is_none());
    }
    let mut game = fixture();
    advance_to(&mut game, 75);
    apply(&mut game, 1000);
    assert_eq!(clock(&game), (0, 0, 100));
    assert_eq!(game.next_creature_event_tick(), Some(175));
}

#[test]
fn frozen_and_detached_cadence_keep_fraction_through_real_record_roundtrip() {
    use crate::streaming::{MemoryRecords, RecordStore, RegionRecordData, RegionTransition};
    for detached in [false, true] {
        let mut game = fixture();
        apply(&mut game, 1000);
        advance_to(&mut game, 50);
        let mut records = MemoryRecords::default();
        game.apply_region_transition(
            &RegionTransition {
                active: Default::default(),
                loaded: if detached {
                    Default::default()
                } else {
                    [RegionId(1)].into()
                },
            },
            &mut records,
        )
        .unwrap();
        let mut reopened_records = MemoryRecords::default();
        for id in records.ids().collect::<Vec<_>>() {
            let data = RegionRecordData::capture(&records.get(id).unwrap());
            let bytes = serde_json::to_vec(&data).unwrap();
            let decoded: RegionRecordData = serde_json::from_slice(&bytes).unwrap();
            reopened_records.put(
                id,
                tor_world::Shared::new(decoded.restore(game.content().clone()).unwrap()),
            );
        }
        let mut game = roundtrip(&game);
        advance_to(&mut game, 500);
        if !detached {
            assert_eq!(clock(&game), (0, 0, 50));
        }
        game.apply_region_transition(
            &RegionTransition {
                active: [RegionId(1)].into(),
                loaded: [RegionId(1)].into(),
            },
            &mut reopened_records,
        )
        .unwrap();
        assert_eq!(clock(&game), (0, 0, 50));
        assert_eq!(hallucination_instance(&game.actors[&OWNER]), Some((0, 950)));
        advance_to(&mut game, 549);
        assert_eq!(clock(&game), (0, 0, 1));
        advance_to(&mut game, 550);
        assert_eq!(clock(&game), (0, 1, 100));
        assert_eq!(roundtrip(&game), game);
    }
}

#[test]
fn current_and_retained_history_require_matching_cadence_and_reject_impossible_age() {
    let mut game = fixture();
    apply(&mut game, 1000);
    advance_to(&mut game, 50);
    game.settle_actor_time(OWNER).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let history = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    advance_to(&mut game, 100);
    let current = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let baseline = serde_json::to_value(&shared).unwrap();
    for snapshot in [history, current] {
        let index = snapshot["actors"].as_u64().unwrap() as usize;
        let restore = |value: serde_json::Value| {
            let shared: crate::checkpoint::SharedState = serde_json::from_value(value).unwrap();
            Game::restore_checkpoint(
                serde_json::from_value(snapshot.clone()).unwrap(),
                &shared,
                game.content().clone(),
            )
        };
        restore(baseline.clone()).unwrap();
        for (field, value) in [
            ("generation", serde_json::json!(42)),
            ("phase", serde_json::json!(u64::MAX)),
        ] {
            let mut invalid = baseline.clone();
            invalid["actors"][index]["1"]["hallucination"][field] = value;
            assert!(matches!(
                restore(invalid),
                Err(crate::checkpoint::PersistError::ActorState { actor: OWNER })
            ));
        }
        let mut absent = baseline.clone();
        absent["actors"][index]["1"]["hallucination"] = serde_json::Value::Null;
        assert!(matches!(
            restore(absent),
            Err(crate::checkpoint::PersistError::ActorState { actor: OWNER })
        ));
        let mut missing = baseline.clone();
        missing["actors"][index]["1"]
            .as_object_mut()
            .unwrap()
            .remove("hallucination");
        assert!(serde_json::from_value::<crate::checkpoint::SharedState>(missing).is_err());
        let mut orphan = baseline.clone();
        orphan["actors"][index]["2"]["hallucination"] =
            baseline["actors"][index]["1"]["hallucination"].clone();
        assert!(matches!(
            restore(orphan),
            Err(crate::checkpoint::PersistError::ActorState { actor: SUBJECT })
        ));
        for (field, value) in [
            ("period", 0),
            ("remaining_to_shift", 0),
            ("remaining_to_shift", 101),
        ] {
            let mut invalid = baseline.clone();
            invalid["actors"][index]["1"]["hallucination"][field] = serde_json::json!(value);
            assert!(serde_json::from_value::<crate::checkpoint::SharedState>(invalid).is_err());
        }
    }
}

#[test]
fn detached_record_rejects_clock_without_condition_and_missing_or_mismatched_clock() {
    use crate::streaming::{MemoryRecords, RecordStore, RegionRecordData, RegionTransition};
    let mut game = fixture();
    apply(&mut game, 1000);
    let mut records = MemoryRecords::default();
    game.apply_region_transition(&RegionTransition::default(), &mut records)
        .unwrap();
    let record_id = records.ids().next().unwrap();
    let data = RegionRecordData::capture(&records.get(record_id).unwrap());
    let value = serde_json::to_value(&data).unwrap();
    data.restore(game.content().clone()).unwrap();
    for actor in ["1", "2"] {
        let mut invalid = value.clone();
        if actor == "1" {
            invalid["actors"][actor]["hallucination"] = serde_json::Value::Null;
        } else {
            invalid["actors"][actor]["hallucination"] =
                value["actors"]["1"]["hallucination"].clone();
        }
        let record: RegionRecordData = serde_json::from_value(invalid).unwrap();
        assert!(record.restore(game.content().clone()).is_err());
    }
    let mut mismatch = value.clone();
    mismatch["actors"]["1"]["hallucination"]["generation"] = serde_json::json!(42);
    assert!(serde_json::from_value::<RegionRecordData>(mismatch)
        .unwrap()
        .restore(game.content().clone())
        .is_err());
    let mut missing = value;
    missing["actors"]["1"]
        .as_object_mut()
        .unwrap()
        .remove("hallucination");
    assert!(serde_json::from_value::<RegionRecordData>(missing).is_err());
}

#[test]
fn cadence_horizon_uses_expiry_and_unmarked_owners_cannot_keep_clock() {
    let mut game = fixture();
    apply(&mut game, 5);
    let mut actor = game.actors[&OWNER].clone();
    actor.active_since = u64::MAX - 10;
    assert!(
        valid_for(&actor),
        "expiry fits even though the later cadence shift does not"
    );
    actor.active_since = u64::MAX - 4;
    assert!(!valid_for(&actor), "the actual earliest boundary must fit");
    let mut ordinary = game.actors[&SUBJECT].clone();
    ordinary.hallucination = actor.hallucination.clone();
    assert!(!valid_for(&ordinary));
}

#[test]
fn extension_and_restore_retain_captured_period_not_changed_base_timing() {
    let mut game = fixture();
    apply(&mut game, 1000);
    advance_to(&mut game, 50);
    // No production setter changes base timing today; this native witness pins
    // the captured-period contract rather than silently imposing equality.
    game.actors.get_mut(&OWNER).unwrap().turn_ticks = NonZeroU64::new(75).unwrap();
    apply(&mut game, 100);
    assert_eq!(clock(&game), (0, 0, 50));
    let restored = roundtrip(&game);
    assert_eq!(restored.actors[&OWNER].turn_ticks.get(), 75);
    assert_eq!(clock(&restored), (0, 0, 50));
    assert_eq!(restored.next_creature_event_tick(), Some(100));
}

#[test]
fn death_clears_cadence_and_restore_rejects_posthumous_clock() {
    let mut game = fixture();
    apply(&mut game, 1000);
    let active = game.actors[&OWNER].hallucination.clone();
    game.apply(crate::abilities::Outcome {
        resistance: None,
        actor: SUBJECT,
        target: OWNER,
        ability: None,
        intention: None,
        result: crate::abilities::OutcomeStatus::Applied,
        check: None,
        participation: crate::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![crate::abilities::ResolvedEffect::Damage { amount: 1_000_000 }],
    })
    .unwrap();
    assert!(!game.actors[&OWNER].alive());
    assert!(game.actors[&OWNER].hallucination.is_none());
    assert!(game.projected_hallucination(OWNER).unwrap().is_none());
    let mut dead = game.actors[&OWNER].clone();
    dead.hallucination = active;
    assert!(!valid_for(&dead));
    let record = crate::actor_record::ActorRecord::capture(&dead);
    assert!(record
        .restore(OWNER, game.content().clone(), &game.items)
        .is_err());
}
