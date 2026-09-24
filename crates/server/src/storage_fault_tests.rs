//! Faults act on the current temporary-file writer and publication boundaries.
//! They establish process recovery behavior, not power-loss durability.
use super::*;

#[test]
fn every_storage_failure_keeps_published_state_and_retry_identity_consistent() {
    for fault in [
        StorageFault::BeforeWrite,
        StorageFault::PartialWrite,
        StorageFault::AfterFlush,
        StorageFault::AfterSync,
        StorageFault::BeforeReplace,
        StorageFault::AfterReplace,
        StorageFault::BeforePublication,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("game.json");
        let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let before = engine.state(actor).unwrap();
        let original_bytes = fs::read(&path).unwrap();
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: 0,
            action: Action::Wait,
        };
        engine.storage_fault = Some(fault);
        let failure = engine
            .command(
                "fault",
                "test",
                actor,
                "same-request",
                &branch,
                command.clone(),
            )
            .unwrap_err();
        assert_eq!(failure.code, ErrorCode::StorageFailure, "{fault:?}");
        assert_eq!(engine.state(actor).unwrap(), before, "{fault:?}");
        assert!(engine
            .retry("fault", actor, "same-request", &branch, &command)
            .unwrap()
            .is_none());
        let installed = matches!(
            fault,
            StorageFault::AfterReplace | StorageFault::BeforePublication
        );
        if !installed {
            assert_eq!(fs::read(&path).unwrap(), original_bytes);
            engine.storage_fault = None;
            let accepted = engine
                .command(
                    "fault",
                    "test",
                    actor,
                    "same-request",
                    &branch,
                    command.clone(),
                )
                .unwrap();
            assert!(!accepted.duplicate);
        }
        drop(engine); // Restart after an uncertain replacement/publication result.
        let mut recovered = Engine::open(&path, Scenario::two_room(42)).unwrap();
        assert_eq!(recovered.profile_counts().0, 1, "{fault:?}");
        let expected_id = recovered
            .retry("fault", actor, "same-request", &branch, &command)
            .unwrap()
            .unwrap()
            .entry
            .id;
        let retried = recovered
            .command("fault", "test", actor, "same-request", &branch, command)
            .unwrap();
        assert!(retried.duplicate);
        assert_eq!(retried.entry.id, expected_id);
        assert_eq!(recovered.state(actor).unwrap().observation.tick, 100);
    }
}

#[test]
fn interruption_before_replace_restarts_at_previous_boundary_then_accepts_retry() {
    for fault in [
        StorageFault::BeforeWrite,
        StorageFault::PartialWrite,
        StorageFault::AfterFlush,
        StorageFault::AfterSync,
        StorageFault::BeforeReplace,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("game.json");
        let mut engine = Engine::open(&path, Scenario::two_room(42)).unwrap();
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: 0,
            action: Action::Wait,
        };
        engine.storage_fault = Some(fault);
        assert!(engine
            .command(
                "fault",
                "test",
                ActorId(1),
                "retry",
                &branch,
                command.clone()
            )
            .is_err());
        drop(engine);
        let mut resumed = Engine::open(&path, Scenario::two_room(42)).unwrap();
        assert_eq!(resumed.profile_counts().0, 0);
        let result = resumed
            .command("fault", "test", ActorId(1), "retry", &branch, command)
            .unwrap();
        assert!(!result.duplicate);
        assert_eq!(resumed.profile_counts().0, 1);
    }
}
