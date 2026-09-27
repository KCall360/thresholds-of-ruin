# Session handoff — scenario packages and item knowledge

Updated 2026-09-27. Milestones 4a and 4b are complete locally and are being
published together from `codex/scenarios-items-knowledge`, as requested. The
[roadmap](milestones.md) is the status source of truth. Do not merge until both
Windows and Linux CI pass; publication does not authorize merging.

## Implemented decisions

See [scenario packages](scenario-packages.md) for authored TOML setup, certificates,
stable IDs and pinned saves, and [items](items.md) for quantity-aware transfers,
stack identity, appearance assignment, character knowledge and disclosure.
NetHack informed defaults; implementation and content remain original.
Protocol 14, save/SQLite version 9, ruleset `items-v14`, validator `tor-scenario-2`
replace older prerelease formats without migration readers. There are 22 ordinary
validated packages, including the item acceptance fixture. Equipment, item use,
capacity and physics remain future work.

Context-preserving practices are in [CONTRIBUTING](../CONTRIBUTING.md): retain full
logs locally, inspect relevant failures, report compact summaries and use targeted
reads. Profiling maintenance remains mandatory as features change, despite 3p
being deferred.

## Verification and evidence

Windows local verification passed: 257 Rust tests in each of debug/release,
108 debug Python checks, 73 release process checks, all-target Clippy, formatting,
architecture and documentation checks, rustdoc with warnings denied, and all 22
package validations. Release binaries were rebuilt. Text, ASCII, and Text + ASCII
Spectator desktop launchers connected, retained fresh saves and cleaned up owned
processes. The removed 256-region desktop shortcut remains removed.

Local logs under `.local/`: `4b-rust-debug-final2.log`, `4b-rust-release.log`,
`4b-python-debug.log`, `4b-python-release.log`, `4b-clippy-final.log`,
`4b-rustdoc.log`, `4b-validation-lf.log`, and `4b-launchers.log`.
The debug Python run preceded the final non-stackable merge-scan optimization;
full Rust checks and release process checks passed afterward. All development
failure logs are retained locally, outside Git.

[Item profiling results](items.md#recorded-results-2026-09-27-windows) link the
raw measurements, summary and source/binary hashes. Scenario workload v1 remains
unchanged; new items workload v1 measures transfers, knowledge, client application
and canvas drawing, persistence, and operation/byte counts. Matching permissions
and two repeats were used to investigate persistence variation. Large-scenario
restart median is about 1.1 ms higher than the local 4a baseline; its cause is not
isolated. Persistence tails vary. These are recorded limitations, not claims of
3p closure or durable-action latency acceptance. Original 4a evidence remains
available through the scenario guide, clearly labelled historical.

## Next work

Milestone 4c is next: multi-cell bodies, rotated portals and gravity. Before
implementation settle deterministic integration, mass/normalization, transformed
occupancy/velocity, support and collision ordering, and impact versus acceleration
damage. Refer to the roadmap and [design plan](game-design-plan.md), rather than
assuming NetHack specifies this project's physics. Keep scenario fixtures,
profiling, recovery, disclosure and real-client tests current as those mechanics
are added. Deferred 3p findings and targets remain open.
