# First simulation slice

The `tor-simulation` crate now implements the first in-memory portion of Milestone
1. It uses only the standard library and `tor-world`; it performs no I/O, rendering,
wall-clock access, or implicit random sampling.

## Scenario and geometry

`Game::two_room_with_doorway(seed)` creates two 5 x 3 x 1 rooms connected
through a 1 x 1 hall containing an initially open wooden door. The hallway is
stored in the extra east column of region 1, with wall cells north and south;
region 2 remains the Gallery. Storage regions are not player-facing rooms.
Only the two room interiors have place hints, at local (2,1,0); the hall has none.

Start an actor at region 1, position (1,1,0), on the seeded token. The stone tablet
is at region 2 (2,1,0), seven eastward steps away through the open hall. Closing
the door blocks movement and sight through the hall. The seed chooses copper,
silver, or iron for the token. Layout generation and random gameplay mechanics
are deferred, so this fixture needs no PRNG state. Historical `Game::two_room`
and `Game::two_room_with_place_hints` layouts remain available for old-save replay.

`World` validates unique region IDs, in-bounds passage endpoints, boundary exits,
and unique (source cell, direction) connections. Cardinal and vertical movement
use checked coordinates. Crossing a passage changes the region-local position;
it does not require a global spatial embedding. Passages can also loop back to
the same cell. Occupancy only blocks another actor.

The [portal-geometry slice](portal-geometry.md) adds clockwise passage rotations,
opaque wall terrain and explicit up/down links for stairs. Horizontal connections
must exit a boundary; vertical links may start in the interior. Gravity and support remain pending; [independent doors](doors.md) now obstruct
sight and movement. Rectangular joins can cover multiple cells. Passages are not door entities
and do not constrain where future doors can be placed. Old-rule saves retain
the original vertical-step behavior.

## Actors and time

No actor exists until `spawn_actor` is called. Each has a stable ID, position,
visited-region set, readiness time, and positive base action duration. Scenario
setup methods are backend operations; [wizard mode](wizard-mode.md) exposes only
validated privileged operations through separate server authority and records
their inputs/results for replay. Ordinary player commands cannot invoke setup.

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
publishing them. The [server layer](protocol.md) now handles streaming and
controller ownership without changing these simulation rules.

## Perception and inventory

New games compute cells within eight Manhattan steps using integer rays through
rotated, potentially multi-cell joins. Walls stop movement and sight; vertical
sight follows explicit links. The backend projects visible occurrences into the
actor’s frame. Clients receive relative positions and opaque cell keys, never
internal region coordinates, names, bounds, or links. Orientation follows the
actor through rotated crossings, preserving input and presentation axes.
Inventory is represented by ownership, so an item cannot be in two inventories.

Seeing an item elsewhere in a room does not make it reachable. Taking hidden,
unknown, already carried, and out-of-reach items produces the same unavailable
error. Visited place knowledge changes when an actor enters a room, not when a
client decides to query it. Looking through a portal does not mark a place visited.
Clients retain separate last-seen cell contents, which may be stale. Existing
`two-room-v1` saves retain original whole-room perception; new saves use
`doorway-v8`. Sound remains later work. New server games add an initially
open door; older fixtures retain their original rules.

## Validation and remaining work

Cross-crate acceptance tests cover observing, taking a token, crossing the
doorway, carrying inventory into the next room, and returning. Other cases cover
multiple actors, stable scheduling, seed-selected content, reproducible state and
event traces, hidden information, reach, duplicate pickup, occupied cells,
invalid requests, and numeric boundaries.

The protocol's actor ID is a separate wire type. The server adapter explicitly
translates between protocol and simulation types. Clients must
continue to depend on the protocol, never on world or simulation crates.

The [server/protocol slice](protocol.md) implements attachment, commands, streamed
updates, and durable action/annotation history. The [text](text-client.md) and
[graphical ASCII](ascii-client.md) frontends now exercise this slice through actual
process tests, including cross-frontend control transfer and save/resume.

[Unnamed place hints](place-hints.md) add perceived cell anchors in protocol 6.
They carry no labels or boundaries. Shared memory retains last-seen hints; ASCII does not render them; text now uses them as described in
[the adventure slice](text-adventure.md). New saves use
`doorway-v8`; earlier saves retain their original rules.

[Backend travel](travel.md) adds protocol 7 and `travel-v5` for new games.
Earlier rules retain their behavior. The [text adventure interface](text-adventure.md)
now adds text travel and approach-then-pickup.
