//! Shared authoritative oracle: public observations alone cannot certify queues,
//! receipts, journal or rewind boundaries. Keep this private to engine tests.
use super::*;

fn assert_authority_equal(actual: &Engine, expected: &Engine, context: &str) {
    assert_eq!(
        actual.game, expected.game,
        "{context}: authoritative game/queue"
    );
    assert_eq!(actual.revisions, expected.revisions, "{context}: revisions");
    assert_eq!(
        actual.receipts, expected.receipts,
        "{context}: receipt index"
    );
    assert_eq!(
        actual.current_branch, expected.current_branch,
        "{context}: branch"
    );
    assert_eq!(
        serde_json::to_value(&actual.archive).unwrap(),
        serde_json::to_value(&expected.archive).unwrap(),
        "{context}: archive/receipts"
    );
    assert_eq!(
        actual.boundaries.len(),
        expected.boundaries.len(),
        "{context}: boundary count"
    );
    for (a, b) in actual.boundaries.iter().zip(&expected.boundaries) {
        assert_eq!(
            (&a.id, a.selectable, &a.game, &a.revisions),
            (&b.id, b.selectable, &b.game, &b.revisions),
            "{context}: rewind boundary"
        );
    }
    for actor in actual.actors() {
        assert_eq!(
            actual.state(actor).unwrap(),
            expected.state(actor).unwrap(),
            "{context}: disclosure"
        );
        assert_eq!(
            actual.history(actor, "player", None, 100).unwrap(),
            expected.history(actor, "player", None, 100).unwrap(),
            "{context}: history index"
        );
        assert_eq!(
            actual.pending_intentions(actor),
            expected.pending_intentions(actor),
            "{context}: intention index"
        );
    }
}

fn restored_both_ways(engine: &Engine, context: &str) {
    let journal = Engine::replay(engine.archive.clone(), None, None).unwrap();
    assert_authority_equal(&journal, engine, &format!("{context}: journal"));
    let snapshot =
        Checkpoint::capture(engine).encode("test-save", engine.archive.records.len() as u64);
    let checkpoint = snapshot.restore(engine.archive.clone()).unwrap();
    assert_authority_equal(&checkpoint, engine, &format!("{context}: checkpoint"));
}

fn wire_wait(engine: &Engine, actor: ActorId) -> Command {
    engine
        .resolve_command(
            actor,
            engine.branch(),
            crate::wire_adapter::decode_command(&tor_protocol::Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: tor_protocol::Action::Wait,
            })
            .unwrap(),
        )
        .unwrap()
}

#[test]
fn every_package_rejects_atomically_and_restores_queued_wire_admission() {
    for path in crate::test_packages::all_packages() {
        let name = path.display().to_string();
        let scenario = crate::scenario_package::load(&path, 7, None, false).unwrap();
        let actor = ActorId(scenario.package.as_ref().unwrap().selected);
        let mut engine = Engine::memory(scenario).unwrap();
        let before = engine.diagnostic_copy();
        let branch = engine.branch().clone();
        let stale = Command::AdmitIntention {
            expected_revision: engine.revision(actor).unwrap() + 1,
            action: Action::Wait,
        };
        for _ in 0..2 {
            let error = engine
                .command("player", "test", actor, "admission", &branch, stale.clone())
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::StaleRevision, "{name}");
            assert_authority_equal(&engine, &before, &name);
        }
        // Reuse the rejected request ID: a failed admission must not consume it.
        let command = wire_wait(&engine, actor);
        assert!(matches!(command, Command::AdmitIntention { .. }));
        let original = engine
            .command(
                "player",
                "test",
                actor,
                "admission",
                &branch,
                command.clone(),
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!original.duplicate);
        assert_eq!(
            engine.state(actor).unwrap(),
            before.state(actor).unwrap(),
            "{name}: admission executed early"
        );
        assert_eq!(
            engine.pending_intentions(actor).len(),
            1,
            "{name}: missing queued admission"
        );
        let admitted = engine.diagnostic_copy();
        let retry = engine
            .command(
                "player",
                "other-client",
                actor,
                "admission",
                &branch,
                command.clone(),
            )
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, original.entry, "{name}");
        assert_authority_equal(&engine, &admitted, &name);
        restored_both_ways(&engine, &name);
        let mut reopened = Engine::replay(engine.archive.clone(), None, None).unwrap();
        let retry = reopened
            .command("player", "reopened", actor, "admission", &branch, command)
            .unwrap();
        assert!(retry.duplicate);
        assert_eq!(
            retry.entry, original.entry,
            "{name}: queued receipt after reopen"
        );
        let cancel = Command::CancelIntention {
            expected_revision: reopened.revision(actor).unwrap(),
            admission: original.entry.id,
        };
        reopened
            .command("player", "test", actor, "cancel", &branch, cancel)
            .unwrap();
        assert!(
            reopened.pending_intentions(actor).is_empty(),
            "{name}: cancellation failed"
        );
        restored_both_ways(&reopened, &name);
        let command = wire_wait(&reopened, actor);
        let executed_admission = reopened
            .command("player", "test", actor, "execute", &branch, command)
            .unwrap();
        let execution = reopened
            .execute_next_intention()
            .unwrap()
            .unwrap_or_else(|| panic!("{name}: initial wire wait was never selected"));
        assert!(
            matches!(execution.entry.content, JournalContent::IntentionStarted { ref admission, .. } if admission == &executed_admission.entry.id),
            "{name}: wrong execution receipt"
        );
        assert!(
            reopened.pending_intentions(actor).is_empty(),
            "{name}: execution did not drain queue"
        );
        restored_both_ways(&reopened, &name);
    }
}

#[test]
fn seeded_queue_control_retry_and_rewind_traces_restore_at_every_boundary() {
    // Short prefixes are reported with the seed/round; fixed seeds keep failures
    // reproducible without a randomized-test dependency or unbounded search.
    for seed in [7_u64, 41, 0x5eed] {
        let mut random = seed;
        let mut engine = Engine::memory(Scenario::two_room(seed)).unwrap();
        engine.enable_wizard().unwrap();
        let actor = ActorId(1);
        for round in 0..16 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let context = format!("seed {seed}, prefix through round {round}");
            let branch = engine.branch().clone();
            let request = format!("admit-{round}");
            let command = wire_wait(&engine, actor);
            let original = engine
                .command("player", "test", actor, &request, &branch, command.clone())
                .unwrap();
            assert_eq!(engine.pending_intentions(actor).len(), 1, "{context}");
            restored_both_ways(&engine, &context);
            if random & 1 == 0 {
                engine.suspend_queued_intention(actor).unwrap().unwrap();
                restored_both_ways(&engine, &context);
                engine
                    .command(
                        "player",
                        "test",
                        actor,
                        &format!("resume-{round}"),
                        &branch,
                        Command::ResumeIntention {
                            expected_revision: engine.revision(actor).unwrap(),
                            admission: original.entry.id.clone(),
                        },
                    )
                    .unwrap();
                restored_both_ways(&engine, &context);
            }
            if random & 2 == 0 {
                engine
                    .command(
                        "player",
                        "test",
                        actor,
                        &format!("cancel-{round}"),
                        &branch,
                        Command::CancelIntention {
                            expected_revision: engine.revision(actor).unwrap(),
                            admission: original.entry.id.clone(),
                        },
                    )
                    .unwrap();
            } else {
                assert!(
                    engine.execute_next_intention().unwrap().is_some(),
                    "{context}"
                );
            }
            assert!(
                engine.pending_intentions(actor).is_empty(),
                "{context}: queue must drain"
            );
            let before_retry = engine.diagnostic_copy();
            for _ in 0..2 {
                let result = engine
                    .command(
                        "player",
                        "reconnected",
                        actor,
                        &request,
                        &branch,
                        command.clone(),
                    )
                    .unwrap();
                assert!(result.duplicate, "{context}");
                assert_eq!(result.entry, original.entry, "{context}");
                assert_authority_equal(&engine, &before_retry, &context);
            }
            restored_both_ways(&engine, &context);
            if round % 4 == 3 {
                engine
                    .command(
                        "player",
                        "test",
                        actor,
                        &format!("rewind-{round}"),
                        &branch,
                        Command::Wizard {
                            expected_revision: engine.revision(actor).unwrap(),
                            operation: WizardOperation::Rewind { target: None },
                        },
                    )
                    .unwrap();
                assert!(engine.pending_intentions(actor).is_empty(), "{context}");
                restored_both_ways(&engine, &context);
            }
        }
    }
}
