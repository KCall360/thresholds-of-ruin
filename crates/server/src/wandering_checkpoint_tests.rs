//! A moving pre-birth clock must not copy the large Rogue population plan at
//! every rewind boundary. Exercise the same bounded writer as the save worker.
use super::*;
use serde_json::{json, Value};
use std::path::Path;

fn quiet_rogue() -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let mut scenario = crate::scenario_package::load(&root, 42, None, false).unwrap();
    let original = scenario.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&original.manifest).unwrap();
    // Isolate storage from unrelated combat without replacing the plan with a
    // toy catalog: every definition, depth band and wandering operand remains.
    let profiles = manifest["floor_activation"]["profiles"]
        .as_object_mut()
        .unwrap();
    let mut changed = 0;
    for profile in profiles.values_mut() {
        for stage in profile["stages"].as_array_mut().unwrap() {
            if stage["kind"] == "actors" {
                stage["count"] = json!([0, 0]);
                changed += 1;
            }
        }
    }
    assert!(changed > 0);
    let policy = manifest["floor_activation"]["wandering"].clone();
    assert_eq!(policy["initial_delay"], json!([6700, 7300]));
    let rules = original
        .resolved_rules_source
        .as_deref()
        .unwrap_or(&original.rules_source);
    let package = crate::scenario_package::Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        original.region_defs().unwrap(),
        rules,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&package.manifest).unwrap()["floor_activation"]["wandering"],
        policy
    );
    scenario.package = Some(Arc::new(package));
    scenario
}

fn stored(engine: &Engine) -> Vec<u8> {
    let disk = Checkpoint::capture(engine).encode(
        "wandering-size-regression",
        engine.archive.records.len() as u64,
    );
    crate::storage::checkpoint_codec::write_json(&disk, Vec::new())
        .expect("ordinary pre-birth clock updates must fit the actual bounded checkpoint writer")
}

fn decoded(bytes: &[u8]) -> Value {
    serde_json::from_slice(&crate::storage::checkpoint_codec::decode(bytes).unwrap()).unwrap()
}

#[test]
fn large_rogue_prebirth_clocks_preserve_every_boundary_with_one_immutable_plan() {
    let mut engine = Engine::memory(quiet_rogue()).unwrap();
    let baseline_bytes = stored(&engine);
    let baseline = decoded(&baseline_bytes);
    let baseline_disk: DiskCheckpoint = serde_json::from_value(baseline.clone()).unwrap();
    let baseline_restored = baseline_disk.restore(engine.archive.clone()).unwrap();
    assert_eq!(
        baseline_restored.game, engine.game,
        "validate positive fixture first"
    );
    let actor_table = baseline["game"]["actors"].as_u64().unwrap() as usize;
    assert_eq!(
        baseline["shared"]["actors"][actor_table]["1"]["combat"]["creature"]["nutrition"]["kind"],
        "required"
    );
    assert_eq!(
        baseline["shared"]["actors"][actor_table]["1"]["combat"]["creature"]["nutrition"]
            ["reserve"],
        1300
    );
    // Production nutrition spends one reserve per100ticks: this6400tick
    // witness stays far above hungry300 without changing or disabling food.
    let initial_health = engine.game.health(SimActor(1)).unwrap();
    assert!(initial_health.0 > 0);
    for index in 0..64 {
        engine
            .command(
                "test",
                "wandering-checkpoint",
                ActorId(1),
                &format!("wait-{index}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert!(
            engine.game.health(SimActor(1)).unwrap().0 > 0,
            "ordinary nutrition/combat cannot invalidate storage fixture"
        );
    }
    assert!(engine.game.tick() >= 6000);
    assert!(
        engine.game.tick() < 6700,
        "all clock witnesses precede the earliest birth"
    );
    assert!(
        engine.boundaries.len() >= 65,
        "rewind history must not be pruned to fit"
    );
    // This is the first-red boundary on the old whole-ActivationState pool:
    // 64 tiny clock changes copied roughly one MiB of identical plan each.
    let bytes = stored(&engine);
    let saved = decoded(&bytes);
    let states = saved["shared"]["floor_activations"].as_array().unwrap();
    let initial_states = baseline["shared"]["floor_activations"].as_array().unwrap();
    assert_eq!(
        states.len(),
        initial_states.len(),
        "clock updates cannot allocate immutable plan states"
    );
    assert!(states.iter().all(|state| state["wandering"].is_null()));
    let snapshots = std::iter::once(&saved["game"]).chain(
        saved["boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| &row["game"]),
    );
    let mut clocks = BTreeSet::new();
    for snapshot in snapshots {
        let wandering = &snapshot["floor_activation"]["wandering"];
        assert!(wandering.is_object());
        assert_eq!(
            wandering["births"],
            json!({}),
            "fixture tests clocks, not generated births"
        );
        clocks.insert(serde_json::to_string(&wandering["clocks"]).unwrap());
    }
    assert!(
        clocks.len() >= 60,
        "all distinct actor-active clock boundaries must survive"
    );
    let immutable_bytes = serde_json::to_vec(&states[0]).unwrap().len();
    assert!(
        immutable_bytes * clocks.len() > crate::storage::checkpoint_codec::LIMIT,
        "fixture must reproduce the former whole-state-pool overflow, not just small-state dedup"
    );
    assert!(bytes.len() < crate::storage::checkpoint_codec::LIMIT);
    let disk: DiskCheckpoint = serde_json::from_value(saved.clone()).unwrap();
    let restored = disk.restore(engine.archive.clone()).unwrap();
    assert_eq!(restored.game, engine.game);
    assert_eq!(restored.boundaries.len(), engine.boundaries.len());
    for (actual, expected) in restored.boundaries.iter().zip(&engine.boundaries) {
        assert_eq!(actual.game, expected.game);
        assert_eq!(actual.revisions, expected.revisions);
        assert_eq!(actual.id, expected.id);
    }
    assert_eq!(
        stored(&restored),
        bytes,
        "current writer must preserve the complete restored timeline"
    );
    let mut missing = saved.clone();
    missing["game"]["floor_activation"]
        .as_object_mut()
        .unwrap()
        .remove("wandering");
    assert!(
        serde_json::from_value::<DiskCheckpoint>(missing).is_err(),
        "clock field remains strict and required"
    );
    let invalid_state = states.len();
    let mut bad_index = saved;
    bad_index["game"]["floor_activation"]["state"] = json!(invalid_state);
    let invalid: DiskCheckpoint = serde_json::from_value(bad_index).unwrap();
    assert!(
        invalid.restore(engine.archive.clone()).is_err(),
        "missing immutable state cannot be synthesized"
    );
}
