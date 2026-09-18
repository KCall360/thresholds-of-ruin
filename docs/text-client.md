# Playable text client

The `tor-client-text` executable connects to the existing loopback WebSocket
server. It only uses disclosed protocol observations; rules remain on the server.
This is the two-room movement/pickup slice, not yet a full dungeon adventure.

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

The client attaches, describes the room, shows inventory and recent history, and
requests control. `--observe` skips that control request. If control is occupied,
the client stays connected as an observer. Enter one command per line; `Ready.`
marks completion of each input. Piped command sequences work too. Live updates
are printed even while waiting for terminal input. There is no full-screen UI.

Try `take token`, `inventory`, and four `east` commands to reach the Gallery.
Positions and passage locations are shown because movement is cell-based.
`north` decreases y; `south` increases y. Seeing an item does not make it reachable.

## Commands

| Command | Behavior |
| --- | --- |
| `look`, `l` | Describe the latest disclosed state without advancing time |
| `inventory`, `i` | List carried items |
| `north/east/south/west/up/down`, `n/e/s/w/u/d`, `go east` | Request one movement action |
| `take token`, `take the copper token`, `take #3` | Resolve a disclosed ground item and request pickup |
| `wait`, `.` | Spend one wait action |
| `control`, `release` | Acquire or release exclusive actor control |
| `sync` | Obtain a fresh snapshot |
| `history`, `history <before-id>` | Show up to 50 visible history entries and the next older-page command |
| `note <text>`, `bookmark <text>` | Private user annotation anchored to the current revision |
| `help`, `?` | List commands |
| `quit`, `q`, EOF, Ctrl+C | Disconnect; the server releases control |

Names are case-insensitive and may be a full name or trailing noun phrase. If
several disclosed items match, the client lists their names and IDs and asks for
`take #id`; this clarification does not send an action or advance time. Arbitrary
undisclosed IDs are rejected locally. The backend still validates reach and rules.

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
visible history. No protocol or save version change is needed for this frontend.

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
