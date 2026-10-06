use tempfile::tempdir;
use tor_protocol::{Action, ActorId, ErrorCode};
use tor_server::{journal::Command, Engine, SavePolicy, Scenario};

fn wait(engine: &mut Engine, id: &str) -> tor_server::CommandResult {
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            id,
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap()
}

#[test]
fn checkpoint_size_and_action_io_do_not_scale_with_retained_history() {
    let dir = tempdir().unwrap();
    let mut sizes = Vec::new();
    for history in [1000, 10000] {
        let mut memory = Engine::memory(Scenario::two_room(42)).unwrap();
        memory.seed_profile_history(history).unwrap();
        let path = dir.path().join(format!("scale-{history}.db"));
        let mut engine = memory
            .attach_profile_save_with_policy(
                &path,
                SavePolicy {
                    checkpoint_interval: 1,
                    ..SavePolicy::default()
                },
            )
            .unwrap();
        let (_, profile) = engine
            .command_profiled(
                "player",
                "test",
                ActorId(1),
                "capture",
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert_eq!(profile.checkpoint_captures, 1);
        assert_eq!(profile.records_serialized, 1);
        assert_eq!(
            (
                profile.file_writes,
                profile.file_syncs,
                profile.bytes_written
            ),
            (0, 0, 0)
        );
        assert!(profile.exclusive_duration() <= profile.authoritative_total);
        engine.flush().unwrap();
        sizes.push(engine.save_status().checkpoint_bytes);
        // Counting diagnostics use the production encoding without allocating a
        // second payload, and must agree byte-for-byte in size with the writer.
        assert_eq!(
            engine.profile_checkpoint_encoding().unwrap().0,
            engine.save_status().checkpoint_bytes
        );
        let expected = engine.state(ActorId(1)).unwrap();
        drop(engine);
        let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
        assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
        assert_eq!(restored.recovery_profile().records_replayed, 0);
        assert_eq!(restored.recovery_profile().records_loaded, history + 1);
    }
    assert!(
        sizes[1] <= sizes[0] * 11 / 10,
        "checkpoint must not embed retained history: {sizes:?}"
    );
}

#[test]
fn checkpoint_preserves_queued_admissions_across_both_full_retained_windows() {
    use tor_server::{
        journal::{JournalContent, Position, WizardOperation},
        ActorSetup,
    };
    let dir = tempdir().unwrap();
    let path = dir.path().join("retained-queues.db");
    let mut scenario = Scenario::two_room(42);
    scenario.regions = 3;
    scenario.actors = (0..4)
        .flat_map(|y| {
            (0..16).map(move |x| ActorSetup {
                position: Position {
                    region: 1,
                    x,
                    y,
                    z: 0,
                },
                turn_ticks: 100,
            })
        })
        .collect();
    let mut engine = Engine::memory(scenario).unwrap();
    let branch = engine.branch().clone();
    let mut roots = std::collections::BTreeMap::new();
    for id in 1..=64 {
        let actor = ActorId(id);
        let result = engine
            .command(
                "player",
                "test",
                actor,
                &format!("admit-{id}"),
                &branch,
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        roots.insert(actor, result.entry.id);
    }
    engine.enable_wizard().unwrap();
    let mark = |engine: &mut Engine, id: &str| {
        engine
            .command(
                "player",
                "test",
                ActorId(1),
                id,
                &branch,
                Command::Wizard {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    operation: WizardOperation::Teleport {
                        actor: ActorId(1),
                        position: Position {
                            region: 1,
                            x: 0,
                            y: 0,
                            z: 0,
                        },
                    },
                },
            )
            .unwrap();
    };
    for index in 0..129 {
        mark(&mut engine, &format!("mark-{index}"));
    }
    let mut last_admission = None;
    for index in 0..64 {
        engine
            .command(
                "player",
                "test",
                ActorId(1),
                &format!("cancel-{index}"),
                &branch,
                Command::CancelIntention {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    admission: roots[&ActorId(1)].clone(),
                },
            )
            .unwrap();
        let command = Command::AdmitIntention {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            action: Action::Wait,
        };
        let result = engine
            .command(
                "player",
                "test",
                ActorId(1),
                &format!("readmit-{index}"),
                &branch,
                command.clone(),
            )
            .unwrap();
        roots.insert(ActorId(1), result.entry.id);
        last_admission = Some(command);
    }
    assert_eq!(engine.profile_counts().1, 256);
    let mut engine = engine
        .attach_profile_save_with_policy(
            &path,
            SavePolicy {
                checkpoint_interval: 1,
                ..SavePolicy::default()
            },
        )
        .unwrap();
    // A private queue change captures without overlapping the selectable window.
    engine
        .command(
            "player",
            "test",
            ActorId(2),
            "capture",
            &branch,
            Command::CancelIntention {
                expected_revision: engine.revision(ActorId(2)).unwrap(),
                admission: roots[&ActorId(2)].clone(),
            },
        )
        .unwrap();
    engine.flush().unwrap();
    let bytes = engine.save_status().checkpoint_bytes;
    assert!(
        bytes > 0 && bytes < 1024 * 1024,
        "shared queue checkpoint is {bytes} bytes"
    );
    assert_eq!(engine.profile_checkpoint_encoding().unwrap().0, bytes);
    let expected: Vec<_> = (1..=64)
        .map(|id| engine.state(ActorId(id)).unwrap())
        .collect();
    drop(engine);
    let mut restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.recovery_profile().records_replayed, 0);
    assert_eq!(restored.profile_counts().1, 256);
    for (index, state) in expected.into_iter().enumerate() {
        assert_eq!(restored.state(ActorId(index as u64 + 1)).unwrap(), state);
    }
    let retry = restored
        .command(
            "player",
            "test",
            ActorId(1),
            "readmit-63",
            &branch,
            last_admission.unwrap(),
        )
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry.id, roots[&ActorId(1)]);
    let started = restored.execute_next_intention().unwrap().unwrap();
    assert!(
        matches!(&started.entry.content, JournalContent::IntentionStarted { admission, action: Action::Wait, .. }
        if admission == &roots[&ActorId(1)])
    );
    restored
        .command(
            "player",
            "test",
            ActorId(3),
            "cancel-restored",
            &branch,
            Command::CancelIntention {
                expected_revision: restored.revision(ActorId(3)).unwrap(),
                admission: roots[&ActorId(3)].clone(),
            },
        )
        .unwrap();
    restored.flush().unwrap();
}

#[test]
fn checkpoint_rotates_replay_tail_without_losing_history_or_retry_results() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.db");
    let policy = SavePolicy {
        checkpoint_interval: 4,
        ..SavePolicy::default()
    };
    let mut engine = Engine::open_with_policy(&path, Scenario::two_room(42), policy).unwrap();
    let branch = engine.branch().clone();
    let first = wait(&mut engine, "first");
    for n in 1..10 {
        wait(&mut engine, &format!("wait-{n}"));
    }
    engine.flush().unwrap();
    let state = engine.state(ActorId(1)).unwrap();
    let history = engine.history(ActorId(1), "player", None, 100).unwrap();
    assert!(engine.save_status().checkpoint_sequence >= 4);
    drop(engine);
    let mut resumed = Engine::open(&path, Scenario::two_room(999)).unwrap();
    assert_eq!(resumed.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        resumed.history(ActorId(1), "player", None, 100).unwrap(),
        history
    );
    assert!(resumed.recovery_profile().records_replayed < 4);
    let retry = resumed
        .command(
            "player",
            "test",
            ActorId(1),
            "first",
            &branch,
            Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        )
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.entry, first.entry);
    assert_eq!(
        resumed
            .command(
                "player",
                "test",
                ActorId(1),
                "first",
                &branch,
                Command::Act {
                    expected_revision: 1,
                    action: Action::Wait
                }
            )
            .unwrap_err()
            .code,
        ErrorCode::RequestConflict
    );
    let conn = rusqlite::Connection::open(&path).unwrap();
    let active: i64 = conn
        .query_row("SELECT count(*) FROM journal WHERE sequence > 0", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(active < 4);
    let retained: i64 = conn
        .query_row("SELECT count(*) FROM history", [], |r| r.get(0))
        .unwrap();
    assert_eq!(active + retained, 10);
}

#[test]
fn checkpoints_preserve_private_notes_forks_and_the_complete_rewind_window() {
    use tor_protocol::{Anchor, AnnotationCategory, Audience, ClientSource};
    use tor_server::journal::{Position, WizardOperation};
    let dir = tempdir().unwrap();
    let path = dir.path().join("wizard.db");
    let mut engine = Engine::open_with_policy(
        &path,
        Scenario::two_room(42),
        SavePolicy {
            checkpoint_interval: 8,
            ..SavePolicy::default()
        },
    )
    .unwrap();
    engine.enable_wizard().unwrap();
    let original = engine.branch().clone();
    let first = wait(&mut engine, "first");
    for n in 0..135 {
        wait(&mut engine, &format!("wait-{n}"));
    }
    let target = wait(&mut engine, "target");
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            "teleport",
            &engine.branch().clone(),
            Command::Wizard {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                operation: WizardOperation::Teleport {
                    actor: ActorId(1),
                    position: Position {
                        region: 2,
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                },
            },
        )
        .unwrap();
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            "rewind",
            &engine.branch().clone(),
            Command::Wizard {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                operation: WizardOperation::Rewind {
                    target: Some(target.entry.id.clone()),
                },
            },
        )
        .unwrap();
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            "secret",
            &engine.branch().clone(),
            Command::Annotate {
                anchor: Anchor::State {
                    revision: engine.revision(ActorId(1)).unwrap(),
                },
                text: "private checkpoint note".into(),
                source: ClientSource::User,
                audience: Audience::Private,
                category: AnnotationCategory::Note,
            },
        )
        .unwrap();
    for n in 0..8 {
        wait(&mut engine, &format!("tail-{n}"));
    }
    engine.flush().unwrap();
    let state = engine.state(ActorId(1)).unwrap();
    let private = engine.history(ActorId(1), "player", None, 100).unwrap();
    let public = engine.history(ActorId(1), "observer", None, 100).unwrap();
    assert_ne!(private, public);
    drop(engine);
    // An independent full replay of identical retained records is the oracle.
    let replay_path = dir.path().join("full-replay.db");
    std::fs::copy(&path, &replay_path).unwrap();
    let conn = rusqlite::Connection::open(&replay_path).unwrap();
    conn.execute_batch("BEGIN; INSERT INTO journal SELECT * FROM history; DELETE FROM history; DELETE FROM checkpoint; COMMIT;").unwrap();
    drop(conn);
    let mut full = Engine::open(&replay_path, Scenario::two_room(0)).unwrap();
    let mut resumed = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(resumed.state(ActorId(1)).unwrap(), state);
    assert_eq!(
        resumed.state(ActorId(1)).unwrap(),
        full.state(ActorId(1)).unwrap()
    );
    assert_eq!(resumed.profile_counts(), full.profile_counts());
    assert_eq!(
        resumed.history(ActorId(1), "player", None, 100).unwrap(),
        private
    );
    assert_eq!(
        resumed.history(ActorId(1), "observer", None, 100).unwrap(),
        public
    );
    assert!(!resumed.wizard_enabled());
    for engine in [&mut resumed, &mut full] {
        assert!(
            engine
                .command(
                    "player",
                    "test",
                    ActorId(1),
                    "first",
                    &original,
                    Command::Act {
                        expected_revision: 0,
                        action: Action::Wait
                    }
                )
                .unwrap()
                .duplicate
        );
        engine.enable_wizard().unwrap();
        let revision = engine.revision(ActorId(1)).unwrap();
        assert!(engine
            .command(
                "player",
                "test",
                ActorId(1),
                "expired",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: revision,
                    operation: WizardOperation::Rewind {
                        target: Some(first.entry.id.clone())
                    }
                }
            )
            .is_err());
        engine
            .command(
                "player",
                "test",
                ActorId(1),
                "retained",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: revision,
                    operation: WizardOperation::Rewind {
                        target: Some(target.entry.id.clone()),
                    },
                },
            )
            .unwrap();
    }
    assert_eq!(
        resumed.observation(ActorId(1)).unwrap(),
        full.observation(ActorId(1)).unwrap()
    );
    assert!(resumed.state(ActorId(1)).unwrap().wizard_game);
}

#[test]
fn damaged_or_misidentified_checkpoint_fails_closed_without_falling_back() {
    for damage in [
        "checksum",
        "sequence",
        "missing",
        "oversized",
        "history_gap",
        "old_format",
    ] {
        let dir = tempdir().unwrap();
        let path = dir.path().join("corrupt.db");
        let mut engine = Engine::open_with_policy(
            &path,
            Scenario::two_room(1),
            SavePolicy {
                checkpoint_interval: 2,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        wait(&mut engine, "one");
        wait(&mut engine, "two");
        engine.flush().unwrap();
        drop(engine);
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(match damage {
            "checksum" => "UPDATE checkpoint SET checksum=checksum+1",
            "sequence" => "UPDATE checkpoint SET sequence=sequence+1",
            "missing" => "DELETE FROM checkpoint",
            "oversized" => "UPDATE checkpoint SET payload=zeroblob(67108865)",
            "history_gap" => "DELETE FROM history WHERE sequence=1",
            "old_format" => "PRAGMA user_version=6",
            _ => unreachable!(),
        })
        .unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        assert!(
            Engine::open(&path, Scenario::two_room(0)).is_err(),
            "{damage}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
