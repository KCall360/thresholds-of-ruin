# Retained spatial registry reconstruction — historical design record

**Status:** the registry, strict pooled retention, canonical source installation,
all-root validation and ordinary Room Light integration described by the later
checkpoints are implemented. The former opening “not implemented proposal” no
longer described main. Use the [current spatial architecture](../architecture/spatial-registry.md)
and [authored source contract](../reference/scenario-packages.md#canonical-room-and-passage-metadata)
for present behavior, not an early scaffold paragraph.

The original document recorded recovery after the 2026-10-24 executor reset and
then accumulated first-red, partial and green checkpoints. Its complete decisions,
fixture corrections and qualified historical results remain available in the
[immutable pre-reorganization record](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/plans/rogue-spatial-registry-reconstruction.md).
That history is not a fresh verification receipt or authority to start a new
implementation milestone.

## Decisions retained

- Simulation owns immutable classifications; World alone owns physical adjacency
  and portal transforms. Do not save a competing transform graph.
- Population scope, spatial scope, room identity and storage region are different.
  Exact z membership is required; neither LOS nor region identity invents a room.
- Doorways are room-primary with passage affiliation. Drain's predicate remains
  directional; abstract stairs do not connect physical passage components.
- Per-room illumination includes actual room volume and appropriate six-face
  surfaces, excluding open corridor mouths and diagonals. A foreign solid face
  can be a surface dependency without acquiring primary-region membership.
- Registration alone creates no AI turn, activation or elapsed actor time.
- Structural validation and strict encoding are necessary but not source
  certification. Current, detached and historical roots require canonical
  expected-set validation, including missing whole scopes.
- Enabled-source capability analysis follows reachable items/grants/acquisition
  paths. An unused imported definition alone must not force topology or add loot.

## Rejected capacity shortcut

The proposed 65,536 combined-reference ceiling could not represent an existing
maximum valid generated scope. The source fixture
`maximal_generated_scope_has_exact_room_volume_and_physical_illumination`
constructs an 8×8 generated package with 32×32×2 members and 30×30 rooms. Its
independent geometry oracle expects 115,200 room-volume cells and 245,760
illumination references minus twice the actual open mouths. Package,
materialization and Engine construction are prerequisites; this is not a
whole-floor residency or complete-archive capacity proof.

The historical estimates below explain the rejection, not new admission limits:

| Geometry | Conservative estimate |
| --- | --- |
| 64 generated 32×32×2 members | At most 131,072 primary cells |
| One closed 30×30×2 room | 3,840 illumination references; ideally 124 x-runs |
| 64 authored 32×32×8 members | At most 524,288 primary cells |
| One full 32×32×8 authored room | 11,264 illumination references; 336 surface plus 256 primary runs |
| 64 such authored rooms | 1,245,184 combined references; 37,888 runs |
| Loose primary plus self-and-six-face work | 4,194,304 combined references |

Open mouths can fragment runs. Generated grid limits do not impose an authored
64-member cap: the existing authored package envelope permits up to 65,536
regions subject to separate source/work limits. The 67,108,864 validation-cell-work
ceiling is not a per-scope registry reference allowance. The real modest
65-member fixture independently expects 1,170 primary and 3,900 surface
references, 1,430 runs and 6,500 scanned terrain cells. It is not a maximum-world
or whole-archive proof. The unchanged 64 MiB archive ceiling covers unrelated
game state too; compact runs cannot guarantee every combination fits it.

## What the checkpoint history actually established

The sequence progressed through trusted structural primitives, canonical compiler
inputs, authored schema, standalone strict codec, Game/history pooling and ordinary
paid Room Light. Early “not wired”, “no-op” and “runtime pending” paragraphs refer
to those intermediate states, not current implementation.

The retained historical record deliberately distinguishes:

- Source-first missing-behavior scaffolds from an observed first-red run.
- Genuine paid-effect failures from actorless restore fixtures or trying to
  acquire an intentionally AI-controlled actor.
- Direct/Exposure publication facts from combat-check diagnostics that a
  self-device does not produce.
- Journal replay success from real compressed-checkpoint persistence acceptance.
- The real strict-decoding defect—canonical serialization attempted before the
  decoded spatial pool had its actual roots bound—from the later correction that
  prepared bounded serialization authority without permitting arbitrary unbound
  serialization.
- Focused library/Service/process receipts from quick/push/full, native, release
  or hosted-CI qualification.

The final historical cohort reports 37 spatial controls plus eight Room Light
core tests, 1,067 server library tests, strict all-target Clippy, certificate
checking and both original imported Text cases including source-deleted,
interval-one compressed-checkpoint reopen. Those are retained past claims with
the original qualifications, not tests rerun by this documentation review and not
a blanket guarantee about deleting every compiled bundle with uncopied inputs.

## Follow-up boundaries

The implemented architecture should remain the single current owner. Future
format changes need explicit approval and owning first-red/green contracts;
performance or larger capacity claims need measurements and real admitted
fixtures. Preserve foreign-face versus disconnected/physically distant same-room
witnesses, actor clocks, payment, malformed-root rejection and source provenance.
Do not weaken restore validation to make a setup fixture pass or replace
canonical source verification with a permissive structural check.
