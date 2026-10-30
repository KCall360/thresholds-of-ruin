# Autonomous decisions and knowledge-limited tactics

This reference describes current decision ordering, not an optimal strategy or
balance target. [Behavior authoring](scenario-packages.md#behavior-defaults-and-ai-overrides)
owns species/profile fields; [perception](../architecture/perception.md) and
[provocation](creature-rules.md#creature-behavior-and-provocation) own sensory and
engagement authority. An actor is the deciding creature, not a global player.

Implementation entry points: [choice](../../crates/simulation/src/ai.rs),
[execution](../../crates/simulation/src/actions.rs),
[item tactics](../../crates/simulation/src/ai_items.rs),
[projectile choice](../../crates/simulation/src/ai_projectiles.rs),
[self-defense](../../crates/simulation/src/ai_defense.rs).

## Admission, choice and commit are different stages

1. The host admits an AI intention through ordinary simulation ownership. This
   neither chooses an action nor advances the actor's RNG or simulation time.
2. For a due actor, execution calls `choose_executable_ai`. The private decision
   contains one action, resulting AI/behavior/engagement/random state and, where
   applicable, a captured ability plan.
3. `choose_ai` reads the actor-specific decision snapshot and copied controller
   state. Repeated pure previews do not change authoritative visits, engagement,
   action queues or random state. A preview reserves nothing.
4. The execution gate handles narrow contention/numeric fallbacks, prepares the
   ordinary action and binds the original admitted intention identity. It does
   not secretly choose a different hidden source after a selected item fails.
5. Action effects and proposed controller state commit through the same private
   candidate. Failure does not publish speculative visits, engagement or RNG.
   Replay validates the recorded action through the ordinary execution gate.

The [simulation cycle](../architecture/simulation-cycle.md) owns scheduling;
[combat](combat.md) owns paid execution. NPCs do not have a second damage engine,
free equipment path or a reaction selected after inspecting a private result.

## Inputs and retained state

| Input/state | Meaning and limit |
| --- | --- |
| Species disposition and memory | Passive/Mean acquisition and actor-active engagement expiry; an AI profile does not replace these |
| Resolved profile | Independent idle, doors, care, equipment, useful loot, Gold and flee permissions; birth stock does not grant tactics |
| Disclosed decision snapshot | Current visible actors/cells, owned inventory, permitted contacts/interactions; not hidden positions or defenses |
| Personal navigation | Learned directed links, orientation and terrain, not unseen current topology |
| Engagement | Target/reference kind, last seen location and actor-active expiry |
| Anonymous alert | A location to investigate/flee, never a target identity or attack handle |
| Retained preparation | Frozen work, claims and committed cursor; resuming does not recapture it |
| Visit history | Saved per-location exploration counts; pure choice does not increment authoritative counts |
| Behavior RNG | Saved actor-owned state used for authored random choices, not a wall clock |

Frozen actors accrue no active time and gain no catch-up turns on thaw. An attack
needs the current disclosed reference to match the engagement, not merely the same
private actor ID. Arrival at a remembered location can support investigation
without authorizing an attack against an undisclosed occupant.

## State classification

After the early intact-guise ambush branch, the decision is labeled Search, Attack
or Flee for the remaining priority branches:

| Frightened? | Licensed threat location? | State |
| --- | --- | --- |
| Yes | Yes | Flee |
| No | Yes | Attack |
| Either | No | Search |

Frightened means a forced-flee objective or
`current Health × 100 <= maximum Health × flee_percent`, using wide arithmetic.
Threat location prefers forced flee, then engagement; retained-work handling can
use an eligible visible candidate, followed by an anonymous awareness alert.

Hostile candidates must be currently disclosed, pass awareness eligibility and
perceived-hostility checks. Ranking uses remembered route length, then actor ID;
no route ranks after reachable candidates. The [known-space routing algorithm](../architecture/perception.md#known-space-pathfinding-and-its-limits)
owns edge costs, deterministic search and path reconstruction. Mean actors with positive memory can
acquire eligible opponents directly, with **no separate wake lottery**. Passive
actors need ordinary provocation, except readied touch ambush. Continual Stealth
still gates awareness. Engagement refresh needs a matching current reference;
reaching last-seen position without seeing the target clears the engagement.

## Exact priority order

The first applicable returning branch wins. Later choices are not evaluated just
because they might appear tactically better to an observer with hidden knowledge.

| Order | Branch | Selection / stopping rule |
| --- | --- | --- |
| 1 | Intact readied-touch guise, no retained work | With voluntary input and an actual toucher, try the qualifying touch plan; otherwise Wait. Adjacency/private recognition is not a touch attack |
| 2 | Reconcile engagement and state | Retained work does not acquire a fresh opponent or refresh its engagement |
| 3 | Resume available paused work outside Flee | Return the retained plan before new care, gear or attacks |
| 4 | Healing care, no retained work, Health at/below 50% | Known safe carried healing item first, otherwise eligible harmless healing plan; no unknown-potion experiments |
| 5 | Food care | Search, no visible hostile exposure, required nutrition not Nourished, no retained work and known carried food |
| 6 | Erratic movement | No retained work and movement allowed: copied actor RNG selects only among disclosed legal translations |
| 7 | Flee | Select a legal disclosed retreat using the current heuristic. Without a committed prefix, a qualifying step ability may implement it; otherwise ordinary Move owns abandonment/debt |
| 8 | Resume committed paused prefix when no retreat chosen | Do not replace it with new gear/healing/attacks or repeat its first strike |
| 9 | Proactive self-defense | Requires a current sensed engaged target and a narrowly recognized useful self-condition plan |
| 10 | Engaged attack | Current matching visible target; normal hostile ability choice outside Flee, basic attacks only when cornered in Flee |
| 11 | Remembered pursuit | Outside Flee, first legal route step toward last seen; unusable pursuit clears engagement and returns to Search |
| 12 | Anonymous-alert investigation | Outside Flee and without engagement, route to licensed alert location; do not invent a target attack |
| 13 | Safe Search items | Equipment improvement, then useful nearby loot, then known Gold; each has policy and visible-hostile gates |
| 14 | Doors | Search plus door permission and voluntary input: first disclosed reachable closed door |
| 15 | Idle exploration | Search + Explore + voluntary input: increment proposed current-cell visits, choose legal disclosed neighbor by fewest visits then stable location order |
| 16 | Fallback | Wait through ordinary action execution |

An idle Wait profile forbids roaming and detours, not retaliation or every useful
underfoot action. Tool permissions remain independent. One decision does not
search future combat outcomes or learn hidden immunity from rejected previews.

## Ability and defensive selection

Eligible attacks are considered by descending authored AI priority, then stable
ability identity. Choice checks the actor's own resources, cooldowns, source and
disclosed geometry; it does not optimize against secret opponent defenses.
Directional/ray candidates use disclosed alignment rather than a hidden trace to
find concealed bodies. Known finite devices are fallback sources only when their
identity and a positive charge fact make them usable; query or birth possession
does not identify them.

Healing care recognizes a narrow safe plan: positive possible healing, with only
healing effects in the relevant branches, not any ability whose name resembles
healing. Finite stock and supply selection remain ordinary [item rules](items.md).
A harmful alternative branch cannot become a free self-care experiment.

Proactive defense likewise recognizes supported checkless self-condition plans,
not arbitrary beneficial-looking effects. It rejects unrelated side effects,
unsupported/reserved condition families and replacement/eviction cases that would
make the action unsafe. The current threat and effect eligibility are rechecked;
this is not a general utility search or optimal buff planner. The exact predicate
is owned by [ai_defense.rs](../../crates/simulation/src/ai_defense.rs).

## Item decisions and bounded search

Known equipment is compared using disclosed usable alternatives and ordinary
capacity/removal rules. Useful inventory pickup reserves finite need: appropriate
gear, one healing reserve and supplies for one payable safe healing use. Food
reserve depends on required nutrition. Gold interest is separately authored; it
does not grant theft or teach hidden magical effects.

The item policy uses bounded remembered-route detours (up to three steps), not a
whole hidden-floor loot oracle. Projectile-source search scans at most 128
inventory rows and considers bounded launcher choices; no unbounded inventory
scan is implied by a generic projectile ability. Selection is deterministic at
its stable tie-breaks. See [item tactics](../../crates/simulation/src/ai_items.rs),
[projectiles](../../crates/simulation/src/ai_projectiles.rs) and
[finite supply selection](../../crates/simulation/src/supplies.rs).

These bounds are selection policies/capacity limits, not guarantees that every
eligible item deep in inventory will be found or that a chosen request will still
be valid at execution. Exact item ownership, holds and contact remain authoritative.

## Execution-only refusal and recovery

The execution gate intentionally handles a small set of expected races:

- An action blocked by a current condition restriction on that action can become Wait.
- A selected projectile request that exceeds its numeric preparation budget can
  become Wait rather than inventing a different attack.
- An exact selected private item that is no longer available can become Wait;
  hidden alternatives are not substituted.
- Unrelated invariant/validation errors propagate and reject the candidate.
  “AI should keep moving” is not permission to swallow all errors.

A successful fallback still passes ordinary scheduling and publication. Partial
paid work retains its committed-prefix and recovery obligations; failed candidate
execution does not leak new engagement or RNG. Human and autonomous controllers
share the same capability, source and target gates.

<a id="current-retreat-limitation"></a>
## Retreat distance and limits

When a current route is known, flee compares **hop counts of stable
minimum-duration remembered routes** from the current position and each legal,
currently disclosed candidate. It does not mix accumulated movement time, local
coordinates or an invented `current + 1` into those scores. Candidate searches
start in the actual post-step location and body orientation. A remembered
**directed** graph with no return path from a known candidate to the otherwise
reachable threat gives that candidate unbounded distance. A candidate not yet
present in the remembered graph is unknown, not proof of an escape.

Current visibility does not require a retained navigation map. Without a current
route, a conservative local fallback compares Manhattan distances on both sides
of the comparison, but only when actor and threat share a region and the existing
physical disclosure chart within that region is unwrapped: every such occurrence
has its ordinary body-relative coordinates and unchanged frame. Candidate steps must likewise be
ordinary same-region displacements. Local mode rejects folded or rotated
occurrences within the actor’s region and does not use portal-crossing candidates
or cross-region coordinate comparisons. It neither inspects hidden links nor
teaches navigation during a decision. Disclosed occurrences in other
regions do not forbid ordinary local steps; a disclosed fold back into the
actor’s region does. It is a local heuristic, not a proof about undisclosed paths. A partial map never causes per-candidate metric
switching.

Only a strictly greater score qualifies; existing direction order resolves ties.
If neither metric is available, retained commitment, visible self-defense/basic
attack and eventual Wait retain their ordinary priority. First-step collision
and disclosure checks still apply; no reverse link or hidden topology is invented.

A separate frontier per candidate adds search work (at most ten candidate
origins); this is a correctness change, not a measured optimization or an optimal
escape planner.
The [regressions](../../crates/simulation/src/ai_flee_tests.rs) cover split/unsplit
corridors, rotated and one-way portals, blocked retreat, missing remembered
connections, no-map local retreat beside visible other-region portals, folded-chart
rejection, pure previews and checkpointed autonomous execution. The
[review record](../development/documentation-review.md#recommended-engineering-work)
retains the original source-only finding and its subsequent confirmation.

## Worked control-flow examples

- A paused preparation outside Flee resumes at row 3, even if carrying a healing
  potion. That priority preserves frozen work rather than recapturing an attack.
- An injured unprepared actor can heal before the flee branch. Flee is a state
  label, not a promise that movement always precedes self-care or erratic choice.
- A lost visible target can leave a last-seen pursuit location. Reaching that cell
  without redisclosure clears engagement; the actor cannot attack an imagined
  hidden occupant by private identity.
- A guised Xeroc remains waiting when another actor privately recognizes it.
  The readied touch branch requires actual touch, not recognition or adjacency.
- Gold under an actor can be useful while idle roaming is disabled, if the
  independent Gold policy permits it. A distant Gold detour still requires the
  appropriate movement policy and remembered route.

## Verification map

Source/test witnesses are static evidence here, not newly executed results.

| Contract | Owner |
| --- | --- |
| Pure choice and stable priorities | [ai.rs](../../crates/simulation/src/ai.rs), [decision tests](../../crates/simulation/src/ai.rs) |
| Idle visits, routes and permissions | [idle tests](../../crates/simulation/src/ai_idle_tests.rs), [hobgoblin host tests](../../crates/server/src/hobgoblin_roaming_tests.rs) |
| Awareness, normal contact and Stealth | [awareness](../../crates/simulation/src/awareness.rs) |
| Safe item/source selection | [ai_items](../../crates/simulation/src/ai_items.rs), [ai_projectiles](../../crates/simulation/src/ai_projectiles.rs) |
| Defense predicate | [ai_defense](../../crates/simulation/src/ai_defense.rs) |
| Candidate execution/fallback | [actions](../../crates/simulation/src/actions.rs), [intention ownership](../../crates/simulation/src/intention.rs) |
