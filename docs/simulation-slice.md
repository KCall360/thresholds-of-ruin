# First simulation slice

The `tor-simulation` crate now implements the first in-memory portion of Milestone
1. It uses only the standard library and `tor-world`; it performs no I/O, rendering,
wall-clock access, or implicit random sampling.

## Scenario and geometry

`Game::two_room(seed)` creates an Entry chamber and Gallery, each 5 x 3 x 1 cells,
connected by two directed passages. Start an actor at region 1, position (1,1,0).
A token is on that cell; a stone tablet is in the initially undisclosed Gallery.
The seed chooses copper, silver, or iron for the token. Layout generation and
random gameplay mechanics are deferred, so this fixture needs no PRNG state.

`World` validates unique region IDs, in-bounds passage endpoints, boundary exits,
and unique (source cell, direction) connections. Cardinal and vertical movement
use checked coordinates. Crossing a passage changes the region-local position;
it does not require a global spatial embedding. Passages can also loop back to
the same cell. Occupancy only blocks another actor.

Regions are unobstructed volumes at this stage. Single-cell passages translate
positions; rotated orientation, larger apertures, interior terrain, gravity,
support, stairs, and door entities still need their own rules. The geometric
vertical-step primitive is not yet a walking/climbing/falling simulation. Passages
are not door entities and do not constrain where future doors can be placed.

## Actors and time

No actor exists until `spawn_actor` is called. Each has a stable ID, position,
visited-region set, readiness time, and positive base action duration. Scenario
setup methods are backend operations and must not be exposed as player commands.
If actors are added during a run, replay must record those external inputs too.

`next_actor` selects the earliest readiness time, breaking ties by actor ID.
`act` rejects requests from any other actor. All actors share these rules; there
is no global player or special player-only inventory.

An action takes effect at the current tick and incurs recovery time:

| Action | Recovery time | Preconditions |
| --- | --- | --- |
| Move | Actor's base duration | Valid geometry; destination free of other actors |
| Take | Half base duration, rounded up | Item lies on the actor's cell |
| Wait | Actor's base duration | Actor is scheduled to act |

After each action the simulation advances to the next actor's readiness time.
Several actors can act at the same tick. Taking an item at tick 0 with a base
duration of 100 leaves a lone actor ready again at tick 50. With another actor
ready at 0, that actor acts next at 0 instead.

Observations are free reads. All rejected actions are free in this slice and
leave the complete state unchanged. Future failed in-world attempts can have
explicit costs. Time and identity exhaustion fail before mutation rather than
wrapping. Positive action durations prevent a zero-cost action loop.

`ActionOutcome` reports authoritative action facts and the next actor/time. It is
not a wire message: the server must filter events for each observer before
publishing them. Streaming and controller ownership are not implemented yet.

## Perception and inventory

The temporary perception rule reveals the current fully lit room on the actor's
elevation, visible actors, local exits, their own inventory, and visited place
names. Exits do not reveal unvisited destination IDs or contents. Inventory is
represented by item ownership, so one item cannot appear in two inventories.

Seeing an item elsewhere in a room does not make it reachable. Taking hidden,
unknown, already carried, and out-of-reach items produces the same unavailable
error. Visited place knowledge changes when an actor enters a room, not when a
client decides to query it. Visibility across portals, occlusion, sound, and
remembered object locations belong to the perception milestone.

## Validation and remaining work

Cross-crate acceptance tests cover observing, taking a token, crossing the
doorway, carrying inventory into the next room, and returning. Other cases cover
multiple actors, stable scheduling, seed-selected content, reproducible state and
event traces, hidden information, reach, duplicate pickup, occupied cells,
invalid requests, and numeric boundaries.

The existing protocol's actor ID is a separate wire type. A future server adapter
will explicitly translate between protocol and simulation types. Clients must
continue to depend on the protocol, never on world or simulation crates.

The next PR implements the protocol/server attachment and command path, with
per-actor snapshots and streamed updates. Actual frontend applications and
save/resume remain subsequent parts of Milestone 1.
