# Graphical ASCII client

`tor-client-ascii` is a native windowed frontend for the two-room playable slice.
It renders only the server's disclosed observations and shares the same
connection/state validation as the text client. Windows and Linux/X11 are tested.

## Run and play

Start the server as described in [the protocol guide](protocol.md), then set
`TOR_SERVER_TOKEN` to the same token in the client terminal:

```sh
cargo run -p tor-client-ascii -- --connect 127.0.0.1:4000 --actor 1
```

Use `--observe` to attach without requesting control. Another client's ownership
does not prevent observation; press C once that client releases control. The
server address must be a numeric loopback socket address (IPv4 or IPv6).
Automatic server launch and packaged builds are planned for a later milestone.

The window shows the current room, position, simulation tick, inventory, visible
items, recent history, and control status. The map uses `@` for your actor, `&`
for another visible actor, `!` for an item, `+` for a passage, and `.` for floor.
An actor glyph takes precedence over items/passages on the same cell; visible
items are also listed in the side panel. North is toward the top of the map.
Items elsewhere in the room remain out of reach until you move onto their cell.

| Key | Behavior |
| --- | --- |
| Arrow keys, H/J/K/L | Move west/south/north/east by one cell |
| U / D | Request movement up/down; the current fixture has no vertical route |
| Space or period | Wait one action |
| G | Pick up an item at your feet; choose with Up/Down and Enter if several match |
| C / R | Acquire / release actor control |
| N | Compose a note anchored to the state where composition began |
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

Only one request is in flight at a time. Movement does not auto-repeat from
holding a key, and gameplay input while a request is pending is discarded rather
than queued into accidental extra turns. Invalid actions and unresolved pickup
choices consume no time. Window resizing and redraws never advance simulation.
The map uses a bounded viewport centered on the actor for larger future rooms.
Overview lists currently show up to five inventory items and four visible items;
the present fixture has only two items. Richer item inspection is future work.

## Switch between clients and resume

1. Start the text client, enter `take token`, and add a note if desired.
2. Start ASCII with `--observe`. It receives the same actor's state and live notes.
3. Enter `release` in text, then press C in ASCII.
4. Press Right four times to reach the Gallery with the token.
5. Press R in ASCII and enter `control` in text to switch back.

Accepted actions and notes are committed automatically by the server. Restart
the server with the same save path and relaunch either frontend to resume.
The GUI keeps its last view with a disconnected status if the connection fails;
close and relaunch to reconnect. An uncertain request is not automatically retried.
Inspect history after reconnecting before repeating it. Disconnecting releases
control. No protocol or save version changes were needed for this frontend.

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
