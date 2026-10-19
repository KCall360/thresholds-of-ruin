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

Structured input and printed state/messages use the protocol's canonical decimal
strings for 64-bit attachment identities, quantities, ticks, revisions and durations,
plus aggregate Health/injury, HD counts/owners, aggregate points, threat and XP prices,
and for signed motion velocity components. Return received strings unchanged in
requests; compare them using exact integer arithmetic. JSON numbers are rejected
for these fields. Protocol versions, bounded counts and 32-bit positions remain
numbers. See [integer representation](protocol.md#integer-representation).

```json
{"type":"inspect"}
{"type":"advancement","command":"overview"}
{"type":"act","action":{"type":"move","direction":"east"}}
{"type":"act","action":{"type":"wait"}}
{"type":"resume_intention"}
{"type":"cancel_intention"}
{"type":"cancel_intention","step":"west"}
{"type":"wizard","command":"creature list"}
{"type":"wizard","command":"rewind initial"}
{"type":"request","request":{"type":"snapshot"}}
{"type":"request","request":{"type":"history","limit":50,"before":null}}
{"type":"request","request":{"type":"release_control"}}
{"type":"request","request":{"type":"acquire_control"}}
```

Entity interactions use [opaque typed targets](protocol.md#entity-target-references),
not numeric IDs. For pickup, copy `state.observation.ground_items[0].item.id`
from a received frame into the `item` field. This is a template, not a literal
request to paste:

```text
{"type":"act","action":{"type":"take","item":"<received i_ target>"}}
```

Ordinary carried-item actions also accept `read` with an opaque `item` target and
`zap` with that target plus a direction. Neither requires a named actor ability.
An Identify scroll is consumed before its owner receives
`state.observation.interactions.identification`: a bounded category, an opaque
`choice` token and currently eligible carried item targets. Send
`{"type":"act","action":{"type":"identify","choice":"<received choice>","item":"<received i_ target>"}}`
to choose, or send an explicit `"item":null` to decline without refund. Keep the
received choice token unchanged; stale or foreign choices reject. Device charge
counts are nullable and disclosed only for the exact instance whose details this
actor knows. Knowing a device type does not reveal another instance's charges.

`advancement` uses the same ordinary bounded navigation and exact preview/confirm
flow as Text and ASCII. Its `command` accepts the commands in the
[advancement guide](../play/advancement.md); it cannot name another actor or
silently refresh a stale confirmation. Frames contain `advancement` display
lines and the current `advancement_page`. Actual confirmed purchase/respec
requests use the existing `act` receipt and intention machinery.

Use the received target unchanged. `act` supplies the current input
context, branch and revision and requires control. `wizard` sends a developer command, in the
same form as the text client's `wizard` command, and likewise supplies the
current branch and revision; the server decides whether the account may use it.
`resume_intention` and `cancel_intention` reference current disclosed work,
supplying its original opaque identity, input context, branch and revision. They
require control. Suspended queued work reuses its queue entry; paused preparation
needs capacity to re-enter while retaining its identity and spent progress.
Cancellation supports pending work and eligible running or paused combat
preparation only when the identity appears in disclosed cancel permissions.
Started item preparation cannot be cancelled, including while paused. Omitting
`step` in this convenience input, or setting it to `null`, cancels in place and
sends an explicit wire null. A direction requests atomic escape only when the own
sequence has a completed prefix and its preparation identity is cancellable. It
selects that preparation ahead of unrelated queued work. Invalid movement rejects
the entire operation; successful movement adds its cost to outstanding sequence
debt. Raw protocol cancellation always requires `step`, even when null.
These inputs do not accept actor/identity overrides; the backend validates current availability.
`request` accepts any structured protocol request. This makes the headless
client suitable for driving privileged scenario setup directly, without
launching the text client. Server validation
always applies. Malformed input, denied access,
and rejected requests yield a `ready` frame with a non-null `error`; the client
remains available. Fatal transport/startup errors are JSON on stderr.

Every stdout line is a JSON frame containing:

- `type`: `ready` at startup and after an input, `update` for idle server messages,
  or `response` for messages received while a request is pending.
- `state`: the current validated disclosed state; `branch`, `cursor`, `role`,
  and `has_control` describe this attachment.
- `readiness`: authoritative admission and disclosed resume/cancel permissions.
- `input_context`: the current stream/epoch and readiness generation. Explicit
  protocol commands must include it as `context`, captured when input is built.
- `history`: up to 100 recent entries disclosed to this identity.
- `intentions`: current disclosed queued, suspended and running work. A primary
  strike result can leave its intention active until the sequence ends.
- `state.observation.combat.sequence`: nullable own multi-strike progress, next
  strike time and cancellation debt. Perceived ability events carry only their
  actual nullable `strike` ordinal, never an opponent's future total.
- `memory`: bounded last-seen appearances, described below; omitted only when
  the client is explicitly started with `--omit-memory`.
- `palette`: the client's current [asset palette](protocol.md#asset-palettes):
  `revision` (null before the first palette arrives), `assets`, and `stale`,
  which is true after a missed revision until a full palette replaces it.
  Palette messages also appear in `message` as they arrive. The client asks for
  the whole palette itself after a missed revision, and once for each asset an
  observation names that the palette lacks; the answer arrives as an `update`
  or `response` frame like any other message.
- `message`: the received protocol message, or null. History query pages and
  accepted action results are available here; no hidden state is added.
- `error`: an input/request error on `ready`, otherwise null.

For long diagnostic runs, `--compact-background` is an explicit opt-in. Raw
`map_page`, `palette` and `waiting` messages still appear unchanged in `message`,
with `type`, `error`, `map_hydration` and `state_omitted: true`. Only their repeated
derived attachment fields are absent: `state`, `branch`, `cursor`, `role`,
`has_control`, `readiness`, `input_context`, `history`, `intentions`, `travel`,
`memory`, `palette`, `advancement`, `advancement_page`, `synchronized` and
`capabilities`. Absence means no derived snapshot was printed, never empty
state. Consumers must read the raw message or wait for a complete frame rather
than clear their state. Every `ready` frame, observation and lifecycle update,
snapshot, protocol error and local error remains complete. Default output is
unchanged; this option can be combined with `--omit-memory`. It changes neither
map hydration nor server traffic and does not remove any raw message or error.

Private strike inspection is available to wizard accounts through the ordinary
`request` command envelope. After `wizard creature inspect <actor-id>`, use
`command.type: "inspect_strike_source"` with the current attached-observer
`expected_revision`, the report's `actor` and `target_revision`, a `source`
selector (`natural`, `selected_melee`, or `ability` with its content `id`), and
required nullable `cursor`. Supply the current `context` and `branch` on the
containing command request. The typed `creature_strike_page` response completes
only its matching request. It is a private query, with no action or receipt.

The main report contains source counts rather than authored strike bodies. Detail
pages carry typed indices and list lengths, source attribution, and `next`;
`next: null` means completion. Pass a returned cursor unchanged to continue.
Revisiting page zero uses its returned `inspection` handle plus its `start`
position, rather than null. A null cursor opens a new source and replaces the
connection's previous handle. A selected-melee ability returns one marker; query
`selected_melee` separately to inspect the actual selected source. Headless emits
one typed reply at a time without accumulating a whole source. After reset,
reconnect or stale-handle rejection, explicitly reopen from a fresh main report.
These details never enter ordinary observations or history.

Consume every frame. A request can receive several updates or unsolicited wizard
snapshots before its matching response. `ready` follows the matching response;
an unsolicited snapshot does not finish a pending request. `inspect` reads local
state without time or network effects. Output can contain the authenticated
user's private notes, according to the server's normal audience rules.

Ability actions use opaque `target` handles, including the observation's
`self_target` for self-care. Include required `supplies`: `{ "kind": "carried" }`
for automatic carried sources, or `{ "kind": "explicit", "items": [...] }` for
exact sources. Each selected row contains only `cost_ordinal` and opaque `item`;
rows are unique and sorted, with at most 32 fragments. Select only disclosed
`known_supply` capabilities. Ground `supply_reachable` is separate from pickup
`reachable`; an adjacent bandage can be usable without being pickable from the
current cell. Physical stack quantities and cooldown disclosure never expose
private holds or promise that admission succeeds.

For gameplay, `ready` acknowledges admission into the simulation queue. Wait for
the matching intention lifecycle update before asserting effects. Consume its
following readiness update before capturing context for the next command. A queued or suspended intention occupies the
actor's queue slot. An immediate request such as `save` or `snapshot` still
finishes at its matching response.

## Current observations and memory

The server persists each actor's last disclosed cell appearances. Clients keep a
bounded local cache and request the remembered neighborhood after attachment,
return or loss of alignment. The headless frontend exposes that cache for
inspection; ASCII uses its aligned [map chart](../architecture/clients.md) to
display dimmed remembered areas. Text describes current observations.

Each memory entry contains an opaque cell key, relative position, terrain,
stairs, a door appearance and item appearances. Each diagnostic ground-item entry
wraps its appearance in `item` with the owning cell's last-seen `position`. Neither
the entry nor its appearance contains entity handles, reachability or action
affordances; memory contains no actors. Item appearances can remain
stale in several cells until each cell is observed again. Inventory always comes
from the current observation. Current sightings replace remembered contents,
including clearing objects that are no longer present; unseen changes do not
refresh knowledge. The local last-seen tick/revision is nullable: a hydrated
appearance does not invent the original observation's timestamp.

Same-branch snapshots refresh the current view. Rewind clears abandoned-future
client knowledge and hydrates from the restored actor's saved knowledge. A new
client after save/reopen receives that same persisted neighborhood. The
`map_hydration` diagnostic reports pending, complete, ambiguous, limited and
failed states plus bounded transfer counts. Gameplay can proceed while pages
arrive, and a new observation cancels a superseded transfer. Appearance pages
are installed atomically; missing or limited coverage is never reported complete.

The cache is independently bounded to 12,474 cell appearances and 8 MiB of
encoded appearance data. Its chart covers the ordinary 63 × 33 viewport at all
six height planes. Larger explored history remains on the server. Geometry is
aligned only through disclosed identities and witnessed placements; ambiguous
placements are suppressed rather than guessed.

For scripted runs that do not inspect these appearances, `--omit-memory` leaves
the top-level `memory` member out of diagnostic frames. This affects JSON output
only: the shared state still receives and installs map pages. All other fields,
including `state.observation`, hydration status, messages, history, intentions,
advancement, readiness, palette and errors, remain present. It changes neither
requests nor gameplay/save behavior. The option reduces repeated output; it
does not bound total logs or complete frames. Process drivers use the explicit
`client(omit_memory=True)` option only when they do not assert remembered cells.

The [geometry model](../architecture/geometry.md) supplies explicit visible cells,
bounded portal sight, rotations, walls and stairs. The protocol requires
observer-relative disclosure. Memory output describes individual cells rather
than room elevations.
Wizard operations still advance their author's actor revision even
when the changed room is hidden; command details remain private.

## Verification

Shared-state tests cover stale contents, refresh/removal, elevation separation,
snapshot retention, rewind reset, reconnect, and atomic rejection of bad streams.
`scripts/test_headless_process.py` drives real server/headless/text processes for
ordinary play, live spectator results, denied writes, control transfer, malformed
input, authentication, save/resume, and wizard-based hidden changes and rewind.
The versioned wizard script is `scripts/fixtures/perception-memory.json`.
These tests join existing Windows/Linux discovery in debug and release; native
text/ASCII process coverage remains required alongside this headless frontend.
Projection tests compare every retained field and actual serialized byte counts,
exercise real projected player/spectator frames and errors, and preserve existing
full-memory refresh, reconnect and rewind coverage. The isolated size assertion
measures diagnostic serialization, not server performance or gameplay latency.

[Unnamed place hints](scenario-packages.md) add perceived cell anchors without labels
or boundaries. Shared memory retains last-seen hints; ASCII doesn't render them;
text uses them as described in [the adventure interface](../play/text.md).

[Backend travel](movement.md) supports known-cell destinations. The
[text adventure interface](../play/text.md) supports travel and approach-then-pickup.

`state.observation.places` contains authoritative durable names and opaque anchor
keys, including offscreen entries. This survives restart independently of the
bounded `memory` cache. A controller can send `rename_place` through the
ordinary `request` command. See [place knowledge](movement.md).
