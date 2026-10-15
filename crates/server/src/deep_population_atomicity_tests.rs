//! One generated entry combines exact deep growth, ordinary carried stock and
//! a later capacity refusal. Nothing from the earlier successful stage leaks.
use super::*;

const START_DEPTH: u32 = 275;
const ENTRY_DEPTH: u32 = 276;
const EARLY_STAGE: usize = 2;
const LATE_STAGE: usize = 3;

fn joint_fixture(later_count: u32) -> Scenario {
    let mut scenario = single_member_foe_fixture(true);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["characters"][0]["anchor"] = json!("dungeon/275/up");
    source["characters"].as_array_mut().unwrap().truncate(1);
    source["archetypes"]["marker"]["stackable"] = json!(true);
    let row = source["archetypes"]["rat"]["creature"]["hit_dice"][0].clone();
    source["archetypes"]["rat"]["creature"]["hit_dice"] = json!([row.clone()]);
    source["archetypes"]["late_rat"] = source["archetypes"]["rat"].clone();
    source["archetypes"]["late_rat"]["creature"]["hit_dice"] = json!(vec![row; 7]);
    source["floor_activation"]["carried_item_pools"] = json!({
        "supplies":{"version":1,"choices":[
            {"archetype":"marker","weight":1,"quantity":3}
        ]}
    });
    let stages = source["floor_activation"]["profiles"]["baseline"]["stages"]
        .as_array_mut()
        .unwrap();
    stages[EARLY_STAGE] = json!({
        "id":"early-carrier","version":1,"kind":"actors","count":[1,1],
        "depth_growth":{"kind":"racial_after","depth":26},
        "bands":[{"depth":{"kind":"open","minimum":ENTRY_DEPTH},"choices":[{
            "archetype":"rat","weight":1,
            "carried_loot":{"version":1,"pool":"supplies",
                            "chance":{"numerator":1,"denominator":1}}
        }]}]
    });
    stages.push(json!({
        "id":"late-capacity","version":1,"kind":"actors",
        "count":[later_count,later_count],
        "depth_growth":{"kind":"racial_after","depth":26},
        "bands":[{"depth":{"kind":"open","minimum":ENTRY_DEPTH},"choices":[{
            "archetype":"late_rat","weight":1
        }]}]
    }));
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

fn full_authority(engine: &Engine) -> Value {
    let regions = engine.regions.as_ref().unwrap();
    json!({
        "authority": super::super::growth_capacity_transaction_tests::authority(engine),
        "history_index": format!("{:?}", engine.history_index),
        "observations": format!("{:?}", engine.observations.snapshot()),
        "region_records": regions.encoded_resident().unwrap(),
        "pending_sources": regions.unsaved_sources().unwrap(),
        "pending_entry": engine.pending_intentions(ActorId(1)),
    })
}

fn capacity_failure(engine: &mut Engine) -> Failure {
    let failure = engine
        .execute_next_intention()
        .expect_err("the later 257-HD actor must refuse the complete entry");
    assert_eq!(failure.code, ErrorCode::ResourceLimit);
    assert!(failure.message.contains("capacity"), "{failure:?}");
    assert!(
        failure
            .message
            .contains("no movement or population was committed"),
        "the controller needs an explanatory retryable refusal: {failure:?}"
    );
    for hidden in ["rat", "251", "257", "supplies"] {
        assert!(!failure.message.contains(hidden), "{failure:?}");
    }
    failure
}

fn assert_original_admission(engine: &Engine, admission: &CommandResult) {
    let pending = engine.pending_intentions(ActorId(1));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].entry_id, admission.entry.id);
    assert_eq!(pending[0].phase, tor_protocol::IntentionPhase::Queued);
    assert_eq!(
        engine.archive.records.last().unwrap().entry,
        admission.entry
    );
    assert!(decisions(engine)["floors"].get("dungeon/276").is_none());
    assert!(!engine
        .archive
        .records
        .iter()
        .any(|record| matches!(record.entry.content, JournalContent::IntentionFailed { .. })));
}

#[test]
fn deep_entry_rolls_back_a_251_hd_carrier_when_a_later_actor_requires_257_hd() {
    joint_capacity_case(false);
}

#[test]
fn retrieval_and_deep_carried_stock_share_the_late_capacity_rollback_boundary() {
    joint_capacity_case(true);
}

fn objective_case(scenario: Scenario, retrieval: bool) -> Scenario {
    if retrieval {
        super::retrieval::attach_objective(scenario)
    } else {
        scenario
    }
}

fn joint_capacity_case(retrieval: bool) {
    let rejected_scenario = objective_case(joint_fixture(1), retrieval);
    let control_scenario = objective_case(joint_fixture(0), retrieval);
    let mut rejected_source =
        serde_json::to_value(&rejected_scenario.package.as_ref().unwrap().manifest).unwrap();
    rejected_source["floor_activation"]["profiles"]["baseline"]["stages"][LATE_STAGE]["count"] =
        json!([0, 0]);
    assert_eq!(
        rejected_source,
        serde_json::to_value(&control_scenario.package.as_ref().unwrap().manifest).unwrap(),
        "the positive control changes only the later stage count"
    );

    let mut control = Engine::memory(control_scenario).unwrap();
    walk_to_port(&mut control, START_DEPTH, "down");
    admit(&mut control, "joint-deep-entry", Direction::Down);
    let published = control.execute_next_intention().unwrap().unwrap();
    assert!(matches!(
        published.entry.content,
        JournalContent::IntentionStarted { .. }
    ));
    let receipt = floor(&control, ENTRY_DEPTH);
    let attempt = &receipt["decisions"][EARLY_STAGE]["result"]["attempts"][0];
    assert_eq!(attempt["kind"], "placed");
    let actor = SimActor(attempt["entity"]["id"].as_u64().unwrap());
    let creature = control.game.creature(actor).unwrap();
    assert_eq!(creature.build().ledger().total_hd(), 251);
    assert_eq!(creature.build().ledger().racial_hd(), 251);
    assert_eq!(creature.build().ledger().entries().len(), 251);
    assert_eq!(creature.derived().health_contributions.len(), 251);
    assert_eq!(
        &*creature.derived().health_contributions,
        &creature
            .build()
            .ledger()
            .health_provenance(&creature.derived().kind, creature.build().ruleset())
            .unwrap(),
        "all 251 actual Health rows derive from the published owned HD seeds"
    );
    let stock = &attempt["entity"]["carried_loot"];
    assert_eq!(stock["kind"], "carried");
    assert_eq!(stock["content"], "marker");
    assert_eq!(stock["quantity"], 3);
    let observed = control.game.observe(actor).unwrap();
    let own_stats = observed
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    assert_eq!(own_stats.hit_dice.total, 251);
    assert_eq!(own_stats.hit_dice.racial, 251);
    let inventory = observed.inventory;
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].id.0, stock["item"].as_u64().unwrap());
    assert_eq!(
        inventory[0].quantity, 3,
        "a real ordinary stack was published"
    );
    restored_both_ways(&control);

    let mut engine = Engine::memory(rejected_scenario).unwrap();
    walk_to_port(&mut engine, START_DEPTH, "down");
    let admission = admit(&mut engine, "joint-deep-entry", Direction::Down);
    let before = engine.diagnostic_copy();
    let state = full_authority(&engine);
    let mut failure = None;
    for _ in 0..2 {
        let refused = capacity_failure(&mut engine);
        if let Some(previous) = &failure {
            assert_eq!(&refused, previous);
        }
        failure = Some(refused);
        exact_equal(&engine, &before);
        assert_eq!(full_authority(&engine), state);
        assert_original_admission(&engine, &admission);
    }
    let checkpoint = Checkpoint::capture(&engine)
        .encode("joint-deep-entry", engine.archive.records.len() as u64);
    let mut restored = Engine::replay(engine.archive.clone(), Some(checkpoint), None).unwrap();
    exact_equal(&restored, &engine);
    let state = full_authority(&restored);
    assert_eq!(capacity_failure(&mut restored), failure.unwrap());
    assert_eq!(full_authority(&restored), state);
    assert_original_admission(&restored, &admission);
}

type LogicalRows = BTreeMap<&'static str, Vec<Vec<rusqlite::types::Value>>>;

pub(super) fn logical_store_rows(path: &Path) -> LogicalRows {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    [
        "journal",
        "history",
        "checkpoint",
        "regions",
        "region_sources",
        "package",
    ]
    .into_iter()
    .map(|table| {
        let mut query = connection
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
            .unwrap();
        let columns = query.column_count();
        let rows = query
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get(column))
                    .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        (table, rows)
    })
    .collect()
}

#[test]
fn deep_entry_capacity_preserves_all_store_rows_and_retries_from_pinned_source() {
    stored_capacity_case(false);
}

#[test]
fn retrieval_deep_capacity_refusal_preserves_every_logical_store_row() {
    stored_capacity_case(true);
}

fn stored_capacity_case(retrieval: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deep-population.db");
    let mut engine = Engine::open_with_policy(
        &path,
        objective_case(joint_fixture(1), retrieval),
        crate::SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )
    .unwrap();
    walk_to_port(&mut engine, START_DEPTH, "down");
    let admission = admit(&mut engine, "saved-joint-deep-entry", Direction::Down);
    engine.flush().unwrap();
    // Drain completed-save cache housekeeping before measuring the entry transaction.
    drop(engine.capture_command_candidate(None));
    let rows = logical_store_rows(&path);
    assert!(
        !rows["checkpoint"].is_empty(),
        "exercise actual saved checkpoint recovery"
    );
    assert!(
        !rows["history"].is_empty(),
        "retain the compacted admission history"
    );
    let sequence = engine.save_status().accepted_sequence;
    assert_eq!(engine.save_status().durable_sequence, sequence);
    let archive = engine.archive.clone();
    let checkpoint = Checkpoint::capture(&engine).encode("pinned-joint-entry", sequence);
    let before = engine.diagnostic_copy();
    let state = full_authority(&engine);
    let failure = capacity_failure(&mut engine);
    exact_equal(&engine, &before);
    assert_eq!(full_authority(&engine), state);
    assert_eq!(engine.save_status().accepted_sequence, sequence);
    assert_eq!(logical_store_rows(&path), rows);
    assert_eq!(capacity_failure(&mut engine), failure);
    exact_equal(&engine, &before);
    assert_eq!(full_authority(&engine), state);
    assert_original_admission(&engine, &admission);
    engine.flush().unwrap();
    assert_eq!(engine.save_status().accepted_sequence, sequence);
    assert_eq!(logical_store_rows(&path), rows);
    drop(before);
    drop(engine);

    let expected = Engine::replay(archive, Some(checkpoint), None).unwrap();
    let mut reopened = Engine::open(&path, Scenario::two_room(999)).unwrap();
    exact_equal(&reopened, &expected);
    assert_eq!(logical_store_rows(&path), rows);
    let before = reopened.diagnostic_copy();
    let state = full_authority(&reopened);
    for _ in 0..2 {
        assert_eq!(capacity_failure(&mut reopened), failure);
        exact_equal(&reopened, &before);
        assert_eq!(full_authority(&reopened), state);
        assert_original_admission(&reopened, &admission);
        reopened.flush().unwrap();
        assert_eq!(reopened.save_status().accepted_sequence, sequence);
        assert_eq!(reopened.save_status().durable_sequence, sequence);
        assert_eq!(logical_store_rows(&path), rows);
    }
}
