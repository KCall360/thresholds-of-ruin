# Performance engineering

Use this guide to choose a workload, collect comparable release measurements,
validate complete runs, and retain evidence. [Setup](setup.md) supplies
Rust, pinned Python and native-display prerequisites; [testing policy](testing.md#performance-testing)
defines when measurements are required. Arena capture, paired experiments and
optimization live in the [arena guide](arena.md).

Read [targets](#targets-and-guiding-criteria) and [measurement boundaries](#measurement-boundaries-and-reproduction)
before interpreting timings. Start with [before-and-after comparisons](#before-and-after-comparisons),
then choose an unchanged versioned workload below. Build real client binaries
explicitly for native drivers; example binaries alone are insufficient. Do not
run measurements concurrently with builds or test suites. Raw output belongs in
gitignored local directories, with stdout/stderr and exit status retained.

Performance closure remains open; completed journal/checkpoint/state-sharing and
streaming work does not relax targets. Historical measurements below are evidence
for specific workloads, not fresh verification of the current commit. The
[status page](../status.md) owns project-wide implementation and acceptance status.

## Choose a workload

| Question | Procedure |
| --- | --- |
| Did this change regress ordinary action cost? | [Interleaved release comparison](#before-and-after-comparisons) |
| Does retained history or discovered knowledge grow cost? | [Deterministic matrix](#measurement-boundaries-and-reproduction), [growing discovery](#focused-phase-d-and-growing-discovery), [saved discovery](#saved-discovery-measurement) |
| Does checkpointing preserve a responsive native client? | [Explored-save acceptance](#explored-save-native-acceptance) |
| Are stream application/rendering or diagnostics slow? | [Client workloads](#client-workloads), [correlation](#opt-in-timing-correlation) |
| Does region-count growth add local command work? | [Streaming evidence and recipe](#streaming-evidence) |
| Are content or wire projections costly? | [Places](#durable-place-workload-v1), [packages](#scenario-package-workload), [combat](#combat-workload-v1), [decoder](#wire-decoder-diagnostic), [collection edits](#individual-observation-collection-diagnostic) |
| How does sight range affect geometry work? | [Lighting/range diagnostics](#lighting-and-range-diagnostics) |

Workload names such as Phase B/C/D are supported immutable selectors, not claims
that their implementation phase is still pending. Use each workload's validator;
JSON syntax alone does not establish complete or comparable coverage.

## Targets and guiding criteria

These product decisions govern performance work:

1. **Ordinary acknowledgements are asynchronous.** An action is acknowledged
   once it's admitted to a bounded in-memory queue and published. Recent
   acknowledged play can be lost in a crash. Explicit save, normal client exit,
   graceful server shutdown, and wizard enablement wait for storage. See
   [background saving](../architecture/persistence.md).
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

### Regeneration and nutrition baseline

Recovery periods, per-point nutrition costs, maximum pools, action costs,
time-drain rate, injury/reset behavior and meal availability are coupled tuning
parameters. Rogue's current all-one regeneration prices and the existing recovery
intervals are provisional baselines, not compatibility or balance contracts.
Keep mechanical correctness separate from a claim that a complete run is balanced.

A bounded next comparison should use one fixed seed and initial build, then replay
the same short resource-spending/rest sequence with explicit zero regeneration
prices and the provisional prices. Cover full pools, a one-point deficit, a mixed
Health/Stamina/Focus/Mana deficit, a hunger crossing and the same mixed sequence
across save/restart. Change one rate or period family at a time; do not start a
broad parameter search before this baseline is understood. Include a fractional
price variant to establish partition/restart equivalence.

`CreatureState::advance_active` returns transient `ActiveTimeOutcome.regenerated`
(gross actual Health and per-resource gains) and `nutrition_spent` (the total
reserve decrease from time plus recovery, clipped at starvation). Gross gains
are measured before a simultaneous maximum reduction, so a negative net pool
change cannot conceal regeneration. Record elapsed active ticks, those totals,
starting/ending reserve and both saved remainders, action count, surviving state
and the exact authored rates. These diagnostics do not persist redundant totals
or disclose other actors' hidden rules through normal observations. Direct
restoration is outside the periodic-gain total. At death no subsequent gain or
charge is counted.

The bulk-settlement bound is four hunger/death crossings plus finite condition
expiries, each using at most 64 monotone-search probes and constant-size resource
arithmetic. An enabled fainting policy retains its existing separately bounded
random deadlines. Compare a short and maximum-tick pure settlement for operation
bounds, then use the ordinary representative release latency cases for overhead.
No performance or balance result is implied by this proposed comparison.

### Private committed survival flows

Set `TOR_SURVIVAL_DIAGNOSTICS_LOG` to a private JSONL output path before
starting the server to opt into observational gross flows. It is off by default.
The simulation collects bounded transaction-local observations; the server emits
one commit only after journal admission and authoritative live publication.
Queries, projections, rejected candidates, duplicate requests and restore replay
add no gross flows. Collection is excluded from save and protocol schemas.

Version 1 records contain a process-segment begin anchor, contiguous committed
history entries and save barriers. Each anchor identifies the initial lineage,
current branch, history sequence, last entry and tick. Live publication is distinct
from asynchronous durable saving. A save barrier does not flush the diagnostic
queue, and this log is not crash-safe exactly-once storage.

Use `scripts/survival_diagnostics.py` to validate the output. Pass its
`expected_final_anchor` argument from an independently read final durable save
or history boundary. A clean file alone establishes only its represented prefix:
a failed writer might leave an earlier valid barrier and omit the later tail.
For whole-run totals, also verify that the first begin anchor matches the known
measurement start; a nonzero begin legitimately measures only a later suffix.
Missing/malformed rows, sequence gaps, duplicate entries, divergent restart
anchors, counter overflow, queue loss and incomplete batches invalidate a sample.
The bounded best-effort writer never blocks or rejects gameplay on diagnostic
failure. Wait for the matching final diagnostic barrier before reading a sample.
Its worker coalesces JSON fragments in a fixed 8 KiB buffer and flushes each
complete record, avoiding one file write per fragment. The 16-record survival
queue remains bounded and nonblocking; slow or failed I/O can still invalidate
evidence. A write or flush failure discards the remaining buffer without retry.

Per-actor fields aggregate every settled segment within one committed entry,
retaining exact total active ticks and the first/last nutrition fractions. They
retain gross actual periodic Health/Stamina/Focus/Mana gains,
living active ticks, total nutrition spend, separate unclamped time and regeneration
demands, combined clipped amount, and before/after nutrition reserve, elapsed
remainder and regeneration remainder. At death the combined clamp has no defined
allocation between the two demands; no component spend is invented. Paid start
and resolution costs, acquired and released unpaid holds, exact consumed item
identities/quantities, direct healing/restoration, nourishment gain and cleared
negative reserve are separate. Cancellation releases unpaid holds and refunds no
paid costs. Food offered/wasted values observe the existing sampled operand once;
other direct Nourish effects may contribute gains without a food offer.

Acquisition, drops, transfers, complete stock ledgers and per-actor action choices
are unavailable in this minimal adapter. Net inventory or resource changes cannot
stand in for those missing gross flows. This instrumentation by itself makes no
claim about Rogue balance, successful earned-HD routes or release latency.

## Current design

- **Append journal.** Each accepted action encodes only its new record and is
  admitted to a bounded queue before publication. One storage worker writes
  atomic SQLite batches outside the engine lock. See
  [background saving](../architecture/persistence.md).
- **Checkpoints.** Periodic snapshots and logical compaction bound the replay
  needed at startup, while all history stays retained. Navigation knowledge is
  pooled by source region. Explored-map storage has separate
  [capacity checks](#explored-map-checkpoint-capacity); pooling alone does not
  establish a size or latency bound. See [checkpoints](../architecture/persistence.md).
- **State sharing.** Command candidates own only decision state, revisions, the
  current branch, and the union of 128 selectable rewind states and 128 raw audit
  states (at most 256 shared boundaries). World collections,
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

## Historical baseline results

The maintainer's Windows baseline (i7-9750H, 16 GiB RAM, NTFS HDD) established
why the engine uses asynchronous append, checkpoints and shared state: synchronous
saving dominated action cost; replay and copied navigation grew with retained
history/discovery. Later changes reduced those costs without weakening recovery.
These are completed design decisions, not pending implementation phases.

Accepted headline cohorts and raw hashes belong in the [ledger](#performance-ledger).
The [original summary](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/performance-persistence.md#what-each-phase-achieved)
and [September raw archive](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs)
preserve figures, workload identities and missing-provenance caveats. Do not
interpret historical fixture improvements as current-build acceptance.

## Health and advancement diagnostic comparison

A Linux release comparison measured the Health/point-ledger implementation
against merged `51ab5f687ab3490f21836d0e329256ebec95c23f`, using the same
combat workload (version 1). Two separate four-round balanced ABBA cohorts used
identical binary hashes and a clean candidate tree
`66aa04bd9d5765b9b008a099acdf23455b6b436f`, before documentation-only reporting.
The VM fingerprint was `c14b64955bd2` (AMD EPYC 9V74, 9 logical CPUs, 9.7 GiB,
Debian 13, kernel 6.18.44); saves used overlay storage with unknown physical
backing. No competing build/test workload was recorded. All 16 benchmark runs
completed and passed their own report validators.

Every timing cell is p50 / p95 / max in milliseconds; n is per side, per cohort.
The second cohort repeats the first rather than replacing it.

| Case / metric | n | First base | First candidate | Repeat base | Repeat candidate |
| --- | ---: | ---: | ---: | ---: | ---: |
| a2-h0 client_apply | 768 | 0.607 / 0.805 / 2.947 | 0.598 / 0.794 / 2.618 | 0.601 / 0.794 / 2.652 | 0.598 / 0.811 / 1.862 |
| a2-h0 client_draw | 768 | 0.364 / 0.499 / 2.001 | 0.360 / 0.513 / 0.920 | 0.360 / 0.514 / 2.963 | 0.357 / 0.482 / 1.489 |
| a2-h0 command | 768 | 0.410 / 0.573 / 2.549 | 0.402 / 0.559 / 0.779 | 0.393 / 0.548 / 2.421 | 0.400 / 0.523 / 1.055 |
| a2-h0 decision | 768 | 0.001 / 0.641 / 2.707 | 0.001 / 0.666 / 1.573 | 0.001 / 0.673 / 1.681 | 0.001 / 0.642 / 1.127 |
| a2-h0 resume | 12 | 27.303 / 33.267 / 33.267 | 25.770 / 34.741 / 34.741 | 26.462 / 36.228 / 36.228 | 25.771 / 31.041 / 31.041 |
| a2-h0 save | 12 | 1.215 / 1.437 / 1.437 | 1.162 / 1.255 / 1.255 | 1.218 / 1.507 / 1.507 | 1.210 / 1.788 / 1.788 |
| a8-h1000 client_apply | 660 | 0.736 / 0.995 / 4.221 | 0.715 / 0.986 / 2.980 | 0.744 / 1.091 / 4.041 | 0.715 / 0.915 / 5.939 |
| a8-h1000 client_draw | 660 | 0.675 / 0.929 / 3.352 | 0.650 / 0.888 / 2.177 | 0.690 / 0.973 / 3.580 | 0.646 / 0.844 / 2.149 |
| a8-h1000 command | 768 | 1.118 / 1.639 / 7.072 | 1.093 / 1.551 / 5.107 | 1.125 / 1.791 / 3.710 | 1.106 / 1.509 / 2.781 |
| a8-h1000 decision | 768 | 0.001 / 0.591 / 1.784 | 0.001 / 0.591 / 0.806 | 0.001 / 0.608 / 1.193 | 0.001 / 0.594 / 0.974 |
| a8-h1000 resume | 12 | 192.799 / 219.753 / 219.753 | 202.773 / 226.991 / 226.991 | 202.088 / 220.081 / 220.081 | 200.916 / 233.263 / 233.263 |
| a8-h1000 save | 12 | 34.526 / 36.308 / 36.308 | 35.657 / 47.717 / 47.717 | 35.681 / 41.962 / 41.962 | 35.064 / 39.943 / 39.943 |

Per-round count sums were unchanged for body cells, scenes and navigation
refreshes: respectively 7119 / 762 / 0 for `a2-h0`, and 408129 / 32028 / 36
for `a8-h1000`. Disclosed bytes increased by 297 in both cases (294969 to
295266; 299607 to 299904). Small-history saved bytes remained 380928.
Large-history saved bytes increased from 7753728 to a varying
7778304–7786496; the page-sized variation is retained in both cohorts.

The first large-history save tail increased 31.4%, from 36.308 to 47.717 ms,
but the repeat decreased 4.8%, from 41.962 to 39.943 ms. Its median moved
+3.3% and −1.7%. Large-history resume p95 increased 3.3% and 6.0%; its
median moved +5.2% and −0.6%. Twelve save/resume observations per side per
cohort do not establish a stable tail regression, an improvement, or neutrality.
The retained first tail and size increases are costs to watch. These combat
fixtures do not close the separate exploration-replay or native-client tail
problems below, and do not measure maximum diagnostic-ledger projection.

Reproduce each cohort with `python scripts/perf_compare.py 51ab5f6
--case combat:a2-h0 --case combat:a8-h1000 --rounds 4 --order balanced`,
using a fresh output directory and matching release sources. Both complete
comparison bundles are retained locally; they have not been uploaded as
accepted performance-ledger headlines. Do not compare this VM with historical
Windows timings as though they shared a machine/storage fingerprint.

## Public-plan observation regression checkpoint

A provisional October 13, 2026 release comparison used owner-advancement tree
`070ca39e3b65a63a7501ca16df32bee0560e2e7f` and public-plan tree
`a6f9d55c0defccbd3978f5ecec83b2ee3598af8c` on the same cloud VM described above
(fingerprint `c14b64955bd2`, Rust 1.99.0, overlay storage). Three balanced rounds
and five latency cycles produced 18 successful, validated invocations, with no
competing build or test runtime. Values below are p50/p95/maximum milliseconds;
`n` is per side.

| Case / metric | n | Baseline | Public-plan head |
|---|---:|---:|---:|
| r8-a1-h100-memory authoritative | 915 | 0.566/0.820/2.731 | 0.564/0.855/2.350 |
| r64-a8-h100-memory authoritative | 7500 | 0.032/2.150/15.875 | 0.031/2.121/12.621 |
| r8-a1-h100-memory restart | 3 | 144.841/145.310/145.310 | 141.847/174.225/174.225 |
| r64-a8-h100-memory restart | 3 | 1191.900/1224.949/1224.949 | 1154.364/1253.496/1253.496 |
| combat a8-h0 command | 576 | 1.262/1.920/4.781 | 1.111/1.539/3.637 |
| combat a8-h1000 command | 576 | 1.156/1.744/3.303 | 1.139/1.732/7.804 |
| combat a8-h1000 save | 9 | 39.821/48.189/48.189 | 39.156/45.693/45.693 |
| combat a8-h1000 resume | 9 | 225.744/282.163/282.163 | 224.715/244.769/244.769 |

Latency operation and byte counts were unchanged. Combat body-cell, scene and
navigation counts were unchanged; disclosed bytes increased by 57 per displayed
case (302310 to 302367 and 301359 to 301416). Small-history saves remained
393216 bytes. Large-history save sizes varied within each side: baseline
8146944–8151040 and head 8187904–8192000 bytes. Retain that footprint increase,
the small-workload restart tail and the large-history command maximum; this
small cohort establishes neither a stable speedup nor performance neutrality.

The latency fixtures use plain actors and exercise no-event observation, wire
and replay paths. Combat exercises real, already-known basic-attack disclosure,
not newly learned identities or paid public-plan capture. These measurements do
not establish the latency of paid capture, retained-record replacement, Readied
consumption or unknown-ability assimilation. Their functional and bounded-work
checks remain separate. All combat dimensions execute even when only these two
case summaries are selected.

Reproduce with `python scripts/perf_compare.py b4fe95b0b9b0 --case
r8-a1-h100-memory --case r64-a8-h100-memory --case combat:a8-h0 --case
combat:a8-h1000 --rounds 3 --cycles 5 --order balanced` on the matching sources.
Raw samples remain local; there is no accepted performance-ledger headline or
public raw-evidence URL, and these results do not close the outstanding targets.

## Earned-growth regression checkpoint

A provisional October 14, 2026 release comparison used public-plan tree
`a96d8df9384100c83309cf66e4e294340198926e` and earned-growth tree
`a6519527f7d8c11fa05c2946940b80c4acd34a68` on the same cloud VM described above
(fingerprint `c14b64955bd2`, Rust 1.99.0, overlay storage). Three balanced rounds
and five latency cycles produced 18 successful, validated invocations, with no
competing build or test runtime. Values below are p50/p95/maximum milliseconds;
`n` is per side. Latency uses `command_call`; combat uses `command_ms`.

| Case / metric | n | Public-plan baseline | Earned-growth head |
|---|---:|---:|---:|
| r8-a1-h100-memory / command | 945 | 0.551/0.862/2.762 | 0.559/0.888/2.723 |
| r64-a8-h100-memory / command | 7530 | 0.032/2.100/17.135 | 0.032/2.068/15.304 |
| a8-h0 / command | 576 | 1.121/1.576/6.492 | 1.092/1.509/3.469 |
| a8-h1000 / command | 576 | 1.128/1.660/4.778 | 1.136/1.762/4.603 |

All reported operation and byte counts were unchanged except physical save size
in combat a8-h1000: baseline files ranged from 8,183,808 to 8,192,000 bytes,
while head files were 8,269,824 bytes. Retain that footprint increase and the
mixed timing changes; this small cohort establishes neither a stable speedup
nor performance neutrality. All combat dimensions execute even when only these
two case summaries are selected.

The latency fixtures use plain actors, and the combat fixture uses high-Health,
nonfatal basic attacks. These measurements exercise no-growth scheduling,
observation, transport and recovery paths. They do not measure fatal XP awards,
automatic HD growth, drain/restoration, poison or pushed-fall attribution, and do
not establish leveling pace or gameplay balance. Focused functional coverage of
those implemented paths remains separate.

Reproduce with `python scripts/perf_compare.py 359092785ccf --case
r8-a1-h100-memory --case r64-a8-h100-memory --case combat:a8-h0 --case
combat:a8-h1000 --rounds 3 --cycles 5 --order balanced` on the matching sources.
Raw samples remain local; there is no accepted performance-ledger headline or
public raw-evidence URL, and these results do not close the outstanding targets.

## Open work

Remaining performance closure requires evidence for:

- **Pre-checkpoint exploration recovery.** During the compact Rogue migration,
  one debug VM diagnostic replayed 493 ordinary moves (986 post-initial journal
  entries) with no checkpoint. Recovery reproduced the exact saved state but
  took 16.2 s, including 16.0 s in replay, exceeding the ordinary process test's
  15 s startup bound. The default checkpoint request interval is 1,024 entries,
  followed by asynchronous persistence; it bounds entry cadence, not recovery
  wall time. A later debug marker-floor acceptance trace had 990 post-initial
  entries (991 journal rows, 719,733 frame bytes), entered replay 169.7 ms after
  startup, and did not complete within the same 15 s deadline. That timeout is
  not a measurement of completed recovery. Neither diagnostic is a release
  comparison or a resolved performance result. Long floor-activation and Rogue
  play acceptance uses checkpoint interval 64; short cases still exercise
  journal-only recovery, and source-free entry also exercises a journal tail
  after a real checkpoint. This is test workload policy, not a production-default
  change or a timeout fix; the deadlines are unchanged.
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
- **Historical eight-actor combat** p95 was about 11.7 ms, above the 8 ms target.
  In that measurement, building before-and-after observations cost about 5 ms per
  command. See [original combat evidence](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/dungeon.md#performance).
- **Historical dense falling physics** (8 actors, 128 moving items) p95 was about
  22.5 ms. Later lighting-era measurements below improved dense-falling action
  latency, but those distinct cohorts do not erase unexplained storage tails.
  See [original physics evidence](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/physics.md#performance).
- **Isolated save tails in the 3s physics comparison.** Two static cases each
  had one slow save among nine samples (p95 0.72 and 0.76 s). Their cause remains
  unexplained; scene caches are not persisted. Keep these as unresolved evidence,
  not a claim of a reproducible sight regression or a fixed storage problem. See
  the [recorded sight comparison](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/sight-3d.md#reference-implementation-findings).

Remaining scaling work (still measured, never an excuse for regression in the
historical diagnostic 8–256-region, 100–10,000-action envelope; authored packages
and streamed worlds have their own limits):

- startup cost that is linear in retained history (frame validation, history
  loading, receipt rebuilding);
- actor-count scaling of non-wait observations; item, place, and history query
  indexes; large travel searches;
- connection-lifetime client map memory and moving-chart work;
- synchronous optional diagnostic I/O;
- hardware power-loss qualification and allocation profiling.

## Completion criteria

Performance closure requires:

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

## Before-and-after comparisons

Use `scripts/perf_compare.py` for the targeted release comparison each
latency-sensitive change needs:

```sh
python scripts/perf_compare.py main --case r8-a1-h100-memory --case r64-a8-h100-memory
python scripts/perf_compare.py main --case combat:a8-h1000 --rounds 4
python scripts/perf_compare.py main --case r8-a8-h100-durable --temp-dir F:/tor-perf-tmp -- --save-target-ms 10
```

The script checks out the base ref in a Git worktree, builds the release
benchmark examples there and in the working tree, and copies both binaries
into a new run directory, recording their SHA-256 hashes. It then runs them
interleaved on this machine: base, head, base, head, for `--rounds` pairs
(default three). Each run is validated by the report validator from its own
tree. Failed or rejected runs are kept, excluded from the tables, and make the
script exit nonzero.

Use `--order balanced` to reverse each pair on even rounds, or
`--order head-first` for every pair, when investigating order-dependent disk or
warm-up effects. These options preserve workload and sample coverage; the
comparison records the selected order. Keep failed comparisons as evidence.
Use an even number of balanced rounds to give both binaries equal coverage in
each position.

The baseline always builds into its own worktree's `target` directory. The head
uses `CARGO_TARGET_DIR` when set, resolved relative to the head worktree, or its
local `target` directory otherwise. Both directories are passed explicitly to
Cargo, checked for overlap, and recorded in the comparison. Binary snapshots
come from these same directories; the baseline cannot inherit the head's cache.

A case is either a `latency_bench` case name (`--cycles` sets its cycle count,
default five) or `WORKLOAD[:GROUP]` for `combat`, `physics`, `items`, `client`,
or `places`. The `latency_bench` region streaming cases, `stream-r16-memory`,
`stream-r16-durable`, `stream-r256-memory` and `stream-r256-durable` (workload
`streaming-v1`), run only when selected by name; see
[region streaming performance](../architecture/streaming.md#performance). Those workloads always run their complete matrix, because their
validators require it; the group only selects what's displayed. Arguments after
`--` go to every benchmark invocation, and validator flags such as `--phase-d`
or `--narration` are passed through to the validators.

The output shows, for each case, pooled n/p50/p95/max per timing metric, the
range of per-round p95 values (a quick read on run-to-run noise), and the
operation and byte counts side by side. Counts should be identical across
rounds; any that vary are marked. A warning is printed if the base and head
workload versions differ, since their timings aren't comparable.

- Saves go to a temporary directory inside the run directory unless
  `--temp-dir` selects a volume. Choose it deliberately; the machine
  fingerprint records the storage type of that volume. The selected directory
  is a parent: each comparison creates and cleans only its own child, including
  after failure or interruption. Existing files in the parent remain untouched.
- The script refuses to start measuring while `cargo` or `rustc` processes are
  running. `--allow-competing` overrides this and is recorded.
- `--no-build` reuses binaries from those selected targets. Use it only after
  building the matching source revisions. Don't edit Rust inputs while a build
  is running.
- Run directories and base worktrees live in the gitignored `.local` directory.
  Remove old worktrees with `git worktree remove`.

Each run directory holds `comparison.json` (machine fingerprint, commits,
binary hashes, commands, every run's exit and validation status, and the pooled
results) and a `.tar.gz` bundle of it with the compressed raw samples and logs.
Record the cases, sample counts, percentiles, counts, and limitations with the
change, as the [testing policy](testing.md#targeted-checks-for-each-change)
requires.

## Performance ledger

[`perf/ledger.jsonl`](../../perf/ledger.jsonl) is a small committed history of
accepted headline measurements, one JSON line per case and metric. Each line
records the date, commit and dirty flag, workload name and version, case,
metric, a machine fingerprint (CPU, logical CPUs, RAM, OS build, and the
storage type, filesystem, model, and bus of the save volume, plus a short hash
of those fields), the build profile, n, p50/p95/max, key operation and byte
counts, and the URL and SHA-256 of the raw data.

Compare timings only between lines with the same machine fingerprint, build
profile, and workload version. Counts are deterministic and comparable across
machines. `python scripts/perf_ledger.py fingerprint --path DIR` prints this
machine's record for the volume holding `DIR`.

The ledger is seeded with the headline cases whose raw samples are preserved in
the [`docs-history-2026-09` archive](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs/measurements):
Phase B, C, and D at 256 regions, 8 actors, and 10,000 actions; the Phase E
client workload; the journal-only saved 256-region discovery; and the action
foundation refactor. Their notes say which machine details the archive
didn't record. The item, place, and scenario sets aren't seeded, because
their archived data doesn't identify the measured commit or their raw samples
weren't kept.

Add a line only for an accepted headline result, in the same change as the
finding it supports:

```sh
python scripts/perf_ledger.py add --comparison RUN/comparison.json --unit latency:r64-a8-h100-memory \
  --group r64-a8-h100-memory --metric authoritative_total \
  --raw-url https://github.com/KCall360/thresholds-of-ruin/releases/download/TAG/BUNDLE
```

`add` takes the head side by default (`--side base` for the reference), omits
counts that varied between rounds, and refuses entries that fail the format
check. `python scripts/perf_ledger.py check` validates the file, and the Python
suite runs the same check. The check covers format only; it never applies
timing thresholds.

## Publishing raw measurements

Raw samples don't go in Git. Each measurement set is published as the bundle
from its run directory, attached to its own GitHub release, and the ledger
links to it. **Ask the maintainer before creating a release or uploading
anything.** Once approved:

1. Make sure the measured head commit has been pushed. The release tag points
   at it.
2. Create one release per measurement set, named
   `perf-YYYY-MM-DD-<short-description>`, marked as a pre-release so it's never
   shown as the latest game release. The notes list the cases, commands,
   machine fingerprint, whether the tree was dirty, and limitations.

   ```sh
   gh release create perf-2026-10-01-combat-observations --target COMMIT --prerelease \
     --title "Performance: combat observations" --notes-file NOTES.md RUN/BUNDLE.tar.gz
   ```

3. Download the asset again and check that its SHA-256 matches the local
   bundle.
4. Add ledger lines with `perf_ledger.py add --raw-url` pointing at the release
   asset, and commit them with the change.

## Shared deterministic fixture and trace

The versioned specification is
[`performance-v1.json`](../../crates/server/fixtures/performance-v1.json). The actual
server constructor, Rust benchmark/test orchestration, and Python headless driver
consume this file. Simulation remains free of clocks, filesystem and client code.
No wizard setup or hidden-state client access is needed for this fixture.

`tor-server --regions 1..=256 --actors 1..=8` selects the diagnostic fixture.
Without `--regions`, new games use an authored scenario package. Existing
saves retain their scenario. One requested region is exactly one region; invalid
counts and unsupported fixture versions fail. Every room has comparable 9x9x2
local geometry, an occluder, a wall with a closed door, explicit up/down links,
ordinary joins, and a rotated upper-level join. Every door placement is checked.

The primary trace returns actor 1 to its starting cell with the door closed.
Other actors take scheduled turns; actor 2 moves into and out of LOS while the
remaining actors wait. The live demo advances into another region between
cycles. A separate benchmark traversal discovers every region, so stationary
local cycles are not evidence of flat scaling with accumulated map knowledge.

| Label | Verified behavior |
| --- | --- |
| `wait_same_region` | Accepted wait, unchanged geometry, scheduler/tick progression |
| `move_same_region` | Cardinal and diagonal displacement to the disclosed destination; 100/142 tick costs |
| `cross_region_boundary` | Forward and reverse crossings, authoritative region change, disclosed offsets |
| `cross_boundary_with_los_change` | Rotated crossing reveals/hides named entities and specified cells; reverse traversal restores the scene |
| `move_near_obstacle` | Routing around the occluder reveals a named marker |
| `open_or_close_door` | Both transitions affect disclosure; actor passes through the opened door |
| `change_elevation` | Explicit up/down passages change the observed scene |
| `multi_actor_visibility_change` | Scheduled movement enters/leaves actor 1's LOS and changes its revision |
| `blocked_obstacle`, `blocked_door` | Deliberate rejection, unchanged state/tick/history; excluded from successful mixed totals |

The shared assertions reject unexpected blocked actions. Client driving resolves
door IDs and destination cell keys from current observations and respects
readiness, branch and revision. The Phase A tests exposed and corrected missing
readiness revision changes during same-tick multi-actor handoffs. Readiness is now
part of revision comparison; these handoffs are delivered to real clients.

## Measurement boundaries and reproduction

Run the full release matrix and validate exact ordered coverage:

```sh
cargo run --release -p tor-server --example latency_bench --locked -- --cycles 5 > samples.jsonl
python scripts/performance_report.py samples.jsonl --summary summaries.jsonl
```

Select the intended local save volume explicitly when collecting a baseline.
The example uses the process's temporary directory; on Windows set `TMP` and
`TEMP` for that invocation to a directory on the selected drive. Record the
volume, filesystem, device and free space with the run. A nearly full unrelated
system volume is not representative of a project saved on another drive.

The matrix combines 1/8/64/256 regions, 1/8 actors, 0/100/1,000/10,000 retained
starting actions, and memory/durable modes: 64 cases. `--quick --cycles 1` selects
16 small cases plus short discovery traces; validate with `--quick`. Boundary
workloads are explicitly inapplicable at one region, and multi-actor workloads
at one actor. Exact actor/action ordering follows the fixture and scheduler;
the independent report validator checks counts, history growth and byte totals.

Each command retains individual phase samples. Output includes mean, nearest-rank
p50/p95, maximum and sample count for every label and the successful mixed trace.
The raw records contain the ordered action, actor, expected result, phase timings,
operation counts, start/end history length, rewind count and client memory size.
Case metadata records seed, trace version, commit, dirty working-tree flag,
platform, architecture, build profile, storage mode, cycles and warmup (currently
zero). Small per-label sample counts remain visible and limit tail conclusions.

Observation diagnostics declare `wire_profile_version: 2` independently of the
engine profile and action trace versions. `wire_encoding` times the shared server
response encoder: candidate construction, complete-envelope size counting and
serialization of the selected response. `wire_decoding` times shared bounded
typed decoding of that response. Client application and rendering retain separate
intervals. These measurements occur after the authoritative command interval and
exclude socket transmission, session scheduling and semantic validation.

Each command sample includes `observation_wire`, either null when no observation
was sent or the full/selected complete response byte counts and selection kind.
The wire summary includes the `ServerMessage` envelope and is checked against
those samples, including exact totals and nearest-rank percentiles. Earlier
unversioned diagnostics timed candidate construction as `delta_encoding` and
counted full state/selected update DTOs without their response envelopes. The
comparison tool keeps those historical byte metrics under distinct legacy names;
they cannot establish a like-for-like change in complete-message sizes or CPU cost.

The latency diagnostic also reports `command_call`, covering the complete trusted
driver call including fresh wire-target resolution, and `disclosure_projection`,
covering the subsequent `Engine::state` call including opaque-reference derivation.
These are separate boundaries: projection is outside command execution, and
`command_call` overlaps its internal engine phases. Do not sum overlapping timers
or interpret these trusted-driver measurements as end-to-end queued client latency.
The comparator retains these fields when present; older reports are not treated
as having measured them. Comparisons need identical timer boundaries on both sides.

Top-level measured command phases are exclusive: candidate capture, checkpoint capture, simulation,
navigation refresh, revision-view perception, revision comparison, rewind
snapshot, serialization, underlying write/flush, file sync, replacement, and
publication and region transition. Simulation's internal door perception belongs to simulation;
navigation's scene work belongs to navigation. These nested calls are counted
at their actual simulation call sites, without adding their time twice.
`unattributed` accounts for the rest of total authoritative command latency.

Profiled commands also report a nested `region_acquisition` object. Its
`fallback_reads` and `fallback_builds` count successful synchronous acquisitions;
`prepared_reads` and `prepared_builds` count acquisitions already completed by
the preloader. Resident record-cache hits are excluded. The two read counts sum
to `region_records_read`, the two build counts sum to `regions_built`, and the
prepared counts sum to `regions_prepared`. Prepared counts depend on thread
timing; the complete read/build counts retain their deterministic work contracts.

`fallback_read` and `fallback_build` durations measure synchronous acquisition,
including decoding or package construction, within region-transition time. The
benchmark emits corresponding `region_fallback_read` and `region_fallback_build`
timing summaries. These are nested diagnostics, so adding them to the top-level
phase sum would count time twice. Zero-work commands have zero fallback time;
failed commands do not return a successful command profile. Ordinary unprofiled
commands do not enable the additional acquisition clocks. Streaming metadata
declares `region_acquisition_version`; its validator rejects missing, inconsistent
or negative counts, invalid durations and nested times exceeding transition time.

Command timing measures record encoding and queue admission in
`serialization`. Command-path write/flush, sync, and replacement counts are zero;
those phases remain in the schema from the original synchronous writer. Worker `save_status` reports accepted/durable sequences, pending
bytes/age, batches, application bytes committed, and last batch duration.
Checkpoint diagnostics add the selected sequence, count, encoded size and encoding
time. Restart diagnostics separate loaded history records from simulated tail
records. `--checkpoint-interval` selects the capture interval; zero supplies a
matched full-replay baseline. These
bytes are not SQLite physical I/O. Final flush is outside action timing and
reported separately. Candidate capture now copies decision state with shared
world/item/navigation ownership, without copying history or receipts.
Allocation counts are omitted because no low-impact allocator instrumentation
is established under the workspace's unsafe-code prohibition.

Client application uses a persistent actor-1 observer, sequential updates, and
growing remembered cells. Harness update construction is outside the timer;
there is no harness-owned client clone inside it. Rendering measures the real
ASCII canvas. Updates are applied only when the production server would publish
a changed revision. Client and rendering timings are separate from authoritative
command latency and must not be summed as if they were server phases.

Fixture history setup is outside timing. It is restricted to detached engines;
ordinary wait records, receipts, scheduler state, navigation, and every retained
rewind boundary are checked against normal execution. Setup may omit redundant
wait perception and snapshots that would immediately be evicted. Attaching a
fixture acquires the save lock and persists it before timed commands. Diagnostic
saves also acquire the lock, including attempts to overwrite an active save.

Every matrix case records final save size and normal restart/replay time, which
includes strict frame validation and checkpoint-tail replay; startup does not rewrite the archive. Replay is not replaced with the
fixture seeding shortcut. The report validates disclosed state after restart.
Raw samples must be retained with the source/build that produced them; see
[publishing raw measurements](#publishing-raw-measurements).

## Observable 256-region run

```sh
cargo build --workspace --bins --locked
python scripts/performance_driver.py --regions 256 --cycles 3 --pace-ms 250
```

Use `--bin-dir target/release` after explicitly building the release binaries for
optimized actual-client measurements. `--output` selects a new directory and
refuses reuse. `--stay-open` leaves the spectator window open after completion;
closing it or a startup failure cleans up owned server/headless processes.
Prior saves are retained. Logs, samples, a final presented framebuffer and result
metadata are kept alongside each fresh save. Failures remain visible in logs and
the desktop wrapper surfaces launch errors.

The driver confirms a presented, server-enforced spectator frame before starting
player actions. Each headless admission response and error is checked, followed
by the matching intention execution update before checking effects. It awaits the
matching spectator state after accepted actor-1 actions. Paced runs publish
actor-visible progress annotations outside action timing; their additional saves
make the demonstration distinct from throughput measurements. Spectator and
player credentials remain separate, ephemeral, and out of version control.

`request_to_ack_ms` and `request_to_presentation_ms` are observed at the driver:
they include transport, native client processing and diagnostic JSON reporting
(and framebuffer capture on the presentation path). They are not wire-only or
GPU timestamps. For queued gameplay, acknowledgement measures admission rather
than simulation execution, so acknowledgement timings across that protocol change
do not measure the same endpoint. The historical `request_to_ready_ms` name now
measures receipt of the matching execution update; immediate operations still
finish at their ready response. It does not include a later attack impact after
wind-up has begun. Spectator presentation follows the state checked after execution.
Pacing, snapshot requests and progress annotations are outside these intervals.

For a workload that can advance without another driver command, use
`JsonProcess.send(..., wait_for_completion=True)` to wait through a multi-stage
intention. The driver's `action_timing` record has its own `version: 1` and the
admitted intention, actor, and branch. Its reader-arrival timestamps distinguish
admission, first execution, the matching `resolved`, `failed`, `cancelled`, or
`suspended` outcome, and the subsequent fresh readiness context. Report successful
and unsuccessful outcomes separately; an action still at `started` has no outcome
sample. Immediate requests have no intention timing record. Copy the record before
sending another request, which replaces it.

The default `send` endpoint and historical `request_to_ready_ms` stay unchanged.
Completion waiting is opt-in: a workload with several controlled actors must keep
driving the other actors instead of blocking their turns while waiting for one
actor. The actual-process timing regression covers an attack through its final
authoritative effect and proves that the returned context accepts the next command.


## Stable tests and verification

Rust tests cover exact world sizes, ordinary and rotated joins, LOS, stairs,
door passage, blocked attempts, multi-actor handoff/disclosure, mixed replay,
seed equivalence, save-lock ownership, phase exclusivity, actual bytes and
bounded rewind. Python tests exercise the real headless/ASCII trace and eight
scheduled clients, and check the independent scheduler/report oracle. Ordinary
CI uses an eight-region process case; set `TOR_PERFORMANCE_REGIONS=256` and
`TOR_PERFORMANCE_CYCLES=3` for the complete large demonstration.

Run the workspace checks in [development practices](../../CONTRIBUTING.md), in debug
and release. Windows native mouse tests require an interactive desktop; sandbox
API denial is an environment failure and must be reported or rerun with desktop
access, never silently skipped. Windows/Linux CI is required before merging.

The [background-saving guide](../architecture/persistence.md) describes current storage
and recovery tests. Process tests do not prove hardware power-loss behavior. Checkpoints and logical compaction are covered by the current recovery tests.

## Focused Phase B run

```powershell
cargo run --release --locked -p tor-server --example latency_bench -- --phase-b --cycles 5 --save-target-ms 10 --save-max-ms 50 --save-idle-ms 1 > phase-b.jsonl
```

This selects eight-region cases at 100/10,000 actions and one/eight actors,
matched memory cases, and one 256-region/eight-actor/10,000-action saved case.
The short policy deliberately exercises saves during the mixed workload.
Production defaults remain configurable and are listed in
[background saving](../architecture/persistence.md). Use the current raw schema when
interpreting asynchronous worker metrics; the Phase A report's physical I/O
counts are not interchangeable with application journal bytes.

## Focused checkpoint run

```sh
cargo run --release --locked -p tor-server --example latency_bench -- --phase-c --cycles 3 --save-target-ms 10 --save-max-ms 50 --save-idle-ms 1 --checkpoint-interval 1024 > phase-c.jsonl
python scripts/performance_report.py phase-c.jsonl --phase-c
```

This runs the four saved eight-region cases (one/eight actors, 100/10,000
retained actions) and the 256-region, eight-actor, 10,000-action case. Run it with
`--checkpoint-interval 0` as well, using identical workloads and save timing, to
compare against full replay. The report covers capture cost, worker encoding size
and time, command p50/p95/maximum, and restart loaded/replayed record counts. Add
`--case NAME` to either command for a single case.

## Focused Phase D and growing discovery

```sh
cargo run --release --locked -p tor-server --example latency_bench -- --phase-d --cycles 3 --save-target-ms 10 --save-max-ms 50 --save-idle-ms 1 > phase-d.jsonl
python scripts/performance_report.py phase-d.jsonl --phase-d
```

This runs the nine Phase B scale cases and complete traversal of eight and 256
regions. `--discovery-only` runs just those two traversals; pass the same flag to
the report. `--case NAME` still selects one mixed case. Discovery keeps every
received cell in the same client observer and grows authoritative navigation.
The original fixture version 1 and action ordering are unchanged. Profiling schema
version 2 adds contracts for zero observation/scene work on waits and separate
bounds on fresh observations and scene computations on other actions. Cached
scene projections can support fresh disclosure without another scene computation.
Retained version-1 profiling remains valid.
Do not interpret a mixed median dominated by scheduled waits as movement latency;
retain per-label distributions and individual tails.

## Client workloads

```sh
cargo run --release --locked -p tor-client-ascii --example client_bench > client.jsonl
python scripts/client_performance_report.py client.jsonl
python scripts/performance_driver.py --bin-dir target/release --output target/client-capture --regions 256 --actors 8 --cycles 1 --pace-ms 0
python scripts/performance_driver.py --bin-dir target/release --output target/client-no-capture --regions 256 --actors 8 --cycles 1 --pace-ms 0 --no-capture
```

The current separate client workload version 3 presents 64/20,956 disclosed
cells and retains at most 12,474 appearance records. Its fixed 49-by-25 plane
geometry populates z=0 upward, so the normal -2..3 viewport contains exactly
64/4,900 chart cells. The fixture keeps 64 current cells, bursts of 1/64 updates
and 20 samples per combination (80 total), applying each burst before one draw.
Setup and count inspection are outside timing; update cloning and validation are
included. This stationary synthetic workload does not measure authoritative
exploration or reconnect cost. Historical versions 1/2 retain their original
connection-cache counts in the validator and are not relabeled as current results.

For semantic narration, run `cargo run --release --locked -p tor-client-ascii
--example client_bench -- --narration` and validate with
`python scripts/client_performance_report.py FILE --narration`. This selects
version 4 with actor and door sight changes; omitting the flag selects version 3.
Version identity remains part of matched comparisons. See
[narration and stream recovery](../architecture/clients.md).

For immutable observation ownership, run `cargo run --release --locked -p
tor-client-ascii --example client_bench -- --ownership` and validate with
`python scripts/client_performance_report.py FILE --ownership`. This diagnostic
pairs deep state copies with shared handles for 64/4,096/20,956 disclosed cells
and 1/8/32 readers, alternating method order over 100 samples per combination
(1,800 rows). Setup, serialization, equality checks and pointer inspection stay
outside timing; reader-vector allocation is included. The validator checks
complete paired samples, payload consistency, distinct retained object counts
and finite timings. Distinct serialized content measures logical duplication,
not heap allocation or resident memory. These isolated clone timings do not
measure server publication, transport, rendering or historical map retention.
The ownership and narration modes are mutually exclusive.

Compare against a reference with `perf_compare.py BASE --case client` (see
[before-and-after comparisons](#before-and-after-comparisons)), which builds
and runs matched binaries and keeps raw samples, sample counts, percentiles,
binary hashes and the machine fingerprint. The validator enforces exact ordering, version, memory/chart
counts and finite nonnegative phase timings, not machine-specific latency gates.
The allocation-retention and glyph-oracle Rust tests provide stable regressions.

Actual-client samples now retain native frame phase diagnostics when available.
`--no-capture` isolates PPM writing while preserving native presentation and JSON
reporting. Default capture behavior is unchanged for existing workloads and the
desktop demonstration. The driver timestamp includes transport, scheduling and
diagnostic work; it is not interchangeable with pure update/draw timing. See the
[ASCII diagnostics](../play/ascii.md#diagnostics-and-acceptance).

`request_to_ack_ms` retains its historical driver-consumption timestamp. New runs
also include `request_to_ack_line_ms`, stamped when the reader receives the JSON
acknowledgement line, before diagnostic log writing/flushing and queue delivery.
Their difference isolates delay inside the driver; neither is a server-only time.
Existing retained reports without the new field remain valid.

## Saved-discovery measurement

```sh
cargo build --release --locked -p tor-server --example latency_bench
# Complete journal-only comparison; does not meet the checkpoint gate:
target/release/examples/latency_bench --saved-discovery --checkpoint-interval 0 --save-target-ms 10 --save-max-ms 50 --save-idle-ms 1 > saved.jsonl
python scripts/performance_report.py saved.jsonl --saved-discovery
# Checkpoint-enabled full traversal:
target/release/examples/latency_bench --saved-discovery --checkpoint-interval 1024 --save-target-ms 10 --save-max-ms 50 --save-idle-ms 1 > checkpoint.jsonl
```

Use `.exe` on Windows and explicitly select the save volume with TMP/TEMP.
`--saved-discovery` implies `--discovery-only`: the unchanged full eight/256-region
traces now attach a real save before any exploration. Successful completion flushes
and reloads the ordinary save and compares final state. Each completion includes
save size, flush/restart timing, saved-prefix/replay counts, and an offline count
of the current logical checkpoint JSON. The counting writer does not allocate a
payload or write a save. Its duration includes capture and deduplication, outside
action/flush timings. Scale comparisons and JSON headroom budgets use this logical
count, independent of compression. A separate compressed counting diagnostic uses
the same streaming encoder and 64 MiB limit as storage. It uses the durable save
identity when attached, or a dummy UUID for an unattached estimate, and the ordinary
workload's record count as sequence; attached counts match stored payload bytes.
Save status reports the actual compressed checkpoint bytes.

Interval 64 supplies a stress comparison and a successful eight-region checkpoint.
An incomplete traversal emits failure records, which the report validator
rejects; both enabled intervals must complete. Never report an accepted prefix as
a complete benchmark pass. Background timing affects where a failing run stops.
Existing detached modes, workload ordering and report interpretation are
unchanged.

## Explored-save native acceptance

```sh
python scripts/saved_exploration_driver.py --bin-dir target/release --regions 256 --checkpoint-interval 64 --output target/explored-native
```

This opt-in driver uses ordinary native ASCII input events to execute every
version-1 traversal action, from a newly attached save. It checks each presented
movement/door result, counts the union of disclosed cells, flushes, restarts,
compares state/branch/history, and continues through ASCII and text inputs.
It retains presentation phase samples and exact action order. Its validator
requires complete coverage, a checkpoint under 16 MiB and a tail shorter than
the configured interval. UI-event-to-reader timing includes native presentation
and diagnostic I/O; it is not physical keyboard-to-photon latency.

The process suite runs eight regions and native OS keyboard input during a
blocked checkpoint on that genuinely explored save. Set
`TOR_SAVED_EXPLORATION_REGIONS=256` for the complete large acceptance case.
The restarted client hydrates its bounded nearby map from persisted actor
knowledge. The traversal driver separately retains every disclosed observation
throughout exploration for its coverage audit.

## Opt-in timing correlation

Pass `--correlate` to either actual-client driver to enable
`TOR_TIMING_DIAGNOSTICS` only in its child processes. Diagnostic stderr records
request UUIDs, client request/send/ack boundaries, headless output durations,
server mailbox-wait/handler durations (`lock_ms` is how long a request waited in
the simulation's mailbox) and acknowledgement send completion. It omits tokens,
request bodies, private world state and protocol changes. Server timestamps are
captured at the producing boundary; a separate worker writes stderr. Its queue
holds at most 256 records, each with at most 16 KiB of variable detail. Save
warnings use the same worker; client warnings and durability barriers are
independent of diagnostic delivery. Direct `Service` use has no implicit console
writer; `Simulation::start` installs the host sink.

Delivery is best effort. Queue overflow, oversized detail and writer failure
count lost records; when writing resumes, `server_diagnostics_dropped` reports
coalesced loss. Timing correlation rejects such captures rather than using a
possibly biased subset. Writer failure stops delivery without blocking play.
Server shutdown does not join a blocked diagnostic writer or guarantee a log
flush; an in-process worker may outlive its server until the write returns.
Client and driver diagnostic I/O can still stall and remains included in their
measured boundaries.

```sh
python scripts/performance_driver.py --bin-dir target/release --output target/correlated --regions 256 --actors 8 --cycles 3 --pace-ms 0 --no-capture --correlate
python scripts/timing_correlation.py target/correlated --output target/correlated-timings.json
```

The report joins accepted actions by request identity and retains the original
acknowledgement and line-receipt metrics.

Outbound frames are encoded once by the service before queue admission.
`server_handled` therefore includes encoding, while `server_ack_sent` measures
the socket send/flush of the prepared frame. Preserve that distinction when
comparing captures from earlier builds that encoded in the socket task.

Native exploration additionally retains all intervening frame profiles and
reader work/queue delay. Cross-process timestamps
use the same host wall clock; the Python reader calibrates its monotonic clock
once and timestamps the exact recorded boundaries through that offset. Phase
durations use monotonic clocks. Clock drift or adjustments can limit cross-process
attribution, and a receiver can observe bytes before the sender returns. Signed
cross-process deltas are retained rather than clamped. Headless report maxima
use the client log's request-to-matching-acknowledgement-report order, including
intervening reports and excluding reports outside that causal window; wall-clock
alignment never determines which reports belong. These are diagnostic boundaries, not wire-only
or physical keyboard-to-photon measurements. Ordinary play leaves this option off.

The client timing records additionally carry `previous_timing_write_ms`: the
complete preceding client diagnostic call, including construction, synchronous
stderr writing and any scheduling within that call. Join it to the preceding
client event, not the current event. In particular, `client_send_ms` still includes
the request-start diagnostic; a following sent-record write can delay reading an
acknowledgement already available from the server. Missing final write costs stay
unknown. Historical files without the field remain valid.

Native frames additionally include `presented_unix_ns`, sampled after the native
presentation call returns and before capture/report construction. The following
frame's `previous_report_encode_ms` and `previous_report_write_ms` separate JSON
construction/encoding from stdout writing/flushing. The original total report,
turn, reader and driver boundaries remain unchanged. These are wall durations:
a long write can contain pipe backpressure or descheduling, and a short native
call does not establish photon delivery. Cross-process clock caveats still apply.

Both drivers accept `--defer-logs` for a targeted diagnostic-I/O comparison.
Decoded stdout/stderr lines are retained until each child stops, then written to
their usual files. UTF-8 content is capped at 512 MiB stdout and 16 MiB stderr per
child; Python string/container overhead is additional. Overflow fails the run,
preserves the retained prefix and partial samples, and attempts cleanup of every
owned child. This opt-in experiment does not change runtime transport/event queues
or suppress frame reporting. It moves diagnostic disk writes outside measured
actions; JSON work, pipe I/O, Python parsing/retention and scheduling remain inside.
The default synchronous stdout logging and historical workload meanings remain.
Use fresh directories and retain failed runs; never interpret a rejected prefix
as full traversal. Current timing findings are in the
[performance plan](performance.md#open-work).

## Durable place workload v1

The independent [place-knowledge-v1 specification](../../crates/server/fixtures/place-knowledge-v1.json)
adds 0/64 isolated rooms with four perceived anchors per added room, producing
2/258 learned places. It leaves `performance-v1` unchanged. Setup uses authorized
wizard edits as defined by this immutable workload version. Samples alternate
50 free renames and 50 ordinary waits per case, with periodic durable checkpoints.

```sh
cargo run --release -p tor-server --example place_bench --locked > places.jsonl
python scripts/place_performance_report.py places.jsonl > places-summary.jsonl
python scripts/place_performance_driver.py --bin-dir target/release --output target/places-native-run
```

The engine harness times discovery/navigation, command processing, observation
construction, wire encoding, shared client application, and ASCII place-list
rendering separately. Update construction is outside client application; rendering
does not include native presentation. Raw records include sample/operation counts,
wire bytes, checkpoint/save bytes and exact durable restart validation. The validator
rejects missing/duplicate samples, incorrect scale, nonfinite timing, extra navigation
refreshes on free renames/waits, and missing recovery. Timing is diagnostic.

The real-client driver uses the same specification, actual headless and native ASCII
processes, and an open spectator place list. Request-to-ack/ready/presentation include
transport, scheduling and stdout diagnostics; native draw/apply timings stay separate.
It uses capped deferred diagnostics, preserves failures and fresh saves, and cleans
up only owned processes. Diagnostic output remains outside timed samples where
possible; native/headless stdout costs and the outstanding 3p timing limitations
still apply. Do not compare these end-to-end intervals directly to engine phases.

Run matching ordinary `latency_bench --case` cases before and after feature edits,
and these small/large feature cases on the same release build and machine. No
latency target or historical 3p finding is relaxed by this workload.

Use `place_performance_driver.py --fresh-player` with a different output directory
for a distinct attachment case: durable names and last-seen map knowledge remain,
while local history retention resets and the bounded map cache hydrates. The
driver also records ack
line arrival, ready reader/queue work, cell counts and the size of ready JSON
re-encoded by Python (not network byte counts). Keep both cases; see the
[archived place-knowledge findings](https://github.com/KCall360/thresholds-of-ruin/blob/docs-history-2026-09/docs/place-knowledge-findings.md).

## Scenario package workload

`cargo run --release -p tor-server --example scenario_bench --locked` reports
20 samples each for 2 and 256 authored regions. It measures source integrity
checks, initial game construction, one ordinary wait, durable creation, explicit
saving, and checkpoint restart, plus one offline validation interval and source/save
bytes. Every restart must match the saved actor state and validation status. The 2-region case also measures the
original in-code fixture constructor on the same build. The 256-region fixture
adds disconnected authored chambers to measure input scaling, not exploration.
Timing ends before JSON reporting. Keep p50/p95/max and raw samples; these are
diagnostics, not machine-independent test thresholds. Pair this feature workload
with unchanged small/large `latency_bench --case` comparisons.

## Item quantities and knowledge

See [items and character knowledge](../reference/items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.

## Combat workload v1

`combat_bench` adds a separate workload without changing `performance-v1`.
It derives ordinary validated packages from `scenarios/tests/dungeon-loop`, scales
to 2/8 combat actors and 0/1,000 preceding commands, and runs three samples per
combination. High HP keeps combat active for 64 measured commands per sample.

```sh
cargo run --release -p tor-server --example combat_bench --locked > combat.jsonl
python scripts/combat_performance_report.py combat.jsonl > combat-summary.json
```

External `decision_ms` and profiled engine work are separate. For AI turns the
external interval selects the next actor; `advance_ai_profiled` computes the AI
decision once inside the engine interval. For the controlled actor it includes
observation-based target/action selection and decoding. It is not evidence of a
duplicate AI decision. Phase totals over measured commands identify
simulation, perception, navigation, revision comparison, and checkpoint-capture
cost; reported phase means divide measured totals by the 192-command case size.
Navigation-refresh counts cover only those measured commands. Disclosed updates are constructed outside the
client-application interval; ASCII canvas drawing excludes native presentation.
Save barriers and exact checkpoint restart are measured separately. Scene/body
work counts include setup-history commands as well as the measured 64-command
window; disclosed bytes describe the final actor state. Save bytes describe the
closed durable database. The validator rejects incomplete scale/sample matrices,
wrong command counts, and nonfinite intervals. Native correctness is covered by
`scripts/test_dungeon_process.py`; this benchmark does not claim transport or
input-to-display latency.

For a same-machine comparison, run the combat workload together with matching
ordinary cases:

```sh
python scripts/perf_compare.py BASE --case combat --case r8-a1-h100-memory --case r64-a8-h100-memory
```

Keep raw samples, p50/p95/max and operation counts with the feature findings in
[dungeon gameplay](../play/gameplay.md). Do not mix results collected during builds or
process-test runs. Existing provisional targets and deferred 3p work still apply.


## Ordered-strike workload v1

`strike_bench` is a head-only repeatability workload for the ordered sequence
engine. It does not emulate these mechanics on a revision without them or claim
an A/B improvement. Keep ordinary unchanged `combat` workload version 1 and latency comparisons
separate. The `strikes` registry entry uses this example and its own validator;
a future matched-revision comparison requires the same workload on both sides.

Run with no other builds, tests or measurements active:

```sh
cargo run --release -p tor-server --example strike_bench --locked > strikes.jsonl
python scripts/strike_performance_report.py strikes.jsonl > strikes-summary.json
```

Repeat that complete invocation into a separate raw file on the same machine
and build. Retain both raw samples and summaries with source/binary hashes,
machine/storage identity and toolchain. Each run has 72 rows: strike/rider pairs
(1, 0), (3, 2) and (16, 8), crossed with 2/256 loaded actors, 16/256 loaded regions,
checkpoint/tail modes and seeds 42, 43 and 44. Every row is one complete sequence,
not a batch mean. Summaries retain n/p50/p95/maximum; three sequence-level samples
make p95 the maximum, not a reliable tail estimate. For a correctness smoke run,
pass `-- --smoke` to the example and `--smoke` to the validator. This is the complete
three-seed, two-mode (3, 2, 2, 16) subset; the default validator rejects it.

The example builds ordinary validated packages through `Package::from_parts`.
Streaming is disabled, so construction loads all authored, disconnected bounded
chambers. The attacker and adjacent target share chamber one; other creatures
are distributed without overlap among the remaining rooms. Every actor waits
once during setup, parking background actors at distant deadlines. These are
loaded-creature and catalog scaling cases, not exploration or a densely packed
combat crowd. Preparation and recovery are each 100 ticks, strike offsets are
100 times the ordinal, and the externally controlled defender waits 150 ticks.
Primary and success-triggered rider checks use real seeded 3d6 rolls; high Health
prevents lethal skips. Primary hits and actually reached rider resolutions are
recorded rather than assuming all riders run. The maximum authored strike/rider-
count layout is sixteen strikes with eight one-effect riders each; it is not
worst-case executed rider work, the aggregate effect/component source-size
maximum, or the separate equal-offset case.

This fixture has one attacker check per resolved primary and per reached rider,
with no immunity, movement or death to bypass those checks. Its actual primary
and rider check counts therefore correspond to `strike_resolutions` and
`rider_resolutions`. Skipped rider slots are `strikes * riders - rider_resolutions`.
These are source-supported derivations for this fixed workload, not an independent
generic check counter. Separate forced-all-rows core counter tests cover the
structural execution bound; rolled misses remain part of every timing case.

`admission_start_ms` is the first direct `command_profiled(Command::Act)` call,
including frozen-plan capture. It is not a wire acknowledgement or asynchronous
intention-admission latency. `command_ms` retains each start/wait interval;
`sequence_ms` sums those intervals and `advancement_ms` sums only the waits.
Simulation, perception, navigation, revision comparison and checkpoint capture
phase totals retain their own names. Operation counters are differences taken
immediately around each measured command, excluding setup, diagnostic reads and
replay. They include normal Engine candidate cloning, live creature derivation
and observation/revision work; they are not a claim of zero Engine clones or a
heap-allocation count.

Plan JSON bytes are counted by a narrow read-only Engine diagnostic directly
from the retained frozen Plan, with no encoded-payload allocation or operand
recapture. Only its byte count and serialization duration enter the report.
Logical checkpoint bytes and encoding time, closed-save bytes, setup work,
projection/equality work, durable barriers and both reopen intervals are separate
measurements. No private frozen operands or full state are written to JSONL.

Both persistence modes first create and flush a real setup checkpoint with
interval 1, then reopen outside the sequence timing. The selected measured policy
uses interval 1 for checkpoint recovery and 4,096 for a bounded journal tail.
A one-strike sample saves pending work; longer samples save after the first
completed strike. `RecoveryProfile` counters must prove the selected nonzero
checkpoint and exactly zero or the expected bounded number of replayed commands.
After that reopen, the sample continues normally to completion and flushes/reopens
again. Each reopen compares every actor's exact disclosed state and paginated
disclosed journal, including entry identities; final own Health, sequence progress
and recovery debt are therefore included. This is disclosed-state/journal equality,
not an assertion that the workload compared every private authoritative field.
Strict production replay checks and the sequence persistence tests independently
cover authoritative save integrity. These intervals exclude transport and native
client presentation and do not close existing performance targets or disk tails.

### October 2026 measurement checkpoint

An isolated release run compared merged Health commit
`7e91f9024f1ba0a147fc11adfdb0cfcaa86845b8` with measured strike tree
`4e45f46e331259e34dad15cddc78be4d2f73d547`, then ran two complete head-only cohorts
on the same binary. The tree identifies the measured contents independently of
any later publication commit mapping or documentation-only follow-up.
The machine fingerprint was `c14b64955bd2`: AMD EPYC 9V74, 9 reported logical
CPUs, 9.7 GiB RAM, Debian 13 / Linux 6.18.44, Rust 1.99.0. Saves used an overlay
filesystem; the physical storage type/model was unknown. Raw evidence remains
local, these findings are provisional, and no headline ledger entry is accepted.
Any future raw publication requires separate approval; it is not a prerequisite
for this feature's code publication. No public raw-evidence URL is claimed.

Ordinary `combat` version 1 used four balanced ABBA rounds with three seeded
samples per case and 64 measured commands per sample. Each side has 768 command
intervals per case. The table is p50/p95/maximum in milliseconds:

| Actors / history | Health baseline | Strike head | Command p95 change |
|---|---:|---:|---:|
| 2 / 0 | 0.414/0.626/2.307 | 0.426/0.710/6.986 | +13.4% |
| 2 / 1000 | 0.456/0.704/2.571 | 0.466/0.685/3.134 | -2.7% |
| 8 / 0 | 1.191/1.720/3.358 | 1.186/1.712/3.645 | -0.4% |
| 8 / 1000 | 1.185/1.771/4.418 | 1.171/1.854/6.375 | +4.7% |

Scene/body-cell counts were unchanged. Disclosed bytes increased by 48, 81, 114
and 48 respectively. History-zero saves had unchanged sizes; history-1,000 saves
grew by 12,288 bytes with two actors, and their across-run size range grew from
7,782,400–7,786,496 to 7,815,168–7,819,264 bytes with eight actors. Timing changes
were mixed, not a uniform improvement. Save/reopen each had only 12 samples per
case, making p95 the maximum. Eight-actor save maxima increased from 1.491 to
3.701 ms at history zero and from 44.479 to 69.550 ms at history 1,000; these
outliers were retained.

Both 72-row strike cohorts passed the complete-matrix validator and both
release example tests passed. Each exact case has three seeds per cohort, so
its p95 is just its maximum. Frozen plan JSON sizes were 608, 3,405 and 47,998
bytes for the 1/0, 3/2 and 16/8 layouts. Every sample captured one plan and
resolved all its primaries. The 16/8 rows reached 88–104 riders from 11–13 primary
hits; they do not time an all-success or maximum-payload-size source.

Two-actor sequence samples stayed below 2.3 ms, while 256-actor samples reached
106.096 ms. Preparation candidates were 8/14/68 for two actors and
1,024/1,792/8,704 for 256 actors across the three layouts. Engine clones remained
four per measured command in checkpoint mode and three in tail mode; no
per-rider clone or plan recapture was observed. The actor scan and normal Engine
comparison/projection work are still present. Changing region count also changes
the background actors’ room distribution, so these rows do not establish
constant-world scaling or isolate region count from geometry.

At 256 actors, logical checkpoint JSON reached 42,646,516 bytes, distinct from
the frozen plan size. Prefix/final reopen samples peaked at 2,695.810/2,576.852 ms,
and disclosed-state/journal comparison at 1,042.162 ms. These intervals remain
separate from sequence timing. Checkpoint rows replayed no measured commands;
journal-tail rows replayed exactly 1/2 prefix commands and 2/3/12 final commands
after a real nonzero setup checkpoint. The workload does not identify whether
any density or recovery cost was introduced by this stage.

#### Logical repeatability and physical footprint

The original cross-cohort check requiring equality of every non-timing field
**failed**. Its complete difference set was `saved_bytes` in 25 rows and
`final_saved_bytes` in 27 rows: 30 distinct seed-matched pairs, all at 256 actors.
Deltas ranged from −16,384 to +12,288 bytes and were multiples of 4,096. The cause
was not isolated; this is not evidence for a particular allocation or compression
explanation. The failed comparison was retained without changing its criterion
or rerunning the cohorts.

All other 47 non-timing fields matched exactly for each case/seed, including
logical plan/checkpoint/disclosed byte counts, operation/recovery counts,
hits/Health and reopen equality flags. Physical SQLite file length is a separate
footprint measurement here, not an exact logical-determinism invariant. These
are its exact per-case ranges in bytes across the three seeds in both cohorts;
variation across different seeds is included as well as repeated-seed variance:

| Strikes/riders | Actors | Regions | Recovery | Prefix save bytes | Final save bytes |
|---|---:|---:|---|---:|---:|
| 1/0 | 2 | 16 | checkpoint | 65,536 | 65,536 |
| 1/0 | 2 | 16 | tail | 65,536 | 65,536 |
| 1/0 | 2 | 256 | checkpoint | 229,376 | 229,376 |
| 1/0 | 2 | 256 | tail | 225,280 | 225,280 |
| 1/0 | 256 | 16 | checkpoint | 4,145,152–4,177,920 | 4,149,248–4,177,920 |
| 1/0 | 256 | 16 | tail | 2,441,216–2,453,504 | 2,441,216–2,453,504 |
| 1/0 | 256 | 256 | checkpoint | 4,313,088–4,321,280 | 4,313,088–4,329,472 |
| 1/0 | 256 | 256 | tail | 2,605,056–2,613,248 | 2,605,056–2,613,248 |
| 3/2 | 2 | 16 | checkpoint | 69,632–73,728 | 73,728–77,824 |
| 3/2 | 2 | 16 | tail | 69,632 | 69,632 |
| 3/2 | 2 | 256 | checkpoint | 233,472–237,568 | 237,568–241,664 |
| 3/2 | 2 | 256 | tail | 229,376 | 229,376 |
| 3/2 | 256 | 16 | checkpoint | 4,153,344–4,173,824 | 4,165,632–4,186,112 |
| 3/2 | 256 | 16 | tail | 2,437,120–2,457,600 | 2,437,120–2,457,600 |
| 3/2 | 256 | 256 | checkpoint | 4,313,088–4,325,376 | 4,317,184–4,337,664 |
| 3/2 | 256 | 256 | tail | 2,600,960–2,613,248 | 2,600,960–2,613,248 |
| 16/8 | 2 | 16 | checkpoint | 118,784 | 159,744 |
| 16/8 | 2 | 16 | tail | 114,688 | 126,976 |
| 16/8 | 2 | 256 | checkpoint | 282,624 | 319,488 |
| 16/8 | 2 | 256 | tail | 274,432 | 286,720 |
| 16/8 | 256 | 16 | checkpoint | 4,202,496–4,222,976 | 4,247,552–4,263,936 |
| 16/8 | 256 | 16 | tail | 2,490,368–2,502,656 | 2,490,368–2,502,656 |
| 16/8 | 256 | 256 | checkpoint | 4,358,144–4,378,624 | 4,399,104–4,427,776 |
| 16/8 | 256 | 256 | tail | 2,650,112–2,654,208 | 2,650,112–2,654,208 |

Small sample sizes, uncharacterized storage and the stated workload boundaries
limit these results. They do not close native input latency, worst-case rider
execution or large-world recovery goals, or establish public raw-evidence
availability.

## Wire decoder diagnostic

Run `cargo run --release -p tor-protocol --example wire_decode_bench --locked -- 200`
to compare ordinary typed Serde parsing with the bounded decoder in the same
executable. It uses identical complete JSON messages, alternates method order,
and checks every result outside timing. Cases cover a small hello, the request
byte ceiling, and synthetic disclosed snapshots with 8, 256 and 4,096 cells.
The large hello measures codec construction rather than account authorization;
the snapshots measure DTO parsing rather than world geometry or topology.

The version-1 JSON-lines report contains a header, one sample per case/method,
and an end record with the expected sample count. `decode_ms` includes the
bounded preflight when selected and typed DTO construction. Equality checks,
DTO destruction, semantic validation, networking and presentation are outside
the interval. Keep n, p50/p95/max, method order and encoded bytes; retain raw
samples locally until publication is authorized. This is a same-build method
comparison, not a historical commit comparison or an end-to-end latency claim.
Pair it with the existing targeted release comparisons when publishing a
protocol checkpoint; engine timings do not measure native JSON decoding.


## Individual observation collection diagnostic

Run `cargo run --release -p tor-protocol --example observation_wire_bench --locked -- 100`
with no other local builds or tests running. It compares full and selected complete
responses in the same executable, alternating order for identical observations.
Each case contains four disclosed collections of 16, 256 or 4,096 entries, with
unchanged contents, separated sparse edits, projected movement or reordered values.
It records complete bytes, inserted candidate values and separate encoding,
bounded decoding and reconstruction/validation timings. Every sample must reproduce
the validated next full state through the shared codec and delta application.

This synthetic diagnostic varies individual observations; the ordinary region-count
benchmark can expose the same observation size at both region counts. It excludes
transport, client context tracking, memory retention, rendering and engine work.
Inserted values count encoded candidate contents, not all allocations or client
copying. Full versus selected is a method comparison, not a prior-version latency
claim. Keep raw samples local until publication is authorized, and retain the
ordinary interleaved release engine comparison for scheduling and persistence tails.

## Lighting and range diagnostics

For lighting diagnostics, `fov_bench` reports fresh sight at ranges 8, 12 and 16
in open space, pillars, rotated joins, portal cycles, long rooms and dark
corridors, with one and eight observers. `rogue_bench` adds the same ranges for
generated floors at depths 1, 6 and 11. Its geometry-only `tests/rogue-geometry` fixture preserves the original workload;
these results do not measure the populated Rogue package. Its `rogue-descriptor-v2` workload samples
depths 1 through 26 from the open dungeon descriptor, not a 26-floor world.
The workload identity differs from the historical version-1 authored-floor
benchmark because geometry and random streams changed; retain older ledger
results under their original identity rather than treating them as matched runs.
These isolated geometry samples flatten
chunk coordinates, while its ordinary action/acquisition rows retain actual
chunk streaming and observation byte counts. Keep n/p50/p95/max for each row.
`physics_bench --profile` additionally retains per-command production phase
profiles and perception work counts, plus checkpoint encoding size/time and
completed save status outside measured action and barrier intervals. The default workload and matched baseline
comparisons remain unprofiled; use the optional instrumented run to locate costs,
not to replace those timing comparisons.
The completed status reports `last_commit_ms`, the SQLite commit-call portion of
`last_batch_ms`; checkpoint encoding remains separately reported. The physics
process acceptance test validates these metrics against the completed durable
prefix, and the report validator rejects malformed or inconsistent durations.

For an explicitly selected correctness smoke, run:

```sh
python scripts/test_runner.py physics_smoke_process.PhysicsCorrectnessSmoke.test_all_shapes_preserve_actions_draw_flush_and_resume
```

Select `TOR_TEST_PROFILE=debug` or `release` in the environment as usual. This
invokes `physics_bench --profile --smoke`: all eight population/footprint/falling
shapes run once, with the same commands, client application/drawing, durable
flush, completed-status assertions and exact reopened-state check in every
sample. The ordinary discovered physics process test still runs all eight
shapes three times; the smoke module is outside default `test_*` discovery and
does not replace a required CI check. Raw smoke output requires the report
validator's explicit `--smoke` option. The default producer, default validator
and `perf_compare.py` retain the complete three-sample matrix. One-sample
correctness smoke is not release timing or tail-performance evidence.
The example and the existing server unit-test target share the private argument
parser. Its three argument-contract tests run with the ordinary server tests in
every CI profile, without building a separate example test executable.



## Streaming evidence

The `streaming-v1` cases `stream-r16-memory`, `stream-r16-durable`,
`stream-r256-memory` and `stream-r256-durable` run a 70-step east-and-back walk
at default radii, detaching and rebuilding halls each cycle. Select them with
`perf_compare.py --case` and validate with `scripts/performance_report.py`.
The stable gate compares horizon, pin, reach, record and build work between the
same walk in 16 and 256 halls; it must not grow with disconnected world size.

The original five-cycle, 700-command Windows release cohort had similar command
and transition distributions across those sizes. Region-first tables also reduced
detach work relative to scanning the world. The original notes did not fingerprint
the host, so these results cannot establish cross-host latency comparisons.
The [immutable streaming record](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/region-streaming.md#L442-L507)
preserves the measured before/after figures, including added pin/body lookups,
replay overhead and save timings too noisy to compare.

The separate `structural-horizon-v1` diagnostic runs 10,000 queries after 100
warmups against 5, 256 and 8,192 catalog regions. Those synthetic sizes do not
set package limits. Its [September 27 Windows baseline](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/region-streaming.md#L68-L106)
records build time separately; sub-microsecond query timing includes timer and
scheduling noise. Operation counts, not those timings, establish scaling.

## Persistence evidence

### Explored-map checkpoint capacity

The existing
[`checkpoint_preserves_queued_admissions_across_both_full_retained_windows` test](../../crates/server/tests/it/checkpoints.rs)
keeps 64 actors, range-16 sight, and both complete retained windows (256 boundaries).
Its logical JSON budget is now 2 MiB; its compressed checkpoint budget remains
1 MiB. The
[`explored_saved_fixture_checkpoint_matches_counting_diagnostic_and_reloads` test](../../crates/server/tests/it/performance_workloads.rs)
keeps the complete eight/256-region traversal, periodic captures and three-record
replay tail, with the unchanged 16 MiB logical budget. No actors, exploration,
retained states, exact recovery comparisons, duplicate retries or queued-work
assertions were removed to meet these limits. Both budgets are bounded regression
contracts, separate from the production codec's unchanged 64 MiB limits.

Both exact tests passed on October 17, 2026 at unchanged source
`899c1e3794953e3d609c4ffc32ba5756c5d439c0`, tree
`c177bbf0cb409a862161c8ede28835b2af6ff5ad`. Their shared server integration-test
executable had SHA-256
`9cbaf38cfa3644a6accf44ebd7f5ae9b303129a9ec6e9b0ef55b69e4b4aefc31`.
It used Rust 1.99.0, the dev profile at opt-level 1 with debug assertions and
overflow checks enabled, and the `service-contract-host` feature. Each test ran
once; these are correctness-run diagnostics, not a release benchmark or matched
before/after timing comparison. The corrected production writer measured:

| Workload | Logical JSON bytes | Compressed checkpoint bytes |
| --- | ---: | ---: |
| 64 actors, both retained windows | 1,541,092 | 188,735 |
| Eight explored regions | 666,872 | 70,319 |
| 256 explored regions | 5,247,096 | 394,708 |

Compressed counting matched actual stored payloads. The existing disclosed-state,
retained-window, duplicate-retry and queued-work recovery assertions passed; the
exploration test replayed exactly three tail records after each reopen.

The first production Rust map representation failed the logical assertions at
20,427,808 bytes for the queue case and 49,987,572 bytes for 256 explored regions.
A read-only Python palette/columns/masks and owner-directory model of those
captures produced 1,541,092 and 5,244,253 logical bytes respectively, round-tripping
all four original map tables: cells, neighbors, witnesses and children. Those
model results remain distinct from the corrected Rust measurements above.

Removing only the queue capture's shared and per-snapshot explored-map fields
left 1,003,010 bytes, only 45,566 bytes below the former 1 MiB logical limit.
That counterfactual retained every other captured value and is not a map-free
baseline execution. The approved 2 MiB queue limit accommodates durable map
knowledge within the unchanged workload; the correction does not change the
navigation codec to recover the old envelope.

The production representation uses a sorted unique appearance palette, explicit
region-local columns, six-neighbor masks, full exceptional/ambiguous evidence and
shared owner directories. Model numbering and Python compression estimates are
not measurements of the Rust serializer's output.

The same correctness runs reported these server-only durations in milliseconds,
rounded to three decimal places:

| Workload | JSON capture/encode | Compressed capture/encode | Checkpoint flush wait | Tail flush wait | Reopen |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 actors, both retained windows | 68.580 | 76.598 | 89.762 | — | 210.839 |
| Eight explored regions | 10.476 | 18.270 | 19.519 | 0.161 | 109.880 |
| 256 explored regions | 122.057 | 159.352 | 190.742 | 0.174 | 3,059.407 |

The two server tests emit diagnostic `checkpoint_capacity` JSON records when run
with `--nocapture`. `json_capture_encode_ms` and
`compressed_capture_encode_ms` include offline capture, pooling and counting
serialization, without allocating a second encoded payload.
`checkpoint_save_barrier_wait_ms` measures the existing flush call after any
background progress; it is not total checkpoint I/O or total save cost. The
exploration case separately reports `tail_save_barrier_wait_ms`. `reopen_ms`
covers production open/validation and, for exploration, the three-record replay
tail, excluding the subsequent equality assertions. The 256-region reopen took
3.06 seconds including that three-record replay. These wall-clock diagnostics
have no pass/fail timing threshold and do not measure client latency. One run
per case establishes neither a general responsiveness guarantee, performance
parity nor an arbitrary-world capacity bound; matched release measurements and
broader verification remain separate requirements.

### Population checkpoint size regression

The deterministic
[`unchanged_population_with_128_ordinary_boundaries_fits_checkpoint_codec` regression](../../crates/server/src/floor_activation_checkpoint_tests.rs)
uses an admitted generated floor with sixteen 32×32×2 regions, 30×30 rooms and
one marker item. It retains 128 ordinary wait boundaries without adding or
changing population decisions. This is a bounded synthetic fixture, not the
production Rogue combat/population workload.

Before complete population-state pooling, its retained checkpoint JSON was
87,318,673 bytes, exceeding the unchanged 67,108,864-byte codec limit. With
pooling, initial JSON was 2,000,504 bytes and the 128-boundary JSON was 2,140,969
bytes. The production checkpoint codec encoded and decoded the latter, and
restore reproduced the exact engine state. These are logical JSON sizes, not
compressed checkpoint sizes, database sizes, memory peaks or latency measurements.
The [pool contract](../architecture/persistence.md#recovery-and-limits) retains
complete-value equality and per-boundary validation. This regression does not
resolve the pre-checkpoint restart timeout in [open work](#open-work), establish
an arbitrary-world bound or support a performance headline.

### Actor and intention pooling

Actor and intention pooling made a 768-actor/256-boundary stress fixture fit the
checkpoint limit, with exact production reopen/retry/cancellation coverage. Restore
memory remained substantial: the complete pooled setup/install/reopen diagnostic
sampled about 759 MB peak private memory. A prior isolated restore exceeded its
1 GiB diagnostic guard and was stopped; it was never a successful restore or a
final peak. This stationary admission/marker fixture is not representative gameplay
or an arbitrary-world bound. The [immutable pooling evidence](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/checkpoints.md#L168-L186)
retains precise encoded/stored sizes, boundary counts and the incomplete-run caveat;
the original note does not identify a machine fingerprint.

### Provisional disk-tail cohort

The October 9 lighting-era comparison used Windows fingerprint `6a1878811f37`
(Core i7-9750H, Windows 11, NTFS HDD), balanced ABBA order and 72 successful runs.
Action workloads met the provisional targets, including improved dense-falling
physics, but a six-round recheck retained longer save/flush tails despite smaller
databases. Optional profiling localized long intervals to SQLite synchronization;
the underlying OS cause remains unresolved. Production durability remains DELETE
journal mode and EXTRA synchronization.

Both sides used F: for saves/journals/builds. Reported system stalls and C: failures
could distort timings even with workload files on F:. Repeat persistence acceptance
under stable conditions; these provisional cohorts do not close prior findings.
The [immutable comparison](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/performance-harness.md#L781-L815)
retains baseline identity, n/p50/p95/max, save sizes and limitations. Raw cohorts
were local pending separate publication authorization when that record was written;
this guide does not claim later publication or fresh verification.

## Historical lighting diagnostics

[October range/lighting evidence](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/performance-harness.md#L753-L779)
uses fingerprint `6a1878811f37`, ranges 8/12/16, eight-observer fresh geometry
batches and generated floors at depths 1/6/11. Adaptive sample sizes differ, and
isolated geometry excludes body-local awareness, transport and whole-action work.
A completely dark floor therefore has no nonlocal geometry disclosure while actual
gameplay still discloses occupied/adjacent cells. Keep those boundaries when using
the linked n/p50/p95/max tables; they are not a matched prior-version comparison.

## Other retained measurement sources

These immutable records retain detailed workload results and limitations removed
from current rule/authoring references. They are historical evidence, not alternate
current contracts or proof that the same results hold today:

- [Simulation diagnostic](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/simulation-slice.md): seeded scaling/work counts and original latency boundaries.
- [Item workload](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/items.md): inventory/stack evidence; disclosed counts are not heap allocation.
- [Package workload](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/scenario-packages.md#performance): disconnected-region input scaling, not exploration.
- [Generation diagnostic](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/generation-recipes.md#verification-and-diagnostics): streamed floor actions, residency, traffic and durable barriers.
- [Interaction adaptation](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/interaction-adaptation.md): source/build-specific comparison and raw asset links.

Do not reconstruct missing sample/host identities from prose or replace failed
measurements with shorter successes. The ledger owns accepted headlines and hashes.

## Finite-claim workload v1

[`finite_claim_bench`](../../crates/server/examples/finite_claim_bench.rs) measures
ordinary admitted finite-item actions through reservation, settlement and saved
reopen. It is a head-only diagnostic, not a before/after performance claim or an
accepted performance ledger result. Run its five explicit example tests and the
independent report-validator tests before measuring; a workspace gate that only
compiles examples does not execute their test cases:

```sh
cargo test -p tor-server --example finite_claim_bench --locked
python scripts/test_runner.py test_finite_claim_report -v
cargo run -p tor-server --release --example finite_claim_bench --locked > OUTPUT.jsonl
python scripts/finite_claim_report.py OUTPUT.jsonl
```

Choose an untracked output path in place of `OUTPUT.jsonl`. Use the worktree's
private target directory and an isolated runtime window.
`--smoke` selects twelve rows; the full matrix has 132 rows across three fixed
seeds, checkpoint versus replay-tail recovery and zero versus 64 background
actors. Carried and exact disclosed-ground cases span one/eight costs and up to
32 fragments. Contention includes a genuine two-actor last unit, 27 neighboring
owners of one stack, and 64 total owners across eight separate stacks. The last
case does not exercise the 64-record per-item safety ceiling.

Admission, start and settlement timings exclude setup and offline diagnostics.
Report scene construction, membership-index cells and copied scene cells as
different counters; none alone is an end-to-end latency measure. Plan/checkpoint
JSON sizes are logical bytes, separate from physical SQLite file lengths.
Reopen compares disclosed states/history and exact physical stock, while each
`claims_equal` flag compares only a scalar profile of holds, references, owners
and held units. The stronger exact checkpoint and late Store-failure tests remain
the authority-conservation evidence. Three samples per case make p95 the maximum;
retain failed rows and physical-file variation rather than treating either as a
balanced-game or full-private-state proof.
