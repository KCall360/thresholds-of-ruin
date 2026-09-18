# Architecture

## Scope

One authoritative backend supports graphical ASCII, Zork-style text, and later
immersive 3D clients using the same protocol. A game can continue across client
switches. Initial gameplay is single player, with explicit actor identities and
controllers so multiplayer can be designed later. Windows first; Linux portable.

This document records intended architecture. Implemented behavior is described in
[the simulation slice](simulation-slice.md) and [the server protocol](protocol.md).
The [text frontend](text-client.md) and [graphical ASCII frontend](ascii-client.md)
are playable. Richer perception, the 3D frontend, and remaining milestones are planned.

## Workspace boundaries

| Crate | Responsibility |
| --- | --- |
| tor-world | Region-local geometry, entities, containment, portals |
| tor-simulation | Rules, scheduler, perception, deterministic transitions |
| tor-protocol | Versioned commands, disclosed observations, semantic events |
| tor-server | Sessions, validation, networking, travel, persistence |
| tor-client-common | Connections and a model of disclosed observations |
| tor-client-ascii | Graphical ASCII input and presentation |
| tor-client-text | Deterministic language parsing, clarification, prose |
| tor-client-headless | JSON-lines scripted play and disclosed-state inspection |
| tor-test-support | Shared scenario fixtures and integration tests |

The world and simulation have no network, filesystem, rendering, or clock
dependencies. The protocol owns DTOs independent of internal simulation state.
The server translates authoritative state into actor-specific observations.
The client crates cannot depend on world or simulation crates.

## Geometry and barriers

Regions are bounded rectangular 3D volumes with integer local coordinates.
Cells are square horizontally; elevations use an integer z coordinate. Separate
regions make sparse allocation natural. Dense, chunked, or sparse cell storage
within a region is an implementation choice to measure later.

Clients know nothing about regions, portals, or their transforms. The backend
projects all visible occurrences into one actor-relative scene, including when
they span several regions. Opaque cell keys support memory without exposing
geometry identity. Movement uses the same observer axes across rotated joins.

Portals connect apertures and transform coordinates and orientation. Initial
transforms allow translation and quarter-turn rotations around the vertical
axis. The world needs no consistent global embedding: overlapping regions and
rooms larger inside than outside are supported conceptually. Multiple elevations,
stairs, and vertical movement are part of the first dungeon. Arbitrary gravity
and reflections are deferred.

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
for an initial place/direction heuristic. Richer grouping, persistent labels and
offscreen waypoint navigation remain future work.

Visibility follows portal paths with explicit range limits and cycle handling.
The backend owns visibility, appearance facts, sound disclosure, and hidden
information. Clients retain their own prior disclosed observations as remembered
knowledge, separately from the backend's current world truth; that memory can be
incomplete or stale. Exact sound propagation rules are deferred.

The [client-memory slice](headless-client.md) retains received cell
views in `tor-client-common`, separate from current state. Same-branch snapshots
preserve memory; new branches clear it. It lasts only for the connection and
does not infer views from known place names or history. [Portal sight](portal-geometry.md)
refreshes only currently visible cells; memory presentation in text/ASCII remains
later work.

## Time and actions

Integer simulation time and a deterministic scheduler support variable action
costs and actor speeds. Equal-time outcomes have stable ordering. The world
advances until player input is required, never because a frame or network delay
elapsed. Each actor uses the same action system.

Queries reveal already available information without consuming time. Physical
inspection and manipulation can be actions with costs. Invalid requests and
unresolved noun ambiguities do not consume time; in-world failed attempts follow
the relevant action rule. Multiplayer waiting and simultaneous input policies
are deferred.

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

Entity references are stable for disclosed entities. Names, properties, contents,
interaction affordances, and events must not reveal undiscovered facts. Clients
never receive a serialization of the whole world. Both clients can observe, with
one active controller per actor initially; control transfer is explicit and atomic.
Server-granted spectator accounts are restricted to attachment, snapshots, and
permitted history. They receive live actor-perspective actions/results but cannot
control, act, annotate, or invoke future wizard mutations. A player's `--observe`
startup option is separate from this enforced permission boundary.

## Annotations

The action/event history also contains sparse, non-simulating annotations from
users, frontends, and trusted backend components. Notes have server-stamped
provenance, branch identity, actor scope, a state or history-entry anchor, and
an explicit private or actor-visible audience. Notes do not advance time or action
revisions. Live updates, history pagination, and durable replay preserve the same
visibility rules. See [protocol version 9](protocol.md) for the implemented format.

## Language and interactions

The text client parses deterministic commands such as `put the key in the chest`.
The backend supplies perceived nouns, aliases, relationships, descriptions, and
interaction vocabulary; the client submits structured actions with entity
references. Ambiguity prompts before time advances. The backend validates every
action against current state, even if previously advertised as available.

Containment, inventory, equipment, doors, locks, and object properties are explicit
world relationships. Implement a small coherent interaction set first.

## Travel

[The backend travel slice](travel.md) is implemented for known cells, with ASCII
`_` selection and mouse-click destinations. Actor navigation knowledge is rebuilt
from perceived connections at committed boundaries and retained through replay;
clients still receive no topology. The text client now interprets visible destinations and composes travel with
optional pickup; see [the adventure interface](text-adventure.md). Active jobs never resume automatically after restart.


The server executes travel toward known destinations as a sequence of ordinary
actions. It resolves and publishes each completed step without disclosing
unresolved route steps or future outcomes. Clients choose how to present completed
progress, including animation or slower pacing, but cannot affect simulation time.
Threats, damage, blocked paths, newly perceived hazards, arrival, and decisions
interrupt travel. Player cancellation takes effect at an action boundary; already
executed actions cannot be cancelled. Navigation cannot use undiscovered terrain.

## Persistence and history

Persist periodic snapshots and a journal of accepted authoritative actions and
external inputs. Include RNG state, scheduler state, IDs, rules/content versions,
and checksums. A seed alone is insufficient. Saves need atomic replacement and
recovery from interrupted writes. Exact replay requires compatible simulation
and content versions; arbitrary cross-version replay is not promised.

Wizard-mode developer undo reconstructs an earlier state; a new action preserves
the old branch and creates a new one. Retained history is limited by available storage,
not held entirely in memory. Player-facing time travel is deferred. Normal play
enforces persistent permadeath and exposes no undo. Local file manipulation is
outside that guarantee.

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
its own authorized response rather than widening normal observations. Protocol
version 9 and save format 3 support the current interface; normal format-1 saves migrate
on open. New games use doorway-v8; existing saves retain their ruleset.
The last 128 chronological decision boundaries are rewindable; older
branch history remains readable. Wizard authority is global to the game and uses
a distinct server-configured credential. Text provides privileged commands; ASCII
displays wizard status and follows explicit setup/rewind snapshots.

## Initial content and deferred decisions

Use original code and content with NetHack as a gameplay reference. A world schema
defines a generation model and its content rules. A scenario defines a particular
starting world and may be fully authored, fully generated from a schema, or
authored with marked procedural-generation regions. Use deterministic scenarios
for integration tests and wizard-mode setups. Start with hand-authored scenarios,
then seeded procedural generation. First dungeon: several rooms across two
elevations, stairs, an unusual portal connection, doors/keys/containers,
inventory/equipment, melee, two enemies, death, and an exit. Hunger,
identification, ranged combat, multiplayer, and a 3D client follow later.

The ASCII client uses minifb for a native pixel-buffer window and font8x8 for
bitmap glyphs, with Win32 and X11 backends. UI input/presentation stay separate
from a background connection worker; the worker applies the shared validated
ClientState and never exposes world/simulation internals. Window size and redraw
rate do not affect game time. See the ASCII guide for display/test requirements.
Choose a 3D renderer after ASCII and text validate the protocol. Avoid speculative
rendering dependencies in the simulation or wire format.
