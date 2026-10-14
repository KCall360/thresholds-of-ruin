# Simulation, actions and scheduling

`tor-simulation` owns authoritative in-memory rules. It performs no I/O,
rendering, wall-clock reads or implicit random sampling. This page explains action
atomicity, saved preparations, scheduling and review principles. Use
[creature rules](../reference/creature-rules.md) for mechanics,
[protocol](../reference/protocol.md) for client envelopes and
[architecture](overview.md) for crate ownership.

## Actors and deterministic time

Every actor has an explicit stable identity and positive base action duration.
There is no global player or player-only inventory. Backend setup creates actors;
ordinary player commands cannot call setup methods. Authorized
[wizard operations](../development/wizard.md) use separate validated, journaled authority.

The scheduler selects the earliest readiness time and breaks actor ties by ID.
The direct action API rejects an actor that is not due. Several actors can act at
the same tick. Integer time advances because of simulated actions and deadlines,
never frames, mailbox delays or save timers. Positive durations prevent zero-cost
action loops; time and identity exhaustion reject instead of wrapping.

For immediate actions, effects occur at execution and recovery follows:

| Action | Recovery | Preconditions |
| --- | --- | --- |
| Cardinal/vertical movement | Actor base duration | Valid geometry and body fit; no blocking actor |
| Diagonal movement | Integer `ceil(base × √2)` | Destination clear and at least one side clear |
| Pickup/drop | Half base duration, rounded up | Reachable ground item or owned carried stack; valid quantity |
| Open/close door | Actor base duration | Visible reachable door, changed state, no closing obstruction |
| Wait | Actor base duration | Actor due to act |

A lone actor with base 100 taking an item at tick 0 is ready at 50. If another
actor is ready at 0, that actor acts next at 0 instead. Movement and door geometry
are specified in [movement](../reference/movement.md). Timed abilities, equipment and
consumables use preparation and resolution, not the immediate-action timing row.
Observation reads, unresolved noun ambiguity and rejected actions consume no
simulation time. Rejected actions leave authoritative state unchanged; a future
in-world failed attempt needs an explicit rule if it is to charge time.

## Actions and atomicity

Human, autonomous and travel requests admit bounded intentions to a simulation-owned
queue. Admission stores the requested action, not a frozen timed-preparation
plan. It is free of simulated effects and does not imply completion. The scheduler
executes due intentions against current state. Server candidates
must pass persistence admission before acceptance or effects become public;
see [publication boundaries](overview.md#authority-intentions-and-publication).

The low-level `Game::act` and backend journal `Command::Act` do not manufacture
intention ownership. Resource-paid abilities and finite-item work, including
Drink, reject atomically through these direct APIs. Callers admit an intention
and execute it instead. Public wire `Command::Act` is already translated to
backend `AdmitIntention`, so ordinary clients and AI keep the same owned execution
path; no alternate item-claim allocator exists.

Inside an uninterrupted action execution, the private `PreparedAction` checks
readiness, preconditions and timing overflow without mutation. It is consumed in
the same call; no callback, yield or external mutation can intervene, and no
caller can save it as a promise about future state.

Commit clones a private candidate, applies the policy hook and effects, synchronizes
actor lifecycle and advances scheduling. Those operations can fail. Only after
all succeed does it replace the authoritative game. The guarantee is atomic
publication/rollback, not an infallible effect tail. `ActionOutcome` is backend
fact, not a wire message: the server filters it for each observer.
See [actions.rs](../../crates/simulation/src/actions.rs).

Saved timed preparations are a different kind of state. `Plan` distinguishes
abilities from equip, unequip and drink work. An ability owns one immutable,
value-serialized shared `AbilityPlan`: up to 16 ordered strikes, each with up to
8 nonrecursive riders. The mutable `PreparationProgress` cursor is outside it.
Execution captures selected sources, check operands, independent shared edge,
costs, scaled timing, reach, effects and token sites once. Cursor advancement,
suspension, revalidation and replay never rebuild these operands.

`sequence_execution.rs` schedules each due strike, breaking ties by actor ID and
then authored ordinal. Equal offsets use a bounded same-tick loop. Each scheduler
invocation currently enumerates the loaded actor table once, including idle actors;
this pre-existing scan is not an actor-density-independent index. Riders never
restart that actor scan. The first strike settles the resolution hold once; a saved prefix has no unpaid hold.
Recovery starts after the last strike. `abilities/execution.rs` resolves primary
then eligible riders through the same observer-generic check/effect kernel, using
fresh target snapshots and one remaining damage-edge budget. Each publication
uses the existing actor-local lifecycle boundary, without a Game clone per rider.
The primary reports its roller-relative success/failure branch, or no branch for
whole-payload immunity. Frozen rider triggers explicitly select one branch; they
are not inferred from the roller's identity or applied damage. A checkless primary
cannot have a failure trigger. Structural shared-check ownership and token claims
remain reserved even when their rider's branch is not selected.
A transient two-participant damage-activity guard spans the primary and riders:
each affected actor resets Health quiet at its first positive applied damage only.
The first-damage policy wins, self-damage deduplicates, and later strikes reset anew.
Per-strike `Resolved` events carry actual ordinals; only `SequenceEnded` is terminal
ability evidence. Item completion remains separate.

Life and reach are checked at each strike. Before any strike has resolved, lost
validity retains the existing early-discard behavior. After a prefix, an unavailable
target skips that strike without RNG or mutation, retaining the timeline. Positive
HP loss suspends unfinished work, and waiting preserves it. An unrelated action
cannot replace a committed prefix. Resume preserves all prior strikes and reanchors
only the remaining active work. Suspended attack work does not stop an actor's
Health clock when the world continues advancing actor-active time; input/wall-clock
waiting alone advances neither clock.

One checked `Preparation::abandon_delay` charges the unfinished timeline and
recovery after a resolved prefix, including when suspended. In-place cancellation
is still available. Atomic cancellation with one step uses ordinary movement
admission, collision, effects and observations, and adds its normal cost to that
debt. Rejection leaves queue identity, plan, cursor, tokens, RNG and state unchanged.
The operation is shared by human and AI actors. Item preparation has no explicit
cancellation action. Recovery is not resumable work.

Next-check tokens are claimed at preparation, never queue admission. Each claim
binds only the first qualifying site in strike zero; if none exists it stays
unclaimed for a future preparation, never transfers to strike one. A skipped,
missed or cancelled bound site spends its token. Suspension and freeze preserve
the same binding. See [creature rules](../reference/creature-rules.md#abilities-and-effects)
for the source-fold and edge-allocation contracts.

Effect publication is actor-local. Each primary success/failure payload and each
rider payload has one application recipient: ordinary damage, healing and
conditions use Target; public-plan observation and its qualified Readied use
Source. Mixed-recipient payloads reject explicitly, even when the two actor IDs
alias. Separate riders can alternate recipients using fresh actor-local state,
and equal roles mutate the same actor once. This is not arbitrary joint-recipient
effect support. Selected-target reach, checks and whole-batch descriptor
protection remain separate from the actual recipient's condition immunity.

Ordinary personal-sensation disclosure uses bounded attempted-role evidence,
not merely the selected target ID. A required frozen check fact preserves
whether the target owns the roll or contributes a defense/opposed roll; a user's
personal difficulty does not involve an incidental selected subject. The outcome
retains that check role and only the actually selected payload's recipient,
including when immunity or a zero result removes effects. Early checked immunity
leaves the payload explicitly unselected. An unavailable target retains the
existing uniform Miss without inventing an attempted check or payload. Neither
these internal reasons nor frozen participation fields enter ordinary wire views.
The source keeps its own event, and genuinely visible participants remain visible.
Only actually published riders can add target participation to their own primary's
single public event. Private publication receipts bind the exact primary, strike
ordinal and actor/target/intention tuple. This changes no primary check, result,
effects or event ordering; untriggered and skipped riders contribute nothing.
An early-blocked primary creates no rider receipt. Executed immune attempts can
still carry a genuine target check or selected payload.

Named current plan/outcome readers reject missing, duplicate, unknown and
positional fields. Pending-plan validation checks the frozen check fact against its source;
publication and restoration reject internally contradictory outcome evidence.
An immune outcome alone cannot authenticate discarded original operands.
Ordinary checkpoint restoration does not replay every gameplay resolution; it
checks required bounded facts and all available structural/semantic consistency.
Coherent user-edited alternative histories, such as changing an erased payload's
Source role to Target, remain outside the existing save-authenticity guarantee.
No duplicate plans, full history reconstruction or signatures are introduced.
Saved status is canonical: Applied means at least one retained effect changed;
an empty selected branch with a failed User roll is Miss; other unchanged selected
payloads are Unaffected. Trusted direct effect publication retains normalization
of non-Miss status hints before storing the actual applied result.

Bounded execution facts enter the same pure resolver through `ResolutionContext`.
Source application previews and lifecycle publication use the same effect
kernel, without cloning Game per effect. An immutable captured public aim does
not follow a moving target. A retained paid observation is shared across actor,
application and outcome snapshots; replacement leaves old snapshots intact.
Historical disclosed aim addresses must be known in loaded or detached region
metadata, with no loading or pinning and no dependence on current walls, light
or occupancy. Actor records and retained resolved-event copies share this check;
historical observation time cannot exceed its restored checkpoint boundary.
Recognition is assimilated only from the final committed observer views, after
region transition and before persistence admission. Opaque backend event IDs are
only a work prefilter, never disclosure authority. Cached queries remain pure,
and failed admission discards the candidate's knowledge with its other changes.

Finite supplies use one item-owned hold store. Preparations name exact item and
cost-ordinal references under their original issued intention, never duplicate
held quantities. Ordinary admission changes no item or cooldown. Successful new
preparation atomically reserves supplies, pays start resources, binds tokens and
starts the frozen actor-active cooldown. Resume reuses the original lineage and
never restarts the countdown, even after its natural expiry. Cancellation or grant
changes cannot erase a retained cooldown.

First resolution validates the entire claim set, spends exact physical units,
marks payment settled, then publishes the preparation cursor and ordinary effect
outcome. This includes Drink: identity is captured before a last-unit source is
deleted, and frozen effects do not reread that deleted item. Later strikes do not
require historical supply IDs to remain live. Retirement distinguishes explicit
spend from release by lineage and payment state. Forced source movement retires
the whole unpaid lineage, while ordinary transfer can move only free units.

AI candidate construction sees the same known physical supplies as its observer
view. Execution-only allocation preflight can select a normal Wait on finite-item
contention without exposing holds through previews. Recorded-action validation
repeats that deterministic choice; fallback has ordinary timing and no separate
scheduler, private availability hint or failed-command resource charge.

Control loss/restart preserve valid work while requiring fresh human input;
backend suspension commands are not wire commands. Restored travel steps instead
settle terminally before control acquisition: a saved step must not restart a
session-local journey. Exact retry and lifecycle proof semantics are in the
[protocol](../reference/protocol.md) and [persistence](persistence.md) references.

## Running until input is needed

A dedicated simulation thread owns the service on its own current-thread Tokio
runtime. Network tasks cannot be starved by a long run, and no session mutex
spreads mutation across tasks. Connects, requests, disconnects and shutdown use one
mailbox. `SimulationHandle::with` runs test/tool closures between actions.

The loop handles a bounded FIFO mailbox pass, executes at most one due action,
and repeats. Each pass is capped at mailbox capacity (256 server messages), so
replacement arrivals cannot extend it indefinitely. A waking request is handled
before the next pass. A closed empty mailbox ends the run before another action,
even if its last message filled the pass. A request waits behind at most one
action, not a whole autonomous run; no action is split by an interruption.

Current step ordering is important:

1. Apply pending preparation pauses and travel cancellations.
2. Respect attached-client output headroom and arena execution policy.
3. Execute a due queued intention, if one exists, with origin-specific authority.
4. Otherwise find the next actor. Advance its journey if one is running.
5. Otherwise admit and execute an eligible AI decision, or announce waiting.

Human queued work requires its controller while alive. AI work requires autonomy
and a driver. Adventures require a living controlled actor; bounded arenas can
run unattended and continue after the selected actor dies. Arena pause/step/stop
policy still applies. A travel intention must match its accepted journey and pass
current travel readiness. Thus “next actor is human” alone is not the stopping
rule: that actor may already have a due intention or journey.

Autonomy is enabled by an action or `continue`, and suspended at release,
disconnect or wizard boundaries as specified by session control. Waiting is
announced once per input stop. A long adventure run has no arbitrary total action
cap, but mailbox fairness remains; arena runs have explicit policy bounds.
See [session execution](../../crates/server/src/session.rs) and
[arena operation](../development/arena.md).

## Travel, requests and backpressure

A journey takes ordinary steps whenever its actor is due and waits for other
actors between them. Before every step, session checks control, HP loss and newly
seen actors; changes while waiting can end the journey. Arrival, a blocked move,
displacement or damage can stop it after execution. Only the server ends travel;
client playback skip does not cancel it. See [travel](../reference/movement.md).

Action or travel requests during an active journey reject with `actor_busy`.
Queries and other eligible metadata commands run at the next boundary. Wizard
changes stop journeys with `world_changed`; release/disconnect stop them with
`control_lost`. `save` is a durability barrier and does not stop travel.
Snapshot, history, branch-history and palette queries run on the simulation
thread; off-thread reads remain a measurement-driven future option.

Play pauses when an attached client's bounded output lacks required headroom,
but the mailbox continues to be serviced. Sustained stalls disconnect that stream,
which must reconnect and resynchronize rather than miss updates. A slow spectator
can delay but cannot indefinitely stop a journey. Queue slots, encoded-byte
budgets, in-flight leases, aggregate admission, socket resets and deadlines are
specified in [protocol](../reference/protocol.md#output-ownership-and-backpressure);
they measure payload retention, not total
resident memory, and do not promise fair allocation of the shared pool.

Save completion and behind-schedule warnings are checked every 50 ms between
actions or by a timer while blocked. That timer never advances the game. If idle,
the thread waits for messages, output capacity or save work.

Client pacing delays the presentation of observation updates, not authoritative
execution. The first update after a pause appears immediately; other message kinds
apply on arrival. The connection holds a cancel-safe pending update until due,
so reading slowly backpressures the server. Skip displays received progress
without waiting until the next request. Defaults and player controls belong in
[text](../play/text.md) and [ASCII](../play/ascii.md) guides; suitable playback
delays remain a playtesting decision.

## Perception, ownership and fixtures

Gameplay observations use illuminated eye-based three-dimensional sight with a
16-cell range, merging separate local awareness and abstract-link landing
projections. This is not horizontal shadowcasting with sight through stair links.
The [perception reference](perception.md) owns ray geometry and disclosure details.
Clients get relative positions and opaque keys, never region coordinates or links.
Rotated crossings preserve observer axes. Seeing an item does not imply reach;
hidden, unknown, carried and out-of-reach pickup targets share the unavailable
boundary. Inventory is ownership, so one item cannot belong to two inventories.

`Game::two_room_in_stone(seed)` remains a diagnostic fixture: two 5×3×2 interiors
inside stone shells, a 1×1 hall with two empty height layers, an initially open
wooden door, and center hints at local (2,1,0). Region 1's extra east column stores
the hall with stone north/south; each region includes a one-cell shell. The actor
starts at region 1 (1,1,0), with a seeded copper/silver/iron token, and a tablet at
region 2 (2,1,0), seven eastward steps away. Token selection does not exercise
procedural-generation or combat RNG. Simpler `two_room` and
`two_room_with_place_hints` constructors are also fixtures. Ordinary games start
from [scenario packages](../reference/scenario-packages.md).

World validation covers unique regions, bounded endpoints, legal boundary exits
and unique source-cell/direction connections. Portal loops need no global spatial
embedding. Visited-region knowledge changes on entry, not on observation queries;
looking through a portal does not mark entry. This is distinct from seeing a
place hint and learning its name.

## Rules design and review

These principles apply at the creature/rules boundary, not as blanket prohibitions
on every tuple or wildcard match anywhere in the repository.

1. **Content is data; code implements general mechanisms.** Abilities, talents,
   conditions, slots and tuning belong in validated rules. Adding content using
   existing mechanisms should not change Rust. New mechanics must be reusable.
2. **One implementation per concept.** Damage, protection, attacks and effects
   have one production path. Delete replaced models and tests with the migration;
   do not retain compatibility views or test-only alternate resolvers.
3. **One ordered derivation fold.** Species, type, subtype, class, template,
   talent, equipment and condition sources feed the same cached result. Inspection
   explains recorded sources, not an independent re-derivation. Invalidate on source
   changes; never persist derived caches as authority.
4. **Validation produces owned typed evidence.** Carry validated rules, builds and
   plans across boundaries. Recheck live conditions, not frozen operands. Avoid
   cross-module `expect("validated …")` assumptions and repeated expensive queries;
   AI uses one perception snapshot per decision and shares route work.
5. **Pure resolution, explicit application.** A plan, snapshots and explicit RNG
   produce an outcome through one observer-generic resolver. The creature lifecycle
   boundary applies injury, pools, conditions, death and events. Preview and commit
   share the effect-state kernel; diagnostics must not change outcomes or RNG.
6. **Types encode meaningful alternatives.** Use tagged variants for action and
   run states, named records for domain facts and typed attribute/resource maps.
   `ModifierOp` is a sum type: Add/Set carry a stat, while immunity/reduction carry
   a selector, preventing nonsensical Health-plus-immunity pairs. Rules variants
   are matched exhaustively. See [rules.rs](../../crates/simulation/src/rules.rs).
7. **Reachable failure has a specific meaning.** Rule, build, scheduling and
   persistence errors are typed. Capacity exhaustion follows declared rejection,
   refresh or eviction policy, never a panic. Condition expiry cannot leave invalid
   stale derived state; the validated permanent baseline bounds reconciliation.
8. **Content IDs are stable and declared once.** String IDs identify catalogs;
   display names are separate. Rust enums represent mechanisms, not a mirrored
   enum of each talent in the protocol and clients. Reordering declarations cannot
   change a saved choice.
9. **Persist shared definitions once, actor choices and mutable state separately.**
   Pin resolved content and dependency hashes at game level. Actor records hold IDs,
   owned advancement, injury, pools, holds, conditions and identity. Strict restore
   validates through runtime constructors and rebuilds derived caches. The
   current-format-only policy does not permit a hidden historical reader.
10. **Tests follow production architecture.** Test pure kernels with tables and
    properties; exercise prepare/interrupt/resume/death/restart/rewind lifecycles;
    use actual-process acceptance for user-visible behavior. Fixtures use shared
    builders and rules data. No obsolete production field survives solely to keep
    an old test compiling.
11. **Documentation describes current contracts.** Replace superseded behavior;
    put status in [project status](../status.md), future choices in plans and
    measurements in the [performance guide](../development/performance.md).

Before reviewing a new feature, ask whether it adds data or a mechanism, whether
another path now duplicates it, whether each number has source provenance,
whether typed validation survives to its consumer, whether publication is atomic,
and whether save/disclosure boundaries and every affected test layer stay intact.

## Verification and measured limits

Action tests exercise immediate effect/recovery for both actor orders, diagonal
movement, pickup, doors and overflow with unchanged state on failure. Queue and
server tests cover human/AI/travel admission, original-identity retry, preparation
suspension and recovery, arena bounds, output pressure and mailbox fairness.
Actual processes exercise control transfer, clients, save/restart and slow
spectators. The pressure test exceeds the combined host-buffer, queue and in-flight
budget, separates server socket release from draining unread bytes, and retains
an EOF delivery test so compact deltas cannot weaken its purpose.

[Simulation integration tests](../../crates/simulation/tests/it),
[run process tests](../../scripts/test_run_until_blocked_process.py) and
[the verification policy](../development/testing.md) define the maintained checks.
`server_handled.lock_ms` measures mailbox waiting, not a session mutex hold.
An acknowledgement benchmark is not completed queued-action latency; neither is
native presentation. Historical direct-action measurements do not close long-run,
dense-physics or persistence tails. See [performance](../development/performance.md).
