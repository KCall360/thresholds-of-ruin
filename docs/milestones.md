# Project status and roadmap

This is the source of truth for project scope, sequence, and the current format
versions. It separates implemented behavior from planned work; feature guides
contain the detailed rules and verification. "Complete" means implemented,
documented, and covered by the tests required by the [testing policy](testing.md).

## Current implementation

The current tree is a playable development build. The default game is the
authored five-chamber dungeon in `scenarios/first-dungeon`: explore, fight,
retrieve the dawn seal, and escape.

**Current formats:** protocol **30**, save format **22**, ruleset
**`dungeon-v23`**, scenario validator **`tor-scenario-8`**. The server rejects
any other protocol, save format, or ruleset rather than migrating it. Other
documents refer to these as "current" instead of repeating the numbers, and
`scripts/test_documentation.py` checks that these values match the code.

| Area | Status | Implemented scope |
| --- | --- | --- |
| Foundation | Complete | Rust workspace, architecture checks, GPL licensing, full Windows/Linux CI in independent debug/release profile jobs |
| Simulation | Complete for current scope | Explicit actors, deterministic scheduling, cardinal/diagonal movement, wait, quantity-aware pickup/drop, inventory, doors, stairs |
| Saved gameplay intentions | Complete for current scope | Simulation queue, session admission/execution, linked journal records, typed receipts, ordered lifecycle updates, pending snapshots, client input guards, queued-work suspension and paused-attack recovery; autonomous decisions use the shared queue path; native travel admission/execution and restart settlement are published; stream contexts, exact bases and bounded resynchronization are published; bounded typed decoding is published; complete encoded observation selection and admission ownership are published; ordered collection edits, retained-state byte validation and bounded transport closure are published |
| Geometry and physics | Complete for current scope | Bounded 3D regions, all 24 portal rotations, finite stone volumes, multi-cell bodies, gravity, actor-relative scenes |
| Perception | Complete (3s) | Three-dimensional sight from declared eye cells, floors and ceilings as seen solid cells, opaque cell keys, stale client memory |
| Server and persistence | Complete for current scope | Local authenticated WebSockets, play that runs until it needs input, background journal, checkpoints, replay, history and annotations; backend action facts and numeric save DTO mappings are independent of wire action types |
| Clients | Complete for current scope | Text, native ASCII, and JSON-lines headless clients using shared disclosed state |
| Access and development | Complete for current scope | Control transfer, enforced spectators, wizard authorization, setup commands, 128-boundary rewind with retained branches |
| Navigation and interaction | Complete (milestone 3) | Known-cell travel, interruption, prose and examination, clarification, compound pickup/doors, durable places, narration, stream recovery |
| Authored scenarios | Complete (4a) | TOML packages, offline validation, pinned inputs |
| Item knowledge | Complete (4b) | Compatible stacks, seeded appearances, character-owned identities |
| Dungeon gameplay | Complete (4d) | Timed melee, typed damage, AI, death, retrieval and escape |
| Region streaming | Complete (4e) | Regions built on demand, detached to disk, generated between authored ones; asset palettes |
| Performance (3p) | Deferred, open | See [performance plan](performance-persistence.md#open-work) |
| Distribution | Not started | Packaged clients and automatic local-server startup |

Notable current limitations: clients must be relaunched to reconnect; active
travel doesn't resume after a server restart; the server listens on loopback
only; wizard history is bounded; generated regions are limited to rooms and
corridors between authored ones; and the ASCII client doesn't draw from asset
palettes yet.

The [architecture refactor](refactoring.md) has completed its accepted implementation review. Typed boundaries, shared
observations and route searches, backend-only portal-aware indexes, queued player,
AI and native travel gameplay, preparation recovery, exact stream contexts and
bounded recovery, opaque targets, safe wire integers, collection deltas, fair
output accounting and bounded transport closure are published. Independent saved
schemas, checkpoint sharing, atomic persistence, lazy prepared scenario content,
source diagnostics and semantic named generation streams are implemented.
Actual queued-action completion timing and the strict save-decoder memory
improvement are published. The integrated requirement review found no further
implementation gap in the accepted refactor scope. A navigation membership
experiment showed no useful release improvement and was removed; its behavioral
equivalence test remains. The published completion-review checkpoint passed the required verification and
Windows/Linux CI. Separate performance follow-up remains open. Save/physics tails and existing performance targets remain
in scope for the roadmap; no broad speedup is claimed.
Text-client product fixes and scripting implementation remain deferred.

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

### 3s — Three-dimensional sight

Complete for the accepted scope. One exact 3D rule sees from
body-declared eye cells across portal rotations; floors, ceilings and walls are
ordinary disclosed solid cells. Shared surface classification, text examination,
ASCII enclosure information and height browsing are implemented. View deltas,
reference equivalence, reciprocal empty-cell sight and one-/two-/three-cell
observer coverage are in the [sight guide](sight-3d.md#closeout-audit).

The closeout audit adds exhaustive segment-predicate checks, missing height
coverage and native presentation assertions without changing gameplay or format
versions. An ASCII redesign and palette-based glyphs remain separate work;
existing physics/save performance findings remain open under 3p.

Local verification passed 965 debug Rust tests and 313 Python/application tests,
with final formatting/lint and sight/documentation rechecks; optimized
world/simulation coverage passed 236 tests and both sight process cases.
Windows/Linux debug/release CI is required before merging the closeout.

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

### 4e — Region streaming, generation, and asset palettes

Regions have a lifecycle driven by reference points in game state (characters
and actors clients control), not hardcoded players: pins keep what an action
can touch loaded, frozen regions don't advance time, and detached regions are
kept on disk as self-contained records that reattach exactly. Package games
start with only the regions their characters need, build each region from the
package just before it's first loaded, move regions in and out after every
command, and preload the regions just beyond the loaded ones in the background.
Packages keep one file per region with a generated index, and saves pin their
package. Generated regions fill the gaps between authored ones, the same in any
build order; see [generated regions](scenario-packages.md#generated-regions).
The server sends each client an [asset palette](protocol.md#asset-palettes);
the text and headless clients resolve it, and the ASCII client's glyph table
waits for the ASCII redesign. Transition work is bounded by the loaded actors
and regions, never the whole world, and existing play costs about the same as
before 4e; see [region streaming](region-streaming.md#performance).

## In progress

### 3p — Performance and scalable persistence

Deferred at the maintainer's direction and still open. Phases A–E and the
explored-world checkpoint reduction are complete; remaining closure work (client
timing tails and the longer eight-client workload) no longer blocks feature
milestones. Every feature still gets performance checks. See the
[performance plan](performance-persistence.md) for targets, results, and open
work.

## Planned milestones

### Rogue scenario and shared TOR foundations

The [Rogue scenario plan](rogue-scenario-plan.md) records the accepted interview
and the [design plan](game-design-plan.md) owns shared requirements. First deliver
an exploration-only scenario: nine rooms/corridors carved through nine broadly
connected stone-filled regions, persistent floors and paired cross-region stairs.
Generate a whole floor when it enters the loading horizon. Extend the existing
streaming and semantic-stream generator foundations with reusable declarative
recipes and coordinated floor groups; do not wait for all mobs/items/effects.

Add omitted rooms, mazes, lighting, secrets, traps and content as shared support
arrives. Standard TOR systems cover builds/progression/CR, effects/defenses,
survival, anatomy equipment, knowledge and ranged/perception behavior. Rogue
supplies its roster and distinctive abilities; TOR supplies mechanics. Runtime
hooks and scripted victory are a separately designed extension. No new package
or generalized recipe syntax is implemented by this plan update.

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
multiplayer input policy remain outside the current sequence. Hunger and ranged
combat are accepted shared foundations for the staged Rogue work above. The
[game design plan](game-design-plan.md) records accepted
requirements for all of these.

## How the roadmap changes

Update this file whenever status or scope changes, alongside the relevant
feature guide. Every milestone must meet the [testing policy](testing.md):
behavior tests at each changed layer, an end-to-end acceptance scenario with the
real applications on Windows and Linux, regression tests for fixed bugs, and
performance checks that keep profiling tools current and don't relax targets.
