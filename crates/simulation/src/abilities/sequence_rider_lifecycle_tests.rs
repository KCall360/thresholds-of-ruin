//! Retained sequence history crosses only the existing preparation boundaries.
use super::sequence_rider_tests::{rider_count, sequence_game};
use crate::combat::PreparationProgress;
use crate::rules::*;
use crate::{ActorId, Game, MemoryRecords, RecordStore, RegionTransition};
use serde_json::{json, Value};
use tor_world::RegionId;

fn lifecycle_game_with(configure: impl FnOnce(&mut [StrikeDef])) -> Game {
    let (game, _) = super::strike_tests::sequence_game_custom(
        &[0, 10, 20, 30],
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![RiderDef {
                    trigger: RiderTrigger::Success,
                    condition: Some(RiderCondition::SequenceSuccessfulHits { minimum: 2 }),
                    check: None,
                    effects: vec![Effect::Damage {
                        damage: DamageExpr::Components {
                            components: vec![DamageComponentExpr {
                                category: crate::combat::DamageType::Keen,
                                descriptor: None,
                                amount: AmountExpr::Rolled {
                                    dice: DiceExpr {
                                        count: 1,
                                        sides: 6,
                                        bonus: 0,
                                    },
                                },
                            }],
                        },
                    }],
                    descriptors: vec![],
                }],
            };
        },
        |_, species| {
            let mut strikes: Vec<_> = [0, 10, 20, 30]
                .into_iter()
                .map(|offset| StrikeDef {
                    offset,
                    unlock: None,
                    check: Some(CheckDef {
                        actor: CheckActor::User,
                        skill: SkillSelector::SelectedWeapon,
                        against: Against::Defense {
                            defense: DefenseKind::Physical,
                        },
                    }),
                    on_success: vec![damage(3)],
                    on_failure: vec![],
                    descriptors: vec![],
                    riders: vec![],
                })
                .collect();
            configure(&mut strikes);
            species.melee = crate::attacks::NaturalAttack::new(
                crate::attributes::Skill::HeavyWeaponry,
                100,
                10,
                20,
                strikes,
            )
            .unwrap();
        },
    );
    game
}

fn lifecycle_game() -> Game {
    lifecycle_game_with(|_| {})
}

fn damage(amount: u32) -> Effect {
    Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: crate::combat::DamageType::Impact,
                descriptor: None,
                amount: AmountExpr::Fixed { amount },
            }],
        },
    }
}

fn progress(next_strike: u16, successful_primaries: u16, receipt: u64) -> PreparationProgress {
    PreparationProgress::Ability {
        next_strike,
        successful_primaries,
        attempted_riders: [receipt, 0],
        linked_hold: None,
    }
}

fn resolve_at(game: &mut Game, tick: u64) {
    game.tick = tick;
    game.resolve_preparations().unwrap();
    // Checkpoints are taken at an ordinary next-decision boundary, as in the
    // existing ordered-strike checkpoint fixture.
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = tick;
}

fn roundtrip(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap()
}

fn assert_checkpoint_rejects_progress(game: &Game, value: Value, label: &str) {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut shared = serde_json::to_value(shared).unwrap();
    shared["actors"][snapshot["actors"].as_u64().unwrap() as usize]["1"]["pending"]["progress"] =
        value;
    let Ok(shared) = serde_json::from_value::<crate::checkpoint::SharedState>(shared) else {
        // Numeric and future-prefix bounds may reject at the stored decoder;
        // source-dependent slot/history checks belong to checkpoint admission.
        return;
    };
    assert!(
        Game::restore_checkpoint(
            serde_json::from_value(snapshot).unwrap(),
            &shared,
            game.content().clone(),
        )
        .is_err(),
        "checkpoint accepted {label}"
    );
}

fn all_regions() -> RegionTransition {
    RegionTransition {
        active: [RegionId(1), RegionId(2)].into(),
        loaded: [RegionId(1), RegionId(2)].into(),
    }
}

#[test]
fn sequence_rider_progress_requires_exact_bounded_fields() {
    let initial = json!({
        "kind": "ability", "next_strike": 0,
        "successful_primaries": 0, "attempted_riders": [0, 0], "linked_hold": null
    });
    assert_eq!(
        serde_json::from_value::<PreparationProgress>(initial.clone()).unwrap(),
        progress(0, 0, 0)
    );
    for field in [
        "next_strike",
        "successful_primaries",
        "attempted_riders",
        "linked_hold",
    ] {
        let mut missing = initial.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PreparationProgress>(missing).is_err(),
            "missing {field} must not become an empty history"
        );
    }
    for (field, value) in [
        ("next_strike", json!(-1)),
        ("next_strike", json!(65536)),
        ("successful_primaries", json!(-1)),
        ("successful_primaries", json!(65536)),
        ("attempted_riders", json!(null)),
        ("attempted_riders", json!([])),
        ("attempted_riders", json!([0])),
        ("attempted_riders", json!([0, 0, 0])),
        ("attempted_riders", json!([-1, 0])),
        ("unexpected", json!(0)),
    ] {
        let mut malformed = initial.clone();
        malformed[field] = value;
        assert!(serde_json::from_value::<PreparationProgress>(malformed).is_err());
    }
    let game = sequence_game();
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(0, 0, 0)
    );
    for forged in [progress(0, 1, 0), progress(0, 0, 1)] {
        let mut pending = game.preparation(ActorId(1)).unwrap().clone();
        pending.progress = forged;
        assert!(
            !pending.progress_valid(),
            "new preparation must have empty evidence"
        );
    }
}

#[test]
fn sequence_rider_pause_resume_preserves_prefix_and_once_receipt() {
    for completed in [1_u16, 2] {
        let mut game = lifecycle_game();
        for index in 1..=completed {
            resolve_at(&mut game, u64::from(index) * 10);
        }
        let expected = progress(completed, (1 << completed) - 1, u64::from(completed == 2));
        assert_eq!(game.preparation(ActorId(1)).unwrap().progress, expected);
        let pending = game.preparation(ActorId(1)).unwrap().clone();
        game.tick += 5;
        game.pause_preparation(ActorId(1)).unwrap();
        let rng = game.rules_rng;
        let hp = game.health(ActorId(2));
        game.tick += 1000;
        game.resolve_preparations().unwrap();
        assert_eq!(
            game.rules_rng, rng,
            "suspension cannot catch up checks or damage rolls"
        );
        assert_eq!(game.health(ActorId(2)), hp);
        assert_eq!(game.preparation(ActorId(1)).unwrap().progress, expected);
        game.start_work(pending.plan.clone(), pending.intention)
            .unwrap();
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, pending.plan);
        assert_eq!(game.preparation(ActorId(1)).unwrap().progress, expected);
        assert_eq!(game.rules_rng, rng);
        let next = game.tick + 5;
        for index in completed..4 {
            resolve_at(&mut game, next + u64::from(index - completed) * 10);
        }
        assert_eq!(
            rider_count(&game),
            1,
            "resume must neither forget hits nor reattempt the rider"
        );
        assert!(game.preparation(ActorId(1)).is_none());
        assert_eq!(game.actors[&ActorId(1)].ready_at, 1060);
    }
}

#[test]
fn sequence_rider_freeze_and_detach_preserve_history_without_rng_catchup() {
    for completed in [1_u16, 2] {
        for detach in [false, true] {
            let mut game = lifecycle_game();
            for index in 1..=completed {
                resolve_at(&mut game, u64::from(index) * 10);
            }
            let pending = game.preparation(ActorId(1)).unwrap().clone();
            let expected = progress(completed, (1 << completed) - 1, u64::from(completed == 2));
            assert_eq!(pending.progress, expected);
            let hp = game.health(ActorId(2));
            game.tick += 5;
            // This fixture advances the clock directly between decisions;
            // a living actor's next decision cannot predate its freeze stamp.
            game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
            let mut records = MemoryRecords::default();
            game.transition_regions_counted_deferred(
                &RegionTransition {
                    active: Default::default(),
                    loaded: if detach {
                        Default::default()
                    } else {
                        all_regions().loaded
                    },
                },
                &mut records,
            )
            .unwrap();
            let rng = game.rules_rng;
            game.tick += 1000;
            game.resolve_preparations().unwrap();
            assert_eq!(game.rules_rng, rng);
            let restored = roundtrip(&game);
            assert_eq!(
                restored, game,
                "checkpoint must retain frozen or detached ownership"
            );
            game = restored;
            game.transition_regions_counted_deferred(&all_regions(), &mut records)
                .unwrap();
            assert_eq!(game.preparation(ActorId(1)).unwrap().progress, expected);
            assert_eq!(game.preparation(ActorId(1)).unwrap().plan, pending.plan);
            assert_eq!(game.health(ActorId(2)), hp);
            assert_eq!(game.rules_rng, rng);
            let next = game.tick + 5;
            assert_eq!(game.actors[&ActorId(1)].ready_at, next);
            for index in completed..4 {
                resolve_at(&mut game, next + u64::from(index - completed) * 10);
            }
            assert_eq!(rider_count(&game), 1);
            assert!(game.preparation(ActorId(1)).is_none());
            assert_eq!(game.actors[&ActorId(1)].ready_at, 1060);
        }
    }
}

#[test]
fn sequence_rider_cancel_and_new_preparation_reset_history_and_latch() {
    let mut game = lifecycle_game();
    resolve_at(&mut game, 10);
    resolve_at(&mut game, 20);
    let pending = game.preparation(ActorId(1)).unwrap().clone();
    assert_eq!(pending.progress, progress(2, 0b11, 1));
    assert_eq!(rider_count(&game), 1);
    game.cancel_intention(ActorId(1), pending.intention.unwrap())
        .unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(
        game.actors[&ActorId(1)].ready_at,
        60,
        "cancellation keeps all paid tail debt"
    );
    game.tick = 60;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let new = game
        .admit_intention(
            ActorId(1),
            pending.plan.work().action(),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    assert_ne!(Some(new), pending.intention);
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(0, 0, 0)
    );
    resolve_at(&mut game, 70);
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(1, 1, 0)
    );
    assert_eq!(rider_count(&game), 0);
    resolve_at(&mut game, 80);
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(2, 0b11, 1)
    );
    assert_eq!(
        rider_count(&game),
        1,
        "the new preparation earns its own attempt"
    );
}

#[test]
fn sequence_rider_checkpoint_continuation_matches_uninterrupted_prefix_and_rng() {
    for completed in [1_u16, 2] {
        let mut game = lifecycle_game();
        for index in 1..=completed {
            resolve_at(&mut game, u64::from(index) * 10);
        }
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            progress(completed, (1 << completed) - 1, u64::from(completed == 2)),
        );
        let mut restored = roundtrip(&game);
        assert_eq!(restored, game);
        for index in completed + 1..=4 {
            resolve_at(&mut game, u64::from(index) * 10);
            resolve_at(&mut restored, u64::from(index) * 10);
            assert_eq!(
                restored, game,
                "HP, history, receipt, events and RNG continue together"
            );
        }
        assert_eq!(rider_count(&restored), 1);
    }
}

#[test]
fn sequence_rider_checkpoint_rejects_future_hits_and_forged_receipt_slots() {
    let mut game = lifecycle_game_with(|strikes| {
        strikes[0].riders.push(RiderDef {
            trigger: RiderTrigger::Success,
            condition: None,
            check: None,
            effects: vec![],
            descriptors: vec![],
        });
    });
    resolve_at(&mut game, 10);
    resolve_at(&mut game, 20);
    // The appended source occurrence is canonicalized to row zero's slot one,
    // even though row one's copy is its local slot zero (absolute slot eight).
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(2, 0b11, 0b10)
    );
    assert_eq!(roundtrip(&game), game);
    for (bad, label) in [
        (progress(2, 0b111, 0b10), "a hit at the unfinished cursor"),
        (
            progress(2, 1 << 15, 0),
            "a hit outside the captured strike list",
        ),
        (
            progress(2, 0b01, 0b10),
            "a receipt without enough qualifying primaries",
        ),
        (
            progress(2, 0b11, 0b11),
            "a receipt for an ordinary ungated rider",
        ),
        (progress(2, 0b11, 0b110), "a nonexistent rider slot"),
        (progress(2, 0b11, 1 << 8), "a noncanonical appended copy"),
    ] {
        assert_checkpoint_rejects_progress(&game, serde_json::to_value(bad).unwrap(), label);
    }
    assert_checkpoint_rejects_progress(
        &game,
        json!({
            "kind": "ability", "next_strike": 2,
            "successful_primaries": 3, "attempted_riders": [2, 1]
        }),
        "a receipt outside the captured rows in the upper word",
    );

    let mut later = lifecycle_game_with(|strikes| {
        strikes[3].riders.push(RiderDef {
            trigger: RiderTrigger::Success,
            condition: Some(RiderCondition::SequenceSuccessfulHits { minimum: 2 }),
            check: None,
            effects: vec![],
            descriptors: vec![],
        });
    });
    resolve_at(&mut later, 10);
    resolve_at(&mut later, 20);
    assert_eq!(
        later.preparation(ActorId(1)).unwrap().progress,
        progress(2, 0b11, 1)
    );
    assert_eq!(roundtrip(&later), later);
    assert_checkpoint_rejects_progress(
        &later,
        serde_json::to_value(progress(2, 0b11, 1 | (1 << 24))).unwrap(),
        "a valid gated source occurrence that has not executed yet",
    );
}

#[test]
fn sequence_rider_checkpoint_rejects_missing_receipt_for_loaded_live_target() {
    let mut game = lifecycle_game();
    resolve_at(&mut game, 10);
    resolve_at(&mut game, 20);
    assert!(game.alive(ActorId(2)));
    assert_eq!(rider_count(&game), 1);
    let mut pending = game.preparation(ActorId(1)).unwrap().clone();
    pending.progress = progress(2, 0b11, 0);
    assert!(
        pending.progress_valid(),
        "structural history alone cannot establish target survival"
    );
    assert_checkpoint_rejects_progress(
        &game,
        serde_json::to_value(pending.progress).unwrap(),
        "a missing eligible receipt while the committed target is alive",
    );
}

#[test]
fn sequence_rider_checkpoint_rejects_forged_hit_for_checkless_primary() {
    let mut game = lifecycle_game_with(|strikes| strikes[0].check = None);
    resolve_at(&mut game, 10);
    resolve_at(&mut game, 20);
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(2, 0b10, 0)
    );
    assert_eq!(rider_count(&game), 0);
    assert_eq!(roundtrip(&game), game);
    assert_checkpoint_rejects_progress(
        &game,
        serde_json::to_value(progress(2, 0b11, 1)).unwrap(),
        "a forged checkless hit used to justify the threshold and receipt",
    );
}

#[test]
fn sequence_rider_checkpoint_rejects_missing_receipt_proven_by_later_hit_even_if_dead() {
    let mut game = lifecycle_game_with(|strikes| strikes[2].on_success = vec![damage(1000)]);
    for tick in [10, 20, 30] {
        resolve_at(&mut game, tick);
    }
    assert!(!game.alive(ActorId(2)));
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(3, 0b111, 1)
    );
    assert_eq!(roundtrip(&game), game);
    let mut pending = game.preparation(ActorId(1)).unwrap().clone();
    pending.progress = progress(3, 0b111, 0);
    assert!(
        !pending.progress_valid(),
        "hit three proves survival of the eligible hit-two site"
    );
    assert_checkpoint_rejects_progress(
        &game,
        serde_json::to_value(pending.progress).unwrap(),
        "a missing receipt whose eligible site precedes another qualifying hit",
    );
}

#[test]
fn sequence_rider_lethal_eligible_primary_preserves_valid_unattempted_paid_tail() {
    for detach in [false, true] {
        let mut game = lifecycle_game_with(|strikes| strikes[1].on_success = vec![damage(1000)]);
        resolve_at(&mut game, 10);
        resolve_at(&mut game, 20);
        assert!(!game.alive(ActorId(2)));
        assert_eq!(
            rider_count(&game),
            0,
            "lethal primary skips the attempt and its damage roll"
        );
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            progress(2, 0b11, 0)
        );
        assert!(game.preparation(ActorId(1)).unwrap().progress_valid());
        let mut records = MemoryRecords::default();
        if detach {
            game.transition_regions_counted_deferred(
                &RegionTransition {
                    active: Default::default(),
                    loaded: Default::default(),
                },
                &mut records,
            )
            .unwrap();
        }
        game = roundtrip(&game);
        if detach {
            game.transition_regions_counted_deferred(&all_regions(), &mut records)
                .unwrap();
        }
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            progress(2, 0b11, 0)
        );
        let rng = game.rules_rng;
        resolve_at(&mut game, 30);
        assert_eq!(
            game.preparation(ActorId(1)).unwrap().progress,
            progress(3, 0b11, 0)
        );
        resolve_at(&mut game, 40);
        assert_eq!(
            game.rules_rng, rng,
            "dead-target tails do not create checks or rider attempts"
        );
        assert_eq!(rider_count(&game), 0);
        assert!(game.preparation(ActorId(1)).is_none());
        assert_eq!(game.actors[&ActorId(1)].ready_at, 60);
    }
}

#[test]
fn sequence_rider_detached_missing_receipt_rejects_alive_target_reattachment_atomically() {
    let mut game = lifecycle_game();
    resolve_at(&mut game, 10);
    resolve_at(&mut game, 20);
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(2, 0b11, 1)
    );
    let mut records = MemoryRecords::default();
    game.transition_regions_counted_deferred(
        &RegionTransition {
            active: Default::default(),
            loaded: Default::default(),
        },
        &mut records,
    )
    .unwrap();
    assert!(game.creature(ActorId(2)).is_none());
    let ids: Vec<_> = records.ids().collect();
    let mut forged_records = MemoryRecords::default();
    let mut forged = 0;
    for id in ids {
        let record = records.get(id).unwrap();
        let mut data = serde_json::to_value(crate::RegionRecordData::capture(&record)).unwrap();
        if data["actors"]["1"]["pending"].is_object() {
            data["actors"]["1"]["pending"]["progress"]["attempted_riders"] = json!([0, 0]);
            forged += 1;
        }
        let data: crate::RegionRecordData = serde_json::from_value(data).unwrap();
        let record = data.restore(game.content().clone()).unwrap();
        forged_records.put(id, tor_world::Shared::new(record));
    }
    assert_eq!(forged, 1, "corrupt exactly the retained source prefix");
    // Detached records do not materialize target Health during checkpoint load.
    // The existing attachment boundary must reject before making it executable.
    game = roundtrip(&game);
    let before = game.clone();
    let record_count = forged_records.len();
    assert!(game
        .transition_regions_counted_deferred(&all_regions(), &mut forged_records)
        .is_err());
    assert_eq!(
        game, before,
        "failed attachment cannot publish actors, history, HP or RNG"
    );
    assert_eq!(forged_records.len(), record_count);
    game.transition_regions_counted_deferred(&all_regions(), &mut records)
        .unwrap();
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        progress(2, 0b11, 1)
    );
    resolve_at(&mut game, 30);
    resolve_at(&mut game, 40);
    assert_eq!(
        rider_count(&game),
        1,
        "valid attachment never retries the consumed occurrence"
    );
}

#[test]
fn sequence_rider_failed_cancel_step_rolls_back_paid_hit_history_and_once_receipt() {
    let mut game = lifecycle_game();
    resolve_at(&mut game, 10);
    resolve_at(&mut game, 20);
    let pending = game.preparation(ActorId(1)).unwrap().clone();
    assert_eq!(pending.progress, progress(2, 0b11, 1));
    let before = game.clone();
    let hp = game.health(ActorId(2));
    let rng = game.rules_rng;
    assert!(
        game.cancel_intention_with_step(
            ActorId(1),
            pending.intention.unwrap(),
            Some(tor_world::Direction::East),
        )
        .is_err(),
        "the target occupies the requested escape cell"
    );
    assert_eq!(
        game, before,
        "the existing action transaction must restore the whole paid prefix"
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap(), &pending);
    assert_eq!(game.health(ActorId(2)), hp);
    assert_eq!(game.rules_rng, rng);
    for tick in [30, 40] {
        resolve_at(&mut game, tick);
    }
    assert_eq!(rider_count(&game), 1);
}
