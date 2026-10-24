# Retained spatial registry reconstruction

This is a reviewed implementation proposal, not implemented gameplay or a new
source-format approval. It preserves design work after the 2026-10-24 executor
reset. All committed device foundation source and fixtures were recovered from
backup commit `1189c83c11baf40f490e5aa3886dbb4ce252dd61`, tree
`bf63bcf8d9940be04ef42d88955aa98766e94066`. Toolchains, binaries, raw local receipts
and ignored drafts were not recovered; later qualification needs fresh execution.
The earlier documented test outcomes remain historical observations, not reusable
artifact receipts. This document reconstructs decisions, not lost draft bytes.

## Authority and representation

- Simulation owns an immutable shared spatial registry, independent of population
  `PreparedScope` and region slices. World remains the sole physical-neighbor and
  portal-transform authority; do not save a second transform/connectivity graph.
- A proposed `CellRun` contains region, z, y and inclusive first/last x. Compute
  length with checked wide subtraction/addition. Sort by region/z/y/x. Primary
  runs are disjoint; merge adjacent runs only with identical complete membership,
  including doorway affiliation. Room and passage IDs have distinct scope-local
  namespaces. Exact z membership supports levitation, not a floor clamp.
- Room illumination has separate maximal, disjoint runs per room. Surfaces may
  overlap between rooms. Include actual room volume and applicable shell faces;
  exclude open corridor mouths and diagonals. Do not derive lighting from only
  walkable floor cells. Current generated geometry uses real six-face surfaces.
- Maze `F_PASS` cells are passage-primary and join their physical component.
  Doorways remain room-primary with passage affiliation; Drain uses the existing
  asymmetric predicate. Unknown scopes/cells return absence, never region/LOS
  approximation. Mutable door closure does not repartition immutable topology.
- Metadata alone creates no residency, population entry, AI turn, clock or RNG
  effect. Abstract stairs do not connect passage components.

## Canonical validation and retention

Simulation structural validation is not geometry certification. Server compilation
uses retained authored/generated sources, seed and World physical adjacency to
verify complete volumes, maximal connected passages, exact doorway rows and room
illumination. Reject split connected passages, omitted/wrong affiliations,
omitted/extended volumes/surfaces and alias scopes. Check current, detached and
every distinct historical root; source-free reopen uses retained package authority.

Capability availability validation must traverse enabled/instantiated items,
carried/kit/population sources and reachable grants/conditions/equipment, including
future valid declared branches. Unused imported catalog definitions alone must not
force topology. No population gate or silent effect-specific identity oracle.

## Capacity evidence and unresolved constants

The rejected 65,536 combined-reference proposal cannot represent existing maximum
valid generated scopes. The tracked
`maximal_generated_scope_has_exact_room_volume_and_physical_illumination` test
constructs a real 8 by 8 generated package with 32 by 32 by 2 members and 30 by 30
rooms. Its independent geometry oracle expects 115,200 room-volume cells and
245,760 illumination references minus twice the actual open mouths. Package,
materialization and Engine construction are positive prerequisites. It is not a
whole-floor load, registry, source-free or archive-capacity proof.

Conservative design estimates, **not finalized admission constants**:

| Geometry | Primary/reference or run estimate |
| --- | --- |
| 64 generated 32 by 32 by 2 members | at most 131,072 primary cells |
| One closed 30 by 30 by 2 room | 3,840 illumination points, ideally 124 x-runs |
| 64 authored 32 by 32 by 8 members | at most 524,288 primary cells |
| One full 32 by 32 by 8 authored room | 11,264 illumination points, 336 surface runs plus 256 primary runs |
| 64 such full authored rooms | 1,245,184 combined references, 37,888 runs |
| Loose primary plus self-and-six-face work | 4,194,304 combined references |

Actual open mouths can split runs. Fragmentation requires bounded work and encoded
size accounting, not silent truncation. Do not present these estimates as proof
that a complete game archive fits its unchanged 64 MiB limit.

A shared decoder budget must cover nested scopes/rooms/runs before append or
allocation, with checked lengths/counts, escaped labels and closed-schema framing;
never trust `size_hint`. Strict named maps reject positional, missing, duplicate
and unknown fields; required-nullable doorway is distinct from omission. Positive
IDs and closed variants remain strict. Exact serializer-based maximum-width charge
proofs and final run/root/byte constants remain required before schema promotion.
Shared immutable payloads are pooled once, but every directory/root reference and
distinct historical-only payload counts. Reject conflicting aliases and invalid
indices. The whole archive also contains unrelated game state.

## First-red and integration sequence

1. Minimal strict structural types and canonical run tests: exact/max-plus-one
   budgets, overlap/unmerged/dangling rejection, different doorway membership not
   merged, per-room overlap rejection and cross-room shared-surface positive.
2. Atomic registry registration: idempotence, conflict/region-alias rollback,
   repeated local IDs in disjoint scopes, exact z lookup and query purity.
3. Independent small expanded-cell oracle plus real maximum generated/authored
   source fixtures; prove decoder allocation/work boundaries before integration.
4. Canonical authored/generated compilation and malformed restore tests across
   current, frozen/detached and historical roots. No duplicate mutable authority.
5. Integrate actual Room Light/Drain and their residency/payment/disclosure tests;
   preserve original doorway asymmetry and paid passage-Light no-op.

Fresh source-first regressions, scoped full suites and remote checkpoint backups
are required at each coherent stage. No unimplemented proposal receives test credit.

## Initial trusted-input primitive checkpoint

The first API checkpoint deliberately has no codec, Game field or default global
limit. `spatial_scope::registry` exposes constructible `CellRun`, `PrimaryRun`,
`RoomSurfaces` and `Scope` values for trusted compiler inputs. A caller supplies
`Budget { scopes, members, rooms, runs, expanded_refs, label_bytes }`; checked
`Usage` counts each surface owner's references, including shared geometry twice.
`Scope::validate` checks structure and usage. `Registry::register` must validate
whole-registry aggregate usage before mutation, reject conflicting IDs or region
aliases, and preserve state on failure. Identical registration is idempotent but
must still pass a newly supplied tighter aggregate budget. Exact scope/region/z
lookup returns absence outside registered membership. This caller budget is not a
future production admission override or an allocation bound for existing vectors.

The source-first fixture has three volume runs and four surface runs, totaling
seven runs and 21 expanded references. Exact limits succeed; six runs or 20
references fail. Independent controls include the full signed-coordinate interval
of 2^32 cells, shared surfaces across distinct rooms versus same-room overlap,
adjacent unequal doorway membership, structurally valid registration conflicts,
disjoint scopes reusing local room IDs, and aggregate failure rollback. The initial
scaffold deliberately lacks structural rejection and registry insertion; it does
not establish implemented gameplay. Strict named decoding with a shared seed,
bounded pre-allocation charging, archive byte limits and pooled historical roots
remain separate subsequent checkpoints, not claims from typed Vec validation.

## Authored source proposal and compiler ownership

The recovered `RegionDef` has chamber, wall, opening, portal and door data, but
no authored room/passage classification. Neither a region nor a connected open
component is evidence of a room. The next compiler checkpoint therefore uses an
explicit **internal, nonserialized** geometry input and generated recipe sources;
it does not add public authoring fields or integrate Room Light.

For a subsequent reviewed authoring change, the minimal proposed optional package
metadata is a named `spatial_scopes` collection. Each entry would contain:

- `members`: the exact region IDs in this immutable scope, independent of any
  population `PreparedScope`. A region belongs to at most one spatial scope.
- `rooms`: positive scope-local room IDs and full three-dimensional volumes.
  Each volume names its region and inclusive `min`/`max` coordinates. A bounded
  union can represent a nonrectangular room; compilation canonicalizes the union
  and rejects conflicting ownership, rather than inferring it from floor cells.
- `passages`: passage-primary row runs, each naming region, z, y and inclusive
  first/last x. Authored passage-component IDs are unnecessary: the compiler
  derives maximal components using World physical adjacency over passage-primary
  cells **and explicit doorway cells**, without bridging through ordinary room
  interiors.
- `doorways`: room-owned row runs, each naming its room ID and locations. They
  must lie in that room's primary volume. They participate in passage-component
  connectivity while retaining room-first primary lookup. A legitimate doorway
  junction can join several passage arms; derive its one affiliation only after
  that component union, not by rejecting its initially separate neighbors. The
  affiliation is never an unverified label or an edge through an abstract stair.

These are proposed fields, not an approved serialized schema. Exact collection
syntax, names, limits and strict source/save mirrors must be reviewed before
promotion. Scope labels are source identities, not public target handles; any
compiled ID assignment must be deterministic and source-verifiable. Generated
scopes instead use retained generation identity/recipe metadata, including final
maze classification; their population room groups alone are insufficient.

The server compiler owns source completeness and geometry certification: validate
region membership and physical bounds, complete primary volumes, disjoint room
and passage ownership, doorway provenance, maximal physical passage components,
and the exact room illumination surface rule. It derives illumination from room
volume and physical open/wall geometry, excluding open corridor mouths and
outside diagonals. It does not save a second portal transform graph, change
mutable doors, activate a floor or enter population. Bounds must cover input
volume expansion and adjacency work as well as compact output, before allocation
or append; output compression is not a substitute for a work bound.

Simulation owns strict compact representation and atomic registry insertion.
Server source-free validation reconstructs canonical classifications from the
retained package and generation provenance, comparing every distinct current,
detached and historical payload rather than trusting saved labels. Capability
closure will require applicable metadata only for genuinely enabled/reachable
spatial effects; absence remains legal for unrelated packages and unused imported
catalog definitions. No compatibility reader or additional format version is
proposed beyond the existing unpublished protocol61/save59 batch.

## Reconstruction checkpoint status

The optional authored metadata direction above was confirmed within the already
approved feature/current-only format scope. Exact canonical coverage, World-owned
connectivity/illumination, bounded strict decoding and all-root source-free checks
remain mandatory; metadata absence stays valid when relevant effects are not
reachable. Public authoring fields are still not implemented here.

The first trusted-input registry and internal/generated compiler scaffolds are
tracked with independent positive fixtures and intended missing-implementation
regressions. Freshly reinstalled tooling passes workspace all-target compilation
on this source after canonical formatting. Runtime first-red collection has not
yet run at this checkpoint. The registry has explicit caller budgets only, no
serialization or Game integration; compiler stubs return an explicit unsupported
error. These scaffolds do not qualify completed geometry or gameplay behavior.
