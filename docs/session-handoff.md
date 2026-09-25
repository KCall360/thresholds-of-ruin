# Session handoff — 2026-09-24

## Current state

Phase B merged in [PR #23](https://github.com/KCall360/thresholds-of-ruin/pull/23)
after Windows and Linux CI passed. Phase C is implemented and locally verified on
`codex/checkpoints-compaction`. The user authorized publishing the Phase C PR and
merging after final-head Windows and Linux CI pass. The parent commit is `5a79c3a` (accepted game-design
requirements and milestone plan). Do not repeat Phase B publication.

The user explicitly requested Phase C and ongoing performance maintenance.
[Development practices](../CONTRIBUTING.md) now require keeping profiling code,
versioned workloads and validators current as features evolve, using focused
release comparisons and investigating material regressions. The full benchmark
matrix is not required on every change. This turn does not implement Phase D.

## Phase C implementation

Read [checkpoints](checkpoints.md), [background saving](background-saving.md), and
[Phase C findings](phase-c-findings.md) for the contract, measurements and limits.

- Protocol 12 and `diagonal-v11` remain unchanged. Save format is now 5; older
  formats are rejected without migration. Existing local saves were preserved.
- Checkpoints capture authoritative simulation/navigation, revisions, branch and
  all 128 rewind boundaries. Repeated worlds, geometry, navigation and item maps
  are shared in the encoded snapshot; these backend types never enter the wire.
- The worker encodes and atomically installs snapshots, retains covered frames in
  history, and rotates the active journal inside one SQLite transaction.
- Startup validates all retained frames, rebuilds receipts and history, restores
  the selected snapshot, and simulates only its tail. History loading remains
  linear and candidate copying remains Phase D work.
- The default capture interval is 1,024 entries; zero disables it for diagnostics.
  Snapshot payloads are capped at 64 MiB. Save failures retain pending work and
  require explicit retry under the existing background-save contract.
- Profiling includes capture cost, worker snapshot bytes/encoding time and startup
  loaded/replayed counts. `--phase-c` selects five cases; `--case NAME` selects a
  single case in both the benchmark and independent validator.

## Verification

Workspace Rust suites passed in debug and release during implementation. After
compaction refinements, focused server storage/checkpoint/recovery/performance
and simulation/world snapshot tests were rerun. Clippy with warnings denied,
formatting, warning-free rustdoc, architecture and documentation checks pass.

The final pre-push run passed full workspace debug/release tests, Clippy, formatting,
architecture checks and rustdoc including private items. Full Python discovery ran
72 tests: 71 passed and the native mouse test hit the sandbox's denied `SetCursorPos`.
All five travel process tests then passed with desktop access outside the sandbox.

All three checkpoint process tests and six existing background-save process tests
pass in both debug and release. They use real headless, text and native ASCII
clients. Fault tests terminate child processes at every checkpoint transaction
stage and include a torn uncommitted database extension. SQLite hot-journal
recovery runs before page-alignment validation. These tests do not prove hardware
power-loss durability.

All four desktop launchers passed real connection, fresh-save retention and
owned-process cleanup checks, including three completed cycles of the 256-region
spectator demonstration. Verification required Windows CIM access outside the
sandbox. Local launcher outputs and saves remain ignored and were preserved.

Retained measurements are in `docs/measurements/phase-c-2026-09-24/`: three
five-case passes plus a consecutive same-binary largest-case comparison, totaling
17,701 ordered attempts. The final paired default interval reduced restart from
119.00 to 7.02 seconds and simulated 474 rather than 11,499 records. Median action
latency was 14.39 versus 13.97 ms. Cross-run tail variability is retained explicitly;
do not infer that checkpointing accelerates ordinary actions or meets all of the
broader milestone's latency goals. The provisional 8 ms p95 target remains unmet.

## Next work

Publish the reviewed Phase C diff after the required pre-push checks from
README/CONTRIBUTING, including full Python discovery. Require Windows/Linux CI
on the final PR head before merging; the live PR is authoritative for publication
status. Keep credentials, local saves and diagnostic logs outside Git.

Phase D (state-copy and observation scaling) is next, with Phase E client
responsiveness after it. Continue to use targeted profiling when implementing
those phases and subsequent gameplay features.
