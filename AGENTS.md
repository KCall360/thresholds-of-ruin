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
  configuration. Full remains required for save-format, protocol, ruleset,
  persistence, storage, toolchain or dependency changes, and when CI can't run.
  Both-platform CI on the final commit remains required before merging. Record
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

Update this section at meaningful checkpoints with the current scope, decisions,
blockers, and next steps. Keep it short and link to guides rather than
duplicating them. Earlier handoff notes are in the
[`docs-history-2026-09` archive](https://github.com/KCall360/thresholds-of-ruin/blob/docs-history-2026-09/docs/session-handoff.md).

**Architecture refactor (active).** The maintainer authorized the
[refactor plan](docs/refactoring.md). Work remains isolated from the original
dirty interactions checkout. Twenty-three checkpoints, merged in PR #64, cover
explicit command mappings, shared boundary observations, portal-local item/body
and privacy-scoped history indexes, shared route searches, checked requests,
immutable body sharing, storage accounting and concurrency, atomic observation
validation, single-decision AI execution, strict save-codec boundaries, bounded
saved/authored input acquisition, separate authoring/prepared scenario types,
contextual declaration diagnostics preserving validation precedence, bounded
FIFO mailbox draining without starving due actions or save polling, nonblocking
runtime diagnostics with loss accounting, and encoded-output byte leases through
queueing, write failures/timeouts and task cancellation, prepared control and AI
references, shared creature installation, item normalization and contextual
construction-reference diagnostics, and a restore-scoped context sharing decoded
worlds, navigation, items and equal body definitions across retained boundaries,
profiled region-acquisition attribution with validated nested timings, and shared
item/combat definitions with full-value pooling across decoded rewind boundaries.
Future scenario extension contracts are documented without adding a runtime.
That merged checkpoint passed quick/full Windows verification, including 237 debug Python/process
tests and 123 release process tests; all twenty-two deployed-client/validator checks passed.

Release comparisons and their limits are recorded in the plan. Reduced query and
copy counts do not establish broad latency or resident-memory improvements.
The body-sharing restore regression and higher falling-physics command tails
remain unresolved. Compiler source diagnostics and reference normalization,
content pooling, fair allocation under aggregate output pressure, further persistence work,
and the admission/scheduler split remain open. Windows/Linux CI is still required
before merging. The maintainer explicitly authorized pushes and PR merges on
2026-10-04; the required verification and both-platform CI gates still apply.

The queue foundation merged in PR #65 on 2026-10-05 after Windows/Linux CI.
The foundation separates human action admission from simulation execution, preserve original intention identity, suspend queued work
on restart/control loss, and retain bounded gameplay and private audit windows.
The three desktop launchers use a verified immutable foundation build; their
real-client smoke checks passed. Local native mouse verification is explicitly
waived while LockApp covers the desktop; retain ordinary CI coverage.

**Preparation recovery (merged, PR #66).** Paused attacks resume/cancel under
the original admission and preserve spent progress. Interruption facts commit
with their causing action; changed observations precede lifecycle updates. One
actor-owned lifecycle model proves replay/checkpoint phase and target linkage,
including independent queued work and preparation. Full-value actor/queue pooling
preserves copy-on-write isolation across retained boundaries. Substantial restore
memory and persistence batch tails remain open; see [checkpoints](docs/checkpoints.md)
and the [refactor plan](docs/refactoring.md).

**Autonomous integration (published, PR #67).** Exact head `042ddbc` passed
unchanged-input full verification, all five Windows/Linux CI checks and 29
immutable-build deployment checks. The three desktop launchers use this verified
AI build with matching scenario files; previous builds and saves remain preserved.
AI decisions use typed backend admissions and the shared executor without RPC
receipts. Extra saved bytes, slower restart and persistence tails remain open.

**Native travel integration (merged, PR #68).** Journey steps admit private
region-local work and execute through the shared simulation queue; progress follows
committed linked outcomes. Required Windows/Linux CI passed on the final commit.
The three desktop launchers now use the verified immutable native travel build.
Local verification retains the reported LockApp mouse limitation under the
maintainer's waiver; ordinary CI coverage passed. Persistence and restore timing
tails remain open, and no broad performance improvement is claimed.

**Stream recovery (merged, PR #69).** Exact head `c114373` passed all five required
Windows/Linux CI checks. All three desktop launchers now use its immutable
verified build; 43 candidate process checks passed, and active binaries/helpers
were hash-verified after activation. Previous builds and saves remain retained.
The local mouse limitation remains narrowly waived; ordinary CI passed it.
 Protocol contexts and exact observation bases
and bounded shared transport resynchronization are implemented with failing-first,
shared-client, WebSocket, native network-worker and actual-process coverage.
Immediate receipts now retain their journal actor/branch across rewind and restart;
shared request tracking preserves confirmed replies through recovery. Simulation
queue/recovery availability now shares the actual resume/cancel validators and
feeds lifecycle disclosure. Required snapshot readiness and ordered permission
updates now combine that availability with session authority and travel policy;
focused control/queue, shared-client, wire and actual-process coverage passed.
Mandatory originating command contexts now reject stale ownership/reset/stream
and unpublished generations after authorized receipt lookup. Actor readiness
queries are shared per publication pass; only client removals repeat it.
Native/headless admission and native resume/cancel controls honor disclosed
permissions; queued native commands also reject stale authority generations before
sending. Shared pending requests retain typed receipts/errors instead of copying
query payloads; client and actual-process recovery regressions passed.
Request processing returns typed receipts to a common permission-before-reply
publication boundary. Output disconnects trigger a conditional follow-up pass;
history/palette replies share that boundary, and deferred save acknowledgements
share a batch pass without a nested request refresh. Failing-first query-order
and output-pressure
regression and session/integration coverage passed.
Successful Ack/history/palette messages now require current reply context built
from cached disclosure, independently of original receipt identity. Schema, client,
real wire recording, session, WebSocket and actual-process checks passed. Shared
transport validation now repairs mismatched boundaries, preserves original
receipts and quarantines query payloads; foreign actor/attachment identities fail
even during repair. Failing-first transport and actual-process checks passed.
Errors now require explicit transport/unattached/attached scope. Host errors
capture cached disclosure; the shared connection validates scoped rejections and
retains them through repair. Schema/client, all server library, WebSocket and
actual-process checks passed.
Fresh gameplay now enforces the same published admission/resume/cancel permissions,
after original receipt resolution and context validation. Failing-first disabled
admission/recovery regressions, server library, WebSocket and actual-process checks
passed; denied recovery preserves the original work for later permitted control.
Shared client requests now use the same bounded JSON encoder as server output.
Protocol request/response byte ceilings also bound fragmented response assembly.
Focused codec, shared-client and output-lease regressions passed; full remains
required before publication. Text-client input and pacing changes remain deferred.
Its existing gameplay completion now consumes the permission boundary before
chaining input, retaining known failures on delayed permission delivery. Failing-first
and original adventure/door/dungeon process assertions passed. The completed full run passed all 824 Rust tests in each profile and every
application check except the expressly waived native mouse test blocked by the
Windows overlay: debug Python passed 252 of 253 tests and release applications
passed 134 of 135. Recovery process checks prove bounded same-connection repair,
unchanged pre-repair state/history, continued delivery and independent relaunch
coverage. Three interleaved release comparison rounds validated all eighteen runs;
operation and retained-byte counts matched. Their timing limits are recorded in
[the refactor plan](docs/refactoring.md#stream-context-release-comparison).
Subsequent real-socket cancellation regressions exposed lost observation delivery
under automatic palette query backpressure. Shared stored send phases now retain
that delivery exactly once and keep playback active; shared-client, application
recovery and palette checks passed. Refreshed full verification includes that
correction: all Rust tests and application checks passed except the expressly
waived native mouse check in each profile. A second eighteen-run comparison
validated on the corrected code, with identical counts; both timing sets and
adverse save results are recorded in the plan. This full is qualified local
evidence, not an
unqualified full pass; ordinary complete
Windows/Linux CI remains required before merging.
Recovery hardening and the other
[work sequences](docs/refactoring.md#work-sequence) stay in scope.
Full protocol verification and final-commit CI remain required before publication.
Scenario certificates must match the current [format registry](docs/milestones.md).

The maintainer requires clean, robust, rational code. Review each increment for
clear ownership, explicit domain states, cohesive interfaces and repeated rules.
Consolidate overlapping special cases before publication; test success alone
cannot establish architectural quality. Broader region/lineage coverage, AI and
travel migration, stream context, compiler/persistence follow-up, and measured
latency/memory work remain in the full [refactor plan](docs/refactoring.md).
The maintainer authorized version changes, pushes and merges on 2026-10-04;
required verification and both-platform CI still apply.

Gameplay must admit intentions to simulation-owned scheduling.
Derived topology indexes stay backend-only. Text-client fixes and scripting
runtime selection/implementation are deferred. Advance format versions with the
corresponding implementation; future scripting informs boundaries without adding a runtime.

**Test suite rationalization (complete, PR #59).** Process tests share
`scripts/process_harness.py` and use the headless client for wizard setup;
every scenario package is checked by the
[package invariants](docs/testing.md#package-invariants); the workload
validators share `workload_report.py`; protocol messages have recorded wire
samples; test packages are named for what they set up. See the
[testing policy](docs/testing.md).

**Interactive fiction engine (text client).** The engine core and places
merged in PR #60; prose descriptions, walking, protocol changes and the corner rule
in PR #61. The text client's `engine` module turns input into game actions and
everything between two prompts into one passage; the prompt returns when the
server's `waiting` signal says it's the player's move. See the
[IF engine](docs/if-engine.md), [parser](docs/if-parser-architecture.md) and
[adventure commands](docs/text-adventure.md). Decisions agreed with the
maintainer: the server sends facts, never sentences; verbs the game can't carry
out yet are recognized and refused plainly; descriptions state only disclosed
facts, coloured by atmosphere (mood words, smells, sounds) that has no
gameplay effect and is fixed per place; scenarios name places (authored
names), and the text client leaves invented names unsaid; a direction keeps
walking through darkness and along corridors until there's something to see.

Places and ways are read from what's in sight plus remembered cells
(`engine::seen`). Pick up from the engine's
[next steps](docs/if-engine.md#next-steps). Playtest the text client directly
(through the real server, as the process tests do) and fix what reads badly.

**Milestone 4e — region streaming, generation, and asset palettes (complete).**
The region lifecycle (PR #44), streaming through disk regions (PR #45),
preloading, per-region packages, generated regions and server palettes
(PR #47, which changed the save format), its review fixes (PR #53) and palettes in the text
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

The three desktop launchers use a separate verified refactor build and copied
scenario package, selected through their local helper configuration. Existing
binaries and saves are retained, and helper backups are available. Real text,
ASCII spectator, and headless client connections and frames were checked after
deployment. Launchers remain outside Git.

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

**Running until blocked and spatial narrative** (PRs #57 and #58): the
simulation runs until it needs client input, only the server ends a journey,
and clients pace the display; see
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

**Save schema boundaries (in development).** Save-owned typed Serde adapters
separate persisted actor/action/annotation encodings and checkpoint revision maps
from wire serializers without changing current saved shapes. Five boundary tests passed, including all stored metadata variants and numeric
extremes. The prior server library run passed 172 tests, and the expanded focused
process/documentation run passed 23, including the new journal-shape/restart test.
Full verification passed all 829 Rust tests in each profile, fmt, clippy,
architecture and rustdoc. Debug Python passed 253/254; release applications
passed 135/136. The sole failure in each profile is the explicitly waived local
native mouse test; ordinary CI remains enabled and final-commit Windows/Linux
CI is still required before merge. This is qualified local evidence, not an
unqualified full pass. Further save DTO independence and JavaScript-safe wire
encoding remain in scope.
