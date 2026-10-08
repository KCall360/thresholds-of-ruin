# Checkpoints and retained history

Checkpoints add periodic snapshots to background saving, so a restart only has
to simulate the actions since the latest snapshot. They preserve the
asynchronous acknowledgement and explicit-save contracts in
[background saving](background-saving.md).

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
remembered place names. Each
navigation instance holds ordered table references; equal region contents are
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

Each database has `journal`, `history`, `checkpoint` and `regions` tables. The initial base
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

The package index and copied region sources remain separate from base metadata.
Metadata decoding initializes no runtime cache, acquires no region file and
stores no diagnostic source text. Storage attaches the bounded index and lazy
source reader before the engine validates and exposes the recovered scenario.
The stored index uses the same strict JSON checks as other save payloads,
including duplicate keys and canonical field presence, then requires unique
increasing region identities. The generated-region flag retains its existing
omission when false. These mappings preserve save format 22 and current writer
shapes; they add no old-format importer.

## Portable export contract (design)

Portable export is a future operation separate from live storage and ordinary
recovery. Its versioned envelope should describe a self-contained, consistent
durable save boundary and a manifest of content identities and digests. The
payload must preserve the checkpoint, replay tail, retained history and branches,
receipt identities, suspended intentions and identity high-water marks together.
Export must not acknowledge or discard an unsaved tail implicitly: first obtain
an explicit durable boundary, then capture a consistent read snapshot without
holding the engine lock during packaging.

The content manifest must cover the pinned scenario definitions, index and every
region source needed for future activation, including unbuilt regions. Export
may acquire and verify those sources explicitly; ordinary restart remains lazy.
Package-directory paths are locators and must not become portable identities.
Missing or mismatched resources fail the export, leaving the original save
unchanged. Connection credentials, observer stream contexts, derived topology
indexes and runtime caches are excluded; save-owned identity state is retained.

A future import should validate supported save, rules, authoring and envelope
versions independently, bound both encoded and expanded resources, reject
duplicate identities and unsafe paths, verify digests, and run the same strict
save/recovery invariants before making the result available. It should create a
new destination atomically and preserve existing saves. Compression, container
layout and export/import commands remain undecided and unimplemented. Future
extension state must use the typed ownership and deterministic scheduling
contracts in the [refactor plan](refactoring.md#future-scenario-extension-contracts).

## Recovery and limits

Loading checks the format, SQLite integrity of the journal, history and
checkpoint tables, contiguous frame sequence, row
placement on the correct side of the checkpoint boundary, checksums, save identity,
checkpoint record count, current ruleset and snapshot structure. Checkpoint JSON
is capped at 64 MiB both during writing and before allocation on reading, with
CRC32C covering the payload including save identity and sequence. Unknown/missing
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

Queue snapshots retain their identity counter and ordered references to complete
intention values in a shared pool. Queued and suspended states, movement guards,
origins and continuation targets participate in equality. The native identity
only narrows lookup candidates; it never substitutes for complete-value equality.
Restore rejects missing, repeated or unordered references and oversized queues,
then applies the ordinary game invariants. Identical restored queues share
copy-on-write storage. A regression exercises 256 boundaries at the full 4,096
intention capacity; production save/reopen coverage checks both retained windows,
original admission retries and execution/cancellation after restore.

A local stress fixture with 768 admitted actors and 256 retained boundaries
encoded 96,339,453 bytes before actor-table pooling, 39,399,690 with actor pooling,
and 13,320,863 with both actor and intention pooling. It used ordinary admission,
same-location wizard markers and cancellation/readmission, without advancing
simulation time. These are absolute fixture diagnostics, not representative
gameplay latencies or an arbitrary-world guarantee.

The production writer installed a 13,303,926-byte checkpoint after the next
marker, retaining 255 boundaries because that selectable marker also occupies
the private window. Reopen replayed zero suffix records, preserved all 768 actor
observations and permitted cancellation under the original admission. Sampled
peak private memory for the complete setup, encoding, installation and reopen
run was 759,328,768 bytes. Before intention pooling, isolated opening of the
installed 39,280,956-byte checkpoint exceeded the diagnostic's 1 GiB guard and
was stopped. Its partial measurement is not a successful restore or a final peak.
Restore memory remains substantial and requires further profiling. The separate
bootstrap-save probe writes archive records; it does not prove checkpoint
installation. The production regression captures on a private queue change to
exercise the full 256-boundary union, original retries and resumed execution.

A snapshot exceeding the size limit fails saving rather than publishing an
unrecoverable checkpoint. The limit bounds encoded bytes, not all in-memory
snapshot allocations. Before navigation was pooled by region, a fully explored
256-region fixture needed a 765 MB checkpoint; pooling reduced it to 9.9 MB.
Regression coverage requires that complete fixture to fit below 16 MiB, with the
production 64 MiB cap unchanged. This is a measured fixture
bound, not an arbitrary-world guarantee or region streaming implementation.
SQLite/process
tests establish transaction recovery, not hardware power-loss guarantees; filesystem/device limitations from the background-save guide apply.

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
startup loaded/replayed counts; see the [performance harness](performance-harness.md).

In the largest measured case (256 regions, 8 actors, 10,000 retained actions),
the default interval cut restart time from 119 s to 7 s with no action-latency
regression. Results for later work are in the
[performance plan](performance-persistence.md#what-each-phase-achieved).
