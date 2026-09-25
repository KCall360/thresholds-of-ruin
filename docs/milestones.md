# Project status and roadmap

This is the source of truth for project scope and sequence. It distinguishes
implemented behavior from planned work; feature guides contain the detailed rules
and verification. “Complete” means implemented, documented, and covered by the
appropriate unit, integration, protocol, and actual-client process tests.

## Current implementation

The current tree is a playable development slice built around a deterministic
two-room fixture. New games use protocol **12**, save format **5**, and ruleset
**`diagonal-v11`**. Older protocols, save formats, and rulesets are rejected
rather than migrated or silently upgraded.

| Area | Status | Implemented scope |
| --- | --- | --- |
| Foundation | Complete | Rust workspace, architecture checks, GPL licensing, Windows/Linux CI |
| Simulation | Complete for the slice | Explicit actors, deterministic scheduling, cardinal/diagonal movement, wait, pickup, inventory, doors, stairs |
| Geometry | Complete for the slice | Bounded 3D regions, rotated and elevated joins, finite stone volumes, actor-relative scenes |
| Perception | Complete for the slice | Symmetric shadowcasting, disclosed surfaces/entities, opaque cell keys, stale client memory |
| Server and persistence | Complete for the slice | Local authenticated WebSockets, action journal, save/replay, protocol validation, history and annotations |
| Clients | Complete for the slice | Text, native ASCII, and JSON-lines headless clients using shared disclosed state |
| Access and development | Complete for the slice | Control transfer, enforced spectators, wizard authorization, setup commands, 128-boundary rewind with retained branches |
| Navigation and interaction | Partial | Known-cell travel, cancellation, prose/examination, clarification, approach-and-pickup, open/close doors |
| Dungeon gameplay | Not started | Equipment, combat, enemies, death, exit objective, authored/generated scenario inputs |
| Distribution | Not started | Packaged clients and automatic local-server startup |

The current fixture, compatibility behavior, and checks are described in the
[documentation index](README.md). Notable limitations are intentional: clients
must be relaunched to reconnect; active travel does not resume after a server
restart; server listeners are loopback-only; wizard history is bounded; and
there is no procedural generator or complete game loop.

## Completed foundations

### 0 — Repository and architectural boundaries

Complete. The workspace enforces a one-way dependency structure: world and
simulation contain no UI, network, filesystem, or wall-clock behavior; protocol
types contain no internal world state; clients consume actor-specific disclosed
observations. CI validates formatting, linting, tests, dependency boundaries, and
native client launch behavior on Windows and Linux.

### 1 — Shared playable slice

Complete. The server, text client, and native ASCII client can play and resume the
same saved game. Actions stream to controllers and observers, control transfer is
explicit, spectator credentials are server-enforced, and history preserves scoped
annotations. Duplicate or stale commands cannot execute an action twice.

### 1a — Wizard development foundation

Complete. A distinct credential authorizes reproducible setup, placement,
teleportation, geometry editing, and bounded rewind. Enabling it permanently marks
the game lineage; rewinds retain abandoned branches and never become available in
normal play. See [wizard mode](wizard-mode.md).

### 2 — Geometry and perception foundation

Complete for the present gameplay needs. Implemented slices include
[portal geometry](portal-geometry.md), [place hints](place-hints.md),
[doors](doors.md), [symmetric shadowcasting](shadowcasting.md),
[finite material volumes](material-volumes.md),
[ASCII map memory](ascii-memory.md), and
[diagonal movement](diagonal-movement.md).

Remaining perception work is driven by gameplay rather than more geometry in
isolation: richer semantic events, sound propagation, and durable player-facing
place knowledge will be added when interactions require them.

## Active direction

The [game design plan](game-design-plan.md) records the September 2026 decisions
and deferred architectural considerations. The sequence below incorporates them
without marking future systems implemented or expanding the current performance
work. New feature work begins after the applicable 3p gates; later scale work
must use scenario/streaming requirements when choosing checkpoint boundaries.

### 3p — Performance and scalable persistence

Status: **Phases A–C complete and merged; Phase D implemented and locally verified; merge requires final Windows/Linux CI**.

The shared versioned fixture drives focused and mixed movement, normal/rotated
crossings, doors, stairs, obstacle LOS, and scheduled actor visibility changes.
The release matrix combines 1/8/64/256 regions, 1/8 actors, and
0/100/1,000/10,000 retained actions in memory and durable modes. It measures
exclusive command phases, client application/rendering, actual I/O counts,
bytes, and normal restart/replay; a separate discovery trace grows map memory.

The [measured findings](phase-a-findings.md) retain the validated 64-case release
baseline, growing discovery and actual-client samples. The
[harness guide](performance-harness.md) describes reproduction, the real
headless/ASCII driver, and the verified 256-region spectator demonstration.
The [storage review](persistence-review.md) retains historical Phase A evidence.
Phase B replaces whole-save rewrites with an atomic SQLite append journal and a
bounded background worker. Ordinary acknowledgements can precede persistence;
explicit save, normal client exit, graceful server shutdown, and wizard enablement
wait for their saved prefix. See [background saving](background-saving.md).
The [Phase B findings](phase-b-findings.md) retain the nine-case comparison and
actual-client run. Phase B merged in PR #23 after Windows and Linux CI passed.
Phase C adds periodic atomic checkpoints, logical journal compaction with retained
history, and bounded tail replay. The [Phase C findings](phase-c-findings.md)
retain focused release measurements and their limits. See [checkpoints](checkpoints.md) for the format,
recovery contract, tests and limits. Phase D removes full-history candidate copies, shares rewind state, and reuses
scene work; see [its findings](phase-d-findings.md). Client responsiveness remains
Phase E; the broader performance milestone is not complete.
Process recovery tests do not establish hardware power-loss behavior.

Scope and sequencing are defined in the
[performance and persistence plan](performance-persistence.md). The milestone
will replace whole-save rewrites with an append-oriented journal plus bounded
snapshot/checkpoint work, remove avoidable full-state cloning, profile client
state application and rendering, and add scale-sensitive regression checks.
Phase B uses the [focused verification subset](performance-persistence.md#phase-b-verification-and-measurement)
against the retained Phase A baseline; a full characterization is conditional on
regressions or broader changes. Storage correctness coverage remains comprehensive.
Caching, alternate collections, and speculative presentation will be adopted
only for measured hot paths and must preserve determinism, disclosure, retry,
rewind, and crash-recovery behavior.

Acceptance requires consistent crash rollback and durable explicit-save barriers;
bounded p95 and maximum action latency as journal history and dungeon size grow;
recovery tests at every write boundary; equivalent replay, retry, and rewind
behavior; and actual-client tests proving the ASCII and text clients remain
responsive during saving. Because the project is pre-release, the new persistence
layout will explicitly reject old save formats rather than add a compatibility
importer.

### 3 — Complete interactions and travel

Status: **in progress; feature expansion paused behind milestone 3p**.

Already implemented: server-managed travel through known cells, interruption and
cancellation at action boundaries, ASCII keyboard/mouse destinations, text
direction intentions, prose and examination, noun clarification, compound
approach-and-pickup, and open/close doors.

Next scope:

- establish the resumable-action extension points needed by later timed actions;
- extend wizard placement so each failure and interaction can be reproduced;
- add named or durable place knowledge without revealing unseen topology;
- expand semantic event narration and threat/damage travel interruption; and
- verify slow-client presentation and resynchronization independently of server
  action timing.

Locks, keys, containers, equipment, and item use are deferred. New item foundations
are milestone 4b. Interruption preserves still-valid progress; retry resumes,
waiting preserves it, and other actions/movement generally discard it. Damage
alone interrupts without erasing progress. Player and AI actors share the model;
per-action policies and meaningful partial effects remain possible. Implement
specific progress behavior when an action needs it, rather than broad speculative
edge-case machinery.

The backend continues to resolve every ordinary step. Clients never receive a
planned route or future outcome, and ambiguity never consumes simulation time.

## Planned milestones

### 4a — Authored scenario packages and offline validation

Build the real scenario format before the first dungeon. Packages contain world
and zones, region geometry/gravity/anchors, local outgoing portals and placements,
archetypes with instance overrides, theme pools, controller assignments, and
objectives. World themes provide defaults; zones replace their pools. Player
starts use anchors; mobs initially use authored placements instantiated at region
activation. Support optional starting characters omitted or controlled by AI.

Use author-controlled major.minor versions and stable IDs, exact content hashes
and dependency identities, and an explicit validation utility. Any authored edit
requires revalidation. Startup uses inexpensive integrity checks and refuses
unvalidated/stale scenarios by default; runtime development options can permit
them. Structural and deterministic validation are required, with explicit limits
on large-world sampling rather than an exhaustive gameplay proof.

Acceptance: ordinary packages load in normal and wizard games and real client
process tests; invalid references, anchors, geometry, and stale validation fail
with useful diagnostics. Assertions remain in tests. Wizard can mutate loaded
structure and existing saves; journal edits, mark validation broken only when
appropriate, and preserve the separate permanent wizard-lineage flag. Package
authorship must not rely on a script of wizard setup commands.

### 4b — Items and character knowledge foundations

Add pickup/drop/inventory with quantities, multiple items per cell, explicitly
stackable archetypes and matching-property merge rules. Keep ownership/identity
ready for future capacity, equipment, containers, and item effects. Separate true
identity from per-character knowledge and deterministic randomized appearances;
confounding descriptions must not identify unrelated effects. Knowledge persists
after dropping/consuming items and through saves/replay.

Acceptance: stack split/merge and individual/requested pickup/drop preserve
quantities and identity, incompatible instances stay separate, and clients never
receive hidden identities. Both playable clients and save/recovery tests exercise
these rules. Full identification mechanics, equipment and item use come later.

### 4c — Multi-cell bodies, rotated portals, and gravity

Extend portal transforms including z-facing apertures independently of stairs.
Add discrete occupied footprints/heights, region gravity and sparse cell overrides,
aggregate diagonal acceleration, persistent velocity, terminal speed, scheduled
one-cell translations, drift, and blocked-component collision response. Other
actors continue on the scheduler. Provide impact-damage hooks and room for
momentum transfer without requiring a complete physics damage model first.

Before implementation settle deterministic integration, aggregate mass/normalization,
transformed occupancy/velocity, support, and collision ordering. No entity facing
system is required. Clarify impact versus general acceleration damage using the
[open physics considerations](game-design-plan.md#portal-geometry-bodies-and-gravity).

Acceptance: multi-cell gravity aggregation, rotated crossings, discontinuous and
zero-gravity fields, diagonal sliding, collision hooks, concurrent scheduled actors,
and equivalent save/replay outcomes. Include real-client disclosure/presentation
checks and reproducible scenario packages. Numerical tuning can follow mechanics.

### 4d — First complete dungeon loop

Deliver explore, fight, retrieve, escape through authored scenarios and both
playable clients. Shared actors support timed d20-plus-bonus attacks versus
physical defense, LOS to any occupied target cell, differing speeds, HP, typed
damage, immunity and flat reductions to zero. Initial damage types are energy,
impact, keen, spirit, and vital; allow multiple components with resolution policies
defined during implementation. Initial enemies use search/attack/flee, a transition
lookup table, and perception-limited expiring target memory. Scenario-selected AI
also controls optional starting characters.

Victory requires one player character at a named anchor, optionally with a specific
authored item; visibility and continued play are scenario-configured. Death is
persistent, leaving a corpse item and inventory at the base cell. Equipment,
containers, locks, keys, and usable-item effects are not prerequisites.

Acceptance: deterministic attack/AI outcomes, actor-independent targeting and
interruption, drops/corpses, durable death/victory, hidden-information protection,
and the complete loop using real text/ASCII clients. Tests use ordinary packages,
with wizard edits only where the test needs them.

### 4e — Region streaming, generation, and asset palettes

Follow the authored loop with region/zone on-demand generation and large-world
loading. Dependencies on neighbors use fixed structural metadata only. Preload
activation persists generated results permanently; distant regions freeze every
actor/effect. Reactivation performs deterministic deferred updates before normal
scheduling. Save complete active state, retain frozen regions on disk, and leave
unactivated areas as pinned scenario/seed/version references. Load the saved
active horizon first. Define cross-boundary effects and checkpoint/event handling
before implementing streaming rather than advancing frozen regions implicitly.

Add separate palette snapshots/deltas over the existing connection, independent
revisions and snapshot requests, no acknowledgements, and identifiers only.
Broad theme pools forecast assets before instances exist; tailor the palette to
the player's horizon without signaling the next room. Clients resolve dependencies,
cache independently, and use fallbacks/retry for unexpected assets. Recompute
palettes on load. The palette protocol can be implemented independently once
scenario themes exist; no 3D renderer is a prerequisite.

Acceptance: equivalent generation/replay with fixed inputs, no region replacement
or frozen-time advancement, deterministic reactivation, save/load of complete
actor/item/physics knowledge, bounded loading versus total world size, theme
assets without entity disclosure, reconnect/gap snapshots, and unexpected-asset
fallback. Preserve current explicit-save barriers and consistent crash rollback.

### 4f — Subsequent interaction extensions

Add equipment after items, then item effects and other selected interactions.
Equipment changes/use consume simulation time and can be interrupted; exercise
shared progress/resume policies as these actions are introduced. Capacity/weight,
containers, locks/keys, and richer identification mechanics remain separate scoped
extensions. Each needs its own behavior and actual-client acceptance scenarios.

### 5 — Rogue-o-matic bot framework

Build bots on the ordinary disclosed client view, including explicitly uncertain
knowledge, map memory, inventory, and events. Begin with deterministic exploration
and scenario-driven test policies. Bots must obey the same visibility, travel, and
action boundaries as human clients and must never access server world state.

### 6 — History, recovery, and distribution

Scale wizard rewind and branch retention beyond the current 128-boundary window;
add replay checksums, stronger interrupted-write recovery, packaged clients, and
automatic local-server startup or attachment. Verify compatible replay, retained
branches, and package launch behavior on both supported platforms.

Keep the server independently startable and client-specific launcher presentations.
Manual saves default to character-named files with no slots, following suspend/resume
and persistent permadeath. Implement safe save replacement/resumption alongside
background journals and recovery. Plan exact-version dependency selection and
multiple installed scenario/ruleset/generator versions; historical migration is
optional later work, not implicit minor-version compatibility. Ordinary test
checkpoint saves remain usable through the same load path.

An immersive 3D frontend is deliberately deferred until ASCII and text gameplay
validate the protocol and interaction model. Remote authentication, encrypted
deployment, multiplayer input policy, hunger, and ranged combat
are also outside the current milestone sequence.

## How the roadmap changes

Performance remains a completion criterion throughout all later milestones.
Maintain the profiling code and evolve versioned workloads with new features;
run focused release-build latency and scaling comparisons on the affected paths.
Investigate material regressions before completing a feature. The full benchmark
matrix is not required for every change; broaden it for cross-cutting changes or
unexplained results. See [development practices](../CONTRIBUTING.md) for the
ongoing profiling and targeted verification requirements.

Every feature must add behavior tests at the layers it changes and an end-to-end
acceptance scenario using the real applications. Update this file when status or
scope changes, and update the relevant implementation guide with behavior,
limitations, compatibility, reasoning, and verification. See
[development practices](../CONTRIBUTING.md) for the full completion criteria.
