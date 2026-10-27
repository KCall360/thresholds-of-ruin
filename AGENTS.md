# Instructions for AI coding agents

This file applies to any AI agent working in this repository. It supplements,
and never replaces, [development practices](CONTRIBUTING.md) and the
[testing policy](docs/development/testing.md), which apply in full.

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

- **Cover every changed behavior contract.** Add or extend its tests in the same
  change, following [contract ownership](docs/development/testing.md#contract-ownership).
  Preserve feature acceptance and unique boundary witnesses; existing coverage
  needs an explicit source/assertion mapping. Each bug fix gets a regression that
  fails at the intended behavior before the fix. A green tier cannot compensate
  for missing coverage.
- Follow the [development loop](docs/development/testing.md#development-loop):
  preflight compilation/lint before expensive links, establish the intended
  first-red failure, and consolidate known fixes into coherent build checkpoints.
  Validate positive fixtures before negative mutations; keep independent expected
  values, ordinary-actor and cross-feature witnesses, and explicit rejection tests.
  Diagnose production, fixture and driver failures from contract evidence rather
  than weakening assertions. Setup failures are not behavioral regression evidence.
  Use
  [`scripts/feedback.py`](docs/development/testing.md#fast-local-rust-feedback)
  for repeated single-target Rust edits; it does not qualify quick, push, full or CI.
  Run affected unit tests during edits, then integration, Service contracts and
  actual-process tests at completed behavior checkpoints; repeat affected checks
  after later edits. **Expedited, risk-based iteration is the default:** select
  the smallest complete affected-contract set, batch fixes, and reuse still-valid
  evidence. Do not repeatedly run broad local tiers as a publication ritual.
  `quick` tests directly changed packages; `quick` and `push` defer
  [extended-cost process tests](docs/development/testing.md#extended-cost-process-tests)
  to full and CI. Run the ones your change targets yourself.
- Give each Cargo target directory one owner and hand it off explicitly. Keep
  checked inputs unchanged during builds/tests; prepare other edits separately.
  On a sequential cross-worktree handoff of a shared warm target, compare source
  input hashes and refresh only changed local input mtimes before Cargo reuse;
  preserve before/after hashes and protected binaries. See the handoff guard in
  the testing guide. Do not broadly delete warm caches or infer test coverage.
  Reuse only qualified evidence and binaries, never duplicate full target builds
  across workers. Use owned sessions and exit receipts to establish run status;
  see [verification evidence](docs/development/testing.md#verification-evidence-and-reuse).
- Before publishing, complete the [risk-based local checkpoint](docs/development/testing.md#broader-checkpoints)
  and report selected, reused and CI-only coverage. Required hosted CI supplies
  qualification; a local `quick`, `push` or `full` run is not mandatory merely
  because a commit or push is due. Use a broader local tier only for a specific
  justified risk or maintainer request, not as a substitute for diagnosis.
  The [prose-only path](docs/development/testing.md#prose-only-documentation-changes)
  still requires its documentation/tooling checks. The CLI still defaults to
  `push`; explicitly choose the intended scope. Keep local feedback distinct from the
  [risk-based CI gate](docs/development/testing.md#risk-based-ci-assurance).
  CI reports **prose-only**, **selected-contract**, **exhaustive**, or the narrowly
  qualified **prior-exact-tree-full plus fresh-main** assurance below.
  Merge only after the canonical final-head plan and both required platform
  aggregates pass; unknown scope/ownership and verification/build control changes
  require full PR coverage. Only an otherwise-full main push may use the
  [exact-tree post-merge proof](docs/development/testing.md#exact-tree-post-merge-assurance)
  plus fresh integration; missing proof retains normal full execution. It is not
  a fresh exhaustive run or authority for another retained qualification.
  Selected PR/main results never qualify a full run or
  satisfy a requirement for exhaustive release/scheduled verification. Preserve
  every changed contract's independent tests and unique real boundary witnesses;
  cadence changes do not authorize deleting coverage. Run targeted local release
  checks for optimized/performance behavior. Record failures, deferred coverage,
  waivers and checks not run, and follow the documented failure-feedback process.
  See [verification evidence](docs/development/testing.md#verification-evidence-and-reuse).
- `verify.py` logs each step under `.local/verify/`, checks exit codes, and
  picks build jobs from free memory. Run other commands the same way,
  redirecting stdout and stderr to log files under `.local/` (gitignored).
- Before running a new focused test owner, verify its registered module path or
  inspect the existing owning binary with `--list`; do not infer a filter from
  its source filename. A successful zero-test run is not qualification.
- On success, report only a compact pass/fail summary. Don't load passing test
  listings or full logs into context.
- On failure, surface the relevant failure output, expanding log inspection only
  as needed to diagnose it.
- Keep logs for investigation, but don't commit routine test output.
- Maintain the [test-effectiveness ledger](docs/development/test-effectiveness.md)
  when tests expose a genuine defect or fail because fixtures/oracles were not
  updated. Distinguish production defects, stale tests, harness/infrastructure
  and unresolved cases. Count root causes, distinct failing methods and repeated
  executions separately; do not inflate totals with platform/profile repeats,
  subtests, deliberate first-red feature work or unverified claims.
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
  [performance harness](docs/development/performance.md#publishing-raw-measurements).

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
[performance harness](docs/development/performance.md#observable-256-region-run)
for the shared demonstration driver.

## Current work and protected context

Use [current capabilities and next work](docs/status.md) for scope and
[architecture](docs/architecture/overview.md) for implemented boundaries.
Performance follow-up remains open. Runtime scripting and its language remain
undecided. Planning approval does not authorize starting a new implementation
milestone.

The accepted [Rogue plan](docs/plans/rogue.md) builds on exploration-only floor
generation, then adds shared mechanics and content. Preserve the distinction
between existing foundations and unfinished survival, content and objective work.

The ASCII client uses messages on top, one full-width map that merges heights,
three status rows and on-demand screens. Its controls and limitations live in the
[ASCII guide](docs/play/ascii.md); clients do not invent missing server authority.

Preserve the original dirty interactions checkout; it contains unpublished work
that is not the published main baseline. Keep future work isolated where needed.
Do not weaken native tests or infer a waiver from historical checkpoint notes.
Desktop deployment remains separate from publishing documentation.

### Local native mouse exception

The standing exception permits only `test_travel_process.TravelProcesses.test_native_underscore_and_mouse_click`
to fail locally without blocking verification. Record it as a waived local failure,
never a pass. Do not remove or weaken the test. CI must run and pass it; unrelated
failures remain blocking. Use the verification tool’s explicit local waiver option.
If the maintainer explicitly grants a broader graphical sandbox exception for
the current task, use the testing guide's exact-method graphical manifest mode;
never infer that permission from an absent display or a historical waiver.
