//! Real host transaction coverage; fixtures enter only through strict saved authority.
use super::*;
use serde_json::{json, Value};

pub(crate) fn scenario() -> Scenario {
    let mut scenario = crate::public_plan_test_fixture::growth_scenario();
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.characters[0]
        .creature
        .as_mut()
        .unwrap()
        .future_growth = crate::creature_authoring::FutureGrowth::Disabled {};
    let mut regions = package.region_defs().unwrap();
    regions[0].items.push(
        serde_json::from_value(json!({
            "id":10,"at":[2,1,0],"name":"Retained memento","carried_by":2
        }))
        .unwrap(),
    );
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            manifest,
            regions,
            &toml::to_string(package.rules().unwrap().document()).unwrap(),
        )
        .unwrap(),
    ));
    scenario
}
fn ability(id: &str, target: u64) -> Action {
    Action::UseAbility {
        id: tor_simulation::content_ids::AbilityId::new(id).unwrap(),
        invocation: crate::journal::AbilityInvocation {
            aim: crate::journal::AbilityAim::ActorTarget {
                target: SimActor(target),
            },
            weapon: crate::journal::WeaponSelection::Default,
        },
        supplies: Default::default(),
    }
}
pub(crate) fn finisher(engine: &Engine, queued: bool) -> Command {
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    let action = ability("growth_finisher", 2);
    if queued {
        Command::AdmitIntention {
            expected_revision,
            action,
        }
    } else {
        Command::Act {
            expected_revision,
            action,
        }
    }
}
fn submit(engine: &mut Engine, request: &str, command: Command) -> Result<CommandResult, Failure> {
    engine.command(
        "owner",
        "headless",
        ActorId(1),
        request,
        &engine.branch().clone(),
        command,
    )
}
pub(crate) fn authority(engine: &Engine) -> Value {
    json!({
        "checkpoint": Checkpoint::capture(engine).encode("capacity-fixture", engine.archive.records.len() as u64),
        "archive": engine.archive,
        "receipts": engine.receipts.iter().collect::<Vec<_>>(),
        "counts": engine.profile_counts(),
    })
}
fn live_items(engine: &Engine) -> Value {
    let disk = authority(engine);
    let index = disk["checkpoint"]["game"]["items"].as_u64().unwrap() as usize;
    disk["checkpoint"]["shared"]["items"][index].clone()
}
pub(crate) fn prepared(maximum: bool) -> Engine {
    let mut engine = Engine::memory(scenario()).unwrap();
    for (actor, target, id) in [(1, 2, "fixture_observe_plan"), (2, 1, "slow_plan")] {
        engine
            .command(
                "owner",
                "headless",
                ActorId(actor),
                id,
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(ActorId(actor)).unwrap(),
                    action: ability(id, target),
                },
            )
            .unwrap_or_else(|error| panic!("setup admission actor{actor} {id}: {error:?}"));
        assert_eq!(
            engine.next_intention_actor(),
            Some(ActorId(actor)),
            "setup due actor for {id}"
        );
        engine
            .execute_next_intention()
            .unwrap_or_else(|error| panic!("setup execution actor{actor} {id}: {error:?}"))
            .expect("setup admitted action executed");
    }
    assert_eq!(engine.next_actor(), Some(ActorId(1)));
    assert!(engine.game.public_plan_observation(SimActor(1)).is_some());
    if maximum {
        with_maximum_xp(&engine)
    } else {
        engine
    }
}

pub(crate) fn with_maximum_xp(engine: &Engine) -> Engine {
    let creature = engine.game.creature(SimActor(1)).unwrap();
    let rules = creature.build().ruleset();
    assert_eq!(rules.document().growth.threshold_base, 100);
    assert_eq!(rules.document().growth.threshold_per_tier, 0);
    let mut disk = serde_json::to_value(
        Checkpoint::capture(engine).encode("capacity-fixture", engine.archive.records.len() as u64),
    )
    .unwrap();
    let mut edited = 0;
    for pool in disk["shared"]["actors"].as_array_mut().unwrap() {
        let record = &mut pool["1"]["combat"]["creature"]["earned_progress"];
        assert!(record.is_object());
        record["earned_xp"] = json!(u64::MAX);
        record["held_xp"] = json!(u64::MAX);
        let applied = record["highest_applied_tier"].as_u64().unwrap();
        assert_eq!(record["authored_baseline"], applied);
        // This fixture's constant 100-XP price has an independent exact quotient.
        record["highest_earned_tier"] = json!(applied.checked_add(u64::MAX / 100).unwrap());
        assert_eq!(record["spent_xp"], 0);
        edited += 1;
    }
    assert!(edited > 0);
    // Conservation and retained-tier consistency are checked by the normal
    // restore path. This does not claim historical authenticity of saved XP.
    let restored: DiskCheckpoint = serde_json::from_value(disk).unwrap();
    let engine = restored.restore(engine.archive.clone()).unwrap();
    assert_eq!(
        engine
            .game
            .creature(SimActor(1))
            .unwrap()
            .earned_progress()
            .held_xp(),
        u64::MAX
    );
    engine
}
fn assert_capacity(failure: Failure) {
    assert_eq!(failure.code, ErrorCode::ResourceLimit);
    assert_eq!(failure.message, "Simulation resource capacity exhausted");
}

#[test]
fn fatal_xp_overflow_rolls_back_real_host_death_loot_and_receipt() {
    let mut control = prepared(false);
    let original_items = live_items(&control);
    let command = finisher(&control, false);
    submit(&mut control, "positive", command).unwrap();
    assert!(!control.alive(ActorId(2)));
    assert_eq!(
        control
            .game
            .creature(SimActor(1))
            .unwrap()
            .earned_progress()
            .held_xp(),
        100
    );
    assert_ne!(
        live_items(&control),
        original_items,
        "fatal control must commit corpse creation and carried-item transfer"
    );

    let mut engine = prepared(true);
    let game = engine.game.clone();
    let before = authority(&engine);
    let command = finisher(&engine, false);
    for _ in 0..2 {
        assert_capacity(submit(&mut engine, "capacity-retry", command.clone()).unwrap_err());
        assert_eq!(
            engine.game, game,
            "death, items, XP and all queued/continuation authority roll back atomically"
        );
        assert_eq!(
            authority(&engine),
            before,
            "revision, receipt, history and retained boundaries remain exact"
        );
    }
}

#[test]
fn queued_fatal_xp_overflow_retains_admission_across_checkpoint_tail_replay() {
    let mut engine = prepared(true);
    let checkpoint = Checkpoint::capture(&engine)
        .encode("capacity-fixture", engine.archive.records.len() as u64);
    let command = finisher(&engine, true);
    let receipt = submit(&mut engine, "queued-capacity", command.clone()).unwrap();
    assert_eq!(engine.next_intention_actor(), Some(ActorId(1)));
    let before = authority(&engine);
    for _ in 0..2 {
        assert_capacity(engine.execute_next_intention().unwrap_err());
        assert_eq!(
            authority(&engine),
            before,
            "capacity is retryable; no IntentionFailed/ContinuationFailed is published"
        );
    }
    let mut replay = Engine::replay(engine.archive.clone(), Some(checkpoint), None).unwrap();
    assert_eq!(
        authority(&replay),
        before,
        "strict checkpoint plus actual admission journal tail restores the same retryable work"
    );
    let duplicate = submit(&mut replay, "queued-capacity", command).unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.entry, receipt.entry);
    assert_capacity(replay.execute_next_intention().unwrap_err());
    assert_eq!(authority(&replay), before);
}

#[test]
fn queued_fatal_growth_store_refusal_preserves_admission_and_full_candidate() {
    let mut engine = prepared(false);
    let command = finisher(&engine, true);
    let receipt = submit(&mut engine, "queued-store", command.clone()).unwrap();
    let checkpoint =
        Checkpoint::capture(&engine).encode("store-fixture", engine.archive.records.len() as u64);
    let mut accepted = Engine::replay(engine.archive.clone(), Some(checkpoint), None).unwrap();
    accepted.execute_next_intention().unwrap().unwrap();
    assert!(
        !accepted.alive(ActorId(2)),
        "identical admitted work must actually reach fatal growth without Store refusal"
    );
    assert_eq!(
        accepted
            .game
            .creature(SimActor(1))
            .unwrap()
            .earned_progress()
            .held_xp(),
        100
    );
    let directory = tempfile::tempdir().unwrap();
    let mut engine = engine
        .attach_profile_save_with_policy(
            directory.path().join("refused.db"),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
    let before = authority(&engine);
    for _ in 0..2 {
        assert_eq!(
            engine.execute_next_intention().unwrap_err().code,
            ErrorCode::StorageFailure
        );
        assert_eq!(authority(&engine), before);
    }
    let duplicate = submit(&mut engine, "queued-store", command).unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.entry, receipt.entry);
    assert_eq!(authority(&engine), before);
}
