# Persistence and recovery

Ordinary acknowledgements mean that the server accepted an action in memory.
They do not wait for disk. A crash or forced termination can lose recent actions,
notes, branches, and their receipts together. Restart restores the last committed
batch as one consistent prefix. A receipt that survives still prevents duplicate
execution; a request lost with the unsaved tail may execute again after restart.

## Player advancement transactions

Player purchase and timed replacement commands persist independent named numeric
operands and the original actor/branch/stream/reset/build-epoch basis. Receipt lookup
keeps its existing authenticated duplicate precedence, while original admission
and lifecycle ancestry validate accepted queued or paused work. Saved state has
one permanent point ledger and one physical item-owned finite-hold authority.

Replacement completion validates the current candidate and live safety, spends
the held ration, then finalizes and publishes the actor through the shared death
boundary. The ordinary Store transaction admits all of these effects together;
a late Store refusal publishes no build change, epoch, ration deletion, corpse,
event or receipt. Retrying a successful persisted request returns its receipt
without another replacement or ration. A deleted last ration cannot reappear in
the corpse inventory. Fresh saved fields are required; old logical formats do not
acquire invented default epochs or advancement policy.


## Admission, durability and save policy

The [CLI reference](../reference/cli.md) owns save-policy flags and defaults. The
policy combines oldest-record age, quiet time and queue pressure. Queue accounting
includes encoded journal records, copied region sources and the batch being written.
It is not a strict bound on total pending memory: when the queue is empty, copied
region sources may exceed `--save-queue-bytes`, provided the journal record itself
fits. This permits first acquisition of a large source without permanently blocking
play; the worker is prompted to drain the admitted prefix. Checkpoint and other
runtime allocations have separate bounds.

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
Saving does not cancel active travel. Cancellation is a separate command.
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

The engine and background worker share one exclusive journal lease. Its final
owner explicitly unlocks the sidecar file after the worker finishes. A copied
file descriptor cannot extend ownership beyond shutdown; an immediate reopen
can acquire the journal while a second live owner remains excluded.

## Save file format

Only the current save format is supported; older saves are rejected without an
importer. The filename extension is immaterial. The database uses bundled SQLite
through `rusqlite`, confined to the server crate. The `journal` table holds the
immutable replay base at sequence zero and the active tail; `history` holds records
covered by the selected checkpoint. Global sequence numbers remain contiguous
across both tables. `regions` holds the records of detached regions that a
checkpoint refers to; see [region streaming](streaming.md#persistence).
The `package` table stores the chunked compiled region index. `region_sources`
contains versioned compiled region inputs, despite its historical table name;
it never requires the authoring TOML parser. The initial metadata pins canonical
resolved rules and catalogs, and procedural descriptors remain compact. The
[compiled-content contract](../reference/scenario-packages.md#saves-and-the-package)
describes immutable external chunks, portable export and bounded development
fallback capture. Source provenance hashes are distinct from semantic identity.

SQLite transactions commit complete batches. The database uses
`journal_mode=DELETE`, `synchronous=EXTRA`, application ID `0x544f524a`, and a
`user_version` equal to the current save format. SQLite's temporary rollback journal is part of transaction
recovery and must not be manually deleted after a crash.

Each framed journal, history or region-record row contains a 24-byte little-endian
header; checkpoint, package-index and source tables have separate payloads:

`magic | frame_format:u16=6 | kind:u16 | sequence:u64 | payload_len:u32 | crc32c:u32`

The frame layout has its own version (6), independent of the save format.
Ordered strike plans, mutable sequence cursors, primary/rider outcomes and optional
cancel steps change the strict logical payload admitted by the archive and SQLite
save revision. They do not change this physical frame, its checksums or bounds,
or the checkpoint compression container. An unchanged outer frame never grants
permission to decode an obsolete logical payload.

The base uses `TORB`, kind 0, sequence 0. Records use `TORJ`, kind 1; the
permanent wizard marker uses kind 2. Region record rows use `TORR`, kind 3,
with the record identity in the sequence field. CRC32C covers header bytes
4–19 and the payload, with the standard Castagnoli polynomial and
initial/final complement. Payloads are limited to 1 MiB, or 16 MiB for a
region record. Loading bounds the row length before allocating its frame.

Compact JSON payloads carry a save UUID and generation zero. The base contains
the current archive metadata, scenario, view salt, root branch, initial wizard
flag, and an empty record list. Command records retain the complete history entry
and receipt (explicit null for backend notes); markers contain `wizard_game:true`.
Unknown/missing fields, duplicate keys, versions, kinds, gaps, identity mismatches,
bad checksums, and inconsistent deterministic replay fail closed. Committed
corruption is not silently truncated. An uncertain commit is retried using exact
sequence/byte reconciliation, never by executing gameplay again.

Entry population retains private exact receipts alongside ordinary journal data.
The archive base carries initial-entry decisions even with no journal records;
forward records carry only the appended authoritative ledger range. Strict replay
compares those ranges, and checkpoint-only restore audits the branch-aware ledger
against the base and journal. Historical placement identities remain valid after
entities move, die, merge or disappear. Rewind restores decisions with entities;
control-only restoration is not a new entry. These receipts are never part of a
client journal or observation.

Recovery also compares immutable population profiles, selected-character policy,
seed and prepared scope geometry against pinned compiled package inputs. Original
scope geometry is checked independently of later mutable terrain changes and
reused across retained boundary validation. Every historical population origin
must be above the package's authored identity reservations, including omitted
character presets, and outside the selected starting kit's item range. These
compiler-derived lower bounds complement the retained allocation high-water
marks without requiring a historical entity to remain alive or owned.
The audit also rejects population identities that alias explicit actor/item
births in retained wizard results, including across abandoned branches. Transfer
results can describe a merge rather than a new allocation, and death events do
not record the corpse's item identity, so neither is treated as independent birth
evidence. These checks validate retained structural and semantic facts; they do
not authenticate a save or reconstruct origins for which no evidence remains.
Nested actor birth-stock receipts retain the chosen ordinary item identity,
content and quantity, or an explicit chance miss. The complete pool map, resolved
item specs and nullable actor policy remain pinned in the population plan and
independent compiled/save adapters. Replay compares exact nested outcomes;
checkpoint audits recompute them and include their item origins in reservations,
all-branch uniqueness and high-water bounds, including expired abandoned receipts.
Consumed or merged origin identities need not remain live. Rewinding before a
birth removes its entities and receipt; re-entry preserves deterministic choices
but allocates new identities above abandoned-future ceilings.
Initial selected-character loadouts
are ordinary carried/equipped items created once during base construction; loading
saved item state does not grant another kit. Store rejection discards entry,
population, allocation and revision changes together with the candidate.


Unique retrieval keeps its small mutable identity/counter state separate from
pooled immutable population data. Each accepted transaction carries its exact
private retrieval batch and required before/after summary; initial creation is
also retained when there are no forward records. Replay compares actual outcomes.
Checkpoint history checks both the receipt fold and independent boundary summaries,
including abandoned histories, rather than treating the absence of a receipt as
proof that no creation occurred. Ordinary item ownership is the location authority;
the objective does not retain a second inventory or a depth-sized lost-item ledger.
Shared origin audits include objective, population and explicit authored births.

Every private record has required before/after simulation clocks. Branch-aware
audits bind entry ticks to that interval before discarding abandoned futures;
retained checkpoint boundaries, rewind targets and the final game tick agree.
Annotations cannot advance the clock. Public history keeps its existing event
clock semantics. Rewinding before Amulet creation may create a new identity in
the new branch, above the allocation high-water mark, but never restores abandoned
ownership or creates duplicate active Amulets. Restore itself is not floor entry.

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
encoded frame bytes. Periodic [checkpoints and logical compaction](persistence.md)
move covered records unchanged into retained history and replace the current
snapshot atomically. Startup restores the checkpoint and simulates only its tail;
all history remains readable and retryable. Command transactions retain history in
place and publish only their new record and decision state after queue admission.

## Save verification

Storage tests cover frame corruption, strict decoding, queue admission, durable
save barriers, retry after a database lock failure, and wizard marking. Existing
history tests cover private annotations, retained branches, conflicting receipts,
and deterministic replay. Actual-process tests cover acknowledged-tail loss,
deadline saves, nonblocking gameplay during a locked writer, normal client exit,
and process termination before/after a SQLite commit. Server shutdown is tested
while the service still owns the engine, so an implicit destructor cannot mask
a missing shutdown barrier.

See the [performance plan](../development/performance.md) and
[harness guide](../development/performance.md) for performance boundaries.

## Capture and scheduling

`--checkpoint-interval 1024` requests a snapshot after that many accepted journal
entries since the last capture. Zero disables captures for diagnostic comparisons;
the maximum is 1,000,000. Captures do not trigger disk writes: the ordinary save
policy still chooses batch timing. A durable save covers any captured checkpoint
in its batch. An unsaved snapshot may disappear with an unsaved tail after a crash.

Capture copies the current game and revision map and shares the existing immutable
rewind boundaries. It does not clone the complete history or receipt index. One
snapshot may be encoding and one newer snapshot may be pending; newer pending
captures replace older ones. Encoding, checksum calculation, world/navigation deduplication,
and database work happen in the storage worker outside the engine/session lock.
Snapshots can still consume CPU and storage bandwidth while gameplay proceeds;
profiling tracks their effect rather than assuming background work is free.

The snapshot contains the current simulation and scheduler state, navigation
knowledge, identities, current branch, revisions, permanent wizard flag and all
retained boundaries. The server keeps the union of the latest 128 selectable
gameplay states and the latest 128 raw transaction states, at most 256 shared
boundaries. Private admission and queue-control records cannot shorten the
selectable rewind window. Selectability is derived from journal content on
restore; it is not trusted from a saved flag. Restore checks the exact retention
set and replays private queue transitions across gaps between retained states,
including their recorded timing and terminal facts. The two windows are derived
in one pass through the archive. Identical worlds and item maps across those boundaries are encoded once.
Navigation cells and edges are pooled independently by source region, and so are
remembered place names. Their checkpoint codec is unchanged by the separate
explored-map compaction.
Explored appearances and bounded witnessed geometry are likewise actor-owned,
region-shared checkpoint data. Changed sightings update their owning region;
identical sightings do not create history entries. Acquisition is part of the
ordinary candidate transaction before rewind capture and storage admission.
Read-only snapshots, spectators and hydration never add knowledge. Saved map
values contain last-seen appearance rather than current entity handles, and
restoration never refreshes them from unseen terrain or items.

The private explored-map codec stores exact appearances in one sorted, unique
palette. Each cell bucket names its region once; ordered x/y columns contain
explicit z/appearance-reference pairs, with one pair per remembered cell. Plain
same-region, zero-rotation neighbors use explicit z/mask pairs in the same column
shape. Masks from 1 through 63 encode only the six witnessed axial neighbors;
checked coordinate addition reconstructs their targets. Exceptional links and
ambiguous evidence retain their full explicit records. Witness and child tables
remain explicit and cross-validated. No run, inferred cell or current-world lookup
fills missing knowledge. Equal actor-to-map-instance owner directories are pooled
across snapshots without merging differing owners or historical map values.
The reader rejects malformed columns, invalid or noncanonical palette references,
zero/out-of-range masks, coordinate overflow and overlapping plain/exceptional
evidence. See the [capacity evidence](../development/performance.md#explored-map-checkpoint-capacity)
for passing bounded production size/reopen checks and their single-run timing
limits; these do not establish general responsiveness.

Each navigation instance holds ordered table references; equal region contents are
stored once even when ownership differs. Reference decoding shares the maps
without expanding repeated cell/edge payloads. Empty or mixed-region table
entries, invalid/duplicate/out-of-order references and malformed fields fail closed.
This preserves historical changes and deletions rather than merging knowledge
from different rewind boundaries. Item specs, quantities, character identity
knowledge, and allocation counters are included. Rewind restores entity state
while retaining the highest actor, item, door, record and intention counters
from the abandoned future. Existing entities keep their identities; new entities
and work cannot reuse them. Checkpoints preserve these counters through restart.
World geometry is shared independently of door state, so opening
a door does not duplicate the entire dungeon. Runtime control leases, client-held map memory and active travel
jobs retain their existing restart behavior and are not restored from this snapshot.
Backend snapshot types never cross the client protocol boundary.

## Atomic selection and compaction

The checkpoint transaction uses `journal`, `history`, `checkpoint` and `regions`.
Package-index and copied-source tables separately retain scenario inputs. The initial base
remains at journal sequence zero. One transaction performs these steps:

1. Append the admitted records, reconciling exact bytes for any uncertain retry.
2. Write the region record rows the checkpoint refers to that aren't stored
   yet, reconciling exact bytes likewise.
3. Replace the selected checkpoint at slot 1.
4. Copy covered nonzero journal rows into retained history without changing bytes.
5. Delete those covered rows from the active journal.
6. Delete region record rows the new checkpoint doesn't refer to, and commit.

Until commit, SQLite rollback recovery preserves the previous selection and rows.
After commit, the new snapshot and its history are selected together. There is no
separate manifest rename, generation file deletion, or interval where the only
recoverable copy of a receipt has been removed. Errors preserve the pending batch
and snapshot for explicit retry and block further mutation under the save contract.

Compaction is logical: only one selected checkpoint and the active replay tail
remain, with obsolete database pages available for reuse. All chronological history,
private annotations, retry payloads and abandoned futures remain retained. The
database therefore still grows with retained history; checkpointing does not delete
history or run a blocking `VACUUM` to shrink the file.

## Stored scenario ownership

The archive's scenario and the separate package-index table use save-owned
schemas with explicit mappings to runtime values. These schemas cover fixture
actors and coordinates, streaming settings, package metadata, nested manifest
definitions, certificates and indexed regions. Encoding borrows definitions;
decoding moves fields into the runtime and builds each container once. Authoring
and runtime serde changes therefore cannot silently redefine the stored fields.

The package index and copied compiled region inputs remain separate from base metadata.
Metadata decoding initializes no runtime cache, acquires no region file and
stores no diagnostic source text. Storage attaches the bounded index and lazy
compiled-input reader before the engine validates and exposes the recovered scenario.
The stored index uses the same strict JSON checks as other save payloads,
including duplicate keys and canonical field presence, then requires unique
increasing region identities. The generated-region flag retains its existing
omission when false. Rules use a versioned, canonical resolved-document envelope rather than source
TOML or derived runtime caches. Its document vocabulary is an explicit save
contract: changing that vocabulary requires version and golden-fixture review.
Procedural descriptors remain compact; geometry and mutable region records are
materialized only when needed. These mappings add no old-format importer.

## Portable saved-game export remains a proposal

The implemented `tor-scenario export` command exports immutable compiled content,
not a played game. A self-contained saved-game export/import operation is not implemented. Its required durable
boundary, content manifest, validation and atomic-destination guarantees belong to
[future requirements](../plans/requirements.md#portable-save-export-unimplemented-design), not the live save contract.

## Recovery and limits

Loading checks the format, SQLite integrity of the journal, history and
checkpoint tables, contiguous frame sequence, row
placement on the correct side of the checkpoint boundary, checksums, save identity,
checkpoint record count, current ruleset and snapshot structure. The checkpoint
storage payload has an eight-byte `TORC | version:u16=1 | reserved:u16=0`
header, one zlib stream of JSON, and a four-byte little-endian decoded length.
JSON is compressed into the bounded writer through a fixed 64 KiB input buffer;
no second JSON payload is allocated. Batching avoids a compression call for each
small JSON token, and draining the buffer propagates write errors. Both stored bytes and decoded JSON retain the 64 MiB cap. Loading
validates the header and length before decoding, limits expansion to the declared
size, and rejects truncation, trailing bytes and extra streams. CRC32C covers the
complete storage envelope, including save identity and sequence in its JSON.
The checkpoint's logical snapshot schema remains independent of this envelope.
Unknown/missing
fields and duplicate keys are rejected. A corrupt selected checkpoint fails
closed; silently falling back could discard a saved prefix.

Retained records rebuild the existing history and receipt index without executing
the commands again. Only the suffix after the checkpoint runs through strict
deterministic replay. This bounds **simulation replay** by the capture interval
for ordinary continued play once a checkpoint is committed. Startup is not constant
in total history: frame validation, history loading and receipt rebuilding remain
linear in retained records. Suffix replay uses the same bounded transaction candidates and shared state
as ordinary actions. Disabling captures, increasing the interval, or waiting to persist a new
capture changes the effective replay bound.

Equal actor tables are encoded once in the shared checkpoint pool and referenced
by each boundary. Equality compares the complete actor values, including body,
combat, timing and motion; actor identity alone never permits sharing distinct
states. Restore rebuilds the derived topology indexes and shares unchanged actor
stores with copy-on-write isolation. Invalid pool references fail closed. The
retained window and 64 MiB writer/reader limits are unchanged.

Required nutrition retains independent digestion and cross-resource regeneration
fractions. Immutable pinned rules store explicit per-resource numerators and their
shared denominator through save-owned adapters. Restore validates both fractions;
dead actors retain neither. Recovery gains and nutrition expenditure share the
ordinary candidate transaction, so checkpoints, replay, reconnect and rollback
cannot repeat an already settled charge. Transient gross-regeneration diagnostics
are neither accumulated save totals nor public protocol fields.

The checkpoint writer interns complete equal population states: each snapshot's
optional `floor_activation` is an index into the shared `floor_activations` table.
Equality includes plans, prepared placement scopes, member indexes, floor
receipts, receipt order and decision counts. Changed population state gets a
distinct entry; sharing does not merge decisions from different boundaries.
Prepared actor recipes include their required explicit nutrition enrollment.
Required and exempt recipes never share a pooled value merely because their
other fields match. Recipes contain no live reserve; each birth starts nutrition
through ordinary actor construction at its own activation boundary.
Games without population omit both the reference and the empty table. The
current reader expects pooled references.

Restored population states share the decoded immutable stores with copy-on-write
isolation. Every boundary still validates receipts against its own tick and actor
and item identity ceilings; shared storage does not cache those checks across
boundaries. Invalid references and malformed referenced states fail closed.
Decoding does not require unique or referenced-only pool rows; semantic audits
apply to the restored states, and the codec size bound applies to the whole pool.
The host's pinned-package and branch-ledger audits above also remain required.
The [bounded population regression](../development/performance.md#population-checkpoint-size-regression)
records size and exact-restore evidence, not a recovery-latency claim.

Queue snapshots retain their identity counter and ordered references to complete
intention values in a shared pool. Queued and suspended states, movement guards,
origins and continuation targets participate in equality. The native identity
only narrows lookup candidates; it never substitutes for complete-value equality.
Restore rejects missing, repeated or unordered references and oversized queues,
then applies the ordinary game invariants. Identical restored queues share
copy-on-write storage. A regression exercises 256 boundaries at the full 4,096
intention capacity; production save/reopen coverage checks both retained windows,
original admission retries and execution/cancellation after restore.

A snapshot exceeding the size limit fails saving rather than publishing an
unrecoverable checkpoint. The limit bounds encoded bytes, not all in-memory
snapshot allocations. The complete explored 256-region diagnostic fixture must fit below 16 MiB,
with the production 64 MiB cap unchanged. This is a measured fixture bound, not
an arbitrary-world guarantee. Historical pooling and restore-memory evidence is
in the [performance guide](../development/performance.md#persistence-evidence).
SQLite/process tests establish transaction recovery, not hardware power-loss
guarantees; the filesystem/device limitations above still apply.

## Ordered ability preparation records

An ability checkpoint stores one immutable value-serialized shared plan, bounded
frozen strikes/riders and primary-branch triggers, authored origins, structural check ownership and explicit
token-site claims. Required `PreparationProgress` stores the next enabled strike,
a 16-bit successful-primary prefix and two 64-bit source-qualified rider-attempt
masks outside that plan. A paid prefix must have no cost reservation; pre-first-strike
resource costs retain their original valid hold and intention lineage. Restore rejects
wrong item/ability tags, out-of-range cursors, impossible current-gap progress,
invalid source shapes or changed rider triggers, duplicate claims, bad check-site ownership and overflowing
remaining commitments. Admission does not re-derive captured numbers from today's
stats, conditions or equipment. Natural provenance names the captured source in
the pinned catalog; it is not replaced or compared with a later same-anatomy
transformation's current species. Checkpoint admission validates captured structure
and values. Unlock eligibility controls new capture, not admission of a previously
captured row: an owned talent may become dormant after preparation without
changing that immutable list, while loss of the whole ability remains a live
lifecycle invalidation. Exact historical capture is a deterministic replay invariant.

Progress admission rejects future or structurally impossible hits, noncanonical or
nonexistent rider slots, and receipts without a completed eligible occurrence.
Missing eligible receipts are invalid when a later hit proves target survival or
the loaded committed target is alive. A dead or unmaterialized target can retain a
missing receipt at its last qualifying hit: the primary or an earlier rider may
have killed it. Checkpoints cannot reconstruct that discarded history; strict replay
remains authoritative. Region attachment rechecks the live-target invariant before
publishing detached state. The current decoder requires every progress field and
every nullable frozen rider condition explicitly, without compatibility defaults.
Independent compiled source/rules riders also require the explicit nullable gate
and named fields; positional aliases, repeated fields and unknown predicates reject.

Per-strike events are distinct from sequence termination. Journal-only replay and
checkpoint tails must preserve completed prefixes without rerolling or repaying
them. Cancel-and-step is one recorded intention transition using independent stored
direction DTOs and normal command context/receipt identity. A rejected step records
no partial movement, cancellation or queue retirement. Current-format readers do
not default missing cursor, strike, check-evidence or nullable-step fields.

## Verification and profiling

`checkpoints.rs` covers bounded replay, preserved history and duplicate/conflicting
requests, private annotations, retained branches, expired and retained rewind
targets, and fail-closed corruption. Storage tests inject errors and terminate
real child processes after append, checkpoint installation, history retention,
rotation, before commit and after commit. They verify recovery and uncertain-commit
retry. A one-page SQLite cache forces dirty-page spill in the crash tests; one case
truncates an uncommitted extension to verify that hot-journal recovery runs before
database page-alignment validation.

`scripts/test_checkpoint_process.py` runs real headless, text and native ASCII
clients across checkpoint saves and restart, including loss of an acknowledged
unsaved tail. The performance harness exposes `checkpoint_capture` separately from
record encoding, worker checkpoint size/encoding time, selected sequence and
startup loaded/replayed counts; see the [performance harness](../development/performance.md).

Paid public-plan records and resolved participation evidence use the current save
schema. Restoring a checkpoint validates their bounded shapes, historical address
bounds and checkable result/recipient consistency. It does not replay every
historical resolution or authenticate a coherently edited alternative history.
Pending plans still validate against their frozen source contract. See
[simulation disclosure](simulation.md) for attempted participation and the
single-event summary of actually published riders.
