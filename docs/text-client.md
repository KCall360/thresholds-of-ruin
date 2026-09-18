# Playable text client

The `tor-client-text` executable connects to the existing loopback WebSocket
server. It only uses disclosed protocol observations; rules remain on the server.
The default interface now supports prose, examination, conversational clarification,
and text intentions backed by travel. See [the adventure slice](text-adventure.md)
for commands, place heuristics, interruptions and limitations. It is not yet a full
dungeon adventure.

## Start a game

In one PowerShell terminal, set a session token and start the server:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --listen 127.0.0.1:4000 --seed 42 --save saves/game.json
```

In another terminal, set `TOR_SERVER_TOKEN` to that same token, then run:

```powershell
cargo run -p tor-client-text -- --connect 127.0.0.1:4000 --actor 1
```

On Linux, use `export TOR_SERVER_TOKEN=...` in both terminals; the Cargo commands
are identical. Keep the token out of URLs, command arguments, and committed files.
The client accepts a numeric loopback socket address, including `[::1]:4000`.

The client attaches and requests control. `--observe` skips that request. If control
is occupied, the client stays connected as an observer. The normal prompt is `>`.
Try `examine token`, `take it`, `east`, and `take tablet`. Directions use backend
travel; `stop` cancels, and `step east` requests one careful step. Read
[adventure commands and behavior](text-adventure.md) before scripting this mode.

## Development scripting interface

Pass `--script` to preserve the original line-oriented diagnostic interface.
It prints coordinates, ticks, IDs, inventory, and history; `Ready.` marks completion
of each input. Directions (`east`, `go east`, `e`) move **one cell**. `take token`
requests **immediate** pickup and fails when out of reach. `look`, `inventory`,
`wait`, `control`, `release`, `sync`, `history [before-id]`, `note`, `bookmark`,
`help`, `quit`, and opaque `wizard` commands retain their original behavior.
Names may be full names or trailing noun phrases; ambiguity asks for `take #id`.
This explicit mode keeps existing development scenarios reproducible. The normal
adventure interface is tested separately through actual client processes.

## Annotation commands

For explicit source, audience, category, and anchor:

```text
annotate <user|frontend> <private|actor> <note|bookmark|explanation> <here|state:N|entry:ID> <text>
```

Examples:

```text
note Return here later.
bookmark The starting room.
annotate user actor note here The token is now in my inventory.
annotate frontend private explanation state:1 Pickup completed.
```

Copy a history entry's bracketed ID to use `entry:ID`. A shared note cannot point
to a private entry. The server validates anchors, stamps authorship, and persists
notes without advancing time or action revisions. `frontend` declares intent and
is stamped with component `text`; clients cannot create backend annotations.
History displays source, audience, category, anchor, ID, and tick. Notes are
single-line terminal input, limited to 4096 UTF-8 bytes. Output escapes control
characters, so annotation text cannot issue terminal commands.

## Switching, saves, and disconnects

Run a second text client with `--observe` to see pushed observations and notes.
Use `release` in the first client and `control` in the second to switch control.
Both terminals using the same token are the same authenticated user and can see
that user's private notes. Other-user privacy is enforced by the server.

The server commits accepted actions and notes automatically. Restart it with the
same save path, then relaunch the text client to restore position, inventory, and
visible history. Protocol 10 is required; save format 3 and existing rules are unchanged.

On a lost connection or invalid stream, the client exits with an error. Automatic
reconnect/retry is not implemented. If a command's response is lost, inspect history
after reconnecting before repeating it: the original command may have committed.
Server rejections are displayed without retrying actions. Requests have deadlines;
normal idle observation has no timeout. Reconnection starts a new stream snapshot.

## Validation

Rust tests exercise parsing, ambiguity, annotation validation, terminal-safe prose,
and the existing shared state ordering model. `scripts/test_text_process.py`
builds and launches the actual server and client binaries with temporary saves,
bounded output waits, and cleanup of child processes. It drives gameplay, idle
observer updates, control switching, history pagination, entry anchors, bad
authentication, EOF, disconnects, and server restart persistence. CI runs this
suite in debug and release on Windows and Linux. The [graphical ASCII client](ascii-client.md)
adds native-window tests and text-to-ASCII control-transfer/save-resume acceptance.

## Enforced spectator access

Configure `TOR_SPECTATOR_TOKEN` on the server, then set the client's
`TOR_SERVER_TOKEN` to that spectator credential and launch normally. See the
[server configuration and permission rules](protocol.md#read-only-spectators).
The client automatically stays read-only and displays spectator status. It follows
every accepted actor action/result and disclosed state, with the existing privacy
rules for notes. History remains available; control, gameplay, and note-writing
inputs are blocked locally and independently rejected by the server.

`--observe` with a player credential remains useful for switching frontends; it
does not restrict that credential. Existing saves are compatible, but server and
clients must all use protocol version 10. Real process tests cover live spectator
updates, denied inputs, note privacy, and read-only access after save/resume.

## Wizard games

Both frontends display a permanent **WIZARD GAME** indicator. Setup and rewind
arrive as explicit fresh snapshots; relaunching is not required for surviving
actors. The text client forwards the opaque development commands described
in [wizard mode](wizard-mode.md). Spectators remain read-only. ASCII clears drafts
and selections from an abandoned branch. Server and clients must use protocol 10.

[Unnamed place hints](place-hints.md) now support the initial text place heuristic.
[Text travel](text-adventure.md) composes backend travel with optional pickup on
arrival. Existing saves retain their original rules; travel-v5, doors-v6, shadowcasting-v7, doorway-v8, and material-rims-v10 support travel.

[Door interactions](doors.md) add open/close, examination, clarification and
approach intentions to the adventure interface.
