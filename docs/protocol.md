# Server protocol and annotations

The `tor-server` executable serves authored dungeon scenarios over JSON WebSockets.
`tor-protocol` defines the wire types without depending on world or simulation
internals. `tor-client-common::ClientState` validates ordered updates and keeps
the current disclosed state plus a bounded recent history. The shared `Connection`
transport applies validated snapshots/updates for the [text client](text-client.md)
and [graphical ASCII client](ascii-client.md).
The [headless client](headless-client.md) uses the same transport and exposes
current state and local last-seen cell memory separately for scripted acceptance.

## Run locally

In PowerShell, generate a token for this terminal session and start the server:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --listen 127.0.0.1:4000 --seed 42 --save saves/game.db
```

Clients need that token. Do not put it in a URL or save it in the repository.
The server prints one JSON readiness line containing its address and protocol
version, never the token. Port 0 chooses an available port for tests or launchers.
The seed only applies when creating a save. Existing saves retain their scenario.

This executable configures a player identity, `local`, authorized for the scenario's
actors. An optional spectator identity is described below. The library accepts
explicit accounts with different tokens, roles, and actor permissions; there is no account-registration API. Loopback binding and native
clients are supported now. Browser origins are rejected. Remote TLS deployment
and account administration remain future work; a secure tunnel can carry the
same protocol to a loopback server.

## Decode boundaries

The shared protocol codec decodes complete client and server envelopes directly
into typed messages. It checks UTF-8 byte limits before deserialization: 16 KiB
for requests and 16 MiB for responses, including the entire JSON envelope and
trailing whitespace. WebSocket limits also apply to the complete assembled
message, so fragmentation cannot bypass the byte ceiling.

Both directions permit at most 64 nested objects/arrays, with the root container
counted as one. An allocation-free scan enforces this bound even inside ignored
fields; quoted delimiters and escaped strings do not contribute to nesting.
Serde still checks syntax, escaping, duplicate/unknown fields according to each
DTO's schema, and trailing content. The scan does not build a JSON value tree or
replace semantic snapshot/delta validation.

Malformed wire input terminates that connection. Clients preserve their last
accepted model and reconnect for a fresh snapshot; malformed input does not
start stream repair. Valid decoded messages with a recoverable stream gap follow
the existing bounded repair policy. Request rejection precedes command admission
and publication; an unrelated healthy connection remains usable.

## Read-only spectators

Before starting the server, optionally set `TOR_SPECTATOR_TOKEN` to a separate
random token (for example, another `[guid]::NewGuid().ToString('N')` in PowerShell).
It must differ from `TOR_SERVER_TOKEN` and contain 16-1024 non-control characters.
Omitting it disables spectator login. Invalid configuration fails before opening
or creating a save. Keep both credentials out of URLs, arguments, and source control.

In a spectator's client terminal, set **`TOR_SERVER_TOKEN` to the spectator token**,
then launch either frontend normally. No `--observe` flag is needed. The server
assigns this credential the `spectator` role and identity, authorized for the
scenario's actors. Only share this credential with someone who should watch;
the player credential continues to allow control and annotations.

Spectators may attach, request snapshots, and browse permitted history. All other
requests are rejected with `Unauthorized`, before receipt lookup or mutation,
even when an actor has no controller. Changing frontend labels, retrying an
accepted command, or sending protocol requests outside the official clients does
not elevate access. The role is fixed for the authenticated connection. New
request variants are denied to spectators unless explicitly added to the read
allowlist after review; future wizard mutations must remain denied.

An attached spectator receives every accepted action and result for their actor,
plus the same disclosed state that a controlling client receives. This includes
pickup, movement, and wait; rejected commands and local UI inputs are not gameplay
history. Combat and autonomous actors publish actor-specific health, progress,
qualitative injuries, and perceived combat events through the same stream. This is an
actor-perspective view: hidden rooms and other actors' private commands and
numerical combat attributes are not exposed. Spectators cannot write even private annotations.

Annotation privacy still follows authenticated identity and audience. The built-in
`spectator` user sees actor-visible notes, not `local`'s private notes. The library
can configure read-only and player credentials for the same user; those credentials
share private-note visibility but have independent write authority. Actor allowlists
apply to both roles. The existing `--observe` option merely skips a player client's
initial control request and is not an access restriction.

The protocol requires a backend-resolved observer-relative scene. Positions
are x/y/z offsets, with the actor at the origin. Each `visible_cells` entry has an
opaque `key`, `position`, `wall`, `stairs_up`, `stairs_down`, and `place_hint`, plus nullable `door` facts.
Cells also carry terrain `material` (empty for carved voids). Floors and
ceilings are ordinary seen solid cells, not separate facts; items include `description` and a
`reachable` flag, and actors carry perceived `name` and `description`. These
appearances are described in [the text adventure slice](text-adventure.md). The client receives no region IDs, bounds, names, portal links,
transforms, or visited-region list. Move events report the chosen direction.
The role in `welcome` and permanent wizard marker remain required. Old clients
must upgrade. Only the current save format and ruleset are accepted. See [geometry](portal-geometry.md).
Roles and credentials are startup/session configuration, never journaled.
Restarting requires supplying the desired credentials again.

## Integer representation

All 64-bit wire values use canonical decimal strings: actor, item and door
identities, quantities, ticks, revisions, epochs, stream sequences, durations
and completed travel steps. Unsigned values range from `"0"` through
`"18446744073709551615"`. Signed motion velocity components range from
`"-9223372036854775808"` through `"9223372036854775807"`. Numeric JSON tokens,
leading zeros, plus signs, negative zero, whitespace and exponent notation are
rejected for these fields. Optional quantities retain missing/null semantics.

Protocol versions, bounded collection counts, and 32-bit observer-relative
position components remain JSON numbers. Keep wire strings intact when returning
identities or contexts in requests; use exact integer arithmetic for comparisons
or calculations. This preserves values above JavaScript's exact Number range.
Persistence owns its numeric schemas independently; this wire change does not
change the current save format or ruleset.

## Connection and control

The first frame authenticates and declares a frontend label:

```json
{"type":"hello","protocol":28,"token":"<session token>","frontend":"text"}
```

The server sends `welcome` with the authenticated user, authorized actor IDs, and
server-granted `role` (`player`, `spectator`, or `wizard`), and required typed
`capabilities`. These state `max_request_bytes`, `max_response_bytes`,
`max_retained_state_bytes`, `max_connections` and `max_history_page_entries` as
bounded numeric counts. Clients validate capabilities before attaching and obey
the advertised request ceiling. The response ceiling follows the configured
frame limit; the connection ceiling is min(128, total/frame), 16 by default.
Existing borrowing may exhaust admission before that ceiling. Capacity rejection
uses `resource_limit`, distinct from `invalid_request`, and consumes no client ID.
Capabilities describe static limits, not available capacity or actor authority.
The retained-state ceiling does not promise that a full recovery envelope fits;
frame admission still checks the complete encoded response.
It rejects bad tokens, unsupported versions, and unknown request fields before
disclosing game state. Attach once per connection:

```json
{"type":"request","request_id":"attach-1","request":{"type":"attach","actor":"1"}}
```

The response is a snapshot containing the actor's observation, action revision,
branch ID, stream cursor, control status, and recent visible history. A client can
observe without controlling the actor. `acquire_control` and `release_control`
transfer exclusive control; acquisition fails while another connection controls
the actor. Disconnecting releases control. Switching actors requires reconnecting.

An action uses the branch and revision from the latest observation:

```json
{
  "type":"request",
  "request_id":"move-1",
  "request":{
    "type":"command",
    "context":{"stream":{"stream":"<attachment stream>","epoch":"1"},"readiness_revision":"1"},
    "branch":"<branch from snapshot>",
    "command":{"type":"act","expected_revision":"0","action":{"type":"move","direction":"east"}}
  }
}
```

Gameplay acknowledgements report admission, before simulation execution:

```json
{"type":"ack","context":{"input":{"stream":{"stream":"<attachment>","epoch":"1"},"readiness_revision":"2"},"actor":"1","branch":"<current branch>","cursor":{"sequence":"3","tick":"0"},"revision":"0"},"request_id":"move-1","receipt":{"type":"admitted","actor":"1","branch":"<request branch>","intention":"<opaque intention>","entry_id":"<admission record>","phase":"queued"}}
```

The simulation chooses when the actor's intention executes. Observations disclose
its effects; ordered `intention` updates report lifecycle changes. Snapshots include
active `intentions`, separately from observation readiness. Each actor can hold
one queued or suspended intention, and the simulation bounds the global queue
at 4,096 entries. New admission requires that actor to have no queued intention
and the global queue to have capacity. An attack's `started` phase retains its preparation
identity while wind-up and impact are in progress. `paused` retains inactive
preparation with its spent progress and original target; it can coexist with a
separate queued action. Changed observations precede their lifecycle updates.
Immediate operations acknowledge with a required
`receipt { type: "immediate", actor: ..., branch: ..., entry_id: ... }`.
Both receipt variants identify the original actor and branch. A committed
command's immediate receipt takes that identity from its journal entry; retrying
it after rewind or restart returns the same receipt, even when the live branch
has changed. Session operations without journal entries use the attached actor
and branch at completion and have a null `entry_id`. Receipt identity does not
claim that the client is currently synchronized or ready to act.

Restart and controller loss suspend queued human intentions and pause running
preparation. Rewind restores the selected work and preparation; Session then
suspends human work until fresh input. Acquiring control preserves that state.
Explicit `resume_intention` and `cancel_intention` commands reference the original
opaque identity using the current branch and observation revision:

```json
{"type":"command","context":{"stream":{"stream":"<attachment stream>","epoch":"1"},"readiness_revision":"4"},"branch":"<current branch>","command":{"type":"resume_intention","expected_revision":"7","intention":"<original admission identity>"}}
```

Resume returns suspended work or paused preparation to the simulation queue under
its existing identity. Continued preparation keeps its remaining progress and
revalidates the original target at execution. Replace `resume_intention` with
`cancel_intention` to discard that exact work or preparation. Cancellation
preserves any independently queued intention and does not advance simulation time.
Both operations require controller authority. Their ordered lifecycle updates
precede the acknowledgement. ASCII exposes F8/F9; headless exposes explicit
`resume_intention`/`cancel_intention` inputs using the current disclosed context.
These default controls select queued work before independent preparation,
regardless of snapshot or update delivery order. The ASCII hint uses that same
selection; queued work must be suspended before it can be resumed. Explicit
protocol commands can still reference either disclosed identity.

Requests require unique IDs per authenticated user for accepted actions and
annotations. Retry the exact same command and ID to recover its original receipt,
including after reconnect or restart. Reusing an accepted ID for different
content is an error. Requests rejected before admission are not committed. An
execution-time failure consumes the accepted intention and records its failure,
without substituting another action. A duplicate successful
command from a player account is acknowledged without applying or broadcasting
it again, even after control has moved to another client. Spectator accounts
cannot submit commands, including receipt retries.

## Pushed updates

Clients receive `update` messages without polling:

| Update body | Meaning |
| --- | --- |
| `observation` | New disclosed state, its revision, and an optional actor action/event entry |
| `observation_delta` | The same as `observation`, with cells and ordered collections sent as changes to the previous state on this stream |
| `annotation` | A visible note was committed; game state is unchanged |
| `travel` | Travel status and optional accepted-request history entry; no future route |
| `control` | This connection gained or lost control |
| `readiness` | Authoritative admission and opaque resume/cancel permissions; its counter advances independently of observation revisions |
| `intention` | An opaque intention's lifecycle in this actor/branch context; no authoritative topology or queued action targets |

When play stops for input, the server sends `waiting { on }` after the updates
of that run, and again after a snapshot: `you` (this client controls the actor
that's next), `others` (another client's actor is next), `unclaimed` (a
character no client controls is next), `paused` (AI play waits for a
controller's action or `continue`) or `stopped` (nothing can act). It's sent
once per stop, and again after every answered request, so a client can end its
turn on it instead of waiting for play to go quiet.

Every update has actor and branch identities, a connection-scoped sequence, and
simulation tick. Multiple updates can share a tick. The stream sequence increments
for every delivered update; the action revision increments only when that actor's
disclosed observation changes. A private note neither advances another user's
sequence nor invalidates anyone's pending action revision.

Snapshots and updates require `context { stream, epoch }`. The host creates an
opaque `stream` for each attachment outside simulation randomness and saved
state. Each snapshot reset advances its checked `epoch` while retaining that
attachment identity. Clients reject updates outside their current context and
reject replacement snapshots from another attachment or an equal/older epoch
before changing presentation state. A new connection establishes a fresh model.
An exhausted reset counter closes the attachment rather than wrapping.

### View deltas

Most observation updates are `observation_delta`s. Scalars and optional combat
and motion fields are carried in full. `visible_cells` is replaced with `cells`:

- `shift` is added to the position of every cell in the previous state. Cell
  positions are observer-relative, so a step moves every retained cell.
- `removed` lists positions, after the shift, of cells no longer in view.
- `changed` lists complete cells that entered view or differ from the shifted
  cell at the same position.

The `inventory`, `ground_items`, `visible_actors` and `places` fields contain
ordered edit lists. Each edit is `{ start, remove, insert }`: `start` is an
unsigned 32-bit index into the original base collection, `remove` is the number
of original entries to remove, and `insert` contains complete replacement values.
An empty list retains the collection. Edits must have increasing distinct starts,
nonoverlapping in-bounds ranges and at least one removal or insertion. All ranges
refer to the original base, so applying an earlier edit does not shift later
indices. Result order exactly matches the next full observation, even when the
full collection is unordered. Unknown edit fields are rejected.

Ground items and visible actors are separate occurrences keyed by their relative
position and underlying identity; the same entity seen through multiple portals
remains distinct. Places use disclosed opaque keys and inventory uses item IDs.
These edits expose no private region coordinates or topology. Cell shifts do not
translate non-cell collections; changed projections are represented explicitly.

A stream's retained full `StateView` is limited to 16 MiB of canonical encoded
JSON, independently of the complete-response limit. Shared semantic validation
checks this bound after snapshot or delta reconstruction; a fitting delta cannot
accumulate unlimited retained strings or entries. The host also rejects an
observation exceeding this bound before choosing its delta. A lower configured
frame ceiling may still admit a fitting delta for a retained state within the
protocol ceiling. Counting uses the same serializer with a constant-space byte
sink, without allocating another full JSON buffer. This bound covers the current
observation; remembered map cells and history have separate retention policies.

An `observation_delta` body requires `base { cursor, revision }`, identifying
the last observation within its enclosing context and branch. Its embedded
state delta also names that state's `base_revision`. Both must match: matching
a revision alone does not establish the correct base. Control, annotation,
travel and intention messages consume stream sequence numbers without changing
this observation base. A snapshot reset establishes a new base at its own cursor.

Applying a valid delta yields a full observation with cells sorted by position.
`tor-client-common` validates the context, ordering, exact base and reconstructed
state before publishing changes; rejection leaves state, memory and cursors
unchanged. The shared connection requests one fresh snapshot after an invalid
update or snapshot. It retains that request across canceled waits, discards
old stream updates until the matching reset arrives, and blocks new requests
while unsynchronized. Its ten-second deadline includes sending, flushing and
waiting. A rejected or invalid matching recovery reply fails the connection.

Replies to previously sent requests retain their original identities. A recovery
snapshot without a correlated reply leaves the outcome uncertain, so clients
stop the affected input chain and direct the user to history; they never replay
gameplay automatically.
Normal authoritative resets remain distinct from internally requested recovery.
Queued native input carries its originating snapshot context and is refused after
a reset. Confirmed receipts and rejections are retained while recovery waits for
its snapshot. History and palette contents are discarded while the stream is
uncertain, including messages whose headers still match the old state.
A known acceptance or rejection remains known after repair; a reset alone does
not prove request acceptance or gameplay completion. Clients distinguish an
unknown request outcome only when no correlated reply was received.
Snapshots include required `readiness { revision, admission, resume, cancel }`.
`admission` describes available queue capacity for new gameplay work under current
control and run state; target validation and simulation timing still apply.
`resume` and `cancel` contain only disclosed opaque intention IDs. The simulation
owns their availability checks; the session applies authority and travel policy.
Permission changes arrive as ordered `readiness` updates after their causing
observation, control or lifecycle changes, and before the request acknowledgement.
Each update advances the readiness counter by exactly one without advancing the
observation base or simulation time. Counter exhaustion disconnects the client.
Clients validate permission structure and disclosed identities atomically.
An observation or terminal journey status can arrive before its following permission
update. Gameplay completion must consume that boundary before building a subsequent
command; an observation's `ready` flag alone does not authorize fresh input.
A known execution failure remains rejected if later permission delivery times out.

Every `command` requires `context { stream: { stream, epoch }, readiness_revision }`.
Build it from the current disclosed attachment and readiness generation. The host
checks authenticated role/actor access and resolves an existing receipt before
freshness validation. Fresh commands must match both the last published generation
and current permissions; old streams, reset epochs, ownership generations and
unpublished predicted generations receive `StaleContext` before admission.
A current generation does not authorize disabled gameplay: new action/travel
requests require `admission`, and resume/cancel requests require the intention ID
in the corresponding permission list. The host rejects disabled admission with
`ActorBusy` and unavailable recovery with `InvalidAction`, before any mutation.
Simulation still validates targets, revisions and execution independently.
Control changes advance this generation even if all permission vectors stay empty.
Capture context when constructing input; never restamp queued or retried requests.
An authorized retry returns its original receipt even after context abandonment.

Acknowledgements, history replies and palette messages require a current reply
`context { input, actor, branch, cursor, revision }`. `input` has the same stream,
epoch and readiness-generation shape as command input. This context names the
client's last disclosed boundary at publication, after ordered permission changes;
it does not advance the stream. The receipt still names the original operation,
so a retry after rewind or reconnect can have a different branch or attachment in
its current context. Context is not persisted with the receipt.

Every error requires `scope`. `{"type":"transport"}` marks a transport failure
with no host ordering boundary, including handshake failures and malformed wire
input. `{"type":"unattached"}` marks a host error before a snapshot establishes
an attachment. `{"type":"attached","context":...}` carries the same disclosed
reply context as successful responses. Scope fields are strict and have no
implicit defaults. An attached client treats transport failures as connection
errors and rejects an unattached host scope; neither can confirm a pending host
operation. Scoped host rejections remain known through bounded stream repair.

The shared connection validates successful reply contexts against its exact
currently disclosed boundary. A mismatch within this attachment starts one
bounded snapshot repair. The reply does not install missing state or permissions;
receipts remain known while repair runs, and query payloads are quarantined.
Another attachment or actor is a connection error even during recovery.
Disconnect still requires a new attachment. Broader pressure/reconnect coverage
and protocol closeout remain refactor work. The server compares complete encoded
`ServerMessage` envelopes, including context, cursor and event, and selects a
delta only when it is strictly smaller than the full response. Equal sizes prefer
a full `observation`. Size counting precedes allocation of the selected text;
only that selected text is encoded for outbound admission. A full response that
exceeds the response ceiling may use a fitting delta. If neither representation
fits, the connection closes without publishing a partial response.

Snapshots are always complete; reconnects and rewinds start from one, so a delta
never skips a state. Sequence, observation tick, reset context and disclosure base
advance only after the response enters the ordered output queue. The server then
retains the owned full state as the next base without copying it again. Output
rejection closes that stream rather than retaining an unpublished base.

Readiness is part of that disclosed state. A same-tick turn handoff advances the
revisions of the actors whose readiness changes, even when their geometry and
tick remain unchanged.

Actions currently have one semantic result. Their history timestamp is when the
action took effect; the accompanying observation reflects the next decision
time. The transport supports multiple updates between user decisions as richer
simulation mechanics are introduced. Other actors can receive changed observations
without receiving the acting actor's private command details. Richer cross-actor
event descriptions still belong to the perception work.

Snapshot generation, persistence, control changes, and update publication are
serialized. Queries can request a fresh `snapshot` at any point. Reconnecting
starts a new sequence at zero and always provides a fresh snapshot; resuming an
old transport sequence is not implemented. Durable history IDs remain unchanged.

Each connection has a 64-message output queue. A client that cannot keep up is
disconnected and releases control instead of silently missing updates. It must
reconnect and rebuild from a snapshot. Handshakes and socket writes have deadlines;
client messages are capped at 16 KiB and server messages at 16 MiB, including
JSON envelopes, UTF-8 bytes and escaping. Both limits apply to complete messages
assembled from WebSocket fragments. Shared clients bound request encoding before
sending and reject oversized responses before JSON decoding. A locally oversized
request leaves the connection usable; oversized inbound traffic ends the transport
without starting snapshot repair. Host output limits may be lower than the protocol
ceiling. There are at most 128 connections.

A client whose attached actor leaves the loaded world (see
[region streaming](region-streaming.md#engine-streaming); a spectator watching
an AI actor, say) receives an unsolicited `error` with code `not_attached`
and is disconnected. It may attach again once the actor is back in play.
Messages already queued for a client, such as that error, are delivered
before the connection closes.

## Asset palettes

A scenario can name assets: dotted lowercase identifiers such as
`creature.rat` (see [scenario packages](scenario-packages.md#assets)). Two
things then reach clients:

- **Assets on disclosed things.** Cells, doors, items and actors in an
  observation carry an optional `asset`. It's only ever attached to something
  already disclosed, and a concealed item carries the asset shared by
  everything that looks like it, never its archetype's.
- **A palette per client:** the assets its actor may soon see. It's the
  union of the asset lists of the themes of every region within one portal
  hop beyond the load radius around the actor, plus the run's characters'
  assets. It comes from the package's structure alone, never from what
  regions hold: two games that differ only in hidden contents send identical
  palettes, and because themes belong to zones, entering a room doesn't
  signal what the next one holds.

Palettes have their own revisions and are separate from the observation
stream: `{"type":"palette","request_id":null,"palette":{"revision":"1","body":{"type":"full","assets":[...]}}}`.

- Attaching (so also reconnecting) sends the whole palette, unasked.
- When a client's palette changes, it gets
  `{"type":"delta","base":<previous revision>,"added":[...],"removed":[...]}`.
- The `palette` request returns the whole palette again, with the request's
  id; spectators may send it. A client that sees an asset missing from its
  palette, or misses a revision, should fall back to its own look and ask.
- Nothing is acknowledged. Palettes aren't saved; they're recomputed after a
  restart.
- A scenario that names no assets sends no palettes (a `palette` request
  still gets an empty one), and its observations carry no assets. A game
  that doesn't stream keeps every region loaded, so its palette covers the
  whole package.
- The server recomputes a client's palette only when its actor's region
  changes, since a palette depends on nothing else.

### Palettes in the clients

Every client's connection (`tor_client_common::Connection`) keeps a
`Palette`: the revision and assets it last heard, applied from full palettes
and deltas. It asks for the whole palette itself when:

- a delta doesn't follow the revision it holds (or arrives before any full
  palette). The palette is then *stale*, and nothing is drawn from it until a
  full palette replaces it. While a request is outstanding, nothing more is
  asked.
- an observation names an asset the palette lacks. Each such asset is asked
  about once per connection, so an asset the answer still lacks can't cause a
  loop. Nothing is asked before the first palette, since attaching sends one.

Clients resolve assets through their own built-in `AssetTable`, falling back
through dotted prefixes: `terrain.floor.cave`, then `terrain.floor`, then
`terrain`, then the client's own look. An asset the palette doesn't currently
hold always gets the client's own look, even when the table knows it.

- **Text:** the adventure interface describes floors, walls and ceilings, and
  figures that have no disclosed name, by its asset words
  (`tor_client_text::adventure::words`), so a `terrain.floor.cave` floor is
  "packed earth". Disclosed names of actors, items and doors stay as they are,
  since commands match them. The `--script` interface doesn't use assets.
- **Headless:** every output line reports the palette held; see the
  [headless client](headless-client.md).
- **ASCII:** keeps the palette but doesn't draw with it yet; its glyph table
  belongs with the ASCII redesign.

`crates/client-common/tests/it/palette.rs` drives a `Connection` against a
scripted server through a missed revision and a missing asset; real servers
with the checked-in fixtures send neither, so the process test
(`test_streaming_process.py`) covers the attach palette, a request, and the
text client's words.

## Annotations

Annotations are explicit plain-text history entries, not actions, queries, or
commands embedded in prose. They never enter `Game::act`, consume a turn, alter
the seed, or use simulation randomness. Routine activity remains ordinary events;
this slice does not generate automatic commentary.

```json
{
  "type":"request",
  "request_id":"note-1",
  "request":{
    "type":"command",
    "context":{"stream":{"stream":"<attachment stream>","epoch":"1"},"readiness_revision":"1"},
    "branch":"<branch from snapshot>",
    "command":{
      "type":"annotate",
      "anchor":{"type":"state","revision":"0"},
      "text":"Return here after exploring the gallery.",
      "source":"user",
      "category":"bookmark",
      "audience":"private"
    }
  }
}
```

`source`, `category`, and `audience` default to `user`, `note`, and `private`.
The text must contain non-whitespace content, occupy at most 4096 UTF-8 bytes,
and exclude control characters other than newline and tab. Frontends should render
it as text, not HTML, terminal control sequences, or executable instructions.

Sources and authors:

- User notes receive `Author::User` with the authenticated user ID.
- Frontend notes receive `Author::Frontend` with that user and the connection's
  declared component label. The label is descriptive, not software attestation.
- Backend notes use a trusted `Service::annotate_backend` API and receive
  `Author::Backend`. Client inputs cannot specify this source or supply an author.

The user/frontend distinction expresses the client's intent; the server verifies
the account identity, not whether a human physically typed the text. Backend
producers must write from disclosed actor facts and use annotations sparingly.
The API validates the actor and anchor but cannot infer whether arbitrary prose
contains a spoiler.

Anchors identify either a disclosed state revision (a decision/observation point)
or a visible history entry ID. An action and its current single semantic event
share one entry ID. Notes can also reference earlier notes. Past state revisions
remain valid; future revisions, inaccessible entries, and another branch are
rejected. A shared note cannot reference a private entry and expose its identity.

Audience `private` means only the authenticated author, across their authorized
frontends. Audience `actor` means all authorized observers of that actor. Neither
scope publishes a note to unrelated actors. Backend annotations are actor-scoped.
Both live delivery and history queries apply the same audience rules.

Each record has an opaque UUID, branch ID, actor, creation tick, author, audience,
and content. These server-generated identities use system entropy outside the
simulation. Branch IDs survive replay, and notes keep their original attachments.
Wizard rewind creates distinct branches and retains existing entry anchors.

## History and persistence

Snapshots contain up to 100 recent visible entries in chronological order.
`history` requests accept `limit` (1–100) and an optional `before` entry ID.
`older_before` provides the next pagination cursor. Filtering happens before
pagination, so private entries do not create visible gaps or total-count leaks.

```json
{"type":"request","request_id":"history-1","request":{"type":"history","before":null,"limit":50}}
```

The versioned journal stores scenario inputs, root branch identity, a permanent
wizard marker, actions, privileged inputs/results, annotations,
and accepted-command receipts. Replay applies actions to the deterministic
simulation and restores notes as metadata, checking the recorded results and
timestamps. Unknown format/rules versions and inconsistent journals fail to load;
they are never replaced with an empty game. Tokens and live connection ownership
are not saved.

Ordinary actions and notes publish after bounded in-memory journal admission.
A background worker saves atomic batches. Acknowledged unsaved play can be lost
on a crash. Explicit save, normal player-client exit, and graceful server shutdown
wait for persistence; enabling wizard authority also waits for its permanent
marker. A sidecar `.lock` file prevents concurrent server writers. See
[background saving](background-saving.md) for policy, failure handling, the save
format, and the tested durability boundaries.

```json
{"type":"request","request_id":"save-1","request":{"type":"save"}}
```

The save acknowledgement covers the accepted prefix at request time. Play
doesn't wait for disk I/O, and saving doesn't stop a journey. Spectators cannot request saves.

## Validation

Tests cover real WebSocket connections, two observing frontends, control transfer,
authentication/version rejection, private and shared annotations, reconnects,
durable retries, all three annotation sources, anchor validation, pagination,
failed writes, save locking, corrupt saves, slow clients, and client-state ordering.
A process test launches the actual server, commits an action and note, terminates
it, and verifies both after restart. CI runs these in debug and release builds on
Windows and Linux. The text frontend additionally has actual process tests for
interactive input/output, annotation commands, control transfer, and restart
persistence. Graphical ASCII process tests launch real native windows, exercise
native keyboard events, transfer control between text and ASCII, and compare
disclosed state/history after server restart. Linux uses Xvfb; Windows uses a
native desktop. Both debug and release profiles run on both platforms.

Spectator tests send raw WebSocket mutations (including same-user receipt retries),
check actor authorization and privacy, compare each action/result and disclosed
state, verify pagination and reconnection, and reject old protocol handshakes.
Actual text and native ASCII processes additionally test spectator credentials,
read-only inputs, live gameplay, unchanged saves on denied inputs, and restart.
The server configuration tests cover absent, duplicate, and invalid spectator tokens.

## Wizard commands and retained branches

See [wizard mode](wizard-mode.md) for configuration, commands, limits, and testing.
`--wizard` and a distinct `TOR_WIZARD_TOKEN` permanently mark the save before
listening. Only server-granted `wizard` accounts can submit `command` with a
`wizard` payload; ordinary actor control is not wizard authority. These accounts
have game-wide developer authority, while ordinary actions still require control.
Spectators can also use `history_branch` to read permitted abandoned history.

Example payload inside a branch-checked `command` request:

```json
{"type":"wizard","expected_revision":"0","operation":"teleport 1 2 1 1 0"}
```

`operation` is opaque developer text: only the server parses geometry commands.
See [wizard commands](wizard-mode.md) and [wide joins](portal-geometry.md).
The server's private journal stores structured inputs/results for deterministic
replay. Public wizard history contains only a summary and a rewind flag, even
for its author. Setup consumes no action time and validates world invariants.

After setup, affected clients receive a `snapshot` with empty `request_id`; rewind
sends new-branch snapshots to all surviving attachments. Such snapshots establish
a new stream boundary and do not complete an outstanding request. Old branch
requests cannot mutate the new branch. Clients attached to removed actors
are disconnected. Normal action updates retain their monotonically increasing
stream rules between snapshots.

`history` returns only the current branch. To inspect an earlier branch:

```json
{"type":"request","request_id":"past-1","request":{"type":"history_branch","branch":"<old branch>","before":null,"limit":50}}
```

Filtering remains actor/user scoped and precedes pagination. Notes never move to
a new branch; new entry anchors cannot reference another branch. Wizard parameters
are private to their author so normal spectators do not receive hidden setup facts.

See [unnamed place hints](place-hints.md) for anchor attributes and authoring,
[travel](travel.md) for travel requests and durable receipts, and
[doors](doors.md) for observations, actions, events, and privileged placement.
[Material volumes](material-volumes.md) describe how clients derive floors and
ceilings from seen solid cells, and wizard chamber authoring. [Diagonal movement](diagonal-movement.md)
describes the four diagonal directions and door reach.
Only the current protocol, save format, and ruleset are supported; there are no
historical rules implementations or save importers.

## Durable places

`observation.places` is the complete authoritative list of learned anchor keys
and the names the character knows them by, each with its `origin`
(`invented`, `authored` or `player`). It includes offscreen knowledge, without
positions, bearings, authored region identities or reachability metadata.
`rename_place { expected_revision, key, name }` requires control; names contain
1–80 UTF-8 bytes with no controls or edge whitespace. The journaled command
increments the actor revision without advancing time and produces a
`place_renamed { key, name }` history event in an ordinary observation update.
Retries, actor audiences, snapshots, rewind and strict replay use the existing
boundaries. See [place knowledge](place-knowledge.md).

## Item quantities and knowledge

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.

## Physics disclosure

Observations include optional own-body `motion`: velocity, fixed-point units per cell,
and displacement/impact sensations from the latest action boundary. Static
single-cell diagnostic views may omit it. Gravity fields, hidden collision targets,
and backend frames are never serialized. Visible actor cells can repeat an actor
ID at different observer-relative positions; undisclosed body cells remain hidden.
Both clients narrate involuntary motion and impact, and ASCII F6/F7 browse disclosed
height slices. See [physics](physics.md) for numerical and persistence rules.

## Dungeon combat

Attack actions carry a disclosed target actor ID. Movement never implicitly
attacks. Combat observations carry own HP, preparation/recovery, qualitative
visible-actor injury (`healthy`, `wounded`, `badly_wounded`, `near_death`) and
hostility, the objective's kind when enabled (`retrieve_and_return` or
`reach_exit`) with `exit`, the opaque key of the cell where it's met (the cell
itself is disclosed only when seen), and durable victory/death status.

They also carry `events`: what the action the view follows did, as far as the
observer knows. An `attack` names its `attacker` and `target` and its `outcome`
(`miss`, `no_injury`, `hit`); a participant the observer couldn't see is `null`.
`interrupted` reports the observer's own wind-up being interrupted, and `died`
names a disclosed actor that died, after the blow that killed it. The server
sends facts, never prose: clients write their own sentences, and actor
descriptions and names are empty when nothing is authored. The observer's own
body, seen from another cell, is the actor whose id is the observation's
`actor`.

Combat observations don't carry enemy numerical attributes, AI memory,
internal coordinates, or RNG state. Non-combat diagnostic fixtures
omit the optional combat view.

After reconnecting during preparation, a journaled input boundary preserves
progress and waits for fresh input. Repeat the attack to resume. If a saved run
is in recovery with AI ready, `continue` resumes autonomous scheduling without
starting a new player action; text `wait` and ASCII Space issue it while unready.
Only the attached controller can continue. Spectators cannot request it.
