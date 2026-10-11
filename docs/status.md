# Current capabilities and next work

This page owns current scope, sequencing and supported format identities.
References describe exact behavior; [future requirements](plans/requirements.md)
record accepted work that is not yet complete. “Implemented” describes code in
this tree, not a promise that every related product or performance goal is closed.
Every change still requires the [testing policy](development/testing.md).

**Current formats:** protocol **33**, save format **26**, ruleset
**`interactions-v27`**, scenario validator **`tor-scenario-12`**. Unsupported
protocol/save/rules identities are rejected rather than migrated. Other current
guides refer here instead of copying these identities.

## Implemented scope

| Area | Current capability | Authoritative detail |
| --- | --- | --- |
| Playable game | Authored five-chamber retrieval/escape dungeon, persistent death, shared text and ASCII play | [Gameplay](play/gameplay.md) |
| Rules and actors | Shared general rules imported by scenarios, validated overrides, pinned resolved catalogs, tracked creature builds and advancement, generic modifiers/conditions/abilities, anatomy equipment, timed consumption | [Creature rules](reference/creature-rules.md), [items](reference/items.md) |
| Actions | Simulation-owned admission/execution, deterministic scheduling, paid preparation/resume, frozen prepared effect values with live execution validation, player/AI/travel lifecycle recovery | [Simulation](architecture/simulation.md) |
| World | Region-local 3D geometry, all 24 portal rotations, multi-cell bodies, gravity, finite material volumes, cardinal/diagonal movement, doors and paired stairs | [Geometry](architecture/geometry.md), [movement](reference/movement.md) |
| Perception | Range-16 illuminated eye-based 3D sight, body-local awareness, opaque disclosed targets, stale client memory and durable place knowledge | [Perception](architecture/perception.md), [clients](architecture/clients.md) |
| Sessions and storage | Loopback authenticated WebSockets, control transfer and spectators, background journaling, explicit durability barriers, checkpoints, replay and bounded wizard rewind | [Protocol](reference/protocol.md), [persistence](architecture/persistence.md) |
| Region lifecycle | Lazy authored/generated content, active/frozen/detached regions, deterministic preparation, independent persistence and asset palettes | [Streaming](architecture/streaming.md) |
| Authoring | Validated TOML packages, source diagnostics, immutable dependency identities, shared rules imports/overrides | [Packages](reference/scenario-packages.md) |
| Rogue exploration foundation | Nine-region declarative floor recipes, whole-group preparation/publication, named stair destinations, persistent 26-floor package, room lighting and dark corridors | [Generation](reference/generation.md) |
| Tools | JSON-lines headless client, privileged inspection/setup/rewind, shared-kernel arena evaluation, matrices and bounded search campaigns | [Headless](reference/headless.md), [wizard](development/wizard.md), [arena](development/arena.md) |

The architecture and creature-rules refactors are implemented and published.
They are current architecture, not active refactor plans. This does not close
all performance work or implement the remaining mechanics below.

## Important limitations

- Clients must be relaunched to reconnect. Active travel does not automatically
  resume after server restart; valid preparation recovery requires fresh input.
- The server accepts loopback connections only. Packaged distribution and automatic
  local-server launch are not provided.
- Wizard rewind retains a 128-boundary window. It is unavailable in normal play
  and enabling wizard mode permanently marks the game lineage.
- The Rogue package is exploration-only: no populated roster, food pressure,
  complete item catalog, traps, Amulet victory or complete Rogue run.
- Recipe generation supports the declared stages, not an arbitrary scripting
  language. Runtime scripting and its language remain undecided.
- Shared rule/effect infrastructure does not imply complete survival, carrying
  capacity, curses, enchantment, rust, identification, ranged projectile/ray,
  secret-search or hallucination gameplay. See the current references and
  [remaining requirements](plans/requirements.md) for their boundaries.
- Asset palettes do not yet drive the ASCII glyph table. There is no 3D frontend.
- Strict saves can still require matching uncopied region sources. Portable save
  export/import and historical migration are not implemented.

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
