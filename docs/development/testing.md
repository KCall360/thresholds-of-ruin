# Testing and verification policy

A change is complete when its changed contracts have independent coverage and
its canonical CI assurance gate passes. This page is the authoritative policy;
[development practices](../../CONTRIBUTING.md) covers the rest of contribution.

CI distinguishes **prose-only**, **selected-contract**, and **exhaustive** results.
The [risk-based gate](#risk-based-ci-assurance) selects required owners from
complete change evidence; uncertain scope or ownership escalates rather than
silently dropping coverage. Exhaustive Windows/Linux debug/release verification
remains available and required on its full cadence and explicit risk triggers.
A selected result is not a full-matrix pass. The narrow
[prose-only documentation path](#prose-only-documentation-changes) remains unchanged.

Start with [development setup](setup.md): Rust components, the pinned
Python environment, a graphical desktop and a local Git comparison base are
required. From the repository root, `python scripts/verify.py` runs the local
pre-push gate. The tier does not substitute for the test-writing obligations below.

## Find the requirement

- [Running checks](#running-the-checks): focused/quick/push/full and CI gates
- [Risk-based CI assurance](#risk-based-ci-assurance): selection, escalation and failure feedback
- [Test objectives](test-objectives.md): independent owners and retained extended witnesses
- [CI cost baseline](ci-cost-baseline.md): measured cost and honest comparisons
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
5. **Windows and Linux are both first-class.** Windows is the primary platform.
   Selected CI retains genuine platform witnesses and affected owners; exhaustive
   cadence retains both complete platforms. Neither a Linux-only result nor a
   missing display silently qualifies a Windows/native contract.
6. **Profiles are distinct evidence.** Optimized builds can expose timing and
   ordering problems debug builds hide. Selected CI retains a release sentinel
   and affected release owners; full cadence covers both complete profiles.
   The debug (`dev`) profile keeps
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

For focused local qualification, follow the changed production dependencies as
well as newly added tests. Include existing owners of affected restore fold
counts, retained build provenance, frozen plan witnesses and complete schema
field inventories even when their test files were not initially edited. Record
the selected test IDs and the contracts they cover; a new feature's tests alone
do not cover those older cross-cutting invariants. Build a small affected-contract
owner matrix (contract, source/fixture, test ID, required boundary): include
external scenario consumers such as `crates/client-3d/scenarios/menagerie`, not
just packages beneath `scenarios/`. Shared actor mechanics need ordinary actors
without creature stats as well as creatures where valid, and cross-feature cases
such as movement with traps, conditions and paid-action recovery. Establish the
supported contract explicitly; do not silently make every actor a creature to
avoid a failing boundary. Check the CI target order too:
fail-fast can hide later owning targets. For broad build, restore or schema
changes, run the directly affected integration target as a whole when its
runtime is small, instead of repeatedly filtering out older invariants. This
does not require a full workspace matrix locally.

Choose owners by the production boundary, not the test language. Python-authored
Service contracts exercise the production Service and encoder. The current host
does not prove socket behavior, authentication setup, durable-store recovery or
actual client behavior. Retain those independent witnesses. See the bounded
[catalog assertion mapping](service-contracts.md#catalog-assertion-ownership).

Keep regression coverage permanently, rather than requiring duplicate assertions
at every layer. Moving or removing a test requires reviewed assertion parity,
demonstrated bug detection and preservation of every unique boundary witness.
Matching names, passing counts or a faster run do not establish equivalence.
Moving assertions and changing execution cadence are separate decisions. The
[risk-based gate](#risk-based-ci-assurance) explicitly defers unrelated exhaustive
execution, but never grants permission to delete unique coverage or claim that a
smaller witness proves a boundary it does not exercise.

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

Choose the completion signal for the contract being tested:

| Driver intent | Required boundary |
| --- | --- |
| UI selection, cursor movement or deliberate early/queue input | Input acknowledgement; retain the default or explicit queue-only policy |
| Witness that paid work started | Matching lifecycle start; do not silently wait through combat, pause or recovery |
| Ordinary sequential gameplay | Establish admission before the first input, then require the completed effect and the authoritative admission permission before the next |
| Finished native paid action followed by another input | `native_press(..., wait_for_ready=True, wait_for_admission=True)` |

Admission alone can be true while paid work is still started; it is not a
completion signal. `key(..., wait_for_admission=True)` is suitable for immediate
ordinary transfers, while a paid-action caller also waits for its finished
state. Native admission waiting occurs after the key's release acknowledgement
and shares the original deadline. Travel `arrived` similarly needs admission
before the next gameplay input. Do not apply these waits to intentional
rejection, spectator, paused, dead or terminal-state witnesses. Review each
caller's precondition, completion signal and negative-case intent; use scripted
frames to distinguish acknowledged effects from later permissions without
sleeps or larger deadlines. Assert readiness, payment and the actual resulting
state separately where each is part of the contract; an accepted command, fixed
tick count or assumed random outcome is not proof of the effect. Legitimate
conditions may require bounded ordinary waiting or replanning in a driver, but
preserve combat decisions, action budgets and explicit rejection witnesses.
Do not turn a negative case into a success path by adding automatic waits.

### Scenario packages, fixtures and wizard commands

Positive gameplay fixtures should use explicit typed constructors/fields when
available, so required runtime fields become compile-time maintenance work.
Keep independently authored schema positives and malformed/missing-field raw
payloads: sharing a production serializer is not an independent schema oracle.
Expected values must come from an independently authored typed or literal contract,
not the production helper, lookup or serializer being tested.
Validate and exercise a known-valid positive baseline before cloning it and
changing only the dimension a negative test intends to reject. Assert that the
mutation fails for the intended reason; an already-invalid baseline or fixture
load failure does not prove that behavioral boundary. Preserve explicit rejection
cases when extending helpers or schema fixtures.

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
   as evidence. For a critical assertion without an available pre-fix run,
   demonstrate sensitivity with a bounded, isolated mutation that breaks the
   intended contract and fails that assertion, then remove it and confirm green.
   This is assertion-sensitivity evidence, not a claimed historical first-red run.
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

`crates/client-3d` is an optional, independently maintained workspace with its
own full verifier. Core gameplay, protocol or other-client changes do not by
themselves trigger its full native/render matrix or require a simultaneous 3D
update. The risk-based core plan does add a lightweight Linux consumer lane for
affected shared protocol/client-common contracts: the actual 3D application must
compile and its model tests must pass. Conservative full plans also run this
compatibility lane. That check is not graphical or visual qualification. The maintenance boundary never waives contract coverage
for changes to the 3D client itself.

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
and release jobs for its own non-scenario subtree, its workflow, or a manual
dispatch. Scenario-only refreshes use core scenario validation instead; mixed
client edits still trigger the full client workflow. Core checks remain required
when a change also touches core files.

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
  Both required platform aggregates validate the canonical selected/full plan
  before merge. Dependency advisories remain warning-only on PRs, so a newly
  published advisory does not block unrelated work; their main/full-cadence policy
  remains strict. Fresh weekly scheduled exhaustive runs surface regressions, advisories
  and toolchain changes without waiting for an affected PR.
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
while the complete suite remains maintained and required by exhaustive cadence.

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
| `push` (default) | **Before every push**, unless a successful full run covers the same unchanged inputs and configuration | Everything in `quick`, plus workspace-wide clippy and warning-free API docs | Catches regressions in what the change can reach, and the lint and doc failures CI would reject, in minutes rather than an hour. CI independently selects its required assurance gate |
| `full` | Required when CI cannot run or on request; otherwise optional | Complete platform suite in debug and release; separate tooling/dependency jobs are not included | A local full run covers the local push gate and can investigate either profile; the canonical CI gate is still required before merging |
| CI | **Required before every merge** | Canonical selected-contract or exhaustive plan, tooling/dependency checks and both platform aggregates; verified prose-only changes use the documentation path below | Report the actual assurance mode; only an exhaustive result proves the complete matrix |

### Risk-based CI assurance

The objective is faster useful failure feedback and lower total execution cost,
not fewer assertions or a target test count. The [measured baseline](ci-cost-baseline.md)
separates compilation, setup, test work, elapsed time and runner-hours. It does not
establish a speedup for a changed workflow. The
[contract registry and objective guide](test-objectives.md) explain what each
boundary proves and which longer campaigns remain unique owners.

The workflow preserves the required `Rust (windows-latest)` and
`Rust (ubuntu-latest)` aggregate names. Their meaning is **the complete required
plan passed**, not that every possible profile ran. A missing, failed, cancelled,
waived, duplicated, stale or incomplete required result cannot become green.

| Assurance | Selection and meaning |
| --- | --- |
| Prose-only | The unchanged complete-diff prose classifier plus documentation/tooling checks; no application qualification |
| Selected-contract | Supported PR and main-push changes with complete Git scope and reviewed ownership; Linux/Windows tooling, plus explicit application/Rust lanes when required, real platform witnesses, release sentinel/affected owners, and any required downstream 3D compatibility |
| Exhaustive | Full profile/partition matrix on both platforms for weekly scheduled, on-demand, release and required risk escalation; all existing full owners remain discoverable |

Pure tooling changes use Linux and Windows tooling closure without compiling
unrelated Rust applications. Windows no-build tooling uses digest-bound Python
startup interception plus a required actual extensionless `cargo -V` probe;
this covers reviewed Python launch paths, not an OS sandbox or arbitrary native
or shell descendants. Process/helper changes add an explicit application
producer and real boundary owners. Semantic Rust/content changes add Linux
deterministic workspace Rust/Service coverage and affected debug/release process
boundaries. The registry's explicit `requires_apps` and `requires_rust` decisions
control these choices; an empty test list is not permission to skip a build or
claim a missing lane passed. Unknown dependencies still escalate.

The selected plan binds policy version, exact checked-out source/tree, current
independent discovery, changed paths, contract reasons and required lanes. PR
scope covers both branch changes from the merge base and the synthetic-merge
delta; supported main pushes cover the complete before/after range. A caller
cannot substitute an arbitrary narrow file list. Missing/shallow/ambiguous Git
history, unsupported events, unknown paths/owners/dependencies, invalid discovery
or missing fixed witnesses select full coverage or block qualification. New owner
registrations require review, not an automatic allowlist refresh to turn CI green.

Changes to verification controls, ownership/planning/receipt code, workflow,
Cargo/toolchain/features or build configuration force exhaustive execution. Other
reviewed domains expand to their explicit broad/extended owners: topology and
paid discovery retain full ordinary routes; persisted authority/growth retain
replay and earned-route owners; transport retains real pressure/recovery owners;
optimized/performance behavior expands release coverage. Consult the registry
rather than guessing from file names. A reviewer can require more coverage for a
new cross-feature contract or uncertain semantic dependency. No override may
narrow a mandatory plan. This testing-regime rollout itself requires full CI.

Selected execution is a deliberate tradeoff: an unrelated regression outside its
mapped contracts can be found later by the exhaustive cadence. Fixed smoke is
not equivalent to the long earned or dark-maze routes, and independent discovery
proves inventory accounting, not the completeness of assertions. Preserve all
full tests until a separately reviewed replacement demonstrates assertion parity,
intended bug detection and every unique process/native/durability boundary.

A release candidate requires exhaustive qualification; do not infer it from a
selected main run. Manual full verification remains available before publishing
a release, and release-event CI also uses full mode.

All active selected and exhaustive CI lanes execute fresh tests. The retired
pass-adoption and corrective-head machinery is removed, not kept as a dormant
alternate gate; Git history retains the experiment. A binary artifact is reusable
code, not test evidence. Qualified ordinary
application builds may be shared between compatible consumers; Service-feature,
test, benchmark and ordinary application recipes are not interchangeable. Bind
source, toolchain/target/profile/features, build recipe and executable bytes before
launch. Missing or mismatched inputs must not silently select an older executable.

#### Explain a local selection

To inspect a proposed affected plan without building or running tests:

```sh
python scripts/ci_test_plan.py --base origin/main
```

Use an available, reviewed comparison ref. The explanation includes branch
changes from its merge base and tracked staged/unstaged changes; untracked source,
unavailable discovery or uncertain ownership conservatively produces full mode.
Inspect the reasons, contract IDs and exact lane test IDs rather than only the
mode. The output is explicitly `local-proposed-plan-not-qualification`: its HEAD
identity does not certify dirty inputs, it runs no tests, cannot set CI job
outputs, and never waives local `quick`/`push` or a required full gate.

The optional `--output` chooses a local destination; the default stays in the
gitignored verification area. In CI, `python scripts/ci_test_plan.py` derives
scope from the trusted event and checkout rather than that local base argument.
[`ci_selected.py`](../../scripts/ci_selected.py) executes each required lane and
reconciles fresh receipts against a newly computed canonical plan. Use the
workflow's exact lane commands and environment for qualification; a hand-edited
JSON plan or a local explanation is not an accepted substitute.

#### Qualified application artifacts

[`ci_binary_artifact.py`](../../scripts/ci_binary_artifact.py) transports ordinary
application binaries within one reviewed workflow/run/attempt. Its manifest and
minimal executable archive contain the server, scenario/arena tools, and headless,
Text and ASCII clients. They do not bundle a Service host, Cargo cache, benchmark
examples, system runtime libraries, or proof that tests passed.

The producer uses a fixed ordinary-application recipe. Validation binds the clean
source SHA/tree, Rust/Cargo/native target, profile and fixed flags/features,
runner OS/architecture/image family and workflow identity; it checks exact archive
membership, regular-file constraints, bounds and per-executable/archive hashes.
The consumer gets only the validated binary directory. Missing native libraries
still fail at actual launch; there is no hidden Cargo fallback or substitute
older executable. The trust boundary is the same-workflow reviewed producer, not
a signature or independent attestation. The fixed recipe also binds supported
tracked Cargo configuration; external ancestor/Cargo-home configuration and
unreviewed compiler/linker overrides are rejected, not silently normalized.

The v2 runtime policy permits only hosted image patch-version drift within
the same image family; independent hosted jobs cannot reliably pin a patch during
rollout. Source, toolchain, target, recipe, OS, architecture and run **attempt**
remain exact. Both bounded image-version identities are retained separately in
the consumer's validation log after successful smoke, bound to the exact producer
manifest bytes; the producer's identity is never rewritten. A consumer-only retry
cannot accept an artifact from an earlier attempt: rerun the producer and its
dependent consumers together rather than relaxing those identity checks.
Mismatch diagnostics name only fixed, reviewed context fields.

An unrelated **full-mode** job retry may retain the already-successful Windows
execution cohort from the same workflow run and source. The aggregate requires
successful producer and shard matrix results plus six unique, success-only role
outputs (two producers and four shards), all naming one positive origin attempt
no later than the current attempt. It downloads only that exact cohort and checks
source/run/origin identity, complete fresh execution receipts and the independently
discovered current test inventory. Missing or mixed outputs or missing/invalid
evidence fail closed; there is no artifact search or older-cohort fallback. The
result records the original execution attempt separately from the aggregation
attempt, not a new execution claim. Selected-mode receipts do not use this path.
A newly rerun consumer still needs binaries from its own attempt; rerun the whole
producer/consumer cohort when necessary rather than mixing attempts.

CI consumers require `validate --smoke` before application tests. It exercises
five binaries' actual `--help` entry paths plus
`tor-scenario check scenarios/two-room`, comparing its result with the checked-in
certificate. Shared Windows producers also validate a fresh transport copy and
run these probes before uploading the original bundle, without another build.
Same-family compatibility is not a guarantee about every runtime dependency:
these noninteractive probes do not prove sockets, graphics or gameplay. Fresh
application tests still own those boundaries, and a missing runtime or failed
probe remains a hard failure, never an implicit rebuild or fallback.

The required Linux application producer supplies this artifact to Linux debug
Python; it shares the deterministic Rust owner when that coverage is required.
Selected Windows debug and Linux release keep their builds co-located. In the
full matrix, one ordinary application producer per Windows profile serves both
Python shards; independent Rust jobs do not wait on that producer. Preserve
Service-feature separation and actual client smoke tests after validation. A
shared build saves only work it actually replaces: measure build, transfer,
validation and runtime costs together before claiming an improvement.

#### Failure feedback and escalation

- A failed selected owner blocks its gate. Diagnose the earliest actual contract
  failure and keep the original result; do not normalize it away with deadlines,
  retries, stronger fixture actors, changed seeds or weakened assertions.
- On a scheduled/full failure, identify the responsible contract and whether the
  earlier selected plan should have included it. Preserve the failing evidence,
  assign the correction, and do not rely on that affected fast-path mapping until
  the missing edge/regression is fixed or exhaustive coverage is required for it.
  This feedback is currently maintainer-enforced: the planner does not query prior
  scheduled-full health or automatically suspend selected mode. Before merging affected
  work, the maintainer must resolve the failure or require a fresh exhaustive run
  covering the affected ownership; a selected pass alone does not clear it. An
  infrastructure failure is not a behavioral pass or a reason to delete an owner.
- Add a cheap independent regression where it proves the defect, plus the unique
  real boundary witness where needed. A setup error is not an intended first-red.
  Selection defects need positive and negative change/ownership cases as well as
  exact selected-result accounting; test-count equality alone is insufficient.
- Recheck the exact final plan after source/control changes. Report selected and
  deferred coverage, full-cadence failures and local environment limitations.
  Main's selected pass does not close a separate failing exhaustive run.
- Compare updated cost only on compatible source/toolchain/runner/cache conditions.
  Include build/transfer overhead and both latency and runner-hours; timing hints
  may reorder work, never decide whether an assertion is required.

### Prose-only documentation changes

The shared predicate in [`scripts/ci_scope.py`](../../scripts/ci_scope.py) allows
only root `README.md`, `AGENTS.md`, `CONTRIBUTING.md` and Markdown below `docs/`.
It excludes `docs/reference/protocol.md`: its wire examples are inputs to real
Rust decoder tests. Code, tests, scripts, workflows, configuration, dependencies,
assets, executable files, symlinks and unknown paths are not prose-only; they
require the normal risk-based planner or its conservative full fallback.
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
Manual, release and weekly scheduled runs remain exhaustive.

The documentation/link suite and tooling lint still gate prose-only CI. Only a
successful scope/documentation job may intentionally skip dependency and full
matrix jobs. The existing required `Rust (windows-latest)` and
`Rust (ubuntu-latest)` aggregates validate the selected path: they require those
intentional skips plus successful documentation/tooling checks, or successful
completion of every lane required by the canonical non-prose plan. Unexpected skips, cancellation and failures do
not become a successful documentation gate. Dependency jobs retain their existing
full-run advisory/policy rules. A prose-only pass is not full-matrix evidence and
cannot qualify later application changes or exact-tree full-run reuse.

### Full CI partitions

In exhaustive mode, CI runs four independent profile jobs on Linux and six on
Windows, selected with
`full --ci-profile debug|release --ci-partition rust|python`:

| Profile | Partition | Complete coverage |
| --- | --- | --- |
| Debug | Rust | Formatting, workspace all-target Clippy, the isolated simulation fixture gate, architecture checks, warning-free rustdoc, workspace Rust tests including doctests, and production Service contracts via `service_contract_ci.py` |
| Debug | Python/native | Every `test_*.py` suite, including tooling and actual-process/native acceptance |
| Release | Rust | Release workspace Rust tests including doctests, and production Service contracts via `service_contract_ci.py` |
| Release | Python/native | Every `test_*process.py` suite, including native acceptance |

Each Windows Python profile uses two isolated jobs with `--ci-python-shard 1|2`.
Whole test modules (including fixture and native owners) belong to one shard;
the exact union retains every current profile test, including newly discovered
modules. Each profile has one separate ordinary-application producer whose
validated artifact serves both Python shards. Each shard retains two headless
workers, a serialized native lane and the 40-minute job deadline; every selected
test still executes freshly. Artifact/receipt names include the run attempt.
The Windows aggregate independently discovers the current full inventory and
requires both successful, complete, source- and profile-bound receipts for each profile. Missing, cancelled,
failed or incomplete shards fail the gate. Shards cannot adopt unsharded prior
results. The workflow is the authoritative full job inventory; selected-mode
lanes never stand in for a missing full partition.

`--ci-partition` requires both `full` and an explicit `--ci-profile`. Each
partition selects complete coverage without consulting Git merge history.
Stages have explicit owners; an unclassified new stage rejects partition
planning rather than silently omitting coverage. Every required test stage and
environment belongs to its complete partition. Python partitions need explicit
ordinary application/compiler prerequisites before discovery. Unshared plans
build them locally; full Windows shards use `--ci-binaries` to validate the
same-attempt producer artifact instead. Only those application build stages are
replaced, never test execution; corrupt/mismatched/missing artifacts fail rather
than fall back to a guessed executable. The dedicated local/full debug compiler
prerequisite remains when no qualified artifact supplies it.

Ordinary application artifacts deliberately exclude benchmark examples. Full
Windows shards and selected assurance lanes use a separate, profile-specific
locked build phase for the examples required by their independently selected
test IDs, before Python workers start.
Full Windows shard 1 currently needs the eight maintained examples; selected
lanes build only their actual owners' subset, and tooling-only lanes build none.
The phase is required in the plan and receipt, runs without competing test-worker
builds, and stays within the existing job budget. Each owner's original build
check still runs (normally a no-op), resolves Cargo's reported executable, and
retains all runtime limits and report assertions. Binary transport does not make
this compilation cost disappear or qualify benchmark execution by itself.

Service contracts belong to the Rust partitions: their wrapper uses the exact
host executable emitted by the successful Cargo test command. Python partitions'
`test_*.py` discovery does not select this suite. Local quick/push runs use the
same wrapper when their selected packages include `tor-server`.

Without `--ci-partition`, each `--ci-profile` selects its complete profile. The
ordered debug/release union is the local `full` plan. A CI run
does not certify a local run, and one partition never satisfies the full or
exhaustive merge gate. Python/native jobs retain the pinned Python interpreter and native
Windows or Linux Xvfb display in both profiles. Rust-only jobs need no display,
but retain the Linux libraries required to compile and link the ASCII crate.
Profile caches remain separate. Artifacts and verification log directories
identify their profile and partition. The required `Rust (windows-latest)` and
`Rust (ubuntu-latest)` checks require every profile/partition on both platforms
in exhaustive mode. A cancelled or unexpectedly skipped constituent cannot pass;
selected and prose-only modes have their separately validated plans above.

Full-job budgets remain 60 minutes for Rust and 40 minutes for Python/native.
These are infrastructure ceilings, not promises of spare capacity or changes to
individual test deadlines. Use the [current cost baseline](ci-cost-baseline.md)
and measured selected/full runs to assess headroom; older timings do not prove
current workloads fit. Cancellation, missing outcomes and partial passes do not
satisfy a required plan. `verify.py` does not run actionlint, Ruff or cargo-deny;
these retain their separate workflow ownership and advisory policy.

The Python authoring tools call the authoritative Rust scenario compiler.
The existing local quick/push/full debug plans build `tor-scenario` before
compiler-dependent authoring tests. Selected CI gives those owners explicit
application requirements even when their classes derive from plain `TestCase`;
class shape or a non-process filename does not prove a test needs no binaries.
Unrelated pure-tooling and prose-only gates need no compiler. The adapter never
silently builds a binary or falls back to a Python resolver.

### Development loop

Before expensive executable linking, run formatting, applicable cheap schema or
fixture validation, and an affected-target compilation/lint preflight, such as
`cargo clippy -p tor-server --all-targets --locked -- -D warnings`.
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
Let the canonical CI plan select its complete affected coverage; use exhaustive
mode when the planner or a reviewed risk decision requires it. Shared schemas,
persistence, rules and toolchain changes need their broad cross-feature and
profile owners, not only a newly added local regression.

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
not waive the canonical final-head CI gate before merge.

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
the exact-head canonical CI gate is required before merge. A local exception
does not reduce the required selected/full plan.

A push-gate exception is not permission to defer predictable failures to CI.
Unless explicitly waived within that authorization, retain a risk-based pre-push
floor: formatting and workspace all-target Clippy; applicable schema/content and
certificate validation across all affected scenario roots; affected older and new
unit/integration owners; and a small set of real-client and save/recovery flows
for changed mechanics. Select these through the contract-owner matrix, not only
the edited test files. Collect the selected failures from each built binary before
rebuilding, then batch fixes and reuse only still-qualified binaries. This floor
is a stable pre-push checkpoint, not a demand for every unit test or full native
suite on each edit. Keep the exact exception visible and report resource-blocked
or unrun checks explicitly; complete platform/profile and graphical CI remains
required under the unchanged gates above.

### Failure diagnostics and timing evidence

Diagnose the failing phase and supported contract before editing expectations.
Use fixture validation, actor capabilities, authoritative lifecycle/results and
retained traces to distinguish a production defect from invalid setup or a driver
that mishandles legitimate gameplay. Fix the responsible owner; documented fixture
isolation must retain a valid mixed-feature boundary witness. Do not automatically
weaken an assertion, grant capabilities, remove hazards or increase budgets to
make CI green. If semantics are unresolved, record the question for the contract
owner rather than encode a convenient answer in the test.

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
Stopped-session rollover and live segmented capture have different ownership
proofs. At the fall-credit test's existing restart boundaries, retention first
requires every owned process to have finished stopping, a known child exit, a
joined reader and closed child/queue handles; it rejects unowned process
directories. Only then may the existing compressor replace complete logs after
verifying their decoded bytes and publishing the retained-log manifest. It does
not add gameplay stops, saves, waits or relaunches. See the
[fall retention guards](../../scripts/test_fall_credit_process.py) and their
[actual-caller regressions](../../scripts/test_fall_credit_retention.py).
A supervisor's disappearance alone proves neither those child exits nor capture
completeness. Never apply a stopped-only compressor to a live pressure session.

The slow-spectator pressure test opts only its normal player's diagnostic stdout
into 8 MiB lossless capture segments (`TOR_TEST_SEGMENTED_STDOUT_BYTES` in that
launch's harness-only `extra_env`). The option is removed before child launch.
The stalled spectator, network relays, wire-byte pressure formula, action counts,
20,000-turn bound and deadlines are unchanged. Complete-record FIFO publication,
Unicode output, consumed transcript offsets and full decoded evidence stay intact.
A separate compressor repacks only sealed byte segments with no producer writer
or FIFO/diagnostic reader; fsynced, atomically published manifests precede removal
of those segment originals. POSIX additionally fsyncs the directory. Windows does not expose that
directory-sync operation here: this is not a universal power-loss durability
guarantee. It never compresses a live raw segment, drops records or changes
client output flags.
Offline readers verify ordered segments and both compressed and decoded hashes;
source-reader failures and unjoined readers cannot mark a prefix complete.
Ordinary captures are unchanged.

Capture fixtures must satisfy the same owned canonical-directory contract as real
process launches. Canonicalize a test-owned temporary directory once at the
fixture boundary; do not weaken storage's alias, symlink or root-identity rejection
to accommodate a platform's temporary-path spelling. Test both the valid adapter
and raw noncanonical rejection without writes.

Keep each stream's byte contract distinct. Ordinary stdout explicitly writes LF;
segmented stdout preserves UTF-8 LF bytes and logical offsets. The existing
separate stderr text writer uses native newline translation, so its exact-byte
oracle uses the platform newline. Exercise real LF and CRLF text writers while
retaining the complete stdout/FIFO/archive assertions; normalizing all output
would conceal a broken exact-byte capture contract. These boundaries are covered
by [the capture tests](../../scripts/test_segmented_process_capture.py).

Only per-segment staging is bounded: an arbitrarily fast producer, slow storage
or incompressible output can still exhaust total disk space. Keep the existing
free-space guards; compression is not permission to relax them. Compression can
change scheduling, so pure capture tests do not substitute for the actual unread
spectator connection/stall/disconnection assertions. Retained manifests and
segments are part of the process diagnostics, not a new gameplay save format.

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

If a disk-bounded environment explicitly hands a shared warm target between
worktrees, retain the outgoing source-input manifest and compare it with the
receiving checkout before reuse. Cargo can otherwise mark an older workspace
variant fresh when the receiving files predate the cached artifacts. Refresh
only changed local input mtimes (including an owning crate input when a source
file was removed), preserving before/after content hashes and protected binary
copies. Confirm that Cargo rebuilds the affected crate/dependency boundary. Do
not broadly delete warm caches or substitute a full test run for this freshness
guard; qualify the changed contracts through their normal focused checks.

A pre-overwrite artifact inventory must include ordinary application outputs,
normal examples and binaries supplied through `CARGO_BIN_EXE_*`, not only test
executables. Cargo may build these for integration tests even when they are not
selected test targets. Record exact profile/features and source provenance;
matching package names or old fingerprints do not prove that the same executable
exists or is independently retained.

For a no-build process selection, inspect transitive launch helpers as well as
test methods. `TOR_TEST_BINARIES` prevents the ordinary harness's build fallback,
but is not a universal build prohibition: the
[benchmark helper](../../scripts/performance_process_support.py) builds examples
explicitly. Classify those owners separately before launching a bounded suite.
Any permitted compiler version/metadata query must be distinguished from a build;
do not count a guard-refused launch as gameplay failure or a successful test.

Determine run status from the owned process session and saved completion/exit-code
receipts. A process list from another shell or container may be isolated; absence
from `ps` proves neither completion nor failure and does not justify a replacement
build. If the owned status is unavailable, report it as unknown. Filtered regression
runs must verify the intended test names and a nonzero matching count; an exit code
of zero with no selected tests is not verification evidence. Report wrong-filter
or zero-test runs, fixture/setup failures, infrastructure failures, blocked and
unrun checks separately from behavioral failures and passes; none proves the
intended regression was exercised.

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

At the same local stable checkpoint, a narrow correction does not require repeating
checks whose complete source, test, fixture, dependency, toolchain and configuration
inputs remain unchanged. Record the original immutable state, the corrected state,
content hashes and the dependency-based affected set. Rerun the changed behavior
and its affected checks, and complete every required target the earlier run did
not reach. Preserve the original failed or interrupted result unchanged. A separate
coverage ledger may reconcile completed, input-qualified checks with the fresh
results; it must identify every required target and explicitly retain any local
waiver. It is not a claim that the original gate passed. Failed checks, newly added
tests, uncertain dependency boundaries and missing targets cannot be covered by
reuse. Broaden verification whenever the changed boundary requires it. The exact-head canonical CI plan remains mandatory before merge; reuse cannot
turn selected evidence into an exhaustive result.

This permits rapid focused iteration without automatically rerunning a broad
local gate after each narrow commit. Run the broad gate at meaningful integration
checkpoints, then close any corrections with the smallest complete evidence set.

The final commit must pass every lane of its canonical selected or exhaustive
CI plan. Evidence reuse does not remove, ignore or permanently exclude tests, and
only an exhaustive plan proves the complete Windows/Linux debug/release matrix.
Active CI executes fresh lanes; a local reuse ledger is not its substitute.
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
process test. Local tiers are feedback filters, not the canonical CI assurance
plan. Both required platform aggregates must validate that final plan before merge.

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
replaces CI. The canonical plan must pass on the final commit before merging;
when it requires exhaustive mode, the full Windows/Linux matrix below remains
mandatory. These individual commands are diagnostic references, not a replacement
for the verifier or its required stages.
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
| Dependency advisories | `cargo deny check advisories` (advisory-only on PRs; fails on `main` and scheduled full runs) |
| Dependency policy | `cargo deny check bans licenses sources` (policy in [`deny.toml`](../../deny.toml)) |

If any check can't run locally, say so in the PR and explain why. CI is still
required on the final commit before merging.

## Specialized requirements

### Determinism and disclosure

- The simulation must be reproducible from its seed and journal. Tests compare
  replayed, restarted, and rewound state with the original, and check the same
  seed yields the same results.
- For probabilistic mechanics, keep a fixed positive and negative outcome sample
  (for example, hit and miss), with explicit preconditions and exact observable
  effects. Bounded offline seed discovery is acceptable when justified and its
  chosen seeds/setup are recorded; normal tests never search or retry until the
  desired outcome occurs. These witnesses prove outcomes, not probability rates.
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
this exclusion require review. Current-writer comparisons must use the current
golden fixture. Preserve published historical fixture bytes as evidence, never
relabel or hand-edit them into current-format fixtures. Test unsupported versions
and malformed schemas by mutating current canonical fixtures or constructing
minimal invalid cases; historical evidence does not authorize compatibility readers. Independent fixture versions follow their own
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
required before merge; the local push floor and final-commit canonical CI gate
remain mandatory. A native waiver never changes selected/full CI obligations.

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

### Measuring verification cost

Use the [cost baseline guide](ci-cost-baseline.md) and
[`ci_cost_report.py`](../../scripts/ci_cost_report.py) to compare complete execution
inventories. Report elapsed latency and runner-hours separately, retain failed
and retried work, and label missing timestamps or incomplete inventory as limits.
An ordered candidate-after-baseline comparison benefits from warmed binaries and
OS caches; test-stage cost alone is neither whole-workflow latency nor unbiased
cold-start performance. A lower duration never justifies dropping a contract.

The old Windows shadow-candidate measurement command and manual workflow input
are removed with the pass-reuse experiment. Their implementation remains in Git
history, not as an alternate current qualification path. Current measurements
must preserve profile/shard accounting and the selected-versus-full distinction;
a partial profile cannot stand in for a complete baseline.

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
second Rust runner or test inventory. The selected runner independently binds
its canonical plan, exact source and fresh lane outcomes; full partition receipts
retain their complete-stage and test accounting. The retained
[`ci_observation.py`](../../scripts/ci_observation.py) helpers still supply bounded
checkout/probe and allowlisted environment data to the verifier and binary-artifact
validation. Separate workflow before/after-observation jobs are removed. Such
metadata is not atomic or authenticated provenance: reviewed-code trust still
applies, installed package metadata does not prove package bytes, and native
library bytes, volatile display state and unlisted inputs remain outside it.
This uses the same trust in reviewed CI/test code as a normal full run, not stronger
attestation. Completeness is not full-matrix qualification or permission to reuse
or skip work, and prose-only results never become full-run evidence.

### Why CI does not adopt passes from other runs

Premerge, corrective-head and main runs execute every lane of their own canonical
plan freshly. The same-run full Windows retry cohort described above is retained
execution, not cross-head/cross-run adoption or a fresh test claim. The previous
exact-tree and expectation-only pass-adoption tools,
registries and implementation-only tests are removed; Git history preserves them.
The removal is not authority to delete gameplay coverage, fresh-gate assertions,
receipt completeness or unique process/native/durability witnesses.

Reusing a pass removes another opportunity to detect nondeterminism and adds
proof/API/receipt complexity. A matching Git tree or cache hit never established
every volatile machine input or guaranteed a cost saving. Source identity, exact
selected outcomes, clean lifecycle completion, toolchain/profile and input
provenance remain necessary checks where applicable, but none turns a reused
binary or old result into fresh test execution. Dependency caches and validated
application artifacts may save compilation; neither is a test pass.

Current corrections rerun their canonical selected or full plan. Failed,
incomplete or composite old receipts cannot turn a fresh lane green; reports keep
their original fingerprints and are never rewritten to qualify another head.
The [local unchanged-input evidence ledger](#verification-evidence-and-reuse)
remains a separate, explicitly bounded development practice. Reintroducing CI
pass adoption would require a reviewed design, negative tests, measured benefit
and an explicit policy change, not an undocumented runner option.

### Source-aware dependency cache trial

The pinned Rust cache action does not include this virtual workspace's root
`Cargo.toml` in its member-manifest hash. A cache created before the development
optimization settings changed was therefore still an exact hit, and its
immutable contents could not be refreshed with the new dependency variants.
Cargo rebuilt incompatible artifacts correctly; cache success was never test
qualification.

CI's shared key includes an explicit root-manifest hash, profile, partition and
the build recipe produced by [`scripts/ci_build_key.py`](../../scripts/ci_build_key.py).
The recipe is a shared profile envelope, not the exact execution of each shard.
It uses the existing verifier plan and Service host's Cargo command builder,
and includes the canonical maintained-example prerequisite for Python partitions,
preserving Cargo arguments and verifier environment overrides.
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

Dependency-only cache cleanup remains in place; these caches do not retain
workspace outputs. Successful main jobs and same-repository `pull_request` jobs
may save caches; failed jobs, forks and `pull_request_target` may not. PR writes
use GitHub's natural merge-ref scope, not a default-branch override: those caches
are reusable only by that PR, not main or other PRs. See GitHub's
[cache access restrictions](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#restrictions-for-accessing-a-cache).
Full-matrix keys still distinguish platform, profile and partition. The selected
application-artifact transport above is separate from this dependency cache and
is not reusable test evidence.
The first successful eligible job can seed a new cache in its own scope.
Workspace compilation still runs. Measure subsequent independent fresh-checkout
runs, including restore size/time, dependency recompilation and whole-job time,
before claiming savings; an initially cold run is not that comparison. A matching
cache key never qualifies tests.
