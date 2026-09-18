# Thresholds of Ruin

A turn-based roguelike with one authoritative Rust backend and interchangeable
frontends: graphical ASCII, interactive fiction, and eventually immersive 3D.
Working title; original code and content inspired by NetHack.

## Status

The simulation and server/protocol slices are implemented: a seeded two-room
fixture, explicit actors, deterministic action timing, movement, inventory pickup,
and actor-specific observations. The local WebSocket server supports pushed
updates, client control transfer, and durable action history with user, frontend,
and backend annotations. A playable text client now supports movement, pickup,
live observation, control transfer, annotations, and paginated history.
The graphical ASCII client now presents a native window with a disclosed-room
map, inventory, notes, history, and explicit control transfer. Both clients can
continue the same saved game. Both support server-enforced read-only spectators
who follow live actions and results and browse permitted history. Enable a separate
spectator credential as described in [the protocol guide](docs/protocol.md#read-only-spectators).
The [wizard mode foundation](docs/wizard-mode.md) provides server-authorized item
and actor placement, teleportation, and bounded rewind with retained branches.
Wizard games are permanently marked in both frontends.

Start playing with [the text client guide](docs/text-client.md).
For the windowed frontend and text-to-ASCII switching, see
[the graphical ASCII guide](docs/ascii-client.md).

See [the simulation slice](docs/simulation-slice.md) for its rules and limitations.
See [the protocol guide](docs/protocol.md) to run the server and understand messages,
annotation audiences, and save/replay behavior.

Windows is the primary platform; Linux is tested from the beginning.

## Development

Install Rust through [rustup](https://rustup.rs/). On Windows, the default MSVC
toolchain also needs Visual Studio Build Tools with the C++ build tools and
Windows SDK. The repository toolchain file selects stable Rust with rustfmt
and Clippy.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --release --locked
python -m unittest discover -s scripts -p "test_*.py" -v
python scripts/check_architecture.py
```

Graphical process tests require a desktop. On Linux install X11 development
libraries, Xvfb, xauth, and xdotool, then run Python discovery under
`xvfb-run -a -s "-screen 0 1280x1024x24"` (see the ASCII guide). Windows tests use
the native desktop. Missing displays are errors, not skipped graphical tests.

Python 3 is used only for development checks; the game remains Rust. The boundary
checker reads Cargo metadata, including optional and platform-specific edges.
GitHub Actions runs these checks on Windows and Linux and builds Rust documentation
with warnings treated as errors. Python discovery includes actual server/text/ASCII
process tests and builds all binaries. To run those process tests against optimized
binaries, set `TOR_TEST_PROFILE=release` before running
`python -m unittest discover -s scripts -p "test_*process.py" -v` (PowerShell:
`$env:TOR_TEST_PROFILE = 'release'`). CI runs both profiles on both platforms.

Dependabot checks weekly for Rust dependency and GitHub Actions updates and
opens reviewable pull requests; updates are not automatically merged.

Read [the architecture](docs/architecture.md), [milestones](docs/milestones.md),
and [development practices](CONTRIBUTING.md) before making changes.

## License

GPL-3.0-only. See [LICENSE](LICENSE). Distributed derivative works must comply
with the GPL; merely operating a modified network service does not trigger a
source-disclosure requirement. See the [GNU GPL text](https://www.gnu.org/licenses/gpl-3.0.html).
