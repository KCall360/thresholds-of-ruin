# Region streaming foundations

Milestone 4e is in progress. Two slices are implemented:

- a backend structural catalog and preload-horizon planner (below), and
- the [region lifecycle](#region-lifecycle-contract) in `tor-simulation`:
  reference points, frozen time, pins, and detaching regions into
  self-contained records.

The engine doesn't use either yet: runtime games still construct and activate
every authored region. Disk storage of region records, engine wiring,
on-demand generation and client asset-palette delivery are
[later slices](#later-slices).

## Inspect a horizon

```powershell
cargo run -p tor-server --bin tor-scenario -- horizon scenarios/first-dungeon 1 2
```

The read-only authoring command requires a current validation certificate. It
prints JSON containing sorted `required`, `activate`, and `deactivate` region IDs,
plus `expanded_regions` and `examined_links` operation counts. It uses an empty
initial active set, so `activate` equals `required` and `deactivate` is empty.
Errors use the utility's existing JSON error envelope and nonzero exit status.
Package parsing and integrity checks still read the complete bounded authored
package; the command does not claim bounded startup for a streamed world.

## Structural contract

`RegionCatalog::from_package` indexes bounds, named anchors, zone/theme metadata,
and directed outgoing region links. It does not create a world, inspect actors
or items, consume RNG, or mutate the package. Anchor resolution works without
loading the destination region. Unknown zones, duplicate/zero region IDs, invalid
bounds/anchors, and missing portal destinations fail catalog construction.
Full portal geometry and entity validation remain the scenario validator's job.

`RegionCatalog::plan` takes a nonempty set of root regions, a portal-hop radius,
and the previous active set. Radius zero includes only the roots; higher radii
include the union of directed neighborhoods. Reverse links must be authored;
closed doors and runtime occupants do not change structural preloading. Multiple
links to one region are deduplicated. Cycles terminate, outputs are stable across
source ordering, and unknown root/active IDs reject the entire query.

The result describes transition candidates, not mutations. Future streaming must
include body/effect dependencies in its roots or otherwise pin them before
applying transitions at committed simulation boundaries. A region outside this
graph neighborhood is not automatically safe to freeze. No fixed runtime radius
has been selected by this authoring interface.

Catalog construction scales with structural metadata. Subsequent queries expand
only reached regions below the radius, with ordered-map lookup costs. Computing
transition sets also costs work proportional to the supplied active/required
sets. The catalog is backend-only; sending it to clients would disclose unseen
topology and is forbidden. Theme metadata does not constitute an asset palette
or reveal item appearance mappings.

## Verification and performance

The `region_horizon` Rust integration suite exercises all checked-in scenario
catalogs, directed cycles, multiple roots, anchor resolution, zone replacement
and inheritance, invalid inputs, and the actual authoring executable. A stable
operation-count regression adds unrelated regions up to 8,192 and checks that a
one-hop query still expands one region and examines one link.

```powershell
cargo test -p tor-server --test region_horizon --locked
cargo test -p tor-server --test region_horizon --release --locked
cargo run -p tor-server --example horizon-profile --release --locked
```

The `structural-horizon-v1` workload reports 10,000 queries after 100 warmups for
5, 256, and 8,192 structural regions. Each case preserves the same five-region
dungeon neighborhood and adds disconnected metadata. It reports catalog-build
time separately from query p50/p95/maximum and operation counts; file parsing,
JSON reporting, and correctness assertions are outside query timing. This is a
synthetic catalog scalability check, not support for runtime packages exceeding
the current 256-region limit. There is no earlier query implementation for a
before/after comparison; the small/large cases establish the initial baseline.

Initial Windows release measurements (2026-09-27, 10,000 samples per case):

| Structural regions | Catalog build | Query p50 | Query p95 | Query maximum |
| --- | --- | --- | --- | --- |
| 5 | 0.036 ms | 0.6 us | 0.6 us | 60.8 us |
| 256 | 0.291 ms | 0.6 us | 0.8 us | 41.0 us |
| 8,192 | 8.221 ms | 0.6 us | 0.7 us | 28.6 us |

All cases expand two regions and examine four directed links. These sub-microsecond
query measurements include timer overhead and host scheduling noise; they are
diagnostic, not timing assertions. The operation-count test is the stable scaling
gate.

The planner changes no formats or scenario certificates. Ordinary simulation, scheduling, persistence, and client behavior do
not use the planner yet.

## Region lifecycle contract

This is the contract that disk streaming is built on. The code is in
`crates/simulation/src/streaming.rs` and `crates/world/src/region_slice.rs`.
The engine doesn't call it yet, so ordinary games are unaffected: nothing
freezes, and saves are byte-identical.

### Region states

A known region is in one of three states:

- **Active:** simulated.
- **Frozen:** loaded in memory, with time stopped.
- **Detached:** held as a self-contained region record, with time stopped.
  Today records stay in memory, inside the game state; the next slice stores
  them on disk.

The existing planner computes a *load* horizon. The active set is smaller:
what the reference points and pins below require. Activation only ever uses
loaded regions, so a slow disk read can delay the next step but can never
change its result.

### Reference points

What stays active and loaded comes from **reference points** in game state.
Players aren't special.

- Each point follows a target: an actor, an item (which follows its carrier),
  or a fixed location. It has optional active and load radii in portal hops,
  where `None` means the game's default, and an `observes` flag.
- `Game::add_default_reference_points` gives each run character an observing
  point. That's a default, not a rule; later features such as scrying or
  autonomous machines add their own points.
- Points are created and removed only by game rules between actions, so replay
  reproduces them. They're saved with the game.
- `Game::region_roots` resolves every point to its region and radii, for the
  planner. The simulation never reads the catalog.
- Spectators are connections, not points, and never keep regions alive.

### Transitions

`Game::apply_region_transition` takes the region sets that should be active
and loaded. `active` must be a subset of `loaded`; loaded regions outside
`active` freeze, and known regions outside `loaded` detach.

- A transition runs only between actions and depends on game state alone.
- It attaches, freezes, thaws and detaches in region order, then checks every
  pin. On any error the game is unchanged.
- If the actor due next froze, time advances to the next decision, exactly as
  after an action.
- `Game::settle_region_transition` grows proposed sets until the pins that
  loaded state reveals hold. `Game::transition_regions` settles, applies, and
  grows again when attaching a region reveals further pins.

### Pins

A transition is rejected unless all of these hold:

- **Reference points:** every point's region is active. Everything an
  observing point sees is active, and every region its view reads is loaded,
  so a player, spectator or scrying view never sees frozen time.
- **Controllers:** every actor waiting for controller input is active.
- **Live references stay active:** for every active actor, the regions holding
  its body, its pending attack target, and everything within three axis steps
  of its body (movement, melee and physics reach) are active.
- **Live references stay loaded:** for every frozen loaded actor, the regions
  holding its body and its attack target are loaded. A transition that would
  split regions tied this way is rejected, and `transition_regions` keeps
  them loaded together.
- **Border:** every region linked from an active region is loaded, and every
  region an active actor's view reads is loaded. Active code therefore never
  looks up a detached region.

Pinning an active actor's reach can grow the active set in a chain when
several actors stand near region edges. That was chosen over freezing
individual entities, because whole regions are the unit that gets stored.
Transitions report what they changed; the engine-wiring slice should measure
how far sets grow in real dungeons.

An active actor that isn't an observing point (an AI creature, say) may see a
frozen actor standing still at the edge of its view. Streaming must be
deterministic; it isn't required to match an unstreamed game exactly.

### Frozen time

- When a region freezes, each actor in it is stamped with the current tick.
  Frozen actors aren't scheduled, don't resolve attacks, don't make AI
  decisions, and aren't moved by physics. Items in frozen regions don't move
  either.
- On thaw, every absolute tick an actor holds moves forward by the time it was
  frozen: its ready time, the start of any attack preparation, and when its AI
  last saw its target. Nothing catches up: remaining recovery, wind-up and AI
  memory are exactly what they were. Motion and displacement are kept as they
  were, so a falling body resumes mid-fall.
- An actor that falls into a frozen region between transitions freezes at the
  tick it enters, with its own stamp. Pins guarantee that happens only where
  no observer can see it.
- Checkpoint validation measures a frozen actor against its stamp rather than
  the current tick.

### Region records and identity

A detached region's record owns everything located in it:

- its share of the world: region metadata, terrain, doors, outgoing links
  with their rotations, physical portals, gravity, chamber extent and place
  hints; links *into* it stay with their source regions;
- the actors anchored there, with their stamps, AI state and navigation;
- its ground items, and the items its actors carry;
- the physics impacts and displacement of those entities.

The world keeps a detached region's metadata, so references into it stay
checkable. Every piece of `World` state is destructured exhaustively when a
region detaches, so new world state can't be added without deciding whether
a record owns it.

An **identity directory** maps every detached actor, item and door to its
region. Id allocators stay game-wide, so identities are never reused.

References come in two kinds:

- **Live references** (bodies, pending attacks, carriers, reach) must stay
  inside active or loaded regions, as the pins require.
- **Knowledge references** may point anywhere the world knows about: AI
  memory and visit counts, navigation memory, the objective's anchor and item,
  visited regions, and run characters. These are checked against the
  directory and the detached regions' bounds, not against loaded state.

### Persistence in this slice

Lifecycle state (points, frozen regions, stamps, records and the directory) is
saved as an optional `lifecycle` field of the checkpoint, and detached region
metadata as an optional `absent` field of the world. Both are omitted while
empty, so games that never stream save exactly as before and no format version
changed. Records are held as shared values, so rewind boundaries share them in
memory; in a save each boundary encodes them again, which the storage slice
replaces.

### Save layout (designed; built in the next slice)

- **Region rows.** Each region record is its own SQLite row keyed by
  `(RegionId, version)`, holding the record's encoding and a checksum. Rows are
  written once and never updated.
- **Versions.** A region gets a new version from a save-wide counter when it's
  first encoded after a change. Unchanged regions keep their version across
  rewind boundaries and checkpoints, so each boundary costs only the regions
  that changed.
- **Checkpoints and boundaries** list `region → version`, plus the game-wide
  state: tick, seed, id allocators, reference points, the identity directory,
  the combat globals (outcome, objective, hostility, input boundaries,
  characters, events) and per-connection revisions.
- **Garbage collection.** A row is deleted in the same transaction that
  removes the last checkpoint or boundary referring to it. Rows are written
  before the checkpoint naming them commits, so crash rollback is unchanged.
- **Loading.** The server reads the game-wide state, then only the rows for the
  saved load horizon. Other rows are read on demand.
- **Format.** One save-format bump covers this layout and per-region
  in-memory storage.

The record encoding in this slice is the encoding those rows will hold.

### Later slices

1. **Per-region storage and the record store,** with one save-format bump:
   the world and entities stored as one shared value per region, so rewind
   shares memory per region and the scheduler scans only active actors; the
   save layout above; horizon-only loading on startup.
2. **Engine wiring:** transitions after every committed command; default
   reference points for the actors the engine controls (packages without
   combat have no run characters, so points can't come only from
   `combat.characters`); choosing load and active radii; background
   preloading; reconnect and gap snapshots.
3. **Deterministic generation** of regions never activated, pinned to
   scenario, seed and version references.
4. **Asset palettes.**

### Lifecycle verification

- `crates/simulation/tests/region_lifecycle.rs` covers:
  - default and added reference points, and points following carriers and
    detached actors;
  - every pin kind, with rejected transitions leaving the game unchanged;
  - frozen actors not acting, then resuming with their remaining recovery and
    wind-up;
  - a body falling into a frozen region, freezing mid-fall and resuming;
  - detaching, playing on, and reattaching, which gives exactly the same game
    as only freezing, including through a save round trip while detached;
  - checkpoint round trips while frozen and detached, and ordinary saves
    without lifecycle fields.
- A unit test in `streaming.rs` checks that AI memory can't expire while
  frozen.
- A server unit test shrinks every checked-in scenario package to what its
  characters' points require, round-trips the checkpoint, and restores
  everything to an identical game. This covers real joins, rotations, physical
  portals, doors and multi-cell bodies.
