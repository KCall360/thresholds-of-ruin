use tempfile::tempdir;
use tor_protocol::*;
use tor_server::journal::Command;
use tor_server::{Engine, Scenario};

fn note(anchor: Anchor, text: &str, audience: Audience) -> Command {
    Command::Annotate {
        anchor,
        text: text.into(),
        audience,
        category: AnnotationCategory::Note,
        source: ClientSource::User,
    }
}

#[test]
fn notes_preserve_state_and_pending_action_revision_and_survive_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    let branch = engine.branch().clone();
    let before = engine.observation(ActorId(1)).unwrap();
    let annotation = engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "note-1",
            &branch,
            note(
                Anchor::State { revision: 0 },
                "Remember this doorway.",
                Audience::Private,
            ),
        )
        .unwrap();
    assert_eq!(engine.observation(ActorId(1)).unwrap(), before);
    assert_eq!(engine.revision(ActorId(1)).unwrap(), 0);
    assert_eq!(annotation.entry.tick, 0);
    engine
        .command(
            "alice",
            "ascii",
            ActorId(1),
            "move-1",
            &branch,
            Command::Act {
                expected_revision: 0,
                action: Action::Move {
                    direction: Direction::East,
                },
            },
        )
        .unwrap();
    let expected_observation = engine.observation(ActorId(1)).unwrap();
    let expected_history = engine.history(ActorId(1), "alice", None, 100).unwrap();
    drop(engine);
    let resumed = Engine::open(&path, Scenario::two_room(99)).unwrap();
    assert_eq!(
        expected_observation,
        resumed.observation(ActorId(1)).unwrap()
    );
    assert_eq!(
        expected_history,
        resumed.history(ActorId(1), "alice", None, 100).unwrap()
    );
    assert_eq!(resumed.branch(), &branch);
}

#[test]
fn duplicate_commands_return_original_receipts_even_after_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    let command = Command::Act {
        expected_revision: 0,
        action: Action::Wait,
    };
    let first = engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "request-1",
            &branch,
            command.clone(),
        )
        .unwrap();
    drop(engine);
    let mut resumed = Engine::open(&path, Scenario::two_room(0)).unwrap();
    let retry = resumed
        .command("alice", "ascii", ActorId(1), "request-1", &branch, command)
        .unwrap();
    assert!(retry.duplicate);
    assert_eq!(first.entry, retry.entry);
    assert_eq!(resumed.observation(ActorId(1)).unwrap().tick, 100);
    assert_eq!(
        resumed
            .command(
                "alice",
                "text",
                ActorId(1),
                "request-1",
                &branch,
                note(
                    Anchor::State { revision: 1 },
                    "different request",
                    Audience::Private
                )
            )
            .unwrap_err()
            .code,
        ErrorCode::RequestConflict
    );
}

#[test]
fn private_notes_and_private_anchors_are_not_exposed_to_other_users() {
    let mut engine = Engine::memory(Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    let private = engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "private",
            &branch,
            note(
                Anchor::State { revision: 0 },
                "My private plan",
                Audience::Private,
            ),
        )
        .unwrap();
    assert!(engine
        .history(ActorId(1), "bob", None, 100)
        .unwrap()
        .entries
        .is_empty());
    assert_eq!(
        engine
            .command(
                "bob",
                "ascii",
                ActorId(1),
                "guess",
                &branch,
                note(
                    Anchor::Entry {
                        id: private.entry.id.clone()
                    },
                    "Guess",
                    Audience::Private
                )
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidAnchor
    );
    assert_eq!(
        engine
            .command(
                "alice",
                "text",
                ActorId(1),
                "leak",
                &branch,
                note(
                    Anchor::Entry {
                        id: private.entry.id
                    },
                    "Shared reference",
                    Audience::Actor
                )
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidAnchor
    );
    engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "shared",
            &branch,
            note(
                Anchor::State { revision: 0 },
                "Shared plan",
                Audience::Actor,
            ),
        )
        .unwrap();
    let bob = engine.history(ActorId(1), "bob", None, 100).unwrap();
    assert_eq!(bob.entries.len(), 1);
    assert_eq!(
        bob.entries[0].author,
        Author::User {
            user: "alice".into()
        }
    );
}

#[test]
fn provenance_is_stamped_and_notes_cannot_execute_actions() {
    let mut engine = Engine::memory(Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    let mut command = note(
        Anchor::State { revision: 0 },
        "move east; take token",
        Audience::Actor,
    );
    if let Command::Annotate { source, .. } = &mut command {
        *source = ClientSource::Frontend;
    }
    let result = engine
        .command("alice", "ascii", ActorId(1), "frontend", &branch, command)
        .unwrap();
    assert_eq!(
        result.entry.author,
        Author::Frontend {
            user: "alice".into(),
            component: "ascii".into()
        }
    );
    let backend = engine
        .annotate_backend(
            ActorId(1),
            "simulation",
            Anchor::State { revision: 0 },
            AnnotationCategory::Explanation,
            "This is a test explanation.",
        )
        .unwrap();
    assert_eq!(
        backend.author,
        Author::Backend {
            component: "simulation".into()
        }
    );
    assert_eq!(engine.observation(ActorId(1)).unwrap().tick, 0);
    assert_eq!(engine.revision(ActorId(1)).unwrap(), 0);
}

#[test]
fn a_save_cannot_be_opened_by_two_writers() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(
        Engine::open(&path, Scenario::two_room(0)).unwrap_err().code,
        ErrorCode::StorageFailure
    );
    drop(engine);
    Engine::open(&path, Scenario::two_room(0)).unwrap();
}

#[test]
fn failed_save_does_not_publish_or_mutate_a_command() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let backup = dir.path().join("committed.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    let before = engine.observation(ActorId(1)).unwrap();
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        engine
            .command(
                "alice",
                "text",
                ActorId(1),
                "failure",
                &branch,
                Command::Act {
                    expected_revision: 0,
                    action: Action::Wait
                }
            )
            .unwrap_err()
            .code,
        ErrorCode::StorageFailure
    );
    assert_eq!(engine.observation(ActorId(1)).unwrap(), before);
    assert!(engine
        .history(ActorId(1), "alice", None, 100)
        .unwrap()
        .entries
        .is_empty());
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&backup, &path).unwrap();
    assert!(
        !engine
            .command(
                "alice",
                "text",
                ActorId(1),
                "failure",
                &branch,
                Command::Act {
                    expected_revision: 0,
                    action: Action::Wait
                }
            )
            .unwrap()
            .duplicate
    );
}

#[test]
fn annotation_validation_and_filtered_history_pagination() {
    let mut engine = Engine::memory(Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    for (id, text) in [
        ("blank", " ".to_owned()),
        ("long", "x".repeat(MAX_NOTE_BYTES + 1)),
        ("control", "bad\0text".to_owned()),
    ] {
        assert_eq!(
            engine
                .command(
                    "alice",
                    "text",
                    ActorId(1),
                    id,
                    &branch,
                    note(Anchor::State { revision: 0 }, &text, Audience::Private)
                )
                .unwrap_err()
                .code,
            ErrorCode::InvalidAnnotation
        );
    }
    assert_eq!(
        engine
            .command(
                "alice",
                "text",
                ActorId(1),
                "future",
                &branch,
                note(Anchor::State { revision: 1 }, "Not yet", Audience::Private)
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidAnchor
    );
    assert_eq!(
        engine
            .command(
                "alice",
                "text",
                ActorId(1),
                "branch",
                &BranchId("wrong".into()),
                note(
                    Anchor::State { revision: 0 },
                    "Wrong branch",
                    Audience::Private
                )
            )
            .unwrap_err()
            .code,
        ErrorCode::WrongBranch
    );
    for i in 0..5 {
        engine
            .command(
                "alice",
                "text",
                ActorId(1),
                &format!("note-{i}"),
                &branch,
                note(
                    Anchor::State { revision: 0 },
                    &format!("Note {i}"),
                    if i % 2 == 0 {
                        Audience::Actor
                    } else {
                        Audience::Private
                    },
                ),
            )
            .unwrap();
    }
    let recent = engine.history(ActorId(1), "bob", None, 2).unwrap();
    assert_eq!(recent.entries.len(), 2);
    let earlier = engine
        .history(ActorId(1), "bob", recent.older_before.as_ref(), 2)
        .unwrap();
    assert_eq!(earlier.entries.len(), 1);
    assert!(earlier.older_before.is_none());
    assert_eq!(
        engine
            .history(ActorId(1), "alice", None, 100)
            .unwrap()
            .entries
            .len(),
        5
    );
}

#[test]
fn replay_preserves_all_origins_and_action_anchors_without_executing_notes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    let action = engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "action",
            &branch,
            Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        )
        .unwrap();
    let anchor = Anchor::Entry {
        id: action.entry.id,
    };
    engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "user",
            &branch,
            note(anchor.clone(), "A user note", Audience::Private),
        )
        .unwrap();
    let mut frontend = note(anchor.clone(), "A frontend note", Audience::Actor);
    if let Command::Annotate { source, .. } = &mut frontend {
        *source = ClientSource::Frontend;
    }
    engine
        .command("alice", "text", ActorId(1), "frontend", &branch, frontend)
        .unwrap();
    engine
        .annotate_backend(
            ActorId(1),
            "simulation",
            anchor,
            AnnotationCategory::Explanation,
            "A backend note",
        )
        .unwrap();
    let expected = engine.history(ActorId(1), "alice", None, 100).unwrap();
    drop(engine);
    let replayed = Engine::open(&path, Scenario::two_room(99)).unwrap();
    assert_eq!(
        replayed.history(ActorId(1), "alice", None, 100).unwrap(),
        expected
    );
    assert_eq!(replayed.observation(ActorId(1)).unwrap().tick, 100);
    assert_eq!(replayed.revision(ActorId(1)).unwrap(), 1);
}

#[test]
fn incompatible_or_inconsistent_archives_are_rejected_without_overwrite() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    let branch = engine.branch().clone();
    engine
        .command(
            "alice",
            "text",
            ActorId(1),
            "action",
            &branch,
            Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        )
        .unwrap();
    drop(engine);
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut unsupported = original.clone();
    unsupported["version"] = 999.into();
    let mut inconsistent = original;
    inconsistent["records"][0]["entry"]["tick"] = 1000.into();
    for bad in [unsupported, inconsistent] {
        let bytes = serde_json::to_vec(&bad).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            Engine::open(&path, Scenario::two_room(0)).unwrap_err().code,
            ErrorCode::InvalidArchive
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
