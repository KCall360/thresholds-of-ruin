# Session handoff — milestone 3p explored checkpoint growth

## Verified starting publication

PR #28 merged at `eff107ce95b7c0f6c8f813597d0fcb94f70fefa5` after Windows and Linux
CI passed on final head `61372ee1958346a90fe423eb75b6e16f182951cc`. GitHub was
queried directly. This follow-up uses branch `codex/explored-checkpoint-growth`;
consult its live PR for final publication and CI state.

Read [the original audit](3p-closeout.md), [this follow-up](3p-checkpoint-growth.md),
[milestones](milestones.md), [checkpoints](checkpoints.md),
[the harness](performance-harness.md), [the performance plan](performance-persistence.md),
[architecture](architecture.md) and [development practices](../CONTRIBUTING.md).

## Implementation and compatibility

The user explicitly approved advancing to save format **6** and rejecting format 5
before implementation, following the pre-release policy. No migration/compatibility
reader was added. Protocol 12 and `diagonal-v11` remain unchanged. Format 6 stores
equal cell/edge knowledge by source region once across navigation/rewind instances.
Decoding restores sharing and validates ordered references and table structure.
Exact boundary/game comparison, malformed-reference tests and full eight/256-region
saved-exploration regressions cover the changed representation.

The complete 256-region checkpoint falls from 765,021,723 to 9,920,494 bytes.
Both interval-64 and default-1024 traversal complete all 2,805 actions and 20,956
cells with exact restart and tails of 53/757 records. The 64 MiB cap, bounded
queues, resynchronization, disclosure, history and rewind contracts are unchanged.
The 256 Region Spectator desktop shortcut was removed as requested. Maintain the
remaining three launchers; preserve the reusable benchmark driver and prior saves.

## Closure and next step

The checkpoint-size blocker is addressed. Full native explored-save acceptance
passes, including a 78 ms real-keyboard modal response on the retained 256-region
save while persistence is blocked. Final acknowledgement maximum is 30.64 ms;
full native traversal still has a 2,915 ms maximum. Correlation places the largest
intervals in client request sending, acknowledgement delivery and frame-reader
receipt; server handlers peak at 10.53 ms. Their root causes and latency disposition
remain unresolved. Opt-in request/reader timing correlation is maintained with
the workloads. Non-reproduction does not prove historical spikes fixed. See the
follow-up for retained raw samples, timings, final verification and remaining
limitations; keep 3p open while that evidence item remains unresolved.

Next isolate those send/delivery/report-reader intervals with focused diagnostics.
After 3p gates are satisfied, milestone 3 resumes with shared resumable actions,
durable place knowledge, semantic narration/interruption, and slow-client
resynchronization acceptance. No interaction or travel feature is added here.
Publish through a PR and merge only after Windows/Linux CI pass on its final head.
