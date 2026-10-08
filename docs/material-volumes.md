# Material volumes and enclosed rooms

Rooms and passages are empty cells carved inside finite solid stone. Each cell
is a **5-foot cube**. In the `scenarios/two-room` package, the two 5-by-3-cell
rooms and their one-cell connecting hall have two empty vertical layers
(10-foot clearance). Actors stand in the lower layer.

| Local z | Standard room |
| --- | --- |
| 2 | Solid stone ceiling |
| 1 | Empty upper interior |
| 0 | Empty lower interior; actor feet |
| -1 | Solid stone floor |

Stone side walls surround both empty layers. The initial shell is one cell thick,
so a wall represents five feet of stone. Unallocated space outside the finite
volume is distinct from both empty cells and solid material. It does not acquire
imaginary walls, floors, or ceilings.

## World representation

`Terrain::Empty` and `Terrain::Solid(Material)` separate occupancy from material
identity. `Material::Stone` is the first backend material. Future brick or
indestructible stone can add material identities and properties without deriving
rules from client appearance strings. No hardness or destruction rules exist yet.

Chamber authoring takes an interior extent, allocates a one-cell shell on every
face, and carves the interior. Storage uses an implicit stone shell and sparse
terrain overrides. Region storage bounds include negative shell coordinates;
interior coordinates and join apertures remain stable.

Regions remain backend storage. Explicit joins replace the adjacent shell at their
apertures. A broad join must look like one continuous carved room, including the
solid rim of the aperture and rotated views from either side. Doors remain
independent, cell-sized barriers; this slice does not add multi-cell door bodies.

## Perception and clients

Sight is [three-dimensional](sight-3d.md). Floors, ceilings and walls are
ordinary seen solid cells, each with its `material`; the server sends no separate
surface facts. Clients classify a seen solid cell by the open cells seen next to
it: an open cell above makes it a floor, one below a ceiling, and one beside it a
wall (`tor_client_common::surfaces`). A standard floor is the solid cell just below
the actor's feet. A standard ceiling is two cells up, placing its underside 10
feet above the actor's feet. A ceiling is only reported when every cell between
it and the actor was seen open.

Text describes the floor underfoot and the seen walls, and supports
`examine floor`, `examine walls`, and `examine ceiling`. Examination reports the
surfaces currently seen; seeing some ceiling does not imply a complete roof. ASCII
displays walls around the map and floor/ceiling information above it, including
the ceiling height above the actor's feet. All clients remember seen solid cells
like any other cell, so remembered surfaces can be stale; headless output exposes
that memory. Revisiting refreshes them, and rewind clears abandoned-future memory.

The older `material` cell field now describes solid terrain in chambers; empty
chamber cells have an empty string. Floors and ceilings are classified from the same disclosed solid-cell material.
Legacy and raw diagnostic regions retain their cosmetic material description.
No internal region identities, dimensions, material catalog, or undisclosed cells
are transmitted.

## Wizard authoring

In a new, separately authorized wizard game:

```text
wizard chamber 3 5 3 2 Stone chamber
wizard place 3 2 1 0 on
wizard teleport 1 3 1 1 0
```

`chamber` takes the same ID, interior width/depth/height, and private developer
name as `room`. Interior limits are 1–32 by 1–32 by 1–8; the stone shell is extra.
Use the interior coordinates for `connect`/`join`, including both height layers
when joining a full-height passage. The existing `room` command remains a raw,
unenclosed diagnostic volume for earlier geometry scenarios.

Existing `wizard wall ... closed|open` sets solid stone or carves an allocated
cell, including shell cells. Ordinary play has no digging command. Removing a
shell cell does not allocate space beyond it. Placement rejects solid cells;
solid edits reject occupied actors, items, and doors. Commands consume no ordinary
action time and retain existing authority, revision, branch, receipt, replay, and
rewind checks. Chamber authoring uses the current ruleset; older rulesets are
unsupported.

## Timing, compatibility, and verification

Digging, destruction, and material-specific interactions remain deferred.
Gravity, falling, and multi-cell bodies are described in [physics](physics.md).
Ordinary vertical movement still requires an explicit stair or link; empty
headroom doesn't grant upward movement. Start a new game after a format or rules
update.

World tests cover finite shells, unallocated space, seeing floors and ceilings
through physical portals, and split/unsplit equivalence across ordinary and
rotated joins. Simulation tests verify seen floor and ceiling cells and free
rejected vertical movement. Server tests
cover current rules, atomic setup, duplicate receipts, solid-cell rejection,
rewind, and restart. Client tests cover surface prose and stale memory.
`scripts/fixtures/material-volumes.json` and `scripts/test_material_process.py`
exercise normal play plus wizard setup through real server/text/headless/native
ASCII processes, including ceiling edits, stale memory, revisit, rewind, and resume.
Existing CI discovery runs these on Windows and Linux in debug and release.

## Doorway rim correction

The initial material-volume implementation handled only solid-to-solid rim
connections. At a narrow doorway, the solid side of the hall borders empty space
in the far room. Ignoring this case substituted a shell wall for the empty cell
or made the two paths through a corner disagree, hiding the cell entirely.

The corrected rules project a consistent face transform for solid-to-empty and
empty-to-solid rims as well as solid-to-solid rims. They do not add implicit
empty-to-empty openings. Incompatible face transforms remain unresolved. The
same layout stored in one volume and across a narrow join must have identical
visible cells, with the door open or closed, from every floor cell and through
all four rotations.

The actual ASCII regression steps to the west of the door, onto it, and to its
east, asserts both floor corners and wall cells, compares headless observations,
and verifies closing/opening and save/resume.
