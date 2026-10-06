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
    fn rejected_persistence_preserves_preparation_queue_revisions_and_receipts() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let scenario = crate::scenario_package::load(&root, 42, None, false).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let actor = ActorId(1);
        for stage in [
            "suspend",
            "resume",
            "cancel_active",
            "cancel_suspended",
            "continue",
            "fail_continue",
        ] {
            let path = directory.path().join(format!("{stage}.db"));
            let mut engine = Engine::open(&path, scenario.clone()).unwrap();
            let admitted = engine
                .command(
                    "p",
                    "test",
                    actor,
                    "attack",
                    &engine.branch().clone(),
                    Command::AdmitIntention {
                        expected_revision: engine.revision(actor).unwrap(),
                        action: Action::Attack { target: ActorId(2) },
                    },
                )
                .unwrap();
            engine.execute_next_intention().unwrap().unwrap();
            if !matches!(stage, "suspend" | "cancel_active") {
                engine.pause_preparation(actor).unwrap().unwrap();
            }
            if stage == "fail_continue" {
                engine.enable_wizard().unwrap();
                engine
                    .command(
                        "p",
                        "test",
                        actor,
                        "move-target",
                        &engine.branch().clone(),
                        Command::Wizard {
                            expected_revision: engine.revision(actor).unwrap(),
                            operation: WizardOperation::Teleport {
                                actor: ActorId(2),
                                position: Position {
                                    region: 1,
                                    x: 3,
                                    y: 1,
                                    z: 0,
                                },
                            },
                        },
                    )
                    .unwrap();
            }
            if matches!(stage, "continue" | "fail_continue") {
                engine
                    .command(
                        "p",
                        "test",
                        actor,
                        "resume",
                        &engine.branch().clone(),
                        Command::ResumeIntention {
                            expected_revision: engine.revision(actor).unwrap(),
                            admission: admitted.entry.id.clone(),
                        },
                    )
                    .unwrap();
            }
            engine.flush().unwrap();
            drop(engine);
            let mut engine = Engine::open_with_policy(
                &path,
                scenario.clone(),
                crate::SavePolicy {
                    max_pending_bytes: 1,
                    ..Default::default()
                },
            )
            .unwrap();
            let before = engine.game.clone();
            let revisions = engine.revisions.clone();
            let boundaries = engine.boundaries.clone();
            let receipt = engine.request_receipt(&admitted);
            let record_count = engine.archive.records.len();
            let receipt_count = engine.receipts.len();
            let error = match stage {
                "suspend" => engine.pause_preparation(actor).unwrap_err(),
                "continue" | "fail_continue" => engine.execute_next_intention().unwrap_err(),
                _ => engine
                    .command(
                        "p",
                        "test",
                        actor,
                        "rejected",
                        &engine.branch().clone(),
                        if stage == "resume" {
                            Command::ResumeIntention {
                                expected_revision: engine.revision(actor).unwrap(),
                                admission: admitted.entry.id.clone(),
                            }
                        } else {
                            Command::CancelIntention {
                                expected_revision: engine.revision(actor).unwrap(),
                                admission: admitted.entry.id.clone(),
                            }
                        },
                    )
                    .unwrap_err(),
            };
            assert_eq!(error.code, ErrorCode::StorageFailure, "{stage}");
            assert_eq!(engine.game, before, "{stage}");
            assert_eq!(engine.revisions, revisions, "{stage}");
            assert_eq!(engine.archive.records.len(), record_count, "{stage}");
            assert_eq!(engine.receipts.len(), receipt_count, "{stage}");
            assert_eq!(engine.request_receipt(&admitted), receipt, "{stage}");
            assert_eq!(engine.boundaries.len(), boundaries.len());
            assert!(engine
                .boundaries
                .iter()
                .zip(&boundaries)
                .all(|(after, before)| Arc::ptr_eq(after, before)));
        }
    }

    #[test]
    fn paused_attack_recovery_preserves_original_admission_through_continuation_and_cancel() {
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
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let verify = |engine: &Engine| {
            let replay = Engine::replay(engine.archive.clone(), None, None).unwrap();
            let restored = Checkpoint::capture(engine)
                .encode("paused-attack", 1)
                .restore(engine.archive.clone())
                .unwrap_or_else(|error| {
                    panic!(
                        "checkpoint after {:?}: {error:?}",
                        engine.archive.records.last().unwrap().entry.content
                    )
                });
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
        let active_state = engine.state(actor).unwrap();
        engine.pause_preparation(actor).unwrap().unwrap();
        let suspended_state = engine.state(actor).unwrap();
        assert!(
            !suspended_state
                .observation
                .combat
                .as_ref()
                .unwrap()
                .preparation_active
        );
        assert!(
            suspended_state.revision > active_state.revision,
            "changed preparation must advance its disclosed observation revision"
        );
        let pending = engine.pending_intentions(actor);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].entry_id, admitted.entry.id);
        assert_eq!(pending[0].phase, IntentionPhase::Paused);
        assert!(
            engine.archive.records.last().unwrap().receipt.is_none(),
            "scheduler suspension must not fabricate a client request"
        );
        assert!(matches!(
            engine.request_receipt(&admitted),
            RequestReceipt::Admitted {
                phase: IntentionPhase::Paused,
                ..
            }
        ));
        verify(&engine);
        let before = engine.game.clone();
        let resume = Command::ResumeIntention {
            expected_revision: engine.revision(actor).unwrap(),
            admission: admitted.entry.id.clone(),
        };
        let resumed = engine
            .command(
                "p",
                "test",
                actor,
                "resume-started",
                &engine.branch().clone(),
                resume.clone(),
            )
            .unwrap();
        assert_eq!(engine.game.tick(), before.tick());
        assert_eq!(
            engine.game.preparation(SimActor(actor.0)),
            before.preparation(SimActor(actor.0))
        );
        let pending = engine.pending_intentions(actor);
        assert_eq!(
            pending.len(),
            1,
            "queued continuation and preparation share one identity"
        );
        assert_eq!(pending[0].entry_id, admitted.entry.id);
        assert_eq!(pending[0].phase, IntentionPhase::Queued);
        verify(&engine);
        let continued = engine.execute_next_intention().unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&continued.entry.content).unwrap()["type"],
            serde_json::json!("intention_continued")
        );
        assert!(engine.archive.records.last().unwrap().receipt.is_none());
        assert_eq!(
            engine.pending_intentions(actor)[0].phase,
            IntentionPhase::Started
        );
        verify(&engine);
        let before_retry = engine.game.clone();
        let retry = engine
            .command(
                "p",
                "test",
                actor,
                "resume-started",
                &engine.branch().clone(),
                resume,
            )
            .unwrap();
        assert_eq!(retry.entry, resumed.entry);
        assert_eq!(engine.game, before_retry);
        let before_cancel = engine.state(actor).unwrap();
        engine
            .command(
                "p",
                "test",
                actor,
                "cancel-started",
                &engine.branch().clone(),
                Command::CancelIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    admission: admitted.entry.id.clone(),
                },
            )
            .unwrap();
        let after_cancel = engine.state(actor).unwrap();
        assert!(after_cancel
            .observation
            .combat
            .as_ref()
            .unwrap()
            .preparation_remaining
            .is_none());
        assert!(after_cancel.revision > before_cancel.revision);
        assert!(engine.pending_intentions(actor).is_empty());
        assert!(engine.game.preparation(SimActor(actor.0)).is_none());
        assert!(matches!(
            engine.request_receipt(&admitted),
            RequestReceipt::Admitted {
                phase: IntentionPhase::Cancelled,
                ..
            }
        ));
        verify(&engine);
        assert_eq!(
            engine
                .archive
                .records
                .iter()
                .filter(|record| matches!(
                    record.entry.content,
                    JournalContent::IntentionAdmitted { .. }
                ))
                .count(),
            1
        );
    }

    #[test]
    fn cancelling_original_preparation_preserves_independent_admission_through_recovery() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&source, 42, None, false).unwrap())
                .unwrap();
        let actor = ActorId(1);
        let attack = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        engine.pause_preparation(actor).unwrap().unwrap();
        let later = engine
            .command(
                "p",
                "test",
                actor,
                "later",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
        let queued = engine.game.pending_intention(SimActor(1)).unwrap().clone();
        let tick = engine.game.tick();
        let pending = engine.pending_intentions(actor);
        assert_eq!(pending.len(), 2);
        assert!(pending
            .iter()
            .any(|status| status.entry_id == attack.entry.id
                && status.phase == IntentionPhase::Paused));
        assert!(pending
            .iter()
            .any(|status| status.entry_id == later.entry.id
                && status.phase == IntentionPhase::Queued));
        let replay = Engine::replay(engine.archive.clone(), None, None).unwrap();
        let checkpoint = Checkpoint::capture(&engine)
            .encode("independent", 1)
            .restore(engine.archive.clone())
            .unwrap();
        for mut recovered in [engine, replay, checkpoint] {
            let before = recovered.game.clone();
            let request = Command::CancelIntention {
                expected_revision: recovered.revision(actor).unwrap(),
                admission: attack.entry.id.clone(),
            };
            let cancelled = recovered
                .command(
                    "p",
                    "test",
                    actor,
                    "cancel-original",
                    &recovered.branch().clone(),
                    request.clone(),
                )
                .unwrap();
            assert_eq!(recovered.game.tick(), tick);
            assert!(before.preparation(SimActor(1)).is_some());
            assert!(recovered.game.preparation(SimActor(1)).is_none());
            assert_eq!(recovered.game.pending_intention(SimActor(1)), Some(&queued));
            assert_eq!(recovered.pending_intentions(actor).len(), 1);
            assert_eq!(
                recovered.pending_intentions(actor)[0].entry_id,
                later.entry.id
            );
            assert!(matches!(
                recovered.request_receipt(&attack),
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Cancelled,
                    ..
                }
            ));
            assert!(matches!(
                recovered.request_receipt(&later),
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Queued,
                    ..
                }
            ));
            let after = recovered.game.clone();
            let retry = recovered
                .command(
                    "p",
                    "test",
                    actor,
                    "cancel-original",
                    &recovered.branch().clone(),
                    request,
                )
                .unwrap();
            assert_eq!(retry.entry, cancelled.entry);
            assert_eq!(recovered.game, after);
            let replay = Engine::replay(recovered.archive.clone(), None, None).unwrap();
            let checkpoint = Checkpoint::capture(&recovered)
                .encode("independent", 2)
                .restore(recovered.archive.clone())
                .unwrap();
            assert_eq!(replay.game, recovered.game);
            assert_eq!(checkpoint.game, recovered.game);
            let executed = recovered.execute_next_intention().unwrap().unwrap();
            assert!(
                matches!(executed.entry.content, JournalContent::IntentionStarted {
                ref admission, intention, action: Action::Wait, ..
            } if admission == &later.entry.id && intention == queued.id)
            );
        }
    }

    #[test]
    fn preparation_phase_lineage_survives_rewind_and_both_recovery_paths() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&source, 42, None, false).unwrap())
                .unwrap();
        let actor = ActorId(1);
        engine.enable_wizard().unwrap();
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let mut targets = Vec::new();
        for (index, expected) in [
            IntentionPhase::Started,
            IntentionPhase::Paused,
            IntentionPhase::Queued,
            IntentionPhase::Started,
            IntentionPhase::Cancelled,
        ]
        .into_iter()
        .enumerate()
        {
            match index {
                1 => {
                    engine.pause_preparation(actor).unwrap().unwrap();
                }
                2 => {
                    engine
                        .command(
                            "p",
                            "test",
                            actor,
                            "resume",
                            &engine.branch().clone(),
                            Command::ResumeIntention {
                                expected_revision: engine.revision(actor).unwrap(),
                                admission: admitted.entry.id.clone(),
                            },
                        )
                        .unwrap();
                }
                3 => {
                    engine.execute_next_intention().unwrap().unwrap();
                }
                4 => {
                    engine
                        .command(
                            "p",
                            "test",
                            actor,
                            "cancel",
                            &engine.branch().clone(),
                            Command::CancelIntention {
                                expected_revision: engine.revision(actor).unwrap(),
                                admission: admitted.entry.id.clone(),
                            },
                        )
                        .unwrap();
                }
                _ => {}
            }
            let retained = engine
                .command(
                    "p",
                    "test",
                    actor,
                    &format!("phase-{index}"),
                    &engine.branch().clone(),
                    Command::Wizard {
                        expected_revision: engine.revision(actor).unwrap(),
                        operation: WizardOperation::SetPlaceHint {
                            position: Position {
                                region: 1,
                                x: 1,
                                y: 1,
                                z: 0,
                            },
                            present: true,
                        },
                    },
                )
                .unwrap();
            targets.push((retained.entry.id, engine.game.clone(), expected));
        }
        for (index, (target, expected_game, expected_phase)) in targets.into_iter().enumerate() {
            let mut fork = Engine::replay(engine.archive.clone(), None, None).unwrap();
            fork.enable_wizard().unwrap();
            fork.command(
                "p",
                "test",
                actor,
                &format!("rewind-{index}"),
                &fork.branch().clone(),
                Command::Wizard {
                    expected_revision: fork.revision(actor).unwrap(),
                    operation: WizardOperation::Rewind {
                        target: Some(target),
                    },
                },
            )
            .unwrap();
            assert_eq!(fork.game, expected_game, "phase {expected_phase:?}");
            let replay = Engine::replay(fork.archive.clone(), None, None).unwrap();
            let checkpoint = Checkpoint::capture(&fork)
                .encode("phase-lineage", 1)
                .restore(fork.archive.clone())
                .unwrap();
            for recovered in [&fork, &replay, &checkpoint] {
                assert_eq!(recovered.game, expected_game);
                assert!(
                    matches!(recovered.request_receipt(&admitted),
                    RequestReceipt::Admitted { phase, .. } if phase == IntentionPhase::Cancelled),
                    "the original branch receipt retains its terminal phase"
                );
                if expected_phase.active() {
                    let pending = recovered.pending_intentions(actor);
                    assert_eq!(pending.len(), 1);
                    assert_eq!(pending[0].entry_id, admitted.entry.id);
                    assert_eq!(pending[0].branch, *recovered.branch());
                    assert_eq!(pending[0].phase, expected_phase);
                } else {
                    assert!(recovered.pending_intentions(actor).is_empty());
                }
            }
        }
    }

    #[test]
    fn lifecycle_requires_the_suspended_continuation_queue_present_in_saved_state() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&source, 42, None, false).unwrap())
                .unwrap();
        let actor = ActorId(1);
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        engine.pause_preparation(actor).unwrap().unwrap();
        engine
            .command(
                "p",
                "test",
                actor,
                "resume",
                &engine.branch().clone(),
                Command::ResumeIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    admission: admitted.entry.id.clone(),
                },
            )
            .unwrap();
        engine.suspend_queued_intention(actor).unwrap().unwrap();
        let boundary = Some(engine.archive.records.last().unwrap().entry.id.clone());
        let mut lifecycle =
            super::super::intention_lifecycle::JournalLifecycle::new(engine.archive.branch.clone());
        for record in &engine.archive.records {
            lifecycle.observe(record).unwrap();
        }
        assert!(lifecycle.valid_game(&engine.game, &boundary));
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let mut saved = serde_json::to_value(engine.game.checkpoint(&mut shared)).unwrap();
        saved["intentions"]["entries"]
            .as_array_mut()
            .unwrap()
            .clear();
        let missing_queue =
            Game::restore_checkpoint(serde_json::from_value(saved).unwrap(), &shared).unwrap();
        assert!(missing_queue.pending_intention(SimActor(actor.0)).is_none());
        assert_eq!(
            missing_queue.preparation(SimActor(actor.0)),
            engine.game.preparation(SimActor(actor.0))
        );
        assert!(
            !lifecycle.valid_game(&missing_queue, &boundary),
            "paused preparation alone cannot replace a journaled suspended continuation queue"
        );
    }

    #[test]
    fn lifecycle_rejects_a_second_queued_admission_for_the_same_actor() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let command = Command::AdmitIntention {
            expected_revision: engine.revision(actor).unwrap(),
            action: Action::Wait,
        };
        engine
            .command(
                "p",
                "test",
                actor,
                "first",
                &engine.branch().clone(),
                command.clone(),
            )
            .unwrap();
        assert!(engine
            .command(
                "p",
                "test",
                actor,
                "second",
                &engine.branch().clone(),
                command
            )
            .is_err());
        let first = engine.archive.records.last().unwrap();
        let mut forged = first.clone();
        forged.entry.id = EntryId(Uuid::new_v4().to_string());
        let JournalContent::IntentionAdmitted { intention, .. } = &mut forged.entry.content else {
            unreachable!()
        };
        intention.0 += 1;
        forged.receipt.as_mut().unwrap().request_id = "second".into();
        let mut lifecycle =
            super::super::intention_lifecycle::JournalLifecycle::new(engine.archive.branch.clone());
        for record in &engine.archive.records {
            lifecycle.observe(record).unwrap();
        }
        assert!(
            lifecycle.observe(&forged).is_err(),
            "journal ownership must enforce the simulation's one-queue-per-actor rule"
        );
    }

    #[test]
    fn lifecycle_rejects_suspension_and_terminal_facts_for_same_work() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&source, 42, None, false).unwrap())
                .unwrap();
        let actor = ActorId(1);
        engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let mut records = engine.archive.records.clone();
        let record = records.last_mut().unwrap();
        let JournalContent::IntentionStarted { intention, .. } = record.entry.content else {
            unreachable!()
        };
        record
            .entry
            .intention_suspensions
            .push(crate::journal::IntentionSuspension { actor, intention });
        for kind in [
            crate::journal::IntentionEndKind::Resolved,
            crate::journal::IntentionEndKind::Failed,
            crate::journal::IntentionEndKind::Cancelled,
        ] {
            let mut conflicting = record.clone();
            conflicting
                .entry
                .intention_ends
                .push(crate::journal::IntentionEnd {
                    actor,
                    intention,
                    kind,
                });
            let mut lifecycle = super::super::intention_lifecycle::JournalLifecycle::new(
                engine.archive.branch.clone(),
            );
            for previous in &engine.archive.records[..engine.archive.records.len() - 1] {
                lifecycle.observe(previous).unwrap();
            }
            assert!(
                lifecycle.observe(&conflicting).is_err(),
                "one boundary cannot suspend and terminate the same work: {kind:?}"
            );
        }
    }

    #[test]
    fn checkpoint_rejects_terminal_effects_on_expired_pause_metadata() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let mut engine =
            Engine::memory(crate::scenario_package::load(&source, 42, None, false).unwrap())
                .unwrap();
        let actor = ActorId(1);
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        let JournalContent::IntentionAdmitted { intention, .. } = admitted.entry.content else {
            unreachable!()
        };
        engine.execute_next_intention().unwrap().unwrap();
        let paused = engine.pause_preparation(actor).unwrap().unwrap();
        let cancelled = engine
            .command(
                "p",
                "test",
                actor,
                "cancel",
                &engine.branch().clone(),
                Command::CancelIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    admission: admitted.entry.id.clone(),
                },
            )
            .unwrap();
        engine.enable_wizard().unwrap();
        for index in 0..140 {
            engine
                .command(
                    "p",
                    "test",
                    actor,
                    &format!("retain-{index}"),
                    &engine.branch().clone(),
                    Command::Wizard {
                        expected_revision: engine.revision(actor).unwrap(),
                        operation: WizardOperation::Teleport {
                            actor,
                            position: Position {
                                region: 1,
                                x: 1,
                                y: 1,
                                z: 0,
                            },
                        },
                    },
                )
                .unwrap();
        }
        for id in [&paused.entry.id, &cancelled.entry.id] {
            assert!(!engine
                .boundaries
                .iter()
                .any(|boundary| boundary.id.as_ref() == Some(id)));
        }
        Checkpoint::capture(&engine)
            .encode("effect-owner", 1)
            .restore(engine.archive.clone())
            .unwrap();
        let mut archive = engine.archive.clone();
        archive
            .records
            .retain(|record| record.entry.id != cancelled.entry.id);
        let pause = archive
            .records
            .iter_mut()
            .find(|record| record.entry.id == paused.entry.id)
            .unwrap();
        pause
            .entry
            .intention_ends
            .push(crate::journal::IntentionEnd {
                actor,
                intention,
                kind: crate::journal::IntentionEndKind::Cancelled,
            });
        assert!(Engine::replay(archive.clone(), None, None).is_err());
        let mut checkpoint = Checkpoint::capture(&engine).encode("effect-owner", 1);
        checkpoint.record_count = archive.records.len();
        assert!(
            checkpoint.restore(archive).is_err(),
            "pause metadata cannot replace an authoritative cancellation record"
        );
    }

    /// A validated authored fixture where the guard interrupts spent player work.
    /// Keep its directory alive while a saved scenario may refer to package inputs.
    fn interruption_scenario() -> (tempfile::TempDir, Scenario) {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/dungeon-loop");
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("regions")).unwrap();
        let mut manifest: toml::Value =
            toml::from_str(&std::fs::read_to_string(source.join("scenario.toml")).unwrap())
                .unwrap();
        manifest["characters"][0]["combat"]["attack"]["wind_up"] = 60.into();
        let mut region: toml::Value =
            toml::from_str(&std::fs::read_to_string(source.join("regions/1.toml")).unwrap())
                .unwrap();
        region["actors"][0]["combat"]["attack"]["wind_up"] = 30.into();
        region["actors"][0]["combat"]["max_hp"] = 100.into();
        std::fs::write(
            directory.path().join("scenario.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("regions/1.toml"),
            toml::to_string(&region).unwrap(),
        )
        .unwrap();
        let scenario = crate::scenario_package::load(directory.path(), 42, None, true).unwrap();
        (directory, scenario)
    }

    #[test]
    fn combat_interruption_rejected_by_storage_preserves_authoritative_state() {
        let (directory, scenario) = interruption_scenario();
        let path = directory.path().join("interruption.db");
        let actor = ActorId(1);
        let mut engine = Engine::open(&path, scenario.clone()).unwrap();
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(
            &path,
            scenario.clone(),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
        // Establish that the rejected operation would actually interrupt this work.
        let mut healthy = Engine::replay(engine.archive.clone(), None, None).unwrap();
        healthy.advance_ai(ActorId(2)).unwrap();
        assert!(!healthy.game.preparation(SimActor(actor.0)).unwrap().active);
        assert_eq!(
            healthy
                .archive
                .records
                .last()
                .unwrap()
                .entry
                .intention_suspensions
                .len(),
            1
        );
        assert!(engine.game.preparation(SimActor(actor.0)).unwrap().active);

        let before = engine.game.clone();
        let revisions = engine.revisions.clone();
        let boundaries = engine.boundaries.clone();
        let records = engine.archive.records.len();
        let receipts = engine.receipts.clone();
        let receipt = engine.request_receipt(&admitted);
        let views = [actor, ActorId(2)].map(|observer| engine.revision_view(observer).unwrap());
        assert_eq!(
            engine.advance_ai(ActorId(2)).unwrap_err().code,
            ErrorCode::StorageFailure
        );
        assert_eq!(engine.game, before);
        assert_eq!(engine.revisions, revisions);
        assert_eq!(engine.archive.records.len(), records);
        assert_eq!(engine.receipts, receipts);
        assert_eq!(engine.request_receipt(&admitted), receipt);
        assert_eq!(engine.boundaries.len(), boundaries.len());
        assert!(engine
            .boundaries
            .iter()
            .zip(&boundaries)
            .all(|(after, before)| Arc::ptr_eq(after, before)));
        for (observer, view) in [actor, ActorId(2)].into_iter().zip(views) {
            assert!(Arc::ptr_eq(&engine.revision_view(observer).unwrap(), &view));
        }
        engine.flush().unwrap();
        drop(engine);
        let restored = Engine::open(&path, scenario).unwrap();
        assert_eq!(restored.game, before);
        assert_eq!(restored.request_receipt(&admitted), receipt);
        assert_eq!(restored.archive.records.len(), records);
    }

    #[test]
    fn combat_interruption_publishes_original_progress_suspension_and_can_resume() {
        let (_package, scenario) = interruption_scenario();
        let mut engine = Engine::memory(scenario).unwrap();
        let actor = ActorId(1);
        let admitted = engine
            .command(
                "p",
                "test",
                actor,
                "attack",
                &engine.branch().clone(),
                Command::AdmitIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        let interrupted = engine.advance_ai(ActorId(2)).unwrap();
        let preparation = engine.game.preparation(SimActor(1)).unwrap();
        assert!(!preparation.active);
        assert!(preparation.remaining < 60 && preparation.remaining > 0);
        assert_eq!(
            engine.pending_intentions(actor)[0].phase,
            IntentionPhase::Paused
        );
        let facts = &engine
            .archive
            .records
            .last()
            .unwrap()
            .entry
            .intention_suspensions;
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].actor, actor);
        for corruption in ["missing", "actor", "identity", "duplicate"] {
            let mut archive = engine.archive.clone();
            let facts = &mut archive
                .records
                .last_mut()
                .unwrap()
                .entry
                .intention_suspensions;
            match corruption {
                "missing" => facts.clear(),
                "actor" => facts[0].actor = ActorId(2),
                "identity" => facts[0].intention = tor_simulation::IntentionId(99),
                "duplicate" => facts.push(facts[0].clone()),
                _ => unreachable!(),
            }
            assert!(
                Engine::replay(archive.clone(), None, None).is_err(),
                "{corruption}"
            );
            assert!(
                Checkpoint::capture(&engine)
                    .encode("interrupted", 1)
                    .restore(archive)
                    .is_err(),
                "checkpoint accepted {corruption}"
            );
        }
        assert!(
            engine
                .intention_updates(&interrupted)
                .iter()
                .any(|status| status.entry_id == admitted.entry.id
                    && status.phase == IntentionPhase::Paused),
            "the action causing interruption must publish its derived suspension"
        );
        engine
            .command(
                "p",
                "test",
                actor,
                "resume",
                &engine.branch().clone(),
                Command::ResumeIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    admission: admitted.entry.id.clone(),
                },
            )
            .unwrap();
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        let restored = Checkpoint::capture(&engine)
            .encode("interrupted", 1)
            .restore(engine.archive.clone())
            .unwrap();
        assert_eq!(replayed.game, engine.game);
        assert_eq!(restored.game, engine.game);
    }

    #[test]
    fn checkpoint_rejects_missing_suspension_after_original_boundaries_expire() {
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
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Attack { target: ActorId(2) },
                },
            )
            .unwrap();
        let started = engine.execute_next_intention().unwrap().unwrap();
        let suspended = engine.pause_preparation(actor).unwrap().unwrap();
        engine
            .command(
                "p",
                "test",
                actor,
                "resume",
                &engine.branch().clone(),
                Command::ResumeIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    admission: admitted.entry.id.clone(),
                },
            )
            .unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        engine
            .command(
                "p",
                "test",
                actor,
                "cancel",
                &engine.branch().clone(),
                Command::CancelIntention {
                    expected_revision: engine.revision(actor).unwrap(),
                    admission: admitted.entry.id.clone(),
                },
            )
            .unwrap();
        engine.enable_wizard().unwrap();
        for index in 0..140 {
            engine
                .command(
                    "p",
                    "test",
                    actor,
                    &format!("retain-{index}"),
                    &engine.branch().clone(),
                    Command::Wizard {
                        expected_revision: engine.revision(actor).unwrap(),
                        operation: WizardOperation::Teleport {
                            actor,
                            position: Position {
                                region: 1,
                                x: 1,
                                y: 1,
                                z: 0,
                            },
                        },
                    },
                )
                .unwrap();
        }
        for expired in [&started.entry.id, &suspended.entry.id] {
            assert!(
                !engine
                    .boundaries
                    .iter()
                    .any(|boundary| boundary.id.as_ref() == Some(expired)),
                "the corruption must precede both retained windows"
            );
        }
        let healthy = Checkpoint::capture(&engine)
            .encode("expired-progress", 1)
            .restore(engine.archive.clone())
            .unwrap();
        assert_eq!(healthy.game, engine.game);
        let mut archive = engine.archive.clone();
        archive
            .records
            .retain(|record| record.entry.id != suspended.entry.id);
        assert_eq!(archive.records.len() + 1, engine.archive.records.len());
        assert!(Engine::replay(archive.clone(), None, None).is_err());
        let mut checkpoint = Checkpoint::capture(&engine).encode("expired-progress", 1);
        // Match the altered count so rejection proves lifecycle validation,
        // rather than merely detecting a stale checkpoint envelope.
        checkpoint.record_count = archive.records.len();
        assert!(
            checkpoint.restore(archive).is_err(),
            "checkpoint must reject resume without suspension in expired history"
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
        let snapshot = serde_json::to_value(engine.game.checkpoint(&mut shared)).unwrap();
        let mut encoded_shared = serde_json::to_value(shared).unwrap();
        let actors = snapshot["actors"].as_u64().unwrap() as usize;
        encoded_shared["actors"][actors]["1"]["combat"]["hp"] = serde_json::json!(0);
        let shared = serde_json::from_value(encoded_shared).unwrap();
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
    fn checkpoint_proves_expired_terminal_effects_and_branch_lineage() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        admit(&mut engine, Action::Wait);
        let started = engine.execute_next_intention().unwrap().unwrap();
        for index in 0..140 {
            engine
                .command(
                    "p",
                    "test",
                    ActorId(1),
                    &format!("retain-{index}"),
                    &engine.branch().clone(),
                    Command::Wizard {
                        expected_revision: engine.revision(ActorId(1)).unwrap(),
                        operation: WizardOperation::Teleport {
                            actor: ActorId(1),
                            position: Position {
                                region: 1,
                                x: 1,
                                y: 1,
                                z: 0,
                            },
                        },
                    },
                )
                .unwrap();
        }
        assert!(!engine
            .boundaries
            .iter()
            .any(|boundary| boundary.id.as_ref() == Some(&started.entry.id)));
        Checkpoint::capture(&engine)
            .encode("expired-phases", 1)
            .restore(engine.archive.clone())
            .unwrap();
        for corruption in ["missing_terminal", "fabricated_branch"] {
            let mut archive = engine.archive.clone();
            match corruption {
                "missing_terminal" => archive.records[1].entry.intention_ends.clear(),
                "fabricated_branch" => {
                    archive.records[2].entry.branch = BranchId(uuid::Uuid::new_v4().to_string())
                }
                _ => unreachable!(),
            }
            assert!(
                Engine::replay(archive.clone(), None, None).is_err(),
                "{corruption}"
            );
            assert!(
                Checkpoint::capture(&engine)
                    .encode("expired-phases", 1)
                    .restore(archive)
                    .is_err(),
                "checkpoint accepted {corruption}"
            );
        }
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
