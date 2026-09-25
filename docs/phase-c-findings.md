# Phase C findings — 2026-09-24

Phase C checkpoints preserve retained history and bound the number of commands
simulated on restart. The final implementation shares repeated worlds, geometry,
navigation and item maps across rewind snapshots. Snapshot capture stays on the
action path; encoding and SQLite work run in the background worker.

## Consecutive comparison on the largest case

The same final release binary ran the 256-region, eight-actor workload starting
with 10,000 retained actions, first without checkpoints and then with the default
1,024-entry interval. Both runs accepted 1,499 measured commands in three cycles.
Each run flushed, reopened and verified identical final disclosed state.

| Metric | Checkpoints disabled | Default checkpoint interval |
| --- | ---: | ---: |
| Action p50 (ms) | 14.39 | 13.97 |
| Action p95 (ms) | 23.61 | 16.20 |
| Action maximum (ms) | 28.39 | 25.96 |
| Restart (seconds) | 119.00 | 7.02 |
| Records simulated on restart | 11,499 | 474 |

This pair shows no default-interval action-latency regression, but does not establish
a speedup for ordinary actions. Timing varied substantially across runs; the retained
five-case passes below show why a single percentile must not be treated as a precise
machine-independent bound. The provisional 8 ms p95 goal is still unmet at long
histories. State-copy work remains Phase D, and native presentation remains Phase E.

The final selected checkpoint was 987,630 encoded bytes. The largest measured checkpoint capture in this pair was 0.583 ms.
No ordinary command performed filesystem writes or synchronization. These byte
counts describe application payloads, not physical SQLite I/O.

## Focused five-case characterization

Three passes each validated five cases and 4,897 ordered attempts: a checkpoint-
disabled reference, a frequent 64-entry interval, and the default 1,024-entry
interval. All used target/max/idle save timing of **10/50/1 ms** to force background
work during play. “Default interval” does not mean the production 30-second save
target was used. Fixture version 1, seed 42, three cycles, and no warmup were held
constant. The complete 64-case matrix was not repeated.

| Regions / actors / initial history | Reference restart (s) | Default-interval restart (s) | Frequent restart (s) | Default tail records |
| --- | ---: | ---: | ---: | ---: |
| 8 / 1 / 100 | 0.15 | 0.15 | 0.17 | 283 |
| 8 / 1 / 10,000 | 46.08 | 2.17 | 1.08 | 182 |
| 8 / 8 / 100 | 3.85 | 2.25 | 0.22 | 579 |
| 8 / 8 / 10,000 | 79.03 | 6.91 | 0.73 | 474 |
| 256 / 8 / 10,000 | 94.44 | 7.15 | 1.19 | 474 |

The short single-actor case never reached the default checkpoint threshold; its
283 records correctly replayed from the initial base. Enabled checkpoint tails
were below their configured intervals. Retained history is still read and indexed
on startup; this is bounded simulation replay, not constant-time total startup.

| Regions / actors / initial history | Reference p95 (ms) | Default-interval p95 (ms) | Frequent p95 (ms) |
| --- | ---: | ---: | ---: |
| 8 / 1 / 100 | 1.23 | 1.20 | 1.12 |
| 8 / 1 / 10,000 | 10.17 | 12.65 | 11.26 |
| 8 / 8 / 100 | 3.56 | 4.12 | 4.40 |
| 8 / 8 / 10,000 | 12.72 | 19.01 | 18.49 |
| 256 / 8 / 10,000 | 14.10 | 24.30 | 23.01 |

The earlier baseline used an earlier implementation binary with checkpointing
disabled. The apparent regressions prompted the consecutive same-binary pair
above: disabling checkpoints also reproduced the high tail latency. Therefore
do not attribute all cross-run differences to checkpoint frequency or claim all
latency targets have been met. Raw p50, p95, maximum, phase costs and samples are
retained, including unfavorable values. No competing builds or test suites ran
during the final paired measurements; normal desktop activity was not controlled.

## Correctness and reproducibility

Local verification includes workspace Rust tests in debug/release, final focused
checkpoint/storage/snapshot tests, Clippy, formatting, warning-free rustdoc,
architecture checks and documentation checks. Three checkpoint process tests and
six existing background-save process tests pass in both debug and release, using
real headless, text and native ASCII clients. The tests cover crash rollback,
explicit-save barriers, concurrent actions while storage is blocked, retry,
private notes, retained forks and the complete rewind window. Fault schedules
include process death at each transaction stage and a torn uncommitted database
extension. See [checkpoints](checkpoints.md) for contracts and limits.

All four desktop launchers also passed connection, fresh-save retention and
owned-process cleanup checks, including three completed 256-region demo cycles.

Windows/Linux CI remains required before publication is merged. Process tests
do not establish hardware power-loss guarantees.

The [manifest](measurements/phase-c-2026-09-24/manifest.json) records commands,
platform, save volume, base commit, dirty state, binary hashes and artifact hashes.
The [summary](measurements/phase-c-2026-09-24/matrix-summary.json) preserves all
mixed-command phase distributions and recovery/save counters. Raw samples:

- [Five-case reference](measurements/phase-c-2026-09-24/baseline.jsonl.gz).
- [Frequent checkpoints](measurements/phase-c-2026-09-24/frequent.jsonl.gz).
- [Default interval](measurements/phase-c-2026-09-24/default-interval.jsonl.gz).
- [Paired disabled](measurements/phase-c-2026-09-24/paired-disabled.jsonl.gz).
- [Paired default interval](measurements/phase-c-2026-09-24/paired-default.jsonl.gz).

Validate five-case files with `python scripts/performance_report.py FILE --phase-c`.
For either paired file, also pass `--case r256-a8-h10000-durable`. The benchmark
accepts the same `--case` selector for focused follow-up comparisons. Keep these
workloads and validators maintained under [development practices](../CONTRIBUTING.md).
