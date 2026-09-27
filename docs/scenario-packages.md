# Authored scenario packages

Milestone 4a adds ordinary TOML input packages and an explicit offline validator.
Packages work in normal games and with `--wizard`; authoring does not execute
wizard commands. Items extend this foundation with protocol 16. Save format 11 and `dungeon-v16`
reject earlier pre-release saves; there is no migration.

## Author and run

The complete example is `scenarios/two-room`. A package directory contains
`scenario.toml`, one or more explicitly listed TOML content files, and the generated
`validation.json`. Paths must stay inside that directory, including symlink
resolution. Files and aggregate source are bounded to 8 MiB.

```powershell
cargo run -p tor-server --bin tor-scenario -- validate scenarios/two-room
cargo run -p tor-server --bin tor-server -- --scenario scenarios/two-room --seed 42 --save saves/new.db
```

Set the usual server authentication token first. Omitting `--scenario` selects
the two-room package. `--character <numeric-id>` selects a starting character;
omitting it uses `default_character`. Connect the client using `--actor <id>`.
`--regions` retains the independently versioned diagnostic workload and conflicts
with `--scenario`. Existing saves own their inputs; scenario/seed arguments do
not replace them, and resuming does not require the original package directory.

Run validation after **every source edit**, including comments. Validation binds
SHA-256 hashes of the manifest and each declared file, the normalized model,
exact ruleset and validator identity, and recorded coverage. Versions are
author-controlled `major.minor`; changing a hash does not require a version bump.
Validator failures produce a nonzero exit status and JSON diagnostics on stderr.
Stale/missing validation is refused by default. `--allow-unvalidated` prints a
warning and permits structurally valid development input. It does not enable
wizard access or permit unavailable mechanics. Invalid geometry is always rejected.

## Format

The manifest declares `format = 1`, `id`, `version`, `ruleset`, `files`,
`default_character`, and `characters`. Optional `themes`, `zones`, `archetypes`,
and `objective` describe the world. Zone theme pools replace world defaults;
omitting a zone pool inherits the world pool. An empty pool deliberately replaces
it with no themes. These are content identifiers, not client asset disclosures.

Content files contain `[[regions]]`. Regions have stable positive numeric `id`,
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
kind. Array order does not assign identities. Doors specify `at` and `open`.
Items specify `at`, `name` or an `archetype`, and optional `carried_by`. Archetype
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
`continue_play`) are implemented in milestone 4d. Generation, dependency
registries, streaming, equipment and the dungeon loop are outside 4a. The current
package is self-contained and depends on one exact built-in ruleset; external
content/generator dependency fields are rejected rather than silently ignored.

## Validation, persistence, and tests

Validation exhaustively constructs the bounded authored world (1–256 regions,
up to 32×32×8 interior cells each), checking IDs, geometry, references, anchors,
placements, inventory, and all character selections. It repeats construction at
seeds 0, 1, and 42 to check determinism. The certificate records this coverage;
it is neither a gameplay/winnability proof nor a cryptographic trust signature.

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

## Local verification and performance

The following records the pre-4b baseline (protocol 13, save format 8,
`scenarios-v13`); current item behavior and verification are described in
[items](items.md). Milestone 4a Windows verification passed 246 Rust tests in each of debug and release,
103 Python checks in debug and 70 real-process checks in release, formatting,
all-target Clippy, architecture checks and rustdoc with warnings denied. This is
local verification, not a claim that new Windows/Linux CI has run. The three
desktop shortcuts also pass actual connection, role, fresh-save and owned-process
cleanup checks using rebuilt release executables. The full desktop
rerun resolves three sandbox temporary-directory errors and one denied cursor
operation; initial logs remain in `.local/4a-*`. Earlier migration failures are
also retained rather than removed from the record.

[Raw samples and summary](measurements/scenario-packages-2026-09-27/summary.json)
and [source/binary hashes](measurements/scenario-packages-2026-09-27/manifest.json)
record release measurements. Existing mixed workloads use 10 cycles, with 290
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
