# Geometry, bodies and physics

Regions are backend storage and topology, not player-visible rooms. The world
uses bounded rectangular three-dimensional regions with integer local
coordinates; a cell represents a **five-foot cube**. Joins may connect them
without a single consistent global embedding. The backend resolves each actor's
view; clients never reconstruct topology from region or portal data.

## Finite material volumes

Chambers carve empty interiors inside finite solid stone. A standard two-cell-high
room has solid floor at local z = -1, empty foot space at z = 0, empty headroom at
z = 1, and solid ceiling at z = 2: ten feet of clearance. Side walls enclose both
interior layers. The initial shell is one cell thick, so its stone is five feet
thick.

`Terrain::Empty` and `Terrain::Solid(Material)` separate occupancy from material
identity. Stone is a backend material, not a rule inferred from an appearance
string. Hardness, digging, destruction and material-specific interactions are
not implemented. Unallocated space outside a finite volume is neither empty air
nor solid stone; it does not create imaginary enclosure.

Chamber storage uses an implicit stone shell with sparse overrides. Its bounds
include negative shell coordinates; interior coordinates and join anchors stay
stable. Explicit joins replace the adjacent shell at their apertures. Making a
shell cell empty does not allocate space beyond it. Raw diagnostic rooms can
remain unenclosed and retain cosmetic material descriptions.

A split room must look like the same room stored in one volume, including its
aperture rim. Consistent face transforms project solid-to-solid,
solid-to-empty and empty-to-solid rim cells. They do **not** create implicit
empty-to-empty openings. Incompatible face transforms remain unresolved. This
invariant applies to narrow doorways, open/closed doors and every planar rotation;
a shell corner must neither substitute a wall for open space nor vanish merely
because storage is partitioned.

Doors are independent one-column entities with authored vertical height, not
necessarily portal objects. Their entire column opens/closes together. See
[door mechanics](../reference/movement.md#doors) for height validation,
occupancy and interaction rules.

## Topology, apertures and frames

Physical portals apply translation and one of the **24 proper cube rotations**.
Reflections are excluded. Transform values 0–3 are the planar quarter turns;
the stable complete table is [`rotation.rs`](../../crates/world/src/rotation.rs).
For example, transform 23 maps `(x,y,z)` to `(z,y,-x)`. An explicit `rotation`
cannot be combined with nonzero planar `turns`.

A rectangular join glues an aperture with one affine transform. All constituent
crossings are validated before the edit publishes. Horizontal apertures can
cover width and height; adjacent joins can cover irregular openings. Reverse
connections are explicit and separately validated. Cycles and self-links consume
route distance; a physical cell may have multiple occurrences in an unfolded
observer scene.

A movement input follows the actor's frame, so repeated north remains what
appeared north even when backend axes rotate. Bodies, velocity, fractional
displacement and the eye all use that persistent frame. There is no separate
facing mechanic or automatic upright rotation: a sideways portal can leave a
character horizontal.

Abstract stairs are traversal links, not physical holes. Their landings are
separate disclosed occurrences when standing on the stair; rays do not continue
through to survey the destination room. Physical z-facing portals, authored with
`kind = "portal"`, do carry geometry and sight. Omitted vertical-link kind retains
the abstract stair convention. Stairs are not gravity conduits or continuous
stair meshes.

### Developer authoring

Scenario packages are the ordinary authoring path; see
[scenarios](../reference/scenario-packages.md). In an authorized wizard save, raw
`room` and enclosed `chamber` take an ID, width, depth, height and private name.
Interior limits are 1–32 × 1–32 × 1–8; a chamber's shell is additional. IDs are
nonzero/unique and names are 1–80 UTF-8 bytes without control characters.
Private developer names do not label the player's view.

`join` takes source region/x/y/z, direction, destination region/x/y/z, planar
turns, width and height. Width follows +y on east/west faces and +x on
north/south faces; height follows +z. Up/down aperture extents lie in x/y.
Destination offsets rotate with the transform. Extents must be positive, at
most 1024 cells, and fit clear endpoints. Horizontal sources exit a region
boundary. `connect` is the one-cell form. Abstract vertical links may start
inside a region; physical z-facing portals must exit a boundary. Structured
`connect_portal` supports physical authoring.

`wall ... closed|open` sets stone or carves an allocated cell. Placement and
solid edits validate full actor bodies, items and doors. A wall may obstruct a
join without deleting its topology. All setup follows authorization, revisions,
atomic validation, receipt/replay and rewind rules and consumes no ordinary
action time. See [wizard tools](../development/wizard.md) for executable syntax;
normal gameplay never implicitly promotes a save to wizard mode.

## Bodies and occupancy

A body declaration contains distinct integer `cells`, including `[0,0,0]`, an
`eye` that is one of those cells, and positive `mass`. Limits are 64 cells and
per-axis offsets from -8 through 8. Mass uses game units; carried inventory
weight does not contribute. The default delver uses two vertical cells, eye
`[0,0,1]`, and mass 80. An omitted body in a diagnostic package is a single cell;
when a body is explicitly declared, its eye is required.

Loose item stacks occupy one cell and have mass equal to quantity. They do not
block movement and may share cells. Carried items follow their carrier.

A body may straddle regions. Every monotone topology route to each occupied
offset must agree on location and frame. Self-overlap, incomplete apertures,
obstruction or conflicting transforms reject placement. Translation checks every
occupied cell. Walking diagonals require one clear component ordering, with both
agreeing when both work; simultaneous non-walking diagonal translations check all
orderings. See [movement](../reference/movement.md#one-movement-action).
Closing a door, wall placement and teleportation validate the whole body.

Bodies are rigid. Crouching, ducking, turning, torque, bounce and crushing remain
unsupported; an actor must fit without changing posture. The bounded ability
push translates one cell with physical clearance; it does not use voluntary
support restrictions or abstract stairs and does not apply an impulse.

### Canonical melee contact

Melee reach and ability displacement share one bounded geometry query. Source
body cells and 26 neighboring deltas are examined in complete shortest nonempty
shells: cardinal, edge, then corner (squared lengths 1, 2, 3). Every component
ordering must agree on endpoint and rotation. The target cell must meet the same
ordinary visibility/local-awareness test used for melee reach.

Each shortest candidate describes the full relative affine embedding of the two
bodies, not just a contact direction. Candidates must agree on rotation and
offset. Conflicting shortest portal embeddings leave the primary reachable but
provide no push direction. Equivalent candidates use the lexicographically least
key: squared source body-offset length, squared target body-offset length, then
source XYZ and target XYZ offsets. Body declaration order and repeated equivalent
portal occurrences cannot select another outcome. Ordinary two-cell and taller
bodies therefore are not immune merely because several contacts exist.

One witness belongs to a primary and its riders. Before displacement it rechecks
the exact participant placements, orientations, body offsets/mappings and selected
route. Moved or reshaped participants invalidate only displacement; unrelated
world edits do not veto it. A prior rider cannot make the next rider silently
select a new contact. The query is bounded by 64 source cells times 26 deltas,
with at most six component orders of three hops per candidate.

Push size is separate content data: Tiny, Small, Medium, Large, Huge and Colossal
have ordinals 0 through 5. Both participants require an explicit displacement
profile; the target must not be anchored or more than one size above the source.
Source anchoring does not bar a push. Footprint, mass, hit dice, creature kind and
content names never infer eligibility. The gate and destination occupancy are
rechecked live, while the selected operation and captured contact stay fixed.

## Gravity and deterministic integration

Region `gravity = [0,0,-1]` is local-axis acceleration. Sparse
`gravity_overrides = [{ at = [x,y,z], vector = [0,0,0] }]` replace it absolutely
at cells. A field is zero or one signed axis with strength at most 1024.
Omitting gravity in diagnostic regions preserves static traversal; explicit zero
gravity enables unsupported-movement restrictions and preserves drift.

Resolve every occupied cell's field into the common body frame, sum and divide
by cell count. Opposing fields cancel, partial exposure weakens acceleration,
and perpendicular fields can produce a diagonal. Do not normalize the result.
Uniform mass distribution cancels from acceleration.

One simulation tick is one integration quantum. Position has 65,536 fixed-point
units per cell; velocity uses these units per tick; one authored gravity unit
adds 16 velocity units per tick. Signed remainders retain weak acceleration.
Semi-implicit integration updates velocity before displacement. Integer square
root and conservative scaling cap total speed, including diagonals, at 8,192
units per tick (12.5 cells per 100-tick ordinary turn). These are game tuning,
not SI physical realism.

Every crossed cell is checked. Fields are resampled at the next quantum after
translation. Physics precedes actor decisions at equal timestamps; actors resolve
in actor-ID order, then loose items in item-ID order. Simultaneous axis ties use
persistent body x/y/z order if a combined move must slide. Other actors keep their
normal recovery. Physics does not advance while waiting for player input. Inert
populations skip unused quanta; removed support is reconsidered at the next
scheduled physics quantum.

### Support, motion and impact

Any obstructed occupied cell may support the rigid body. Ordinary walking in a
gravity-enabled region requires support against at least one nonzero component
of the resultant field. Unsupported bodies may wait and manipulate inventory,
but cannot walk through air. Jumping, flight and zero-gravity push-off controls
are not implemented. Walking preserves existing velocity.

A collision clears only the obstructed velocity component and its fractional
displacement. Tangential components survive. Resting support suppresses
acceleration into the obstruction without repeated impacts. No torque or balance
is modeled, so a small ledge can support a large body. Stable identity order
resolves competing bodies; there are no simultaneous swaps or momentum transfer.

Impact records carry entity, tick, reference location, collision axis,
region-local normal, applicable other actor, incoming normal velocity and mass.
Ordinary acceleration and portal coordinate changes do not cause impact events.
For each obstructed velocity component, impact damage is
`ceil(max(0, abs(incoming_velocity) - 2048) / 512)` before the shared damage rules.
Only the moving actor takes damage; item damage and momentum transfer remain
unsupported. Damage uses the same resistance, interruption and death machinery
as combat; see [creature rules](../reference/creature-rules.md).

Dropped items inherit carrier motion/frame. Pickup absorbs their motion without
transferring momentum to the carrier. Moving stacks merge only with compatible
motion. Forced displacement or collision stops automatic travel at the next
action boundary. Routes use learned reference-cell topology, while execution
checks current whole-body clearance.

## Actor-relative disclosure

The [perception model](perception.md) combines eye-based illuminated sight and
body-local awareness. Observations locate occurrences relative to the actor's
reference cell, not a global coordinate system. The observing body is one
narrative presence, even when several of its cells are perceived. Only perceived
cells of another actor are disclosed. Own motion/collision sensations do not
expose gravity maps, hidden bodies or collision-target identities.

Solid cells carry wall/material facts. Clients derive surfaces from perceived
adjacency: open above means floor, open below ceiling, and open beside wall.
A ceiling height is reported only with a perceived open column between it and
the feet. No region dimensions, material catalog or separate hidden surface
probe are sent. ASCII merges heights into one map; text can examine currently
seen floor/walls/ceiling. Memory may retain stale surfaces; neither frontend
learns undisclosed enclosure.

Opaque actor-specific cell keys derive from the private saved salt, observer and
internal location. They are stable across movement and save/resume, and repeated
occurrences share a key. They reveal no internal region, transform or coordinate.
The saved salt stays private. A remembered relative offset describes a past
sighting, not a global position; [client architecture](clients.md) explains
alignment and reset behavior.

## Persistence, streaming and verification

Bodies, frames, velocity, displacement, gravity, integration remainders and
boundary sensations participate in checkpoint/replay/rewind. Retries never
integrate twice. Gravity tables share immutable geometry storage across actions
and rewind boundaries. Derived sight and presentation caches are not saved.
Only current save/rules formats are accepted.

Regions may be active, frozen or detached; all authored regions are not kept
running. Frozen time does not catch up on reactivation. See
[region lifecycle](streaming.md) and [persistence](persistence.md) for residency,
physics scheduling and restore ownership.

Coverage includes world split/unsplit and all-rotation equivalence, finite shells
and missing space; simulation support, drift, straddling bodies, motion and
impact; and strict save/retry/rewind tests. Actual-client suites are
[`test_material_process.py`](../../scripts/test_material_process.py),
[`test_physics_process.py`](../../scripts/test_physics_process.py), and the
portal/geometry fixtures under [`scripts/fixtures`](../../scripts/fixtures).
They verify perceived floors/ceilings, narrow rim corners, falling/landing,
hidden changes, stale memory and saved continuation.

The [historical physics measurements](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/physics.md#performance)
retain same-host September 2026 baselines, dense-falling overruns and sparse
save/resume samples. Those measurements are not proof that current latency goals
are met. The current [performance guide](../development/performance.md) owns
workloads, targets and how to reproduce comparisons.
