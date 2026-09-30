//! Application journal and checkpoints. SQLite supplies atomic batches and recovery;
//! the worker owns all disk I/O after bootstrap, never the engine/session lock.
use crate::{
    engine::{invalid_archive, storage_failure, Archive, Checkpoint, DiskCheckpoint, Record},
    Failure,
};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MAX_PAYLOAD: usize = 1024 * 1024;
const MAX_CHECKPOINT: usize = 64 * MAX_PAYLOAD;
/// A region record row may be larger than a journal record.
const MAX_REGION: usize = 16 * MAX_PAYLOAD;
const APP_ID: i64 = 0x544f524a;
/// SQLite `user_version`: the save format, as `ARCHIVE_VERSION`.
const SAVE_FORMAT: i64 = crate::engine::ARCHIVE_VERSION as i64;

#[derive(Clone, Debug)]
pub struct SavePolicy {
    /// Accepted journal entries between snapshots; zero disables checkpointing.
    pub checkpoint_interval: u64,
    pub target_interval: Duration,
    pub max_unsaved_age: Duration,
    pub idle_interval: Duration,
    pub max_pending_bytes: usize,
}
impl Default for SavePolicy {
    fn default() -> Self {
        Self {
            checkpoint_interval: 1024,
            target_interval: Duration::from_secs(30),
            max_unsaved_age: Duration::from_secs(60),
            idle_interval: Duration::from_millis(750),
            max_pending_bytes: 8 * MAX_PAYLOAD,
        }
    }
}
impl SavePolicy {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.checkpoint_interval > 1_000_000
            || self.target_interval.is_zero()
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
    pub checkpoint_sequence: u64,
    pub checkpoints: u64,
    pub checkpoint_bytes: u64,
    pub last_checkpoint_ms: u64,
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
    bytes.extend_from_slice(&6u16.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let crc = crc32c(bytes[4..].iter().copied().chain(payload.iter().copied()));
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
/// A region record row: the journal frame layout with magic `TORR`, kind 3
/// and the record identity in the sequence field.
fn region_frame(id: u64, record: &tor_simulation::RegionRecord) -> Result<Vec<u8>, Failure> {
    let payload = serde_json::to_vec(record).map_err(|_| storage_failure())?;
    if payload.len() > MAX_REGION {
        return Err(storage_failure());
    }
    let mut bytes = Vec::with_capacity(24 + payload.len());
    bytes.extend_from_slice(b"TORR");
    bytes.extend_from_slice(&6u16.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes());
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let crc = crc32c(bytes[4..].iter().copied().chain(payload.iter().copied()));
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
fn decode_region(bytes: &[u8], id: u64) -> Result<tor_simulation::RegionRecord, Failure> {
    if bytes.len() < 24
        || bytes.len() > MAX_REGION + 24
        || &bytes[..4] != b"TORR"
        || bytes[4..6] != 6u16.to_le_bytes()
        || bytes[6..8] != 3u16.to_le_bytes()
        || u64::from_le_bytes(bytes[8..16].try_into().unwrap()) != id
        || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize != bytes.len() - 24
        || crc32c(
            bytes[4..20]
                .iter()
                .copied()
                .chain(bytes[24..].iter().copied()),
        ) != u32::from_le_bytes(bytes[20..24].try_into().unwrap())
    {
        return Err(invalid_archive());
    }
    strict(&bytes[24..])
}
/// Region record rows a checkpoint's transaction writes, and the rows it
/// keeps; it deletes every other row.
pub(crate) struct RegionWrite {
    rows: Vec<(u64, Vec<u8>)>,
    keep: BTreeSet<u64>,
}
impl RegionWrite {
    fn encode(checkpoint: &Checkpoint) -> Result<Self, Failure> {
        Ok(Self {
            rows: checkpoint
                .records
                .iter()
                .map(|(id, record)| Ok((id.0, region_frame(id.0, record)?)))
                .collect::<Result<_, Failure>>()?,
            keep: checkpoint.referenced().into_iter().map(|id| id.0).collect(),
        })
    }
}
fn decode(bytes: &[u8], sequence: u64) -> Result<(u16, &[u8]), Failure> {
    if bytes.len() < 24 || bytes.len() > MAX_PAYLOAD + 24 {
        return Err(invalid_archive());
    }
    let kind = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    let magic = if sequence == 0 { b"TORB" } else { b"TORJ" };
    if &bytes[..4] != magic
        || bytes[4..6] != 6u16.to_le_bytes()
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
fn commit_batch(
    conn: &mut Connection,
    entries: &[Pending],
    checkpoint: Option<(u64, &[u8])>,
    regions: Option<&RegionWrite>,
    mut fault: impl FnMut(&Connection, &str) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let tx = conn.transaction().map_err(|_| storage_failure())?;
    for entry in entries {
        // Retrying an uncertain commit reconciles exact stored bytes. A conflicting
        // sequence is never overwritten and no request can execute twice.
        let existing: Option<Vec<u8>> = tx
            .query_row(
                "SELECT frame FROM journal WHERE sequence=?1 UNION ALL SELECT frame FROM history WHERE sequence=?1",
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
    fault(&tx, "after_append")?;
    if let Some((sequence, bytes)) = checkpoint {
        // Rows are written before the checkpoint naming them, in the same
        // transaction. A row is never rewritten: a retry must match it exactly.
        for (id, frame) in regions.map(|r| r.rows.as_slice()).unwrap_or_default() {
            let existing: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT frame FROM regions WHERE record=?1",
                    [*id as i64],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| storage_failure())?;
            match existing {
                Some(old) if old == *frame => {}
                Some(_) => return Err(invalid_archive()),
                None => {
                    tx.execute(
                        "INSERT INTO regions(record,frame) VALUES (?1,?2)",
                        params![*id as i64, frame],
                    )
                    .map_err(|_| storage_failure())?;
                }
            }
        }
        fault(&tx, "after_regions")?;
        let current: Option<(i64, Vec<u8>)> = tx
            .query_row(
                "SELECT sequence,payload FROM checkpoint WHERE slot=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| storage_failure())?;
        if current.as_ref().is_some_and(|(seq, old)| {
            *seq > sequence as i64 || (*seq == sequence as i64 && old != bytes)
        }) {
            return Err(invalid_archive());
        }
        tx.execute(
            "INSERT OR REPLACE INTO checkpoint VALUES (1,?1,?2,?3)",
            params![
                sequence as i64,
                bytes,
                i64::from(crc32c(bytes.iter().copied()))
            ],
        )
        .map_err(|_| storage_failure())?;
        fault(&tx, "after_checkpoint")?;
        tx.execute(
            "INSERT INTO history SELECT * FROM journal WHERE sequence>0 AND sequence<=?1",
            [sequence as i64],
        )
        .map_err(|_| storage_failure())?;
        fault(&tx, "after_history")?;
        tx.execute(
            "DELETE FROM journal WHERE sequence>0 AND sequence<=?1",
            [sequence as i64],
        )
        .map_err(|_| storage_failure())?;
        fault(&tx, "after_rotation")?;
        // Delete rows the new checkpoint doesn't refer to. Record identities
        // are never reused, so nothing will refer to them again.
        let keep = regions.map(|r| &r.keep);
        let stored: Vec<i64> = tx
            .prepare("SELECT record FROM regions")
            .and_then(|mut statement| {
                statement
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<_, _>>()
            })
            .map_err(|_| storage_failure())?;
        for id in stored {
            if !keep.is_some_and(|keep| keep.contains(&(id as u64))) {
                tx.execute("DELETE FROM regions WHERE record=?1", [id])
                    .map_err(|_| storage_failure())?;
            }
        }
        fault(&tx, "after_gc")?;
    }
    fault(&tx, "before_commit")?;
    tx.commit().map_err(|_| storage_failure())?;
    fault(conn, "after_commit")
}
use rusqlite::OptionalExtension;

fn encode_checkpoint(checkpoint: &DiskCheckpoint) -> Result<Vec<u8>, Failure> {
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > MAX_CHECKPOINT {
                return Err(std::io::Error::other("checkpoint exceeds size limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded(Vec::new());
    serde_json::to_writer(&mut writer, checkpoint).map_err(|_| storage_failure())?;
    Ok(writer.0)
}

fn read_checkpoint(conn: &Connection) -> Result<Option<DiskCheckpoint>, Failure> {
    let count: i64 = conn
        .query_row("SELECT count(*) FROM checkpoint", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    if count == 0 {
        return Ok(None);
    }
    if count != 1 {
        return Err(invalid_archive());
    }
    let (sequence, length): (i64, i64) = conn
        .query_row(
            "SELECT sequence,length(payload) FROM checkpoint WHERE slot=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| invalid_archive())?;
    if sequence <= 0 || length <= 0 || length > MAX_CHECKPOINT as i64 {
        return Err(invalid_archive());
    }
    let (bytes, checksum): (Vec<u8>, u32) = conn
        .query_row(
            "SELECT payload,checksum FROM checkpoint WHERE slot=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| invalid_archive())?;
    if crc32c(bytes.iter().copied()) != checksum {
        return Err(invalid_archive());
    }
    let checkpoint: DiskCheckpoint = strict(&bytes)?;
    if checkpoint.sequence != sequence as u64 {
        return Err(invalid_archive());
    }
    Ok(Some(checkpoint))
}

/// Read a format-6 save for diagnostics. Gameplay uses normal strict replay too.
pub fn inspect_save(path: impl AsRef<Path>) -> Result<serde_json::Value, Failure> {
    let (_, archive, _, _, _) = load(path.as_ref())?;
    serde_json::to_value(archive).map_err(|_| invalid_archive())
}
type Loaded = (Connection, Archive, String, u64, Option<DiskCheckpoint>);
fn load(path: &Path) -> Result<Loaded, Failure> {
    // Refuse JSON and other formats without letting SQLite change the input.
    let mut file = std::fs::File::open(path).map_err(|_| storage_failure())?;
    let mut header = [0u8; 18];
    std::io::Read::read_exact(&mut file, &mut header).map_err(|_| invalid_archive())?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(invalid_archive());
    }
    let page_size = match u16::from_be_bytes([header[16], header[17]]) {
        1 => 65536u64,
        size => u64::from(size),
    };
    if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
        return Err(invalid_archive());
    }
    let conn = connection(path).map_err(|_| invalid_archive())?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    let app: i64 = conn
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .map_err(|_| invalid_archive())?;
    // Region record rows aren't checked here, so opening a save doesn't read
    // the whole world; each row's checksum is checked when it's read.
    let integrity = ["journal", "history", "checkpoint"]
        .iter()
        .map(|table| {
            conn.query_row(&format!("PRAGMA quick_check({table})"), [], |r| {
                r.get::<_, String>(0)
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid_archive())?;
    // SQLite must get the first opportunity to recover a hot rollback journal,
    // including a partially extended database page from an interrupted write.
    if version != SAVE_FORMAT
        || app != APP_ID
        || integrity.iter().any(|result| result != "ok")
        || file.metadata().map_err(|_| storage_failure())?.len() % page_size != 0
    {
        return Err(invalid_archive());
    }
    let checkpoint = read_checkpoint(&conn)?;
    let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
    let mut checkpoint_records = 0;
    let mut statement = conn
        .prepare("SELECT sequence,length(frame),frame,0 FROM journal UNION ALL SELECT sequence,length(frame),frame,1 FROM history ORDER BY sequence")
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
        let retained: bool = row.get(3).map_err(|_| invalid_archive())?;
        if retained != (sequence > 0 && sequence <= checkpoint_sequence) {
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
                if sequence <= checkpoint_sequence {
                    checkpoint_records += 1;
                }
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
    if checkpoint.as_ref().is_some_and(|c| {
        c.save_id != save_id
            || c.sequence >= next
            || c.sequence == 0
            || c.record_count != checkpoint_records
    }) {
        return Err(invalid_archive());
    }
    Ok((
        conn,
        archive.ok_or_else(invalid_archive)?,
        save_id,
        next - 1,
        checkpoint,
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
    checkpoint: Option<(u64, Checkpoint)>,
    last_checkpoint_requested: u64,
    status: SaveStatus,
    oldest: Option<Instant>,
    last_activity: Instant,
    force: u64,
    closing: bool,
    /// After a checkpoint commits: the records on disk, and the identity
    /// watermark at its capture. Taken by the engine.
    written: Option<(BTreeSet<tor_simulation::RecordId>, tor_simulation::RecordId)>,
}
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    policy: SavePolicy,
    save_id: String,
}
#[derive(Debug)]
struct Owner {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
    save_id: String,
    path: PathBuf,
    reader: Mutex<Option<Connection>>,
}
#[derive(Clone, Debug)]
pub(crate) struct Store(Arc<Owner>);
impl Store {
    pub(crate) fn open(
        path: &Path,
        initial: impl FnOnce() -> Result<Archive, Failure>,
        policy: SavePolicy,
        lock: Arc<std::fs::File>,
    ) -> Result<(Self, Archive, Option<DiskCheckpoint>), Failure> {
        policy.validate()?;
        let (conn, archive, save_id, sequence, checkpoint) = if path.exists() {
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
            tx.execute_batch(&format!("PRAGMA application_id=1414484554; PRAGMA user_version={SAVE_FORMAT}; CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE history(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE checkpoint(slot INTEGER PRIMARY KEY CHECK(slot=1), sequence INTEGER NOT NULL, payload BLOB NOT NULL, checksum INTEGER NOT NULL) STRICT; CREATE TABLE regions(record INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT;")).map_err(|_| storage_failure())?;
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
            (conn, initial, save_id, sequence, None)
        };
        let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
        let shared = Arc::new(Shared {
            save_id: save_id.clone(),
            policy,
            wake: Condvar::new(),
            state: Mutex::new(State {
                pending: VecDeque::new(),
                checkpoint: None,
                last_checkpoint_requested: checkpoint_sequence,
                status: SaveStatus {
                    checkpoint_sequence,
                    accepted_sequence: sequence,
                    durable_sequence: sequence,
                    ..SaveStatus::default()
                },
                oldest: None,
                last_activity: Instant::now(),
                force: sequence,
                closing: false,
                written: None,
            }),
        });
        let worker = shared.clone();
        let owned = path.to_owned();
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
                path: owned,
                reader: Mutex::new(None),
            })),
            archive,
            checkpoint,
        ))
    }
    pub(crate) fn enqueue(
        &self,
        record: &Record,
        capture: impl FnOnce() -> Checkpoint,
    ) -> Result<usize, Failure> {
        self.push(1, record, Some(capture))
    }
    fn push<T: Serialize>(
        &self,
        kind: u16,
        record: &T,
        capture: Option<impl FnOnce() -> Checkpoint>,
    ) -> Result<usize, Failure> {
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
        if shared.policy.checkpoint_interval > 0
            && sequence.saturating_sub(s.last_checkpoint_requested)
                >= shared.policy.checkpoint_interval
        {
            if let Some(capture) = capture {
                s.checkpoint = Some((sequence, capture()));
                s.last_checkpoint_requested = sequence;
            }
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
    /// The records on disk after the latest committed checkpoint, once.
    pub(crate) fn take_written(
        &self,
    ) -> Option<(BTreeSet<tor_simulation::RecordId>, tor_simulation::RecordId)> {
        self.0.shared.state.lock().unwrap().written.take()
    }
    /// Read a region record row, on its own connection so the engine never
    /// waits on the worker's lock beyond SQLite's busy timeout.
    pub(crate) fn read_region(
        &self,
        id: tor_simulation::RecordId,
    ) -> Result<Option<tor_simulation::RegionRecord>, Failure> {
        let mut reader = self.0.reader.lock().unwrap();
        if reader.is_none() {
            *reader = Some(connection(&self.0.path)?);
        }
        let bytes: Option<Vec<u8>> = reader
            .as_ref()
            .unwrap()
            .query_row(
                "SELECT frame FROM regions WHERE record=?1",
                [id.0 as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| storage_failure())?;
        bytes.map(|bytes| decode_region(&bytes, id.0)).transpose()
    }
    pub(crate) fn wizard(&self) -> Result<(), Failure> {
        self.push(2, &(), None::<fn() -> Checkpoint>)?;
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
        let checkpoint = s.checkpoint.take();
        s.status.saving = true;
        drop(s);
        let start = Instant::now();
        let checkpoint_started = Instant::now();
        let encoded = checkpoint
            .as_ref()
            .map(|(seq, state)| {
                Ok((
                    encode_checkpoint(&state.encode(&shared.save_id, *seq))?,
                    RegionWrite::encode(state)?,
                ))
            })
            .transpose();
        let checkpoint_ms = checkpoint_started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        let checkpoint_bytes = encoded
            .as_ref()
            .ok()
            .and_then(|v| v.as_ref())
            .map(|(b, regions)| {
                (b.len() + regions.rows.iter().map(|r| r.1.len()).sum::<usize>()) as u64
            })
            .unwrap_or(0);
        let outcome = encoded.and_then(|encoded| {
            if conn.is_none() {
                conn = Some(connection(&path)?);
            }
            commit_batch(
                conn.as_mut().unwrap(),
                &batch,
                checkpoint
                    .as_ref()
                    .zip(encoded.as_ref())
                    .map(|((seq, _), (bytes, _))| (*seq, bytes.as_slice())),
                encoded.as_ref().map(|(_, regions)| regions),
                |_, _| Ok(()),
            )
        });
        let mut s = shared.state.lock().unwrap();
        s.status.saving = false;
        match outcome {
            Ok(()) => {
                if let Some((seq, state)) = &checkpoint {
                    s.written = Some((state.referenced(), state.watermark));
                    s.status.checkpoint_sequence = *seq;
                    s.status.checkpoints += 1;
                    s.status.checkpoint_bytes = checkpoint_bytes;
                    s.status.last_checkpoint_ms = checkpoint_ms;
                }
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
                if s.checkpoint.is_none() {
                    s.checkpoint = checkpoint;
                }
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
    use crate::{Engine, Scenario};
    use tor_protocol::{Action, ActorId};

    fn checkpoint_fixture(path: &Path) -> (Vec<u8>, tor_protocol::StateView) {
        let mut engine = Engine::open_with_policy(
            path,
            Scenario::two_room(42),
            SavePolicy {
                checkpoint_interval: 0,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        for index in 0..4 {
            engine
                .command(
                    "player",
                    "test",
                    ActorId(1),
                    &format!("{index}"),
                    &engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_revision: engine.revision(ActorId(1)).unwrap(),
                        action: Action::Wait,
                    },
                )
                .unwrap();
        }
        engine.flush().unwrap();
        let (_, _, save_id, _, _) = load(path).unwrap();
        let bytes = encode_checkpoint(&Checkpoint::capture(&engine).encode(&save_id, 4)).unwrap();
        (bytes, engine.state(ActorId(1)).unwrap())
    }

    #[test]
    fn region_rows_commit_with_their_checkpoint_and_are_collected_after() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("regions.db");
        let (bytes, _) = checkpoint_fixture(&path);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let package = crate::scenario_package::read_package(&root).unwrap();
        let index = package.index(0).unwrap();
        let record = package.build_region(0, &index, 1).unwrap();
        let row = region_frame(7, &record).unwrap();
        let write = |keep: &[u64], rows: Vec<(u64, Vec<u8>)>| RegionWrite {
            rows,
            keep: keep.iter().copied().collect(),
        };
        let stored = |conn: &Connection| -> Vec<(i64, Vec<u8>)> {
            conn.prepare("SELECT record,frame FROM regions ORDER BY record")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let mut conn = connection(&path).unwrap();
        let commit = |conn: &mut Connection, regions: &RegionWrite, stage: &str| {
            commit_batch(conn, &[], Some((4, &bytes)), Some(regions), |_, at| {
                if at == stage {
                    Err(storage_failure())
                } else {
                    Ok(())
                }
            })
        };
        // A failure anywhere in the transaction leaves no row behind.
        let first = write(&[7], vec![(7, row.clone())]);
        for stage in [
            "after_regions",
            "after_checkpoint",
            "after_gc",
            "before_commit",
        ] {
            assert!(commit(&mut conn, &first, stage).is_err(), "{stage}");
            assert!(stored(&conn).is_empty(), "{stage}");
        }
        commit(&mut conn, &first, "").unwrap();
        assert_eq!(stored(&conn), [(7, row.clone())]);
        // A retry must match the stored row exactly; a row is never rewritten.
        commit(&mut conn, &first, "").unwrap();
        let other = region_frame(7, &package.build_region(0, &index, 2).unwrap()).unwrap();
        assert!(commit(&mut conn, &write(&[7], vec![(7, other)]), "").is_err());
        assert_eq!(stored(&conn), [(7, row.clone())]);
        // Rows decode to their record, and only under their own identity.
        assert_eq!(decode_region(&row, 7).unwrap(), record);
        assert!(decode_region(&row, 8).is_err());
        let mut damaged = row.clone();
        *damaged.last_mut().unwrap() ^= 1;
        assert!(decode_region(&damaged, 7).is_err());
        // Rows the checkpoint still refers to stay without being rewritten;
        // the others are deleted with the checkpoint that stops referring to
        // them, and a failure keeps them.
        commit(&mut conn, &write(&[7], vec![]), "").unwrap();
        assert_eq!(stored(&conn).len(), 1);
        assert!(commit(&mut conn, &write(&[], vec![]), "after_gc").is_err());
        assert_eq!(stored(&conn).len(), 1);
        commit(&mut conn, &write(&[], vec![]), "").unwrap();
        assert!(stored(&conn).is_empty());
    }

    #[test]
    fn checkpoint_transaction_failures_and_uncertain_commits_are_retryable() {
        for stage in [
            "after_append",
            "after_regions",
            "after_checkpoint",
            "after_history",
            "after_rotation",
            "after_gc",
            "before_commit",
            "after_commit",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("fault.db");
            let (bytes, expected) = checkpoint_fixture(&path);
            let mut conn = connection(&path).unwrap();
            assert!(
                commit_batch(&mut conn, &[], Some((4, &bytes)), None, |_, at| {
                    if at == stage {
                        Err(storage_failure())
                    } else {
                        Ok(())
                    }
                })
                .is_err()
            );
            drop(conn);
            let recovered = Engine::open(&path, Scenario::two_room(0)).unwrap();
            assert_eq!(recovered.state(ActorId(1)).unwrap(), expected, "{stage}");
            assert_eq!(
                recovered.recovery_profile().records_replayed,
                if stage == "after_commit" { 0 } else { 4 }
            );
            drop(recovered);
            let mut conn = connection(&path).unwrap();
            commit_batch(&mut conn, &[], Some((4, &bytes)), None, |_, _| Ok(())).unwrap();
            assert_eq!(
                conn.query_row("SELECT count(*) FROM checkpoint", [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                conn.query_row("SELECT count(*) FROM history", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                4
            );
        }
    }

    #[test]
    fn checkpoint_crash_child() {
        let Ok(path) = std::env::var("TOR_CHECKPOINT_TEST_PATH") else {
            return;
        };
        let stage = std::env::var("TOR_CHECKPOINT_TEST_STAGE").unwrap();
        let path = Path::new(&path);
        let bytes = std::fs::read(path.with_extension("checkpoint")).unwrap();
        // A streaming fixture also passes its region rows and sequence.
        let (sequence, regions) = match std::fs::read(path.with_extension("rows")) {
            Ok(rows) => {
                type Rows = (u64, Vec<(u64, Vec<u8>)>, BTreeSet<u64>);
                let (sequence, rows, keep): Rows = serde_json::from_slice(&rows).unwrap();
                (sequence, Some(RegionWrite { rows, keep }))
            }
            Err(_) => (4, None),
        };
        let mut conn = connection(path).unwrap();
        conn.execute_batch("PRAGMA cache_size=1").unwrap();
        commit_batch(
            &mut conn,
            &[],
            Some((sequence, &bytes)),
            regions.as_ref(),
            |connection, at| {
                if at == stage {
                    connection.cache_flush().unwrap();
                    std::process::exit(81);
                }
                Ok(())
            },
        )
        .unwrap();
        panic!("crash stage was not reached");
    }

    #[test]
    fn process_death_at_each_checkpoint_transition_recovers_a_complete_prefix() {
        for stage in [
            "after_append",
            "after_regions",
            "after_checkpoint",
            "after_history",
            "after_rotation",
            "after_gc",
            "before_commit",
            "after_commit",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("crash.db");
            let (bytes, expected) = checkpoint_fixture(&path);
            let original_size = std::fs::metadata(&path).unwrap().len();
            std::fs::write(path.with_extension("checkpoint"), bytes).unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage::tests::checkpoint_crash_child",
                    "--nocapture",
                ])
                .env("TOR_CHECKPOINT_TEST_PATH", &path)
                .env("TOR_CHECKPOINT_TEST_STAGE", stage)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(81));
            if stage == "after_checkpoint" {
                let length = std::fs::metadata(&path).unwrap().len();
                assert!(
                    length > original_size,
                    "checkpoint must spill new pages before the crash"
                );
                // Model a torn extension while the hot journal still owns the
                // previous durable prefix. Recovery must precede alignment checks.
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(length - 1)
                    .unwrap();
            }
            let mut recovered = Engine::open(&path, Scenario::two_room(0)).unwrap();
            assert_eq!(recovered.state(ActorId(1)).unwrap(), expected, "{stage}");
            assert_eq!(
                recovered.recovery_profile().records_replayed,
                if stage == "after_commit" { 0 } else { 4 }
            );
            assert!(
                recovered
                    .command(
                        "player",
                        "test",
                        ActorId(1),
                        "0",
                        &recovered.branch().clone(),
                        crate::journal::Command::Act {
                            expected_revision: 0,
                            action: Action::Wait
                        }
                    )
                    .unwrap()
                    .duplicate
            );
        }
    }
    /// Steps from the corridor's start to the middle of hall 4.
    const TO_HALL_4: usize = 68;

    fn corridor() -> Scenario {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/streaming-corridor");
        let mut scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        scenario
    }

    fn walk(engine: &mut Engine, direction: tor_protocol::Direction, steps: usize) {
        for _ in 0..steps {
            let revision = engine.revision(ActorId(1)).unwrap();
            let request = format!("walk-{direction:?}-{revision}");
            engine
                .command(
                    "player",
                    "test",
                    ActorId(1),
                    &request,
                    &engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_revision: revision,
                        action: Action::Move { direction },
                    },
                )
                .unwrap();
        }
    }

    /// A saved corridor game with halls 1 and 2 detached and no checkpoint
    /// yet, and the checkpoint (with its two region rows) that would follow.
    fn streaming_fixture(path: &Path) -> (Vec<u8>, RegionWrite, tor_protocol::StateView) {
        let mut engine = Engine::open_with_policy(
            path,
            corridor(),
            SavePolicy {
                checkpoint_interval: 0,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        walk(&mut engine, tor_protocol::Direction::East, TO_HALL_4);
        assert_eq!(engine.region_counts().unwrap().detached, 2);
        engine.flush().unwrap();
        let (_, _, save_id, _, _) = load(path).unwrap();
        let checkpoint = Checkpoint::capture(&engine);
        let bytes = encode_checkpoint(&checkpoint.encode(&save_id, TO_HALL_4 as u64)).unwrap();
        let regions = RegionWrite::encode(&checkpoint).unwrap();
        assert_eq!(regions.rows.len(), 2);
        (bytes, regions, engine.state(ActorId(1)).unwrap())
    }

    /// Killing the process at any stage of a checkpoint that writes region
    /// rows leaves either the checkpoint with its rows or neither, and the
    /// game continues: detached halls come back from their rows, or from
    /// records replay makes again.
    #[test]
    fn process_death_while_writing_region_rows_recovers_a_complete_prefix() {
        for stage in [
            "after_append",
            "after_regions",
            "after_checkpoint",
            "after_history",
            "after_rotation",
            "after_gc",
            "before_commit",
            "after_commit",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("crash.db");
            let (bytes, regions, expected) = streaming_fixture(&path);
            std::fs::write(path.with_extension("checkpoint"), bytes).unwrap();
            std::fs::write(
                path.with_extension("rows"),
                serde_json::to_vec(&(TO_HALL_4 as u64, &regions.rows, &regions.keep)).unwrap(),
            )
            .unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage::tests::checkpoint_crash_child",
                    "--nocapture",
                ])
                .env("TOR_CHECKPOINT_TEST_PATH", &path)
                .env("TOR_CHECKPOINT_TEST_STAGE", stage)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(81), "{stage}");
            let committed = stage == "after_commit";
            let rows: i64 = connection(&path)
                .unwrap()
                .query_row("SELECT count(*) FROM regions", [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, if committed { 2 } else { 0 }, "{stage}");
            let mut recovered = Engine::open(&path, Scenario::two_room(0)).unwrap();
            assert_eq!(recovered.state(ActorId(1)).unwrap(), expected, "{stage}");
            assert_eq!(
                recovered.recovery_profile().records_replayed,
                if committed { 0 } else { TO_HALL_4 },
                "{stage}"
            );
            walk(&mut recovered, tor_protocol::Direction::West, TO_HALL_4);
            let counts = recovered.region_counts().unwrap();
            assert_eq!(counts.detached, 3, "{stage}");
            assert_eq!(
                counts.records_read,
                if committed { 2 } else { 0 },
                "{stage}"
            );
        }
    }

    /// A damaged or missing region row fails closed: the save still opens
    /// (rows aren't read then), the command that needs the row is rejected
    /// without changing the game, its history or the file, and the game can
    /// still be played up to that point.
    #[test]
    fn damaged_and_missing_region_rows_fail_closed_atomically() {
        for damage in ["flipped", "missing"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("damaged.db");
            let policy = SavePolicy {
                checkpoint_interval: 4,
                ..SavePolicy::default()
            };
            let mut engine = Engine::open_with_policy(&path, corridor(), policy.clone()).unwrap();
            walk(&mut engine, tor_protocol::Direction::East, TO_HALL_4);
            engine.flush().unwrap();
            drop(engine);
            let conn = connection(&path).unwrap();
            let (id, mut frame): (i64, Vec<u8>) = conn
                .query_row(
                    "SELECT record,frame FROM regions ORDER BY record",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            match damage {
                "flipped" => {
                    *frame.last_mut().unwrap() ^= 1;
                    conn.execute(
                        "UPDATE regions SET frame=?1 WHERE record=?2",
                        params![frame, id],
                    )
                    .unwrap();
                }
                _ => {
                    conn.execute("DELETE FROM regions WHERE record=?1", [id])
                        .unwrap();
                }
            }
            drop(conn);

            let mut engine = Engine::open_with_policy(&path, corridor(), policy.clone()).unwrap();
            let mut failure = None;
            for step in 0..TO_HALL_4 {
                let before = engine.state(ActorId(1)).unwrap();
                let history = engine.history(ActorId(1), "player", None, 100).unwrap();
                engine.flush().unwrap();
                let file = std::fs::read(&path).unwrap();
                let result = engine.command(
                    "player",
                    "test",
                    ActorId(1),
                    &format!("back-{step}"),
                    &engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_revision: before.revision,
                        action: Action::Move {
                            direction: tor_protocol::Direction::West,
                        },
                    },
                );
                if let Err(error) = result {
                    assert_eq!(
                        error.code,
                        tor_protocol::ErrorCode::StorageFailure,
                        "{damage}"
                    );
                    assert_eq!(engine.state(ActorId(1)).unwrap(), before, "{damage}");
                    assert_eq!(
                        engine.history(ActorId(1), "player", None, 100).unwrap(),
                        history,
                        "{damage}"
                    );
                    engine.flush().unwrap();
                    assert_eq!(std::fs::read(&path).unwrap(), file, "{damage}");
                    failure = Some(step);
                    break;
                }
            }
            assert!(
                failure.is_some(),
                "{damage}: the damaged row was never needed"
            );
        }
    }

    /// Reading a region row needs only a shared lock: it succeeds while
    /// another connection holds a write reservation, as the save worker does
    /// during a commit, without waiting for it.
    #[test]
    fn region_rows_can_be_read_during_another_connections_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reader.db");
        let mut engine = Engine::open_with_policy(
            &path,
            corridor(),
            SavePolicy {
                checkpoint_interval: 4,
                ..SavePolicy::default()
            },
        )
        .unwrap();
        walk(&mut engine, tor_protocol::Direction::East, TO_HALL_4);
        engine.flush().unwrap();
        let id: i64 = connection(&path)
            .unwrap()
            .query_row("SELECT min(record) FROM regions", [], |r| r.get(0))
            .unwrap();
        let writer = connection(&path).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let started = Instant::now();
        let record = engine
            .flush_handle()
            .unwrap()
            .read_region(tor_simulation::RecordId(id as u64));
        assert!(matches!(record, Ok(Some(_))), "{record:?}");
        assert!(started.elapsed() < Duration::from_millis(500));
        writer.execute_batch("ROLLBACK").unwrap();
    }

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
            "CREATE TABLE journal(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT; CREATE TABLE history(sequence INTEGER PRIMARY KEY,frame BLOB NOT NULL) STRICT;",
        )
        .unwrap();
        let mut batch = vec![Pending {
            sequence: 1,
            bytes: vec![1, 2, 3],
            queued: Instant::now(),
        }];
        commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
        commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM journal", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        batch[0].bytes[0] = 9;
        assert!(commit_batch(&mut conn, &batch, None, None, |_, _| Ok(())).is_err());
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
