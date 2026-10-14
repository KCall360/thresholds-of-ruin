# Playing in text

`tor-client-text` is the interactive-fiction frontend. It presents what your
character perceives as places, objects and prose, while the server owns the
rules. Normal play and the explicit `--script` diagnostic interface have
**different movement and approach behavior**.

## Start and attach

Complete the [development setup](../development/setup.md) first. In one
PowerShell terminal, create a session token and start a fresh game:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --listen 127.0.0.1:4000 --seed 42 --save saves/game.db
```

In a second terminal, set `TOR_SERVER_TOKEN` to the same value, then run:

```powershell
cargo run -p tor-client-text -- --connect 127.0.0.1:4000 --actor 1
```

On Linux, set the variable with `export TOR_SERVER_TOKEN=...` in both terminals;
the Cargo commands are identical. Keep credentials out of URLs, command-line
arguments and committed files. The address must be numeric loopback, including
`[::1]:4000`. Choose a new save path for a new game; reusing a save resumes it.
The default package is [the first dungeon](gameplay.md).

The client attaches and requests control. `--observe` skips that request; if
another client already controls the actor, you remain an observer. The normal
prompt is `>`. Try `look`, `stats`, `inventory`, `east` and `examine <thing>`.
`help` gives ordinary commands; `help session` gives connection and history tools.

## Commands

Commands use disclosed names. A direction walks toward a way onward; it is not
necessarily one cell. `step east` is the explicit one-step alternative.

| Command | Meaning |
| --- | --- |
| `look`, `l` | Describe the current place in full |
| `examine <thing>`, `x`, `look at` | Read disclosed appearance and condition |
| `read <thing>` | Read its disclosed description, if available |
| `examine floor`, `walls`, `ceiling` | Describe currently perceived surfaces |
| `examine me`, `diagnose` | Your condition |
| `status`, `score` | Condition and the disclosed objective |
| `stats` | Detailed personal creature build, without advancing time |
| `advancement` | Owner-only point ledger, exact purchase/respec previews and confirmation; see [controls](advancement.md) |
| `inventory`, `i` | Carried items, quantities and equipment |
| `take/get/pick up/grab <thing>` | Approach if necessary, then pick up |
| `take 3 arrows` | Transfer a quantity from the selected stack |
| `drop/put down <thing>`, `put <thing> on floor` | Drop carried items |
| `wear/wield <item>` | Equip a carried item |
| `remove <item>` | Unequip a carried item |
| `drink <potion>` | Use one carried potion |
| `eat <item>` | Eat one explicitly edible carried item |
| `open/close <door>` | Approach if necessary, then manipulate the door |
| `attack/kill/hit <creature>` | Approach if necessary, then attempt basic melee |
| `rattling blow <creature>`, `measured cut <creature>` | Invoke the named Warrior technique if granted |
| `brace for impact`, `brace for impact me` | Invoke granted self-defense |
| `spark lance fire <creature>`, `spark lance cold <creature>` | Invoke only your acquired Spark Lance form |
| `falter <creature>` | Invoke the granted fixed Falter form |
| `shell impact`, `shell keen`, `shell energy` (optionally `me`) | Invoke only your acquired self-target Shell form |
| `north`, `ne`, `go east`, other compass directions | Walk toward a way onward, continuing through empty corridors where possible |
| `up`, `down` | Use a disclosed stair or vertical link underfoot |
| `step <direction>` | Request one move |
| `go to <thing/creature/door>` | Approach without interacting |
| `go to <place name>` | Travel to a remembered named anchor |
| `go to start`, `go back to the start` | Return to the cell where this client began |
| `go to exit` | Travel to the objective's disclosed exit |
| `wait`, `z` | Wait, or continue existing recovery when unready |
| `resume` | Resume eligible disclosed suspended or paused work |
| `cancel` | Cancel eligible work in place |
| `escape <direction>`, `cancel <direction>` | After your own strike prefix, cancel its remaining sequence and attempt one step |
| `again`, `g` | Repeat the previous command |
| `listen`, `smell` | Describe cosmetic atmosphere |
| `places` | List known places |
| `name room <name>`, `name <number> <name>` | Rename a learned place anchor |
| `brief`, `verbose`, `superbrief` | Set arrival description detail |
| `quit` | Save and disconnect |

Ability commands come from the currently disclosed name or ID, with underscores
accepted as spaces; `stats` lists your grants. Examples above do not grant missing
forms. Techniques validate reach and resources when executed and do not approach
for you.

Finite-supply abilities use carried inventory automatically unless you add
`using <item>`. For example, `bind wounds me using clean bandage` selects that
exact known, touch-reachable ground or carried stack. Use full item names or
opaque `#item` handles; separate multiple stacks with commas. `using carried`
explicitly restores automatic carried selection. Quantities come from the
ability's rules. The server still checks current availability; menus and item
counts do not reveal other actors' reservations. Stats include disclosed cooldown
remaining and required supplies.

A self-or-ally ability accepts `me`, `self` or `myself` in scripted text, and
`me` in adventure input. Omitting the target chooses yourself. A named other
target must be a disclosed ally; a nonhostile stranger does not qualify.

Granted abilities act **from your current position**; unlike ordinary `attack`,
they do not automatically approach. Choosing one does not pay its cost. The
server checks reach, grants and resources at execution. Equipment and potion
use also have server-owned preparation and validation; a menu or parsed command
is not a completed action. See [items](../reference/items.md) and
[creature rules](../reference/creature-rules.md).

Your own multi-strike status reports completed strikes, the next-strike delay and
cancellation recovery debt. `cancel` retains outstanding debt after a completed
prefix; `escape west` adds an ordinary step's cost and effects if the step succeeds.
A blocked escape leaves the sequence and resources unchanged. Escape targets the
active/paused preparation, while plain resume/cancel keeps queued-first selection.
These commands require current disclosed permission and work in both adventure
and diagnostic modes. Ordinary Text direction commands retain their existing
movement/travel behavior; they do not select escape automatically. Later visible enemy results identify the actual strike,
including a miss after you move out of reach, without revealing its future length.

Detailed stats show attributes, skills, defenses, hit-die composition, Mana
binding, resources and reservations, talents and abilities. They are free and
available to observers. Diagnostic actors without creature builds show their
available health information instead.

The parser recognizes more verbs than the engine implements. Containers,
locks/keys, eating, throwing and conversation are examples of unsupported
interactions; recognizing their grammar does not make an effect available.
Missing objects are reported before an unsupported interaction is refused.

### Several things, chains and clarification

Use `take token and tablet`, `take all`, `drop everything except the key`, or
`take tokens`. Commands can be chained: `take sword. go east. open door` or
`take key, then go north`. Counts select quantities; item use is one at a time.

Every meaningful word must match the disclosed thing. The last noun can use a
known synonym, such as `body` for a corpse. Location can disambiguate: `open the
east door`, `attack the rat to the north`. Verb preferences apply first, so
`take scout` prefers its corpse and `attack scout` a creature.

Indistinguishable things in the same situation do not force a question. Distinct
candidates do. Answer with a number, an ordinal or words identifying one choice:

```text
> take thing
Which do you mean, the copper token or the stone tablet?
> the first one
```

Clarification consumes no time and retains the rest of the command chain. A new
command abandons the question. `it` refers to the last thing mentioned by you or
the narration; `them` refers to the last group and `him`/`her` to the last creature.

## Places and descriptions

Places and ways are inferred from perceived geometry and aligned last-seen cells,
not backend region boundaries. Doors and narrow gaps separate places; stairs
underfoot and openings are ways onward. Walkable floor within the place is not
itself an exit. A direction can also lead toward visible open ground beyond the
current view; corridors may be followed around bends until something needs a
choice. The server still validates the route against your character's knowledge.
Travel never opens doors or attacks automatically.

Bearings follow your view even across rotated joins. An undisclosed boundary is
not called a wall. Floor, wall and ceiling descriptions require perceived solid
cells; seeing part of a ceiling does not establish a complete roof. Materials,
item descriptions and creature names come from disclosed facts, not a hidden
world catalog. Atmosphere is deterministic cosmetic prose with no rule effects.

A place title is its authored or player-assigned name. Unnamed anchors are listed
as unnamed in text even though the backend retains a personal mnemonic. A learned
anchor represents a point, not a discovered room boundary. `places` distinguishes
`in sight` from `remembered`; the latter is no promise that a route is clear.
Names persist across clients and saves. Acceptable names are 1–80 UTF-8 bytes,
with no control characters or leading/trailing whitespace. Renaming is free,
requires control and cancels an active travel job at its command boundary.

`brief` gives full first descriptions and shorter return descriptions; `verbose`
always describes fully; `superbrief` uses name and contents. `look` always gives
a full description. Description-visit state and unhinted place identity are
client-local and restart with the client. Durable names are a separate
[character-knowledge contract](../architecture/clients.md#durable-place-knowledge).

## Turns, interruption and combat

Everything between prompts is composed as one passage. An approach and successful
pickup can become “You walk over to the stone tablet and pick it up.” A failed
journey keeps its purpose and explains why it stopped. If a new creature appears
on the arrival step, the client does not perform the intended follow-up merely
because the destination was reached.

The prompt waits for the authoritative execution and input boundary, rather than
returning on request admission. A refusal, hazard, loss of control, snapshot or
terminal run ends the current plan and discards its remaining chain. If the
server stops for another actor, subsequent observations are narrated between
turns. Commands typed while a turn is running are read afterwards. Journeys have
no ordinary player cancellation command; `stop` cannot cancel one.

Positive HP loss can interrupt preparation. Repeat the same valid attack to
resume retained progress; waiting preserves it, while movement or another action
can discard it. After reconnecting during recovery, `wait` continues scheduling.
Blows, injuries and deaths are told in order, without displaying scheduling ticks
in normal narration. Explicit `stats`, `status` and history remain inspection
commands. See [gameplay](gameplay.md) for the default adventure.

## Session tools, notes and recovery

`control` and `release` switch ownership; `sync` requests a snapshot; `save` waits
for a durable barrier; `history [before-id]` reads earlier history. `pace` controls
presentation pacing. Wizard tools require separate server-granted privilege; see
[development tools](../development/wizard.md).

After `wizard creature inspect <actor-id>`, both the adventure interface and
`--script` accept `strikes natural`, `strikes selected`, or
`strikes ability <ability-id>`. These read-only queries use the report's target
revision and do not require control. `strikes next` and `strikes previous` move
between bounded detail pages; completion is explicit. Main-report authored
counts include locked strikes. A “Uses selected melee” ability marker links to
`strikes selected`, which queries the actual current natural or weapon source.
Reopen the main inspection after changed state, reset, reconnect or a stale
source error. Details stay private to the authenticated wizard connection.

`note <text>` and `bookmark <text>` create annotations. For explicit provenance,
audience and anchoring, use:

```text
annotate <user|frontend> <private|actor> <note|bookmark|explanation> <here|state:N|entry:ID> <text>
```

Copy entry IDs from history. Actor-visible notes cannot point to private entries.
The server validates anchors and stamps authorship; frontend notes are stamped
with component `text`, never backend authority. Notes are single-line input,
limited to 4096 UTF-8 bytes, and consume no action time. Output escapes terminal
control characters. Clients using the same credential share that user's private
notes; a second connection is not a second privacy identity.

`--observe` is voluntary observation with a player credential. An enforced
spectator instead uses the server's separately configured spectator credential
as its client `TOR_SERVER_TOKEN`. Spectators receive live disclosed state and
permitted history but cannot control, act, annotate or rename. Both clients show
permanent **WIZARD GAME** status for wizard lineages.

Normal quitting saves before disconnecting. Ordinary accepted play is saved in
background batches, so a crash can lose an acknowledged unsaved tail. Restart
the server with the same save path and reconnect to the saved prefix. Active
travel and client command chains do not resume automatically. See
[persistence](../architecture/persistence.md).

A lost connection or invalid stream exits with an error. Automatic reconnect and
action retry are not implemented. If a response was lost, reconnect and inspect
history before repeating the action: it may already have executed. Idle
observation has no timeout; active requests have deadlines.

## Diagnostic mode

Add `--script` for the line-oriented development interface. It prints offsets,
ticks, disclosed opaque targets, inventory and history, with `Ready.` framing.
Directions (`east`, `go east`, `e`) request **one cell**. Pickup, attacks and door
operations act immediately without approach travel. Names may be full names or
trailing noun phrases; explicit `#` selectors use the received opaque target,
not an internal numeric entity ID. Copy the selector from client output.

`take 3 #<received target>` and `drop 2 #<received target>` transfer quantities;
omitting a count transfers the selected stack. The angle-bracket text is a
placeholder, not literal input. Diagnostic `stats`, session tools, notes and
opaque `wizard` commands remain available. For structured automation, use the
[headless interface](../reference/headless.md).

## Implementation and acceptance

[Client architecture](../architecture/clients.md) describes parsing, resolution,
place inference and turn composition. Actual-process coverage includes
[`test_text_process.py`](../../scripts/test_text_process.py),
[`test_adventure_process.py`](../../scripts/test_adventure_process.py),
[`test_interactions_process.py`](../../scripts/test_interactions_process.py), and
[`test_doors_process.py`](../../scripts/test_doors_process.py). These cover real
server/client connections, transcript boundaries, interruption, spectators,
control transfer and save/resume. Follow the shared
[verification policy](../development/testing.md) rather than treating illustrative
transcripts as a substitute for those tests.
