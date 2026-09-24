# Performance and scalable persistence plan

## Purpose

This milestone makes responsiveness and scale explicit engineering constraints
before dungeon gameplay expands the world, entity count, and action history. It
does not weaken deterministic simulation, actor-specific disclosure, idempotent
requests, or wizard rewind.

Phase A is complete. The [measured findings](phase-a-findings.md) retain the
validated baseline. The harness measures ordinary mixed commands through a shared deterministic
fixture, including boundaries, doors, elevations, LOS changes, and multiple
actors. The [harness guide](performance-harness.md) defines reproducible commands
and measurement boundaries. The [storage review](persistence-review.md) separates
verified process recovery from the current missing name-durability barrier.
Phase B requires separate authorization.

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

The checked-in Phase A harness measures:

- retain mean, p50, p95, and maximum rather than aggregate averages;
- measure 1, 8, 64, and 256 connected regions, including portals, obstacles,
  doors, multiple elevations, and repeated movement across region boundaries;
- measure histories at 0, 100, 1,000, and 10,000 actions;
- isolate simulation transition, navigation, perception, revision detection,
  candidate capture, rewind snapshot, encoding, write/flush, sync, replacement,
  publication, client update application, and rendering;
- add multi-actor cases because revision detection currently observes every
  actor before and after an action; and
- record save size, bytes written per action, and normal restart/replay time.
  Allocation instrumentation is deferred; its overhead and unsafe-code policy
  implications have not been resolved.

Wall-clock benchmarks remain diagnostic. Deterministic operation counts, file
growth bounds, and ratios between small and large cases are suitable for stable
automated regression tests.

## Persistence design

Use an append-oriented journal and periodic atomic checkpoints:

1. A journal frame contains a length, format/version marker, monotonic sequence,
   payload, and checksum. A partial or corrupt final frame is detectable and can
   be handled according to the reviewed recovery contract. Unknown durable
   frontiers fail closed; never silently discard possibly acknowledged data.
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
complex delta system. Append-oriented persistence removes full-history encoding;
candidate cloning, file sync, and perception remain separate measured costs.

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

The shared fixture and trace, combined scale matrix, exclusive phase timings,
actual-client driver, and current-writer fault tests are implemented. Production
storage remains the version-3 whole archive; buffered streaming serialization
has been restored to measure the intended writer. No append journal, checkpoint,
rotation, or caching implementation is included.

#### Findings and reviewed proposal

The earlier wait-only averages and sync values divided by an unrelated batch
size are withdrawn. Use only the complete per-command release measurements
linked from the [harness guide](performance-harness.md). Successful mixed
commands and intentional blocked attempts have separate distributions; actual
client acknowledgement/presentation measurements have different boundaries from
server-only phases.

The [storage review](persistence-review.md) specifies the proposed version-4
frame layout, checksum coverage, maximum payload, save/generation identity,
uncertain-write reconciliation, conservative corrupt-tail handling, checkpoint
contents/selection, and future file fault schedules. It also records the missing
Windows/Linux name-durability guarantee in the current writer. Passing restart
tests does not meet the full OS/power-failure contract.

This reviewed proposal is a handoff for later work, not an implemented format.
Phase A stops here; Phase B and C remain unimplemented.

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
