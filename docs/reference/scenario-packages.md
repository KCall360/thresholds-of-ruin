# Authored scenario packages

A scenario package is ordinary TOML content checked by an explicit offline validator.
Packages work in normal games and with `--wizard`; authoring does not execute
wizard commands. Packages target the current ruleset, and older prerelease saves
are rejected without migration.
Rules documents, ability definitions and item archetypes use named records,
including their public tool-facing DTOs; positional arrays are not aliases.
Documented optional authoring fields retain their omission defaults.

## Author and run

The default game is `scenarios/first-dungeon`; `scenarios/two-room` is a minimal
example, `scenarios/mob-arena` is a three-HD two-team creature arena, and
`scenarios/tests/` holds the test fixtures. A package directory contains
`scenario.toml`, its rules import/override TOML file (normally `rules.toml`), one file
per authored region in `regions/` (named `<region id>.toml`), optional catalog
modules and procedural descriptors. A purely procedural package needs no region
files. Validation generates `index.json`, `validation.json` and immutable compiled
inputs under `.compiled/`.
Paths must stay inside that directory, including symlink resolution. The
manifest and rules file are each bounded to 8 MiB, each region file to 1 MiB,
and a package to 65,536 authored regions and 64 MiB of authored region source.
Module source, compiled metadata, procedural address spaces and validation work
have separate bounded contracts; the region-file count is not a procedural world
size limit.

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

## Source resolution and catalog modules

`tor-scenario inspect <directory>` resolves the authoring source closure and
prints the merged manifest, declaration provenance, and raw source hashes as
JSON. `tor-scenario check <directory>` runs the same proof as validation without
writing certificates or changing any package files. `validate` additionally
publishes the certified output. All three use the server's Rust compiler.
`tor-scenario resolve-rules` reads bounded rules TOML from standard input and
returns the effective TOML, parsed document and dependency hashes as JSON; arena
Python tools call this interface rather than implementing a second resolver.

A manifest may declare `modules = ["catalog/creatures.toml", "catalog/items.toml"]`.
Each module is a strict TOML document with a `namespace`, and any of `creatures`
(species/templates), typed `actors` and `items` catalogs, `archetypes`, `ai_profiles`, `appearance_pools`, or
`generation_recipes`, `generation_groups`, `stair_pairs` and `procedural_worlds`.
Topology modules use the same explicit namespaces and duplicate rejection as
content modules; they retain compact descriptors rather than expanding regions.
Other keys, nested imports, scripts and network
resolution are rejected. For example:

```toml
namespace = "expedition"

[items."expedition.token"]
name = "copper token"
```

Use the complete ID (`expedition.token`) in references. Module IDs must begin
with their namespace and a dot; qualification never rewrites references or
implicitly opts content into a generator. Dotted namespace components use
lowercase ASCII letters, digits, underscores and hyphens. Namespaces may be
shared by multiple files, but duplicate IDs within a catalog are errors, including
collisions with root-manifest declarations. There is no last-writer-wins merge
or inheritance. Authored ordering of independent module includes does not change
the resolved catalogs, which have deterministic key ordering.

Includes must name distinct package-local TOML files, cannot traverse parent
paths or use symlinks, and cannot refer to the root manifest or region files.
A source closure allows at most 64 module files, 8 MiB per file and 32 MiB total
including the manifest. Resolved rules and manifest metadata must also fit the
compiled snapshot metadata budget (960 KiB); raw source limits do not override
that checked save-frame budget. Every module is part of the raw integrity closure, so a
comment edit requires recertification. Resolved manifests contain their merged
catalogs and no live include directives; saved content does not reread modules.
Parser errors name the actual module. Inspection identifies each declaration's
source; semantic errors retain their declaration without inventing coordinates
in the generated merged text.

Ordinary generated maps should use the versioned named preset:

```toml
[generation_recipes.floor]
kind = "grid_rooms"
version = 1
room_width = [5, 10]
room_height = [3, 5]
extra_connections = [0, 2]
```

The compiler lowers this preset to private, stable stage identities. The explicit
low-level recipe form remains available for advanced engine fixtures; its stage
order is fixed by the algorithm contract. It is not a freely composable pipeline.
See [generation](generation.md) for capabilities and constraints.

Arena parameter targets use stable catalog IDs (for example
`["resources", "mana", "recovery_period"]`) and numeric actor/character IDs,
not positions in those arrays. Positional indexing remains only inside
non-identity arrays such as ordered effects. See the
[arena guide](../development/arena.md) for the versioned edit contract.

Use `actors` for reusable actor definitions and `items` for reusable item
definitions. These root/module tables are catalogs, distinct from region files'
concrete `actors` and `items` arrays. Both compile to the existing shared
archetype identities; region instances still reference them through `archetype`.
An actor definition accepts `creature`, `anatomy`, `body`, `turn_ticks` and
`asset`. Actor names belong to `creature.name`; a standalone actor `name` field
is rejected because the current engine has no anatomy-only naming mechanism. An item definition accepts `name`, `class`, `equipment`,
`consumable`, `supply`, `identity`, `appearance_pool`, `stackable`, `properties` and `asset`.
A supply names a rules-defined finite-cost capability; declaring it alone does
not grant an eating or drinking action.
Irrelevant fields are rejected: an actor cannot declare item stacking or
consumable effects, and an item cannot declare actor anatomy or turn duration.
Duplicate IDs across `actors`, `items` and the low-level `archetypes` catalog
are errors. The broad low-level catalog remains available for advanced fixtures.

Semantic errors identify the owning module and declaration. Reference spans use
the original field/table shape, including typed catalogs, only after the source
hash matches the resolved closure. If source has changed or disappeared, errors
retain filename and declaration without guessed coordinates. Inspection exposes
both source-view and compiled declaration provenance.

### Mixed authored and procedural example

The [first dungeon](../../scenarios/first-dungeon/scenario.toml) demonstrates
separate strict [actor](../../scenarios/first-dungeon/catalog/actors.toml) and
[item](../../scenarios/first-dungeon/catalog/items.toml) catalogs. The
[mob arena](../../scenarios/mob-arena/scenario.toml) keeps scenario-specific
species and character choices in its root and imports reusable actor definitions.
Modules do not implicitly qualify references: those actors explicitly reference
the root species IDs.

An authored entrance and a generated chain can share one package. Add a topology
module to the root's explicit module list. For an authored region `1` with a
traversable anchor named `descent`, a `worlds/caves.toml` fragment can contain:

```toml
namespace = "caves"

[generation_recipes."caves.rooms"]
kind = "grid_rooms"
version = 1
room_width = [4, 6]
room_height = [3, 5]
extra_connections = [0, 1]

[procedural_worlds."caves.depths"]
kind = "grid_chain"
version = 1
base_region = 100
grid = [1, 1]
size = [12, 10, 2]
recipe = "caves.rooms"
domain = { kind = "seeded_finite", min = 2, max = 6 }

[stair_pairs."caves.entry"]
upper = "1/descent"
lower = "100/up"
```

This declares two to six seed-selected floors without region stub files. The
reserved generated identity range must not overlap authored regions or another
family. The ordinary typed stair pair joins the authored anchor to a named
generated port; coordinates are selected when its generation unit materializes.
The finite domain is a world-generation choice, not an implicit objective.
Run `check` for the complete package before publishing its validation output.

## Format

The read-only `tor-scenario horizon <directory> <region-id> <portal-hops>` command
inspects a structural preload neighborhood without constructing gameplay state.
See [region streaming foundations](../architecture/streaming.md) for its output and limits.

The manifest declares the [current `format`](../status.md), `id`, `version`, `ruleset`,
`default_character`, and `characters`. Older formats are refused at the shared
admission boundary for authored, compiled and saved package metadata. Current
semantic hashes or validator evidence cannot bypass header validation.
The optional fields include `themes`, `zones`, `archetypes`, `appearance_pools`,
`creatures`, `factions`, `ai_profiles`, `arena`, `objective`, `assets`, `terrain`,
`generation_recipes`, `generation_groups`, `procedural_worlds`, `stair_pairs`,
`capabilities` and `floor_activation`. These organize content,
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
are explicit. Physical portal declarations connect authored regions; targeting a
procedural family is rejected in version 1. Use `stair_pairs` for authored/generated
composition. Validation constructs authored geometry and bounded procedural samples
before checking their supported connections.

### Selected-character starting loadout

A character can declare a `loadout` array of ordinary item archetypes:

```toml
loadout = [
  { archetype = "rogue.mace", quantity = 1, equipped_slot = 0 },
  { archetype = "rogue.ring_mail", quantity = 1, equipped_slot = 2 },
  { archetype = "rogue.clean_bandage", quantity = 3 },
  { archetype = "rogue.ration", quantity = 1 },
]
```

These rows belong inside the selected character declaration, not inside its
`creature` table. The compiler resolves the same ordinary item definitions used
by region items. The selected character receives the items once before its first
playable view. Quantities and equipped slots must satisfy ordinary stacking,
anatomy and equipment validation; numeric slots are specific to that anatomy.
Use `known_identities` on the character to establish starting item knowledge.
Unselected omitted characters receive no inventory. The saved base preserves
ordinary item identities and ownership; recovery never reapplies the kit on top
of saved inventory. These rows are independent of floor stock.

### First-entry population

Geometry preparation and gameplay population are separate. A manifest can bind
one typed initial-entry profile to a compact procedural family. This minimal
emu/bandage schema example is not the bundled Rogue population; the
[Rogue play guide](../play/rogue.md#floors-keep-their-history) describes that profile:

```toml
[capabilities]
npc_stair_traversal = false

[floor_activation]
version = 1
protagonist = "selected_character"
families = { dungeon = "rogue.low_depth" }

[floor_activation.profiles."rogue.low_depth"]
version = 1

[[floor_activation.profiles."rogue.low_depth".stages]]
id = "rogue.emus"
version = 1
kind = "actors"
count = [2, 2]
bands = [{ depth = { kind = "open", minimum = 1 }, choices = [{ archetype = "rogue.emu", weight = 1 }] }]
ai = "rogue_hostile"
avoid_protagonist_sight = true

[[floor_activation.profiles."rogue.low_depth".stages]]
id = "rogue.bandages"
version = 1
kind = "items"
count = [1, 1]
bands = [{ depth = { kind = "open", minimum = 1 }, choices = [{ archetype = "rogue.clean_bandage", weight = 1 }] }]
quantity = 2
```

The selected-character selector resolves to the concrete declared character for
the saved run, including an alternate preset. A numeric protagonist is available
for fixtures but must agree with that selection. `families` maps existing
procedural family IDs to declared profile IDs. Each ordered stage has a stable
ID and version, an inclusive bounded attempt-count range and 1–32 ordered depth
bands. A band has an explicit `finite` inclusive minimum/maximum or `open` minimum;
minimums are positive, bands cannot overlap, and an open band must be last. Gaps
and retired content produce a saved `no_eligible_content` stage skip. Every band
has 1–64 distinct archetypes with positive integer weights whose sum fits u32.
A profile has at most 32 stages and 256 total maximum attempts; the package
supports at most 32 profiles. No implicit terminal depth or HD clamp is added.
Actor stages reference ordinary actor archetypes and an AI profile; item stages
reference ordinary item archetypes and a stack quantity. The tagged forms are
strict: `quantity` belongs only to item stages, while `ai`,
`avoid_protagonist_sight` and `depth_growth` belong only to actor stages. Archetypes
referenced by these stages do not create entities merely by existing in a catalog.

Actor stages may author `depth_growth = { kind = "racial_after", depth = 26 }`.
The default `{ kind = "fixed" }` keeps the base recipe. Racial growth appends
`max(0, floor_depth - depth)` actual racial HD to the authored base, preserving
all original class/racial rows, choices and ordinal Health seeds. New rows have
explicit empty choices and retain their ordinary unspent grants. An authored
restoration recipe grows by those same empty racial ordinals; choose-again and
future-source policies retain their meanings. A drained prototype whose retained
authored restoration recipe extends beyond its present base is rejected, rather
than overwriting its pending source or purchase choices.

Construction makes bounded per-birth passes over the complete owned rows instead
of rebuilding every growing prefix. Prototype restoration, build validation and
actor installation retain separate derivation/compilation boundaries. The narrow
helper counter does not measure all of that work; end-to-end costs still need
separate measurement.

This is a checked intermediate implementation with the existing 256-HD and
ordinary derived-sheet limits. A selected birth exceeding capacity refuses the
entire entry with a recoverable resource-limit error: no time or movement, RNG,
identities, stock prefix, partial destination or completed receipt is published.
It is not a terminal floor, a silent HD clamp or completed deep scaling. Wide
counts and lossless chunked persistence remain separate work. The policy does
not add haste or change XP coefficients; species selection belongs to the
authored population bands.

Weighted content selection uses one random word from each attempt's independent
content stream: multiply-high maps the word into the sum of weights, then the
first containing cumulative interval selects the choice. This deterministic
fixed-work mapping is a deliberate current-format boundary for newly weighted
content; it is not an exact arbitrary-weight probability claim. Existing count
and cell-shuffle sampling is unchanged. Singleton bands retain their content,
count and placement streams. Stage ordering still affects occupancy, so changing
actor choices may change later supply placement even when supply streams agree.
The historical recovery excerpt also replaced count/cell sampling; that unrelated
change was intentionally not replayed. Independent streams, transaction rollback,
strict receipts and saved source-free replay remain mandatory.

Preparation can make geometry and placement candidates available without
choosing content, allocating entities or advancing gameplay RNG. The selected
protagonist's successful first entry commits population before later simulation
time advances. Normal body-fit, occupancy and protected arrival-cell checks
still apply. Each stage records evaluated counts, placed entities or precise
skips; a later return never rerolls a skip or replenishes consumed stock. The
ledger and ordinary entities share the action's atomic publication and recovery.
These private decisions are not client navigation or population disclosures.

Actor choices may also declare optional finite birth stock, using only a named,
source-owned item pool. For example, this declaration belongs on one actor choice
inside an actor depth band:

```toml
# Actor choice:
# { archetype = "carrier", weight = 1,
#   carried_loot = { version = 1, pool = "supplies",
#     chance = { numerator = 1, denominator = 5 } } }

[floor_activation.carried_item_pools.supplies]
version = 1
choices = [{ archetype = "ration", weight = 1, quantity = 2 }]
```

Pools and carry policies currently require version 1. There may be at most 32
named pools, each with 1–64 distinct ordinary item archetypes, positive weights
with a checked u32 sum, and fixed positive u64 quantities. Non-stackable items
require quantity one. Every pool is validated and resolved through the ordinary
item adapter, even when unused, unreachable or referenced by a zero-chance policy.
The fraction requires a positive denominator and numerator no greater than it;
zero and certainty are valid. `carried_loot` belongs only to actor choices.
Omission means no policy; compiled and saved current-format records retain an
explicit nullable policy and an explicit pool map.

A legally placed actor receives zero or one ordinary carried stack, with its own
item identity and exact authored quantity, before the entry can publish. Chance
uses exact bounded fraction sampling. A separate item-content stream uses the
same multiply-high weighted mapping as ordinary population. Both use a separate
birth-stock domain keyed by seed, scope, profile/stage identities and versions,
attempt ordinal, actor content key and policy version; only item-content includes
the pool identity/version. Existing count, placement, build and combat streams
are unchanged. A placement skip does not evaluate a carry policy.

The actor's existing placement receipt retains a nullable nested result: no
policy, an explicit chance miss, or the chosen content/quantity and ordinary item
ID. Audits recompute this result from pinned inputs and include nested births in
identity reservations, all-branch uniqueness and bounded decision counts. Birth
receipts describe historical provenance, not current ownership: ordinary taking,
splitting, merging, consumption and death drops remain authoritative. Carrying
stock does not equip it, identify it or enable care/equipment/loot AI tactics.
No death roll, replacement stock, species switch or extra inventory ledger exists.
Initial and later entries both discard the whole candidate on construction,
identity, decision-capacity or Store failure; legal-cell exhaustion alone remains
a placement skip. Query, preload, revisit, ascent and restart never grant stock.

The optional `npc_stair_traversal` capability defaults to allowing NPC stairs.
When false, only the selected declared player character can voluntarily traverse
an abstract stair; this role does not change with connections, control transfer
or AI toggles. The restriction applies to remembered route edges and final
movement execution. It does not suppress visible stair cells, horizontal moves
from them, physical portals, forced displacement or falling. This fixed scenario
policy is independent of whether a population profile is declared.

Profiles are finite entry-time content, not a scripting API, wandering clock,
refill rule, food-pressure mechanism or Amulet objective. The
[Rogue package](../../scenarios/rogue-exploration/scenario.toml) is the complete
source example; its provisional content is described in the
[Rogue guide](../play/rogue.md).

### Unique retrieval objective

`unique_retrieval` declares a small typed entry objective separately from the
once-only `floor_activation` population. The Rogue package uses:

```toml
[unique_retrieval]
version = 1
protagonist = "selected_character"
family = "dungeon"
minimum_depth = 26
archetype = "rogue.amulet_of_yendor"
exit_anchor = "dungeon/1/up"
```

The family and surface endpoint must resolve through the declared procedural
world. The objective binds the selected character, not its current controller.
Its item must be one ordinary nonstackable identity, excluded from authored
births, starting loadouts and population stock. The initial eligible entry
creates it once; every actual selected-player entry at or below the threshold
then relocates the same identity unless that player carries it. Eligible revisits
still run this objective after the independent once-only population decision.
Ground piles, the entrant's cell and same-cell reselection remain legal.

Destination candidates use current supported, reachable geometry. A remote
source is loaded only for ordinary ownership reconciliation; that dependency
cannot generate or activate another floor, thaw its actors or become a streaming
reference point. Movement, population, relocation, identity allocation and RNG
publish together, or the whole entry is rejected. No candidate means a recoverable
entry refusal, not a missing item or duplicate creation. Queries, preparation,
NPC travel and restore do not trigger relocation.

The surface stair is a disclosed terminal endpoint with no invented exterior
landing. Only the configured player deliberately executing ordinary `up` there
with the bound item wins. Missing-item refusal costs no time. Occupancy, forced
movement and teleportation alone never complete the objective. Dungeon-to-dungeon
stairs keep their usual semantics. Successful surface escape is an immediate
terminal resolution: it adds no world ticks, recovery interval or subsequent
dungeon actor work. It does not fabricate an exterior landing or a paid journey
after the accepted victory.

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
Procedural starts may use an existing `family/depth/up` or `family/depth/down`
port within the seeded domain and operational limit; see
[logical endpoints](generation.md#compact-grid-chain-families).
Only the declared starting groups and their streaming neighborhood are needed,
regardless of the depth number. `family/1/start` remains the first-floor room entry.
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
import = "interactions-v37"

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
an existing one. Catalog path segments select stable `id` values (`resource` for resource
definitions, `pool` for point grants), never array positions. For example,
`["classes", "warrior", "point_grants", "advancement", "points"]` tunes a
class grant without changing its pool identity. Added/replaced definitions
must retain the selected ID. Other arrays are replaced as whole values. Duplicate
or overlapping paths, unknown fields, missing targets and an override of the
ruleset ID are rejected. Operations are applied in authored order, then the
complete result passes the same strict schema, reference, cycle and numeric
validation as a standalone rules document. No creature is constructed from a
partially validated ruleset.

Standalone full rules documents remain supported for detached experiments and
explicit custom rulesets with the current format ID. The resolved catalogs cover
abilities, ability tags, talents, point pools, conditions, types, subtypes, classes,
descriptors, flags, resources and equipment slots.

Definitions use stable string IDs. Changing declaration order cannot change a
saved choice. Adding a talent using existing requirements and modifier operations
requires only data edits and revalidation. Species and templates remain in the
scenario’s `creatures` catalog and refer to the resolved rules’ IDs.

Condition definitions require a typed `family` ID and `lifecycle` (`timed`,
`next_check` or `next_check_against`). A one-use family has one definition, is
not source-scoped and cannot stack. Its modifiers are signed unit outgoing or
incoming check-only edge matching its lifecycle. Global `check_edge` supports
unconditional or `versus_source` scope; `edge` remains a shared check/effect
budget. `incoming_edge` can select a defense for
timed conditions; `ability`/`ability_tag` modifiers support `check_edge`.
Talent and condition sources reject flat `check_bonus` and reject `set` for
all edge stat forms, including through flags; capped edge grants use `add`.
Uncapped species/equipment sources retain typed-stat `set` behavior.
Stat objects reject unknown fields in rules and scenario authoring, including
payload-free `health`, `edge` and `check_edge` forms; these contain only `kind`.
See [condition and edge contracts](creature-rules.md#conditions-and-active-time).

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

Use [generation](generation.md) for independent `rooms` regions, explicit bounded
groups and compact `procedural_worlds` grid-chain descriptors. A descriptor-only
package needs no `regions/` directory and its authored `index.json` is empty;
possible procedural regions are not pre-enumerated in either source or artifact.
Descriptors require an explicit algorithm kind/version and finite, seed-selected
finite or open operational domain. They support general bounded rectangular grids,
not a fixed nine-member public contract.

Structural address/neighbor/port resolution happens before geometry and remains
backend-private. Logical family ports compose with authored maps through ordinary
stair pairs. Generation publishes a bounded group once; its members then use
ordinary independent residency, mutable records and save/rewind behavior. See
[generation domains and limits](generation.md#compact-grid-chain-families) and
[authored connections](generation.md#connecting-authored-maps-and-families) for
schema, deterministic identity, work budgets and validation coverage.

## Validation, persistence, and tests

Validation exhaustively constructs the bounded authored regions (up to 65,536,
up to 32×32×8 interior cells each), checking IDs, geometry, references, anchors,
placements, inventory, and all character selections. It repeats construction at
seeds 0, 1, and 42 to check determinism. Procedural families are validated
structurally and through bounded, budgeted group samples rather than expanded in
full. The certificate records this distinction;
it is neither a gameplay/winnability proof nor a cryptographic trust signature.
Before constructing geometry, aggregate cell-work is capped at 67,108,864, including
chamber shells, repeated seeds and character selections. Procedural sample work has
an additional 4,194,304-cell cap. See [generation](generation.md) for accounting;
small-region scalability fixtures are judged by their bounded volume rather than
region count alone.

### Saves and the package

Source integrity and simulation identity are separate. `content_hash` pins the
raw normalized source closure, index and import dependencies. `model_hash` pins
resolved definitions, effective rules, generator contracts and semantic region
metadata/compiled inputs, excluding source filenames, comments and release
labels. Random generation uses its own named, versioned streams rather than
either package hash. Changing a comment requires revalidation, not a new world.

Validation publishes a complete immutable directory under `.compiled/` before
replacing frontend index/certificate files. The directory contains a versioned
`package.json` and canonical compiled JSON inputs for authored regions. A
procedural family stays one descriptor: compilation/export does not enumerate
its potential members or create their terrain. Derived artifact directories are
build outputs, not hand-edited sources. Output symlinks and nonregular targets
are rejected; interrupted frontend metadata replacement fails closed through
certificate checks. These checks assume a trusted, nonconcurrently modified
package directory, not an adversarial filesystem sandbox.

A save pins the resolved manifest/catalog, a versioned canonical rules document,
dependency provenance and the compiled region index. It does not store authored
rules TOML or re-import current general rules. The `region_sources` table retains
its database name, but its values are now versioned compiled JSON inputs, not
source text. Inputs actually read by a published build are copied in the same
transaction as the journal admission. A corrupt saved input fails closed; the
loader never substitutes an external copy to hide corruption.

Unvisited authored regions can be read lazily from the exact immutable compiled
artifact. Editing or removing original TOML/modules does not alter it. A save
retains a location hint; if it still needs uncopied authored inputs and no matching
bundle is available, resume refuses and requests `--scenario` pointing at matching
compiled content. Individual region chunks are checked when read, not eagerly at
resume; a missing or corrupt chunk rejects the attempted load without publishing
partial world changes. For a new development/in-memory save with no published
artifact, the server captures all authored compiled inputs once, with a 128 MiB aggregate
bound. That fallback has an upfront authored-input cost; ordinary artifact-backed
saves retain lazy growth. Neither route eagerly generates procedural geometry.

Use `tor-scenario export <source-package> <new-destination>` to create a portable
compiled package. The destination must not already exist. It contains no required
authoring files and can be passed directly to `--scenario`; its ordinary streaming
policy is unchanged. Keep that exported content with a save that still needs its
unvisited authored inputs. The export is a content bundle, not a saved game or a
promise to support historical generator implementations.

Package metadata embedded in the initial save is bounded to 960 KiB, preserving
headroom in the existing 1 MiB journal-frame limit. Region indexes are chunked
separately. The compiler reports over-capacity metadata before publishing a
certificate; large catalog source files are not a guarantee their compiled form
fits a runnable saved package.

A creature recipe may declare `future_growth = { kind = "selected", source =
{ kind = "racial" } }`, or use a class source such as `{ kind = "class", class =
"warrior" }`. The other policies are `{ kind = "disabled" }` and `{ kind =
"authored" }`. Omitted authoring defaults to disabled; persisted recipes require
the policy explicitly. Controller identity never selects a future source.
NPCs grow only when their recipe explicitly enables a future source. Invalid
class references reject package compilation. Changing the selected future source
does not replace an authored lost-HD restoration source or choices.

Actor records keep species/template IDs, owned advancement choices and mutable
state. Restore resolves those IDs against the pinned catalog and rebuilds derived
values; checkpoints and detached records do not repeat immutable definitions.
Restoration validates modifiers, effects, attack descriptors, actor clocks and
ownership before publishing a restored game. Procedural publication, terrain
mutations, knowledge, timers and active/frozen time remain ordinary journaled
simulation state. Revisiting a generated region never rerolls it.

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

The default dungeon declares a four-HD human Warrior with explicit starting ranks,
shared-pool purchases and attribute advancement. Its paid choices are Rattling
Blow, Brace for Impact, another Heavy Weaponry rank and Measured Cut; its starting
ranks are Heavy Weaponry and Discipline.
Its iron greatsword is a separate equipped item; the species' natural attack is
an Impact strike. The scout uses a Light Weaponry racial build, the guardian
uses a two-HD Construct build with source-granted Impact protection, and the
wisp uses a Fire Elemental build with no equipment slots. Their health, skills,
defenses, protection and phases derive through the same creature rules used by
the arena. These actors have no separate flat combat path.


The manifest declares `factions` as faction names mapped to hostile faction names,
and `ai_profiles` as names mapped to required `flee_percent`, `care`, `equipment`
and `loot` settings. Tactics are explicit booleans; inventory alone never enables
them. Every species requires `behavior = { disposition = "passive" | "mean",
idle = "wait" | "explore", memory_ticks = ... }`. This species-owned memory policy
is independent of the current controller. See
[creature behavior](creature-rules.md#creature-behavior-and-provocation).
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
