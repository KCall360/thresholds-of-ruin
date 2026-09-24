# Phase A storage and Phase B proposal review

Historical review date: 2026-09-24, at the Phase A baseline commit. The writer
and proposal below are preserved as measurement/design evidence, not current
implementation instructions. The user subsequently authorized asynchronous
acknowledgements. [Background saving](background-saving.md) supersedes the
synchronous publication and separate base/journal installation proposal with
atomic SQLite batches. The old temporary-file fault tests were replaced with
current storage, contention, corruption, and real-process recovery tests.

## Phase A writer and verified recovery boundaries

The engine locks a canonical save path, builds a transactional candidate, creates
a temporary file in the save directory, streams JSON through a buffered writer,
flushes the writer, calls `sync_all`, replaces the destination, then publishes
the candidate. Diagnostics use the same writer. They acquire the save lock;
history seeding refuses attached engines. Fixture attachment persists before
ordinary commands can publish anything.

`storage_fault_tests.rs` injects failures before serialization, partway through
an actual underlying write, after flush, after file sync, before replacement,
after replacement, and immediately before publication. Before replacement, the
published state, receipt index, and original file bytes remain unchanged; both
same-process and restart retries execute exactly once. After replacement but
before publication, memory remains unchanged while restart recovers the new
receipt. Retrying its original request returns that receipt without another
transition. This is an uncertain outcome, not evidence that an error means the
command did not happen. The injected post-replacement tests restart immediately.

The integration recovery tests cover lost acknowledgement, conflicting retries,
truncated JSON, inconsistent recorded outcomes, and unsupported versions. Invalid
archives fail closed and remain unchanged on disk. These tests replace the toy
FNV scanner and value-selection checkpoint model. They do not implement journal
framing or checkpoint storage. Format 3 has no checksum: syntactically valid
corruption that preserves replay invariants is not generally detectable.

## Platform durability findings

**The current writer does not establish the stated power-loss acknowledgement
contract.** Successful process-restart tests establish a narrower property.

On Linux, syncing the temporary file does not persist the directory entry that
installs its new name. A same-directory atomic rename requires a subsequent
successful directory `fsync` before publishing an acknowledgement under the stated
contract. Creation of a new save directory also requires consideration of its
parent directory. Atomic name replacement and durable name replacement are
different properties. See the Linux project's [fsync manual](https://www.man7.org/linux/man-pages/man2/fsync.2.html).

The pinned `tempfile` 3.27.0 Windows implementation calls `MoveFileExW` with
`MOVEFILE_REPLACE_EXISTING`; it does not request `MOVEFILE_WRITE_THROUGH`. The
preceding file sync flushes data, but the present code supplies no explicit
replacement durability barrier. Microsoft documents [FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
and [MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
separately. The latter's write-through description specifically discusses a
copy/delete move; it must not be treated as a tested guarantee for every local
filesystem rename. `REPLACEFILE_WRITE_THROUGH` is explicitly unsupported in
[ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew).

Phase B must choose and verify a Windows filesystem/API contract for initial
journal installation; Phase C must do so for checkpoint installation and
rotation. Pre-existing append files avoid a name replacement on every action,
but do not solve first creation or rotation. Do not substitute an unsupported
flag, swallow barrier errors, or claim that a process kill simulates power loss.
The filesystem, device, and controller must honor completed durability barriers.
No directory/rename storage behavior is changed in Phase A.

## Reviewed append proposal

Retain the proposed 24-byte little-endian frame header:

`"TORJ" | format:u16=4 | kind:u16 | sequence:u64 | payload_len:u32 | crc32c:u32`

The CRC32C covers header bytes 4 through 19 followed by the payload. Use Castagnoli
CRC32C, reflected polynomial `0x82f63b78`, initial/final complement; test standard
vectors and byte flips in every protected header field and payload. Magic is
validated independently. Reject unknown kinds, versions, overflow, sequence gaps,
duplicate sequences, and payloads larger than 1 MiB before allocation. First
sequence is the selected checkpoint's sequence plus one. Sequence is global to
the save, including retained branches; it is not an actor revision or tick.

Kind 1 is a committed command/annotation record. Kind 2 is the monotonic wizard
lineage marker described below; all other kinds are rejected. This is a review
correction to the original record-only proposal: `enable_wizard` persists a
lineage change even when no command or annotation follows it.

The kind-1 payload is compact UTF-8 JSON without a BOM or trailing newline,
with exactly three required envelope fields: `save_id` (UUID string),
`generation` (u64), and `record`. Its shape is
`{"save_id":"<UUID>","generation":0,"record":{"entry":<HistoryEntry>,"receipt":<Receipt or null>}}`.
`record` freezes the serialized fields and enum tags of the current
[`Record`/`Receipt`](../crates/server/src/engine.rs) and
[`HistoryEntry`](../crates/server/src/journal.rs) types, including their nested
command, outcome, author and audience types. Both record keys are required;
backend notes use an explicit null receipt. Integer fields must fit their
declared Rust types; identifiers retain their existing validation rules.

Version-4 decoding must reject missing or unknown fields recursively, including
fields currently supplied by serde defaults in format 3. Writers emit those
fields explicitly. Reject records that exceed the frame limit before writing
or publishing. This preserves complete identity, actor, branch, tick, command,
request, disclosure and resulting outcome data; future schema changes require a
new version. CRC is over the exact stored bytes, so recovery does not depend on
JSON field order or canonical re-encoding. Save/generation identity prevents
splicing a valid frame from another file.

Kind 2 has exactly `save_id`, `generation`, and `wizard_game:true`. It consumes a
global sequence but no actor turn or request identity. Persist it before enabling
wizard administration; never permit a false value or clear the marker on rewind.
Replay applies this marker before any subsequent wizard records. Checkpoints
must retain it even when every later command is rewound. Test enabling wizard
mode followed immediately by interruption, without any gameplay command.

Before admitting the first command, install an immutable generation-zero replay
base containing format/ruleset, save ID, generation, sequence zero, scenario,
view salt, initial branch and initial wizard marker. Its receipt/branch history
starts empty and its initial game/navigation/rewind state is reconstructed by
the deterministic scenario constructor. Frame the base with the same length,
CRC32C coverage and 1 MiB payload limit, but distinct magic `TORB`, kind zero,
and sequence zero; a base frame is never valid inside the journal. Install and
durably name both this base
and the matching empty journal before acknowledging readiness. Incomplete or
inconsistent bootstrap pairs fail closed. This base is initial-save metadata;
periodic checkpointing and rotation remain Phase C work.

Validation and simulation precede encoding. Write the new frame, flush buffered
bytes, and sync the journal before publishing state or acknowledgement. A failed
write/sync leaves the candidate unpublished and does not consume its request in
memory. It does **not** prove the disk contains no complete frame. After uncertain
I/O, stop accepting writes until recovery has reconciled the file. Recovery
adopts a complete valid frame even if its acknowledgement was lost, establishes
its durability before making it available, and rebuilds the same receipt index.
The same request returns the original result; different content conflicts.

An incomplete or corrupt tail must not silently erase an acknowledged command.
In the live process, a known unacknowledged append can be rolled back to its
recorded starting offset only after successful truncation and sync. At restart,
if the available evidence cannot distinguish an uncommitted torn tail from
damage to durable data, **fail closed and preserve the file**. Automatic tail
repair requires an additional proven durable frontier/redundancy contract; it
is not implied by CRC or by reaching the end of a file. This conservative rule
is the reviewed starting proposal, with availability tradeoffs made explicit.

## Checkpoint content and future fault schedule

A checkpoint must contain the replay base and all authoritative deterministic
state, global sequence, save/generation identity, current branch, retained branch
metadata, receipt index, view salt, permanent wizard marker, navigation knowledge,
and the complete retained 128-boundary rewind window. Runtime control leases and
active travel remain ephemeral under the existing restart contract. Reject a
checkpoint that would silently drop receipts or abandoned futures.

Retain the complete history entries and receipt/result payloads needed by history
pagination, private-annotation filtering, and duplicate-request replies, including
abandoned branches. An index without its referenced payloads is insufficient.
Payloads may live in the checkpoint or in immutable history segments selected
durably with it; rotation must not delete their only recoverable copy. Compaction
may relocate retained history, but does not authorize discarding it.

Write a bounded, checksummed checkpoint candidate, flush/sync, install it with
the required platform name durability barrier, then advance the manifest or
generation selection, and only then retire the old journal. Retain the previous
recoverable generation until the new selection is durable. Select by verified
save ID, sequence, checksum, and generation; never just modification time or
the highest filename. A corrupt newest selection fails closed unless an older
generation plus retained journal can recover every acknowledged sequence.

The following schedules are acceptance requirements for later phases:

| Fault | Required proof |
| --- | --- |
| Before append / every byte cut through header and payload | Prior published state and receipt remain intact; recovery either proves a safe tail or fails closed |
| Complete frame before flush or before sync | No acknowledgement; reconcile any complete surviving frame before retry |
| Sync error / failure immediately after sync | Outcome is uncertain; prevent further writes until reconciliation |
| Publication or acknowledgement loss | Restart retry returns the same receipt, branch, and outcome exactly once |
| Corrupt header, length, sequence, checksum, payload, identity | Bounded parsing, explicit failure, retained evidence; no silent loss of durable records |
| Checkpoint partial write / flush / sync failure | Previous recoverable generation remains usable |
| Install before/after file and directory durability barriers | No old journal retirement before durable selection |
| Manifest update / journal rotation / deletion interruption | At least one complete acknowledged prefix and its receipts remain recoverable |
| Retry, rewind, retained branch, private annotation, wizard command at each boundary | Same identities, disclosure, wizard marking, and replay as ordinary execution |

These are schedules for real file/process tests when those storage operations
exist. An in-memory scanner cannot establish their filesystem guarantees.
