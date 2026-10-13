# Current capabilities and next work

This page owns current scope, sequencing and supported format identities.
References describe exact behavior; [future requirements](plans/requirements.md)
record accepted work that is not yet complete. “Implemented” describes code in
this tree, not a promise that every related product or performance goal is closed.
Every change still requires the [testing policy](development/testing.md).

**Current formats:** protocol **33**, save format **28**, scenario format **4**, ruleset
**`interactions-v28`**, scenario validator **`tor-scenario-15`**. Unsupported
protocol/save/rules identities are rejected rather than migrated. Other current
guides refer here instead of copying these identities.

## Implemented scope

| Area | Current capability | Authoritative detail |
| --- | --- | --- |
| Playable game | Authored five-chamber retrieval/escape dungeon, persistent death, shared text and ASCII play | [Gameplay](play/gameplay.md) |
| Rules and actors | Shared general rules imported by scenarios, validated overrides, pinned resolved catalogs, tracked creature builds and advancement, generic modifiers/conditions/abilities, deterministic 3d6 checks, permanent requirement predicates, deduplicated edge families, preparation-owned one-use tokens, active-time Health recovery, anatomy equipment, timed consumption | [Creature rules](reference/creature-rules.md), [items](reference/items.md) |
| Actions | Simulation-owned admission/execution, deterministic scheduling, paid preparation/resume, frozen prepared effect values with live execution validation, player/AI/travel lifecycle recovery | [Simulation](architecture/simulation.md) |
| World | Region-local 3D geometry, all 24 portal rotations, multi-cell bodies, gravity, finite material volumes, cardinal/diagonal movement, doors and paired stairs | [Geometry](architecture/geometry.md), [movement](reference/movement.md) |
| Perception | Range-16 illuminated eye-based 3D sight, body-local awareness, opaque disclosed targets, stale client memory and durable place knowledge | [Perception](architecture/perception.md), [clients](architecture/clients.md) |
| Sessions and storage | Loopback authenticated WebSockets, control transfer and spectators, background journaling, explicit durability barriers, checkpoints, replay and bounded wizard rewind | [Protocol](reference/protocol.md), [persistence](architecture/persistence.md) |
| Region lifecycle | Lazy authored/generated content, active/frozen/detached regions, deterministic preparation, independent persistence and asset palettes | [Streaming](architecture/streaming.md) |
| Authoring | Strict modular TOML packages, one compiler, source provenance, immutable compiled bundles, shared rules imports/overrides | [Packages](reference/scenario-packages.md) |
| Procedural worlds | Compact seeded finite/open grid-chain descriptors, random stable ports, authored graph composition, lazy materialization and bounded validation | [Packages](reference/scenario-packages.md), [streaming](architecture/streaming.md) |
| Rogue exploration foundation | Compact open-ended lazy floors, randomized entry/stairs, bounded whole-group preparation/publication, persistent paired returns, room lighting and dark corridors | [Generation](reference/generation.md) |
| Tools | JSON-lines headless client, privileged inspection/setup/rewind, shared-kernel arena evaluation, matrices and bounded search campaigns | [Headless](reference/headless.md), [wizard](development/wizard.md), [arena](development/arena.md) |

The architecture and creature-rules refactors are implemented and published.
They are current architecture, not active refactor plans. This does not close
all performance work or implement the remaining mechanics below.

The shared advancement ledger supports rules-defined point pools and purchase
kinds, two immutable starting ranks, owned skill/talent purchases, explicit
choose-again/authored restoration, disclosed dormancy reasons and atomic
replacement previews with exact subtree refunds. Immediate wizard editing uses
the same replacement kernel. The default
one-point-per-HD economy is provisional. Ordinary player progression selection,
XP credit attribution and timed ration-consuming respec are not yet implemented; see
[creature rules](reference/creature-rules.md#advancement-points-and-starting-ranks).

## Important limitations

- Clients must be relaunched to reconnect. Active travel does not automatically
  resume after server restart; valid preparation recovery requires fresh input.
- The server accepts loopback connections only. Packaged distribution and automatic
  local-server launch are not provided.
- Wizard rewind retains a 128-boundary window. It is unavailable in normal play
  and enabling wizard mode permanently marks the game lineage.
- The 3d6 kernel keeps the existing initial defense/bonus values; this is not
  a claim of balanced Rogue builds. The point economy, Health recovery and new
  playable talent slice still precede content calibration.
- The Rogue package is exploration-only: no populated roster, food pressure,
  complete item catalog, traps, Amulet victory or complete Rogue run.
  Its explicit operational depth limit is not a gameplay endpoint; persistent
  floors and paired returns deliberately differ from Rogue's floor lifecycle.
- Recipe generation supports the declared stages, not an arbitrary scripting
  language. Runtime scripting and its language remain undecided.
- Shared rule/effect infrastructure does not imply complete survival, carrying
  capacity, curses, enchantment, rust, identification, ranged projectile/ray,
  secret-search or hallucination gameplay. See the current references and
  [remaining requirements](plans/requirements.md) for their boundaries.
- Asset palettes do not yet drive the ASCII glyph table. There is no 3D frontend.
- Saves can still require matching compiled bundles for unvisited authored inputs.
  Portable saved-game export/import and historical migration are not implemented.

## Next work

### Performance and scalable persistence

Performance closure remains deferred and open; it does not block all feature
milestones. Existing targets still apply to every change. Resolve save/restore
and client timing tails, explain longer multi-client workloads, and measure
remaining history/actor scaling costs. Do not call the work complete from lower
operation counts or one favorable timing sample. The
[performance guide](development/performance.md) owns targets, workload boundaries,
reproduction, evidence and remaining acceptance criteria.

### Rogue adaptation and shared mechanics

Build on the implemented exploration foundation rather than recreate it. Its
ongoing performance acceptance, including persistence tails, is distinct from
whether floor generation exists. Add omitted rooms, mazes, secrets and traps as
shared support arrives. Extend shared survival, items, ranged/perception and
species behavior; then populate the Rogue roster and finish the objective/run.
The [Rogue plan](plans/rogue.md) defines the adaptation and dependency order.
Rogue species are built through the [progression plan](plans/progression.md)'s
advancement ledger, edge rules, regeneration and strike sequences; each of its
phases still needs separate approval.
Do not equate available framework types with all content being playable.

### Other scoped extensions

- Implement a disclosed-view bot framework with uncertain memory and ordinary
  action/travel boundaries; bots must not inspect server world state.
- Design [script extensions](plans/scripting.md) separately from declarative
  generation, including authority, budgets and deterministic saved state.
- Reconcile suspend/resume file handling, portable export, stronger recovery,
  larger retained history, packaging and client launch workflows.
- Treat remote authentication/encrypted deployment, multiplayer input policy and
  an immersive 3D client as separate later milestones.

Planning approval does not authorize starting an implementation milestone or
choosing a compatibility-breaking format. Follow [development practices](../CONTRIBUTING.md)
and obtain the maintainer's required approval.

## Updating status

Update this page with the relevant guide when scope changes. Remove fulfilled
items from plans, replace superseded behavior, and keep historical test counts,
CI gates and rollout narratives in Git history. Keep a necessary benchmark or
rejected-alternative record only with its scope, limitations and durable evidence.
The [documentation checks](../scripts/test_documentation.py) compare format
identities against code; they do not certify semantic accuracy or fresh CI.
