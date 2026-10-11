# Authored scenario packages

Milestone 4a adds ordinary TOML input packages and an explicit offline validator.
Packages work in normal games and with `--wizard`; authoring does not execute
wizard commands. Packages target the current ruleset, and older prerelease saves
are rejected without migration.

## Author and run

The default game is `scenarios/first-dungeon`; `scenarios/two-room` is a minimal
example, `scenarios/mob-arena` is a three-HD two-team creature arena, and
`scenarios/tests/` holds the test fixtures. A package directory contains
`scenario.toml`, one file per region in `regions/` (named `<region id>.toml`),
and two files the validator generates: `index.json` and `validation.json`.
Paths must stay inside that directory, including symlink resolution. The
manifest is bounded to 8 MiB, each region file to 1 MiB, and a package to
65,536 regions.

```powershell
cargo run -p tor-server --bin tor-scenario -- validate scenarios/two-room
cargo run -p tor-server --bin tor-server -- --scenario scenarios/two-room --seed 42 --save saves/new.db
```

Set the usual server authentication token first. Omitting `--scenario` selects
`scenarios/first-dungeon`. `--character <numeric-id>` selects a starting character;
omitting it uses `default_character`. Connect the client using `--actor <id>`.
`--regions` retains the independently versioned diagnostic workload and conflicts
with `--scenario`. Existing saves own their inputs; scenario and seed arguments
don't replace them. See [saves and the package](#saves-and-the-package) for when
resuming needs the package directory.

Run validation after **every source edit**, including comments. Validation
writes the region index and binds SHA-256 hashes of the manifest and the index
(which holds each region file's hash), the normalized model, exact ruleset and
validator identity, and recorded coverage. Versions are
author-controlled `major.minor`; changing a hash does not require a version bump.
Manifest and region source hashes normalize CRLF to LF automatically, so
validation on Windows survives an LF checkout and vice versa. Other source edits,
including comments, still require validation. Generated TOML uses LF; source byte
limits apply before normalization, and validation does not rewrite authored text.
Validator failures produce a nonzero exit status and JSON diagnostics on stderr.
Reference errors identify their authored declaration and, when its original
source still matches, the exact line and column of the failing value. This
includes numeric carrier/item references and individual generator/identity array
entries. Comments, repeated values and escaped strings do not redirect a
location. Missing, changed or ambiguous source retains the semantic error and
declaration context without guessed coordinates. Validation does not rewrite a
rejected package.
Stale/missing validation is refused by default: an edited manifest when the
package loads, and an edited region file when its region is first built, since
a validated game reads a region file only then. `--allow-unvalidated` reads and
indexes every region file afresh instead. It prints a
warning and permits structurally valid development input. It does not enable
wizard access or permit unavailable mechanics. Invalid geometry is always rejected.

## Format

The read-only `tor-scenario horizon <directory> <region-id> <portal-hops>` command
inspects a structural preload neighborhood without constructing gameplay state.
See [region streaming foundations](region-streaming.md) for its output and limits.

The manifest declares `format = 2`, `id`, `version`, `ruleset`,
`default_character`, and `characters`. Older formats are refused. Optional `themes`, `zones`, `archetypes`,
and `objective` describe the world. Zone theme pools replace world defaults;
omitting a zone pool inherits the world pool. An empty pool deliberately replaces
it with no themes. These are content identifiers, not client asset disclosures.

Each region file holds one region's fields at its top level. Regions have stable positive numeric `id`,
`name`, `size = [width, depth, height]`, and optional `chamber`, `zone`,
`anchors`, `walls`, `openings`, `places`, `portals`, `doors`, `items`, and `actors`.
All positions are region-local integer `[x, y, z]` triples. Chambers add a finite
stone shell. Walls/openings explicitly alter cells. Named anchors are single-cell
authoring references such as `1/start`; they are separate from disclosed place
hints. A place hint is a position, or a position with the name the character
learns on seeing it: `places = [[3,2,0], { at = [5,1,0], name = "Threshold" }]`.
Names follow the [place name](place-knowledge.md) rules.

Outgoing portals specify `at`, cardinal/vertical `direction`, destination anchor
`to`, and optional `turns`, `width`, `height` (defaults 0, 1, 1). Reverse portals
are explicit. The validator constructs every region before checking connections.

### Paired stairs

The manifest can declare named stair pairs once:

```toml
[stair_pairs.main]
upper = "1/down"
lower = "2/up"
```

Pair names are stable connection identities, independent of coordinates. Each
endpoint names either an authored anchor or a generated stair anchor. A pair
creates a down link at `upper` and an up link at `lower`, both with identity
rotation. Regions may be the same, and one cell may have both an up and a down
link. Duplicate exits in the same direction are rejected, including conflicts
with existing authored links. Backtracking is allowed; scenario gates are not
implemented by this declaration.

Using stairs arrives exactly at the matching anchor, preserves facing and the
existing movement velocity behavior, and uses ordinary movement recovery time.
The actor's entire body must fit. A blocked arrival consumes no time and does
not move the actor, displace occupants, or choose a nearby cell. Stairs remain
transport links, separate from falling and physical see-through portals.

Structural pair destinations are available to horizon planning before generation
chooses their positions. Each endpoint is generated from its own region's pinned
inputs, independently of destination coordinates and load order. Resolved links
and terrain persist in region records; the saved manifest retains pair identities
and the pinned region sources support deterministic replay. Revisits do not
reroll positions. `scenarios/tests/paired-stairs` demonstrates generated endpoints,
same-region traversal, and a cell with stairs in both directions.

Generation groups can retain a named destination until its floor publishes its
anchors. The current group does not read the destination source or generate its
room early. See [generation recipes and groups](generation-recipes.md) for the
preparation, publication, and independent streaming contracts.

Actors, items, and doors have positive numeric IDs, unique within their entity
kind. Array order does not assign identities. Doors specify `at`, `open`, and `height` (default 1). Validation rejects a door
that doesn't fit, or that leaves its walled doorway open above it; see
[doors](doors.md).
Items specify `at`, `name` or an `archetype`, and optional `carried_by`. A carried
item is authored in the region file where its carrier starts, so building a
region reads only that region's file and its neighbours'. Archetype
names and actor `turn_ticks` are overridden by instance fields. `seed_names` is
an explicit deterministic name pool selected by `seed % length`, used to preserve
the original fixture's three material variants. It is not world generation.

Characters specify `id`, starting `anchor`, and optional `turn_ticks` (100).
Unselected characters default to omission, including their inventory. Region
actors specify `id`, `at`, optional archetype/duration, and `controller`.
`external` supports the existing actor-control/test-driver interface. Optional
AI assignments use `controller = "ai"`, or character `unselected = "ai"`, plus
an `ai` identifier. AI identifiers resolve to manifest `ai_profiles`; see [dungeon gameplay](dungeon.md).

Region gravity vectors/sparse overrides, body declarations, initial velocity,
and full portal rotations are implemented; see [physics](physics.md).
Objective declarations (`anchor`, optional authored item ID, `disclosed`,
`continue_play`) are implemented; see [dungeon gameplay](dungeon.md). Regions
can be generated; see [generated regions](#generated-regions). Dependency
registries and equipment aren't supported yet. The current
package is self-contained and depends on one exact built-in ruleset; external
content/generator dependency fields are rejected rather than silently ignored.

## Assets

Assets are optional. A manifest may give:

- `[assets]`: for each theme, the asset identifiers a client near a region
  with that theme may need (`caves = ["terrain.floor.cave", "creature.rat"]`).
  Identifiers are dotted lowercase names.
- `terrain = { floor, wall, door }` for the world, and per zone
  (`zones.<name>.terrain`), naming the assets of a region's cells and doors.
- `asset` on an archetype (its actors and items), on a character, and on an
  appearance pool. A concealed item shows its pool's asset, never its
  archetype's, so the asset can't disclose its identity.

The validator requires everything a region shows (its terrain, and its
actors' and items' assets, including a generated region's pools) to be among
the assets of that region's themes, so a palette always forecasts them. See
[asset palettes](protocol.md#asset-palettes).

## Generated regions

A region file with a `[generate]` table authors only the region's structure:
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

- `rooms` (the only generator, version 2) keeps a clearing two cells wide
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
  [region streaming](region-streaming.md#never-built-regions-and-region-sources))
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

## Validation, persistence, and tests

Validation exhaustively constructs the bounded authored world (1–65,536 regions,
up to 32×32×8 interior cells each), checking IDs, geometry, references, anchors,
placements, inventory, and all character selections. It repeats construction at
seeds 0, 1, and 42 to check determinism. The certificate records this coverage;
it is neither a gameplay/winnability proof nor a cryptographic trust signature.

### Saves and the package

A save pins its package by the certificate's model hash. It keeps the manifest
and the region index, and copies each region file into its `region_sources`
table the first time a region is built from it (with the neighbours whose walls
that build reads), in the same transaction as the journal record of the command
that built it. A build reports exactly the files it read, and those are
the ones copied; declaring a generated region reads its file too, so that's
copied as well. Replay rebuilds regions from these copies, read from the save
only when first needed, and each copy is checked against the index's hash when
it's used; a damaged copy fails closed. A command whose build copies more than
the save queue's limit is still accepted when nothing else is queued.
So a save's size grows with the regions played, not the package.

Regions not built yet are read from the package directory, checked against the
index. A save remembers where its package was. If a region the game has
declared could still need a file the save doesn't hold (its own, or a
neighbour's), and that directory is gone or holds a different package, the
server refuses to resume and asks for `--scenario` naming the same package,
rather than failing when play reaches it. Otherwise it resumes without the
package, even if the package has regions nothing leads to.

### Runtime changes

Saved inputs are immutable snapshots with exact identities and hashes. Runtime
wizard mutations remain journaled, and each wizard entry records resulting
scenario validation status. World mutation APIs enforce structural consistency;
blocking an authored anchor or portal endpoint breaks scenario validation. Ordinary play, enabling
wizard access, and structurally valid edits do not. Rewind restores the target
structure and validation result, but never removes permanent wizard lineage.
Runtime edits never rewrite or automatically revalidate the source package.

Converted initial setups live under `scenarios/tests`. Test actions and assertions
remain in Python/Rust, with explicit stable fixture references. Memory-building
journeys, hidden changes, and wizard authorization/rewind operations stay in the
test harness. Rewind now returns to the package start, retaining its geometry;
tests assert restoration of runtime mutations and clearing of later knowledge.

## Performance

Measured 2026-09-27, when packages were introduced (before items), in release
builds on the maintainer's Windows machine. The
[archived samples and manifest](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs/measurements/scenario-packages-2026-09-27) record the
distributions and source and binary hashes. Existing mixed workloads use 10 cycles, with 290
small/630 large command attempts. Command-call p50/p95/max milliseconds change
from 0.233/0.591/0.798 to 0.226/0.434/0.819 (1 region), and from
0.402/1.193/2.038 to 0.383/1.097/1.491 (256 regions). These runs show no material
command regression. The baseline is the pre-existing release benchmark binary,
identified by SHA-256, not a freshly rebuilt separate checkout.

Package workload v1 uses 20 samples per size; values below are p50/p95/max ms.

| Operation | 2 regions | 256 regions |
| --- | --- | --- |
| Source integrity/parse | 0.929 / 3.390 / 6.824 | 5.578 / 7.162 / 7.627 |
| Game construction | 0.111 / 0.324 / 0.375 | 0.802 / 1.313 / 1.523 |
| Ordinary wait | 0.008 / 0.024 / 0.087 | 0.009 / 0.013 / 0.017 |
| New durable game | 70.572 / 85.347 / 105.655 | 95.861 / 164.606 / 181.568 |
| Explicit save barrier | 36.946 / 50.263 / 54.569 | 63.547 / 84.673 / 86.583 |
| Checkpoint restart | 3.371 / 10.546 / 11.652 | 13.588 / 31.225 / 31.735 |

Every restart matches saved actor state and validation status. Source sizes are
1,215/58,515 bytes; saved databases are 24/136 KiB. Offline validation takes
10.152/15.912 ms, with only one timed invocation per size. The original in-code
2-region constructor measures 0.068/0.220/0.248 ms in the same feature run.
The larger package adds disconnected chambers and measures source/construction
scaling, not exploration or many-file I/O. The existing mixed trace separately
exercises connected worlds. Measurements are diagnostic and do not close 3p,
explain its outstanding timing tails, or relax any timing gate.

## Item quantities and knowledge

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.

## Creature arena

`scenarios/mob-arena` provides a repeatable encounter using ordinary creature
builds, AI, abilities and combat. All four participants have three hit dice: the
selected blue adept combines Warrior and Mage training, its blue sentinel ally
is a Warrior, and the red team has a Warrior and a Mage. The adept has Power
Strike, Magic Bolt and Fear; `stats` shows its owned training, talents and pools.
Use the normal Text ability commands or the native ASCII ability menu to play.

```sh
cargo run -p tor-server --bin tor-server -- --scenario scenarios/mob-arena --seed 42 --save saves/mob-arena-42.db
```

A species declares its natural `melee` attack with a skill, check bonus, base
wind-up and recovery, and a damage bundle. For example:

```toml
melee = { skill = "light_weaponry", bonus = 2, wind_up = 90, recovery = 70, damage = { primary = { category = "energy", descriptor = "fire", sides = 6 }, components = [{ category = "energy", descriptor = "fire", amount = { type = "rolled", count = 2, sides = 6, bonus = -1 } }, { category = "keen", amount = { type = "fixed", value = 3 } }] } }
```

Only Heavy/Light Weaponry are valid melee skills. The primary key identifies a
component by category, optional descriptor and die size; omit `sides` for a fixed
primary. Rolled amounts declare `count`, `sides` and signed `bonus`; fixed amounts
declare `value`. The compiler canonicalizes and bounds the bundle. Speed scales
physical phase durations. Permanent melee modifiers and Power Strike combine
before zero clamping; extra dice apply only to a rolled primary. Source inspection
shows the base definition, including every component and its primary designation.

Weapon archetypes use the same attack bundle under `equipment.attack`, with
`equipment.slot = "weapon"`. For example, a fixed Keen weapon is:

```toml
[archetypes.blade]
class = "weapon"
name = "blade"
equipment = { slot = "weapon", attack = { skill = "heavy_weaponry", bonus = 4, wind_up = 60, recovery = 40, damage = { primary = { category = "keen" }, components = [{ category = "keen", amount = { type = "fixed", value = 6 } }] } } }
```

Declare archetypes as tables or as one inline table; TOML cannot extend an inline
table with a later table declaration. An equipped weapon supplies its own skill,
bonus, base phases and damage bundle. The creature supplies its current attributes,
training and permanent melee modifiers. Speed scales the selected attack's phases
once. Identified inventory items disclose their full base attack definition;
unidentified items retain the existing knowledge boundary. Saves retain the source
and reconstruct the same definition on restart.

Mob equipment choices compare their own check score and Speed-adjusted phases,
plus a damage estimate grouped by category and descriptor. Rolled damage uses a
clipped raw mean as a ranking heuristic. It does not predict each roll or include
an enemy's hidden protection. Replacing fire damage with cold is a tradeoff,
so it does not qualify as a straight upgrade under this conservative policy.

The manifest's `arena` setting names participant IDs, `control = "manual"` or
`"all_ai"`, and tick/action limits. Manual control waits for the selected actor's
input; surviving arena AI continues after its death. All-AI control also uses
the selected character's declared AI profile. This package declares that profile
already, so changing `control` to `"all_ai"` and revalidating enables unattended
play. Team elimination can stop an encounter before its bounds. Limits cannot
exceed 100000 ticks or 10000 committed actions; free admissions do not count.

For controlled experiments, set `start_paused = true` in the manifest's `arena`
section and revalidate. With server wizard mode enabled, a wizard can issue
`arena pause`, `arena resume`, or `arena step [count]`. The default step count is
one; counts must be between 1 and 10000 and fit the encounter's remaining action
budget. Players and spectators cannot issue these controls.

A step permits committed actions through the ordinary simulation queue. It counts
an action's start or resume, rather than elapsed ticks or effect resolutions.
After the last permitted action, the arena freezes before advancing to another
decision. A manual participant can still require input before a step completes.
Pause preserves queued intentions and paid preparations, including their reserved
finish costs; snapshots do not advance time. Restart preserves the arena pause
state and active AI preparations without charging another start cost. Resume
continues ordinary scheduling until the encounter stops or is paused again.
Control commands retain private wizard receipts while clients receive ordinary
readiness updates.

`wizard creature remove-hd <actor>` removes that actor's latest hit die and the
training, attribute choice and talent owned by it. Derived Health retains injury;
losing an ability grant interrupts its preparation, releases unpaid reservations
and retains charges already paid. Removing the final hit die causes persistent
death through ordinary cleanup. The command pauses an active arena and rejects
edits to a stopped encounter so its recorded result stays consistent. Retries do
not remove another hit die. Restart preserves the edited build; wizard rewind
restores the build at the chosen retained boundary. Actor IDs here belong to the
trusted wizard interface; player ability commands continue using opaque targets.

Wizard advancement uses `creature add-hd <actor> <racial|warrior|mage>`,
`creature train <actor> <hit-die> <skill>`, `creature attribute <actor> <hit-die>
<attribute>`, and `creature talent <actor> <hit-die> <talent>`, each prefixed by
`wizard`. Hit-die owners are numbered from one, in advancement order. The commands
use the same training budgets, fourth-HD attribute opportunities, caps and talent
eligibility as authored choices. Append preserves retained seeds and choices and
uses the next ordinal of the original actor health stream, without consuming
combat randomness. Removing/re-adding an ordinal gives the same health seed and
empty owned choice slots. Retained choices can become dormant when requirements
are lost and reactivate when those requirements return. Invalid choices reject
atomically. These edits preserve injury and persistent death, do not refill growing
resource pools, and share latest-HD removal's arena, retry, restart and rewind rules.

`wizard creature template <actor> <template> <on|off>` applies or removes a
validated named template from the package catalog. Conflicts and unknown names
reject atomically. Removal affects only that template's grants, preserves owned
advancement and injury, and releases preparations whose grants or funding were
lost. Increasing resource capacity again does not refill it. Mage HD independently
grant magical capability, so removing an additional magical template does not
remove the class grant. These edits share the arena pause, stopped-result,
retry/restart and rewind rules of latest-HD removal.

Creature definitions live in `creatures.species` and `creatures.templates`.
Character and actor `creature` builds name a species, optional templates, initial
attributes, Mana binding, faction and ordered `hit_dice` choices. Each entry owns
its source, training, talent and optional attribute increase. Actor archetypes
can provide a complete creature build. The arena uses an explicit `arcane`
template for magical capability rather than granting Mana merely because an
actor knows a spell. Shared validation rejects invalid or dormant initial
choices before any encounter starts.

Use a fresh save when comparing edited definitions or different seeds. A resumed
save retains its original authored inputs. Revalidate after source edits; the
validator updates the package index and certificate. Arena optimization, private
traces, and wizard creature transformation controls remain in the
[implementation scope](creature-implementation.md).

## Combat authoring

The default dungeon declares a four-HD human Warrior with owned training,
attribute advancement and Power Strike, Guard, Heavy Blows and Mighty Blows.
Its iron greatsword is a separate equipped item; the species' natural attack is
an Impact strike. The scout uses a Light Weaponry racial build, the guardian
uses a two-HD Construct build with source-granted Impact protection, and the
wisp uses a Fire Elemental build with no equipment slots. Their health, skills,
defenses, protection and phases derive through the same creature rules used by
the arena. This replaces the dungeon's flat health/defense/damage profiles.


The manifest declares `factions` as faction names mapped to hostile faction names,
and `ai_profiles` as names mapped to `memory_ticks` and `flee_percent` settings.
Characters and actors declare combat through owned `creature` builds; an actor
archetype can provide the recipe, and an instance recipe replaces it as a whole.
The former flat `combat` field is rejected at all three authoring boundaries.
Species, advancement and templates provide health, defenses, natural attacks and
protection; equipped weapons supply their independent attack definitions.
The default dungeon and bundled combat fixtures use these shared definitions.

AI actors require an owned creature build and a known AI profile. Unselected AI
starting characters retain their authored inventory. The selected character
begins at an explicit input boundary even when its numeric ID follows an AI
actor. Ordinary actor decisions otherwise retain stable time/identity ordering.

An objective does not create a creature build or infer combat attributes.
Packages with an objective require declared builds for every starting character,
so outcome and combat status remain observable. Exploration actors in packages
without objectives may omit a build.

## Cell illumination

Regions accept `lit = true` (the default), or `lit = false`, plus sparse overrides:
`lighting = [{ at = [4, 2, 1], lit = true }]`. Coordinates must lie within the
region's stored bounds (including a chamber's shell). Duplicate cell overrides
are rejected. Illumination is independent of terrain and survives region eviction,
resume, replay and rewind. See [three-dimensional sight](sight-3d.md#ambient-illumination-and-local-awareness)
for how lighting controls perception and [generation recipes](generation-recipes.md)
for generated room lighting.
