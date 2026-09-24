# Project status and roadmap

This is the source of truth for project scope and sequence. It distinguishes
implemented behavior from planned work; feature guides contain the detailed rules
and verification. “Complete” means implemented, documented, and covered by the
appropriate unit, integration, protocol, and actual-client process tests.

## Current implementation

The current tree is a playable development slice built around a deterministic
two-room fixture. New games use protocol **12**, save format **4**, and ruleset
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

### 3p — Performance and scalable persistence

Status: **Phase A complete; Phase B implemented and locally verified**.

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
actual-client run. All 200 Rust tests pass in debug/release; local client suites
pass except the native mouse test blocked by desktop access/occlusion. All four
desktop launchers are verified. Windows/Linux CI is required before merge; see
[the publication handoff](session-handoff.md) and the live PR for its status.
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

- add locks, keys, containers, and the corresponding item properties;
- extend wizard placement so each failure and interaction can be reproduced;
- add named or durable place knowledge without revealing unseen topology;
- expand semantic event narration and threat/damage travel interruption; and
- verify slow-client presentation and resynchronization independently of server
  action timing.

The backend continues to resolve every ordinary step. Clients never receive a
planned route or future outcome, and ambiguity never consumes simulation time.

## Planned milestones

### 4 — Dungeon gameplay

Add equipment, melee, two enemy types, differing actor speeds, death, and an exit
objective. Player actors and mobs use the same actions; controllers supply intent.
Introduce explicit scenario and world-schema inputs before seeded procedural
generation becomes the default, supporting authored, generated, and hybrid maps.

Acceptance requires deterministic combat, victory, persistent permadeath, and all
scenario forms through both playable clients. Wizard setup must reproduce combat
and death cases without bypassing the ordinary actions under test.

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

An immersive 3D frontend is deliberately deferred until ASCII and text gameplay
validate the protocol and interaction model. Remote authentication, encrypted
deployment, multiplayer input policy, hunger, identification, and ranged combat
are also outside the current milestone sequence.

## How the roadmap changes

Every feature must add behavior tests at the layers it changes and an end-to-end
acceptance scenario using the real applications. Update this file when status or
scope changes, and update the relevant implementation guide with behavior,
limitations, compatibility, reasoning, and verification. See
[development practices](../CONTRIBUTING.md) for the full completion criteria.
