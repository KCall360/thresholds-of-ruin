# Architecture and design rationale

## Scope

One authoritative backend supports graphical ASCII and interactive-fiction
clients through the same protocol; an immersive 3D client is a later possibility.
A game can continue across client switches. Gameplay is currently single-player,
but actor identity and control are explicit so the design does not assume a
global player. Windows is primary and Linux is continuously tested.

This document records durable boundaries and their rationale. It is not a
roadmap or changelog. The [project status and roadmap](milestones.md) identifies
implemented and planned scope; the [documentation index](README.md) links to the
current behavioral specifications.

The [game design plan](game-design-plan.md) records accepted future requirements
and explicitly deferred decisions. Those requirements are not claims of current
runtime support; use the roadmap to distinguish planned work from the slice.

## Workspace boundaries

| Crate | Responsibility |
| --- | --- |
| tor-world | Region-local geometry, entities, containment, portals |
| tor-simulation | Rules, scheduler, perception, deterministic transitions |
| tor-protocol | Versioned commands, disclosed observations, semantic events |
| tor-server | Sessions, validation, networking, travel, persistence, scenario packages; also builds the `tor-scenario` authoring tool |
| tor-client-common | Connections and a model of disclosed observations |
| tor-client-ascii | Graphical ASCII input and presentation |
| tor-client-text | Deterministic language parsing, clarification, prose |
| tor-client-headless | JSON-lines scripted play and disclosed-state inspection |
| tor-test-support | Shared scenario fixtures and integration tests |

The world and simulation have no network, filesystem, rendering, or clock
dependencies. The protocol owns DTOs independent of internal simulation state.
The server translates authoritative state into actor-specific observations.
The client crates cannot depend on world or simulation crates.

Server diagnostic examples/tests have explicitly reviewed development edges to
client-common, client-ascii, and test-support. Those edges are not permitted as
runtime/build dependencies; other server development dependencies receive the
same boundary checks as every crate. Shared diagnostic traces belong in fixture
data and test-support orchestration. Thread-local simulation call counters are
outside persisted game state, read no clock, and never affect rule decisions.

## Combat and autonomous actors

Combat state belongs to each actor, while the simulation owns a seeded,
checkpointed combat random stream, directed faction hostility, and run objectives.
Hit checking and typed damage resolution are separate steps; future attack
outcomes can extend them without changing the d20 rule implicitly. Wind-up is
persisted progress; recovery remains scheduler time. Physics applies impact damage
at contact, using the same death and interruption rules as attacks.

AI consumes its actor's perception and expiring memory. Its deterministic choice
is committed through ordinary journaled actions. The server bounds autonomous
work between delivery/input opportunities and records human preparation suspension
on control loss or recovery boundaries. Backend suspension commands have no wire
representation. Clients receive only their own exact health/progress and visible
qualitative enemy injuries. See [dungeon gameplay](dungeon.md) for the rule contract.

## Geometry and barriers

Regions are bounded rectangular 3D volumes with integer local coordinates.
Cells are five-foot cubes; elevations use an integer z coordinate.
[Material volumes](material-volumes.md) define finite stone storage with carved
interiors, including floors and ceilings. Standard interiors are two cells high.
Terrain occupancy and backend material identity are separate; unallocated space
is neither empty terrain nor stone. Separate regions make sparse allocation natural. Dense, chunked, or sparse cell storage
within a region is an implementation choice to measure later.

Clients know nothing about regions, portals, or their transforms. The backend
projects all visible occurrences into one actor-relative scene, including when
they span several regions. Opaque cell keys support memory without exposing
geometry identity. Movement uses the same observer axes across rotated joins.

Portals connect apertures using translation and any of the 24 proper cube
rotations, including z-facing apertures. Stairs are explicit traversal links,
separate from physical apertures. The world needs no consistent global embedding;
reflections remain deferred.

Rigid bodies occupy discrete cells in a persistent body frame. Region-local
gravity and sparse cell overrides are averaged over occupied cells. Fixed-point
velocity drives scheduled cell crossings with a magnitude cap, drift, support,
and component-wise collision response. Body frames preserve observer axes across
portals without introducing gameplay facing. Static gravity tables share world
geometry storage; dynamic motion and impact hooks belong to simulation snapshots.
See [physics](physics.md) for rules, limits, and deterministic ordering.

[Doors](doors.md) are implemented interactive world entities, independent of
portals. A door may occupy an interior opening or obstruct a portal aperture.
Its closed state (and future locked state)
does not define portal topology. Movement and perception evaluate terrain and
barriers along their paths.

[Unnamed place hints](place-hints.md) are map-authored anchor points separate from
geometry regions. A perceived space may span regions; a region can contain several
anchors or none. Hints have no names, descriptions, or area boundaries. Clients
may combine them with perceived geometry and contents to organize locations.
The [text adventure slice](text-adventure.md) uses visible hints and object names
for an initial place/direction heuristic. Characters also keep persistent names
for anchors they've seen (see [durable place knowledge](#durable-place-knowledge)).
Richer grouping and offscreen waypoint navigation remain future work.

Visibility follows portal paths with explicit range limits and cycle handling.
The backend owns visibility, appearance facts, sound disclosure, and hidden
information. Clients retain their own prior disclosed observations as remembered
knowledge, separately from the backend's current world truth; that memory can be
incomplete or stale. Exact sound propagation rules are deferred.

The [client-memory slice](headless-client.md) retains received cell
views in `tor-client-common`, separate from current state. Same-branch snapshots
preserve memory; new branches clear it. It lasts only for the connection and
does not infer views from known place names or history. [Portal sight](portal-geometry.md)
refreshes only currently visible cells. ASCII displays an aligned
[remembered map](ascii-memory.md); text memory presentation remains later work.

## Time and actions

[Diagonal movement](diagonal-movement.md) uses exact integer `ceil(base × √2)`
recovery and permits a clear diagonal destination when at least one side is clear.
Door manipulation supports the same diagonal reach at its normal action cost.

Integer simulation time and a deterministic scheduler support variable action
costs and actor speeds. Equal-time outcomes have stable ordering. The world
advances until player input is required, never because a frame or network delay
elapsed. Each actor uses the same action system.

Queries reveal already available information without consuming time. Physical
inspection and manipulation can be actions with costs. Invalid requests and
unresolved noun ambiguities do not consume time; in-world failed attempts follow
the relevant action rule. Multiplayer waiting and simultaneous input policies
are deferred.

Gameplay requests first admit bounded intentions without advancing time or applying
effects. The simulation selects due queued work and rebuilds its transient
preparation against current state. Admission and execution each pass through a
private candidate, persistence admission and ordered publication. A typed receipt
acknowledges admission; client readiness comes from the simulation stream.

The simulation's private action boundary separates read-only validation and timing,
effect application, and scheduler advancement. `Game::act` completes the command
boundary without yielding. Immediate actions apply effects and then incur recovery;
attacks instead commit per-actor wind-up progress and resolve at a later simulation
boundary. Fallible command validation, including timing overflow, precedes mutation.
Existing direct-action profiling covers this internal boundary; queue admission
and scheduled-execution timing still need coordinated profiling. Simulation code
introduces no wall clock.

Attack identity, target, remaining work, and active/suspended state are authoritative
and checkpointed. Resolution revalidates perception, reach, and displacement before
applying damage. Positive HP loss interrupts, waiting preserves valid progress,
and retrying the same attack resumes it. Movement, another action, or loss of target
validity discards progress. Recovery time remains distinct from resumable work.
Control-loss/restart boundaries preserve valid work but require fresh human input;
travel keeps its no-auto-resume policy. Future timed actions can
reuse these boundaries with their own concrete partial-effect policies.

## Protocol and streaming

The initial network target is versioned JSON over WebSockets. Local and remote
clients use the same transport. Bind local servers to loopback by default;
remote authentication and encrypted deployment must be designed before exposing
a server publicly.

Message families cover session control, queries, actions, and server-pushed
observations/events. Commands include request identity, actor/control context,
and expected observation revision. Retry handling must not execute actions twice.

Attaching sends a complete actor-specific observation snapshot, followed by
ordered updates at meaningful simulation boundaries. Updates include an
actor-stream sequence number, simulation time, disclosed state changes,
perceived semantic events, and control transitions such as input required,
travel interrupted, or death. Stream numbering is per observation stream, not a
global counter that reveals other actors' activity. New attachments reset the
stream through an explicit snapshot.

A single action or travel request can generate multiple updates. Clients do not
poll to discover changes. Delivery and rendering are independent: a client may
animate, summarize, or fast-forward without influencing simulation outcomes.
Slow clients use bounded queues and explicit resynchronization rather than
unbounded buffering. Reconnection resumes a retained stream or requests a fresh
snapshot. Snapshot publication and subsequent updates must have a consistent
boundary.

The server caches each actor's observation and exact projected scene at the
committed boundary. Revision comparison, navigation refresh, and publication
reuse that derived work where valid. Candidate views are published only after
persistence admission; region transitions invalidate pre-transition views.
Caches are absent from saved state and rewind boundaries. A broadcast shares
disclosed actor state across watchers while each connection retains its own
sequence, delta base, and control state. Private annotations remain separately
filtered by actor and user.

ASCII presentation receives ordered disclosed updates, not whole historical-memory
copies. A bounded channel backpressures its dedicated connection worker; the
native loop limits update work per turn before handling input. Every received
observation still refreshes memory, including intermediate views in a burst.
Shared-client validation finishes before mutation; snapshot branch boundaries
control memory retention. Slow-server-stream failure still requires relaunch
and a fresh snapshot, rather than silently skipping missing state.

Entity references are stable for disclosed entities. Names, properties, contents,
interaction affordances, and events must not reveal undiscovered facts. Clients
never receive a serialization of the whole world. Both clients can observe, with
one active controller per actor initially; control transfer is explicit and atomic.
Server-granted spectator accounts are restricted to attachment, snapshots, and
permitted history. They receive live actor-perspective actions/results but cannot
control, act, annotate, or invoke future wizard mutations. A player's `--observe`
startup option is separate from this enforced permission boundary.

Asset palettes are separate, independently revisioned messages on this
same connection. They forecast top-level asset IDs from broad themes within the
player's preload horizon, without disclosing entity instances. Attachment and
reconnect send a full palette; subsequent deltas and independently requested
snapshots need no acknowledgements. Clients resolve assets and dependencies,
manage retention, and use fallbacks/retry for unexpected assets. Palette state is
recomputed after loading and is not authoritative gameplay state.

## Annotations

The action/event history also contains sparse, non-simulating annotations from
users, frontends, and trusted backend components. Notes have server-stamped
provenance, branch identity, actor scope, a state or history-entry anchor, and
an explicit private or actor-visible audience. Notes do not advance time or action
revisions. Live updates, history pagination, and durable replay preserve the same
visibility rules. See the [protocol guide](protocol.md) for the implemented format.

## Language and interactions

The text client parses deterministic commands such as `put the key in the chest`.
The backend supplies perceived nouns, aliases, relationships, descriptions, and
interaction vocabulary; the client submits structured actions with entity
references. Ambiguity prompts before time advances. The backend validates every
action against current state, even if previously advertised as available.

Containment, inventory, equipment, doors, locks, and object properties are explicit
world relationships. Implement a small coherent interaction set first.

The [item core](items.md) supports stack quantities, pickup/drop/inventory,
archetypes and instance overrides, and multiple items per cell. Only marked-stackable items
with matching relevant properties merge. Keep hidden identity distinct from
per-character identification and deterministic per-game appearances; clients
receive only known facts. Equipment, item use, capacity, containers, and locks
are later extensions.

## Travel

[The backend travel slice](travel.md) is implemented for known cells, with ASCII
`_` selection and mouse-click destinations. Actor navigation knowledge is rebuilt
from perceived connections at committed boundaries and retained through replay;
clients still receive no topology. The text client interprets visible
destinations and composes travel with optional pickup; see
[the adventure interface](text-adventure.md). Active jobs never resume
automatically after restart.

One thread owns the session and [runs play until it needs a client's
input](run-until-blocked.md): AI turns and journey steps run back to back, and
requests are handled between actions. No timer advances the game. The server
executes travel toward known destinations as a sequence of ordinary actions. It
resolves and publishes each completed step without disclosing unresolved route
steps or future outcomes. Clients choose how to present completed progress,
including animation or slower pacing, but cannot affect simulation time. Threats,
damage, blocked paths, newly perceived hazards, arrival, and being thrown off
course interrupt travel; only the server ends a journey. Navigation cannot use
undiscovered terrain.

## Durable place knowledge

Perceived anchors become character-owned remembered points, independent of region
membership. The simulation stores names beside navigation in shared source-region
maps, and learns only from authoritative perception boundaries. Deterministic
mnemonics name unnamed hints by seed and discovery order, never by hidden
contents; a hint the scenario names is learned by that name, and each name
records its origin.
Protocol observations carry opaque cell keys and names, without reconstructing
unseen geometry. Rename is a controller-only, revision-checked, free journal
command and rewind boundary. Checkpoints share remembered name maps across
boundaries. Client lists derive visibility only from the current disclosed scene;
full map persistence and offscreen destination selection remain deferred. See
[durable place knowledge](place-knowledge.md).

## Persistence and history

The server encodes only newly accepted records and admits them to a bounded
queue before publishing state. A separate worker appends atomic SQLite batches;
normal acknowledgements do not wait for disk. Explicit save, graceful shutdown,
and wizard enablement retain durability barriers. Restart rolls back unsaved
play consistently, including receipts and branch history. See
[background saving](background-saving.md) for the exact contract and limitations.

SQLite is a server-only I/O dependency with a bundled native implementation.
Simulation remains deterministic and independent of storage and wall-clock time.
Framed records add application versioning, checksums, and save identity to SQLite's
transaction boundary. There is no historical save importer or rules implementation.

Startup validates the immutable replay base and retained records, restores the
latest [checkpoint](checkpoints.md), and simulates only its tail. Snapshot selection,
history retention and journal rotation share a SQLite transaction. Deterministic
backend snapshot types use Serde without I/O or clocks; they never enter protocol
messages. Complete history remains in memory for existing queries and retries;
ordinary command candidates contain only decision state, revisions and shared
rewind boundaries. Retained records and receipt indexes stay owned by the engine;
a single new record is admitted before publishing the candidate. World collections,
items and actor navigation use deterministic copy-on-write ownership. The private
item store owns all item insertions, removals, and edits; ground-location and
inventory-owner indexes cannot be mutated separately. Ground keys are resolved
region-local locations, including locations reached through portals. Observation
and stack merging query these buckets in deterministic identity order. Indexes
are derived backend data, rebuilt from authoritative items on checkpoint restore,
and omitted from checkpoints and client messages. Navigation
shares maps by source region, so discovering a local connection does not copy all
remembered cells. This sharing
never crosses the protocol boundary. Format-6 checkpoints also pool equal source-region
navigation maps across retained boundaries. One restore-scoped context rebuilds
world, navigation and item ownership from decoded pool indexes across the current
game and retained rewind states; equal body definitions share immutable storage
by complete value, including cell order. Restored games retain copy-on-write
ownership after the context is dropped. Game validation and serialized fields
are unchanged. Detached-region records use a separate decoding path and are not
eagerly loaded by checkpoint restoration.
Wizard undo preserves abandoned branches and
can rewind the last 128 decision boundaries; normal play exposes no undo.

Future scenario games persist activated regions and their full simulation state,
while unactivated areas remain references to pinned scenario/generator inputs.
Frozen activated regions can remain serialized outside memory; reload the saved
active preload set first. Persist RNG, velocities, AI memory, action progress,
and identification so continued gameplay is equivalent after recovery. Require
fresh input after recovery, preserving valid saved progress rather than
automatically restarting interrupted work. Exact dependency selection and future
multiple-version support are design goals; current pre-release formats still
reject historical saves. See the [persistence requirements](game-design-plan.md#region-activation-and-persistence).

## Wizard mode

[Wizard mode](wizard-mode.md) is a server-enabled development capability for
placing basic actors and ground objects, teleporting actors, and bounded rewind.
[Geometry setup](portal-geometry.md) adds rooms, rotated passages, walls and explicit
vertical links. Richer creature/object archetypes remain future work. The
server owns authorization and validates privileged commands; clients only expose
the capabilities granted to their connection. Wizard commands use opaque text
interpreted only by the
server, separate from ordinary actor actions and annotations. The private journal
retains structured operations; public history contains sanitized summaries.

Enabling wizard mode permanently marks the entire game lineage as a wizard game.
The marker is durable before privileged commands can execute and survives replay,
restart, save copies, rewinds, and forks, including rewinds before enablement.
Disabling privileged access never restores normal-game status. Every frontend
displays this status, and normal-play results exclude wizard games.

Privileged mutations must preserve world invariants and deterministic replay.
The server records their inputs and results, rebuilds affected observations, and
publishes a fresh snapshot boundary when rewind changes time or branch. Ordinary
observers keep actor-specific disclosure; privileged inspection, if added, needs
its own authorized response rather than widening normal observations. Only the
current formats are supported; older saves and rulesets are rejected rather than
migrated. The last 128 chronological decision boundaries are rewindable; older
branch history remains readable. Wizard authority is global to the game and uses
a distinct server-configured credential. Text provides privileged commands; ASCII
displays wizard status and follows explicit setup/rewind snapshots.

With scenario packages, wizard lineage and validation status remain distinct.
Enabling wizard authority still permanently marks the lineage; only mutations
that break scenario validation mark the running state unvalidated. Journal those
mutations and status changes. Wizard games may start from packages or saves;
ordinary test scenarios use the same package format as normal games.

## Authored package boundary

The server owns TOML parsing, file integrity, validation artifacts, and immutable
scenario inputs in saves. `tor-scenario` explicitly validates complete bounded
authored packages. Construction calls deterministic simulation/world APIs; these
crates do not acquire filesystem dependencies. The package schema is independent
of wizard commands and client protocol types. Clients continue receiving only
actor-specific observations. See [scenario packages](scenario-packages.md).

## Initial content and deferred decisions

Use original code and content with NetHack as a gameplay reference. Validated
self-contained scenario packages describe worlds/zones, region-local portals,
geometry, gravity, anchors, authored placements, themes, and objectives. An
explicit offline validator binds content hashes and exact dependencies to
author-controlled major.minor versions. Any authored change requires revalidation;
startup performs lightweight checks and refuses unvalidated inputs by default.

Authored scenarios provide the first explore, fight, retrieve, escape loop: a named exit cell optionally requires a specific authored item.
Scenarios configure objective disclosure and post-victory continuation. Actors
differ by controller assignment, not separate player/mob types. Initial combat
uses timed d20 attacks versus physical defense, typed HP damage, immunity/flat
reductions, and search/attack/flee AI using perception and expiring memory.
Death leaves an ordinary corpse item and separately dropped inventory.

Later deterministic generation uses fixed neighboring structural metadata and
activates within the preload horizon. Activated results persist permanently;
distant regions freeze all actors/effects. Reactivation batches deferred updates
deterministically before normal scheduling. Theme palettes describe possibilities
even before content is instantiated. The [design plan](game-design-plan.md)
contains acceptance intent and open considerations. Equipment, containers,
locks/keys, item use, and richer identification gameplay follow the item core;
hunger, ranged combat, multiplayer, and the 3D frontend remain later work.

The ASCII client uses minifb for a native pixel-buffer window and font8x8 for
bitmap glyphs, with Win32 and X11 backends. UI input/presentation stay separate
from a background connection worker; the worker applies the shared validated
ClientState and never exposes world/simulation internals. Window size and redraw
rate do not affect game time. See the ASCII guide for display/test requirements.
Choose a 3D renderer after ASCII and text validate the protocol. Avoid speculative
rendering dependencies in the simulation or wire format.
