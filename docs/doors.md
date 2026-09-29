# Doors

The `scenarios/two-room` package has an initially open wooden door in a 1x1
hall between two 5x3 rooms. The hall has no
place hint; each room retains its own interior anchor for text navigation.

```text
.....#.....
...../.....
.....#.....
```

Here `/` is the open door and `#` the walls flanking its one-cell hall. The
hall is two cells high, and so is the door.

A door is an independent world entity with an authored `height`: it occupies
its base cell and the `height - 1` cells straight above it, which must all be
free. It opens, closes and blocks as one door from any of its cells. Interior
doors and doors on join endpoints obey the same rules. Opening or closing does
not change topology.

A door should fill its doorway, or a taller creature looks over it. Package
validation rejects a door whose doorway continues above it: the cell above
the door's top is open and, like the top cell, walled on both sides. A door in
a low wall under open space isn't flagged. A doorway at a region join has no
walls beside it inside the room, so its height is the author's responsibility.

## Playing

Text supports `examine door`, `open door`, `close door`, and `go to door`.
Visible doors participate in noun clarification and `it` references. A distant
open/close intention travels to a currently visible approach cell, then submits a
separate ordinary action. Success is one sentence and one completion prompt.
`stop`, hazards (including hazards revealed on arrival), control loss, snapshot,
rewind, replacement intentions, or disconnect discard the follow-up. Reach and
state are rechecked on arrival. Restart never resumes an intention.

ASCII displays `+` for closed doors and `/` for open doors. Press **O** to open
or **C** to close, then an arrow key or **HJKL/YUBN** to choose the adjacent cell.
The client says "There is no door in that direction." if no reachable door is
there, sending no action and consuming no ticks. Escape cancels the direction
prompt for free. The prompt clears when the observation changes or control is
lost. These controls do not walk automatically. **F3** acquires actor control;
**R** releases it. Actors and ground items take glyph precedence over an open
door. Spectators can watch actions and history but cannot manipulate doors.

The diagnostic text `--script` interface accepts `open door`, `close door`, or
`open #id` / `close #id` to select a disclosed door explicitly. It acts immediately
without approaching. Headless clients submit ordinary actions:

```json
{"type":"act","action":{"type":"set_door","door":1,"open":true}}
```

## Rules and perception

- Open and close each cost the acting actor's normal movement/wait recovery time.
- Doors can be reached cardinally or diagonally, using the same one-clear-side
  corner rule as movement. The backend resolves rotated joins. Standing in a doorway is not standing beside it.
- Closed doors block movement and sight but are not walls. The door itself is
  visible; cells, objects, and actors beyond it are disclosed only if another
  unobstructed sightline exists. New games use [three-dimensional sight](sight-3d.md),
  with beveled convex corners. Open doors do not obstruct either.
- Closing is unavailable while an actor or ground item occupies any door cell.
  Placement and wall edits cannot create overlapping walls and doors.
- Unknown, hidden, out-of-reach, obstructed, and already-in-that-state requests
  are rejected without action time or mutation. Examining and clarifying are free.
- Travel never opens doors. A known closed door excludes that route; if a
  remembered open door is now closed, ordinary movement validation stops the job.
  Alternative known routes may still be used. No hidden state supplies a shortcut.

Each perceived cell has a nullable `door`: stable identity, perceived name and
appearance, open state, current reach, and opaque keys for currently perceived
approach cells. Every cell of a tall door reports the same door, but only its
base cell reports reach and approach cells, so the door is offered once. Approach connections must occur in the resolved scene; they do
not reveal internal regions, transforms, hidden cells, or a planned route. Text
chooses the nearest displayed approach candidate; the backend validates routing.
This conservative heuristic can reject an approach when that candidate has no
known route. It does not search undiscovered space or silently try other routes.
Shared memory retains last-seen door facts, which can become stale. Door events
enter the acting actor's saved history and spectator stream. Richer cross-actor
semantic event narration remains future perception work.

## Authoring and compatibility

An authorized wizard can place a door, consuming no ordinary action time:

```text
wizard door <region> <x> <y> <z> <open|closed> [height]
```

`height` defaults to 1. A door taller than the free space above its base is
rejected, and the error names the most that fits. Structured wizard JSON
`place_door` takes a required `height`. Wizard placement doesn't apply the
package check for doorways left open above a door.

Use ordinary open/close actions to test interactions. Placement follows the
existing authorization, atomic validation, revision, receipt, restart and rewind
rules. Closed placement cannot cover actors or items; duplicate placement is
rejected. Door identity and state rewind with the world. Privileged coordinates
remain backend-only; ordinary history contains sanitized wizard summaries.

Only the current rules are supported; older saves aren't migrated.
Locks, keys, containers, destruction, transparent doors, and doors wider than
one cell remain future work. A wide join can have individual door cells.

## Verification

Simulation tests cover obstruction, time, reach, occupancy and atomic failure;
world tests compare an interior door with the same scene split by a wide join.
Rotated join interaction, disclosure, durable retries, replay and rewind
have focused tests. Shared memory tests cover stale door state and refresh.
Frontend tests cover clarification, intentions, glyphs, selection and commands.
Raw WebSocket tests enforce spectator and wizard permissions.

`scenarios/tests/doors-*` and `scripts/test_doors_process.py` drive actual
server/text/headless/native ASCII processes: normal-game approach-and-close,
native O/C followed by direction keys, actor-perspective spectator agreement, hidden contents and stale
memory, closed-route rejection, rotated joins, cancellation, arrival hazards,
save/resume and rewind. Existing CI discovery runs them in debug and release on
Windows and Linux.
