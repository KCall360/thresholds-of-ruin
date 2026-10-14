# Generation recipes and groups

Use this reference when authoring deterministic generated terrain.
[Scenario packages](scenario-packages.md) owns validation, source integrity and
recovery; [region lifecycle](../architecture/streaming.md) owns residency and active time.
There are two generators: independent `rooms` regions and recipe-backed `group`
regions. The examples below are package fragments; the linked fixtures provide
the surrounding manifest, region declarations and validation artifacts.

Generation has three authoring forms: independent `rooms` regions, explicit
bounded generation groups, and compact `grid_chain` families. The family form
stores a descriptor rather than a region document, group entry or index row per
possible address. It currently generates rectangular two-dimensional room grids
with two-cell body clearance. Authored maps retain arbitrary supported 3D geometry
and irregular portal/stair graphs. Neither form is a runtime scripting system.

A generation group owns independently streamed regions. Preparation builds one
complete bounded group; publication installs its ordinary region records in stable
member order. Geometry-only group membership does not extend active/loading
horizons or retain members in memory. A declared first-entry population profile
adds a load-ready requirement for its complete bounded, unactivated scope before
an actor can enter; it adds no active pins. Revisited terrain comes from mutable
saved records, never a fresh generation pass.

## Compact grid-chain families

This manifest fragment defines a hundred groups with four slots each, without
writing four hundred region files. The starting character can use
`expedition/1/start`.

```toml
[generation_recipes.caves]
kind = "grid_rooms"
version = 1
room_width = [4, 10]
room_height = [3, 5]
extra_connections = [0, 2]

[procedural_worlds.expedition]
kind = "grid_chain"
version = 1
base_region = 1
grid = [2, 2]
size = [12, 7, 2]
recipe = "caves"
domain = { kind = "finite", count = 100 }
```

`kind` and `version` are explicit contracts. The supported family algorithm is
`grid_chain`, version 1. `grid` gives columns and rows, each from 1 through 8;
slots use row-major order. `size` bounds each region, with width/height 5–32 and
exactly two vertical cells. Room maximum ranges must fit the one-cell slot margin.
An optional `zone` supplies the ordinary region theme/terrain policy. Family IDs
are nonempty, at most 40 bytes, and contain no slash or control characters.

The three domain variants are:

- `finite`: `count` is an explicit positive group count.
- `seeded_finite`: `min` and `max` are positive ordered bounds; a dedicated
  deterministic stream selects a count once for the world's seed.
- `open`: `max_depth` is an explicit operational materialization budget, not a
  generated final floor or a victory condition. The structural address domain
  continues beyond this budget. An operation requiring generation outside the
  budget fails without committing a partial group or silently creating an endpoint.

Integer depth and region-handle arithmetic is checked. Finite families reserve
their maximum possible handle interval; open families reserve the representable
positive `u32` depth interval. Intervals cannot overlap each other or authored
region IDs. Reserving an interval allocates no region stubs or entity identities.
For depth `d` and zero-based slot `s`, the stable handle is
`base_region + (d - 1) * columns * rows + s`. Changing the seed never renumbers a
logical address. `family/1/start` resolves a seed-selected owner and interior
room cell using independent entry streams. Its room is carved around that
reserved cell, guaranteeing a safe entry distinct from the stair endpoints.
Use the logical endpoint rather than assuming the first slot owns the start.

Within a group, reciprocal complete-face boundary joins are derived once from
the grid. A seeded spanning tree connects rooms through orthogonal neighbors;
extra connections add loops. Corridors are one cell wide and two cells high.
Carving preserves the exterior stone boundary. The chain links adjacent groups
through named up/down ports. Port-region selection samples the group's slots;
port-cell selection samples that slot's generated open room cells. Endpoint
positions are distinct from each other and reserved entry cells.

### Connecting authored maps and families

Explicit `stair_pairs` compose handcrafted maps with generated ports:

```toml
[stair_pairs.hub_entry]
upper = "1000/stair"
lower = "expedition/1/up"

[stair_pairs.chamber_exit]
upper = "expedition/last/down"
lower = "1001/stair"
```

Here regions 1000 and 1001 are authored outside the family's reserved interval.
Logical endpoint syntax is `family/depth/port`; `depth` is a positive decimal
integer, or `last` for a finite or seed-selected finite domain. `up` and `down`
resolve to the seeded slot before terrain exists. Both named ports are reserved
on every group, so an otherwise unconnected first or last port can link to an
authored map. `family/1/start` names the independently seeded initial room entry;
its owner is not necessarily the first slot.
The default chain already occupies interior downward/upward exits; an explicit
pair cannot assign the same port direction again. There is no last-writer-wins
connection override. The `region/anchor` syntax remains available for anchors
with known ownership; logical references avoid assuming seeded port owners.

Structural connections are merged into the ordinary private streaming graph.
Only the requested group's cells are generated; coordinates of a later port are
resolved through its named anchor after that group publishes. Leaving a generated
group for an authored chamber uses the same traversal and persistence machinery.

Family-domain selection, port-slot selection, room geometry, graph connections,
corridors, stair-cell selection and optional lighting use separated deterministic
streams. Family/group identity, seed and relevant normalized parameters participate;
source comments, unrelated catalogs, exploration order and worker timing do not.
Changing population tables does not reroll terrain or stairs.

Structural description, address inversion, neighbors and group-input resolution
are pure bounded queries. The runtime retains the immutable descriptor and looks
up only requested neighborhoods; it never expands the potential world into an
index at startup. Procedural horizon queries have a 16,384-region and 131,072-link work budget,
including roots and shallow leaf layers. Loading horizons do not materialize past
an open family's operational budget; the downward structural connection remains
present, and trying to generate beyond it fails rather than ending the world.
Descriptors and structural topology remain backend-only and are not map disclosure.

Validation checks domain arithmetic, nonoverlapping handles, grid bounds, recipe
fit and input closures statically. It validates every authored region with bounded
initial procedural closures, then samples at most four depths per family for
seeds 0, 1 and 42, repeating generation for equality. A 4,194,304-cell budget preflights all repeated initial closures and samples
across selected characters and seeds before carving; it limits this work independently of the potential world size. The whole validator additionally preflights a 67,108,864-cell-work budget:
authored volumes include chamber shells, every character selection and repeated
seeded builds; generated-region checks and procedural samples are included. Each
explicit group is generated once per comparison side, not repeatedly per member.
These limits bound validation work before carving or publishing artifacts.
This is structural validation and sampled construction, not exhaustive proof of
all generated worlds or gameplay.

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
- Saves pin compiled generated-region inputs; replay regenerates from those
  immutable inputs rather than reparsing live authoring files.
- The validator checks each generated region at seeds 0, 1 and 42:
  generating it twice gives the same result, and its entries are connected.
  Building the whole package, as validation does, also checks its links.

`scenarios/tests/generated-filler` has two authored halls around two
generated caves.

## Recipe and group declarations

Use the typed `grid_rooms` preset above for ordinary authoring. Optional lighting
uses `lighting = { darkness_roll = 10, darkness_start = 1 }`; a room is dark when
a zero-based roll is below its group depth minus `darkness_start`. The roll divisor
is 1–10,000. Lighting does not change geometry or stair positions.

Explicit bounded groups remain useful for handcrafted layouts and engine fixtures:

```toml
[generation_groups.chamber]
depth = 1
recipe = "caves"
grid = [2, 2]
members = [1, 2, 3, 4]
```

The member list must have exactly `columns * rows` distinct nonzero IDs, and
members must have equal bounds. An omitted explicit-group grid uses the 3×3 preset.
This default does not constrain the generic public group model to nine members.
Explicit members have region sources; compact families do not.

A member uses `[generate]` with `generator = "group"`, `version = 1`, its `group`
name, `rooms = [1, 1]`, declared `boundary_anchors` and `stair_anchors`. The compiler
validates exact reciprocal grid faces, matching membership and recipe identity.
Group geometry is not an arbitrary graph callback.

The compiler lowers `grid_rooms` into private stable stage identities. The strict
low-level fixture representation remains available: recipe version 1 has stone
fill, grid partition, rooms, connected graph, corridors and stairs; version 2 adds
lighting. Stage order is fixed by this algorithm, not freely composable. Unique
stage IDs are bounded ASCII alphanumeric/hyphen/underscore names, each stage uses
version 1, room-range bounds are 3–30, and extra-edge ranges are ordered and at most
4. See [recipe validation](../../crates/server/src/generation_recipe.rs).

## Preparation and publication

The ordinary preparation horizon requests a group when any member enters it.
One worker deduplicates requests and prioritizes demand. The 32-record preparation
budget charges the actual member count per group, together with a bounded cell-work
budget. Oversized groups use deterministic synchronous demand instead of exceeding
the retained preparation budget. Obsolete speculative results are dropped;
demand can displace speculation. A demand waits for its specific result. Worker
startup failure and recovery use the same deterministic synchronous generator.

Prepared output remains speculative until a member enters the actual loading
horizon at a candidate transition. All validated records in that group publish together.
Generated records and exact source dependencies persist in the journal transaction,
including transactions without a checkpoint. Ordinary lifecycle records are the
only terrain state; there is no original generated-terrain store.

The compiler, runtime catalog, persistence identity lookup and preparation costs
share a pure `FamilyCatalog` for logical endpoint parsing, address ownership,
seed-dependent bounds and structural connections. It borrows immutable descriptors
and retains no generated metadata cache.

### Operational availability and pins

The server exposes only a pure source-availability decision to the simulation's
existing record store. Ordinary stores allow preparation. A source may defer an
unresolved named-stair destination that it cannot materialize within its immutable
operational budget. The structural stair remains present, and its destination
stays declared but unbuilt. Waiting, moving around the current group and returning
to already available groups remain valid; an attempted unavailable descent cannot
partially publish terrain or advance the failed action.

This policy does not relax body, live-target, sight, reach, physical-passage,
coordinate-resolved stair or explicitly requested destination requirements. Those
remain mandatory, even when the source says it cannot prepare new content. Missing
required records fail the candidate transition. The policy contains no procedural
family logic in simulation, performs no I/O and mutates no caches or state.

## Named stair destinations

Version 1 composes authored regions and procedural families through explicit
`stair_pairs` declarations. Authored physical `portals` may connect arbitrary
authored regions, but cannot target a procedural family, including through its
numeric handles. The compiler rejects that unsupported combination and directs
authors to named stair pairs. Synthesized physical joins inside each grid group
remain part of its deterministic generator.

A group-generated stair stores its destination region and anchor name. It is
visible and contributes its structural edge before destination coordinates exist.
Preparing an explicit group reads only its own member sources. A descriptor group
uses its pinned descriptor and recipe. Neither path carves the next group.

Publication of the destination floor registers its immutable named anchor
coordinates. Movement resolves the logical reference through that structural
directory, while ordinary body clearance, occupancy, and movement timing decide
whether traversal succeeds. Undisclosed destination terrain remains private.
Coordinates survive region detachment, so resolving a destination does not require
rewriting, attaching, or retaining the source floor. The source's named link travels
with its ordinary region record.

The [Rogue starter package](../../scenarios/rogue-exploration/scenario.toml)
selects an open domain, a 3×3 grid of 26×7×2 regions, and depth-dependent room
lighting. Its single descriptor replaces per-floor groups, region stubs and
authored descent pairs. Initial entry and stair owners/cells vary with the seed;
descent continues beyond depth 26. The operational limit of 1,000,000 floors is
an execution budget, not a terminal floor or a victory condition. No unvisited
floor geometry is generated to establish that limit.

These choices are scenario data, not requirements of the generic family API.
The [accepted adaptation](../plans/rogue.md#adaptation-decisions) deliberately
retains persistent floors and paired return stairs; Rogue itself regenerates
floors and chooses arrivals independently of the stair cell. The package now
authors a finite initial emu/snake and bandage/ration profile, separately from
geometry preparation; see [floor activation](scenario-packages.md#first-entry-population)
and the [Rogue play guide](../play/rogue.md). The full creature/item catalog,
survival mechanics, calibrated depth pressure and Amulet/victory remain unfinished. In the reference,
depth 26 starts Amulet eligibility while it is not carried; it does not end descent.

## Verification and diagnostics

Generation validates connected carving, a solid exterior, reciprocal face mappings,
and two-cell clearance before accepting a floor. Tests cover independent region
eviction with terrain edits, journal-only and checkpoint recovery, explicit
first/interior/terminal groups and compact open-domain boundaries, generation
rewind within the ordinary retained window, and
authored entrances linked to generated anchors. Actual clients exercise headless
exploration, adventure text, native ASCII stairs, spectators, and reconnects.
Synchronous demand, settled preparation, and racing preparation produce identical
observations, lifecycle membership, and saved region rows, with journal-only and
per-command checkpoint saves. Player and spectator process responses keep
generation metadata and undisclosed floor identifiers private.

Run `cargo run -p tor-server --release --example rogue_bench -- 5` for JSONL
diagnostics. Workload `rogue-descriptor-v2` measures pure generation at sampled
depths 1 through 26 of the open `dungeon` family, independently of its operational
depth limit. These are finite benchmark samples, not a terminal floor count.
Membership, chart dimensions, and actual start/down locations come from the
seeded descriptor and generated anchors. It then measures the same
first-floor expedition with synchronous demand, settled background preparation,
and racing background preparation, each in memory and with durable storage.
Acquisition profiles separate group waiting and synchronous generation inside the
ordinary transition timing. A committed group costs its actual member count in records; the
preparation budget remains 32 record equivalents. Residency follows ordinary
horizons and pins, including both ends of a stair. Residency counters include
finite authored regions and only actually declared or
materialized family regions, without enumerating unvisited open-domain potential
or reporting it as an unbuilt count.
Timing samples are diagnostic; the report validator and process test enforce
completeness and work-count bounds.

The workload reports initial materialized groups separately from groups committed
during movement. Radius-zero horizons still honor sight, reach, and linked-landing
pins: reach three plus one loaded landing allows at most three initial groups and
four groups (36 records) over this first-floor-to-second-floor route. Each command
must conserve initial plus newly committed groups, and every expedition must show
actual detachment. The five default seeds currently materialize two groups in
total; seed 45 already needs both at startup. Those observed counts are not a
universal bound for other seeds.

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
