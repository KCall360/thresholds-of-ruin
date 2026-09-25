# Session handoff — 2026-09-25

## Publication state

Phase D merged in [PR #26](https://github.com/KCall360/thresholds-of-ruin/pull/26)
at `61f9258d7df49355ab7c662bbe72ddefee99dbd0`. Both Windows and Linux CI passed.
Phase E is implemented on `codex/client-responsiveness`, based on that merge.
The user authorized a PR and merge only after Windows/Linux CI pass on the final
commit. Consult the live PR for authoritative publication state; do not republish
Phase D. The implementation and local checks below are complete; the live PR
records final Windows/Linux CI and publication.

Read [Phase E findings](phase-e-findings.md), [the harness](performance-harness.md),
[ASCII behavior and timing](ascii-client.md#responsiveness-and-diagnostic-timing),
[the performance plan](performance-persistence.md) and
[development practices](../CONTRIBUTING.md) before continuing.

## Phase E implementation

- Protocol 12, save format 5 and `diagonal-v11` are unchanged; local saves remain.
- Shared updates validate stream, payload and history consistency before mutation.
  They no longer clone historical memory; same-branch snapshots move retained
  memory after validation. Invalid boundaries remain atomic; branch changes clear
  abandoned memory. Stationary charts reuse their index.
- ASCII sends ordered disclosed updates/snapshots through a bounded 64-event
  channel, with backpressure on its dedicated worker. Every intermediate received
  view still updates memory. The native loop targets 60 Hz, consumes at most 16
  network events per turn and stops starting more after four milliseconds.
- Rendering indexes cells/occupants with an existing-vector glyph oracle. Input
  selection invalidation, visible-only targets and stale-memory colors remain.
- Frame profiling separates application, drawing, native pacing/presentation,
  capture and previous reporting time. The original actual-client metric remains;
  an additional acknowledgement-line timestamp isolates driver queue/log delay.
- Client workload version 1 covers 64/20,956 historical cells and bursts of 1/64
  observations, with exact sample/memory/chart/timing validation. Existing server
  fixtures, profiling versions and retained report validation are unchanged.

## Measurements and limitations

Retained samples, summaries and hashes are in
`docs/measurements/phase-e-2026-09-25/`. Clean matched release client measurements
show large-history 64-update burst p95 improving from 549.084 to 39.164 ms and
rendering from 6.554 to 3.031 ms. The real growing-discovery trace validates 2,882
actions and 20,956 remembered cells, with authoritative 256-region p95 1.801 ms.

A final actual-client presentation tail of 148.577 ms follows a measured 116.258 ms
PPM capture stall. No-capture presentation p95/max are 37.492/43.843 ms. Separate
182.930/304.187 ms secondary-actor acknowledgement tails remain under investigation.
A consecutive same-server/headless-binary follow-up has reference/final no-capture
presentation p95 69.638/37.825 ms and maxima 74.141/38.909 ms, without repeated
acknowledgement spikes. The historical 673/738 ms tails were not reproduced or proved fixed. All diagnostic
I/O remains synchronous; ordinary play omits it. No hard real-time guarantee or
physical keyboard-to-photon measurement is claimed.

Native Win32/X11 tests verify local input during a deliberately blocked SQLite
writer, both with and without a pending checkpoint in a 256-region world. A
160-action real-client burst verifies native input, exact final state, retained
history and the durable checkpoint.
Unit tests cover allocation retention, atomic rejection, intermediate memory,
branch resets, bounded backpressure and glyph equivalence.

## Completion and next work

Formatting, Clippy, architecture/documentation checks, warning-free private-item
rustdoc and all 219 Rust tests in each of debug/release pass. All 84 Python checks
have been verified, including native mouse input with desktop access, and the full
64-test release process suite passes. The desktop debug discovery exposed a
cross-client wizard-test race: actor 2 acted before consuming actor 1's readiness
handoff. The test now requests its snapshot before acting; both affected debug
scenarios and the full release suite pass. Expanded responsiveness tests also pass
in debug/release. No product rule or stale-revision check was relaxed.

All four desktop launchers passed actual connection, fresh-save retention and
owned-process cleanup checks, including three completed 256-region demo cycles.
Their existing helper paths still target the rebuilt debug binaries.
Keep logs, credentials and fresh local saves outside Git.
The full 64-case matrix is not required for this client-focused change.

The broader milestone 3p remains incomplete. Historical memory grows with disclosed
cells; moving charts still process their bounded cache. Diagnostic I/O, OS/driver
scheduling and remaining acknowledgement tails need separate interpretation.
Server non-wait observations still scale with actors; item/visited-place/history
queries, travel searches and startup loading scale with inputs. Checkpoint encoding
and the 64 MiB cap are unchanged. Detached discovery does not prove arbitrary-world
checkpoint bounds. Maintain instrumentation, versioned workloads and validators,
with focused release comparisons for future changes.
