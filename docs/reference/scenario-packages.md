# Authored scenario packages

A scenario package is ordinary TOML content checked by an explicit offline validator.
Packages work in normal games and with `--wizard`; authoring does not execute
wizard commands. Packages target the current ruleset, and older prerelease saves
are rejected without migration.

## Author and run

The default game is `scenarios/first-dungeon`; `scenarios/two-room` is a minimal
example, `scenarios/mob-arena` is a three-HD two-team creature arena, and
`scenarios/tests/` holds the test fixtures. A package directory contains
`scenario.toml`, its rules import/override TOML file (normally `rules.toml`), one file
per region in `regions/` (named `<region id>.toml`), and two files the validator
generates: `index.json` and `validation.json`.
Paths must stay inside that directory, including symlink resolution. The
manifest and rules file are each bounded to 8 MiB, each region file to 1 MiB,
and a package to
65,536 regions.

Run from the repository root after completing [development setup](../development/setup.md),
and use a fresh save path for a new scenario or seed:

```sh
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
writes the region index and binds SHA-256 hashes of the manifest, authored rules
source, shared import dependencies and index (which holds each region file's hash), the normalized model,
exact ruleset and
validator identity, and recorded coverage. Versions are
author-controlled `major.minor`; changing a hash does not require a version bump.
Manifest, rules and region source hashes normalize CRLF to LF automatically, so
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
See [region streaming foundations](../architecture/streaming.md) for its output and limits.

The manifest declares `format = 2`, `id`, `version`, `ruleset`,
`default_character`, and `characters`. Older formats are refused. The optional fields include `themes`, `zones`, `archetypes`, `appearance_pools`,
`creatures`, `factions`, `ai_profiles`, `arena`, `objective`, `assets`, `terrain`,
`generation_recipes`, `generation_groups` and `stair_pairs`. These organize content,
run policy and structural generation; unknown fields are rejected. This is a
navigation guide rather than a duplicate schema: [Manifest](../../crates/server/src/scenario_package.rs)
and the complete [first-dungeon](../../scenarios/first-dungeon/scenario.toml) and
[rogue-exploration](../../scenarios/rogue-exploration/scenario.toml) packages define
working examples. Zone theme pools replace world defaults;
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
Names follow the [place name](movement.md) rules. A hint has no extent,
room membership or prescribed presentation. It is independent of storage regions
and may coexist with objects and actors. Making a cell solid retains its authored
hint but suppresses disclosure until the cell becomes visible again. Terrain edits
do not automatically infer new hints. Clients can derive places without hints;
a visible hint is never proof that the surrounding area has been explored.

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
room early. See [generation recipes and groups](generation.md) for the
preparation, publication, and independent streaming contracts.

Actors, items, and doors have positive numeric IDs, unique within their entity
kind. Array order does not assign identities. Doors specify `at`, `open`, and `height` (default 1). Validation rejects a door
that doesn't fit, or that leaves its walled doorway open above it; see
[doors](movement.md).
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
an `ai` identifier. AI identifiers resolve to manifest `ai_profiles`; see [dungeon gameplay](../play/gameplay.md).

Region gravity vectors/sparse overrides, body declarations, initial velocity,
and full portal rotations are implemented; see [physics](../architecture/geometry.md).
Objective declarations (`anchor`, optional authored item ID, `disclosed`,
`continue_play`) are implemented; see [dungeon gameplay](../play/gameplay.md). Regions
can be generated; see [generated regions](#generated-regions). Dependency
registries are not supported. Rules imports use the explicit built-in general
rules identity; arbitrary filesystem paths and network imports are rejected.
External content/generator dependency fields are rejected rather than silently
ignored. Equipment uses modifier sources and may supply a weapon attack.

## Shared rules, scenario overrides and content identities

The manifest’s `ruleset = "rules.toml"` names a relative TOML file inside the
package. Ordinary scenarios import the general rules rather than copy them:

```toml
import = "interactions-v27"

[[overrides]]
op = "set"
path = ["resources", "mana", "recovery_period"]
value = 800
```

The import resolves the shared source at
`crates/simulation/data/default-ruleset.toml`. It is embedded in the executable,
so package loading has no working-directory dependency or network access. A
canonical edit therefore applies to every importing scenario after rebuild and
revalidation. An unknown import is an error, never a fallback.

Overrides are explicit structural operations: `set` replaces an existing field
or definition, `add` creates a missing field or definition, and `remove` deletes
an existing one. Catalog path segments select stable `id` values (or `resource`
for resource definitions), never array positions. Added/replaced definitions
must retain the selected ID. Other arrays are replaced as whole values. Duplicate
or overlapping paths, unknown fields, missing targets and an override of the
ruleset ID are rejected. Operations are applied in authored order, then the
complete result passes the same strict schema, reference, cycle and numeric
validation as a standalone rules document. No creature is constructed from a
partially validated ruleset.

Standalone full rules documents remain supported for detached experiments and
explicit custom rulesets with the current format ID. The resolved catalogs cover
abilities, ability tags, talents, conditions, types, subtypes, classes,
descriptors, flags, resources and equipment slots.

Definitions use stable string IDs. Changing declaration order cannot change a
saved choice. Adding a talent using existing requirements and modifier operations
requires only data edits and revalidation. Species and templates remain in the
scenario’s `creatures` catalog and refer to the resolved rules’ IDs.

Species, templates and equipment expose `modifiers`, each with an explicit scope
and operation. For example, a physical-defense source is:

```toml
modifiers = [{ scope = "always", op = { kind = "add", stat = { kind = "defense", defense = "physical" }, value = 2 } }]
```

Consumables use the same effect vocabulary as abilities. Fixed healing is
`{ kind = "heal", amount = { kind = "fixed", amount = 5 } }`. Damage uses a
`components` expression whose entries name a category, optional descriptor and
fixed or rolled amount. Conditions name a condition ID and duration; resource
restoration names a resource and amount. Equipment defense and damage reduction
are modifiers, not separate protection fields.

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

Use [generation](generation.md) for independent `rooms` regions,
recipe/group declarations, deterministic streams and publication. Structural
metadata is available to horizon planning before generated coordinates exist.

## Validation, persistence, and tests

Validation exhaustively constructs the bounded authored world (1–65,536 regions,
up to 32×32×8 interior cells each), checking IDs, geometry, references, anchors,
placements, inventory, and all character selections. It repeats construction at
seeds 0, 1, and 42 to check determinism. The certificate records this coverage;
it is neither a gameplay/winnability proof nor a cryptographic trust signature.

### Saves and the package

A save pins its package by the certificate's model hash and records its ruleset
ID, effective rules hash and each shared import’s source hash. It stores the
authored import/overrides, one resolved rules snapshot and the scenario catalog
with the initial manifest. Standalone rules documents need no second snapshot.
Restore uses that saved resolution rather than re-importing a possibly changed
general rules source. Actor records contain species/template IDs,
owned advancement choices and mutable state; restore resolves those IDs against
the saved catalog and recomputes derived values. Checkpoints and detached-region
records do not repeat species or template definitions. The original rules file
is not needed to restore these definitions. If a streamed save still needs
uncopied region files, the loader matches their manifest/index and authored
import source against the saved rules context, without re-resolving the current
general import. Each region source still must match its pinned hash.

Restoration validates item modifiers, consumable effects and attack descriptors
against those same rules, including ground and detached-region items. Actor
records retain their raw active-time clock and mutable timers; the clock cannot
exceed the actor’s active or frozen time. Derived equipment and condition sources
are rebuilt from their owners, and cannot replace permanent catalog sources.
Invalid references and inconsistent state return typed persistence errors before
a restored game is published.

The save also keeps the region index and copies each region file into its `region_sources`
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

## Item quantities and knowledge

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.

## Creature builds and arenas

Species, templates, owned advancement and natural/equipped attack declarations
are documented in [creature rules](creature-rules.md#authoring-creature-builds-and-attacks).
Use [arena evaluation](../development/arena.md) for the `mob-arena` example, bounded unattended
runs, pause/step controls, search and diagnostic workflows. Privileged advancement
and template commands are in [wizard tools](../development/wizard.md). Every edited package
needs revalidation, and an existing save retains its original content.

## Combat authoring

The default dungeon declares a four-HD human Warrior with owned training,
attribute advancement and Power Strike, Guard, Heavy Blows and Mighty Blows.
Its iron greatsword is a separate equipped item; the species' natural attack is
an Impact strike. The scout uses a Light Weaponry racial build, the guardian
uses a two-HD Construct build with source-granted Impact protection, and the
wisp uses a Fire Elemental build with no equipment slots. Their health, skills,
defenses, protection and phases derive through the same creature rules used by
the arena. These actors have no separate flat combat path.


The manifest declares `factions` as faction names mapped to hostile faction names,
and `ai_profiles` as names mapped to `memory_ticks` and `flee_percent` settings.
Characters and actors declare combat through owned `creature` builds; an actor
archetype can provide the recipe, and an instance recipe replaces it as a whole.
A flat `combat` field is rejected at all three authoring boundaries.
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
resume, replay and rewind. See [three-dimensional sight](../architecture/perception.md#ambient-illumination-and-local-awareness)
for how lighting controls perception and [generation recipes](generation.md)
for generated room lighting.

## Verification and performance boundaries

The [scenario integration suite](../../crates/server/tests/it/scenario_packages.rs),
[package reference check](../../scripts/test_scenario_references.py) and actual-process
package tests cover strict loading, source normalization, replay, restart and
rewind. Package invariants also exercise atomic rejection and retry receipts.
Validation deliberately distinguishes sampled generated worlds from proof over
all seeds. Performance workloads distinguish source parsing, construction,
authoritative play, explicit durable flush and restart; neither a disconnected
large package nor a fast command call establishes exploration or disk latency.
See [performance measurement](../development/performance.md) for workloads, historical
evidence and unresolved persistence tails.
