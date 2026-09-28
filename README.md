# Thresholds of Ruin

A turn-based roguelike dungeon crawler with one authoritative game server and
interchangeable frontends. Play the same game as a **text adventure** or in a
**graphical ASCII** window, switch between them mid-game, or let a friend watch
through a read-only spectator view. An immersive 3D frontend may come later.

The world is built from connected regions that don't need to fit together in
ordinary space: passages can rotate, loop back on themselves, or lead sideways
into a shaft with its own gravity. You only ever see what your character can
perceive. The server never sends a client anything the character hasn't
discovered.

The name is provisional. The code and content are original, with NetHack as a
gameplay reference.

## What you can play today

The default adventure is a compact five-chamber dungeon. Fight its inhabitants
(a scout, a guardian, and a wisp), recover the **dawn seal** from the far
chamber, and return to the entrance. Death is permanent.

Along the way you'll find:

- timed melee combat with typed damage and enemies that search, attack, and
  flee based on what they can see and remember;
- items that stack and split, and a corpse and dropped belongings where
  anyone falls;
- doors, stairs, diagonal movement, and travel to any place you've seen;
- places your character names as they discover them, which you can rename; and
- a map that remembers what you've seen, greyed out once it's out of sight.

This is an early development build: there's one authored dungeon, no
procedural generation yet, and no packaged installer.

## Play

You need [Rust](https://rustup.rs/). On Windows, the default MSVC toolchain also
needs Visual Studio Build Tools with the C++ tools and Windows SDK. The
repository's toolchain file selects stable Rust automatically.

**1. Start the server.** It needs a secret token that clients use to connect.
In PowerShell:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --seed 42 --save saves/game.db
```

On Linux, run `export TOR_SERVER_TOKEN=$(openssl rand -hex 16)` instead of the
first line.

**2. Connect a client** in another terminal, after setting `TOR_SERVER_TOKEN` to
the same token:

```powershell
cargo run -p tor-client-text                 # text adventure
cargo run -p tor-client-ascii                # graphical ASCII window
```

In the text client, try `look`, `examine`, `take`, `attack`, compass directions
like `east`, and `help`. In the ASCII client, use the arrow keys or HJKL/YUBN to
move, A to attack, G to pick up, and `_` or a mouse click to travel.

To resume later, restart the server with the same `--save` path. Quitting a
client normally saves first; a crash can lose the last few moments of play.

**Guides:** [text client](docs/text-client.md) and
[adventure commands](docs/text-adventure.md) ·
[ASCII client](docs/ascii-client.md) · [dungeon rules](docs/dungeon.md) ·
[all options](docs/server-options.md)

The server only accepts connections from the same computer. Remote play,
packaged builds, and automatic server launch are on the
[roadmap](docs/milestones.md).

## Develop

Start with the [documentation index](docs/README.md), then read:

- [development practices](CONTRIBUTING.md): architecture rules, compatibility,
  documentation, and publishing;
- the [testing policy](docs/testing.md): every change needs unit, integration,
  and real-application tests, passing on Windows and Linux in debug and release,
  with performance tooling kept up to date;
- the [architecture](docs/architecture.md) and the
  [project status and roadmap](docs/milestones.md); and
- [AGENTS.md](AGENTS.md) if you're an AI coding agent.

Run the full local check suite before publishing a change:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --release --locked
python -m unittest discover -s scripts -p "test_*.py" -v
python scripts/check_architecture.py
```

The Python suite launches the real server and clients, including native windows,
so it needs a desktop (Linux CI uses Xvfb). GitHub Actions runs every check on
Windows and Linux. See the [testing policy](docs/testing.md#running-the-checks)
for details, including release-mode process tests and API documentation builds.

Windows is the primary platform; Linux is tested continuously.

## License

GPL-3.0-only. See [LICENSE](LICENSE). Distributed derivative works must comply
with the GPL; merely operating a modified network service does not trigger a
source-disclosure requirement. See the
[GNU GPL text](https://www.gnu.org/licenses/gpl-3.0.html).
