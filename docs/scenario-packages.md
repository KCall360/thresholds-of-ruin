# Authored scenario packages

Milestone 4a adds ordinary TOML input packages and an explicit offline validator.
Packages work in normal games and with `--wizard`; authoring does not execute
wizard commands. Packages target the current ruleset, and older prerelease saves
are rejected without migration.

## Author and run

The default game is `scenarios/first-dungeon`; `scenarios/two-room` is a minimal
example, and `scenarios/tests/` holds the test fixtures. A package directory contains
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
Validator failures produce a nonzero exit status and JSON diagnostics on stderr.
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
hints and do not reveal authored room names.

Outgoing portals specify `at`, cardinal/vertical `direction`, destination anchor
`to`, and optional `turns`, `width`, `height` (defaults 0, 1, 1). Reverse portals
are explicit. The validator constructs every region before checking connections.

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

## Generated regions

A region file with a `[generate]` table authors only the region's structure:
`id`, `name`, `size`, optional `zone` and `chamber`, its entry `anchors` and its
`portals`. Walls, openings, places, doors, items, actors and gravity overrides
are refused there. The generator fills the region the first time it's built:

```toml
[generate]
generator = "rooms"
version = 1
rooms = [3, 6]
actors = { archetypes = ["rat"], ai = "wander", count = [1, 3] }
items = { archetypes = ["coin"], count = [2, 4] }
```

- `rooms` (the only generator, version 1) keeps a clearing two cells wide
  around every anchor, places 1–16 rooms, and joins the anchors and rooms in
  turn with corridors, so every entry reaches every other. Two entries on the
  same row are joined by a straight corridor along it. Actors (up to 64, each
  an archetype from the pool, run by the named AI profile) and items (up to
  64) go on open floor away from the entries.
- A region's content depends only on the game's seed, the region's own file
  and its id: never on the rest of the package, or on which regions were
  built before. Its actors and items take identities from a range of 256 fixed
  by its region id, above every authored identity; generated region ids are
  at most 65,536, and a game reserves the whole space. So identities don't
  depend on build order either, and the preloader can build generated regions
  ahead of need.
- Declaring a generated region (see
  [region streaming](region-streaming.md#never-built-regions-and-region-sources))
  runs its generator to learn the identities it will hold.
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
that built it. Replay rebuilds regions from these copies, and each copy is
checked against the index's hash when it's used; a damaged copy fails closed.
So a save's size grows with the regions played, not the package.

Regions not built yet are read from the package directory, checked against the
index. A save remembers where its package was. If regions remain unbuilt and
that directory is gone or holds a different package, the server refuses to
resume and asks for `--scenario` naming the same package, rather than failing
when play reaches them. Once every region the game can build has been copied,
it resumes without the package.

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

## Combat authoring

The manifest declares `factions` as faction names mapped to hostile faction names,
and `ai_profiles` as names mapped to `memory_ticks` and `flee_percent` settings.
Actors and characters may specify `combat`; actor archetypes may provide it as a
default. An instance `combat` record replaces the archetype record as a whole.
The record supports `name`, `max_hp`, `defense`, `faction`, `attack`, `immunities`,
and `reductions`. Attack records contain `bonus`, `wind_up`, `recovery`, and a
`damage` map keyed by damage type. See `scenarios/first-dungeon` for an example.

AI actors require combat attributes and a known AI profile. Unselected AI starting
characters retain their authored inventory. The selected character begins at an
explicit input boundary even when its numeric ID follows an AI actor. Ordinary
actor decisions otherwise retain stable time/identity ordering.

A participating character without an explicit combat record receives the default
combat attributes when the package defines an objective. This keeps objective-only
packages observable in both clients, including immediate victory at the start.
Packages without combat or objectives retain the noncombat diagnostic behavior.
