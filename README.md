# Thresholds of Ruin

A turn-based roguelike with one authoritative game server and interchangeable
text-adventure and graphical ASCII clients. Switch clients during a game or
connect a read-only spectator. Regions can join through rotated, non-Euclidean
passages; clients see disclosed observations rather than private world state.

The default five-chamber dungeon has creature-backed combat, equipment and item
interactions: recover the **dawn seal** and return to the entrance. Death is
persistent. A separate **Rogue exploration** scenario provides 26 generated,
persistent floors; its creatures, items, survival systems and victory are not
implemented. This is a development build without an installer; a launcher
script starts local games. Windows and Linux are supported; remote connections
and a future 3D frontend remain planned.

The name is provisional. Code and content are original; NetHack and Rogue are
gameplay references. See [current capabilities and limitations](docs/status.md).

## Play from source

Install [Rust](https://rustup.rs/). The repository selects the stable toolchain.
On Windows, install Visual Studio C++ Build Tools and the Windows SDK. On Linux,
the ASCII window needs X11 development libraries and a working display; see
[platform setup](docs/development/setup.md). Run commands from the repository root.

### Quick start: the launcher

Build once, then open the launcher window, which starts the server and your
clients:

```sh
cargo build --release
python scripts/launch_game.py
```

Choose the player client (text or ASCII) and an optional read-only spectator
(text or ASCII). On **New game**, pick the scenario, seed and save name; on
**Saved games**, pick a save from the list or **Browse…** for a save file
elsewhere. Each game gets its own directory under `saves/games`. Closing the
player client, or **End game**, saves and stops everything, and the window stays
open for the next game. On Windows, text clients open in their own console
windows. `--console` uses terminal menus instead; see
[the launcher reference](docs/reference/cli.md#launch_gamepy). The steps below
run the same pieces by hand.

### 1. Start a new game

Choose a save path that does not already exist. An existing save resumes its
original game instead of adopting the new seed or scenario.

PowerShell:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --seed 42 --save saves/game.db
```

Linux shell:

```sh
export TOR_SERVER_TOKEN="$(openssl rand -hex 16)"
cargo run -p tor-server -- --seed 42 --save saves/game.db
```

Keep the token private. The server listens on loopback only.

### 2. Connect a client

In a second terminal, set `TOR_SERVER_TOKEN` to the same value, then choose:

```sh
cargo run -p tor-client-text
# Or open the graphical ASCII window:
cargo run -p tor-client-ascii
```

In text mode, start with `look`, `help`, `inventory`, and compass directions.
In ASCII, use arrow keys or HJKL/YUBN to move, A to attack, G to pick up, D to
drop, and `_` or a map click to travel. Full controls and examples:

- [Text play](docs/play/text.md)
- [ASCII play](docs/play/ascii.md)
- [Current gameplay](docs/play/gameplay.md)
- [Command-line options and spectator credentials](docs/reference/cli.md)

### 3. Save and resume

Normal client exit requests a durable save. Restart the server with the same
`--save` path to resume. Ordinary action acknowledgements can precede disk
persistence; a crash can lose an unsaved suffix. Saves require the current
formats, and the project does not migrate older pre-release saves. Package games
may need their matching source package for region data not yet copied into the
save. See [saving and recovery](docs/architecture/persistence.md).

## Develop or author content

Start with the [documentation index](docs/README.md), then choose:

- [Setup](docs/development/setup.md) and [development practices](CONTRIBUTING.md)
- [Architecture](docs/architecture/overview.md) and [testing policy](docs/development/testing.md)
- [Scenario authoring](docs/reference/scenario-packages.md) and [generation recipes](docs/reference/generation.md)
- [Shared creature rules](docs/reference/creature-rules.md)
- [AGENTS.md](AGENTS.md) for AI coding agents

Use the maintained verification runner rather than assembling a partial suite:

```sh
python scripts/verify.py quick
```

The default `push` tier is required before publication; full Windows/Linux
debug/release CI is required before merge. Platform prerequisites, focused tests,
separate tooling/dependency jobs and evidence-reuse rules are in the
[testing policy](docs/development/testing.md).

## License

GPL-3.0-only. See [LICENSE](LICENSE). Distributed derivative works must comply
with the GPL; merely operating a modified network service does not trigger a
source-disclosure requirement. See the
[GNU GPL text](https://www.gnu.org/licenses/gpl-3.0.html).
