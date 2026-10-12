# Command-line reference

Options and environment variables for the launcher, the server, the scenario
tool, and the three clients. Arena evaluation and search have a [separate guide](../development/arena.md). The server and clients accept `--help`; `tor-scenario` prints
its usage when its command arguments are missing or invalid.

## `launch_game.py`

```sh
cargo build --release
python scripts/launch_game.py [options]
```

Starts a server on a free loopback port, then the player client and an optional
spectator, with fresh random credentials for each launch. By default it opens a
window (Tkinter; on Linux install `python3-tk`):

- **Player client** and **Spectator (read-only)** choose the clients.
- **New game** chooses the scenario, the seed (**Random** draws another) and the
  save name.
- **Saved games** lists every save in the saves directory with its scenario,
  seed and last save time, newest first. **Browse…** opens a file dialog in the
  saves directory to add a save file from anywhere. A save without a launch
  record asks which scenario it **Started from**.
- **Play** (or double-clicking a save) starts the game; **End game** closes the
  player client, which saves and stops the game. Closing the window during a
  game ends it the same way first.

On Windows, text clients open in their own console windows; elsewhere the text
player uses the terminal that started the launcher, and a text spectator is
unavailable. Choice options prefill the window. With `--console`, numbered
terminal menus ask instead, the text player uses that terminal, and any choice
given as an option skips its menu.

| Option | Default | Meaning |
| --- | --- | --- |
| `--client text\|ascii` | ASCII | Player client. |
| `--spectator none\|text\|ascii` | none | Read-only spectator. A text spectator opens its own console window, which is Windows-only. |
| `--new [name]` | New game | Start a new game in `<saves-dir>/<name>`; without a name, `game-<date>-<time>`. Names must not already exist. |
| `--save <name\|dir\|file>` | none | Resume a save: a game name under the saves directory, a run directory, or a save file anywhere. |
| `--scenario <name\|dir>` | `rogue-exploration` | Scenario for a new game. With `--console` and a resumed game, it overrides the package recorded at creation. |
| `--seed <n>` | random | Seed for a new game, 0 to 2^64-1. Rejected with `--save`; a save keeps its seed. |
| `--bin-dir <dir>` | `target/release` | Directory with `tor-server`, `tor-client-text`, `tor-client-ascii` and `tor-client-headless`. |
| `--scenarios-dir <dir>` | `scenarios` | Packages offered by name; directories with a `scenario.toml`. |
| `--saves-dir <dir>` | `saves/games` | Where new games are created and previous games are listed. |
| `--console` | off | Terminal menus instead of the window. |
| `--pause-on-exit` | off | With `--console`, wait for Enter before the console closes. |
| `--report-frames` | off | Diagnostic: pass `--report-frames` to ASCII clients and log their reports. |

Each game directory holds the save (`game.db`; older launchers wrote
`game.json`), a `launch.json` record of its scenario and seed (no
credentials), and one `logs/<date>-<time>` directory per launch with server
output, client transcripts or logs, and `status.json` process IDs. When the
player client exits, the launcher requests an explicit save through
`tor-client-headless`, then stops the server, the spectator and anything they
started. On Windows each game's processes share a job object, so they also end
if the launcher is killed; unsaved recent turns can then be lost, as after a
crash.

Process tests drive the window with a hidden `--automation` option: JSON
commands on stdin act through its widgets, and it prints a JSON state report
for each command and game event. See `scripts/launcher_window.py`.

## `tor-server`

```sh
cargo run -p tor-server -- [options]
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--listen <addr>` | `127.0.0.1:4000` | Numeric loopback socket address. Non-loopback addresses are refused. Port 0 picks a free port. |
| `--save <path>` | `saves/game.db` | Save database. If it exists, resume it; seed and character options do not replace saved state. `--scenario` may relocate its exact pinned package when needed. |
| `--seed <n>` | `0` | Seed for a **new** game. |
| `--scenario <dir>` | `scenarios/first-dungeon` | Authored [scenario package](scenario-packages.md) for a new game. |
| `--character <id>` | package default | Starting character to play in a new authored game. |
| `--allow-unvalidated` | off | Allow a structurally valid package whose validation certificate is missing or stale, with a warning. |
| `--wizard` | off | Enable [wizard mode](../development/wizard.md). **Permanently** marks the game, even if no wizard command is used. Requires `TOR_WIZARD_TOKEN`. |
| `--save-target-ms <ms>` | `30000` | Prefer saving once the oldest unsaved record reaches this age. |
| `--save-max-ms <ms>` | `60000` | Start saving even if activity continues. |
| `--save-idle-ms <ms>` | `750` | Quiet time before a target-age save. |
| `--save-queue-bytes <n>` | `8388608` | Encoded pending-data budget; copied-source empty-queue exception applies (below). |
| `--checkpoint-interval <n>` | `1024` | Journal entries between [checkpoints](../architecture/persistence.md); `0` disables them (for diagnostics), maximum 1,000,000. |
| `--outbound-frame-bytes <n>` | `16777216` | Maximum encoded response frame size; must not exceed the protocol response limit. |
| `--outbound-client-bytes <n>` | `67108864` | Per-client queued/inflight output byte budget. |
| `--outbound-total-bytes <n>` | `268435456` | Aggregate output byte budget; includes reserved client allowances. |
| `--regions <1..=256>` | none | Use the diagnostic performance fixture instead of a scenario. Conflicts with `--scenario` and `--character`. |
| `--actors <1..=8>` | `1` | Actor count for the diagnostic fixture. Nondefault values require `--regions`; explicit `--actors 1` without it has no fixture effect. |

Save timing constraints: the target must be positive; `--save-max-ms` must be at least the target and at most
one day; `--save-idle-ms` can't exceed the maximum; the queue allows 1 byte to
1 GiB. See [background saving](../architecture/persistence.md) for what each setting
means for crash recovery, including the empty-queue copied-region-source exception.
The queue setting is not a strict bound on all pending memory.

Output limits require `0 < frame <= client <= total`; total is capped by the
implementation's integer/semaphore limits. See [output accounting](protocol.md#pushed-updates).

On startup the server prints one JSON line with its address and protocol
version. Ctrl+C shuts it down gracefully, waiting for pending saves.

### Environment variables

| Variable | Required | Meaning |
| --- | --- | --- |
| `TOR_SERVER_TOKEN` | Yes | Player credential; byte validation is described below. |
| `TOR_SPECTATOR_TOKEN` | No | Separate read-only credential. Must differ from the player token. See [spectators](protocol.md#read-only-spectators). |
| `TOR_WIZARD_TOKEN` | With `--wizard` only | Wizard credential. Must differ from both other tokens. |

Credential validation requires at least 16 UTF-8 bytes after trimming leading and
trailing whitespace, at most 1,024 UTF-8 bytes before trimming, and no control
characters anywhere. Use generated ASCII tokens rather than relying on Unicode
character counts. `TOR_TIMING_DIAGNOSTICS` enables optional diagnostic events;
see [timing correlation](../development/performance.md#opt-in-timing-correlation).

The default scenario is located relative to the server's compile-time source tree,
not the shell's current directory. Use an explicit package path when launching a
built binary elsewhere; standalone distribution is not yet a complete workflow.

Keep tokens out of URLs, command-line arguments, and committed files. In
PowerShell, `[guid]::NewGuid().ToString('N')` generates a suitable token.

## `tor-scenario`

```sh
cargo run -p tor-server --bin tor-scenario -- check <package-dir>
cargo run -p tor-server --bin tor-scenario -- inspect <package-dir>
cargo run -p tor-server --bin tor-scenario -- validate <package-dir>
cargo run -p tor-server --bin tor-scenario -- export <package-dir> <new-dir>
cargo run -p tor-server --bin tor-scenario -- resolve-rules < rules.toml
cargo run -p tor-server --bin tor-scenario -- horizon <package-dir> <region-id> <portal-hops>
```

`check` runs package validation without writing outputs. `inspect` reports resolved
source declarations and provenance; it does not certify geometry. `validate`
checks a package and publishes an immutable compiled artifact, `index.json` and
`validation.json`. Run it after every package edit. `export` writes a self-contained
compiled content bundle to a new directory and never overwrites an existing target.
`resolve-rules` exposes the authoritative Rust resolver to authoring tools using
bounded standard input. `horizon` prints the structural preload neighborhood
of a region; see [region streaming](../architecture/streaming.md). Errors are JSON on
stderr with a nonzero exit status.

## Clients

All clients read `TOR_SERVER_TOKEN`. Set it to the player, spectator, or wizard
credential; the server decides the role.

| Option | Clients | Default | Meaning |
| --- | --- | --- | --- |
| `--connect <addr>` | all | `127.0.0.1:4000` | Numeric loopback server address (IPv4 or IPv6). |
| `--actor <id>` | all | `1` | Actor to attach to. |
| `--observe` | all | off | Attach without requesting control. This isn't an access restriction. |
| `--script` | text | off | Original line-oriented diagnostic interface; see the [text client](../play/text.md#diagnostic-mode). |
| `--pace <ms>` | text, ASCII | text `0`, ASCII `75` | Client presentation delay; never changes simulation time. |
| `--bump-attacks hostile\|any\|off` | ASCII | `hostile` | Whether moving into an actor attacks it. |
| `--automation` | ASCII | off | Test-only: read JSON input events on stdin and report presented frames. |
| `--report-frames` | ASCII | off | Test-only: report frames while keeping native keyboard input. |
| `--capture <file.ppm>` | ASCII | none | With `--automation` or `--report-frames`, save the last presented frame. |

See the [text client](../play/text.md), [ASCII client](../play/ascii.md), and
[headless client](headless.md) guides for controls and input formats.

## `tor-arena`

```sh
cargo run --release --locked -p tor-server --bin tor-arena -- --scenario scenarios/mob-arena --seed 42 --all-ai
```

This offline evaluator uses ordinary Engine admission/execution and emits a
structured report. It does not alter authored packages or existing saves. See
[arena evaluation](../development/arena.md) for bounds, reports, exit codes, paired
batches and resumable search campaigns.
