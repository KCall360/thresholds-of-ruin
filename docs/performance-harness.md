# Performance harness

This page explains how to run the benchmarks, profiling drivers, and report
validators. Targets, results, and open work are in the
[performance plan](performance-persistence.md). Keep this harness working as
features change, and use targeted release-build checks as described in the
[testing policy](testing.md#performance-testing); the full matrix isn't required
for every change.

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

A case is either a `latency_bench` case name (`--cycles` sets its cycle count,
default five) or `WORKLOAD[:GROUP]` for `combat`, `physics`, `items`, `client`,
or `places`. The `latency_bench` region streaming cases, `stream-r16-memory`,
`stream-r16-durable`, `stream-r256-memory` and `stream-r256-durable` (workload
`streaming-v1`), run only when selected by name; see
[region streaming performance](region-streaming.md#performance). Those workloads always run their complete matrix, because their
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
  fingerprint records the storage type of that volume.
- The script refuses to start measuring while `cargo` or `rustc` processes are
  running. `--allow-competing` overrides this and is recorded.
- `--no-build` reuses binaries that are already built. Don't edit Rust inputs
  while a build is running.
- Run directories and base worktrees live in the gitignored `.local` directory.
  Remove old worktrees with `git worktree remove`.

Each run directory holds `comparison.json` (machine fingerprint, commits,
binary hashes, commands, every run's exit and validation status, and the pooled
results) and a `.tar.gz` bundle of it with the compressed raw samples and logs.
Record the cases, sample counts, percentiles, counts, and limitations with the
change, as the [testing policy](testing.md#targeted-checks-for-each-change)
requires.

## Performance ledger

[`perf/ledger.jsonl`](../perf/ledger.jsonl) is a small committed history of
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
[`performance-v1.json`](../crates/server/fixtures/performance-v1.json). The actual
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
and recovery tests. Process tests do not prove hardware power-loss behavior. Checkpoints and logical compaction are covered by the current recovery tests.

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
version 2 adds contracts for zero observation/scene work on waits and one scene per
actual observation on other actions. Retained version-1 profiling remains valid.
Do not interpret a mixed median dominated by scheduled waits as movement latency;
retain per-label distributions and individual tails.

## Client workloads

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

For semantic narration, run `cargo run --release --locked -p tor-client-ascii
--example client_bench -- --narration` and validate the JSON lines with
`python scripts/client_performance_report.py FILE --narration`. This selects
workload version 2 with actor and door sight changes; omitting the flag keeps
version 1. Both use 64/20,956 remembered cells and 1/64-update bursts. See
[narration and stream recovery](narration-and-recovery.md).

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
[ASCII diagnostic boundaries](ascii-client.md#responsiveness-and-diagnostic-timing).

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
of the current checkpoint JSON. The counting writer does not allocate the encoded
payload, does not write it, and does not bypass the real 64 MiB cap. Its duration
includes capture and deduplication, outside action/flush timings. It uses an
equal-length dummy save UUID and the ordinary workload's record count as sequence.

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
The restarted client starts with its normal connection-local memory, while the
traversal client retains every disclosed observation throughout exploration.

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
durations use monotonic clocks. Clock adjustments can limit cross-process attribution. A receiver can
observe bytes just before the sender returns from sending, so tiny negative deltas
are retained rather than clamped. These are diagnostic boundaries, not wire-only
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
[performance plan](performance-persistence.md#open-work).

## Durable place workload v1

The independent [place-knowledge-v1 specification](../crates/server/fixtures/place-knowledge-v1.json)
adds 0/64 isolated rooms with four perceived anchors per added room, producing
2/258 learned places. It leaves `performance-v1` unchanged. Setup uses authorized
wizard edits; later scenario packages will replace authored setup. Samples alternate
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
for a distinct attachment case: durable names remain, while connection-local
diagnostic cell memory and history retention reset. The driver also records ack
line arrival, ready reader/queue work, cell counts and the size of ready JSON
re-encoded by Python (not network byte counts). Keep both cases; see the
[archived place-knowledge findings](https://github.com/KCall360/thresholds-of-ruin/blob/docs-history-2026-09/docs/place-knowledge-findings.md).

## Scenario package workload v1

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

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
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

Decision selection and profiled engine command intervals are separate (engine
validation also recomputes an AI decision). Phase totals over measured commands identify
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
[dungeon gameplay](dungeon.md). Do not mix results collected during builds or
process-test runs. Existing provisional targets and deferred 3p work still apply.


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
