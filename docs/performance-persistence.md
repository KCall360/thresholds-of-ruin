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
the historical Phase A writer from the revised [background-save contract](background-saving.md).
The user authorized Phase B with asynchronous acknowledgements and performance as a primary driver.
The [Phase B findings](phase-b-findings.md) retain the focused comparison.

## Recorded decisions and guiding criteria

The milestone uses these product decisions:

1. **Ordinary acknowledgements are asynchronous.** They follow successful bounded
   queue admission and in-memory publication. Recent acknowledged play may be
   lost after a crash. Configurable target age, idle opportunity, maximum age,
   and queue pressure schedule background saves. Explicit save, normal client
   exit, graceful server shutdown, and wizard enablement retain durable barriers.
2. **Old saves are intentionally unsupported.** This is a pre-release project,
   so the new persistence layout will advance the save format and reject older
   formats. There is no historical-format importer or compatibility reader. Tests,
   fixtures, documentation, and the sample saves move to the new format together.
3. **Latency targets are provisional.** Initial release-build goals are p95 below
   8 ms and maximum below 33 ms for an ordinary locally saved action, with no
   meaningful upward trend between 100 and 10,000 retained actions. Measurement
   may show that these limits should be tightened or adjusted. They are not a
   license to stop optimizing a clearly wasteful path.

The governing criteria are **scalable and fast**, in that order when a tradeoff
is unavoidable. Work proportional to newly committed data is preferred over work
proportional to total history, dungeon size, or retained branches. Within that
constraint, minimize absolute and tail latency under the chosen save contract while preserving
determinism, disclosure, and consistent recovery. CI enforces scale and regression ratios
rather than fragile machine-specific wall-clock numbers.

## Measurement model

Profiling remains maintained after milestone 3p. Every later gameplay and client
feature must keep instrumentation and report validation working and add relevant
representative workloads when it introduces new performance-sensitive behavior.
Use targeted release-build comparisons to check that new features do not add
material action or presentation latency; a full matrix is reserved for broad
changes or evidence that focused checks are insufficient. Record matched cases,
sample counts, p50/p95/maximum, scale/operation counts, and unresolved regressions.
Follow [development practices](../CONTRIBUTING.md) for workload versioning,
before/after comparisons, and escalation. Correctness checks remain required.

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

The [background journal](background-saving.md) introduced in Phase B now uses
format 5 with Phase C checkpoints:

1. Validate and simulate in a transactional candidate, encode only the new record,
   and admit it to the bounded pending queue. Rejection leaves published state
   and request identity unchanged.
2. Publish immediately after admission. One worker owns SQLite batch transactions
   outside the engine/session lock. Idle opportunities defer normal saving;
   maximum age and queue pressure force progress under continuing activity.
3. Explicit saves wait for the accepted sequence captured by the request. Failed
   background saves retain pending records and block new mutations until retry.
4. Startup recovers SQLite transactions, validates every frame, restores the
   selected checkpoint and replays its tail. Receipts, abandoned branches, and
   private notes remain retained. The permanent wizard marker is saved before authority is enabled.
5. Phase C atomically selects a checkpoint and moves its covered journal rows
   into retained history. SQLite rollback journals protect this entire transaction;
   obsolete checkpoint pages are reused without deleting history.

## State and rollback work

Phase D removes broad command candidates and repeated scene construction:

- transactions own game state, revisions, current branch and at most 128 shared
  rewind boundaries; history and receipt indexes are never cloned by commands;
- world collections, items and actor navigation use copy-on-write ownership;
- ordinary waits update revisions from explicit time/readiness effects without
  constructing scenes or observations; full-view oracle tests verify equivalence;
- other actions conservatively compare every actor, with one scene reused for
  observation, material surfaces, door approaches and navigation; and
- navigation refresh examines visible connections and detaches shared knowledge
  only when facts change, copying the affected source-region maps rather than all
  discovered cells. Region exits use the existing ordered map's range.

Rollback stays internal to the authoritative server. Queue rejection discards the
candidate before changing history, receipts or published game state. No speculative
client simulation or long-lived observation cache is introduced. Full-history
copying remains available only to explicit detached persistence diagnostics.

## Data structures and caching

Ordered collections are currently valuable for deterministic behavior and are
not globally replaced. Optimize individual access patterns only after profiling:

- retain the bounded ordered-map range for region exits; add a separate index
  only if later measurements justify it;
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
actual-client driver, and current-writer fault tests are implemented. At that checkpoint,
storage used the version-3 whole archive; buffered streaming serialization
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
Windows/Linux name-durability guarantee in the Phase A writer. Passing restart
tests does not meet the full OS/power-failure contract.

That review records Phase A evidence. The current Phase B contract supersedes its
synchronous publication and two-file installation proposal. Phase C uses the same
SQLite transaction boundary for checkpoint selection and rotation.

### Phase B — Append journal

Implement framed append records in atomic background batches. Preserve command
atomicity, saved-receipt recovery, history filtering, and branch identity while
allowing publication before persistence. Policy and storage details are in the
[background-saving guide](background-saving.md).

#### Phase B verification and measurement

Keep comprehensive storage correctness coverage: bootstrap durability on Windows
and Linux, interrupted writes, corruption, uncertain I/O, lost acknowledgements,
duplicate/conflicting retries, retained branches, annotation privacy, and permanent
wizard marking. Process-restart tests alone do not prove power-loss durability.

Reuse Phase A workload definitions and retained baseline results with this focused
performance subset; do not routinely repeat the full 64-case characterization:

- Measure per-action encoded bytes and committed batch bytes at 100 and 10,000 retained actions.
  Assert that only the new frame is appended, previous row payloads remain intact,
  and the replay base is unchanged. SQLite physical page writes are a separate metric. Persistence work must scale with new data.
- Run the mixed durable workload at eight regions, one/eight actors, and
  100/10,000 retained actions (four cases). Compare encoding, write/flush, sync,
  and total p50/p95/maximum with the matching Phase A samples. Add matched memory
  cases when needed to isolate remaining engine costs.
- Run one 256-region, eight-actor, 10,000-action durable case and a representative
  actual-client workload, using the established timing boundaries.
- Measure normal restart/replay and verify recovered state for each selected save.
  Bounded startup work belongs to Phase C.

Repeat the broader matrix or discovery/rendering study only for unexplained
regressions, inconsistent focused results, or changes affecting those paths.
Record the selected cases, results, and any expanded investigation in the findings.
These Phase B measurements preceded Phase D candidate-copy removal; file-sync
latency can remain substantial. Phase B must remove history-dependent persistence
encoding/write amplification under the revised asynchronous save contract; it need not meet every
later phase's total-latency or startup target.

### Phase C — Checkpoints and compaction

Complete and merged in PR #25. See [checkpoints](checkpoints.md) for the
contract and [Phase C findings](phase-c-findings.md) for retained measurements.
Periodic background checkpoints retain simulation/navigation, revisions, current
branch and all 128 rewind boundaries. History frames and receipts move unchanged
into retained storage in the same transaction that installs the snapshot and
rotates the journal. Startup reads history but only simulates the remaining tail.

Focused release verification uses `latency_bench --phase-c`: the four saved
eight-region cases at one/eight actors and 100/10,000 retained actions, plus the
256-region/eight-actor/10,000-action case. Run matched checkpoint-disabled and
enabled policies with identical workloads and save timing. Record capture cost,
worker encoding size/time, p50/p95/maximum command latency and restart/replayed
record counts. Validate with `performance_report.py --phase-c`. Include actual
text/ASCII/headless restart and unsaved-tail rollback tests. Full matrix expansion
is conditional on unexplained regressions or broader changes.

### Phase D — State-copy and observation scaling

Implemented; see [Phase D findings](phase-d-findings.md) for measurements and
limits. `--phase-d` selects the nine focused memory/durable cases plus full growing
discovery at eight and 256 regions. The original version-1 fixture ordering remains
unchanged; profiling schema version 2 validates the reduced operation counts.
Use `--discovery-only` to isolate accumulated map knowledge. Actual-client process,
rewind, retry, rejection, checkpoint and full-view equivalence tests remain required.
Phase E remains separate client application/rendering work.

### Phase E — Client responsiveness

Implemented and locally verified. Shared client updates validate before
mutation without cloning historical memory; ASCII delivers ordered updates through
bounded queues, budgets event work per turn, and indexes rendering lookups.
The versioned client workload covers large remembered maps and burst updates;
native process tests exercise blocked saves and checkpoints. See
[Phase E findings](phase-e-findings.md) for measurements and unresolved native
presentation tail limits. The broader 3p acceptance criteria remain separate.

## Verification and completion

The milestone is complete when:

- ordinary acknowledgements follow queue admission; explicit save acknowledgements
  follow a committed batch covering their captured prefix;
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
