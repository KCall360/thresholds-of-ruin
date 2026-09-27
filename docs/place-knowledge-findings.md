# Durable place knowledge verification — 2026-09-27 UTC

This feature includes the preceding locally verified action refactor. Protocol
13, format 7 and `places-v12` add first-sight anchor memory, mnemonic names and
free renaming, with text and ASCII lists. Offscreen destination selection remains
deferred. Milestone 3p remains deferred and open.

## Correctness

Full workspace Rust tests passed in debug and release. Full Python discovery ran
97 checks; three filesystem checks and one native mouse check required reruns
outside the Windows sandbox, where they passed. The release process suite ran
66 checks, with the same native mouse check passing on its desktop-enabled rerun.
The new actual-client scenario passes discovery, both rename interfaces, spectator
denial, stale knowledge, save/reconnect and rewind. Targeted final simulation,
protocol and client tests, Clippy, formatting, architecture and warning-free
rustdoc checks pass. These findings record local verification; publication also
requires Windows and Linux CI to pass on the final PR head.

Retained failures include the initial client event-validation regression (fixed
with a failing-then-passing shared-client test), the navigation reference test's
missing knowledge refresh, a process harness prompt-ordering error, and sandbox
filesystem/cursor denials. Successful listings and failure logs remain locally
under `target/local-logs/place-knowledge`, outside Git.

The three desktop shortcuts and their helper targets were inspected. Both debug
and release binaries were rebuilt; real text/native connections and spectator
access were exercised. Existing saves and fresh-save launcher behavior remain.

## Focused release measurements

Windows x86-64, the same checkout/machine/toolchain, one actor and 100 retained
starting actions. Existing `performance-v1` is unchanged. Each mixed movement
case contains 610 successful samples across ten cycles. Times are milliseconds.

| Case | Before p50 / p95 / max | After p50 / p95 / max |
| --- | --- | --- |
| 8 regions | 0.321 / 0.835 / 1.104 | 0.347 / 0.867 / 2.773 |
| 256 regions | 0.384 / 1.088 / 1.356 | 0.402 / 1.148 / 1.850 |

The 2.773 ms door sample included 1.995 ms in perception and 0.369 ms in
navigation. Repeating the 8-region case produced 0.317 / 0.815 / 1.720 ms.
The original tail is retained, not declared fixed. These remain well within the
existing ordinary-command target; no target was relaxed. Host scheduling/load is
uncontrolled, and the initial baseline overlapped baseline verification work;
these are diagnostic comparisons, not statistically isolated cost estimates.

The independent `place-knowledge-v1` workload measures 50 renames and 50 waits
at each scale. Rename command p50 / p95 / max is 0.021 / 0.037 / 0.051 ms for
2 places and 0.560 / 1.306 / 1.570 ms for 258 places. Large-case observation
construction has p95 1.650 ms. Update sizes peak at 10,593 / 29,651 bytes.
Every sample serializes one journal record and performs zero navigation refreshes.
Both durable cases recover exactly; checkpoint sizes are 49,564 / 1,243,249 bytes,
with restart times 6.82 / 174.67 ms. Discovery exercises 64 additional rooms.

Actual headless/native runs also contain 50 samples per label per scale. Native
rename presentation p95 is 33.98 / 41.82 ms (max 34.97 / 44.22 ms). Large-case
headless acknowledgement p95 is 52.21 ms, with 1,639 diagnostic remembered cells.
A distinct fresh-attachment run retains all 258 durable names while reducing
connection-local cell memory to 39: rename acknowledgement p95 becomes 10.07 ms
and native presentation p95 33.25 ms. The small fresh case has acknowledgement
p95 7.31 ms. Native application/draw maxima in the original large run are
0.268 / 2.217 ms.

The fresh comparison points to diagnostic map-memory/output costs, not a large
engine rename cost. Attachment also resets client-local history retention, so it
does not isolate a single encoding or scheduling cost. Reader/queue timings are
retained in the fresh run. Presentation intervals include transport, pacing,
stdout and driver scheduling; deferred logging removes reader disk writes only.
This is not closure of the existing 3p native/headless diagnostic timing work.

[Compact summaries](measurements/place-knowledge-2026-09-27/summary.json) and
[raw-file hashes](measurements/place-knowledge-2026-09-27/manifest.json) are retained
in Git. Raw data, logs and fresh benchmark saves remain local. Reproduction and
measurement boundaries are in the [harness guide](performance-harness.md#durable-place-workload-v1).
