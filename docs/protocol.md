# Server protocol and annotations (version 3)

The `tor-server` executable serves the two-room simulation over JSON WebSockets.
`tor-protocol` defines the wire types without depending on world or simulation
internals. `tor-client-common::ClientState` validates ordered updates and keeps
the current disclosed state plus a bounded recent history. The shared `Connection`
transport applies validated snapshots/updates for the [text client](text-client.md)
and [graphical ASCII client](ascii-client.md).
The [headless client](headless-client.md) uses the same transport and exposes
current state and local last-seen room memory separately for scripted acceptance.

## Run locally

In PowerShell, generate a token for this terminal session and start the server:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --listen 127.0.0.1:4000 --seed 42 --save saves/game.json
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
history. The current game has no combat or autonomous mobs. As these systems are
added, their perceived actions/events must enter this stream with integration
coverage. This is an actor-perspective view: hidden rooms and other actors' private
commands are not exposed. Spectators cannot write even private annotations.

Annotation privacy still follows authenticated identity and audience. The built-in
`spectator` user sees actor-visible notes, not `local`'s private notes. The library
can configure read-only and player credentials for the same user; those credentials
share private-note visibility but have independent write authority. Actor allowlists
apply to both roles. The existing `--observe` option merely skips a player client's
initial control request and is not an access restriction.

Protocol version 3 requires the role in `welcome` and the wizard marker in state.
Older clients are rejected and must be upgraded with the server. Save format 2
migrates normal format-1 saves on open; simulation rules remain `two-room-v1`.
Roles and credentials are startup/session configuration, never journaled.
Restarting requires supplying the desired credentials again.

## Connection and control

The first frame authenticates and declares a frontend label:

```json
{"type":"hello","protocol":3,"token":"<session token>","frontend":"text"}
```

The server sends `welcome` with the authenticated user, authorized actor IDs, and
server-granted `role` (`player`, `spectator`, or `wizard`).
It rejects bad tokens, unsupported versions, and unknown request fields before
disclosing game state. Attach once per connection:

```json
{"type":"request","request_id":"attach-1","request":{"type":"attach","actor":1}}
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
    "branch":"<branch from snapshot>",
    "command":{"type":"act","expected_revision":0,"action":{"type":"move","direction":"east"}}
  }
}
```

Requests require unique IDs per authenticated user for accepted actions and
annotations. Retry the exact same command and ID to recover its original receipt,
including after reconnect or restart. Reusing an accepted ID for different
content is an error. Failed commands are not committed. A duplicate successful
command from a player account is acknowledged without applying or broadcasting
it again, even after control has moved to another client. Spectator accounts
cannot submit commands, including receipt retries.

## Pushed updates

Clients receive `update` messages without polling:

| Update body | Meaning |
| --- | --- |
| `observation` | New disclosed state, its revision, and an optional actor action/event entry |
| `annotation` | A visible note was committed; game state is unchanged |
| `control` | This connection gained or lost control |

Every update has actor and branch identities, a connection-scoped sequence, and
simulation tick. Multiple updates can share a tick. The stream sequence increments
for every delivered update; the action revision increments only when that actor's
disclosed observation changes. A private note neither advances another user's
sequence nor invalidates anyone's pending action revision.

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
incoming messages are capped at 16 KiB, and there are at most 128 connections.

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
    "branch":"<branch from snapshot>",
    "command":{
      "type":"annotate",
      "anchor":{"type":"state","revision":0},
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

Every accepted action or note is committed by writing a same-directory temporary
file, flushing its contents, then replacing the journal before publishing updates
or acknowledging success. A failed write leaves in-memory state and history
unchanged. A sidecar `.lock` file prevents concurrent writers and remains on disk
after shutdown; the OS lock is released when the process exits.

This first implementation rewrites and replays the complete journal, making it
suitable for the small scenario. Periodic snapshots and more efficient long-history
storage are still planned. Process-termination recovery is tested; hardware power
loss durability also depends on the filesystem and operating system.

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
{"type":"wizard","expected_revision":0,"operation":{"type":"teleport","actor":1,"position":{"region":2,"x":1,"y":1,"z":0}}}
```

Operations are `place_item` (kind `token` or `tablet`, position), `spawn_actor`
(position, positive `turn_ticks`), `teleport` (actor, position), and `rewind`
(`target`: a retained entry ID, or null for the initial boundary). Each records
private authenticated wizard history with structured inputs/results, separate
from annotations. Setup preserves ordinary action time and validates invariants.

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
