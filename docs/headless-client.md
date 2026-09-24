# Headless client and disclosed memory

`tor-client-headless` is a JSON-lines frontend built on `tor-client-common`.
It connects to the actual server as a player, spectator, or authorized wizard.
It has no world, simulation, or server dependency and cannot inspect a save.
Use it for scripted play and perception acceptance without a native window. It
is also the preferred frontend for authorized wizard setup and scenario-driving
in new automation; use the text client when the behavior under test is
specifically text input or presentation.

## Run

Start the server as described in [the protocol guide](protocol.md). Set the
client's `TOR_SERVER_TOKEN` to the appropriate credential, then run:

```sh
cargo run -p tor-client-headless -- --connect 127.0.0.1:4000 --actor 1
```

Players request control on startup; `--observe` skips that request. Spectators
automatically stay read-only. Numeric loopback addresses are required. EOF,
`{"type":"quit"}`, or Ctrl+C disconnects. Connection loss and response timeouts
exit unsuccessfully; uncertain commands are never automatically retried.

## JSON-lines contract

Send one input object per line, waiting for `ready` before the next command:

```json
{"type":"inspect"}
{"type":"act","action":{"type":"take","item":1}}
{"type":"act","action":{"type":"move","direction":"east"}}
{"type":"act","action":{"type":"wait"}}
{"type":"request","request":{"type":"snapshot"}}
{"type":"request","request":{"type":"history","limit":50,"before":null}}
{"type":"request","request":{"type":"release_control"}}
{"type":"request","request":{"type":"acquire_control"}}
```

Use an item ID from the received observation. `act` supplies the current branch
and revision and requires control. `request` accepts a structured protocol
request, including explicitly branch/revision-checked wizard commands when
authorized. This makes the headless client suitable for driving privileged
scenario setup directly, without launching the text client. Server validation
always applies. Malformed input, denied access,
and rejected requests yield a `ready` frame with a non-null `error`; the client
remains available. Fatal transport/startup errors are JSON on stderr.

Every stdout line is a JSON frame containing:

- `type`: `ready` at startup and after an input, `update` for idle server messages,
  or `response` for messages received while a request is pending.
- `state`: the current validated disclosed state; `branch`, `cursor`, `role`,
  and `has_control` describe this attachment.
- `history`: up to 100 recent entries disclosed to this identity.
- `memory`: local last-seen cell contents, described below.
- `message`: the received protocol message, or null. History query pages and
  accepted action results are available here; no hidden state is added.
- `error`: an input/request error on `ready`, otherwise null.

Consume every frame. A request can receive several updates or unsolicited wizard
snapshots before its matching response. `ready` follows the matching response;
an unsolicited snapshot does not finish a pending request. `inspect` reads local
state without time or network effects. Output can contain the authenticated
user's private notes, according to the server's normal audience rules.

## Current observations and memory

All clients now retain last-seen cells in their shared `ClientState`.
The headless frontend exposes this historical memory for inspection. ASCII now
uses a separate aligned [map cache](ascii-memory.md) to display dimmed remembered
areas; text continues to describe current observations.

Each memory entry holds an opaque cell key, last-seen relative position, wall and
stair facts, contents, and last-seen tick/revision. Entries are ordered by opaque
key. A remembered offset is historical, not a current map coordinate. Unseen
cells stay unknown; history never manufactures sightings.
Changes outside the current visible cells do not refresh memory. Observing a cell
again replaces its remembered contents, including removing absent objects; other
cells in the same room can remain stale. Stale actors/items can appear in more
than one remembered cell: these are historical sightings, not current locations.
Inventory remains in current state and is not inferred from remembered objects.

Same-branch snapshots refresh the current view and retain other memories.
Rewind changes branch and clears abandoned-future memory, starting with the
restored snapshot. Memory is local to a connection and is not saved or recovered
from history: restarting a client starts with its attachment snapshot only.

The [portal-geometry slice](portal-geometry.md) introduces explicit visible cells,
bounded portal sight, rotations, walls and stairs. Protocol version 11 requires
observer-relative disclosure. Memory output now describes individual cells rather
than room elevations.
Wizard operations still advance their author's actor revision even
when the changed room is hidden; command details remain private.

## Verification

Shared-state tests cover stale contents, refresh/removal, elevation separation,
snapshot retention, rewind reset, reconnect, and atomic rejection of bad streams.
`scripts/test_headless_process.py` drives real server/headless/text processes for
ordinary play, live spectator results, denied writes, control transfer, malformed
input, authentication, save/resume, and wizard-based hidden changes and rewind.
The versioned wizard script is `scripts/scenarios/perception-memory.json`.
These tests join existing Windows/Linux discovery in debug and release; native
text/ASCII process coverage remains required alongside this headless frontend.

[Unnamed place hints](place-hints.md) add perceived cell anchors in protocol 6.
They carry no labels or boundaries. Shared memory retains last-seen hints; ASCII does not render them; text now uses them as described in
[the adventure slice](text-adventure.md). Saves use `diagonal-v11`.

[Backend travel](travel.md) supports known-cell destinations. The
[text adventure interface](text-adventure.md) supports travel and approach-then-pickup.
