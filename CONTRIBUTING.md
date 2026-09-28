# Development practices

This page describes how changes to Thresholds of Ruin are designed, tested,
documented, and published. AI coding agents should also read [AGENTS.md](AGENTS.md).

Before making changes, read the [architecture](docs/architecture.md), the
[project status and roadmap](docs/milestones.md), and the
[testing policy](docs/testing.md).

## Testing

Every change must be tested, and the full policy is in
[testing and verification](docs/testing.md). In short:

- Use test-driven development for simulation rules, protocol behavior, and
  client interactions: write a failing behavior test, implement, then refactor.
- Every feature needs behavior tests at each layer it changes **and** an
  acceptance test that launches the real server and clients.
- Every bug fix needs a regression test that fails before the fix.
- Everything must pass on **Windows and Linux**, in **debug and release**.
- Keep profiling instrumentation, workloads, and validators working, and run
  targeted release-build performance comparisons for latency-sensitive changes.
- Keep `main` green. Don't merge ignored or known-failing tests.

Run the checks listed in the [testing policy](docs/testing.md#running-the-checks)
before pushing, and report any check you couldn't run.

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
[roadmap](docs/milestones.md#current-implementation).

- When a format changes, update callers, fixtures, scenario certificates, and
  documentation together.
- Saves are disposable across revisions. Don't add importers, compatibility
  readers, historical rules implementations, or defaults just to load old saves.
- Keep version rejection and strict replay checks.
- Supporting multiple installed ruleset or generator versions is planned as a
  later, explicit compatibility milestone. It isn't an exception to this policy.

## Documentation

Keep documentation roles distinct:

- [`docs/milestones.md`](docs/milestones.md) is the single source of truth for
  status, scope, and the current format versions.
- [`docs/architecture.md`](docs/architecture.md) records durable boundaries and
  the reasons for them. It isn't a changelog.
- Feature guides describe **current** behavior, controls, limitations, and how
  the feature is verified.
- The [game design plan](docs/game-design-plan.md) records accepted future
  requirements.

When behavior changes, update the relevant guide and the roadmap in the same
change. Replace superseded statements rather than leaving history beside current
behavior. Guides say "the current protocol" rather than repeating version
numbers. Git history and pull requests record who changed what and when, so
guides don't list PR numbers, commit hashes, or CI runs. Link every new guide
from [`docs/README.md`](docs/README.md); `scripts/test_documentation.py` rejects
broken local links, unindexed guides, and version numbers that don't match the
code. The [documentation index](docs/README.md#maintaining-the-docs) has the
checklist.

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
