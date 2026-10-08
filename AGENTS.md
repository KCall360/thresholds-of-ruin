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
- During development, run the failing regression first, then affected unit,
  integration and process tests after each meaningful change. At a stable,
  cohesive checkpoint, run `scripts/verify.py quick`; a required higher tier
  can cover that checkpoint without a preceding duplicate quick run.
- Before every push, pass the default `push` tier or `full`. A successful full
  run satisfies the push gate for the same unchanged inputs, toolchain and test
  configuration. The push tier runs all debug checks; broad release suites run
  in CI before merge, including for compatibility and persistence changes. Run
  targeted local release checks for performance or release-specific behavior.
  Local full is required when CI cannot run or on request. Both-platform
  debug/release CI on the final commit remains required before merging. Record
  the tested state, commands and logs; report failures and checks not run.
  See [verification evidence](docs/testing.md#verification-evidence-and-reuse).
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

**Architecture refactor (active).** The maintainer authorized the
[refactor plan](docs/refactoring.md), format decisions, pushes and PR merges.
Work stays isolated from the original dirty interactions checkout.

Merged work includes typed transport/domain/persistence and authoring/prepared
boundaries; shared observations and AI decisions/routes; backend-only portal-local
actor/item indexes and privacy-scoped history indexes; admitted intentions executed
by simulation for players, AI and native travel; durable preparation recovery;
exact stream/reset/observation and readiness contexts; opaque interaction references;
JavaScript-safe wire integers; bounded typed decoding, collection deltas and complete
snapshot assembly; fair output guarantees with queued/inflight byte ownership;
bounded transport shutdown; immutable checkpoint sharing; atomic persistence and
short worker-lock scope; lazy scenario compilation with source provenance, digest
pinning, semantic generator seeds and named random streams; nonblocking diagnostics.
Future scenario-extension contracts are documented without implementing a runtime.

PR #82 is merged after all nine exact-head CI jobs passed. Its immutable desktop
build is active and passed 43 copied-executable checks; previous builds and saves
remain retained. PR #83 is merged after all nine exact-head CI jobs passed and
adds actual queued-action outcome timing without changing runtime executables.

The strict save-decoder checkpoint compares saved input against one canonical
JSON tree rather than retaining a second input tree. Semantic equivalence, server
unit/integration and actual-release recovery checks pass. Twelve exact-fixture
restores show lower peak memory; the first small-fixture startup outlier remains
part of the evidence. The standard six-round release comparison passed all 36 validators with unchanged
operation and byte counts; timing changes were mixed and adverse samples remain.
Broad checkpoint verification, CI, publication/deployment, measured tail review
and the integrated requirement audit remain unfinished.

Use the updated [testing policy](docs/testing.md): affected tests during development,
one broad local debug gate before push, targeted release measurements/checks as
needed, and complete Windows/Linux debug/release CI before merging. The maintainer
waived only the local native mouse test; retain ordinary CI coverage and report
qualified local evidence. Text-client product fixes and scripting implementation
remain deferred. Portable save export remains a future design.
