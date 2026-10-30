# Thresholds of Ruin

A turn-based roguelike with one authoritative game server and interchangeable
text-adventure and graphical ASCII clients. Switch clients during a game or
connect a read-only spectator. Regions can join through rotated, non-Euclidean
passages; clients see disclosed observations rather than private world state.

The default five-chamber dungeon has creature-backed combat, equipment and item
interactions: recover the **dawn seal** and return to the entrance. Death is
persistent. A separate [**Rogue starter slice**](docs/play/rogue.md) authors Warrior
and Mage presets, ordinary low-depth foes and finite supplies on open-ended,
generated persistent floors. Its full 26-species roster, hunger and recovery,
current loot and Amulet retrieval/ascent objective are implemented; calibrated
depth/food pressure and complete-run balance remain unfinished. This is a development build, without an installer or automatic
server launch for the Text and ASCII clients. Windows and Linux are supported.
An optional [procedural 3D prototype](crates/client-3d/README.md) has its own
workspace, checks and private local-server launcher. Remote deployment remains
planned.

The name is provisional. Code and content are original; NetHack and Rogue are
gameplay references. See [current capabilities and limitations](docs/status.md).

## Play from source

Install [Rust](https://rustup.rs/). The repository selects the stable toolchain.
On Windows, install Visual Studio C++ Build Tools and the Windows SDK. On Linux,
the ASCII window needs X11 development libraries and a working display; see
[platform setup](docs/development/setup.md). Run commands from the repository root.

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
In ASCII, use arrow keys or HJKL/YUBN to move, bump a disclosed opponent to
attack, `,` to pick up, `d` to drop, and `_` or a map click to travel. Full controls and examples:

- [Text play](docs/play/text.md)
- [ASCII play](docs/play/ascii.md)
- [Optional 3D prototype](crates/client-3d/README.md)
- [Current gameplay](docs/play/gameplay.md)
- [Choose a Rogue Warrior or Mage](docs/play/rogue.md)
- [Command-line options and spectator credentials](docs/reference/cli.md)

### 3. Save and resume

Normal client exit requests a durable save. Restart the server with the same
`--save` path to resume. Ordinary action acknowledgements can precede disk
persistence; a crash can lose an unsaved suffix. Saves require the current
formats, and the project does not migrate older pre-release saves. Saves retain canonical content and procedural descriptors; unvisited authored
regions may still require the exact immutable compiled bundle. Original TOML
modules are not re-imported on resume. See [saving and recovery](docs/architecture/persistence.md).

## Develop or author content

Start with the [documentation index](docs/README.md), then choose:

- [Setup](docs/development/setup.md) and [development practices](CONTRIBUTING.md)
- [Architecture](docs/architecture/overview.md) and [testing policy](docs/development/testing.md)
- [Scenario authoring](docs/reference/scenario-packages.md) and [generation recipes](docs/reference/generation.md)
- [Shared creature rules](docs/reference/creature-rules.md)
- [AGENTS.md](AGENTS.md) for AI coding agents

Use the maintained verification runner with an explicitly selected affected scope:

```sh
python scripts/verify.py quick
```

Local publication checkpoints are expedited and risk-based; a `quick`, `push`
or `full` run is not mandatory solely because publication is due. The canonical
final-head CI plan and both required platform aggregates must pass before merge. Platform prerequisites, focused tests,
separate tooling/dependency jobs and evidence-reuse rules are in the
[testing policy](docs/development/testing.md).

## License

GPL-3.0-only. See [LICENSE](LICENSE). Distributed derivative works must comply
with the GPL; merely operating a modified network service does not trigger a
source-disclosure requirement. See the
[GNU GPL text](https://www.gnu.org/licenses/gpl-3.0.html).
