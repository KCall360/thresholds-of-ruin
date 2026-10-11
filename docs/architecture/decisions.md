# Decisions and evidence worth preserving

This is a small record of rationale that prevents repeated mistakes. Current
contracts live in [architecture](overview.md), [persistence](persistence.md),
[protocol](../reference/protocol.md) and [performance guidance](../development/performance.md).
It is not an implementation diary or current CI certificate.

## Optimize measured work without moving costs out of view

An acknowledgement, an executed action and a presented frame are different
boundaries. Queue admission can become faster without improving player-visible
completion. Report all relevant boundaries; never compare old completion timing
with new admission timing. Likewise, moving AI decision work into a preparation
phase requires comparing the combined decision/command cost, not just the smaller
remaining phase.

Measure complete wire envelopes rather than an obsolete partial DTO. Smaller
encoded deltas can increase client validation/reconstruction work; count and byte
reductions do not establish lower RSS, CPU time or end-to-end latency. Shared
content and immutable checkpoint pooling need both ownership tests and measured
allocation/restore effects.

Historical evidence (fixed snapshots, not current performance claims):

- [Admission versus completion](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L483-L540)
- [Moved AI decision costs](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L1820-L1872)
- [Complete wire-envelope accounting](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L2331-L2378)
- [Compact payload/client CPU tradeoff](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L2439-L2502)

## Do not reinstate rejected optimizations without new evidence

A navigation-membership experiment retained equivalent results and operation/byte
counts but did not demonstrate a useful release improvement. Its runtime changes
were removed; its behavioral-equivalence test remained. That is a useful outcome,
not an unfinished optimization obligation. See the
[comparison and rejection](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L304-L337).

An earlier single-parse save-reader experiment regressed restore behavior and was
removed. A later strict streaming-comparison decoder reduced peak restore memory
while retaining validation; timing remained mixed, with an outlier preventing a
broad speedup claim. Keep these experiments distinct. Their useful lesson is to
avoid overlapping typed state and unnecessary canonical trees, while measuring
validation, memory and restore timing together:

- [Rejected reader](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L1322-L1339)
- [Retained comparison](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L1875-L1913)
- [Later decoder memory results and limitations](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L168-L284)

Some tables are the only published summary; raw samples were not all published.
Immutable links preserve that evidence without claiming independent
reproducibility. Use the performance ledger's machine/workload identity before
comparing numbers. Existing save/physics/client tails remain open where listed in
[current status](../status.md).

## Keep representation boundaries independent

Saved schemas must not inherit changes to wire serialization. Backend action
facts, explicit save DTOs, authoring declarations, compiled definitions and public
observations serve different trust/version boundaries. Map them exhaustively;
do not reuse a convenient public type as authoritative storage. Shared definitions
are pinned once rather than duplicated into each actor record.

Typed rule operations make invalid stat/operator pairs unrepresentable. Resolve
content strings to stable typed IDs once; keep one modifier fold and a shared
plan/resolve/apply kernel. Prepared effect operands freeze at preparation while
live target/authority conditions are revalidated. These are current contracts in
[simulation](simulation.md) and [creature rules](../reference/creature-rules.md),
not invitations to recreate completed refactor steps.

[Historical schema rationale](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L1989-L2011)
explains why seemingly harmless wire changes can otherwise break strict saves.

## Ownership and bounds are correctness, not timing tricks

Release an explicit save-lock owner when its operation ends; do not add sleeps or
retry loops to mask a lingering handle. Bound the reconstructed retained state as
well as each individual delta/frame. A multipart transfer needs one total deadline,
not a fresh allowance per part. Preserve allocation high-water marks across rewind
so abandoned actor/receipt identities cannot be reassigned to unrelated state.

Evidence:
[lock lifetime](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L2060-L2084),
[retained-state bounds](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L2414-L2436),
[transfer deadlines](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L2902-L2947),
[rewind identity watermarks](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md#L2802-L2841).

## Preserve evidence, not chronology

The full former [refactor chronicle](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/history/refactoring-increments.md)
and earlier [September archive](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs)
remain readable. Their pending statuses, test counts, formats, deployment notes and
permission narratives describe their historical checkpoints. They do not override
current contracts, authorize new work or prove a current verification run.
