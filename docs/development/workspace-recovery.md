# Recovering project knowledge before workspace cleanup

This guide distinguishes durable project state from machine-local evidence. It
records the October 2026 Rogue handoff and a repeatable recovery procedure, not a
license to delete files or a substitute for the current [testing policy](testing.md).
[Status](../status.md) owns current scope and formats; plans describe unfinished
work, not permission to start it.

## Published checkpoint

The completed darkness/perception batch is
[PR #186](https://github.com/KCall360/thresholds-of-ruin/pull/186), merged on
2026-10-22 at
[`36f2ae8808eb1443a71aaebeda9fa847e4f83d49`](https://github.com/KCall360/thresholds-of-ruin/commit/36f2ae8808eb1443a71aaebeda9fa847e4f83d49).
Its tree is `2f757df308af475903b76cc28f5697b1836fb1b8`. Both the
[final premerge run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/39731755709)
and [postmerge run, attempt 2](https://github.com/KCall360/thresholds-of-ruin/actions/runs/39770714421/attempts/2)
passed their complete required gates. The first postmerge attempt hit the bounded
Ubuntu package-download deadline before Rust verification; one failed-job retry
passed without changing source, dependencies, test selection or time limits.
That original setup failure was not relabeled a test pass.

This is a historical recovery anchor, not a claim that every later main revision
has passed those checks. GitHub logs and uploaded workflow artifacts have their
own retention limits; a run link is not a permanent backup of every raw trace.
Local native waivers and deferred local benchmarks recorded in the PR were not
passes; the final platform CI supplied the required qualification.

The preceding work is already in the checkpoint, not an unmerged implementation
backlog:

| Published work | Review boundary |
| --- | --- |
| Shared wearable mechanics and Rogue rings | [PR #178](https://github.com/KCall360/thresholds-of-ruin/pull/178) |
| Observer-owned discovery, secret passages and contact traps | [PR #179](https://github.com/KCall360/thresholds-of-ruin/pull/179) |
| Hazards, escape, levitation and private disguise discovery | [PR #180](https://github.com/KCall360/thresholds-of-ruin/pull/180) |
| Actual-caller idle-action completion regressions | [PR #181](https://github.com/KCall360/thresholds-of-ruin/pull/181) |
| Living-dungeon awareness, wandering populations and control items | [PR #182](https://github.com/KCall360/thresholds-of-ruin/pull/182) |
| Opt-in room omissions and coverage-preserving release CI partitioning | [PR #183](https://github.com/KCall360/thresholds-of-ruin/pull/183) |
| Coverage-preserving Windows debug CI partitioning | [PR #184](https://github.com/KCall360/thresholds-of-ruin/pull/184) |

The owners of the implemented contracts remain the
[generation](../reference/generation.md), [perception](../architecture/perception.md),
[items](../reference/items.md), [simulation](../architecture/simulation.md), and
[earned-route](rogue-earned-route.md) guides. Neither live omission nor live maze
tuning was enabled by this work. Topology/persistence witnesses do not establish
general survival balance or a complete Rogue run.

## What was and was not recovered

At the 2026-10-22 preservation checkpoint, the available cloud filesystem no
longer contained the earlier project worktrees or their ignored evidence. A fresh
clone recovered the published source; no cleanup operation was performed as part
of discovering that absence. Why those paths were absent was not established.

The last released local implementation's recorded tree matches the published
checkpoint exactly; no further tracked implementation was pending in that known
handoff. This narrow equality does not resolve other local inventories.

Remote Git preserves committed code, documentation and checked-in current and
historical fixtures. It does **not** by itself preserve old local receipts, failed
saves, captures, independently retained executables, ignored design notes,
untracked helpers, or changes in other worktrees. A recorded digest can verify
bytes if they are found; it cannot recreate them. Conversation-backed summaries
preserve conclusions and known limitations, not exact copies of missing files.
There is no verified complete inventory of unpublished work from that old VM.

Keep these boundaries explicit:

- The separate [3D art PR #185](https://github.com/KCall360/thresholds-of-ruin/pull/185)
  was still draft and unmerged when this handoff was checked. Its remote branch
  preserves that proposed work, not evidence that it belongs to main or is ready
  to merge. It has independent ownership and review.
- [AGENTS.md](../../AGENTS.md) still warns about an original dirty interactions
  checkout. This preservation pass did not inspect or recover it. A separate
  historical desktop ring prototype was also reported; the later published ring
  implementation does not prove every unpublished desktop change is disposable.
  Neither location is covered by a cloud cleanup inventory.
- The reconstructed [missing-consumables plan](../plans/rogue-missing-consumables.md)
  preserves agreed design direction and unresolved choices. It is not implemented
  content, a recovered byte-identical local file, or authorization to resume work.
- Development was paused after the completed darkness/perception batch. Resume
  implementation or live tuning only with a new go-ahead; documentation
  preservation does not restart that work.

## Repeatable preservation and restart procedure

1. **Identify the actual environment.** Check the repository root, worktree list,
   branch, HEAD, dirty files and active commands. Do not assume a new executor is
   the old VM merely because its path looks similar. Do not switch to or clean a
   separate desktop to compensate for missing cloud files.
2. **Inventory before deciding what is disposable.** Include staged, unstaged,
   untracked and ignored files, unpushed commits, auxiliary worktrees, external
   save/fixture inputs and active build/test owners. If the old environment cannot
   be inspected, record that blocker instead of claiming an empty inventory.
3. **Curate durable knowledge into its owning guide.** Preserve accepted design
   decisions, unresolved choices, independent regression ownership, meaningful
   negative findings and reproducible procedures. Link immutable source/review
   evidence where available. Do not publish credentials, private configuration,
   raw logs, routine receipts, binaries or caches in documentation.
4. **Preserve irreplaceable bytes separately when available and authorized.**
   Inspect the intended destination and verify the copied/archive bytes, hashes,
   ownership mappings and retrieval path before considering removal. Compression
   must not discard failed evidence or change a live capture. A summary is not a
   byte-preserving archive. See [verification evidence](testing.md#verification-evidence-and-reuse).
5. **Verify publication, not just a local commit.** Use a documentation-only PR
   when the full diff satisfies the [prose-only gate](testing.md#prose-only-documentation-changes).
   Read back the remote head/tree, pass the selected CI gate, merge with appropriate
   authorization, then verify actual main and postmerge CI. Keep unrelated PRs and
   unpublished implementations separate.
6. **Rebuild environments from declared inputs.** Follow [setup](setup.md), the
   repository toolchain, lockfiles and pinned Python requirements. A recovered
   executable or warm Cargo cache is not proof of current source identity or test
   coverage. Re-establish a single target owner and input/protection manifests
   before reuse; otherwise build and qualify a fresh target.
7. **Request cleanup only after preservation is verified.** Approval to publish
   documentation is not approval to delete the VM, caches, saves, evidence or
   unrelated work. Name the exact proposed cleanup set and its recoverability;
   stop if ownership, active use or unpublished contents remain uncertain.
