# Graphical ASCII client

`tor-client-ascii` is a native windowed frontend for the game, laid out like
NetHack's terminal interface. It renders only the server's disclosed
observations and shares the same connection/state validation as the text
client. Windows and Linux/X11 are tested.

Wizard accounts can press F7 to enter an administrative command. Enter sends it
and Esc cancels. `creature inspect 2` opens the private report for loaded actor 2;
Up/Down and Page Up/Page Down scroll it. Esc closes the report, and opening ordinary
stats with `@` returns to the attached actor's personal stats. A snapshot reset or
disconnect clears the private report. See [wizard mode](wizard-mode.md).

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
observation (see [narration and stream recovery](narration-and-recovery.md))
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
| `a`-`z` | A creature: the first letter of the last word of its name (`s` for ruin scout, `g` for stone guardian), in a colour fixed by its name; `&` if it has no name |
| `#` (blue-grey) | Wall |
| `#` (tan) | Low wall, ledge or step: solid at your feet, open above |
| `^` | A drop or pit: open at your feet and below |
| `.` | Floor |
| `<` `>` | Stairs up / down |
| `+` `/` | Closed / open door, at foot or head height |
| `)` `[` `!` `%` `(` `=` `"` `?` `/` `$` `*` | Items, by class, at your level or down in a drop |
| grey | Remembered, not in sight now |

Heights more than two cells below or three above your feet aren't drawn: a
stair's far landing, a distant ceiling. Cells are a fixed size, so the map
never rescales; it stays centred on you. Remembered cells within the window
come from [map memory](ascii-memory.md). Creatures out of sight disappear.
No portal markers or region labels are shown; see
[observer scenes](portal-geometry.md).

## Run and play

Start the server as described in [the protocol guide](protocol.md), then set
`TOR_SERVER_TOKEN` to the same token in the client terminal:

```sh
cargo run -p tor-client-ascii -- --connect 127.0.0.1:4000 --actor 1
```

Use `--observe` to attach without requesting control. Another client's ownership
does not prevent observation; press F3 once that client releases control. The
server address must be a numeric loopback socket address (IPv4 or IPv6).
Automatic server launch and packaged builds are planned for a later milestone.

| Key | Behavior |
| --- | --- |
| `h` `j` `k` `l` `y` `u` `b` `n`, arrow keys | Move one cell; moving into a hostile creature attacks it (`--bump-attacks hostile\|any\|off`) |
| Shift + direction | Run until something happens: a message, a creature in sight, or the way ahead isn't open floor. Any key stops a run |
| `<` / `>` | Go up / down where a stair or vertical link is |
| `.` or Space | Wait one action |
| `a` | Choose a creature to attack; the choice is outlined on the map |
| `z` | Choose a granted ability, then a visible target. Up/Down selects, Enter confirms and Esc cancels; choosing is free |
| `g` or `,` | Pick up. A lone object is taken at once; otherwise choose by letter (digits first set a count) |
| `d` | Drop: choose by inventory letter, digits first set a count |
| `w` / `t` / `q` | Equip / remove / drink: always asks, by inventory letter |
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
| F8 / F9 | Resume a suspended or paused action / cancel queued work |
| Enter on the end screen | Close the victory or death summary |
| Escape | Cancel or close what's open, otherwise quit |

Inventory letters stay with an item for as long as you carry it; a letter
freed by dropping an item is reused. In a choice menu, letter keys choose
items rather than acting as commands; arrow keys and Enter also work.

Refusals are said plainly: a move the server refuses reads "You can't go that
way.", travel without a known route "You don't know a way there."

Notes are user-source notes, limited to 4096 UTF-8 bytes. Advanced source/category
and historical-anchor commands remain available in the text client. The bitmap
renderer displays Basic Latin; other characters appear as `?` while the original
Unicode text is preserved in the protocol and save. Long labels end with `~`.

See [backend travel](travel.md) for destination selection, progress, interruptions,
and compatibility. Text now supports [adventure intentions](text-adventure.md); `--script` preserves
the original one-cell interface.

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
- **Creature appearance.** Letters and colours come from names. Authored
  glyph and colour assets for actors (like the terrain palettes) would let
  scenarios choose them.
- **Objective item.** Nothing marks which carried or visible item the
  objective asks for, or which cell is the exit.
- **Travel nearby.** Travel to a visible cell without a known route is
  refused; NetHack's travel moves as close as it can.
- **Stair landings.** Abstract stair landings arrive as ordinary cells far
  below or above; the client hides them by height. A flag on landing
  occurrences would make that exact.

## Switch between clients and resume

1. Start the text client, take an action, and add a note if you like.
2. Start ASCII with `--observe`. It receives the same actor's state and live notes.
3. Enter `release` in text, then press F3 in ASCII.
4. Move around in ASCII.
5. Press R in ASCII and enter `control` in text to switch back.

Accepted actions and notes are saved in background batches. Normal player-window
exit waits for saving; a crash can lose recent play. See [background saving](background-saving.md). Restart
the server with the same save path and relaunch either frontend to resume.
The GUI keeps its last view with a disconnected status if the connection fails;
close and relaunch to reconnect. An uncertain request is not automatically retried.
Inspect history after reconnecting before repeating it. Disconnecting releases
control.

## Development and graphical validation

The frontend uses [minifb](https://docs.rs/minifb/0.29.0/minifb/) for its native
pixel-buffer window and [font8x8](https://docs.rs/font8x8/0.3.1/font8x8/) for bitmap
glyphs. Simulation and protocol crates have no rendering dependencies. A background
worker consumes WebSocket updates independently of window input, through bounded
queues. Slow or invalid streams fail explicitly rather than silently losing state.

On Debian/Ubuntu, install the X11 build/runtime and graphical test tools:

```sh
sudo apt-get install libx11-dev libxcursor-dev libxrandr-dev xvfb xauth xdotool
xvfb-run -a -s "-screen 0 1280x1024x24" python -m unittest discover -s scripts -p "test_*.py" -v
TOR_TEST_PROFILE=release xvfb-run -a -s "-screen 0 1280x1024x24" python -m unittest discover -s scripts -p "test_*process.py" -v
```

An existing X11 desktop also works. Native Wayland is not enabled in this slice;
use XWayland or Xvfb. On Windows, run the Python commands on a native desktop and
use `$env:TOR_TEST_PROFILE = 'release'` for optimized process tests. CI explicitly
configures both environments. Missing displays or native window failures fail
the tests; there is no headless-success fallback or skipped launch-test claim.

Rust tests cover native key mappings, disclosed-cell rendering, pickup ambiguity,
observation changes, modal notes/history, control loss, busy input, bounded
rendering, message composition and `--More--` paging, the single map's height
merging, inventory letters, look, travel-to-stairs, running and plain refusals.
`scripts/test_ascii_messages_process.py` plays the first dungeon's opening fight
through the real server and window and checks the messages, the map and the
on-demand screens. `scripts/test_ascii_process.py` launches the actual server, text client,
and graphical client. It verifies switching, idle pushes, notes, rejected actions,
authentication failures, window exit/disconnects, and persistence after restart.
One scenario sends native Win32/X11 keyboard events for pickup and exit. The other
scenarios inject UI events through the same input model while presenting real
frames in the native window. They are not substitutes for the native-event test.

For diagnostics, `--automation` reads JSON input events on stdin, for example
`{"type":"key","key":"right"}` or `{"type":"text","text":"A note"}`. It reports
JSON frames only after successful native presentation. Frames include
`narration` (the current observation's disclosed prose, as before),
`messages` (the rows shown, whether `--More--` is pending, this turn's text
and the log), `map_tiles` (one per drawn column, with its glyph and `kind`),
`status_lines` and `screen` (which on-demand screen or cursor is open). `--report-frames` emits
the same diagnostics while retaining normal keyboard input. These explicit test
options expose only the connected actor's disclosed state/history, which may
include that user's private notes. `--capture file.ppm` with either option writes
the last presented framebuffer for visual review. They never bypass server
authentication, actor control, revisions, or action validation.

## Enforced spectator access

Configure `TOR_SPECTATOR_TOKEN` on the server, then set the client's
`TOR_SERVER_TOKEN` to that spectator credential and launch normally. See the
[server configuration and permission rules](protocol.md#read-only-spectators).
The client automatically stays read-only and displays spectator status. It follows
every accepted actor action/result and disclosed state, with the existing privacy
rules for notes. History remains available; control, gameplay, and note-writing
inputs are blocked locally and independently rejected by the server.

`--observe` with a player credential remains useful for switching frontends; it
does not restrict that credential. Real process tests cover live spectator
updates, denied inputs, note privacy, and read-only access after save/resume.

## Wizard games

Both frontends display a permanent **WIZARD GAME** indicator. Setup and rewind
arrive as explicit fresh snapshots; relaunching is not required for surviving
actors. The text client forwards the opaque development commands described
in [wizard mode](wizard-mode.md). Spectators remain read-only. ASCII clears drafts
and selections from an abandoned branch.

[Unnamed place hints](place-hints.md) add perceived cell anchors without labels
or boundaries. Shared memory retains last-seen hints; ASCII doesn't render them;
text uses them as described in [the adventure interface](text-adventure.md).

[Material volumes](material-volumes.md) add visible stone enclosure and a header
with the floor material and ceiling height derived from seen solid cells.

## Responsiveness and diagnostic timing

The native loop pumps events at a target 60 Hz and repaints only when state or
input changes. Each turn consumes at most 16 network events and stops starting
another event after four milliseconds. A single update, draw, native call or OS
stall can exceed that budget; this is not a hard real-time deadline.

The dedicated connection worker sends ordered disclosed updates/snapshots through
a 64-event channel instead of copying historical memory into each event. Full
presentation queues backpressure that worker; they never discard intermediate
observations. The window applies all received boundaries before presenting their
combined result. Server queues remain bounded with their existing slow-client
disconnect policy. Invalid streams fail explicitly; relaunch establishes a fresh
snapshot. Automatic reconnect and retained-stream resume are not implemented.

Shared state validates ordering and payload/history consistency before mutation.
Same-branch snapshots retain connection-local memory, branch changes clear it,
and changed observations invalidate pending selections. No client-side simulation
or undisclosed knowledge is added. A stationary aligned map avoids rebuilding
its coordinate index; moving charts still process at most 4096 retained cells.
ASCII indexes disclosed cells and occupants once per tile preparation, preserving
first-occurrence glyph precedence, stale-memory color and visible-only clicking.

Diagnostic frames include `profile.version=1`: update application, drawing,
native presentation/pacing, framebuffer capture, previous report duration and
turn interval, in milliseconds, plus the number of consumed network events.
Native time includes the window library's frame limiter, not just GPU work.
Reports still follow presentation and optional PPM writing. `previous_report_ms`
includes capture, JSON construction, stdout writing/flushing for the preceding
reported frame. Diagnostic I/O is synchronous and can delay input; leave these
options off for ordinary play. They do not measure physical keyboard-to-photon
latency.

Additional diagnostic fields retain that boundary while separating report work:
`presented_unix_ns` samples host time just after native presentation returns;
`previous_report_encode_ms` and `previous_report_write_ms` describe construction/
encoding and stdout writing/flushing for the preceding reported frame. All include
scheduling within the measured interval. The harness can defer its disk logs for
comparison; this does not disable the application's synchronous diagnostic output.
Measured tails and their attribution are in the
[performance plan](performance-persistence.md#open-work).

`test_client_responsiveness_process.py` verifies native Win32/X11 note input while
SQLite saving is deliberately blocked, both with and without a pending checkpoint,
and exercises native input during a 160-action burst, followed by exact final-state,
history and durable-checkpoint checks.
These run alongside the existing disclosure, mouse, rewind and restart tests.

## Remembered places

F5 opens the [durable place list](place-knowledge.md). Up/Down selects an entry;
Enter opens the name editor, Enter saves, and Escape cancels or closes. Spectators
can read the list. Names persist independently of connection-local map memory.

## Item quantities and knowledge

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.

The ASCII client draws every height in one map; see [the map](ascii-client.md#the-map).

## Combat

Press A to select a visible actor, Up/Down to choose, and Enter to attack.
Manual movement toward a visible hostile attacks by default. Set
`--bump-attacks hostile|any|off` to choose the client interpretation; travel stops
instead of attacking. The status lines show exact own HP and whether an attack is
under way, paused or recovering; look (`;`) gives a visible creature's
hostility and qualitative injury. Space continues saved recovery
when unready, or performs an ordinary wait when ready. Repeat an interrupted
attack to resume valid preparation. Victory/death remain visible after restart.

Travel selection and left click accept a column whose cell at your own foot
level is perceived or remembered and walkable. Current observations override
stale memory. Unknown cells, columns known only at head height, remembered
walls and known closed doors are rejected; the server handles stale obstacles
and routes using actor knowledge.
