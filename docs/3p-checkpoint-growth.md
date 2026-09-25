# Milestone 3p explored-world checkpoint reduction

This focused change removes the explored-world checkpoint-size blocker without
raising the 64 MiB cap or disabling checkpoints. It adds no interactions or travel
features. The remaining closure disposition is recorded below.

## Publication and compatibility

PR #28 merged at `eff107ce95b7c0f6c8f813597d0fcb94f70fefa5` after Windows and Linux
CI succeeded on final head `61372ee1958346a90fe423eb75b6e16f182951cc`; GitHub was
queried before implementation. The user explicitly chose save format **6**, with
format-5 rejection under the existing pre-release policy, before implementation.
Protocol 12 and ruleset `diagonal-v11` remain unchanged. No compatibility reader,
migration, checkpoint bypass, replay-validation relaxation, or cap change is added.
Old saves remain on disk but require their old executable; new launchers still
create fresh saves.

## Representation and regression

Format 5 stored each distinct whole navigation map in full. Retained exploring
boundaries share most knowledge but differ locally, so whole-map equality does
not remove overlap. Format 6 pools equal cell and edge maps by source region,
then stores ordered region references for each navigation instance. Deduplication
runs on the existing storage worker; capture, queue admission, publication and
transaction boundaries are unchanged. Decoding shares immutable region maps,
with copy-on-write updates preserving each boundary's exact knowledge, including
changed cells and removed edges. Clients receive the same disclosed protocol data.

The full saved-discovery regression first failed on the old representation at
765,021,723 bytes. It now requires both eight and 256 completely traversed regions
to fit below 16 MiB, writes a real checkpoint, compares counting/writer bytes and
restarts. Additional tests compare complete game/navigation and revisions at every
retained rewind boundary, validate shared ownership and deletions, and reject bad
indices, duplicate/out-of-order references, malformed region tables and fields.
Existing rewind, branching, retry, corruption, queue and crash-recovery suites
remain required. This is a bound on the existing fixture, not arbitrary-world
streaming or a bound on every intermediate allocation.

## Targeted release comparisons

Runs use the same Windows machine and F: NTFS HDD as the prior closeout. The old
release executable is retained separately before rebuilding. Fixture version 1,
profiling version 2, action ordering and validators are maintained. Measurements
run sequentially without competing builds/tests; ordinary desktop activity and
OS scheduling are uncontrolled. Save policy is 10/50/1 ms target/maximum/idle.
Raw samples, binary hashes, commands, validation and limitations accompany the
[measurement manifest](measurements/3p-checkpoint-growth-2026-09-25/manifest.json)
and [client summary](measurements/3p-checkpoint-growth-2026-09-25/summary.json).
Final binaries use runtime source `f19ed4b11e87659e2d990c2c48cc94b24bc833fc`;
subsequent changes expose a recorded offline timing field and package evidence.

Final-state checkpoint JSON falls from 7,457,174 to **1,051,196 bytes** at eight
regions and from 765,021,723 to **9,920,494 bytes** at 256 regions (98.7% reduction).
The count includes all retained rewind boundaries. It is an offline diagnostic,
not necessarily the last asynchronously selected checkpoint.

| 256-region saved traversal | p50 / p95 / max action ms | Selected checkpoint / tail records | Flush / restart ms |
| --- | --- | --- | --- |
| Before, checkpoints disabled | 1.053 / 1.782 / 5.505 | none / 2,805 | 874 / 3,222 |
| After, checkpoints disabled (comparison only) | 1.080 / 1.834 / 3.487 | none / 2,805 | 404 / 3,256 |
| After, interval 64 | 1.066 / 1.807 / 3.620 | 2,752 / 53 | 871 / 1,618 |
| After, interval 1024 | 1.076 / 1.809 / 3.569 | 2,048 / 757 | 706 / 2,172 |

All completed rows contain 2,805 actions and 20,956 disclosed cells. Both enabled
runs restore exactly. The final selected checkpoint is 9,806,500 bytes at interval
64 and 7,840,164 bytes at interval 1024, with last worker encodings of 56/46 ms.
The respective database files are 21,807,104/14,471,168 bytes; physical database
size includes reusable pages and retained history. Eight-region interval-64 restart
falls from 431 to 86 ms, with checkpoint size 5,268,464 to 890,141 bytes. Neither
hardware power-loss durability nor constant-time history loading is claimed.

The fresh old-binary interval-64 run again fails during large exploration and is
retained as a rejected partial run. It must not pass the completion validator.
The representative eight-actor/10,000-history mixed trace has 1,499 accepted actions
per run (1,505 ordered samples including blocked attempts). Final p50/p95/max is
0.016/1.949/3.279 ms, versus 0.017/1.881/2.881 before. An exploratory 9.404 ms
after-sample occurred in navigation (5.555 ms) and perception (3.723 ms), with zero
checkpoint capture and command-path I/O; its focused repeat peaked at 2.867 ms.
These samples are retained, not removed from the evidence. The focused cases
exercise the changed scale and existing interactions; no broad matrix was needed
to explain a new checkpoint regression.

## Actual clients and timing correlation

The final native ASCII driver traverses all 256 regions using ordinary actions,
with interval-64 checkpoints enabled from the start. It verifies 2,805 actions,
20,956 disclosed cells, a 9,806,500-byte checkpoint at sequence 2,752, and 53 tail
records. Restart takes 1,775 ms through the first native frame and restores exact
state, branch and history. Continued ASCII input takes 50.7 ms; text control,
ordinary action and explicit save also succeed. A separate real Windows keyboard
check on this same retained explored save opens a local modal in **78 ms** while
SQLite persistence is deliberately blocked, then verifies the checkpoint commits.
This is actual-client acceptance with discovered knowledge, not an empty large map.

The final three-cycle eight-actor run has 1,675 accepted acknowledgements:
p50/p95/max is **6.82/21.36/30.64 ms**, reader-line receipt is
6.08/18.95/28.15 ms, and 205 actor-one presentations are 33.81/41.02/46.94 ms.
Every accepted request has matching server/client/reader records. Server handlers
peak at 8.28 ms. Earlier runs retained 1,182 ms driver and 312 ms reader tails;
one instrumented run completed all actions but timed out on its final save
response. That run remains a failure, even though inspection afterward found its
actions persisted. Buffered diagnostic stderr improves the final acknowledgement
run, but non-reproduction does not establish that all historical tails are fixed.

The full native traversal has p50/p95/max **49.50/51.06/2,914.76 ms**. All 2,805
actions are correlated. The largest sample includes 2,866 ms inside the client
request-send interval, with an 8.50 ms server handler. Another 832 ms sample
includes 781 ms in request sending; a 928 ms sample includes 873 ms between server
acknowledgement send and client receipt. A 2,193 ms sample spends 2,176 ms between
client acknowledgement and frame-reader receipt, despite short measured apply,
draw, native and report calls. These intervals include scheduling and opt-in
diagnostic output; they do not identify a single root cause. Server handlers peak
at 10.53 ms and session-lock waits at 0.037 ms in this traversal.

Earlier native runs exposed long diagnostic report calls; the correlator now
uses the following frame's `previous_report_ms` to recover the measured frame's
own report cost. Final correlation uses cached precise host-clock calibration
and the same monotonic boundaries as elapsed times. Earlier clock/logging
limitations are labelled in the manifest. Reported-frame receipt is not pure
drawing time or hardware keyboard-to-photon latency. The remaining tails require
a focused follow-up separating diagnostic pipe backpressure, scheduling and
client delivery; do not subtract them or claim they satisfy the latency target.

## Verification

Local verification passes: 223 Rust tests in each of debug/release, 92 Python
checks in debug, 65 release process checks, formatting, Clippy with warnings
denied, dependency boundaries, documentation links and private rustdoc with
warnings denied. The full 256-region native acceptance also passes, and the four
responsiveness tests pass again after the offline correlator exposes send timing.
Debug/release executables are built. Text, ASCII and Text + ASCII Spectator
desktop launchers each connect successfully using fresh saves and owned cleanup.
The requested 256 Region Spectator shortcut/helper is removed; the reusable
benchmark driver and prior saves remain. Windows/Linux CI must pass on the final
PR commit before merge; the PR records publication status.

## Closure and next step

The checkpoint-growth blocker is **closed** for the supported saved-discovery
workload: complete checkpoint-enabled exploration, exact durable recovery, bounded
tail replay and explored-save native responsiveness pass without changing the cap.
Milestone **3p remains open** for the recurrent client/diagnostic timing tails
above. Historical spikes are not proved fixed by a size reduction. The next
focused step is to isolate those measured send/delivery/report-reader intervals
and establish their latency disposition before expanding milestone 3 features.
Then resume the documented shared resumable-action
extension points, durable place knowledge, semantic narration/interruption and
slow-client resynchronization acceptance for interactions and travel.
