# Development setup

Run commands from the repository root. Read [contribution practices](../../CONTRIBUTING.md)
and the [testing policy](testing.md) before changing code. Windows and Linux are
supported; native tests require a usable graphical desktop on either platform.

## Rust and Git

Install Git and Rust through rustup, including the native compiler/linker required
by your platform (Visual Studio C++ build tools on Windows; a C/C++ build toolchain
on Linux). The repository's [rust-toolchain.toml](../../rust-toolchain.toml) selects
stable Rust with `rustfmt` and `clippy`:

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
git fetch origin main
cargo build --workspace --bins --locked
```

On Windows, install Git for Windows with its bundled Bash and keep `git.exe` on
`PATH`. Workflow-contract tests locate that native Bash to execute the checked-in
Ubuntu gate command. On Linux, Bash must be available on `PATH`.

`cargo check` does not update runnable executables. Keep the lockfile unchanged
unless dependency changes are part of the task. `verify.py` compares local changes
against `origin/main` by default; a shallow, offline or differently named checkout
needs an available comparison ref and `--base REF`. Do not use `--ci-profile` or
`--ci-partition` to silently change local coverage; these selectors serve the
complete [CI matrix](testing.md#running-the-checks).

The optional [3D prototype](../../crates/client-3d/README.md) is a separate Cargo
workspace. Core workspace commands do not build its executable or fetch Bevy.
Use its guide for builds and [its independent checks](testing.md#independent-3d-client)
when maintaining it; it shares the core backend's current protocol.

## Python test environment

Use a Python interpreter supported by the hash-pinned wheels in
[requirements-arena-search.txt](../../.github/requirements-arena-search.txt), with
`venv` and `pip` available. Python 3.12 is a suitable starting choice; dependency
installation must succeed before claiming the test environment is ready. The
source uses modern Python features including `tomllib` (Python 3.11+). An unsupported
platform/interpreter must not bypass the dependency lock or silently skip sampler
tests. Search dependencies belong only to development tools, not the game runtime.

Linux:

```sh
python3 -m venv target/arena-search-python
target/arena-search-python/bin/python -m pip install --require-hashes -r .github/requirements-arena-search.txt
. target/arena-search-python/bin/activate
python scripts/verify.py quick
```

Windows PowerShell:

```powershell
python -m venv target/arena-search-python
target/arena-search-python/Scripts/python.exe -m pip install --require-hashes -r .github/requirements-arena-search.txt
$env:PATH = "$PWD/target/arena-search-python/Scripts;$env:PATH"
$env:PYTHONUTF8 = '1'
$env:CARGO_BUILD_JOBS = '2'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
python scripts/verify.py quick
```

Use the same environment for direct Python checks and arena tools. On the
maintainer's Windows host, use workspace-local temporary files and the bundled
runtime Python for scripts needing Pillow; system Python does not include it.
Do not edit Rust build inputs while a build is running.

## Native display prerequisites

Linux CI installs these packages in addition to its compiler/Python toolchain:

```sh
sudo apt-get update
sudo apt-get install -y xvfb xauth xdotool libx11-dev libxcursor-dev libxrandr-dev
```

Both CI partitions install the Linux development libraries needed by the ASCII
crate. Only Python/native partitions install the virtual display/input tools or
check the Windows desktop; Rust-only partitions do not require a display.

A real desktop or Xvfb must be available for native ASCII tests and benchmark
drivers. The verifier configures its Linux process-test display; for direct Python
process commands without a desktop, use:

```sh
xvfb-run -a -s "-screen 0 1280x1024x24" python -m unittest discover -s scripts -p test_travel_process.py -v
```

Windows requires an interactive native desktop. A denied mouse/keyboard API call
is an environment failure, not a passing test. The [one exact local mouse
exception](testing.md#focused-iteration-and-local-mouse-exception) requires an
explicit verifier option; CI still runs and must pass that unchanged test.

## First run and verification

Use the [player quick start](../../README.md) to create a fresh save and connect a
client. [CLI options](../reference/cli.md) define tokens, numeric loopback addresses,
scenario paths and resume behavior. Keep credentials and saves outside Git.

Run focused checks while editing, `quick` at cohesive checkpoints, and the default
`push` tier before every push unless a successful unchanged `full` run covers it.
The final commit must pass its canonical selected or exhaustive CI plan and both
required platform aggregates; selected assurance is not a full-matrix claim.
Separate workflow/Python/dependency lint jobs are listed in the
[testing policy](testing.md#running-the-checks). Record failures and checks not run;
setup completion or a historical green run is not current verification evidence.
