//! Human movement resolves once at checked admission, retaining its raw receipt.
use super::linked_hold_boundary_tests::{establish, establish_with_holders, scenario};
use super::*;
use serde_json::{json, Value};

const SUBJECT: ActorId = ActorId(1);

fn action_request(engine: &Engine, action: Action, direct: bool) -> Command {
    let expected_revision = engine.revision(SUBJECT).unwrap();
    if direct {
        Command::Act {
            expected_revision,
            action,
        }
    } else {
        Command::AdmitIntention {
            expected_revision,
            action,
        }
    }
}

fn movement(engine: &Engine, direct: bool) -> Command {
    action_request(
        engine,
        Action::Move {
            direction: Direction::West,
        },
        direct,
    )
}

fn request(engine: &mut Engine, id: &str, request: Command) -> Result<CommandResult, Failure> {
    engine.command(
        "player",
        "move-escape-test",
        SUBJECT,
        id,
        &engine.branch().clone(),
        request,
    )
}

fn wait(engine: &mut Engine, serial: usize) {
    let actor = engine.next_actor().unwrap();
    engine
        .command(
            "player",
            "move-escape-test",
            actor,
            &format!("advance-{serial}"),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
}

fn authority(game: &Game) -> Value {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    json!([game.checkpoint(&mut shared), shared])
}

fn assert_recovery(engine: &Engine) {
    let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
    assert_eq!(replayed.game, engine.game);
    let restored = Checkpoint::capture(engine)
        .encode("move-escape", 1)
        .restore(engine.archive.clone())
        .unwrap();
    assert_eq!(authority(&restored.game), authority(&engine.game));
}

#[test]
fn held_move_admits_escape_against_oldest_hold_and_retains_raw_move_receipt() {
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    let oldest = engine
        .game
        .linked_holds(SimActor(1))
        .iter()
        .map(|hold| hold.id)
        .min()
        .unwrap();
    assert!(engine.game.pending_intention(SimActor(1)).is_none());
    let mut generic = engine.game.clone();
    assert_eq!(
        generic.admit_intention(
            SimActor(1),
            tor_simulation::Action::Move(tor_world::Direction::West),
            tor_simulation::IntentionOrigin::Human,
        ),
        Err(tor_simulation::GameError::ActorBusy),
        "the generic simulation still rejects held movement; conversion is server-owned"
    );
    assert_eq!(generic, engine.game);
    let requested = movement(&engine, false);
    let tick = engine.game.tick();
    let admitted = request(&mut engine, "move", requested.clone())
        .expect("a held human Move must be accepted as an ordinary paid Escape");
    assert!(
        matches!(admitted.entry.content,
        JournalContent::IntentionAdmitted { action: Action::Escape { hold }, .. } if hold == oldest),
        "a held human move must admit a paid Escape against the oldest active hold"
    );
    let record = engine.archive.records.last().unwrap();
    assert_eq!(record.receipt.as_ref().unwrap().command, requested);
    assert_eq!(
        record.action_resolution,
        ActionResolution::MoveEscape { hold: oldest }
    );
    assert_eq!(
        engine.game.tick(),
        tick,
        "admission does not spend preparation or move time"
    );
    let queued = engine.game.pending_intention(SimActor(1)).unwrap();
    assert_eq!(
        queued.work,
        tor_simulation::IntentionWork::Action(tor_simulation::Action::Escape { hold: oldest })
    );
    assert!(
        queued.movement_context.is_none(),
        "no later step is retained"
    );
    assert_recovery(&engine);
}

#[test]
fn single_hold_move_and_direct_act_match_explicit_escape_without_extra_rng_or_movement() {
    for direct in [false, true] {
        let mut engine = Engine::memory(scenario()).unwrap();
        establish_with_holders(&mut engine, &[ActorId(2)]);
        let hold = engine.game.linked_holds(SimActor(1))[0].id;
        let at = engine.game.observe(SimActor(1)).unwrap().location;
        let mut explicit = Engine::replay(engine.archive.clone(), None, None).unwrap();
        let raw = movement(&engine, direct);
        let chosen = action_request(&explicit, Action::Escape { hold }, direct);
        let accepted = request(&mut engine, "move", raw.clone()).unwrap();
        request(&mut explicit, "escape", chosen).unwrap();
        assert_eq!(
            engine.game, explicit.game,
            "only the private request provenance differs"
        );
        assert_recovery(&engine);
        if !direct {
            let started = engine.execute_next_intention().unwrap().unwrap();
            explicit.execute_next_intention().unwrap().unwrap();
            assert!(matches!(started.entry.content,
                JournalContent::IntentionStarted { action: Action::Escape { hold: actual },
                    event: crate::journal::Event::EscapeStarted { hold: event }, .. }
                    if actual == hold && event == hold));
        } else {
            assert!(matches!(accepted.entry.content,
                JournalContent::Action { action: Action::Escape { hold: actual },
                    event: crate::journal::Event::EscapeStarted { hold: event } }
                    if actual == hold && event == hold));
        }
        assert!(engine.game.preparation(SimActor(1)).is_some());
        for serial in 0..40 {
            if engine.game.linked_holds(SimActor(1)).is_empty() {
                break;
            }
            wait(&mut engine, serial);
            wait(&mut explicit, serial);
            assert_eq!(
                authority(&engine.game),
                authority(&explicit.game),
                "Move pays the same preparation/recovery and exact RNG as explicit Escape"
            );
        }
        assert!(engine.game.linked_holds(SimActor(1)).is_empty());
        assert!(engine.game.pending_intention(SimActor(1)).is_none());
        assert_eq!(engine.game.observe(SimActor(1)).unwrap().location, at);
        assert!(!engine
            .archive
            .records
            .iter()
            .any(|record| record.entry.actor == SUBJECT
                && matches!(
                    record.entry.content,
                    JournalContent::Action {
                        event: crate::journal::Event::Moved { .. },
                        ..
                    } | JournalContent::IntentionStarted {
                        event: crate::journal::Event::Moved { .. },
                        ..
                    }
                )));
        let completed = authority(&engine.game);
        let repeated = request(&mut engine, "move", raw.clone()).unwrap();
        assert!(repeated.duplicate);
        assert_eq!(repeated.entry, accepted.entry);
        assert_eq!(authority(&engine.game), completed);
        let mut conflict = raw;
        match &mut conflict {
            Command::Act { action, .. } | Command::AdmitIntention { action, .. } => {
                *action = Action::Move {
                    direction: Direction::North,
                };
            }
            _ => unreachable!(),
        }
        assert_eq!(
            request(&mut engine, "move", conflict).unwrap_err().code,
            ErrorCode::RequestConflict
        );
        assert_eq!(authority(&engine.game), completed);
        assert_recovery(&engine);
    }
}

#[test]
fn explicit_alternative_stays_exact_and_protection_is_not_a_move_candidate() {
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    let holds = engine.game.linked_holds(SimActor(1));
    let newer = holds.iter().map(|hold| hold.id).max().unwrap();
    let older = holds.iter().map(|hold| hold.id).min().unwrap();
    let selected = action_request(&engine, Action::Escape { hold: newer }, false);
    let accepted = request(&mut engine, "explicit", selected).unwrap();
    assert!(matches!(accepted.entry.content,
        JournalContent::IntentionAdmitted { action: Action::Escape { hold }, .. } if hold == newer));
    assert_eq!(
        engine.archive.records.last().unwrap().action_resolution,
        ActionResolution::Unchanged
    );
    engine.execute_next_intention().unwrap().unwrap();
    for serial in 0..40 {
        if engine.game.linked_holds(SimActor(1)).len() == 1 {
            break;
        }
        wait(&mut engine, serial);
    }
    assert_eq!(
        engine
            .game
            .linked_holds(SimActor(1))
            .iter()
            .map(|hold| hold.id)
            .collect::<Vec<_>>(),
        vec![older]
    );
    let raw = movement(&engine, false);
    let admitted = request(&mut engine, "move", raw).unwrap();
    assert!(matches!(admitted.entry.content,
        JournalContent::IntentionAdmitted { action: Action::Escape { hold }, .. } if hold == older));
    assert_recovery(&engine);
}

#[test]
fn stale_move_metadata_and_busy_slots_preserve_exact_authority_and_receipts() {
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    for direct in [false, true] {
        let mut stale = movement(&engine, direct);
        match &mut stale {
            Command::Act {
                expected_revision, ..
            }
            | Command::AdmitIntention {
                expected_revision, ..
            } => *expected_revision += 1,
            _ => unreachable!(),
        }
        let before = authority(&engine.game);
        let archive = serde_json::to_value(&engine.archive).unwrap();
        assert_eq!(
            request(&mut engine, "stale", stale).unwrap_err().code,
            ErrorCode::StaleRevision
        );
        assert_eq!(authority(&engine.game), before);
        assert_eq!(serde_json::to_value(&engine.archive).unwrap(), archive);
    }
    let chosen = engine.game.linked_holds(SimActor(1))[1].id;
    let explicit = action_request(&engine, Action::Escape { hold: chosen }, false);
    request(&mut engine, "existing", explicit).unwrap();
    for suspended in [false, true] {
        if suspended {
            engine.suspend_queued_intention(SUBJECT).unwrap().unwrap();
        }
        let before = authority(&engine.game);
        let archive = serde_json::to_value(&engine.archive).unwrap();
        let raw = movement(&engine, false);
        assert_eq!(
            request(&mut engine, "busy", raw).unwrap_err().code,
            ErrorCode::ActorBusy
        );
        assert_eq!(authority(&engine.game), before);
        assert_eq!(serde_json::to_value(&engine.archive).unwrap(), archive);
    }
    assert_recovery(&engine);
}

#[test]
fn move_admission_preserves_existing_paid_preparation_like_explicit_escape() {
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    let holds = engine.game.linked_holds(SimActor(1));
    let first = holds.iter().map(|hold| hold.id).min().unwrap();
    let second = holds.iter().map(|hold| hold.id).max().unwrap();
    let explicit = action_request(&engine, Action::Escape { hold: second }, false);
    request(&mut engine, "paid", explicit).unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    let paid = engine.game.preparation(SimActor(1)).unwrap().clone();
    let mut comparison = Engine::replay(engine.archive.clone(), None, None).unwrap();
    let raw = movement(&engine, false);
    let explicit = action_request(&comparison, Action::Escape { hold: first }, false);
    request(&mut engine, "move", raw).unwrap();
    request(&mut comparison, "escape", explicit).unwrap();
    assert_eq!(engine.game, comparison.game);
    assert_eq!(
        engine.game.preparation(SimActor(1)),
        Some(&paid),
        "admission never cancels paid work"
    );
    assert_recovery(&engine);
}

#[test]
fn released_selected_hold_fails_queued_escape_without_rebinding_to_remaining_hold() {
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    engine.enable_wizard().unwrap();
    let holds = engine.game.linked_holds(SimActor(1));
    let oldest = holds.iter().min_by_key(|hold| hold.id).unwrap();
    let other = holds.iter().find(|hold| hold.id != oldest.id).unwrap();
    let raw = movement(&engine, false);
    let admitted = request(&mut engine, "move", raw.clone()).unwrap();
    let warp = Command::Wizard {
        expected_revision: engine.revision(SUBJECT).unwrap(),
        operation: WizardOperation::Teleport {
            actor: ActorId(oldest.holder.0),
            position: Position {
                region: 1,
                x: 4,
                y: 2,
                z: 0,
            },
        },
    };
    request(&mut engine, "release-holder", warp).unwrap();
    assert_eq!(engine.game.linked_holds(SimActor(1)), vec![*other]);
    let rng = authority(&engine.game)[0]["rules_rng"].clone();
    let at = engine.game.observe(SimActor(1)).unwrap().location;
    let failed = engine.execute_next_intention().unwrap().unwrap();
    assert!(
        matches!(failed.entry.content, JournalContent::IntentionFailed { admission, .. }
        if admission == admitted.entry.id)
    );
    assert_eq!(authority(&engine.game)[0]["rules_rng"], rng);
    assert_eq!(engine.game.observe(SimActor(1)).unwrap().location, at);
    assert_eq!(engine.game.linked_holds(SimActor(1)), vec![*other]);
    assert!(engine.game.pending_intention(SimActor(1)).is_none());
    assert!(engine.game.preparation(SimActor(1)).is_none());
    assert!(request(&mut engine, "move", raw).unwrap().duplicate);
    assert_recovery(&engine);
}

#[test]
fn move_resolution_is_required_strict_and_cannot_authorize_other_receipt_shapes() {
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    let raw = movement(&engine, false);
    request(&mut engine, "move", raw).unwrap();
    let original = engine.archive.records.last().unwrap();
    let encoded = serde_json::to_value(original).unwrap();
    let mut missing = encoded.clone();
    missing.as_object_mut().unwrap().remove("action_resolution");
    assert!(serde_json::from_value::<Record>(missing).is_err());
    for invalid in [
        json!(null),
        json!([]),
        json!(["move_escape", 1]),
        json!({"kind":"unknown"}),
        json!({"kind":"unchanged", "hold":1}),
        json!({"kind":"move_escape", "hold":0}),
        json!({"kind":"move_escape", "hold":u64::MAX}),
        json!({"kind":"move_escape", "hold":1, "extra":true}),
    ] {
        let mut malformed = encoded.clone();
        malformed["action_resolution"] = invalid;
        assert!(serde_json::from_value::<Record>(malformed).is_err());
    }
    let other = engine.game.linked_holds(SimActor(1))[1].id;
    for mutation in 0..8 {
        let mut record = original.clone();
        match mutation {
            0 => record.action_resolution = ActionResolution::Unchanged,
            1 => record.action_resolution = ActionResolution::MoveEscape { hold: other },
            2 => record.receipt = None,
            3 => record.receipt.as_mut().unwrap().command = Command::PausePreparation,
            4 => record.entry.actor = ActorId(2),
            5 => record.entry.branch = BranchId(new_id().0),
            6 => {
                record.entry.author = Author::Backend {
                    component: "scheduler".into(),
                }
            }
            _ => record.entry.audience = Audience::Actor,
        }
        assert!(!record.valid_admission(), "forgery {mutation}");
    }
}

#[test]
fn forged_newer_hold_is_rejected_by_replay_and_authoritative_checkpoint_predecessor() {
    for (direct, private_gap) in [(false, false), (false, true), (true, false)] {
        let mut engine = Engine::memory(scenario()).unwrap();
        establish(&mut engine);
        let newer = engine
            .game
            .linked_holds(SimActor(1))
            .iter()
            .map(|hold| hold.id)
            .max()
            .unwrap();
        let raw = movement(&engine, direct);
        let explicit = action_request(&engine, Action::Escape { hold: newer }, direct);
        let accepted = request(&mut engine, "forged-move", explicit).unwrap();
        let index = engine.archive.records.len() - 1;
        if private_gap {
            for serial in 0..REWIND_BOUNDARIES {
                engine.suspend_queued_intention(SUBJECT).unwrap().unwrap();
                let resume = Command::ResumeIntention {
                    expected_revision: engine.revision(SUBJECT).unwrap(),
                    admission: accepted.entry.id.clone(),
                };
                request(&mut engine, &format!("resume-{serial}"), resume).unwrap();
            }
            assert!(!engine
                .boundaries
                .iter()
                .any(|boundary| boundary.id.as_ref() == Some(&accepted.entry.id)));
        }
        assert_recovery(&engine);
        let mut forged = engine.archive.clone();
        let record = &mut forged.records[index];
        record.receipt.as_mut().unwrap().command = raw;
        record.action_resolution = ActionResolution::MoveEscape { hold: newer };
        assert!(
            record.valid_admission(),
            "structural evidence alone cannot prove oldest selection"
        );
        assert!(Engine::replay(forged.clone(), None, None).is_err());
        assert!(Checkpoint::capture(&engine).encode("wrong-oldest", 1).restore(forged).is_err(),
            "the matching queue/preparation cannot replace the exact preceding hold selection proof");
    }
}

#[test]
fn completed_move_receipt_survives_retired_hold_and_expired_checkpoint_boundaries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("move-escape.db");
    let scenario = scenario();
    let mut engine = Engine::open(&path, scenario.clone()).unwrap();
    establish_with_holders(&mut engine, &[ActorId(2)]);
    let raw = movement(&engine, false);
    let admitted = request(&mut engine, "move", raw.clone()).unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    for serial in 0..REWIND_BOUNDARIES + 40 {
        wait(&mut engine, serial);
    }
    assert!(engine.game.linked_holds(SimActor(1)).is_empty());
    assert!(!engine
        .boundaries
        .iter()
        .any(|boundary| boundary.id.as_ref() == Some(&admitted.entry.id)));
    assert_recovery(&engine);
    let completed = authority(&engine.game);
    engine.flush().unwrap();
    drop(engine);
    let mut reopened = Engine::open(&path, scenario).unwrap();
    let retried = request(&mut reopened, "move", raw).unwrap();
    assert!(retried.duplicate);
    assert_eq!(retried.entry, admitted.entry);
    assert_eq!(authority(&reopened.game), completed);
    assert!(matches!(
        reopened.request_receipt(&retried),
        RequestReceipt::Admitted {
            phase: IntentionPhase::Resolved,
            ..
        }
    ));
}

#[test]
fn unheld_move_keeps_its_original_action_and_direct_escape_requires_exact_event() {
    for direct in [false, true] {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let raw = action_request(
            &engine,
            Action::Move {
                direction: Direction::East,
            },
            direct,
        );
        request(&mut engine, "unheld-move", raw).unwrap();
        let record = engine.archive.records.last().unwrap();
        assert_eq!(record.action_resolution, ActionResolution::Unchanged);
        assert!(matches!(
            record.entry.content,
            JournalContent::Action {
                action: Action::Move {
                    direction: Direction::East
                },
                ..
            } | JournalContent::IntentionAdmitted {
                action: Action::Move {
                    direction: Direction::East
                },
                ..
            }
        ));
        assert_recovery(&engine);
    }
    let mut engine = Engine::memory(scenario()).unwrap();
    establish(&mut engine);
    let raw = movement(&engine, true);
    request(&mut engine, "direct-move", raw).unwrap();
    let original = engine.archive.records.last().unwrap();
    let other = engine.game.linked_holds(SimActor(1))[1].id;
    for mutation in 0..4 {
        let mut record = original.clone();
        match mutation {
            0 => {
                let JournalContent::Action { event, .. } = &mut record.entry.content else {
                    unreachable!()
                };
                *event = crate::journal::Event::EscapeStarted { hold: other };
            }
            1 => {
                let JournalContent::Action { action, .. } = &mut record.entry.content else {
                    unreachable!()
                };
                *action = Action::Escape { hold: other };
            }
            2 => {
                let Command::Act { action, .. } = &mut record.receipt.as_mut().unwrap().command
                else {
                    unreachable!()
                };
                *action = Action::Wait;
            }
            _ => {
                record.receipt.as_mut().unwrap().command = Command::CancelIntention {
                    expected_revision: engine.revision(SUBJECT).unwrap(),
                    admission: original.entry.id.clone(),
                    step: Some(Direction::West),
                };
            }
        }
        assert!(!record.valid_admission(), "direct forgery {mutation}");
    }
}
