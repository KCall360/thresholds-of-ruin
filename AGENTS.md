# Instructions for AI coding agents

This file applies to any AI agent working in this repository. It supplements,
and never replaces, [development practices](CONTRIBUTING.md) and the
[testing policy](docs/testing.md), which apply in full.

## Working style

- **Preserve context.** Search for relevant symbols and sections before reading
  whole files. Start with change summaries, then inspect targeted diffs. Bound
  command output and expand only when needed.
- **Reuse established facts.** Revisit their sources only when something
  changes or you're uncertain.
- **Summarize investigations** as conclusions, evidence paths, and unresolved
  questions, not full transcripts. Keep progress updates to new findings,
  meaningful changes, and blockers.
- **Saving context never justifies skipping work.** Don't skip required
  checks, and don't hide failures, limitations, or evidence someone would need
  to assess a conclusion.

## Test output

- Run every required unit, integration, and process test. Redirect stdout and
  stderr to log files under `.local/` (gitignored) and check exit codes.
- On success, report only a compact pass/fail summary. Don't load passing test
  listings or full logs into context.
- On failure, surface the relevant failure output, expanding log inspection only
  as needed to diagnose it.
- Keep logs for investigation, but don't commit routine test output.
- Quiet reporting must not skip tests or hide failures, skipped checks, or
  checks that couldn't run.
- `.local/` files exist only on the development machine. Don't link to them from
  committed documentation as if other readers can open them. Summarize the
  results in the relevant guide instead.

## Performance measurements

- For release before/after comparisons, use `scripts/perf_compare.py BASE --case ...`
  rather than hand-built reference binaries. Don't run it while builds or test
  suites are running, and report its tables compactly: cases, n, p50/p95/max,
  and any count changes or failed runs.
- Add accepted headline results to `perf/ledger.jsonl` with
  `scripts/perf_ledger.py add`. Compare timings only between lines with the same
  machine fingerprint.
- Raw samples go to GitHub release assets, never Git. **Ask the maintainer
  before creating any release or uploading anything.** See the
  [performance harness](docs/performance-harness.md#publishing-raw-measurements).

## Publishing

Batch plan and documentation updates and publish at meaningful checkpoints or
when the maintainer asks (see [publishing](CONTRIBUTING.md#publishing)). Get the
maintainer's explicit authorization before starting a new milestone, merging, or
making a compatibility-breaking format decision.

## Windows development host

On the maintainer's Windows machine:

- Use `CARGO_BUILD_JOBS=2` and `CARGO_PROFILE_DEV_DEBUG=0`, UTF-8 Python, and
  temp files inside the workspace.
- Don't edit Rust inputs while a build is running.
- Native mouse tests need interactive desktop permission. A sandbox denial
  (for example of `SetCursorPos` or temp-directory access) is an environment
  failure: rerun with the needed access and report it.
- System Python doesn't include Pillow; scripts that need it use the bundled
  runtime Python.

### Desktop launchers

Whenever the build is updated, keep the maintainer's three desktop launchers
current: **Text**, **ASCII**, and **Text + ASCII Spectator**. (The former
"256 Region Spectator" launcher was removed at the maintainer's request. Keep
its benchmark driver, `scripts/performance_driver.py`, but don't recreate the
shortcut.)

For each launcher:

1. Build every required binary. `cargo check` doesn't update executables.
2. Verify the helper scripts and the actual executable targets.
3. Check a real client connection and presented frames.
4. Confirm that each launch creates a fresh save, keeps prior saves, uses a
   separate spectator credential, and cleans up only its own processes.

Launchers, their credentials, and saves stay outside Git. See the
[performance harness](docs/performance-harness.md#observable-256-region-run)
for the shared demonstration driver.

## Current work

Update this section at meaningful checkpoints with the current scope, decisions,
blockers, and next steps. Keep it short and link to guides rather than
duplicating them. Earlier handoff notes are in the
[`docs-history-2026-09` archive](https://github.com/KCall360/thresholds-of-ruin/blob/docs-history-2026-09/docs/session-handoff.md).

**Milestone 4e — region streaming, generation, and asset palettes (in progress).**
The first slice is merged: a structural `RegionCatalog`, deterministic directed
preload-horizon planning, the `tor-scenario horizon` command, and the
`horizon-profile` example. See [region streaming foundations](docs/region-streaming.md).
Ordinary games still activate every region, and no format bump was needed.

Next, define the runtime transition contract before wiring the planner into
gameplay:

1. Pin cross-boundary body and effect dependencies.
2. Freeze scheduler and AI time without catch-up.
3. Keep stable references into inactive regions. Current checkpoint validation
   assumes that referenced locations exist in the loaded world, so decide how
   inactive regions keep those references before changing that invariant.
4. Then partition persistence and add deterministic generation and palette
   delivery.

Don't treat the planner's `deactivate` candidates as permission to unload state.
The streaming design must account for frozen attack progress and recovery, AI
memory and visit locations, motion and pending effects, and stable item and
objective references, with deterministic reactivation. Keep the 4d dungeon,
checkpoint, retry, rewind, disclosure, and native-client acceptance tests
passing, along with its performance requirements.

**3s — three-dimensional sight (in progress, separate from 4e).** See
[three-dimensional sight](docs/sight-3d.md). Done on the `design/3d-sight`
branch: the exact reference and accelerated builders, gameplay sight from
declared body eye cells, authored door heights, and floors and ceilings as seen
solid cells derived by clients, and the scene cache. It's open as PR #40. The
latency regression that blocked it is fixed: profiling showed that door
approaches, not scenes, were most of the cost, and with that fixed and the
scene cache added, `r64-a8-h100-memory` p95 is 2.6 ms against 3.3 ms on
`main` (see
[reference implementation findings](docs/sight-3d.md#reference-implementation-findings)).
Next: update the PR description, wait for CI on the final commit, and **merge
only with the maintainer's approval**.

After that: view-delta observation updates, then the remaining client changes.
A NetHack-style ASCII client redesign is deferred until after 3s.

On this machine, rustc can run out of memory when other applications use most
of it. If a build fails with "memory allocation failed", lower
`CARGO_BUILD_JOBS` and rerun the failed step; never start a build while another
build or test suite is running.

**Open performance items** are tracked in the
[performance plan](docs/performance-persistence.md#open-work), including the
eight-actor combat and dense-falling p95 overruns and the deferred client
timing-tail investigation.

**Known intermittent issue:** one 4d debug run timed out on the final text save
in the dungeon acceptance test. It passed in isolation and in both later full
runs; no cause was found. The test now keeps server diagnostics when a save
barrier fails. If it recurs, investigate it using those diagnostics.
