//! Cross-layer stock publication, branch provenance and saved recovery.
use super::*;

fn carrier_fixture(minimum: u32) -> Scenario {
    let mut scenario = single_member_foe_fixture(true);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["archetypes"]["marker"]["stackable"] = json!(true);
    source["ai_profiles"]["wander"] =
        json!({"care":false,"equipment":false,"loot":false,"flee_percent":0});
    source["creatures"]["species"]["cave_rat"]["behavior"] =
        json!({"disposition":"passive","idle":"wait","memory_ticks":1000});
    source["floor_activation"]["carried_item_pools"] = json!({"supplies":{"version":1,"choices":[{"archetype":"marker","weight":1,"quantity":3}]}});
    let stage = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][2];
    stage["bands"][0]["depth"]["minimum"] = json!(minimum);
    stage["bands"][0]["choices"][0]["carried_loot"] =
        json!({"version":1,"pool":"supplies","chance":{"numerator":1,"denominator":1}});
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    scenario
}

fn nested_stock(receipt: &Value) -> &Value {
    &receipt["decisions"][2]["result"]["attempts"][0]["entity"]["carried_loot"]
}

fn settle_carrier_ai(engine: &mut Engine) {
    for _ in 0..16 {
        let actor = engine.next_actor().expect("live fixture turn");
        if actor == ActorId(1) {
            return;
        }
        assert!(engine.is_ai(actor));
        engine.advance_ai(actor).unwrap();
    }
    panic!("passive birth fixture did not reach the player boundary");
}

#[test]
fn carried_stock_initial_base_and_later_entry_recover_without_new_births() {
    let engine = Engine::memory(carrier_fixture(1)).unwrap();
    let first = floor(&engine, 1);
    assert_eq!(nested_stock(&first)["kind"], "carried");
    assert_eq!(nested_stock(&first)["quantity"], 3);
    assert!(engine.archive.records.is_empty());
    restored_both_ways(&engine);
    // Later-entry fixture starts without an active NPC, so the existing route
    // helper's player-only scheduler precondition remains explicit.
    let mut engine = Engine::memory(carrier_fixture(2)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    let second = floor(&engine, 2);
    assert_eq!(nested_stock(&second)["kind"], "carried");
    assert!(nested_stock(&second)["item"].as_u64().unwrap() > 0);
    let receipts = decisions(&engine);
    settle_carrier_ai(&mut engine);
    step(&mut engine, Direction::Up);
    settle_carrier_ai(&mut engine);
    step(&mut engine, Direction::Down);
    assert_eq!(decisions(&engine), receipts);
    restored_both_ways(&engine);
}

#[test]
fn carried_stock_late_store_failure_rolls_back_all_authority_and_exact_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("birth-stock.db");
    let mut engine = Engine::open(&path, carrier_fixture(2)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    admit(&mut engine, "carrier-entry", Direction::Down);
    engine.flush().unwrap();
    drop(engine);
    let mut rejected = Engine::open_with_policy(
        &path,
        Scenario::two_room(0),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let before = rejected.diagnostic_copy();
    let mut healthy = Engine::replay(rejected.archive.clone(), None, None).unwrap();
    healthy.execute_next_intention().unwrap().unwrap();
    assert_eq!(nested_stock(&floor(&healthy, 2))["kind"], "carried");
    assert!(rejected.execute_next_intention().is_err());
    exact_equal(&rejected, &before);
    assert_eq!(authority(&rejected), authority(&before));
    drop(before);
    drop(rejected);
    let mut retry = Engine::open(&path, Scenario::two_room(0)).unwrap();
    retry.execute_next_intention().unwrap().unwrap();
    // Independent accepted executions allocate fresh journal UUIDs. Gameplay,
    // request receipts and the complete birth evidence must still match exactly.
    assert_eq!(retry.game, healthy.game);
    assert_eq!(retry.revisions, healthy.revisions);
    assert_eq!(retry.receipts, healthy.receipts);
    assert_eq!(decisions(&retry), decisions(&healthy));
    let retried = retry.archive.records.last().unwrap();
    let control = healthy.archive.records.last().unwrap();
    assert_eq!(retried.entry.content, control.entry.content);
    assert_eq!(retried.activations, control.activations);
    restored_both_ways(&retry);
}

#[test]
fn carried_stock_initial_receipt_corruption_rejects_replay_and_checkpoint() {
    let engine = Engine::memory(carrier_fixture(1)).unwrap();
    for change in ["miss", "absent", "content", "quantity", "zero", "future"] {
        let mut archive = serde_json::to_value(&engine.archive).unwrap();
        let loot = &mut archive["initial_activations"][0]["decisions"][2]["result"]["attempts"][0]
            ["entity"]["carried_loot"];
        match change {
            "miss" => *loot = json!({"kind":"not_carried"}),
            "absent" => *loot = Value::Null,
            "content" => loot["content"] = json!("outside-pool"),
            "quantity" => loot["quantity"] = json!(4),
            "zero" => loot["item"] = json!(0),
            "future" => loot["item"] = json!(u64::MAX - 1),
            _ => unreachable!(),
        }
        let archive: Archive = serde_json::from_value(archive).unwrap();
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "{change}"
        );
        let disk = Checkpoint::capture(&engine).encode("birth-stock-corruption", 0);
        assert!(disk.restore(archive).is_err(), "{change}");
    }
}

#[test]
fn carried_stock_expired_abandoned_item_cannot_alias_a_retained_wizard_birth() {
    let (mut engine, abandoned) = expired_abandoned_activation(carrier_fixture(2));
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let created = wizard(
        &mut engine,
        WizardOperation::PlaceItem {
            kind: crate::journal::WizardItem::Token,
            position: Position {
                region: at.region.0,
                x: at.position.x,
                y: at.position.y,
                z: at.position.z,
            },
        },
    );
    let JournalContent::Wizard {
        result: WizardResult::ItemPlaced { item },
        ..
    } = created.entry.content
    else {
        panic!()
    };
    restored_both_ways(&engine);
    let mut archive = serde_json::to_value(&engine.archive).unwrap();
    archive["records"][abandoned]["activations"][0]["decisions"][2]["result"]["attempts"][0]
        ["entity"]["carried_loot"]["item"] = json!(item);
    let archive: Archive = serde_json::from_value(archive).unwrap();
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    let disk = Checkpoint::capture(&engine).encode(
        "birth-stock-wizard-alias",
        engine.archive.records.len() as u64,
    );
    assert!(
        disk.restore(archive).is_err(),
        "expired nested origins remain audited"
    );
}

#[test]
fn carried_stock_reentry_after_rewind_keeps_choices_and_allocates_above_future() {
    let mut engine = Engine::memory(carrier_fixture(2)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    let target = engine.archive.records.last().unwrap().entry.id.clone();
    step(&mut engine, Direction::Down);
    let first = floor(&engine, 2);
    let first_item = nested_stock(&first)["item"].as_u64().unwrap();
    let first_actor = first["decisions"][2]["result"]["attempts"][0]["entity"]["id"]
        .as_u64()
        .unwrap();
    wizard(
        &mut engine,
        WizardOperation::Rewind {
            target: Some(target),
        },
    );
    assert!(decisions(&engine)["floors"].get("dungeon/2").is_none());
    step(&mut engine, Direction::Down);
    let second = floor(&engine, 2);
    assert_eq!(
        nested_stock(&first)["content"],
        nested_stock(&second)["content"]
    );
    assert_eq!(
        nested_stock(&first)["quantity"],
        nested_stock(&second)["quantity"]
    );
    assert!(nested_stock(&second)["item"].as_u64().unwrap() > first_item);
    assert!(
        second["decisions"][2]["result"]["attempts"][0]["entity"]["id"]
            .as_u64()
            .unwrap()
            > first_actor
    );
    assert!(!engine
        .game
        .known_actor_ids()
        .contains(&SimActor(first_actor)));
    restored_both_ways(&engine);
}

#[test]
fn carried_stock_queued_identity_refusal_spends_no_time_or_failed_intention_receipt() {
    let mut engine = Engine::memory(carrier_fixture(2)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    // A complete earlier ground stage fits. The carrier's ordinary item cannot
    // allocate, after its actor constructor has already run in the candidate.
    engine.game.reserve_identities(1, u64::MAX - 1, 1);
    admit(&mut engine, "birth-capacity", Direction::Down);
    let before = engine.diagnostic_copy();
    for _ in 0..2 {
        let failure = engine
            .execute_next_intention()
            .expect_err("capacity is a retryable refusal, not IntentionFailed");
        assert_eq!(failure.code, ErrorCode::ResourceLimit);
        assert_eq!(failure.message, "Simulation resource capacity exhausted");
        exact_equal(&engine, &before);
        assert_eq!(authority(&engine), authority(&before));
    }
}
