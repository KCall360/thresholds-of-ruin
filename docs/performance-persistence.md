# Performance and scalable persistence plan

## Purpose

This milestone makes responsiveness and scale explicit engineering constraints
before dungeon gameplay expands the world, entity count, and action history. It
does not weaken deterministic simulation, actor-specific disclosure, idempotent
requests, or wizard rewind.

The first release measurement is decisive: a 64-region scene takes about 0.02 ms,
observation about 0.06 ms, and portal movement plus navigation refresh about
0.22 ms. In-memory server commands remain below 1 ms in the sampled history,
whereas disk-backed commands grow from roughly 14 ms to 23 ms mean latency and
show stalls above 80 ms. Persistence and whole-state copying are therefore the
first targets. These figures are diagnostic observations from one machine, not
portable acceptance thresholds.

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

#### Phase A results (2026-09-22)

The checked-in harness is `crates/server/examples/latency_bench.rs` and emits
diagnostic CSV with mean, p50, p95, and maximum timings. It covers 1, 8, 64,
and 256 connected two-level regions with portals, obstacles, a door, and up to
8 actors; server histories are seeded at 0, 100, 1,000, and 10,000 actions.
The command profile isolates simulation transition, perception, revision
detection, candidate/rollback capture, serialization, write, and sync. The
client-common update path and ASCII renderer are measured separately. Stable
contracts assert actor-proportional observation/comparison counts and record /
byte accounting rather than machine-specific timings. Recovery fixtures cover
partial/corrupt final frames and checkpoint install/rotation interruption
boundaries; existing save-failure tests continue to verify publication atomicity.

Release measurements on the development Windows host found perception and
revision detection scale with actor count (about 0.36 ms and 0.18 ms for one
actor versus 3.2 ms and 1.6 ms for eight in the 10,000-action case). The
candidate clone grows from about 0.05 ms at an empty history to about 10.6 ms at
10,000 records; durable sync is roughly 0.10 ms to 4.2–5.0 ms. Region count is
not the dominant term in this fixture, while actor count and full-archive
copy/serialization are. These are diagnostic observations, not CI thresholds.

#### Reviewed Phase B design

Advance the save format to version 4 and reject version 3. The append journal
uses little-endian frames:

`magic[4] = "TORJ" | format:u16 = 4 | kind:u16 | sequence:u64 |
payload_len:u32 | crc32c:u32 | payload[payload_len]`

The 24-byte header is followed by canonical serde payload bytes. `kind=1` is a
committed command/annotation record; reserved kinds are rejected. The checksum
covers the header fields after `magic` plus payload, and sequence numbers are
strictly increasing. Startup scans only complete, checksum-valid frames and
truncates an incomplete/corrupt final frame; an acknowledged sequence must be
present in a durable frame. A failed append or sync leaves the in-memory
publication, receipt index, and request identity untouched.

Checkpoints are separate files containing the authoritative replay base,
receipt index, branch metadata, rewind boundaries, and deterministic world
state, encoded with the same version marker. They are written to a temporary
file, flushed and synced, atomically renamed, and only then is the old journal
rotated. Recovery chooses the newest valid checkpoint and replays subsequent
valid frames. Phase B should preserve current filtering, idempotency, branch,
rewind, annotation, wizard, and lock behavior before Phase C adds rotation and
compaction.

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
