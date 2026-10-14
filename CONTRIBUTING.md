# Development practices

This page describes how changes to Thresholds of Ruin are designed, tested,
documented, and published. AI coding agents should also read [AGENTS.md](AGENTS.md).

Before making changes, read the [architecture](docs/architecture/overview.md), the
[project status and roadmap](docs/status.md), and the
[testing policy](docs/development/testing.md).

## Testing

Every feature needs behavior tests at each changed layer and an acceptance test
that launches the real applications. Every bug fix needs a regression test that
fails before the fix. Add these tests in the same change; a passing tier is not a
substitute for missing coverage. Use TDD for simulation, protocol and interaction
behavior. Keep performance instrumentation and workloads current.

Follow [setup](docs/development/setup.md), then the authoritative
[testing workflow](docs/development/testing.md): focused tests while iterating,
`python scripts/verify.py quick` at stable checkpoints, and a passing default
`push` tier (or `full`) before every push. `push` tests what the change can
reach; CI runs everything. Run targeted release checks for
performance/release-specific behavior; local full is required when CI cannot run
or on request. Full Windows/Linux debug/release CI on the final commit remains
required before merge, including separate tooling/dependency jobs.

A full run covers the local push gate for unchanged inputs, toolchain and test
configuration. Record the tested state, failures and checks not run; stale,
incomplete or failed evidence does not satisfy a gate. The only standing local
exception is the exact native travel mouse test documented in the testing policy,
reported as a waived failure and still required in CI. Never weaken tests or
infer new waivers from historical records.

## Architecture rules

These are enforced in review and, where possible, by tests.

- **Deterministic simulation.** `tor-world` and `tor-simulation` must not read
  wall-clock time, access the filesystem or network, or depend on UI code.
  Randomness must be explicitly seeded and persisted. Don't use unordered
  iteration to resolve simulation outcomes.
- **Backend owns the truth.** The backend owns rules, visibility, appearance
  facts, and actor knowledge. Clients receive only disclosed observations. Don't
  serialize internal world state into protocol messages or leak hidden facts
  through interaction metadata or errors.
- **No global player.** Every actor has explicit identity and control
  ownership. Doors are independent entities; they don't need to sit on portal
  apertures.
- **Dependency boundaries.** `scripts/check_architecture.py` enforces the
  permitted internal crate edges for all targets, including optional, build, and
  development dependencies. New crates and intentional boundary changes need an
  explicit policy update in that script. The check isn't a sandbox: new external
  libraries still need review (for example, to avoid I/O in simulation code).
- **Original work.** Write original code and content. NetHack is a gameplay
  reference, not a source to copy.

## Formats and compatibility

The project is pre-release, so it supports only the **current** protocol, save
format, and ruleset. They're listed in the
[roadmap](docs/status.md#implemented-scope).

- When a format changes, update callers, fixtures, scenario certificates, and
  documentation together.
- Saves are disposable across revisions. Don't add importers, compatibility
  readers, historical rules implementations, or defaults just to load old saves.
- Keep version rejection and strict replay checks.
- Supporting multiple installed ruleset or generator versions is planned as a
  later, explicit compatibility milestone. It isn't an exception to this policy.

## Documentation

Use the [documentation index](docs/README.md) and its maintenance rules.
Current status/formats belong in [status](docs/status.md), durable boundaries and
rationale in [architecture](docs/architecture/overview.md), current behavior in
references/player guides, procedures in development guides, and unfinished
requirements in plans. External research retains its pinned edition and caveats.

Update the owning guide and status with a behavior change. Replace superseded
statements; do not preserve rollout history beside current instructions. Keep one
authoritative control/schema/policy table and link to it. Guides say “current”
instead of repeating format numbers. Preserve useful rationale, negative results
and measurement limitations with accessible immutable evidence; routine PR/test
counts and local-only logs are not current reference material.

Every new page must be reachable from the docs index, including nested pages.
Check links, anchors and executable examples after moves. Documentation tests
check structure and selected contracts, not every semantic claim; review against
code and tests as well. Update relevant source comments and CLI help pointers.

## Publishing

- Work on a feature branch and open a pull request. Windows and Linux CI must
  pass before merging.
- Batch small plan, strategy, and documentation updates, and publish them at
  meaningful checkpoints (a completed feature or milestone, or a consolidated
  strategy revision), or when the maintainer asks. Don't open a separate PR for
  every planning clarification.
- Publishing later never postpones verification. Features are tested as they're
  built.

## Secrets and local files

Never commit credentials, local saves, logs, or private configuration. Server
tokens belong in environment variables, not in URLs, command-line arguments, or
files in the repository. Machine-local launchers and their credentials stay
outside Git; reusable scenario specifications and drivers belong in version
control.
