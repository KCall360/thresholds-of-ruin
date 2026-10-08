# Simulation

`tor-simulation` implements the authoritative in-memory rules. It uses only the standard library and `tor-world`; it performs no
I/O, rendering, wall-clock access, or implicit random sampling. This page covers
the core actor, time, and perception rules and the built-in test fixture. Combat
and AI are in [dungeon gameplay](dungeon.md), items in [items](items.md), and
bodies and gravity in [physics](physics.md). Games normally start from an
authored [scenario package](scenario-packages.md).

## Built-in fixture and geometry

`Game::two_room_in_stone(seed)` creates two 5 x 3 x 2 empty room interiors
inside finite stone shells, connected through a 1 x 1 hall with two empty height
layers and an initially open wooden door. The hallway is
stored in the extra east column of region 1, with solid cells north and south;
both regions include an additional one-cell stone shell on every face.
Storage regions are not player-facing rooms.
Only the two room interiors have place hints, at local (2,1,0); the hall has none.

Start an actor at region 1, position (1,1,0), on the seeded token. The stone tablet
is at region 2 (2,1,0), seven eastward steps away through the open hall. Closing
the door blocks movement and sight through the hall. The seed chooses copper,
silver, or iron for the token. This fixture chooses its token deterministically from the seed; it does not
exercise the procedural generator or combat random stream. The simpler `Game::two_room`
and `Game::two_room_with_place_hints` layouts remain as diagnostic fixtures for
tests.

`World` validates unique region IDs, in-bounds passage endpoints, boundary exits,
and unique (source cell, direction) connections. Cardinal and vertical movement
use checked coordinates. Crossing a passage changes the region-local position;
it does not require a global spatial embedding. Passages can also loop back to
the same cell. Occupancy only blocks another actor.

The [portal-geometry slice](portal-geometry.md) adds clockwise passage rotations,
opaque wall terrain and explicit up/down links for stairs. Horizontal connections
must exit a boundary; vertical links may start in the interior. Gravity and
support are described in [physics](physics.md); [independent doors](doors.md)
obstruct sight and movement. Rectangular joins can cover multiple cells.
Passages aren't door entities and don't constrain where doors can be placed.

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
| Cardinal/vertical move | Actor's base duration | Valid geometry; destination free of other actors |
| Diagonal move | `ceil(base × √2)` | Clear destination and at least one clear side |
| Take | Half base duration, rounded up | Item lies on the actor's cell |
| Open/close door | Actor's base duration | Visible, reachable door; changed state; no obstruction when closing |
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

### Shared action extension points

`Game::act` now uses a private action module with separate preparation, effect
application and scheduler advancement. Preparation checks actor readiness, action
preconditions and timing overflow without changing game state. The prepared value
is consumed within the same uninterrupted call; no public API can retain it across
world changes. Effect application and scheduling have no fallible operations.

Immediate actions apply their effects at execution and then incur recovery.
Attacks implement saved preparation, interruption and resumption; see
[dungeon gameplay](dungeon.md). The server admits player, AI and travel intentions
to the shared simulation queue, which revalidates them when due. Admission is
distinct from completion. See [queued execution](run-until-blocked.md) and
[architecture](architecture.md#time-and-actions) for the current execution path.

The action-boundary behavior tests cover immediate effects and exact recovery for
both first and second actors, including diagonal movement, pickup and doors, and
verify that overflow rejects every action without mutation. Existing simulation,
server recovery/replay and actual-client door/travel tests exercise the unchanged
public path. Profiling continues to measure the complete call inside the server's
existing simulation phase, preserving workload and measurement definitions.

### Focused release profiling

The September 26 action-boundary comparison uses five cycles of the unchanged
`performance-v1` mixed trace, in memory with zero initial retained actions. The
small case has one region/actor; the large case has 256 regions/eight actors.
Each pair runs on the same Windows machine in release mode, without concurrent
task builds or tests during collection. Values below are p50/p95/maximum ms.

| Case | Version | Accepted samples | Simulation | Authoritative total |
| --- | --- | --- | --- | --- |
| 1 region / 1 actor | Before | 135 | 0.0017 / 0.2224 / 0.5232 | 0.5055 / 1.4741 / 2.1645 |
| 1 region / 1 actor | After | 135 | 0.0009 / 0.1191 / 0.2206 | 0.2303 / 0.4951 / 0.7163 |
| 256 regions / 8 actors | Before | 2,496 | 0.0010 / 0.0047 / 2.3798 | 0.0356 / 7.6378 / 17.8676 |
| 256 regions / 8 actors | After | 2,496 | 0.0004 / 0.0017 / 0.4016 | 0.0129 / 2.5157 / 4.0016 |

The report validator confirms exact ordered workload coverage and accounting.
Operation and byte totals match before/after in both cases. No regression appears
in these samples; lower timings do not establish a refactor-induced speedup because
host scheduling and CPU conditions are uncontrolled. These stationary traces do
not qualify persistence, fully explored map memory or native presentation, and do
not close deferred 3p findings. The
[archived manifest and summary](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs/measurements/action-foundation-2026-09-26) record the
reproduction commands, checksums, distributions, and operation totals.

## Perception and inventory

New games compute horizontal cells within eight Manhattan steps using symmetric
shadowcasting through rotated, potentially multi-cell joins. Walls stop movement and sight; vertical
sight follows explicit links. The backend projects visible occurrences into the
actor’s frame. Clients receive relative positions and opaque cell keys, never
internal region coordinates, names, bounds, or links. Orientation follows the
actor through rotated crossings, preserving input and presentation axes.
Inventory is represented by ownership, so an item cannot be in two inventories.

Seeing an item elsewhere in a room does not make it reachable. Taking hidden,
unknown, already carried, and out-of-reach items produces the same unavailable
error. Visited place knowledge changes when an actor enters a room, not when a
client decides to query it. Looking through a portal does not mark a place visited.
Clients retain separate last-seen cell contents, which may be stale. Sound
remains later work.
Server games add an initially open door.

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

[Unnamed place hints](place-hints.md) add perceived cell anchors without labels
or boundaries. Shared memory retains last-seen hints; ASCII doesn't render them;
text uses them as described in [the adventure interface](text-adventure.md).

[Backend travel](travel.md) supports known-cell destinations. The
[text adventure interface](text-adventure.md) supports travel and approach-then-pickup.
