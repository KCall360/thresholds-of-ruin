# Retained spatial registry

The spatial registry is implemented simulation/save state. It supplies canonical
room, passage, doorway, illumination and mapping membership without replacing
World's physical geometry. This page owns the current architecture;
[scenario packages](../reference/scenario-packages.md#canonical-room-and-passage-metadata)
owns authored syntax, and the [reconstruction plan](../plans/rogue-spatial-registry-reconstruction.md)
retains historical design/capacity reasoning rather than another current contract.

## Separate authorities

| Owner | Responsibility | Not authority for |
| --- | --- | --- |
| `World` | Physical neighbors, walls, openings, body placement and portal transforms | Inferring authored room identity from a storage region |
| Server canonical compiler | Source-complete room volumes, maximal physical passage components, doorway provenance and exact illumination/mapping certification | Mutable actor decisions or a second portal graph |
| Simulation registry | Immutable classifications and exact scope-local lookup | Activating every member, advancing clocks or discovering hidden cells |
| Region lifecycle | Loaded/active/frozen/detached state and action dependencies | Redefining immutable room/passage membership |
| Population `PreparedScope` | Population preparation/entry identity | A substitute for spatial scope or room geometry |
| Observation/chart projection | Authorized disclosed results | Raw registry dumps or ordinary targets from historical map samples |

Metadata registration does not itself grant residency, take an AI turn or consume
actor-active time. Physical links and abstract stairs remain different: abstract
transport is not evidence that two passage components are physically connected.
Closing a door changes current traversal/opacity, not immutable classification.

## Representation and membership

A `CellRun` stores a region, exact z/y and inclusive first/last x. Checked wide
arithmetic computes its length. Sorted canonical runs compress cells without
flattening height. Primary runs carry full room/passage membership and optional
doorway affiliation; adjacent runs merge only when that full membership agrees.
Room and passage IDs are positive, separate scope-local namespaces. Different
scopes may reuse local IDs, but a region has at most one primary spatial owner.

Doorways remain room-primary with a passage affiliation. Passage-primary maze
cells join their physical component. Illumination has separate per-room surface
runs: surfaces can be shared by different rooms but may not overlap within a
room's canonical set. A solid face across a real physical link can be a
surface-only dependency in another region. That does not grant primary membership
in the foreign region or permission to activate its actors.

Missing scopes/cells return absence, not a room guessed from region ID, line of
sight or connected open space. The pure `Membership::includes_drain_target`
predicate is deliberately directional: same primary room/passage matches;
a doorway caster includes its affiliated passage; a passage caster includes
its affiliated doorway targets. Two doorway cells in different rooms do not
become the same room merely because they share passage affiliation.

Source: [classifications and predicates](../../crates/simulation/src/spatial_scope.rs),
[registry primitives](../../crates/simulation/src/spatial_scope/registry.rs).

## Canonical compilation and lazy installation

The server decodes optional authored `spatial_scopes` and canonicalizes bounded
source volumes/rows. World physical adjacency certifies full room volumes,
maximal passage components, doorway ownership and six-face illumination. It
rejects gaps, conflicting owners, wrong affiliations and noncanonical geometry.
Authors do not supply passage IDs, precomputed illumination or another transform
graph. Generated scopes come from the actual retained generation recipe/result,
including omitted rooms and maze cells, not inferred rectangles.

Reachable Room Light capability requires valid canonical metadata for authored
regions. The enabled-source traversal includes reachable items/grants and future
valid acquisition paths; merely importing an unused definition is not activation.
Source expansion and terrain work are bounded separately from encoded retention.
The package source guide owns exact rules and limits.

An authored scope installs once one of its primary members is realized. Its other
members need not become active. Effects can request load-only physical/surface
dependencies when necessary, then complete under the normal candidate transaction.
Missing required geometry is an error, not a partial successful effect or an
invented passage no-op.

Source: [source adapters](../../crates/server/src/scenario_spatial_authoring.rs),
[canonical compiler](../../crates/server/src/scenario_spatial.rs),
[installation and audit](../../crates/server/src/scenario_spatial_runtime.rs),
[enabled-source analysis](../../crates/server/src/scenario_spatial_sources.rs),
[streaming](streaming.md).

## Gameplay consumers do not redefine the registry

- **Room Light** selects the caster's primary room, proves complete physical
  residency/geometry, then illuminates exact registered volume/surfaces. A
  passage/maze has no room runs and produces the ordinary paid no-op; unknown
  geometry is not equivalent to that valid no-op. An already lit room reports no
  changed cells. Foreign surfaces and distant same-room members remain distinct
  residency cases.
- **Drain Life** uses exact scope membership, not sight or light. It requires all
  structural members loaded before its eligible living-actor census, excludes
  self from that census, and computes the distribution before death/growth side
  effects. Its Health and payment rules belong to the item/effect reference.
- **Mapping** uses retained source certification and its bounded chart contract;
  remembered/historical samples do not become ordinary sight or action authority.

Source: [Room Light](../../crates/simulation/src/spatial_scope/room_light.rs),
[Drain Life](../../crates/simulation/src/spatial_scope/drain_life.rs),
[mapping](../../crates/simulation/src/spatial_scope/mapping.rs),
[items](../reference/items.md), [charts](../reference/protocol.md#detection-and-mapping-charts).

## Retention and restore are separate validation layers

1. Structural validation checks canonical runs, IDs, ownership, arithmetic and
   caller budgets. It does not prove those values describe the authored world.
2. The strict codec uses one shared nested meter before append/allocation, rejects
   noncanonical/duplicate/unknown fields and counts pooled payloads plus directory
   and root references. Required nullable data is distinct from omission.
3. `Game::register_spatial_scope` validates a candidate registry and checks that
   its single root can encode before publication. This does not prove a complete
   game plus every retained historical root fits the archive.
4. Checkpoint retention pools immutable scopes/directories. It binds the actual
   current and retained historical root set; decoded pools cannot serialize as
   though an unbound/poisoned root set were already validated. Canonical comparison
   prepares only the actual bounded serialization witness; restore independently
   checks its own roots against the original meter.
5. Server `SpatialAudit` checks each restored boundary against canonical source
   and that boundary's realized members. Expected-set equality rejects whole-scope
   omissions as well as corrupt extra payloads; checking only the current game
   is insufficient. Canonical recompilation can be shared across checks without
   sharing one boundary's expected membership with another.

Current checkpoint limits include at most 257 game roots (current plus up to 256
retained transaction/gameplay boundaries). Codec reference/work limits and the
whole archive byte ceiling are separate constraints. Historical-only payloads
still count. Registration idempotence does not waive a newly tighter budget.

Already retained canonical source supports registry verification without
re-importing original authoring files. This is not a blanket claim that every
uncopied, unvisited authored input in every save can be reconstructed after its
immutable compiled bundle is removed; see
[package source ownership](../reference/scenario-packages.md) and
[persistence](persistence.md).

Source: [retention](../../crates/simulation/src/spatial_scope/retention.rs),
[strict codec](../../crates/simulation/src/spatial_scope/codec.rs),
[simulation checkpoints](../../crates/simulation/src/checkpoint.rs),
[server checkpoints](../../crates/server/src/checkpoint.rs),
[`SpatialAudit`](../../crates/server/src/scenario_spatial_runtime.rs).

## Capacity and evidence boundaries

Do not restore the rejected 65,536 combined-reference budget or impose a
64-member cap on authored scopes. The 64-member bound belongs to generated grids;
authored packages have their own larger admitted envelope. Source work,
expanded-reference arithmetic, compact run counts, pooled encoded charge,
whole-save bytes and runtime residency are different limits. Neither a geometry
maximum fixture nor compact registry sharing proves whole-game capacity or peak
memory use.

Existing witnesses cover maximum generated geometry, a modest source-valid
65-member authored scope, foreign solid faces, distant same-room load-only work,
strict pooled retention, malformed historical roots and compressed checkpoint
reopen. Their owning files include
[maximal generated geometry](../../crates/server/src/maximal_generated_scope_geometry_tests.rs),
[compiler tests](../../crates/server/src/scenario_spatial_tests.rs),
[runtime/source audit tests](../../crates/server/src/scenario_spatial_runtime_tests.rs)
and [retention tests](../../crates/simulation/src/spatial_scope/retention.rs).
This source map is not a fresh execution receipt or performance qualification.
