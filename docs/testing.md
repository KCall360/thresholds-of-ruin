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
| Crate integration | Public APIs of one crate work together | `crates/<crate>/tests/*.rs` |
| Cross-crate integration | Simulation, world, and server behave correctly together | `crates/test-support/tests`, `crates/server/tests` |
| Protocol | Real WebSocket traffic, authorization, disclosure, retries, and ordering | `crates/server/tests/websocket.rs`, `wizard_websocket.rs`, `crates/protocol/tests` |
| Persistence and recovery | Saves, checkpoints, crash rollback, corruption, and replay | `crates/server/tests/background_save.rs`, `checkpoints.rs`, `recovery_fixtures.rs` |
| Client model | Parsing, presentation models, input mapping, and shared client state | `crates/client-*/tests` |
| Actual-process acceptance | The real server and clients work end to end | `scripts/test_*_process.py` |
| Scenario packages | Authored content validates and loads | `scenarios/`, `crates/server/tests/scenario_packages.rs` |
| Documentation | Local links resolve, guides are indexed, and stated versions match the code | `scripts/test_documentation.py` |
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
  feature interacts with them.

### Scenario packages and wizard scripts

- Use ordinary validated [scenario packages](scenario-packages.md) for the initial
  setup of unit, integration, and process tests. Test-only packages live in
  `scenarios/tests/`. Keep action sequences and assertions in the test harness,
  and reference stable authored entity IDs and anchors.
- Use scripted [wizard mode](wizard-mode.md) commands to test privileged
  behavior and deliberate runtime changes (for example: place a monster and
  equipment, teleport into position, then fight using ordinary actions). Test a
  wizard feature through its own privileged commands. Versioned wizard scripts
  live in `scripts/scenarios/*.json`.
- Setup shortcuts must not bypass the behavior under test. A wizard-built
  scenario doesn't prove that a feature works, or is properly restricted, in a
  normal game, so normal-play coverage is still required.
- New scripted automation should use the headless client for setup and
  driving. Use the text client when the behavior under test is text input or
  presentation.
- Checkpoint fixtures are ordinary saves created by reproducible setup
  sequences. They load through the normal path.
- Scenario certificates (`validation.json`) must be regenerated after any
  package source edit.

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

- **CI:** [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs every
  check on `windows-latest` and `ubuntu-latest`. Both must pass before a PR is
  merged.
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

Run the full suite before publishing a change:

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

What CI runs, on both platforms:

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

The project supports only the current protocol, save format, and ruleset (listed
in the [roadmap](milestones.md#current-implementation)). When a format changes,
update code, fixtures, scenario certificates, and tests together, and keep the
tests that reject older versions. Don't add compatibility readers to keep old
fixtures loading. `scripts/test_documentation.py` checks that the versions stated
in the docs match the code.

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
2. Preserve the pre-change release binary and run matching cases on the **same
   machine and configuration**, with no builds or test suites running at the
   same time.
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
  one-hop horizon queries expand one region regardless of catalog size).
  Put stable scaling regressions in automated tests.
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
  hashes with measurements. Keep raw samples and logs out of Git; summarize the
  results, cases, and limitations in the relevant feature guide.
