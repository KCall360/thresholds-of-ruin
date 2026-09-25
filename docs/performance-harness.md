# Performance harness and Phase A verification

Status: Phase A complete, 2026-09-24. The [measured findings](phase-a-findings.md)
retain the complete release baseline and verification. Phase B background
storage is implemented; see the [Phase B findings](phase-b-findings.md).
Phase C adds [checkpoints](checkpoints.md) and focused `--phase-c` comparisons.
Phase D adds `--phase-d` state-copy and full growing-discovery checks; see the
[Phase D findings](phase-d-findings.md).
Maintain this harness as features evolve and use targeted release-build checks
under the [development practices](../CONTRIBUTING.md); the full matrix is not
required for every change. The governing scope remains
[performance and scalable persistence](performance-persistence.md).

## Shared deterministic fixture and trace

The versioned specification is
[`performance-v1.json`](../crates/server/fixtures/performance-v1.json). The actual
server constructor, Rust benchmark/test orchestration, and Python headless driver
consume this file. Simulation remains free of clocks, filesystem and client code.
No wizard setup or hidden-state client access is needed for this fixture.

`tor-server --regions 1..=256 --actors 1..=8` selects the diagnostic fixture.
Without `--regions`, ordinary new games retain the two-room scenario. Existing
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
Protocol 12, save format 5, and `diagonal-v11` are currently in use. Multi-actor archives
whose receipts were produced before this readiness correction may fail strict
replay because their expected revisions differ. Failed replay preserves the
original file; no migration or relaxed replay validation is provided.

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

Measured command phases are exclusive: candidate capture, checkpoint capture, simulation,
navigation refresh, revision-view perception, revision comparison, rewind
snapshot, serialization, underlying write/flush, file sync, replacement, and
publication. Simulation's internal door perception belongs to simulation;
navigation's scene work belongs to navigation. These nested calls are counted
at their actual simulation call sites, without adding their time twice.
`unattributed` accounts for the rest of total authoritative command latency.

Phase A's retained results measured buffered whole-archive JSON writes, sync,
and replacement. Current Phase B command timing measures record encoding/queue
admission in `serialization`; command-path write/flush, sync, and replacement
counts are zero. Worker `save_status` reports accepted/durable sequences, pending
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
Raw samples must be retained with the source/build that produced them.

## Observable 256-region run and desktop maintenance

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
player actions. Each headless ready response and error is checked. It awaits the
matching spectator state after accepted actor-1 actions. Paced runs publish
actor-visible progress annotations outside action timing; their additional saves
make the demonstration distinct from throughput measurements. Spectator and
player credentials remain separate, ephemeral, and out of version control.

`request_to_ack_ms` and `request_to_presentation_ms` are observed at the driver:
they include transport, native client processing and diagnostic JSON reporting
(and framebuffer capture on the presentation path). They are not wire-only or
GPU timestamps. `request_to_ready_ms` records the complete headless response.
Pacing, snapshot requests and progress annotations are outside these intervals.

The four machine-local shortcuts are Text, ASCII, Text + ASCII Spectator, and
256 Region Spectator. Their scripts and icons use the explicitly rebuilt
`target/debug` binaries. The paired game's text helper uses that same directory.
The fourth invokes this checked-in driver. Verify real connections and presented
frames, helper paths, separate spectator credentials, fresh saves and owned
cleanup whenever updating binaries; `cargo check` is insufficient.

## Stable tests and verification

Rust tests cover exact world sizes, ordinary and rotated joins, LOS, stairs,
door passage, blocked attempts, multi-actor handoff/disclosure, mixed replay,
seed equivalence, save-lock ownership, phase exclusivity, actual bytes and
bounded rewind. Python tests exercise the real headless/ASCII trace and eight
scheduled clients, and check the independent scheduler/report oracle. Ordinary
CI uses an eight-region process case; set `TOR_PERFORMANCE_REGIONS=256` and
`TOR_PERFORMANCE_CYCLES=3` for the complete large demonstration.

Run the workspace checks in [development practices](../CONTRIBUTING.md), in debug
and release. Windows native mouse tests require an interactive desktop; sandbox
API denial is an environment failure and must be reported or rerun with desktop
access, never silently skipped. Windows/Linux CI is required before merging.

The [background-saving guide](background-saving.md) describes current storage
and recovery tests. The [persistence review](persistence-review.md) retains
historical Phase A evidence. Process tests do not prove hardware power-loss
behavior. Checkpoints and logical compaction are covered by the current recovery tests.

## Focused Phase B run

```powershell
cargo run --release --locked -p tor-server --example latency_bench -- --phase-b --cycles 5 --save-target-ms 10 --save-max-ms 50 --save-idle-ms 1 > phase-b.jsonl
```

This selects eight-region cases at 100/10,000 actions and one/eight actors,
matched memory cases, and one 256-region/eight-actor/10,000-action saved case.
The short policy deliberately exercises saves during the mixed workload.
Production defaults remain configurable and are listed in
[background saving](background-saving.md). Use the current raw schema when
interpreting asynchronous worker metrics; the Phase A report's physical I/O
counts are not interchangeable with application journal bytes.

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
version 2 adds contracts for zero observation/scene work on waits and one scene per
actual observation on other actions. Retained version-1 profiling remains valid.
Do not interpret a mixed median dominated by scheduled waits as movement latency;
retain per-label distributions and individual tails.


## Focused Phase E client study

```sh
cargo run --release --locked -p tor-client-ascii --example client_bench > client.jsonl
python scripts/client_performance_report.py client.jsonl
python scripts/performance_driver.py --bin-dir target/release --output target/client-capture --regions 256 --actors 8 --cycles 1 --pace-ms 0
python scripts/performance_driver.py --bin-dir target/release --output target/client-no-capture --regions 256 --actors 8 --cycles 1 --pace-ms 0 --no-capture
```

The separate client workload version 1 has 64/20,956 previously disclosed cells,
a bounded 64/4096-cell chart, 64 current cells, bursts of 1/64 observations and
20 samples per combination (80 total). It applies all updates before one canvas
draw. Setup and memory-count inspection are outside timing; update payload cloning
and validation are included. This stationary synthetic chart isolates historical
memory and tile lookup costs; it is not an authoritative world/discovery workload.
Continue `latency_bench --discovery-only` for real movement and growing knowledge.
The original server fixture/version/order and retained validators are unchanged.

Build a reference checkout with this same example, retain its executable, then
build the changed implementation. Run matched binaries without competing builds
or tests. Keep raw samples, sample counts, percentiles, source/binary hashes and
hardware metadata. The validator enforces exact ordering, version, memory/chart
counts and finite nonnegative phase timings, not machine-specific latency gates.
The allocation-retention and glyph-oracle Rust tests provide stable regressions.

Actual-client samples now retain native frame phase diagnostics when available.
`--no-capture` isolates PPM writing while preserving native presentation and JSON
reporting. Default capture behavior is unchanged for existing workloads and the
desktop demonstration. The driver timestamp includes transport, scheduling and
diagnostic work; it is not interchangeable with pure update/draw timing. See
[Phase E findings](phase-e-findings.md) and the [ASCII diagnostic boundaries](ascii-client.md#responsiveness-and-diagnostic-timing).


`request_to_ack_ms` retains its historical driver-consumption timestamp. New runs
also include `request_to_ack_line_ms`, stamped when the reader receives the JSON
acknowledgement line, before diagnostic log writing/flushing and queue delivery.
Their difference isolates delay inside the driver; neither is a server-only time.
Existing retained reports without the new field remain valid.
