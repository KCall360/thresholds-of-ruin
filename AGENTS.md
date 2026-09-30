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

Merged (PR #44): the
[region lifecycle contract](docs/region-streaming.md#region-lifecycle-contract),
implemented in the simulation. Decisions, agreed with the maintainer:

- Whole regions are active, frozen (loaded, time stopped) or detached into a
  self-contained record. The planner's horizon is what's *loaded*; the active
  set is smaller.
- **Reference points** in game state, not hardcoded players, decide what stays
  active. Characters get observing points by default; scrying or machines may
  add more.
- Frozen actors carry freeze stamps; thawing shifts their tick fields, so
  nothing catches up. Pins keep live references (bodies, attacks, reach)
  active or loaded; knowledge references may point into detached regions,
  checked through an identity directory.
- Lifecycle state is saved only when non-empty, so ordinary saves are
  unchanged. The on-disk save layout is designed in the doc; building it is
  the next slice, with **one save-format bump the maintainer has agreed to**.

**Disk streaming plan** (agreed 2026-09-29; one PR per phase, stacked in
order on `main`):

- Only *detached* regions get disk rows, keyed by a record ID the game
  allocates, so replay and checkpoint retries reproduce them byte for byte.
  Loaded regions stay inside the checkpoint, and the in-memory world keeps its
  global tables. Per-region memory tables come last, and only if measurements
  need them. This replaces the doc's save-wide version counter.
- Never-needed regions are **never built**. A region source (the package now,
  a generator later) builds one region just before it's loaded, independent
  of build order. Pinning the package by manifest hash (instead of embedding
  it) waits for per-region package files, with its own format bump
  (maintainer's decision, 2026-09-29).
- Phases (the maintainer asked to keep going without opening PRs yet):
  0. range-based detach/attach, per-region sight invalidation, and record
     validation at attach (done, branch `design/region-streaming-prep`);
  1. record IDs and a record-store interface (done,
     `design/region-record-store`);
  2. unbuilt regions and region sources (done, `design/region-sources`);
  3. disk rows, save format 13 and engine wiring, merged into one slice at
     the maintainer's request (done, `design/region-streaming-engine`):
     package games stream after every command; fixtures don't. A review of
     phases 0–3 then fixed eight findings with regression tests, added the
     policy's missing tests (validated streaming packages, an actual-process
     test, row crash tests, a `latency_bench` streaming workload and a CI
     scaling contract), exit fields for reach, and a headless `wizard` input.
     Next: one PR for all of it, merged only with the maintainer's approval;
  4. background preloading;
  5. large per-region packages, the package pin, and generation.

Don't treat the planner's `deactivate` candidates as permission to unload
state; only `Game::apply_region_transition` detaches. Keep the 4d dungeon,
checkpoint, retry, rewind, disclosure, and native-client acceptance tests
passing, along with its performance requirements.

**3s — three-dimensional sight (in progress, separate from 4e).** See
[three-dimensional sight](docs/sight-3d.md). 3D sight, authored door heights,
client-derived floors and ceilings, and the scene cache are merged (PR #40).

View-delta observation updates are merged too (PR #42, protocol 18): messages
are 78–87% smaller in the headline `latency_bench` cases; see
[view deltas](docs/protocol.md#view-deltas). No ledger line was added; that
needs a raw-data upload, which the maintainer must approve.

The remaining verification cases (lintels, pit rims, edge-touching blocks, and
a three-cell giant with its `sight-3d-giant` package) are merged too (PR #43),
so every case in the verification plan is covered.

After that: the remaining 3s client changes, which still need their scope
agreed with the maintainer. A NetHack-style ASCII client
redesign is deferred until after 3s.

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
