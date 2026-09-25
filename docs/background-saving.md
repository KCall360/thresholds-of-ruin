# Background saving

Ordinary acknowledgements mean that the server accepted an action in memory.
They do not wait for disk. A crash or forced termination can lose recent actions,
notes, branches, and their receipts together. Restart restores the last committed
batch as one consistent prefix. A receipt that survives still prevents duplicate
execution; a request lost with the unsaved tail may execute again after restart.

## Policy and controls

The server exposes these startup options (ages in milliseconds, queue in bytes):

| Option | Default | Meaning |
| --- | --- | --- |
| `--save-target-ms` | 30000 | Prefer saving once the oldest pending record reaches this age |
| `--save-max-ms` | 60000 | Start saving even if activity continues |
| `--save-idle-ms` | 750 | Quiet time since the last accepted record before a target-age save |
| `--checkpoint-interval` | 1024 | Accepted journal entries between checkpoint captures; 0 disables |
| `--save-queue-bytes` | 8388608 | Bound encoded pending data, including the batch being written |

Saving also starts at 75% queue capacity. This uses activity and queue pressure as
cheap resource-availability signals; it does not sample OS-wide CPU or disk load.
One worker owns disk I/O outside the engine/session lock. Maximum age is a
scheduling deadline, not a promise that slow or failed storage will finish on time.
The server broadcasts a warning when saving is overdue or fails.

The maximum age must be at least the positive target, and at most one day. Idle
time must not exceed maximum age. Queue capacity is between one byte and 1 GiB;
it must be large enough for an individual record to be accepted.

`save` in the text command interface, or protocol `{"type":"save"}`, waits for
all records accepted before that request. Normal exit from all player clients
does the same before disconnecting; spectator exit has no write effect. Client
exit reports failure after a failed save or a 30-second timeout. Server Ctrl+C
stops admissions and waits for pending records. Closing a console forcibly,
killing a process, or losing power is not a graceful shutdown.

Save requests require an attached non-spectator account, but no control lease.
A controller's save request cancels its active travel at an action boundary.
Other players can continue acting during a save; those later actions need not be
covered by its acknowledgement. There is at most one pending save per connection.

A full queue rejects the new command before publication and prompts the worker to
save the existing prefix. A background I/O failure preserves accepted state and
pending bytes, reports the problem, and rejects further mutations. An explicit
save retries the pending batch. Reads and already accepted duplicate requests
remain available. No error is reported as a successful save.

Enabling wizard authority remains a synchronous barrier: its permanent marker
must be saved before privileged commands can execute. Once queued, the lineage
remains marked even if the first flush fails; authority stays disabled until a
successful retry. Rewind cannot clear the marker.

## Format 6

The format-6 compatibility decision rejects format 5, as explicitly authorized
for this pre-release checkpoint change. Protocol 12 and `diagonal-v11` are unchanged.
Only the current format is supported; old JSON and format-4/5 SQLite saves are rejected without an
importer. The filename extension is immaterial. The database uses bundled SQLite
through `rusqlite`, confined to the server crate. The `journal` table holds the
immutable replay base at sequence zero and the active tail; `history` holds records
covered by the selected checkpoint. Global sequence numbers remain contiguous
across both tables. SQLite transactions commit complete batches. The database uses
`journal_mode=DELETE`, `synchronous=EXTRA`, application ID `0x544f524a`, and
`user_version=6`. SQLite's temporary rollback journal is part of transaction
recovery and must not be manually deleted after a crash.

Each row contains a 24-byte little-endian frame header:

`magic | format:u16=6 | kind:u16 | sequence:u64 | payload_len:u32 | crc32c:u32`

The base uses `TORB`, kind 0, sequence 0. Records use `TORJ`, kind 1; the permanent
wizard marker uses kind 2. CRC32C covers header bytes 4–19 and the payload, with
the standard Castagnoli polynomial and initial/final complement. Payloads are
limited to 1 MiB. Loading bounds the row length before allocating its frame.

Compact JSON payloads carry a save UUID and generation zero. The base contains
the current archive metadata, scenario, view salt, root branch, initial wizard
flag, and an empty record list. Command records retain the complete history entry
and receipt (explicit null for backend notes); markers contain `wizard_game:true`.
Unknown/missing fields, duplicate keys, versions, kinds, gaps, identity mismatches,
bad checksums, and inconsistent deterministic replay fail closed. Committed
corruption is not silently truncated. An uncertain commit is retried using exact
sequence/byte reconciliation, never by executing gameplay again.

SQLite performs transaction recovery before frame validation. Its
[atomic commit design](https://www.sqlite.org/atomiccommit.html) and
[EXTRA synchronization](https://www.sqlite.org/pragma.html#pragma_synchronous)
provide the platform storage barriers, including rollback-journal directory
synchronization where supported. This depends on a local filesystem and device
that honor locking and flush requests. Process-kill tests do not prove hardware
power-loss behavior; newly created parent directories have no additional
application-level durability barrier. Do not claim stronger bootstrap guarantees.

The application never rewrites prior records during normal saves. Database page
and rollback-journal writes are SQLite's responsibility and are not equal to
encoded frame bytes. Periodic [checkpoints and logical compaction](checkpoints.md)
move covered records unchanged into retained history and replace the current
snapshot atomically. Startup restores the checkpoint and simulates only its tail;
all history remains readable and retryable. Command transactions retain history in
place and publish only their new record and decision state after queue admission.

## Verification

Storage tests cover frame corruption, strict decoding, queue admission, durable
save barriers, retry after a database lock failure, and wizard marking. Existing
history tests cover private annotations, retained branches, conflicting receipts,
and deterministic replay. Actual-process tests cover acknowledged-tail loss,
deadline saves, nonblocking gameplay during a locked writer, normal client exit,
and process termination before/after a SQLite commit. Server shutdown is tested
while the service still owns the engine, so an implicit destructor cannot mask
a missing shutdown barrier.

See the [focused measurement plan](performance-persistence.md#phase-b-verification-and-measurement)
and [harness guide](performance-harness.md) for performance boundaries.
