# Bounded ordinary earned-route qualification, revision 3

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

Production is an unchanged compiled snapshot of scenarios/rogue-exploration: its
ordinary floor population remains two emus and two snakes. Candidate is a
separate compiled output of rogue_activation_candidate at start_depth=1. Its
bounded E/S/H/I/Z/Q/U bands, including U-only depths 18–26, remain an unmeasured
candidate. Both start at the authored depth-one anchor. C/Y and starts beyond 26
are outside this contract. No production files, weights, actor dispositions,
rules, kits, attributes, skills, supplies or nutrition values are rewritten.

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
first ordinary entries to depths 5, 8, 11, 17 and 26, retaining original cohort
attrition; depth 1 is the initial entry record. Stop at first entry to 26. Later
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
900-second wall deadline applies to each launched segment, measured monotonically;
wall/process timeout is `failed_infrastructure`, not game survival or starvation.
Continuation is a new segment of the same run, never a fresh sample. Wall budgets
may be too short on another machine; changing them requires a recorded new run
configuration and never converts the original timed-out attempt to success.

Outcome and evidence status are separate. Outcomes include earned_hd2,
checkpoint_depth_26, death (with observed cause, or unknown),
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
numeric tuning recommendation or evidence that a surviving U route exists.

## Running and interpreting the tooling

`scripts/rogue_earned_route.py` owns the disclosed deterministic policy and its
serialized controller memory. `scripts/rogue_earned_route_process.py` owns real
server/headless execution, immutable input fingerprints, ordinary first purchase,
source-free save continuation and private post-decision evidence audit.

Prepare the fixed inventory without launching any route:

```sh
python scripts/rogue_earned_route_process.py /tmp/earned-route --prepare-only
```

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

Completed raw process logs may be losslessly gzip-compressed. A retention manifest
records original size and SHA256, compressed SHA256, and verified decompression
before the redundant raw copy is reclaimed. Active execution stops with an
infrastructure failure before free storage falls below 512MiB; such an incomplete
trace is never a gameplay censor or survival result. The headless driver uses
`omit_memory=True` and retains ordinary observations and public history.
