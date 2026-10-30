# Perception and exact three-dimensional sight

Every actor uses one authoritative perception model: illuminated eye-based
three-dimensional sight, immediate awareness around its occupied body, and
Awareness-governed nonvisual perception when sight is obscured.
Player observations, AI, items, place discovery and navigation use that same
actor scene. Clients receive disclosed facts, never the hidden world.

Sight starts at the body's declared eye, while protocol positions remain
relative to its reference cell (usually the feet). Gameplay range is **16**
Manhattan cells, matching the world API cap. Range is not a guarantee of
perception: topology, occlusion and illumination still matter.

[Geometry](geometry.md) explains finite material volumes, bodies, rotations and
stairs. [Client architecture](clients.md) owns remembered sightings and surface
presentation. Equipped or grounded finite-fuel portable lights contribute
occluded physical illumination through the ordinary world scene. General light
transport, sound propagation, transparent materials, partial visibility,
posture-dependent eyes and continuous stair meshes remain outside the model.

## Concealed physical features

Secret entrances are ordinary opaque, colliding doors with concealed exterior
appearance; their collision and sight blocking never depend on the observer.
Discovering a closed entrance teaches only that actor where it is. Opening it
changes shared geometry. An observer who physically sees an already-open secret
entrance learns its presence without a roll and retains that fact if it closes.
Ground traps similarly separate presence
from family knowledge: discovery reveals presence, while authorized adjacent
examination or perceptible activation reveals the family. Discovery does not
open entrances or disarm traps.

All discovery uses Awareness-governed Perception and the shared 3d6/Edge kernel:

- **Passive opportunities:** each actor gets one attempt when an exact concealed
  cue first enters genuine illuminated physical eye sight, and a second when
  that actor first becomes physically adjacent. An initially adjacent sighting
  makes one roll and consumes both opportunities. Both failures persist; leaving
  and returning does not create another attempt.
- **Distance:** a cue within one square of any occupied body cell adds one
  advantage, two–three squares are neutral, and four or more add one
  disadvantage. These are nearest physical projected-cell, diagonal-inclusive
  bands, not an extension of the Manhattan sight-range cap. Passive checks add
  one further disadvantage. Thus adjacent passive is neutral before other
  modifiers; distant passive has two disadvantages. Ordinary cancellation and
  the shared three-extra-dice cap apply. Distance origins are the actual resolved
  body cells even when those cells are dark; an illuminated adjacent cue does
  not become a distant check merely because the observer stands in darkness.
- **Paid Search:** normal 50-tick preparation and 50-tick recovery remain subject
  to ordinary haste/slow timing. Preparation freezes the actor's Perception bonus
  and Edge; completion uses live physical candidates and distances. One shared
  six-die sample yields correlated near/middle/far totals by retaining the same
  kernel's three dice from the appropriate prefix. It is not a new independent
  roll for every candidate or distance band. Repeated paid searches are allowed.
  One-use outgoing next-check modifiers participate through one normal paid
  reservation lifecycle even when nothing is found; hidden candidate count does
  not change the sample or reservation. Incoming next-check-against modifiers do
  not apply to this targetless check.
- **Tactile Search:** without usable illuminated sight, paid Search can feel for
  a concealed entrance or trap on an occupied body cell or an immediately
  reachable physical surface. It adds one disadvantage: the ordinary adjacent
  advantage and this penalty cancel before other modifiers. Darkness and
  blindness together do not apply the penalty twice. Visual candidates keep
  their visual total; tactile candidates use the same sample's middle total,
  never a second roll. Every diagonal component route must be clear and agree
  on the endpoint; walls, intervening closed doors and other bodies obstruct
  touch. A closed entrance may itself be the endpoint, not a passage through it.
  Actual rotated physical joins work, but abstract stair landing projections do
  not. This rule never identifies a creature's disguise or triggers a trap.
- **Periodic search:** worn searching sources share one actor-active cadence;
  every 100 eligible ticks the ring runs the same active search for free. It does
  not consume, reserve, or borrow one-use check tokens. Passive checks and initial
  disguise formation likewise exclude those tokens. Ordinary timed, equipment
  and talent Edge still applies. In darkness an eligible scan shares the tactile
  rule; blindness still pauses the existing ring clock and prevents automatic
  scans. A blind actor may instead initiate an ordinary paid tactile Search.

Physical features retain authored DCs; disguises instead use the stored contest
described below. Paid and periodic searches do not automatically break adjacent
guises. Empty and failed feedback does not disclose whether concealed candidates
existed. A successful Search reveals trap presence, not its hidden family.
A successful active discovery makes further passive attempts unnecessary; a
failed active search does not consume either passive opportunity.

Passive visual discovery still excludes darkness and blindness. Neither visual
nor tactile discovery uses abstract stair projections or radius-only proximity;
opaque barriers, sleep and voluntary-action incapacity remain effective gates.
Waking and thawing reconcile at authoritative boundaries. Reentry, reconnect,
control transfer, rank changes and reload never reset failed passive receipts.
Observation, previews and rendering neither roll nor mutate knowledge. Checks,
knowledge and RNG publish transactionally; candidates have stable exact-ID order.

Build-less exploration actors remain physical trap-contact owners. They retain
contact identity and activation disclosure, including across restoration, but
cannot make creature skill checks or receive creature injury, corrosion or
condition payloads. A triggered arrow has no eligible creature to hit and leaves
its ordinary ammunition; no defense or damage roll is invented. Trapdoors still
open shared topology and teleport traps still relocate the body. This eligibility
boundary belongs to trap dispatch, not the generic creature effect APIs; missing
actors remain errors rather than harmless recipients.

A branch-local derived witness skips unchanged passive reconciliation and is not
saved authority. Initial construction explicitly reconciles before publishing its
first immutable boundary; restoration never grants extra attempts.

DCs, placement probabilities and effect durations are authored provisional balance
choices, not user-approved numerical tuning. Future property interpretation keeps
its separate rank/evidence lock policy; it must not make required physical routes
permanently undiscoverable after a failed roll.

## Object guise and private recognition

Object Guise is a shared source-authored capability, not a Xeroc name check.
Its source chooses a skill and check Edge. Rogue authors Stealth (Speed) with
zero additional Edge; an authored talent benefit can improve the disguise check
without improving unrelated attacks. On the first legitimate acquisition, the
simulation rolls once and persists the total with that actor's one-lifetime
guise. Source loss/regrant, restoration and ordinary observation do not reroll
it. Perception (Awareness) contests that stored total; ties favor Perception.

An observer's successful discovery changes only that observer's disclosure and
targeting authority. Other observers still see the apparent object. The
disguised actor receives no notification, AI engagement, movement or attack
trigger: it does not know that its disguise was recognized. Failed first-sight
and first-adjacency receipts use stable actor identity, separately from door and
trap identities, and survive save, streaming, control changes and reentry.

The separately authored readied-touch-ambush policy makes a predator wait
motionless while disguised rather than attack merely because a foe is adjacent.
Touch can release its explicitly granted initial-ready attack; an incoming
attack breaks the guise and permits ordinary fighting. Recognition alone does
neither. A guise is not an inventory item and cannot be picked up as loot.
The one-lifetime readiness stores the source creature, body, launch placement and
eligible natural/selected BASIC MELEE ability, never an opponent. This policy
rejects attacks with costs, supplies or cooldowns; touch binds and revalidates
the live toucher, releases at that tick, and pays ordinary recovery. It does not
grant a second attack, reset readiness or stack a second ambush advantage.
Source/body/placement changes and unfinished work invalidate the entitlement;
restoration does not rearm it. Initial authored readiness is captured only after
the spawn's body is assembled. These records remain separate from observer
discovery receipts. Touch without an actual released attack does not globally
expose the guise. If the initial entitlement was invalidated, a touch may record
a provocation for ordinary preparation when the actor is capable; it never
manufactures another initially-ready attack. A still-ready frozen actor cannot
release its attack or acquire a free queued attack from touch.

Successful recognition invalidates identity/event and sensed-hostile recovery
witnesses; failed receipts do not. The derived guised-owner index keeps
guise-only discovery active without scanning all actors on every no-cue
transition. Physical subject changes invalidate candidate reuse even when the
observer's eye chart is unchanged. These caches are not saved authority.

## Model

All geometry uses **doubled coordinates** relative to the eye. Cell `(x, y, z)`
has its centre at `(2x, 2y, 2z)` and its faces on the odd planes `2x ± 1`,
`2y ± 1` and `2z ± 1`. Every point used below is then an integer point.

### Eye

A sight line always starts at the **centre of one eye cell**. The body
definition names that cell:

- `BodySpec` includes an `eye` offset, which must be one of its `cells`.
- Every actor and archetype `body` declaration must declare `eye`, including
  single-cell bodies. The offline validator rejects a body without one, so no
  creature silently looks from its feet.
- A typical two-cell humanoid, `cells = [[0,0,0], [0,0,1]]`, uses
  `eye = [0,0,1]`. The built-in default playable character is defined this way.
- The eye offset rotates with the body frame like any other body cell. A body
  that crosses a sideways portal looks from the correct cell.

The eye cell is occupied by the observer's body, so it is never opaque.

### Targets

A sight line ends at one of these points:

- **A non-opaque cell** (open air, walkable space, an open door): its centre.
- **An opaque cell** (solid terrain, a closed door): the centre of each of its
  **exposed faces**. A face is exposed when the neighbouring cell across it
  exists and is not opaque. The cell is visible if any of these points is
  visible.
- **Missing, unloaded or ambiguous geometry** is never a target. It blocks sight
  and is never disclosed as a wall.

Visible occupied cells make actors and items candidates for disclosure. A body
can be physically seen through any visible occupied cell, but actor identity and
appearance still pass the ordinary illumination, awareness, concealment and
private recognition gates; geometry alone is not an unconditional identity grant.

### Blockers

Every opaque cell is a blocker shaped as the cube `[-1, 1]³` around its centre,
with **bevels on its convex exposed edges only**:

- An edge is beveled when **both** faces meeting at it are exposed.
- The bevel cuts the cube with the plane through those two faces' centres. For
  the `+x/+y` edge, that is `x + y ≤ 1` in local doubled coordinates.
- Edges between an exposed face and a covered face, or between two covered faces,
  stay square.

As a result:

- A flat floor or ceiling slab has no beveled edges between its cells, so it
  leaves no gaps.
- Straight walls and inside corners stay square. In 2D, Ford's diamond bevel
  only changes results at outside corners.
- Outside corners, door jambs and pillar edges get Ford's bevel. Seen
  horizontally, a free-standing pillar is exactly Ford's diamond.
- Two blocks that touch only along an edge are both beveled there, so sight
  passes between them, under this model.
- The edges of ledges, pit rims and the tops of waist-high walls are beveled
  horizontally, so sight passes over them in the same way it passes around wall
  corners.

A segment is **blocked** by a blocker only if it passes through the blocker's
**interior**. Touching a face, edge or vertex does not block.

### Visibility

A target point is visible when:

1. The target cell is within range.
2. The segment from the eye centre to the target point passes through the
   interior of no blocker, excluding the target cell itself.

### Range

Range is the Manhattan distance, in cells, from the eye cell to the target cell
along the resolved route. Portal crossings consume distance as in topology traversal.
The game uses a range of 16, matching the world API cap. At range 16 there
are 6,017 candidate offsets; range 8 has 833. These are geometric candidates,
not a guarantee of perception.

### Ambient illumination and local awareness

Every cell has boolean ambient illumination, independent of terrain. Authored
regions default to lit and may specify a dark default plus sparse overrides.
Occupied cells take precedence when local neighbourhoods or an eye projection
resolve the same body-chart offset differently at a one-way physical join.
The merge is independent of authored body-cell order. Streaming activation uses
this same merged actor scene; geometry needed to compute it may remain loaded
without being perceived or active.

Ordinary eye sight requires illuminated targets within range and line of sight.
Dark air remains transparent: a lit room may be seen across a dark corridor.
Target eligibility is checked before expensive occlusion tests.

Actors always perceive occupied cells and all 26 immediate neighbors around
every body cell, including vertical and diagonal neighbors. This existing local
awareness does not require illumination or eye line of sight. It is not an
unoccluded cube that grows with Awareness.

Beyond that neighbourhood, the current effective **Awareness attribute** governs
nonvisual perception of unilluminated cells, or all cells while blind. The
[Cosmere senses-range table](https://app.demiplane.com/nexus/cosmererpg/rules/senses-awareness)
is adapted to the world's five-foot cells and explicitly capped at the existing
normal sight limit; it does not increase that limit:

| Effective Awareness | Source range | Current maximum |
| --- | --- | --- |
| 0 | 5 feet | 1 cell |
| 1–2 | 10 feet | 2 cells |
| 3–4 | 20 feet | 4 cells |
| 5–6 | 50 feet | 10 cells |
| 7–8 | 100 feet or normal sight limit, whichever is smaller | 16 cells / 80 feet |
| 9+ | Unaffected by obscured senses, within normal range | 16 cells / 80 feet |

Extended nonvisual perception uses the same physical occlusion and Manhattan
geometry as sight, measured from occupied body cells in their portal-local
frames. Walls, closed doors, missing geometry and ambiguous routes still block
it. Abstract stairs do not transmit it. Darkness and blindness use the same
range, without a second reduction when both apply. Effective equipment,
conditions and restorable attribute losses participate; observation, recovery
and AI cache witnesses change with the resulting range.

This is ordinary disclosure, **not genuine sight, a discovery check, or true
sight**. It does not recognize guises, expose secret doors or ground traps,
identify invisible creatures at range, or license gaze/sight-required abilities.
Paid adjacent Search remains a separate checked action. Contact, touch and melee
reach do not grow with the disclosure range. Illuminated sight retains its own
eye origin and eligibility rather than acquiring extra body-origin viewpoints.

Physical joins carry the local and extended layers through their frames. The
merged actor-specific scene is shared by items, actors, places, navigation and
AI; load-only geometry dependencies are not disclosed automatically. The
separate illuminated eye-sight vector remains authoritative for sight-required
mechanics. Stale map memory remains memory, not current perception.

Geometric scenes and illuminated scenes share the exact occlusion implementation
but have distinct cache entries. Lighting edits invalidate only illumination
versions; geometry witnesses and geometric caches remain valid. Detached regions
carry their defaults and overrides in ordinary region records. Checkpoints,
replay and rewind preserve lighting. Streaming activates perceived regions and
loads geometry/lighting dependencies needed to resolve the scene exactly.

Every disclosed cell carries its current boolean `lit` value, independently of
why it was perceived. This lets a renderer distinguish dark local awareness
from illuminated sight without inferring illumination from visibility. Memory
retains only the last disclosed value; it is not current authority. Text and
ASCII retain their existing presentation. The 3D client mutes dark silhouettes
and gates decorative glow on current illumination. Portable light remains
item-owned: fuel, equipment/location and the exact emitter signature govern
illumination, independently of ambient lighting and item identification. General
light transport is deferred. The current save/ruleset versions reject older
incompatible saves; see [status](../status.md).

### Symmetry

These are geometric line-of-sight properties, before illumination eligibility
and body-local awareness are applied. Actor perception as a whole need not be
reciprocal:

- **Between non-opaque cells, sight is reciprocal.** The segment and the blocker
  set are the same in both directions, and neither endpoint is a blocker. This
  geometric guarantee holds on ordinary grids and bidirectionally joined
  partitions. It is not
  promised across one-way or ambiguous topology, which matches the current
  promise.
- **Between actors, eye-to-eye sight is reciprocal.** Beyond that, sight between
  actors of different heights may be one-sided. A rat under a table can see a
  humanoid's legs while the humanoid, looking from its head, can't see the rat.
- **Solid cells don't see.** Reciprocity is not defined for them.

### Stairs and physical portals

Abstract stair links are traversal links, not geometry. Standing on a stair
discloses its landing as a separate occurrence and does not
cast rays through the link. Physical z-facing portals are geometry, so sight
passes through them like any other join.

### Portals, rotations and repeated occurrences

Observer-relative topology resolution stays separate from opacity, with these invariants:

- Each candidate offset resolves to a location and frame.
- Crossings consume distance. All 24 proper rotations are supported.
- A physical cell may appear at several offsets.
- Along the straight unfolded route, tied axis-crossing orders must agree on
  the resolved location and frame. Disagreement is treated as missing geometry.
  This is not an enumeration of every arbitrary path to the same offset.

The segment test takes place in the observer's unfolded space. Blocker shapes
depend on neighbours, so face exposure is evaluated on **resolved neighbouring
offsets**, not on the neighbours in backend storage. A room split across a broad
join must still produce the same scene as the unsplit room, including its bevels.

## Disclosure and view updates

Solid cells are ordinary perceived cells with wall/material facts. Empty cells,
including headroom, are also disclosed. Clients classify floors, ceilings and
walls from those facts; there are no separate server surface probes. A missing
cell is never invented as a wall or a room boundary. ASCII merges heights into
one map, and text examines disclosed surfaces; neither changes the sight rule.

Snapshots contain complete views. Demand-driven picture updates can use deltas
relative to an exact connection base; ordered causal outcomes and readiness are
separate update families and do not require a new picture. Occurrences are matched by relative position after a frame
shift, not solely by opaque key: a physical cell can occur more than once.
Sequence gaps or base mismatches cannot be applied as partial truth. Reconnect
and rewind establish snapshot boundaries. See the [protocol reference](../reference/protocol.md)
for delta encoding and recovery, and [clients](clients.md) for atomic application.

## Exact implementations and derived caches

The reference defines geometry. Accelerated and cached builders must produce
exactly the same result. These are implementation layers, not different sight
rules selectable by a player.

1. **Reference implementation** (`World::eye_scene_reference`). A
   straightforward exact segment-against-polyhedron test. Each beveled cube is
   the intersection of at most 18 half-spaces, so testing a segment means
   clipping it against those planes in integers. This implementation defines
   correctness and serves as the test oracle.
2. **Accelerated scene** (`World::eye_scene`). The same model, made faster:
   - **Direct routes.** A route inside the eye's region whose bounding box spans
     no exit plane is a plain offset. It's computed directly, and a target
     outside the region is missing geometry.
   - **Plain steps.** Other routes are walked, but a step is looked up in the
     topology only if it leaves the region's storage or starts on an exit's
     plane in that exit's direction. Only those steps can be redirected by a
     passage or by rim projection.
   - **Per-scene caches.** Each cell's state and exposed faces are computed
     once per scene.
   - **Traversal instead of a bounding box.** A sight line tests only the cells
     whose closed cube it meets, found slab by slab along its major axis, rather
     than every cell in its bounding box.
   - **Cube first.** A blocker is tested as a plain cube before its bevels are
     looked up.

   Randomized and layout-specific tests require identical output to the
   reference.
3. **Not needed so far: precomputed occlusion masks.** On an ordinary grid, the
   sight lines a blocker at a given offset cuts are the same wherever the
   observer stands, so they could be precomputed as bitmasks per piece of the
   cube. Lines running exactly along seams between pieces make this delicate.
   The initial measurements justified layers 2 and 4; this layer remains
   deferred unless later measurements call for it.
4. **Scene cache** (`World::eye_scene`; the builder alone is
   `World::eye_scene_uncached`). Scenes are keyed by eye location, frame and
   radius. Validity is tracked per region rather than per chunk:
   - **Versions.** Door and terrain edits give their region a new version. Any
     change to regions, passages, rotations, physical portals or chambers gives
     the world a new topology version, which invalidates every scene.
   - **Dependencies.** A scene records every region a route stepped through,
     plus every region those regions' exits lead to. Routes can end in a
     neighbouring region without stepping through it, and a blocker's bevels
     can read cells there. Rim projection also reads walls on the far side of
     an exit even when the step doesn't cross. A scene is reused only while
     the topology version and all of its regions' versions are unchanged.
   - **Clones share the cache.** Games are cloned for every command's rollback
     capture and for rewind boundaries, so a cache that started empty in each
     clone would rarely be hit. Versions come from a process-wide counter, so
     two worlds that hold the same version for a region hold the same content
     there, however they diverged. Restoring a checkpoint clones one geometry
     per instance and then replaces its doors, so each restored world draws a
     new topology version. A viewpoint can retain up to four validated scene
     versions so before/after comparisons do not overwrite each other's useful
     results. Geometry, illumination and presence witnesses still govern every
     lookup; retaining a version never permits stale disclosure.
   - **Not world content.** The cache is ignored by equality and never saved,
     and a loaded world starts empty. It keeps at most 512 scene versions,
     evicting the oldest when a new scene would exceed that limit. Reaching
     capacity does not flush other useful viewpoints.

   Region detachment invalidates scenes that depend on the detached region.
   Scenes with unrelated dependencies remain reusable.

   Streaming observing points reuse the actor's combined scene, including body
   neighbourhoods and abstract stair landings. This derived cache retains at
   most 512 actors and validates geometry, illumination, residency, location,
   orientation, body and whether the actor is disclosed. Candidate clones share
   it, but validate their own witnesses; a candidate edit cannot leak a scene
   into the original game. It is omitted from saves and simulation equality.
   An unchanged resting wait therefore needs no new actor scene for its pins.

   Ground-supply eligibility uses a lazy location-membership index in that same
   actor-cache entry. The first membership request visits the scene once;
   subsequent requests with the same witness neither scan nor copy its cells.
   The full projected occurrence array is retained, including repeated locations
   and abstract stair landings. Membership proves disclosure only: supply use
   still separately requires physical body-cell touch, so a disclosed stair
   landing does not become reachable. The index shares the cache's existing
   witness, rollback, eviction and save-exclusion rules.

   Offline work counters distinguish scene construction, membership requests,
   cells visited to construct membership, and cells copied into or out of this
   cache. A scene-build count alone does not establish per-fragment lookup cost.

   Navigation also distinguishes a changed disclosure from a translated chart.
   With unchanged geometry, a uniform translation that preserves each cell's
   identity, rotation and opacity preserves its disclosed connections. It needs
   no navigation rebuild. Abstract stairs remain conservative because their
   separate landing offsets are anchored to the observer's origin.

Only the layers the measurements justify are added. Measure with an extended
`fov_bench` that covers `volume_scene` and the new builder, and compare release
builds with `scripts/perf_compare.py`. Observation cost already contributes to
open [performance work](../development/performance.md), so the change must
not make those items worse.

The accelerated resolver also proves a shared height slab for small physical
components, then omits candidate offsets outside it. The proof examines at most
32 regions and requires equal storage heights, height-preserving join rotations
and anchors, and no physical exits along that axis. Unloaded regions end their
proof branch, just as exact geometry routes cannot enter them; their presence
remains a witness so attaching them recomputes the proof. Translated heights
and larger components use the general resolver. Abstract stairs remain
outside physical sight. Successful proofs record every inspected region as a
cache-invalidation witness; these witnesses do not keep distant regions loaded.
Only actual terrain-read dependencies pin residency. Proof witnesses track region
availability separately: far-away wall, door and lighting edits do not invalidate
a height proof, while detaching or attaching an inspected region does. Topology
changes invalidate both the proof and scene.
Reference-equivalence tests cover all 24 observer frames, changed-height joins,
and detach/reload. This optimization applies to ordinary world geometry.

Straight sight segments inside a proven transparent convex region interior skip
ray traversal. The proof checks only that region's terrain overrides and doors;
solid overrides or any closed door disable it. Chamber edge/corner surfaces and paths crossing exit planes retain exact rays. An
enclosing solid face directly adjacent to the box is also proven visible; edge
and corner surfaces retain exact rays. The eye must lie inside the box, and
other targets must lie inside or immediately across one exposed face. Geometry-version invalidation and
reference comparisons cover doors and internal wall edits.


## The 2D diagnostic comparison

`World::shadow_scene` remains a comparison builder and `fov_bench` input. It is
**not gameplay perception**, does not define gameplay range or lighting, and is
not the exact 3D oracle. Its point-floor/diamond-wall model follows the geometric
ideas described by [Albert Ford](https://www.albertford.com/shadowcasting/), with
an independent integer implementation.

Four quadrant scans split angular intervals at opaque cells, compare exact
rational slopes and use signed Euclidean rounding. Floors use centers; opaque
walls/doors have beveled corners and exposed portions. Actors/items do not block
sight. Topology is resolved separately: exact-corner routes must agree on frame
and location, missing/ambiguous offsets shadow rather than become walls, and
repeated physical cells retain distinct projected occurrences. Cyclic traversal
is bounded by radius. Ordinary-region scanning is O(r²), but remote resolution
can make the full pipeline O(r³); it has no persistent sight cache.

The world API accepts a bounded radius up to 16; historical range-eight
comparisons describe that diagnostic workload, not the current game's range.
The older builder's vertical-link behavior must not be used to infer current
physical-portal sight.

On single-level maps with floor-to-ceiling walls, the current 3D builder sees all
open cells seen by the 2D comparison plus exact bevel-tip grazes. In 3D, touching
never blocks; row-scan rounding can break those ties differently. The regression
classifies every extra open cell rather than accepting arbitrary differences.
Wall-cell differences are also intentional: the 3D model requires a visible
exposed face center, while a 2D scan may count a peeking wall corner. These are
bounded comparison findings, not a substitute for the reference oracle.

## Verification and source map

- [`world/src/sight3d.rs`](../../crates/world/src/sight3d.rs) contains the exact
  reference. Its segment predicate is independently checked by an exact-rational
  oracle that partitions surface crossings and tests interval midpoints: all
  exposed-face masks and endpoint pairs on a small doubled-coordinate grid,
  including face/edge/vertex and degenerate contact.
- [`world/tests/it/sight3d.rs`](../../crates/world/tests/it/sight3d.rs) checks open
  floors/ceilings, reciprocity, exact 2D differences, low walls, lintels, pit rims,
  one-sided body sight, stairs, cycles, narrow doorway rims and all 24 rotations.
  Every 3×3 blocker pattern is exercised at one-, two- and three-cell eye heights,
  comparing scenes to the reference. Giant fixtures use sufficiently tall halls.
- [`world/tests/it/sight_cache.rs`](../../crates/world/tests/it/sight_cache.rs)
  checks cached/uncached equivalence under edits, clones and rewind, including
  dependency-specific invalidation, joins whose last cell crosses a boundary,
  and checkpoint worlds that differ only in doors.
- [`world/tests/it/lighting.rs`](../../crates/world/tests/it/lighting.rs) covers
  illumination, transparent dark space and local body awareness.
- [`simulation/tests/it/sight.rs`](../../crates/simulation/tests/it/sight.rs)
  verifies declared eyes, feet-relative output and sideways bodies.
- [simulation discovery guise tests](../../crates/simulation/src/discovery_guise_tests.rs)
  own private recognition, no subject notification, distance-band sample reuse,
  the two persisted passive opportunities, and strict contest/receipt records.
- [`test_sight_process.py`](../../scripts/test_sight_process.py) launches real
  headless/text/native ASCII clients at multiple observer heights, with waist
  walls, tall doors, hovering creatures and save/resume. Shared surface and
  protocol delta tests cover presentation and exact disclosed-state updates.
- [`world/tests/it/shadowcasting.rs`](../../crates/world/tests/it/shadowcasting.rs)
  preserves the diagnostic builder's obstacle-pattern, corner, split-room and
  bounded-cycle comparisons.

Run `cargo run --release --locked -p tor-world --example fov_bench` for scene
construction diagnostics, including topology and sorting. It compares 2D,
voxel-height, reference, accelerated and cached 3D builders at selected radii;
those timings are not pass/fail thresholds. Use
[performance methodology](../development/performance.md) for gameplay comparisons.

The [historical sight findings](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/sight-3d.md#reference-implementation-findings)
retain the September 2026 intermediate regression, door-approach optimization,
scene-cache comparisons, dense-falling overrun and unexplained save outliers.
They explain why optimizing the scene builder alone was insufficient: quadratic
approach construction and repeated unchanged observers also dominated cost.
They are dated evidence with specific workloads and measurement boundaries,
not current guarantees. Precomputed occlusion masks remain unjustified by the
accepted measurements; adding them would require new equivalence evidence.

## AI and remembered-route authority

[Autonomous decisions](../reference/ai.md) owns current AI priority, engagement,
care, attack, fallback and retreat limitations. AI uses this same authorized actor
scene; private engine identity is not a targeting grant. Admission queues a
decision without observation, path search or RNG; execution and publication obey
the [simulation cycle](simulation-cycle.md#4-select-work-respecting-session-stop-conditions).

### Known-space pathfinding and its limits

Navigation remembers only perceived walkable cells and confirmed disclosed links.
A previously known cell can become remembered opaque; never-seen solid/unknown
cells are not walkable route candidates. Link admission checks both scene
endpoints and composed physical frame. Hash sets used for membership do not
choose authoritative ordering. Remembered information can remain stale outside
current perception, so each executed move revalidates current geometry/body fit.

`RouteSearch` is a deterministic Dijkstra-style minimum-tick search over
`(Location, orientation)` nodes. Its ordered frontier stores accumulated `u128`
cost, insertion order and node; distances/predecessors are ordered maps. Edge
cost is the actor's checked movement duration. Physical link rotations and
flight-gated vertical edges matter. For a diagonal, the component routes must
produce a legal agreeing endpoint/frame when both exist; one valid side route
can suffice under the movement rule. A stationary actor expands no movement.

Target ranking and pursuit share one search across destinations within a decision.
It incrementally
expands until a requested destination is settled, then reconstructs a complete
`Vec<TravelStep>` through predecessors. Thus it is not a separate full search per
target, but repeated long destinations still reconstruct route vectors. The
algorithm finds a minimum cost in **remembered** topology; it is not an omniscient
shortest path through currently hidden changes, and route step count used by
some policy ranking is not the same quantity as accumulated movement time.

Retreat compares the same route-step quantity from the current and prospective
positions, using a separate remembered frontier for each legal disclosed candidate
and its full post-step orientation. Its [distance and limits](../reference/ai.md#retreat-distance-and-limits)
include directed escapes and unavailable current routes. This does not change the
minimum-tick search policy or establish an optimal escape planner; the extra
candidate-search cost has not been performance-qualified.

Source: [remembered routing](../../crates/simulation/src/travel.rs), `RouteSearch`,
`refresh_navigation_scene`; [retreat scoring](../../crates/simulation/src/ai.rs),
`choose_ai`; [movement contracts](../reference/movement.md).
