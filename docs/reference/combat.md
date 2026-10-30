# Combat execution reference

This page connects the current attack algorithms to their scheduling, resource,
geometry, lifecycle and disclosure boundaries. It describes implemented behavior,
not a proposed balance model or a conversion from a d20 game. The numeric examples
below are calculations or named test fixtures, not measurements of class balance.

The [simulation architecture](../architecture/simulation.md) owns the whole
simulation cycle and transaction boundary. [Creature rules](creature-rules.md)
owns exact check-edge and authored ability contracts;
[equipment](equipment.md) owns equipment eligibility and physical item definitions;
[progression](progression.md) owns build funding and restoration. This reference
owns the ordering that connects them. [Status](../status.md) owns current formats.

## State and authority

| Record | Authority and lifetime |
| --- | --- |
| Submitted intention | An authorized request and its admitted identities. Queue admission is not a frozen attack plan or payment. Human automatic weapon/supply choices retain private admission-owned identities; they cannot silently substitute different items later. |
| Immutable plan | Constructed at preparation start against the pinned rules. Captures selected source, ordered strikes, checks, effects, costs, offsets and timings. Rays/projectiles capture an origin and direction, not a future victim. |
| Preparation | Owns active/suspended work, remaining time, cost/item/token reservations and the immutable plan. |
| Sequence progress | Separate bounded mutable cursor, qualifying-hit history and once-only rider receipts. Resume advances these; it does not rebuild the plan. |
| Resolution snapshot | Settled actor-local candidate state at the due boundary, including live life, protection and effect context. It is not a new global Game clone for every rider. |
| Published outcome | Actual committed effects, participant/event provenance and permitted observations. Privileged diagnostics and private save records are not ordinary client observations. |

The scheduler resolves deterministic actor-ID order and authored strike order,
including equal-offset strikes. RNG is explicitly owned by simulation state.
Observations and diagnostics read the result; they do not perform another roll.
See [sequence execution](../../crates/simulation/src/sequence_execution.rs),
[sequence state](../../crates/simulation/src/sequence.rs), and
[publication lifecycle](../../crates/simulation/src/lifecycle.rs).

## Attack lifecycle

1. **Admit the request.** Validate input authority and the disclosed operand
   identity. An actor-target request must first have a usable target reference;
   hidden existence, Health or capabilities are not an error-message oracle.
   Admission does not reserve an item or freeze all combat operands.
2. **Start preparation.** When the actor can begin work, validate life, the
   effective ability grant, invocation shape, cooldown, selected weapon/supply,
   target authorization and reach. Compile the current eligible strike list.
   Natural and selected-equipped sources remain distinguishable.
3. **Commit reservations and initial costs atomically.** Require enough available
   resources for the whole bundle; spend the start portion, hold the resolution
   portion, reserve exact items/tokens and start the authored cooldown. Failure
   before this transaction commits does not leave partial reservations.
4. **Advance active work.** The actor owns a due time and may be suspended or
   discarded according to lifecycle rules. Frozen actors accrue no active work.
   Missing required geometry is a dependency to prepare, not an invented clear
   path or fabricated miss.
5. **Revalidate at boundaries.** Validate the preparation's effective grant and
   bound source. Forced displacement can discard it. Newly visible threats can
   suspend item work. At a due boundary settle actor time and, where applicable,
   target time, then recheck dependencies before payment or census.
6. **Settle release once.** The first due strike spends the resource remainder
   and ordinary held item payment. It installs the next cursor/due work, or final
   recovery, before applying effects. Later strikes and resume do not pay again.
   Projectiles have their exact-unit release path; special weak Drain Life
   refusal releases its held charge rather than spending it.
7. **Resolve delivery.** Actor-target strikes use live reach/life with captured
   operands. Projectile and ray traversal discover recipients in current physical
   geometry. Delivery can fail without rolling an absent damage branch.
8. **Resolve the primary payload.** Whole-payload immunity precedes the primary
   check; the check selects a branch; applicable exposure resistance precedes its
   effects. Apply the selected effects in authored order through the common
   damage/condition/lifecycle kernel.
9. **Resolve eligible riders in order.** They observe live results of earlier
   effects but retain frozen checks/payloads. A lethal primary or earlier rider
   prevents later mutations. The primary and riders share one residual damage-edge
   budget and one per-recipient positive-damage activity guard.
10. **Publish and continue.** The same outer transaction reconciles preparations,
    cleanup, death attribution, XP and combat events. Later primaries run at their
    captured offsets, or the actor waits through recovery. Public projection
    discloses only what each observer is entitled to know.

Implementation: [preparation compilation](../../crates/simulation/src/abilities/compile.rs),
[item work](../../crates/simulation/src/interactions.rs),
[cost ledger](../../crates/simulation/src/costs.rs),
[release scheduler](../../crates/simulation/src/sequence_execution.rs),
[strike execution](../../crates/simulation/src/abilities/execution.rs).

## Frozen operands versus live facts

“Frozen plan” does not mean the future world is frozen. These distinctions matter
when a condition expires, a creature moves, equipment changes, or a ray meets an
unexpected recipient.

| Operand | Captured or live? |
| --- | --- |
| Actor-target attack source, skill/attribute/flat bonus, defense threshold or both opposed-check operands | Captured at preparation; ordinary targeted attacks do not substitute a freshly derived defense at release. |
| Eligible authored strike rows, original ordinals, rider triggers/checks/effects, reach policy, cost, duration expressions and scaled offsets | Captured. A newly eligible unlock does not add a row to an existing sequence. |
| Exact selected weapon identity/socket and attack payload | Captured, with live checks that the bound item remains legally available. A replacement weapon is not silently selected. |
| Source one-use token bindings and source outgoing modifiers | Captured/reserved under their role policy. Expiry or changing conditions do not cause a new source fold on resume. |
| Actor/target life, active/frozen participation, effective whole ability availability | Live lifecycle checks. Loss of the whole grant can cancel unresolved work. |
| Melee contact/reach and collision geometry | Live. A previous legal target can become unavailable and can return before a later strike. |
| Recipient protection, current Health, condition slots, exposure resistance and effect state | Live at application. A frozen damage expression does not freeze old armor or current Health. |
| Projectile source check/payload, threatened-at-launch state, direction, origin and ranges | Captured. Actual recipient and its Physical defense/incoming check modifiers are live at collision. |
| Ray source skill/edge/effect operands, direction, origin and launch footprint | Captured. Recipient named skill, protection, eligibility and incoming tokens are live at each encounter. |
| Multiattack cursor and qualifying hit/attempt receipts | Mutable preparation state, retained across pause, freeze and restore. Not recomputed from visible events. |

See [targeted check capture](../../crates/simulation/src/abilities/compile.rs#L134-L221),
[weapon binding](../../crates/simulation/src/abilities/weapon_selection.rs),
[projectile planning](../../crates/simulation/src/abilities/projectile.rs),
[ray planning](../../crates/simulation/src/abilities/ray.rs) and
[live projectile checks](../../crates/simulation/src/abilities/projectile_execution.rs).

## Targeting, geometry and equipment choice

Actor-target checks are not permission to address an undisclosed actor. Admission
validates the usable reference before querying target-specific facts. Self use,
contact-target use and directional use have distinct invocation shapes; a client
cannot turn an ordinary directional unknown device into a known self-use action.

Melee reach is based on resolved **physical body cells**, not glyph adjacency or a
single reference-point distance. Contact searches the 26 one-cell offsets around
occupied cells, grouped by squared distances 1, 2 and 3. Those are adjacent axial,
edge and corner shells, not a three-cell weapon reach. Physical portal orientation
and available geometry matter. A contact can establish reach while leaving an
ambiguous push-away direction; in that case a push cannot invent a direction.
Each primary selects a live contact witness; its ordered riders reuse that
witness instead of selecting an unrelated route after each effect.

The default equipped attack uses the anatomy's designated **primary weapon
socket**, not the numerically greatest occupied weapon slot. Explicit secondary
selection retains the exact item/socket. Its Focus resolution surcharge is one
with Offhand eligibility and two otherwise. Natural attacks remain a separate
provenance source. Expertise changes the effective profile/traits through the
builder; it is not a hidden flat damage bonus added by the hit kernel.

A frozen equipped source must still be carried and equipped in the captured
attack-capable socket. It cannot borrow a newly available weapon or rederive the
captured profile from new expertise. General equipment anatomy/capacity and curse
rules are in [equipment](equipment.md).

Sources: [physical contact](../../crates/simulation/src/combat/contact.rs),
[selected source derivation](../../crates/simulation/src/creatures/derive.rs#L1142-L1150),
[selection and validation](../../crates/simulation/src/abilities/weapon_selection.rs).

## Checks, defenses and edge

The check kernel is:

```text
retained 3d6 + governing attribute + skill rank + flat modifiers >= threshold
```

A tied total succeeds. An opposed check uses the opposing total as the threshold;
there is no tie reroll. Natural sums 3 and 18 have no automatic failure/success.
There is no implicit natural-18 critical hit, doubling rule or universal critical
multiplier. Extra effects must come from authored damage, effects or riders.

With bundled base defense 10, the unmodified pairs are:

| Defense | Attribute pair |
| --- | --- |
| Physical | Strength + Speed |
| Cognitive | Intellect + Willpower |
| Spiritual | Awareness + Presence |

Authored modifiers can change these derived defenses. A target-rolled save is
not an attack roll with its success inverted by the kernel: its own success branch
is selected. Thus a successful Agility check against a ray's launch total can
select an empty branch and avoid damage, while a user attack's success selects
its damage branch.

Positive and negative edge cancel. Up to three net units add d6; keep the highest
three for advantage or lowest three for disadvantage. The check makes at most
six draws. Check-only edge and shared effect edge are **not interchangeable**:
shared edge spends up to three units for a user-owned check independently of the
combined accuracy roll; its remainder can reach damage. Check-only penalties do
not refund that expenditure. A target-only resistance does not consume the
caster's shared damage budget. A checked rider shares the primary's remaining
budget rather than receiving a fresh unlimited one.

For a rolled N-die damage pool, available residual edge adds at most N extra dice
and retains the best/worst N. Fixed damage does not consume damage edge. Distinct
pools consume the remaining budget in canonical order; unused edge is not banked
for a later preparation. Source-family deduplication, talent sign limits,
`ignore_negative`, self-target role separation and exact token semantics belong
to [checks and edge allocation](creature-rules.md#checks-and-edge-allocation).

Breath is a named **Willpower-governed skill**, not a resource pool or an automatic
HD bonus. Its rank and governing attribute enter the ordinary kernel. Authored
Breath attacks still specify their own reach, checks, effects and costs.

Sources: [check arithmetic](../../crates/simulation/src/attributes.rs#L321-L347),
[dice kernel](../../crates/simulation/src/dice.rs),
[defense derivation](../../crates/simulation/src/creatures/derive.rs#L2351-L2376).

## Primary, exposure and effect ordering

The following order is observable through outcomes and RNG use:

1. Reject a dead application recipient or apply **whole-payload descriptor
   immunity**. A checked immune primary rolls no check and selects neither rider
   branch. This differs from a damage component becoming zero later.
2. Resolve the captured primary check, if present. A checkless primary selects
   success. Select only that branch's effects; do not roll the other branch.
3. If the selected branch is nonempty and has an exposure-resistance policy,
   capture the recipient's current hazard skill check and applicable tokens.
   A successful resistance prevents that batch's effects. It does not rewrite
   the already selected primary branch into the other branch.
4. Apply selected effects in authored order. Effects operate on current candidate
   state; later effects see earlier mutations. Report actual Health/resource
   change rather than unapplied healing or overkill.
5. Individual condition immunity and admission apply at their own effect site.
   They are not equivalent to pre-check whole-payload immunity; a duration roll
   can already have occurred before an immune condition is rejected.
6. Commit through lifecycle reconciliation, then attempt matching riders while
   source and target are living and the ability remains available.

For example, a bite whose damage is reduced to zero can still trigger an authored
poison rider. The poison can then have its own Athletics resistance; a successful
resistance avoids its loss roll. A failed attack with an empty success-only damage
branch does not roll that damage. Persistent Haste/Slow remove the opposite
persistent state according to their effect contract rather than performing a
second generic condition admission.

Sources: [branch and resistance kernel](../../crates/simulation/src/abilities/resolve.rs#L1265-L1382),
[ordered effects](../../crates/simulation/src/abilities/resolve.rs#L1537-L1685),
[payload resistance tests](../../crates/simulation/src/abilities/payload_resistance_tests.rs).

## Damage composition and protection

Damage is not one untyped integer until the final stage. Each `DamageSpec`
contains canonical components keyed by category, optional descriptor and rolled
pool identity. Matching keys merge; primary ordering is explicit. Categories
include Impact, Keen and Energy; descriptors such as fire or cold refine them.
The protection fold applies **per damage effect**, not once over every attack,
primary and rider combined.

For each damage effect:

1. Roll canonical pools, spending available residual damage edge in order.
   Apply each pool's signed bonus and floor its result at zero. Fixed components
   already supply their amount.
2. Apply component immunity. A fire component can disappear while cold and Impact
   components survive. This is later than whole-payload immunity.
3. Group eligible amounts by **(category, descriptor)** and apply descriptor
   reduction within each group, not across different categories.
4. Combine surviving amounts by category and apply category reduction.
5. Sum remaining amounts, then apply Health loss capped by current Health.

A category's reduction is not charged again separately to every die or descriptor
component. Conversely, two separately authored damage effects each use their own
protection pass. A universal rolled-damage bonus is added once per rolled pool,
not once per die and not to fixed damage. The ordinary weapon kernel does not
also add Strength or weapon skill rank to damage: those affect checks unless an
authored damage expression/modifier says otherwise.

Ordinary protection sources use their ordinary accumulation fold. Talent category
reductions use the strongest-per-category talent bucket instead of stacking every
such talent; signed contributions are folded before the final nonnegative floor.
The generic modifier system is not permission to infer an unimplemented legacy
critical, armor-class or saving-throw subsystem.

Taking or dealing positive actual Health loss resets each involved actor's quiet
progress at most once across a primary plus its riders. The recipient's first
positive loss uses the policy current at that effect; later effects cannot
retroactively preserve it. Another rider does not repeatedly reset the same
actor. A later primary owns a new guard. Zero mitigated damage,
a fully resisted exposure or overkill that causes no further actual loss cannot
stand in for a fresh positive-loss event. Exact quiet time, periodic regeneration,
nutrition and resource recovery are in [creature rules](creature-rules.md).

Sources: [canonical components](../../crates/simulation/src/damage.rs#L110-L195),
[damage/protection fold](../../crates/simulation/src/damage.rs#L295-L637),
[damage application](../../crates/simulation/src/abilities/resolve.rs),
[damage tests](../../crates/simulation/tests/it/damage.rs).

## Ordered strikes and riders

An actor-target or self-step preparation contains at most 16 strikes and 8 riders
per strike, with aggregate limits on effects and descriptors. The first offset is
zero; later offsets are nondecreasing. Preparation freezes only currently unlocked
rows while retaining original authored ordinals. Later loss of an unlock does not
rewrite that list, but loss of the whole granted ability follows cancellation.

Each due primary checks live reach. An unavailable target in a committed sequence
produces a no-RNG miss, not a free refund or automatic retarget. A target returning
before a later primary can again be reachable. The primary resolves before its
matching riders. `success`/`failure` refer to the primary **roller's branch**, even
when the roller was the target. Whole-primary immunity selects neither; zero
post-protection damage does not erase a successful check. Riders have no separate
timing, cost, retargeting or recursive riders.

A `sequence_successful_hits` rider counts only executed successful **user-rolled**
primaries structurally involving this target. Zero applied damage can qualify;
misses, unavailable targets, whole immunity, checkless/difficulty-only primaries,
target saves and rider checks do not. The current primary must itself qualify.
Each conditional occurrence attempts once per preparation; its receipt is consumed
before its check/effect, including a failed or immune attempt. An imported natural
rider can share one occurrence across rows, while row-local riders remain distinct.

Death or loss of the source ability stops later rider mutations. The source/target
state after each effect is real: later healing cannot resurrect a target already
killed. Suspension and region freezing preserve cursor, qualifying hits and
receipts; a new preparation starts fresh. Clients may see the authored rider
condition, not hidden progress or eligibility.

Exact authoring and sequence examples remain in
[shared ability rules](creature-rules.md#shared-ability-rules). Witnesses include
[strike tests](../../crates/simulation/src/abilities/strike_tests.rs),
[rider tests](../../crates/simulation/src/abilities/sequence_rider_tests.rs),
[contact tests](../../crates/simulation/src/abilities/sequence_rider_contact_tests.rs),
and [lifecycle tests](../../crates/simulation/src/abilities/sequence_rider_lifecycle_tests.rs).

## Projectiles, rays and environment variants

| Delivery | Captured source and live resolution |
| --- | --- |
| Targeted melee/ability | Captured target/check/defense operands; live target life and reach; primary then riders. |
| Physical projectile | Captured exact ammunition/thrown unit, source check/payload, direction/ranges and threat state; first live creature-backed collision uses current recipient defense. |
| Bouncing directional ray | One launch roll against each live recipient's authored named-skill check. Clear-cell visit budget; walls reverse direction. |
| Bounded first-body contact ray | Same opposed launch/recipient roles, but stops on the first eligible creature-backed recipient regardless of dodge/immunity/effect and never bounces. |
| UntilObstruction first-body ray | Closed traversal to obstruction/body/cycle; opposed source roll only after a creature-backed recipient exists, or checkless Cancellation with no dummy roll. |
| Environmental attack | Shared kernel with live Physical defense and authored environment attack bonus; no invented caster attributes. |
| Exposure | Optional target hazard resistance followed by effects; no fabricated delivery attack. |

### Physical projectile release

A projectile invocation gives direction, not a homing target. Planning captures
its attack operands, ranges, origin/body frame and whether the source is threatened.
Grid traversal costs five feet per step. The short boundary is inclusive with no
range penalty; through the inclusive long boundary the attack has one disadvantage;
beyond long range is unavailable. These are not Euclidean diagonal distances.

Release removes the exact reserved unit. A cursed equipped thrown weapon can
produce a paid blocked release and reveal its curse without extracting it. Normal
flight follows current physical geometry, does not bounce or generate new world
content, and stops at unavailable/frozen boundaries under the traversal contract.
A sheetless actor is not assigned a fictional Physical defense: physical projectile
recipient selection skips it. A physical route that loops back after leaving the
launch footprint can hit the source.

At collision the recipient supplies live Physical defense, incoming modifiers and
eligible one-use tokens; source attack/payload and launch threat remain captured.
Hit recovery follows the authored consumed-on-hit or durable policy. Miss landing
uses the bounded legal landing/merge path; if there is no legal landing, the projectile can be lost.
Exact-unit extraction, landing and stack merging are atomic, not a duplicate item
created in addition to the original.

Sources: [planning](../../crates/simulation/src/abilities/projectile.rs),
[release](../../crates/simulation/src/abilities/projectile_release.rs),
[flight](../../crates/simulation/src/abilities/projectile_flight.rs),
[collision](../../crates/simulation/src/abilities/projectile_execution.rs),
[range profile](../../crates/simulation/src/weapon_profiles.rs).

### Ray release and repeat encounters

A bounded ray makes one frozen source launch roll, even if traversal finds no
recipient. Each real recipient uses its live named-skill check against that total;
a tie succeeds. Source outgoing folds do not predict an opponent: unconditional
and ability-qualified sources are captured, while opponent-qualified source
conditions cannot borrow a future recipient as a fake launch target. Separately
captured source incoming modifiers can evaluate their opponent predicate against
the actual recipient.

For bouncing rays each clear physical cell spends a visit. Solid walls and closed
doors reverse the whole direction without spending a visit; open doors pass.
Physical portal rotations rotate direction; sight-only images and abstract stairs
do not carry the ray. Independent probe/reversal limits prevent no-progress loops.
The launch footprint is initially suppressed, but a returning ray can hit the
source after exiting it. A contiguous multi-cell body gets one check; leaving and
re-entering permits another. Each previous actual check by that recipient adds one
disadvantage on its next encounter. Whole immunity does not manufacture a prior
check or consume a recipient check token. A successful avoidance continues; a failed avoidance check intercepts even if
protection or resistance leaves zero actual damage.

Bounded contact stops on its first eligible creature-backed recipient whether it
dodges, is immune or changes. UntilObstruction instead traces to the first body,
and only a creature-backed recipient causes the
opposed launch roll. A sheetless body still intercepts this traversal, without
inventing checks or looking behind it. Checkless Cancellation contains no skill,
check token or RNG seam. Its hostile authorization is not generic attack
provocation or permission to run another stealth contest.

UntilObstruction uses a location/direction/left-launch-footprint cycle key and an
operational guard of 65,536 states. That is a work bound, not advertised weapon
range. Exceeding it is atomic failure, not a truncated successful shot. Required
frontier geometry is prepared load-only before payment; it does not activate a
frozen population. Full ray authoring constraints and smaller bouncing guards
remain in [shared ability rules](creature-rules.md#shared-ability-rules).

Sources: [ray execution](../../crates/simulation/src/abilities/ray.rs),
[first-body execution](../../crates/simulation/src/abilities/contact_execution.rs),
[contact trace](../../crates/simulation/src/abilities/contact_trace.rs),
[ray tests](../../crates/simulation/src/abilities/ray_tests.rs),
[Cancellation tests](../../crates/simulation/src/abilities/cancellation_missile_tests.rs).

### Environmental delivery and exposure

An environmental attack supplies its authored attack bonus and uses the shared
check against current Physical defense. An exposure such as a poison payload can
instead request a named-skill resistance without a preceding attack roll. Both
use ordinary effect application and actual-loss/death publication. Neither path
receives fabricated source attributes, selected weapon, class level or extra
resource costs just to resemble an actor ability.

Sources: [environment and exposure execution](../../crates/simulation/src/abilities/execution.rs#L803-L921),
[hazard checks](../../crates/simulation/src/hazard_checks.rs).

## Timing, payment, interruption and recovery

For ordinary Speed-scaled ability/weapon timing, each phase uses the ceiling of
`base_ticks × 5 / (5 + Speed)`, with the supported minimum and bounds. Nonzero
strike offsets are scaled from their absolute authored offset, not by adding
individually rounded gaps. Haste/Slow action-duration scaling follows that timing
fold: Haste halves with upward rounding, Slow doubles. Some actions explicitly
use fixed/unscaled timing; do not extrapolate the weapon rule to every activity.

Item Zap is one such distinct path: preparation is authored item
`consume_turns × actor turn_ticks`, then action-duration scaling, and **zero extra
recovery**. A device ability's 100/100 metadata is not the ordinary Zap duration.
The canonical item contract is [Zap timing](magic-items.md#zap-timing).

| Phase | Resource/item authority |
| --- | --- |
| Queue admission | Request identity and automatic selections; not whole-cost payment or item reservation. |
| Preparation start | Require full bundle available. Spend start cost; hold unpaid remainder and exact item/supply/token bindings. Start cooldown once. |
| Suspension | Keep frozen plan, unfinished work, cursor and reservations. No second start payment or reroll on resume. |
| First due strike/release | Spend held resolution cost and ordinary held consumable/charge once, even for misses/resistance. Projectile release and weak Drain Life refusal use their explicit item paths. |
| Later strikes | No second bundle payment. Effects and checks still execute in their scheduled order. |
| Discard before prefix | Release unpaid holds; already spent start costs are not refunded. |
| Discard after resolved prefix | Release unresolved work but retain completion debt through the last strike plus recovery. Paid costs remain paid. |

For a current active sequence, completion debt is remaining work to the next strike
plus `last_offset − next_offset` plus recovery. Suspension freezes unfinished work;
waiting while paused does not erase that debt. A valid cancel-and-step adds ordinary
step cost and geometry/effects; an invalid escape is atomic. Explicit cancellation
is not offered for every item preparation. Forced displacement, unavailable bound
items/grants, condition policies, death and resource-capacity reconciliation can
interrupt through their own lifecycle rules. Lowering a capacity cancels newest
unfundable holds deterministically rather than allowing an underfunded release.

Bundled capacities are Stamina `2 + Strength`, Focus `2 + Willpower`, and Mana
`2 + binding attribute` only for magically enabled creatures. Their ordinary
recovery periods are respectively 100, 300 and 800 **active** ticks per point.
This is resource recovery, not Health regeneration. Greater capacity extends a
reserve; it does not discount action cost or speed up the recovery period. Health
recovery, regeneration/quiet conditions, starvation and frozen-time semantics
remain [creature-rule contracts](creature-rules.md#health-recovery).

Sources: [duration derivation](../../crates/simulation/src/creatures/derive.rs#L1912-L1928),
[action-duration scaling](../../crates/simulation/src/action_duration.rs),
[cost lifecycle](../../crates/simulation/src/costs.rs#L244-L355),
[interruption](../../crates/simulation/src/lifecycle.rs#L124-L205),
[completion debt](../../crates/simulation/src/sequence.rs#L390-L421),
[default resources](../../crates/simulation/data/default-ruleset.toml).

## Death, attribution and body changes

Health outcomes record actual loss, capped by current Health. The transition from
living to dead creates one fatal cause; striking an already dead body cannot earn
another kill. Death cleanup and XP credit are part of the authoritative lifecycle
publication, not a client interpretation of a damage message. Self-killing does
not grant self-XP. A later healing effect does not undo an earlier fatal transition.

Forced motion/falling retains its explicit causal provenance where supported;
credit is not automatically assigned to whichever actor most recently appeared in
the log. Environmental loss without an eligible actor cause does not invent one.
The movement/fall tests distinguish ordinary movement, physics, delayed impact and
source-free restore. Generic body reconstruction can retire causal fall state;
**Polymorph explicitly preserves the existing fall cause** during its atomic body
transition, so the generic rule must not be applied blindly to it.

Build changes derive a candidate and reconcile effective grants, resources,
equipment and preparations through the same lifecycle boundary. Retained learned
training can become dormant when a new body lacks prerequisites. A new preparation
uses the new effective body; an already captured source is not silently rewritten.
Losing an entire ability can cancel unresolved work while preserving prefix debt.
A paid natural-source plan can remain valid across a transition when its actual
authority remains valid; equipment plans still require their exact bound source.

Current [baleful Polymorph](polymorph.md) preserves absolute injury, not current-HP
percentage. A frailer destination can therefore kill the actor. It is indefinite;
zero Health is actual death, never protective temporary reversion. Incompatible
equipment becomes carried even if cursed. Cancellation suppresses and attempts the
base return, with persisted pending return if a living actor cannot fit or retains
blocking package-HD losses. Future
temporary-form snapshot/reversion rules must not be substituted for this behavior.

Sources: [death publication](../../crates/simulation/src/lifecycle.rs#L1638-L1726),
[earned growth](../../crates/simulation/src/earned_growth.rs),
[fall attribution witnesses](../../crates/simulation/src/abilities/movement_tests/fall_credit.rs),
[Polymorph transition](../../crates/simulation/src/polymorph.rs),
[natural-source witnesses](../../crates/simulation/src/abilities/natural_source_tests.rs).

## Worked calculations

These examples fix their operands/dice explicitly. They do not claim those rolls
are random samples or that the illustrative actor is an actual starter build.

### 1. Attack total and armor

Suppose a Heavy Weaponry attack has Strength2, rank1 and hit modifier+1. Its retained
dice are 4,3,4: total `11 + 2 + 1 + 1 = 15`. Against captured Physical14 it succeeds.
Suppose the selected damage is 2d3 with dice3,2 and damage bonus+1: raw damage6.
Impact reduction2 leaves4. It removes4 Health only if at least4 remains; it does
not also add Strength2 or rank1 to damage.

### 2. Shared edge consumes check then damage dice

The literal `five_advantages_cover_three_check_and_two_damage_dice` fixture uses
seed42 and seven advantages minus two disadvantages, net+5. The check consumes3
shared units, draws `[2,2,1,1,5,1]`, retains `[5,2,2]`, sum9. The remaining+2 goes to
2d6+2 damage: draws `[2,3,2,3]`, keeps `[3,3]`, total8. The budget is then neutral.
See [dice fixture](../../crates/simulation/tests/it/dice.rs#L73-L85).

### 3. Immunity before two reduction levels

One damage effect has Energy/fire7, Energy/cold5 and Impact3. Fire immunity removes7.
Cold descriptor reduction1 leaves4 cold; Energy category reduction2 leaves2 Energy.
Unreduced Impact3 makes total5. Category reduction is not applied separately to
fire and cold before grouping. See the independent
[damage test](../../crates/simulation/tests/it/damage.rs#L199-L243).

### 4. Avoidance, zero damage and resistance are separate

A ray launch total16 meets a target Agility total16: the target succeeds on the tie,
so an empty success branch causes no damage. In a separate bite example, a hit can
have its physical damage reduced to0 and still attempt its poison rider. With
Strength2, Athletics1 and retained dice3,3,3, the resistance total is12; against
DC12 it succeeds, so no poison-loss roll follows. Had an ordinary damage effect
instead left4 mitigated damage, actual Health loss would be `min(4, current HP)`.

### 5. Timing and commitment

At Speed2, weapon phases60/40 become `ceil(300/7)=43` and `ceil(200/7)=29` ticks.
With Haste those become22 and15. A two-Stamina ability split1 at start/1 at
resolution requires2 available before starting, spends1 immediately, and holds1;
a miss still spends the second point.

For a separate already-scaled sequence with preparation40, offsets0/20/50 and
recovery30 beginning at tick100, primaries are due140/160/190 and readiness220.
Cancellation at150 after the first primary owes10 to the next,30 between next and
last, and30 recovery:70 ticks, still ready220. A pause freezes unfinished work;
resume later shifts remaining execution rather than catching up free of cost.

## Disclosure, persistence and implementation map

Ordinary clients receive permitted actor references, visible outcomes and authored
interaction affordances, not private form IDs, unknown item identities, full target
sheets, hidden sequence progress or predicted collision paths. A historical event
uses the disclosure captured for that event: later identifying a device does not
retroactively rename an unknown old attack. Optional diagnostics use the same
checks/effects/RNG, and remain subject to their separate privileged boundary.

Saves retain the immutable plan and exact private reservations, cursor, selected
forms, random provenance and paid progress needed to resume without rerolling or
reimporting source. The current-format validator rejects malformed/truncated state;
it does not rebuild an approximate preparation from visible history. Private
load-only continuation boundaries must settle before checkpoint publication.
See [persistence](../architecture/persistence.md), [wire protocol](protocol.md),
and [client architecture](../architecture/clients.md).

| Contract area | Focused implementation/test entry points |
| --- | --- |
| Frozen targeting, roles and ordered plan validation | [compile](../../crates/simulation/src/abilities/compile.rs), [plan validation](../../crates/simulation/src/abilities/plan_validation.rs), [strike tests](../../crates/simulation/src/abilities/strike_tests.rs) |
| Check/edge arithmetic and replayable draws | [dice](../../crates/simulation/src/dice.rs), [dice integration](../../crates/simulation/tests/it/dice.rs), [edge policy tests](../../crates/simulation/src/abilities/edge_policy_tests.rs) |
| Protection, exposure, ordered application | [damage](../../crates/simulation/src/damage.rs), [resolve](../../crates/simulation/src/abilities/resolve.rs), [resistance tests](../../crates/simulation/src/abilities/payload_resistance_tests.rs) |
| Supplies/costs and interruption | [costs](../../crates/simulation/src/costs.rs), [cost tests](../../crates/simulation/src/costs_tests.rs), [interactions](../../crates/simulation/src/interactions.rs), [sequence scheduler](../../crates/simulation/src/sequence_execution.rs) |
| Weapon binding, physical devices and missiles | [weapon tests](../../crates/simulation/src/abilities/weapon_selection_tests.rs), [physical-device tests](../../crates/simulation/src/abilities/physical_device_tests.rs), [projectile release](../../crates/simulation/src/abilities/projectile_release.rs) |
| Rays, checkless delivery and privacy | [ray tests](../../crates/simulation/src/abilities/ray_tests.rs), [Cancellation tests](../../crates/simulation/src/abilities/cancellation_missile_tests.rs), [publication tests](../../crates/simulation/src/abilities/publication_boundary_tests.rs) |
| Rider effects and death/lifecycle ordering | [rider lifecycle tests](../../crates/simulation/src/abilities/sequence_rider_lifecycle_tests.rs), [participation tests](../../crates/simulation/src/abilities/participation_tests.rs), [fall tests](../../crates/simulation/src/abilities/movement_tests/fall_credit.rs) |

These are source and assertion pointers, not a claim of a fresh runtime test pass.
Future balance, temporary Polymorph, broader form eligibility and deferred trap
feedback remain separate design/qualification work, not hidden rules in this page.
