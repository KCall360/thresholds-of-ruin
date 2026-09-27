# Milestone 3p client timing investigation

Milestone 3p is deferred, still open, at the user's direction; it no longer blocks
milestone 3 feature work. The latency disposition below remains unresolved. This follow-up
keeps format 6, protocol 12, `diagonal-v11`, the 64 MiB checkpoint cap, exact
recovery/rewind, disclosure and bounded runtime queues unchanged. It adds no
interactions or travel features. The complete checkpoint remains 9.92 MB; format-5
rejection remains the approved pre-release policy.

## Starting publication and evidence

GitHub directly confirmed PR #29 merged at
`37a913dedc05b18b366c34e6ba6f728468381ec4` on September 25, 2026 at 17:49:33 UTC.
Windows and Linux CI succeeded on its final head
`a098f341553c256d7775f71754d470bece2765d6` at 17:48:33 and 17:49:00 UTC.
The [retained checkpoint study](3p-checkpoint-growth.md) remains evidence, including
its failed save-response run and the 2,915 ms native maximum. Non-reproduction is
not proof of resolution, and new measurements cannot retrospectively supply
missing boundaries for old samples.

## Attribution and experiment

The historical request-send timer begins before synchronous diagnostic stderr.
Its apparent delivery tail can therefore include a diagnostic write. The sent
diagnostic also precedes acknowledgement reading, and the acknowledgement
diagnostic precedes delivery to the presentation worker. Previously their costs
were not separately observable. Each client record now reports the preceding
diagnostic call's wall duration, joined offline to that preceding event.

The original reader logs and flushes each full frame before reading another.
The retained 256-region native run produced approximately 432 MB of frame logs.
An opt-in capped deferred-log mode isolates those reader disk writes while keeping
ordinary native rendering, JSON reports, pipe transport, validation and action
ordering. The [harness guide](performance-harness.md#opt-in-timing-correlation)
defines caps, failure handling and all old/new boundaries. Encoding and stdout
write durations are separate; an after-presentation timestamp separates actual
native-call completion from frame-report reader receipt.

Measurements use sequential release workloads on the same Windows machine and
F: NTFS HDD. Every native traversal starts with a real save, explores by ordinary
actions, retains disclosed knowledge, checkpoints at interval 64, then verifies
exact restart and ASCII/text continuation. Small and large traversals contain
77/2,805 actions and 620/20,956 disclosed cells. No empty large-world stand-in is
used. Ordinary host/desktop scheduling remains uncontrolled.

The intermediate instrumented synchronous run peaks at 502.52 ms: its measured
frame report takes 454.83 ms with a 2.84 ms server handler. Another sample has
97.65 ms between server acknowledgement sending and client acknowledgement
receipt; the preceding client sent-record diagnostic takes 97.33 ms. This directly
locates time in diagnostic calls, without establishing which part is kernel I/O
wait versus scheduling. The intermediate deferred large run peaks at 160.02 ms,
including a 114.40 ms report call. Its start overlaps offline analysis of the
preceding run; it is exploratory evidence, not final qualification.

Final matched native runs retain every action. Values are p50/p95/maximum ms;
reporting remains part of these original intervals. Native pacing is not
comparable to the server-only 8/33 ms provisional target.

| Regions / logging | Samples | Input to reported-frame receipt | Checkpoint bytes / tail | Restart to frame ms |
| --- | --- | --- | --- | --- |
| 8 / synchronous | 77 | 50.10 / 51.88 / 62.95 | 890,141 / 13 | 207 |
| 8 / deferred | 77 | 50.18 / 52.24 / 233.47 | 890,141 / 13 | 207 |
| 256 / synchronous | 2,805 | 49.37 / 50.94 / 683.50 | 9,806,500 / 53 | 1,778 |
| 256 / deferred | 2,805 | 49.62 / 51.06 / 87.76 | 9,806,500 / 53 | 2,214 |

The 683.50 ms sample includes 632.99 ms in the historical send interval, of
which 632.82 ms is the request-start diagnostic call. Another 490.72 ms sample
finishes native presentation 29.74 ms after acknowledgement but reaches the
reader 443.10 ms later: encoding takes 1.48 ms and stdout writing 441.59 ms.
This separates a real diagnostic write stall from encoding and native delivery.
Server handlers peak at 12.76/11.56 ms in the synchronous/deferred large runs;
acknowledgement-to-native-return peaks at 46.92/44.75 ms respectively.

The deferred small run is a necessary counterexample to claiming all tails are
diagnostic disk I/O: its 233.47 ms door sample spends 196.95 ms inside the native
presentation/pacing call, with 0.20 ms application and 1.79 ms drawing work.
Acknowledgement-to-native-return is 213.38 ms. Reporting follows this stall.
The deferred large maximum still includes 37.13 ms in stdout writing after
native presentation. Without a kernel/thread scheduling trace these native and
pipe-call walls cannot be divided into running CPU, blocked waits and descheduled
time. The results are measurements, not promised bounds or proof that historical
tails are fixed.

A local Windows Performance Recorder CPU-trace capability probe was attempted
after confirming no recording was active. WPR refused to enable the system
performance profiling policy (`0xc5585011`); no kernel scheduling trace was
started and no machine policy was changed. The retained `wpr-capability.log.gz`
records this limitation. Scheduling attribution therefore remains unqualified,
rather than being inferred from a low CPU counter or a quiet repeat.

The eight-client deferred-log attempt reaches its diagnostic stdout cap after
1,418 samples and two completed cycles. It is retained as a failure with partial
samples and available timing/frame records. The cap is not raised to turn that
prefix into a pass. The synchronous three-cycle attempt also fails, after 950
samples and one completed cycle, at a snapshot readiness deadline. Actor 6's
snapshot handler takes 0.256 ms, while its headless ready-report call takes
27,347.28 ms. The response-report boundary also includes an earlier unassigned
gap. These remain failed runs, not successful latency samples or proof that
ordinary application delivery takes 27 seconds. The separately named one-cycle
comparison does not replace either longer failure. The retention cap belongs
to the opt-in harness, not the server save queue, checkpoint cap or client channel.

The matched one-cycle eight-actor comparisons each complete 497 ordered samples,
including 495 accepted acknowledgements. Synchronous driver acknowledgement
p50/p95/max is 6.71/19.19/26.17 ms; deferred is 6.62/18.56/30.66 ms. Reader receipt
is 6.04/17.37/24.11 and 6.04/17.07/27.94 ms. These short successes do not qualify
the failed longer workload. The unchanged scheduler/action validator is retained.

Raw samples, projected frame records, client/server timings, failures, binary
hashes and limitations are retained in the
[manifest](measurements/3p-client-timing-2026-09-25/manifest.json) and
[summary](measurements/3p-client-timing-2026-09-25/summary.json). Final unmeasured
report/write costs remain null when no following record exists; they are not
filled with zero. Full logs and saves remain on the development machine.

On the final deferred run's genuinely explored save, real Windows keyboard input
opens a local modal in 47 ms while SQLite saving is deliberately blocked. After
release, the checkpoint commits at sequence 2,808 with 9,915,709 bytes. This is
a separate continuation after the full traversal/restart boundary, not a rewrite
of the traversal's checkpoint measurements.

## Verification

Local verification completed September 26: 223 Rust tests pass in each of debug
and release, all 95 Python debug checks and 65 release process checks pass,
and formatting, Clippy with warnings denied, dependency boundaries, documentation
links/indexing and private-item rustdoc with warnings denied pass. Debug/release
application binaries are rebuilt. Text, ASCII and Text + ASCII Spectator desktop
launchers each connect with fresh retained saves and owned-process cleanup; the
removed 256 Region Spectator launcher is not restored.

The retained projections reproduce every successful correlation exactly, and
the failed prefixes remain failures. The debug Rust test elapsed time includes
the overnight interruption; it is not performance evidence. All performance
measurements above completed September 25 before that interruption. Windows/Linux
CI on the PR's final head is required before merge; the PR records publication.

## Closure assessment

The experiment establishes diagnostic calls as a source of some recurrent tails,
but synchronous-call wall duration alone cannot distinguish blocked I/O from
descheduling inside the call. A deferred-log comparison removes reader disk
writes, not pipe waits, JSON work, parsing, retention or OS scheduling. No residual
interval is silently attributed to scheduling, subtracted from the original
metric, or used to relax the latency targets.

The checkpoint-growth gate remains addressed. When deferred 3p work resumes,
the next focused investigation should capture thread scheduling and
blocked-write evidence around the native-call and headless-report stalls, with
the new post-presentation boundary and capped log mode retained. Fix or explicitly
resolve those costs, then qualify the longer client workload without replacing
failed prefixes or treating quiet repeats as proof. Broader simulation/persistence
matrix expansion is not warranted by these isolated diagnostic/native-call changes.
The next active feature work is shared resumable actions, durable
place knowledge, semantic narration/interruption and slow-client resynchronization
acceptance for milestone 3 interactions and travel.
