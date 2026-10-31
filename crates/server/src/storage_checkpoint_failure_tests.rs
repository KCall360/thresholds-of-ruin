//! Per-store faults exercise the real worker/encoder without global environment
//! overrides. A reduced test limit is not evidence that gameplay reaches the cap.
use super::*;
use crate::journal::{Action, Command};
use crate::{Engine, Scenario};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use tor_protocol::ActorId;

#[derive(Debug)]
struct Pause {
    entered: Sender<()>,
    resume: Receiver<()>,
}
#[derive(Debug)]
pub(super) struct CheckpointTest {
    limit: AtomicUsize,
    attempts: AtomicUsize,
    pause: Mutex<Option<Pause>>,
}
impl Default for CheckpointTest {
    fn default() -> Self {
        Self {
            limit: AtomicUsize::new(checkpoint_codec::LIMIT),
            attempts: AtomicUsize::new(0),
            pause: Mutex::new(None),
        }
    }
}
pub(super) fn encode(
    shared: &Shared,
    checkpoint: &DiskCheckpoint,
) -> Result<Result<Vec<u8>, checkpoint_codec::EncodeError>, SaveFailure> {
    let test = &shared.checkpoint_test;
    test.attempts.fetch_add(1, Ordering::SeqCst);
    let limit = test.limit.load(Ordering::SeqCst);
    let pause = test.pause.lock().unwrap().take();
    if let Some(pause) = pause {
        pause.entered.send(()).map_err(|_| SaveFailure::Retryable)?;
        pause
            .resume
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| SaveFailure::Retryable)?;
    }
    Ok(checkpoint_codec::write_json_classified_with_test_limit(
        checkpoint,
        Vec::new(),
        limit,
    ))
}

fn fixture(path: &Path) -> Engine {
    Engine::open_with_policy(
        path,
        Scenario::two_room(42),
        SavePolicy {
            target_interval: Duration::from_secs(3_600),
            idle_interval: Duration::ZERO,
            max_unsaved_age: Duration::from_secs(3_600),
            checkpoint_interval: 1,
            ..SavePolicy::default()
        },
    )
    .unwrap()
}
fn accept_wait(engine: &mut Engine, request: &str) -> Result<(), Failure> {
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            request,
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .map(|_| ())
}
#[derive(Debug, PartialEq, Eq)]
struct PendingWitness {
    sequence: u64,
    bytes: Vec<u8>,
    regions: Vec<(u64, Vec<u8>)>,
    sources: Vec<(u64, Arc<str>)>,
}
#[derive(Debug, PartialEq, Eq)]
struct RetainedWork {
    pending: Vec<PendingWitness>,
    checkpoint: Option<(u64, Vec<u8>)>,
    accepted: u64,
    durable: u64,
    pending_bytes: usize,
}
fn retained(store: &Store) -> RetainedWork {
    let state = store.0.shared.state.lock().unwrap();
    RetainedWork {
        pending: state
            .pending
            .iter()
            .map(|entry| PendingWitness {
                sequence: entry.sequence,
                bytes: entry.bytes.clone(),
                regions: entry.regions.clone(),
                sources: entry.sources.clone(),
            })
            .collect(),
        checkpoint: state.checkpoint.as_ref().map(|(seq, checkpoint)| {
            (
                *seq,
                serde_json::to_vec(&checkpoint.encode(&store.0.save_id, *seq)).unwrap(),
            )
        }),
        accepted: state.status.accepted_sequence,
        durable: state.status.durable_sequence,
        pending_bytes: state.status.pending_bytes,
    }
}
fn attempts(store: &Store) -> usize {
    store
        .0
        .shared
        .checkpoint_test
        .attempts
        .load(Ordering::SeqCst)
}
fn wait_for_failure(store: &Store) {
    let shared = &store.0.shared;
    let state = shared.state.lock().unwrap();
    let (state, timeout) = shared
        .wake
        .wait_timeout_while(state, Duration::from_secs(10), |state| {
            state.status.error.is_none()
        })
        .unwrap();
    assert!(!timeout.timed_out(), "worker did not report failure");
    assert!(state.status.error.is_some());
}

#[test]
fn unchanged_capacity_failure_keeps_work_and_explicit_save_does_not_reencode() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capacity.db");
    let mut engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(0, Ordering::SeqCst);
    accept_wait(&mut engine, "accepted").unwrap();
    let expected_state = engine.state(ActorId(1)).unwrap();
    let work = retained(&store);
    assert_eq!(work.accepted, 1);
    assert_eq!(work.durable, 0);
    assert_eq!(work.pending.len(), 1);
    assert_eq!(work.checkpoint.as_ref().unwrap().0, 1);
    assert!(work.pending_bytes > 0);
    // The real queued state fits the ordinary codec. Only the per-store test
    // ceiling is impossible; this is not a production cap-reachability claim.
    assert!(encode_checkpoint(&Checkpoint::capture(&engine).encode(&store.0.save_id, 1)).is_ok());
    assert_eq!(
        store.flush().unwrap_err().code,
        tor_protocol::ErrorCode::StorageFailure
    );
    assert_eq!(attempts(&store), 1);
    assert_eq!(retained(&store), work);
    let failure = store.status().error.unwrap();
    assert_eq!(
        store.flush().unwrap_err().code,
        tor_protocol::ErrorCode::StorageFailure
    );
    assert_eq!(
        attempts(&store),
        1,
        "the unchanged capacity failure must not rerun encoding"
    );
    assert_eq!(store.status().error.as_deref(), Some(failure.as_str()));
    assert!(failure.contains("Retrying this unchanged checkpoint cannot help"));
    assert_eq!(retained(&store), work);
    assert_eq!(
        accept_wait(&mut engine, "blocked").unwrap_err().code,
        tor_protocol::ErrorCode::StorageFailure
    );
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected_state);
    assert_eq!(retained(&store), work);
    let disk = connection(&path).unwrap();
    assert_eq!(
        disk.query_row("SELECT max(sequence) FROM journal", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        disk.query_row("SELECT count(*) FROM checkpoint", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn retryable_database_failure_keeps_exact_batch_and_reopens_after_save_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("locked.db");
    let mut engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    let blocker = connection(&path).unwrap();
    blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
    accept_wait(&mut engine, "accepted").unwrap();
    let work = retained(&store);
    let expected = engine.state(ActorId(1)).unwrap();
    assert!(store.flush().is_err());
    assert_eq!(attempts(&store), 1);
    assert_eq!(retained(&store), work);
    assert!(store
        .status()
        .error
        .unwrap()
        .ends_with("Use save to retry."));
    assert!(accept_wait(&mut engine, "blocked").is_err());
    assert_eq!(engine.state(ActorId(1)).unwrap(), expected);
    blocker.execute_batch("ROLLBACK").unwrap();
    store.flush().unwrap();
    assert_eq!(attempts(&store), 2);
    let status = store.status();
    assert_eq!(status.accepted_sequence, 1);
    assert_eq!(status.durable_sequence, 1);
    assert_eq!(status.checkpoint_sequence, 1);
    assert_eq!(status.pending_bytes, 0);
    assert!(status.error.is_none());
    let disk = connection(&path).unwrap();
    let frame: Vec<u8> = disk
        .query_row("SELECT frame FROM history WHERE sequence=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(frame, work.pending[0].bytes);
    drop(disk);
    drop(blocker);
    drop(store);
    drop(engine);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}

#[test]
fn capacity_failure_of_older_snapshot_does_not_block_a_newer_retained_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("superseded.db");
    let mut engine = fixture(&path);
    let store = engine.flush_handle().unwrap();
    let (entered, observed) = mpsc::channel();
    let (resume, continuation) = mpsc::channel();
    *store.0.shared.checkpoint_test.pause.lock().unwrap() = Some(Pause {
        entered,
        resume: continuation,
    });
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(0, Ordering::SeqCst);
    accept_wait(&mut engine, "first").unwrap();
    store.request_flush();
    observed.recv_timeout(Duration::from_secs(10)).unwrap();
    accept_wait(&mut engine, "second").unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    store
        .0
        .shared
        .checkpoint_test
        .limit
        .store(checkpoint_codec::LIMIT, Ordering::SeqCst);
    resume.send(()).unwrap();
    wait_for_failure(&store);
    let work = retained(&store);
    assert_eq!(attempts(&store), 1);
    assert_eq!(work.accepted, 2);
    assert_eq!(work.durable, 0);
    assert_eq!(
        work.pending
            .iter()
            .map(|row| row.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(work.checkpoint.as_ref().unwrap().0, 2);
    assert!(store
        .status()
        .error
        .unwrap()
        .contains("a newer checkpoint is retained"));
    store.flush().unwrap();
    assert_eq!(attempts(&store), 2);
    assert_eq!(store.status().durable_sequence, 2);
    assert_eq!(store.status().checkpoint_sequence, 2);
    assert_eq!(store.status().pending_bytes, 0);
    drop(store);
    drop(engine);
    let restored = Engine::open(&path, Scenario::two_room(0)).unwrap();
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
}
