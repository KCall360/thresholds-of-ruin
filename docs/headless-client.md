# Headless client and disclosed memory

`tor-client-headless` is a JSON-lines frontend built on `tor-client-common`.
It connects to the actual server as a player, spectator, or authorized wizard.
It has no world, simulation, or server dependency and cannot inspect a save.
Use it for scripted play and perception acceptance without a native window.

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
authorized. Server validation always applies. Malformed input, denied access,
and rejected requests yield a `ready` frame with a non-null `error`; the client
remains available. Fatal transport/startup errors are JSON on stderr.

Every stdout line is a JSON frame containing:

- `type`: `ready` at startup and after an input, `update` for idle server messages,
  or `response` for messages received while a request is pending.
- `state`: the current validated disclosed state; `branch`, `cursor`, `role`,
  and `has_control` describe this attachment.
- `history`: up to 100 recent entries disclosed to this identity.
- `memory`: local last-seen room-elevation views, described below.
- `message`: the received protocol message, or null. History query pages and
  accepted action results are available here; no hidden state is added.
- `error`: an input/request error on `ready`, otherwise null.

Consume every frame. A request can receive several updates or unsolicited wizard
snapshots before its matching response. `ready` follows the matching response;
an unsolicited snapshot does not finish a pending request. `inspect` reads local
state without time or network effects. Output can contain the authenticated
user's private notes, according to the server's normal audience rules.

## Current observations and memory

All clients now retain last-seen room views in their shared `ClientState`.
Only the headless frontend exposes that memory for inspection in this slice;
text and ASCII still present current observations as before.

Each memory entry holds a disclosed region, elevation, ground items, actors,
exits, and last-seen tick/revision. Entries are ordered by region ID and elevation.
An unseen room stays unknown even if its name appears in `known_places` or history.
Leaving a room keeps its last disclosed contents; changes outside the current
view do not refresh those memories. Revisiting replaces that elevation's view,
including removing objects no longer seen. Stale actors/items can appear in more
than one remembered view: these are historical sightings, not current locations.
Inventory remains in current state and is not inferred from remembered objects.

Same-branch snapshots refresh the current view and retain other memories.
Rewind changes branch and clears abandoned-future memory, starting with the
restored snapshot. Memory is local to a connection and is not saved or recovered
from history: restarting a client starts with its attachment snapshot only.

This is a foundation for the existing fully disclosed room-elevation rule.
It does not implement occlusion, portal views, rotated portals, or stairs.
Partial visibility will require explicit disclosed cells before unseen areas can
be retained or refreshed correctly. Protocol version 3 and save format 2 remain
unchanged. Wizard operations still advance their author's actor revision even
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
