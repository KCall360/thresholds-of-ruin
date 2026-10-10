use crate::support;
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
        .contains(
            &engine
                .disclose_entry(&placed.entry)
                .expect("completed command has history")
        ));
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
fn rewind_restores_inventory_knowledge_scheduler_without_reusing_actor_identity() {
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
                action: engine
                    .decode_action(
                        ActorId(1),
                        &tor_protocol::Action::Take {
                            item: initial.observation.ground_items[0].item.id,
                            quantity: None,
                        },
                    )
                    .unwrap(),
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
    let actor_created = |entry: &tor_server::journal::JournalEntry| {
        let tor_server::journal::JournalContent::Wizard {
            operation:
                WizardOperation::SpawnActor {
                    position: at,
                    turn_ticks: 75,
                },
            result: tor_server::journal::WizardResult::ActorSpawned { actor },
            ..
        } = &entry.content
        else {
            panic!("expected successful actor creation");
        };
        assert_eq!(*at, position);
        *actor
    };
    let abandoned = actor_created(&spawned.entry);
    let replacement = actor_created(&spawned_again.entry);
    assert!(replacement.0 > abandoned.0);
    assert_eq!(engine.actors(), vec![ActorId(1), replacement]);
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
    let mut archive: serde_json::Value = support::read(&copy);
    archive.as_object_mut().unwrap().remove("wizard_game");
    support::write(&copy, archive);
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
fn failed_marker_commit_never_enables_authority_and_promotion_remains_permanent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.db");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    let blocker = rusqlite::Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
    assert!(engine.enable_wizard().is_err());
    assert!(!engine.wizard_enabled());
    assert!(engine.state(ActorId(1)).unwrap().wizard_game);
    blocker.execute_batch("ROLLBACK").unwrap();
    engine.flush().unwrap();
    engine.enable_wizard().unwrap();
    assert!(engine.wizard_enabled());
}

#[test]
fn pending_wizard_rewind_is_saved_with_its_branch_and_retry_identity() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("game.db");
    let mut engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
    engine.enable_wizard().unwrap();
    let old = engine.branch().clone();
    let result = wizard(
        &mut engine,
        "rewind",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    assert_ne!(engine.branch(), &old);
    engine.flush().unwrap();
    let branch = engine.branch().clone();
    drop(engine);
    let mut resumed = Engine::open(&path, Scenario::two_room(0)).unwrap();
    resumed.enable_wizard().unwrap();
    assert_eq!(resumed.branch(), &branch);
    assert_eq!(
        resumed
            .history_branch(ActorId(1), "wizard", &branch, None, 100)
            .unwrap()
            .entries
            .last()
            .unwrap()
            .id,
        result.entry.id
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
fn old_save_and_corrupt_wizard_journal_do_not_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    drop(Engine::open(&path, Scenario::two_room(0)).unwrap());
    let mut archive: serde_json::Value = support::read(&path);
    archive["version"] = 1.into();
    archive.as_object_mut().unwrap().remove("wizard_game");
    support::write(&path, archive);
    assert_eq!(
        Engine::open(&path, Scenario::two_room(0)).unwrap_err().code,
        ErrorCode::InvalidArchive
    );

    let path = dir.path().join("corrupt.json");
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
    let mut archive: serde_json::Value = support::read(&path);
    archive["records"][0]["entry"]["content"]["result"]["item"] = 999.into();
    support::write(&path, archive);
    let corrupt = std::fs::read(&path).unwrap();
    assert_eq!(
        Engine::open(&path, Scenario::two_room(0)).unwrap_err().code,
        ErrorCode::InvalidArchive
    );
    assert_eq!(std::fs::read(&path).unwrap(), corrupt);
}

/// Wizard operations name a known archetype and carry no forged properties.
#[test]
fn wizard_operations_reject_unknown_archetypes_and_forged_properties() {
    for operation in [
        serde_json::json!({"type":"place_item","kind":"sword","position":{"region":1,"x":1,"y":1,"z":0}}),
        serde_json::json!({"type":"spawn_actor","position":{"region":1,"x":1,"y":1,"z":0},"turn_ticks":100,"god":true}),
    ] {
        assert!(serde_json::from_value::<tor_server::journal::WizardOperation>(operation).is_err());
    }
}

#[test]
fn arena_step_retry_checkpoint_and_rewind_preserve_original_paid_work() {
    let mut scenario = support::load("mob-arena", 42);
    let template = scenario.package.take().unwrap();
    let mut manifest = template.manifest.clone();
    let arena = manifest.arena.as_mut().unwrap();
    arena.start_paused = true;
    arena.control = tor_server::scenario_package::ArenaControl::AllAi;
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
        )
        .unwrap(),
    ));
    let directory = tempdir().unwrap();
    let path = directory.path().join("arena.db");
    let mut engine = Engine::open_with_policy(
        &path,
        scenario.clone(),
        tor_server::SavePolicy {
            checkpoint_interval: 1,
            ..tor_server::SavePolicy::default()
        },
    )
    .unwrap();
    assert!(engine.next_ai_action().is_none());
    assert_eq!(
        wizard(
            &mut engine,
            "denied-step",
            WizardOperation::ArenaControl {
                paused: true,
                advance: 1,
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    let initial_stats = engine
        .observation(ActorId(1))
        .unwrap()
        .combat
        .unwrap()
        .own_stats;
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: WizardOperation::ArenaControl {
            paused: true,
            advance: 1,
        },
    };
    let step = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "one-step",
            &branch,
            command.clone(),
        )
        .unwrap();
    engine.advance_ai(ActorId(1)).unwrap();
    assert!(engine.next_ai_action().is_none());
    let stopped = engine.state(ActorId(1)).unwrap();
    assert!(
        stopped
            .observation
            .combat
            .as_ref()
            .unwrap()
            .preparation_active
    );
    let retry = engine
        .command("wizard", "text", ActorId(1), "one-step", &branch, command)
        .unwrap();
    assert_eq!(retry.entry, step.entry);
    assert_eq!(engine.state(ActorId(1)).unwrap(), stopped);
    assert!(engine.next_ai_action().is_none());
    engine.flush().unwrap();
    drop(engine);
    let mut resumed = Engine::open(&path, scenario).unwrap();
    assert_eq!(resumed.state(ActorId(1)).unwrap(), stopped);
    assert!(resumed.next_ai_action().is_none());
    resumed.enable_wizard().unwrap();
    wizard(
        &mut resumed,
        "rewind-paused",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    let initial = resumed.observation(ActorId(1)).unwrap();
    assert_eq!(initial.tick, 0);
    assert_eq!(initial.combat.unwrap().own_stats, initial_stats);
    assert!(resumed.next_ai_action().is_none());
}

#[test]
fn creature_latest_hd_removal_is_owned_atomic_retryable_and_reversible() {
    let mut scenario = support::load("mob-arena", 42);
    let template = scenario.package.take().unwrap();
    let mut manifest = template.manifest.clone();
    manifest.arena.as_mut().unwrap().start_paused = true;
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
        )
        .unwrap(),
    ));
    let dir = tempdir().unwrap();
    let path = dir.path().join("remove-hd.db");
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    let operation: WizardOperation = serde_json::from_value(serde_json::json!({
        "type": "remove_creature_hit_die", "actor": 1
    }))
    .expect("latest-HD removal is a supported wizard operation");
    let original = engine
        .observation(ActorId(1))
        .unwrap()
        .combat
        .unwrap()
        .own_stats;
    assert_eq!(
        wizard(&mut engine, "unauthorized", operation.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: operation.clone(),
    };
    let removed = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "remove-first",
            &branch,
            command.clone(),
        )
        .unwrap();
    let after = engine.state(ActorId(1)).unwrap();
    let stats = after
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    assert_eq!(stats.hit_dice.len(), 2);
    assert!(!stats.abilities.contains(&Technique::Fear));
    let retry = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "remove-first",
            &branch,
            command,
        )
        .unwrap();
    assert_eq!(removed.entry, retry.entry);
    assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    let missing = serde_json::from_value(
        serde_json::json!({ "type": "remove_creature_hit_die", "actor": 999 }),
    )
    .unwrap();
    assert!(wizard(&mut engine, "missing", missing).is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    engine.enable_wizard().unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    wizard(&mut engine, "remove-second", operation.clone()).unwrap();
    wizard(&mut engine, "remove-last", operation.clone()).unwrap();
    let dead = engine.state(ActorId(1)).unwrap();
    assert!(dead.observation.combat.as_ref().unwrap().dead);
    assert!(dead
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .hit_dice
        .is_empty());
    assert!(wizard(&mut engine, "remove-empty", operation).is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), dead);
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, scenario).unwrap();
    engine.enable_wizard().unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), dead);
    wizard(
        &mut engine,
        "restore-hd",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    assert_eq!(
        engine
            .observation(ActorId(1))
            .unwrap()
            .combat
            .unwrap()
            .own_stats,
        original
    );
}

#[test]
fn stopped_arena_rejects_hit_die_mutation_without_changing_its_result() {
    let mut scenario = support::load("mob-arena", 42);
    let template = scenario.package.take().unwrap();
    let mut manifest = template.manifest.clone();
    let arena = manifest.arena.as_mut().unwrap();
    arena.start_paused = true;
    arena.control = tor_server::scenario_package::ArenaControl::AllAi;
    arena.actions = 1;
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
        )
        .unwrap(),
    ));
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    let add_unspent_owner: WizardOperation = serde_json::from_value(serde_json::json!({
        "type":"advance_creature", "actor":1,
        "advancement":{"type":"add_hit_die","source":"warrior"}
    }))
    .unwrap();
    wizard(&mut engine, "add-before-stop", add_unspent_owner).unwrap();
    wizard(
        &mut engine,
        "step-stop",
        WizardOperation::ArenaControl {
            paused: true,
            advance: 1,
        },
    )
    .unwrap();
    engine.advance_ai(ActorId(1)).unwrap();
    let stopped = engine.state(ActorId(1)).unwrap();
    assert!(stopped.observation.combat.as_ref().unwrap().terminal);
    assert!(wizard(
        &mut engine,
        "edit-stop",
        WizardOperation::RemoveCreatureHitDie { actor: ActorId(1) }
    )
    .is_err());
    assert!(wizard(
        &mut engine,
        "template-edit-stop",
        WizardOperation::SetCreatureTemplate {
            actor: ActorId(1),
            template: "arcane".into(),
            enabled: false
        },
    )
    .is_err());
    for (request, advancement) in [
        (
            "advance-stop",
            serde_json::json!({"type":"add_hit_die","source":"mage"}),
        ),
        (
            "train-stop",
            serde_json::json!({"type":"train","owner":4,"skill":"athletics"}),
        ),
    ] {
        let operation: WizardOperation = serde_json::from_value(serde_json::json!({
            "type":"advance_creature", "actor":1, "advancement":advancement
        }))
        .unwrap();
        assert!(wizard(&mut engine, request, operation).is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), stopped);
    }
    assert_eq!(engine.state(ActorId(1)).unwrap(), stopped);
}

#[test]
fn wizard_templates_use_authored_definitions_and_preserve_replay_retry_and_conflict_atomicity() {
    let mut scenario = support::load("mob-arena", 42);
    let template = scenario.package.take().unwrap();
    let mut manifest = template.manifest.clone();
    manifest.arena.as_mut().unwrap().start_paused = true;
    manifest
        .creatures
        .templates
        .get_mut("arcane")
        .unwrap()
        .grants
        .push(tor_server::creature_authoring::Grant::Mana { amount: 3 });
    for (id, kind) in [("undead_form", "undead"), ("construct_form", "construct")] {
        manifest.creatures.templates.insert(
            id.into(),
            serde_json::from_value(serde_json::json!({
                "priority": 10, "kind": kind
            }))
            .unwrap(),
        );
    }
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
        )
        .unwrap(),
    ));
    let operation = |template: &str, enabled| -> WizardOperation {
        serde_json::from_value(serde_json::json!({
            "type":"set_creature_template", "actor":1, "template":template, "enabled":enabled
        }))
        .expect("template controls are supported wizard operations")
    };
    let dir = tempdir().unwrap();
    let path = dir.path().join("templates.db");
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    let original = engine.state(ActorId(1)).unwrap();
    let initial_stats = original
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .clone()
        .unwrap();
    let initial_mana = initial_stats
        .resources
        .iter()
        .find(|pool| pool.resource == Resource::Mana)
        .unwrap()
        .maximum;
    assert!(initial_mana > 0);
    let remove = operation("arcane", false);
    assert_eq!(
        wizard(&mut engine, "denied-template", remove.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: remove,
    };
    let changed = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "template-off",
            &branch,
            command.clone(),
        )
        .unwrap();
    let after = engine.state(ActorId(1)).unwrap();
    let stats = after
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    assert_eq!(stats.hit_dice, initial_stats.hit_dice);
    assert_eq!(stats.skills, initial_stats.skills);
    assert!(stats.abilities.contains(&Technique::MagicBolt));
    assert_eq!(
        stats
            .resources
            .iter()
            .find(|pool| pool.resource == Resource::Mana)
            .unwrap()
            .maximum,
        initial_mana - 3
    );
    assert_eq!(after.observation.tick, original.observation.tick);
    assert_eq!(
        engine
            .command(
                "wizard",
                "text",
                ActorId(1),
                "template-off",
                &branch,
                command
            )
            .unwrap()
            .entry,
        changed.entry
    );
    assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    assert!(wizard(&mut engine, "unknown-template", operation("missing", true)).is_err());
    assert!(wizard(&mut engine, "already-removed", operation("arcane", false)).is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    engine.enable_wizard().unwrap();
    wizard(&mut engine, "template-on", operation("arcane", true)).unwrap();
    let stats = engine
        .observation(ActorId(1))
        .unwrap()
        .combat
        .unwrap()
        .own_stats
        .unwrap();
    let mana = stats
        .resources
        .iter()
        .find(|pool| pool.resource == Resource::Mana)
        .unwrap();
    assert_eq!(mana.maximum, initial_mana);
    assert_eq!(mana.balance, initial_mana - 3);
    wizard(&mut engine, "undead-on", operation("undead_form", true)).unwrap();
    let transformed = engine.state(ActorId(1)).unwrap();
    assert_eq!(
        transformed
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .kind,
        CreatureType::Undead
    );
    assert!(wizard(
        &mut engine,
        "conflicting-form",
        operation("construct_form", true)
    )
    .is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), transformed);
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, scenario).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), transformed);
    engine.enable_wizard().unwrap();
    wizard(
        &mut engine,
        "rewind-template",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    assert_eq!(
        engine
            .observation(ActorId(1))
            .unwrap()
            .combat
            .unwrap()
            .own_stats
            .unwrap(),
        initial_stats
    );
}

#[test]
fn creature_advancement_is_owned_atomic_retryable_and_persistent() {
    let mut scenario = support::load("mob-arena", 42);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.arena.as_mut().unwrap().start_paused = true;
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(manifest, package.region_defs().unwrap())
            .unwrap(),
    ));
    let operation = |choice: serde_json::Value| -> WizardOperation {
        serde_json::from_value(serde_json::json!({
            "type": "advance_creature", "actor": 1, "advancement": choice
        }))
        .expect("wizard advancement is supported")
    };
    let dir = tempdir().unwrap();
    let path = dir.path().join("advancement.db");
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    let initial = engine.observation(ActorId(1)).unwrap().combat.unwrap();
    let add = operation(serde_json::json!({"type":"add_hit_die","source":"warrior"}));
    assert_eq!(
        wizard(&mut engine, "denied-advance", add.clone())
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    let denied_unknown: WizardOperation = serde_json::from_value(serde_json::json!({
        "type":"advance_creature", "actor":999,
        "advancement":{"type":"add_hit_die","source":"mage"}
    }))
    .unwrap();
    assert_eq!(
        wizard(&mut engine, "denied-unknown-actor", denied_unknown)
            .unwrap_err()
            .code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    let branch = engine.branch().clone();
    let command = Command::Wizard {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        operation: add.clone(),
    };
    let added = engine
        .command(
            "wizard",
            "text",
            ActorId(1),
            "add-hd",
            &branch,
            command.clone(),
        )
        .unwrap();
    let after_add = engine.state(ActorId(1)).unwrap();
    let stats = after_add
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    assert_eq!(stats.hit_dice.len(), 4);
    assert_eq!(
        &stats.hit_dice[..3],
        &initial.own_stats.as_ref().unwrap().hit_dice
    );
    let retry = engine
        .command("wizard", "text", ActorId(1), "add-hd", &branch, command)
        .unwrap();
    assert_eq!(retry.entry, added.entry);
    assert_eq!(engine.state(ActorId(1)).unwrap(), after_add);
    for (id, choice) in [
        (
            "athletics",
            serde_json::json!({"type":"train","owner":4,"skill":"athletics"}),
        ),
        (
            "weapon",
            serde_json::json!({"type":"train","owner":4,"skill":"heavy_weaponry"}),
        ),
        (
            "strength",
            serde_json::json!({"type":"increase_attribute","owner":4,"attribute":"strength"}),
        ),
        (
            "hardiness",
            serde_json::json!({"type":"select_talent","owner":4,"talent":"hardiness"}),
        ),
    ] {
        wizard(&mut engine, id, operation(choice)).unwrap();
    }
    let advanced = engine.state(ActorId(1)).unwrap();
    let stats = advanced
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap();
    assert_eq!(
        stats.attributes.strength,
        initial.own_stats.as_ref().unwrap().attributes.strength + 1
    );
    assert!(stats.active_talents.contains(&Talent::Hardiness));
    assert_eq!(advanced.observation.tick, 0);
    for (id, choice) in [
        (
            "zero-owner",
            serde_json::json!({"type":"train","owner":0,"skill":"lore"}),
        ),
        (
            "missing-owner",
            serde_json::json!({"type":"train","owner":5,"skill":"lore"}),
        ),
        (
            "overspend",
            serde_json::json!({"type":"train","owner":4,"skill":"lore"}),
        ),
        (
            "wrong-opportunity",
            serde_json::json!({"type":"increase_attribute","owner":3,"attribute":"strength"}),
        ),
        (
            "used-attribute",
            serde_json::json!({"type":"increase_attribute","owner":4,"attribute":"speed"}),
        ),
        (
            "used-talent",
            serde_json::json!({"type":"select_talent","owner":4,"talent":"guard"}),
        ),
    ] {
        assert!(wizard(&mut engine, id, operation(choice)).is_err(), "{id}");
        assert_eq!(engine.state(ActorId(1)).unwrap(), advanced);
    }
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, scenario).unwrap();
    assert_eq!(engine.state(ActorId(1)).unwrap(), advanced);
    engine.enable_wizard().unwrap();
    wizard(
        &mut engine,
        "remove-new-owner",
        WizardOperation::RemoveCreatureHitDie { actor: ActorId(1) },
    )
    .unwrap();
    let regressed = engine.observation(ActorId(1)).unwrap().combat.unwrap();
    assert_eq!(regressed.own_stats, initial.own_stats);
    assert_eq!(regressed.hp, initial.hp);
    wizard(&mut engine, "re-add", add).unwrap();
    assert_eq!(
        engine.observation(ActorId(1)).unwrap().combat,
        after_add.observation.combat
    );
    wizard(
        &mut engine,
        "rewind-advance",
        WizardOperation::Rewind { target: None },
    )
    .unwrap();
    let rewound = engine.observation(ActorId(1)).unwrap().combat.unwrap();
    assert_eq!(rewound.own_stats, initial.own_stats);
    assert_eq!(rewound.hp, initial.hp);
}

#[test]
fn privileged_creature_inspection_is_authorized_readonly_and_reconstructed() {
    let mut scenario = support::load("mob-arena", 42);
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.arena.as_mut().unwrap().start_paused = true;
    manifest.arena.as_mut().unwrap().control = tor_server::scenario_package::ArenaControl::AllAi;
    scenario.package = Some(std::sync::Arc::new(
        tor_server::scenario_package::Package::from_parts(manifest, package.region_defs().unwrap())
            .unwrap(),
    ));
    let dir = tempdir().unwrap();
    let path = dir.path().join("inspection.db");
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    for actor in [ActorId(1), ActorId(999)] {
        assert_eq!(
            engine.inspect_creature(actor).unwrap_err().code,
            ErrorCode::Unauthorized
        );
    }
    engine.enable_wizard().unwrap();
    wizard(
        &mut engine,
        "prepare-inspection",
        WizardOperation::ArenaControl {
            paused: true,
            advance: 1,
        },
    )
    .unwrap();
    engine.advance_ai(ActorId(1)).unwrap();
    let before = engine.state(ActorId(1)).unwrap();
    assert!(
        before
            .observation
            .combat
            .as_ref()
            .unwrap()
            .preparation_active
    );
    let report = engine.inspect_creature(ActorId(1)).unwrap();
    assert!(report.validate().is_ok());
    assert_eq!(report.name, "blue adept");
    assert_eq!(report.species.id, "human");
    assert_eq!(report.templates[0].id, "arcane");
    assert_eq!(report.hit_dice[0].talent, Some(Talent::PowerStrike));
    assert_eq!(report.hit_dice[2].talent, Some(Talent::Fear));
    assert!(report.grants.iter().any(|group| group.source
        == InspectionGrantSource::Class {
            class: InspectionClass::Mage
        }
        && group.grants.contains(&InspectionGrant::Magical)));
    assert_eq!(
        report
            .stats
            .resources
            .iter()
            .find(|pool| pool.resource == Resource::Focus)
            .unwrap()
            .reserved,
        1
    );
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    assert!(engine.inspect_creature(ActorId(999)).is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), before);
    let other = engine.inspect_creature(ActorId(2)).unwrap();
    assert_eq!(other.name, "blue sentinel");
    assert_eq!(other.hit_dice[1].talent, Some(Talent::Hardiness));
    assert!(other.validate().is_ok());
    engine.flush().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path, scenario).unwrap();
    assert_eq!(
        engine.inspect_creature(ActorId(1)).unwrap_err().code,
        ErrorCode::Unauthorized
    );
    engine.enable_wizard().unwrap();
    assert_eq!(engine.inspect_creature(ActorId(1)).unwrap(), report);
    assert_eq!(engine.inspect_creature(ActorId(2)).unwrap(), other);
}
