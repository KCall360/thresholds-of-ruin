//! Generated-entry and Store refusal retain all authority, not only live actors.
use super::*;

fn growth_fixture(start: u32, base: usize) -> Scenario {
    let mut scenario = single_member_foe_fixture(true);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["characters"][0]["anchor"] = json!(format!("dungeon/{start}/up"));
    source["characters"].as_array_mut().unwrap().truncate(1);
    let row = source["archetypes"]["rat"]["creature"]["hit_dice"][0].clone();
    source["archetypes"]["rat"]["creature"]["hit_dice"] = json!(vec![row; base]);
    let stage = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][2];
    stage["ai"] = Value::Null;
    stage["bands"][0]["depth"]["minimum"] = json!(start + 1);
    stage["depth_growth"] = json!({"kind":"racial_after","depth":26});
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).expect("racial depth policy is valid authoring"),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    scenario
}
fn full_authority(engine: &Engine) -> Value {
    let regions = engine.regions.as_ref().unwrap();
    json!({
        "authority": super::super::growth_capacity_transaction_tests::authority(engine),
        "region_records": regions.encoded_resident().unwrap(),
        "pending_sources": regions.unsaved_sources().unwrap(),
    })
}
fn assert_capacity(failure: Failure) {
    assert_eq!(failure.code, ErrorCode::ResourceLimit);
    assert!(failure.message.contains("capacity"), "{failure:?}");
    assert!(
        !failure.message.contains("rat"),
        "no hidden occupant/build disclosure"
    );
}
#[test]
fn racial_growth_generated_destination_capacity_is_retryable_after_replay() {
    let mut engine = Engine::memory(growth_fixture(275, 7)).unwrap();
    walk_to_port(&mut engine, 275, "down");
    let before = full_authority(&engine);
    let command = Command::Act {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        action: Action::Move {
            direction: Direction::Down,
        },
    };
    for _ in 0..2 {
        let branch = engine.branch().clone();
        assert_capacity(
            engine
                .command(
                    "player",
                    "growth-test",
                    ActorId(1),
                    "capacity",
                    &branch,
                    command.clone(),
                )
                .unwrap_err(),
        );
        assert_eq!(full_authority(&engine), before);
    }
    let admission = admit(&mut engine, "queued-capacity", Direction::Down);
    let before = full_authority(&engine);
    for _ in 0..2 {
        assert_capacity(engine.execute_next_intention().unwrap_err());
        assert_eq!(full_authority(&engine), before);
    }
    let disk =
        Checkpoint::capture(&engine).encode("growth-capacity", engine.archive.records.len() as u64);
    let mut replay = Engine::replay(engine.archive.clone(), Some(disk), None).unwrap();
    exact_equal(&replay, &engine);
    assert_eq!(
        admission.entry.id,
        engine.archive.records.last().unwrap().entry.id
    );
    let before = full_authority(&replay);
    assert_capacity(replay.execute_next_intention().unwrap_err());
    assert_eq!(full_authority(&replay), before);
}
#[test]
fn racial_growth_generated_entry_materializes_exact_rows_and_replays() {
    let mut engine = Engine::memory(growth_fixture(26, 7)).unwrap();
    walk_to_port(&mut engine, 26, "down");
    step(&mut engine, Direction::Down);
    let receipt = floor(&engine, 27);
    let actor = receipt["decisions"][2]["result"]["attempts"][0]["entity"]["id"]
        .as_u64()
        .unwrap();
    let creature = engine.game.creature(SimActor(actor)).unwrap();
    let build = creature.build();
    assert_eq!(build.ledger().total_hd(), 8);
    assert_eq!(build.ledger().racial_hd(), 8);
    restored_both_ways(&engine);
}
#[test]
fn racial_growth_store_refusal_discards_generated_destination_rows_and_receipts() {
    let mut engine = Engine::memory(growth_fixture(26, 7)).unwrap();
    walk_to_port(&mut engine, 26, "down");
    admit(&mut engine, "store-entry", Direction::Down);
    let disk =
        Checkpoint::capture(&engine).encode("growth-store", engine.archive.records.len() as u64);
    let mut control = Engine::replay(engine.archive.clone(), Some(disk), None).unwrap();
    control.execute_next_intention().unwrap().unwrap();
    assert_eq!(
        floor(&control, 27)["decisions"][2]["result"]["attempts"][0]["kind"],
        "placed"
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
    let before = full_authority(&engine);
    for _ in 0..2 {
        assert_eq!(
            engine.execute_next_intention().unwrap_err().code,
            ErrorCode::StorageFailure
        );
        assert_eq!(full_authority(&engine), before);
    }
}

fn identity_exhaustion_engine() -> Engine {
    let mut engine = Engine::memory(growth_fixture(26, 7)).unwrap();
    walk_to_port(&mut engine, 26, "down");
    // Reserve through the ordinary allocator interface. The first supply
    // allocates; the second reaches the operational identity bound.
    engine.game.reserve_identities(1, u64::MAX - 1, 1);
    // This synthetic capacity reservation is fixture setup, not an unjournaled
    // gameplay transition. Retain the same reservation at setup boundaries so
    // strict replay of their private admissions remains an exact comparison.
    for boundary in &mut engine.boundaries {
        Arc::make_mut(boundary)
            .game
            .reserve_identities(1, u64::MAX - 1, 1);
    }
    engine
}

#[test]
fn direct_population_identity_exhaustion_preserves_all_entry_authority_on_retry() {
    let mut engine = identity_exhaustion_engine();
    let before = full_authority(&engine);
    let command = Command::Act {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        action: Action::Move {
            direction: Direction::Down,
        },
    };
    for _ in 0..2 {
        let branch = engine.branch().clone();
        assert_capacity(
            engine
                .command(
                    "player",
                    "growth-test",
                    ActorId(1),
                    "identity-capacity",
                    &branch,
                    command.clone(),
                )
                .unwrap_err(),
        );
        assert_eq!(full_authority(&engine), before);
    }
}

#[test]
fn admitted_population_identity_exhaustion_remains_retryable_across_checkpoint() {
    let mut engine = identity_exhaustion_engine();
    let admission = admit(&mut engine, "identity-capacity", Direction::Down);
    let before = full_authority(&engine);
    for _ in 0..2 {
        assert_capacity(engine.execute_next_intention().unwrap_err());
        assert_eq!(
            full_authority(&engine),
            before,
            "no time, RNG, partial stock, IntentionFailed or receipt may publish"
        );
    }
    let disk = Checkpoint::capture(&engine)
        .encode("entry-identities", engine.archive.records.len() as u64);
    let mut replay = Engine::replay(engine.archive.clone(), Some(disk), None).unwrap();
    exact_equal(&replay, &engine);
    assert_eq!(
        replay.archive.records.last().unwrap().entry.id,
        admission.entry.id
    );
    let before = full_authority(&replay);
    assert_capacity(replay.execute_next_intention().unwrap_err());
    assert_eq!(full_authority(&replay), before);
}

#[test]
fn depth_growth_source_rejects_invalid_policy_and_unbuilt_actor_before_entry() {
    let scenario = growth_fixture(26, 7);
    let original = scenario.package.as_ref().unwrap();
    let source = serde_json::to_value(&original.manifest).unwrap();
    for policy in [
        json!({"kind":"racial_after","depth":0}),
        json!({"kind":"racial_after"}),
        json!({"kind":"fixed","maximum":256}),
        json!({"kind":"capped","maximum":256}),
    ] {
        let mut bad = source.clone();
        bad["floor_activation"]["profiles"]["baseline"]["stages"][2]["depth_growth"] = policy;
        if let Ok(manifest) = serde_json::from_value(bad) {
            assert!(
                Package::from_parts(manifest, vec![], &original.rules_source)
                    .and_then(|package| package.check())
                    .is_err()
            );
        }
    }
    // A plain actor is still a valid Fixed population prototype. The same
    // source cannot silently ignore requested racial growth without a build.
    for fixed in [true, false] {
        let mut source = source.clone();
        let stage = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][2];
        stage["bands"][0]["choices"][0]["archetype"] = json!("marker");
        if fixed {
            stage["depth_growth"] = json!({"kind":"fixed"});
        }
        let mut scenario = scenario.clone();
        scenario.package = Some(Arc::new(
            Package::from_parts(
                serde_json::from_value(source).unwrap(),
                vec![],
                &original.rules_source,
            )
            .unwrap(),
        ));
        match Engine::memory(scenario) {
            Ok(_) => assert!(fixed),
            Err(error) => {
                assert!(!fixed);
                assert!(
                    error.message.contains("depth-growth prototype"),
                    "{error:?}"
                );
            }
        }
    }
}
