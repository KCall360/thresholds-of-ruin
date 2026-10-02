# Running play until it needs input

The server runs the game until it needs input from a client. AI turns and
journey steps run back to back, and no timer advances the game. Clients space
out what the player sees. The maintainer agreed this design on 2026-10-01; it
replaced a 75-millisecond server pump and client travel cancellation.

## Why

- **Wall-clock timing no longer decides game order.** With the pump, whether a
  client's command landed before or after an AI step depended on when the pump
  ticked; the race fixed in PR #52 came from that.
- **Interruption belongs to the server.** Stopping a journey from the keyboard
  doesn't fit the roguelike model. The server stops travel on hazards, HP loss,
  lost control, world changes and being thrown off course.
- **"Blocked" was already defined.** The scheduler names the next actor; the
  server keeps going until that actor is one a client controls and it has no
  journey running.

## The simulation thread

`Simulation::start` gives the `Service` a single owner: a dedicated thread with
its own current-thread Tokio runtime, so a long run never starves the socket
tasks. There's no session mutex. Everything that changes the session arrives
in one mailbox: connects, requests, disconnects and shutdown. `serve` connects
clients to it and returns the service once it's shut down and flushed.
`SimulationHandle::with` runs a closure on the service between actions, for
tests and tools.

The thread's loop:

1. Handle every message already waiting.
2. Take one action (an AI action or a journey step) if anything can act, then
   go back to 1.
3. Otherwise wait for a message, for a client's queue to drain, or for the next
   save check.

A request waits for at most one action, never a whole run. Interruptions land
only between actions, so no action is ever split. What happens in the game
depends only on the game and the commands it receives.

### When a run stops

A run continues while one of these holds:

- the next actor has a journey running; or
- the next actor is AI-controlled, autonomy is enabled (today's rule: on after
  an action or `continue`, off after release, disconnect or a wizard change),
  and at least one actor a client controls is alive.

It stops when the next actor is one a client controls with no journey running,
when an uncontrolled non-AI actor is next, when nothing can act (for example
after the selected character dies), or when a client's queue is nearly full.
A run that never reaches a controlled turn can't hang the server, because
requests are still handled between actions; there's no action limit.

### Journeys

A journey takes a step whenever its actor is next. While another player's
actor is next, it waits, and continues when its turn comes back. Before every
step, the server checks for control loss, HP loss and newly seen actors, so
something that happened while the journey waited, such as another player
opening a door, can end it. After a step it also stops on arrival, a blocked
move, or being displaced or struck (`decision_required`). See
[travel](travel.md#execution-and-interruption).

### Outgoing queues and slow clients

Each client has a 256-message queue. A journey's steps arrive almost at once,
so the server applies backpressure instead of filling queues blindly:

- **Play pauses while any attached client's queue has fewer than 16 free
  slots**, and resumes when the client reads. The thread keeps handling its
  mailbox while it waits.
- **A client that stays that full for five seconds is disconnected**, like a
  socket write that times out. A slow spectator can delay a journey but can't
  stop it.
- A queue that overflows while a request is handled still disconnects the
  client; it never silently misses an update.
- Updates stay one per step: view deltas keep them small, and clients need step
  boundaries to animate.

### Requests during a run

| Request | Behavior |
| --- | --- |
| Action or travel command for an actor whose journey is running | Rejected with `actor_busy` |
| Other commands (for example renaming a place) | As before |
| Wizard commands, including rewind | Applied at the next boundary; journeys stop with `world_changed` |
| `save` | A durability barrier only; it doesn't stop travel |
| `snapshot`, `history`, `history_branch`, `palette` | Answered at the next boundary |
| `acquire_control`, `release_control`, disconnect | At the next boundary; release and disconnect stop the journey with `control_lost` |
| `continue` | As before |

Reads are answered on the simulation thread. Serving them from published
snapshots elsewhere is deferred until measurements show reads waiting too long
behind single actions.

### Saves

Save acknowledgements and the "saving is behind schedule" warning are checked
every 50 ms, between actions while a run continues and on a timer while play
waits. That timer never advances the game.

## Clients

`Connection` in `client-common` spaces out the updates the player sees:

- Observation updates (full or delta) are shown at least `pace` apart; the
  first after a pause shows at once. Everything else applies on arrival. The
  server sends an observer an update only when its own state changes, so
  actions the player can't see take no display time.
- A held update waits inside `Connection::next`, which stays safe to cancel.
  The client reads the next update only when it's due, so a slow display paces
  the server through the queue backpressure above.
- `skip` shows what has arrived without waiting, until the next request.
- The text and ASCII clients default to 75 ms (`--pace <ms>`); the text client's
  `--script` mode and the headless client use 0. In the text client, `pace [ms]`
  shows or changes it, and another command first shows the rest of a journey at
  once. In the ASCII client, `[` and `]` step through 0–500 ms, and any key
  during a journey shows the rest of it.

## Protocol changes

In the current protocol version:

- `Request::CancelTravel` and the `cancelled` travel phase are gone.
- `ErrorCode::ActorBusy` rejects action and travel commands during a journey.
- `save` no longer stops an active journey.

No save-format change was needed: travel status and jobs are session-local.

## Tests

- **Session service tests** (`session.rs`, `run_tests.rs`): AI turns run
  until a controlled actor is due; AI doesn't play when nobody controls an
  actor; a full client queue pauses the run and it resumes without losing an
  update; a journey completes in one run; commands during a journey get
  `actor_busy`; `save` during a journey doesn't stop it; a journey waits while
  another player acts and then continues; another player's action (opening a
  door) can interrupt a waiting journey; a slow spectator pauses the journey
  until dropped and a replacement resynchronizes at the committed state.
- **Runner tests**: mail already waiting is handled before the next action
  (the PR #52 race); a spectator that never reads is dropped after the stall
  timeout and the controller's journey finishes, over the real mailbox.
- **Clients**: the ASCII presentation tests skip a journey and change the pace;
  the client-common travel tests use the remaining phases.
- **Process**: `scripts/test_run_until_blocked_process.py` pauses a real
  spectator's connection while the player keeps acting: play continues, the
  spectator is disconnected after the stall timeout, and a new spectator
  starts at the committed state. The travel, adventure, dungeon and stream
  recovery suites also run against the new server.

## Performance

Requests no longer wait for a pump tick, and a journey finishes as fast as its
steps can be computed and delivered. The `server_handled` timing event's
`lock_ms` is now how long a request waited in the mailbox.

## Open questions

1. **Default playback delays,** and whether they should vary by kind of update
   (for example, movement quicker than combat). To be settled by playtesting.
