//! Named recipes retain the existing creation, finite-stock and save contracts.
use super::*;

fn scenario_with(mut change: impl FnMut(&mut Value)) -> Scenario {
    let mut scenario = carried_stock::carrier_fixture(2);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["floor_activation"]["profiles"]["baseline"]["stages"][2]["count"] = json!([0, 0]);
    change(&mut source);
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
fn fixture_engine() -> Engine {
    Engine::memory(scenario_with(|_| {})).unwrap()
}
fn available(engine: &Engine) -> Position {
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    // These fixtures have single-cell bodies. A travel route may deliberately
    // end at an occupied attack destination, which is not a legal birth cell.
    let occupied: BTreeSet<_> = engine
        .game
        .loaded_actor_ids()
        .map(|actor| engine.game.observe(actor).unwrap().location)
        .collect();
    let location = engine
        .game
        .known_cells(SimActor(1))
        .find(|location| {
            location.region == at.region
                && *location != at
                && !occupied.contains(location)
                && engine.game.travel_route(SimActor(1), *location).is_ok()
        })
        .unwrap();
    Position {
        region: location.region.0,
        x: location.position.x,
        y: location.position.y,
        z: location.position.z,
    }
}
fn command(selector: &str, position: Position, depth: Option<u32>) -> WizardOperation {
    crate::developer::parse_wizard(&format!(
        "creature spawn {selector} {} {} {} {}{}",
        position.region,
        position.x,
        position.y,
        position.z,
        depth.map_or(String::new(), |depth| format!(" depth {depth}"))
    ))
    .unwrap()
}
fn spawn(engine: &mut Engine, selector: &str, depth: Option<u32>) -> (ActorId, Option<u64>) {
    let operation = command(selector, available(engine), depth);
    let result = wizard(engine, operation);
    let JournalContent::Wizard {
        result:
            WizardResult::CreatureSpawned {
                actor,
                carried_item,
                ..
            },
        ..
    } = result.entry.content
    else {
        panic!("named birth result")
    };
    (actor, carried_item)
}

#[test]
fn named_population_birth_retains_build_ai_stock_and_ordinary_scheduling() {
    let mut engine = fixture_engine();
    let before_tick = engine.game.tick();
    let (actor, item) = spawn(&mut engine, "rat", Some(2));
    assert!(engine.is_ai(actor));
    let observation = engine.game.observe(SimActor(actor.0)).unwrap();
    assert_eq!(observation.tick, before_tick);
    assert_eq!(
        engine
            .game
            .inspect_creature(SimActor(actor.0))
            .unwrap()
            .identity
            .name,
        "cave rat"
    );
    assert_eq!(observation.inventory.len(), 1);
    assert_eq!(observation.inventory[0].quantity, 3);
    assert_eq!(Some(observation.inventory[0].id.0), item);
    assert!(engine.game.loaded_actor_ids().any(|id| id.0 == actor.0));
    let seeded = engine
        .game
        .creature(SimActor(actor.0))
        .unwrap()
        .build()
        .clone();
    let mut second = fixture_engine();
    let (second_actor, second_item) = spawn(&mut second, "population:baseline/foe/rat", Some(2));
    assert_eq!((actor, item), (second_actor, second_item));
    assert_eq!(
        &seeded,
        second.game.creature(SimActor(actor.0)).unwrap().build()
    );
    restored_both_ways(&engine);
}

#[test]
fn named_standalone_birth_does_not_invent_population_ai_or_stock() {
    let mut engine = fixture_engine();
    let (actor, item) = spawn(&mut engine, "archetype:rat", Some(999));
    assert!(!engine.is_ai(actor));
    assert!(item.is_none());
    assert!(engine
        .game
        .observe(SimActor(actor.0))
        .unwrap()
        .inventory
        .is_empty());
    assert_eq!(
        engine
            .game
            .creature(SimActor(actor.0))
            .unwrap()
            .build()
            .ledger()
            .entries()
            .len(),
        1
    );
    let JournalContent::Wizard {
        result: WizardResult::CreatureSpawned { depth, .. },
        ..
    } = &engine.archive.records.last().unwrap().entry.content
    else {
        panic!()
    };
    assert_eq!(
        *depth, None,
        "standalone result explicitly discloses authored base"
    );
    restored_both_ways(&engine);
}

#[test]
fn named_spawn_bad_id_location_depth_and_capacity_leave_every_identity_and_rng_unchanged() {
    let mut engine = Engine::memory(scenario_with(|source| {
        source["floor_activation"]["profiles"]["baseline"]["stages"][2]["depth_growth"] =
            json!({"kind":"racial_after","depth":2});
    }))
    .unwrap();
    engine.enable_wizard().unwrap();
    let position = available(&engine);
    let occupied = engine.game.observe(SimActor(1)).unwrap().location;
    let mut occupied_position = position;
    occupied_position.x = occupied.position.x;
    occupied_position.y = occupied.position.y;
    let mut absent = position;
    absent.region = u64::MAX;
    for (selector, position, depth, code) in [
        ("missing", position, Some(2), ErrorCode::InvalidAction),
        ("rat", occupied_position, Some(2), ErrorCode::InvalidAction),
        ("rat", absent, Some(2), ErrorCode::InvalidAction),
        ("rat", position, Some(1), ErrorCode::InvalidAction),
        ("rat", position, Some(258), ErrorCode::ResourceLimit),
    ] {
        let before = engine.diagnostic_copy();
        let operation = command(selector, position, depth);
        let result = engine.command(
            "player",
            "activation-test",
            ActorId(1),
            "rejected-birth",
            &engine.branch().clone(),
            Command::Wizard {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                operation,
            },
        );
        assert_eq!(result.unwrap_err().code, code);
        exact_equal(&engine, &before);
    }
    let (actor, _) = spawn(&mut engine, "rat", Some(257));
    assert_eq!(
        engine
            .game
            .creature(SimActor(actor.0))
            .unwrap()
            .build()
            .ledger()
            .entries()
            .len(),
        256
    );
    restored_both_ways(&engine);
}

#[test]
fn named_spawn_resolves_known_destination_depth_without_loading_for_listing() {
    let mut engine = Engine::memory(scenario_with(|source| {
        source["floor_activation"]["profiles"]["baseline"]["stages"][2]["bands"][0]["depth"]
            ["minimum"] = json!(1);
        source["floor_activation"]["profiles"]["baseline"]["stages"][2]["depth_growth"] =
            json!({"kind":"racial_after","depth":1});
    }))
    .unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.diagnostic_copy();
    let page = engine
        .creature_catalog_query(
            ActorId(1),
            engine.branch(),
            engine.revision(ActorId(1)).unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(page.depth, Some(1));
    assert!(page
        .entries
        .iter()
        .any(|entry| entry.selector == "population:baseline/foe/rat"));
    exact_equal(&engine, &before);
    let (actor, _) = spawn(&mut engine, "rat", None);
    let JournalContent::Wizard {
        result: WizardResult::CreatureSpawned { depth, .. },
        ..
    } = &engine.archive.records.last().unwrap().entry.content
    else {
        panic!()
    };
    assert_eq!(*depth, Some(1));
    assert_eq!(
        engine
            .game
            .creature(SimActor(actor.0))
            .unwrap()
            .build()
            .ledger()
            .entries()
            .len(),
        1
    );
}

#[test]
fn named_spawn_ambiguous_context_requires_a_qualified_selector() {
    let mut engine = Engine::memory(scenario_with(|source| {
        let mut other = source["floor_activation"]["profiles"]["baseline"]["stages"][2].clone();
        other["id"] = json!("other");
        other["ai"] = Value::Null;
        other["bands"][0]["choices"][0]["carried_loot"] = Value::Null;
        source["floor_activation"]["profiles"]["baseline"]["stages"]
            .as_array_mut()
            .unwrap()
            .push(other);
    }))
    .unwrap();
    let regions = engine.regions.as_ref().unwrap();
    assert!(crate::wizard_creatures::resolve(&engine.game, regions, "rat", Some(2)).is_err());
    let (actor, stock) = spawn(&mut engine, "population:baseline/other/rat", Some(2));
    assert!(!engine.is_ai(actor));
    assert!(stock.is_none());
}

#[test]
fn named_spawn_retry_and_rewind_preserve_receipts_and_future_identity_floors() {
    let mut engine = fixture_engine();
    engine.enable_wizard().unwrap();
    let operation = command("rat", available(&engine), Some(2));
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation,
    };
    let first = engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "birth",
            &branch,
            command.clone(),
        )
        .unwrap();
    let before = engine.diagnostic_copy();
    let retry = engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "birth",
            &branch,
            command,
        )
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry, first.entry);
    exact_equal(&engine, &before);
    let JournalContent::Wizard {
        result:
            WizardResult::CreatureSpawned {
                actor: first_actor,
                carried_item: first_item,
                ..
            },
        ..
    } = first.entry.content
    else {
        panic!()
    };
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    let (next_actor, next_item) = spawn(&mut engine, "rat", Some(2));
    assert!(next_actor.0 > first_actor.0);
    assert!(next_item.unwrap() > first_item.unwrap());
    restored_both_ways(&engine);
}

#[test]
fn named_spawn_catalog_pages_are_bounded_and_reject_unknown_cursors() {
    let mut engine = Engine::memory(scenario_with(|source| {
        for index in 0..40 {
            source["archetypes"][format!("rat_{index:02}")] = source["archetypes"]["rat"].clone();
        }
    }))
    .unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.diagnostic_copy();
    let query = |after| {
        engine.creature_catalog_query(
            ActorId(1),
            engine.branch(),
            engine.revision(ActorId(1)).unwrap(),
            Some(2),
            after,
        )
    };
    let first = query(None).unwrap();
    assert_eq!(first.entries.len(), 32);
    let second = query(first.next.as_deref()).unwrap();
    assert!(!second.entries.is_empty());
    assert!(second
        .entries
        .iter()
        .all(|entry| entry.selector > first.next.clone().unwrap()));
    assert!(second.next.is_none());
    assert!(query(Some("archetype:unknown")).is_err());
    exact_equal(&engine, &before);
}

#[test]
fn named_spawn_checkpoint_rejects_forged_recipe_depth_stock_and_authored_identity() {
    let mut engine = fixture_engine();
    spawn(&mut engine, "rat", Some(2));
    for field in ["selector", "depth", "carried_item", "actor"] {
        let mut archive = serde_json::to_value(&engine.archive).unwrap();
        let last = archive["records"].as_array().unwrap().len() - 1;
        let result = &mut archive["records"][last]["entry"]["content"]["result"];
        result[field] = match field {
            "selector" => json!("population:baseline/foe/missing"),
            "depth" => json!(1),
            "carried_item" => Value::Null,
            "actor" => json!(1),
            _ => unreachable!(),
        };
        let archive: Archive = serde_json::from_value(archive).unwrap();
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "journal {field}"
        );
        let checkpoint = Checkpoint::capture(&engine)
            .encode("named-birth-tamper", engine.archive.records.len() as u64);
        assert!(checkpoint.restore(archive).is_err(), "checkpoint {field}");
    }
}

#[test]
fn named_spawn_base_depth_requires_consistent_authored_band_policies() {
    let engine = Engine::memory(scenario_with(|source| {
        let bands = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][2]["bands"];
        let mut later = bands[0].clone();
        bands[0]["depth"] = json!({"kind":"finite","minimum":2,"maximum":2});
        later["depth"] = json!({"kind":"open","minimum":3});
        later["choices"][0]["carried_loot"] = Value::Null;
        bands.as_array_mut().unwrap().push(later);
    }))
    .unwrap();
    let regions = engine.regions.as_ref().unwrap();
    for selector in ["rat", "population:baseline/foe/rat"] {
        assert!(crate::wizard_creatures::resolve(&engine.game, regions, selector, None).is_err());
        assert!(crate::wizard_creatures::resolve(&engine.game, regions, selector, Some(2)).is_ok());
        assert!(crate::wizard_creatures::resolve(&engine.game, regions, selector, Some(3)).is_ok());
    }
}

#[test]
fn named_spawn_store_admission_failure_leaves_no_birth_and_exact_retry_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("named-birth.db");
    let mut initial = Engine::open(&path, scenario_with(|_| {})).unwrap();
    initial.enable_wizard().unwrap();
    initial.flush().unwrap();
    drop(initial);
    let mut engine = Engine::open_with_policy(
        &path,
        Scenario::two_room(0),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.diagnostic_copy();
    let command = Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: command("rat", available(&engine), Some(2)),
    };
    let branch = engine.branch().clone();
    assert!(engine
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "birth-after-failure",
            &branch,
            command.clone()
        )
        .is_err());
    exact_equal(&engine, &before);
    drop(before);
    drop(engine);
    let mut restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    restored.enable_wizard().unwrap();
    restored
        .command(
            "player",
            "activation-test",
            ActorId(1),
            "birth-after-failure",
            &branch,
            command,
        )
        .unwrap();
    restored_both_ways(&restored);
}

#[test]
fn authored_future_purchases_cross_real_entry_and_named_birth_after_source_free_reopen() {
    use tor_simulation::attributes::Skill;
    for interval in [1, 4096] {
        let scenario = scenario_with(|source| {
            let stage = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][2];
            stage["count"] = json!([1, 1]);
            stage["depth_growth"] = json!({"kind":"racial_after","depth":1});
            stage["bands"][0]["choices"][0]["advancement"] = json!([
                {"ordinal":2,"purchases":[{"pool":"advancement","selection":{"kind":"skill_rank","skill":"agility"}}]},
                {"ordinal":4,"purchases":[{"pool":"advancement","selection":{"kind":"skill_rank","skill":"agility"}}]}
            ]);
        });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("future-purchases.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, scenario, policy.clone()).unwrap();
        walk_to_port(&mut engine, 1, "down");
        step(&mut engine, Direction::Down);
        let receipt = floor(&engine, 2);
        let actor = SimActor(
            receipt["decisions"][2]["result"]["attempts"][0]["entity"]["id"]
                .as_u64()
                .unwrap(),
        );
        let born = engine.game.creature(actor).unwrap().build().clone();
        assert_eq!(born.ledger().total_hd(), tor_simulation::HdCount(2));
        assert_eq!(born.derive().unwrap().skills.get(Skill::Agility), 1);
        assert_eq!(born.choices()[1].purchases.len(), 1);
        let tor_simulation::creatures::RestorationPolicy::Authored(plan) =
            born.restoration_policy()
        else {
            panic!()
        };
        assert_eq!(plan[1].choices, born.choices()[1]);
        restored_both_ways(&engine);
        engine.flush().unwrap();
        // A diagnostic copy retains the live journal lock. Compare with an
        // independently replayed in-memory state while reopening the real save.
        let before = Engine::replay(engine.archive.clone(), None, None).unwrap();
        exact_equal(&before, &engine);
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, Scenario::two_room(0), policy).unwrap();
        exact_equal(&engine, &before);
        if interval == 1 {
            assert!(engine.recovery_profile().checkpoint_sequence > 0);
        } else {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        let (same_depth, _) = spawn(&mut engine, "population:baseline/foe/rat", Some(2));
        let comparison_state = engine.game.creature(SimActor(same_depth.0)).unwrap();
        let comparison = comparison_state.build();
        assert_eq!(comparison.choices(), born.choices());
        assert_eq!(
            comparison.derive().unwrap().skills,
            born.derive().unwrap().skills
        );
        let (deeper, _) = spawn(&mut engine, "population:baseline/foe/rat", Some(4));
        let deeper_state = engine.game.creature(SimActor(deeper.0)).unwrap();
        let deeper = deeper_state.build();
        assert_eq!(deeper.ledger().total_hd(), tor_simulation::HdCount(4));
        assert_eq!(deeper.derive().unwrap().skills.get(Skill::Agility), 2);
        assert_eq!(deeper.choices()[3].purchases.len(), 1);
        restored_both_ways(&engine);
    }
}
