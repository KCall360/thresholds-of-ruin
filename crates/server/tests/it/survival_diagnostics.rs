use super::support;
use serde_json::Value;
use tor_server::{Engine, Scenario};

fn rows(path: &std::path::Path, expected: usize) -> Vec<Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let rows: Vec<Value> = std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if rows.len() >= expected || std::time::Instant::now() >= deadline {
            return rows;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[test]
fn only_successful_live_history_emits_survival_commits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("survival.jsonl");
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    engine.enable_survival_diagnostics(&path).unwrap();
    let branch = engine.branch().clone();
    let command = tor_server::journal::Command::Act {
        expected_revision: engine.revision(support::CHARACTER).unwrap(),
        action: tor_server::journal::Action::Wait,
    };
    let first = engine
        .command(
            "player",
            "test",
            support::CHARACTER,
            "same-request",
            &branch,
            command.clone(),
        )
        .unwrap();
    let retry = engine
        .command(
            "player",
            "test",
            support::CHARACTER,
            "same-request",
            &branch,
            command.clone(),
        )
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry, first.entry);
    assert!(engine
        .command(
            "player",
            "test",
            support::CHARACTER,
            "stale-request",
            &branch,
            command
        )
        .is_err());
    for _ in 0..3 {
        engine.state(support::CHARACTER).unwrap();
    }
    let entry = first.entry;
    let rows = rows(&path, 2);
    assert_eq!(rows.len(), 2);
    // The diagnostic two-room fixture has no selected authored character.
    assert!(rows[0]["selected_actor"].is_null());
    assert_eq!(rows[1]["actor"], 1);
    assert_eq!(rows[1]["event"], "survival.commit");
    assert_eq!(rows[1]["entry_id"], entry.id.0);
    assert_eq!(rows[1]["history_sequence"], 1);
}

#[test]
fn restore_anchors_new_segments_without_historical_flow_republication() {
    let dir = tempfile::tempdir().unwrap();
    let save = dir.path().join("save.db");
    let log = dir.path().join("survival.jsonl");
    let mut engine = Engine::open(&save, Scenario::two_room(42)).unwrap();
    engine.enable_survival_diagnostics(&log).unwrap();
    support::act(&mut engine, tor_server::journal::Action::Wait).unwrap();
    engine.flush().unwrap();
    let first = rows(&log, 3);
    let expected = engine.state(support::CHARACTER).unwrap();
    drop(engine);
    let mut engine = Engine::open(&save, Scenario::two_room(999)).unwrap();
    assert_eq!(engine.state(support::CHARACTER).unwrap(), expected);
    engine.enable_survival_diagnostics(&log).unwrap();
    engine.flush().unwrap();
    let restored = rows(&log, 5);
    assert_eq!(restored[3]["event"], "survival.begin");
    assert_eq!(restored[3]["lineage"], first[0]["lineage"]);
    assert_eq!(restored[3]["entry_id"], first[1]["entry_id"]);
    assert_eq!(restored[3]["history_sequence"], 1);
    assert_ne!(restored[3]["segment"], first[0]["segment"]);
    assert_eq!(
        restored
            .iter()
            .filter(|r| r["event"] == "survival.commit")
            .count(),
        1
    );
    support::act(&mut engine, tor_server::journal::Action::Wait).unwrap();
    engine.flush().unwrap();
    let final_rows = rows(&log, 7);
    assert_eq!(final_rows[5]["history_sequence"], 2);
    assert_eq!(final_rows[6]["durable_sequence"], 2);
}

#[test]
fn late_save_admission_rejection_discards_the_entire_flow_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("survival.jsonl");
    let policy = tor_server::SavePolicy {
        max_pending_bytes: 1,
        ..Default::default()
    };
    let mut engine =
        Engine::open_with_policy(dir.path().join("save.db"), Scenario::two_room(42), policy)
            .unwrap();
    engine.enable_survival_diagnostics(&log).unwrap();
    let before = engine.state(support::CHARACTER).unwrap();
    assert!(support::act(&mut engine, tor_server::journal::Action::Wait).is_err());
    assert_eq!(engine.state(support::CHARACTER).unwrap(), before);
    assert_eq!(engine.save_status().accepted_sequence, 0);
    engine.flush().unwrap();
    let rows = rows(&log, 2);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["event"], "survival.begin");
    assert_eq!(rows[1]["event"], "survival.barrier");
    assert_eq!(rows[1]["history_sequence"], 0);
}
