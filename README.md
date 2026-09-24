# Thresholds of Ruin

A turn-based roguelike with one authoritative Rust backend and interchangeable
frontends: graphical ASCII, interactive fiction, and, eventually, immersive 3D.
The name is provisional; the code and content are original, with NetHack used as
a gameplay reference.

## Current state

The repository contains a playable development slice rather than a complete
dungeon game. It currently provides:

- a deterministic simulation with movement, waiting, pickup, doors, inventory,
  finite material volumes, portal-connected geometry, stairs, visibility, and
  actor-specific perception;
- a local, authenticated WebSocket server with saved action history, annotations,
  control transfer, read-only spectators, travel, replay, and bounded wizard
  rewind;
- playable text and native graphical ASCII clients, plus a JSON-lines headless
  client for scripted acceptance tests; and
- client-held last-seen map memory that never exposes undisclosed world state.

New games use protocol **12**, save format **5**, and ruleset
**`diagonal-v11`**. The server rejects any other protocol, save format, or ruleset.
The current fixture is deliberately small: two rooms and a connecting hall. It is
a proving ground for architecture and interaction, not the planned dungeon.

For the exact implementation matrix and next work, see the
[project status and roadmap](docs/milestones.md). The [documentation index](docs/README.md)
routes to player guides, implementation details, and design reasoning.

## Run locally

Install Rust through [rustup](https://rustup.rs/). On Windows, the default MSVC
toolchain also requires Visual Studio Build Tools with the C++ tools and Windows
SDK. The checked-in toolchain file selects stable Rust, rustfmt, and Clippy.

Start the server in PowerShell:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --listen 127.0.0.1:4000 --seed 42 --save saves/game.db
```

In another PowerShell terminal, set the same token and choose a client:

```powershell
$env:TOR_SERVER_TOKEN = '<same token>'
cargo run -p tor-client-text -- --connect 127.0.0.1:4000 --actor 1
# or
cargo run -p tor-client-ascii -- --connect 127.0.0.1:4000 --actor 1
```

See the [text client guide](docs/text-client.md),
[ASCII client guide](docs/ascii-client.md), or
[headless client contract](docs/headless-client.md) for controls and other roles.
The server accepts numeric loopback addresses only; remote deployment and account
administration are not implemented.

Ordinary acknowledgements do not wait for disk. See [background saving](docs/background-saving.md)
for configurable save timing, crash rollback, and explicit save/normal-exit barriers.

## Development

Run the full local verification suite before publishing a change:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --release --locked
python -m unittest discover -s scripts -p "test_*.py" -v
python scripts/check_architecture.py
```

Graphical process tests require a desktop. Linux CI supplies X11 development
libraries, Xvfb, xauth, and xdotool and runs Python discovery under
`xvfb-run -a -s "-screen 0 1280x1024x24"`. Windows uses the native desktop.
Missing displays are test failures rather than skipped graphical tests.

Python is used only for development checks. GitHub Actions runs the checks on
Windows and Linux, in debug and release where applicable, and builds Rust API
documentation with warnings denied. Set `TOR_TEST_PROFILE=release` to point the
process tests at optimized binaries.

Read [the architecture](docs/architecture.md),
[project status and roadmap](docs/milestones.md), and
[development practices](CONTRIBUTING.md) before making changes.

## Platform and scope

Windows is the primary platform; Linux is tested continuously. Automatic server
launch, packaged builds, remote networking, procedural dungeon generation,
combat, death, and the exit objective remain roadmap work.

## License

GPL-3.0-only. See [LICENSE](LICENSE). Distributed derivative works must comply
with the GPL; merely operating a modified network service does not trigger a
source-disclosure requirement. See the
[GNU GPL text](https://www.gnu.org/licenses/gpl-3.0.html).
