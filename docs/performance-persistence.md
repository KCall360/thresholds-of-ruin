# Performance and scalable persistence

This page covers milestone **3p**: the latency targets, the persistence and
state-sharing design, what has been achieved, and what's still open. The
[performance harness](performance-harness.md) has the reproduction commands, and
the [testing policy](testing.md#performance-testing) has the rules every change
follows.

**Status:** deferred and still open. Phases A–E and the explored-world checkpoint
reduction are implemented. At the maintainer's direction, the remaining closure
work no longer blocks feature milestones, but every feature is still
performance-checked, and no target has been relaxed.

## Targets and guiding criteria

These product decisions govern the milestone:

1. **Ordinary acknowledgements are asynchronous.** An action is acknowledged
   once it's admitted to a bounded in-memory queue and published. Recent
   acknowledged play can be lost in a crash. Explicit save, normal client exit,
   graceful server shutdown, and wizard enablement wait for storage. See
   [background saving](background-saving.md).
2. **Old saves are unsupported.** Format changes reject older saves; there's no
   importer.
3. **Latency targets are provisional:** for an ordinary locally saved action in a
   release build, **p95 below 8 ms and maximum below 33 ms**, with no meaningful
   upward trend between 100 and 10,000 retained actions. Measurements may justify
   tightening or adjusting them. Meeting them isn't a license to leave a clearly
   wasteful path in place.

Optimize for being **scalable, then fast**. Prefer work proportional to newly
committed data over work proportional to total history, dungeon size, or retained
branches. Within that constraint, minimize absolute and tail latency while
preserving determinism, disclosure, and consistent recovery. CI enforces
operation counts and scale ratios, not machine-specific wall-clock numbers.

## Measurement model

The harness:

- keeps mean, p50, p95, and maximum rather than aggregate averages;
- covers 1, 8, 64, and 256 connected regions (portals, obstacles, doors,
  elevations, and repeated boundary crossings), 1 and 8 actors, and histories of
  0, 100, 1,000, and 10,000 actions;
- times exclusive phases: candidate capture, checkpoint capture, simulation,
  navigation, perception, revision comparison, rewind snapshot, encoding, I/O,
  publication, client update application, and rendering;
- records save size, bytes per action, and restart/replay time; and
- grows remembered map knowledge in a separate full-traversal (discovery) trace,
  because stationary local loops can't reveal costs that grow with exploration.

Allocation instrumentation is deferred: no low-overhead approach has been
established under the workspace's `unsafe` prohibition.

## Current design

- **Append journal.** Each accepted action encodes only its new record and is
  admitted to a bounded queue before publication. One storage worker writes
  atomic SQLite batches outside the engine lock. See
  [background saving](background-saving.md).
- **Checkpoints.** Periodic snapshots and logical compaction bound the replay
  needed at startup, while all history stays retained. Navigation knowledge is
  pooled by source region, so explored worlds stay small. See
  [checkpoints](checkpoints.md).
- **State sharing.** Command candidates own only decision state, revisions, the
  current branch, and at most 128 shared rewind boundaries. World collections,
  items, and navigation use copy-on-write ownership. Waits construct no scenes;
  other actions build one scene per observation and reuse it.
- **Client delivery.** Client updates are validated before mutation without
  copying remembered map memory. ASCII delivers ordered updates through a bounded
  channel, limits event work per frame, and indexes rendering lookups.

Rollback stays inside the authoritative server. There's no speculative client
simulation or long-lived observation cache.

### Data structures and caching

Ordered collections are kept for deterministic behavior and aren't replaced
wholesale. Optimize individual access patterns only after profiling shows
they matter. Cache scenes or observations by authoritative revision only if
construction remains a measured cost. Invalidate caches from explicit change
sets, and test cached results against uncached ones. Speculative presentation
is a possible later experiment, only once durable local p95 meets the target
and remote latency dominates.

## What each phase achieved

Measured on the maintainer's Windows machine (Intel i7-9750H, 16 GiB RAM,
saves on an NTFS hard disk). Timings are diagnostic; the ratios and operation
counts are the durable result.

| Phase | Change | Headline result |
| --- | --- | --- |
| A | Baseline harness and contracts | Durable p95 was 51–1,293 ms, dominated by file sync. The largest case wrote 12.4 GiB for a 5.7 MiB save, and restart took 110 s. |
| B | Append journal, asynchronous acknowledgements | 256 regions / 8 actors / 10,000 actions: p95 fell from 855 ms to 15.6 ms. Each action encodes one ~600-byte record. |
| C | Checkpoints and compaction | Largest-case restart fell from 119 s to 7 s (records replayed: 11,499 to 474). |
| D | Removing state copies, scaling observations | Largest saved case p95 fell from 14.2 ms to 2.5 ms; all selected cases met 8 ms / 33 ms. Restart fell to 0.66 s. |
| E | Client responsiveness | 64 updates on a 20,956-cell remembered map: apply p95 fell from 549 ms to 39 ms and render p95 from 6.6 ms to 3.0 ms. |
| Format 6 | Region-shared checkpoint navigation | Fully explored 256-region checkpoint fell from 765 MB to 9.9 MB (the cap stays 64 MiB). Full checkpoint-enabled exploration, exact restart, and native input during a blocked save all pass. |

The headline cases are recorded in the
[performance ledger](performance-harness.md#performance-ledger). Full reports,
raw samples, manifests, and the original storage review are preserved in the
[`docs-history-2026-09` archive](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs).

## Open work

These items stop 3p from closing:

- **Client timing tails.** Rare end-to-end delays up to 2.9 s were recorded in
  native-client runs. Investigation located some of them in diagnostic calls:
  a 633 ms request-diagnostic write and a 442 ms stdout write, while server
  handlers stayed short. One 197 ms stall is inside the native presentation and
  pacing call itself. Next step: capture thread-scheduling and blocked-write
  evidence around these stalls. (A Windows Performance Recorder kernel trace was
  refused by system policy, `0xc5585011`, so a different method or machine is
  needed.)
- **Longer eight-client workload.** Two three-cycle attempts failed: one hit the
  diagnostic log cap, the other a snapshot-readiness timeout with a 27 s headless
  ready-report call. They stand as failures until that workload passes; shorter
  successes don't replace them.
- **Historical acknowledgement and presentation tails** (182–772 ms, recorded
  during Phases D and E) didn't recur in later runs, but not reproducing them
  doesn't make them fixed.
- **Eight-actor combat** p95 is about 11.7 ms, above the 8 ms target.
  Building before-and-after observations for every actor costs about 5 ms per
  command. See [dungeon gameplay](dungeon.md#performance).
- **Dense falling physics** (8 actors, 128 moving items) p95 is about 22.5 ms.
  See [physics](physics.md#performance).

Explicitly deferred scaling work (still measured, never an excuse for a
regression within the current 8–256-region, 100–10,000-action envelope):

- region streaming and active-horizon loading (milestone 4e);
- startup cost that is linear in retained history (frame validation, history
  loading, receipt rebuilding);
- actor-count scaling of non-wait observations; item, place, and history query
  indexes; large travel searches;
- connection-lifetime client map memory and moving-chart work;
- synchronous optional diagnostic I/O;
- hardware power-loss qualification and allocation profiling.

## Completion criteria

3p is complete when:

- ordinary acknowledgements follow queue admission, and explicit-save
  acknowledgements follow a committed batch covering the captured prefix;
- interrupted writes recover the last valid committed boundary without duplicate
  execution or disclosure inconsistency;
- append and checkpoint growth are bounded and restart replay is measured;
- latency doesn't materially grow across the history and dungeon-size matrix;
- rewind, retained branches, annotations, request retry, save locking, and wizard
  marking keep their behavior;
- the historical and current timing tails are explained and resolved;
- Windows and Linux pass unit, integration, protocol, release, and actual-client
  process tests; and
- actual text and ASCII clients stay responsive during saving and checkpointing.

Process-restart tests don't establish hardware power-loss durability; that
remains outside these criteria.
