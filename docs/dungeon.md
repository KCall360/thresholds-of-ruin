# Dungeon gameplay — milestone 4d

Merged in PR #35 at `2fb5169`, after Windows and Linux CI passed on final head
`f887435`. Region streaming and generation (4e) have started with structural
preload planning; see [region streaming foundations](region-streaming.md).

## Rules

The first authored dungeon provides exploration, melee combat, retrieval of a
unique item, and escape. Equipment, healing, ranged combat, item effects,
opportunities/complications, and procedural generation are deferred.

Actors use one authored attack: d20 plus bonus meets physical defense. Natural
1 and 20 have no automatic outcomes. Damage has independent energy, impact, keen,
spirit, and vital components. Each applies its own immunity or flat reduction,
down to zero. Damage is fixed rather than rolled.

Attacks prepare for an authored wind-up before resolving against current reach
and perception, then recover. Positive HP loss interrupts preparation and travel;
fully resisted blows do not. Still-valid interrupted progress resumes with the
same attack, waiting preserves it, and movement or another action discards it.
Loss of validity costs spent time without additional recovery. Hits and misses
both charge recovery.

Impact damage per obstructed component is
`ceil(max(0, abs(incoming_velocity) - 2048) / 512)`, using fixed-point velocity
units. Only the moving actor takes damage. Momentum transfer and item damage
remain deferred.

Scenario factions have directed hostility. Search/attack/flee AI uses its own
perception and remembered targets, with default 1,000-tick memory and a 25% HP
flee threshold. Optional starting characters may use scenario-selected AI.

Death leaves inventory and a non-stackable corpse at the actor's base cell.
Death of the selected human character ends the run and freezes simulation time;
its last disclosed view, history, and save remain available. Any living starting
character may satisfy an objective at its named anchor, optionally carrying its
unique authored item. Objective visibility and continued play after victory are
scenario settings.

## Controls and disclosure

Text uses `attack <name>` or `attack #id`, with clarification for ambiguous names.
ASCII uses A to select an actor, Up/Down to choose, and Enter to attack.
`--bump-attacks hostile|any|off` controls manual movement interpretation; the
default is hostile. Simulation movement remains distinct from attacking, and
travel never attacks automatically.

Clients receive exact own HP and preparation/recovery, qualitative visible enemy
injuries, and disclosed combat narration. Enemy numerical statistics, AI memory,
combat RNG, hidden identities, and hidden locations remain backend-only.

## Authored content and scheduling

`scenarios/first-dungeon` is the ordinary launch default. Five chambers connect
along a main route with a shortcut. Recover the dawn seal in the far chamber and
return to the entrance. The scout, guardian, and wisp have different wind-up,
recovery, defenses, and damage. No healing is available. Diagnostic packages may
omit combat attributes; the authored dungeon configures every participating actor.

Simulation actions are deterministic and ordered by readiness time and actor ID.
An explicit human input boundary takes priority over autonomous actors at the
same tick, including a selected character whose ID sorts after an AI. Attacks
revalidate current perception and reach when their wind-up finishes. Melee reach
uses occupied body cells and conservative diagonal/vertical portal traversal;
all cardinal traversal orders must agree and remain unobstructed.

The server executes at most one autonomous action per pump, leaving delivery and
cancellation opportunities between actions. Disconnect, release, restart, and
rewind suspend human preparation and stop autonomous continuation. Resubmit the
same attack to resume preserved progress. During recovery, text `wait` or ASCII
Space requests continuation without inventing a second simulation action.

Death drops carried items and one corpse at the base cell, retains the actor ID
for history/save references, and removes the actor from living occupancy and
scheduling. Objectives are evaluated at committed action/physics boundaries,
so victory need not advance the clock. Ordinary monsters cannot win; any living
starting character can, including a scenario-controlled optional companion.

## Compatibility and verification

The implementation uses protocol 16, save format 11, ruleset `dungeon-v16`, and
validator `tor-scenario-4`. Previous prerelease saves are rejected. Scenario
certificates must be regenerated after package source edits.

Focused simulation boundary tests, the complete authored dungeon walkthrough,
and durable preparation/victory/death restart tests pass. Four actual-client
acceptance tests pass, including the full default dungeon in native ASCII.
The Rust workspace passed 307 tests in each of debug and release, with none
ignored. Formatting, all-target Clippy, private rustdoc with warnings denied,
architecture checks, and all 28 package certificates pass. Python debug discovery
and the full release process suite passed with the environment retries described
in the [handoff](session-handoff.md); final dungeon tests passed in both profiles.
The three desktop launchers connect, retain fresh saves, and clean up their owned
processes. Native target selection, victory, death, and controls were inspected.
Performance findings and limitations follow; Windows/Linux CI passed before merge.

## Local performance evidence — 2026-09-27

Release builds on the same Windows machine, with no build or process-test work
running concurrently. Combat workload v1 has three samples per case and 64
measured commands per sample; 0 or 1,000 prior commands exercise history scaling.
The engine is profiled, and reporting, update construction, save barriers, and
restart are outside command timing. Client drawing measures the software canvas,
not native presentation. See the [harness guide](performance-harness.md#combat-workload-v1).

| Actors / prior commands | Commands | p50 ms | p95 ms | Maximum ms |
|---|---:|---:|---:|---:|
| 2 / 0 | 192 | 2.752 | 3.855 | 7.022 |
| 2 / 1000 | 192 | 2.713 | 3.690 | 6.985 |
| 8 / 0 | 192 | 7.994 | 11.013 | 15.759 |
| 8 / 1000 | 192 | 7.428 | 11.650 | 12.280 |

Two-actor combat remains within the provisional 8 ms p95 / 33 ms maximum targets.
Eight-actor combat exceeds the p95 target. Before/after observation construction
for every actor accounts for about 5.1–5.4 ms per command on average in those
cases; simulation contributes about 1.8–2.4 ms and navigation about 0.5–1.0 ms.
Removing duplicate same-tick validation, using visibility-only targeting, and
skipping unchanged navigation lowered cost without changing disclosure or rules.
The remaining all-actor observation cost is recorded in deferred 3p; no target
has been relaxed and this result does not claim 3p completion.

| Actors / history | Decision p95 ms | Apply p95 ms | Draw p95 ms | Save p95 ms | Resume p95 ms |
|---|---:|---:|---:|---:|---:|
| 2 / 0 | 0.937 | 0.325 | 0.995 | 1714.854 | 195.867 |
| 2 / 1000 | 0.931 | 0.311 | 0.881 | 1796.318 | 191.291 |
| 8 / 0 | 0.945 | 0.360 | 0.981 | 236.519 | 555.101 |
| 8 / 1000 | 0.944 | 0.304 | 0.988 | 1755.032 | 546.355 |

| Actors / history | Scene calls | Body-cell requests | Disclosed bytes | Saved bytes |
|---|---:|---:|---:|---:|
| 2 / 0 | 722 | 3396 | 35489 | 65536 |
| 2 / 1000 | 12058 | 56738 | 35520 | 1769472 |
| 8 / 0 | 1598 | 28460 | 36414 | 65536 |
| 8 / 1000 | 28914 | 438918 | 36418 | 3067904 |

Counters include history construction plus the measured command window. Client
application/draw samples occur only on disclosed revisions: the eight-actor
cases have fewer client updates than engine commands. Save/resume have only three
samples each; their longer explicit barriers are separate from ordinary command
latency. This matrix does not establish 10,000-action scaling or network latency.

The long-history run exposed invalid JSON encoding of AI visit locations at
checkpoints. The corrected ordered-entry encoding is covered by a forced-checkpoint
restart regression. Failed and pre-optimization runs remain under `.local/4d-*`.
Accepted combat evidence: `.local/4d-combat-navigation.jsonl` and
`.local/4d-combat-navigation-summary.json`.


### Matching ordinary-play baseline

The preserved 4c release binary (`c8efdd5`) and final 4d build ran the same
in-memory `latency_bench` cases on this host: `r8-a1-h100-memory --quick --cycles 5`
and `r64-a8-h100-memory --cycles 3`. This comparison keeps the existing workload
meaning; combat is measured separately above. Values are p50 / p95 / maximum ms.

| Case | Phase | Samples | 4c | 4d |
|---|---|---:|---:|---:|
| small | command call | 305 | 0.339 / 0.946 / 1.702 | 0.344 / 0.964 / 1.395 |
| small | client application | 305 | 0.123 / 0.173 / 0.309 | 0.132 / 0.197 / 0.411 |
| small | rendering | 305 | 0.663 / 0.954 / 1.541 | 0.715 / 1.175 / 1.516 |
| large | command call | 1503 | 0.016 / 3.389 / 11.562 | 0.018 / 3.410 / 5.131 |
| large | client application | 430 | 0.128 / 0.195 / 0.467 | 0.124 / 0.191 / 0.604 |
| large | rendering | 430 | 0.864 / 1.156 / 1.888 | 0.843 / 1.121 / 1.616 |

Command p95 is essentially unchanged and remains within the provisional targets.
Small-case draw p95 rose by about 0.22 ms; large-case draw p95 decreased slightly.
Neither case shows a material responsiveness regression. The large mixed workload
includes rejected actions, explaining its low median; client counts include only
applied updates. These cases are in memory and do not measure disk durability or
native presentation. Disclosed command bytes and operation counts are retained in
the raw profiles; disk bytes and serialized records are zero in both builds.

| Case / version | Scene calls | Perception calls | Actors observed | Transitions |
|---|---:|---:|---:|---:|
| small / 4c | 640 | 640 | 600 | 305 |
| small / 4d | 640 | 640 | 600 | 305 |
| large / 4c | 5912 | 5912 | 5888 | 1503 |
| large / 4d | 5912 | 5912 | 5888 | 1503 |

Evidence: `.local/4d-complete-{before,after}-{small,large}.jsonl` and
`.local/4d-complete-comparison.json`; baseline binary hashes are retained in
`.local/4d-baseline/hashes.json`. The selected cases do not establish the deferred
full 3p matrix or large-world streaming performance.
