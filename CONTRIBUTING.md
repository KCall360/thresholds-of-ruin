# Development practices

Accumulate small project-plan, strategy, and documentation updates locally.
Publish them to GitHub at meaningful checkpoints, such as a completed feature or
milestone or a consolidated strategy revision, or when the user explicitly asks.
Do not create or push a separate PR for every planning clarification. A request
to update the plan alone does not require immediate publication.

When publishing, use a feature branch and PR; require Windows and Linux CI to
pass before merging. Keep this publication cadence separate from the testing
requirements below: delaying a push does not postpone feature verification.

Use test-driven development for simulation rules, protocol behavior, and client
interactions: write a failing behavior test, implement it, then refactor with
the tests passing. Prefer assertions about outcomes to copies of implementation
details. Keep main green; incomplete acceptance scenarios belong in the milestone
document until implementation starts, not in permanently ignored tests.

Every new feature must include automated behavior tests at the layers it changes
and an integration acceptance scenario for its complete user-visible behavior.
Add regression tests for bug fixes. Update the project documentation and milestone
status with the behavior, limitations, and how the feature is verified.

Treat performance as an ongoing feature requirement, not a one-time milestone.
Maintain the profiling instrumentation, versioned workload fixtures, real-client
drivers, and report validators as production code changes. When adding a feature,
extend the representative workloads to exercise its latency-sensitive paths and
relevant scale dimensions; keep existing workload versions and recorded baselines
meaningful instead of silently changing their meaning.

Run targeted release-build performance checks for changes to simulation,
perception, persistence, protocol delivery, or client application/rendering.
Choose cases that exercise the changed behavior plus a representative existing
interaction, and compare matching before/after cases on the same machine and
configuration. Include small/large cases for affected scale dimensions such as
history, regions, actors, items, or remembered cells. Record the selected cases,
sample counts, p50/p95/maximum latency, relevant operation/byte counts, and any
limitations. Investigate material regressions and tail spikes before considering
the feature complete; do not silently relax targets to accommodate new features.
Preserve determinism, disclosure, recovery, and input responsiveness while
optimizing. See the [performance plan](docs/performance-persistence.md) for the
current provisional latency targets and measurement boundaries.

The full performance matrix is not required for every change. Expand the focused
checks when results are inconsistent, a regression is unexplained, or the change
affects several subsystems. Documentation-only changes do not require latency
benchmarks. Targeted profiling does not replace required correctness tests or the
pre-publication verification below. Keep stable scale/operation-count regressions
in automated tests; machine-dependent timing measurements remain diagnostic.

Keep documentation roles distinct. `docs/milestones.md` is the status and roadmap
source of truth, `docs/architecture.md` records durable boundaries and rationale,
and feature guides specify implemented behavior. Link every guide from
`docs/README.md`; `scripts/test_documentation.py` rejects broken local links and
unindexed guides. Delete superseded claims instead of preserving an unlabelled
historical plan beside current behavior.

Use scripted [wizard mode](docs/wizard-mode.md) commands where appropriate as the
final integration test: launch the actual server and frontend, construct a
reproducible scenario through authorized wizard commands, exercise the feature,
and assert the resulting state and client-visible behavior.
For example, place a mob and equipment, teleport into position, then use ordinary
combat actions to verify combat. Test a wizard feature through its own privileged
commands. Include save/resume or rewind when relevant to the feature.

Once scenario packages are implemented, use ordinary validated packages for
authored initial setup in unit, integration, and real-client process tests.
Keep assertions/action sequences in the test harness, with stable entity/anchor
references checked against the fixture. Wizard commands remain appropriate for
testing privileged behavior and deliberate runtime mutations, not as a substitute
for the scenario format. Test checkpoints are ordinary reproducibly created saves.

Wizard scenarios complement focused unit/protocol tests and normal-play coverage.
Setup shortcuts must not bypass the behavior under test, and wizard success does
not establish that the feature works or is properly restricted in a normal game.
Keep command scripts, seeds, and expected outcomes in version control and run the
applicable process tests in Windows and Linux CI.

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

When protocol, save, or rules formats change, update callers and fixtures together;
the project supports only the current versions. During pre-release, saves are
disposable across revisions: do not add importers, compatibility readers,
historical rules implementations, or defaults solely to load older saves.
The future scenario design records exact dependencies and anticipates multiple
installed ruleset/generator versions; this is a later explicit compatibility
milestone, not an exception to the current pre-release policy.
Keep version rejection and strict current-rules replay checks; update fixtures
with the implementation instead of preserving obsolete behavior.
Preserve original code and content;
NetHack is a gameplay reference, not a source to copy.

Frontend milestones must include tests launching the actual applications in
addition to parser, input-model, presentation-model, and protocol tests. The
graphical tests need an explicitly configured display environment in CI.

Never commit credentials, local saves, or private configuration.

On the Windows development machine, keep the user's desktop launchers current
whenever the build is updated: Text, ASCII, and Text + ASCII Spectator.
The 256 Region Spectator desktop launcher was removed at the user's request;
retain its reusable benchmark driver, not a desktop shortcut. Verify each launcher's helper scripts and
actual executable targets, build every required binary, and check a real client
connection. `cargo check` alone does not update executables. Preserve fresh saves
per launch, prior saves, separate spectator credentials, and owned-process
cleanup. Machine-local links, credentials, and saves stay outside Git; reusable
scenario specifications/drivers belong in version control. See the
[harness guide](docs/performance-harness.md#observable-256-region-run-and-desktop-maintenance)
for the shared demonstration and verification procedure.
