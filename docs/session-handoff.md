# Session handoff — milestone 3 interactions and travel

## Current direction

At the user's direction, milestone 3p is deferred, still open, and no longer
blocks feature work. The initial shared action extension points now separate
validation/timing, effect application and scheduling without changing gameplay,
protocol, saves or rules. Concrete partial progress remains future work; see the
[simulation guide](simulation-slice.md#shared-action-extension-points) and
[architecture](architecture.md#time-and-actions). Durable place knowledge is implemented and verified locally;
see [the feature guide](place-knowledge.md) and
[verification findings](place-knowledge-findings.md). Continue milestone 3 with semantic
narration/interruption and slow-client resynchronization acceptance. Extend wizard placement as needed to
reproduce interactions and failures. Follow the [roadmap](milestones.md) and
retain the ongoing feature performance checks in [development practices](../CONTRIBUTING.md).
The timing findings below remain unresolved evidence, not a completion claim.

## Starting checkout and retained publication context

The action extension-point refactor starts from local commit `ea80533`, the merge
of PR #30. The publication details below describe the preceding 3p investigation.

PR #29 merged at `37a913dedc05b18b366c34e6ba6f728468381ec4` at
2026-09-25 17:49:33 UTC. Windows/Linux CI passed on final head
`a098f341553c256d7775f71754d470bece2765d6` at 17:48:33/17:49:00 UTC.
GitHub was queried directly during that investigation. The timing follow-up used
`codex/3p-client-timing`; consult PR #30 for its final publication and CI state.

Read [the client timing findings](3p-client-timing.md),
[checkpoint reduction](3p-checkpoint-growth.md), [original audit](3p-closeout.md),
[milestones](milestones.md), [checkpoints](checkpoints.md),
[harness](performance-harness.md), [performance plan](performance-persistence.md),
[architecture](architecture.md) and [development practices](../CONTRIBUTING.md).

## Preserved contracts

Current durable places use format 7, protocol 13 and `places-v12`, rejecting older
versions under the pre-release policy. The following checkpoint measurements
describe the preceding format-6 baseline. The complete 256-region checkpoint
remains 9.92 MB under the unchanged 64 MiB cap. Exact recovery, rewind, disclosure,
bounded queues and resynchronization behavior are unchanged. Maintain the Text,
ASCII and Text + ASCII Spectator desktop launchers; do not restore the removed
256 Region Spectator launcher. Preserve the driver and existing saves.

## Findings and closure

Milestone **3p remains open**. The historical 2,915 ms tail is retained, not declared
fixed by non-reproduction. New timing fields locate a 632.82 ms request-start
diagnostic call inside a 632.99 ms apparent send interval, and a 441.59 ms stdout
write after native presentation already returned. Encoding is separately measured.
Capped deferred logs remove reader disk writes during actions; they do not remove
pipe backpressure, CPU work or scheduling. The original metric boundaries remain.

Both final 256-region traversals complete 2,805 actions and 20,956 cells with exact
restart, checkpoint sequence 2,752, 9,806,500 bytes and 53 replay-tail records.
Synchronous/deferred maxima are 683.50/87.76 ms. The small deferred run retains
a 233.47 ms tail including 196.95 ms inside native presentation/pacing, before
reporting. Real keyboard modal input on the fully explored save takes 47 ms
while SQLite is blocked and the subsequent checkpoint commits.

The longer deferred eight-client attempt fails at its diagnostic retention cap
after 1,418 samples/two cycles. The synchronous attempt fails after 950 samples/
one cycle at a snapshot readiness deadline; one headless ready report takes
27,347 ms despite a 0.256 ms snapshot handler. Both failures are retained.
Separate one-cycle comparisons complete 495 acknowledgements each; their success
does not qualify the failed longer runs. See the findings for raw evidence,
hashes, verification, measurement boundaries and limitations.

## Deferred 3p follow-up

When 3p resumes, capture thread scheduling and blocked-write evidence around the remaining native
and headless-report stalls, then fix or explicitly resolve those costs and qualify
the longer client workload. WPR's CPU-trace probe failed to enable the local
system performance profiling policy (`0xc5585011`); no policy was changed or
kernel trace started. Obtain that profiling capability before assigning wall
intervals to descheduling or pure I/O.
Keep failed runs, nullable final diagnostic costs and historical measurements.

The action refactor and durable-place feature form one publication checkpoint
alongside the retained sequencing changes. Offscreen place travel and concrete resumable action
progress remain deferred. Accumulate
planning edits locally; publish at a meaningful checkpoint through a PR and merge
only after Windows/Linux CI pass on its final head.
