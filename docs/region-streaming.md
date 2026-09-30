# Region streaming foundations

Milestone 4e is in progress. These parts are implemented:

- a backend structural catalog and preload-horizon planner (below);
- the [region lifecycle](#region-lifecycle-contract) in `tor-simulation`:
  reference points, frozen time, pins, and detaching regions into
  self-contained records kept in a record store;
- [never-built regions](#never-built-regions-and-region-sources): a game can
  start with no region built, and a scenario package builds each region when
  it's first loaded;
- [engine streaming](#engine-streaming): package games start with only the
  regions their characters need, move to the regions their reference points
  ask for after every command, and keep detached regions on disk.

Generation, larger scenarios and client asset-palette delivery are
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

A known region is in one of four states:

- **Unbuilt:** known from its metadata only, because nothing has needed it
  yet. See [never-built regions](#never-built-regions-and-region-sources).
- **Active:** simulated.
- **Frozen:** loaded in memory, with time stopped.
- **Detached:** held as a self-contained region record, with time stopped.
  The game keeps only the record's identity; the record itself is in a
  [record store](#region-records-and-identity): on disk in a save, and in
  memory until it's written.

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

### Never-built regions and region sources

A game can start knowing every region without building any. An unbuilt
region has metadata (name and bounds) and the identities it will hold, and
nothing else. Loading it for the first time builds it:

- `Game::add_unbuilt_region` declares a region with the actors, items and
  doors it will hold. Those identities go into the identity directory, so
  references to them (the objective's item, a character) are checkable, and
  the id allocators move past them.
- A transition that loads an unbuilt region asks its record store to build
  the region's starting record (`RecordStore::build`), then attaches it like
  a detached record, frozen. Attaching checks that the record holds exactly
  the declared identities. `TransitionReport::built` lists the regions built.
- A starting record's actors carry freeze stamps of zero, so a region's time
  starts when it's first active; nothing catches up.
- After building regions, a transition checks the objective, since a
  character may start where it's met, as it would if everything had been
  built at once.
- Run characters and actors awaiting input may be unbuilt or detached; pins
  make an actor awaiting input active before it acts.

For scenario packages, `Package::start` declares every region unbuilt and
configures the run; `Package::build_region` builds one region's record. It
builds the region in a scratch game that holds the region and its
neighbours' geometry, so links and entities are checked exactly as building
the whole package checks them, then takes the record with
`Game::into_region_record`. It reads only that region's definition and its
neighbours' walls, so the result doesn't depend on which regions were built
before. Authored identities are fixed by the package, so building lazily
doesn't renumber anything.

A server test builds every checked-in package region by region, for two
seeds and three build orders, and checks that the result equals building
the whole package at once.

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

World tables sort by region first, so detaching or attaching a region visits
only that region's entries. Detaching one region of a 256-region world with
dense terrain and reattaching it took 0.20 ms at p50 (0.24 ms p95), against
1.71 ms (2.67 ms) when every table was scanned; at 8 regions the costs are
0.17 ms and 0.21 ms. (Release build, this machine, 2,000 samples each:
`cargo run -p tor-world --release --example region-detach-profile`.) Only
sight scenes that list the region are invalidated. That's exact, because a
scene lists every region it entered and every region linked from them.

An **identity directory** maps every detached actor, item and door to its
region. Id allocators stay game-wide, so identities are never reused.
Every directory entry names a detached region, and that check needs no record
contents. Attaching a record checks the record itself, and that its actors,
items and doors are exactly the directory's entries for its region, because
a record may come from storage that restoring the game never read.

The **record store** holds records outside the game:

- Detaching allocates a **record identity** from a game-wide counter and
  gives the record to the store. Records never change after they're made, so
  the identity also names the content. Replay reproduces identities because
  the counter is game state.
- A transition reads records only through the store (`RecordStore`), and puts
  new ones there only once the whole transition has succeeded. A store that
  can't provide a record fails the transition with the game unchanged.
- A rewind restores an earlier game whose counter is behind records the
  abandoned future made, and retained boundaries may still refer to them. The
  rewound game must continue the counter (`Game::continue_record_ids`) so
  identities stay unique. The engine wiring slice does this.
- `MemoryRecords` keeps records in memory, for tests and until records are
  stored on disk.

References come in two kinds:

- **Live references** (bodies, pending attacks, carriers, reach) must stay
  inside active or loaded regions, as the pins require.
- **Knowledge references** may point anywhere the world knows about: AI
  memory and visit counts, navigation memory, the objective's anchor and item,
  visited regions, and run characters. These are checked against the
  directory and the detached regions' bounds, not against loaded state.

### Engine streaming

Package games stream; the diagnostic fixtures (the two-room game and the
performance fixture) don't, so their measurements stay comparable.

- **Radii.** A scenario's `streaming` setting gives the portal hops kept
  active and loaded around each reference point that doesn't set its own.
  Packages default to 1 active and 2 loaded. The setting is saved with the
  scenario, so replay applies the same transitions. Pins still add whatever
  the points see and reach, and keep linked regions loaded.
- **Start.** A new game starts with every region unbuilt, gives each run
  character an observing point (or, in a package without combat, the
  selected character), gives one to every actor the package leaves to a
  client (`controller = "external"`), and runs one transition. Only the regions that needs
  are built, so creating a save and replaying it from the start scale with
  the horizon, not the world.
- **Every command** ends with a transition on the candidate game, before its
  rewind boundary and journal record, so replay, checkpoints and rewind all
  see it. Records the transition makes stay with the candidate until the
  command publishes, so a rejected command leaves nothing behind.
- **Revisions.** When a transition changes anything, each loaded actor's
  view is compared before and after, and only actors whose view changed get
  a new revision. So an update never discloses a change nobody could see,
  such as a region detaching out of sight. Loaded actors' revisions are
  copied with every command; the rest are parked in a shared map, and move
  across (with a new revision) when their region attaches or detaches.
- **Clients.** A client attached to an actor whose region leaves the loaded
  world (a spectator watching an AI actor, say) gets an unsolicited
  `not_attached` error and is disconnected, as after a rewind removes its
  actor; the command that caused it still succeeds. Actors that clients
  control keep reference points, so this doesn't happen to them.
- **Rewind** continues the record counter (`Game::continue_record_ids`).
- **Wizard operations** load and activate the regions they act on first, so
  a wizard can reach a place nobody has needed yet. A region a wizard adds
  isn't in the package's catalog, so a reference point there keeps just
  that region; pins still follow its links.
- **Validation** of an edited package game checks anchors in loaded regions
  only; regions that aren't loaded can't have been edited.

### Persistence

Lifecycle state (points, frozen regions, stamps, record identities, the
record counter and the directory) is saved as an optional `lifecycle` field
of the checkpoint, and region metadata that isn't loaded as an optional
`absent` field of the world. Both are omitted while empty. Identical
lifecycle states across rewind boundaries are encoded once, the way worlds
are.

- **Record rows.** Each detached region's record is its own row in the
  `regions` table, keyed by its record identity. A row uses the journal's
  frame layout with magic `TORR`, kind 3 and the identity in the sequence
  field, and is capped at 16 MiB. Rows are written once: identities come
  from game state, so a retried checkpoint write produces the same rows
  byte for byte, and a different row under an existing identity fails the
  save closed.
- **Loaded regions** stay inside the checkpoint; the loaded set is bounded by
  the reference points' horizons.
- **Writing.** A checkpoint capture carries the records it refers to that
  aren't on disk yet. Its transaction writes them before the checkpoint,
  then deletes every row the new checkpoint doesn't refer to. That's safe
  because identities are never reused: anything the engine refers to is
  either in the latest checkpoint (with its boundaries) or newer than it.
  Crash rollback is unchanged, and the process-death tests cover the new
  `after_regions` and `after_gc` stages.
- **Memory.** Records stay in memory until a committed checkpoint has
  written them; then only a few durable ones stay cached, and records made
  before that capture that it doesn't refer to are dropped.
- **Loading** reads the game-wide state only, and the integrity check covers
  the journal, history and checkpoint tables. Records are read when their
  regions attach, on a separate connection. A row read while the save
  worker is writing pages waits for that commit (rollback-journal mode);
  that only happens when a record has left memory.
- **Checks.** An attached record gets the checks restoring a checkpoint gives
  loaded state: its checksum, its own consistency and its identities against
  the directory, then each actor and item (identities, combat, orientation,
  readiness, motion, navigation), bodies against every loaded body, and the
  game-wide combat and physics checks. A record that can't be read or fails
  them fails that command with a storage error and changes nothing.
- **Format.** Save format 13 adds the `regions` table and the scenario's
  streaming setting. Saves still embed their package; pinning a package by
  hash waits for per-region package files.

Per-region in-memory tables, with loaded regions also stored as rows, are
deferred until measurements show the global tables or re-encoding the loaded
horizon per boundary matter.

### Performance

Each command's transition costs work bounded by the loaded actors and
regions, never the whole world:

- the pins computed for a state are reused while settling grows the sets,
  and a transition that changes nothing isn't applied, copied or checked
  again;
- which regions an observer sees comes from the scene cache without copying
  the scene;
- an actor's reach (every region within three steps) comes from its
  region's **exit field**: for each cell near an exit, the steps to the
  nearest cell whose next step leaves the region. It's built once per region
  with the search's own steps, so it's exact however links, rims and walls
  lie, and it's cached under the scene cache's region version tokens. A body
  at least three steps from every exit reaches only its region, which costs
  a lookup per body cell even while it moves. Near exits, the search runs,
  and its result is cached by position;
- lifecycle state, the identity directory (indexed by region, so attaching
  touches only that region's identities) and unloaded actors' revisions are
  shared copy-on-write, so every command's copy of the game stays small;
- building a region reads only it and its neighbours: the package's
  whole-package facts are indexed once per game.

`region_streaming::transition_work_does_not_grow_with_the_world` enforces
this in CI with operation counts: the same walk through 16 and 256 halls
does identical horizon planning, pin, reach, record and build work.

The `latency_bench` streaming cases (`stream-r16-memory`,
`stream-r16-durable`, `stream-r256-memory`, `stream-r256-durable`; workload
`streaming-v1`) walk 70 steps east and back through the lengthened streaming
corridor at the default radii, detaching and rebuilding halls every cycle,
and are validated by `scripts/performance_report.py`. Release build, this
machine, 5 cycles (700 commands) each:

| Case | Command p50 | Command p95 | Command max | Transition p50 | Transition p95 |
| --- | --- | --- | --- | --- | --- |
| stream-r16-memory | 0.209 ms | 0.388 ms | 0.562 ms | 0.013 ms | 0.047 ms |
| stream-r256-memory | 0.209 ms | 0.416 ms | 0.695 ms | 0.013 ms | 0.049 ms |
| stream-r16-durable | 0.225 ms | 0.389 ms | 0.555 ms | 0.013 ms | 0.046 ms |
| stream-r256-durable | 0.229 ms | 0.453 ms | 1.362 ms | 0.013 ms | 0.058 ms |

The world's size doesn't change the command or transition time; a 256-hall
game keeps 250 halls unbuilt. These cases are new, so there's no earlier
base to compare them against.

### Later slices

1. **Background preloading:** reading rows and building regions ahead of
   need on another thread. Correctness never depends on it; today a record
   is read when its region attaches.
2. **Large scenarios and generation:** per-region package files with a
   manifest, lifting the 256-region limit; pinning the package by manifest
   hash instead of embedding it, and copying each region's source into the
   save when it's first built (another save-format bump); and a procedural
   region source. A generated region's content mustn't depend on the order
   regions were built in, so each region gets its own random seed; generated
   identities come from the game-wide allocators, which replay reproduces.
3. **Asset palettes.**

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
- Unit tests in `streaming.rs` check that AI memory can't expire while
  frozen; that attaching a record that disagrees with the directory, or a
  record the store can't provide, is rejected with the game unchanged; that
  records reach the store only when a transition succeeds; that record
  identities stay unique across a rewind; and that unbuilt regions build
  from their source, reject a source that loses a declared identity, and
  can't declare an identity already in use.
- `crates/world/tests/sight_cache.rs` detaches and reattaches random regions
  of world clones between random views and edits, and checks every cached
  scene against an uncached one. Another test checks which scenes a detach or
  attach invalidates.
- `crates/server/tests/region_streaming.rs` plays the checked-in
  `streaming-corridor` package (seven halls) with radii of zero. It checks the regions built at the start,
  detaches regions behind the character, restarts from a checkpoint without
  reading a row, reattaches regions from their rows, and replays the whole
  history from the start. A wizard rewind past a detach, followed by a
  different future, checks that new records never reuse an identity, and a
  wizard teleport reaches a region that was never built. Using
  `streaming-controlled`, an actor a client controls keeps its region in
  play, and a transition leaves an unseeing observer's revision alone.
- `crates/server/tests/streaming_websocket.rs` detaches a spectator whose
  actor leaves the loaded world, over real connections, while the player's
  commands keep succeeding.
- `scripts/test_streaming_process.py` plays across detached halls with real
  headless and native ASCII clients: spectators, a crash and restart, a
  reconnect, and a wizard rewind past a detach through the headless client.
- Storage tests kill a real process at every stage of a checkpoint that
  writes region rows, and fail closed, atomically, on a damaged or missing
  row. A save attached to a game that started in memory reads evicted
  records back.
- Storage tests write, retry, reject and collect rows in the checkpoint
  transaction, with failures injected at every stage.
- A server unit test shrinks every checked-in scenario package to what its
  characters' points require, round-trips the checkpoint, and restores
  everything to an identical game. This covers real joins, rotations, physical
  portals, doors and multi-cell bodies.
