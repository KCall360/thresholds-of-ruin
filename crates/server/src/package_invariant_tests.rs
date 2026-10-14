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
            step: None,
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
                            step: None,
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

#[test]
fn replacement_with_health_work_survives_checkpoint_tail_journal_retry_and_rewind() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/interactions");
    let mut scenario = crate::scenario_package::load(&root, 42, None, false).unwrap();
    let package = scenario.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.characters[0].turn_ticks = 37;
    manifest.characters[0].creature.as_mut().unwrap().hit_dice[0]
        .purchases
        .push(
            serde_json::from_value(serde_json::json!({
                "pool": "advancement",
                "selection": {"kind": "talent", "talent": "fixture_recovery_health"}
            }))
            .unwrap(),
        );
    // Explicitly opt into a +2 Health and -25% recovery-period fixture. The
    // test package's carried poison supplies injury through normal consumption.
    let rules = format!(
        "{}\n[[overrides]]\nop = 'add'\npath = ['talents', 'fixture_recovery_health']\nvalue = {{ id = 'fixture_recovery_health', name = 'Fixture Recovery Health', prerequisites = [], requirement = {{ kind = 'total_hd', minimum = 1 }}, modifiers = [{{ scope = 'always', op = {{ kind = 'add', stat = {{ kind = 'health' }}, value = 2 }} }}, {{ scope = 'always', op = {{ kind = 'add', stat = {{ kind = 'health_recovery_period_percent' }}, value = -25 }} }}] }}\n",
        package.rules_source
    );
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &rules,
        )
        .unwrap(),
    ));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("health-replacement.db");
    let mut engine = Engine::open_with_policy(
        &path,
        scenario.clone(),
        crate::SavePolicy {
            checkpoint_interval: 8,
            ..crate::SavePolicy::default()
        },
    )
    .unwrap();
    let actor = ActorId(1);
    engine.enable_wizard().unwrap();
    let branch = engine.branch().clone();
    engine
        .command(
            "player",
            "test",
            actor,
            "drink-poison",
            &branch,
            Command::AdmitIntention {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Drink { item: 21 },
            },
        )
        .unwrap();
    for _ in 0..8 {
        if engine.pending_intentions(actor).is_empty() {
            break;
        }
        assert!(engine.execute_next_intention().unwrap().is_some());
    }
    assert!(engine.pending_intentions(actor).is_empty());
    assert_eq!(engine.inspect_creature(actor).unwrap().injury, 8);

    let wait = |engine: &mut Engine, request: &str| {
        engine
            .command(
                "player",
                "test",
                actor,
                request,
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap()
    };
    // The final ordinary wait is both a retained rewind boundary and a forced
    // checkpoint. Replacement itself must remain in the replay tail.
    let mut rewind_target = None;
    for step in 0..8 {
        rewind_target = Some(
            wait(&mut engine, &format!("checkpoint-wait-{step}"))
                .entry
                .id,
        );
        if engine.save_status().accepted_sequence.is_multiple_of(8) {
            break;
        }
    }
    engine.flush().unwrap();
    let checkpoint_sequence = engine.save_status().checkpoint_sequence;
    assert!(checkpoint_sequence > 0);
    assert_eq!(checkpoint_sequence, engine.save_status().accepted_sequence);
    assert!(matches!(
        engine.game.creature(SimActor(1)).unwrap(),
        std::borrow::Cow::Owned(_)
    ));
    let before = engine.inspect_creature(actor).unwrap();
    assert_eq!(before.injury, 8);
    let recovery = before.stats.health_recovery;
    assert_eq!(recovery.period_ticks, 900);
    assert!(recovery.elapsed_ticks > 0 && recovery.elapsed_ticks < 900);

    let replacement = |pool: &str| -> WizardOperation {
        serde_json::from_value(serde_json::json!({
            "type": "advance_creature", "actor": 1,
            "advancement": {"type": "replace_purchase", "owner": 1, "purchase": 1,
                "replacement": {"kind": "skill_rank", "pool": pool, "skill": "athletics"}}
        }))
        .unwrap()
    };
    {
        let unchanged = engine.diagnostic_copy();
        assert!(engine
            .command(
                "wizard",
                "test",
                actor,
                "bad-replacement",
                &branch,
                Command::Wizard {
                    expected_revision: engine.revision(actor).unwrap(),
                    operation: replacement("absent"),
                },
            )
            .is_err());
        assert_authority_equal(&engine, &unchanged, "recovering replacement rejection");
    }
    let command = Command::Wizard {
        expected_revision: engine.revision(actor).unwrap(),
        operation: replacement("advancement"),
    };
    let replaced = engine
        .command(
            "wizard",
            "test",
            actor,
            "replace-recovery-talent",
            &branch,
            command.clone(),
        )
        .unwrap();
    let after = engine.inspect_creature(actor).unwrap();
    assert_eq!(after.stats.health_recovery.period_ticks, 1200);
    assert_eq!(
        after.stats.health_recovery.elapsed_ticks,
        recovery.elapsed_ticks * 1200 / 900
    );
    assert_eq!(after.injury, before.injury);
    assert_eq!(after.stats.resources, before.stats.resources);
    assert_eq!(after.starting_skills, before.starting_skills);
    assert_eq!(
        after.hit_dice[0].health_seed,
        before.hit_dice[0].health_seed
    );
    assert_eq!(after.hit_dice[0].point_pools[0].spent, 1);
    assert!(!after
        .stats
        .active_talents
        .iter()
        .any(|talent| talent.id == "fixture_recovery_health"));
    assert_eq!(
        after.hit_dice[0].purchases[0].selection,
        tor_protocol::InspectionPurchaseSelection::SkillRank {
            skill: tor_protocol::ContentRef {
                id: "athletics".into(),
                name: "Athletics".into(),
            },
        }
    );
    wait(&mut engine, "recover-after-replacement");
    let tail = engine.inspect_creature(actor).unwrap();
    assert_eq!(tail.injury, 8);
    assert_eq!(
        tail.stats.health_recovery.elapsed_ticks,
        after.stats.health_recovery.elapsed_ticks + 37
    );
    engine.flush().unwrap();
    let sequence = engine.save_status().accepted_sequence;
    assert_eq!(sequence, checkpoint_sequence + 2);
    assert_eq!(
        engine.save_status().checkpoint_sequence,
        checkpoint_sequence
    );
    let checkpoint =
        serde_json::to_value(Checkpoint::capture(&engine).encode("health-replacement", sequence))
            .unwrap();
    let archive = serde_json::to_value(&engine.archive).unwrap();
    drop(engine);

    // Independent, journal-only recovery of exactly the same retained records.
    let journal_path = directory.path().join("health-replacement-journal.db");
    std::fs::copy(&path, &journal_path).unwrap();
    let connection = rusqlite::Connection::open(&journal_path).unwrap();
    connection
        .execute_batch("BEGIN; INSERT INTO journal SELECT * FROM history; DELETE FROM history; DELETE FROM checkpoint; COMMIT;")
        .unwrap();
    drop(connection);
    let mut resumed = Engine::open(&path, scenario.clone()).unwrap();
    let mut journal = Engine::open(&journal_path, scenario).unwrap();
    assert_eq!(
        resumed.recovery_profile().checkpoint_sequence,
        checkpoint_sequence
    );
    assert_eq!(resumed.recovery_profile().records_replayed, 2);
    assert_eq!(journal.recovery_profile().checkpoint_sequence, 0);
    assert!(journal.recovery_profile().records_replayed > 2);
    assert_authority_equal(
        &resumed,
        &journal,
        "replacement checkpoint plus tail versus journal",
    );
    for state in [&mut resumed, &mut journal] {
        assert_eq!(serde_json::to_value(&state.archive).unwrap(), archive);
        assert_eq!(
            serde_json::to_value(Checkpoint::capture(state).encode("health-replacement", sequence))
                .unwrap(),
            checkpoint
        );
        state.enable_wizard().unwrap();
        assert_eq!(state.inspect_creature(actor).unwrap(), tail);
        {
            let unchanged = state.diagnostic_copy();
            let retry = state
                .command(
                    "wizard",
                    "test",
                    actor,
                    "replace-recovery-talent",
                    &branch,
                    command.clone(),
                )
                .unwrap();
            assert!(retry.duplicate);
            assert_eq!(retry.entry, replaced.entry);
            assert_authority_equal(state, &unchanged, "replacement retry after reopen");
        }
        state
            .command(
                "player",
                "test",
                actor,
                "rewind-replacement",
                &branch,
                Command::Wizard {
                    expected_revision: state.revision(actor).unwrap(),
                    operation: WizardOperation::Rewind {
                        target: rewind_target.clone(),
                    },
                },
            )
            .unwrap();
        assert_eq!(state.inspect_creature(actor).unwrap(), before);
        restored_both_ways(state, "recovering replacement rewind");
    }
    assert_eq!(resumed.game, journal.game);
}
