# Session handoff — 2026-09-24

Phase A is complete. Phase B has not started and is not authorized by this
handoff. The user authorized publishing the completed work and pre-release
cleanup, then merging PR #22 after Windows and Linux CI pass.
The [roadmap](milestones.md) remains the source of truth for project status.

## Original Phase A publication checkpoint

- Workspace: `F:\Codex\Roguelike`.
- Branch: `codex/performance-harness-phase-a`.
- Published commit: `5ad0c1e8271ec188060c5b8176f362822062dd75`.
- [PR #22](https://github.com/KCall360/thresholds-of-ruin/pull/22) was opened as a
  draft for this checkpoint. Its live status supersedes this historical snapshot.
- [Windows and Linux CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/35963342072)
  both passed on that published commit; this was rechecked during wrap-up.
- This guide and its index link were added after the published checkpoint above.
- Pre-existing untracked `target-phase-a/` and `target-playtest/` directories
  remain untouched. Do not commit them or treat them as newly generated work.

## Completed evidence

Read the [Phase A findings](phase-a-findings.md) for results, the
[harness guide](performance-harness.md) for reproduction, and the
[persistence review](persistence-review.md) for the proposed storage contract.
The complete baseline and raw samples are retained under
`docs/measurements/phase-a-2026-09-24/`.

The 64-case release matrix covers 1/8/64/256 regions, 1/8 actors,
0/100/1,000/10,000 retained actions, and memory/durable execution. Timed actions
include ordinary and rotated boundary crossings in both directions, stairs,
cardinal/diagonal movement, doors, blocked attempts, and visibility changes.
Separate discovery walks traverse the 8/64/256-region worlds. Actual clients
also completed a mixed workload in a 256-region world; that run is not a claim
that the real clients explored all 256 regions.

Whole-save persistence is the principal measured bottleneck. The largest case
wrote 12.42 GiB for a final 5.69 MiB archive. Growing client map memory also raises
update costs. The actual-client request-to-ack p95 was 236 ms across mixed
actions, including diagnostic overhead; it is not an isolated crossing metric.

TDD and local debug/release, lint, architecture, documentation, actual-client,
and desktop-launcher verification are complete. The native debug mouse test
required an interactive permission rerun; see the findings for the precise
verification boundaries. All four Windows launchers were verified. Local
credentials, saves, helper scripts, and diagnostic logs remain ignored.

Production still uses protocol 11, save format 3, and ruleset `diagonal-v11`.
The readiness-revision fix can cause older multi-actor archives to fail strict
replay; failures preserve the original file. Current process recovery tests do
not establish power-loss durability or durable installation of replacement names.

## Pre-Phase B cleanup

The user authorized a focused Phase B measurement plan and pre-release cleanup,
followed by publication and CI-gated merge, not Phase B implementation. Saves need not remain compatible across revisions;
retain only current rules and reject unsupported versions. The local cleanup
removes historical ray visibility implementations, moves geometry coverage and
the visibility benchmark to shadowcasting, removes the missing-region save
fallback, and replaces the retired-ruleset catalogue with general rejection
coverage. Feature guides now describe current behavior. Existing Phase A
measurements remain evidence for their recorded commit. Local build directories
`target-phase-a/` and `target-playtest/` are now ignored and remain untouched.

Cleanup verification: debug world, simulation, and server tests passed, as did
workspace Clippy, formatting, architecture and documentation checks. Actual-client
shadowcasting/resume and wizard scenarios passed. All workspace binaries were
rebuilt and all four desktop launchers verified, including the 256-region
spectator demonstration and owned-process cleanup. The launcher verifier needed
process-query permission and a refreshed process inventory to avoid a text-client
startup race. Publication checks additionally passed all 192 Rust tests in each of debug and
release, plus the required lint, formatting, architecture, and rustdoc checks.
The full Python debug suite passed 62 of 63 tests and the release frontend suite
passed 51 of 52. The native mouse test was blocked locally: the sandbox denied
`SetCursorPos`; elevated runs found another window over the intended click target.
The assertion remains intact. Final Windows/Linux CI must validate it before merge.
No new release performance matrix was run; the published Phase A measurements
above refer to their original commit.

## Next session

1. Read this handoff, `CONTRIBUTING.md`, the roadmap, findings, and storage review.
   Check the current branch, worktree, and PR before changing anything.
2. Check the live status of Phase A PR #22. The user authorized merging after
   Windows/Linux CI pass on its final head; do not repeat an already completed merge.
3. When the user explicitly authorizes Phase B, follow TDD for the append journal.
   Resolve and verify the Windows/Linux bootstrap durability contract first.
   Cover framing and strict decoding, interrupted writes, corruption, uncertain
   I/O, lost acknowledgements, duplicate/conflicting retries, retained branches,
   annotations, and permanent wizard marking with failing behavior tests.
4. Implement the reviewed format-4 replay base, append journal, wizard marker,
   and recovery/publication ordering. Reject old formats; no importer is planned.
   Preserve evidence and fail closed when a corrupt tail cannot be proved safe.
5. Reuse the Phase A workloads with the focused Phase B measurement subset in
   [the plan](performance-persistence.md#phase-b-verification-and-measurement).
   Keep comprehensive storage correctness and applicable real-client tests;
   expand measurements only when results or changed paths justify it. Update
   the findings and roadmap, and verify Windows/Linux CI before any merge.
   **Stop before Phase C** unless the user broadens the scope.

Periodic checkpoints and rotation belong to Phase C. State-copy/observation
scaling belongs to Phase D; client responsiveness work belongs to Phase E.
Do not rerun the entire baseline merely to resume the session: the published
measurements are complete, and a new run should evaluate an actual change.
