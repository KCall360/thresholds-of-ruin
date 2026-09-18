# Development practices

Use test-driven development for simulation rules, protocol behavior, and client
interactions: write a failing behavior test, implement it, then refactor with
the tests passing. Prefer assertions about outcomes to copies of implementation
details. Keep main green; incomplete acceptance scenarios belong in the milestone
document until implementation starts, not in permanently ignored tests.

Every new feature must include automated behavior tests at the layers it changes
and an integration acceptance scenario for its complete user-visible behavior.
Add regression tests for bug fixes. Update the project documentation and milestone
status with the behavior, limitations, and how the feature is verified.

Once [wizard mode](docs/wizard-mode.md) is available, use scripted wizard commands
where appropriate as the final integration test: launch the actual server and
frontend, construct a reproducible scenario through authorized wizard commands,
exercise the feature, and assert the resulting state and client-visible behavior.
For example, place a mob and equipment, teleport into position, then use ordinary
combat actions to verify combat. Test a wizard feature through its own privileged
commands. Include save/resume or rewind when relevant to the feature.

Wizard scenarios complement focused unit/protocol tests and normal-play coverage.
Setup shortcuts must not bypass the behavior under test, and wizard success does
not establish that the feature works or is properly restricted in a normal game.
Keep command scripts, seeds, and expected outcomes in version control and run the
applicable process tests in Windows and Linux CI. Until wizard mode is implemented,
use existing fixtures and real process tests; do not postpone feature testing.

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
