# End-to-end simulation cycle

This is the execution-order reference: how a request becomes accepted work,
simulated effects, a journal boundary and observer-safe output. It describes
implemented behavior, not a proposed fixed-rate loop. The source review used
main `dacbc215e12aaa13ef3cf89900836a296127f074`.

[Actions and scheduling](simulation.md) owns action timings, paid plans and
cancellation. [Creature rules](../reference/creature-rules.md) owns combat and
survival arithmetic. [Protocol lifecycle](../reference/protocol-lifecycle.md)
joins connection, retry and recovery sequences; [protocol](../reference/protocol.md)
owns wire fields and limits. [Persistence](persistence.md) owns durable formats and
replay. This page joins those contracts without creating another schema owner.

## The clocks and boundaries are different

| Boundary | Meaning | Does not mean |
| --- | --- | --- |
| Mailbox pass | The single service owner handles a bounded batch of messages. | A turn, tick, or fair CPU-time share per client. |
| Intention admission | Queue identity and journal acceptance commit. | Execution, preparation payment or completed recovery. |
| Action execution | A due actor validates and applies work on a private candidate. | Exactly one tick, one strike or one outbound update. |
| Simulation tick | Integer rule time, advanced through deadlines and physics. | Wall-clock milliseconds or a rendering frame. |
| Actor-active time | Elapsed time for that actor, stopped while frozen. | The global tick difference during its frozen interval. |
| Committed engine boundary | Candidate effects, knowledge, journal and input generations passed save admission. | Disk durability or delivery to a client. |
| Outcome update | Ordered observer-filtered causal facts and possible visual invalidation. | A full picture or permission to infer hidden events. |
| Observation | Requested picture of committed state with connection-specific identity. | Receipt, durable checkpoint or simulation clock. |

One action can cross several timer/preparation deadlines before another actor
needs input. Several actors can instead act at the same tick.
`ActionOutcome.at_tick` records the action time; `next_tick` is the resulting
clock. Journal records retain before/after ticks separately. Initial state,
accepted operation order, content and recorded random state determine the run;
competing mailbox arrival order still determines which requests are admitted.

## Numbered request-to-publication cycle

### 1. Receive a bounded message and hand it to the owner

Transport framing/decoding, authentication and handshake precede session
requests. Network tasks send connects, requests, disconnects and shutdown through
one bounded mailbox. A dedicated simulation thread owns `Service` on its own
current-thread Tokio runtime; network tasks do not mutate `Game`.

The runner drains at most mailbox capacity (256 messages) per pass, polls save
status when due, then calls `Service::step` once. It repeats after progress. While
blocked it waits for mail, output capacity or save/stall timers. No request
interrupts a half-executed action, and replacement arrivals cannot extend one
drain pass indefinitely. This bounds message count, not action/query CPU cost.
Save polling and sustained-output-stall deadlines use wall time; neither advances
the game. A closed, empty mailbox ends the run before another action.

Source: [runner.rs](../../crates/server/src/runner.rs), `run`, `receive`;
[session.rs](../../crates/server/src/session.rs), `Service::step`.

### 2. Check authority before treating a command as fresh work

For a command on an attached session:

1. Check wizard enablement/role where applicable and the role allowlist. The
   attachment's allowed-actor check already establishes its acting identity.
2. Decode the command and look for an already accepted matching receipt. An
   authorized exact retry returns that result, not a second execution.
3. For fresh work, check attachment/input context and readiness generation, then
   branch, then required controller ownership.
4. Check operation-specific disclosed capabilities, selections and live
   preconditions. A client noun, coordinate or old picture identity is not raw
   engine authority.

Receipt lookup before freshness does **not** bypass role/actor authorization.
It permits an accepted operation's retry after its old context has expired.
Read-only requests, control changes, immediate metadata and gameplay admissions
have different handlers; not every request enters the action path. The engine
also checks its own receipt/request contract before creating its candidate.

Source: [session.rs](../../crates/server/src/session.rs), `process`;
[engine.rs](../../crates/server/src/engine.rs),
`apply_command_profiled_with_annotation`, `execute_checked_command`.

### 3. Admit an intention without doing its gameplay work

Ordinary wire `Act` becomes backend `AdmitIntention`. Admission checks the bounded
queue slot, validates the action, captures human operand authority and the
selected movement/terminal-exit context. Travel additionally binds its expected
destination. The queue stores requested work, not a paid frozen preparation.
Action-duration rate, preparation timing and costs are captured when execution
starts, not when the request arrived.

Autonomous admission stores `AiDecision` and does no observation, path search or
random choice. Those occur at execution against current state. Admission itself
is a candidate journal mutation and must pass persistence admission before
publication. Its receipt proves acceptance, not completion. An uncertain retry
keeps the original command identity rather than manufacturing new queued work.

Source: [intention.rs](../../crates/simulation/src/intention.rs),
`admit_action_intention`, `admit_ai_intention`;
[server intention.rs](../../crates/server/src/intention.rs).

### 4. Select work, respecting session stop conditions

`Service::step_inner` first retries pending travel cancellations and preparation
pauses; if they remain pending, it blocks. It next checks attached-client output
headroom and arena execution policy. Then:

1. Execute the selected queued intention, if any.
2. Otherwise find the next actor; announce waiting if none exists.
3. If that actor has a journey, run its next travel step.
4. Otherwise admit and execute an eligible AI decision, or announce waiting.

Human-origin work needs a controller while its actor is alive. Autonomous work
needs autonomy enabled and a driver: a living controlled actor in adventures,
or the bounded unattended-arena policy. Travel work must match the journey's
pending admission and pass current readiness. A due human with no executable
intention/journey is an input stop; later actors do not skip ahead because they
have submitted commands.

The core scheduler is more specific than “earliest actor first”:

- Terminal runs have no scheduled actor. Paused arena execution exposes no
  executable next actor.
- Ready, living, unfrozen actors whose conditions allow voluntary input and who
  belong to the explicit **input-boundary set** take priority in actor-ID order.
  Lifecycle interruption/input-return paths maintain that set.
- Otherwise choose minimum `(ready_at, ActorId)` among living, unfrozen actors
  allowed voluntary input and without an active preparation.
- Intention selection first retires work whose actor is truly unavailable, in
  intention-ID order. A known but unloaded actor is not simply treated as absent.
  Otherwise only queued work belonging to `next_actor` is selected.

This is neither FIFO command execution nor simultaneous turn resolution. Stable
admission order breaks unavailable-work cleanup ties, not ordinary actor turns.

Source: [session.rs](../../crates/server/src/session.rs), `step_inner`,
`execute_queued_intention`; [lib.rs](../../crates/simulation/src/lib.rs),
`next_actor`, `next_scheduled_actor`;
[intention.rs](../../crates/simulation/src/intention.rs), `selected_intention`.

### 5. Revalidate and prepare at the execution boundary

The engine captures a private command candidate and starts its retrieval
transaction. Simulation selects the intention again through the same scheduler.
Movement/terminal-exit context must still match. Resume uses retained owned work;
fresh human work uses its captured operand binding. AI chooses executable work
now through ordinary visibility and rules.

The short-lived `PreparedAction` checks readiness, live preconditions, timing
arithmetic and plan requirements. It cannot survive the call as a promise about
future state. `commit_action` then clones a private simulation candidate. This
nested candidate is another atomicity boundary, not an independently committed
turn. No callback or network yield intervenes between preparation and use.

Source: [actions.rs](../../crates/simulation/src/actions.rs), `act_at_entry`,
`prepare_action`; [intention.rs](../../crates/simulation/src/intention.rs),
`execute_next_intention`.

### 6. Apply the action and walk to the next decision

The ordinary `commit_action` order is:

1. Initialize discovery; begin the action/event boundary, clear prior physics
   impacts/displacement, remove the actor's old input boundary.
2. Redirect confused movement on the candidate, update the run's action policy,
   and validate bound human operands.
3. Apply the action effect. Immediate actions take effect now; timed work installs
   its paid preparation rather than pretending to resolve immediately.
4. Install chosen AI/behavior state if applicable and reconcile its recovery
   contexts. Synchronize the acting actor's region lifecycle.
5. Set captured readiness/recovery. Unless the outcome is the special escaped
   case or a region handoff is pending, resolve already-due preparations, check
   run policy, then advance through deadlines to the next decision.
6. Reconcile passive discovery, awareness and control contacts; settle aggravation
   and periodic search; seal participant events; publish the inner candidate only
   if all of this succeeded.

The deadline walk chooses the minimum of the next actor's readiness, active
preparation completion, indexed creature timer and run deadline. Physics can stop
earlier at another exact boundary. After a changed tick, the caller resolves
preparations and checks run policy again. It stops for a due decision, terminal
or paused policy, or a region handoff. A timer wake does not itself grant input:
if everyone is still preparing, it continues to the next useful deadline.

Source: [actions.rs](../../crates/simulation/src/actions.rs), `commit_action`,
`finish_action`, `advance_to_next_decision`;
[combat.rs](../../crates/simulation/src/combat.rs), `next_attack_tick`.

### 7. Finish region handoffs before exposing a boundary

Host entry APIs can return `CandidateStep::AwaitingRegions`. This is a private
continuation, not a partially successful public action. The host settles residency,
activation and deferred entry work through the region manager before final
observer views and recomputes final `next_actor`/`next_tick`. The action transition
refreshes navigation when geometry, locomotion or disclosed scene changes and
reconciles attached actor membership. A candidate cannot commit with
`region_transition_pending` still set.

Moving physics completes its existing entity cohort before the host settles
horizons. Explicit exclusions prevent activation/retrieval-born items and, during
a pending entry, newly created corpses from receiving an inappropriate same-tick
integration. Deferred body returns have a guard covering early exits. Do not add
a host return halfway through the actor/item iteration.

Source: [floor_activation.rs](../../crates/simulation/src/floor_activation.rs),
`CandidateStep`, host entry methods;
[engine.rs](../../crates/server/src/engine.rs), `settle_action_entry`,
`resolve_action_transition`, `transition_with_observations`;
[physics.rs](../../crates/simulation/src/physics.rs), `advance_physics`.

### 8. Capture disclosure and candidate journal metadata

Before mutation, `Service` captures compact semantic state for unique attached
observer actors. During the candidate transition the engine validates observer
projection, captures event-time observation receipts and refreshes navigation
from authorized scenes. After simulation/region work it derives intention
suspensions/ends from typed before/after state, records activation/retrieval
metadata, and records explored and observation knowledge before retaining the
immutable game boundary.

These are authoritative knowledge/event receipts, not eagerly issued rich wire
pictures or persisted display revisions. Later visibility cannot retroactively
grant or remove an event's original disclosure authority. Exact appearances,
redaction and historical evidence belong to [perception](perception.md) and the
protocol's event/history contracts.

Source: [engine.rs](../../crates/server/src/engine.rs), `resolve_action_transition`,
`Candidate::record_explored_knowledge`, `Candidate::record_observation_knowledge`,
`commit_candidate_with_resolution`;
[intention_lifecycle.rs](../../crates/server/src/intention_lifecycle.rs).

### 9. Admit persistence, then publish authoritative state

Input generations are prepared but not yet published. The engine validates and
admits the journal record and associated source/region payloads to storage. The
producer gate checks capacity and worker failure/shutdown state, encodes the
journal frame, captures a checkpoint if due and rechecks admission before enqueue.
Some operations require extra checkpoint preflight; asynchronous checkpoint
encoding is not a universal pre-publication durability barrier.

Only successful admission allows candidate publication, history/receipt append
and input-generation publication. With a save worker this means **accepted**, not
necessarily **durable**. The worker later commits its database batch and advances
`durable_sequence`; explicit save waits for its relevant accepted prefix. An
asynchronous failure retains pending work and reports an error. It does not undo
accepted gameplay, and later mutations cannot silently pass the failed save gate.

Source: [engine.rs](../../crates/server/src/engine.rs),
`commit_candidate_with_resolution`, `admit`, `append_record`;
[storage.rs](../../crates/server/src/storage.rs); [persistence](persistence.md).

### 10. Publish causal updates and readiness; project pictures on demand

For an executed intention, service accounts committed travel movement before
output failure can disconnect its owner. `action_result_update` emits the action
update then derived intention status; service finishes any travel result.
`Service::step` refreshes readiness after `step_inner`, including blocked paths.
Request handlers can also emit effects/readiness before their correlated reply;
do not assume one universal ack-before-update ordering.

For each attached observer, `action_update` compares compact semantic state,
applies role-specific item redaction and derives causal facts from the committed
event boundary. Only the acting observer receives that operation's own history
entry. Required palette keys precede the outcome that uses them; obsolete keys
retire afterwards. If no visible change, disclosable facts or own entry exists,
a hidden-only boundary sends no empty outcome. This is not permission to drop an
already issued outcome.

`Observe` separately requests current committed presentation state. It neither
advances time nor retrospectively narrates missed events. Clients can coalesce
pending visual demand, not reorder or discard causal outcomes. Input freshness
is independent of picture revisions. Serialization/queue admission precedes
commit of stream sequence and the new picture delta base. If post-commit causal
projection or output admission fails, the affected stream disconnects rather
than silently missing an outcome; gameplay is not rolled back. Healthy peers can
continue. Recovery follows the [protocol lifecycle](../reference/protocol-lifecycle.md).

Source: [session.rs](../../crates/server/src/session.rs),
`execute_queued_intention`, `action_result_update`, `action_update`, `step`,
`refresh_readiness`, `observe`, `update_at`.

## Exact tick advancement and simultaneous deadlines

There is no global “update every actor, then render” sweep. Actor time stays lazy
until touched or due; an ordered timer index wakes the scheduler. Both physics
paths settle due creature changes before due work completion, but do not execute
an identical list once per tick.

### Stationary path

`advance_physics` bounds its endpoint by applicable automatic-event, wandering
and light-fuel deadlines. Stochastic nutrition deadlines cannot be stepped over.
If there is no world gravity and no moving actor/item, it can:

1. Burn light fuel for the elapsed interval.
2. Set the tick to the endpoint.
3. Settle due creature time and automatic work.
4. Refresh light and return through the deferred-body-return guard.

The caller then resolves preparations and checks run policy. A world with gravity
can also become quiescent after an integration pass finds nothing active; its
remaining interval uses final timer/light settlement at the endpoint. These are
exact analytical jumps, not omitted regeneration or hunger.

### Moving path

For each integer tick while physical integration is active:

| Phase | Operation / early-exit rule |
| --- | --- |
| 1 | Enter deferred-body-return guard, burn one tick of light fuel, set tick. |
| 2 | If an indexed creature deadline is due, settle creature time **before physics**. Terminal outcome can stop the tick. |
| 3 | Snapshot actor IDs and integrate living, unfrozen actors in order. Publish position/frame/motion, mark displacement, reconcile trap contact, holds and spatial condition anchors, then synchronize lifecycle after each actor. |
| 4 | Snapshot eligible ground-item IDs and integrate unfrozen items in order. Exclude specified newborns; invalidate item claimants when moved. |
| 5 | Refresh light. If no integration is active, return for pending region handoff or use resting fast-forward. |
| 6 | Settle creature time again for consequences born during physics; stop if terminal. |
| 7 | Resolve preparations, check run policy, respect terminal outcome/pending region handoff. |
| 8 | Return if next actor is due; otherwise integrate another tick. |

The outer deadline walk can resolve preparations again after this return. Due
preparation processing also settles its owner/target first. These are deliberate
settlement/revalidation points, not repeated payment or extra attacks. Equal tick
numbers do not imply simultaneous frozen snapshots: earlier timers, actors and
strike rows can make later work unavailable.

Source: [physics.rs](../../crates/simulation/src/physics.rs), `advance_physics`;
[actions.rs](../../crates/simulation/src/actions.rs), `advance_to_next_decision`.

## Creature time, regeneration, hunger and death

The timer index orders `(deadline, ActorId)`. `settle_creature_time` collects due
owners, settles them in that order, then reconciles recovery contexts, passive
discovery, periodic searches, periodic relocations, aggravation, awareness,
control contacts and wandering population, in that order. Unaffected actors'
elapsed fractions need not all be materialized at every boundary.

For one actor, `actor_outcome_at_clock` computes elapsed active time. The old
hallucination generation, relocation/awareness clocks and earned search work
advance before changing the conditions that supplied them. Engagement expiry can
split an interval so recovery uses the correct old/new context. Holds advance
and `active_since` updates afterwards. Input-denied actors normalize elapsed
readiness, not future recovery debt or their paused plan's remaining work.

`CreatureState::advance_active_with_endpoint_context` segments living time at
condition expiry and nutrition-band boundaries. Regeneration can cause a nutrition
crossing; a monotone integer search finds its earliest tick instead of iterating
through every tick or point. Each segment:

1. Determines resource/Health gains under the preceding rates.
2. Advances resource regeneration, Health recovery, cooldowns and received-loss
   activity time.
3. Charges nutrition for elapsed time and actual regeneration, preserving integer
   remainders and recording empty-time depletion.
4. Advances/expires timed conditions and rebuilds changed condition-derived sources.
5. Applies endpoint recovery context and rephases earned Health recovery under the
   final combined condition/nutrition context.
6. Applies starvation depletion if required; dead actors accrue no more living time.

Expiry does not retroactively change the rate of earned work. Coincident hunger
and expiry changes share the endpoint rephase. Until-removed conditions have no
invented expiry deadline. Dead actors do not regenerate. Detailed arithmetic and
balance policy remain in [creature rules](../reference/creature-rules.md).

Configured empty-nutrition triggers are **authoritative**, after time settlement.
Eligibility requires a living actor whose conditions do not restrict
`VoluntaryActions`; this is not a control-lease test. The draw, earned automatic
work, ordinary condition application, possible paid-work pause and events share
a candidate transaction. Pure observation uses deterministic elapsed-time
projection but does not perform or commit this random trigger.

Death is ordinary actor-local publication, not a renderer cleanup or separate
end-of-frame sweep. It cancels preparation, clears equipment/claims as applicable,
retires input-boundary membership, drops carried items at the death placement
with inherited motion/frame, creates the corpse, invokes run policy and stages
any valid attributed reward. Later work sees that candidate state. A later checked
failure or failed persistence admission can still roll back the outer action.

Frozen actors use their freeze stamp as their clock, leave active timer scheduling
and consume none of the intervening world time. Thaw shifts absolute readiness
and preparation clocks by the frozen interval using checked arithmetic; global
time elsewhere does not make their work instantly complete.

Source: [actor_store.rs](../../crates/simulation/src/actor_store.rs), timer index;
[creature_time.rs](../../crates/simulation/src/creature_time.rs),
`settle_creature_time`, `settle_actor_time`;
[lifecycle.rs](../../crates/simulation/src/lifecycle.rs), `actor_outcome_at_clock`,
`publish_actor_outcome`, `death_outcome`;
[creatures/state.rs](../../crates/simulation/src/creatures/state.rs),
`advance_active_with_endpoint_context`;
[streaming.rs](../../crates/simulation/src/streaming.rs), `actor_clock`, `thaw_actor`.

## Preparation resolution at a shared deadline

`resolve_preparations` snapshots actor IDs once and processes them in ID order,
then processes one owner's equal-offset strike rows in authored order. Each
owner's loop is bounded. Terminal policy stops it; frozen actors are skipped.
Live validation also checks paused/not-yet-due work; only due active work performs
the extra owner/target settlement.

For a due row, settle owner and target, then detect whether settlement changed
the preparation. Recheck dependencies where required, including residency
handoff. Unavailable work or physical displacement discards the plan; newly
visible threats can suspend timed item work. Paused work does not complete.
Valid completion settles its resolution hold once, handles item payment and
publishes the next cursor/gap or recovery before invoking the ordinary effect
kernel. Later equal-offset rows revalidate against earlier consequences. Offsets
and recovery anchor to **scheduled due time**, not delayed mailbox/picture time.

Escape, respec and projectile release have explicit completion paths, not an
attack-only tail. Frozen attacker operands, live target defense, primary/rider
checks, cancellation and payment remain in
[actions and atomicity](simulation.md#actions-and-atomicity) and creature rules.

Source: [sequence_execution.rs](../../crates/simulation/src/sequence_execution.rs),
`resolve_preparations`; [lifecycle.rs](../../crates/simulation/src/lifecycle.rs).

## Failure and rollback matrix

| Failure point | Published simulation/journal effect | Client consequence |
| --- | --- | --- |
| Decode, authority, context, disclosure or admission validation rejects | No new action effects or accepted intention. | Typed host error where available; malformed/oversized wire can end transport without a correlated reply. |
| Ordinary queued work becomes unavailable | No substitute target/action. Queue retirement and failed-intention journal commit if save admission succeeds; failed continuation can discard retained preparation. | Terminal intention status, not another admission or an invisible retry loop. |
| Typed simulation-capacity/growth execution refusal | Outer candidate is discarded, including queue removal and RNG/effects. | Queue remains retryable; controller receives execution error, not false storage warning. |
| Region settlement, projection validation or storage admission fails before publication | Candidate, knowledge, history append and staged input generations do not publish. | Error/waiting or save warning according to the failing path. |
| Background save fails after acceptance | Accepted gameplay remains; pending save work is retained and save gate blocks later mutations. | Warning and explicit save/retry; acceptance was not durability. |
| Causal projection/serialization/output admission fails after commit | No gameplay rollback. | Affected stream disconnects; healthy peers continue; reconnect/resynchronize. |

“All rejection leaves everything unchanged” does not describe a committed
`IntentionFailed`. Failed execution and rejected publication are different
operations. Reconnect or a missing receipt is not proof the command was not
accepted. An authorized exact retry can recover its original receipt when that
accepted record survives. A crash can lose accepted but unsaved records; absence
after reopening does not prove the command was never accepted.

Source: [server intention.rs](../../crates/server/src/intention.rs),
`execute_intention_profiled`; [engine.rs](../../crates/server/src/engine.rs),
`Failure::simulation_failure`, `commit_candidate_with_resolution`;
[session.rs](../../crates/server/src/session.rs), `report_execution_failure`;
[storage.rs](../../crates/server/src/storage.rs).

## Worked sequences

### Two ready actors, one admitted move

Assume living actors 1 and 2 both ready at 0, no special input boundary, active
preparation or physical/timer interruption, and actor 1's ordinary cardinal move
costs 100 ticks. These are illustrative fixture values, not universal balance.

1. Actor 1's admission commits at tick 0. Receipt/queue exist; position/readiness
   are unchanged.
2. Execution moves actor 1 at tick 0 and sets readiness to 100. Actor 2 remains
   due at 0, so the deadline walk need not advance.
3. Outcome can have `at_tick = 0`, `next_tick = 0`, next actor 2. Completion does
   not require a newer picture revision.
4. If actor 2 is human without queued work/journey, service waits. More submissions
   by actor 1 cannot skip actor 2. If actor 2 acts and becomes ready at 100, the
   ordinary tie returns to actor 1 by ID.
5. Outcome/status/readiness delivery remains independent of picture demand/rendering.

### Condition expiry and a due strike share tick 10

An existing fixture gives an attack 10-tick preparation and 1-tick recovery.
Its target survives only with temporary Health that expires at tick 10 while
physics is moving.

1. The deadline walk reaches the due creature timer at 10.
2. The pre-physics timer pass expires Health and publishes ordinary death before
   the due strike can use an old living-target snapshot.
3. Preparation validation interrupts the attack; no successful strike resolves
   against the just-expired state.
4. Expiry/death/interruption remain candidate events until save admission allows
   publication of each observer's permitted facts.

This is an inspected source witness, not a new test run:
`target_health_expiry_interrupts_due_attack_during_moving_physics` in
[creature_time.rs](../../crates/simulation/src/creature_time.rs). The same owner
contains `health_condition_expiry_at_arena_deadline_has_elimination_precedence`:
a run deadline does not move expiry behind timeout processing.

### A committed move whose client disconnects

1. The candidate passes save admission and publishes.
2. Service accounts journey progress, then cannot admit its causal output. It
   disconnects the affected stream without undoing the move.
3. Reconnect/attachment creates fresh stream/context/state. An authorized exact
   retry with the original accepted command identity returns its surviving
   receipt even though the original input context is stale.
4. The retry does not move again. A new ID is fresh work, not a substitute for
   resolving uncertainty. If a process crash also lost the unsaved record, the
   retry cannot resurrect it merely from the old acknowledgement.

See [wire lifecycle](../reference/protocol-lifecycle.md) for actual families and
recovery authority.

## Source and validation map

Ranges are navigation evidence at the reviewed commit. They are not claims that
these tests ran during the prose review. Future contract changes need their
owning behavioral tests as well as documentation updates. Paths below are under
`crates/`.

| Contract | Source ranges | Existing witness/owner |
| --- | --- | --- |
| Owner loop / session gates | `server/src/runner.rs:139–218`; `server/src/session.rs:1658–1785` | Runner/session and backpressure contracts |
| Role, retry, context | `server/src/session.rs:538–591,718–765` | Session request/permission contracts |
| Admission and actor ordering | `simulation/src/intention.rs:277–332,473–562`; `simulation/src/lib.rs:1268–1302` | `admission_has_no_gameplay_effect_until_simulation_executes`; `queued_actions_follow_actor_scheduler_order_not_submission_order` |
| Action/deadline walk | `simulation/src/actions.rs:314–344,984–1048` | Action/scheduling contracts |
| Timer/physics/work order | `simulation/src/physics.rs:950–1186`; `simulation/src/creature_time.rs:63–168` | Moving-expiry/arena-deadline witnesses above |
| Regeneration/nutrition endpoints | `simulation/src/creatures/state.rs:1888–2075`; `simulation/src/lifecycle.rs:302–394` | `coincident_condition_and_engagement_expiry_rephase_earned_work_once` in `behavior_tests.rs`; survival/recovery suites |
| Preparation ties/payment | `simulation/src/sequence_execution.rs:7–459` | Ability/condition/supply/continuation contracts |
| Region continuation | `simulation/src/floor_activation.rs:1171–1216`; `server/src/engine.rs:1980–2046,3774–3977` | Activation/region/host-continuation contracts |
| Save admission/publication | `server/src/engine.rs:4276–4404`; `server/src/storage.rs:1176–1255,1510–1535` | `failed_ai_journal_admission_does_not_publish_candidate_effects` in server performance contracts; storage contracts |
| Outcomes/lifecycle/readiness | `server/src/session.rs:1272–1299,1337–1463,1658–1661,1758–1767` | Outcome/privacy/session contracts |

This execution description is source-backed, not a new benchmark or a promise
of fixed action/update frequency.
