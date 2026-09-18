# Architecture

## Scope

One authoritative backend supports graphical ASCII, Zork-style text, and later
immersive 3D clients using the same protocol. A game can continue across client
switches. Initial gameplay is single player, with explicit actor identities and
controllers so multiplayer can be designed later. Windows first; Linux portable.

This document records intended architecture. Implemented behavior is described in
[the simulation slice](simulation-slice.md) and [the server protocol](protocol.md).
The [text frontend](text-client.md) is playable. Richer perception, graphical
frontends, and the remaining milestones are planned.

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

Portals connect apertures and transform coordinates and orientation. Initial
transforms allow translation and quarter-turn rotations around the vertical
axis. The world needs no consistent global embedding: overlapping regions and
rooms larger inside than outside are supported conceptually. Multiple elevations,
stairs, and vertical movement are part of the first dungeon. Arbitrary gravity
and reflections are deferred.

Doors are interactive world entities, independent of portals. A door may occupy
an interior opening or obstruct a portal aperture. Its closed or locked state
does not define portal topology. Movement and perception evaluate terrain and
barriers along their paths.

Named places and waypoints are semantic concepts separate from geometry regions.
A room may span regions; a region can contain several meaningful places. Text
navigation refers to known places rather than exposing geometry partitions.

Visibility follows portal paths with explicit range limits and cycle handling.
The backend owns visibility, appearance facts, memory, sound disclosure, and
hidden information. Exact sound propagation rules are deferred.

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

## Annotations

The action/event history also contains sparse, non-simulating annotations from
users, frontends, and trusted backend components. Notes have server-stamped
provenance, branch identity, actor scope, a state or history-entry anchor, and
an explicit private or actor-visible audience. Notes do not advance time or action
revisions. Live updates, history pagination, and durable replay preserve the same
visibility rules. See [protocol version 1](protocol.md) for the implemented format.

## Language and interactions

The text client parses deterministic commands such as `put the key in the chest`.
The backend supplies perceived nouns, aliases, relationships, descriptions, and
interaction vocabulary; the client submits structured actions with entity
references. Ambiguity prompts before time advances. The backend validates every
action against current state, even if previously advertised as available.

Containment, inventory, equipment, doors, locks, and object properties are explicit
world relationships. Implement a small coherent interaction set first.

## Travel

The server executes travel toward known destinations as ordinary actions in
bounded batches. Clients choose when to request more and how to present progress.
Threats, damage, blocked paths, relevant discoveries, arrival, and decisions
interrupt travel. Cancellation takes effect at an action boundary; already
executed actions cannot be cancelled. Navigation cannot use undiscovered terrain.

## Persistence and history

Persist periodic snapshots and a journal of accepted authoritative actions and
external inputs. Include RNG state, scheduler state, IDs, rules/content versions,
and checksums. A seed alone is insufficient. Saves need atomic replacement and
recovery from interrupted writes. Exact replay requires compatible simulation
and content versions; arbitrary cross-version replay is not promised.

Developer undo reconstructs an earlier state; a new action preserves the old
branch and creates a new one. Retained history is limited by available storage,
not held entirely in memory. Player-facing time travel is deferred. Normal play
enforces persistent permadeath and exposes no undo. Local file manipulation is
outside that guarantee.

## Initial content and deferred decisions

Use original code and content with NetHack as a gameplay reference. Start with
hand-authored scenarios, then seeded procedural generation. First dungeon:
several rooms across two elevations, stairs, an unusual portal connection,
doors/keys/containers, inventory/equipment, melee, two enemies, death, and an exit.
Hunger, identification, ranged combat, multiplayer, and a 3D client follow later.

Choose the graphical ASCII framework when implementing that client. Choose a 3D
renderer after ASCII and text validate the protocol. Avoid speculative rendering
dependencies in the simulation or wire format.
