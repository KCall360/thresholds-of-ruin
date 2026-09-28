# Project status and roadmap

This is the source of truth for project scope, sequence, and the current format
versions. It separates implemented behavior from planned work; feature guides
contain the detailed rules and verification. "Complete" means implemented,
documented, and covered by the tests required by the [testing policy](testing.md).

## Current implementation

The current tree is a playable development build. The default game is the
authored five-chamber dungeon in `scenarios/first-dungeon`: explore, fight,
retrieve the dawn seal, and escape.

**Current formats:** protocol **16**, save format **11**, ruleset
**`dungeon-v16`**, scenario validator **`tor-scenario-4`**. The server rejects
any other protocol, save format, or ruleset rather than migrating it. Other
documents refer to these as "current" instead of repeating the numbers, and
`scripts/test_documentation.py` checks that these values match the code.

| Area | Status | Implemented scope |
| --- | --- | --- |
| Foundation | Complete | Rust workspace, architecture checks, GPL licensing, Windows/Linux CI |
| Simulation | Complete for current scope | Explicit actors, deterministic scheduling, cardinal/diagonal movement, wait, quantity-aware pickup/drop, inventory, doors, stairs |
| Geometry and physics | Complete for current scope | Bounded 3D regions, all 24 portal rotations, finite stone volumes, multi-cell bodies, gravity, actor-relative scenes |
| Perception | Complete for current scope | Symmetric shadowcasting, disclosed surfaces and entities, opaque cell keys, stale client memory |
| Server and persistence | Complete for current scope | Local authenticated WebSockets, background journal, checkpoints, replay, history and annotations |
| Clients | Complete for current scope | Text, native ASCII, and JSON-lines headless clients using shared disclosed state |
| Access and development | Complete for current scope | Control transfer, enforced spectators, wizard authorization, setup commands, 128-boundary rewind with retained branches |
| Navigation and interaction | Complete (milestone 3) | Known-cell travel, interruption, prose and examination, clarification, compound pickup/doors, durable places, narration, stream recovery |
| Authored scenarios | Complete (4a) | TOML packages, offline validation, pinned inputs |
| Item knowledge | Complete (4b) | Compatible stacks, seeded appearances, character-owned identities |
| Dungeon gameplay | Complete (4d) | Timed melee, typed damage, AI, death, retrieval and escape |
| Region streaming | In progress (4e) | Structural catalog and preload-horizon planning only |
| Performance (3p) | Deferred, open | See [performance plan](performance-persistence.md#open-work) |
| Distribution | Not started | Packaged clients and automatic local-server startup |

Notable current limitations: clients must be relaunched to reconnect; active
travel doesn't resume after a server restart; the server listens on loopback
only; wizard history is bounded; every authored region is loaded at once; and
there's no procedural generation.

## Completed milestones

### 0 — Repository and architectural boundaries

The workspace enforces a one-way dependency structure: world and simulation
contain no UI, network, filesystem, or wall-clock behavior; protocol types
contain no internal world state; clients consume actor-specific disclosed
observations. CI validates formatting, linting, tests, dependency boundaries,
and native client launch behavior on Windows and Linux.

### 1 — Shared playable slice

The server, text client, and native ASCII client can play and resume the same
saved game. Actions stream to controllers and observers, control transfer is
explicit, spectator credentials are server-enforced, and history preserves scoped
annotations. Duplicate or stale commands can't execute an action twice.

### 1a — Wizard development foundation

A distinct credential authorizes reproducible setup, placement, teleportation,
geometry editing, and bounded rewind. Enabling it permanently marks the game
lineage; rewinds retain abandoned branches and are never available in normal
play. See [wizard mode](wizard-mode.md).

### 2 — Geometry and perception foundation

[Portal geometry](portal-geometry.md), [place hints](place-hints.md),
[doors](doors.md), [symmetric shadowcasting](shadowcasting.md),
[finite material volumes](material-volumes.md),
[ASCII map memory](ascii-memory.md), and
[diagonal movement](diagonal-movement.md).

Further perception work follows gameplay needs: richer semantic events and sound
propagation will be added when interactions require them.

### 3 — Complete interactions and travel

Server-managed [travel](travel.md) through known cells, interruption and
cancellation at action boundaries, ASCII keyboard and mouse destinations,
[text intentions](text-adventure.md), prose and examination, noun clarification,
compound approach-and-pickup, open/close doors,
[durable place knowledge](place-knowledge.md), shared
[narration and stream recovery](narration-and-recovery.md).

The simulation's [action extension points](simulation-slice.md#shared-action-extension-points)
separate validation and timing, effect application, and scheduling. Offscreen
named-place destinations remain deferred.

**Rules for timed actions** (implemented for attacks in 4d; future timed actions
follow the same model): interruption preserves still-valid progress. Retrying
the same action resumes it, waiting preserves it, and other actions or movement
generally discard it. Damage alone interrupts without erasing progress. Player
and AI actors share the model, and each action can define its own policies and
meaningful partial effects. Implement specific progress behavior when an action
needs it, not as broad speculative machinery.

The backend resolves every ordinary step. Clients never receive a planned route
or future outcome, and ambiguity never consumes simulation time.

### 4a — Authored scenario packages and offline validation

Ordinary TOML packages describe the world and zones, region geometry, gravity
and anchors, outgoing portals and placements, archetypes with instance
overrides, theme pools, controller assignments, and objectives. Packages have
author-controlled `major.minor` versions and stable IDs, and an explicit
validation utility binds exact content hashes and dependency identities. Any
authored edit requires revalidation. Startup does inexpensive integrity checks
and refuses unvalidated or stale scenarios unless a development option allows
them. Validation is structural and deterministic, with recorded coverage rather
than an exhaustive gameplay proof.

Wizard edits can mutate loaded structure; they're journaled, mark validation
broken only when appropriate, and keep the separate permanent wizard-lineage
flag. See [scenario packages](scenario-packages.md).

### 4b — Items and character knowledge

Pickup, drop, and inventory with quantities, multiple items per cell, and
explicitly stackable archetypes with matching-property merge rules. True identity
is separate from per-character knowledge and deterministic randomized
appearances; confounding descriptions don't identify unrelated effects.
Knowledge persists after dropping items and through saves, replay, and rewind.
Identification gameplay, equipment, and item use come later. See
[items](items.md).

### 4c — Multi-cell bodies, rotated portals, and gravity

Portal transforms in all 24 cube rotations, including z-facing apertures
independent of stairs. Discrete multi-cell bodies, region gravity with sparse
overrides, averaged acceleration, persistent velocity with a speed cap, drift,
fixed-point simulation ticks, persistent body frames, rigid support and sliding,
and impact hooks. Crouching and ducking are deferred; whole bodies must fit. See
[bodies, portals, and gravity](physics.md).

### 4d — First complete dungeon loop

Explore, fight, retrieve, and escape through authored scenarios and both
playable clients. Timed d20-plus-bonus attacks against physical defense, line of
sight to any occupied target cell, differing speeds, HP, and typed damage
(energy, impact, keen, spirit, vital) with per-type immunity or flat reduction.
Enemies use search/attack/flee AI with perception-limited, expiring target
memory. Victory needs a player character at a named anchor, optionally carrying a
specific item. Death is persistent and leaves a corpse and dropped inventory.
Equipment, containers, locks, keys, and usable items aren't included. See
[dungeon gameplay](dungeon.md).

## In progress

### 4e — Region streaming, generation, and asset palettes

The first slice implements a structural region catalog, deterministic directed
preload-horizon queries, and a read-only authoring command; see
[region streaming foundations](region-streaming.md). Runtime freezing and
loading, generation, and palette delivery aren't implemented yet.

**Streaming.** Generate regions and zones on demand, depending on neighbors only
through fixed structural metadata. Activation within the preload horizon
persists generated results permanently; distant regions freeze every actor and
effect. Reactivation performs deterministic deferred updates before normal
scheduling. Save the complete active state, keep frozen regions on disk, and
leave unactivated areas as pinned scenario/seed/version references. Load the
saved active horizon first. Define cross-boundary effects and checkpoint and
event handling before implementing streaming, rather than advancing frozen
regions implicitly. Choose checkpoint boundaries using these scenario and
streaming requirements.

**Palettes.** Add separate palette snapshots and deltas over the existing
connection, with independent revisions and snapshot requests, no
acknowledgements, and identifiers only. Broad theme pools forecast assets before
instances exist, tailored to the player's horizon without signaling the next
room. Clients resolve dependencies, cache independently, and use fallbacks and
retry for unexpected assets. Recompute palettes on load. The palette protocol
doesn't need a 3D renderer.

**Acceptance:** equivalent generation and replay with fixed inputs; no region
replacement or frozen-time advancement; deterministic reactivation; save and
load of complete actor, item, and physics knowledge; loading bounded relative to
total world size; theme assets without entity disclosure; reconnect and gap
snapshots; fallback for unexpected assets. Keep the current explicit-save
barriers and consistent crash rollback.

### 3s — Three-dimensional sight

A separate effort from 4e: design accepted, implementation not started. It
replaces the plane shadowcasting, voxel height slices and floor/ceiling probes
with a single 3D rule for every observer. Sight lines start at a declared eye
cell. Floors, ceilings and walls are ordinary seen solid cells, and only their
convex exposed edges are beveled. Observation updates become view deltas. This
breaks the protocol and save format, which the maintainer has authorized. See
the [three-dimensional sight design](sight-3d.md).

**Acceptance:** an exact reference implementation; equivalence with today's
shadowcasting on single-level maps, with every difference reviewed; reciprocity
between empty cells; the scenario cases and existing suites listed in the
design, each run with one-, two- and three-cell-tall observers; the 4d acceptance tests and performance requirements; and any
accelerated layer proven identical to the reference.

### 3p — Performance and scalable persistence

Deferred at the maintainer's direction and still open. Phases A–E and the
explored-world checkpoint reduction are complete; remaining closure work (client
timing tails and the longer eight-client workload) no longer blocks feature
milestones. Every feature still gets performance checks. See the
[performance plan](performance-persistence.md) for targets, results, and open
work.

## Planned milestones

### 4f — Subsequent interaction extensions

Equipment after items, then item effects and other selected interactions.
Equipment changes and item use consume simulation time and can be interrupted,
using the shared progress/resume model. Capacity and weight, containers, locks
and keys, and richer identification are separate scoped extensions, each with
its own behavior and actual-client acceptance tests.

### 5 — Rogue-o-matic bot framework

Bots built on the ordinary disclosed client view, including explicitly uncertain
knowledge, map memory, inventory, and events. Begin with deterministic
exploration and scenario-driven test policies. Bots obey the same visibility,
travel, and action boundaries as human clients and never access server world
state.

### 6 — History, recovery, and distribution

Scale wizard rewind and branch retention beyond the current 128-boundary window;
add replay checksums, stronger interrupted-write recovery, packaged clients, and
automatic local-server startup or attachment. Verify compatible replay, retained
branches, and package launch behavior on both platforms.

Keep the server independently startable, with client-specific launcher
presentations. Manual saves default to character-named files with no slots,
following suspend/resume and persistent permadeath. Implement safe save
replacement and resumption alongside the background journal and recovery;
ordinary test checkpoint saves stay loadable through the same path. Plan exact-version
dependency selection and multiple installed scenario/ruleset/generator versions;
historical migration is optional later work.

### Later

An immersive 3D frontend is deferred until ASCII and text gameplay validate the
protocol and interaction model. Remote authentication, encrypted deployment,
multiplayer input policy, hunger, and ranged combat are outside the current
sequence. The [game design plan](game-design-plan.md) records accepted
requirements for all of these.

## How the roadmap changes

Update this file whenever status or scope changes, alongside the relevant
feature guide. Every milestone must meet the [testing policy](testing.md):
behavior tests at each changed layer, an end-to-end acceptance scenario with the
real applications on Windows and Linux, regression tests for fixed bugs, and
performance checks that keep profiling tools current and don't relax targets.
