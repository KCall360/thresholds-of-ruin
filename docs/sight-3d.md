# Three-dimensional sight

**Status: in progress.** This is its own effort,
[3s](milestones.md#3s--three-dimensional-sight), separate from 4e. The maintainer
has authorized the protocol and save-format break it requires. Every observer
now sees with the model below, from the eye cell its body declares. Floors and
ceilings are seen solid cells that clients classify (see
[material volumes](material-volumes.md)). Still to come: view-delta updates and
the remaining client changes. When 3s is complete, this note becomes the sight
guide.

## Why the current model falls short

`Game::scene` chooses between two scene builders for each observer:

- **`shadow_scene`** is used for single-cell, static observers. It runs 2D
  symmetric shadowcasting on the observer's plane and adds only the vertical
  column through explicit stair links.
- **`volume_scene`** is used for multi-cell bodies, any active gravity or motion,
  and off-axis orientations. In the first dungeon, that means every actor. It
  keeps shadowcasting on the observer's plane and adds height slices using
  conservative voxel rays. Those rays start at the body's reference cell and are
  rejected as soon as they cross any opaque cell before the target.

Floors and ceilings are not seen as cells. Each visible empty cell carries
`floor` and `ceiling` facts from a separate straight-line probe up or down.

This produces visible defects:

- **Missing floors and ceilings.** A voxel ray to a distant floor cell must cross
  nearer floor cells first, so only the floor directly below the reference cell
  is disclosed. Ceilings behave the same way. The ASCII height panels show this
  as a single block.
- **Wrong eye position.** Sight starts at the reference cell, which for the
  default two-cell character is the feet, not the head.
- **Blocking is not modelled.** A waist-high wall, a lintel or a pit rim can't
  hide or reveal surfaces correctly, because surface facts are probes rather
  than lines of sight.
- **Two rule sets.** Observers see by different rules depending on their body
  and region, and the observer's plane is a special case inside the 3D builder.

## Goals and non-goals

Goals:

- One sight rule for every observer, used both for player observations and for
  AI perception.
- Floors, ceilings and walls are ordinary solid cells that are seen or not seen.
  Surface facts are derived by clients, not probed by the server.
- On maps whose walls run floor to ceiling, keep today's 2D shadowcasting results
  for empty cells on the eye's plane.
- Exact integer arithmetic and deterministic results, including across all 24
  portal rotations, cycles and repeated occurrences.
- Cost within the observation performance budget.

Non-goals: lighting, sound, transparency or partial visibility, posture changes
of the eye, and continuous stair meshes.

## Model

All geometry uses **doubled coordinates** relative to the eye. Cell `(x, y, z)`
has its centre at `(2x, 2y, 2z)` and its faces on the odd planes `2x ± 1`,
`2y ± 1` and `2z ± 1`. Every point used below is then an integer point.

### Eye

A sight line always starts at the **centre of one eye cell**. The body
definition names that cell:

- `BodySpec` gains an `eye` offset, which must be one of its `cells`.
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

Actors and items are disclosed when a cell they occupy is visible, as they are
today. An actor is visible if any of its body cells is visible.

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
  passes between them, as it does today.
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
along the resolved route. Portal crossings consume distance as they do today.
The game uses a range of 8, and the world API caps it at 16. At range 8 there
are 833 candidate offsets, compared with about 145 on a single plane.

### Symmetry

What this model promises:

- **Between non-opaque cells, sight is reciprocal.** The segment and the blocker
  set are the same in both directions, and neither endpoint is a blocker. This
  holds on ordinary grids and on bidirectionally joined partitions. It is not
  promised across one-way or ambiguous topology, which matches the current
  promise.
- **Between actors, eye-to-eye sight is reciprocal.** Beyond that, sight between
  actors of different heights may be one-sided. A rat under a table can see a
  humanoid's legs while the humanoid, looking from its head, can't see the rat.
- **Solid cells don't see.** Reciprocity is not defined for them.

### Stairs and physical portals

Abstract stair links are traversal links, not geometry. They keep today's rule:
standing on a stair discloses its landing as a separate occurrence and does not
cast rays through the link. Physical z-facing portals are geometry, so sight
passes through them like any other join.

### Portals, rotations and repeated occurrences

Observer-relative topology resolution stays separate from opacity, as it is now:

- Each candidate offset resolves to a location and frame.
- Crossings consume distance. All 24 proper rotations are supported.
- A physical cell may appear at several offsets.
- If two topology routes to the same offset disagree, the offset is treated as
  missing geometry.

The segment test takes place in the observer's unfolded space. Blocker shapes
depend on neighbours, so face exposure is evaluated on **resolved neighbouring
offsets**, not on the neighbours in backend storage. A room split across a broad
join must still produce the same scene as the unsplit room, including its bevels.

## Disclosure and protocol

- Visible solid cells are disclosed as cells, with their wall flag and material.
- Clients derive floors and ceilings from the solid cells they have seen. The
  `floor` and `ceiling` surface fields and the server's surface probes are
  removed.
- Text descriptions such as `examine floor` and `examine ceiling` use the seen
  solid cells directly below and above the player's column.
- Seen empty cells are disclosed too, including bare headroom, so clients know
  which space has been explored.
- To limit the extra volume, observation updates carry **view deltas**: cells
  that entered or left view, or changed, relative to the previous observation on
  the same connection. Snapshots stay complete. The existing sequenced stream
  already has what deltas need: a sequence gap, a reconnect or a rewind forces
  a fresh snapshot. The delta encoding (for example, add and remove sets keyed by
  opaque cell key, or run-length spans per column) is chosen by measuring
  message size and client apply time.
- Changing the ASCII layout is out of scope for this note. Until then, the
  existing height panels draw whatever is disclosed.

This is an authorized compatibility-breaking change to both the protocol and the
save format: the observation shape changes, updates become deltas, and `BodySpec`
gains `eye`. Following the compatibility policy, older saves are rejected rather
than migrated, and version bumps are recorded in the roadmap.

## Performance plan

Speed-ups are built and verified in layers. Each faster layer must produce
exactly the same results as the layer below.

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
   Layer 2 already beats the current builder, so this layer is deferred unless
   measurements call for it.
4. **Scene cache, if needed.** Scenes are keyed by eye location and frame. Each world chunk
   has a change counter, increased by door changes and terrain edits. A cached
   scene is valid while every chunk within range plus one cell is unchanged. The
   extra cell is needed because bevels depend on neighbours. The cache is
   per-region, dropped when a region unloads, rebuilt on demand, and never saved.

Only the layers the measurements justify are added. Measure with an extended
`fov_bench` that covers `volume_scene` and the new builder, and compare release
builds with `scripts/perf_compare.py`. Observation cost already contributes to
open [performance items](performance-persistence.md#open-work), so the change must
not make those items worse.

## Verification plan

- **Geometry predicates:** exhaustive small-case tests of segment-against-
  beveled-cube tests, including grazing, edge and vertex contact.
- **Equivalence with 2D shadowcasting:** random single-level maps with walls
  from floor to ceiling, compared with `shadow_scene` for non-opaque cells on the
  eye's plane. Differences in which wall cells are seen are reported, then either
  accepted as intended or fixed.
- **Reciprocity:** every pair of non-opaque cells in random 3D volumes, on
  ordinary grids and on rotated split rooms viewed from both sides.
- **Observer heights:** every scenario case runs with three observers: a
  one-cell creature, a two-cell humanoid with `eye = [0,0,1]` (the most
  important case), and a three-cell giant with `eye = [0,0,2]`. A standard room has only two
  cells of headroom, so giant fixtures use halls at least three cells tall.
  There, the giant looks over obstacles that block a humanoid. Most diagnostic scenarios omit `body` and so only test one-cell
  observers. New diagnostic fixtures declare two-cell and three-cell observers,
  and the sight process tests drive real clients with a two-cell observer.
- **Scenario cases:** full floor and ceiling in an open room, waist-high wall,
  lintel, pit rim, airborne actor, observers of different heights seeing each
  other (including one-way sight), closed and open doors, diagonal blockers,
  stair landing, physical z-facing portal, sideways portal with a tall body
  lying horizontally, cycles with repeated occurrences, and missing geometry
  never disclosed.
- **Existing suites:** adapt the world sight tests (`shadowcasting.rs`,
  `visibility.rs`, `scene.rs`, `rotations.rs`, `materials.rs`,
  `place_hints.rs`), including the exhaustive 512-pattern test, and the process
  tests that drive real clients.
- **Acceptance:** the 4d dungeon, checkpoint, retry, rewind, disclosure and
  native-client tests, with their performance requirements.
- **Accelerated layers:** randomized comparisons against the reference
  implementation, including portals and door edits for cache invalidation.

**Coverage so far.**

- *World:* `crates/world/tests/sight3d.rs` covers:
  - open-room floors and ceilings at one-, two- and three-cell eye heights
  - the waist wall, head-height air, and one-way sight
  - reciprocity, and equivalence with 2D shadowcasting
  - all 24 join rotations, the narrow rotated doorway, the vertical portal shaft,
    frame rotation, and stairs
  - door heights, and the accelerated builder matching the reference
- *Game:* `crates/simulation/tests/sight.rs` checks that sight starts at the
  declared eye cell, with offsets kept at the feet, for an upright humanoid and
  for a body lying sideways after a rotated portal.
- *End to end:* `scripts/test_sight_process.py` drives the real server with
  headless, text and ASCII clients through a two-cell character's view over a
  waist wall, a hovering creature, a two-cell door, and save and resume.
- *Server:* repeated occurrences through a portal loop stay disclosed through
  the server (`crates/server/tests/place_hints.rs`).
- *Not yet covered:* lintels and pit rims, three-cell observers outside the
  open-room world test, and a three-cell scenario package.

## Decisions

1. **Wall-cell differences are expected to be accepted.** The equivalence tests
   list every difference from today's shadowcasting for review. Differences
   aren't hidden, and a surprising one is fixed rather than accepted by default.
2. **Every body declares `eye`.** There is no default.
3. **Empty cells are disclosed,** and delta updates keep their volume down.
4. **The protocol and save-format break is authorized,** and the work is its own
   effort, separate from 4e. The 4e preload horizon must still reach 8 cells
   vertically from the eye as well as horizontally.

5. **Doors have authored heights.** A one-cell door in a two-cell-high doorway
   let a humanoid, looking from head height, see over it. A door now has a
   `height`, and package validation rejects one that leaves its walled doorway
   open above it. An automatic "fill the opening" rule was tried and dropped: it
   gave tall doors in low walls under open space; see [doors](doors.md).

Still open: the delta encoding, and whether the accelerated layers are needed.
Both are settled by measurement.

## Reference implementation findings

The reference is `World::eye_scene`, in `crates/world/src/sight3d.rs`, with
tests in `crates/world/tests/sight3d.rs`. It isn't wired into gameplay yet.

- **Portals and rotations.** A room split across a join looks identical to the
  unsplit room from both sides under all 24 cube rotations, including sideways
  and upside-down storage. The same holds for a narrow rotated doorway with its
  door open or closed, and for a shaft split by a physical vertical portal.
  Rotating the observer's frame rotates offsets and changes nothing else.
  Abstract stair links are never seen through; the physical ceiling above a
  stair is seen instead. A vertical join is only see-through when it's built as
  a physical portal.

- **Empty cells compared with 2D shadowcasting.** Across 2,000 random
  single-level maps (about 97,000 open cells seen by Ford), 3D sight saw every
  open cell Ford saw, plus 619 more (0.6%). Every extra cell is on a line that
  passes exactly through a wall's bevel tip (a face centre). Touching never
  blocks in 3D, while Ford's row scan breaks these exact ties by rounding, and
  not consistently: one line in the sample passed one tip and was stopped at
  the next. The comparison test asserts exactly this: 3D sees a superset of
  Ford's open cells, and every extra cell grazes a bevel tip.
- **Walls.** Visible wall counts are similar (30,101 with Ford and 30,345 in 3D
  on the 11×11 sample), with differences in both directions. Ford counts a wall
  whose corner peeks out from behind another; 3D requires a visible face centre.
- **Speed.** Release build, range 8, mean time per scene, from `fov_bench`:

  | Case | 2D shadowcasting | Current voxel builder | 3D reference | 3D accelerated |
  | --- | --- | --- | --- | --- |
  | Room, humanoid eye | 10 µs | 307 µs | 881 µs | 124 µs |
  | Three-cell hall, giant eye | 13 µs | 426 µs | 1,028 µs | 166 µs |
  | First-dungeon layout, room centre | 28 µs | 470 µs | 1,133 µs | 208 µs |
  | First-dungeon layout, in a doorway | 25 µs | 452 µs | 1,125 µs | 271 µs |

  The accelerated scene is about twice as fast as the voxel builder the dungeon
  uses today. It's still slower than 2D shadowcasting, which sees far less.
  Profiling showed that routes across joins dominated until plain steps
  skipped the topology lookups. Gameplay-level timings come from
  `scripts/perf_compare.py` once 3D sight is wired in.
