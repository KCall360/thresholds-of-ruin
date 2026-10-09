# Testing and verification policy

Testing is a core part of every change in this repository. A feature, fix, or
refactor is not complete until it has been tested at every layer it touches, on
both supported platforms, in debug and release builds. This page is the
authoritative statement of that policy. [Development practices](../CONTRIBUTING.md)
cover the rest of the contribution workflow.

## Principles

1. **Test-driven development.** For simulation rules, protocol behavior, and
   client interactions, write a failing behavior test first, then implement,
   then refactor with the tests passing.
2. **Test outcomes, not implementation details.** Assert what a player, client,
   or save file can observe. Don't copy the implementation into the test.
3. **Every layer that changes gets tests.** A feature needs unit tests where
   logic changes, integration tests across crate boundaries, and an end-to-end
   acceptance test that launches the real applications.
4. **Every bug gets a regression test.** The test must fail before the fix and
   pass after it. Keep it permanently.
5. **Windows and Linux are both first-class.** Windows is the primary platform;
   Linux is tested continuously. A change isn't done until CI passes on both.
6. **Debug and release both run.** Optimized builds can expose timing and
   ordering problems that debug builds hide.
7. **Performance is a tested requirement.** Profiling code, workloads, and
   report validators are maintained like production code (see
   [performance testing](#performance-testing)).
8. **Keep `main` green.** Don't merge failing, flaky, or permanently ignored
   tests. An acceptance scenario that isn't implemented yet belongs in the
   [roadmap](milestones.md), not in an `#[ignore]` test.
9. **Report honestly.** A check that couldn't run is reported as not run, never
   as passing. Failed runs, skipped checks, and limitations are recorded and
   never hidden.

## Required test layers

| Layer | What it proves | Where it lives |
| --- | --- | --- |
| Unit | Individual rules and data structures behave correctly, including boundaries and overflow | `#[cfg(test)]` modules inside each crate |
| Crate integration | Public APIs of one crate work together | `crates/<crate>/tests/it/*.rs`, compiled as one binary per crate |
| Cross-crate integration | Simulation, world, and server behave correctly together | `crates/test-support/tests`, `crates/server/tests` |
| Protocol | Real WebSocket traffic, authorization, disclosure, retries, and ordering; recorded samples of every message kind round-trip exactly | `crates/server/tests/it/websocket.rs`, `wizard_websocket.rs`, `crates/protocol/tests` (samples recorded by `scripts/record_wire_samples.py`) |
| Persistence and recovery | Saves, checkpoints, crash rollback, corruption, and replay | `crates/server/tests/it/background_save.rs`, `checkpoints.rs`, `recovery_fixtures.rs` |
| Client model | Parsing, presentation models, input mapping, and shared client state | `crates/client-*/tests` |
| Actual-process acceptance | The real server and clients work end to end | `scripts/test_*_process.py` |
| Scenario packages | Authored content validates and loads, and every test package is named by a test | `scenarios/`, `crates/server/tests/it/scenario_packages.rs`, `scripts/test_scenario_references.py` |
| Package invariants | Every package rejects atomically, answers retries, replays exactly after a restart, and rewinds to its start | `crates/server/tests/it/invariants.rs` |
| Documentation | Local links resolve, guides are indexed, and stated versions match the code | `scripts/test_documentation.py` |
| Performance tooling | Comparison logic, workload report validators, and the performance ledger's format (never timing thresholds) | `scripts/test_perf_compare.py`, `scripts/test_workload_reports.py`, `scripts/test_perf_ledger.py` |
| Dependency boundaries | Crates only depend on permitted crates | `scripts/check_architecture.py`, `scripts/test_check_architecture.py` |

### Acceptance tests with real applications

Every user-visible feature needs an **actual-process acceptance test**: a Python
test in `scripts/` that builds and launches the real `tor-server` and the
affected clients (text, native ASCII, and/or headless), drives the feature, and
asserts both the server's authoritative result and what the client shows.

- Frontend features must launch the actual application. Parser,
  input-model, and presentation-model tests are required too, but they don't
  replace a launched client.
- Graphical tests use a real native window. At least one scenario per
  frontend feature should send genuine native keyboard (and, where relevant,
  mouse) events rather than only injecting events through the input model.
- Include save/resume, reconnect, spectator access, and rewind when the
  feature interacts with them: when a client holds state for the feature (map
  memory, place names, palettes, a pending intention) or presents it
  differently to a spectator. Server-side replay, restart, retry and rewind of
  every package are already covered by the
  [package invariants](#package-invariants); don't repeat them per feature.

Process tests share [`scripts/process_harness.py`](../scripts/process_harness.py).
Derive each test class from `ProcessTestCase`, which builds the binaries once
per run and gives each test its own directory and save. Its helpers are grouped
by client:

| Helper | Use it for |
| --- | --- |
| `server(*args, wizard=, scenario=, ...)` | Start `tor-server` on a free port; sets `self.address` |
| `client()`, `act()`, `request()`, `command()`, `frame()` | Drive and read the headless client |
| `wizard()`, `wizard_command()` | Privileged setup through the headless client; the wizard never takes control |
| `play(client, steps)` | Play fixture steps (`{"move": ...}`, `{"take": ...}`) as ordinary actions |
| `text_client()` | The text client's scripted line interface |
| `adventure()`, `say()`, `send()` | The text client's interactive prompt |
| `window()`, `ascii_frame()`, `key()`, `native_keys()` | The native ASCII client, through automation or genuine OS events |

Set `graphical = True` on a class that opens native windows. Test modules
never import each other; anything two of them share belongs in the harness.

`act()` waits for the matching simulation lifecycle update after admission;
`command()` returns at request acknowledgement. Native `key()` tracks the input
acknowledgement across frames and waits until queued work has executed. Use
`wait_for_simulation=False` when testing a queue control whose acknowledgement
must return while work remains queued for a later turn.

### Scenario packages, fixtures and wizard commands

- Use ordinary validated [scenario packages](scenario-packages.md) for the initial
  setup of unit, integration, and process tests. Test-only packages live in
  `scenarios/tests/`. Keep action sequences and assertions in the test harness,
  and reference stable authored entity IDs and anchors.
- Use scripted [wizard mode](wizard-mode.md) commands to test privileged
  behavior and deliberate runtime changes (for example: place a monster and
  equipment, teleport into position, then fight using ordinary actions). Test a
  wizard feature through its own privileged commands. Versioned process-test
  fixtures (wizard command lists and fixture walks) live in
  `scripts/fixtures/*.json`; load them with `load_fixture()`.
- Setup shortcuts must not bypass the behavior under test. A wizard-built
  scenario doesn't prove that a feature works, or is properly restricted, in a
  normal game, so normal-play coverage is still required.
- Use the headless client for setup and driving, including wizard setup.
  Use the text client only when the behavior under test is text input or
  presentation, and the native ASCII client only for its own input and
  presentation.
- Name a test package after the feature or situation it sets up (`doors`,
  `travel-hazard`), in lowercase words joined by hyphens.
- Checkpoint fixtures are ordinary saves created by reproducible setup
  sequences. They load through the normal path.
- Scenario certificates (`validation.json`) must be regenerated after any
  package source edit.

### Package invariants

[`crates/server/tests/it/invariants.rs`](../crates/server/tests/it/invariants.rs)
plays every package in `scenarios/` and `scenarios/tests/` for a few turns,
the package's own AI included, and checks that:

- a rejected command changes no actor's state;
- a retried request is answered without acting again;
- a restart replays every actor's state exactly;
- rewinding to the start restores the initial observation.

A new package is covered as soon as it exists. Feature tests assert what is
particular to the feature and leave these generic checks to the invariants.

### Test organization and names

- Name a test for the behavior it proves, as a sentence:
  `a_rejected_door_action_changes_nothing`, not `test_doors_2`. Rust tests
  don't take a `test_` prefix; Python test methods need it, followed by the
  sentence.
- Keep one behavior per test. Split a test when its failure would leave the
  reader guessing which of several behaviors broke; shared setup goes in a
  helper, not in one long test.
- Rust integration tests share helpers through each binary's support module
  (for the server, [`support.rs`](../crates/server/tests/it/support.rs):
  `act`, `play`, `run_ai_turns`, `wizard`, `package`, `load`). Add a helper
  there rather than another private copy in a test file.
- Put a test in the file for the behavior's own subject. A file of mixed
  subjects is a sign to split it.

## Bugs and regressions

When you find or fix a bug:

1. Write a test that reproduces it and confirm that it **fails**. Keep the
   failing output locally as evidence.
2. Fix the bug and confirm the test passes.
3. Add the test at the lowest layer that reliably reproduces the bug. If the
   bug was user-visible, also extend the relevant process test.
4. If a performance investigation exposes a correctness bug, fix it with a
   regression test in the same way (for example, an invalid checkpoint encoding
   found during long-history profiling gets a forced-checkpoint restart test).
5. When a bug can't be fixed yet, record it with its reproduction in the
   [roadmap](milestones.md) or feature guide. Don't add an ignored test.

## Cross-platform requirements

- **CI:** [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs the
  build and test checks on `windows-latest` and `ubuntu-latest`, and the
  platform-independent tooling and dependency checks once on `ubuntu-latest`.
  All of them must pass before a PR is merged, except the dependency-advisory
  check, which only warns on PRs so a newly published advisory doesn't block
  unrelated work. CI also runs weekly on `main`,
  so new advisories and stable-toolchain changes surface without waiting for
  an unrelated PR.
- **Graphical tests require a display.** Linux CI installs X11 libraries,
  Xvfb, xauth, and xdotool and runs the Python suites under
  `xvfb-run -a -s "-screen 0 1280x1024x24"`. Windows uses its native desktop.
  A missing display is a **test failure**, never a skip. There is no
  headless-success fallback.
- **Local Windows runs:** native mouse and keyboard tests need interactive
  desktop access. If a sandbox denies an OS call (for example `SetCursorPos`),
  that's an environment failure: rerun with desktop access and report it. Don't
  record it as a pass or skip it silently.
- **Platform-specific code** (native windows, file durability, paths) needs
  tests that run on both platforms, or a documented reason why one platform
  can't exercise it.

## Running the checks

**The tiers change when checks run, never what gets tested.** Every rule above
still applies to every change. That includes TDD, tests at every layer that
changes, an actual-process acceptance test for every user-visible feature, and
a regression test for every bug. A tier can only run tests that exist, so a
change that adds a feature or fixes a bug without adding its tests is
incomplete, however green the tiers look. Tiered runs only save time
because the suite is complete and CI runs all of it.

[`scripts/verify.py`](../scripts/verify.py) runs these checks in tiers. It
writes each step's log to the gitignored local directory, checks exit codes, and prints a compact
table showing which steps passed, failed, or didn't run, with their durations.
Use focused checks during development and broad tiers at stable checkpoints.
The publication gates remain required; a higher tier can cover a lower tier
without a duplicate run on unchanged inputs:

| Tier | Required | What it runs | Why it matters |
| --- | --- | --- | --- |
| Focused checks | During edits and at completed behavior checkpoints | Failing behavior/regression and affected unit tests during edits; affected integration and actual-process scenarios when the behavior is complete, repeated after changes affecting it | Establish a failing reproduction first and give fast feedback while retaining complete coverage before merge |
| `quick` | At a stable, cohesive checkpoint, unless a required higher tier covers it | Formatting, clippy and debug tests for the affected packages, the dependency check, the Python tool tests, and the affected process tests | Catch broader regressions after focused checks and architectural review, before moving to another checkpoint |
| `push` (default) | **Before every push**, unless a successful full run covers the same unchanged inputs and configuration | Every debug check CI runs | Catches regressions anywhere in the workspace and in any client before publishing the branch; CI checks release behavior before merge |
| `full` | Required when CI cannot run or on request; otherwise optional | Everything CI runs on one platform, debug and release | A local full run covers the local push gate and can investigate either profile; both-platform CI is still required before merging |
| CI | **Required before every merge** | The full matrix on Windows and Linux, plus the tooling and dependency checks | The only gate that proves both platforms and both profiles. Nothing replaces it |

CI runs the debug and release partitions of the full plan in separate jobs on
each platform. `full --ci-profile debug` runs the first six stages, and
`full --ci-profile release` runs the two release stages. Their ordered union is
the unchanged local full plan. The debug partition contains the same checks as
local `push`, but a CI run does not certify a local run. Neither partition alone
satisfies the full or merge gate. Native Windows tests and Linux Xvfb tests remain in
both profiles. Profile caches are separate, and each job preserves its logs for
investigation. The existing required platform checks fail unless every profile
on both platforms succeeds, including when a profile is cancelled or skipped.
This changes CI scheduling, not test selection or individual test deadlines.

### Development loop

Batch a cohesive change through focused
checks and architectural review before starting a broad tier. Run the new
regression against the failing implementation first, apply the fix, then run
its affected unit tests. Run affected integration and process scenarios when
the behavior is complete; repeat them when later changes affect it. Expand the focused set
when a failure or changed boundary warrants it. Do not run quick, push and full
back to back merely because all three commands exist. Run one broad debug
`push` gate per stable PR checkpoint; a successful unchanged full run also
covers that gate. Let CI run the broad release suites before merge, including
for protocol, ruleset, save-format, persistence, storage, toolchain and dependency
changes. These boundaries still require complete coverage in both profiles.

Run targeted local release tests when investigating debug/release differences,
optimized behavior, timing or ordering issues, and use release builds for
performance measurements. Select the affected tests and workloads based on the
behavior being investigated. Do not run the whole local release suite merely
because a performance measurement needs release binaries. Escalate to local
`full` when CI cannot run or a maintainer requests it. Local full coverage does
not waive the requirement for both-platform CI before merge.

Focused commands use the existing suite; they introduce no separate test tier
or exclusions. For example:

```sh
cargo test -p tor-server --lib checkpoint_rejects_terminal_effects_on_expired_pause_metadata --locked
python -m unittest -v scripts.test_intention_process
```

Set `PYTHONPATH=scripts` for targeted process-module commands that import shared
harness modules. Keep command output and exit codes under the gitignored local
log directory. Shared simulation, protocol or persistence changes need broader
coverage at the stable checkpoint even if their focused regression is small.
Do not overlap builds, suites or performance measurements on the development
host. Keep native graphical tests serialized.

### Verification evidence and reuse

A successful full run covers all checks in the local push gate. It can satisfy
that gate without an additional push-tier run only when the tested inputs,
toolchain, build profiles and test configuration are unchanged. Record the
checkout commit, uncommitted changes, relevant environment/configuration,
commands, profiles, exit codes and log location. A commit that only records
exactly the tested file contents does not invalidate the evidence.

Do not infer validity from a timestamp, a previous green result or a matching
branch name. Source, tests, fixtures, scenario packages, generated certificates,
lockfiles, build configuration or toolchain changes invalidate affected evidence;
run the required gate again after changes to its inputs. Failed, incomplete,
waived or targeted runs do not count as a successful full run. Report waivers
separately; they do not remove ordinary CI coverage.

The final commit must still pass the complete Windows/Linux CI matrix, in debug
and release. No tests are removed, ignored or permanently excluded by this
workflow. Harness parallelism, timing changes or test selection changes require
their own reviewed implementation and coverage; this policy does not silently
change the runner or CI.

When a feature is implemented or a bug is fixed, add its tests to the suite
in the same change, at the layers described in
[required test layers](#required-test-layers) and
[bugs and regressions](#bugs-and-regressions):

- **New feature:** unit tests for new rules, integration tests where it crosses
  crate boundaries, protocol, persistence, and client-model tests where those
  change, and an actual-process acceptance test (`scripts/test_*_process.py`)
  that launches the real server and clients.
- **Bug fix:** a regression test at the lowest layer that reproduces it,
  confirmed to fail before the fix. Extend the relevant process test too if the
  bug was user-visible.
- **Latency-sensitive behavior:** extend the performance workloads and
  contracts (see [performance testing](#performance-testing)).

Put new Rust integration tests in the crate's single test binary
(`crates/<crate>/tests/it/`) as a module listed in `tests/it/main.rs`. Put new
process tests in `scripts/test_*_process.py`, where every tier finds them
automatically. A process test for a new client binary also needs an entry in
`verify.py`'s `CLIENT_BINARIES`, and `scripts/test_verify.py` must cover it.

```sh
python scripts/verify.py quick
python scripts/verify.py            # push tier
python scripts/verify.py full --rerun-failed
```

The affected packages are the changed crates plus every workspace crate that
depends on them. Shared inputs (`Cargo.lock`, toolchain, scenarios) and any
path the script doesn't recognize select everything, so a tier can run more
than it needs but never less. A change to a runtime crate runs every process
test. A change to a client, or to a process-test helper script, runs the
process tests that can reach it.

The script chooses `CARGO_BUILD_JOBS` from free memory, refuses to start while
another cargo or rustc process is running, and never overlaps its own steps.
`--rerun-failed` reruns failed Python tests once. The step stays failed, and
the summary says whether the rerun passed, so an intermittent failure is
visible instead of hidden.

A lower tier never replaces a higher one when that one is required, and no tier
replaces CI. The full Windows and Linux matrix below must pass on the final
commit before merging. The individual commands, which the
`full` tier runs, are:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --release --locked
python -m unittest discover -s scripts -p "test_*.py" -v
python scripts/check_architecture.py
```

Also build the API documentation with warnings denied, as CI does:

```sh
cargo doc --workspace --no-deps --document-private-items --locked
```

Set `RUSTDOCFLAGS=-D warnings` first (`$env:RUSTDOCFLAGS = '-D warnings'` in
PowerShell). To run the process tests against optimized binaries, set
`TOR_TEST_PROFILE=release` and run `python -m unittest discover -s scripts -p "test_*process.py" -v`.
Python is used only for development checks.

The tooling and dependency checks use tools that aren't part of the Rust
toolchain. Install the pinned versions from
[`.github/requirements-lint.txt`](../.github/requirements-lint.txt), and
`cargo-deny` with `cargo install cargo-deny --locked`, then run:

```sh
actionlint
ruff check scripts
cargo deny check
```

What CI runs on both platforms:

| Step | Command |
| --- | --- |
| Formatting | `cargo fmt --all --check` |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Python checks and debug process tests | `python -m unittest discover -s scripts -p "test_*.py" -v` |
| Dependency boundaries | `python scripts/check_architecture.py` |
| API docs | `cargo doc --workspace --no-deps --document-private-items --locked` with `RUSTDOCFLAGS=-D warnings` |
| Rust tests (debug) | `cargo test --workspace --locked` |
| Rust tests (release) | `cargo test --workspace --release --locked` |
| Release process tests | `TOR_TEST_PROFILE=release`, `test_*process.py` discovery |

What CI runs once, on Linux:

| Step | Command |
| --- | --- |
| Workflow lint | `actionlint` |
| Python lint | `ruff check scripts` (bug-focused rules in [`ruff.toml`](../ruff.toml)) |
| Dependency advisories | `cargo deny check advisories` (advisory-only on PRs; fails on `main` and the weekly run) |
| Dependency policy | `cargo deny check bans licenses sources` (policy in [`deny.toml`](../deny.toml)) |

If any check can't run locally, say so in the PR and explain why. CI is still
required on the final commit before merging.

## Specialized requirements

### Determinism and disclosure

- The simulation must be reproducible from its seed and journal. Tests compare
  replayed, restarted, and rewound state with the original, and check the same
  seed yields the same results.
- Test that clients receive only disclosed observations: hidden items,
  identities, geometry, AI memory, and other actors' private commands must never
  appear in protocol messages, history, or error responses.
- Rejected commands must be atomic: test that they change no state, time,
  receipts, or history.

### Persistence and recovery

- Storage changes need fault-injection tests at every write boundary: errors
  and real child-process termination before and after append, checkpoint
  installation, history retention, rotation, and commit.
- Test crash rollback of acknowledged-but-unsaved play, explicit-save barriers,
  duplicate and conflicting retries, retained branches, private annotations,
  and permanent wizard marking across restart.
- Corrupt or unsupported saves must fail closed with the file left unchanged.
- Process-kill tests establish transaction recovery, **not** hardware
  power-loss durability. Don't claim otherwise. See
  [background saving](background-saving.md) for the durability contract.

### Formats and fixtures

When the protocol version changes, rerun `python scripts/record_wire_samples.py`
to record `crates/protocol/tests/fixtures/wire-v<version>.json` and review its
difference: every change in it is a change to the wire format.

The project supports only the current protocol, save format, and ruleset (listed
in the [roadmap](milestones.md#current-implementation)). When a format changes,
update code, fixtures, scenario certificates, and tests together, and keep the
tests that reject older versions. Don't add compatibility readers to keep old
fixtures loading. `scripts/test_documentation.py` checks that the versions stated
in the docs match the code.

Saved scenario metadata has a golden fixture produced by the current storage
writer. After an accepted save-schema change, set `TOR_RECORD_SCENARIO_FIXTURES`
to the new fixture's absolute path and run
`cargo test -p tor-server --lib current_writer_matches_golden_scenario_fixture`.
Review the generated diff, unset the variable, and rerun the ordinary schema
tests. Recording is an explicit fixture-update mode; normal tests compare the
writer against the committed fixture and round-trip it through the strict reader.
Obsolete package metadata remains covered by rejection tests.

## Performance testing

Performance is an ongoing requirement, not a finished milestone. The provisional
targets, measurement model, and open work are in the
[performance plan](performance-persistence.md); reproduction commands are in the
[performance harness](performance-harness.md).

### Keep profiling tools useful

- Maintain the instrumentation, versioned workload fixtures, real-client
  drivers, and report validators as production code changes. If a change breaks
  a benchmark or validator, fixing it is part of that change.
- When a feature adds latency-sensitive behavior, extend the workloads to cover
  it, with small and large cases for each relevant scale dimension (history,
  regions, actors, items, remembered cells, and so on).
- Never silently change what an existing workload version measures. Add a new
  workload or version instead, so older baselines stay comparable.
- Periodically check that representative profiling runs still produce
  actionable measurements.

### Targeted checks for each change

Run targeted **release-build** before/after comparisons for changes to
simulation, perception, persistence, protocol delivery, or client
application/rendering:

1. Choose cases that exercise the changed behavior plus a representative
   existing interaction (for example `latency_bench --case r8-a1-h100-memory` and
   `--case r64-a8-h100-memory`).
2. Run base and head release binaries interleaved on the **same machine and
   configuration**, with no builds or test suites running at the same time.
   [`scripts/perf_compare.py`](performance-harness.md#before-and-after-comparisons)
   does this: `python scripts/perf_compare.py main --case r8-a1-h100-memory`.
3. Record the cases, sample counts, p50/p95/maximum latency, relevant
   operation and byte counts, and limitations.
4. Investigate material regressions and tail spikes before calling the feature
   complete. Don't relax targets to fit a new feature.

The full benchmark matrix isn't required for every change. Expand it when
results are inconsistent, a regression is unexplained, or a change affects
several subsystems. Documentation-only changes don't need benchmarks.
Profiling never replaces the correctness tests above.

### What CI enforces, and what it doesn't

- **Enforced in CI:** deterministic operation counts, scale ratios, byte-growth
  bounds, and report-validator contracts (for example, waits construct no scenes;
  one-hop horizon queries expand one region regardless of catalog size), and
  the performance ledger's format. Put stable scaling regressions in automated
  tests.
- **Diagnostic only:** wall-clock timings. They depend on the machine, so
  they're recorded with the change rather than used as pass/fail gates.

### Measurement discipline

- Keep timing boundaries explicit. Separate application work from diagnostic
  or reporting overhead, and don't compare end-to-end client intervals directly
  with server-only phases.
- Keep failed and incomplete runs. A rejected prefix is not a passing run.
- Not reproducing a tail spike doesn't mean it's fixed. Don't subtract
  unexplained intervals or attribute them without evidence.
- Record the machine, storage volume, toolchain, source commit, and binary
  hashes with measurements. Wall-clock timings are comparable only on the same
  machine, storage, build profile, and workload version; operation and byte
  counts are deterministic.
- Keep raw samples and logs out of Git. Publish each accepted measurement set's
  raw data as a GitHub release asset (ask the maintainer first), add its
  headline cases to the
  [performance ledger](performance-harness.md#performance-ledger), and
  summarize the results, cases, and limitations in the relevant feature guide.
  See [publishing raw measurements](performance-harness.md#publishing-raw-measurements).

## Focused iteration and local mouse exception

During individual edits, run failing behavior/regression tests and affected unit
tests. Run integration and real-process acceptance tests at completed behavior
checkpoints, repeating them after changes to the covered behavior. Run quick at
cohesive subsystem checkpoints, rather than after every small edit. Coverage at
every changed layer remains required before merge; push and final-commit
Windows/Linux debug/release CI gates remain mandatory.

The maintainer authorizes a local-only waiver for the native underscore/mouse
travel test. `python scripts/verify.py push --waive-local-mouse` records that
exact test as a waived
local failure if it fails, preserving its diagnostics. It never waives another
test, a build failure, or any CI failure. CI runs the unchanged test normally.
