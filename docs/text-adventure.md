# Text adventure slice

The normal text client presents places, objects and intentions rather than grid
coordinates. Start it with the command in [the text guide](text-client.md). The
server remains authoritative; text travel uses the same ordinary saved moves as
ASCII travel. How commands become game actions and narration is described in
[the IF engine](if-engine.md).

```text
> examine token
A small copper disc, stamped with a worn spiral.
> get token and tablet
You pick up the copper token. You walk over to the stone tablet and pick it up.
> east
You set off east. A ruin scout comes into view to the east, and you stop warily.
> attack it
You ready an attack on the ruin scout. It strikes you, spoiling your attack.
HP 48/50
> again
You strike the ruin scout, and it falls dead.
> take corpse
You walk over to the ruin scout corpse and pick it up.
```

Everything that happens between two prompts is one passage, and the prompt
returns only when it's the player's move again. Names come from disclosed
appearance; the client doesn't invent monsters, causes or rooms.

## Descriptive facts

The current protocol supplies an item `description`, actor `name` and `description`, and cell
`material`. The backend supplies only appearances belonging to disclosed objects
and cells, including carried items. Shared cell memory retains these appearances
as potentially stale sightings. No world-wide appearance catalog is sent.

This is a deliberately small cosmetic foundation: all existing floor/wall cells
use stone; the three token materials and stone tablet have authored examination
text; other items and all actors have no authored description, and the client
says so in its own words. The observing actor's own body, seen from another
cell, is identified by its id, never listed as a figure in the room. These stubs add no item
abilities, identification rules, hardness, digging, lighting, or material editing.
Existing opaque wall terrain supplies the actual sight and movement obstruction.

Descriptions are free reads of disclosed facts, with no simulation action or
revision change. The client does not invent inscriptions, room names, enclosed
boundaries, or hidden properties. Walls are described only when wall cells are
actually visible; an undisclosed cell or region boundary is not called a wall.

## Places and directions

The character is always in a *place*: the open floor reachable without passing
a door or a narrow gap in the walls, worked out from what is seen (see
[places and ways](if-engine.md#places-and-ways)). A description, in prose,
says what kind of place it is and what it's made of, its atmosphere, where it
goes on out of sight and its ways out, then who and what is in it:

```text
Hollow Promise
You are in a small, dusty chamber of stone. Motes of dust hang in the still
air, stirring as you move. An open wooden door leads east.
A copper token lies on the floor nearby; a stone tablet lies to the east.
```

Arriving somewhere already described gives its name, ways and contents only;
`verbose` describes every arrival in full, `superbrief` names places only, and
`brief` returns to the default. `look` always describes in full. See
[descriptions](if-engine.md#descriptions).

Things inside the place are "at your feet" or "on the floor nearby"; anything
outside it keeps its direction. The ways onward are the place's doors and gaps,
stairs underfoot, and the directions in which the place fades into darkness
(out of sight), which lead as far as can be seen that way. Walkable floor inside a place is not an exit. A direction
heads through the opening that way to the first open cells beyond it, asking
which when there are several; a closed door answers "The wooden door to the
east is closed." With no opening that way, an authored place hint seen in
another place is the fallback. The server validates the actual route; a
blocked journey says so, naming a creature in the way.

Bearings follow the observer frame, including rotated joins. Diagonal sectors
cover ratios from 1:2 to 2:1. [Durable place knowledge](place-knowledge.md)
adds persistent names (authored by the scenario, or your own) and renaming.
Descriptions never invent walls or names the protocol doesn't disclose. Atmosphere (a mood word, the
feel of the air, a smell or a sound) colours each place, has no effect on play,
and is the same every time the place is described; see
[atmosphere](if-engine.md#atmosphere).

## Commands

Many words share one action, and one command may need several actions: `take
tablet` walks over first when it's out of reach. See the
[IF engine](if-engine.md#verb-semantics-and-plans) for the full verb table.

- `look` / `l`; `examine <thing>` / `x` / `look at`; `read <thing>`;
  `examine walls`, `floor` or `ceiling` for visible surface materials;
  `examine me` and `diagnose` for the character's condition; `status` (or
  `score`) adds the run's objective.
- `brief`, `verbose` and `superbrief` choose how arrivals are described.
- `inventory` / `i`.
- `take` / `get` / `pick up` / `grab <thing>`; `drop` / `put down`; `put <thing>
  on floor`. Counts take from one stack: `take 3 arrows`.
- Several things: `take token and tablet`, `take all`, `drop everything except
  the key`, `take tokens`.
- `open` / `close` a door, walking over first when needed.
- `attack` / `kill` / `hit <creature>`, closing in first when needed.
- A direction (`east`, `ne`, `up`), or `go east`: head for a way onward, and
  keep walking through darkness or along a corridor until something comes
  into view or the way needs a choice.
  `go to <thing>` walks over without acting on it, and `go to <place name>`
  goes back to a place listed by `places`; `go to start` (or `go back to the
  start`) returns to where the character stood when the client began, and
  `go to exit` heads for the objective's exit.
  `step east` makes one step.
- `wait` / `z`.
- `again` / `g` repeats the last command.
- Chains: `take sword. go east. open door`, or `take key, then go north`.
- `listen` and `smell` describe the place's atmosphere.
- `places`, `name room <name>`, `name <number> <name>`. Places the scenario
  hasn't named, and you haven't, are listed as unnamed.
- Session tools are in `help session`: `control`, `release`, `sync`, `save`,
  `history`, `note`, `bookmark`, `pace` and `wizard`. `quit` disconnects.

Verbs the game has no rules for yet (`wear`, `wield`, `eat`, `drink`, `give`,
`throw`, `unlock`, `push`, `talk`, `search`, `pray` and others) are recognized
and refused plainly: "You can't wear anything yet." A missing object is named
first: "You can't see any lamp here."

### Names, questions and pronouns

Every word must name the thing; the last can be a synonym (`body` for a corpse,
`door` for a gate). Things in sight can also be named by where they lie: `open the
east door`, `attack the rat to the north`. The verb's preferences come first: `take scout` means the
scout's corpse, `attack scout` the scout.

Things the player can't tell apart are interchangeable: with two copper tokens
at your feet, `take token` takes one without asking. Things that differ need a
choice:

```text
> take thing
Which do you mean, the copper token or the stone tablet?
> the first one
You pick up the copper token.
```

An answer can be a number, an ordinal or words that pick one choice. Anything
else asks again; a new command abandons the question. An answered question
continues the rest of its chain. Asking takes no game time.

`it` is the last thing mentioned by the player or by the narration, so after
"A ruin scout comes into view", `attack it` means the scout. `them` is the last
group, and `him` and `her` the last creature.

### Turns and narration

A turn runs every command on the line, then composes one passage. The prompt
returns when the game is waiting for this player, never while a journey or an
attack is still unfolding; commands typed meanwhile run afterwards. Journeys
can't be stopped partway, so `stop` has nothing to do.

- A journey and what it was for are one sentence: "You walk over to the stone
  tablet and pick it up."
- An interrupted journey keeps its purpose and says why it stopped: "You head
  toward the stone tablet, intent on picking it up. A figure comes into view to
  the east, and you stop warily."
- Arriving as someone new comes into view never authorizes the next action,
  even when the server reports arrival.
- A pickup after a journey waits until the character is ready, as after a fight.
- Blows, deaths and HP are told in order; ticks, readiness and recovery never
  are. HP follows the passage when it changed.
- Arriving somewhere new by a direction describes the place.

Updates that arrive between turns (another player acting, or the watched player
for a spectator) are told the same way above a fresh prompt. Spectators can't
act. Nothing of a turn survives a restart, and a snapshot or rewind ends the
turn and describes the scene again.

## Compatibility and acceptance

The text client uses the current protocol and rules. Appearances are deterministic
presentation facts, and the client composes travel and pickup requests.
Older save formats and rulesets are unsupported.

`--script` preserves the original deterministic text command interface: single-cell
directions, immediate-only pickup, detailed diagnostic output, and `Ready.` framing.
Existing wizard/geometry process scenarios explicitly select it. Ordinary startup
and the desktop launchers select the adventure interface. Explicit `history` remains
a detailed reference tool, including IDs needed for annotation/rewind anchors.

The engine's tests are listed in [the IF engine](if-engine.md#testing). The
actual-process suite `scripts/test_adventure_process.py` verifies normal play,
travel then pickup, exact successful and interrupted transcripts, prompt
boundaries, spectators, persistence, the first dungeon's fight and corpse pickup,
and wizard-authored geometry and hazards using `scenarios/tests/text-adventure-*`,
`wide-join` and `portal-geometry`. The existing discovery runs these tests in
debug and release on Windows and Linux.

## Door interactions

The [door slice](doors.md) adds visible doors to descriptions, examination, noun
clarification and pronouns. `open door` / `close door` approach a disclosed standing
cell when necessary, then submit an ordinary action with the same interruption
checks as pickup. `go to door` approaches without manipulating it. Travel never
automatically opens a door.

[Material volumes](material-volumes.md) add real stone enclosure and
`examine ceiling`. Surface descriptions use the solid cells currently seen as
floors, ceilings and walls; missing enclosure is not inferred from a storage
boundary.

## Item quantities and knowledge

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.
