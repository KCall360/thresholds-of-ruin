# Backend travel and ASCII destinations

The current rules support travel to an actor's known cell,
identified by its opaque disclosed key. This is a server capability, independent of place hints,
region names, and frontend language.

## ASCII controls

Press `_` to select a destination. Arrows/HJKL/YUBN move the cursor, `<`/`>` change its
relative height, Enter starts travel, and Escape cancels selection. Selection is
free and clears on a changed observation, branch, or lost control. Alternatively,
left-click a perceived or remembered walkable map cell to travel immediately. Clicks use the same map
layout as rendering, including stair panels and resized-window letterboxing.
Walls, cells never disclosed, known closed doors, and panels outside the map are
not destinations. Current observations override stale remembered contents.

Only the server ends a journey; there's no way to cancel one. While a journey
is shown, any key shows the rest of it at once, and `[` and `]` show steps more
slowly or quickly. The map shows travel status and the number of completed
steps. Spectators can observe progress but cannot start travel. The [text adventure interface](text-adventure.md) now interprets directions and
object intentions through this backend travel. Its explicit `--script` mode
preserves one-cell commands and diagnostic output.

ASCII selects currently perceived and aligned remembered map cells, using the
last disclosed opaque key. The backend accepts these previously perceived keys,
so frontends can use travel underneath higher-level
intentions such as going east to a location or approaching a key. The travel service itself never performs a manipulation on arrival; the text
client can issue a separately validated pickup after checking interruption state.

## Knowledge and routing

The backend remembers each actor's perceived cells and observed directed
connections at initial setup and committed action/setup boundaries. It records a
connection only when its endpoints occur next to one another in the resolved
scene with matching orientation. Merely knowing two cells does not reveal a link.
Opaque keys are stable across restart; physical locations and transforms stay
private. Navigation memory is reconstructed by deterministic journal replay and
restored by rewind. It is distinct from connection-local frontend display memory.

Deterministic minimum-tick search uses only the remembered graph and terrain.
Diagonals are composed from disclosed cardinal connections
through at least one remembered clear side. Their cost is `ceil(base × √2)`.
Ties use stable discovery order: north/east/south/west/up/down, then
northeast/southeast/southwest/northwest in the actor's frame.
Orientation is part of the search state, including rotated joins and self-loops.
Stairs require a disclosed explicit connection. Neither hidden current terrain
nor unknown connections can supply a shortcut. Stale routes are validated through
ordinary movement one step at a time; a blocked attempt stops without rerouting.

At action boundaries, navigation refresh runs only when world geometry or the
observer's scene changes. Entity motion, disclosure and readiness still receive
their normal observation comparisons. Same-tick physics scheduler handoffs do not
refresh an unchanged map; falls, movement and door changes refresh the observers
whose navigation inputs changed.

Navigation refresh indexes the shared scene by its unique projected offsets, then
compares six proposed links per source cell. Stable source grouping preserves
the chart's proposal order for repeated portal occurrences and builds only actual
changes, avoiding duplicate deletion/reinsertion proposals. Hash lookup is used
only for membership; ordered sources preserve deterministic updates. Existing edges are read
through a source-cell range in their region bucket, so refresh does not scan all
remembered topology. Undisclosed endpoints retain stale links, while disclosed
terrain edits are checked against the ordinary world step rules. Reference-refresh
tests compare dense rotated populations, repeated rotated portal occurrences,
lighting changes, stale obstacles, and
retained prior boundaries.

Sight and navigation share the world's region bounds and exit-plane geometry.
Area joins retain one geometry proof plane per axis, direction and coordinate,
while the ordinary topology resolver retains every individual endpoint. This
avoids repeated plane checks without changing routing at passage boundaries.
A read-only adjacency batch reuses those facts for ordinary steps, retaining the
world resolver for joins and stairs. Opacity is captured once from the shared
perception scene; both endpoints must be disclosed and walkable before a link is
learned. The batch borrows the world, so edits cannot invalidate it while in use.
Compact perception charts use a dense offset index for neighboring-cell lookup.
It allocates at most eight slots per disclosed cell and 262,144 slots in total;
widely separated offsets use a sparse index instead. Holes remain unknown, and
both indices preserve the same occurrence precedence and rotations. Indexing
does not change the order in which navigation knowledge is updated.

## Execution and interruption

Accepting travel costs no action time, records an actor-visible request, and
acknowledges it immediately. The backend admits each planned movement to the
simulation-owned queue without advancing time. A private journal record links
its identity and ordinal to the accepted journey; no client RPC is fabricated.
The saved movement context fixes its region-local destination and portal frame.
The shared simulation executor revalidates and applies movement with the actor's
normal action cost, revision, event, and observer update. Only its committed
result advances the route and completed-step count. No planned route or future
outcome is sent to clients.

Control and disclosed hazards are checked before admission and again after the
mailbox pass before execution. Ending a journey settles any queued step through
linked backend cancellation. Rejected persistence admission preserves the original
route and queue identity; rejected cancellation blocks further simulation until
settlement succeeds. Published observation output cannot resurrect a stopped job.

The server [runs play until it needs a client's input](run-until-blocked.md),
so a journey takes a step whenever its actor is next to act. When another
player's actor is next, the journey waits for its turn and then continues.
Clients space the steps out on screen. A journey ends on:

- Arrival at the requested cell.
- A blocked move or save-queue admission failure; no further steps are attempted.
- A newly perceived potential hazard relative to the trip's starting view.
  Other actors conservatively count as potential hazards, including nonhostile
  actors. Repeated views of your own actor do not count.
  New ordinary terrain, ground items, and place hints do not interrupt travel.
  Hazard detection stops before another step; arrival takes precedence when the
  revealing step also reaches the destination.
- Being displaced or struck while moving (`decision_required`).
- Controller release/disconnect or an accepted wizard setup/rewind.

These are checked before every step, so whatever happened while a journey waited
for its turn, such as another player opening a door, can end it. While a journey
runs, the controller's action and travel commands are rejected with
`actor_busy`; saving doesn't stop it.

Positive HP loss interrupts travel at the next service action boundary; fully
resisted damage does not. Travel never turns movement into an attack. Actors
already visible when a trip starts do not count as new hazards, but still block
movement normally. Wizard changes stop active jobs even when their changes are
elsewhere. Traps and additional dangerous-terrain classifications remain future
work. See [dungeon gameplay](dungeon.md) for damage and attack interruption, and
[narration and stream recovery](narration-and-recovery.md) for slow and broken
connections.

Travel status and active jobs are session-local. Restart restores completed moves,
request receipts, and actor navigation knowledge but never resumes travel. Before
clients can acquire control, startup terminally cancels any restored pending step;
it fails if that cancellation cannot be admitted to persistence. Rewind
clears travel before publishing the new branch snapshot. Terminal status lasts
until another trip or branch reset during the current server session; terminal
reasons are not durable history records in this slice.

## Protocol

Send an ordinary branch-checked command envelope:

```json
{"type":"command","context":{"stream":{"stream":"<attachment stream>","epoch":1},"readiness_revision":4},"branch":"<current branch>","command":{"type":"travel","expected_revision":12,"destination":"<known opaque cell key>"}}
```

The server requires control, readiness, a current revision, and a known traversable
route. Unknown targets and unavailable routes share a generic error. Exact retries
of an accepted request return its retained receipt without restarting the job,
including after reconnect, restart, or a later branch change. Role checks precede
receipt lookup.

Snapshots contain nullable `travel`. Ordered `travel` updates carry `status` and
an optional history `entry` for the accepted request. Status contains the travel
entry `id`, opaque `destination`, `completed_steps`, and `phase`: `active`,
`arrived`, `blocked`, `hazard`, `decision_required`, `control_lost`,
`world_changed`, or `failed`. These updates do not alter action revisions.
Headless frames expose the same status for scripted clients.

## Compatibility and verification

Checkpoint navigation pools retain shared region ownership. Remembered cells
store their region once per pool entry and group explicit z/opacity entries in
ordered x/y columns. Plain edge masks use the same coordinate columns; unusual
links retain full endpoints and rotations. Each decoded cell has an explicit
encoded entry. Decoding rejects empty columns, duplicate or unsorted
coordinates and invalid opacity values; ordinary navigation validation still
checks world bounds. This preserves stale terrain knowledge while limiting the
persistence cost of the expanded perception range.
Restore contexts reuse navigation validation only for the same immutable decoded
navigation/world pair; actor membership remains checked in every restored state.

Only the current ruleset is supported. Start a fresh game after an incompatible
revision; saves are not migrated.

Focused tests cover remembered routing, hidden shortcuts, rotations, stairs,
cycles, stale terrain, free rejection, deterministic replay/rewind, receipts,
permissions, busy rejection, harmless discoveries, newly seen actors, waiting
for and being interrupted by another player, and control/lifecycle interruption.
`scenarios/tests/travel-*` supplies ordinary validated setups for actual
server/text/headless/native ASCII process acceptance. Ordinary non-wizard travel
is tested separately. Native keyboard and mouse events exercise `_` selection and
click travel, alongside presentation-model and automated window-input coverage.
The existing Windows/Linux CI discovery runs these in debug and release.

The hazard-only interruption policy governs session interruption/status behavior.

[Doors](doors.md) add explicit barriers. Travel never opens a closed door; known
closed doors exclude routes, and stale open-door routes stop on blocked movement.

Travel steps follow the same [background-save contract](background-saving.md)
as other actions. An acknowledged unsaved tail can be lost after a crash.
