# Server and protocol performance review

## Scope and conclusion

**Latest measured status (2026-10-28, compact causal capture):** frozen
headless smoke runs completed **3.026 and 3.235 moves/second** with 100 active AI
and 20 portal-connected regions, versus 2.058 and 1.864 for the original display
split. The **5 Hz goal remains unmet**. These small pre-integration ABBA runs
preserved identical execution traces, portal crossings and save/restart state;
they do not establish sustained throughput or a cumulative gain over phase20.
The earlier unexplained recovery tail remains open, not waived.

The causal/display split and compact capture are implemented and tested. Fresh
current-main integration, genuine wire/save recordings and the focused
qualification below are complete. Native 3D UI and required hosted CI remain
separate, unqualified here. Historical cohorts below retain their negative
results and measurement limits; they are not claims about the final integrated
source's performance.

This review covers authoritative scheduling and AI/navigation, observer projection,
protocol delivery and backpressure, and persistence/recovery. It excludes frontend
art/UI and does not claim a line-by-line security audit. Inspection began at
`c759fb9714c5449934354d7e19668a08df7aa469`; the measured production baseline is
merged main `279ba9d30aa48113d28377777dcc9afc8578f801`, including the current worn
appearance projection. The new workload is held identical across comparisons.

The architecture is sound: typed boundaries separate protocol, simulation and save
records; candidate state is unpublished until journal admission; simulation owns
ordering and seeded decisions; disclosure and opaque targets remain observer
specific. Bounded inbound/outbound queues and asynchronous storage protect the
single authoritative owner. Refactor measured repeated work, not these guarantees.

The principal scaling risk is population-wide work per individual AI action.
A large region catalog or parked actor population is not evidence of active load.

## Findings

The table records the initial review findings and proposed remedies. Later
sections distinguish implemented/measured changes from unresolved risks and
unmeasured candidates; the initial findings are not all still-open defects.

| Priority | Finding | Evidence and disposition |
|---|---|---|
| High | Existing maintained workloads do not prove 100 active AI across 20 regions. | `Scenario::performance` accepts at most eight actors; ordered-strike large populations explicitly park actors. Add a new versioned workload, retain older workload identities. |
| High | Every perception-changing action rebuilds all observers' disclosure. | `Engine::resolve_action_transition` obtains each old revision view and reconstructs each surviving actor's new view. Cached geometry does not eliminate entity/cell projection. One complete AI round can amplify this to population-squared work. Do not filter to connected clients: knowledge, event privacy, navigation and replay depend on unconnected actors too. |
| High | Retained melee preparations force fresh geometry repeatedly. | `finish_action → resolve_preparations → target_in_reach_snapshot → attack_available` invokes force-fresh `Game::scene` for each retained preparation. The eight-step baseline built 43,938 ordinary scenes for 1,100 AI executions. Reuse exact witnessed geometry while revalidating live contact every call. |
| High | Awareness constructs rich observations and fresh hearing geometry for every listener. | Reconciliation, attack capture and attack application primarily need fresh actor-reference rows, not terrain/inventory/combat DTOs. Keep canonical row eligibility/order, contests and RNG; share geometry and exactly witnessed actor classifications; keep rich disclosure fresh. |
| High | Resident history grows with all accepted AI records. | `Archive.records`, receipt and `HistoryIndex` structures retain the journal. AI admission and execution are separate records. Checkpoints bound replay tails, not resident history. Measure retained entries and RSS slope; paging requires a separate design preserving immutable history and retries. |
| Medium | Actor-table COW may copy every actor at a mutation. | The original shared ordered map held owned actors. The per-actor snapshot batch below replaces payload ownership while preserving ordered iteration, clone isolation, indexed mutation and save adapters; release impact remains pending. |
| Medium | Duplicate location indexes add repeated projection work. | Ordinary observation builds scene and eye location sets, then entity projection constructs the same sets again. Pass borrowed immutable indexes to the existing classifier; preserve distinct genuine sight and merged disclosure. |
| Medium | Copy diagnostics underreport real work. | Actor insertion/removal omit COW accounting; scene cache hits clone disclosed and sight arrays without accounting. Correct the metric and preserve causal tests before interpreting zero counts as zero work. Historical values are not complete accounting. |
| Medium | Per-cell opaque key rendering allocates tiny strings repeatedly. | `adapt::cell_key` formats 20 digest bytes independently. Fixed ASCII rendering can preserve exactly the same SHA1 inputs and key bytes without changing protocol identity. |
| Conditional | AI scene recomputation and portable-light unions may multiply work. | AI decision snapshots recompute geometry despite an exact-witness cache; lit-cell queries rebuild emitter unions per observer. Profile the relevant workload before adding caches; the torch-free baseline cannot qualify torch performance. |
| Lower | Scheduler scans and path reconstruction repeat bounded work. | Due-actor selection scans the ordered actor map; target ranking reconstructs route vectors merely for their lengths. Retain until profiles justify indexing or distance/first-step APIs from the same settled search. |

## Clarity and robustness assessment

The core boundaries are clearer than the central files' size suggests: execution,
intention admission, checkpoint capture, projection and protocol validation already
have separate modules. Nevertheless, `Engine::resolve_action_transition` combines
candidate execution, every-observer projection, event proof capture, navigation
refresh, revision comparison and profiling. Keep its ordering explicit; extract
small typed phase results only when changing a measured responsibility. A broad
Engine/Service rewrite would add risk without demonstrated runtime benefit.

The implemented refactors remove duplicate representations: one slice-based wire
adapter, one canonical actor-row classifier for rich and awareness-only views,
and one bounded immutable geometry payload with independently lazy indexes.
Compatibility APIs that promise owned vectors still return independent vectors.
Pure queries must not roll RNG, teach knowledge or publish rejected candidates.
Existing positive and negative tests cover these boundaries, not just timing.

The review found no established authorization bypass, privacy leak or recovery
corruption. This is bounded code review, not a security certification. Queue
admission precedes expensive publication; overflow disconnects rather than
silently dropping authoritative state. Explicit durable barriers remain distinct
from ordinary asynchronous acknowledgements. Long-lived history/RSS growth and
population-wide projection remain architectural risks even after local speedups.

## Refactoring constraints

A terrain cache is not safe under scene geometry alone. The second optimization
therefore caches only raw World facts under complete non-owning
`SharedIdentity<World>` plus the exact scene/emitter witness; it keeps one
replaceable payload per existing bounded scene. Every observation still projects
private discovery, contact, occupancy-dependent door clearance, terminal policy
and hallucination afresh. No Actor snapshot or per-tick history is retained.
Its measured effect and remaining acceptance failures are recorded below.

Scene geometry alone is insufficient because door approaches depend on
actor clearance; place-hint edits do not advance the current scene witness;
discovery controls secret-door/trap facts; terminal exits require their policy;
hallucinated cosmetics must remain fresh. Rejected candidate cache entries must
never replace live authority. Prefer removing duplicate derivation or sharing
immutable arrays before a broader cache-invalidation redesign.

Do not replace ordered collections wholesale, drop historical records, slow AI,
freeze distant mobs, suppress observer updates, increase queues/timeouts or weaken
recovery/privacy to meet a latency number. Large file extraction should follow
cohesive responsibility while changing a measured hotspot, not churn central
Engine/Service paths solely for style.

## Workload and acceptance boundaries

`active-mobs-v1` contains 20 portal-connected rooms and 100 ordinary AI creatures,
five per room, with two hostile factions, real pursuit, obstacles and nonfatal
basic combat. All 20 regions remain active through the ordinary streaming path.
A neutral controlled actor follows a clear connected corridor; the corridor avoids
crowding as a confounder, not simulation work. AI care, loot, equipment and gold
policies are disabled. This is not every AI tactic or combat balance acceptance.

The workload rejects absent/dead/frozen/passive populations, missing region
activity, no pursuit/strike work, missing actual movement/portal crossings and
recovery mismatch. Its private proof counts each AI and region; a catalog count,
loaded-region count, or successful command admission alone cannot qualify it.

The real headless run offers 300 movement commands on fixed 5 Hz deadlines over
60 seconds, waits for authoritative move completion and the following AI round,
and records lateness without resetting the schedule. A separate unpaced burst
measures catch-up headroom. Report p50/p95/p99/max for admission, execution,
outcome, fresh context and next-ready; report server/client/driver RSS separately.
The proposed stress budget is p95 below 100 ms, p99 below 200 ms, all planned moves
within the 60-second window, no growing driver deadline backlog and throughput
headroom above 5 Hz. Driver deadline backlog is not server mailbox occupancy.

The existing ordinary saved-action target remains p95 below 8 ms and maximum
below 33 ms. The stress budget is an additional boundary, not a relaxation.
Action tick/cooldown semantics remain separate from wall-clock responsiveness.

Cold activation and source-free restart are separate from warm portal crossing.
The all-active workload deliberately does not measure cold streamed traversal.
Direct Engine phase timings distinguish candidate capture, simulation including
AI, navigation, perception, revision comparison, checkpoint capture, serialization,
journal admission and publication. They do not substitute for real client latency.

## Measurement and verification

### Baseline: target not met

The optimized production baseline is `279ba9d30aa48113d28377777dcc9afc8578f801`
with harness-only commit `9fe8b6d818cc8c0a3364251e99924237d32b3303`.
The first focused release build took 33m39s; source was frozen and guarded, not a
series of broad test runs. Four separately hashed binaries were retained before
any implementation changes. The maintainer approved sequential shared-cache
baseline/head builds for this task; this is not the normal isolated-cache recipe.

One complete eight-move direct cohort proved 100 meaningful AI identities,
20 active regions, 1,100 AI executions, 720 resolved strikes and one player portal
crossing. All actors remained alive and strict replay reproduced all 101 actor
views. Player-cycle p50/p95/max were 3,202/3,760/3,760 ms (n=8), while individual
player-command p95 was 30.2 ms. Simulation accounted for 47.3% and observer
projection 40.7% of measured exclusive phases; navigation was 0.05% and journal
serialization 0.57%. There were 230,068 full observations and 43,938 ordinary
scene builds. These small-tail samples establish a bottleneck, not a stable SLA.

The real-headless 60-second/5Hz attempt completed only 17 of 300 planned moves
before its 90-second completion allowance expired (last next-ready at 93.6s).
Request-to-authoritative-outcome p50/p95 were 56.5/198.7 ms; next-ready p50/p95
were 5,778.5/6,433.5 ms. This incomplete run is explicitly failed load acceptance.
Cleanup completed. Its abort path had no successful explicit-save barrier, so
reopening recovered a durable 12-move prefix, not all 17 acknowledgements; that
matches existing asynchronous-save semantics. The retained prefix independently
contained all 100 active AI across all 20 regions. Do not use it as complete-run
replay proof. The direct complete cohort supplies the separate replay witness.

### First measured implementation: improved, target still not met

The first optimized head is `472d9f680417e9e10f1b4e29523e2c70aa72a310`,
measured against the unchanged `9fe8b6d8` harness baseline on the same Xeon 8370C
VM, release profile, seed and runtime fixture. The head build took 20m44s;
separately verified binaries and immutable baseline runtime inputs were retained.
The task-specific sequential-cache exception does not qualify the default
isolated-cache build recipe.

Two interleaved balanced rounds (ABBA, 16 player cycles per side) passed both the
own-tree validators and the stronger current activity validator. Player-cycle
p50 fell from 3,014.0 to 1,505.2 ms (50.1%); p95/max fell from 5,439.1 to
2,928.9 ms (46.2%). Each round retained exactly 1,100 AI executions, 720 resolved
strikes, 100 meaningfully active AI, 20 active regions and one player crossing.
Run variance is substantial; the earlier single eight-cycle head median of
1,311.8 ms is not the paired estimate or a stable SLA.

In that first head profile, exclusive observer projection consumed 6,341.3 ms
(61.1%), simulation 2,258.2 ms (21.8%) and navigation-memory refresh 9.2 ms
(0.09%). **Simulation includes AI choice, decision geometry, pathfinding and
execution.** The separate navigation phase is not AI pathfinding. Individual AI
outer calls had p50/p95/p99 of 9.019/12.649/32.449 ms (n=1,100); these calls
include overlapping projection/publication phases and must not be added to the
exclusive phase totals. Decision-only time is not separately instrumented.

The real headless smoke completed its 16 paced moves in 33.93 seconds, only
0.472 Hz. Outcome p50/p95 was 52.5/66.7 ms, but next-ready p50/p95 was
1,935.0/3,193.7 ms and the fixed-origin deadline backlog grew. Thus responsive
individual acknowledgements do not establish usable five-move-per-second play.
The explicit save and private inspection succeeded; all 100 AI showed bounded
meaningful-action gaps of at most 100 simulation ticks. The subsequent source-free
restart exceeded the unchanged 30-second readiness limit, so the smoke is failed,
not accepted. A protected-copy diagnostic subsequently became ready in 8.664s
(store/decode 3.954s, replay 4.702s); it did not reproduce or resolve that failure.
No timeout was increased and a clean final process run remains required.

Canonical traces of the first 1,631 successful executions matched exactly between
the baseline's durable 12-move prefix and the head's 16-move save. The canonical
hash was `705d4eba2e7b696f269cf8bae1e43f905636c2e0aa1530cc51d67934875015b5`.
This compares actor, tick, full action/event, intention sequence and lifecycle
facts while excluding only audited run-specific identities. It is not proof that
the aborted baseline durably saved all 17 acknowledged moves.

Existing ordinary saved-action guardrails were measured separately, with two
balanced rounds and unchanged workload counts:

| Case | n per side | Baseline p50 / p95 / max (ms) | Head p50 / p95 / max (ms) |
|---|---:|---:|---:|
| r8-a1-h100-durable | 610 | 1.138 / 2.164 / 5.274 | 1.045 / 1.826 / 3.276 |
| r64-a8-h100-durable | 5,000 | 0.167 / 4.681 / 117.381 | 0.138 / 4.043 / 132.478 |

Both head p95 values satisfy the existing 8 ms target. The larger case violates
the existing 33 ms maximum at both baseline and head; its rare tails remain
unresolved and the whole ordinary guardrail is not green. No throughput,
full-duration memory-growth, cold streamed-traversal or final-main acceptance is
claimed yet.

Local checks are focused by changed contract. New counter regressions require an
observed intended first-red; disclosure tests retain independent sight/contact and
hidden-identity assertions. Correctness, replay and actual process witnesses remain
separate from timing. No local selected result is exhaustive hosted qualification.
No publication or raw release-asset upload is authorized by this local review.


### Raw World-fact cache: second measured implementation

Head `da492247cfb57a6043a2a39e826d983bc57b371a` reuses only bounded raw
World facts under full World identity and exact scene/emitter evidence. Actor
knowledge, concealment, live door clearance, contacts, terminal policy and
hallucination are still projected afresh. The sequential release build completed
in 26m02s; its unchanged-input receipt and four frozen binary hashes survived the
executor interruption. No duplicate build was necessary.

A fresh same-host ABBA comparison against the unchanged harness baseline had
16 player cycles per side: median 2,810.2 to 954.3 ms (**66.0% lower**). Head
p95/max was 1,051.6 ms. The baseline p95/max included a 23,321.1 ms outlier,
retained rather than used to advertise a stable tail improvement. The direct
8-cycle head profile had median 928.4 ms, with 2,694.2 ms exclusive projection
and 2,677.6 ms simulation total; the latter still includes AI choice, pathfinding
and execution. Activity counts and complete replay remained exact.

The real headless smoke now passed correctness, explicit save, private inspection,
source-free restart (7.21s) and cleanup. Its 16 moves still took 17.62s: **0.908 Hz,
not 5 Hz**, with next-ready p50/p95 of about 982/2,033 ms. This clean restart does
not diagnose or erase the earlier timeout. Full-duration acceptance is pending.

The matching ordinary ABBA cohort met both existing budgets: r8-a1 p95/max
1.832/3.768 ms (n=610), r64-a8 2.702/20.769 ms (n=5,000). Earlier rare spikes
remain recorded; this cohort alone does not establish a stable tail SLA.


### Exact geometry and shared cell projections

The next implementation keeps the bounded exact scene witness for both ordinary
observations and AI decisions; only a cache miss counts as a decision-scene build.
Every decision still projects current entities, knowledge, effects and interaction
facts. Exact eye sight remains distinct from broader nonvisual disclosure.

Raw feature-free `CellView` arrays can also share storage between retained
simulation observations. Door/trap discovery, occupancy-dependent approach checks
and hallucination detach an observer-owned copy before modifying any rows.
Caller mutations cannot edit another view or the raw template. The wire adapter
still produces the ordinary owned protocol vectors; no wire/save format changes
or observer suppression are involved. The projection-scoped copied-row counter
includes feature/cosmetic COW detachments, not shared-handle clones or consumer
copies. Older frozen binaries lack that newly introduced counter; its absent
baseline is unknown, not zero.

These changes were measured together at `fe010f83`, as reported below.
Geometry-only correctness remains separately recoverable at `a7be751e`; no
individual timing benefit is attributed to either change without an isolated
comparison.


### Combined geometry/cell-sharing release: measured, not accepted at 5 Hz

The exact clean production candidate is
`fe010f83748223f6e377918939ddfb054dfc4fa6`, tree
`66db2ceeca34fea7492c799c0ba4b874018aaa09`. All four release executables were
frozen with source/build receipts and SHA-256 hashes. An interrupted build was
safely resumed against verified unchanged inputs and available Cargo locks; its
last successful segment took 140.735 seconds. That is not the total compilation
time of the interrupted build. The previously approved sequential-cache exception
remains explicit; it does not qualify the default isolated-cache build recipe.

Two balanced interleaved rounds (ABBA, 16 player cycles per side) compared the
unchanged `9fe8b6d8` harness on production baseline `279ba9d3` against `fe010f83`:

| Player-cycle metric | Baseline | Combined head |
|---|---:|---:|
| p50 | 2,980.4 ms | 649.5 ms |
| p95 / maximum | 3,460.3 ms | 942.3 ms |

The median fell 78.2%, and p95/maximum fell 72.8% **in this small cohort**. These
are not stable tail estimates or an SLA. Every eight-cycle sample retained
1,100 AI executions, 720 resolved strikes, 100 meaningfully active AI, 20 active
regions and exact within-run save/replay. Each sample passed its own-tree
validator and the stronger current activity validator. Geometry-only and
cell-sharing gains are not separately identified by this combined comparison.

A separate direct eight-cycle diagnostic had p50/p95 of 868.742/1,333.213 ms.
Its exclusive command-phase totals were simulation 2,771.739 ms, perception
2,008.412 ms, region transition 799.524 ms and publication 718.932 ms. Simulation
includes AI choice, pathfinding and action execution; the separate 9.354 ms
navigation phase measures navigation-memory refresh, not AI pathfinding.
Counters showed zero decision-scene rebuilds and zero observation-cell copies,
with 88 raw World-fact builds. `actor_table_copies = 111,908` counts actor
payloads copied, **not** that many whole-table detach events.

The matching stock headless smoke passed correctness, explicit save, source-free
restart and process cleanup. Nevertheless, its 16 paced moves took 14.139 seconds:

| Actual-process boundary | p50 | p95 |
|---|---:|---:|
| Request to authoritative outcome | 49.239 ms | 53.456 ms |
| Request to next-ready state | 815.132 ms | 1,807.289 ms |

Completed rate was **1.132 Hz**, against 5 Hz offered. The driver accumulated
52 unsent fixed-deadline slots; this is not a measurement of server mailbox depth.
The four-move burst achieved 1.244 Hz. Source-free restart reached the same state
in 6.657 seconds. Fast individual outcomes therefore do not establish responsive
successive movement. No full-duration throughput, bounded-memory-growth or
final-main integration acceptance is claimed. Earlier 0.472 Hz and 0.908 Hz runs
remain separate historical cohorts, not matched incremental speedup estimates.

Focused qualification for the combined candidate includes 162 simulation methods
and selected documentation/report/comparison, formatting and architecture checks.
The earlier geometry-only checkpoint also passed selected integration methods; those
are separately qualified evidence, not a fresh integration run for every later
edit. Required final-head hosted CI remains necessary before any eventual merge.

### Nested diagnostics: identifying the remaining critical path

Sampling with GNU gprofng produced inconsistent CPU accounting relative to process
resource usage. Those samples are retained but are not used as quantitative
function budgets or evidence of CPU throttling. Instead, the benchmark gained
opt-in nested timing. Simulation emits clock-free thread-local span notifications;
the benchmark owns wall-clock reads, accumulation and output. Notifications and
timings do not enter game decisions, protocol records or saves.

This diagnostic source was frozen as an exact working-tree snapshot and then
committed unchanged as `45c723dc435d21f0e46b9dbe4d1138f42c8b8e77`, tree
`c565cf66f3e4930c0359873308e36d901c7a3f06`. Only `active_mob_bench` was rebuilt
for this checkpoint, using the same release profile; its SHA-256 is
`7176af9d718f626d185b3d0d9489f40c032a6aa3379f170593b53b1ce94e36d2`.
The server/headless results above belong to `fe010f83`, not this later executable.
The diagnostic build completed with unchanged inputs in 1,275.799 seconds.
Three focused utility tests covered notification nesting, thread isolation,
unwind restoration and deterministic inclusive/exclusive arithmetic. Scoped
Clippy, formatting and architecture checks also passed; this is not full CI.

Eight-cycle observer-disabled and observer-enabled runs both passed the activity
validator and their own save/replay checks. All activity records and work counts
matched. Cross-run public-state hashes did not match because they include
run-specific references; they are not canonical cross-run equivalence evidence.
Disabled workload time was 7,163.780 ms and enabled time 7,723.419 ms. This single
sequential pair is not a stable overhead estimate. Instrumented timings include
callback, clock and aggregation overhead and are diagnostic rather than
acceptance measurements.

The scope is the measured main-thread loop, including harness queries, excluding
setup, explicit save/replay and worker-thread work:

| Span | Calls | Inclusive ms | Exclusive ms | Exclusive / instrumented workload |
|---|---:|---:|---:|---:|
| Rich observation projection | 113,728 | 1,707.923 | 1,707.923 | 22.1% |
| Preparation reconciliation | 1,124 | 1,085.320 | 1,068.447 | 13.8% |
| Control-contact reconciliation | 1,124 | 853.280 | 847.083 | 11.0% |
| Region pins | 3,324 | 545.022 | 545.022 | 7.1% |
| Awareness reconciliation | 1,124 | 352.374 | 349.392 | 4.5% |
| Scheduler selection | 124,064 | 337.940 | 337.940 | 4.4% |
| Revision-view wrapper | 111,908 | 2,144.611 | 211.507 | 2.7% |
| AI choice | 1,108 | 232.780 | 196.584 | 2.5% |
| Actor-table write helper | 172,796 | 168.732 | 168.732 | 2.2% |
| Action effect | 1,108 | 216.246 | 57.673 | 0.7% |
| Commit action | 1,108 | 2,598.465 | 55.555 | 0.7% |
| Finish action | 1,108 | 1,142.057 | 18.828 | 0.2% |

Inclusive values overlap descendants and must not be added. Exclusive values
subtract instrumented children; their sum is 5,564.686 ms, not the entire workload.
They must not be added to the command-phase totals above. Scheduler and pin spans
can include calls outside the similarly named command-profile buckets.
`actor_table_write` measures access/detachment inside the helper, not all later
mutation bookkeeping. Its call count is not a detach count. AI choice includes
choice/pathfinding work at that boundary, not the whole cost of an AI turn.

These measurements favor eliminating population-wide repeated work. A
pathfinding-only rewrite targets a small measured fraction; a wholesale
per-actor-storage rewrite is not justified first by copied-payload counts alone.
Readiness batching is useful but cannot alone bridge the gap to 5 Hz. The
remaining real-process gap requires measured structural gains,
not a promise based on summing hypothetical savings.

### Owner facts and exact no-op work: measured, target still not met

Commit `53854bca044b6ac9ef20ddce976276229121d20f` contains three related changes:

1. `OwnStatsFacts` captures an immutable effective-sheet handle plus fresh dynamic
   owner facts. Equality selects exactly the 22 legacy `OwnStats` fields, not the
   entire hidden derived creature. Full owned collections materialize only when
   demanded by the consumer. This avoids repeated personal-inspection expansion
   without suppressing observation, knowledge or private-event work.
2. Control-contact reconciliation recognizes exact absence of relevant ground
   scare items, while retaining recipient eligibility, body validation and stale
   contact cleanup. On the present-item path it avoids mutable actor guards when
   the resulting control state is unchanged. Earned flee ownership and RNG order
   remain independent of contact receipts.
3. A short-lived readiness context reuses one settled scheduler answer across
   observer projection loops whose receipt/navigation updates cannot change
   scheduler dependencies. It is not a persistent readiness cache or a change
   to ready-time/actor-ID tie ordering.

These changes passed 59 focused methods (25 simulation library, 26 server library,
and 8 simulation integration), plus no-link Clippy and architecture checks. Four
new regressions reached intended old-code failures, then passed: owned-stat
expansions, absent-feature neighbor probes, unchanged-contact actor copies and
per-observer scheduler rescans. Independent eager projection, retained/fresh
snapshots, hidden-field equality, wire/delta privacy and replay/rollback witnesses
remain covered. One new fixture assumption and one missed existing test API
migration were corrected and are recorded in the test-effectiveness ledger.
These are selected local checks, not full CI. The optimized four-target build
completed with unchanged source inputs; source/toolchain-qualified binaries were
frozen independently before measurement. An earlier interrupted invocation had no
terminal result and is not counted as a test failure or successful build.

The direct eight-cycle cohort retained 1,100 AI executions and 720 strikes,
exact replay and a durable barrier: cycle p50/max 575.288/697.631 ms, simulation
1,564.159 ms total and projection 1,359.041 ms total. Owned-stat materializations
and absent-feature neighbor probes were zero. Matched ABBA against the unchanged
baseline (16 cycles per side) measured baseline p50/p95 2,506.409/2,662.189 ms
versus head 553.130/656.677 ms: 77.9% median and 75.3% cohort-tail reductions,
not a stable latency guarantee. All four activity/replay validations passed.

The stock headless smoke completed 16 moves in 9.367 seconds (**1.708 Hz**),
with next-ready p50/p95 588.110/628.202 ms and authoritative-outcome p95
51.921 ms. Four burst moves achieved 1.833 Hz. Source-free restart took 6.509
seconds and restored equal state; cleanup passed. There were 28 unsent fixed-rate
driver deadlines, not measured server-mailbox entries. **5 Hz remains failed**;
a short smoke is not sustained qualification. The previous 1.132 Hz result is a
separate cohort, not a matched throughput ratio.

Ordinary ABBA guardrails also validated. r8-a1 head p95/max were 2.909/13.707 ms
versus baseline 1.852/12.427 ms: the tail worsened in this cohort, with head
round-p95 values spanning 1.600–3.813 ms. r64-a8 head p95/max were 2.720/31.368 ms
versus baseline 3.773/107.583 ms. Both retained the existing budgets; variation
and the r8 tail worsening are not discarded.

One diagnostic eight-cycle run measured exclusive projection 1,420.386 ms,
preparations 920.485 ms, pins 482.187 ms, awareness 324.296 ms, scheduler
49.093 ms and contacts 38.314 ms. These host-owned diagnostic spans include
instrumentation overhead. Inclusive spans overlap and must not be summed.

### Witnessed component batch: measured, target still not met

Commit `41b8e4987bbe8c8f11b1c8ca6121b4dcce651939` reduces copies and repeated derived queries without
suppressing any actor or observer:

- The preparation resolver borrows non-due and paused work. Only due work owns a
  pre-settlement snapshot, preserving time-settling rechecks, validation and
  interruption order, overflow, equal-offset strikes and paid-prefix ownership.
- Geometric reach and perception-load regions reuse the existing scene's exact
  world/illumination/emitter/body/chart/sense witness. Missing or mismatched scenes
  compute fresh values. Live holds, pending targets, falls, item claims, roots and
  fixed-point settlement remain current; settled region sets are never cached.
- A shared immutable classification component uses the existing visibility epoch
  inside an exact scene. The canonical classifier is unchanged. Rich health,
  timing, hostility, events, items, equipment and cosmetic facts are still fresh;
  arbitrary placement/sight queries stay on the uncached path. Rejected branches
  cannot upgrade or poison retained classifications.

`observe_event_shared_scene` consumes scene membership and actor classifications
for proof, but the server also needs the complete observation for exact revision
comparison, map knowledge and publication. A proof-only API would not by itself
remove that work and could duplicate classification. Share the canonical component
instead; preserve every event-time receipt before sealing. Missing historical
receipts cannot safely be reconstructed later.

Skipping full projections additionally requires **exact component dependency
proofs**, not proximity heuristics. Start with cheap per-observer checks and
component reuse before introducing a reverse index. A bounded index would need
old and new body/item locations, portal-transformed sight, non-primary senses,
hearing, owner knowledge/conditions, illumination, freeze/resume and whole-scope
charts. Uncertain or global changes must fall back to full projection. A dirty
candidate is recomputed and exactly compared; blindly bumping revisions could
expose hidden changes and invalidate otherwise-valid commands. Neither connected
client status nor same-region membership is a safe observer filter.

Preparation work has a similar boundary. A due-only queue is incorrect:
displacement, lost dependencies and new threats must cancel or suspend plans
before their completion time. An ordered pending-owner index can remove empty
entries, but must retain actors gaining a plan earlier in the same ordered pass,
initial actor membership, equal-offset strike ordering and residency handoffs.
Further savings need plan-specific dependency witnesses for owner/target/item,
reservation, geometry and sensory facts, while retaining deadline and displacement
checks. Keep special Escape/Respec/ray and item-threat cases on the canonical path
until their exact dependency sets are established. Measure the next sub-boundary
rather than assuming all preparation time is removable.

Protocol and data structures may change when a cleaner authoritative boundary
justifies the integration and format costs. Stable privacy, deterministic
ordering, recovery and observable gameplay semantics are requirements; current
DTO layout and accidental coupling are not. Final measurement must integrate
current main in a separate worktree so the pinned baseline's compiled runtime
inputs remain intact. Existing ordinary-action guardrails and the full real-client
100-AI/20-region/5-Hz acceptance workload remain unchanged.

### Evidence and publication scope

The source identities, workload counts and aggregate results above are the
portable review record. Exact task-local build receipts, binary hashes, raw runs
and validation outputs are retained separately, not presented as repository links.
Publication of private raw data or release artifacts is a separate decision.


The witnessed component checkpoint passed 78 focused Rust methods and documentation,
architecture and formatting checks. Its four optimized targets were frozen from the
same unchanged source. All direct activity/replay validators passed: 100 genuinely
active AI, 20 regions, 1,100 AI executions and 720 resolved strikes per eight moves.
Matched balanced cohorts (16 cycles per side) gave baseline p50/p95
2,633.234/4,031.384 ms versus 466.771/634.032 ms. These small-cohort tails are not an SLA.

The real stock headless client completed 16 moves in 7.917816 seconds (2.020759 Hz).
Next-ready p50/p95 was 490.799/522.379 ms, compared with outcome p95 53.862 ms.
Source-free restart took 6,819.012 ms and preserved equality; cleanup passed.
There were 22 unsent fixed-deadline slots, not a measured server mailbox depth.
Burst throughput was 1.956547 Hz. The target remains unmet.
Ordinary current-head p95/max was 1.599/11.274 ms (8 regions, one actor) and
2.530/23.015 ms (64 regions, eight actors), both within existing budgets.

Exclusive instrumented totals across eight moves were projection 1,044.988 ms,
preparations 729.112 ms, pins 285.937 ms and awareness 206.582 ms. Inclusive spans
overlap; instrumentation adds overhead. Preparation checks still visited 113,524
candidates despite owning only 720 due snapshots. This supports removing redundant
physical reach construction rather than reducing AI activity.

### Next bounded batch: canonical contact endpoints and absent ground work

Reuse the existing canonical physical contact endpoint enumerator under the exact
source scene and nonowning World identity, with one current payload per bounded
scene. Resolve the target body freshly and intersect it with those endpoints. Keep
living combat-presence gates, caller freeze checks, plan/hold validation and target
movement current. Arbitrary scenes, placements and selected hit-direction witnesses
retain their existing path; there is no second route algorithm or per-target matrix.

When the authoritative ground-item index is empty, bypass visible-cell ground
lookups but still enumerate carried inventory. This needs no new index or epoch.
Neither change has a measured speedup yet. Duplicate disclosed-event construction
is a possible later responsibility refactor, not a reason to expose incomplete
observations or postpone event-time authority.

### Per-actor snapshot ownership

Actor snapshots keep a shared, deterministically ordered table of shared actor
payloads. Read APIs continue to return borrowed `Actor` values, not ownership
handles, and `ActorEdit` retains the same drop-time region, timer, body and
visibility invalidation rules. A retained candidate/history snapshot no longer
requires copying every unrelated actor's payload when one actor is edited.
Replacement and removal still return owned Actors; a shared returned payload is
copied once, while a unique payload is moved. Checkpoint adapters continue to
capture existing `ActorRecord` values. No protocol or save format changes.

The outer ordered map still copies O(actor-count) **handles** when detached;
this is not a persistent-tree or zero-copy-map claim. Diagnostics distinguish
`actor_table_entries_copied` from the historically named `actor_table_copies`
(actual payload clones). Weak-only identity disassociation copies neither.
`actor_table_write`, `actor_payload_write` and `actor_payload_return` are narrow
host-timed diagnostic seams, not the complete cost of a mutation. No wall clock
is read by simulation and no counter/cache identity affects saved gameplay.

The first-red ownership witness observed three payload clones for one edit.
The focused checkpoint now passes 29 unique methods (19 simulation library, six
server library, four simulation integration), including that causal bound,
independent retained snapshot values, weak identities, owned returns, indexed
mutation, visibility, checkpoint, revision, rewind and persistence-rejected
candidates. Clippy preflight passed. These local checks do not substitute for
hosted CI. The ownership batch stock-client smoke was 2.115 moves/sec, versus
2.187 for the previous phase9 smoke, both below the requested 5/sec.
The clean matched comparison round improved total workload by about 2.8%;
no robust real-client throughput gain was established. Payload copies fell
from 111,908 to 44,082 while ordered-map handle copies remained 111,908. A causal allocation/copy reduction is not sufficient evidence that the
throughput or ordinary tail-latency target is met.


### Change-driven commit boundaries

The next batch separates commit-time event authority from rich client views.
An opaque `CommitEventFacts` captures the canonical scene and actor references;
its shared validator preserves owner, tick, event-time visibility and frozen
receipt authority. Committed disclosed history supplies recognition knowledge
without constructing terrain, inventory and combat DTOs just to discard them.

A borrowed `BoundaryEffects` compares immutable old/current games once and
classifies each observer conservatively. Proven changes (tick, location, or
readiness) advance the display revision without a rich equality comparison.
Proven unchanged observers retain their revision. Changed shared roots,
ambiguous dependencies and active charts take the canonical full-view fallback.
Dependencies include apparent/concealed scene bodies, exact disclosed histories,
map-memory roots and local physics. Fast paths retain fallible projection-input
validation. No event-time privacy, navigation ordering, candidate rollback,
replay, save format or protocol semantics change in this internal batch.

Awareness still performs its required rescans and contests, but identical contact
assignments no longer detach an actor payload and falsely invalidate dependents.
Preparation diagnostics now separate owner visits from actual validation and
residency evaluations; this batch does not claim to eliminate the owner scan.

The causal server witness first passed observation/revision/replay equality and
then failed at 100 rich views for one local move. It now satisfies the affected
observer bound (fewer than 50), while the map-adapter fixed-point witness and
rollback/streaming checks pass. The focused checkpoint covers 68 unique methods
(13 server, 55 simulation), plus production Clippy. These are local correctness results, not hosted qualification.

The four fixed release binaries were built from unchanged source. The isolated
phase10-to-phase11 balanced comparison (16 cycles per side) improved player-cycle
p50 from 414.894 to 288.595 ms and p95/max from 478.296 to 393.220 ms. The real
stock-client smoke completed 16 moves in 5.448 seconds (2.937 Hz), versus the
prior 2.115 Hz smoke. Next-ready p50/p95 was 291.609/813.965 ms; ten unsent
driver deadlines remained. Source-free restart took 5,557.496 ms with equality.
Canonical execution traces match at 2,236 measured and 2,800 final executions.
The 100-AI/20-region/5-Hz target remains unmet.

Rich observations fell from 113,728 to 9,588; payload copies from 44,082 to
2,758. Classification work increased (8,888 to 43,560), which remains visible
rather than being hidden behind the rich-view reduction. Across the matched
phase11 rounds, simulation took 739/744 ms, region transitions 339/360 ms,
revision proofs 244/260 ms and perception 309/331 ms per eight moves. A separate
instrumented run attributed 252.769 ms exclusive to preparation settlement and
269.616 ms to region pins. Those spans include profiling overhead and are not
added to the uninstrumented timings. The ordinary-action validator now requires complete typed boundary-proof
coverage and bounded materialization, retaining legacy checks for old profiles.
All eight retained raw runs revalidated without rebuilding or rerunning.
Head authoritative p95/max was 1.646/3.219 ms for eight regions/one actor and
2.613/32.943 ms for 64 regions/eight actors, within the existing budgets.
The larger maximum is close to its 33 ms limit; this small cohort is not an SLA.


### Preload attribution is a separate diagnostic

The default active-mob direct benchmark does not start speculative preloading;
the real server does. Its publication envelope therefore cannot establish
preload planning cost. `TOR_PROFILE_PRELOADING=1` requires
`TOR_PROFILE_SPANS=1` and enables a separately labeled direct diagnostic. It
reports planning and submission spans plus expanded-region/link/job counts.
The standard workload validator rejects this mode as baseline qualification.
Do not interpret its timing as real-headless latency or silently mix it into
frozen default comparisons. The diagnostic does not change preload policy:
worker demand, retries and required synchronous region acquisition remain intact.


## Causal records and demand-driven presentation (phase21)

The user approved separating ordered actions/events from visual snapshots.
Rules, RNG, knowledge acquisition, visibility receipts, cancellation, save/replay
and rewind retain their authoritative boundaries. No offline delivery guarantee
or actor/AI throttling is introduced.

The Engine no longer maintains a per-actor display-revision map or compares every
actor's rich picture after each action. Private backend freshness uses a branch
and causal journal boundary. This private ordinal is never a public picture ID.
Attached observers still receive role-filtered, commit-time semantic facts;
English narration remains client-owned. Reliable ordered outcome messages retain
existing bounded-queue backpressure. Empty hidden-only changes produce no outcome
or public global-activity counter.

Pictures are requested through nonresetting `Observe`. Each admitted demand
receives a fresh random snapshot capability and attachment-local display generation,
even if the picture is unchanged. Clients coalesce picture demand independently of
ordered event delivery. `InputContext` remains command authority, not a picture
version. Notes bind a requested capability to its private durable action anchor;
raw requested identity remains available for exactly-once retries before capability
lookup. Pages and read-only inspections use the issued immutable boundary, not a
live state mislabeled with an older snapshot. Reset, branch or access changes
invalidate capabilities; bounded retention can expire old ones explicitly.

This is an intentional protocol66/archive64 migration. Historical files using
older epochs are not silently reinterpreted. Optional legacy observation event
fields remain structurally representable during this migration but are never
emitted and are rejected by the new client: ordered outcomes alone own narration
and action history. Frozen release, real headless and recovery results are reported below, with
their workload and current-main integration limits.

The pre-PR cleanup makes ownership explicit rather than retaining compatibility
wrappers: live `ClientSessionState` separates stream authority from its immutable
picture, and reset export is validated/fallible. `PresentationHistory` retains
bounded scoped capabilities, frozen Game boundaries and small chart summaries;
only the latest picture is kept for deltas, and admission commits registry state.
Display-bound commands have an explicit decoded stage before a private causal
guard exists. Named event provenance replaces tuple conventions; unused revision
readiness/proof storage and ineffective cross-client whole-picture scans are gone.
Observe writes share a read-first, cancellation-safe pump with background queries;
causal-only outcomes do not request pictures. Pause obligations remain owned until
capture and suspension succeed. Projection delivery failures close only affected
streams and do not overwrite persistence warnings. Prose renders typed facts
rather than depending on parallel line-array positions.

The cleanup's focused local run passed 63 selected methods after correcting the
recorded fixture issues, including real TCP blocked-flush delivery, two-actor
pause retry, foreign-self rejection, reset export, capability retention/admission,
exact cold event knowledge, frozen pages and narration. Static checks passed for
the affected targets. This is expedited local evidence, not a full CI claim;
the later compact measurement and current-main qualification are reported below.

### Compact semantic capture: measured follow-up

The first frozen display/causal split (`836ceaf2`) completed its real headless
check at 2.230 moves/second, below the 5 Hz target. Source attribution found that
Service still adapted complete protocol pictures before and after AI admissions
and executions just to derive narration. This was not a duplicate Observe retry:
16 moves produced 48 dirty outcomes and 48 pictures, including legitimate
recovery transitions. The original direct recovery outlier was 63.729 seconds;
a frozen repeat was 8.719 seconds. That tail remains unexplained, not dismissed.

The retained follow-up introduces a non-wire `OutcomeObservation`: explicit,
minimal narration inputs, adapted directly from immutable simulation facts.
Full and compact capture share one narration algorithm. A conservative borrowed
picture-input comparison proves equality only; every uncertainty falls back to
canonical role-filtered pictures. Hidden-state differences alone never publish
a dirty signal. Fallback pairs are transient and shared once per observing actor,
not rebuilt per recipient. No persistent proof cache or wire revision is added.

A sequential frozen ABBA comparison used the same headless client, seed 42,
100 active living AI actors, 20 portal-connected regions, and 16 paced moves per
run at an offered 5 Hz. No builds overlapped measurements:

| Run | Variant | Completed moves/s | Next-ready p50/max (ms) | Unsent deadline slots |
| --- | --- | ---: | ---: | ---: |
| A1 | Original display split | 2.058 | 427 / 803 | 21 |
| B1 | Compact capture | 3.026 | 321 / 398 | 9 |
| B2 | Compact capture | 3.235 | 302 / 378 | 8 |
| A2 | Original display split | 1.864 | 445 / 993 | 25 |

Pooled throughput is 1.956 versus 3.127 moves/s, about 60% higher in this small
smoke cohort. This supports retaining the clearer separation, not a sustained-load
or 5 Hz qualification. It does not establish improvement over pre-display phase20.
Every run preserved the same 2,236-entry execution-trace digest, 16 player moves,
two portal crossings and save/restart disclosed-state equality. Compact restart
was 5.849/5.118 seconds; original was 5.738/5.380 seconds. Cleanup completed in all
runs. The direct eight-move compact proof retained exact replay and all activity
contracts; workload was 2.071 seconds and resume 6.880 seconds (not a matched
simulation speedup claim).

The compact source passed four reused protocol and thirteen fresh server methods,
scoped Clippy, and a bounded optimized server/benchmark build. The fan-out witness
checks zero or exactly two full adaptations for 1, 8 and 32 watchers; cold
canonical inputs and event/privacy oracles remain. Twenty pure driver tests cover
the separately recorded stale-picture harness correction. The local evidence bundle retains raw ABBA results, source and binary hashes,
the frozen patch/untracked sources and the aggregate summary; the measured values
and limitations needed for review are reproduced above.
The new server and direct benchmark were rebuilt; the unchanged wire66 headless
client was deliberately reused from `836ceaf2`, not represented as a new build.

### Current-main integration qualification

The integration uses a new worktree based on `cf7c4498`; frozen performance
artifacts and their original source remain unchanged. Scoped Clippy passed for
server, protocol, simulation/world and shared/Text/ASCII/headless client targets.
Four real-process binaries were built once and reused for the genuine wire
recorder. It completed all lifecycle cases and captured 73 server samples and
nine client queries, alongside 53 client schema examples. These include paid
items, Detection/Mapping source-free restore, polymorph, ordered outcomes and
fragmented snapshots. The current archive fixture came from the production
writer, not a relabeled older file. The unpublished intermediate wire fixture
was omitted from the PR; the final recording and main's historical fixtures remain.

Focused current-main checks passed: seven archive/causal writer checks, ten
archive/schema/queue-lease checks, 17 wire sample tests, six strict action/readiness
codec tests, nine recovery tests, two codec tests, the exact Text staff regression,
two wire benchmark contract tests and all 50 no-GPU 3D model tests. Pure recorder
and harness witnesses cover the changed test machinery. The zero-test selector
mistake was rejected, corrected from the binary's registered list and rerun without
rebuilding; it is not counted as a pass. Failed recorder fixtures and two disk-guard
interruptions are classified in the test-effectiveness ledger.

This is selected, risk-based local qualification, not an exhaustive suite or a
new integrated-main performance measurement. Full native 3D renderer/UI and the
required exact-head hosted CI aggregates remain unqualified here. No further
speculative optimization is planned for this PR.
