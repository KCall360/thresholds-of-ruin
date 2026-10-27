# Development practices

This page describes how changes to Thresholds of Ruin are designed, tested,
documented, and published. AI coding agents should also read [AGENTS.md](AGENTS.md).

Before making changes, read the [architecture](docs/architecture/overview.md), the
[project status and roadmap](docs/status.md), and the
[testing policy](docs/development/testing.md).

## Testing

Cover every changed behavior contract in the same change, following
[contract ownership](docs/development/testing.md#contract-ownership). Preserve
feature acceptance and unique boundary witnesses; existing coverage needs an
explicit source/assertion mapping. Every bug fix needs a regression that fails at
the intended behavior before the fix. A passing tier cannot compensate for
missing coverage. Use TDD for simulation, protocol and interaction behavior.
Keep performance instrumentation and workloads current.

Follow [setup](docs/development/setup.md), then the authoritative
[testing workflow](docs/development/testing.md): focused tests while iterating
(use [`scripts/feedback.py`](docs/development/testing.md#fast-local-rust-feedback)
for repeated single-target Rust edits; it does not qualify a verification gate),
then the [risk-based local checkpoint](docs/development/testing.md#broader-checkpoints)
before publication. Expedited iteration is the default: batch fixes, reuse valid
results and select affected contracts rather than repeatedly running broad local
`quick`/`push`/`full` tiers. Broaden locally for a specific justified risk or a
maintainer request, not merely because a push is due. Verified
[prose-only documentation changes](docs/development/testing.md#prose-only-documentation-changes)
retain their documentation/link and tooling checks. Run targeted release checks
for performance/release-specific behavior. Required hosted CI provides final
qualification; the canonical
[risk-based CI plan](docs/development/testing.md#risk-based-ci-assurance) chooses
prose-only, selected-contract, or exhaustive coverage. Both required platform
aggregates must pass for the exact final head. Selected results deliberately
leave unrelated exhaustive coverage to weekly scheduled/on-demand/release verification;
they must not be described as a full Windows/Linux debug/release pass. Unknown
scope or ownership and verification/build-control changes require full PR coverage.
Only an otherwise-full main push can use
[prior exact-tree full qualification plus fresh main integration](docs/development/testing.md#exact-tree-post-merge-assurance).
Ordinary selected/prose main runs are unchanged; missing proof retains full
execution. This distinct assurance must not be reported as freshly rerunning the
full matrix, or used to qualify another retained result.
A scheduled-full failure is actionable evidence to fix and feed back into ownership,
not permission to ignore the failing test or keep relying on a missed fast path.

A full run covers narrower local checks only for unchanged inputs, toolchain and
test configuration; it is not a prerequisite for focused work. Record the tested
state, failures and checks not run; stale, incomplete or failed evidence does not
satisfy a gate. Maintain the [test-effectiveness ledger](docs/development/test-effectiveness.md)
with evidenced defects and stale-test failures, not inflated pass/fail totals.
The only standing local failure waiver is the exact native travel mouse test
documented in the testing policy, reported as a waived failure and still required in CI. Never weaken tests or
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
and measurement limitations with accessible immutable evidence. The
[test-effectiveness ledger](docs/development/test-effectiveness.md) retains bounded,
classified failure evidence; routine pass listings and local-only logs are not
current reference material.

Every new page must be reachable from the docs index, including nested pages.
Check links, anchors and executable examples after moves. Documentation tests
check structure and selected contracts, not every semantic claim; review against
code and tests as well. Update relevant source comments and CLI help pointers.

## Publishing

- Work on a feature branch and open a pull request. The exact-head canonical CI
  plan and both required platform aggregates must pass before merging. Report
  the actual assurance mode. Selected coverage is not exhaustive; post-merge
  exact-tree assurance distinguishes the prior full run from newly executed
  main checks rather than relabeling old tests as fresh.
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
