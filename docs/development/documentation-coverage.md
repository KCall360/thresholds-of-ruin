# Documentation depth and coverage

This map answers where the system is specified in detail. It distinguishes
source-backed explanation from fresh runtime qualification and accompanies the
[post-206 review](documentation-review.md), not another rules/schema owner.
Reviewed source: [`dacbc215`](https://github.com/KCall360/thresholds-of-ruin/commit/dacbc215e12aaa13ef3cf89900836a296127f074).
A bounded integration check against merged PR 207,
[`b758868`](https://github.com/KCall360/thresholds-of-ruin/commit/b758868a6c790788d2ad46597a8932eac1f954d4),
adds current advancement pending-reply and asynchronous 3D/UI behavior without
claiming a new graphical or runtime qualification.

The original documentation already had substantial exact contracts. The review
found contradictions, current mechanics stranded in completed plans, and missing
joined explanations across detailed modules. Reorganization alone did not prove
completeness. New chapters add order, invariants, frozen/live operands, failure
matrices and worked cases rather than treating shorter pages as a specification.

## Engineering reading paths

### Follow a request through the simulation

1. [Architecture](../architecture/overview.md): ownership, clocks, identities and
   candidate/admission/publication boundaries.
2. [Simulation cycle](../architecture/simulation-cycle.md): ingress to publication,
   special input-boundary priority, scheduler ties, stationary/moving tick order,
   timers, physics, actor settlement, preparation, region handoff and rollback.
3. [Actions and atomicity](../architecture/simulation.md#actions-and-atomicity):
   paid plans, holds, retained progress, partial sequences, pause/cancel and travel.
4. [Persistence](../architecture/persistence.md): admission versus durability,
   strict records, checkpoint capture/selection, restoration and failure modes.

This is not a fixed render loop. One request can remain queued, one action can
cross several deadlines, and several actors can act at the same tick.

### Implement a protocol client

1. [Protocol lifecycle](../reference/protocol-lifecycle.md): handshake, attachment,
   control/input/readiness states, complete family index, gate order, paging,
   exact retry and worked recovery.
2. [Wire reference](../reference/protocol.md): exact fields/bounds/null rules,
   opaque references, causal events, pictures/deltas, charts and tested envelopes.
3. [Client architecture](../architecture/clients.md): validated reducers, memory,
   parser/clarification, narration and separate visual demand.
4. [Headless](../reference/headless.md): a distinct JSON-lines interface, not raw
   WebSocket protocol envelopes.

Acknowledgement, input readiness, new picture and durable save are independent.
A client cannot substitute a picture revision for receipt or input authority.

### Explain or change combat

1. [Combat](../reference/combat.md): lifecycle, frozen/live operands, checks/ties/
   edge, immunity/resistance/damage/riders, deliveries, payment/debt and examples.
2. [Creature rules](../reference/creature-rules.md): source/modifier folds,
   defenses, condition lifecycle, cooldowns, nutrition and recovery.
3. [Equipment](../reference/equipment.md), [progression](../reference/progression.md),
   [Polymorph](../reference/polymorph.md): weapon/grip selection, expertise, retained
   body/personal ownership and runtime form interactions.
4. [Magic items](../reference/magic-items.md), [effect details](../reference/consumable-effects.md):
   item timing, identification, growth/overflow/expiry and special devices.

The joined chapter links exact subcontracts; it does not define a parallel combat
engine or imply balance from an arithmetic example.

## Whole-project coverage matrix

**Detailed** means concrete current invariants/algorithms with source/witness
pointers. **Joined** means a path added across previously separate contracts.
Neither means every code branch has prose or the referenced tests were freshly run.

| Area | Canonical detailed owner | Review disposition / remaining boundary |
| --- | --- | --- |
| Ownership/authority | [Overview](../architecture/overview.md), [decisions](../architecture/decisions.md) | Distinct clocks, identities and typed boundaries clarified; save-owned leaf adaptation remains proposed hardening |
| Simulation | [Cycle](../architecture/simulation-cycle.md), [actions](../architecture/simulation.md) | Joined input→scheduler→timer/physics→preparation→handoff→publication; actor-ID/authored-row order, failure and async-save distinctions; not a fairness proof |
| Wire | [Lifecycle](../reference/protocol-lifecycle.md), [protocol](../reference/protocol.md) | Complete enum-family index, retry/gates, page installation, previously unnamed projectile-choice/weapon-preview integration; typed wire code remains exhaustive field authority |
| Clients/controls | [Clients](../architecture/clients.md), [Text](../play/text.md), [ASCII](../play/ascii.md), [headless](../reference/headless.md) | Current controls and causal narration/readiness distinct from pictures; cursor-only pending advancement progress distinguished from exact displayed-page freshness; optional 3D was not visually requalified |
| Combat | [Combat](../reference/combat.md), [creature rules](../reference/creature-rules.md) | Frozen targeted checks vs collision-time recipient operands, per-pool mitigation, exposure/riders, holds/debt, death and worked examples; balance unestablished |
| AI | [AI](../reference/ai.md) | Sixteen priority branches, pure choice vs committed RNG, knowledge, retained work, care/items/defense and narrow fallback; flee metric has an unexecuted reproduction candidate |
| Perception/navigation | [Perception](../architecture/perception.md) | Discovery/guise receipts, eyes/contact, exact geometry/cache/oracle and known-space route algorithms; no new performance/security certification |
| Geometry/movement | [Geometry](../architecture/geometry.md), [movement](../reference/movement.md) | Frames/portals, finite volume/body placement, swept motion/gravity, remembered travel, stairs/doors; automatic trap-hazard classification remains deferred |
| Region lifecycle | [Streaming](../architecture/streaming.md) | State/transition/pin/reference-point contracts, freeze/no-catch-up, lazy sources/load/prefetch and first-entry activation |
| Spatial source ownership | [Registry](../architecture/spatial-registry.md), [metadata](../reference/scenario-packages.md#canonical-room-and-passage-metadata) | Source membership, validation, retained roots, floor/room/passage meanings; membership predicates do not replace geometry validation |
| Persistence/replay | [Persistence](../architecture/persistence.md) | Admission vs flush, bounded records/jobs, strict formats/checkpoints/atomic selection, pinned sources, replay and saved work; export remains proposed |
| Compilation | [Compilation](../architecture/scenario-compilation.md), [packages](../reference/scenario-packages.md) | TOML/modules/identities, canonical compiled inputs vs mutable saves, source provenance and lazy immutable bundle requirements; no runtime scripting implied |
| Progression/body | [Progression](../reference/progression.md), [Polymorph](../reference/polymorph.md) | Package/personal rows, locked pools, expertise/Intellect, XP, exact drain receipts, dormant choices and pending return; temporary modes/roster/consent future |
| Physical items/equipment | [Items](../reference/items.md), [equipment](../reference/equipment.md) | Stacks/holds/transfers, actor knowledge, primary socket/grips, curses/corrosion/birth; general carrying/containers separate |
| Magic/effects | [Magic](../reference/magic-items.md), [effects](../reference/consumable-effects.md) | Current growth/sensing/fuel/area effects promoted from history; real item Zap timing separated from ability metadata; torch not live stock |
| Generation/content/objective | [Generation](../reference/generation.md), [live policy](../reference/scenario-packages.md#live-rogue-content-policy) | Independent seeded stages, omission/maze/secrets/traps, preparation/population/stock/treasure and retrieval; depth/HD/receipt caps operational, not balanced endgame |
| Player onboarding | [Quickstart](../play/rogue.md), [mechanics](../play/rogue-mechanics.md), [advancement](../play/advancement.md) | Correct first HD2 purchase, checked private Search, actual roster/stock/survival; player summaries link exact references |
| Testing/performance | [Testing](testing.md), [objectives](test-objectives.md), [effectiveness](test-effectiveness.md), [performance](performance.md), [server measurements](server-performance-review.md) | Policies and negative/scoped evidence retained; this documentation change is not fresh runtime/native/benchmark/final-head CI qualification |
| Optional 3D/art | [3D guide](../../crates/client-3d/README.md), [art direction](../art/3d-art-direction.md), [continuation](../plans/3d-art-continuation.md) | Merged asynchronous stand-ins, responsive startup and live inventory/equipment/advancement/map/journal documented; independent workspace and unqualified crowd-performance target retained; no comprehensive graphical source/visual audit claimed |
| Plans/research | [Plan map](../plans/README.md), [pinned research](../research/rogue-5.4.4/README.md) | Implemented rationale versus future work, immutable checkpoint history, external research not a second TOR ruleset |

## What is not claimed complete

- This is a cross-domain static review, not formal verification, exhaustive API
  prose, a security audit or proof every edge case has a test. New examples are
  explanatory inputs/results, not new execution logs.
- No new timings settle flee behavior, cell/entity adaptation cost, capability-held
  Game memory, history startup cost or checkpoint retry classification. See
  [qualified engineering findings](documentation-review.md#recommended-engineering-work).
- Full-run balance, food/treasure/XP/recovery calibration and later-depth acceptance
  require separate play/measurement. Guard exhaustion is not intended endgame.
- Optional 3D/native/art acceptance, scripting, export and temporary forms remain
  separately scoped. Do not fill a future contract with invented current behavior.

## Maintaining useful depth

Update a mechanic's exact owner and any joined ordering sequence it affects.
Preserve frozen/live inputs, authorization/disclosure ceilings, failure/rollback,
clocks and ownership. Use worked edge cases where they explain interactions;
do not copy full schemas or logs into many pages. Move durable rationale to a
current owner or labeled design record before trimming chronology. The
[documentation policy](../README.md#maintaining-this-documentation) and
[testing policy](testing.md) remain authoritative.
