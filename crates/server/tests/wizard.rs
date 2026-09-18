use tempfile::tempdir;
use tor_protocol::*;
use tor_server::journal::{Command, Position, WizardItem, WizardOperation};
use tor_server::{Engine, Scenario};

fn wizard(
    engine: &mut Engine,
    id: &str,
    operation: WizardOperation,
) -> Result<tor_server::CommandResult, tor_server::Failure> {
    let branch = engine.branch().clone();
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    engine.command(
        "wizard",
        "text",
        ActorId(1),
        id,
        &branch,
        Command::Wizard {
            expected_revision,
            operation,
        },
    )
}

#[test]
fn wizard_marker_rewind_and_retained_future_survive_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("wizard.json");
    let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
    assert!(!engine.state(ActorId(1)).unwrap().wizard_game);
    let operation = WizardOperation::Teleport {
        actor: ActorId(1),
        position: Position {
            region: 2,
            x: 1,
            y: 1,
            z: 0,
        },
    };
    assert_eq!(
        wizard(&mut engine, "denied", operation.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    let original = engine.branch().clone();
    let placed = wizard(&mut engine, "teleport", operation).unwrap();
    assert!(engine
        .observation(ActorId(1))
        .unwrap()
        .ground_items
        .iter()
        .any(|i| i.item.name == "stone tablet"
            && i.position == tor_protocol::Position { x: 1, y: 0, z: 0 }));
    wizard(
        &mut engine,
        "rewind",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    assert_ne!(engine.branch(), &original);
    assert!(engine
        .observation(ActorId(1))
        .unwrap()
        .ground_items
        .iter()
        .any(|i| i.item.name == "copper token" && i.reachable));
    assert!(engine.state(ActorId(1)).unwrap().wizard_game);
    assert!(engine
        .history_branch(ActorId(1), "wizard", &original, None, 100)
        .unwrap()
        .entries
        .contains(&placed.entry.disclosed()));
    let state = engine.state(ActorId(1)).unwrap();
    let branch = engine.branch().clone();
    drop(engine);
    let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    assert_eq!(engine.branch(), &branch);
    assert!(!engine.wizard_enabled());
}

#[test]
fn invalid_placement_is_atomic_and_successful_placement_is_idempotent() {
    let mut engine = Engine::memory(Scenario::two_room(1)).unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    assert!(wizard(
        &mut engine,
        "bad",
        WizardOperation::PlaceItem {
            kind: WizardItem::Token,
            position: Position {
                region: 1,
                x: -1,
                y: 1,
                z: 0
            }
        }
    )
    .is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: before.revision,
        operation: WizardOperation::PlaceItem {
            kind: WizardItem::Tablet,
            position: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
        },
    };
    let first = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "place",
            &branch,
            command.clone(),
        )
        .unwrap();
    let second = engine
        .command("wizard", "text", ActorId(1), "place", &branch, command)
        .unwrap();
    assert!(second.duplicate);
    assert_eq!(first.entry, second.entry);
    assert_eq!(
        engine.observation(ActorId(1)).unwrap().ground_items.len(),
        before.observation.ground_items.len() + 1
    );
    assert_eq!(engine.observation(ActorId(1)).unwrap().tick, 0);
}

#[test]
fn rewind_restores_inventory_knowledge_scheduler_and_identity_allocation() {
    let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
    engine.enable_wizard().unwrap();
    let initial = engine.state(ActorId(1)).unwrap();
    let position = Position {
        region: 2,
        x: 2,
        y: 1,
        z: 0,
    };
    let spawned = wizard(
        &mut engine,
        "spawn",
        WizardOperation::SpawnActor {
            position,
            turn_ticks: 75,
        },
    )
    .unwrap();
    let branch = engine.branch().clone();
    engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "take",
            &branch,
            Command::Act {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: Action::Take {
                    item: initial.observation.ground_items[0].item.id,
                },
            },
        )
        .unwrap();
    assert!(!engine.observation(ActorId(1)).unwrap().ready);
    wizard(
        &mut engine,
        "tele",
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: Position { x: 1, ..position },
        },
    )
    .unwrap();
    wizard(
        &mut engine,
        "undo",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), initial);
    assert_eq!(engine.actors(), vec![ActorId(1)]);
    let spawned_again = wizard(
        &mut engine,
        "spawn-again",
        WizardOperation::SpawnActor {
            position,
            turn_ticks: 75,
        },
    )
    .unwrap();
    assert_eq!(spawned.entry.content, spawned_again.entry.content);
}

#[test]
fn promotion_without_commands_survives_copy_and_disabled_restart() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("normal.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    drop(engine);
    let copy = dir.path().join("copy.json");
    std::fs::copy(&path, &copy).unwrap();
    let mut resumed = Engine::open(&copy, Scenario::two_room(0)).unwrap();
    assert!(resumed.state(ActorId(1)).unwrap().wizard_game);
    assert!(!resumed.wizard_enabled());
    assert_eq!(
        wizard(
            &mut resumed,
            "denied",
            WizardOperation::Rewind { target: None }
        )
        .unwrap_err()
        .code,
        ErrorCode::Unauthorized
    );
    drop(resumed);
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    archive.as_object_mut().unwrap().remove("wizard_game");
    std::fs::write(&copy, serde_json::to_vec(&archive).unwrap()).unwrap();
    assert_eq!(
        Engine::open(&copy, Scenario::two_room(0)).unwrap_err().code,
        ErrorCode::InvalidArchive
    );
}

#[test]
fn rewind_retry_after_restart_recovers_receipt_without_another_fork() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: 0,
        operation: WizardOperation::Rewind { target: None },
    };
    let first = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "retry",
            &branch,
            command.clone(),
        )
        .unwrap();
    let fork = engine.branch().clone();
    drop(engine);
    let mut resumed = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(
        resumed
            .command(
                "wizard",
                "text",
                ActorId(1),
                "retry",
                &branch,
                command.clone()
            )
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    resumed.enable_wizard().unwrap();
    let retried = resumed
        .command("wizard", "text", ActorId(1), "retry", &branch, command)
        .unwrap();
    assert!(retried.duplicate);
    assert_eq!(retried.entry, first.entry);
    assert_eq!(resumed.branch(), &fork);
}

#[test]
fn failed_marker_commit_does_not_enable_or_mark_the_game() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        engine.enable_wizard().unwrap_err().code,
        ErrorCode::StorageFailure
    );
    assert!(!engine.wizard_enabled());
    assert!(!engine.state(ActorId(1)).unwrap().wizard_game);
}

#[test]
fn failed_wizard_commit_preserves_state_branch_and_retry_identity() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.json");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    let saved = std::fs::read(&path).unwrap();
    let state = engine.state(ActorId(1)).unwrap();
    let branch = engine.branch().clone();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        wizard(
            &mut engine,
            "retry",
            WizardOperation::Rewind { target: None }
        )
        .unwrap_err()
        .code,
        ErrorCode::StorageFailure
    );
    assert_eq!(engine.state(ActorId(1)).unwrap(), state);
    assert_eq!(engine.branch(), &branch);
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, saved).unwrap();
    assert!(
        !wizard(
            &mut engine,
            "retry",
            WizardOperation::Rewind { target: None }
        )
        .unwrap()
        .duplicate
    );
}

#[test]
fn rewind_targets_are_bounded_and_notes_keep_original_branch_anchors_and_privacy() {
    let mut engine = Engine::memory(Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    let branch = engine.branch().clone();
    let first = wizard(
        &mut engine,
        "first",
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: Position {
                region: 1,
                x: 2,
                y: 1,
                z: 0,
            },
        },
    )
    .unwrap();
    let note = Command::Annotate {
        anchor: Anchor::Entry {
            id: first.entry.id.clone(),
        },
        text: "Retained private future".into(),
        source: ClientSource::User,
        audience: Audience::Private,
        category: AnnotationCategory::Note,
    };
    engine
        .command("wizard", "text", ActorId(1), "note", &branch, note.clone())
        .unwrap();
    wizard(
        &mut engine,
        "rewind",
        WizardOperation::Rewind {
            target: Some(first.entry.id.clone()),
        },
    )
    .unwrap();
    assert!(engine
        .history(ActorId(1), "wizard", None, 100)
        .unwrap()
        .entries
        .iter()
        .all(|e| e.branch != branch));
    let new_branch = engine.branch().clone();
    assert_eq!(
        engine
            .command(
                "wizard",
                "text",
                ActorId(1),
                "cross-anchor",
                &new_branch,
                note
            )
            .unwrap_err()
            .code,
        ErrorCode::InvalidAnchor
    );
    assert_eq!(
        engine
            .history_branch(ActorId(1), "wizard", &branch, None, 100)
            .unwrap()
            .entries
            .len(),
        2
    );
    assert!(engine
        .history_branch(ActorId(1), "spectator", &branch, None, 100)
        .unwrap()
        .entries
        .is_empty());
    for i in 0..128 {
        wizard(
            &mut engine,
            &format!("fill-{i}"),
            WizardOperation::Teleport {
                actor: ActorId(1),
                position: Position {
                    region: 1,
                    x: 2,
                    y: 1,
                    z: 0,
                },
            },
        )
        .unwrap();
    }
    let before = engine.state(ActorId(1)).unwrap();
    assert!(wizard(
        &mut engine,
        "expired",
        WizardOperation::Rewind {
            target: Some(first.entry.id)
        }
    )
    .is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
}

#[test]
fn old_normal_save_migrates_but_corrupt_wizard_journal_does_not_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    drop(Engine::open(&path, Scenario::two_room(0)).unwrap());
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    archive["version"] = 1.into();
    archive.as_object_mut().unwrap().remove("wizard_game");
    std::fs::write(&path, serde_json::to_vec(&archive).unwrap()).unwrap();
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    wizard(
        &mut engine,
        "place",
        WizardOperation::PlaceItem {
            kind: WizardItem::Tablet,
            position: Position {
                region: 1,
                x: 1,
                y: 1,
                z: 0,
            },
        },
    )
    .unwrap();
    drop(engine);
    let mut archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    archive["records"][0]["entry"]["content"]["result"]["item"] = 999.into();
    let corrupt = serde_json::to_vec(&archive).unwrap();
    std::fs::write(&path, &corrupt).unwrap();
    assert_eq!(
        Engine::open(&path, Scenario::two_room(0)).unwrap_err().code,
        ErrorCode::InvalidArchive
    );
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
}
