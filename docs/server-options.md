# Command-line reference

Options and environment variables for the server, the scenario tool, and the
three clients. Every executable also accepts `--help`.

## `tor-server`

```sh
cargo run -p tor-server -- [options]
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--listen <addr>` | `127.0.0.1:4000` | Numeric loopback socket address. Non-loopback addresses are refused. Port 0 picks a free port. |
| `--save <path>` | `saves/game.db` | Save database. If it exists, the game resumes from it and scenario/seed options are ignored. |
| `--seed <n>` | `0` | Seed for a **new** game. |
| `--scenario <dir>` | `scenarios/first-dungeon` | Authored [scenario package](scenario-packages.md) for a new game. |
| `--character <id>` | package default | Starting character to play in a new authored game. |
| `--allow-unvalidated` | off | Allow a structurally valid package whose validation certificate is missing or stale, with a warning. |
| `--wizard` | off | Enable [wizard mode](wizard-mode.md). **Permanently** marks the game, even if no wizard command is used. Requires `TOR_WIZARD_TOKEN`. |
| `--save-target-ms <ms>` | `30000` | Prefer saving once the oldest unsaved record reaches this age. |
| `--save-max-ms <ms>` | `60000` | Start saving even if activity continues. |
| `--save-idle-ms <ms>` | `750` | Quiet time before a target-age save. |
| `--save-queue-bytes <n>` | `8388608` | Bound on encoded unsaved data. |
| `--checkpoint-interval <n>` | `1024` | Journal entries between [checkpoints](checkpoints.md); `0` disables them (for diagnostics), maximum 1,000,000. |
| `--regions <1..=256>` | none | Use the diagnostic performance fixture instead of a scenario. Conflicts with `--scenario` and `--character`. |
| `--actors <1..=8>` | `1` | Actor count for the diagnostic fixture; requires `--regions`. |

Save timing constraints: `--save-max-ms` must be at least the target and at most
one day; `--save-idle-ms` can't exceed the maximum; the queue allows 1 byte to
1 GiB. See [background saving](background-saving.md) for what each setting
means for crash recovery.

On startup the server prints one JSON line with its address and protocol
version. Ctrl+C shuts it down gracefully, waiting for pending saves.

### Environment variables

| Variable | Required | Meaning |
| --- | --- | --- |
| `TOR_SERVER_TOKEN` | Yes | Player credential, 16–1,024 non-control characters. |
| `TOR_SPECTATOR_TOKEN` | No | Separate read-only credential. Must differ from the player token. See [spectators](protocol.md#read-only-spectators). |
| `TOR_WIZARD_TOKEN` | With `--wizard` only | Wizard credential. Must differ from both other tokens. |

Keep tokens out of URLs, command-line arguments, and committed files. In
PowerShell, `[guid]::NewGuid().ToString('N')` generates a suitable token.

## `tor-scenario`

```sh
cargo run -p tor-server --bin tor-scenario -- validate <package-dir>
cargo run -p tor-server --bin tor-scenario -- horizon <package-dir> <region-id> <portal-hops>
```

`validate` checks a package and writes its `validation.json` certificate. Run it
after every package edit. `horizon` prints the structural preload neighborhood
of a region; see [region streaming](region-streaming.md). Errors are JSON on
stderr with a nonzero exit status.

## Clients

All clients read `TOR_SERVER_TOKEN`. Set it to the player, spectator, or wizard
credential; the server decides the role.

| Option | Clients | Default | Meaning |
| --- | --- | --- | --- |
| `--connect <addr>` | all | `127.0.0.1:4000` | Numeric loopback server address (IPv4 or IPv6). |
| `--actor <id>` | all | `1` | Actor to attach to. |
| `--observe` | all | off | Attach without requesting control. This isn't an access restriction. |
| `--script` | text | off | Original line-oriented diagnostic interface; see the [text client](text-client.md#development-scripting-interface). |
| `--bump-attacks hostile\|any\|off` | ASCII | `hostile` | Whether moving into an actor attacks it. |
| `--automation` | ASCII | off | Test-only: read JSON input events on stdin and report presented frames. |
| `--report-frames` | ASCII | off | Test-only: report frames while keeping native keyboard input. |
| `--capture <file.ppm>` | ASCII | none | With `--automation` or `--report-frames`, save the last presented frame. |

See the [text client](text-client.md), [ASCII client](ascii-client.md), and
[headless client](headless-client.md) guides for controls and input formats.
