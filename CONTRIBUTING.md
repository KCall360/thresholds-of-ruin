# Development practices

Use test-driven development for simulation rules, protocol behavior, and client
interactions: write a failing behavior test, implement it, then refactor with
the tests passing. Prefer assertions about outcomes to copies of implementation
details. Keep main green; incomplete acceptance scenarios belong in the milestone
document until implementation starts, not in permanently ignored tests.

Run the commands in README.md before pushing. Both Windows and Linux CI must pass.
Document any checks that could not run locally.

CI also tests optimized builds and builds documentation with `RUSTDOCFLAGS=-D warnings`.
The dependency policy in `scripts/check_architecture.py` enforces declared internal
crate edges for all targets, including optional, build, and development dependencies.
New crates and intentional boundary changes require an explicit policy update.
External library suitability (such as avoiding I/O in simulation code) still
requires review; this guard is not a sandbox for Rust code.

The simulation must not read wall-clock time, access the filesystem/network, or
depend on UI code. Randomness must be explicitly seeded and persistable. Do not
use unordered iteration to resolve simulation outcomes.

The backend owns rules, visibility, appearance facts, and actor knowledge. Clients
may only receive disclosed observations. Do not serialize internal world state
into protocol messages or provide hidden facts through interaction metadata.

Each actor has explicit identity and control ownership. Avoid a global player.
Doors are independent entities; they need not be on portal apertures.

Add protocol and save versioning decisions when those formats change. Preserve
original code and content; NetHack is a gameplay reference, not a source to copy.

Frontend milestones must include tests launching the actual applications in
addition to parser, input-model, presentation-model, and protocol tests. The
graphical tests need an explicitly configured display environment in CI.

Never commit credentials, local saves, or private configuration.
