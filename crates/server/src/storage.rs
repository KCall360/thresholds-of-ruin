//! Append-only application journal. SQLite supplies atomic batches and recovery;
//! the worker owns all disk I/O after bootstrap, never the engine/session lock.
use crate::{
    engine::{invalid_archive, storage_failure, Archive, Record},
    Failure,
};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MAX_PAYLOAD: usize = 1024 * 1024;
const APP_ID: i64 = 0x544f524a;

#[derive(Clone, Debug)]
pub struct SavePolicy {
    pub target_interval: Duration,
    pub max_unsaved_age: Duration,
    pub idle_interval: Duration,
    pub max_pending_bytes: usize,
}
impl Default for SavePolicy {
    fn default() -> Self {
        Self {
            target_interval: Duration::from_secs(30),
            max_unsaved_age: Duration::from_secs(60),
            idle_interval: Duration::from_millis(750),
            max_pending_bytes: 8 * MAX_PAYLOAD,
        }
    }
}
impl SavePolicy {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.target_interval.is_zero()
            || self.max_unsaved_age < self.target_interval
            || self.max_unsaved_age > Duration::from_secs(86400)
            || self.max_pending_bytes == 0
            || self.max_pending_bytes > 1024 * MAX_PAYLOAD
            || self.idle_interval > self.max_unsaved_age
        {
            return Err(Failure::new(
                tor_protocol::ErrorCode::InvalidRequest,
                "Invalid background save policy",
            ));
        }
        Ok(())
    }
    fn due(&self, age: Duration, idle: Duration, bytes: usize, forced: bool) -> bool {
        forced
            || age >= self.max_unsaved_age
            || bytes >= self.max_pending_bytes.saturating_mul(3) / 4
            || (age >= self.target_interval && idle >= self.idle_interval)
    }
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct SaveStatus {
    pub accepted_sequence: u64,
    pub durable_sequence: u64,
    pub pending_bytes: usize,
    pub unsaved_age_ms: u64,
    pub saving: bool,
    pub overdue: bool,
    pub error: Option<String>,
    pub batches: u64,
    /// Application bytes committed, not physical filesystem writes.
    pub journal_bytes: u64,
    pub last_batch_ms: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    save_id: String,
    generation: u64,
    record: T,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    save_id: String,
    generation: u64,
    wizard_game: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Base {
    save_id: String,
    generation: u64,
    archive: Archive,
}

fn crc32c(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
pub(crate) fn frame<T: Serialize>(kind: u16, sequence: u64, value: &T) -> Result<Vec<u8>, Failure> {
    let payload = serde_json::to_vec(value).map_err(|_| storage_failure())?;
    if payload.len() > MAX_PAYLOAD {
        return Err(storage_failure());
    }
    let mut bytes = Vec::with_capacity(24 + payload.len());
    bytes.extend_from_slice(if kind == 0 { b"TORB" } else { b"TORJ" });
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let crc = crc32c(bytes[4..].iter().copied().chain(payload.iter().copied()));
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
fn decode(bytes: &[u8], sequence: u64) -> Result<(u16, &[u8]), Failure> {
    if bytes.len() < 24 || bytes.len() > MAX_PAYLOAD + 24 {
        return Err(invalid_archive());
    }
    let kind = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let magic = if sequence == 0 { b"TORB" } else { b"TORJ" };
    if &bytes[..4] != magic
        || bytes[4..6] != 4u16.to_le_bytes()
        || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != sequence
        || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize != bytes.len() - 24
        || crc32c(
            bytes[4..20]
                .iter()
                .copied()
                .chain(bytes[24..].iter().copied()),
        ) != u32::from_le_bytes(bytes[20..24].try_into().unwrap())
        || (sequence == 0 && kind != 0)
        || (sequence != 0 && !matches!(kind, 1 | 2))
    {
        return Err(invalid_archive());
    }
    Ok((kind, &bytes[24..]))
}
fn strict<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, Failure> {
    let value: T = serde_json::from_slice(bytes).map_err(|_| invalid_archive())?;
    let original = serde_json::from_slice::<UniqueJson>(bytes)
        .map_err(|_| invalid_archive())?
        .0;
    if serde_json::to_value(&value).map_err(|_| invalid_archive())? != original {
        return Err(invalid_archive());
    }
    Ok(value)
}
// Typed maps otherwise silently retain the last duplicate key.
struct UniqueJson(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<UniqueJson, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = a.next_element::<UniqueJson>()? {
                    values.push(v.0);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                    values.insert(key, a.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
fn connection(path: &Path) -> Result<Connection, Failure> {
    let conn = Connection::open(path).map_err(|_| storage_failure())?;
    conn.busy_timeout(Duration::from_secs(2))
        .map_err(|_| storage_failure())?;
    conn.execute_batch(
        "PRAGMA trusted_schema=OFF; PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA;",
    )
    .map_err(|_| storage_failure())?;
    Ok(conn)
}
fn append(conn: &mut Connection, entries: &[Pending]) -> Result<(), Failure> {
    let tx = conn.transaction().map_err(|_| storage_failure())?;
    for entry in entries {
        // Retrying an uncertain commit reconciles exact stored bytes. A conflicting
        // sequence is never overwritten and no request can execute twice.
        let existing: Option<Vec<u8>> = tx
            .query_row(
                "SELECT frame FROM journal WHERE sequence=?1",
                [entry.sequence as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| storage_failure())?;
        match existing {
            Some(bytes) if bytes == entry.bytes => {}
            Some(_) => return Err(invalid_archive()),
            None => {
                tx.execute(
                    "INSERT INTO journal(sequence,frame) VALUES (?1,?2)",
                    params![entry.sequence as i64, entry.bytes],
                )
                .map_err(|_| storage_failure())?;
            }
        }
    }
    tx.commit().map_err(|_| storage_failure())
}
use rusqlite::OptionalExtension;

/// Read a format-4 save for diagnostics. Gameplay uses normal strict replay too.
pub fn inspect_save(path: impl AsRef<Path>) -> Result<serde_json::Value, Failure> {
    let (_, archive, _, _) = load(path.as_ref())?;
    serde_json::to_value(archive).map_err(|_| invalid_archive())
}
fn load(path: &Path) -> Result<(Connection, Archive, String, u64), Failure> {
    // Refuse JSON and other formats without letting SQLite change the input.
    let mut file = std::fs::File::open(path).map_err(|_| storage_failure())?;
    let mut header = [0u8; 16];
    std::io::Read::read_exact(&mut file, &mut header).map_err(|_| invalid_archive())?;
    if &header != b"SQLite format 3\0" {
        return Err(invalid_archive());
    }
    let conn = connection(path).map_err(|_| invalid_archive())?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    let app: i64 = conn
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    let integrity: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    if version != 4 || app != APP_ID || integrity != "ok" {
        return Err(invalid_archive());
    }
    let mut statement = conn
        .prepare("SELECT sequence,length(frame),frame FROM journal ORDER BY sequence")
        .map_err(|_| invalid_archive())?;
    let mut rows = statement.query([]).map_err(|_| invalid_archive())?;
    let mut archive = None;
    let mut save_id = String::new();
    let mut next = 0u64;
    while let Some(row) = rows.next().map_err(|_| invalid_archive())? {
        let sequence: u64 = u64::try_from(row.get::<_, i64>(0).map_err(|_| invalid_archive())?)
            .map_err(|_| invalid_archive())?;
        let size: usize = usize::try_from(row.get::<_, i64>(1).map_err(|_| invalid_archive())?)
            .map_err(|_| invalid_archive())?;
        if sequence != next || !(24..=MAX_PAYLOAD + 24).contains(&size) {
            return Err(invalid_archive());
        }
        let bytes: Vec<u8> = row.get(2).map_err(|_| invalid_archive())?;
        let (kind, payload) = decode(&bytes, sequence)?;
        match kind {
            0 => {
                let base: Base = strict(payload)?;
                if base.generation != 0
                    || uuid::Uuid::parse_str(&base.save_id).is_err()
                    || !base.archive.records.is_empty()
                {
                    return Err(invalid_archive());
                }
                save_id = base.save_id;
                archive = Some(base.archive);
            }
            1 => {
                let envelope: Envelope<Record> = strict(payload)?;
                if envelope.save_id != save_id || envelope.generation != 0 {
                    return Err(invalid_archive());
                }
                let a = archive.as_mut().ok_or_else(invalid_archive)?;
                if matches!(
                    envelope.record.entry.content,
                    crate::journal::HistoryContent::Wizard { .. }
                ) && !a.wizard_game
                {
                    return Err(invalid_archive());
                }
                a.records.push(envelope.record);
            }
            2 => {
                let marker: Marker = strict(payload)?;
                if marker.save_id != save_id || marker.generation != 0 || !marker.wizard_game {
                    return Err(invalid_archive());
                }
                archive.as_mut().ok_or_else(invalid_archive)?.wizard_game = true;
            }
            _ => return Err(invalid_archive()),
        }
        next = next.checked_add(1).ok_or_else(invalid_archive)?;
    }
    drop(rows);
    drop(statement);
    Ok((
        conn,
        archive.ok_or_else(invalid_archive)?,
        save_id,
        next - 1,
    ))
}

#[derive(Debug)]
struct Pending {
    sequence: u64,
    bytes: Vec<u8>,
    queued: Instant,
}
#[derive(Debug)]
struct State {
    pending: VecDeque<Pending>,
    status: SaveStatus,
    oldest: Option<Instant>,
    last_activity: Instant,
    force: u64,
    closing: bool,
}
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    policy: SavePolicy,
}
#[derive(Debug)]
struct Owner {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    save_id: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Store(Arc<Owner>);
impl Store {
    pub(crate) fn open(
        path: &Path,
        initial: impl FnOnce() -> Result<Archive, Failure>,
        policy: SavePolicy,
        lock: Arc<std::fs::File>,
    ) -> Result<(Self, Archive), Failure> {
        policy.validate()?;
        let (conn, archive, save_id, sequence) = if path.exists() {
            load(path)?
        } else {
            let initial = initial()?;
            let mut conn = connection(path)?;
            let save_id = uuid::Uuid::new_v4().to_string();
            let mut base = initial.clone();
            base.records.clear();
            let bytes = frame(
                0,
                0,
                &Base {
                    save_id: save_id.clone(),
                    generation: 0,
                    archive: base,
                },
            )?;
            let tx = conn.transaction().map_err(|_| storage_failure())?;
            tx.execute_batch("PRAGMA application_id=1414484554; PRAGMA user_version=4; CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT;").map_err(|_| storage_failure())?;
            tx.execute("INSERT INTO journal VALUES (0,?1)", [bytes])
                .map_err(|_| storage_failure())?;
            for (index, record) in initial.records.iter().enumerate() {
                let seq = index as u64 + 1;
                let bytes = frame(
                    1,
                    seq,
                    &Envelope {
                        save_id: save_id.clone(),
                        generation: 0,
                        record,
                    },
                )?;
                tx.execute(
                    "INSERT INTO journal VALUES (?1,?2)",
                    params![seq as i64, bytes],
                )
                .map_err(|_| storage_failure())?;
            }
            tx.commit().map_err(|_| storage_failure())?;
            let sequence = initial.records.len() as u64;
            (conn, initial, save_id, sequence)
        };
        let shared = Arc::new(Shared {
            policy,
            wake: Condvar::new(),
            state: Mutex::new(State {
                pending: VecDeque::new(),
                status: SaveStatus {
                    accepted_sequence: sequence,
                    durable_sequence: sequence,
                    ..SaveStatus::default()
                },
                oldest: None,
                last_activity: Instant::now(),
                force: sequence,
                closing: false,
            }),
        });
        let worker = shared.clone();
        let path = path.to_owned();
        let thread = std::thread::Builder::new()
            .name("save-journal".into())
            .spawn(move || {
                let _lock = lock;
                run_worker(worker, conn, path);
            })
            .map_err(|_| storage_failure())?;
        Ok((
            Self(Arc::new(Owner {
                shared,
                thread: Mutex::new(Some(thread)),
                save_id,
            })),
            archive,
        ))
    }
    pub(crate) fn enqueue(&self, record: &Record) -> Result<usize, Failure> {
        self.push(1, record)
    }
    fn push<T: Serialize>(&self, kind: u16, record: &T) -> Result<usize, Failure> {
        let shared = &self.0.shared;
        let mut s = shared.state.lock().unwrap();
        if s.status.error.is_some() || s.closing {
            return Err(storage_failure());
        }
        let sequence = s
            .status
            .accepted_sequence
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or_else(storage_failure)?;
        let bytes = if kind == 2 {
            frame(
                kind,
                sequence,
                &Marker {
                    save_id: self.0.save_id.clone(),
                    generation: 0,
                    wizard_game: true,
                },
            )?
        } else {
            frame(
                kind,
                sequence,
                &Envelope {
                    save_id: self.0.save_id.clone(),
                    generation: 0,
                    record,
                },
            )?
        };
        let len = bytes.len();
        if s.status.pending_bytes.saturating_add(len) > shared.policy.max_pending_bytes {
            s.force = s.status.accepted_sequence;
            shared.wake.notify_one();
            return Err(Failure::new(
                tor_protocol::ErrorCode::StorageFailure,
                "Save queue is full; wait for saving to finish before retrying",
            ));
        }
        let now = Instant::now();
        s.pending.push_back(Pending {
            sequence,
            bytes,
            queued: now,
        });
        s.status.accepted_sequence = sequence;
        s.status.pending_bytes += len;
        s.oldest.get_or_insert(now);
        s.last_activity = now;
        shared.wake.notify_one();
        Ok(len)
    }
    pub(crate) fn wizard(&self) -> Result<(), Failure> {
        self.push(2, &())?;
        Ok(())
    }
    pub(crate) fn status(&self) -> SaveStatus {
        let s = self.0.shared.state.lock().unwrap();
        let mut status = s.status.clone();
        let age = s.oldest.map(|t| t.elapsed()).unwrap_or_default();
        status.unsaved_age_ms = age.as_millis().min(u128::from(u64::MAX)) as u64;
        status.overdue = s.oldest.is_some() && age >= self.0.shared.policy.max_unsaved_age;
        status
    }
    pub(crate) fn request_flush(&self) -> u64 {
        let mut s = self.0.shared.state.lock().unwrap();
        s.force = s.status.accepted_sequence;
        s.status.error = None;
        self.0.shared.wake.notify_one();
        s.force
    }
    pub(crate) fn flush(&self) -> Result<(), Failure> {
        let target = self.request_flush();
        let shared = &self.0.shared;
        let mut s = shared.state.lock().unwrap();
        while s.status.durable_sequence < target {
            if s.status.error.is_some() {
                return Err(storage_failure());
            }
            s = shared.wake.wait(s).unwrap();
        }
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        {
            let mut s = self.shared.state.lock().unwrap();
            s.closing = true;
            s.force = s.status.accepted_sequence;
            self.shared.wake.notify_one();
        }
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}
fn run_worker(shared: Arc<Shared>, conn: Connection, path: PathBuf) {
    let mut conn = Some(conn);
    loop {
        let mut s = shared.state.lock().unwrap();
        let age = s.oldest.map(|t| t.elapsed()).unwrap_or_default();
        let due = !s.pending.is_empty()
            && shared.policy.due(
                age,
                s.last_activity.elapsed(),
                s.status.pending_bytes,
                s.force > s.status.durable_sequence,
            );
        if s.closing && (s.pending.is_empty() || s.status.error.is_some()) {
            break;
        }
        if !due || s.status.error.is_some() {
            if s.pending.is_empty() || s.status.error.is_some() {
                drop(shared.wake.wait(s).unwrap());
            } else {
                let deadline = shared.policy.max_unsaved_age.saturating_sub(age);
                let opportunity = shared.policy.target_interval.saturating_sub(age).max(
                    shared
                        .policy
                        .idle_interval
                        .saturating_sub(s.last_activity.elapsed()),
                );
                drop(
                    shared
                        .wake
                        .wait_timeout(s, deadline.min(opportunity).max(Duration::from_millis(1)))
                        .unwrap(),
                );
            }
            continue;
        }
        let batch: Vec<_> = s.pending.drain(..).collect();
        s.status.saving = true;
        drop(s);
        let start = Instant::now();
        let outcome = if let Some(conn) = &mut conn {
            append(conn, &batch)
        } else {
            match connection(&path) {
                Ok(mut reopened) => {
                    let result = append(&mut reopened, &batch);
                    conn = Some(reopened);
                    result
                }
                Err(error) => Err(error),
            }
        };
        let mut s = shared.state.lock().unwrap();
        s.status.saving = false;
        match outcome {
            Ok(()) => {
                let bytes = batch.iter().map(|p| p.bytes.len()).sum::<usize>();
                s.status.pending_bytes -= bytes;
                s.status.journal_bytes += bytes as u64;
                s.status.durable_sequence = batch.last().unwrap().sequence;
                s.status.batches += 1;
                s.status.last_batch_ms =
                    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                s.oldest = s.pending.front().map(|p| p.queued);
            }
            Err(_) => {
                for entry in batch.into_iter().rev() {
                    s.pending.push_front(entry);
                }
                s.status.error = Some(
                    "Background save failed; recent play is not saved. Use save to retry.".into(),
                );
                // Discard uncertain connection state before an explicit retry.
                drop(s);
                conn = None;
                s = shared.state.lock().unwrap();
            }
        }
        shared.wake.notify_all();
        drop(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_and_frame_corruption() {
        assert_eq!(crc32c(b"123456789".iter().copied()), 0xe3069283);
        let bytes = frame(1, 1, &serde_json::json!({"test":1})).unwrap();
        for index in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[index] ^= 1;
            assert!(decode(&bad, 1).is_err(), "byte {index}");
        }
        for cut in 0..bytes.len() {
            assert!(decode(&bytes[..cut], 1).is_err());
        }
    }
    #[test]
    fn strict_json_rejects_nested_duplicate_keys() {
        assert!(strict::<serde_json::Value>(br#"{"map":{"a":1,"a":1}}"#).is_err());
        assert!(strict::<Marker>(br#"{"save_id":"id","generation":0}"#).is_err());
    }
    #[test]
    fn uncertain_commit_retry_reconciles_bytes_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = connection(&dir.path().join("retry.db")).unwrap();
        conn.execute_batch(
            "CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT;",
        )
        .unwrap();
        let mut batch = vec![Pending {
            sequence: 1,
            bytes: vec![1, 2, 3],
            queued: Instant::now(),
        }];
        append(&mut conn, &batch).unwrap();
        append(&mut conn, &batch).unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM journal", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        batch[0].bytes[0] = 9;
        assert!(append(&mut conn, &batch).is_err());
        assert_eq!(
            conn.query_row("SELECT frame FROM journal", [], |r| r.get::<_, Vec<u8>>(0))
                .unwrap(),
            vec![1, 2, 3]
        );
    }
    #[test]
    fn idle_opportunities_never_starve_deadline_or_pressure() {
        let p = SavePolicy::default();
        assert!(!p.due(Duration::from_secs(20), Duration::from_secs(2), 0, false));
        assert!(!p.due(Duration::from_secs(40), Duration::ZERO, 0, false));
        assert!(p.due(Duration::from_secs(40), Duration::from_secs(1), 0, false));
        assert!(p.due(Duration::from_secs(60), Duration::ZERO, 0, false));
        assert!(p.due(Duration::ZERO, Duration::ZERO, p.max_pending_bytes, false));
        assert!(p.due(Duration::ZERO, Duration::ZERO, 0, true));
    }
}
