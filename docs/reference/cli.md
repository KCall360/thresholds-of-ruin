# Command-line reference

Options and environment variables for the server, the scenario tool, and the
three clients. Arena evaluation and search have a [separate guide](../development/arena.md). The server and clients accept `--help`; `tor-scenario` prints
its usage when its command arguments are missing or invalid.

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
cargo run -p tor-server --bin tor-scenario -- validate <package-dir>
cargo run -p tor-server --bin tor-scenario -- horizon <package-dir> <region-id> <portal-hops>
```

`validate` checks a package and writes its `index.json` and `validation.json` certificate. Run it
after every package edit. `horizon` prints the structural preload neighborhood
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
