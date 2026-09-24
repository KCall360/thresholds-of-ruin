# Phase B findings — 2026-09-24

This report records the pre-checkpoint Phase B implementation. See the
[Phase C findings](phase-c-findings.md) for subsequent checkpoint and restart work.

Background persistence removes disk waits and whole-history encoding from ordinary
acknowledgements. The focused release comparison completed all nine cases and
13,798 ordered attempts using the unchanged version-1 mixed workload. Every saved
case flushed, reopened, and reproduced its final disclosed state. The full
64-case Phase A matrix was not repeated.

The product contract changed: Phase A waited for storage before acknowledging;
Phase B acknowledges bounded in-memory admission and permits crash rollback of
unsaved play. These latency improvements include that deliberate tradeoff.
Explicit save, graceful shutdown, and wizard enablement still wait for storage.
See [background saving](background-saving.md).

## Server-only comparison

Times are milliseconds for accepted mixed commands; blocked attempts are retained
separately in the raw samples. Each case runs five cycles, with no warmup. The
eight-actor scheduler produces more commands than the one-actor trace.

| Regions | Actors | Starting history | Phase A p95 | Phase B p50 | Phase B p95 | Phase B max |
| --- | --- | --- | --- | --- | --- | --- |
| 8 | 1 | 100 | 58.56 | 0.58 | 1.11 | 1.38 |
| 8 | 1 | 10,000 | 301.76 | 9.03 | 10.16 | 10.92 |
| 8 | 8 | 100 | 133.09 | 2.95 | 4.28 | 7.14 |
| 8 | 8 | 10,000 | 691.41 | 11.44 | 13.74 | 25.31 |
| 256 | 8 | 10,000 | 855.30 | 13.08 | 15.58 | 26.76 |

Matched eight-region memory-only p95 values were 1.11/9.36 ms for one actor at
100/10,000 history, and 4.23/18.76 ms for eight actors. Saved and memory timings
are close; the apparent faster saved result in the long eight-actor case is
measurement variation, not evidence that persistence accelerates simulation.
Full-state candidate copies and disposal still grow with retained history.
The provisional 8 ms p95 goal is not met at 10,000 actions; Phase D remains needed.

## Saving work and recovery

The stress policy was target 10 ms, maximum age 50 ms, idle 1 ms, queue 8 MiB,
so saves ran during the workload rather than only at its end. This is intentionally
more aggressive than production defaults. Action-path write/flush/sync/replacement
counts were zero; each accepted saved action encoded exactly one record.

| Case | Encoded bytes/action | Encode/admit mean (ms) | Batches | Final save wait (ms) | Restart/replay (s) |
| --- | --- | --- | --- | --- | --- |
| 8 regions, 1 actor, 100 history | 675.8 | 0.0090 | 30 | 13.14 | 0.23 |
| 8 regions, 1 actor, 10,000 history | 679.8 | 0.0124 | 142 | 33.75 | 43.80 |
| 8 regions, 8 actors, 100 history | 597.5 | 0.0077 | 462 | 38.68 | 6.96 |
| 8 regions, 8 actors, 10,000 history | 599.7 | 0.0129 | 1,205 | 7.99 | 82.90 |
| 256 regions, 8 actors, 10,000 history | 599.7 | 0.0110 | 1,518 | 8.12 | 100.16 |

These are newly committed application frame bytes, excluding bootstrap history.
They are **not physical SQLite write counts** and must not be compared directly
with Phase A's measured filesystem bytes as a physical write-amplification ratio.
The near-constant frame size and single-record encoding establish removal of
whole-history application serialization. Tests also verify prior row payloads
and the replay base stay unchanged across new saves.

The largest final database was 7.23 MiB, versus Phase A's 5.69 MiB JSON archive;
framing and database pages add storage overhead. Its timed actions added 1.43 MiB
of application frames. The largest sampled pending queue across cases was
22,715 bytes, well below the configured 8 MiB bound.

Under this aggressive policy the largest sampled unsaved age was 485 ms. Maximum
age triggers saving; it cannot make a slow flush finish instantly. All final
durable sequences caught up, no queues exceeded their limits, and no batch errors
were recorded. Production warns when a save falls behind. Full replay remains
expensive; no application checkpoint or compaction work was included.

## Actual clients

The release headless player and native ASCII spectator completed three cycles
in a 256-region world with one actor, seed 42, no added pacing, and default save
policy. There were 205 accepted timed actions; remembered cells grew from 44 to
581. The driver requested a durable save outside action timing before cleanup.
This verifies a representative mixed run, not exploration of all 256 regions.

| Boundary | Phase A p95 (ms) | Phase B p50 (ms) | Phase B p95 (ms) | Phase B max (ms) |
| --- | --- | --- | --- | --- |
| Request to acknowledgement | 236.37 | 16.02 | 21.88 | 479.28 |
| Request to headless ready | 237.83 | 19.29 | 25.94 | 482.65 |
| Request to spectator presentation | 295.39 | 65.48 | 94.78 | 997.83 |

These boundaries include process/JSON diagnostic overhead and native frame
capture; they are not server-only or isolated crossing times. Tail spikes remain,
including a presentation maximum above the Phase A run's 814 ms maximum. Their
cause is not established by this sample. Do not claim the actual-client maximum
latency goal is met; client responsiveness remains Phase E work.

## Evidence and limits

The [manifest](measurements/phase-b-2026-09-24/manifest.json) records commands,
base commit, dirty working-tree status, tool versions, binary hash, and artifact
hashes. Retained evidence:

- [Raw server samples](measurements/phase-b-2026-09-24/samples.jsonl.gz).
- [Case and phase summaries](measurements/phase-b-2026-09-24/matrix-summary.json).
- [Actual-client samples](measurements/phase-b-2026-09-24/actual-client.json).
- [Phase A comparison](phase-a-findings.md).

Measurements ran locally on Windows x86-64 without competing builds/test suites.
Normal desktop activity was not controlled. Timing is diagnostic, not a CI gate.
The report independently validates scheduling, actions, record counts, bounded
queue accounting, and final saved sequences:

```powershell
python scripts/performance_report.py docs/measurements/phase-b-2026-09-24/samples.jsonl.gz --phase-b
```

Correctness verification and remaining platform limitations are recorded in the
[handoff](session-handoff.md). SQLite/process recovery tests do not establish
hardware power-loss behavior. Windows/Linux CI remains required before merging.
