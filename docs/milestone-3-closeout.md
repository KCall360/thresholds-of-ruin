# Milestone 3: interactions, narration, and stream recovery

The milestone completes the current interaction slice: known-cell travel,
cancellation and hazard interruption, text intentions and clarification,
approach-and-pickup/open/close, and durable named places. Protocol 13, save format 7,
and `places-v12` remain unchanged.

## Disclosed narration

Shared client state derives prose from consecutive validated observations and
explicit action results. Text and ASCII share action wording, including perceived
item and door names. Sight changes report a newly seen figure, a figure no longer
in sight, or a door now open/closed. Repeated portal views count once; seeing oneself
is not a discovery. Door changes require sight in both observations. Disappearance
never claims death or movement, and a changed door never identifies an unseen
actor as its cause. Hidden actions and unchanged views produce no invented prose.
Changes in disclosed readiness say when the character must wait or can act again,
without attributing that change to an unseen action.

These messages are transient presentation, not new durable history records.
Snapshots, including rewind and relaunch, reset the comparison baseline without
replaying old notices. Invalid updates leave state and prose unchanged. ASCII
shows the latest observation's prose above the status bar (up to two lines),
alongside the existing history panel. Adventure text preserves one-response
journey summaries and suppresses intermediate step narration during compound
intentions. The explicit `--script` diagnostic interface is unchanged.

## Interruption and recovery

Existing backend travel stops before another step when an actor is newly
perceived. Actors visible at departure are not new hazards; harmless
terrain/items/places do not stop travel. A revealing arrival can finish movement,
but text still cancels pending pickup or door use. Cancellation, blockers, another
actor's input turn, control loss, wizard changes, and rewind retain their existing
action-boundary behavior. See [travel](travel.md).

Bounded queues preserve ordered observations. Server queue overflow disconnects
the slow connection; it never silently drops an update. A slow spectator cannot
stop the controller's journey. A replacement connection receives committed state
and current travel status in a fresh snapshot. Clients reject sequence gaps
without applying the offending update. Relaunch remains the reconnection
mechanism; automatic reconnection is not added.

Real-process relay tests hold delivery while the controller commits actions,
then verify catch-up. The ASCII local places panel remains usable while delivery
is paused. A deliberately omitted observation verifies gap rejection and relaunch
recovery in both playable clients. This fault injection tests client ordering;
deterministic service tests separately exercise actual queue overflow. Neither
claims to resolve the deferred 3p scheduling/presentation tails.

## Verification and next session

`scripts/test_stream_recovery_process.py` covers delayed delivery, broken streams,
fresh snapshots, and both playable clients' semantic narration. Its two-actor
door fixture is `scripts/scenarios/semantic-narration.json`, reproduced with
existing wizard room, door, and teleport commands. Shared-state tests cover
deduplication, disclosure limits, atomic rejection, snapshot resets, and names.
Travel, adventure, doors, places, persistence, and native-input suites remain
part of full verification.

Focused release checks compare the ordinary client workload and real small/large
world driver before/after. `client_bench --narration` adds workload version 2 with
alternating disclosed actors and doors, 64/20,956 remembered cells, and 1/64-update
bursts; default version 1 is unchanged.

### Local verification, 2026-09-27 UTC

Formatting, all-target Clippy, debug/release workspace Rust tests, architecture
checks, and rustdoc with warnings denied pass. Final Python discovery passes
**102 tests** in debug; release actual-process discovery passes **69 tests**.
The added shared-state tests also pass in both profiles. An earlier run hit a
Cargo build-lock timeout and a native mouse-test process exit; both passed targeted
reruns and the clean final suites. Initial logs remain in `.local/m3-*`.
Final release desktop shortcuts pass real connection, spectator-role and owned
cleanup checks. Native framebuffer review verifies the narration footer.

### Focused release measurements

[Raw samples, summary](measurements/milestone-3-2026-09-27/summary.json) and
[binary/source hashes](measurements/milestone-3-2026-09-27/manifest.json) retain
the matched reference (`14daa91`) and final builds. Values below are milliseconds,
**p50 / p95 / maximum**, with 20 samples per synthetic case.

| Narration workload | Apply | Render |
| --- | --- | --- |
| 64 remembered cells, 1 update | 0.076 / 0.138 / 0.143 | 0.557 / 1.016 / 1.357 |
| 64 remembered cells, 64 updates | 4.703 / 5.313 / 5.902 | 0.773 / 1.531 / 1.820 |
| 20,956 remembered cells, 1 update | 0.644 / 0.979 / 4.935 | 2.455 / 3.664 / 3.798 |
| 20,956 remembered cells, 64 updates | 37.503 / 42.084 / 43.108 | 2.455 / 3.199 / 3.237 |

The unchanged version-1 large 64-update case has reference/final apply p95
43.906/37.629 ms and render p95 3.858/3.030 ms. The large single-update render p95
is 3.604/4.203 ms, while its median decreases from 2.992 to 2.561 ms. The new work
compares current disclosed views; it does not traverse retained history or add a
second copy of historical map memory.

| Real native presentation | Reference | Final | Committed actions |
| --- | --- | --- | --- |
| 1 region | 26.026 / 33.792 / 34.542 | 25.478 / 33.846 / 35.312 | 27 |
| 256 regions | 27.037 / 37.359 / 39.654 | 29.373 / 37.556 / 39.914 | 61 |

Each real run uses one actor and one mixed cycle at 15 ms driver pacing, with
correlation, deferred logs and no framebuffer capture. There are two additional
rejected blocked attempts per run. Discovered memory grows 44→156/263 cells;
these are short mixed traces, not complete 256-region exploration. Native final
apply p95 is 0.181/0.410 ms and drawing p95 is 1.435/1.669 ms for small/large worlds.
The extra narration field is only in diagnostic frames, not server wire traffic.

The [initial samples](measurements/milestone-3-2026-09-27/initial/summary.json)
retain a 159.810 ms large-burst apply maximum and reference native presentation
tails of 197.635/1672.990 ms. Three interleaved
[repeat comparisons](measurements/milestone-3-2026-09-27/initial/repeats.json)
have large-burst apply maxima of 38.681–42.341 ms before and 38.994–53.766 ms after;
medians vary in both directions. The final matched run above follows the footer
layout correction. These checks find no consistent material feature regression,
but they do not explain or fix the isolated tails. They remain diagnostic evidence,
and **do not close 3p or relax its timing gates**.

Next is **4a: authored scenario packages and offline validation**. At the user's
explicit direction, damage-triggered interruption belongs to 4d with HP/damage and
combat. Concrete resumable timed actions, offscreen named-place destinations,
locks/keys/containers/equipment/item use remain deferred as recorded in the
[roadmap](milestones.md). Milestone 3p remains deferred and open.
