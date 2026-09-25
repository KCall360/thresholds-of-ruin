# Session handoff — 2026-09-25

## Current state

Phase C merged in [PR #25](https://github.com/KCall360/thresholds-of-ruin/pull/25).
Phase D is implemented on `codex/state-observation-scaling`, based on merged
`main` at `cea088c`. The user authorized publication and merge after Windows and
Linux CI pass on the final PR commit. The live PR is authoritative for publication
status; do not repeat Phase C publication.

Read [Phase D findings](phase-d-findings.md), the
[performance plan](performance-persistence.md), [harness](performance-harness.md),
and [development practices](../CONTRIBUTING.md) before continuing.

## Phase D implementation

- Protocol 12, save format 5 and `diagonal-v11` remain unchanged. No migrations,
  client-side rules or observation cache were introduced. Local saves are retained.
- Transaction candidates own decision state and shared rewind boundaries, with no
  journal, receipt index or storage handle. Admission precedes publication and
  receipt insertion; rejection preserves the full published boundary.
- World maps, items and navigation use copy-on-write snapshots. Navigation shares
  source-region maps so local discovery does not copy all prior knowledge.
- Ordinary waits use explicit time/readiness effects. Other actions conservatively
  compare every actor, building one scene per observation and reusing it for
  surfaces, door approaches and navigation. Navigation examines visible edges;
  region exits use an ordered-map range.
- The version-1 workload ordering is unchanged. Profiling version 2 asserts zero
  geometry work for waits and one scene per observation. `--phase-d` runs nine
  mixed cases plus full 8/256-region discovery; `--discovery-only` isolates growth.
  The maintained validator also accepts retained Phase A/B/C measurements.

## Measurements

Retained samples, summaries, source/binary hashes, hardware and commands are in
`docs/measurements/phase-d-2026-09-25/`. Clean matched reference/final passes each
validate 8,289 mixed attempts and 2,882 discovery actions. No competing builds or
test suites ran during these final comparisons.

The 256-region/eight-actor/10,000-action saved case improves from 14.246 to 2.547 ms
p95, with maximum 23.772 versus 3.385 ms. Full 256-region discovery improves from
11.772 to 1.771 ms p95 and reaches 20,956 remembered cells. Selected cases meet the
provisional server latency targets on this machine. Eight-actor mixed medians are
dominated by waits; preserve per-label distributions and limitations.

An initial optimized discovery run exposed whole-navigation copying. Source-region
sharing removes that cost. Initial runs are retained explicitly; the earliest
baseline overlapped compilation and is not the clean comparison.

The actual-client comparison retains isolated presentation tails (738/673 ms in
current runs) despite approximately 81 ms matched p95. A reference run had a 773 ms
acknowledgement outlier. These include native presentation, diagnostic framebuffer
I/O and driver scheduling; the precise source is unresolved. Keep this evidence
for Phase E rather than treating server-only timing as client responsiveness.

## Verification and next work

Workspace formatting, Clippy, 215 Rust tests in each of debug/release, warning-free
private-item rustdoc, architecture/documentation checks and all 77 Python tests
pass locally. All 61 release process tests also pass. All four desktop launchers
passed real connection, fresh-save retention and owned-process cleanup checks,
including three completed cycles of the 256-region spectator demonstration.
Publication still requires Windows/Linux CI on the final commit. Keep diagnostic logs, credentials and local saves outside Git.

Phase E client responsiveness is next. Non-wait observations still scale with
actors; item inspection, visited places, travel searches, history queries and
startup loading still scale with their inputs. Checkpoint encoding and the 64 MiB
limit remain unchanged; detached discovery does not prove bounded checkpoint size
for arbitrary explored worlds. Future features must maintain instrumentation,
workloads and validators, with targeted release comparisons during development.
The broader milestone 3p is not complete and the full 64-case matrix is not required
on every change.
