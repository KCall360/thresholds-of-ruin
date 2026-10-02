# Running the simulation until it blocks (planned)

This is a plan, not a description of current behavior. The maintainer agreed
the direction on 2026-10-01; nothing here is implemented yet.

Today a 75-millisecond server pump advances autonomous play and travel one
action at a time (see [travel](travel.md#execution-and-interruption) and
[dungeon gameplay](dungeon.md)). This plan removes the pump. The server runs
the simulation until it needs input from a client, and clients pace what the
player sees.

## Why

- **Wall-clock timing decides game order.** Whether a client's command lands
  before or after an AI step depends on when the pump ticks. The race fixed in
  PR #52, where an AI step made an already-read command stale, came from this.
- **The pump's only real feature is client cancellation.** Stopping a journey
  from the keyboard doesn't fit the roguelike model. The server already stops
  travel on hazards, HP loss, lost control and world changes, and that's where
  interruption belongs.
- **"Blocked" is already defined.** `Engine::next_ai_action` returns nothing
  when the next scheduled actor has no AI. The server only has to keep going
  until that's true and no journey is running for that actor.

## Decisions

- The pump is removed. No timer ever advances the simulation.
- Client travel cancellation is removed completely: `Request::CancelTravel`
  and the `cancelled` travel phase go away in the next protocol version.
- Only the server interrupts runs and journeys, at action boundaries.
- Clients pace display themselves. The headless client applies updates
  immediately.

## The simulation task

The `Session` gets a single owner: a dedicated OS thread running its own
current-thread Tokio runtime, so a long run never starves the socket tasks.
The `Mutex<Session>` goes away.

Everything that changes the session arrives as a message in one mailbox:
requests, connect, disconnect, the save notifications below, and shutdown.
Connect and disconnect, which lock the session directly today, move into the
mailbox too.

The task loop:

1. Wait for a mailbox message while blocked.
2. Handle it.
3. While not blocked, take one action (an AI action or a journey step), then
   handle every message already waiting with a non-blocking receive.

Requests wait at most one action, never a whole run. There's no chunk size to
tune. Interruptions land only between actions, so no action is ever split.

### When a run stops

A run continues while all of these hold:

- autonomy is enabled (today's `autonomous_enabled`: on after an action or
  `continue`, off after release, disconnect or a wizard change);
- at least one actor has a controller;
- the next actor is AI-controlled, or is a controlled actor whose journey is
  still running;
- no controller's outgoing queue is full (see below).

When the next actor is controlled and has no journey running, the run blocks
on that actor's input. Other actors' journeys stay active and continue when
their actors' turns come round.

Journeys keep their current end conditions: arrival, blocked movement, hazards,
HP loss, displacement or impact (`decision_required`), control loss, world
changes and failures. They're checked before every step, so a journey ends only
if something that happened while it waited (an AI action or another player's
action) meets one of those conditions.

Three end conditions go away:

- Another actor being due. Today a journey ends with `decision_required` when
  its actor isn't next. Instead it waits for its actor's turn.
- A manual action, and a replacement journey. Commands for an actor with a
  running journey are now rejected (see
  [requests during a run](#requests-during-a-run)).

If no controlled actor is alive and loaded, autonomy switches off, so a run
can't continue with nobody to play. There's no limit on how many actions a run
can take: a run never stops the server handling requests, so a run that never
reaches a player's turn would only be a bug costing CPU, not a hang. It's
reported in the run's timing diagnostics.

### Outgoing queues and slow clients

Without the pump, a 40-step journey produces 40 updates almost at once. That
would overflow today's 64-message client queues, and overflow disconnects the
client.

- **Controllers apply backpressure.** When a controller's queue is full, the run
  pauses, as if blocked, until there's space. The task keeps handling its
  mailbox while it waits. A dead socket still fails within the existing
  write timeout and disconnects.
- **Spectators keep today's rule.** Overflow disconnects them, and a slow
  spectator still can't stop a journey.
- **Updates stay one per step.** View deltas keep messages small, and clients
  need step boundaries to animate. No coalescing.

### Requests during a run

| Request | Behavior |
| --- | --- |
| Action or travel command for an actor whose journey is running | Rejected with a new `actor_busy` error code |
| Other commands (for example renaming a place) | As today |
| Wizard commands, including rewind | Applied at the next boundary; journeys stop with `world_changed`, as today |
| `save` | A durability barrier only; it no longer stops travel |
| `snapshot`, `history`, `history_branch`, `palette` | Answered at the next boundary |
| `acquire_control`, `release_control`, disconnect | At the next boundary; release and disconnect stop the journey with `control_lost` |
| `continue` | As today |

Serving reads from published snapshots outside the simulation thread is
deferred. Add it only if measurements show reads waiting too long behind single
actions.

### Saves

`poll_saves` runs on every pump tick today, and save acknowledgements depend on
it. Instead, storage sends a mailbox message when the durable sequence advances
or a save fails. The "saving is behind schedule" warning depends on wall time,
so a slow housekeeping timer remains for it. That timer only checks save status
and never runs the simulation.

## Client changes

- `client-common` gets a playback queue. It receives and validates updates as
  they arrive, then releases them for display at a pace set by the client. The
  headless client releases them immediately.
- Only updates the player can see are spaced out. Actions the player can't see
  take no display time: the server already sends an observer an update only
  when its revision changes, and the client skips the delay for updates that
  change nothing on screen.
- The delay between visible updates is a client option, and the player can
  change it during play. It may vary with the kind of update; for example,
  movement steps could be quicker than combat. The defaults will be set by
  playtesting.
- The text client narrates journey steps at that pace. The ASCII client
  animates them at its frame rate.
- A key press during playback skips to the latest state. It only affects the
  display and sends nothing to the server.
- The text and ASCII clients' cancel-travel paths are removed.

## Protocol changes

All in one protocol version bump:

- Remove `Request::CancelTravel` and `TravelPhase::Cancelled`.
- Add `ErrorCode::ActorBusy`.
- `save` no longer stops an active journey.

No save-format change is needed: travel status and jobs are already
session-local.

## Slices

Each slice keeps the existing tests passing, except those it deliberately
replaces.

1. **Single-owner session.** Move the session onto its own thread with a
   mailbox, route connect and disconnect through it, and remove the mutex. The
   pump stays, driven by a mailbox tick. No behavior change.
2. **Run until blocked.** Replace the pump with the run loop, add controller
   backpressure, the stop rules and the save notifications.
3. **Protocol.** Remove cancellation, add `actor_busy`, stop `save` from
   cancelling travel, and bump the protocol version.
4. **Clients.** Add the playback queue, remove the cancel paths and update the
   headless client.
5. **Docs, performance and launchers.** Run the comparison, rewrite the guides
   below and rebuild the three desktop launchers.

## Tests

Following the [testing policy](testing.md), every layer gets tests, including
actual-process acceptance tests:

- **Session (service tests):**
  - a journey completes in one delivery;
  - AI turns run until a controlled actor is due;
  - a disconnect, release or rewind during a run takes effect at the next
    boundary;
  - a full controller queue pauses the run, and it resumes without losing an
    update;
  - a slow spectator is still disconnected and the journey continues;
  - commands during a journey get `actor_busy`;
  - `save` during a journey is acknowledged without stopping it;
  - a journey waits while another player acts and then continues, and ends
    only when that player's action triggers a standard interruption;
  - a run with no controlled actor alive and loaded switches autonomy off.

  These replace the two pump tests, `the_autonomous_pump_moves_the_revision_when_nothing_is_waiting`
  and `an_already_read_wizard_command_is_applied_before_the_autonomous_pump`,
  and change `slow_client_is_disconnected_and_releases_control_instead_of_losing_updates`
  for controllers.
- **Determinism:** the same commands produce byte-identical journals whether or
  not the stream relay delays delivery.
- **Protocol:** round-trips without the removed request and phase; the new
  error code.
- **Clients:** in `client-common`, pacing of visible updates, no delay for
  invisible ones, changing the delay and skipping ahead; immediate release in
  headless; the ASCII presentation tests without cancel travel.
- **Process:** update `test_travel_process.py`, `test_adventure_process.py`,
  `test_dungeon_process.py` and `test_stream_recovery_process.py`, and remove
  cancellation from `performance_driver.py`.

This changes the protocol, so the `full` verification tier runs before every
push, and CI on both platforms is required before merging.

## Performance

- Compare with `scripts/perf_compare.py` against the base: the `latency_bench`
  headline cases, travel, and the 4e streaming cases. Journeys will finish much
  faster in wall time, so report that separately from per-action costs.
- Add a timing event for how long a request waited in the mailbox, and report
  its p95 and max during long runs.
- Show that per-action overhead from the mailbox check doesn't grow with map
  size, using operation counts at 16, 256 and 4,096 regions.

Start this work after the 4e performance comparison, so that removing the pump
doesn't change the 4e baseline.

## Guides to update when it lands

[Travel](travel.md), [dungeon gameplay](dungeon.md),
[narration and stream recovery](narration-and-recovery.md),
[protocol](protocol.md), [text adventure](text-adventure.md),
[headless client](headless-client.md), [ASCII client](ascii-client.md),
[architecture](architecture.md) and the [roadmap](milestones.md).

## Open questions

1. **Default playback delays,** and whether they vary by kind of update. To be
   settled by playtesting once the playback queue exists.
