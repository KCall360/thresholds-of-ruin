# Doors

Doors were introduced in **doors-v6**. New games use **material-rims-v10**, with an
initially open wooden door in a 1x1 hall between two 5x3 rooms. The hall has no
place hint; each room retains its own interior anchor for text navigation.

```text
.....#.....
...../.....
.....#.....
```

Here `/` is the open door and `#` the walls flanking its one-cell hall. Doors
remain independent, cell-sized world entities: interior doors and doors on join
endpoints obey the same rules. Opening or closing does not change topology.
Protocol **10** adds surface facts; save format **3** is unchanged.

## Playing

Text supports `examine door`, `open door`, `close door`, and `go to door`.
Visible doors participate in noun clarification and `it` references. A distant
open/close intention travels to a currently visible approach cell, then submits a
separate ordinary action. Success is one sentence and one completion prompt.
`stop`, hazards (including hazards revealed on arrival), control loss, snapshot,
rewind, replacement intentions, or disconnect discard the follow-up. Reach and
state are rechecked on arrival. Restart never resumes an intention.

ASCII displays `+` for closed doors and `/` for open doors. Press **O** to open
or **C** to close, then an arrow key or **H/J/K/L** to choose the adjacent cell.
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
- Reach is one horizontal cardinal connection, resolved by the backend, including
  rotated joins. Standing in a doorway is not standing beside it.
- Closed doors block movement and sight but are not walls. The door itself is
  visible; cells, objects, and actors beyond it are disclosed only if another
  unobstructed sightline exists. New games use [symmetric shadowcasting](shadowcasting.md),
  with beveled corners for sight. Open doors do not obstruct either.
- Closing is unavailable while an actor or ground item occupies the door cell.
  Placement and wall edits cannot create overlapping walls and doors.
- Unknown, hidden, out-of-reach, obstructed, and already-in-that-state requests
  are rejected without action time or mutation. Examining and clarifying are free.
- Travel never opens doors. A known closed door excludes that route; if a
  remembered open door is now closed, ordinary movement validation stops the job.
  Alternative known routes may still be used. No hidden state supplies a shortcut.

Each perceived cell has a nullable `door`: stable identity, perceived name and
appearance, open state, current reach, and opaque keys for currently perceived
approach cells. Approach connections must occur in the resolved scene; they do
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
wizard door <region> <x> <y> <z> <open|closed>
```

Use ordinary open/close actions to test interactions. Placement follows the
existing authorization, atomic validation, revision, receipt, restart and rewind
rules. Closed placement cannot cover actors or items; duplicate placement is
rejected. Door identity and state rewind with the world. Privileged coordinates
remain backend-only; ordinary history contains sanitized wizard summaries.

Existing `two-room-v1`, `portal-sight-v2`, `observer-scene-v3`, `place-hints-v4`, and
`travel-v5` saves retain their original fixture, replay, and rules, with no doors.
They reject door placement and manipulation; travel-v5 continues supporting
travel. The doors-v6 and shadowcasting-v7 fixtures retain their original interior door.
No existing game is silently upgraded. All clients must use protocol 10.
Locks, keys, containers, destruction, transparent doors, and multi-cell door
entities remain future work. A wide join can have individual door cells.

## Verification

Simulation tests cover obstruction, time, reach, occupancy and atomic failure;
world tests compare an interior door with the same scene split by a wide join.
Rotated join interaction, disclosure, old rules, durable retries, replay and rewind
have focused tests. Shared memory tests cover stale door state and refresh.
Frontend tests cover clarification, intentions, glyphs, selection and commands.
Raw WebSocket tests enforce spectator and wizard permissions.

`scripts/scenarios/doors.json` and `scripts/test_doors_process.py` drive actual
server/text/headless/native ASCII processes: normal-game approach-and-close,
native O/C followed by direction keys, actor-perspective spectator agreement, hidden contents and stale
memory, closed-route rejection, rotated joins, cancellation, arrival hazards,
save/resume and rewind. Existing CI discovery runs them in debug and release on
Windows and Linux.
