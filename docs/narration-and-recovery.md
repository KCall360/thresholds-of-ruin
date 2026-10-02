# Narration and stream recovery

Both playable clients describe what the character perceives changing, and both
recover from slow or broken connections without losing or inventing state.

## Disclosed narration

The shared client state derives prose from consecutive validated observations,
explicit action results and combat events. The ASCII client shows that prose;
the text client composes its own from the same facts (see
[the IF engine](if-engine.md#the-chronicle)). The server sends facts, never
sentences.

Sight changes report:

- a newly seen figure;
- a figure no longer in sight; or
- a door now open or closed. Door changes are reported only when the door was
  in sight in both observations.

The narration never invents facts:

- Repeated portal views of the same thing count once, and seeing yourself is
  not a discovery.
- A figure leaving sight is never described as dying or moving.
- A changed door never names an unseen actor as its cause.
- Hidden actions and unchanged views produce no prose.
- ASCII says when the character must wait or can act again; the text client
  never mentions readiness, and returns its prompt when the character can act.

Narration is transient presentation, not durable history. Snapshots, including
those after rewind and relaunch, reset the comparison baseline without replaying
old notices. Invalid updates leave both state and prose unchanged.

**ASCII** shows the latest observation's prose above the status bar (up to two
lines); F2 still opens durable history. Automation frames include the same prose
in a `narration` array, which is empty immediately after a snapshot.

**Text** tells everything between two prompts as one passage: the intention,
what happened on the way, how it ended, and what else came into or left sight.
Updates between turns are gathered and told together above a fresh prompt. The
`--script` diagnostic interface is unchanged.

## Interruption

Travel stops before its next step when an actor is newly perceived. Actors
already visible when the trip started aren't new hazards, and harmless terrain,
items, and place hints don't stop travel. If the step that reveals a hazard also
reaches the destination, the movement finishes, but text still doesn't go on to
the pickup or door use it walked over for. Positive HP loss also interrupts travel; see
[dungeon gameplay](dungeon.md). Blocked moves, being thrown off course,
control loss, wizard changes, and rewind behave as described in
[travel](travel.md); a journey waits, rather than stopping, while another
player acts.

## Slow and broken streams

- Bounded queues preserve ordered observations, and the server never silently
  drops an update. Play pauses while a connection's queue is nearly full and
  resumes when it reads; a connection that stays nearly full for five seconds,
  or that overflows while a request is handled, is disconnected.
- A slow spectator can delay the controller's journey but can't stop it.
- A replacement connection receives the committed state and current travel
  status in a fresh snapshot.
- Clients reject sequence gaps without applying the offending update.
- Relaunching the client is the reconnection mechanism. Automatic reconnection
  isn't implemented.

## Verification

- `scripts/test_stream_recovery_process.py` covers delayed delivery, broken
  streams, fresh snapshots, and semantic narration in both playable clients. A
  relay holds delivery while the controller commits actions and then checks
  catch-up. The ASCII places panel stays usable while delivery is paused. A
  deliberately omitted observation checks gap rejection and relaunch recovery.
  The two-actor door fixture is `scenarios/tests/semantic-narration`.
- Deterministic service tests exercise actual server queue overflow, prove
  that play pauses for a nearly full queue without losing an update, and that
  the server drops a spectator that stops reading while the journey finishes.
- Shared-state tests cover deduplication, disclosure limits, atomic rejection,
  snapshot resets, and perceived names.
- The client workload has a narration variant:
  `client_bench --narration` (workload version 2) alternates disclosed actors and
  doors at 64 and 20,956 remembered cells, with 1- and 64-update bursts. The
  default version 1 workload is unchanged. See the
  [performance harness](performance-harness.md#client-workloads).

When this was introduced, the 20,956-cell, 64-update narration burst applied in
42.1 ms p95 in total and rendered in 3.2 ms p95. Real native presentation was
unchanged within noise. The comparison work touches only the current disclosed
views; it doesn't traverse history or copy remembered map memory.
