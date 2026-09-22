# Material volumes and enclosed rooms

New games use **diagonal-v11** and protocol **11**. Rooms and passages are
empty cells carved inside finite solid stone. Each cell is a **5-foot cube**.
The two starting rooms retain their 5-by-3-cell interiors and one-cell connecting
hall; both rooms and the hall have two empty vertical layers (10-foot clearance).
Actors stand in the lower layer. The room anchors, items, door, and travel distances
are unchanged.

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

Horizontal sight retains symmetric shadowcasting. Protocol 10 adds nullable
`floor` and `ceiling` facts to disclosed cells, each with a material name and a
positive `distance` in cells to the solid cell. A standard floor is at distance 1
downward; its top face is at the actor's feet. A standard ceiling is at distance 2
upward, placing its underside 10 feet above the actor's feet.

These facts use bounded vertical column probes from disclosed empty cells, with
the remaining eight-cell sight budget. Probes stop at the first solid material,
closed barrier, range limit, or missing storage. They do not follow stair links or
disclose hidden contents of upper cells. This is a surface-perception foundation,
not a full three-dimensional field of view or an eye-height/body model. Repeated
occurrences of a cell share its disclosed surface facts.

Text describes the stone floor and walls and supports `examine floor`,
`examine walls`, and `examine ceiling`. Examination reports the surfaces currently
disclosed; seeing some ceiling does not imply a complete roof. ASCII displays
walls around the map and floor/ceiling information above it, including the ceiling
height above the actor's feet. All clients retain last-seen surfaces separately
from current observations; headless output exposes that potentially stale memory.
Revisiting refreshes surfaces, and rewind clears abandoned-future memory.

The older `material` cell field now describes solid terrain in chambers; empty
chamber cells have an empty string. Floors and ceilings use their own fields.
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
rewind checks. Chamber authoring is supported by material-volumes-v9 and material-rims-v10 and diagonal-v11;
rulesets predating material volumes reject it.

## Timing, compatibility, and verification

Gravity, falling, digging, destruction, body clearance, and material-specific
interactions remain deferred. Actor position is the cell containing their feet;
ordinary vertical movement still requires an explicit stair/link. Empty headroom
does not grant upward movement, and removing support does not cause a fall.

Save format remains **3**, and the only supported ruleset is `diagonal-v11`.
Start a new game after a format or rules update. All connected
clients must use protocol 11. Existing launchers use `target/doors/debug` and
continue creating fresh normal saves.

World tests cover finite shells, unallocated space, vertical probe limits and
barriers, and split/unsplit equivalence across ordinary and rotated joins.
Simulation tests verify surfaces and free rejected vertical movement. Server tests
cover current rules, atomic setup, duplicate receipts, solid-cell rejection,
rewind, and restart. Client tests cover surface prose and stale memory.
`scripts/scenarios/material-volumes.json` and `scripts/test_material_process.py`
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
and verifies closing/opening and save/resume. Protocol 10 and save format 3 stay
unchanged; material-rims-v10 distinguishes corrected replay from the initial
material-volumes-v9 rules.

## Movement latency diagnostics

`cargo run -p tor-server --example latency_bench --locked` measures scene and
observation costs separately from action processing with memory-only and disk
journals. Add `--release` for optimized timings. Results are diagnostic, with no
machine-dependent pass/fail threshold.

On the development Windows machine, debug observations took about 1.2 ms.
Unbuffered JSON journal writes dominated movement latency and grew with history:
disk-backed action batches averaged 62 ms initially and 220 ms by 300 actions.
Buffering serialization reduced these to 19 ms and 24 ms respectively. A separate
real headless client/server measurement of 30 moves improved from a 39 ms median
to 23 ms. These are local measurements, not latency guarantees.

The writer explicitly flushes before the existing file sync and atomic replacement;
save format, replay rules, and acknowledgement ordering are unchanged. Existing
saves benefit immediately. Complete-journal rewriting still limits long sessions;
periodic snapshots and more efficient storage remain future work.
