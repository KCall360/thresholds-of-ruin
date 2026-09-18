# Thresholds of Ruin

A turn-based roguelike with one authoritative Rust backend and interchangeable
frontends: graphical ASCII, interactive fiction, and eventually immersive 3D.
Working title; original code and content inspired by NetHack.

## Status

Foundation stage. This repository contains the architecture, Rust workspace,
foundational geometry and observation-stream code, and automated tests. It does
not yet contain a playable game, network server, or runnable frontends.

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

Python 3 is used only for development checks; the game remains Rust. The boundary
checker reads Cargo metadata, including optional and platform-specific edges.
GitHub Actions runs these checks on Windows and Linux and builds Rust documentation
with warnings treated as errors. The initial integration
tests exercise crate boundaries; actual frontend launch tests will be added
alongside the first runnable clients.

Dependabot checks weekly for Rust dependency and GitHub Actions updates and
opens reviewable pull requests; updates are not automatically merged.

Read [the architecture](docs/architecture.md), [milestones](docs/milestones.md),
and [development practices](CONTRIBUTING.md) before making changes.

## License

GPL-3.0-only. See [LICENSE](LICENSE). Distributed derivative works must comply
with the GPL; merely operating a modified network service does not trigger a
source-disclosure requirement. See the [GNU GPL text](https://www.gnu.org/licenses/gpl-3.0.html).
