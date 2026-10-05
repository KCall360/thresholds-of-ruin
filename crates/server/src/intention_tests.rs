use super::*;

#[cfg(test)]
mod intention_admission_tests {
    use super::*;

    #[test]
    fn boundary_retention_is_a_bounded_union_of_gameplay_and_audit_windows() {
        let mut retained = VecDeque::new();
        let mut selectable = Vec::new();
        for index in 0_usize..1_000 {
            let gameplay = index < 600 && index % 3 == 0;
            if gameplay {
                selectable.push(index);
            }
            retained.push_back((index, gameplay));
            retain_boundaries(&mut retained, |b| b.1);
            assert!(retained.len() <= MAX_RETAINED_BOUNDARIES);
            let raw_start = (index + 1).saturating_sub(REWIND_BOUNDARIES);
            for raw in raw_start..=index {
                assert!(retained.iter().any(|b| b.0 == raw));
            }
            for selected in selectable.iter().rev().take(REWIND_BOUNDARIES) {
                assert!(retained.iter().any(|b| b.0 == *selected));
            }
        }
        assert_eq!(retained.len(), MAX_RETAINED_BOUNDARIES);
        assert!(
            !retained.iter().any(|b| b.0 == 0),
            "old gameplay still expires"
        );
    }

    #[test]
    fn private_intention_traffic_preserves_bounded_selectable_rewind_history() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        for index in 0..70 {
            engine
                .command(
                    "p",
                    "test",
                    actor,
                    &format!("admit-{index}"),
                    &engine.branch().clone(),
                    Command::AdmitIntention {
                        expected_revision: engine.revision(actor).unwrap(),
                        action: Action::Wait,
                    },
                )
                .unwrap();
            engine.execute_next_intention().unwrap().unwrap();
        }
        assert!(
            engine.boundaries.iter().any(|b| b.id.is_none()),
            "admission must not halve the selectable rewind window"
        );
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "pending",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        for index in 0..200 {
            engine.suspend_queued_intention(actor).unwrap().unwrap();
            engine
                .command(
                    "p",
                    "test",
                    actor,
                    &format!("resume-{index}"),
                    &engine.branch().clone(),
                    Command::ResumeIntention {
                        expected_revision: engine.revision(actor).unwrap(),
                        admission: admitted.entry.id.clone(),
                    },
                )
                .unwrap();
        }
        assert!(engine.boundaries.len() <= REWIND_BOUNDARIES * 2);
        assert!(engine.boundaries.iter().any(|b| b.id.is_none()));
        let mut restored = Checkpoint::capture(&engine)
            .encode("rewind", 541)
            .restore(engine.archive.clone())
            .unwrap();
        let mut corrupt = engine.archive.clone();
        let hidden = corrupt
            .records
            .iter_mut()
            .find(|r| matches!(r.entry.content, JournalContent::IntentionChanged { .. }))
            .unwrap();
        assert!(!engine
            .boundaries
            .iter()
            .any(|b| b.id.as_ref() == Some(&hidden.entry.id)));
        hidden.entry.tick += 1;
        assert!(
            Checkpoint::capture(&engine)
                .encode("corrupt-gap", 541)
                .restore(corrupt)
                .is_err(),
            "unretained metadata must still be validated"
        );
        let mut replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(restored.game, engine.game);
        assert_eq!(replayed.game, engine.game);
        for recovered in [&mut engine, &mut restored, &mut replayed] {
            recovered.enable_wizard().unwrap();
            recovered
                .command(
                    "p",
                    "test",
                    actor,
                    "rewind",
                    &recovered.branch().clone(),
                    Command::Wizard {
                        expected_revision: recovered.revision(actor).unwrap(),
                        operation: WizardOperation::Rewind { target: None },
                    },
                )
                .unwrap();
            assert_eq!(recovered.game.tick(), 0);
            assert!(recovered.pending_intentions(actor).is_empty());
        }
    }

    #[test]
    fn queued_lifecycle_replays_retries_and_cancels_original_identity_without_effects() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let branch = engine.branch().clone();
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "queued",
                &branch,
                Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            )
            .unwrap();
        let before = engine.state(actor).unwrap();
        let suspended = engine.suspend_queued_intention(actor).unwrap().unwrap();
        assert!(engine.archive.records.last().unwrap().receipt.is_none());
        assert_eq!(engine.state(actor).unwrap(), before);
        assert_eq!(
            engine.pending_intentions(actor)[0].entry_id,
            admitted.entry.id
        );
        assert_eq!(
            engine.pending_intentions(actor)[0].phase,
            IntentionPhase::Suspended
        );
        assert!(engine.execute_next_intention().unwrap().is_none());
        assert!(engine.suspend_queued_intention(actor).unwrap().is_none());
        let verify = |engine: &Engine| {
            let replay = Engine::replay(engine.archive.clone(), None, None).unwrap();
            let restored = Checkpoint::capture(engine)
                .encode("lifecycle", 1)
                .restore(engine.archive.clone())
                .unwrap();
            for recovered in [&replay, &restored] {
                assert_eq!(recovered.game, engine.game);
                assert_eq!(
                    recovered.pending_intentions(actor),
                    engine.pending_intentions(actor)
                );
                assert_eq!(
                    recovered.request_receipt(&admitted),
                    engine.request_receipt(&admitted)
                );
            }
        };
        verify(&engine);
        for corrupt in 0..4 {
            let mut archive = engine.archive.clone();
            let record = archive.records.last_mut().unwrap();
            match corrupt {
                0 => record.entry.actor = ActorId(2),
                1 => {
                    if let JournalContent::IntentionChanged { intention, .. } =
                        &mut record.entry.content
                    {
                        *intention = tor_simulation::IntentionId(99);
                    }
                }
                2 => {
                    if let JournalContent::IntentionChanged { change, .. } =
                        &mut record.entry.content
                    {
                        *change = crate::journal::IntentionChange::Resumed;
                    }
                }
                _ => {
                    record.entry.author = Author::User {
                        user: "forged".into(),
                    }
                }
            }
            assert!(Engine::replay(archive.clone(), None, None).is_err());
            assert!(Checkpoint::capture(&engine)
                .encode("lifecycle", 1)
                .restore(archive)
                .is_err());
        }
        let command = Command::ResumeIntention {
            expected_revision: 0,
            admission: admitted.entry.id.clone(),
        };
        let resumed = engine
            .command("p", "test", actor, "resume", &branch, command.clone())
            .unwrap();
        assert_eq!(engine.state(actor).unwrap(), before);
        assert_eq!(
            engine.pending_intentions(actor)[0].phase,
            IntentionPhase::Queued
        );
        let retried = engine
            .command("p", "other", actor, "resume", &branch, command)
            .unwrap();
        assert!(retried.duplicate);
        assert_eq!(retried.entry, resumed.entry);
        verify(&engine);
        engine.execute_next_intention().unwrap().unwrap();
        verify(&engine);
        let revision = engine.revision(actor).unwrap();
        let next = engine
            .command(
                "p",
                "test",
                actor,
                "second",
                &branch,
                Command::AdmitIntention {
                    expected_revision: revision,
                    action: Action::Wait,
                },
            )
            .unwrap();
        let before = engine.game.clone();
        let count = engine.archive.records.len();
        assert!(engine
            .command(
                "p",
                "test",
                actor,
                "foreign",
                &branch,
                Command::CancelIntention {
                    expected_revision: revision,
                    admission: suspended.entry.id
                }
            )
            .is_err());
        assert_eq!(engine.game, before);
        assert_eq!(engine.archive.records.len(), count);
        let cancelled = engine
            .command(
                "p",
                "test",
                actor,
                "cancel",
                &branch,
                Command::CancelIntention {
                    expected_revision: revision,
                    admission: next.entry.id.clone(),
                },
            )
            .unwrap();
        assert!(cancelled.entry.disclosed().is_none());
        assert_eq!(
            cancelled.entry.intention_ends[0].kind,
            crate::journal::IntentionEndKind::Cancelled
        );
        assert_eq!(engine.game.tick(), before.tick());
        assert!(engine.pending_intentions(actor).is_empty());
        assert!(matches!(
            engine.request_receipt(&next),
            RequestReceipt::Admitted {
                phase: IntentionPhase::Cancelled,
                ..
            }
        ));
        verify(&engine);
    }

    #[test]
    fn rejected_suspension_and_cancellation_preserve_saved_queue_and_archive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lifecycle.db");
        let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        let branch = engine.branch().clone();
        let admitted = engine
            .command(
                "p",
                "test",
                ActorId(1),
                "queued",
                &branch,
                Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            )
            .unwrap();
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(
            &path,
            Scenario::two_room(42),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let before = engine.game.clone();
        let count = engine.archive.records.len();
        assert_eq!(
            engine
                .suspend_queued_intention(ActorId(1))
                .unwrap_err()
                .code,
            ErrorCode::StorageFailure
        );
        assert_eq!(engine.game, before);
        assert_eq!(engine.archive.records.len(), count);
        assert_eq!(
            engine
                .command(
                    "p",
                    "test",
                    ActorId(1),
                    "cancel",
                    &branch,
                    Command::CancelIntention {
                        expected_revision: 0,
                        admission: admitted.entry.id
                    }
                )
                .unwrap_err()
                .code,
            ErrorCode::StorageFailure
        );
        assert_eq!(engine.game, before);
        assert_eq!(engine.archive.records.len(), count);
        assert!(
            crate::Service::with_outbound_limits(engine, crate::OutboundLimits::default()).is_err(),
            "startup must refuse to run when required suspension cannot be journaled"
        );
    }

    #[test]
    fn attack_progress_snapshot_and_receipt_resolve_with_another_actors_effects() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&root, 42, None, false).unwrap()).unwrap();
        let actor = ActorId(1);
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        assert!(matches!(
            engine.request_receipt(&admitted),
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ));
        let progress = engine.pending_intentions(actor);
        assert_eq!(
            progress.len(),
            1,
            "snapshots must retain running attack identity"
        );
        assert_eq!(progress[0].phase, IntentionPhase::Started);
        assert_eq!(progress[0].entry_id, admitted.entry.id);
        let mut restored = Checkpoint::capture(&engine)
            .encode("attack", 1)
            .restore(engine.archive.clone())
            .unwrap();
        assert_eq!(restored.pending_intentions(actor), progress);
        let result = restored.advance_ai(ActorId(2)).unwrap();
        assert!(restored.game.preparation(SimActor(1)).is_none());
        assert!(
            matches!(
                restored.request_receipt(&admitted),
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Resolved,
                    ..
                }
            ),
            "another actor's action must commit the original attack conclusion"
        );
        assert!(restored.pending_intentions(actor).is_empty());
        assert_eq!(
            Engine::replay(restored.archive.clone(), None, None)
                .unwrap()
                .game,
            restored.game
        );
        assert_eq!(
            Checkpoint::capture(&restored)
                .encode("attack", 2)
                .restore(restored.archive.clone())
                .unwrap()
                .request_receipt(&admitted),
            restored.request_receipt(&admitted)
        );
        assert_ne!(result.entry.id, admitted.entry.id);
        for corruption in ["identity", "actor", "duplicate", "kind", "missing"] {
            let mut archive = restored.archive.clone();
            let ends = &mut archive.records.last_mut().unwrap().entry.intention_ends;
            match corruption {
                "identity" => ends[0].intention = tor_simulation::IntentionId(99),
                "actor" => ends[0].actor = ActorId(2),
                "duplicate" => ends.push(ends[0].clone()),
                "kind" => ends[0].kind = crate::journal::IntentionEndKind::Failed,
                "missing" => ends.clear(),
                _ => unreachable!(),
            }
            assert!(
                Engine::replay(archive.clone(), None, None).is_err(),
                "{corruption}"
            );
            assert!(
                Checkpoint::capture(&restored)
                    .encode("attack", 2)
                    .restore(archive)
                    .is_err(),
                "checkpoint accepted {corruption}"
            );
        }
    }

    #[test]
    fn session_resolves_dead_queued_actor_without_control_or_a_due_turn() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        engine
            .game
            .configure_combat(SimActor(1), tor_simulation::combat::CombatSpec::default())
            .unwrap();
        engine
            .command(
                "p",
                "test",
                actor,
                "queued",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            )
            .unwrap();
        // A valid checkpoint fixture with death between admission and selection.
        // Keep damage-rule coverage in simulation; this tests Session driving.
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let mut snapshot = serde_json::to_value(engine.game.checkpoint(&mut shared)).unwrap();
        snapshot["actors"]["1"]["combat"]["hp"] = serde_json::json!(0);
        engine.game =
            Game::restore_checkpoint(serde_json::from_value(snapshot).unwrap(), &shared).unwrap();
        assert!(!engine.alive(actor));
        assert_ne!(engine.next_actor(), Some(actor));
        let mut service = crate::Service::new(engine);
        assert!(
            matches!(service.step(), crate::session::Step::Progress),
            "dead queued work must resolve without a controller"
        );
        assert!(
            matches!(service.step(), crate::session::Step::Blocked),
            "terminal work must be consumed exactly once"
        );
    }

    #[test]
    fn admission_is_durable_and_retryable_without_simulation_effects_or_history() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let branch = engine.branch().clone();
        let before = engine.state(actor).unwrap();
        let command = Command::AdmitIntention {
            expected_revision: 0,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        let result = engine
            .command("p", "test", actor, "queued", &branch, command.clone())
            .unwrap();
        let JournalContent::IntentionAdmitted { intention, action } = &result.entry.content else {
            panic!("admission must have its own journal content");
        };
        assert_eq!(
            *action,
            Action::Move {
                direction: Direction::East
            }
        );
        assert_eq!(
            engine.game.pending_intention(SimActor(1)).unwrap().id,
            *intention
        );
        assert_eq!(engine.state(actor).unwrap(), before);
        assert!(result.entry.disclosed().is_none());
        assert!(engine
            .history_branch(actor, "p", &branch, None, 10)
            .unwrap()
            .entries
            .is_empty());
        let retry = engine
            .command("p", "other-client", actor, "queued", &branch, command)
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, result.entry);
        assert_eq!(engine.archive.records.len(), 1);
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(replayed.game, engine.game);
        let checkpoint = Checkpoint::capture(&engine).encode("test-save", 1);
        let restored = checkpoint.restore(engine.archive.clone()).unwrap();
        assert_eq!(restored.game, engine.game);
        assert_eq!(
            restored
                .retry(
                    "p",
                    actor,
                    "queued",
                    &branch,
                    &Command::AdmitIntention {
                        expected_revision: 0,
                        action: Action::Move {
                            direction: Direction::East
                        },
                    }
                )
                .unwrap()
                .unwrap()
                .entry,
            result.entry
        );
    }

    #[test]
    fn rejected_persistence_does_not_publish_an_admission_or_consume_its_identity() {
        let directory = tempfile::tempdir().unwrap();
        let mut engine = Engine::open_with_policy(
            directory.path().join("queued.db"),
            Scenario::two_room(42),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let before = engine.game.clone();
        let branch = engine.branch().clone();
        for _ in 0..2 {
            let error = engine
                .command(
                    "p",
                    "test",
                    ActorId(1),
                    "queued",
                    &branch,
                    Command::AdmitIntention {
                        expected_revision: 0,
                        action: Action::Wait,
                    },
                )
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::StorageFailure);
            assert_eq!(engine.game, before);
            assert!(engine.archive.records.is_empty());
            assert!(engine.receipts.is_empty());
            assert_eq!(engine.boundaries.len(), 1);
        }
    }

    #[test]
    fn checkpoint_rejects_admission_records_that_disagree_with_receipts() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine
            .command(
                "p",
                "test",
                ActorId(1),
                "queued",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Wait,
                },
            )
            .unwrap();
        for case in 0..5 {
            let mut archive = engine.archive.clone();
            let record = &mut archive.records[0];
            match case {
                0 => {
                    record.receipt.as_mut().unwrap().command = Command::Act {
                        expected_revision: 0,
                        action: Action::Wait,
                    }
                }
                1 => {
                    record.entry.content = JournalContent::IntentionAdmitted {
                        intention: tor_simulation::IntentionId(1),
                        action: Action::Move {
                            direction: Direction::East,
                        },
                    }
                }
                2 => {
                    record.entry.content = JournalContent::IntentionAdmitted {
                        intention: tor_simulation::IntentionId(0),
                        action: Action::Wait,
                    }
                }
                3 => {
                    record.entry.author = Author::User {
                        user: "other".into(),
                    }
                }
                4 => {
                    record.entry.content = JournalContent::IntentionAdmitted {
                        intention: tor_simulation::IntentionId(99),
                        action: Action::Wait,
                    }
                }
                _ => unreachable!(),
            }
            assert!(
                Engine::replay(archive.clone(), None, None).is_err(),
                "replay case {case}"
            );
            assert!(
                Checkpoint::capture(&engine)
                    .encode("test-save", 1)
                    .restore(archive)
                    .is_err(),
                "checkpoint case {case}"
            );
        }
    }
}

#[cfg(test)]
mod intention_execution_tests {
    use super::*;

    #[test]
    fn wait_updates_readiness_revisions_for_both_the_acting_and_next_actor() {
        let mut engine = Engine::memory(Scenario::performance(42, 16, 2).unwrap()).unwrap();
        assert_eq!(engine.game.next_actor(), Some(SimActor(1)));
        let before = [
            engine.revision(ActorId(1)).unwrap(),
            engine.revision(ActorId(2)).unwrap(),
        ];
        admit(&mut engine, Action::Wait);
        engine.execute_next_intention().unwrap().unwrap();
        assert_eq!(engine.game.next_actor(), Some(SimActor(2)));
        assert!(engine.revision(ActorId(1)).unwrap() > before[0]);
        assert!(engine.revision(ActorId(2)).unwrap() > before[1]);
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            engine.game
        );
    }

    #[test]
    fn checkpoint_rejects_duplicate_execution_linkage_with_an_unchanged_record_count() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let first = admit(&mut engine, Action::Wait);
        engine.execute_next_intention().unwrap().unwrap();
        engine
            .command(
                "p",
                "test",
                ActorId(1),
                "queued-again",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let mut archive = engine.archive.clone();
        let JournalContent::IntentionAdmitted { intention, .. } = first.entry.content else {
            unreachable!()
        };
        let JournalContent::IntentionStarted {
            admission,
            intention: duplicate,
            ..
        } = &mut archive.records[3].entry.content
        else {
            unreachable!()
        };
        *admission = first.entry.id;
        *duplicate = intention;
        assert!(Engine::replay(archive.clone(), None, None).is_err());
        assert!(Checkpoint::capture(&engine)
            .encode("test-save", 1)
            .restore(archive)
            .is_err());
    }

    #[test]
    fn rewind_restores_pending_work_on_a_new_branch_without_reusing_future_ids() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let first = admit(&mut engine, Action::Wait);
        let retained = engine
            .command(
                "p",
                "test",
                ActorId(1),
                "retained",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: 0,
                    operation: WizardOperation::SetPlaceHint {
                        position: Position {
                            region: 1,
                            x: 3,
                            y: 1,
                            z: 0,
                        },
                        present: true,
                    },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let abandoned = engine
            .command(
                "p",
                "test",
                ActorId(1),
                "abandoned",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        engine
            .command(
                "p",
                "test",
                ActorId(1),
                "rewind",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    operation: WizardOperation::Rewind {
                        target: Some(retained.entry.id),
                    },
                },
            )
            .unwrap();
        let JournalContent::IntentionAdmitted {
            intention: first_id,
            ..
        } = first.entry.content
        else {
            unreachable!()
        };
        let JournalContent::IntentionAdmitted {
            intention: abandoned_id,
            ..
        } = abandoned.entry.content
        else {
            unreachable!()
        };
        assert_eq!(
            engine.game.pending_intention(SimActor(1)).unwrap().id,
            first_id
        );
        let repeated = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(repeated.entry.content, JournalContent::IntentionStarted { intention, .. } if intention == first_id)
        );
        let latest = engine
            .command(
                "p",
                "test",
                ActorId(1),
                "new-future",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert!(
            matches!(latest.entry.content, JournalContent::IntentionAdmitted { intention, .. } if intention > abandoned_id)
        );
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            engine.game
        );
        assert_eq!(
            Checkpoint::capture(&engine)
                .encode("test-save", 1)
                .restore(engine.archive.clone())
                .unwrap()
                .game,
            engine.game
        );
    }

    fn admit(engine: &mut Engine, action: Action) -> CommandResult {
        engine
            .command(
                "p",
                "test",
                ActorId(1),
                "queued",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action,
                },
            )
            .unwrap()
    }

    #[test]
    fn scheduled_execution_has_linked_history_and_preserves_the_original_receipt() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let accepted = admit(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        );
        let branch = engine.branch().clone();
        let before = engine.state(ActorId(1)).unwrap();
        let intention = engine.game.pending_intention(SimActor(1)).unwrap().id;
        let result = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(&result.entry.content, JournalContent::IntentionStarted {
            admission, intention: actual, action: Action::Move { direction: Direction::East }, ..
        } if admission == &accepted.entry.id && actual == &intention)
        );
        assert!(engine.game.pending_intention(SimActor(1)).is_none());
        assert_ne!(engine.state(ActorId(1)).unwrap(), before);
        assert!(engine.archive.records.last().unwrap().receipt.is_none());
        assert!(engine.execute_next_intention().unwrap().is_none());
        let retry = engine
            .command(
                "p",
                "test",
                ActorId(1),
                "queued",
                &branch,
                Command::AdmitIntention {
                    expected_revision: 0,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, accepted.entry);
        let history = engine.history(ActorId(1), "p", None, 10).unwrap();
        assert_eq!(history.entries, vec![result.entry.disclosed().unwrap()]);
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(replayed.game, engine.game);
        let restored = Checkpoint::capture(&engine)
            .encode("test-save", 1)
            .restore(engine.archive.clone())
            .unwrap();
        assert_eq!(restored.game, engine.game);
        assert_eq!(
            restored.history(ActorId(1), "p", None, 10).unwrap(),
            history
        );
        for case in 0..2 {
            let mut archive = engine.archive.clone();
            if case == 0 {
                let JournalContent::IntentionStarted { admission, .. } =
                    &mut archive.records[1].entry.content
                else {
                    unreachable!()
                };
                *admission = EntryId(Uuid::new_v4().to_string());
            } else {
                let mut duplicate = archive.records[1].clone();
                duplicate.entry.id = new_id();
                archive.records.push(duplicate);
            }
            assert!(Engine::replay(archive.clone(), None, None).is_err());
            assert!(Checkpoint::capture(&engine)
                .encode("test-save", 1)
                .restore(archive)
                .is_err());
        }
    }

    #[test]
    fn stale_target_failure_is_journaled_without_effects_and_replays_exactly() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let accepted = admit(
            &mut engine,
            Action::Move {
                direction: Direction::East,
            },
        );
        engine
            .command(
                "p",
                "test",
                ActorId(1),
                "block",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: 0,
                    operation: WizardOperation::SetWall {
                        position: Position {
                            region: 1,
                            x: 2,
                            y: 1,
                            z: 0,
                        },
                        wall: true,
                    },
                },
            )
            .unwrap();
        let before = engine.state(ActorId(1)).unwrap();
        let failed = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(&failed.entry.content, JournalContent::IntentionFailed { admission, .. }
            if admission == &accepted.entry.id)
        );
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        assert!(engine.game.pending_intention(SimActor(1)).is_none());
        assert!(failed.entry.disclosed().is_none());
        assert!(engine.execute_next_intention().unwrap().is_none());
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            engine.game
        );
        assert_eq!(
            Checkpoint::capture(&engine)
                .encode("test-save", 1)
                .restore(engine.archive.clone())
                .unwrap()
                .game,
            engine.game
        );
    }
}
