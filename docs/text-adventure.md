# Text adventure slice

The normal text client presents places, objects and intentions rather than grid
coordinates. Start it through the existing Text desktop shortcut or the command
in [the text guide](text-client.md). The server remains authoritative; text travel
uses the same ordinary saved moves as ASCII travel.

```text
You stand in a space with a stone floor.
You see a copper token at your feet.
You see a stone tablet to the east.
You can head east.
>
examine token
A small copper disc, stamped with a worn spiral.
>
get token
You pick up the copper token.
>
get tablet
You walk over to the stone tablet and pick it up.
>
west
You walk west.
You stand in a space with a stone floor.
You can head east.
>
```

An interrupted journey is also one response, for example:
`You start walking east, but stop when a figure comes into view.` The actor's
name comes from disclosed appearance; the client does not invent monsters.


## Descriptive facts

Protocol **8** adds an item `description`, actor `name` and `description`, and cell
`material`. The backend supplies only appearances belonging to disclosed objects
and cells, including carried items. Shared cell memory retains these appearances
as potentially stale sightings. No world-wide appearance catalog is sent.

This is a deliberately small cosmetic foundation: all existing floor/wall cells
use stone; the three token materials and stone tablet have authored examination
text; other item names receive a neutral fallback. Actors are generic figures;
repeated views of the observing actor identify itself. These stubs add no item
abilities, identification rules, hardness, digging, lighting, or material editing.
Existing opaque wall terrain supplies the actual sight and movement obstruction.

Descriptions are free reads of disclosed facts, with no simulation action or
revision change. The client does not invent inscriptions, room names, enclosed
boundaries, or hidden properties. Walls are described only when wall cells are
actually visible; an undisclosed cell or region boundary is not called a wall.

## Places and directions

Hints remain unnamed anchors, not regions, room extents, or discoveries of an
entire area. The nearest visible anchor represents the local place for this
initial heuristic, even when the actor moves away from its center to reach an
object. Directional commands choose other visible anchors in that bearing,
labeling them by a co-located visible item when possible. Multiple candidates
ask a numbered/named clarification before sending a request. Repeated
occurrences of one opaque cell are deduplicated. Items assigned to the same
nearest visible anchor are described as on the floor nearby, or at your feet
when reachable; directions are reserved for items in another place. With no
visible anchors, same-elevation items are treated as nearby.

Walkable floor within a place is not an exit. The starting fixture therefore
offers east only, and the other place offers west. There is no arbitrary floor-ray
fallback. Without visible hints, compass travel is unavailable; object approach
still works, and `step` remains an explicit fine-movement tool in session help.
Vertical travel also accepts stairs at the actor's cell with a disclosed landing.
The backend validates the actual known route; apparent adjacency does not give
the frontend authority to invent a connection. Bearings follow the observer frame,
including rotated joins.

This is a conservative visible-anchor grouping, not a general room segmentation
algorithm. Named/persistent places, unhinted-place inference, remembered offscreen
destinations and richer shape summaries remain future work. Descriptions do not
invent enclosed walls or use internal region names.

## Intentions and conversation

- `look` / `l`: describe current sight and visible ways onward.
- `examine <thing>` / `x <thing>` / `look at <thing>`: inspect disclosed appearance;
  `examine walls` and `examine floor` describe visible surface material.
- `inventory` / `i`: list carried names.
- Diagonal names and `ne`/`se`/`sw`/`nw` work with travel and `step`; descriptions
  use eight horizontal bearings (diagonal sectors cover ratios from 1:2 to 2:1).
- Directions / `go east`: travel to a visible destination as described above.
- `go to tablet` / `approach tablet`: travel to a visible ground item, without
  manipulating it on arrival.
- `take tablet`: take it immediately if reachable, otherwise travel to its current
  disclosed cell and attempt ordinary pickup on arrival.
- `stop` / `cancel`: cancel travel and discard any pending pickup.
- `step east`: explicitly request one ordinary movement action.
- `wait`, `control`, `release`, `sync`, notes, history and wizard commands retain
  their existing authority and timing rules; `quit` disconnects.

Nouns match whole words in disclosed names, ignoring articles. Clarification
accepts a listed number or distinguishing words such as `the copper one`.
Clarification is discarded after an observation revision changes; snapshots
also reset conversation. `it` refers to the last selected item, and only while
that item is still visible or carried. Identical names can be selected by number.
There is no unrestricted natural-language parser or generated narration.

The prompt is `> `, with the cursor immediately after the space, and returns when
the intention has completed or
been interrupted, not when the server accepts a travel request. Input remains live:
`stop` cancels during movement, and queries can still display information. Another
movement/manipulation intention replaces the old trip. Piped commands that intend
sequential journeys must wait for the completion prompt.

Successful approach-and-pickup produces one sentence. Directional arrival reports
the direction and describes the destination once. Interruptions summarize the
attempt and why it stopped; internal moves, arrival status, and scheduling are
not printed as separate responses. Nearby pickup does not claim the actor walked.
Normal startup omits control-acquisition chatter, and basic help contains gameplay
commands only. `help session` exposes connection/history tools and fine movement.

A pending pickup is connection-local and tied to the exact travel receipt and
branch. It is discarded on cancellation, any non-arrival termination, snapshot,
rewind, control loss, replacement intent, or disconnect. Arrival rechecks control,
readiness, item identity and reach. Even if backend arrival takes precedence over
a newly visible actor, text pauses without pickup. Every eventual pickup is a
normal revision-checked server command. Restart never resumes compound intentions.
Spectators receive prose but remain read-only.

## Compatibility and acceptance

The text client uses the current protocol and rules. Appearances are deterministic
presentation facts, and the client composes travel and pickup requests.
Older save formats and rulesets are unsupported.

`--script` preserves the original deterministic text command interface: single-cell
directions, immediate-only pickup, detailed diagnostic output, and `Ready.` framing.
Existing wizard/geometry process scenarios explicitly select it. Ordinary startup
and desktop shortcuts select the adventure interface. Explicit `history` remains
a detailed reference tool, including IDs needed for annotation/rewind anchors.

Behavior tests cover nouns, pronouns, clarification invalidation, ambiguous places,
disclosure, repeated views, and floor-versus-exit distinctions. The actual-process
suite `scripts/test_adventure_process.py` verifies normal play, travel then pickup,
exact successful/interrupted transcripts, prompt boundaries, cancellation,
spectators, persistence, and wizard-authored geometry/hazards using
`scripts/scenarios/text-adventure.json`, `wide-join.json`, and `portal-geometry.json`.
The existing discovery runs these tests in debug and release on Windows and Linux.

## Door interactions

The [door slice](doors.md) adds visible doors to descriptions, examination, noun
clarification and pronouns. `open door` / `close door` approach a disclosed standing
cell when necessary, then submit an ordinary action with the same interruption
checks as pickup. `go to door` approaches without manipulating it. Travel never
automatically opens a door. Saves use `diagonal-v11`.

[Material volumes](material-volumes.md) add real stone enclosure and
`examine ceiling`. Surface descriptions use backend-disclosed floor and ceiling
facts; missing enclosure is not inferred from a storage boundary.
