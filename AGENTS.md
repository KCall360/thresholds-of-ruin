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
- During development, run the failing regression first, then affected unit
  tests during edits. Run affected integration and actual-process
  tests at completed behavior checkpoints and repeat them when later edits affect
  that behavior. At a stable,
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
- Scenario validation hashes manifest and region text with CRLF normalized to
  LF automatically; generated TOML uses LF. After package edits, regenerate
  certificates and run `python scripts/test_scenario_references.py` to verify
  the hashes a fresh checkout receives. Keep `.gitattributes` LF rules intact.
- Native mouse tests need interactive desktop permission. A sandbox denial
  (for example of `SetCursorPos` or temp-directory access) is an environment
  failure: rerun with the needed access and report it, except for the explicitly
  permitted local native mouse exception below.
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

Use [the roadmap](docs/milestones.md) for current scope and
[the refactor guide](docs/refactoring.md) for the completed implementation review.
The architecture refactor is published; performance follow-up remains open.
Three-dimensional sight is complete for its accepted scope, with the closeout
merged after the required CI passed.

The accepted [Rogue plan](docs/rogue-scenario-plan.md) starts with exploration-only
floor generation before the shared creature/item/survival features are complete.
Planning approval does not by itself authorize starting a new implementation
milestone. Runtime scripting and its language remain undecided.

The ASCII client follows NetHack's layout: messages on top that stay until the
next command, one full-width map that merges every height, two status lines,
and on-demand screens (no side panels). It's client-only; what needs server or
protocol support is listed in [the ASCII client guide](docs/ascii-client.md#needs-server-or-protocol-support).

Preserve the original dirty interactions checkout; it contains unpublished work
that is not the published main baseline. Keep future work isolated where needed.
Do not weaken native tests or infer a waiver from historical checkpoint notes.
Desktop deployment remains separate from publishing documentation.

### Local native mouse exception

The maintainer permits only `test_travel_process.TravelProcesses.test_native_underscore_and_mouse_click`
to fail locally without blocking verification. Record it as a waived local failure,
never a pass. Do not remove or weaken the test. CI must run and pass it; unrelated
failures remain blocking. Use the verification tool’s explicit local waiver option.
