# Milestone 3p closeout audit — 2026-09-25

**Not ready to close.** Phases A–E are implemented, but genuinely explored saved
worlds exceed the existing checkpoint limit within the 256-region fixture.
Feature expansion in milestone 3 remains paused. This audit adds reproducible
measurement and records the remaining gate; it does not change persistence,
protocol, simulation, delivery, save compatibility, or latency targets.

## Publication and method

Phase E merged in [PR #27](https://github.com/KCall360/thresholds-of-ruin/pull/27)
at `3783326ab3ba730b3c9994aedbff6cfd147f8799`. Its final head was
`9e059354b93b2b599d16ed00b7b7676c5e4ffc5d`; both Windows and Linux CI completed
successfully before the merge. The closeout started from that clean main tree.

All retained measurements use optimized Windows binaries, the same fixture
version 1 and profiling version 2, without competing builds/tests. The save
volume is F: (NTFS, HDD). Ordinary desktop activity is uncontrolled. The
[manifest](measurements/3p-closeout-2026-09-25/manifest.json) records commands,
source/binary/artifact hashes and limitations; the
[summary](measurements/3p-closeout-2026-09-25/summary.json) retains distributions.
No full matrix was repeated: there is no runtime optimization, and the new
saved-discovery workload directly addresses the missing acceptance evidence.

## Acknowledgement and presentation tails

Two unchanged Phase E native runs each complete three cycles, 1,675 accepted
acknowledgements and 205 actor-1 presentations, with eight actors and 256 regions.
They explore only a few regions, unlike the full traversal below. PPM capture is
disabled; JSON reporting and native presentation remain enabled. Values are
**p50 / p95 / maximum milliseconds**.

| Metric | Run 1 | Run 2 |
| --- | --- | --- |
| Driver acknowledgement | 6.293 / 20.570 / 26.421 | 6.359 / 20.717 / 28.172 |
| Acknowledgement line receipt | 5.648 / 18.354 / 23.907 | 5.705 / 18.551 / 23.660 |
| Driver log/queue delay | 0.627 / 2.081 / 3.701 | 0.629 / 2.061 / 5.929 |
| Native presentation report | 31.788 / 40.292 / 333.706 | 31.214 / 41.117 / 424.215 |

The 182.930/304.187 ms Phase E acknowledgement tails did not recur in 3,350
accepted samples. This narrows the observed envelope; it does **not** identify
their cause or prove them fixed. Both retained historical spikes were actor-2
southward `multi_actor_visibility_change` actions; their files predate the line
receipt field, so that boundary cannot be recovered retrospectively. Each new run
contains 106 actor-2 southward moves, with acknowledgement p95/max of
13.665/22.346 and 13.730/15.604 ms. The native binary hashes exactly match the
retained Phase E final binaries. The line timestamp separates driver logging and
queue consumption from the preceding headless/transport/server interval. It
cannot separate those preceding components or OS scheduling. The old metric and
all Phase E instrumentation remain intact.

Presentation spikes did recur without PPM capture. Frame diagnostics record
300.426/435.812 ms previous-report stalls, with small application/drawing work
and ordinary native-call durations. Reporting includes JSON construction and
stdout flushing; a report can wait for the Python reader, whose disk logging is
synchronous. The evidence locates stalls in diagnostic reporting, but does not
separate serialization, pipe backpressure, disk logging and scheduling. Do not
attribute them to checkpoint encoding or claim physical keyboard-to-photon bounds.
The historical 673/738 ms tails remain unexplained. Ordinary play omits diagnostic
reporting. Retained [run 1](measurements/3p-closeout-2026-09-25/ack-1.json),
[run 2](measurements/3p-closeout-2026-09-25/ack-2.json) and their
[first](measurements/3p-closeout-2026-09-25/ack-1-frames.json) and
[second](measurements/3p-closeout-2026-09-25/ack-2-frames.json) frame timings
preserve the evidence without credentials or local saves.

## Genuinely explored saved worlds

`latency_bench --saved-discovery` attaches the empty fixture to a real save before
ordinary movement. It traverses every region, applies every disclosed observation
to one growing client, flushes, releases the lock and reloads through `Engine::open`.
The existing detached discovery mode is unchanged. The new validator requires
complete action ordering, saved-prefix accounting, checkpoint selection and replay
counts. Failed runs are retained as failures and rejected by the completion validator.

Checkpoint-disabled exploration establishes the saved/journal comparison, not a
solution to checkpoint growth:

| Fully traversed regions | Actions / remembered cells | Action p50 / p95 / max (ms) | Database bytes | Flush / restart (ms) |
| --- | --- | --- | --- | --- |
| 8 | 77 / 620 | 0.664 / 1.156 / 1.358 | 77,824 | 165.917 / 57.284 |
| 256 | 2,805 / 20,956 | 1.053 / 1.762 / 5.348 | 2,318,336 | 817.936 / 3,324.045 |

Both commits reload exactly the final disclosed state. All 77/2,805 records replay
because checkpoints are disabled. These server-only action samples meet the
provisional 8/33 ms targets, but are not native input measurements. Retained
[samples](measurements/3p-closeout-2026-09-25/saved-disabled.jsonl.gz) validate with
`performance_report.py --saved-discovery`.

An offline counting writer measures the production format-5 checkpoint JSON for
the final state, including retained rewind boundaries, without allocating the
encoded payload or writing it. Capture/deduplication/serialization are outside
action and flush timing. It substitutes an equal-length UUID and uses the current
record count as sequence; these ordinary non-wizard workloads have equal record
and sequence counts. Unit/integration checks compare its count to real saved
checkpoint bytes, including a fully explored eight-region world.

The final snapshots require **7,457,174 bytes** at eight regions and
**765,021,723 bytes (729.582 MiB)** at 256, versus the **67,108,864-byte (64 MiB)**
production cap. Counting took 10.798/762.121 ms; these are offline diagnostic
measurements, not worker encoding or save durations. No oversized payload was
persisted and no cap was relaxed.

Matched checkpoint-enabled runs use the same executable and 10/50/1 ms target,
maximum-age and idle policy:

| Interval / case | Accepted actions before completion or rejection | Action p50 / p95 / max (ms) | Result |
| --- | --- | --- | --- |
| 64 / 8 regions | 77, complete | 0.711 / 1.282 / 1.881 | Selected checkpoint 64: 5,268,464 bytes; 13 replayed records |
| 64 / 256 regions | 654, incomplete | 0.955 / 1.680 / 2.796 | Background save failed; durable prefix 212, last checkpoint 192: 37,493,248 bytes |
| 1024 / 256 regions | 1,101, incomplete | 0.972 / 1.619 / 2.641 | Background save failed; durable prefix 1,019, no checkpoint installed |

The eight-region checkpoint worker reports 29 ms encoding; final flush/restart
are 1,353.600/463.380 ms. In the partial stress run, the last successful worker
encoding was 205 ms. Capture maxima were only 0.005 ms (eight regions) and
0.004 ms or less (large runs). Small capture/action times do not make a failed
save acceptable. At rejection, counting the **current**, not necessarily last
captured, snapshot gives 164,580,816/284,728,803 bytes. The generic storage error
does not expose the worker's individual failure cause; the reproducible failures,
encoding limit and independently measured oversized snapshots establish the
checkpoint-size blocker without claiming the failed worker's exact byte count.

An earlier exploratory interval-64 run rejected at action 407 instead. The worker
can replace pending snapshots, so the exact rejection point is scheduling-dependent,
not a deterministic world-size limit. Retain the
[default-interval failure](measurements/3p-closeout-2026-09-25/saved-default.jsonl.gz)
and [stress failure](measurements/3p-closeout-2026-09-25/saved-stress.jsonl.gz);
neither passes the complete-traversal validator.

Source inspection explains the growth mechanism: checkpoints deduplicate whole
equal navigation maps, while successive exploring rewind boundaries contain
different maps with much overlapping knowledge. Runtime region-level sharing
does not become region-level sharing in the current JSON. The size cap bounds
encoded bytes, not all intermediate allocations. A larger cap or disabled
checkpoints would mask the acceptance failure and sacrifice the replay bound.

## Acceptance audit and disposition

| Criterion | Evidence and disposition |
| --- | --- |
| Queue admission before ordinary acknowledgement; explicit-save prefix durability | Implemented, covered by background-save and actual-process suites; retained unchanged |
| Consistent crash rollback and write-boundary recovery | Covered by SQLite fault/kill tests before and after checkpoint installation, history retention, rotation and commit; hardware power loss remains outside these claims |
| Equivalent replay, retry, rewind, branches, annotations, locking and wizard lineage | Existing regression suites retained; explored eight-region checkpoint and large journal reload add coverage |
| Bounded append/checkpoint growth and measured restart | Append path passes; **checkpoint growth is a blocker within the existing scale fixture**. Disabling captures is a diagnostic only |
| Action latency across history and dungeon scale | Phase D/E retained comparisons plus saved exploration support ordinary action costs; checkpoint-enabled large traversal cannot complete, so the overall gate fails |
| Historical acknowledgement outliers | **Open closure-evidence item**, not proved fixed or reclassified as deferred; new exact-binary repetitions did not reproduce them |
| Actual ASCII/text responsiveness during saving | Existing blocked-writer/checkpoint and process suites remain applicable; new native tail runs distinguish reporting stalls. Fully explored large-world checkpoint/native-input acceptance remains blocked until saving succeeds |
| Windows/Linux debug, release, protocol and actual-process checks | Required on the closeout PR's final commit before merge; CI is not a substitute for the failed performance gate |
| Maintained instrumentation, workloads and validators | Existing versions/order retained; opt-in saved traversal, byte-count diagnostic, failure evidence and validator checks added |

**Blockers before 3p closure:** checkpoint representation/growth at the present
exploration envelope; successful checkpoint-enabled full traversal with durable
restart and bounded tail replay; actual-client input during checkpointing of that
explored state. The historical acknowledgement tails also remain an open closure-evidence item;
collect correlated headless/server/reader timings if they recur, and resolve their
disposition explicitly before declaring closure. Non-reproduction is not a fix,
and this audit does not silently defer that investigation.

**Explicitly deferred:** arbitrary-world/region streaming and active-horizon
loading (4e); linear retained-history validation and receipt rebuilding at startup;
actor-count scaling of non-wait observation, item/place/history query indexes and
large travel searches beyond the measured fixture; connection-lifetime historical
client memory retention and bounded moving-chart work; eliminating synchronous
optional diagnostic I/O; hardware power-loss qualification and allocation profiling.
These remain measured feature requirements, not exemptions for a regression in
the current 8/256-region and 100/10,000-history envelope. No target was relaxed.

## Local verification

All 220 Rust tests pass in each of debug and release, including exact counting
versus writer bytes and the explored eight-region checkpoint round trip. All 87
Python debug checks and 64 release process tests pass with native Windows desktop
access. Formatting, Clippy, dependency architecture, documentation links/indexing
and warning-free private-item rustdoc pass. Both sets of client/server binaries
were explicitly rebuilt. All four desktop launchers pass real connections, fresh
save retention and owned-process cleanup, including three complete 256-region
demo cycles. Their existing helpers target the rebuilt debug executables. Final
publication still requires Windows/Linux CI.

## Next step toward milestone 3

Keep milestone 3 feature expansion paused. The next focused 3p change should
address repeated navigation/rewind checkpoint data while preserving exact recovery,
rewind, disclosure and supported saves. First settle an encoding/sharing approach;
if a representation change cannot preserve current saves, raise that compatibility
decision explicitly rather than silently changing format 5. Add a failing full
saved-exploration regression when implementing that fix, and require it to pass
with checkpoints enabled, then repeat targeted small/large measurements and native
input/save/restart acceptance. Broaden to the full matrix only if those results
show unexplained regressions or the chosen change affects several subsystems.

After those gates pass, milestone 3 resumes with the documented resumable-action
extension points, reproducible wizard interaction cases, durable place knowledge,
semantic narration/interruption, and slow-client resynchronization acceptance.
No interactions or travel features are added by this audit.
