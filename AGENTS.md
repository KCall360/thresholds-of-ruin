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

- **Add tests with every change.** Each feature gets tests at every layer it
  touches, including an actual-process acceptance test. Each bug fix gets a
  regression test that fails first. Write them in the same change, following
  the [testing policy](docs/testing.md). Tiers only choose which existing tests
  run; a green tier doesn't count if the tests for the change are missing.
- Run checks through `scripts/verify.py`, and don't skip a required tier:
  `quick` after each meaningful edit (the TDD loop), the default `push` tier
  before **every** push, and `full` for save-format, protocol, ruleset,
  persistence, storage, toolchain, or dependency changes, or when CI can't run.
  CI on both platforms is required before merging. Report which tier ran, and
  any step that failed or didn't run.
- `verify.py` logs each step under `.local/verify/`, checks exit codes, and
  picks build jobs from free memory. Run other commands the same way,
  redirecting stdout and stderr to log files under `.local/` (gitignored).
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

**Test suite rationalization (in progress, branch `tests/rationalize`).**
Process tests share `scripts/process_harness.py` and use the headless client
for wizard setup; every scenario package is checked by the
[package invariants](docs/testing.md#package-invariants); the workload
validators share `workload_report.py`; protocol messages have recorded wire
samples; test packages are named for what they set up. See the
[testing policy](docs/testing.md). It needs a PR, CI on both platforms, and
the maintainer's approval to merge.

**Interactive fiction parser** (PR #55): the text client's natural-language
parser and narration; see the [parser architecture](docs/if-parser-architecture.md)
and [adventure commands](docs/text-adventure.md). The resolver modules
(`parser::scope`, `matcher`, `context`) are built and tested but not yet used:
`Dialogue` still resolves names and pronouns itself.

**Milestone 4e — region streaming, generation, and asset palettes (complete).**
The region lifecycle (PR #44), streaming through disk regions (PR #45),
preloading, per-region packages, generated regions and server palettes
(PR #47, save format 14), its review fixes (PR #53) and palettes in the text
and headless clients (PR #54). Against the pre-4e `main`, command p95 rose by
at most 13% and by under 8% in most cases; see
[region streaming](docs/region-streaming.md#performance). No ledger line was
added; that needs a raw-data upload, which the maintainer must approve. The
ASCII glyph table waits for the ASCII redesign.

**ASCII redesign attempt:** PRs #48-#50 (flat map, message log, glyph table)
were merged and then reverted by PR #52, which also fixed a race where the
autonomous pump could stale a command's revision. Their code is kept on
branch `archive/ascii-hack-prs-48-51`.

Decisions agreed with the maintainer:

- Reference points in game state, not hardcoded players, decide what stays
  active. Characters and actors clients control (`controller = "external"`)
  get them by default; AI actors don't keep regions alive.
- Only *detached* regions get disk rows, keyed by a record ID the game
  allocates, so replay and checkpoint retries reproduce them byte for byte.
  Loaded regions stay inside the checkpoint; the in-memory world keeps its
  global tables. Per-region memory tables only if measurements need them.
- Never-needed regions are never built; the package builds one region just
  before it's loaded.
- Packages have one file per region and a generated index; package format 1
  is gone (no compatibility readers). Saves pin their package and copy each
  region file they build from; resuming with regions still unbuilt needs the
  package directory.
- Generated regions fill gaps between authored ones (rooms and corridors), with
  identities fixed by region id, so nothing depends on build order; palettes
  are asset ids that clients resolve through built-in tables.
- Performance fixes must help large maps, not just small ones; prove scaling
  with operation counts at more than one size (16, 256 and 4,096 regions).
- The headless client is the automation client: add capabilities to it (it
  now takes `{"type":"wizard","command":...}`) rather than using another
  client.

The server reads a package's index, which is proportional to its region
count, and saves keep a copy of it; everything else a game holds or saves
grows with the regions played.

The desktop launchers run the 4e build (save format 14); playtest saves from
before it won't resume with them.

Don't treat the planner's `deactivate` candidates as permission to unload
state; only `Game::apply_region_transition` detaches. Keep the 4d dungeon,
checkpoint, retry, rewind, disclosure, and native-client acceptance tests
passing, along with its performance requirements.

**3s — three-dimensional sight (in progress, separate from 4e).** See
[three-dimensional sight](docs/sight-3d.md). 3D sight, authored door heights,
client-derived floors and ceilings, and the scene cache are merged (PR #40).

View-delta observation updates are merged too (PR #42): messages
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

**Running until blocked and spatial narrative** (PRs #57 and #58, protocol
20): the simulation runs until it needs client input, only the server ends a
journey, and clients pace the display; see
[running until blocked](docs/run-until-blocked.md) and the
[spatial narrative](docs/spatial-narrative-architecture.md).

**Open performance items** are tracked in the
[performance plan](docs/performance-persistence.md#open-work), including the
eight-actor combat and dense-falling p95 overruns and the deferred client
timing-tail investigation.

**Known intermittent issue:** one 4d debug run timed out on the final text save
in the dungeon acceptance test. It passed in isolation and in both later full
runs; no cause was found. The test now keeps server diagnostics when a save
barrier fails. If it recurs, investigate it using those diagnostics.
