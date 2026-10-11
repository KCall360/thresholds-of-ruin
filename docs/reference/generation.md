# Generation recipes and groups

Use this reference when authoring deterministic generated terrain.
[Scenario packages](scenario-packages.md) owns validation, source integrity and
recovery; [region lifecycle](../architecture/streaming.md) owns residency and active time.
There are two generators: independent `rooms` regions and recipe-backed `group`
regions. The examples below are package fragments; the linked fixtures provide
the surrounding manifest, region declarations and validation artifacts.

The exploratory Rogue package uses one generation group per floor. Each group
owns nine independently streamed regions. Generation prepares a complete floor;
publishing it creates ordinary detached region records in stable member order.
Only the settled loading horizon and simulation pins attach members. Membership
does not extend either horizon or keep terrain resident.

Recipes are versioned, typed declarations. Version 1 has six ordered, uniquely
named stages: stone fill, grid partition, rooms, connected graph, corridors, and
stairs. Version 2 appends room lighting with configurable `darkness_roll`
(1–10,000) and `darkness_start` (depth offset). Earlier geometry stages keep their
version-1 random streams. Stage streams derive from the game seed, group identity, stage identity
and version, and semantic inputs. Generation performs no I/O and allocates no
runtime record identities. Worker scheduling does not affect generated output.

Each floor has nine 26×7×2 regions in a 3×3 grid. Rooms retain a one-cell slot
margin. A random spanning tree connects orthogonal neighbors, with up to two
extra edges. Corridors have one-cell width and two-cell clearance. Full reciprocal
boundary joins preserve stone: carving determines where bodies can cross.

## Independent rooms regions

An independent region file with a `[generate]` table authors only the region's structure:
`id`, `name`, `size`, optional `zone` and `chamber`, its entry `anchors` and its
`portals`. Walls, openings, places, doors, items, actors and gravity overrides
are refused there. The generator fills the region the first time it's built:

```toml
[generate]
generator = "rooms"
version = 2
salt = 0 # optional; omitted means zero
rooms = [3, 6]
stair_anchors = ["up", "down"] # optional generated named endpoints
actors = { archetypes = ["rat"], ai = "wander", count = [1, 3] }
items = { archetypes = ["coin"], count = [2, 4] }
```

- `rooms` (version 2) keeps a clearing two cells wide
  around every authored anchor, places 1–16 rooms, and joins the anchors and rooms in
  turn with corridors, so every entry reaches every other. Two entries on the
  same row are joined by a straight corridor along it. Actors (up to 64, each
  an archetype from the pool, run by the named AI profile) and items (up to
  64) go on open floor away from the entries.
- Generation uses the game's seed, stable region id, generator name/version,
  explicit `salt`, and normalized semantic parameters. Comments, whitespace,
  table ordering and omitted/default-zero salt do not reroll content. Raw file
  hashes remain mandatory integrity identities for validation and pinned saves.
  Geometry and placement streams use bounds, ordered anchors and room ranges;
  population and loot each use their own pool. Changing one pool cannot reroll
  geometry or the other pool, and build order does not enter any stream.
- Generated stair anchors use an independent stream per name and are placed on
  distinct open floor cells, away from authored anchor cells. Population and
  loot settings do not change stair placement or terrain. Spawned actors/items
  avoid stair cells. Names must be unique and must not shadow authored anchors;
  insufficient floor space is a construction error. Generated names can be used
  by stair pairs and portal destinations, but character starts and objectives
  still require authored anchors. Structural inspection resolves a generated
  anchor's region, not its as-yet-unknown coordinates.
  A generated region may have only generated stair anchors, without fixed entries.
- Open floor outside entry clearings is shuffled deterministically and split
  into alternating actor/item placement lanes. These disjoint lanes keep their
  positions and capacities even when a pool is absent or its count changes.
  Counts are sampled between the authored minimum and the smaller of the
  authored maximum and that lane's capacity. A minimum above capacity is a
  contextual construction error; generation never silently drops below it.
  Validation samples seeds 0, 1 and 42 and does not prove capacity for every
  possible seed.
- Its actors and items take identities from a range of 256 fixed
  by its region id, above every authored identity; generated region ids are
  at most 65,536, and a game reserves the whole space. So identities don't
  depend on build order either, and the preloader can build generated regions
  ahead of need.
- Declaring a generated region (see
  [region streaming](../architecture/streaming.md#never-built-regions-and-region-sources))
  runs its generator to learn the identities it will hold. Authored character
  starts remain declared alongside generated inhabitants, so selecting a
  character starting in a generated region works during lazy startup.
- Saves copy a generated region's file like any other, and replay regenerates
  it from the copy.
- The validator checks each generated region at seeds 0, 1 and 42:
  generating it twice gives the same result, and its entries are connected.
  Building the whole package, as validation does, also checks its links.

`scenarios/tests/generated-filler` has two authored halls around two
generated caves.

## Recipe and group declarations

The complete [Rogue manifest](../../scenarios/rogue-exploration/scenario.toml)
is a validated example of a version-2 recipe, nine-member floors and named
stair pairs. The first recipe and group declaration are reproduced here;
this fragment requires the corresponding region files and later floor groups.

```toml
[generation_recipes.exploration]
version = 2
[[generation_recipes.exploration.stages]]
id = "stone"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "stone_fill"
[[generation_recipes.exploration.stages]]
id = "slots"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "grid_partition"
[[generation_recipes.exploration.stages]]
id = "rooms"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "rooms"
width = [4, 24]
height = [3, 5]
[[generation_recipes.exploration.stages]]
id = "connections"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "connected_graph"
extra = [0, 2]
[[generation_recipes.exploration.stages]]
id = "corridors"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "corridors"
[[generation_recipes.exploration.stages]]
id = "stairs"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "stairs"
[[generation_recipes.exploration.stages]]
id = "lighting"
version = 1
[generation_recipes.exploration.stages.operation]
kind = "room_lighting"
darkness_roll = 10
darkness_start = 1
[generation_groups."floor-1"]
depth = 1
recipe = "exploration"
members = [1, 2, 3, 4, 5, 6, 7, 8, 9]
```

A member uses `[generate]` with `generator = "group"`, `version = 1`, its
`group` name, `rooms = [1, 1]`, declared `boundary_anchors` and any
`stair_anchors`. See [region 1](../../scenarios/rogue-exploration/regions/1.toml)
for a complete declaration with reciprocal boundary joins. Group membership,
recipe identity and region sources must agree; arbitrary runtime callbacks and
network dependencies are not recipe stages.

Stage IDs are unique, bounded ASCII alphanumeric/hyphen/underscore names.
Every current stage uses version 1, even in a version-2 recipe. The operation
sequence is fixed; recipe version is not permission to reorder it. Room width
and height ranges are ordered, with each bound from 3 through 30; extra-edge
ranges are ordered and at most 4. These are validation bounds; the bundled
floor's geometry imposes its own fit constraints. See
[recipe validation](../../crates/server/src/generation_recipe.rs).

## Preparation and publication

The ordinary preparation horizon requests a group when any member enters it.
One worker deduplicates requests and prioritizes demand. The 32-record preparation
budget charges nine records per floor. Obsolete speculative results are dropped;
demand can displace speculation. A demand waits for its specific result. Worker
startup failure and recovery use the same deterministic synchronous generator.

Prepared output remains speculative until a member enters the actual loading
horizon at a candidate transition. All nine validated records publish together.
Generated records and exact source dependencies persist in the journal transaction,
including transactions without a checkpoint. Ordinary lifecycle records are the
only terrain state; there is no original generated-terrain store.

## Named stair destinations

A group-generated stair stores its destination region and anchor name. It is
visible and contributes its structural edge before destination coordinates exist.
Preparing the current floor reads only its own nine sources; it does not inspect
or partially generate the next floor.

Publication of the destination floor registers its immutable named anchor
coordinates. Movement resolves the logical reference through that structural
directory, while ordinary body clearance, occupancy, and movement timing decide
whether traversal succeeds. Undisclosed destination terrain remains private.
Coordinates survive region detachment, so resolving a destination does not require
rewriting, attaching, or retaining the source floor. The source's named link travels
with its ordinary region record.

The `rogue-exploration` package contains 26 reversible floors, starts on floor 1,
and has no upward exit on floor 1 or downward stair on floor 26. Room lighting uses `darkness_roll = 10` and `darkness_start = 1`: a room is dark
when a zero-based roll is below depth minus one. Corridors outside room footprints
remain dark. Lit rooms include both open body-height cells and their enclosing
solid surfaces. A lighting-only edit does not change corridors or stair positions.
Creatures, items, survival mechanics, and victory are reserved for later increments.

## Verification and diagnostics

Generation validates connected carving, a solid exterior, reciprocal face mappings,
and two-cell clearance before accepting a floor. Tests cover independent region
eviction with terrain edits, journal-only and checkpoint recovery, all 26 floors
in both directions, generation rewind within the ordinary retained window, and
authored entrances linked to generated anchors. Actual clients exercise headless
exploration, adventure text, native ASCII stairs, spectators, and reconnects.
Synchronous demand, settled preparation, and racing preparation produce identical
observations, lifecycle membership, and saved region rows, with journal-only and
per-command checkpoint saves. Player and spectator process responses keep
generation metadata and undisclosed floor identifiers private.

Run `cargo run -p tor-server --release --example rogue_bench -- 5` for JSONL
diagnostics. The workload measures pure generation of all floors, then the same
first-floor expedition with synchronous demand, settled background preparation,
and racing background preparation, each in memory and with durable storage.
Acquisition profiles separate group waiting and synchronous generation inside the
ordinary transition timing. A committed group costs exactly nine records; the
preparation budget remains 32 record equivalents. Residency follows ordinary
horizons and pins, including both ends of a stair. Timing samples are diagnostic;
the report validator and process test enforce completeness and work-count bounds.

Durable flush barriers include queued journal and region-row persistence and
must be reported separately from expedition commands. Short flush distributions
have not resolved the existing save-tail findings. Historical lighting/range
results and their workload boundaries are retained in
[performance evidence](../development/performance.md). They are diagnostics, not a
matched prior-version speedup or proof of an accepted performance target.

Implementation and coverage: [recipe compiler](../../crates/server/src/generation_recipe.rs),
[group integration tests](../../crates/server/tests/it/generation_groups.rs) and the
[Rogue benchmark](../../crates/server/examples/rogue_bench.rs). See
[project status](../status.md) for acceptance and remaining Rogue scope.
