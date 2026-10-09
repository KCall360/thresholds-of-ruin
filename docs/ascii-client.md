# Graphical ASCII client

`tor-client-ascii` is a native windowed frontend for the game.
It renders only the server's disclosed observations and shares the same
connection/state validation as the text client. Windows and Linux/X11 are tested.

The latest disclosed action and sight-change prose appears above the status bar,
up to two lines. It shares perceived names and conservative sight-change wording
with adventure text. This transient narration resets on a fresh snapshot; F2
still opens durable history. See [narration and stream recovery](narration-and-recovery.md).

## Run and play

Automation frames include `narration`, the current observation's disclosed prose
as an array (empty immediately after a snapshot). It is presentation data, not
another history stream.

Start the server as described in [the protocol guide](protocol.md), then set
`TOR_SERVER_TOKEN` to the same token in the client terminal:

```sh
cargo run -p tor-client-ascii -- --connect 127.0.0.1:4000 --actor 1
```

Use `--observe` to attach without requesting control. Another client's ownership
does not prevent observation; press F3 once that client releases control. The
server address must be a numeric loopback socket address (IPv4 or IPv6).
Automatic server launch and packaged builds are planned for a later milestone.

The window shows the complete disclosed scene, simulation tick, inventory, visible
items, recent history, and control status. The map uses `@` for your actor, `&`
for another visible actor, `!` for an item, `<`/`>` for stairs, `#` for walls,
`+` for closed doors, `/` for open doors, and `.` for floor. Previously seen areas
and items remain grey when out of sight; unseen actors disappear. Never-seen cells
are blank. See [map memory](ascii-memory.md) for alignment and lifetime limits.
An actor glyph takes precedence over items/stairs on the same cell; visible
items are also listed in the side panel. North is toward the top of the map.
Items elsewhere in the room remain out of reach until you move onto their cell.

| Key | Behavior |
| --- | --- |
| Arrow keys, H/J/K/L | Move west/south/north/east by one cell |
| Y/U/B/N | Move northwest/northeast/southwest/southeast by one cell |
| `<` / `>` | Move up/down where a stair or vertical link exists |
| Space or period | Wait one action |
| O / C, then a direction | Open / close the adjacent door using arrows or HJKL/YUBN; no door means a local message and no ticks |
| G / D | Pick up at your feet / drop from inventory; Up/Down selects, digits set a count, Enter confirms (blank = whole stack) |
| `_` / left mouse click | Select a perceived or remembered travel destination / travel to the clicked floor cell |
| Any key during a journey | Show the rest of the journey at once; journeys can't be cancelled |
| `[` / `]` | Show journey steps more slowly / quickly (`--pace <ms>` sets the start, default 75) |
| F3 / R | Acquire / release actor control |
| F8 / F9 | Resume a suspended queued action / cancel queued work; requires control |
| F4 | Compose a note anchored to the state where composition began |
| Tab in note editor | Switch between private and actor-visible audience; defaults to private |
| Enter / Backspace in note editor | Save / edit the note |
| F2 | Open the latest history page |
| Up/Down in history | Scroll without moving the actor |
| Page Up / Page Down in history | Request an older page / return to live view |
| Escape | Cancel/close an open panel, otherwise quit |

Notes are user-source notes, limited to 4096 UTF-8 bytes. Advanced source/category
and historical-anchor commands remain available in the text client. The graphical
history displays server-stamped source/audience metadata. The bitmap renderer
displays Basic Latin; other characters appear as `?` while the original Unicode
text is preserved in the protocol and save. Long overview labels end with `~`;
the history panel wraps full note text for scrolling.

See [backend travel](travel.md) for destination selection, progress, interruptions,
and compatibility. Text now supports [adventure intentions](text-adventure.md); `--script` preserves
the original one-cell interface.

Only one request is in flight at a time. Movement does not auto-repeat from
holding a key, and gameplay input while a request is pending is discarded rather
than queued into accidental extra turns. Invalid actions and unresolved pickup
choices consume no time. Window resizing and redraws never advance simulation.
The map fits the current scene and nearby remembered cells into the panel, including visible cells
across internal boundaries. Separate visible heights get adjacent panels. The
actor remains at the view origin; cells outside sight use grey last-seen facts. `#` is wall,
`.` floor, `!` item, `&` actor, and `<`/`>` stairs. No portal markers or region
labels are shown. See [observer scenes](portal-geometry.md).
Overview lists show up to five inventory items and four visible items. Richer
item inspection is future work.

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

The frontend uses [minifb](https://docs.rs/minifb/0.28.0/minifb/) for its native
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
observation changes, modal notes/history, control loss, busy input, and bounded
rendering. `scripts/test_ascii_process.py` launches the actual server, text client,
and graphical client. It verifies switching, idle pushes, notes, rejected actions,
authentication failures, window exit/disconnects, and persistence after restart.
One scenario sends native Win32/X11 keyboard events for pickup and exit. The other
scenarios inject UI events through the same input model while presenting real
frames in the native window. They are not substitutes for the native-event test.

For diagnostics, `--automation` reads JSON input events on stdin, for example
`{"type":"key","key":"right"}` or `{"type":"text","text":"A note"}`. It reports
JSON frames only after successful native presentation. `--report-frames` emits
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

ASCII F6/F7 browse disclosed height slices without advancing time; spectators can use them too. Mouse selection follows the displayed slice.

## Combat

Press A to select a visible actor, Up/Down to choose, and Enter to attack.
Manual movement toward a visible hostile attacks by default. Set
`--bump-attacks hostile|any|off` to choose the client interpretation; travel stops
instead of attacking. The header shows exact own HP and attack progress; visible
enemies have qualitative injury descriptions. Space continues saved recovery
when unready, or performs an ordinary wait when ready. Repeat an interrupted
attack to resume valid preparation. Victory/death remain visible after restart.

Travel selection and left click accept currently perceived or remembered walkable
map cells on the selected height slice. Current observations override stale
memory. Unknown cells, remembered walls and known closed doors are rejected;
the server handles stale obstacles and routes using actor knowledge.

Travel selection and left click accept perceived or remembered walkable map cells
on the selected height slice. Current observations override stale memory. Unknown
cells, remembered walls and known closed doors are rejected; the server handles
stale obstacles and routes using actor knowledge.
