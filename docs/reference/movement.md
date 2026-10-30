# Movement, doors and travel

The simulation owns movement and manipulation rules. Frontends choose commands
and destinations; the server routes and executes travel using the acting
character's knowledge. Controls live in the [text](../play/text.md) and
[ASCII](../play/ascii.md) guides. Bodies, frame transforms and gravity are defined
in [geometry](../architecture/geometry.md).

## One movement action

Eight horizontal directions are available. Cardinal movement uses the actor's
normal movement recovery. A diagonal is one atomic move with recovery
`ceil(base_ticks × √2)`: 100 ticks becomes 142, and 75 becomes 107. Checked integer
arithmetic avoids floating-point drift; overflow rejects before mutation.
Up/down requires an explicit vertical connection or an effective flying grant.
Flight permits open vertical paid movement; stationary denies all voluntary
translation, including self-step abilities, without preventing imposed motion.

The destination must be traversable and unoccupied. At least one side route must
be clear: northeast needs north **or** east. Walls, closed doors and actors
obstruct sides; ground items do not. There is no occupied intermediate position,
extra action or intermediate observation. A rejected move consumes no time.

A side route follows actual cardinal connections and composes their rotations.
When both routes are usable, they must agree on destination and final frame.
Conflicting non-Euclidean routes make the diagonal unavailable. Sight-only
material-rim projection never creates movement connections.

For a multi-cell body, one ordering of the two component steps must carry the
**whole body** clear; if both work they must agree. This walking rule also applies
in gravity. Other simultaneous diagonal translations, such as sliding while
falling, conservatively check all component orderings. Walking additionally
requires support where gravity rules apply. Every travel step performs the same
current-body validation as ordinary movement; a reference-cell route alone does
not guarantee clearance for a large body.

The frontend names northeast as `ne` or `northeast`; wire direction values use
`north_east` (and the corresponding other diagonals). Wizard joins use
cardinal/vertical faces, not authored diagonal portals. ASCII `d` drops an item;
`>` descends. Keep full key mappings in the client guide.

### Moving while held

For an ordinary human movement command, the server resolves a current incoming
linked hold into a normal paid Escape attempt against the oldest active link.
The requested direction does not move the actor or become a follow-up step,
even when Escape succeeds. Each further attempt is a new command; explicit
Escape can select a different hold. Ordinary busy/context checks still apply.
This input convenience does not change simulation movement, autonomous actions,
travel steps or directional cancellation. See
[linked holds and Escape](protocol.md#linked-holds-and-escape).

## Doors

Doors are independent entities, including doors within a region and on join
endpoints. They occupy a one-cell-wide vertical column with authored `height`:
the base and `height - 1` cells above it. All cells share the same state and
identity. Opening or closing changes obstruction, not topology.

Closed doors block movement and sight but are not solid wall terrain. The door
itself can be perceived; anything beyond requires another unobstructed sightline.
Open doors obstruct neither. A short door can be seen over by a taller observer.
Package validation rejects a door that leaves its continuing walled doorway open
above it. A low wall under open space is allowed. At a join with no flanking
walls in that region, correct height remains the author's responsibility.

### Manipulation rules

- Open and close use the actor's normal movement/wait recovery duration.
- Cardinal and diagonal manipulation use the one-clear-side reach rule and
  resolve rotated joins. Standing inside the door is not standing beside it.
  The closed target may be opened; a side door must be open to allow reach.
- Closing fails if any door cell contains an actor or a ground item.
- Unknown, hidden, out-of-reach, obstructed or already-in-that-state requests
  fail without time or mutation. Examining and clarification are free.
- Walls, door placement and body placement cannot create forbidden overlaps.
  A body occupying any part of the door column matters, not only its base.

Each currently perceived door occurrence includes its opaque target, perceived
appearance and state. Only its base cell supplies manipulation reach and opaque
keys for currently perceived approach cells, so a tall door is offered once.
Approach candidates must occur in the resolved scene. They disclose no private
region, transform, hidden cell or future route.

Normal text `open door`/`close door` may travel to a disclosed standing cell, then
submit a separately validated action. It rechecks target, reach, state and
interruptions on arrival. The nearest displayed candidate is a conservative
heuristic: it can fail if that candidate has no known route, without silently
searching unknown space or trying every other candidate. ASCII directional door
commands and text `--script` act immediately without approach.

For structured actions, copy the received door target into `set_door`; never use
an internal numeric door identity. The current target format and command
envelopes belong to the [protocol reference](protocol.md), and the
[headless interface](headless.md) describes submission. Displayed `#` selectors
are likewise based on received opaque targets.

### Door authoring

In an authorized wizard game:

```text
wizard door <region> <x> <y> <z> <open|closed> [height]
```

Height defaults to one. Structured `place_door` requires it explicitly. Placement
validates free space above the base and reports how much fits; it does not apply
the package-only check for a doorway left open above a short door. Closed
placement cannot cover actors/items and duplicate placement fails atomically.
Identity and state participate in replay and rewind. Public wizard history is
sanitized. See [wizard tools](../development/wizard.md) and
[scenario authoring](scenario-packages.md).

Locks, keys, destruction, transparent doors and doors wider than a single column
are not implemented. A wide join may use multiple independent doors.

## Travel

Travel targets a known cell through its actor-specific opaque key. It is
independent of place hints or region names. Acceptance records the request and
costs no action time; each executed step is an ordinary movement action. Travel
does not include an interaction on arrival: text may compose a separate pickup,
door operation or attack after rechecking its own goal.

### Knowledge and deterministic routing

The backend learns perceived cells and directed connections at initial setup and
committed action/setup boundaries. A connection is learned only when endpoints
occur next to one another in the resolved scene with matching orientation.
Knowing both endpoints alone does not establish a link. Knowledge persists
through replay and restores with rewind, separately from the clients' display
memory. Opaque keys remain stable across restart.

Minimum-tick search uses only this remembered graph and terrain. Diagonals use
remembered cardinal connections with at least one clear side. Equal-cost paths
preserve stable discovery order: north/east/south/west/up/down, then
northeast/southeast/southwest/northwest. Orientation is part of the search state,
including rotated joins and self-loops. Explicit stairs require a learned link.
Hidden current geometry never supplies a shortcut.

A stale route is checked one executed step at a time. A newly blocked step stops
the job without automatic rerouting. Known closed doors exclude routes, while a
remembered open door can stop travel when it is now closed. Ground items and
place hints alone do not obstruct a route. Character knowledge is historical,
not live knowledge of every offscreen cell.

Flying creatures can learn open vertical connections from the same disclosed
scene. Each connection remembers whether it required flight when disclosed;
ordinary vertical portals and stairs do not require it. Search filters that
remembered requirement against the actor's current capability, including for
offscreen connections. Capability changes refresh the current scene's movement
links without discovering hidden cells or rescanning unseen geometry. Retained
offscreen knowledge can become stale, and live whole-body execution still
decides whether a step is legal.

### Execution and interruption

The server admits each planned move into the simulation-owned intention queue.
A private journal record links its identity/ordinal to the journey; no client
RPC is fabricated. The saved context fixes its region-local destination and
portal frame. Execution revalidates it, and only a **committed result** increments
route progress and the completed-step count. Clients never receive a planned
route or unresolved future outcomes.

Control and hazard checks occur before admission and again after the mailbox
pass before execution. Ending a job settles an already queued step by linked
backend cancellation. Failed persistence admission preserves the original route
and queue identity; failed cancellation prevents further simulation until it can
be settled. Publishing an observation cannot resurrect stopped travel.

The job advances when its actor is next. Another human's turn can make it wait.
A journey ends on:

- arrival;
- blocked movement or save-queue admission failure;
- a newly perceived actor relative to the starting view, conservatively including
  nonhostile actors but excluding repeated sightings of the observer;
- positive HP loss, displacement or collision requiring a decision;
- controller release/disconnect or accepted wizard setup/rewind.

Hazards are checked before another step. If the revealing step also arrives,
arrival takes precedence in backend status, but the text client still suppresses
an intended follow-up when a new actor appeared. Actors already visible at the
start are not new hazards; they still block movement. Fully resisted damage
does not interrupt. New terrain, ordinary items and place hints do not count as
hazards. Traps already exist and their ordinary damage/displacement can stop a
journey. A separate travel-hazard classification for known traps and richer
dangerous terrain remains future work; the current route planner does not imply
such an avoidance policy.

While travel is active, other ordinary action/travel requests are rejected as
busy. Saving does not cancel it. Presentation pacing and fast-forward only
change how completed progress is shown. There is no ordinary player travel
cancellation command; queued-intention controls are a distinct mechanism.

### Lifecycle, retries and wire boundary

The server requires control, readiness, fresh command context/revision and a
known traversable route. Unknown destinations and unavailable routes return a
generic error rather than exposing hidden topology. Role checks precede retry
receipt lookup. Exact retries return the accepted receipt without restarting the
job, even after reconnect, restart or later branch changes.

Snapshots include nullable travel state. Ordered travel updates include status
and, for acceptance, its history entry. Status reports request entry identity,
opaque destination, completed steps and phase: active, arrived, blocked, hazard,
decision required, control lost, world changed or failed. These status updates
do not themselves increment the action revision. Use the exact enum spellings
and serialization rules in [protocol](protocol.md). Epochs and 64-bit revisions
are canonical decimal **strings**, not JSON numbers; copying an old numeric
travel envelope is invalid.

Active jobs and terminal status are session-local. Restart restores completed
moves, receipts and navigation knowledge but does not resume a job. Before
control can be acquired, startup terminally cancels any restored pending travel
step; startup fails if this cannot be admitted to persistence. Rewind clears
travel before its new snapshot. Within a session, terminal status lasts until
another trip or branch reset; terminal reasons are not durable history entries.
Ordinary background-save rules still apply to steps: acknowledged unsaved play
can be lost after a crash.

### Navigation implementation constraints

Refresh runs only when geometry or the observer's scene changes. It indexes
resolved occurrences, checks six proposed cardinal links per source and updates
in stable source order; hash indexes are membership aids, never output order.
Undisclosed endpoints keep stale links. Translation reuse is valid only when
identity, frame and opacity agree; abstract stair offsets remain conservative.

A dense offset index uses at most eight slots per disclosed cell and 262,144
slots in total; sparse scenes fall back to a sparse index. Holes remain unknown
and occurrence precedence is identical. Read-only adjacency batches borrow the
world and share region bounds/exit-plane facts, retaining the topology resolver
for joins and stairs. These are accelerations of the same learned-graph rule,
not access to hidden routes.

## Verification map

World/simulation tests cover corner masks, whole-body clearance, exact diagonal
timing and overflow, rotated/conflicting routes, door occupancy/reach, stale
knowledge, hidden shortcuts and deterministic weighted routing. Reference refresh
comparisons exercise dense rotated populations, repeated occurrences, lighting
changes and retained prior boundaries. Persistence decoding and restore details
belong to [persistence](../architecture/persistence.md).

Real-server/client suites include
[`test_diagonal_process.py`](../../scripts/test_diagonal_process.py),
[`test_doors_process.py`](../../scripts/test_doors_process.py), and
[`test_travel_process.py`](../../scripts/test_travel_process.py), using ordinary
validated `scenarios/tests/diagonal-*`, `doors-*` and `travel-*` packages as well as
authorized wizard edits. They cover native controls, disclosure, arrival hazards,
stale routes, spectators, retry, restart and rewind. Sources are
[`world`](../../crates/world/src), [`navigation`](../../crates/simulation/src),
and [`server travel`](../../crates/server/src).
