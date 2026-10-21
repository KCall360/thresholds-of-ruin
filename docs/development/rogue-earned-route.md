# Bounded ordinary earned-route qualification, revision 6

Status: experimental tooling with explicit acceptance gates. A run is complete
only when its ordinary, committed-flow, replay and source-identity checks pass. This is a fixed experimental controller, not a recommended
player strategy or a numerical balance assessment.

## Frozen cohort and inputs

The measurement pilot consists of exactly eight planned attempts, in this order:
production/Warrior/0, production/Warrior/1, production/Mage/0,
production/Mage/1, candidate/Warrior/0, candidate/Warrior/1,
candidate/Mage/0, candidate/Mage/1. Seeds are decimal 0 and 1. Every planned slot
exists in the inventory before the first launch, initially `unrun`. Adverse seeds
are never replaced. Seed 42 may be a separately labeled known-path acceptance
fixture, never a replacement or an independent rate sample.

The route arm named `production` is the explicit, frozen preactivation E/S source
snapshot at `8d09063840c51aa4189b146c287829102856b279`, with an explicitly recorded
adaptation for the current scenario format. Its ordinary population remains two
emus and two snakes. The source-only fixture
`scripts/fixtures/rogue-earned-baseline-v19` retains exact package inputs and its
current bundled-rule dependency hash. Its source-only transition advances the
previous scenario format and bundled rules import to the current identities.
All other authored package bytes, including anatomy, items, player kits and
population stages, are unchanged; no utility-magic or ring loot, discovery stages, traps or secret closures are
imported. Frozen v6–v18 inputs and their source/transition receipts remain immutable research
evidence. Shared hazards, levitation, discovery, wearable, utility-magic, persistent-condition and
action-duration rules
make this a new experiment input, with no claim of identical outcomes or a
requalified pilot. The source
receipt alone does not establish compiler validation, regenerated certificates
or route acceptance. Current route CI must still qualify the adapted fixture.
Certificate regeneration must select gameplay packages under `scenarios/` (and
the explicit client menagerie), excluding source-only comparator snapshots.
Validate comparators only in campaign copies: generated `index.json`,
`validation.json`, and `.compiled` files violate their exact source closure.
New campaigns have distinct source fingerprints and cannot resume an earlier
pilot with changed inputs. This is experimental input, not a historical save reader.
The arm name remains unchanged when the bundled package
selects twenty-four species: it always names this historical comparator, never the
current production manifest. Its independent gameplay fixture is unchanged and lacks the
complete Vampire catalog/rule closure needed as an eight-species generator source.
The retained pilot candidate is a separately frozen eight-species E/S/H/I/Z/Q/U/V
package with its declared finite bands, V-only open depth27+ tail, exact
`racial_after` depth26 growth and provisional carried-stock pool. Both arms use
the same compiler and authored depth-one player anchor. The normal adapter takes
explicit source snapshots and never silently copies newly activated production
into the old baseline arm. No weights, dispositions, kits, skills, supplies or
nutrition values are adjusted by measurement.

Earlier reports, controller dumps and frozen cohorts remain unchanged. Revision 6
retains the current canonical wide-scalar wire representation and live compiler
input fingerprint, and opens a disclosed closed entrance before attempting its
planned cardinal step. This revision creates a new campaign; it does not import
or migrate old controller memory. A saved game retains its embedded package, so
continuing an old E/S save is not evidence for the new Vampire arm.

Freeze before measurement: source tree/relevant input hashes, compiler/server/
headless executable hashes, compiled package file hashes, protocol/schema
versions, controller source/hash and this policy revision, seed, selected actual
actor (Warrior 1, Mage 2), cohort order and all bounds. Check hashes again after
execution. A drifted or partly generated cohort remains failed/incomplete.

## First purchase and first milestone

Assert the actual unspent authored HD-one start, future class, finite kit and
current nutrition. Select the actual character through the ordinary server
option; never renumber actors. Request owner advancement overview, then preview
and confirm exactly one existing-point purchase:

- Warrior: `purchase 1 advancement talent rattling_blow`.
- Mage: `purchase 1 advancement talent evocation_initiate form spark_lance_cold`.

Verify preview is nonmutating and confirmation spends exactly the existing one
point. Any refusal or unexpected starting state is a failed attempt, with trace.
No default Mage spell, granted XP, playerHD, extra talent/rank/attribute, wizard
command or saved-state edit is allowed. Existing authored future class remains.

The first milestone is the first settled ordinary state with a second earned HD,
verified by both owner XP/growth history and the actual two-HD history. Save that
state before issuing another gameplay action. It is not enough to see two active
HD after a granted edit. Stop the first segment there, or at death, disclosed-route
exhaustion, deterministic budget censoring, or an infrastructure/contract failure.

Continuation resumes exactly that surviving save and controller memory. This
revision makes no later purchases: later earned points remain unspent. Record
first ordinary entries to depths 5, 8, 11, 17, 26 and 27, retaining original cohort
attrition; depth 1 is the initial entry record. Stop at first entry to27. This records earned arrival and activation, not
Vampire combat survival; dedicated real-process tests cover its drain, recovery
and carried-loot mechanisms. Later
build policies require a new revision, not substitution of warrior_mixed. A
failed first milestone cannot silently become a deep-tail start.

## Deterministic decision policy

Use only the current ordinary observation, owner advancement pages and memory of
previously disclosed opaque cell keys. Private save contents, activation receipts,
flow diagnostics and unseen foes never choose actions. Preserve passive species;
ordinary player attacks can initiate contact. There is no AI disposition override.

At every settled state, refresh the disclosed graph. Stable direction priority is
west, east, north, south, matching DisclosedFloorGraph. Resolve actor ties by Manhattan ground distance, then disclosed relative x, y,
z, name, description and asset. Resolve ground-supply ties by disclosed relative
x/y/z and public item facts; carried food uses public item facts (known supply,
class, name, appearance, description, identified state and quantity). Indistinguishable
public keys cause an ordinary wait with an explicit ambiguity phase, never an
opaque-ID tie break. Opaque IDs are target handles and equality keys only.
Resolve cells by deterministic breadth-first traversal with that direction order. Only observed hostile faction relations identify visible foes.
Use server-disclosed available resources, supply quantities, ability costs,
cooldown and reach; never infer a hidden immunity from species name.

A selected cardinal step into a currently disclosed, reachable closed door first
issues one ordinary `set_door` opening instead. It consumes the same command
budget as other actions, does not advance the remembered position, and requires
a fresh settled observation before selecting the next action. No hidden wall is
interpreted as a door and no rejected move is retried.

Decision priority:

1. Terminal state or milestone/checkpoint/limit handling.
2. With visible hostile foes and Health at or below 25% of current maximum, try
   retreat. This threshold copies AiProfile::general exactly as a disclosed
   controller choice; all other route choices here are newly authored policy.
3. With visible hostile foes, fight or approach as specified below. Eating,
   bandaging and idle recovery are not selected during visible combat.
4. With no visible foe, take a reachable disclosed bandage/ration stack in stable
   item order. Take its full available quantity through an ordinary action.
5. With no visible foe and hunger hungry, weak or empty, eat one carried ration,
   if present and edible. Never eat more than one unit in an action.
6. With no visible foe and any Health/resource pool below current maximum, enter
   or continue a rest episode, as specified below.
7. Otherwise approach a remembered disclosed supply cell, then an unvisited
   disclosed walkable cell. Once neither is reachable, approach disclosed down
   stairs and descend normally. No hidden kill-count, XP110 or fixed-stock oracle
   defines a cleared floor. If all these choices are unavailable, end
   `censored_route_exhausted` with the last disclosed graph and observation.

Combat: choose the nearest visible foe by Manhattan ground distance, then the public-fact tie order above. Warrior uses Rattling Blow on a cardinal ground neighbor if the disclosed
ability is affordable, supplied and off cooldown; otherwise Basic Melee there.
Mage uses Cold Lance only outside cardinal melee when its disclosed sight range
admits the conservative ground witness and the ability is affordable/off cooldown;
inside cardinal melee use Basic Melee. Otherwise approach a reachable unoccupied
cardinal neighbor of the target using the disclosed graph. No hidden resistance
lookup or spell change is allowed. If the preferred target has no legal disclosed
approach, try other visible targets in the same order. If no visible target has a legal disclosed approach, take one fixed
west/east/north/south breadth-first step toward a reachable unvisited disclosed
cell and reassess. If no such route exists, stop
`censored_no_disclosed_combat_route`. This is a controller limit, never a won
encounter. Indistinguishable public target keys still take an explicit ambiguity
wait and retain the global bound. Selected ability use with zero damage,
miss, resistance or unaffordable availability is retained in the action record.

Retreat: if standing on a disclosed up stair above depth 1, ascend once. Otherwise
choose among currently visible, walkable, unoccupied cardinal neighbor cells the
one with greatest minimum Manhattan ground distance from visible foes, requiring
strict improvement over the current cell; break ties west/east/north/south. If
none improves distance, use the ordinary combat decision. Reassess after each
step; no guaranteed-safe destination or private shortest-path knowledge exists.
An ascent never resets budgets, memory or original cohort identity. At depth 1
an up stair is not used as an artificial successful escape endpoint.

Rest episode: starts when priority 6 first applies and ends at full current pools,
a newly visible foe, a new injury after leaving rest, floor transition, terminal
state or limit. At most one Bind Wounds attempt per episode, only with injury,
owned available bandage and the disclosed ability off cooldown/affordable. Use
ordinary self targeting and carried supplies. Then ordinary wait actions until
full current pools or interruption. Eating retains priority 5 during rest. Lack
of food or bandages does not reset or end the episode; continue bounded ordinary
recovery and retain starvation/downtime tails. A rejected action is a controller
failure, not an invitation to retry until accepted. Newly appearing foes interrupt
rest and regain combat/retreat priority. No restore, refill or nutrition override.

Each gameplay intention settles through its disclosed readiness/permission.
Paused paid work is explicitly resumed, keeping the same logical intention, up
to 16 resumes. It is not replaced by a fresh paid action. The finite-resume limit
is a controller incompleteness failure, never a survived encounter. No concurrent
command issue, speculative action, or hidden wait-until-favorable behavior.

## Bounds and outcome classification

First-HD segment: at most 2,500 mutating owner commands, counting initial purchase
confirmation, every gameplay action and every resume, including rejected commands.
Read-only queries do not consume action budget or simulation time. Continuation:
at most 2,500 additional mutating commands on each first-entered floor, with a
run-wide maximum of 65,000. Revisiting a floor retains its spent budget; restarting
retains all spent budgets. An unfinished segment at a deterministic action bound
is `censored_action_budget`, preserving its full tail. The first-HD 2,500 limit
is not reset by floor changes.

The existing harness has a 15-second individual process-response deadline. A
900-second wall deadline applies to each logical first-HD or continuation segment;
wall/process timeout is `failed_infrastructure`, not game survival or starvation.
Continuation is a new logical segment of the same run, never a fresh sample.
Saved-prefix transport chunks do not refresh its deadline. Execution uses a
monotonic clock; across stopped chunks the retained elapsed time includes the
intervening wall interval, including compression. A backward wall-clock jump
fails evidence rather than granting extra time. Wall budgets
may be too short on another machine; changing them requires a recorded new run
configuration and never converts the original timed-out attempt to success.

Outcome and evidence status are separate. Outcomes include earned_hd2,
checkpoint_depth_27, death (with observed cause, or unknown),
censored_action_budget and censored_route_exhausted. Evidence status is unrun,
running, complete, failed or incomplete. Trace/replay/source failures invalidate
completeness even if a milestone was observed. A stopped no-contact interval,
passive-pair cap, missing trace, controller error or incomplete process is never
reported as successful combat. Report every planned slot and all segment attempts.

## Commit-only diagnostic consumer contract

Opt-in server environment: TOR_SURVIVAL_DIAGNOSTICS_LOG points to a private
append-only JSONL path. Versioned begin/commit/barrier records anchor each segment
in stable authoritative save/history identity. Every accepted logical commit has
a unique stable identity across source-free restart, compacted history and load;
its event ordinals are unique. UNION ALL history/journal audit detects duplicates
rather than hiding them. Read-only projection, duplicate request, rejected work,
rolled-back candidates, and loading/replay do not emit new accepted flow.

Consumer requires selected-actor exact active ticks; Health/Stamina/Focus/Mana
periodic gains separate from direct restoration; paid start/resolution costs
separate from acquired/released holds where available; exact item consumption
separate from reservation/release. Accepted ordinary take commands and matching
owner stock provide acquisition evidence; this controller never drops. Other
stock transfers absent from the adapter are unavailable, not inferred net flows; nutrition reserve/remainders before/after, exact actual
nutrition_spent, time demand, weighted regeneration demand and clipping. A clipped
combined charge does not imply an identifiable per-component actual allocation.
Nourishment must distinguish applied food, cleared debt and capped waste; any
unavailable field is explicit null, never zero or an inferred exact value.

Save barrier attests last authoritative position, record count, completeness and
writer-failure state. No silently truncated tail or dropped error is accepted.
Action-phase attribution uses logged authoritative sequence boundaries; an
interval spanning phases is labeled mixed, not proportionally invented. Private
flow data is consumed only after action selection and never feeds the controller.

## Replay and source-free restart acceptance

A pristine run owns one accepted trace. Replays use separate outputs, compare
exact ordinary command sequence/state/receipts/flow totals, and do not enter cohort
counts. A new execution comparison starts from a saved initial ordinary-game boundary,
preserving privacy identity. Fresh games generate independent privacy salts, so
same-seed fresh games are separate acceptance attempts, never byte-exact replays.
New execution compares a narrowly audited canonical mapping of nondeterministic
history-entry UUIDs and receipt-request UUIDs and their exact typed references.
All raw data and mapping diagnostics are retained; gameplay IDs, RNG, results,
costs, progression and event order remain strict. Source-free replay of already
accepted history remains exact. The replay helper publishes the exhaustive
allowed paths and rejects inconsistent references; no generic UUID matcher is
permitted. Source-free resume
uses only the saved package and saved controller memory; remove the generated
source package, launch with explicit --regions 2 only to avoid harness default
package injection, and assert saved selected actor, game tick, state, history and
complete receipt audit exactly. Accidentally starting a new game must fail.

Before qualification, prove unsegmented versus save/restart-segmented execution
has identical accepted logical flow identities and totals, including fractional
nutrition/recovery remainders and partially paid work. New-execution identity
comparison follows the explicit envelope mapping above; it is never labeled
byte-exact. Saved-history replay retains exact logical identities. Snapshot/advancement reads
must not add flow. Reconnects, repeated save barriers, retry admission and rolled-
back candidates must not double count. Collector on/off must leave gameplay,
RNG, receipts and public views identical. Admission or writer failures are retained
and fail completeness. A sidecar that cannot establish these invariants cannot
support gross-flow claims.

## Per-run inventory

Persist before launch and update atomically: cohort/run/segment identity, exact
input/source/controller/executable hashes; seed and package arm; actual actor,
initial and final build/HD history/XP/points, attributes, equipped/carried stock,
Health/resources/nutrition/conditions; all reached and unreached checkpoints;
command/phase/resume counts and spent bounds; simulation active time and wall
runtime separately; outcome/evidence status, error/censor reason and tail location.

Keep raw command/response logs, saves, sidecar, sequence/ordinal duplicate audit,
source-free restart and replay results. Record initial/final/minimum pools,
periodic/direct recovery, paid costs and holds, nutrition demand/charged/clipped,
food debt/cap terms, actual stock flows and zero-stock intervals, elapsed recovery
and hunger-band time, visible-contact versus no-contact intervals, and every
fatal/censored rest episode. Where an exact diagnostic is absent use unavailable
with reason. Never label net changes as gross flows.

Aggregate original-cohort outcomes and attrition first. Checkpoint resource or
combat summaries are explicitly survivor-conditional. The eight-slot pilot is a
bounded tooling qualification, not a confidence interval, approved distribution,
numeric tuning recommendation or evidence that a surviving deep-tail route exists.

## Running and interpreting the tooling

`scripts/rogue_earned_route.py` owns the disclosed deterministic policy and its
serialized controller memory. `scripts/rogue_earned_route_process.py` owns real
server/headless execution, immutable input fingerprints, ordinary first purchase,
source-free save continuation and private post-decision evidence audit.

For a new campaign, materialize the candidate from an explicit complete source
package, then create its generic source snapshot through
`source_snapshot(package, output, compiler, reference=...)`. Each explicit arm
snapshot contains `package/` and `source.json`, with a provenance reference,
exact package hashes and rules dependency hashes. The same current compiler
must resolve those pinned dependencies and compile both arms. Tests use the
committed E/S fixture as the independent `production` arm and generate their
candidate from an explicitly supplied supported source package; they need no Git
history, network or environment overrides. The strict candidate generator accepts
the independently reviewed 21-species input with room Gold and a separate exact
24-species input adding M/P/X. Both condition to the retained eighteen-species,
four-stage diagnostic output without L/N/O or M/P/X, the Leprechaun birth-Gold
pool or room-Gold stage. The 21-species oracle/result is unchanged. The new
24-species oracle retains M's upper-clamp open tail, exact M/P/X birth policies
and appended choice order; overlapping depth bands or policy drift reject.
Accepting the new input does not qualify an expanded encounter study.
Each oracle pins activation policy,
not arbitrary supplied catalog/rules bytes; source closure and resolved package
rules remain retained and fingerprinted. New generations have their own source
fingerprints and qualification; an
old compiler receipt cannot qualify them. The generator also accepts a retained
complete preactivation or eighteen-species source closure. It does not use the
independent E/S fixture to reconstruct missing Vampire content.

```sh
python scripts/rogue_activation_candidate.py --compiler TOR_SCENARIO --source-package SOURCE_PACKAGE --source-provenance '{"reference":"SOURCE_REVISION_OR_LABEL"}' --output NEW_CANDIDATE
```

The corresponding create API requires `source_package` and `source_provenance`;
provenance must be a nonempty JSON object with a nonempty string `reference`.
The new diagnostic inventory records observed `source_population_shape`
(`preactivation_es`, `canonical_eighteen_species`,
`reviewed_twenty_one_species_input` or `reviewed_twenty_four_species_input`),
explicit input/output population scopes,
package provenance and separate package/tool/compiler-dependency hashes, including
the independent input oracle. Its `production_mutated = false` and
diagnostic artifact purpose replace the obsolete global `production_activation`
claim. See [generator contract](arena.md#rogue-roster-study-and-bounded-candidate-inputs).
The candidate generator's declared target remains the eighteen-species population.
Its retained output scope and historical input labels are preserved rather than
relabeled to match current 24-species production. The retained eight-species pilot
remains old-source evidence, not acceptance of later roster additions, flight, linked holds,
ordinary restoration stock, theft, Gold or M/P/X perception. Extending these studies to L/N/O and
room Gold and M/P/X is separate from the production feature batch.
These new results do not rewrite historical inventories, report schemas, cohort
order or source identities. Resume an existing campaign from its retained sources
and packages; never regenerate them from the current bundled package.

Set both source paths explicitly before using the existing CLI:

```sh
export TOR_EARNED_ROUTE_PRODUCTION_SOURCE="$PWD/scripts/fixtures/rogue-earned-baseline-v19"
export TOR_EARNED_ROUTE_CANDIDATE_SOURCE=/absolute/path/to/candidate-source-snapshot
python scripts/rogue_earned_route_process.py /tmp/earned-route --prepare-only
```

The equivalent API is `prepare_campaign(output, bin_directory,
arm_sources={"production": baseline_snapshot, "candidate": candidate_snapshot})`.
Missing sources fail before any implicit binary build. Use `TOR_TEST_BINARIES`
to select already-built, verified executables during a frozen campaign.

The separate `--acceptance-seed42` mode runs the fixed production Warrior witness;
it never changes the eight unrun cohort rows. `--without-diagnostics` is an
ordinary-controller development check only and must retain unavailable gross
flow fields. Never interpret such a run as complete measurement evidence.

Revision 1 originally required fresh-seed exact replay and opaque-ID tie order.
Source inspection established randomized privacy salts and history identities,
so revision 3 explicitly replaces those impossible assumptions. It does not
change game rules, randomness, species behavior, kits, costs or candidate weights.
`Engine` initializes privacy identity in `crates/server/src/engine.rs`;
`crates/server/src/wire_targets.rs` derives scoped target handles. New history
entry identities and client request identities are UUIDv4 envelopes, with retry
validation retaining independent actor/branch/command checks.

A milestone detected in ordinary owner data is saved before another gameplay
action. The private checkpoint audit then verifies the selected actor's ordered
HD history and earned progression. Those private fields never choose an action.
The ordinary advancement API has no detailed per-HD history page; do not invent
one or use a wizard command in the route.

All acceptance failures, timed-out segments, unavailable diagnostics and original
cohort attrition remain visible. Local focused checks do not replace the repository's
quick/push/full and exact-head cross-platform CI publication gates.

Revision 3 adds only the blocked-combat disclosed-frontier fallback and explicit
no-route censor. The revision-2 fixed-seed42 acceptance reached starvation after
a visible foe had no disclosed cardinal approach while reachable unvisited cells
remained. That failed controller policy and its full adverse tail remain retained;
it is not a game-balance result, and no game rule was changed for the correction.

Completed process logs are losslessly gzip-compressed only after their processes
stop. Retention manifests append rather than replace earlier entries, verify
existing compressed files and their exact decompressed bytes, and publish new
hashes before reclaiming redundant raw bytes. Every transport endpoint retains
a compact immutable boundary manifest binding the full current report hash,
independent authority anchor/record/creature hashes, controller/source pins and
exact command/flow/process trace bytes. The append-only original traces and
current authoritative save remain retained. Full immutable reports and consistent
SQLite backups, including committed WAL pages, are kept at earnedHD2, reached
logical depth checkpoints and terminal outcomes; initial.db anchors the start.
Ordinary transport chunks do not duplicate the entire growing save/report.
A later resume verifies those bindings, all retained actor/seed/arm/configuration
and source/executable/package bindings, and the exact independent accepted-prefix
anchor, records, receipts and creature before issuing new gameplay. Already
accepted history never uses envelope normalization.

Each live `RouteExecution` keeps at most 256 verified archive-content bindings in
memory. Every reuse hashes the current compressed bytes again. One open file
handle is retained through hashing, decoding and cache acceptance. The first
verification also hashes the exact compressed stream being decoded and checks its
raw SHA and length; only successful checks enter the least-recently-used cache.
Changed manifest claims, altered archives and evicted entries require full
verification. A reconstructed executor starts empty. No cache trust is written to
the retention manifest, and standalone cleanup performs a fresh verification.
This avoids repeatedly decompressing unchanged earlier logs; it changes no route,
restart, output bound, failure classification or comparison surface.
Longer attempts exceeding the cache capacity may regain repeated decompression
cost; eviction never substitutes an unchecked result.

For a matched diagnostic run, set `TOR_EARNED_ROUTE_TIMING=1` alongside the usual
artifact-retention setting. Each attempt writes a bounded `timing.json` sidecar
with fixed counters, inclusive timings and at most 128 segment summaries; extra
summaries are counted explicitly. Nested timings overlap and must not be summed.
Archive-hash bytes count the initial compressed-byte validation on every use;
archive-decode bytes count expanded raw bytes on cache misses. The decode span
also includes hashing the compressed stream it consumes. The observer wall span
includes gaps within that live executor; a reconstructed executor starts a new
observation. The sidecar is not qualification evidence or persisted cache trust.
Failure to write it is reported on stderr without masking a route failure.
Compare the full test's wall time separately on the same quiet host, profile,
binaries, frozen package and configuration; no speedup is implied by cache hits.

### Minimal serial pilot orchestration

Freeze all eight new slots before any launch and run them in the declared order.
For each slot create one `RouteExecution` with its actual actor, seed and arm.
Call `execute(prefix_commands=128)`. At an `acceptance_prefix_saved` transport
boundary, verify the successful ordinary evidence, compress stopped logs, then
call `resume(prefix_commands=report["controller"]["commands"] + 128)`. These are
chunks of the same attempt, not additional samples or survival outcomes. Every
paid resume remains counted; a nominal prefix ends only after settlement.

The same loop starts continuation only for an evidence-valid surviving
`earned_hd2`, through `resume(continuation=True, prefix_commands=commands + 128)`.
Later transport resumes omit `continuation=True`. A destructive source-deletion
acceptance uses its own launch-package copy, leaving the canonical frozen arm
available for the remaining original slots. Record the HD2 endpoint before
continuing it. Never replace a dead/censored slot, refresh command budgets, alter
purchases, or reuse another arm's survivor. A final depth27 arrival terminates
that route; unreached checkpoints remain explicit.

Each chunk has a declared16MiB process-output soft boundary and32MiB hard limit,
using the shared process-output guard. A settled soft boundary may shorten a
128-command chunk without changing controller policy or logical budgets. A hard
breach, response/wall timeout, storage floor below512MiB or64MiB JSON artifact cap
is an infrastructure failure, never a gameplay censor. Output is checked before
and after commands and on every consumed JSON frame; `omit_memory=True` is used from initial attachment. Ordinary
observations and public history remain retained. Failed/incomplete tails are
never resumed as if they succeeded.

The existing seed42 acceptance uses this same bounded chunk loop. Its accepted
trace and a second execution from the identical initial save have separate
outputs. Exact source-free prefix restoration and narrowly normalized new
execution are separate results. A fully qualified fixed acceptance does not
silently qualify unexecuted or unpaired cohort rows. Preserve ordinary-evidence,
restart, comparison and source-identity statuses independently. A pilot that
never reaches27 is still a useful, honestly bounded observation; it does not
justify changing the sample or numerical balance.

## Revision-4 first-growth checkpoint

The fixed eight-slot pilot reached ordinary earned HD 2 on floor 1 for both
Warrior and Mage, seeds 0 and 1, in both the frozen E/S comparator and the
eight-species candidate. Every slot used its actual starting kit and ordinary
first talent purchase, with no substituted seeds or granted progression. The
immutable initial and HD-two saves, accepted command prefixes and independently
anchored gross-flow records were retained and verified.

This is first-growth evidence only. Deep continuations and per-sample new-command
reexecution remain separate, unfinished qualifications; these eight arrivals do
not establish deep survival or numerical balance. The fixed seed-42 acceptance
separately passed exact source-free restoration and the audited new-execution
comparison. Historical evidence keeps its original frozen package and tool
identities when current production changes.

## Qualified ordinary HD-two example

One source-qualified revision-3 acceptance run used the unchanged production
package, Warrior actor 1 and seed 42. The first-HD limit was 2,500 mutating owner
commands, the segment wall limit 900 seconds and the paid-work limit 16 resumes.
It made the ordinary first Rattling Blow purchase and left later points unspent.

The run reached its first actual earned second HD after **727 mutating commands**
(one confirmation, 725 actions and one resume). Owner progression and the saved
ordered HD history agreed: **110 earned XP, 100 spent XP and 10 held XP**.
The independently anchored committed-flow log recorded:

- **742 nutrition spent:** 726 time demand plus 16 regeneration demand, with no
  clipping;
- **15 Stamina and 1 Health** gained through periodic recovery;
- **3 Health** restored directly;
- **two bandages consumed and no food consumed**.

These are gross committed flows, not differences between initial and final pools.
One successful acceptance route is not a balance approval, an optimal-build claim,
or evidence about the fixed eight-slot cohort or the U-only deep candidate tail.
The recorded source window was `e00a6112763fb7078d20d9b41aece565f11d178d`;
subsequent adapter fixes classify an exact-budget paused tail and correct the
initial-entry report snapshot, without changing this controller's decisions.

A second execution used the retained initial ordinary save. It also took 727
commands and matched the reviewed gameplay/controller surfaces and exact gross
flow totals. The two checks have different meanings:

1. Source deletion followed by restoration of already accepted history preserved
   state, public history and chronological population receipts **exactly**.
2. Newly issued commands were compared with the explicitly audited envelope UUID
   mapping. That result is **normalized execution equivalence, not byte-exact
   output equality**. Raw histories and identities remain retained.

For a fresh current-revision acceptance against the pinned E/S source, set the
explicit arm snapshots above and choose a new output directory:

```sh
python scripts/rogue_earned_route_process.py /tmp/earned-route-example --acceptance-seed42
```

For the paired acceptance, set `TOR_EARNED_ROUTE_ARTIFACTS` to a new directory if
persistent outputs are wanted, then run in the pinned development environment:

```sh
python -m unittest discover -s scripts -p test_rogue_earned_route_process.py
```

Each campaign's generated `inventory.json` pins its source, executable and compiled
package inputs. Its `attempt.json` retains the ordinary observations, controller,
independent initial/final anchors, full accepted records and validated gross totals;
`commands.jsonl` preserves issued commands. The paired `comparison.json` records
the reviewed surface selection, normalization schema and complete identity mapping.
The source-owned schema is in
[`rogue_route_replay.py`](../../scripts/rogue_route_replay.py).
`retained-raw-logs.json` pins compressed and exact decompressed bytes. Lightweight
report/source bundles do not include the separately retained large raw logs,
sidecars or saves unless those files are explicitly bundled with them.
