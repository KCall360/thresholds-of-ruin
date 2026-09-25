# Phase E findings — 2026-09-25

Phase E removes avoidable historical-memory copies from shared client updates and
ASCII delivery, indexes rendering, and budgets native event processing. Protocol 12,
save format 5 and ruleset `diagonal-v11` are unchanged. Phase D merged in
[PR #26](https://github.com/KCall360/thresholds-of-ruin/pull/26) with Windows/Linux CI.

## Matched release client measurements

The same version-1 client example runs on merged Phase D and the changed tree.
Each case has 20 samples; numbers below are **p50 / p95 / maximum**, milliseconds.
There are 64 current cells, 64 or 20,956 historical cells, and a chart capped at
4096 cells. A burst applies every update before drawing once. Setup is untimed.

| Historical cells / burst | Phase | Phase D | Phase E |
| --- | --- | --- | --- |
| 64 / 1 | apply_ms | 0.123 / 0.232 / 0.447 | 0.058 / 0.069 / 0.081 |
| 64 / 1 | render_ms | 0.394 / 0.599 / 0.661 | 0.390 / 0.524 / 0.621 |
| 64 / 64 | apply_ms | 6.322 / 6.997 / 7.101 | 3.343 / 3.732 / 3.770 |
| 64 / 64 | render_ms | 0.533 / 0.661 / 1.654 | 0.429 / 0.611 / 1.126 |
| 20,956 / 1 | apply_ms | 11.458 / 17.434 / 24.431 | 0.590 / 0.882 / 3.642 |
| 20,956 / 1 | render_ms | 6.397 / 8.478 / 9.933 | 2.186 / 3.265 / 3.339 |
| 20,956 / 64 | apply_ms | 435.177 / 549.084 / 639.423 | 35.322 / 39.164 / 39.656 |
| 20,956 / 64 | render_ms | 5.798 / 6.554 / 6.556 | 2.365 / 3.031 / 3.106 |

Large-history burst application falls from **549.084 to 39.164 ms p95**. This is
the total for 64 updates, not one action. Large-chart rendering falls from 6.554
to 3.031 ms p95 in that case. Historical memory remains unbounded by design, but
updates no longer copy it. Work still includes current-view refresh, map alignment
and at most 4096 chart cells. The synthetic stationary case does not establish
constant-time movement; the maintained discovery trace also exercises translation.

Full actual discovery validates 77 eight-region and 2,805 256-region actions,
reaching 20,956 remembered cells. Authoritative p95 is 1.188/1.801 ms, with maxima
1.746/3.471 ms. All individual client/server phases remain in the raw file.

## Native presentation and tail investigation

The version-1 actual-client trace uses 256 regions, eight scheduled headless clients
and a native ASCII spectator: 495 accepted actions and 61 actor-1 presentations per
run. The final native loop targets 60 Hz versus 30 Hz in the reference, still drawing
only on changes. These are driver-to-report intervals, not keyboard-to-photon time.

| Run | Acknowledgement p50 / p95 / max | Presentation p50 / p95 / max |
| --- | --- | --- |
| actual-reference | 5.208 / 14.591 / 17.555 | 70.067 / 79.658 / 83.388 |
| actual-final | 5.256 / 15.601 / 182.930 | 33.899 / 45.479 / 148.577 |
| actual-no-capture | 5.896 / 16.308 / 304.187 | 28.947 / 37.492 / 43.843 |

The final capture run records a **148.577 ms** presentation tail. The preceding
frame spent **116.258 ms** in PPM capture, with 117.161 ms total diagnostic reporting;
the next turn interval was 134.013 ms. This identifies diagnostic I/O as a source
of a measured tail, not proof that it caused the earlier Phase D 673/738 ms spikes.
Without capture, presentation p95/max are 37.492/43.843 ms; native-call maxima in
both final runs were approximately 16.5 ms, including the frame limiter.

Separate secondary-actor acknowledgement outliers of 182.930/304.187 ms remain
unresolved. They are driver/headless/backend intervals, not ASCII application or
drawing measurements. Historical 673/738 ms tails did not recur. Do not claim all
outliers fixed or conflate server-only action targets with native presentation.
Diagnostic capture and stdout remain synchronous; ordinary play omits them.

### Same-binary follow-up

Consecutive no-capture runs use identical final server/headless executables and
change only the ASCII executable. Each again has 495 acknowledgements and 61
presentations. Reference presentation p50/p95/max is **52.231 / 69.638 / 74.141 ms**;
final is **29.928 / 37.825 / 38.909 ms**. Legacy acknowledgement p95/max is
15.090/19.287 ms versus 15.358/18.050 ms. The earlier acknowledgement spikes did
not recur; this does not establish their cause or eliminate scheduling variance.

The additional line-receipt timestamp records acknowledgement p95/max of
13.744/17.542 ms versus 13.844/16.887 ms. Driver log/queue delay peaks at
2.534/3.220 ms. The old `request_to_ack_ms` definition is preserved, and this
new metric makes that diagnostic overhead visible. The [reference follow-up](measurements/phase-e-2026-09-25/matched-reference.json)
and [final follow-up](measurements/phase-e-2026-09-25/matched-final.json) retain all
samples; source/binary hashes are in the manifest. No competing builds, tests or
launcher checks ran during the retained performance comparisons.

## Contracts and verification

Validation precedes mutation, including stream ordering and history consistency.
Invalid updates preserve the entire client boundary. Same-branch snapshots retain
memory allocations; branch changes clear them. Stationary charts reuse their index.
Rendering retains first-occurrence precedence, stale colors, current actors only
and visible-only input targets, checked against the existing vector glyph oracle.

ASCII sends ordered update/snapshot payloads through a 64-event bounded channel,
backpressuring the dedicated worker. A native turn consumes at most 16 events and
stops starting more after four milliseconds. It still applies every received view;
there is no last-state replacement that loses intermediate sightings. Server output
queues, slow-client disconnects and snapshot-on-relaunch behavior remain unchanged.
One update or OS call can exceed the budget. No speculative state is introduced.

Native Win32/X11 tests open a local note modal while SQLite is deliberately locked
during saving, both without checkpoints and with a pending interval-1 checkpoint
in a 256-region world. They verify the modal before releasing the lock, then verify
durability and checkpoint selection. A 160-action real-client burst checks native
input, final state, all 100 retained history entries and the durable checkpoint. Existing retry, privacy, rewind, mouse
and recovery suites continue to apply. Full local verification is recorded in the
session handoff: 219 Rust tests per debug/release, 84 Python checks with the
corrected wizard synchronization rechecked, all 64 release process tests, lint,
rustdoc, architecture/documentation and all four desktop launchers. The launcher
checks include three complete 256-region cycles and owned cleanup. Publication
requires Windows/Linux CI on the final commit.

The broader milestone 3p remains incomplete: unresolved timing tails, whole-world
checkpoint limits and scale-sensitive queries remain documented. No hardware
power-loss guarantee follows from process tests.

## Reproduction and retained evidence

See the [Phase E harness commands](performance-harness.md#focused-phase-e-client-study)
and [native timing boundaries](ascii-client.md#responsiveness-and-diagnostic-timing).
The [manifest](measurements/phase-e-2026-09-25/manifest.json) records source/binary
hashes, hardware, commands and limitations; the [summary](measurements/phase-e-2026-09-25/summary.json)
retains distributions. Raw [reference client](measurements/phase-e-2026-09-25/reference-client.jsonl.gz),
[final client](measurements/phase-e-2026-09-25/final-client.jsonl.gz) and
[discovery](measurements/phase-e-2026-09-25/discovery.jsonl.gz) samples are validated.
Actual-client [reference](measurements/phase-e-2026-09-25/actual-reference.json),
[final](measurements/phase-e-2026-09-25/actual-final.json) and
[no-capture](measurements/phase-e-2026-09-25/actual-no-capture.json) samples retain
individual tails. Frame phase reports are stored alongside them without credentials.
