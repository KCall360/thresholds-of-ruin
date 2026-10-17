# Testing and verification policy

Testing is a core part of every change in this repository. A feature, fix, or
refactor is not complete until its changed contracts and affected boundaries are
covered and the full Windows/Linux debug/release CI matrix passes. This page is the
authoritative statement of that policy. [Development practices](../../CONTRIBUTING.md)
cover the rest of the contribution workflow.

The narrow [prose-only documentation path](#prose-only-documentation-changes)
uses documentation/link and tooling checks instead of application builds/tests.
All other changes retain the full gates below; uncertain scope selects full.

Start with [development setup](setup.md): Rust components, the pinned
Python environment, a graphical desktop and a local Git comparison base are
required. From the repository root, `python scripts/verify.py` runs the local
pre-push gate. The tier does not substitute for the test-writing obligations below.

## Find the requirement

- [Running checks](#running-the-checks): focused/quick/push/full and CI gates
- [Evidence reuse](#verification-evidence-and-reuse): when an unchanged run counts
- [Exact local mouse exception](#focused-iteration-and-local-mouse-exception): one opt-in local waiver, never a CI waiver
- [Required layers](#required-test-layers): unit through real-process acceptance
- [Platform requirements](#cross-platform-requirements): native desktop and both profiles
- [Persistence and formats](#specialized-requirements): faults, replay and fixture updates
- [Performance testing](#performance-testing): reproducible release evidence and limits

## Principles

1. **Test-driven development.** For simulation rules, protocol behavior, and
   client interactions, write a failing behavior test first, then implement,
   then refactor with the tests passing.
2. **Test outcomes, not implementation details.** Assert what a player, client,
   or save file can observe. Don't copy the implementation into the test.
3. **Every changed contract gets coverage.** Follow
   [contract ownership](#contract-ownership), preserving feature acceptance and
   every unique boundary guarantee.
4. **Every bug gets a regression test.** The test must fail before the fix and
   pass after it. Keep its coverage permanently; a replacement must satisfy the
   [contract ownership](#contract-ownership) requirements.
5. **Windows and Linux are both first-class.** Windows is the primary platform;
   Linux is tested continuously. A change isn't done until CI passes on both.
6. **Debug and release both run.** Optimized builds can expose timing and
   ordering problems that debug builds hide. The debug (`dev`) profile keeps
   debug assertions and overflow checks but is lightly optimized
   (`opt-level = 1`, dependencies at 2; see `Cargo.toml`), which roughly halves
   the simulation-heavy process tests for about half a minute more per
   incremental rebuild.
7. **Performance is a tested requirement.** Profiling code, workloads, and
   report validators are maintained like production code (see
   [performance testing](#performance-testing)).
8. **Keep `main` green.** Don't merge failing, flaky, or permanently ignored
   tests. An acceptance scenario that isn't implemented yet belongs in the
   [roadmap](../status.md), not in an `#[ignore]` test.
9. **Report honestly.** A check that couldn't run is reported as not run, never
   as passing. Failed runs, skipped checks, and limitations are recorded and
   never hidden.

## Required test layers

| Layer | What it proves | Where it lives |
| --- | --- | --- |
| Unit | Individual rules and data structures behave correctly, including boundaries and overflow | `#[cfg(test)]` modules inside each crate |
| Crate integration | Public APIs of one crate work together | `crates/<crate>/tests/it/*.rs`, compiled as one binary per crate |
| Cross-crate integration | Simulation, world, and server behave correctly together | `crates/test-support/tests`, `crates/server/tests` |
| Service contracts | Production Service and raw encoded messages with Python-authored fixtures/assertions; distinct from transport and client acceptance | [Service contract pilot](service-contracts.md), `scripts/service_contract_tests.py` |
| Protocol | Real WebSocket traffic, authorization, disclosure, retries, and ordering; recorded samples of every required message kind round-trip exactly | `crates/server/tests/it/websocket.rs`, `wizard_websocket.rs`, `crates/protocol/tests` (samples recorded by `scripts/record_wire_samples.py`) |
| Persistence and recovery | Saves, checkpoints, crash rollback, corruption, and replay | `crates/server/tests/it/background_save.rs`, `checkpoints.rs`, `recovery_fixtures.rs` |
| Client model | Parsing, presentation models, input mapping, and shared client state | `crates/client-*/tests` |
| Actual-process acceptance | The real server and clients work end to end | `scripts/test_*_process.py` |
| Scenario packages | Authored content validates and loads, and every test package is named by a test | `scenarios/`, `crates/server/tests/it/scenario_packages.rs`, `scripts/test_scenario_references.py` |
| Package invariants | Every package rejects atomically, answers retries, replays exactly after a restart, and rewinds to its start | `crates/server/tests/it/invariants.rs` |
| Documentation | Local links resolve, guides are indexed, and stated versions match the code | `scripts/test_documentation.py` |
| Performance tooling | Comparison logic, workload report validators, and the performance ledger's format (never timing thresholds) | `scripts/test_perf_compare.py`, `scripts/test_workload_reports.py`, `scripts/test_perf_ledger.py` |
| Dependency boundaries | Crates only depend on permitted crates | `scripts/check_architecture.py`, `scripts/test_check_architecture.py` |

### Contract ownership

Add or extend tests for each changed behavior contract at the narrowest production
boundary that proves it. Every user-visible feature needs evidence of its
externally observable acceptance behavior, with real transport, persistence,
process and client witnesses for the guarantees it touches. Existing witnesses
may satisfy unchanged boundaries only with an explicit mapping of the relevant
source paths, assertions and test IDs. New unique guarantees and interactions
need new evidence; a generic invariant does not prove states it never reaches.

Choose owners by the production boundary, not the test language. Python-authored
Service contracts exercise the production Service and encoder. The current host
does not prove socket behavior, authentication setup, durable-store recovery or
actual client behavior. Retain those independent witnesses. See the bounded
[catalog assertion mapping](service-contracts.md#catalog-assertion-ownership).

Keep regression coverage permanently, rather than requiring duplicate assertions
at every layer. Moving or removing a test requires reviewed assertion parity,
demonstrated bug detection and preservation of every unique boundary witness.
Matching names, passing counts or a faster run do not establish equivalence.
This policy does not authorize skipping tests, deferring detection, or reducing
the full CI matrix.

### Acceptance tests with real applications

**Actual-process acceptance** launches the real `tor-server` and affected clients
(text, native ASCII, and/or headless), drives the behavior, and asserts both the
server's authoritative result and what the client shows. Add or extend Python
tests in `scripts/` for new application-boundary guarantees; existing witnesses
may cover unchanged boundaries only under [contract ownership](#contract-ownership).
Do not repeat every backend scenario permutation at this layer when qualified
lower-level tests own it.

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

Process tests share [`scripts/process_harness.py`](../../scripts/process_harness.py).
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
| `window()`, `ascii_frame()`, `key()`, `native_keys()`, `native_press()` | The native ASCII client, through automation or genuine OS events |

Set `graphical = True` on a class that opens native windows. In a mixed class,
mark each audited headless/Text-only test method with the harness's
`@non_graphical` decorator. Include its setup and helper call paths in that audit;
a Text test with a native observer still needs a display. Every unmarked method
in a graphical class, including newly added methods, requires a display by
default. The display check runs per test, so non-graphical methods can run
without a display even when native methods in the same class are explicitly
waived. Direct `Process` launches of `tor-client-ascii` also require a display,
regardless of method classification or authentication outcome. This annotation
does not skip any test or authorize a waiver. Test modules never import each
other; anything two of them share belongs in the harness.

Long headless drivers that never inspect accumulated map memory may explicitly
use `client(omit_memory=True)`. This omits only the top-level diagnostic `memory`
field at serialization; current observations and all other reported fields stay
intact. It neither changes client state nor relaxes deadlines. Memory/disclosure
tests retain default full output; see the [headless contract](../reference/headless.md#current-observations-and-memory).

`act()` waits for the matching simulation lifecycle update after admission,
including an action that starts and pauses in one publication. For paused or
finished work it also waits for the ordered permission update. A pause does not
mean completion: the caller decides whether to resume the same paid intention;
Rogue walkthroughs use `settle_rogue_action()` for that policy.
`command()` returns at request acknowledgement. Native `key()` tracks the input
acknowledgement across frames and waits until queued work has executed. Use
`wait_for_simulation=False` when testing a queue control whose acknowledgement
must return while work remains queued for a later turn.

### Scenario packages, fixtures and wizard commands

- Use ordinary validated [scenario packages](../reference/scenario-packages.md) for the initial
  setup of unit, integration, and process tests. Test-only packages live in
  `scenarios/tests/`. Keep action sequences and assertions in the test harness,
  and reference stable authored entity IDs and anchors.
- Use scripted [wizard mode](wizard.md) commands to test privileged
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

[`crates/server/tests/it/invariants.rs`](../../crates/server/tests/it/invariants.rs)
plays every package in `scenarios/` and `scenarios/tests/` for a few turns,
the package's own AI included, and checks that:

- a deliberately stale command is rejected without changing disclosed state,
  history or accepted sequence;
- a retried request returns the original journal entry without acting again,
  both before and after reopening the save;
- a restart reproduces the disclosed states, while an internal package test
  compares authoritative game, revision, receipt, archive and index state;
- a wire-resolved queued admission survives reopening and can be cancelled;
- rewinding to the start restores all initially loaded actors' observations.

The internal package tests also exercise journal and checkpoint restoration.
These generic guards complement focused admission, execution, disclosure and
storage-fault tests; they do not certify every possible feature interaction.

A new package is covered as soon as it exists. Feature tests assert what is
particular to the feature and leave these generic checks to the invariants.

The invariants run in-process for a few turns from each package's opening
state, with the source package present and no real clients. A process test
whose name mentions restart, resume, rewind or spectators is therefore not a
duplicate when it checks something they never reach:

- resuming after the source package was deleted;
- a spectator observing over real transport;
- recovery after a process kill, a transport limit or a checkpoint tail;
- client-held or presented state;
- feature state reached mid-game.

A 2026-10-14 audit classified the 129 tests that name-matching flagged. Spectator,
source-deleted, arena-replay and recovery tests took 785 of their 910 seconds,
and none of them duplicates the invariants. The remaining 63 feature-state tests
averaged 1.4 seconds each. Trimming them would save about a minute of parallel
worker time while risking feature coverage, so none were removed.

### Test organization and names

- Name a test for the behavior it proves, as a sentence:
  `a_rejected_door_action_changes_nothing`, not `test_doors_2`. Rust tests
  don't take a `test_` prefix; Python test methods need it, followed by the
  sentence.
- Keep one behavior per test. Split a test when its failure would leave the
  reader guessing which of several behaviors broke; shared setup goes in a
  helper, not in one long test.
- Rust integration tests share helpers through each binary's support module
  (for the server, [`support.rs`](../../crates/server/tests/it/support.rs):
  `act`, `play`, `run_ai_turns`, `wizard`, `package`, `load`). Add a helper
  there rather than another private copy in a test file.
- Put a test in the file for the behavior's own subject. A file of mixed
  subjects is a sign to split it.

## Bugs and regressions

When you find or fix a bug:

1. Write a test that reproduces it and confirm that it **fails at the intended
   behavior assertion**. Validate fixture preconditions and include a positive
   control when rejection or unchanged state could otherwise pass vacuously.
   A fixture-load, startup or setup-budget failure remains a setup failure, not
   a first-red reproduction of later behavior. Keep the failing output locally
   as evidence.
2. Fix the bug and confirm the test passes.
3. Add the test at the lowest layer that reliably reproduces the bug. For a
   user-visible bug, verify the affected acceptance path and extend its process
   witness whenever the regression or changed boundary is not already covered
   by an explicitly mapped witness. Follow [contract ownership](#contract-ownership).
4. If a performance investigation exposes a correctness bug, fix it with a
   regression test in the same way (for example, an invalid checkpoint encoding
   found during long-history profiling gets a forced-checkpoint restart test).
5. When a bug can't be fixed yet, record it with its reproduction in the
   [roadmap](../status.md) or feature guide. Don't add an ignored test.

## Independent 3D client

`crates/client-3d` is an optional, independently maintained workspace. The core
verifier and dependency graph deliberately exclude it. Changes to core gameplay,
protocol or other clients do not run 3D checks or require a simultaneous 3D update.
This is a maintenance boundary, not a waiver of contract coverage for changes
to the 3D client itself.

When maintaining that client, run both profiles explicitly from the repository
root:

```sh
python crates/client-3d/scripts/verify.py --profile debug
python crates/client-3d/scripts/verify.py --profile release
```

The verifier owns one target directory and records source/toolchain fingerprints,
commands and exit codes in its ignored local evidence directory, whose location
it prints. It checks tooling, the authored
showcase, formatting, lint, architecture, renderer/model contracts and actual
native process acceptance using matching backend binaries. Source changes during
a run invalidate its receipt. Its dedicated workflow runs Windows/Linux debug
and release jobs only for its own subtree, its workflow, or a manual dispatch.
Core checks remain required when a change also touches core files.

Native cases open the actual Bevy window, capture GPU frames and deliver OS input.
They cover native movement, door toggling, fetching to a disclosed item cell,
selected-target combat and its motion, neutral model fallback, inventory action
durability, private-launch save/process ownership and surfaced connection/save
failure. Shared appearance and corpse persistence also have backend process
contracts. These tests use the
same display requirements and failure reporting rules as other graphical clients.
See the [3D guide](../../crates/client-3d/README.md) and
[client boundary](../architecture/clients.md#independently-maintained-3d-prototype).

## Cross-platform requirements

- **CI:** [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) runs the
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
  A missing display is a **test failure** by default. There is no automatic
  skip or headless-success fallback. An explicitly authorized local sandbox
  exclusion is recorded as **waived, not run**, as described below; CI rejects it.
- **Local Windows runs:** native mouse and keyboard tests need interactive
  desktop access. If a sandbox denies an OS call (for example `SetCursorPos`),
  that's an environment failure: rerun with desktop access and report it. Don't
  record it as a pass or skip it silently.
- **Platform-specific code** (native windows, file durability, paths) needs
  tests that run on both platforms, or a documented reason why one platform
  can't exercise it.

## Running the checks

**The tiers change when checks run, never the coverage obligations.** TDD,
[contract ownership](#contract-ownership), feature acceptance and regression
coverage still apply. A tier can only run tests that exist; missing coverage
leaves a change incomplete, however green the tiers look. Tiered runs save time
while the complete suite remains required in CI.

[`scripts/verify.py`](../../scripts/verify.py) runs these checks in tiers. It
writes each step's log to the gitignored local directory, checks exit codes, and prints a compact
table showing which steps passed, failed, or didn't run, with their durations.
Use focused checks during development and broad tiers at stable checkpoints.
The publication gates remain required; a higher tier can cover a lower tier
without a duplicate run on unchanged inputs:

| Tier | Required | What it runs | Why it matters |
| --- | --- | --- | --- |
| Focused checks | During edits and at completed behavior checkpoints | Failing behavior/regression and affected unit tests during edits; affected integration, Service contracts and actual-process scenarios when the behavior is complete, repeated after changes affecting it | Establish a failing reproduction first and give fast feedback while retaining complete coverage before merge |
| `quick` | At a stable, cohesive checkpoint, unless a required higher tier covers it | Formatting, clippy and debug tests for the affected packages, the dependency check, an explicit scenario-compiler build, the Python tool tests, and the process tests the change touches, plus the smoke set when the change can reach every process test | Catch broader regressions after focused checks and architectural review, before moving to another checkpoint |
| `push` (default) | **Before every push**, unless a successful full run covers the same unchanged inputs and configuration | Everything in `quick`, plus workspace-wide clippy and warning-free API docs | Catches regressions in what the change can reach, and the lint and doc failures CI would reject, in minutes rather than an hour. CI runs the complete suites before merge |
| `full` | Required when CI cannot run or on request; otherwise optional | Complete platform suite in debug and release; separate tooling/dependency jobs are not included | A local full run covers the local push gate and can investigate either profile; both-platform CI is still required before merging |
| CI | **Required before every merge** | The full matrix on Windows and Linux, plus tooling/dependency checks; verified prose-only changes use the documentation path below | Full CI proves both platforms and profiles; a documentation-only result makes no such claim |

### Prose-only documentation changes

The shared predicate in [`scripts/ci_scope.py`](../../scripts/ci_scope.py) allows
only root `README.md`, `AGENTS.md`, `CONTRIBUTING.md` and Markdown below `docs/`.
It excludes `docs/reference/protocol.md`: its wire examples are inputs to real
Rust decoder tests. Code, tests, scripts, workflows, configuration, dependencies,
assets, executable files, symlinks and unknown paths require the normal full path.
Windows-reserved or ambiguous names also select full: prose paths use simple
lowercase components without spaces or trailing dots, except exact `README.md` files.
Add any new executable documentation input to the shared exclusion map when introduced.
The classifier itself and workflow changes therefore run full CI.

Before a local documentation-only push, inspect the entire proposed change,
including staged, unstaged and untracked files and both sides of renames, against
that predicate. If every change is ordinary prose, run these checks instead of
the Rust-building quick/push/full tiers:

```sh
python -m unittest discover -s scripts -p test_documentation.py -v
ruff check scripts
actionlint
```

CI checks the exact checkout and complete Git history. PR classification covers
both the whole branch from its merge base and the actual synthetic merge delta;
push classification covers the complete before/after range. Renames are compared
as deletion plus addition, so renaming code into Markdown cannot hide it. Empty,
missing, shallow, malformed, oversized or ambiguous evidence selects full CI.
Manual and weekly scheduled runs always remain full.

The documentation/link suite and tooling lint still gate prose-only CI. Only a
successful scope/documentation job may intentionally skip dependency and full
matrix jobs. The existing required `Rust (windows-latest)` and
`Rust (ubuntu-latest)` aggregates validate the selected path: they require those
intentional skips plus successful documentation/tooling checks, or the full
matrix for every other change. Unexpected skips, cancellation and failures do
not become a successful documentation gate. Dependency jobs retain their existing
full-run advisory/policy rules. A prose-only pass is not full-matrix evidence and
cannot qualify later application changes or exact-tree full-run reuse.

### Full CI partitions

CI runs four independent jobs on each platform, selected with
`full --ci-profile debug|release --ci-partition rust|python`:

| Profile | Partition | Complete coverage |
| --- | --- | --- |
| Debug | Rust | Formatting, workspace all-target Clippy, the isolated simulation fixture gate, architecture checks, warning-free rustdoc, workspace Rust tests including doctests, and production Service contracts via `service_contract_ci.py` |
| Debug | Python/native | Every `test_*.py` suite, including tooling and actual-process/native acceptance |
| Release | Rust | Release workspace Rust tests including doctests, and production Service contracts via `service_contract_ci.py` |
| Release | Python/native | Every `test_*process.py` suite, including native acceptance |

`--ci-partition` requires both `full` and an explicit `--ci-profile`. Each
partition selects complete coverage without consulting Git merge history.
Stages have explicit owners; an unclassified new stage rejects partition
planning rather than silently omitting coverage. Every command and environment
in a complete profile runs in exactly one partition. Python partitions also
explicitly build all workspace application/compiler binaries for their profile
before discovery. The debug partition retains the dedicated scenario-compiler
build. These prerequisite builds intentionally overlap the process harness's
own build check; no test command is replaced or skipped.

Service contracts belong to the Rust partitions: their wrapper uses the exact
host executable emitted by the successful Cargo test command. Python partitions'
`test_*.py` discovery does not select this suite. Local quick/push runs use the
same wrapper when their selected packages include `tor-server`.

Without `--ci-partition`, each `--ci-profile` selects its complete profile. The
ordered debug/release union is the local `full` plan. A CI run
does not certify a local run, and one partition never satisfies the full or
merge gate. Python/native jobs retain the pinned Python interpreter and native
Windows or Linux Xvfb display in both profiles. Rust-only jobs need no display,
but retain the Linux libraries required to compile and link the ASCII crate.
Profile caches remain separate. Artifacts and verification log directories
identify their profile and partition. The required `Rust (windows-latest)` and
`Rust (ubuntu-latest)` checks fail unless every profile/partition on both
platforms succeeds, including when any constituent is cancelled or skipped,
except for the explicit prose-only path above.

Whole-job budgets are 60 minutes for Rust and 40 minutes for Python/native.
With parallel workers and the optimized debug profile,
[measured CI evidence](https://github.com/KCall360/thresholds-of-ruin/actions/runs/38620000651)
puts the slowest Python/native job (Windows release) at 23.0 minutes and Windows
debug at 22.3 minutes, each including setup and builds without a warm dependency
cache. The 40-minute budget leaves over 70% headroom for cold caches. The
slowest Rust job (Windows release) measured 31.0 minutes. Scheduling and
whole-job budgets do not change individual test deadlines. Cancelled, skipped
or incomplete jobs never satisfy the merge gate, regardless of partially passing
results. `verify.py` does not run actionlint, ruff or cargo-deny; these remain
separate required CI jobs, with the advisory-only PR exception documented below.

The Python authoring tools call the authoritative Rust scenario compiler.
Every debug verification plan explicitly builds `tor-scenario` before Python
tool tests and selects that binary, including when only documentation changed.
The adapter never silently builds a binary or falls back to a Python resolver.

### Development loop

Before expensive executable linking, run formatting and an affected-target
compilation/lint preflight, such as `cargo clippy -p tor-server --all-targets --locked -- -D warnings`.
Use `cargo check` when compilation alone is the useful diagnostic; do not repeat
already-qualified unchanged preflights merely to run both. A successful check
neither updates runnable executables nor replaces a behavioral test. A Rust
test-name filter reduces test execution, not compilation/linking of its test binary.


#### Fast local Rust feedback

Use [`scripts/feedback.py`](../../scripts/feedback.py) for repeated edits to one
Rust target. It explicitly enables incremental compilation in its child process,
even when the shell setup exports `CARGO_INCREMENTAL=0`, and uses a separate
gitignored target directory. It keeps the manifest's existing optimization
settings; it does not add a profile or change CI. The default is two compiler
jobs, with debug information disabled as in the verifier.

```sh
python scripts/feedback.py -p tor-simulation --test it --check
python scripts/feedback.py -p tor-simulation --test it --filter health::
python scripts/feedback.py -p tor-simulation --lib --filter health::tests::
```

Choose exactly one package and `--lib`, `--test NAME`, or `--bin NAME`. Omitting
`--filter` runs that target's tests; `--exact` selects one complete test name.
Use `--features NAME` (repeatable) and `--no-default-features` when the selected
target requires an explicit feature configuration; these are recorded in the receipt.
A filter still compiles the whole target. `--check` is compilation-only and
cannot be combined with a test filter. Do the affected preflight before expensive
linking; do not repeat an already-qualified unchanged preflight just for ceremony.
`--no-incremental` is an explicit diagnostic comparison override, not a gate.
The helper leaves its parent's environment and normal target directory alone.

This is **local feedback, not quick/push/full or CI qualification**. CI invocation
is rejected. Empty test matches fail rather than masquerade as behavioral success.
The printed local log directory contains receipts for the selected command, toolchain,
configuration, source hashes, executable hashes and observed test counts. Input
changes invalidate the result. The helper uses the verifier's existing ownership
lock; keep each target directory single-owner and do not change checked inputs
while it is running. This lock is per worktree; coordinate ownership explicitly
across worktrees. Retain the tested checkout: receipts contain hashes, not a
standalone copy of untracked files. These focused receipts do not widen test coverage or replace
an optimized/release/platform gate.

For Python-only Service fixture/assertion edits, use a qualified unchanged host
as described in [Service iteration](service-contracts.md#iterate-against-a-qualified-host).
Rust implementation, protocol, host or feature changes require fresh affected
build evidence.

A bounded Linux pilot on `dc28b060` (Rust 1.99.0, two jobs, workspace opt1,
dependencies opt2, debug information off) compared the same simulation `it`
health selection with incremental off/on. A production edit changed the living
health subtraction to checked subtraction with an invariant assertion; a test
edit added a wide-injury healing boundary case. Each edit was then reversed.
Source contents matched between configurations, and all six/seven selected tests
passed, with 779 others filtered out. The hot edit-to-result observations were:

| Edit | Incremental off | Incremental on |
| --- | --- | --- |
| Production change / reversal | 121.73 / 123.46 seconds | 14.48 / 13.34 seconds |
| Test change / reversal | 76.19 / 78.01 seconds | 2.97 / 3.08 seconds |

This is one target and two transitions per edit kind, not a workspace-wide speedup
claim. Initial executable construction took 123.36 seconds; warming the incremental
variant took 135.86 seconds with external dependencies already built. Those are
not matched cold-build measurements. The combined experimental target grew from
about 157 MiB to 1.19 GiB. Full qualification, other targets, Windows and release
costs were not measured by this pilot. Keep qualification at coherent checkpoints,
and budget for both the incremental cache and the later qualification build.

#### Broader checkpoints

Batch a cohesive change through focused
checks and architectural review before starting a broad tier. Run the new
regression against the failing implementation first, apply the fix, then run
its affected unit tests. Consolidate already-known source fixes before the next
build, then build each required target/profile/configuration once per coherent
checkpoint and reuse those qualified binaries for the selected checks. For a bounded
no-run build, derive explicit library, binary, integration and test-enabled example
targets from Cargo metadata: the default `cargo test --no-run` also builds non-test
examples. Run affected
integration, Service contract and process scenarios when the behavior is complete;
repeat them when later changes affect it. Expand the focused set
when a failure or changed boundary warrants it. Do not run quick, push and full
back to back merely because all three commands exist. Run one `push` gate per
stable PR checkpoint; a successful unchanged full run also covers that gate.
Let CI run the complete debug and release suites before merge, including
for protocol, ruleset, save-format, persistence, storage, toolchain and dependency
changes. These boundaries still require complete coverage in both profiles.

Keep the intended package/feature configuration consistent within a qualification
checkpoint. Cargo workspace feature unification can produce different dependency
graphs and test binaries from a scoped package build; scoped binaries are not
workspace-equivalent evidence. This does not require full-workspace compilation
for each focused first-red test.

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
python -m unittest discover -s scripts -p test_intention_process.py -v
```

The discovery form above makes the shared process harness importable. Keep command output and exit codes under the gitignored local
log directory. Shared simulation, protocol or persistence changes need broader
coverage at the stable checkpoint even if their focused regression is small.
Do not overlap builds, suites or performance measurements on the development
host. Keep native graphical tests serialized.

An explicit maintainer-approved local adjustment applies only to its stated task
and scope. Record the exact tested, waived and CI-only checks and the authorization;
never generalize it into a new default tier or waiver. Use the existing
[local graphical waiver mechanism](#focused-iteration-and-local-mouse-exception)
when applicable. Otherwise the documented push/full gates remain unchanged, and
complete exact-head Windows/Linux debug/release CI is required before merge
outside the prose-only documentation path.

### Failure diagnostics and timing evidence

The verifier invokes `scripts/test_runner.py`, which preserves ordinary unittest
selection, assertions, failures and exit status. Each Python stage writes
append-only JSONL timing records beside its text log; reruns use separate files.
The same `TOR_TEST_TIMINGS` stream flushes a bounded `diagnostic` record as soon
as a failure or error is reported, before teardown and artifact retention. It
includes the test identity, exception type and traceback tail, with explicit
truncation when needed. Its `seconds` is elapsed time since the runner created
the result, including errors before a test starts. These records are separate
from the single final test outcome and do not change failure, skip or waiver
accounting.
Test durations include per-test setup and cleanup. Suite duration additionally
includes module/class setup; errors before a test starts appear in suite totals,
not as invented test executions. Shared binary builds have their own records. These
measurements are diagnostic evidence, never wall-clock pass/fail thresholds or a
reason to remove cases. The verifier also records machine-readable stage results:
`gate_passed` includes only explicitly authorized gate waivers, while
`clean_passed` requires every planned stage to pass without a waiver. Waived and
not-run stages are listed separately, along with the tier, profile and scope.
An interrupted method is recorded as `interrupted`, never `passed`; an interrupted
suite has no successful suite result and cannot be reused as a completed gate.
Report measured Cargo compile/link elapsed time separately from test execution.
When only the aggregate Cargo interval is measured, label it compile/link; do not
invent separate compilation and linker timings. State any included setup or
cleanup, and distinguish summed test durations from elapsed suite wall time.

Failed actual-process tests retain their synthetic save, scenarios, captures and
process diagnostics: command/profile, PID/exit state, full stdout, separate stderr
and startup-stage JSONL. Successful synthetic directories are removed. Large
text logs may be losslessly compressed; saves are never truncated. When copying
to the artifact destination, large logs are compressed directly from the source
in bounded chunks, without another raw copy on disk. Saves, sidecars and other
files retain their bytes and metadata. The original remains available until the
complete staged directory is published; a failed transfer removes its staging
directory and leaves the source evidence in place.
Repeated earned-route attempt snapshots use lossless compact JSON. Every full
snapshot, checkpoint, byte limit, flush/fsync and atomic replacement remains;
only whitespace changes, not report fields or qualification evidence.
During verification, these artifacts belong under that run's log directory so CI
uploads them with the logs (`TOR_TEST_ARTIFACTS` supplies the destination). Direct
runs print their retained directory. Ordinary process waits retain their default
15-second deadline and enforce elapsed time even with queued output or a slow
predicate. A bounded diagnostic tail points to the complete output. Startup
breadcrumbs identify scenario selection, journal locking/opening, replay,
simulation creation and listener readiness. A timeout is still a failure: a
missing or slow breadcrumb does not establish a root cause by itself.

Native input acknowledgement is not necessarily completed gameplay: ordinary
ASCII UI checks may intentionally stop when an intention starts. Tests that send
sequential gameplay commands use `ascii_input_completion(..., wait_for_ready=True)`
to wait for readiness after active preparation, queued/started intentions and
recovery. Paused work and death remain observable. Keep the enclosing action and
wall-time bounds; do not cover a readiness race by adding unbounded input retries.

Use `native_press(window, native_keys(window), name, reported)` for sequential
genuine key presses. It waits for the presented input acknowledgement, posts
key-up and waits for that press's fresh native-release diagnostic within the
same deadline. A key-up message alone is insufficient: minifb updates held-key
duration before dispatching Windows/X11 events, so the released level must span
another native pump before a repeated key can reliably produce a new edge.
The helper returns the latest actual presented frame observed through release,
preserving simulation/readiness progress that arrives during the wait. Its
default still permits started work; use `wait_for_ready=True` for sequential
gameplay. Cleanup attempts key release without masking an earlier failure.
Raw `native_keys()` remains available for held modifiers and specialized input
tests. Its portable `PageUp`/`PageDown` names map to Windows extended navigation
keys and X11's `Prior`/`Next` keysyms. These diagnostics do not inject input,
redraw idle frames or replace the genuine native acceptance tests.

Opt-in save-worker barriers make responsiveness tests prove that a writer is
held before the foreground request completes. These hooks are inert without the
explicit test environment and have a 30-second watchdog with observed entry and
exit. Mixed headless/native test modules use separate classes so a focused
headless check does not require a display. The barriers do not change the
normal save/durability contract. Preserve the failing save before cleanup; do not
issue an extra save after a test fails merely to shut the process down.

When a test compares images or asserts that nothing changed, include a positive
control proving the relevant content/path was exercised. An absent frame or
expired transport wait is not evidence of rejection. Keep resource bounds,
version rejection and independent reference oracles when consolidating setup.

### Verification evidence and reuse

A successful full run covers all checks in the local push gate. It can satisfy
that gate without an additional push-tier run only when the tested inputs,
toolchain, build profiles and test configuration are unchanged. Record the
checkout commit, uncommitted changes, relevant environment/configuration,
commands, profiles, exit codes and log location, plus the binary paths and content
hashes for executable-dependent evidence. Reuse requires identical relevant
source, fixtures, configuration, toolchain and qualified binaries; an unchanged
binary alone cannot qualify changed source. A commit that only records exactly
the tested file contents does not invalidate the evidence.

Give each live worktree its own `CARGO_TARGET_DIR` in a private build directory.
Assign one exclusive owner to each target directory. Hand off the source snapshot,
active command/session and target explicitly; do not start another build or edit
relevant source, fixtures or configuration until the owner confirms completion
of the current build/test checkpoint. Other workers can review or prepare patches
outside the checked inputs meanwhile. Do not duplicate full target builds across
workers, including in separate target directories, for the same checkpoint.

Determine run status from the owned process session and saved completion/exit-code
receipts. A process list from another shell or container may be isolated; absence
from `ps` proves neither completion nor failure and does not justify a replacement
build. If the owned status is unavailable, report it as unknown. Filtered regression
runs must verify the intended test names and a nonzero matching count; an exit code
of zero with no selected tests is not verification evidence.

Do not infer validity from a timestamp, a previous green result or a matching
branch name. Source, tests, fixtures, scenario packages, generated certificates,
lockfiles, build configuration or toolchain changes invalidate affected evidence;
run the required gate again after changes to its inputs. Failed, incomplete,
waived or targeted runs do not count as a successful full run. Report waivers
separately; they do not remove ordinary CI coverage.

A superseded CI run may be canceled, but preserve its partial, failed, canceled
and not-run results accurately; it is not a qualification pass. Cancellation
must not obstruct the newer run. The required gate selected for the final head
must still pass.

At the same stable checkpoint, a narrow correction does not require repeating
checks whose complete source, test, fixture, dependency, toolchain and configuration
inputs remain unchanged. Record the original immutable state, the corrected state,
content hashes and the dependency-based affected set. Rerun the changed behavior
and its affected checks, and complete every required target the earlier run did
not reach. Preserve the original failed or interrupted result unchanged. A separate
coverage ledger may reconcile completed, input-qualified checks with the fresh
results; it must identify every required target and explicitly retain any local
waiver. It is not a claim that the original gate passed. Failed checks, newly added
tests, uncertain dependency boundaries and missing targets cannot be covered by
reuse. Broaden verification whenever the changed boundary requires it. Full
exact-head Windows/Linux debug and release CI remains mandatory before merge
outside the prose-only documentation path.

This permits rapid focused iteration without automatically rerunning a broad
local gate after each narrow commit. Run the broad gate at meaningful integration
checkpoints, then close any corrections with the smallest complete evidence set.

Outside the prose-only documentation path, the final commit must still pass the
complete Windows/Linux CI matrix, in debug and release. Evidence reuse does not
remove, ignore or permanently exclude tests.
Separate test replacements require the reviewed parity evidence described in
[contract ownership](#contract-ownership). Changes to harness parallelism, timing
or test selection need their own reviewed implementation and tests (`test_parallel_tests.py`,
`test_verify.py`); this policy does not silently change the runner or CI.

When a feature is implemented or a bug is fixed, add its tests to the suite
in the same change, at the layers described in
[required test layers](#required-test-layers) and
[bugs and regressions](#bugs-and-regressions):

- **New feature:** add or extend tests at the owning production boundaries,
  preserving externally observable acceptance behavior and unique real-application
  guarantees under [contract ownership](#contract-ownership).
- **Bug fix:** a regression test at the lowest layer that reproduces it,
  confirmed to fail before the fix, plus the affected acceptance evidence
  required by [bugs and regressions](#bugs-and-regressions).
- **Latency-sensitive behavior:** extend the performance workloads and
  contracts (see [performance testing](#performance-testing)).

The debug plan also invokes the simulation fixture contract in an isolated package
command, so another workspace member cannot hide missing fixture dependencies.
The full workspace still runs the fixture assertion in both profiles.

Simulation's internal fixture helper is private and `cfg(test)`-only. External
integration fixtures parse the bundled TOML with a dev dependency and call the
public production `Ruleset::validate` API. In particular, `tor-test-support` must
not enable a simulation fixture feature: that previously changed the production
simulation library's feature identity when moving from isolated simulation tests
to workspace tests. Keeping parsing in the test binaries removes that source of
feature churn without adding crate edges or bypassing validation. It does not
promise identical artifacts across every command (other dependencies and profiles
still matter), and no numerical build-time saving is claimed for this change.
The manifest boundary has a regression in `scripts/test_check_architecture.py`;
`simulation_slice` checks the local fixture document and invalid-bound rejection.

Put new Rust integration tests in the crate's single test binary
(`crates/<crate>/tests/it/`) as a module listed in `tests/it/main.rs`. Put new
process tests in `scripts/test_*_process.py`, where every tier finds them
automatically. A process test for a new client binary also needs an entry in
`verify.py`'s `CLIENT_BINARIES`, and `scripts/test_verify.py` must cover it.

Use the [pinned Python environment](setup.md#python-test-environment)
for both the verifier and direct Python tests. With it activated:

```sh
python scripts/verify.py quick
python scripts/verify.py            # push tier
python scripts/verify.py full --rerun-failed
```

The local default comparison base is `origin/main`. Fetch it before verification,
or supply `--base` with an available reviewed baseline. CI profile options are
partition selectors, not a local shortcut around missing setup.

JSON fixtures and other non-Python inputs under `scripts/` conservatively select
the process suite. The protocol reference is an explicit input to the Rust real-
decoder documentation tests, so changing it selects `tor-protocol` and its reverse
dependents even though ordinary prose changes are inert for Rust selection.

The affected packages are the changed crates plus every workspace crate that
depends on them. Shared inputs (`Cargo.lock`, toolchain, scenarios) and any
path the script doesn't recognize select everything, so a tier can run more
than it needs but never less. In `quick`, a change to a client, or to a
process-test module or helper script, runs the process tests that can reach it.
A change that can reach every process test (a runtime crate, scenarios, shared
script data or the test infrastructure) runs those plus the smoke set,
`SMOKE_PROCESS_TESTS` in `verify.py`: fast suites that start every client and
cover movement, items, doors, intentions, checkpoints and package loading.
`push` selects process tests the same way; `full` and full CI always run every
process test. The local tiers are fast filters, not the complete gate: CI on
both platforms is, and nothing merges without it.

### Parallel test execution

The Python steps run in parallel through `scripts/test_runner.py --workers N`
([`parallel_tests.py`](../../scripts/parallel_tests.py)). The verifier picks
`N` from the CPU count and free memory (`--test-workers` overrides it; `1`
runs serially). Parallelism changes when tests run, never which tests run or
how they are counted:

- Each test runs once in a fresh worker process. A class or module that
  defines its own fixtures (`setUpClass`, `setUpModule`) runs as one batch;
  other tests are scheduled individually, and short ones are packed together.
- Classes with `graphical = True` run one batch at a time in their own lane,
  alongside the headless workers, so native windows never compete for input.
- Test modules the change touched start first, then the longest tests, using
  [`test_durations.json`](../../scripts/test_durations.json). Durations only
  order the work; they never select tests or set deadlines. Refresh them from a
  run's timing records with `python scripts/verify.py ... --record-durations`
  or `python scripts/parallel_tests.py <timings.jsonl>...`. The verifier's
  summary also lists the five slowest tests of each Python step. Supply the
  Windows/Linux debug/release timing files together: each test's slowest
  passed observation in that refresh wins, independent of file order. An
  observed test replaces its previous hint; absent tests retain their hints.
  Refresh after adding expensive acceptance tests rather than leaving them at
  the three-second unknown-process estimate. Before publishing, check that
  the longest new methods become standalone early batches; retain the exact
  selected test IDs, fixture grouping and graphical lane isolation.

  The [PR 122 measurement](https://github.com/KCall360/thresholds-of-ruin/actions/runs/38718998419)
  illustrates why: an unrecorded 845-second Windows earned-route method was
  packed into a late 75-method batch. Updating ordering hints does not remove
  that acceptance or reduce its deadline. Its actual CI wall-time effect must
  be measured; summed test durations are not elapsed runner time.
- The output keeps unittest's `FAIL:`/`ERROR:` lines and ends with one
  `Ran`/`OK`/`FAILED` summary whose counts match a serial run. A run succeeds
  only when every selected test has exactly one result (or, with fail-fast, is
  reported as not started). A worker that cannot launch, exits without
  recording a test, leaves truncated or malformed records, reports a duplicate
  or foreign result, lacks exactly one suite summary consistent with its
  records and exit status, or exits with failure although its results passed fails
  the run with a `BATCH ERROR` line, and any unrecorded test fails visibly with
  its own `ERROR:` line. Timing records from
  every worker are merged into one file that ends with one suite record, so
  waiver accounting and `--rerun-failed` work unchanged.

Process tests must therefore not depend on each other's order or share mutable
state outside their own directory; the harness already gives each test its own
directory, save and server port. `verify.py --fail-fast` stops starting tests
after the first failure for quicker local feedback; such a run never satisfies a
gate unless everything ran and passed.

The script chooses `CARGO_BUILD_JOBS` from free memory, refuses to start while
another cargo or rustc process is running, and never overlaps its own steps.
`--rerun-failed` reruns failed Python tests once. The step stays failed, and
the summary says whether the rerun passed, so an intermittent failure is
visible instead of hidden.

A lower tier never replaces a higher one when that one is required, and no tier
replaces CI. Outside the prose-only path, the full Windows and Linux matrix below must pass on the final
commit before merging. The following individual commands are a diagnostic reference, not a replacement
for the verifier: rustdoc and release-process checks below are also required.
The `full` tier includes:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --features tor-server/service-contract-host -- -D warnings
python scripts/service_contract_ci.py --profile debug --workspace
python scripts/service_contract_ci.py --profile release --workspace
python -m unittest discover -s scripts -p "test_*.py" -v
python scripts/check_architecture.py
```

Also build the API documentation with warnings denied, as CI does:

```sh
cargo doc --workspace --no-deps --document-private-items --locked --features tor-server/service-contract-host
```

Set `RUSTDOCFLAGS=-D warnings` first (`$env:RUSTDOCFLAGS = '-D warnings'` in
PowerShell). To run the process tests against optimized binaries, set
`TOR_TEST_PROFILE=release` and run `python -m unittest discover -s scripts -p "test_*process.py" -v`.
Python is used only for development checks.

The tooling and dependency checks use tools that aren't part of the Rust
toolchain. Install the pinned versions from
[`.github/requirements-lint.txt`](../../.github/requirements-lint.txt), and
`cargo-deny` with `cargo install cargo-deny --locked`, then run:

```sh
actionlint
ruff check scripts
cargo deny check
```

What full CI runs on both platforms:

| Step | Command |
| --- | --- |
| Formatting | `cargo fmt --all --check` |
| Lint | `cargo clippy --workspace --all-targets --locked --features tor-server/service-contract-host -- -D warnings` |
| Python checks and debug process tests | `python scripts/test_runner.py discover -s scripts -p "test_*.py" -v` |
| Dependency boundaries | `python scripts/check_architecture.py` |
| API docs | `cargo doc --workspace --no-deps --document-private-items --locked --features tor-server/service-contract-host` with `RUSTDOCFLAGS=-D warnings` |
| Isolated simulation fixture | `cargo test -p tor-simulation --test it --locked fixture_contract::` |
| Rust tests and Service contracts (debug) | `python scripts/service_contract_ci.py --profile debug --workspace` |
| Rust tests and Service contracts (release) | `python scripts/service_contract_ci.py --profile release --workspace` |
| Release process tests | `TOR_TEST_PROFILE=release`, `scripts/test_runner.py` with `test_*process.py` discovery |

What full CI runs once, on Linux:

| Step | Command |
| --- | --- |
| Workflow lint | `actionlint` |
| Python lint | `ruff check scripts` (bug-focused rules in [`ruff.toml`](../../ruff.toml)) |
| Dependency advisories | `cargo deny check advisories` (advisory-only on PRs; fails on `main` and the weekly run) |
| Dependency policy | `cargo deny check bans licenses sources` (policy in [`deny.toml`](../../deny.toml)) |

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
  [background saving](../architecture/persistence.md) for the durability contract.

### Formats and fixtures

When the protocol version changes, rerun `python scripts/record_wire_samples.py`
to record `crates/protocol/tests/fixtures/wire-v<version>.json` and review its
difference: every change in it is a change to the wire format. The required-kind
manifest includes waiting messages and explicitly identifies the full
`update.observation` kind as unrecorded by the delta-only sample scenario; full
observation serialization remains covered by separate protocol tests. Changes to
this exclusion require review. Keep only current golden fixtures. Test
unsupported versions and malformed schemas by mutating current canonical
fixtures or constructing minimal invalid cases, rather than retaining snapshots
of earlier implementations. Independent fixture versions follow their own
schema, not the aggregate protocol or save version.

The project supports only the current protocol, save format, and ruleset (listed
in the [roadmap](../status.md#implemented-scope)). When a format changes,
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
[performance plan](performance.md); reproduction commands are in the
[performance harness](performance.md).

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
   [`scripts/perf_compare.py`](performance.md#before-and-after-comparisons)
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
  [performance ledger](performance.md#performance-ledger), and
  summarize the results, cases, and limitations in the relevant feature guide.
  See [publishing raw measurements](performance.md#publishing-raw-measurements).

## Focused iteration and local mouse exception

During individual edits, run failing behavior/regression tests and affected unit
tests. Run integration, Service contracts and real-process acceptance tests at
completed behavior checkpoints, repeating them after changes to the covered
behavior. Run quick at cohesive subsystem checkpoints, rather than after every
small edit. Coverage of [every changed contract](#contract-ownership) remains
required before merge; push and final-commit Windows/Linux debug/release CI gates
remain mandatory.

The standing local waiver is
`test_travel_process.TravelProcesses.test_native_underscore_and_mouse_click`. `python scripts/verify.py push --waive-local-mouse` records that
exact test as a waived
local failure if it fails, preserving its diagnostics. It never waives another
test, a build failure, or any CI failure. CI runs the unchanged test normally.

When the maintainer explicitly permits skipping impossible graphical tests in a
local sandbox, use `--waive-local-graphical MANIFEST` with `full`, the only
local tier that selects every test.
The manifest is local evidence, not a committed test filter. It must contain
exactly `reason` (a nonempty string describing the verified display blocker and
authorization) and `tests` (a list of exact unittest method IDs). Audit each
listed method's native display requirement and preserve the original failure
diagnostics. Do not list a headless method just because its class also contains
graphical methods. The flag itself does not grant permission to skip tests.

The verifier snapshots that manifest in its log directory. Only named methods
on classes declaring `graphical = True`, without `@non_graphical`, can be
excluded. Waiver admission uses the same per-method classification as the
harness display check; unknown, duplicate, already-skipped and non-graphical
IDs reject the run. Unlisted methods, including
newly added tests, still run. Entirely waived classes do not start native class
fixtures; mixed classes retain their fixtures and other methods. No original
test class or method is modified. Separate waiver markers run after the ordinary
cases, whose relative order is preserved. The runner records each excluded ID
with outcome `waived` and its reason, and the verifier
checks the exact count and IDs before accepting the local gate. Such a gate has
`clean_passed = false`, even when all non-graphical checks pass. Neither missing
display variables nor a zero Python exit status can authorize a waiver.

This option disables the verifier's automatic Xvfb wrapper for that local run.
Every non-graphical check remains required. Both the verifier and direct test
runner reject the flag when `CI` is set, and the verifier also rejects it for a
CI profile partition. The normal CI matrix runs every graphical test unchanged;
all jobs on the exact final head must pass before merging.

Paid public-plan acceptance uses `test_public_plan_process.PublicPlanProcesses`
for ordinary headless/Text payment, entitled observer spectators, immutable
history, pause/save/reconnect and rewind. Its separate
`PublicPlanNativeProcesses` class selects the authored ability through the real
ASCII menu and checks the presented historical stats row. The fixture grants the
generic capability directly; these checks do not certify the final Warrior talent
prerequisite chain. `record_wire_samples.py` records an actual paid snapshot from
the same ordinary fixture. Never hand-edit a prior recording to add the new fields.

### Optional Windows candidate measurement

The CI workflow's `measure_windows_candidate` manual-dispatch input is an
explicit comparison aid. Leave it disabled for ordinary verification. When
enabled, both Windows Python partitions first complete their unchanged full
suite, then run the source-audited candidate with the same binaries, profiles,
worker count and native desktop. Invalid audits or an incomplete baseline stop
the comparison. Missing, duplicated, failed or skipped selected tests fail it;
there are no graphical waivers or automatic retries.

The candidate retains new tests by default. Explicit deferrals carry their own
semantic justification and relevant source/fixture fingerprints in
`scripts/ci_coverage_manifest.json`; they do not assert redundant coverage.
`scripts/ci_policy.py` reports the proposed selection. Missing, unknown, deleted,
risky or stale inputs select exhaustive coverage in the ordinary policy path.
Manual measurement uses its separately named `reviewed_candidate` helper after
the full baseline; it never fabricates pull-request change evidence.

Artifacts under `shadow-debug` and `shadow-release` include the exact selected
IDs, raw timings, failure artifacts and comparison report. Compare test-step
elapsed times only. The second pass benefits from already-built binaries and
warm application/OS caches, so this ordered comparison is biased and does not
predict whole-job cost or merge latency. Debug includes pure tooling tests, as
does its full baseline; release compares process tests only. All existing full
checks remain required, and a failed optional measurement is visible as a failed
manual workflow. Broader cadence or merge-gate changes require separate measured
qualification and review.

Each full profile partition also uploads bounded before/after checkout, run,
configuration and resolved-tool observations. These explicitly identify themselves
as untrusted observations: PR-controlled code can forge them, collection is not
atomic, and a complete report is not an attestation or permission to reuse tests.
Shallow checkouts record the actual commit object's parent headers. Missing
metadata remains visible as incomplete; the producer does not collect credentials,
arbitrary environment contents or network data. Only the separately checked
[postmerge consumer](#reusing-exact-tree-postmerge-cells) may adopt a prior cell.

The Windows free-memory CIM observation uses a 15-second timed wait; other
probes retain 5 seconds. These budgets exclude the existing reader joins and
process cleanup, so they are not total-function deadlines or a promise that
every CIM query completes. The memory output cap remains 128 bytes. A timed-out
or unavailable sizing observation remains null and incomplete, with no retry or
cached sizing; missing adoption evidence still requires fresh full verification.

### Reporting possible exact-tree qualification

The optional `Qualification source` job records GitHub's evaluated source SHA in
its job name. Every checkout explicitly uses that same `github.sha`; on a PR this
is the synthetic merge checkout, not the PR head reported by the workflow-runs
API. The job executes no repository code on PR events. On main pushes,
[`scripts/ci_qualification.py`](../../scripts/ci_qualification.py) reads GitHub's
run, attempt, job, PR and Git-object APIs and reports whether a recent completed
premerge full run is a possible same-tree qualification candidate.

This metadata decision remains reporting-only: every result sets
`reporting_only: true` and `full_required: true`. A candidate alone cannot waive
verification; it supplies a source identity to the separate per-cell consumer
below. Fresh main dependency/advisory checks for non-prose changes and the weekly
exhaustive schedule remain unchanged. Prose-only runs are ineligible as full-run
qualification evidence. The report job is non-blocking and introduces no builds
or test reruns. Its API/error handling cannot turn a skipped required job green.

The bounded comparison supports only this repository's ordinary two-parent
same-repository PR merges and first-attempt CI runs. It checks the exact PR
base/head, synthetic merge parents/tree, main target, workflow identity, complete
successful job inventory, per-cell checkout/verification step outcomes and
unchanged qualification-control content and modes in the base, PR head and final
merge. Checking the head prevents a modified workflow from fabricating the source
identity and then restoring baseline controls during merge. Missing or ambiguous evidence, reruns, forks,
unsupported merge methods, changed controls, expired evidence, incomplete API
pagination and API errors report no candidate. Evidence must start within the
previous 24 hours and finish before the associated merge. Changed qualification
controls require fresh full verification. GitHub may
empty a run's PR-association array after merge; the merged-PR record, source head
and independently resolved synthetic/target parents still bind the exact tree in
that case, but do not independently identify the run's originating PR. A nonempty
conflicting association rejects the candidate. Source discovery includes
failed and pending runs, so an old green run cannot hide another result; the
report rechecks the current run, matching run inventory and main tip at the end.
Attempt and current-run endpoints can retain different update timestamps. The
report compares their identity and lifecycle fields, independently requires every
response's interval to contain all source jobs and finish before merge, and
rechecks each attempt/list snapshot against the same endpoint. A changed timestamp
within an endpoint still rejects the candidate; no tolerance or retry is applied.

The report does not trust uploaded observations or cached build products. Even
a positive comparison is metadata evidence only: it does not independently
certify clean inputs throughout execution, per-test completeness or equivalence
of resolved hosted images/toolchains. Those require the separate per-cell checks
below. No saving is established by a matching metadata report.

### Reporting verifier receipt completeness

The verifier's existing summary now includes reporting-only receipt completeness
for its own invocation. It reconciles the planned/executed stages, actual command,
working directory and allowlisted environment fingerprints. Python's original
selected IDs are recorded once in the existing timing JSONL and reconciled with
one passing outcome each and a terminal count; the summary references that file's
digest instead of copying the records. Missing, malformed, duplicate, interrupted,
skipped or waived evidence is incomplete and never overrides command exits.
Rust retains the normal successful Cargo invocation as its trusted boundary;
the summary also preserves the precise Service host artifact/result, without a
second Rust runner or test inventory. Existing before/after observations identify
the source tree, configuration, tools and installed Python distribution metadata.
Parent and per-stage environment observations are distinct. The observer also
records bounded Linux native-package database metadata and the fixed graphical
setup mode; native library bytes, volatile display state, package contents and
unlisted inputs remain outside this evidence.
This uses the same trust in reviewed CI/test code as a normal full run, not stronger
attestation. Completeness is not full-matrix qualification or permission to reuse
or skip work, and prose-only results never become full-run evidence.

### Reusing exact-tree postmerge cells

On main pushes, [`scripts/ci_adoption.py`](../../scripts/ci_adoption.py) may adopt
one of the eight fully tested premerge cells after setup and before verification.
Ordinary premerge, manual and weekly full runs execute every selected cell; the
bounded corrective-head exception below is separate. The prose-only path, matrix and required aggregate names are unchanged. An adopted
run cannot become a source: sources must be successful first-attempt PR runs
whose actual full verification steps passed. There is no adoption chain.

Each cell needs the metadata match above and an exact Rust dependency-cache hit.
The cache requirement protects cache population/performance; it is never test
qualification. The consumer separately binds a unique artifact to the API's
source run, attempt and successful job, verifies its compressed digest and byte
limits, and reads bounded ZIP members without extracting archive paths. API
credentials never follow artifact redirects to storage. It rechecks the source
run, current main tip and active target run before accepting.

The source's clean before/after observations must be stable and match the target's
tree, configuration hashes, hosted image, tool versions, Python distribution
metadata, allowlisted environment fingerprints, Linux package database and
graphical setup. Windows native dependencies remain covered by the matching
hosted image and unchanged setup contract rather than a separate package scan.
Unknown external display/runtime overrides are ineligible. Target memory-based
build/test limits must match the actual source invocation. Planned stages,
effective stage environments and digest-bound Python/Cargo/Service receipts are
revalidated using the existing verifier. Successful Cargo remains the trusted
Rust boundary; this does not introduce another runner or stronger attestation.
The observation, adoption-helper and full-verification steps use the same explicit
Bash shell on both platforms so inherited inputs such as `PATH` remain comparable.
Per-stage overrides remain distinct and exactly checked; shell differences do not
permit ignoring or normalizing an environment mismatch.

Missing, ambiguous, expired, malformed or mismatched evidence, a cache miss, and
ordinary helper errors or its 90-second timeout all fall back to normal full
verification. Cancellation stops work. The final cell assertion accepts only a
successful fresh verifier or an explicit successful adoption with the verifier
intentionally skipped. An adoption receipt identifies both source and target
runs, their links, the source artifact digest and that tests did not execute
again in this cell. Required aggregates still require successful cell outcomes.

Adoption preserves the premerge coverage of the exact tree while removing a
second execution; it also removes that extra chance to detect nondeterministic
failures. Package metadata is not package-byte provenance, and neither path
certifies every volatile machine input. Keep the existing weekly exhaustive
run. The control-changing rollout itself requires full CI; measure savings only
after a real subsequent adoption and a deliberate fallback have been observed.

### Reusing unchanged results after an expectation-only PR correction

[`scripts/ci_head_reuse.py`](../../scripts/ci_head_reuse.py) is a separate optional
PR-only proof, not a relaxation of exact-tree postmerge adoption. Initially it
supports modifications to existing study, roster and chill process-test leaves
listed in [`scripts/ci_reuse_registry.json`](../../scripts/ci_reuse_registry.json).
Every other tracked path, file mode and inventory must remain identical between
the old and new synthetic trees. Unknown paths, additions, deletions, renames,
shared helpers, controls, fixtures and action changes select full verification.

The reviewed leaf skeleton permits only bounded, side-effect-free expected-value
edits at acceptance assertions, including primitive containers, safe `len`, and
simple comprehensions over declared immutable values. Actual operands, setup
guards, control flow and actions stay fixed. Optional immutable imports and
expectation-only local bindings have explicit rules. Source must be UTF-8.
The registry pins broad consumer owners and the import-time closure; immutable
species tuple data can evolve without approving new provider code. Maintaining
that audit when introducing outside consumers is a code-review obligation.
Literal-reference guards add protection; they do not infer arbitrary dynamic
Python/Rust dependencies. Consumer-owner drift needs explicit review and a fresh
baseline, not an automatic pin refresh.

Source and target must be first attempts of the same open, same-repository PR,
with unchanged current main base and a direct parent corrective head. The proof
binds exact API synthetic commits/trees, unchanged controls, unique digest-checked
artifacts, complete terminal cells, strict resolved environments and sizing, and
source age within 24 hours. A superseded globally cancelled run may contribute
already completed cells; interrupted/cancelled cells never qualify. Old receipts
without the new failure-phase and explicit fresh-origin evidence are ineligible.

Successful whole Rust cells can be borrowed. Python requires exact exhaustive
selection and terminal accounting; every failed ID must be an ordinary method
AssertionError at an eligible old-source assertion in a changed leaf. Setup,
teardown, cleanup, worker errors, failed subtests, skips, waivers, duplicates and
missing/not-started IDs reject. Every changed module method and applicable cheap
tooling test runs freshly, with the ordinary application/compiler builds intact.
A successful final verifier proves the disjoint source/fresh coverage union and
rechecks clean checkout, live PR/head/base and source/target attempts. Probe failure
selects full verification; failed or partial fresh execution cannot become a pass.

Completed earned-route results retain their original whole-source fingerprints;
this reviewed provenance exception never rebases reports or permits cross-head
resume. Existing fingerprint/resume guards remain unchanged. Hashing overhead and
ordinary setup/build time remain, so reuse is not an exact runtime prediction.
Composite summaries identify both sources and cannot be borrowed again, including
by postmerge adoption: main conservatively runs fresh after a composite PR cell.
This can reduce correction-to-merge latency without reducing total worker compute
by the same amount. The control-changing rollout runs full, and requires a future
fresh baseline before any reuse. Measure a real qualifying correction and deliberate
fallback before claiming an end-to-end speedup; local synthetic fixtures prove the
selection/accounting contract, not hosted CI performance.

### Source-aware dependency cache trial

The pinned Rust cache action does not include this virtual workspace's root
`Cargo.toml` in its member-manifest hash. A cache created before the development
optimization settings changed was therefore still an exact hit, and its
immutable contents could not be refreshed with the new dependency variants.
Cargo rebuilt incompatible artifacts correctly; cache success was never test
qualification.

CI's shared key includes an explicit root-manifest hash, profile, partition and
the build recipe produced by [`scripts/ci_build_key.py`](../../scripts/ci_build_key.py).
The recipe uses the existing verifier plan and Service host's Cargo command
builder, preserving actual Cargo arguments and verifier environment overrides.
It normalizes only the checkout prefix and recognized Python launcher identity;
it does not hash complete workflow/verifier files. Reporting-only and logging
edits therefore need not discard dependency caches. The pinned action still adds
OS/architecture, Rust versions, compiler environment, member manifests, lockfile,
Cargo configuration and toolchain-file identities. Effective debug-information
and incremental flags remain visible before the action computes its key.

Unknown top-level commands or unavailable recipe inputs disable cache restore
and save for that job; the unchanged full verification still runs. This is a
cache-population hint, not a dependency resolver or verification certificate.
Known Python test/architecture stages are opaque: the recipe does not inspect
their bodies for hidden builders. Today's process-harness app build matches the
explicit workspace-binaries prerequisite; the debug scenario-tool prerequisite
is included too. A future hidden builder can require additional dependency
variants without changing this key. Cargo still checks artifact freshness, but
an exact cache hit will not save newly built variants, so watch for repeated
dependency compilation and update the explicit recipe when introducing builders.

Dependency-only cleanup and main-only writes remain in place; workspace outputs
are not newly retained. Profile/partition keys retain eight platform variants.
The first successful main run seeds new caches. Measure subsequent independent
fresh-checkout runs, including restore size/time, dependency recompilation and
whole-job time, before claiming savings; an initially cold run is not that
comparison. A matching cache key never qualifies tests.
