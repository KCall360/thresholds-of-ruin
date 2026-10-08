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

**Persisted schemas and journal ownership (merged, PR #70).** Exact head
`2f6a741` passed the full local gate and all five final-head Windows/Linux CI
checks. Save-owned serializers retain numeric stored schemas independently of
wire encodings. A storage-owned lease explicitly unlocks at its final owner
boundary while preserving worker ownership and shutdown joining. Deterministic
ownership/checkpoint regressions and real restart tests passed. All three desktop
launchers use the verified immutable build; prior builds and saves remain retained.
Further save DTO independence and persistence/restore tails remain open.

**Lossless wire integers (merged, PR #71).** The current protocol uses canonical
strings for all 64-bit wire values. Typed Rust and independent numeric stored
schemas are unchanged. Exact tested contents passed the full local gate and all
five final-head CI jobs; the three desktop launchers use its verified immutable
build. The release comparison retained matching counts/saves and an unresolved
command tail, with StreamUpdate bytes up about 0.8%/1.6%. No broad speedup is
claimed. Prior builds, saves and helper backups remain retained.

**Bounded typed decoding (merged, PR #72).** Shared request/response decoding
checks complete UTF-8 bytes and a 64-container nesting ceiling before typed
construction. The scan includes ignored fields and respects string escaping;
Serde owns syntax/schema checks. Failing-first unit, fragmented-peer and actual
headless-process regressions passed; healthy-player continuation, reconnect and
save/reopen rejection checks passed. The direct release diagnostic validated
2,000 samples and measured added scan cost (4,096-cell median 7.06 → 7.99 ms).
All 24 engine reports and six focused repeat reports validated with matching
counts; command and persistence timing tails remain unresolved. Exact head
`f85ca2e` passed full local verification and all five final-head CI jobs; the
three desktop launchers use its verified immutable build. The same-build diagnostic
compares identical typed payloads and excludes network/UI/semantic validation;
ordinary engine benchmarks do not measure native JSON decoding. Text-client
product fixes and scripting runtime implementation remain deferred. All six
architecture sequences in the [refactor plan](docs/refactoring.md) remain active.

**Complete observation encoding (merged, PR #73).** Shared response selection
compares complete encoded envelopes and admits the selected bounded text once.
Snapshot/update disclosure state commits after output admission and transfers
owned full state into the next base. Focused Rust, real headless reset/restart,
and streaming diagnostic checks passed. Wire measurement version 2 distinguishes
shared encoding/decoding timings and complete sizes from historical partial DTO
measurements. Exact head `8cbc637` passed all eight full local stages and all
five final-head Windows/Linux jobs. The three desktop launchers use its immutable
verified build; 16 copied-build process checks passed and active binary/helper/
backup/scenario hashes were verified. Collection deltas and fair aggregate output
pressure remain in the full refactor scope.


**Ordered observation collections (in development).** Protocol collection edits
retain unchanged inventory, ground items, projected actors and opaque places.
Original-base spans reconstruct exact order with checked ranges and a forward
merge; projected occurrences remain distinct. Failing-first size regressions,
shared atomic rejection, real item checkpoint/restart/rewind and held-repair
ASCII/text process tests passed. Current wire samples were recorded from real
processes. Individual collection diagnostics vary 16/256/4,096 entries; release
measurements validated 24 interleaved reports, 12 focused repeats and 2,400
individual samples with stable operation/save counts. Earlier Windows full
verification passed, but Linux CI exposed a fixed-size slow-reader workload that
no longer filled socket buffers with compact deltas. The revised traffic-based
workload passed both Windows pressure/restart cases at normal and 4 MiB budgets;
refreshed final-head full verification and Windows/Linux CI remain required.
The first Linux traffic-sized run passed its 268 Python/process tests but hit
CI's 35-minute job limit before completing Rust checks. The shared-fact counter
now includes identically broadcast observation events and intention status,
excluding replies, readiness and stream metadata. Both Windows 4 MiB cases
passed at 4,527 turns each instead of 12,509, preserving pressure thresholds and
stall/exit deadlines. That checkpoint passed Windows CI but Linux failed both
spectator child-exit checks. The test relay now separates server socket closure
from client drain: Linux checks exact endpoint/process-owned inode while reads
remain paused, then restores a normal receive buffer before draining. Unit
regressions failed first; Linux acceptance remains unverified. The required
full gate and final-head CI still apply; do not merge or activate a failing head.
Save/rules/scenario versions are unchanged. Shared validation and host selection now bound retained
full observations independently of individual frame size, with failing-first
regressions and real ASCII/text held-repair checks. Earlier pre-bound measurements
are preserved separately; refreshed evidence
includes production validation cost. Smaller wire payloads do not establish a
broad engine, persistence or resident-memory gain.
No scripting runtime or text-client product fix is added.
Aggregate output fairness and the other six-sequence work remain active.


**Observation ownership and output failure types (in development).** Disclosed
views now share immutable ownership across readers with independent stream
metadata; codec/admission failures distinguish capacity from preparation without
string matching. Library/integration checks passed 457 tests and all-target
Clippy passed; 59 selected process/tool tests passed. Aggregate allocation now
reserves one maximum frame per connection with bounded borrowing and preserves
reservations through outstanding output. Its quick gate passed, including 331
Rust tests and 147 actual-process tests. Required typed welcome capabilities now
advertise static limits; shared clients enforce request/response ceilings and
capacity rejection uses resource_limit. Its unit/integration/actual-process checks
passed; release/full/final-head CI remain pending before publication. See the full
[refactor plan](docs/refactoring.md). The local
native mouse waiver does not remove ordinary Windows/Linux CI coverage.

Earlier capability full gates exposed a save-warning test race, an intermittent
control timeout and a stale decoder fixture. The corrected process module and
278-test Python stage passed, as did all 877 workspace Rust tests including
examples after current fixture/version guards were updated. Production behavior
and concurrent-reader deadlines are unchanged. Fresh full verification remains
required; failed evidence is retained.


**Backend action facts (in development).** Receipt commands and journal action
facts use independent backend action/direction types, with exhaustive wire,
simulation, disclosure and save DTO mappings. Save bytes and gameplay semantics
are preserved. Focused Rust checks passed 188 unit and 147 integration tests;
all 17 item/intention actual-process cases, all-target workspace lint and focused
documentation checks passed. The release comparison validated 24 reports and six focused repeats with unchanged
counts and mixed tails. Full/publication gates are pending. The preceding protocol
checkpoint hit Linux CI's 35-minute job budget during release processes. CI now
partitions the unchanged full plan into separate debug/release jobs per platform,
with existing platform check names guarded by all four profile results. Profile
regressions, CLI dry-run plans and workflow/tooling lint passed; full local and
actual final-head CI remain required before publication/merge/activation. Save/restore tail attribution remains open. Original
receipt identity must be resolved before ephemeral target checks. See the
[refactor plan](docs/refactoring.md). Protocol/observation PR CI runs separately
against its immutable published checkpoint; do not attribute it to this increment.

**Verification correction (2026-10-07).** Published head `143c87a` passed the
complete local full gate (283 Python checks, 880 Rust checks per profile and 148
release process checks) and 25 immutable-build smoke checks. Both Windows CI
profiles passed; Linux debug timed out draining the artificial spectator relay,
and Linux release stalled acquiring graphical dependencies before testing.
Failed CI prevents merge and desktop activation. The fixture now restores
Linux's independent advertised-window clamp before releasing receive pressure;
a failing-first regression covers this. Dependency setup has a separate bounded
budget. Actual Linux verification and the updated publication gate remain
required. Test deadlines, thresholds and native coverage are preserved.

Local worktrees now use exclusive compiler output directories after shared
outputs caused mismatched executable startup failures. Those attempts are invalid
verification evidence. Published full evidence and immutable copies preceded the
overlap. All six refactor sequences and save/restore tail attribution remain open.


**Semantic generation (in development).** Rooms v2 uses canonical semantic seed
inputs and separate geometry/placement/population/loot streams; raw source hashes
remain integrity identities. Default-zero salt is explicit, disjoint placement
lanes preserve pool independence and infeasible minima fail instead of truncating
silently. Lazy identity assembly now includes authored characters starting in
generated regions. The generator ruleset and validator identities changed;
the then-current protocol, save 21 and authoring format 2 did not. Focused backend/validator and
real-process regressions passed; final full, measurements and CI remain. The
separate PR #74 checkpoint has passed Windows profiles, but Linux debug spectator
drain failed its existing deadline; do not merge or activate it until corrected
complete CI passes. See the refactor plan for all six remaining sequences.

**Testing policy update (2026-10-07).** At the user's direction, the local push
gate now runs every debug check and leaves broad release suites to exact-head
Windows/Linux CI before merge. This supersedes earlier notes requiring a local
full run for compatibility or persistence changes. Targeted local release checks
remain necessary for performance measurements and release-specific behavior;
local full remains required when CI cannot run or on request. No CI coverage,
native coverage or individual deadlines were removed. One broad debug gate per
stable PR checkpoint covers the local publication requirement; focused
failing-first development checks continue between checkpoints.

PR #74 subsequently passed all nine exact-head CI jobs, merged, and its verified
immutable desktop build was activated. Semantic generation remains unpublished:
its broad debug gate, release measurements and final-head CI remain pending.
All six original refactor sequences remain in scope.

**Scenario checkout integrity correction (2026-10-07).** PR #75's first Linux
profiles exposed certificates generated from local CRLF manifest edits, whereas
Git's existing LF policy supplies different bytes on checkout. All 33 manifests
and two edited recipes were restored to LF and certificates regenerated from
those exact bytes. Integrity hashing remains byte-exact. A cheap repository
regression checks both LF source bytes and every manifest/index/region digest.
The actual-process regression fails against the prior committed certificates,
then passes with corrected certificates: both authored and generated packages
start without revalidation and recover exact state after their source disappears.
All 33 release validators and all scenario process cases passed. Final debug
publication evidence and replacement exact-head CI are still required; failed CI
is retained, not rerun as a substitute for a correction. All original work
sequences remain in scope.

**Command conversion ownership (2026-10-07).** Wire command/action conversion
and developer parsing now live in the explicit `wire_adapter`, with journal
conversion methods and the implicit command conversion trait removed. Workspace
callers use the same exhaustive adapter. Admission, authority, receipt ordering,
numeric save payloads and simulation execution retain their behavior. The
expanded boundary tests passed before and after extraction; 197 server unit
tests, 148 server integration tests, all-target server Clippy and 30 selected
process/documentation checks passed. Opaque targets and observer-scoped history
projection remain open; this increment does not complete domain/save separation.
Broad publication verification and final-head CI are required before pushing
and merging this follow-up. PR #75's preceding scenario checkpoint has separately
passed its 294-Python/889-Rust local debug gate and all nine final-head CI checks
on `58081b3`, and is merged. All six accepted sequences remain in scope.

**Opaque interaction references (in progress).** The current wire schema uses
observer/save-scoped actor, item and door references and an explicit self target.
Fresh resolution uses cached native disclosure after metadata checks; receipt
comparison reconstructs original references without live target lookup. The
save schema, deterministic simulation and queued execution remain independent.
The real wire samples were regenerated. Focused verification passed 201 server
unit tests, then the expanded three scope/retry regressions; 11 protocol unit,
50 protocol integration and two diagnostic-example tests; 25 shared-client unit
and 52 integration tests; and 46 text-client unit and 80 integration tests.
Server integration, ASCII, diagnostic examples and process fixtures are migrated.
All workspace test targets compile, and workspace/all-target Clippy passed before
the latest recovery/order regression additions. The first broad server integration
run passed 145 tests and failed three fresh-save comparison tests; their scoped
reference normalization correction passed all four focused preloading tests.
The new real-WebSocket taken-target reconnect retry passed, as did six ASCII unit
and 30 integration tests, 31 selected process tests and the corrected wizard-client
scenario. Four wire-request tests now also prove drop receipt recovery across
rewind/restart without inventory lookup. An opaque-byte ordering regression failed
first; scene ground ordering now preserves disclosure order and nearest portal
occurrences. Door and tied figure order and identical-item selection likewise
use disclosure order instead of handle bytes, with failing-first regressions;
all 46 text unit and 84 integration tests passed. The broad debug run passed all
904 workspace Rust tests, formatting, workspace Clippy, architecture and strict
rustdoc, but its Python stage had 288 passes and ten failures. Nine were remaining
legacy fixture assumptions; the corrected dungeon, physics, sight, generated
content and invalid-state recovery suites passed all 23 focused process tests.
The tenth was the narrowly waived LockApp mouse overlay; retain ordinary CI
coverage and do not describe that broad run as an unqualified pass. Fresh final
publication verification, targeted release measurements and exact-head CI remain pending.
This committed follow-up is unpublished and must not be deployed or treated
as a completed protocol checkpoint. No text product fixes or scripting runtime
were added. All six refactor sequences remain in scope.

The initial targeted release comparison validated all eighteen reports with
matching operation/save counts and larger wire payloads. The 1,000-item client
application interval increased; no broad performance improvement is claimed.
Its latency timers omitted disclosure projection. Identical benchmark-only
projection instrumentation is now prepared on both baseline and current source;
the comparator retains request-call and projection intervals without inventing
missing historical measurements. The new extraction regression failed first,
then all 26 tooling tests passed with task-owned temporary storage; the initial
default-temp sandbox failures remain recorded. Example Clippy and documentation
checks passed. The three-round instrumented comparison then validated all twelve
reports with matching operation/save counts: projection p95 was 0.405 to 0.392 ms
in memory and 0.376 to 0.381 ms durably, with 945 samples per side. No broad speedup
or all-load projection claim is made. Final debug publication evidence and all
exact-head Windows/Linux CI checks remain required.


**Interaction identity follow-up (merged, PR #76).** Native target
choices now retain disclosure order instead of sorting opaque bytes. Rewind
continues all simulation allocation counters so abandoned dynamic entities
cannot lend their handles to replacements. Failing-first native and durable
actor/item/door regressions passed, as did actual-process pickup/restart coverage.
Save 22 and dungeon-v23 identify the changed replay semantics; all 33 scenario
certificates were regenerated from LF inputs. Wire 29, authoring 2 and validator
8 stay unchanged. All nine exact-head CI checks passed. The three desktop launchers use its
verified immutable build; all 30 copied-build smokes passed, with previous
builds and saves preserved. The separate scenario-diagnostics branch remains
in development; see the refactor plan for scope and evidence.

**Scenario reference provenance (merged, PR #77).** AI configuration and authored
actor/item archetype failures now carry exact parser-span source locations,
with typed missing-reference names and one declaration-context formatter. Source
is acquired only on failure; unavailable, changed or ambiguous provenance keeps
the original declaration error without guessed coordinates. Failing-first
real-validator regressions passed, as did all 211 server unit tests, seven scenario
integration tests, nine scenario process tests, workspace all-target Clippy and
changed Python lint. The complete local debug gate passed 301 Python/process and
914 Rust tests, with unchanged inputs and no waiver; all nine exact-head CI jobs
passed before merge. Desktop remains PR #76 until the next coherent update.
Further reference normalization and all other original obligations remain open.

**Bounded snapshot recovery (merged/deployed, PR #78).** Wire 30 transfers complete
snapshots under independent state/frame/logical ceilings. One queue entry, whole
byte leases and one absolute write deadline prevent interleaving and slow-reader
renewal. Shared client assembly survives canceled reads and exposes only complete
validated snapshots. Final debug verification passed 302 Python/application and
926 Rust tests on 544 unchanged sources, without a waiver; all nine exact-head CI
jobs passed before merge. All 31 copied-release real-client checks passed. The
three desktop launchers use immutable `2f79b3d`; post-activation hashes verified
sources, binaries, backups and shortcuts. Previous builds and saves remain intact.

**Scenario reference ownership (merged/deployed, PR #79).** Shared declaration
provenance belongs to diagnostics; character anchors and concealed-item assets
use one rule each. Full debug passed 304 Python/application and 931 Rust tests
on 544 unchanged inputs, without waiver; all nine exact-head CI jobs passed.
All 33 copied-release client/validator checks passed. The three desktop launchers
use immutable `76ff104`; source, binary, backup and shortcut hashes were verified
after activation. Previous builds and saves remain available.

**Save scenario ownership (in development).** Explicit save schemas now cover
scenario/actor/coordinate/streaming values, package metadata, nested authoring
schemas and the separately stored region index. Encoding borrows definitions;
decoding moves fields and builds containers directly. Diagnostic text, caches and
derived topology are excluded, and region acquisition stays lazy. Actual-process
regressions failed first for duplicate anchor keys and omitted canonical index
fields, then passed; all 28 save/scenario/streaming process checks passed.
Pre-refactor writer fixtures pin four current-format scenario shapes. Eight schema
units, the borrowed-encoding test, five checkpoint, twelve package-pinning and
seven scenario integrations, and workspace Clippy passed. All 18 targeted release
comparison runs validated with matching counts/saved sizes; variable streamed
flush maxima increased and 256-region restart p95 rose 3.9%, so no broad speedup
is claimed. Full debug publication verification remains pending. Save 22 and
other format axes are unchanged. Portable export is documented as a future
contract in [checkpoints](docs/checkpoints.md); no export or scripting runtime is
implemented. All remaining compiler/performance/final-audit obligations stay open.

**Catalog reference provenance (in development).** Six actual-validator cases
failed first for combat factions, faction enemies, asset-theme keys and authored
default character. Existing parser spans now cover unique decoded values and
terminal keys; ambiguous/stale source has no guessed coordinates. Combat attribute
precedence and failure-only source acquisition are tested. Explicit CLI selections
are not attributed to the manifest default, including an identical invalid ID.
Forty scenario/diagnostic units, seven integrations, sixteen actual scenario cases,
workspace Clippy and Python lint passed. Empty faction catalogs and zone-local
themes remain valid. Combine this with the preserved disclosure optimization for
one final publication gate; transport PR #81 remains at its separately tested
1d59f3e head under fresh CI. No format or scripting runtime change.

**Scenario reference paths (in development).** Typed parser traversal now covers
root/table fields, array entries and numeric references using the existing
failure-only provenance owner. Nine actual-validator failures reproduced first;
36 scenario units, seven scenario integrations, twelve actual-process tests and
workspace Clippy passed. Validation precedence, lazy acquisition and unavailable-
source behavior remain intact. PR #81 passed the full local debug gate (306
Python/application and 943 Rust tests), but Linux debug and a later Linux release
run exceeded the stalled client's exit bound after timely server descriptor
closure. The fixed-capacity relay repair did not resolve the release failure.
The next candidate explicitly owns transport closure: resource failures and
cancellation reset upgraded TCP connections; intentional detach explanations
share one bounded drain deadline. Successful rejection/normal closure preserves
output, and byte leases survive until socket destruction. Two real-TCP behavioral
regressions failed first, then passed; the old drain loop also fails the deadline
regression. Final transport units passed; 236 server units, seventeen WebSocket
integrations and nineteen actual-process cases passed before the last drain
cleanup. Full final debug and both-platform CI are required before publication,
merge or deployment. Original deadlines and pressure/recovery assertions remain.
PR #80 is merged and its separately verified immutable desktop build is active;
the PR #81 candidate remains inactive. Broader compiler references, measured
performance tails and the final requirements audit remain in scope.
