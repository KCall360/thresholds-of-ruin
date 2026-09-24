# Performance and scalable persistence plan

## Purpose

This milestone makes responsiveness and scale explicit engineering constraints
before dungeon gameplay expands the world, entity count, and action history. It
does not weaken deterministic simulation, actor-specific disclosure, idempotent
requests, or wizard rewind.

Early measurements identified whole-archive persistence and state copying as
likely bottlenecks. Phase A remains incomplete: the current harness mostly waits
and does not measure representative boundary, door, elevation, or mixed action
traces. Follow the [harness completion plan and session handoff](performance-harness.md)
before treating those observations as a complete baseline or beginning Phase B.

## Recorded decisions and guiding criteria

The milestone uses these product decisions:

1. **Acknowledgement durability is mandatory.** An accepted action is not
   acknowledged or presented as committed until it is durably recoverable after
   an OS or power failure. Append and group-commit implementations are allowed,
   but every affected acknowledgement waits for the corresponding durable flush.
2. **Old saves are intentionally unsupported.** This is a pre-release project,
   so the new persistence layout will advance the save format and reject older
   formats. There will be no format-3 importer or compatibility reader. Tests,
   fixtures, documentation, and the sample saves move to the new format together.
3. **Latency targets are provisional.** Initial release-build goals are p95 below
   8 ms and maximum below 33 ms for an ordinary locally saved action, with no
   meaningful upward trend between 100 and 10,000 retained actions. Measurement
   may show that these limits should be tightened or adjusted. They are not a
   license to stop optimizing a clearly wasteful path.

The governing criteria are **scalable and fast**, in that order when a tradeoff
is unavoidable. Work proportional to newly committed data is preferred over work
proportional to total history, dungeon size, or retained branches. Within that
constraint, minimize absolute and tail latency without weakening durability,
determinism, disclosure, or recovery. CI enforces scale and regression ratios
rather than fragile machine-specific wall-clock numbers.

## Measurement model

Extend the checked-in harness before changing persistence:

- retain mean, p50, p95, and maximum rather than aggregate averages;
- measure 1, 8, 64, and 256 connected regions, including portals, obstacles,
  doors, multiple elevations, and repeated movement across region boundaries;
- measure histories at 0, 100, 1,000, and 10,000 actions;
- isolate simulation transition, perception, revision detection, rollback
  capture, journal encoding/write/sync, client update application, and rendering;
- add multi-actor cases because revision detection currently observes every
  actor before and after an action; and
- record save size, bytes written per action, allocations where practical, and
  restart/replay time.

Wall-clock benchmarks remain diagnostic. Deterministic operation counts, file
growth bounds, and ratios between small and large cases are suitable for stable
automated regression tests.

## Persistence design

Use an append-oriented journal and periodic atomic checkpoints:

1. A journal frame contains a length, format/version marker, monotonic sequence,
   payload, and checksum. A partial or corrupt final frame is detectable and can
   be truncated or ignored according to the recorded recovery contract.
2. An accepted command is validated and simulated in a transactional candidate.
   Only the new record is encoded and appended; the complete archive is not
   serialized for each action.
3. Publication occurs at the durability boundary chosen above. Failed appends or
   syncs do not mutate the published engine or consume a request identity.
4. A checkpoint stores the authoritative replay base, receipt index, branches,
   and other required deterministic state. It is written to a temporary file,
   synced, and atomically installed. The journal is rotated only after the new
   checkpoint is recoverable.
5. Checkpoint creation is triggered by bounded journal bytes or record count,
   not by every command. Expensive serialization operates from an immutable
   snapshot outside the action critical section where ownership permits.
6. Startup loads the newest valid checkpoint and replays subsequent valid
   frames. Recovery explicitly tests interruption before append, during a frame,
   after frame flush, during checkpoint creation, after checkpoint installation,
   and during journal rotation.

Do not introduce acknowledgement before durability. Background checkpoint and
compaction work may lag behind, but the journal record needed to recover every
acknowledged action must already be durable.

## State and rollback work

After append persistence lands, measure and remove the remaining broad clones:

- replace full `Engine` candidate cloning with a transaction that owns only the
  changed game state, revision changes, receipt, record, and rollback material;
- represent the 128 rewind boundaries with structural sharing or compact deltas
  where measurement justifies it;
- avoid computing complete before/after observations for unaffected actors;
  maintain explicit change impact and verify it conservatively against the old
  comparison path in tests; and
- keep rollback internal to the authoritative server. Speculative client
  simulation must not gain hidden geometry or become a second rules engine.

The initial implementation should favor simple, auditable ownership over a
complex delta system. Append-only persistence removes the largest measured cost
without requiring simulation redesign.

## Data structures and caching

Ordered collections are currently valuable for deterministic behavior and are
not globally replaced. Optimize individual access patterns only after profiling:

- add a region-to-exits index if global passage scans grow with total topology;
- index visible cells and occupants for repeated lookup during observation and
  rendering instead of repeatedly scanning vectors;
- make client update application transactional without cloning the entire
  remembered map on every update, using a small validated change set or
  copy-on-write ownership;
- cache actor scenes/observations by authoritative revision only if repeated
  construction remains material after redundant calls are removed; and
- invalidate caches from explicit world/entity change sets, with tests comparing
  cached and uncached results.

Speculative presentation is a later, optional experiment. It cannot reduce
authoritative completion time, and rollback artifacts may be more distracting
than a short bounded wait in a turn-based client. Reconsider it only after local
durable p95 latency meets the target and remote latency becomes the dominant
cost.

## Delivery phases

### Phase A — Baseline and contracts

Complete the scale matrix, add phase timings, encode the recorded durability and
format decisions in tests, and add recovery fixtures. No storage behavior changes
in this phase.

#### Preliminary Phase A measurements and limitations

The checked-in harness is `crates/server/examples/latency_bench.rs`. It constructs
1/8/64/256-region fixtures and seeds two-room histories at 0/100/1,000/10,000
actions, with 1/8 actors. Timed transitions are waits; static queries do not prove
portal, door, elevation, or changing-LOS behavior. The region and history cases
are separate, and persistence is a single save after each memory-command batch.
Phase reporting has overlapping perception/revision intervals, missing navigation
timing, and means without per-phase percentile distributions. The current client
sample includes harness clones. Existing operation/byte assertions and recovery
models are a starting point, not complete scaling or storage-fault coverage.

The initial Windows release run observed candidate cloning grow from roughly
0.05 ms in the first batch to 10.6 ms near 10,000 retained actions. Counts show
two revision views per actor and full-archive encoding on save. These support
investigating history and actor scaling, but do not establish total durable
latency or the cost of traversal. The previously quoted sync numbers were
incorrectly divided by 100 when a single save was aggregated with 100 commands;
withdraw those numbers and remeasure using separate samples. Retain raw results
and update findings after implementing the [mixed-workload plan](performance-harness.md).

#### Draft Phase B design — review pending completion of Phase A

Advance the save format to version 4 and reject version 3. The append journal
uses little-endian frames:

`magic[4] = "TORJ" | format:u16 = 4 | kind:u16 | sequence:u64 |
payload_len:u32 | crc32c:u32 | payload[payload_len]`

The proposed 24-byte header is followed by versioned JSON payload bytes. `kind=1` is a
committed command/annotation record; reserved kinds are rejected. The checksum
covers the version, kind, sequence, length, and payload (excluding the checksum
field itself). Sequence numbers must be contiguous. Before implementation,
specify maximum lengths, exact payload schema/encoding, save/generation identity,
and recovery rules for torn tails versus corruption in previously durable data.
Do not silently discard acknowledged records. A failed append/sync must not
publish state or consume request identity in memory; define recovery of complete
but unacknowledged frames and safe retry after an uncertain I/O result.

Checkpoints are separate files containing the authoritative replay base,
receipt index, branch metadata, rewind boundaries, and deterministic world
state, encoded with the same version marker. They are written to a temporary
file, flushed and synced, atomically renamed, and only then is the old journal
rotated. Recovery chooses the newest valid checkpoint and replays subsequent
valid frames. Phase B should preserve current filtering, idempotency, branch,
rewind, annotation, wizard, and lock behavior before Phase C adds rotation and
compaction.

This is a proposal, not an implemented or fully reviewed recovery contract.
Resolve platform-specific directory/rename durability, checkpoint contents and
selection, retained branches/receipts, and fault schedules before Phase B. The
current toy recovery fixture does not implement this CRC32C framing contract.

### Phase B — Append journal

Introduce framed records and recovery scanning behind focused storage tests.
Preserve command atomicity, duplicate-request recovery, history filtering,
branch identity, and current publication ordering.

### Phase C — Checkpoints and compaction

Add periodic atomic checkpoints, bounded journal rotation, startup replay timing,
and fault injection at all file transition boundaries.

### Phase D — State-copy and observation scaling

Remove measured clone and repeated-observation costs. Add multi-actor and large
map cases before changing collection types or adding caches.

### Phase E — Client responsiveness

Measure shared client-state update application and ASCII rendering with large
remembered maps and burst updates. Preserve bounded queues and resynchronization;
verify native input remains responsive while the server saves or checkpoints.

## Verification and completion

The milestone is complete when:

- every accepted acknowledgement satisfies the chosen durability contract;
- interrupted writes recover the last valid committed boundary without duplicate
  action execution or disclosure inconsistency;
- append and checkpoint growth are bounded and restart replay is measured;
- latency does not materially grow across the history and dungeon scale matrix;
- rewind, retained branches, annotations, request retry, save locking, and wizard
  marking retain their existing behavior;
- Windows and Linux pass unit, integration, protocol, release, and actual-client
  process tests; and
- this guide is updated from proposed design to implemented behavior and known
  limitations.
