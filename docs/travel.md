# Backend travel and ASCII destinations

The current rules support travel to an actor's known cell,
identified by its opaque disclosed key. This is a server capability, independent of place hints,
region names, and frontend language. Protocol 11 is required. Save format remains 3.

## ASCII controls

Press `_` to select a destination. Arrows/HJKL/YUBN move the cursor, `<`/`>` change its
relative height, Enter starts travel, and Escape cancels selection. Selection is
free and clears on a changed observation, branch, or lost control. Alternatively,
left-click a visible floor cell to travel immediately. Clicks use the same map
layout as rendering, including stair panels and resized-window letterboxing.
Walls, undisclosed cells, and panels outside the map are not destinations.

During active travel, Escape requests cancellation instead of quitting. The map
shows travel status and the number of completed steps. Selecting with `_` again
requires cancelling the active trip first; a new valid click can replace it.
Spectators can observe progress but cannot start or cancel travel. The [text adventure interface](text-adventure.md) now interprets directions and
object intentions through this backend travel. Its explicit `--script` mode
preserves one-cell commands and diagnostic output.

ASCII selects currently visible cells. The backend also accepts previously
perceived cell keys, so frontends can use travel underneath higher-level
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
In diagonal-v11 games, diagonals are composed from disclosed cardinal connections
through at least one remembered clear side. Their cost is `ceil(base × √2)`.
Ties use stable discovery order: north/east/south/west/up/down, then
northeast/southeast/southwest/northwest in the actor's frame.
Orientation is part of the search state, including rotated joins and self-loops.
Stairs require a disclosed explicit connection. Neither hidden current terrain
nor unknown connections can supply a shortcut. Stale routes are validated through
ordinary movement one step at a time; a blocked attempt stops without rerouting.

## Execution and interruption

Accepting travel costs no action time, records an actor-visible request, and
acknowledges it immediately. Each completed step is a separately committed ordinary
move with the actor's normal action cost, revision, event, and observer update.
No planned route or future outcome is sent to clients.

A server pump attempts at most one step per actor every 75 milliseconds, releasing
the session lock between pumps. This is delivery/cancellation pacing, not game
time: only ordinary actions advance simulation ticks. Missed pumps do not cause a
catch-up burst. A journey ends on:

- Arrival at the requested cell.
- Cancellation, an accepted manual action, or a replacement travel request.
- A blocked move or persistence failure; no further steps are attempted.
- A newly perceived potential hazard relative to the trip's starting view.
  Currently, other actors count conservatively as potential hazards because
  hostility is not modeled. Repeated views of your own actor do not count.
  New ordinary terrain, ground items, and place hints do not interrupt travel.
  Hazard detection stops before another step; arrival takes precedence when the
  revealing step also reaches the destination.
- Another actor requiring input, rather than automatically waiting its turn.
- Controller release/disconnect or an accepted wizard setup/rewind.

There is no combat, hostility, trap, or dangerous-terrain system yet. The backend
hazard classification can grow with those mechanics without treating all new
information as dangerous. Actors already visible when a trip starts do not count
as new hazards, but still block movement normally. Wizard changes stop active jobs even when their changes are elsewhere.

A cancel request applies at an action boundary and cannot undo committed steps.
Travel status and active jobs are session-local. Restart restores completed moves,
request receipts, and actor navigation knowledge but never resumes travel. Rewind
clears travel before publishing the new branch snapshot. Terminal status lasts
until another trip or branch reset during the current server session; terminal
reasons are not durable history records in this slice.

## Protocol

Send an ordinary branch-checked command envelope:

```json
{"type":"command","branch":"<current branch>","command":{"type":"travel","expected_revision":12,"destination":"<known opaque cell key>"}}
```

The server requires control, readiness, a current revision, and a known traversable
route. Unknown targets and unavailable routes share a generic error. Exact retries
of an accepted request return its durable receipt without restarting the job,
including after reconnect, restart, or a later branch change. Role checks precede
receipt lookup.

Snapshots contain nullable `travel`. Ordered `travel` updates carry `status` and
an optional history `entry` for the accepted request. Status contains the travel
entry `id`, opaque `destination`, `completed_steps`, and `phase`: `active`,
`arrived`, `cancelled`, `blocked`, `hazard`, `decision_required`, `control_lost`,
`world_changed`, or `failed`. These updates do not alter action revisions.
Headless frames expose the same status for scripted clients.

```json
{"type":"cancel_travel","branch":"<current branch>","travel_id":"<travel entry id>"}
```

Cancellation requires the current controller and matching branch/job identity.
Repeating cancellation for the same completed job is harmless. An old cancellation
cannot stop a newer trip. A failed replacement request leaves an existing job alone.

## Compatibility and verification

Only the current `diagonal-v11` ruleset is supported. Start a fresh game after
an incompatible revision; saves are not migrated.

Focused tests cover remembered routing, hidden shortcuts, rotations, stairs,
cycles, stale terrain, free rejection, deterministic replay/rewind, receipts,
permissions, cancellation, harmless discoveries, newly seen actors, and
control/lifecycle interruption.
`scripts/scenarios/travel.json` supplies reproducible wizard setups for actual
server/text/headless/native ASCII process acceptance. Ordinary non-wizard travel
is tested separately. Native keyboard and mouse events exercise `_` selection and
click travel, alongside presentation-model and automated window-input coverage.
The existing Windows/Linux CI discovery runs these in debug and release.

The hazard-only interruption policy governs session interruption/status behavior.

[Doors](doors.md) add explicit barriers. Travel never opens a closed door; known
closed doors exclude routes, and stale open-door routes stop on blocked movement.
