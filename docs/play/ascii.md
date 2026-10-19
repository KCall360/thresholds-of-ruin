# Playing in ASCII

`tor-client-ascii` is a native windowed frontend for the game, laid out like
NetHack's terminal interface. It renders only the server's disclosed
observations and shares the same connection/state validation as the text
client. Windows and Linux/X11 are tested.

Wizard accounts can press F7 to enter an administrative command. Enter sends it
and Esc cancels. `creature inspect 2` opens the private report for loaded actor 2;
Up/Down and Page Up/Page Down scroll it. Tab opens **Strike sources**;
Up/Down and Enter choose natural, selected melee or a granted ability. Within
strike details, `[` requests the previous page and `]` the next; scrolling stays
within the current page. Esc returns to source selection, then the main report,
then closes it. A “Uses selected melee” marker can be followed through Tab and
**Selected melee**. Opening ordinary stats with `@` returns to the attached actor's
personal stats. Changed state, a snapshot reset or disconnect clears private
strike navigation; reopen the report after a stale-source error. See [wizard tools](../development/wizard.md).

## Screen

- **Message area** (top, three rows). Everything that happened since your last
  command, composed by the client from disclosed changes. Messages stay until
  you act again, then move to the message log (Ctrl-P). Prompts and refusals
  appear on the row below them.
- **Map** (the window's width). One map for every height; see [the map](#the-map).
- **Status lines** (bottom). The place you're in (or your name) and the
  objective; then hit points, time in ticks, and conditions such as
  Recovering, Attack paused, Travelling or Running.
- **Key hint and control** (bottom row). Whether you're in control, observing
  or spectating, and whether a request is waiting for the server.

Inventory, creature details, help and logs never take permanent space; they
open on demand over the map.

### Messages

The client composes messages from the shared narration of each disclosed
observation (see [client narration](../architecture/clients.md#narration-and-stream-recovery))
and leaves out what the screen already shows: your own steps and waits,
"You can act again" and "You must wait", hit point lines, starting an attack,
and creatures leaving sight. It merges related lines: a hit that kills reads
"You kill the ruin scout!"; starting and finishing item work reads as the
finished action, adding "It was a potion of poison." when the item turned out
to be something else; a repeated line is counted ("(x3)"). After you move it
says what's underfoot ("You see here a sword.", "There is a staircase down
here."). A creature is announced when it first comes into sight and again
only after 1,000 ticks out of sight. A journey that stops short says why.

When a turn's messages need more than three rows, the last row ends with
`--More--`. Your next key shows the next rows and does nothing else, so no
message scrolls away unseen; Esc skips to the end. Spectators can't page, so
they see the newest rows and are never held.

### The map

Each column of cells is drawn as what stands at your own height there:

| Glyph | Meaning |
| --- | --- |
| `@` | You (your whole body, however tall) |
| common asset glyph | A creature, using the shared asset vocabulary (for example `u` for an equine); `&` when its asset is absent, unknown or unavailable |
| `#` (blue-grey) | Wall |
| `#` (tan) | Low wall, ledge or step: solid at your feet, open above |
| `^` | A drop or pit: open at your feet and below |
| `.` | Floor |
| `<` `>` | Stairs up / down |
| `+` `/` | Closed / open door, at foot or head height |
| `)` `[` `!` `%` `(` `=` `"` `?` `/` `$` `*` | Items, by disclosed asset with class fallback, at your level or down in a drop |
| grey | Remembered, not in sight now |

Map, look and inventory share the same current [asset palette](../reference/protocol.md#palettes-in-the-clients).
Authored names still identify things and commands. A palette-only update redraws
without waiting for a turn; a missed revision uses safe fallbacks until recovery.
Terrain assets tint floors, walls and doors without changing geometric symbols,
open/closed doors, stair directions or remembered-cell greying.

Heights more than two cells below or three above your feet aren't drawn: a
stair's far landing, a distant ceiling. Cells are a fixed size, so the map
never rescales; it stays centred on you. Remembered cells within the window
come from [map memory](#remembered-map). Creatures out of sight disappear.
No portal markers or region labels are shown; see
[observer scenes](../architecture/geometry.md).

## Run and play

Complete the [development setup](../development/setup.md), then start the server
with the [text guide’s launch commands](text.md#start-and-attach). Set
`TOR_SERVER_TOKEN` to the same token in the client terminal:

```sh
cargo run -p tor-client-ascii -- --connect 127.0.0.1:4000 --actor 1
```

Use `--observe` to attach without requesting control. Another client's ownership
does not prevent observation; press F3 once that client releases control. The
server address must be a numeric loopback socket address (IPv4 or IPv6).
Automatic server launch and packaged builds are not available; see [status](../status.md).

| Key | Behavior |
| --- | --- |
| `h` `j` `k` `l` `y` `u` `b` `n`, arrow keys | Move one cell; after your own strike prefix, cancel-and-step when permitted; otherwise moving into a hostile creature attacks it (`--bump-attacks hostile\|any\|off`) |
| Shift + direction | Run until something happens: a message, a creature in sight, or the way ahead isn't open floor. Any key stops a run |
| `<` / `>` | Go up / down where a stair or vertical link is |
| `.` or Space | Wait one action |
| `a` | Choose a creature to attack; the choice is outlined on the map |
| `z` | Choose a granted ability, then a disclosed target. Supportive abilities offer yourself and eligible allies. Up/Down selects, Enter confirms and Esc cancels; choosing is free |
| `g` or `,` | Pick up. A lone object is taken at once; otherwise choose by letter (digits first set a count) |
| `d` | Drop: choose by inventory letter, digits first set a count |
| `w` / `t` / `q` / `e` | Equip / remove / drink / eat: always asks, by inventory letter |
| `F6` / `F12` | Read a carried item / select a device then a direction |
| `o` / `c`, then a direction | Open / close an adjacent door |
| `i` | Inventory: letters, what's worn or in hand, and known equipment numbers |
| `@` (Shift + 2) | Personal creature stats: attributes, skills, defenses, resources, talents and granted abilities. Up/Down scroll, Page Up/Down page; Esc or `@` closes |
| `;` | Look: move a cursor, then `.`, `;` or Enter describes the cell without taking time |
| `_` | Travel: move the cursor, `<` / `>` jump it to the nearest known stairs, `.` or Enter goes |
| Left mouse click | Travel to the clicked known floor cell |
| Any key during a journey | Show the rest of the journey at once; journeys can't be cancelled |
| `[` / `]` | Show journey steps more slowly / quickly (`--pace <ms>` sets the start, default 75) |
| `?` | Help: commands and map symbols |
| Ctrl-P | Message log: earlier turns, newest at the bottom; Up/Down, Page Up/Down scroll |
| F2 | Game history from the server, opening at the newest entries; Page Up for older pages |
| F5 | Remembered places; Enter renames |
| F4 | Write a note anchored to the state where composition began; Tab switches private / actor-visible |
| F3 / R | Acquire / release actor control |
| F8 / F9 | Resume or cancel eligible work when the server grants that control |
| F10 | Owner-only advancement/respec screen; [command and preview controls](advancement.md) |
| F11 | Escape a linked hold. A sole hold is selected immediately; otherwise Up/Down chooses and Enter confirms |
| Enter on the end screen | Close the victory or death summary |
| Escape | Cancel or close what's open, otherwise quit |

When the selected ability permits a weapon choice, Tab in the attack, ability
or direction picker opens WEAPON. The first row preserves the default primary
weapon (or natural attack); other rows are your currently observed equipped
attack items and exact sockets. Up/Down selects, Enter returns to the original
picker, and Esc keeps your previous choice. Choosing a weapon creates no extra
attack. A changed observation clears the draft; a moved or missing item is
refused rather than replaced. Choose again before confirming. The same exact
choice is retained if the action also asks for supplies.
After choosing the target and any supplies, an explicit equipped choice opens
EXACT WEAPON ACTION PREVIEW. Review the server's cost bundle, time to the next
strike, and separate recovery, then press Enter to send that one exact action or
Esc to cancel. Enter cannot send it while the reply is pending. Resuming labels
start already paid and resolution reserved; a paid sequence prefix labels
settled costs with no new resource charge. Time to the next strike is not the
completion time for the whole sequence. Previewing reserves nothing and does
not promise affordability. A stale response refreshes the observation and
requires choosing again. The default row retains the ordinary immediate path.

For an ability that requires finite supplies, confirming its target opens the
SUPPLIES screen. The first choice uses carried inventory automatically. Up/Down
focuses an exact known carried or touch-reachable ground stack; Space toggles
stacks, and Enter confirms the checked selection (or the focused stack when none
are checked). At most 32 stacks can be selected. Escape cancels without spending
anything. The ability menu and stats show required supplies and active-time
cooldown remaining; the server still decides whether the action can start.

Inventory letters stay with an item for as long as you carry it; a letter
freed by dropping an item is reused. In a choice menu, letter keys choose
items rather than acting as commands; arrow keys and Enter also work.

When held, status shows the number of incoming holds. F11's choices name a
holder only while that creature is ordinarily visible; otherwise they say
"Unseen holder". Esc or F11 dismisses the choice without acting. A refreshed
observation clears an open selection, so a row can never silently change to a
different hold. Escape takes time and attempts to release exactly the chosen
hold; another holder can still prevent movement.

During your own multi-strike preparation, status shows the completed prefix,
next-strike delay and cancellation recovery debt. If the server permits it, a
normal direction key cancels that preparation and attempts one step atomically,
before bump-attack conversion. A blocked step leaves the sequence intact. F9
cancels in place and keeps the outstanding debt; F8 resumes eligible paused work.
Ordinary F8/F9 selection remains queued-first, while directional escape selects the
actual preparation even when unrelated work is queued. Enemy results can identify
a later strike or miss, but never reveal how many strikes remain.

Refusals are said plainly: a move the server refuses reads "You can't go that
way.", travel without a known route "You don't know a way there."

Notes are user-source notes, limited to 4096 UTF-8 bytes. Advanced source/category
and historical-anchor commands remain available in the text client. The bitmap
renderer displays Basic Latin; other characters appear as `?` while the original
Unicode text is preserved in the protocol and save. Long labels end with `~`.

See [backend travel](../reference/movement.md#travel) for destination selection, progress, interruptions,
and compatibility. [Normal text play](text.md) uses approach intentions; its explicit `--script`
mode uses one-cell movement and immediate interactions.

Only one request is in flight at a time. Movement does not auto-repeat from
holding a key, and gameplay input while a request is pending is discarded rather
than queued into accidental extra turns. Invalid actions and unresolved choices
consume no time. Window resizing and redraws never advance simulation.

### Needs server or protocol support

These would improve the client but need more than client changes, so they
aren't implemented:

- **Depth.** Observations don't say which floor you're on, so the status line
  can't show NetHack's `Dlvl`.
- **Turns.** Time is shown in ticks; a character's ticks per turn isn't disclosed.
- **Scenario appearance overrides.** The common asset vocabulary supplies
  glyphs and colours. Scenario-specific glyph/colour overrides, including
  Rogue's letter assignments, need the later palette-extension design.
- **Objective item.** The client has no disclosed marker identifying which
  carried or visible item the objective requires.
- **Travel nearby.** Travel to a visible cell without a known route is
  refused; NetHack's travel moves as close as it can.
- **Stair landings.** Abstract stair landings arrive as ordinary cells far
  below or above; the client hides them by height. A flag on landing
  occurrences would make that exact.

The protocol already supplies an opaque exit cell key for a disclosed objective
(text uses it for `go to exit`). A dedicated ASCII exit command or map marker
would be a client presentation improvement, not missing exit support in the
protocol.

## Switch between clients and resume

1. Start the text client, take an action, and add a note if you like.
2. Start ASCII with `--observe`. It receives the same actor's state and live notes.
3. Enter `release` in text, then press F3 in ASCII.
4. Move around in ASCII.
5. Press R in ASCII and enter `control` in text to switch back.

Accepted actions and notes are saved in background batches. Normal player-window
exit waits for saving; a crash can lose recent play. See [persistence](../architecture/persistence.md). Restart
the server with the same save path and relaunch either frontend to resume.
The GUI keeps its last view with a disconnected status if the connection fails;
close and relaunch to reconnect. An uncertain request is not automatically retried.
Inspect history after reconnecting before repeating it. Disconnecting releases
control.




## Remembered map

Current cells use normal colors; previous sightings are grey. Terrain, doors,
stairs and ground items retain their last disclosed state. Creatures disappear
when they leave perception. Revisiting a cell refreshes it, including removing
items no longer there. Hidden changes never refresh memory.

Travel selection and left-click accept a column with a perceived **or remembered**
walkable cell at your foot level. Current observations override stale memory.
Unknown columns, columns known only at head height, walls and known closed doors
are rejected. The server handles stale obstacles using the character’s known
routes. The travel cursor’s `<`/`>` keys find known stairs; they do not browse
height. Height inspection belongs to the look cursor.

The server saves each character's last-seen appearances and witnessed spatial
relationships. Returning to an explored neighborhood or reconnecting hydrates
that character's memory. Rewind restores the selected boundary's knowledge.
Different characters do not share sightings, and spectators cannot teach them
new cells. The character's durable place names and navigation routes remain
separate from appearance memory.

The active chart covers 31 cells each side in x and 16 in y, with heights from
two below to three above the feet merged into one map: all 12,474 possible cell
positions fit. Bounded pages populate this window atomically while live input
and current observations remain available. Unknown cells stay blank; remembered
actors and actionable old item or door handles are never retained.

Shared opaque keys align successive views. After an unaligned crossing, the
server reconstructs placement from witnessed relationships, including rotations,
without consulting hidden current terrain. Ambiguous placements are suppressed
while independently certified cells remain. An exceptional projection or transfer
limit is reported as incomplete; it does not silently claim a complete map or
erase the last aligned chart. The bounded client cache is not the permanent
knowledge store, so older floors can be hydrated again after cache eviction.

F5 lists durable places. Up/Down selects; Enter edits and saves a name; Escape
cancels or closes. Spectators may read the list. Names are 1–80 UTF-8 bytes without
control characters or leading/trailing whitespace; renaming requires control.

## Combat, equipment and observation

Use `a` to choose a visible creature, Up/Down to select and Enter to attack.
`--bump-attacks hostile|any|off` controls manual movement interpretation; the
default is `hostile`. Travel stops rather than attacking. Exact own HP and action
state appear in the status lines; look gives qualitative enemy injury and
hostility. Space continues recovery when unready, or waits normally when ready.
Repeat a valid interrupted attack to resume retained preparation. F8/F9 use
the server’s current readiness permissions for the selected intention. They
can resume eligible suspended/paused work or cancel eligible queued work/combat
preparation; item preparation cannot be cancelled this way. These controls do
not introduce a general travel-cancellation command.

Equipment, removal, drinking and eating use inventory letters, with arrow/Enter selection
also available. Choosing is free; server execution controls timing and effects.
See [items](../reference/items.md) and [creature rules](../reference/creature-rules.md).
Victory/death summaries persist through restart. The [gameplay guide](gameplay.md)
describes what the default adventure actually contains.

`--observe` leaves a player credential unrestricted. For enforced read-only
access, configure the server’s spectator credential and use that value as the
client’s `TOR_SERVER_TOKEN`. Spectators receive actor-perspective state and
permitted history but cannot act, rename or write notes. See
[protocol permissions](../reference/protocol.md).

Wizard lineages show **WIZARD GAME** permanently. Setup and rewind produce fresh
snapshots; abandoned-branch drafts and selections clear. Public observations
remain actor-specific even in a wizard game. Privileged F7 inspection reports are
separate, clear on disconnect/reset, and never become ordinary map knowledge.

## Diagnostics and acceptance

The native frontend uses minifb and bitmap glyphs. Windows and Linux/X11 are
supported; native Wayland is not enabled, so use XWayland or Xvfb. Follow
[setup](../development/setup.md) and [verification](../development/testing.md)
for native desktop prerequisites and the maintained test commands.

`--automation` reads JSON input events on stdin, such as
`{"type":"key","key":"right"}`. It reports frames only after successful native
presentation. `--report-frames` emits diagnostics with ordinary keyboard input;
`--capture file.ppm` alongside either writes the last presented framebuffer.
Each capture is published by replacing the destination only after the complete
PPM is written, so concurrent readers see one complete frame. On Windows, live
readers must share delete access to permit replacement; the test harness's
`read_capture` helper does this. A capture write or replacement failure is
reported as an error, rather than as a successfully captured frame.
Frames include raw observation `narration`, displayed `messages`, `map_tiles`,
`status_lines` and the modal `screen`. These options expose only the connected
actor’s disclosed state/history, potentially including the user’s private notes.
They do not bypass authentication, revisions, control or rules.

Native reports also identify each acknowledged key press and emit a separate
release acknowledgement after the window has processed its release. Consumers
must distinguish `frame` from `native_release` records; release records are not
newly presented images. The [test harness](../development/testing.md#failure-diagnostics-and-timing-evidence)
uses them to synchronize repeated genuine key presses without sleeps.

Diagnostic capture and reporting are synchronous and can delay input. Leave them
off during ordinary play. Timing fields and presentation budgets are explained
in [client architecture](../architecture/clients.md#native-presentation-and-diagnostics);
they do not measure physical keyboard-to-photon latency.

Acceptance suites include
[`test_ascii_process.py`](../../scripts/test_ascii_process.py),
[`test_ascii_messages_process.py`](../../scripts/test_ascii_messages_process.py),
[`test_ascii_memory_process.py`](../../scripts/test_ascii_memory_process.py), and
[`test_travel_process.py`](../../scripts/test_travel_process.py). They test real
frames, native input, control transfer, notes, memory, travel, recovery and saved
continuation. Input-model injection supplements genuine Win32/X11 events; it does
not replace them. Missing displays and failed native launches are test failures,
not successful headless fallback coverage.

Required nutrition appears as a hunger label on the status line and as reserve,
capacity and active-time progress in personal stats. Exempt creatures have no
hunger label. See [nutrition](../reference/creature-rules.md#shared-nutrition-eating-and-starvation).

### Paid identification choices

After an identify scroll is read and consumed, the server supplies one category
and its eligible carried items. Choose by inventory letter or arrows and Enter.
Escape declines this paid choice without refund; Escape in an ordinary item or
direction picker before submission is free. The choice is restored from server
state on reconnect. Selection clears on changed observations or control loss.
Only the owning player sees the choice and explicitly learned instance charges;
spectators see neither. R remains release-control and Z remains the ability menu.
