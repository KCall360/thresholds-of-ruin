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

Conservative design estimates, **not finalized admission constants or an
authored-scope maximum**:

| Geometry | Primary/reference or run estimate |
| --- | --- |
| 64 generated 32 by 32 by 2 members | at most 131,072 primary cells |
| One closed 30 by 30 by 2 room | 3,840 illumination points, ideally 124 x-runs |
| 64 authored 32 by 32 by 8 members | at most 524,288 primary cells |
| One full 32 by 32 by 8 authored room | 11,264 illumination points, 336 surface runs plus 256 primary runs |
| 64 such full authored rooms | 1,245,184 combined references, 37,888 runs |
| Loose primary plus self-and-six-face work | 4,194,304 combined references |

The generated grid limit is 64 members, but authored packages permit up to
65,536 regions, each at most 32 by 32 by 8, subject to existing aggregate
validation-work and source limits. The checked package route caps repeated
cell-work at 67,108,864; this is not a per-scope reference allowance. Population
PreparedScope limits are independent. Retain that authored admission envelope;
use a modest source-valid scope with more than 64 members as a witness rather
than materializing an impractical maximum world or imposing an artificial
64-member spatial cap. Derive production budgets from existing validated work
and the actual encoded representation.

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
`Budget { scopes, members, rooms, passages, runs, expanded_refs, label_bytes }`; checked
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
  may lie outside that room's interior/illumination volume, but must have
  validated room ownership. Keep this provenance separate from the interior;
  never substitute an inward neighbor. They participate in passage-component
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
interior volume and physical open/wall geometry, excluding open corridor mouths
and outside diagonals. Actual exterior closure faces are included for their
source-owned room without growing a second shell around the doorway. Doorway
ownership uses the agreed planar mouth semantics; physical components and
illumination use World face neighbors, including vertical faces. It does not save a second portal transform graph, change
mutable doors, activate a floor or enter population. Bounds must cover input
volume expansion and adjacency work as well as compact output, before allocation
or append; output compression is not a substitute for a work bound. Explicit
compiler work limits count source entries and scanned terrain separately from
exact output references. Repeated source doorway/closure entries are not extra
output membership. No production default limits are selected here.

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

The first frozen trusted-input cohort compiled successfully. Four spatial controls
passed and six registry tests reached their intended missing-enforcement or
missing-insertion failures; three appearance controls also passed. Two server
fixtures reached the unsupported compiler/source stubs. The third junction
fixture had an inverted positive prerequisite (asserting a deliberately open cell
was blocked); that is fixture failure, not behavioral evidence. Its correction
keeps all three open-cell and physical-neighbor prerequisites. These focused
receipts are not verification gates or geometry integration qualification.

The revised source-first cohort established all intended boundaries: four core
controls passed and nine registry tests failed at missing enforcement, insertion
or derived passage counts. All eight server tests reached the unsupported
compiler/source stubs after their actual World and generated-package
prerequisites succeeded, including the corrected junction, selected maze and
chance-zero/full secret-door contrasts. The standalone World prerequisite probe
also passed; it did not qualify generated compilation.

The reviewed green implementation passes the fresh full simulation library
(1,783 tests) and server library (1,033 tests), with no ignored or filtered tests
in those runs. Workspace all-target Clippy with warnings denied and 19
documentation tests passed after canonical formatting and before those runtimes.
These are focused development receipts, not quick/push/full or native gates.
Implemented boundaries are structural registry checks/atomic insertion,
explicit-World compilation, and a bounded generated-source projection retaining
labels from the same generation pass. The server compiler and ephemeral
generation provenance are currently test-only because there is no runtime
consumer yet; this does not install or retain spatial topology in a game. No
codec, Game integration, generated-World compilation or source-free canonical
restore claim follows from this checkpoint. Production integration must promote
the same authority with actual lifecycle witnesses, not retain test-only behavior
as completed gameplay.

With ample measured executor headroom, repeated development feedback may use
the supported incremental mode with one owner/job and unchanged resource guards.
Receipts distinguish it from the preceding non-incremental cohorts; final
canonical gate configuration is unchanged. Interrupted caches and obsolete
artifacts never qualify source, and cleanup still needs its applicable authority.

## Derived passage namespace and source provenance

A passage component need not contain a passage-primary cell. Two maximum-sized
neighboring generated rooms can own both cells of their intervening corridor as
exterior doorways; a valid authored component can also contain one cell. Do not
invent a degree minimum or a passage-primary representative. Count distinct IDs
from primary passage and doorway references together. There is no duplicate
passage-ID directory: no consumer needs one, and self-consistent declarations
would not prove canonical connectivity. Count the same local ID in separate
scopes separately in aggregate usage. Structural validity does not certify a
partition; immutable-source reconstruction does.

Retain ordinary room ownership when generated mouths are discovered, before a
coordinate set discards their owners. Reject competing ownership rather than
choosing by distance or iteration order. Closure-placement chance, maximum,
reserved-cell and narrow-mouth filters must not silently become the immutable
classification rule. Preserve existing placement order and RNG. Actual maze
slots remain passage-primary even though population metadata groups their carved
floor cells under room IDs. Reserved/wide-mouth and omitted-slot coverage remain
explicit additional acceptance work, not implied by the narrow generated tests.

## Retained registry integration seam (not implemented)

Reuse the existing checkpoint shared-payload/directory pattern: pool immutable
scope payloads, then reference them from immutable registry directories and
snapshot roots. Pooling only whole registries would duplicate unchanged floors
when later floors register. Detached region records must not own another mutable
topology copy. One restore context validates indices and preserves sharing;
server canonical verification covers every distinct current/historical registry
root, not just the current game. A shared decoder budget must charge nested
runs/labels and every directory/root reference before allocation. Per-list bounds
alone are insufficient. Keep the outer 64 MiB archive limit unchanged; final
encoded-byte, run and root limits still require measured proof.


## Next production integration and strict codec contracts

The next codec should be closed and named, using existing named-map helpers and
budgeted seeds rather than a general serialization framework. `CellRun` names
region/z/y/first_x/last_x; membership uses a closed primary kind/positive ID and
required nullable doorway. Scope contains id/members/primary/rooms; passage IDs
remain derived. Reject positional, missing, duplicate and unknown fields at every
layer. World RegionId zero remains structurally legal even where authored
package IDs have stricter rules.

Proposed checkpoint ownership is one `SharedState.spatial` pool containing scope
payloads and registry directories, plus a required `Snapshot.spatial` directory
index. Directories are ordered by scope identity, not allocation order. Reject
invalid references, duplicate/alias identities, overlapping member ownership and
unreferenced surplus pool entries. Reuse one RestoreContext for current and all
historical snapshots. Equal immutable payloads are pooled once; every root and
directory binding still counts. Exact field promotion remains coordinated with
current-format writers and genuine fixtures.

Thread one aggregate decode budget through nested seeds. Charge entry minima
before nested allocation, run widths after their fixed scalar record but before
retaining it, and labels before copying them into retained strings. Do not use
untrusted size hints, parse a discarded oversized tail, or build a serde Value
first. A JSON tokenizer may already allocate an escaped scalar before its visitor;
the existing 64 MiB raw/decompressed bound remains relevant, so do not claim seeds
eliminate that parser allocation. Serializer controls must cover maximum-width
operands, escaped labels, empty/multiple entries and all framing. Root-reference
charging must reuse the independently bounded history reader or a proved fixed
reserve; post-hoc validation is not pre-allocation protection.

For actual World compilation, reuse existing Package region and portal parsing
through one bounded ephemeral join descriptor consumed by both ordinary Game
construction and a temporary source World. Apply walls then openings and retain
exact anchor/rotation diagnostics. Do not expose mutable Game.world, open runtime
doors to obtain a baseline, persist another portal graph, or include abstract
stairs as physical component edges. The current internal compiler still needs a
source-first exterior-mouth ambiguity guard: multiple neighboring cells of the
same room are valid, but neighboring interiors of different rooms must reject
regardless of supplied owner. Generated provenance already rejects competing
owners. Add clear diagnostics and authored/source-free contrasts before public
schema integration; current eight fixtures do not qualify that edge.

Install a generated scope atomically with its prepared region records, independent
of optional population activation, before candidate publication. Authored
installation policy remains explicit: prefer full scope metadata on the first
prepared member without loading, activating or pinning its peers, then prove the
initial/frozen/detached/history behavior. Enabled-source validation separately
requires metadata availability. Do not silently infer scopes from zones or
physical connectedness.

Canonical restore must compare the independently expected scope set as well as
each payload. Per-root loaded (active/frozen) plus detached member records are the
candidate existing preparation witness; unbuilt declarations, population scopes,
the registry under audit and archive-global caches are not substitutes. Prove
that witness through initial construction, eviction and historical restore before
relying on it. Earlier roots must not require scopes prepared only later. Reject
whole-scope omission from historical-only and detached roots, not only malformed
rows. Any missing lifecycle evidence must be resolved explicitly rather than
adding parallel mutable history.

Acceptance includes real Engine/Service admission, population-free contrast,
source-deleted reopen using retained package authority, structural-but-canonical
corruption in current/detached/history roots, exact retry and late-store rollback.
Keep request IDs monotonic across reconnects. A source-free codec roundtrip alone
is not the deleted-scenario Service witness. Production source integration,
rotated portals, nonmonotonic generator member order, reserved/wide mouths,
omitted rooms, actual encoded-capacity proof and Room Light remain unfinished.


### Reviewed next codec experiment (not production admission)

Stage a standalone strict pooled codec before Game/Snapshot installation. Candidate
maximum-width JSON charges are 107 bytes per surface run, 195 per primary run,
21 per member, 30 plus surface charges per room, and 47 plus six times decoded
label bytes plus members/primary/rooms per scope. A directory charges 3 plus
11 per u32 payload binding; the pool header charges 30 and each snapshot root
reserves 21. These are conservative closed-schema calculations, not yet Rust
serializer proofs or accepted constants. Prepaid nested leaves must not be charged
twice. Use checked arithmetic and one shared allowance, not independent per-list
limits; retain the existing whole-archive 64 MiB ceiling rather than adding a
second allowance. No authored 64-member cap follows from these calculations.

The first matrix must compare actual maximum-width/escaped serialization, admit
more than 64 members, reject positional/missing/duplicate/unknown fields and
unread excess tails, preserve required-nullable doorway fields, and prove pooled
payload identity plus directory/index/reference integrity. Decode must not expand
hostile coordinate intervals into cells. Canonical comparison still needs the
existing source-work bounds. A proposed 257-root reserve is valid only after the
history reader enforces its 256 retained-boundary limit before allocation; its
current post-decode check is not such protection. Actual encoded fixtures,
whole-game headroom and bounded history integration remain prerequisites.

### Exterior-owner regression cohort

The next two test-only compiler witnesses retain a valid U-shaped room with three
adjacent cells of one owner, then split it into two ordinary rooms. The split
geometry first compiles with a passage-primary mouth. Claiming that exterior
mouth for either adjoining room must fail with an explicit ambiguous-owner error;
a true interior marker retains its existing rule. This isolates physical-owner
ambiguity from invalid geometry and preserves World equality. The frozen first-red ran all ten compiler tests: nine controls passed and the
ambiguity negative failed because the compiler accepted the supplied matching
owner despite a second physical owner. The narrow correction now checks unique
planar ownership; all ten compiler tests now pass. This remains a test-only
compiler, not installed production source validation.


### Standalone codec first-red implementation checkpoint

The next source cohort adds the reviewed standalone `PoolSeed`/`EncodedPool`
scaffold and three tests, without Game fields or save-format migration. Private
`DecodedPool` must be consumed by root validation before producing private
`ValidatedPool`; only the latter may be encoded. Indices are explicit u32 with
checked future capture conversion. Caller limits retain the actual root reserve,
independent directory bound and shared charged-byte allowance.

The initial independent serialization skeleton test is an arithmetic control.
A structurally valid 65-member typed envelope establishes the intended missing
codec failure, not a canonical authored-geometry witness. Top-level malformed
cases are anchored by the same successful-decode prerequisite. Encoding, decoding
and root validation are deliberate error stubs until this first-red is observed;
no bounded nested decoder or persistence acceptance is implied. Subsequent
max-width actual-writer, nested-shape, allocation-tail, exact aggregate, sharing
and source-free/current-history tests remain required.


### Standalone codec green implementation cohort

The initial codec first-red produced one passing arithmetic control and two
intended valid-decode stub failures. The next implementation replaces those stubs
with strict named streaming seeds, a single checked precharge meter, borrowed
serialization adapters and consuming root validation. Expanded witnesses cover
actual writer bounds, nested required/unknown/duplicate/positional fields,
required-nullable doorway and numeric domains, exact charge versus one byte less,
shared historical subsets and rejecting oversized tails before reading them.
These expanded tests now pass in the fresh 1,792-test simulation library. They
do not inherit a strictness pass from the initial missing-decoder failures.

This pool explicitly represents immutable package-and-seed geometry: each scope
identity and region ownership is assigned once, and historical roots select
subsets rather than replacing or reassigning payloads. Global identity/member
uniqueness is checked once before directory index/order/reference completeness,
avoiding repeated full member scans for every history directory. A valid shared
subset and a conflicting payload used only by another historical root are
separate acceptance cases. This is not an assertion that the generic per-root
registry previously enforced a global history invariant.

The grouped server witness constructs and validates a modest 65-member authored
package under the existing admission envelope, then compiles complete typed
spatial volumes using the same real region structures. Its independent expected
counts are 1,170 primary and 3,900 surface references, 1,430 runs, and 6,500 scanned
terrain cells. It is not a public spatial metadata schema, loaded Game registry,
maximum-world/archive-capacity proof or source-free restore witness. Those
production integration boundaries remain next work after standalone codec green.

The first green preflight caught three mechanical lint issues before linking:
manual saturating arithmetic and two visitor implementations nested below their
type scope. The equivalent idiomatic correction passes strict workspace
all-target Clippy. At source `d80a11975fb73dadfda113686bce70480064bef5`,
fresh full simulation and server libraries pass 1,792 and
1,037 tests respectively, with none ignored or filtered. The real 65-member
authored witness is included. These are focused development receipts, not
quick/push/full/native verification gates; Game/save installation is next.

The next checkpoint integration should reuse infallible trusted capture of shared
references and fallible checked serialization, rather than migrate every capture
caller to a new transaction API. Construct validated encoding authority only after
capacity/conflict checks; internal usize indices require checked u32 wire
conversion. Since JSON field order is arbitrary, shared-pool decoding may need an
explicit internal deferred-root mode. It must retain the same meter, charge actual
root references before copying them or returning validated authority, and consume
the candidate once. Keep the public exact-reserve standalone route unchanged.
Bound history before appending/reading the excess subtree, collect all current and
retained root indices before restore publication, and reject nonempty restored
registries until canonical source/expected-set verification is installed. The
final source verifier must also reject an empty registry that omits required
scopes; a temporary nonempty rejection is not that final verifier.


### Game/history retention first-red checkpoint (runtime pending)

The next source cohort introduces only the minimal shared Game registry and
registration/binding test seams, with three retention witnesses. The current
checkpoint schema deliberately remains unchanged in this first-red: native
installation, encoded pooled retention and the 257/258 archive envelope are not
yet implemented. Four server witnesses independently establish genuine captured
checkpoint prerequisites, the decode-only 256-boundary envelope, rejection before
an oversized 257th subtree, and the closed nonempty-registry audit seam. These
are missing-behavior tests, not valid-history or source-canonical acceptance.
The standalone runtime was not run. After the maintainer requested fewer testing
and recompilation cycles, this scaffold joins one coherent Room Light feature
batch rather than requiring a separate retention-only link. Strict all-target
Clippy passed after an equivalent comparison cleanup; that is not runtime proof
of these retention cases.

### Combined Room Light implementation checkpoint

Complete shared Game/save retention, bounded history/root binding, canonical
source installation and all-root source-free verification together with the paid
Room Light effect. The production verifier must replace the interim closed
nonempty-registry guard in this batch. Authored/generated spatial metadata stays
independent of population, and real World topology remains the physical
adjacency authority.

The user-visible acceptance is an ordinary paid Light device illuminating the
actual room volume and surfaces, including a nonresident portion of the same
room, with exact payment and source-free persistence. Passage/maze no-op,
true-height membership, missing-region staging, malformed history and canonical
source mismatches remain distinct assertions in the combined matrix. Validate
fixture prerequisites together, collect the intended missing-effect failure at
the paid Room Light seam, then run focused green tests from the same owning
binaries. The unexecuted retention scaffold must not be reported as an observed
first-red result.

Batch related implementation before compilation, reuse qualified binaries, and
reserve broad suites for meaningful completed behavior checkpoints and required
final gates. This changes cadence, not correctness or gate requirements. Back up
coherent source and recovery notes to the authorized PR-free remote branch
without requiring an expensive build for each backup.


The first combined source checkpoint promotes the reviewed retention wiring and
strict Room Light adapters without another standalone build. It includes staged
one-root archive admission, shared structural limits derived from the unchanged
codec ceiling, bounded history decoding and all-root binding. One-root admission
does not prove full history or whole-game capacity; final serialization remains
checked and failures remain transactional. Source-side geometry work limits are
separate from arithmetic expanded-reference safety bounds.

The Room Light application seam deliberately remains a no-op while its paid-room
and passage/already-lit controls are staged. Canonical production installation,
all-root source verification and imported Light acceptance are still being
completed. These newly promoted tests have not been compiled or run; the last
successful library results above do not qualify this new source. The interim
server nonempty-registry restore guard is not the final canonical verifier.


This backup also retains the in-progress optional authored `spatial_scopes`
source type and production generated provenance. Its saved source mirror,
installation and canonical restore verifier are not yet wired. Consequently
this is explicitly an uncompiled work-in-progress backup, not a compile-ready
or playable Room Light checkpoint. New pure catalog assertions are staged before
the opt-in content definition; no live pool changes are included.

The consolidated next source backup includes production authored/generated
installation, seed-resolved one-hop physical neighbor geometry, enabled-source
metadata admission and all-root canonical restore checks, plus actual paid
Room Light Service fixtures. These inputs are frozen for grouped preflight;
no new compilation or runtime result is claimed. Room illumination itself still
remains the deliberate missing-effect seam. The pure Light catalog test also
precedes its content definition.

One explicit authored-geometry issue remains unresolved: a real boundary link
can expose a solid illumination face stored in another scope's region. Canonical
World geometry must not silently lose that face, but exclusive primary-region
membership must not be inferred merely from a surface dependency. The current
representation rejects that case; resolving it is required before claiming
general cross-scope support. This backup protects the exact incomplete state.


The foreign-face disposition is now implemented in the staged source: existing
surface runs encode bounded dependencies independently of exclusive primary
members, without a new serialized field. Canonical source checks certify the
actual solid faces; structural registry and codec checks alone do not. New
positive/negative and managed no-wake fixtures are present but unrun. The opt-in
Light content is also promoted after the pure missing-definition failure with an
unchanged-live-stock positive control. Actual illumination remains the intended
core first-red seam, and all combined Rust/process qualification is still pending.
