# Creature rules reference

The simulation evaluates validated scenario content through one modifier fold and
one plan/resolve/apply pipeline. The [architecture](../architecture/overview.md),
[rules principles](../architecture/simulation.md#rules-design-and-review), [scenario format](scenario-packages.md)
and [testing policy](../development/testing.md) describe the surrounding contracts.

## Content and identity

Each scenario manifest names a rules import with optional scenario overrides, or
an explicit standalone rules document. Its validated definitions
include abilities, fixed starter forms, talents, conditions, creature types, subtypes, classes,
descriptors, ability tags, equipment slots, flags, resources and tuning constants. Species and
templates form the scenario catalog. Content IDs are bounded strings; display
names are separate, non-authoritative text. Unknown IDs, duplicate definitions,
cycles, invalid modifiers and out-of-bound values fail loading.

A game owns one immutable shared ruleset and catalog. A build references that
content, its species and templates by ID, and retains its owned hit-die ledger
and advancement choices. Saves store the content once, its ruleset identity and
effective hash, imported source hashes, and each actor's mutable state and IDs.
Imported rules are resolved once and saved as immutable content; restore does not
re-import changed general rules. Restoring these rules and catalog definitions
does not require their original source directory. A streamed save may still need
uncopied region sources from the matching package; see
[package recovery](scenario-packages.md#saves-and-the-package). Only the current coordinated formats in the
[roadmap](../status.md) are accepted; there is no compatibility importer.

## Derivation and sources

Species, type, subtype, class, template, active talent, equipment and condition
sources feed the same exhaustive modifier vocabulary. The fold handles additions,
sets, protection, flags, abilities and ability-specific values. Flags expand their
explicit data-defined modifiers. Template priorities retain aggregate-then-floor
semantics, reject conflicting sets, and do not depend on map insertion order.

The actor-wide `action_duration` operation has exactly the string values
`normal`, `haste` and `slow`, with `scope = "always"` only. All expanded sources
participate in one idempotent fold: repeated haste or slow does not stack; any
haste together with any slow yields normal, regardless of source count. Normal
is neutral and does not suppress another source. This is independent of Speed,
attributes, checks and defenses. After ordinary distance/attribute timing,
haste halves duration rounded up and slow doubles it with checked arithmetic;
zero-time work remains zero. See [captured action timing](../architecture/simulation.md#actors-and-deterministic-time).

Actors compile their exact permanent build records into one immutable, shared
basis. It contains source-expanded permanent modifiers, attributes/ranks, HD and
class counts, talent gates, anatomy, and the exact per-HD Health contributions
with a checked cached sum. The effective character sheet folds equipment,
conditions and restorable losses from that basis through the same source pipeline;
it never folds an already-adjusted sheet. Dynamic changes neither resample Health
nor revisit advancement records. Snapshot clones share the basis and provenance.
Damage, pools, clocks, cooldowns and nutrition use the effective cache directly.
Ordinary observations share the compiled HD total and per-source counts; they do
not copy or compare ordered HD records. Wire projections and Text/ASCII summaries
visit distinct sources only. Authorized detailed inspection still exposes the
exact ordered seeds, Health contributions and owned choices, and checks their
source identities, labels and counts against the ordinary summary.

Counts and zero-based HD owners use distinct exact 64-bit domains. Aggregate
Health, injury, point totals, threat and XP prices are also checked 64-bit values;
products and signed Health folds use wider checked intermediates. Every actual
HD and its sampled Health contribution remains recorded. Bounded per-HD operands,
resource capacities, ranks, attributes, maximum-Health deficits and purchase
indices retain their existing limits. The current 256 actual-HD admission guard
is accepted for functional Rogue; exact archive and disclosure paging beyond it
are deferred. A wide summary or funded tier does not admit additional rows.

Permanent statistical edits replace the compilation atomically. Future-source or
restoration-policy-only edits still advance the build epoch when changed, but do
not rebuild statistics. Condition refresh, immunity, reservation and expiry compare
actual source contributions before refolding, including qualified token identity.
Saves retain every actual HD seed and owned choice; caches are private and rebuild
from validated records on restore. Standalone build derivation remains a fresh
compile followed by the same effective fold. Source/family/talent/token-qualified
edge selection and frozen paid plans keep their existing contracts.

Talent prerequisites form a validated graph. Requirements use permanent build
facts; temporary equipment and conditions cannot unlock owned advancement.
Requirements support subtype membership, species-owned catalog flags, and owned
talents inside `all`/`any` groups. A species flag must originate in that species'
expanded source; a class, template, talent, item or condition cannot substitute.
Parent talents must be active. Draining a parent's HD or invalidating its gates
therefore makes dependent choices dormant; an eligible alternative in an `any`
group can keep a child active. The loader rejects missing references and cycles,
including cycles hidden inside OR alternatives. Declaration order has no effect.
Unsupported choices remain dormant rather than being silently deleted. Build edits
clone once, validate once, derive once, then commit atomically. Rebuilds preserve
injury and retained ability cooldowns, and reconcile resource balances,
reservations, conditions and death.

Equipment slots are ruleset content IDs. Each slot declares its display name,
equipped phrase, compatible item classes, attack capability, and equip/remove
turn multipliers. Rules data also declares the default anatomy. An actor’s
numeric socket index identifies a particular position, including repeated slots;
clients receive the slot ID and disclosed presentation/capability data. Unknown
slot references are rejected in authored content and restored state. Attributes
and resources use named fields. Health rolls remain seeded per hit die
and independent of the rules RNG. The hit-die ledger retains its per-ordinal
seeds: removing and re-adding an ordinal yields the same Health seed. Its explicit
restoration policy either leaves the restored purchases empty or refills that
ordinal from the saved authored build. Maximum-Health changes preserve injury and cannot revive a
dead creature. Pool recovery saturates while retaining fractional active-time
progress; capacity increases do not refill pools. Dice pools are bounded and
use rejection sampling rather than biased modulo selection. Damage uses bounded canonical components with
optional descriptor IDs. Descriptor and category immunity precede reductions.
Category reduction from permanent talent sources uses the strongest amount in
that category. The explicit `talent_reduction` operation (`category`, `amount`)
lets an ordinary timed condition join that same strongest bucket. Repeated
applications, causers and stacks never add its protection. Expiry exposes the
strongest surviving contribution; no ward owns an extra clock. Ordinary
`reduction` from species, type, equipment and conditions remains additive, as do
existing descriptor-scoped reductions. The combined category total is bounded.
All protection is live at resolution, including for an already captured attack;
category and descriptor immunity still take precedence.

Equipment contributes modifiers directly; the highest ordered equipped
weapon-capable slot supplies the selected weapon, otherwise the natural attack is
used. Equip and unequip timing comes from the selected slot definition; consume
timing is a ruleset constant. Body armor’s longer timing is authored data. There
is no compatibility combat profile or category-only hazard path.

Inspection reads cached health die sizes and rolls, selected natural/equipped
attack provenance, check operands, effective ability timings and reach. It does
not re-roll health or re-derive the inspected build. Natural attack and selected
weapon are disclosed separately, including item and numeric socket attribution.

## Checks and edge allocation

A check rolls three d6, then adds its frozen attribute, skill rank and permanent
flat modifiers. Meeting the difficulty succeeds; an opposed check succeeds on a
tied total, preserving the same comparator. Natural totals of 3 and 18 have no
automatic failure or success. Both sides of an opposed check use this kernel.
The initial defense baseline remains the ruleset's base plus its paired
attributes; arena measurements, rather than a numeric d20-probability conversion,
provide evidence for tuning.

Breath is an ordinary named skill (`breath`) governed by Willpower, independent of
Mana binding. Its checks use the compiled sheet's Willpower and owned Breath rank;
neither creature identity nor raw HD adds a separate runtime bonus. Choosing Breath
as one of the two starting skills grants rank 1. Each later rank costs an ordinary
advancement point and the combined rank remains capped at five. For example,
Willpower 3 with starting Breath 1 and one purchased rank gives a +5 check bonus.

Positive and negative sources cancel one-for-one. Up to three net check-edge
units add that many d6: keep the highest three for advantage, or the lowest three
for disadvantage. The check is bounded to six draws and allocates no heap memory.
Diagnostics record actual draw order, retained dice, their sum, modifiers and the
inclusive threshold; they do not reroll the result.

Shared effect edge and check-only edge have separate frozen roles. A source that
specifies only a check (including a next-check token or incoming-check condition)
changes the combined check roll, never a damage pool. For a user-owned check,
three units at most are debited from the user's shared edge independently of the
check-only contributions; only the shared remainder reaches later damage. Thus
shared +3 with Readied rolls at most six dice and leaves no damage edge; shared
+5 with Readied leaves +2 damage edge. Check-only penalties likewise cannot
refund shared edge into damage. With shared +5 and check-only −6, the check has
−1 edge while damage retains +2: an accuracy-only penalty does not alter damage
edge. Family deduplication and talent-source limits apply to the check fold, not
the independent shared effect budget. For example, a talent's shared +5 is
capped to +1 on the check, while its shared budget still spends three units and
leaves +2 for damage. `CheckEdge`, ability check edge and incoming check edge
never enter that shared budget. Check traces distinguish unused combined check
edge from the shared budget spent and the remainder reserved for damage.
A target-only resistance check does not spend
the caster's shared edge; an opposed caster check does. User and target are
structural roll roles even when both refer to one actor: the target role retains
its own shared check edge without debiting the user's damage budget. Damage keeps its existing
canonical rolled pools, keep-best/keep-worst policy and protection order. A miss
never rolls an absent success damage branch, and unused edge is not banked.

Abilities may author `outgoing_edge_policy = { kind = "ignore_negative" }`.
Omission in source means `{ kind = "ordinary" }`; compiled and saved ability
records require the explicit named policy object. Unknown variants, surplus
fields and positional forms are rejected. The policy removes applicable
negative-valued outgoing `add` and `set` contributions before cancellation,
condition-family deduplication and talent-sign limits, including implied flags,
ability/tag check edge and the separately frozen shared budget. Zero and positive
`set` values retain ordinary override behavior, even when they reduce an accumulated
value: this is source-sign selection, not a clamp on the net result.

Selection applies to the user's roll role in every primary, opposed and rider
site. The target's resistance/opposed role and the opponent's incoming modifiers
remain ordinary. Roles stay distinct when an ability targets its own user.
A target-only self-resistance check keeps the target role's shared check edge
without debiting the user's damage budget; an opposed user roll debits it once.
Planning freezes the selected check operands and independent shared budget once.
Resume and replay use those operands and the pinned rules definition, not current
conditions or re-imported source. The roll and damage kernels are unchanged.

The unmodified 3d6 mean is 10.5. Exact enumeration gives 108/216 success for a
natural target of 11; one, two and three advantage dice give 947/1296,
6688/7776 and 43305/46656 respectively. These are kernel probabilities, not
claims that a class or species is balanced. Pinned streams and full integer
outcome histograms are checked by [dice tests](../../crates/simulation/tests/it/dice.rs)
and the [kernel's exhaustive tests](../../crates/simulation/src/dice.rs).

## Actor-active cooldown state

An internal cooldown kernel stores one positive remaining active-time interval
per stable ability ID. Its required mutable-record field is a bounded list of
named `ability`/`remaining` rows, emitted in ability-ID order. Unknown IDs,
duplicate IDs, missing or extra fields, positional rows, zero durations and
durations above the numeric limit reject. Entry count cannot exceed the ability
catalog bound. Empty state is explicitly recorded as an empty list.

Starting an already-active cooldown rejects without renewing it. The interval
is frozen once started and is not scaled by Speed or changed by rebuilds.
Permanent drain, reacquisition, temporary conditions and equipment changes keep
remaining entries even when their ability is no longer granted. Expiry removes
the entry; there is no cast-history list. Death retains and freezes this state.
The existing actor-active clock and deadline index handle countdowns alongside
condition and recovery timers; frozen or detached actors earn no catch-up time.

Snapshots share untouched cooldown storage. Advancing a touched actor examines
its active entries once per condition-expiry interval, with a copy on the first
shared write. Finding its next cooldown deadline scans those entries. Operation
counters expose those costs; they are bounded linear work, not constant-time
claims or a wall-clock performance certification.

[Game clock tests](../../crates/simulation/tests/it/cooldown_game_time.rs) seed
cooldowns through current checkpoint records and exercise read-only projection,
active settlement, exact expiry, resting/moving physics, grant loss, rebuilds and
death. Frozen checkpoints and separately byte-decoded detached region records
exercise deadline removal, reattachment and thaw without catch-up. Malformed
cooldown records reject at both restoration boundaries; a failed attachment
publishes no partial game state.

Abilities author an optional positive cooldown, frozen into their prepared plan.
Successful atomic preparation starts it once alongside resource and finite-item
holds; a failed start changes none of those authorities. Fresh preparation is
blocked while that ability's entry remains active. Resuming the original paid
preparation neither repays the start nor restarts a naturally expired cooldown.
Cancellation and grant loss release unpaid item holds without refunding cooldown.
An actor's own observation discloses its remaining intervals, without exposing
other actors' preparations or item claims. Ordinary
player/AI actions, journal replay and source-independent restart exercise this
integration. Ordinary [player advancement and timed respec](../play/advancement.md)
use the same cooldown-preserving lifecycle. [Earned progression](#earned-xp-and-future-hit-dice)
adds HD through that rebuild boundary while leaving purchased choices unspent.

## Restorable attribute and maximum-Health losses

Creatures retain one fixed restorable-loss record: six named attribute deficits
and one maximum-Health deficit. These mutable aggregates have no causer history,
timer, expiry, automatic recovery or separate damage ledger. Ordinary healing,
waiting, nutrition changes, equipment, conditions, respec and region freezing do
not erase them. Current saves require the complete bounded record.

The shared effects are `lose_attribute` and `restore_attribute` (an `attribute`
and `amount`), and `lose_maximum_health` and `restore_maximum_health` (`amount`).
Amounts use the ordinary fixed or rolled expressions, frozen at preparation and
sampled at resolution. They work in ordinary abilities, checked riders and
consumables. Existing rider descriptors provide immunity and checks; loss does
not invent an armor reduction, saving throw or implicit descriptor.

Attribute deficits apply after the validated ordinary attribute modifier fold,
including Set. The effective attributes govern defenses, Health, resources,
checks and timing. Maximum-Health deficits subtract after the validated Health
calculation. Undrained computations must also remain valid, so deficits cannot
conceal an otherwise overflowing source combination. Permanent talent ownership
and eligibility remain based on the unchanged build and permanent facts.

A loss records only the actual amount allowed by the current effective value
and the aggregate's remaining capacity. Attribute deficits are bounded at 1000
each, maximum-Health deficits at 1000000. Restoration removes at most the recorded
deficit. Later legitimate source changes may leave a deficit larger than today's
capacity; effective values floor at zero while the full deficit remains available
for restoration. Partial restoration can therefore reduce the deficit without
immediately increasing the effective statistic.

Maximum changes preserve injury and use the ordinary sticky-death boundary.
Strength loss can be fatal because Strength contributes to Health. Zero in an
attribute introduces no additional death rule. Dead recipients remain unchanged;
restoration never revives, heals injury or renews the received-damage quiet interval.
Resource capacities, reservations and recovery fractions use their ordinary
reconciliation. A fatal loss awards its actual effect owner once through the
same transactional death/XP path as other fatal effects.

Loss/restoration failures reject the complete effect transaction, including its
RNG and consumed item. Existing soft condition-admission rejection remains soft
for ordinary actions. Prepared operands stay frozen through legitimate source
changes while the grant and ordinary capacity requirements remain valid. Changed
loss aggregates can retry already-funded growth without themselves advancing the
permanent build epoch.

Personal stats and authorized inspection disclose the bounded restorable values;
ordinary opponent observations disclose no deficit or causer. Text and ASCII use
the same summary. Rogue supplies an ordinary concealed restore-strength potion
using only `restore_attribute(strength, fixed 1000)`: the operand equals the
canonical attribute-deficit bound and clamps to actual loss. It adds no healing,
maximum-Health restoration or bonus Strength. Its finite availability is described
in the [Rogue guide](../play/rogue.md#strength-loss-and-ordinary-restoration).


### Latest-Hit-Die drain and restoration

`drain_latest_hit_die` removes one actual latest HD and its owned purchases.
`restore_hit_die` restores at most one previously applied HD. Both effects have
no authored operands: source, Health seed, ordinal and purchases come from the
ordinary advancement ledger. They use the same abilities, riders, consumables,
frozen plans and transactional publication as the other loss effects.

Restorable count is the highest applied growth tier minus current actual HD.
A funded but unapplied earned tier never becomes free restoration. Authored
ordinal recipes restore their exact source and purchases. ChooseAgain creates
an empty slot and requires an explicitly selected future racial or class source.
The original Health root and ordinal are required, so neither restoration nor
paid regrowth rerolls a retained Health stream. Missing source/root rejects new
admission atomically, with an explanation in owner advancement inspection. A
previously committed plan that becomes ineligible retains its normal costs and
debt and resolves unaffected.

Drain and restoration re-evaluate permanent gates and dormancy through the
ordinary rebuild. They preserve injury, cooldowns, paid balances, quiet-time
progress and valid frozen plans. Genuine grant loss cancels the unresolved tail
with its full commitment/recovery debt; restoring a grant does not replay the
prefix or resurrect cancelled work. Removing the last HD causes ordinary sticky
death, inventory drop and once-only fatal-owner XP. Restoration never revives.

The applied and earned growth watermarks remain distinct saved facts; no
second lost-HD history is stored. Owner stats disclose the computed count and
advancement discloses the applied ceiling and missing-source/root reason.
Restoration never spends or refunds XP. Drain may unblock previously earned,
unspent credit for ordinary paid regrowth; already spent credit is never reused.
See [earned growth accounting](#earned-xp-and-future-hit-dice).

A newly restored contextual recovery source uses the ordinary lazy growth
context boundary. Frozen actors preserve a pending denominator until thaw;
noncontextual drain never asks a frozen actor to perceive the world.

## Health recovery

Health retains one authoritative value: injury against the derived maximum.
Natural recovery removes injury; it does not add a second balance or make Health
available to ability costs. Dead actors never recover. Full Health has no recovery timer
and banks no progress.

Rules define `health_recovery` with `base_period`, `reduction_per_hd` and
`minimum_period`. The HD curve is `max(minimum_period, base_period -
reduction_per_hd × (HD - 1))`, with saturating subtraction. The current provisional
candidate is 1 Health per `max(200, 1200 - 40 × (HD - 1))` active ticks. These are
local tuning values, not a numerical conversion from Rogue or d20; see the
[recovery baseline](../development/arena.md#health-rest-to-full-baseline).

The common source fold accepts three Health-specific stats from species,
talents, equipment and conditions:

- `health_recovery_period`: add or set ticks before percentage scaling, minimum 1
- `health_recovery_period_percent`: add or set a percentage, starting at 100;
  the final percentage must be 1–10000. Scale the tick period and round up.
- `health_recovery_reset_percent`: add or set the fraction of earned progress
  removed by actual damage, starting at 100; the final percentage must be 0–100

The final normal-nutrition period must be at most 1000000 ticks. Percentage modifiers
combine through the same additive/set fold as other stats; two additive −25
percentage-point modifiers yield 50%. A reset percentage of 0 bypasses quiet.
A reset percentage of 50 removes half the accrued progress, rounding remaining
progress down. This is the provisional U07 interpretation; recovery itself has no
separate post-hit timeout. The received-loss safety interval below is independent.
The two period stats also accept `while_no_sensed_hostile` scope. They contribute
only when ordinary actor perception discloses no living hostile. The reset stat
and unrelated operations reject that scope. Both possible periods are bounded
by the same fold; scoped contributions remain tracked even when their net
percentage currently cancels. There is still one Health recovery clock.

Taking or dealing positive applied damage resets quiet at most once per strike
for each actor involved. Primary and ordered riders share that reset guard; a
later strike starts a new one. Each actor uses the policy at its first positive
applied damage, so an earlier rider can change the policy before any damage, but a
later effect cannot retroactively preserve progress. Self-damage and multi-effect
outcomes cannot reset twice.
Misses, immunity, zero applied damage and walking do not reset it. Healing items
remove injury faster without resetting remaining progress, unless Health is full.

Recovery consumes only actor-active simulation ticks. Frozen/detached regions,
a paused simulation and disconnected wall-clock time earn nothing; queries do
not advance time. Suspending prepared actor work does not itself pause that
clock: recovery can continue while the actor's region advances. Moving across
a region seam never grants Health. Existing lazy active-clock deadlines schedule
each next Health point alongside resource recovery and condition expiry.

The nutrition hook accepts normal recovery, an integer slowdown from 2 to 100,
or stopped recovery. Required nutrition owns this policy (see below); exempt
creatures can still use the standalone hook. Before a policy
or derived-period change, settle time under the old policy. Rephase earned
progress as `floor(old_elapsed × new_period / old_period)`; stopped recovery
retains its fraction using the base period. Changing or repeatedly toggling
policies never heals or increases that fraction. Source acquisition seeds the
recipient's actual perception context before its single ordinary rephase.

A derived index selects only contextual recovery owners. Authoritative movement,
lighting, relationship, source and lifecycle boundaries use the ordinary cached
scene and perceived-body index; reads never refresh the recovery policy. A hidden
hostile cannot change the observer's Health timing. Context reconciliation is
bounded by the initial watched-owner count plus one: settlement can expire
existing contributions but cannot introduce a previously unindexed owner, and
each owner can settle the same pre-boundary active interval only once. Public
mutation transactions keep world and clock changes atomic on rejection.

A derived branch-local witness covers geometry, illumination, actor placement,
orientation, body, life, faction, watched membership, frozen activity and
relationship policy. Unchanged boundaries avoid repeated owner-scene queries and
copies even above scene-cache capacity. Games without contextual owners skip
actor witness capture; contextual games still pay bounded mutation-witness work.
These work-count properties are not a claim of overall measured speedup.

Save records store elapsed progress, nutrition, and the captured sensed-hostile
clock context, deriving the denominator from the current bounded policy. Active
restore rejects a context inconsistent with restored perception. Frozen actors
retain their captured denominator until the legitimate thaw transition; they
earn no hidden elapsed time. Invalid policy, missing/extra/positional clock
fields, progress at/above the period, or nonzero progress on full/dead Health
rejects.
Changing an existing frozen owner's contextual sources retains that captured
context. Acquiring the first contextual source on an already-existing frozen
ordinary actor rejects atomically until thaw; ordinary noncontextual edits remain
available. Initial construction has no elapsed history and remains separate.
The reconciliation witness is never saved or trusted by restore: active contexts
are checked against actual restored perception before rebuilding it.
Personal stats and privileged inspection disclose period, progress, stopped state
and reset percentage; enemies still disclose only qualitative injury.

## Shared nutrition, eating and starvation

The rules document requires an explicit tagged `nutrition` policy: `{ kind =
"disabled" }`, or `kind = "enabled"` with `initial`, `maximum`, `hungry_below`,
`weak_below`, `starve_at`, `unit_ticks`, `hungry_recovery`, `weak_recovery`,
`empty_recovery`, and `regeneration_cost`. Each recovery field explicitly names `{ kind = "normal" }`,
`{ kind = "slowed", multiplier = 2 }` (integer multiplier 2–100), or
`{ kind = "stopped" }`. All three fields are required, with no legacy alias.
Every authored creature separately declares `nutrition = { kind = "exempt" }`
or `{ kind = "required" }`; required creatures need an enabled policy. There
is no species inference or compatibility default. The shared bundled policy stays
disabled; the Rogue package explicitly enables it for Warrior/Mage while its emus
and snakes remain exempt. Other authored NPCs can opt in through the same Required
construction path. The small mechanics fixtures and provisional Rogue mapping are
not evidence of calibrated survival balance.
Prepared population recipes retain the same explicit enrollment field as ordinary
player and NPC construction. Enrollment starts at the actor's birth boundary;
generation, waiting for first entry and pre-birth global ticks consume no reserve.

One reserve unit is consumed per `unit_ticks` actor-active ticks. Initial reserve
is nonnegative, at most maximum, and above the fatal boundary. Maximum, thresholds
and tick periods are bounded by 1000000, with `0 < weak_below < hungry_below <=
maximum` and `-1000000 <= starve_at <= 0`. Required state records
signed `reserve`, digestion remainder `elapsed < unit_ticks`, and a separate
`regeneration_remainder < regeneration_cost.denominator`; death clears both
remainders. The reserve must be above `starve_at` while alive and at least that
boundary when dead. Exempt state records only its tag.

`regeneration_cost = { denominator = 3, health = 1, stamina = 1, focus = 2,
mana = 3 }` declares nutrition per actual periodically regenerated point as the
corresponding numerator divided by the shared denominator. Every field is required;
the denominator is 1–1000000 and each numerator is 0–1000000. Zero explicitly
disables that resource's cost. Integer arithmetic accumulates all four resources
into the same saved fractional remainder, so three one-third gains consume one
reserve unit even across different resources, saves or separate actions. There is
no floating-point rounding or per-event minimum charge. Full pools, zero gain and
excess beyond a cap cost nothing. Direct Heal/Restore effects, potions, bandages
and other consumables do not pay this periodic-regeneration cost. Time drain
continues independently. Eating preserves both fractions.

Rebuilds preserve enrollment and retained nutrition state. Required creatures
reject any change to their exact nutrition policy atomically, including after
death; rebuild is not a policy-migration boundary. Exempt creatures retain their
independent Health-recovery nutrition hook across standalone ruleset rebuilds.
Live games additionally require rebuilds to use their pinned content.

At or above `hungry_below`, the creature is Nourished and Health recovery is
Normal. Hungry reserve (`weak_below <= reserve < hungry_below`) uses
`hungry_recovery`; Weak reserve (`0 < reserve < weak_below`) uses `weak_recovery`;
Empty reserve (`reserve <= 0`) uses `empty_recovery`. Each band is independent:
hunger labels do not impose a slowdown or stop, and all-Normal is a valid neutral
policy. The Slowed/Stopped mechanics fixtures are not a selected Rogue policy.
These transitions use the existing Health clock, preserving its fraction. An
interval is settled analytically, splitting only at bounded hunger/empty/death crossings and existing condition
expiries. Resources and Health first earn work at their old rates; simultaneous
source expiry and hunger changes rephase once to the final denominator. Runtime
arithmetic cost is independent of the number of elapsed ticks or nutrition points.
Each combined time/regeneration crossing is found by monotone integer projection
with at most 64 search probes, bounded by the existing hunger bands and condition
expiries. All simultaneous actual gains occur at the old rates, then their combined
nutrition cost and time drain settle, expiry reconciles, and Health rephases once.
Starvation cancels subsequent recovery, including the remainder of a long interval.
The existing next-point resource/Health deadlines already precede every possible
regeneration-driven crossing; no new scheduler or per-point catch-up loop exists.
An enabled stochastic trigger adds bounded, individually ordered random deadlines,
as described below.
Perception-conditioned recovery keeps its saved Known/Pending context. Hunger
changes only the authored nutrition policy of that existing clock; frozen pending
clocks keep their raw denominator until the admitted thaw resolves perception.

Existing actor-active deadlines schedule the next crossing. Queries, frozen and
detached time, reconnection and region seams neither consume nor grant nutrition.
Pausing a preparation does not freeze a creature whose region remains active.
Eating adds explicit item nourishment through the shared frozen effect plan:
`min(maximum, max(0, reserve) + nourishment)`. It preserves the digestion
remainder and grants no Health or XP directly. A meal freezes a strictly positive
fixed or rolled declaration; the shared rules RNG samples rolled nourishment only
when the live held meal resolves. The canonical applied gain excludes cleared debt
and excess above the cap. Item quantities, reserve changes and RNG advances commit
or roll back together. The exact-unit hold, hidden operand and ordinary item-work replacement rules
are in [items](items.md#explicit-edibility).

Reaching `starve_at` invokes ordinary injury/death and lifecycle cleanup. It has
no invented attacker or XP recipient; the death event names its cause as
starvation, and every other death as injury. Timers/effects precede action completion:
if Eat and starvation share a tick, death cancels Eat and releases its held unit
unconsumed; ordinary death drops it. Completing one tick earlier can save the
creature. Strict restore checks the reserve against the saved Health nutrition
policy. Personal stats and authorized inspection show hunger and reserve;
ordinary visible actors do not disclose another creature's reserve. Protocol
validation checks the disclosed Health recovery period, fraction and stopped flag
independently of hunger labels; it does not infer or expose the hidden rules policy.

Automatic fainting uses the authored trigger below. Authored Held/Frozen encounters,
final Rogue constants and full survival balance remain separate accepted work.
The provisional Rogue fatal boundary is explicitly mapped from the pinned source,
while shared condition denial and pause semantics remain generic.

### Authored empty-depletion conditions

The rules document also requires `nutrition_condition_trigger`, explicitly
`{ kind = "disabled" }` or `{ kind = "on_empty_depletion", condition = "...",
chance = { one_in = 5 }, duration = { kind = "rolled", dice = { count = 1,
sides = 8, bonus = 3 } }, ticks_per_unit = 100 }`. Enabled triggers require enabled
nutrition and a known Timed condition. The existing fixed/rolled amount operand
must be strictly positive; checked multiplication by positive `ticks_per_unit`
must fit the shared 1–1000000 active-tick condition duration. `one_in` is 1–1000.
All fields and nested records have strict named current shapes, with independent
saved adapters and no aliases or defaults. A live Required actor's numeric and
trigger policies cannot change through a rebuild.

The existing actor deadline index stops at every eligible empty-depletion boundary.
The first check follows reserve 0 → -1; 1 → 0 has no check. At a due tick, expiry
and reserve arithmetic settle first, starvation wins, and a living actor still
restricted from VoluntaryActions skips the draw. Otherwise one chance draw occurs;
only success samples the duration and invokes the ordinary condition effect pipeline.
These checks belong to time-drain unit boundaries. Regeneration cost alone draws
no chance or duration; when both costs share a tick, eligibility uses the reserve
after time drain but before the simultaneous regeneration charge. Combined
starvation still wins before any draw. Regeneration that empties the reserve can
therefore make the next time-drain boundary eligible without adding catch-up rolls.
OrdinaryMovement/MovementAbilities restrictions and protection-phase conditions do
not suppress checks. Denied intervals skip analytically; expiry preserves digestion
phase and never creates catch-up rolls. Queries, previews and restoration use pure
arithmetic and consume no random values.

Deadlines retain the shared `(deadline, ActorId)` ordering. Due timers precede
same-tick moving physics and work completion; a second timer pass handles physical
consequences without repeating a check. RNG, elapsed time, effects, events and paid
holds publish atomically or roll back. A full Reject-capacity condition book or other
invalid condition application rejects the transaction, including all sampled RNG.
Legitimate immunity and unchanged Keep applications remain successful no-ops.
A successful condition can pause a tied Eat
with zero remaining work while keeping its immutable nourishment and exact held unit.
Expiration never auto-resumes that work. Explicit Resume or a later successful
ordinary replacement remains required; Eat does not support explicit cancellation.

A triggered policy permits at most 1024 nonfatal empty checks per uneaten episode:
`-1025 <= starve_at < 0`. A draw costs one chance plus, on success, the existing
bounded amount pool; this is bounded random work, not an O(1) catch-up claim.
Disabled triggers retain the broader analytical nutrition envelope. Existing finite
conditions and actor limits bound additional timer work.

The provisional Rogue package uses initial 1300, maximum 2000, Hungry below 300,
Weak below 150, 100 active ticks per reserve unit, and death at -852. All hunger
bands retain Normal Health recovery. The same finite ration used by respec is
edible for `1d400 + 1099`; it is not a second supply balance. Fainting uses chance
1/5 and `(1d8 + 3) * 100` active ticks, target-wide Keep, VoluntaryActions denial,
Pause preparation and Remove expiry, with no cold descriptor or protection phase.
Rogue additionally authors one reserve unit per actual periodic Health, Stamina,
Focus or Mana point (`denominator = 1`, all four numerators 1). These rates and
the recovery intervals are provisional coupled tuning parameters, not calibrated
balance or compatibility promises. See the bounded
[comparison plan](../development/performance.md#regeneration-and-nutrition-baseline).

These arithmetic boundaries follow the pinned
[Rogue stomach daemon](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/daemons.c):
-850 → -851 survives and may check; -851 → -852 dies before RNG, leaving 851
possible nonfatal checks. Rogue runs that daemon after commands; TOR's existing
timer-before-completion ordering and the 100-tick mapping are explicit adaptations.
Food distribution, opponent pressure and full-run balance remain unqualified.

## Received Health loss and quiet time

A creature separately records actor-active quiet time still required since its
last actual positive received Health loss. Any positive applied damage, including
self or environmental damage, renews the full authored interval. Zero damage,
immunity, merely dealing damage and reductions to maximum Health do not renew it.
Full healing, regeneration and nutrition changes do not clear or rephase it.
Permanent-build changes preserve the interval and independently advance the build
epoch.

The countdown follows the existing lazy creature clock and earliest-deadline
index. Frozen, detached and dead actors consume no countdown time; reactivation
never earns catch-up progress. A required, bounded current-format record preserves
the remaining value. This is one scalar timer, not a damage log or a second
Health/resource ledger. The interval is separate from the recovery fraction:
full healing can stop recovery while a received-loss countdown remains active.

The required rules-owned `advancement` policy has four candidate timings:
`purchase_recovery = 0`, `respec_prepare = 600`, `respec_recovery = 0` and
`received_loss_interval = 600`. Each is bounded by 1000000 ticks; preparation and
the received-loss interval must be positive. These provisional actor-active values
are configurable, unscaled by Speed and not calibrated balance. Missing fields,
unknown fields and positional policy records reject rather than providing old-
format defaults.

The same policy requires one known `respec_ration` supply, quantity one, with
`replacement_commit` settlement. Its access may be carried-only or
carried-or-ground-touch. No client chooses the cost or preparation duration.
The default ration is carried-only; scenarios may author the wider access policy.

The owner-authorized advancement overview discloses the controlled actor's quiet
interval and perceived danger. Other actors' timers are not ordinary observations;
hidden preparations never contribute to this safety projection. See the
[player controls](../play/advancement.md).

## Advancement points and starting ranks

Rules define named `point_pools`, each with a display name and allowed purchase
kinds (`skill_rank`, `talent`). `constants.racial_point_grants` and each class's
`point_grants` declare `{ pool, points }` grants per hit die. A source may grant
multiple restricted or shared pools, or no points. Unknown pool references,
duplicate grants, empty allowed-kind lists, duplicate allowed kinds and grants
over the structural bounds reject before a game starts. Pool definitions are
identity-keyed: declaration order does not affect compiled semantic hashes. Grant
order is preserved because convenience purchase commands select the first
compatible unspent pool.

The default campaign provisionally grants one shared `advancement` point per HD.
One point buys one talent or one skill rank. This economy has not been accepted
as final balance. Purchased ranks, purchased talents and unspent points always
sum to each pool's grant; a purchase cannot borrow another pool's balance. There
are at most 16 grants and 256 total points per HD, with at most 256 total owned
talents across all HD (active and dormant combined). Extra grants can remain
unspent or buy ranks; granting points does not expand the ownership bound.
Construction, authoring, purchase/replacement and saved current/authored builds
enforce the same limit before producing an observation. Together with twenty
skills capped at rank five and two permanent starting ranks, this bounds paid
purchases at 354 and per-HD pool disclosures at 4,096. Attribute opportunities remain
separate, every four HD in the default rules.

Creation explicitly records exactly two distinct rank-1 starting skills, outside
the HD-owned purchases. They cannot be reclaimed by drain, multiclassing or
restoration. Starting and purchased ranks together cap at five. A purchase may
belong to an older unspent HD and uses current permanent eligibility. Existing
owned talents whose requirements cease to hold remain dormant; they spend their
owned points until their HD is removed. Inspection discloses each pool's grant,
spent and unspent balance, every purchase, the two starting skills, restoration
policy, and failed parent/requirement descriptions for dormant talents. Large
requirement descriptions are bounded summaries with an explicit omission marker.

### Fixed acquisition forms

A fixed form is an immutable `{ id, name, ability }` definition referencing one
ordinary ability. A talent may declare up to sixteen unique `form_choices`.
Acquiring such a root requires one `talent_form` selection containing both its
talent ID and a listed form ID; a choice-free purchase, an unknown form or a form
from another root rejects atomically. Ordinary roots reject form operands.
Either selection kind spends one talent point under the original HD owner, and
choosing another form cannot purchase the same root twice.

An active root contributes its chosen ability through the ordinary grant under
that root's source. Dormancy removes the grant without replacing the saved
choice. Eligibility restoration reinstates that exact form; explicit ownership
replacement can select another listed form. Records validate the operand even
while the root is dormant. Form grants share the source modifier bound, so a
choice-bearing root reserves one of its modifier slots. Forms add no separate
casting, timing, effect resolution or discovery subsystem.

Creature authoring, private inspection, owner catalog and exact player operands
retain the chosen form. The compiled rules save adapter requires explicit
`forms` and every talent's `form_choices`, including empty lists, and rejects
missing, null, duplicate, unknown or positional field shapes. Source authoring
may omit an empty `form_choices`; it cannot omit the root `forms` catalog.
Form catalog order is canonicalized by identity; a root's option order is retained.
The [advancement controls](../play/advancement.md) use the same exact operands
in text, headless and native ASCII clients.

### Restoration policy

`restoration = "choose_again"` appends an empty restored slot so a character can
choose again. `restoration = "authored"` saves the complete intended source and
purchase list per ordinal, independently of player/control ownership. Regrowing
an intended ordinal requires its original source and refills only that ordinal;
retained choices are never overwritten. An ordinal beyond the fixed authored
blueprint is empty when regrown. Saved current and intended sources must agree
for every overlapping ordinal, even when retained purchases have been edited. Neither path grants starting skills again or
consumes the combat RNG. Rebuilds retain injury, resource balances and persistent
death; regrowing Health capacity is not healing or revival. Authored restoration
can restore ownership of a still-dormant purchase. A retained edit may temporarily
block a future authored attribute or duplicate talent choice. The current build
remains saveable with its blueprint intact; the blocked restoration fails
atomically until the conflicting choices are resolved.

For example, a two-HD Mage with a spell and one purchased rank can declare:

```toml
starting_skills = ["spellcasting", "lore"]
restoration = "authored"
nutrition = { kind = "exempt" }
hit_dice = [
  { source = "mage", purchases = [{ pool = "advancement", selection = { kind = "talent_form", talent = "evocation_initiate", form = "spark_lance_fire" } }] },
  { source = "mage", purchases = [{ pool = "advancement", selection = { kind = "skill_rank", skill = "spellcasting" } }] }
]
```

The core ledger exposes read-only append-purchase and replacement previews using
the same validation as commit. Replacement targets one existing purchase by its
HD and purchase position, and either refunds it or buys one explicit replacement.
Its immutable preview discloses all removed purchases at their original positions,
gross refunds grouped by owning HD/pool, final point balances, and the resulting
active/dormant talent sets. The replacement spends one point separately, so its
net unspent balance can be smaller than the gross refund. Commit accepts only the
validated candidate and rejects a changed source build atomically.

Replacement removes the dependent **ownership** subtree to a fixed point. All
remaining owned alternatives count, including dormant ones: removing A preserves
a child requiring A OR C while C remains owned. If C is removed transitively too,
the child is refunded once. Explicit parent chains and convergent branches use
the same rule. Non-parent gates govern **activation** separately. Removing a skill
rank can make a talent dormant without refunding it; a temporary HD gate or an
unrelated pre-existing dormant branch does not enlarge the refunded subtree.
Users must explicitly replace a retained dormant purchase to recover its point.
Starting skills, attributes/opportunities, HD sources/seeds and the fixed authored
restoration blueprint cannot be rewritten by this transaction.

Previews return a permanent-build candidate and cached derived facts; equipment,
injury, clocks and reservations must be projected through actor state. Actor
revision, safety, time and supply authorization remain action-layer concerns.
Actor-owned creature state also retains a positive permanent-build epoch. A
successful change of the permanent build advances it once with checked integer
arithmetic; changing a build and later restoring equal values still advances it
twice. An identical-build rebuild, temporary conditions, equipment, Health,
resources and active time leave it unchanged. Failed rebuilds preserve the
entire state, and exhaustion rejects further build changes rather than wrapping.
The current creature record requires and preserves this epoch. It is a conflict
identity, not permission. The simulation purchase action carries an exact typed
HD owner, named pool and skill/talent choice together with actor, build epoch and
opaque host branch/stream/reset provenance. Admission requires an active, ready
actor with no active or paused preparation. Execution rechecks the actor and
epoch, reruns the permanent preview, and publishes one runtime-preserving rebuild
through the ordinary action boundary. Authored purchase recovery is checked for
overflow before spending. Pure preview uses a settled actor candidate without
mutating clocks, observations, queues, randomness or the game. Its typed runtime
consequences come from that same validated candidate: effective Health/death,
Health-recovery state, resource maxima/balances, resulting epoch, condition-removal
counts without private causers, and the number of canceled resource holds. These
consequences include equipment/temporary sources and the shared finalized death
state after equipment removal. Corpse-identity exhaustion rejects preview too.
The transient finalized outcome is published once; death planning is not repeated.
Permanent derived values alone cannot describe runtime clamps or death. Rebuilds preserve surviving runtime state
but can remove newly immune conditions or clamp capacities under existing rules.
The host must still authenticate the provenance and owner; these domain values
grant no authority.

### Timed ordinary replacement

An ordinary replacement freezes its exact source HD/purchase position, optional
replacement choice, original actor/build-epoch provenance, authored timings and
ration policy. Fresh admission requires an active, ready actor with no active or
paused preparation. Other paid work must finish or be explicitly canceled, and
its retained recovery debt must expire first.

Safety uses perceived hostile actors and the actor-active quiet interval since
actual positive received Health loss. It does not inspect another actor's
private queued actions or preparations. Visible danger, received damage and a
permanent-build change discard the attempt and release unspent ration holds;
a retry starts the full authored duration. Ordinary explicit suspension retains
remaining work and the original intention lineage. Only that exact paused
intention can resume. Frozen or detached time earns no preparation, recovery or
quiet-time progress, including when suspension or damage occurs while frozen.

The plan uses the same item-owned holds as other finite-cost actions. Neither
preparation nor cancellation consumes its ration. At completion, the simulation
rechecks live safety and epoch, reruns the permanent replacement preview, then
spends the held ration and finalizes/publishes the runtime-preserving actor
candidate within one transaction. Death finalization happens after spending so
a deleted last ration cannot enter the corpse. Any late failure rolls back the
whole operation, including payment, holds, timers, events and build epoch.

The replacement preserves current injury and legitimate recovery, resources
subject to their resulting maxima, equipment until ordinary death cleanup,
cooldowns and knowledge. New immunities may remove conditions, and reduced
maximum Health can kill through the single ordinary death boundary. Maximum
reduction is not received damage. Its read-only preview reports the finalized
runtime consequences and complete permanent dependent refunds; it is never a
stored whole-game candidate used later for authority.

Ordinary owner queries/actions and independent host save adapters enforce current
control for fresh access, exact authenticated receipt retry precedence and original
admission ancestry for accepted work. Private operations are excluded from ordinary
observer history and interaction preparation views. Text, headless and ASCII use
the same bounded owner pages and exact confirmation/resume operations. The
[earned-XP foundation](#earned-xp-and-future-hit-dice) shares these owner controls.
Instant wizard edits remain a separate privileged surface.

### Earned XP and future Hit Dice

The applied effect that crosses alive to dead owns the award. An ally's fatal
effect earns that ally's XP; earlier contributors receive none. Zero damage,
immunity, self aliases and unattributed environmental deaths earn no credit.
The existing death candidate validates corpse allocation and XP overflow, then
publishes death and its award in the same caller-owned transaction. Retry,
checkpoint and rewind restore death and progression together; no defeated-ID
history accumulates beside the lifecycle.

Actors declare `future_growth`: `disabled`, `selected` with an explicit racial
or class source, or `authored` using their restoration plan. Control ownership
never enables NPC growth. Earning a threshold immediately adds the next HD when
the declared source is available, preserving the original ordinal Health stream.
An authored ordinal restores its saved source and choices; ChooseAgain creates
an empty purchased-choice slot. An authored plan may extend past the current
Hit Dice as a growth schedule: the next new HD then takes that row's source and
choices, including for body-package characters, whose future rows are keyed by
the next personal HD identity. Existing ordinary previews spend those points.
The owner can preview and confirm a different future source through the same
authenticated branch/reset/BuildEpoch contracts as other advancement.

Drain retains the current XP balance and highest earned tier. HD 6 drained to 5
with 360 of a provisional 600 XP keeps that 360; 240 new XP regrows HD 6. Explicit
restoration first leaves the 360 toward HD 7. A completed threshold is spent
once and is never reused after another drain. Dead effect owners retain awards
without HD growth or resurrection. An active paid strike sequence retains its
frozen operands, resolved prefix, remaining tail and recovery through growth while its ability remains granted;
genuine grant dormancy/removal cancels only the unresolved tail and preserves the full existing commitment/recovery debt, with no cost refund or replayed prefix;
ordinary rebuild capacity, immunity and preparation consequences still apply.

Disabled, exhausted or temporarily inadmissible future sources retain earned XP.
Restoring an authored ordinal can temporarily conflict with retained attribute or
skill ranks, duplicate an owned talent, or exceed the owned-talent bound. These
ownership conflicts defer that authored row, including when a selected future
source follows an existing restoration plan. Clearing the conflict retries the
same held XP, source, purchases and ordinal Health seed. Empty newly selected
rows do not receive this authored-choice exception.
Current numeric derived bounds and override conflicts also defer the HD without
vetoing the fatal effect. These authored bounds are distinct from operational
allocation, checked-arithmetic and representation failures. Operational failures,
malformed builds and interaction-invariant violations reject the whole candidate;
no fatal damage, XP award, growth, inventory, random state or clock change from
that candidate commits. The host reports capacity and invariant refusals
generically to the current controller and retains the original queued execution
for deliberate retry or cancellation. Ordinary unavailable actions still use
their established failed-intention lifecycle. An interaction contradiction is
an invariant failure, never a reason to discard a condition or bank an award.
Automatic growth also defers rather than evicting an otherwise valid timed condition to fit a combined derived bound or override. Genuine immunity removals and explicit rebuild/respec reconciliation retain their normal semantics.
New XP or authoritative source-fold changes run one bounded growth pass at actor finalization;
ordinary damage, unchanged timers and reads do not retry an unchanged obstruction.
Recorded-only Health ledgers also retain XP, but cannot automatically add HD without a genuine saved root.
An exact analytical projection of that balance advances the recipient's comparison tier
even before its missing source is supplied. Actual HD growth uses the same
threshold ordering, without a separate credit bank or refund economy. Checked
arithmetic-series sums and bounded binary search project funded tiers without a
loop proportional to an untrusted saved tier. Tiers zero and one share the base
price; the provisional tier-65,537 price is exactly 6,553,700 XP. The
temporary actual-HD admission guard also retains XP. Earned, spent and held XP are checked `u64`
amounts with `earned = spent + held`; restored applied/comparison tiers must also satisfy their exact minimum spent/earned totals. Extra paid regrowth remains valid. Exhaustion rejects the whole transaction,
including its fatal effect, rather than saturating or discarding value.
This representational limit is 18,446,744,073,709,551,615 XP, distinct from the
actual-HD admission guard. Rules bound one award to at most 1,000,000 XP, so exhaustion
requires at least 18.4 trillion maximum-rule awards. The host reports only a
generic resource-limit error, without revealing another actor's balance.

Rules provide provisional integer threshold/reward parameters and permanent
source contributions, with bounded identity overrides. Assessment includes HD,
starting/purchased training, species, resolved type/subtypes, class, templates
and owned talents; equipment and temporary conditions do not contribute.
The cached victim threat peak is separate from the recipient's highest earned
tier and neither falls through temporary drain. Assessment runs at permanent
build boundaries, not on each damage tick or owner query. These category weights
are tuning inputs, not a validated CR model or balance claim; arena measurement
and ordinary Warrior/Mage HD 1–6 acceptance remain required.

A successful forced push can retain one bounded causal fall episode on the
displaced actor. A lethal impact in that continuing episode credits the push
owner through the same atomic death/award boundary. A later applied push replaces
the owner; a blocked push does not. Settled landing, voluntary displacement, teleport
or body reshaping retire the episode so unrelated later falls earn no credit.
Frozen motion preserves it without accruing activity, and journal/checkpoint
recovery preserves the episode without awarding twice. Items and corpses never
inherit actor fall provenance. See
[one-cell translation](../architecture/simulation.md#one-cell-ability-translation).

An off-floor causal owner is loaded for the award without thawing its floor or
granting catch-up activity. A still-detached owner blocks the transaction until
loaded; an actually removed historical owner leaves the impact unattributed
rather than preventing death. This is bounded causal state, not a contributor
history, unbounded pin set or deferred-death ledger. Damaging condition content
and its causal adapter remain future work; the typed condition origin alone
does not implement poison credit.

## Delivery and exposure defenses

Rogue distinguishes avoiding delivery from resisting a delivered exposure. Rays
and breath can be dodged with Agility through their ordinary opposed ray check;
wand magic does not gain an extra Spiritual or Discipline gate merely because it
is magical. Gaze and fear effects cannot be dodged that way and target Spiritual
defense. Explicit poison payloads instead use the shared Athletics resistance
policy after any required physical delivery hits, with immunity checked first.
This classification belongs to the authored attack/payload, not its display name
or the condition it may apply. Self-read magical sleep retains its existing
behavior; it is neither poison nor implicitly granted a Discipline check.

## Locomotion capabilities

The shared `locomotion` modifier carries one typed value: `stationary` or
`flying` or `levitating` with an explicit `enabled` boolean, or `erratic` with a required
nullable `chance`. A probability has unsigned 16-bit numerator/denominator,
nonzero denominator and numerator no greater than denominator. Effective
capabilities derive from ordinary active sources, never species identity.
TOML creature/item authoring omits `chance` for suppression because TOML has no
null; lowering emits the explicit nullable field required by canonical saves.

Positive stationary/flying/levitating grants combine by OR. Any explicit false suppresses
that capability across all sources. Erratic grants choose the greatest exact
rational probability, normalized to lowest terms; any null suppresses erratic
movement. Scoped modifiers do not silently change these actor-wide capabilities.
Removing a suppressing source exposes remaining grants through the normal fold.

Stationary denies every voluntary translation, including ordinary movement,
AI pursuit/retreat and self-step abilities. It does not anchor a body against
forced movement. Flying permits paid unsupported and open-vertical translation
with ordinary body clearance, portal rotation and scenario stair policy.
It gives the creature zero effective gravity without erasing drift. Capability
changes reset only acceleration remainder when the actor publishes; they do not
teleport, brake or create fall credit. Items never inherit the grant.

Levitation separately grants horizontal hover and suppresses actor gravity, but
never grants voluntary Up/Down control, ceiling ascent, ground pickup or abstract
stairs/terminal surface escape. Independent flight can supply vertical control;
levitation's ground-interaction restrictions still apply. Dropped items retain
ordinary motion and gravity. Ground trap contact is suppressed except for real
imposed collision. Losing hover over support establishes contact once; over a
hole it resumes physical falling without a safe snap or grace period.

The shared `arrest_descent` effect is distinct from the capability: a successful
levitation drink applies it at paid completion, not preparation start. It removes
only descending ambient-gravity velocity and obsolete fall attribution, preserving
tangential velocity. For oblique aggregate gravity, integer projection must remain
non-descending and bounded. If projecting the subcell fraction would exceed its
cell-local bound, that fraction is discarded rather than publishing an invented
cell crossing or impact. Repeated doses can arrest a newly imposed descent. Earlier
impact/death is not undone, and ordinary flight does not gain braking.

An erratic choice can replace a fresh attack, pursuit, retreat or idle decision,
after ordinary self-care and without replacing retained paid preparation.
It chooses only legal movement disclosed to that actor, then submits an ordinary
paid Move. Zero chance, stationary/restricted movement and an empty candidate set
draw no randomness. Preview uses a copied actor-owned behavior RNG; only a
committed candidate publishes the draw, so rejection and reopen do not reroll.
This is the TOR action-priority adaptation, not a claim of exact Rogue turn parity.
See [movement](movement.md) and [physics](../architecture/geometry.md).

## Abilities and effects

An ability defines cost, timing, reach, check, success/failure effects, descriptors,
validated ability tags and AI hints. Tags are named ruleset content IDs with
preindexed membership. Modifiers may target one ability or every ability carrying
a tag. The narrower `weapon_attack` tag supports weapon-specific modifiers;
the `attack` tag also includes offensive spell checks and qualifies Read the
Rhythm's source-owned token. Defensive self-actions and observation are not
attacks. Added abilities opt in through ordinary authored tags. The derived cache keeps the original tagged
modifier and its source for inspection; resolution consumes the per-ability values. Basic melee is an ordinary disclosed ability. Effects are damage,
healing, resource restoration and condition application; items and physics use the
same resolution machinery. Fixed amounts do not consume RNG; rolled amounts use
the deterministic bounded dice kernel. Canonical tuning lives in
[default-ruleset.toml](../../crates/simulation/data/default-ruleset.toml); scenarios import it and can declare
strict, identity-keyed overrides. Ability tags select families of abilities for
modifiers without enumerating each technique ID.

`heal_per_hit_die` captures its dice count at preparation and compares resolved
healing with current injury. Its explicit overflow policy is `none`,
`improve_earliest_hit_die` (one credit for strictly positive excess), or
`improve_earliest_hit_dice_by_excess` (one credit for positive excess, a second
only when excess exceeds the captured HD count plus one). At most two ordered
unit improvements are selected from eligible active owned acquisition rows;
the same earliest row can receive both, up to its original die maximum.
Resolution retains the exact bounded prefix and build epoch. Application
validates the entire prefix before changing Health or the ledger; it does not
resample dice or choose replacement rows. No eligible row means healing only.

`hold_damage` is a hostile melee primary-success payload bound to one exact
linked hold. Its category is required; TOML authoring omits `descriptor` for
unmarked damage because TOML has no null value. Stored content and frozen plans
instead require the explicit nullable descriptor field. An initial grab has no
damage payload. Later executed qualifying primaries advance the frozen
`hold_damage_increment` once, including a miss; damage applies only on success.

### Creature behavior and provocation

Every ability requires source-owned `interaction = "non_hostile"` or
`"hostile_to_target"`. AI priority and offensive-selection hints are separate.
Non-hostile authored primary branches and riders cannot contain finite direct
harm: positive damage, forced target displacement, operational restrictions,
preparation pauses, or harmful signed condition modifiers, including through
validated, once-per-flag expansion. Health or resource-capacity Sets at or below
zero are direct killing/depletion effects; positive recipient-relative Sets and
check-role-dependent Difficulty changes do not imply a universal harm direction.
Source-only benefits, target ownership, names and arbitrary future consequences
do not infer hostile intent. Selected-weapon melee requires hostile interaction;
other composed sources validate all inherited and declared rows. Current unlocked
operands are checked again after each source/modifier fold, using the same authored
rolled-component and natural-primary modifier scope as capture. Equipment and
condition changes cannot turn a non-hostile ability into positive direct damage.
Frozen plans validate their captured effects against the pinned interaction on
admission and restore, without re-deriving today's equipment. Hit-die drain and theft are
direct harm; restoring a drained die is not. Hostile intent does not guarantee an
effect.

Each committed resolved strike provokes at most once through its existing ordered
cursor and transaction. Misses, zero-damage contact and attempted target-involving
riders count; unavailable targets, cancelled work and uncommitted preparation do
not. The target can remember a source only when the ordinary resolved event
identifies its caster and a matching visible-body occurrence supplies a licensed
last-seen location. A partial body with an undisclosed caster grants no identity.
No new preparation cue or public provocation field bypasses paid plan inspection.

Species own `passive`, `mean` or `greedy` acquisition, `wait` or `explore` idle behavior,
`memory_ticks` from zero through 1000000 inclusive, and defaults for door use,
care, equipment, useful loot, Gold interest and fleeing. Omitted behavior resolves to passive
exploration without tool use and a 25% Health flee threshold. Named behavior
records and the independent concrete saved policy enforce the same bounds;
restoration does not use authoring fallbacks. Tactical profiles partially override
supplied fields without replacing acquisition or memory; see
[behavior authoring](scenario-packages.md#behavior-defaults-and-ai-overrides).
Greedy acquisition follows Rogue's greedy monsters: while the species can
safely collect known Gold under its Gold interest (no visible hostile beside
it, and a short disclosed route), it collects; otherwise it acquires a visible
hostile exactly as mean acquisition does. Without Gold interest it is mean.
Mean acquisition adapts the pinned
Rogue reference's two-in-three chance to one committed eligible TOR AI decision,
using that actor's current perception. This is a TOR cadence adaptation, not an
extra clock or a claim of identical Rogue action/RNG timing. Queries, preload,
frozen time, decisions without a visible eligible opponent and retained paid
work consume no wake draws. A frightened owner can choose an already-approved
ordinary escape from a currently visible threat while preserving the existing
atomic step and abandonment-debt rules, without fresh engagement or wake RNG.

One engagement, its required licensed location and a zero-valid deterministic
random stream belong to the actor. They survive controller reconfiguration and
save/restore. Memory expires on the existing actor-active timer index, freezes
with its owner and remains bounded by the pinned species policy. Contextual
Health recovery settles the pre-expiry interval at its old rate and rephases the
post-expiry interval once. Nutrition uses only the final positive interval's
time-depletion result; a zero-length suffix must not erase an exact-boundary
trigger. Source/controller changes cannot manufacture catch-up recovery.

Allied targeting uses authored faction relationships and the requester's own
engagement, never the other actor's private grudge. Directed faction edges still
retain their authored meaning. Species defaults and partial AI profiles select
generic door, care, equipment, useful-loot and Gold-interest tactics; inventory ownership alone
grants neither permission nor identity knowledge. Explicit Wait prevents idle
movement and loot detours, but permits enabled useful underfoot actions. It does
not change hostile acquisition, remembered pursuit or combat. Exploration and
loot goals use only the actor's own senses and known routes/items. Gold pursuit
has its own explicit policy: self-preservation and retaliation retain priority,
then known reachable Gold takes precedence over ordinary idle behavior. It uses
ordinary currency stacks, never unseen stock or an inferred purse.
Equipment- and loot-enabled actors can acquire an identified launcher and matching
ammunition from their inventory or currently disclosed safe loot within three known
route steps. Pair lookahead examines at most 128 inventory rows, 128 ground rows
and 16 launchers; an inventory beyond the bound declines ranged acquisition rather
than undercounting stock. Compatible ammunition has a provisional six-shot reserve,
counted across carried stacks. Known shot costs must fit the loadout's resource
maxima; collection may wait for ordinary resource recovery. Equipped launchers
precede carried ones and ground candidates, with stable item-ID ties; the policy does not collect spare launchers.

An owned hostile Launch ability with strictly higher authored AI priority than the
basic melee ability permits a melee-for-range equipment trade-off. Equal priorities
retain the conservative policy: adding range must preserve the existing melee score.
Expertise supplies only ordinary capability and costs, never a tactical preference.
Every prospective draw checks real anatomy and current capacity after removal;
health, protection, recovery and other capabilities remain protected. A loaded
preferred launcher is retained rather than immediately replaced by a stronger
melee weapon. Once carried ammunition is depleted, ordinary safe gear improvement
can restore melee, including releasing an empty secondary launcher when it blocks
a strictly better carried primary weapon. Neither this heuristic nor item ownership
grants free equipment, expertise or ammunition. Gearing and collection retain the existing safe Search-only
precedence; combat continues to use authored priorities, disclosed alignment and
range, normal resource checks, and remembered pursuit, without kiting.

Pair selection reads identified structural families, not hidden projectile numbers.
Equipment comparisons use remembered instance facts or ordinary defaults on both
sides. Candidate projections reuse one private workspace; they never compile a
projectile against an opponent to choose loot. Unknown curses are learned only by
ordinary execution. The six-shot limit and structural first-choice heuristic are
provisional TOR policy, not a claim of optimal combat or calibrated balance.

These reusable mechanisms do not activate the full Rogue roster or establish
numerical balance. Owner-disposition inspection is a separate presentation pass.

### Autonomous self-defense

A currently sensed hostile can prompt an owned, active nonhostile `ActorTarget`
ability with effective `SelfOnly` reach. The initial policy recognizes one
unconditional condition application, with no check, failure branch, rider or
other effect. The condition must reject capacity overflow, must not stack or be
source-scoped, and may contain only always-active negative incoming check-edge
Adds or positive talent-category reduction. Other payloads are not inferred to
be safe from their names, class, talent or display text.

Healing, a valid retreat and a committed strike prefix retain priority. Candidate
order is authored AI priority, then ability ID. The actor compares only its own
current protection and the disclosed threat identity: equal family edge, equal
or stronger talent-category reduction, own immunity, a full condition list and
reserved matching-family tokens prevent wasted reapplication. Ordinary reduction
still adds to the strongest talent-category protection. An active equal condition
is not refreshed even on its last tick. Hidden opponents and remembered locations
do not justify proactive protection; enemy damage categories and immunities are
not consulted.

The scan captures one immutable own-family comparison baseline and the existing
single disclosed decision scene. It neither reserves resources nor applies
conditions speculatively. Selected abilities retain the ordinary immutable plan,
resource/supply/cooldown checks and physical admission. A valid exact paid pause
resumes its original plan and holds after healing/escape obligations, without a
new start charge. Expiry uses ordinary actor-active time; there is no maintenance
timer. This policy makes eligible authored abilities usable by AI, but does not
establish their numerical balance.

### Trickster theft

TR1 Filch Gold and TR2 Filch Charm use the shared hostile checked-rider effect,
not a parallel species combat executor. The currency selector captures a checked
positive quantity range; the magical selector captures the one-free-unequipped-
unit policy. A theft effect must be the rider's sole payload and its separate
Cognitive check must succeed. Theft is not permitted in a primary branch, failure
branch or consumable effect.

TR3 Vanish contributes an active source flag used to compile the optional
same-floor relocation gate when the plan is captured. The frozen gate survives
ordinary paid pause/recovery without consulting later source changes. Only a
positive actual transfer permits relocation; an attempted theft is not enough.
Eligibility, live inventory/held-unit validation, conservation, knowledge and
ordinary death recovery are owned by [the item contract](items.md#theft-and-gold).
No extra roll, item menu or disclosure rule is inferred from the talent's name.

### Finite supplies and universal care

Finite supplies are named rules catalog entries (`id`, `name`), initially bandage
and ration. An ability may declare `supply_costs`: at most eight positive quantity
rows, ordered uniquely by `(spend_at, supply)`. Each row selects `carried_only` or
`carried_or_ground_touch` access. Abilities require the `first_resolution` spend
stage; `replacement_commit` is reserved for replacement actions. Quantities and
positive optional cooldown intervals are bounded by the structural numeric limit.
Authored rules may omit `cooldown` and `supply_costs`; the independent compiled-save
adapter requires both fields, with explicit null for no cooldown and an empty
array for no supply cost. Missing owned fields do not load as an older format.
Runtime supply selections, payment states and their changed action, preparation,
item and creature containers also require named records. This raw typed boundary
is independent of the save reader's canonical-shape comparison.

Bind Wounds is a universal default-granted healing ability declared in ordinary
rules data. Its provisional candidate values are two Health, 300 preparation ticks,
100 recovery ticks, a 1,200-active-tick cooldown, and one bandage at first resolution.
Its selector permits self or allied touch. An initial ordinary-Engine measurement
retains these configurable values as a conservative candidate, not final balance
or the stronger benefit of the planned E01 talent.

With seed 42, half-Health initial injury, unchanged natural recovery, and zero
versus three starting bandages, the [ordinary Engine tests](../../crates/server/tests/it/supply_lifecycle.rs)
measured these actor-active ticks:

| HD | Rest to full, no bandages | Rest to full, three bandages | Bandages spent | Explicit / natural Health restored with bandages |
| --- | ---: | ---: | ---: | ---: |
| 1 | 4,800 | 1,500 | 2 | 3 / 1 |
| 6 | 13,000 | 7,000 | 3 | 6 / 7 |
| 11 | 16,800 | 12,000 | 3 | 6 / 15 |

The no-bandage controls restored 4, 13 and 21 Health naturally. Under an ordinary
hostile AI delivering one fixed damage every 200 ticks, the 300-tick preparation
was interrupted: HD 1 made one attempt and died after 600 measured ticks; HD 6
and 11 each made two attempts during 2,000 ticks, finishing at 3 and 11 Health.
Every pressure case spent zero bandages and restored zero explicit or natural
Health, with the same health outcomes as its no-bandage control. Pressure began
from its own post-injury snapshot, including the hostile's initial attack.
These are deterministic authored-pressure and finite-stock samples, not a broad
encounter or talent-balance study. Reproduce retained boundary observations with
[`scripts/measure_bind_wounds.py`](../../scripts/measure_bind_wounds.py).

### Paid public-plan observations

The shared `observe_public_plan` effect records a visible, living selected
opponent other than the user. Ordinary Sight eligibility includes neutral
relationships, as manual attacks already do. Preparation exposes only an
authorized public ability plan: exact remaining actor-active work, a frozen
public aim, and an ability name only when the observer previously recognized it.
Hidden aim is unavailable rather than reconstructed from a target's live location.
Idle, recovery and private/non-ability work share `no_public_plan`; distinguishing
recovery awaits separately licensed public readiness evidence.

`apply_qualified_condition_to_source` can bind the existing outgoing Readied
instance to an opponent and an authored ability tag. Both Edge contribution and
reservation require that opponent, the tag, and the structural ability-user
check role. Wrong opponents, nonattack checks and defender opposed rolls receive
neither the bonus nor a claim. Ordinary Readied and incoming Exposed keep their
existing semantics. Renewal, reservation, expiry and one-unreserved-family rules
still apply; no second Readied family is introduced.
The qualified effect is valid only for an outgoing `next_check`, non-source-scoped
condition. Readied and Distracted qualify; timed Braced/Fear and incoming Exposed
do not. Ability branches, riders, natural attacks and item sources use the same
authoring rule and reject ineligible conditions before action admission.

The accepted Read the Rhythm content contract is base 100/100 Speed-scaled phases,
Focus 1 at preparation and 1 at resolution, Sight 6, and Readied for the next tagged
attack against that opponent within 300 active ticks. Its acquisition requires
HD 6, Warrior 3 and active Measured Cut or Rattling Blow. A successful paid
record persists until a later paid observation replaces it. Pause, occlusion,
later recognition and free reads do not refresh it; saved history and rewind
retain the corresponding record and knowledge. See the
[historical wire projection](protocol.md#read-only-spectators) for entitled
observer scope and client-facing privacy.

Selecting a foe does not make an otherwise unseen source-only action perceptible
to that foe. Disclosure follows an actually attempted target payload or a
target-owned/target-defense/opposed check, while visible participants retain their
normal event. Personal-difficulty source effects and dormant target branches do
not license an ability name to a hidden selected subject. The same rule feeds
committed recognition. Saved disclosed aim addresses remain valid after wall
changes or region detachment, but reject unknown regions and out-of-bounds cells.

### Shared ability rules

The following values describe the shared default rules, not hard-coded engine
constraints. A validated scenario override can change them.

The bundled catalog contains seven Warrior nodes, three independent Mage roots,
and two shared recovery nodes. These twelve talents replace the former permanent
Health, pool, flat-defense, damage and ward chains. Health comes from HD and
pools from attributes; the generic modifier operations remain available for
explicit authored species, equipment and other content. There are no legacy
runtime aliases or automatic higher-damage spell upgrades.

Every talent costs one shared point. Gates read permanent build facts and active
owned parents; racial HD and temporary equipment do not substitute for class or
rank gates. All values below are provisional pending the paired starter study.
The timings are preparation/recovery before the ordinary Speed scaling. Weapon
means the selected weapon's phases; Guarded Step also pays ordinary step time.

| Talent | Permanent gates | Paid action or effect |
| --- | --- | --- |
| Brace for Impact | HD 1, Warrior 1, Discipline 1 | 100/100, Stamina 2; Braced for 300 ticks |
| Measured Cut | HD 1, Warrior 1, Heavy or Light Weaponry 1 | Weapon phases, Stamina 2; normal attack ignoring only the owner's outgoing negative edge |
| Rattling Blow | HD 1, Warrior 1, Heavy or Light Weaponry 1 | Weapon phases, Stamina 2; normal hit and Rattled for 200 ticks |
| Guarded Step | HD 6, Warrior 3, Brace or Measured Cut | 100/100, Stamina 2; ordinary one-cell self-step then Braced for 200 ticks |
| Read the Rhythm | HD 6, Warrior 3, Measured Cut or Rattling Blow | 100/100, Focus 2, Sight 6; paid public observation and opponent-qualified Readied within 300 ticks |
| Dislocating Strike | HD 6, Warrior 3, Brace or Rattling Blow | Weapon preparation +50/recovery, Stamina 3; normal hit then one-cell push away |
| Open the Guard | HD 11, Warrior 6, Guarded Step or Dislocating Strike | Weapon phases, Stamina 3; normal hit then Exposed for 200 ticks |
| Evocation Initiate | HD 1, Mage 1, Spellcasting 1 | Acquire exactly fire or cold Spark Lance; 100/100, Mana 2, Sight 6; Spellcasting vs Cognitive, 1d6+1 matching-descriptor energy |
| Binding Initiate | HD 1, Mage 1, Spellcasting 1 | Acquire the fixed Falter form; 100/100, Mana 2, Sight 5; Spellcasting vs Spiritual applies Fear for 300 ticks |
| Warding Initiate | HD 1, Mage 1, Spellcasting 1 | Acquire exactly impact, keen or energy Shell; 100/100, Mana 2; self-only category reduction 2 for 400 ticks |
| Steady Recovery | HD 1, Survival 1 | Health recovery period −25% while no hostile is sensed |
| Hardy Recovery | HD 11, Survival 2, Steady Recovery | Damage-triggered quiet reset −50%; does not shorten the recovery period |

Resource totals split one point at preparation and the remainder at resolution,
including failed checks. A blocked push adds no damage. Open the Guard cannot
consume the Exposed token it creates on that same hit. Falter's source-scoped
outgoing disadvantage does not force fleeing, slow movement or disable pursuit.
Shell contributions share the strongest-per-category talent reduction bucket;
independently authored species and equipment protection retain their usual fold.

The six immutable forms are `spark_lance_fire`, `spark_lance_cold`, `falter`,
`shell_impact`, `shell_keen` and `shell_energy`. Each Mage root has no direct
ability grant: its one explicit acquired form owns one ability. Even singleton
Falter uses the exact `talent_form` operand. Owning one variant never grants its
siblings; drain and restoration retain the recorded choice. Ordinary paid
replacement is the way to change that choice.

Magical creatures have Mana capacity `2 + binding`. Mana recovers one point per
800 active ticks, so a two-point cast needs 1,600 ticks of regeneration after
its initial reserve is spent. Capacity changes extend that reserve without
reducing the ongoing cost. Bind Wounds retains its fixed unscaled timing and
finite bandage cost. Mental-attribute timing and AI learning from Fear outcomes
are not implemented; current timing scales with Speed and AI uses declared hints.

The [starter arena study](../development/arena.md#starter-allocation-study) and
ordinary earned-HD/pressured-escape acceptance remain separate evidence gates.
Catalog availability does not establish useful choices, class parity or complete
Rogue content. Shared ordinary retreat and paid cancel-and-step provide escape
authority for both classes; there is no extra Mage movement node.

A `directional_ray` invocation selects one of the ten ordinary movement directions,
with no actor target, destination or predicted collision path. Its bounded `sight`
range supplies the clear-visit budget. Preparation captures the source's reference
cell, body frame, selected direction, source skill/check/effect operands, token
bindings, costs and timing. Release discovers recipients through current geometry;
recipient skill, protection and eligibility are live when an encounter occurs.
Source-side outgoing folds use unconditional and ability-qualified source modifiers
and exclude opponent-qualified condition sources, retaining ordinary family and
talent cancellation. A future recipient never becomes a substitute launch target
for `versus_source` or watched-context source bonuses. Source-side incoming
modifiers are captured separately; their `versus_source` predicate is evaluated
against each actual recipient. A launch performs one frozen named-skill roll.
Every live recipient performs the authored named-skill check against that same
total; ties succeed. The only supported shape is one target-owned opposed named
check, no success effects, and explicit component damage on failure. There are no
riders, unlocks or selected-weapon payloads in this invocation.

Each clear physical cell spends one range visit. A solid wall or closed door
reverses the whole direction without spending a visit; an open door passes.
Unavailable or ambiguous physical continuations stop the ray. Independent limits
of 64 probes and 32 reversals terminate no-progress geometry. Physical portal
rotations rotate the direction; sight-only images and abstract stair links do not
carry rays. The source's frozen launching footprint is suppressed until the ray
exits it, after which a returning ray can hit the source. A contiguous pass through
a multi-cell body performs one check. Exiting and re-entering permits another;
each previous actual check by that recipient against this release adds one
Disadvantage before ordinary edge cancellation and the three-extra-die cap.
Success deals no damage and continues; failure applies the full frozen damage
through ordinary current protection and intercepts. Repeat counts are bounded
release-local state, not persistent identities.

Preparation holds applicable source tokens; a source incoming one-use token
contributes to the first actual recipient check. Live recipient tokens are chosen
and spent atomically only when their check occurs. The common preparation
transaction owns payment, supplies, cooldown, recovery, all rolls and effects.
Public historical aim discloses only an observer-frame direction, and AI chooses
from disclosed exact directional alignment without previewing collision. Optional
combat diagnostics attach the shared launch receipt to each real recipient
encounter; an empty traversal has no fabricated recipient diagnostic.

With bounded Sight reach, `directional_contact_ray` uses the same frozen launch,
opposed named-skill check
and physical traversal, but stops at the first creature body whether it dodges,
is immune or receives the effect. A wall or closed door stops it without bounce;
later creatures receive no check or effect. Its failure branch may contain the
same component damage or ordinary finite/persistent condition application and
condition removal. Success effects, riders, unlocks and selected-weapon payloads
remain unsupported. Delivery is captured explicitly and validated against the
pinned invocation on restore; current visibility never predicts collision.

`directional_contact_ray` also accepts `{ kind = "until_obstruction" }` reach.
It captures a closed first-body traversal independently of its closed resolution:
either the existing opposed named-skill operands or a checkless Cancellation
payload. Checkless delivery contains no dummy skill, check token or roll.
The supported checkless source has the sole `cancel_specials` success effect,
no check, failure branch, riders or unlock, and hostile target authorization;
that authorization does not add attack provocation or a stealth contest.
Selected-weapon payloads and bouncing UntilObstruction rays are unsupported.

Traversal uses World ray steps and their returned orientation. Preparation
loads known required frontier geometry without activating frozen actors;
missing preparable geometry is not treated as an empty successful shot. Cycle
detection includes location, direction and whether the ray has left the launch
footprint; a later physical return can hit the caster. The operational guard is
65,536 visited states. Exceeding it is an atomic resource/work failure, not an
advertised range, truncated success or fabricated miss.

Source `AbilityDef`, modifiers, strikes and riders accept `cancellable = true`;
omission defaults to false. Cancellation retains an explicit saved creature
state, suppresses only marked effective contributions and reconciles ordinary
lifecycle effects. It does not delete owned progression or rewrite frozen plan
provenance. Current AI selection and execution use the same effective payload;
ordinary previews expose timing/counts, while privileged authored-source
inspection and historical observations remain canonical.
Current save fields are required; omission is not an old-save compatibility path.

Actor-target and self-step preparation freezes one bounded ordered strike plan.
Earlier queue admission stores the request, not the plan. Each ability authors strikes
or selects the current melee source. Every strike-source variant is a named,
`kind`-tagged record; positional arrays are rejected at the typed source boundary.
Both authored abilities and natural sources
may unlock later rows through active owned talents. A dormant owned unlock omits
its row from a new preparation without changing the remaining authored ordinals.
Restoring eligibility includes the row in subsequent preparations. The captured
list of an existing preparation does not change when an unlock later becomes
dormant; losing the whole ability grant still follows ordinary cancellation.
At most 16 strikes and 8 riders per strike are allowed, with aggregate limits of
256 effects and 256 descriptor entries across both branches and riders.
The first offset is zero and later offsets are nondecreasing. Speed scales
nonzero offsets directly, rather than accumulating separately rounded gaps.
The complete scaled preparation/last-offset/recovery horizon is bounded.

Check bonuses, thresholds, reach, costs, durations, effect values, dice expressions,
source provenance and token bindings remain frozen. Rolls occur only at resolution.
The cursor is separate mutable preparation state. Start costs are paid once; the
first due strike pays the reserved remainder and clears the hold. Later strikes
and resume never pay again. Suspension keeps unfinished active work and the cursor.
Before the first strike, cancellation releases unpaid costs. After a prefix it owes
the remaining timeline plus recovery, even if later targets are unavailable.

For preparation 10, offsets 0/10/20 and recovery 20, strikes occur at 10/20/30 and
readiness is 50. In-place cancellation at 15 keeps readiness 50; one valid escape
step costing 100 makes readiness 150. Pausing at 15 and resuming at 25 moves later
strikes to 30/40 and readiness to 60. If still paused, cancellation at 35 gives
readiness 70. Invalid escape is atomic and free of state changes. Ordinary movement
geometry, collision, effects and costs apply to human and AI escape alike.

At each due strike, live life and reach are checked. A later unavailable target
produces a no-RNG miss while keeping commitment; returning before a later strike
can make that later strike legal. The primary resolves before ordered riders.
Each rider requires `trigger = "success"` or `trigger = "failure"`, referring to
the primary roller's selected branch, regardless of whether user or target rolled.
A failed resistance can therefore select failure riders; a positive target check
can select success riders without inversion. A checkless primary selects success
and cannot author failure riders. Whole-primary immunity selects neither branch.
Zero applied primary damage still permits matching riders; lethal damage stops
later mutations. Each rider sees the live state after earlier effects, with frozen
trigger/check/effect operands. Checked riders are
attacker rolls against a captured defense and have no independent timing, cost,
retargeting or recursive riders.

A success rider may author `condition = { kind = "sequence_successful_hits", minimum = 2 }`,
with a minimum from 1 through 16. A minimum of 1 is the generic once-after-any-hit contract. It counts only executed, successful user-rolled
primary checks that structurally involve this preparation's target. Component
protection reducing damage to zero still qualifies; misses, unavailable targets,
whole-primary immunity, checkless/personal-difficulty checks, target saves and
rider checks do not. The current primary must qualify. A later miss cannot trigger
a condition merely because earlier hits met its threshold.

Each conditional source occurrence attempts once per preparation, immediately in
ordinary rider order when eligible. An appended `natural_melee` rider shares that
occurrence across all imported rows; a row-local rider retains its own occurrence.
The receipt is consumed before its own check/effects, including a failed check or
immune/zero-damage payload. A lethal primary or earlier rider can prevent the
attempt. Pause, resume and region freezing preserve the bounded hit history and
receipts; cancellation and a new preparation start fresh. Clients show only the
authored condition, never hidden progress or eligibility. Ordinary source TOML
may omit the condition. Current frozen, compiled and wire records require its
field explicitly, including null. See [preparation records](../architecture/persistence.md#ordered-ability-preparation-records).

Primary, riders, items and environmental effects share one pure check/effect
pipeline and one effect-state application kernel. A strike's primary and riders
share one residual damage-edge budget. Fixed effects preserve unused edge.
Outcomes record actual applied amounts, not overkill or unapplied healing. Death
cannot be undone by later healing. Optional diagnostics use the same pipeline and
do not change RNG. Authoritative publication uses actor-local candidates within
the existing outer transaction, without a Game clone per rider.

All creature lifecycle changes produce actor-local outcomes. One application
boundary commits them, reconciles preparations, performs death cleanup and emits
combat events. Equip/rebuild, resource recovery, condition expiry and item effects
use that boundary as well as completed abilities. Resolver preview and commit
share one effect-state kernel; this adds no whole-game clone to an action.

## Conditions and active time

Conditions define a family, lifecycle, keep/refresh/longest/extend/bounded stacking
behavior, source scope, descriptors, explicit finite or until-removed timing, operation
restrictions, preparation response and a capacity policy. `keep` preserves the first
application's remaining time and provenance without refreshing or stacking. `extend`
adds another application's duration to the remaining time of the same single entry,
retaining its original causer; exceeding the timer bound rejects without changing it.
Each family contributes at most
one signed edge to a check, after selecting applicable causers. Fear and Rattled
penalize checks against their source; Braced penalizes incoming Physical checks.
Multiple instances and aliases in one family cannot multiply that edge. Talent
edge contributes at most +1 and −1 before other sources combine. Ability-scoped
check edge is selected before that clamp. The loader rejects flat check-bonus
modifiers on talents and conditions, including through implied flags. Their
edge grants must use `add`: `set` is rejected for shared, global check-only,
ability/tag and incoming edge, directly or through flags. There is no override
precedence across capped groups. Permanent species/equipment modifiers retain
ordinary flat accuracy and typed-stat `add`/`set` precedence.

Readied grants the holder's next check advantage, Distracted gives that check
disadvantage, and Exposed grants the next check against the holder advantage.
They reserve at preparation start, never at queue admission. Each bounded claim
binds the first qualifying initiating, opposed or rider check site in strike zero.
If strike zero has no qualifying site, the token remains available for a future
preparation; it cannot move to strike one. Reservation spends the token even if
its site misses, is skipped by death or is cancelled. Pause/resume preserve the
binding. Reserved entries cannot refresh, stack, be evicted or be rewritten,
even after expiry. Completion/discard retires their private noncontributing records.
A token applied by an effect cannot benefit an already-prepared check.
An outgoing policy that removes every applicable contribution of a token leaves
that token unclaimed. A mixed-sign token with a surviving positive contribution
is claimed at its first eligible role and consumed normally. Incoming tokens and
tokens on the target's roll role retain their ordinary qualification; no ignored
penalty is reserved solely to consume it.

For each strike, `edge` is a signed shared budget S, distinct from combined check
edge Q. Global/ability/incoming check edge and one-use tokens affect checks only.
The first structurally authored attacker check reserves clamp(S, −3, +3) before
primary damage, even if that owner is a later rider skipped by lethal damage or
bound to the unchosen primary branch. Its token claim and shared debit are not
refunded or transferred to a later matching rider.
Damage receives D = S − clamp(S, −3, +3), or all S when no attacker check exists.
No skipped owner refunds its reservation. Check Q folds the full S with applicable
check-only sources before the final dice clamp: S=5 and check-only −6 gives Q=−1
and D=2. Later attacker checks select only scoped check-only sources before family
and talent caps; they do not receive another S. Primary damage and riders consume
D once in canonical order. Shared talent damage edge is not family/check capped:
a talent's shared +5 still reserves three for its check owner and leaves two for
damage, although talent check contributions retain their own signed cap.

Other temporary stats use ordinary unconditional sources, apart from the explicit
perception-conditioned Health period scope described above. Unsupported scoped
operations fail validation rather than being silently ignored.

Finite timing is a required named `{ kind = "actor_active", on_expiry = ... }` record.
Its end policy is either `{ kind = "remove" }` or
`{ kind = "protect", duration = N }`; active application and protection durations
are positive and bounded by 1000000 ticks. Protection replaces the active phase
in the same book entry, carries no modifiers or restrictions, and rejects
applications from every source without changing its original causer or deadline.
Expiry and newly gained matching descriptor immunity use the same end transition;
repeated immunity reconciliation never restarts protection. Death clears entries
without granting protection. Administrative changes to mechanical definitions clear
unreserved applications before the new modifier fold; display-name-only changes
preserve them. A changed definition still owned by a reserved token rejects the
rebuild atomically rather than removing or reinterpreting its paid claim.

Alternatively, `{ kind = "until_removed" }` requires lifecycle `timed` and
stacking `keep`. It has no expiry deadline or protection phase; elapsed time,
freezing and reopening cannot expire it. Ordinary explicit removal, immunity,
death and reconciliation retain their existing authority. Persistent application
uses `apply_persistent_condition`, not a zero or sentinel finite duration. Its
optional `cancels` names a different until-removed condition: when that condition
is active, remove it without applying the new condition; otherwise apply normally.
Cancellation is ordinary condition removal and remains independent of immunity
to establishing the incoming condition. Delivery and whole-payload protections
still gate either operation. Repeated application keeps the existing entry and provenance. Canonical stored
lifetimes require an explicit nullable `remaining`: null means until removed,
while active finite lifetimes must be positive. The existing private reserved-token
zero tombstone remains distinct from an active finite or persistent condition.

Restrictions are a bounded, unique set of operation kinds: `voluntary_actions`,
`ordinary_movement` and `movement_abilities`. The required preparation response is
`preserve` or `pause`; pause applies only to work matching the condition's
restrictions. Protected lifecycle/restriction entries must be timed and target-wide.
Their only permitted modifiers are unconditional bounded confusion locomotion;
other modifiers use ordinary evictable conditions. Post-expiry protection requires
`keep`, so neither the active application nor protection can refresh. Restrictions
with `remove` expiry may instead use `extend`, retaining their non-evictable book
slot while adding bounded duration. Independent ordinary modifier conditions compose
with them; ordinary `keep` modifiers remain legal and normally evictable.

For Frozen, content supplies a 200-tick application and 400-tick protection.
Fainting uses the same voluntary denial and pause operations with its independently
authored duration and no follow-up protection. These are content combinations,
not condition-ID branches. Ordinary Held denies movement and cancellation with a
step; attacks and nonmovement escape actions remain available. A stronger condition
can additionally deny movement abilities, preserving the distinct Grasp GR3 effect.

Status pause retains frozen operands, the resolved strike prefix, pending cursor,
unresolved remaining work and existing resource/item holds. Denied actors cannot
start or resume voluntary work. Inspection and authorized in-place cancellation
remain available; cancellation retains the existing full commitment rules.
Thaw never resumes work automatically. Damage, grant loss and unsafe-respec rules
retain their cause-specific discard behavior and cannot resurrect discarded work.
Already-queued continuations are suspended too: this includes a typed Resume and
an ordinary matching Action that would reuse the same captured plan under a newer
queue identity. Fresh queued work that does not continue a preparation remains
queued, but cannot execute while denied. Checkpoint and arriving-region admission
reject a queued continuation inconsistent with an active pause before any thaw or
reconciliation can normalize it.
Explicit Resume keeps its existing Human-origin scope. A suspended concrete
Autonomous continuation requires controller cancellation and a fresh decision;
ordinary AI queues a deferred decision and chooses through its normal path after
release. Changing controllers does not rewrite an admitted intention's origin.
Nutrition, Health/resources, cooldowns and already-established recovery deadlines
continue on actor-active time while paused work does not count down.

Condition sources are bounded. An oldest-eviction policy deterministically removes
the oldest eligible instance at capacity; reserved tokens and protected lifecycle
entries are ineligible, and no eligible victim means atomic rejection. Protection
transitions reuse their existing slot even in a full book. Permanent
and equipment sources must also derive without temporary conditions. An explicit
equipment or rebuild candidate whose combined operands contradict source-owned
interaction fails atomically, retaining existing conditions; this hard error cannot
be repaired by evicting a condition. Ordinary soft modifier conflicts retain the
reconciliation policy, including a harmless rolled pool whose combined dice-count
modifiers exceed its bound. If expiry or
immunity removal leaves an invalid combination of residual temporary sources,
oldest remaining ordinary modifier conditions are evicted until derivation succeeds. This bounded
policy prevents expired conditions from persisting through stale caches. Metadata
reconciliation precedes each fold; every failed contributing fold removes an
eligible entry. With no live condition modifiers, the independently validated
condition-free baseline applies while protected entries and reserved tombstones remain.

Actors store an active-time baseline. Reads project only the requested actor;
mutations settle that actor, and an active-only deadline index finds future Health,
resource, condition, received-loss and internal cooldown events. Freezing removes
deadline entries; thawing rebases and restores them, so
expired frozen deadlines are never scanned by active scheduling. Frozen regions
accrue no active time. Saves retain raw state and its baseline,
so restoration neither double-advances timers nor loses fractional recovery.
Voluntarily denied actors do not stop the decision clock: existing condition
deadlines wake settlement. A lazy mid-denial save may retain elapsed readiness only
when a current active denial with valid provenance proves why input was bypassed;
settlement normalizes elapsed readiness without moving future recovery debt.
The existing input-boundary membership separately retains `permitted`, `denied`
or `pending` eligibility. Status changes update eligibility without inventing or
forgetting a boundary, including an explicitly required AI boundary. Denied
input-only pins do not keep disconnected regions active or reattach their records;
independent observation, body and paid-plan pins still apply. Explicit selection
of a detached actor creates pending eligibility, which permits normal attachment
but no action or timer execution before the arriving actor is validated. Known
eligibility must match the condition book at restore/record admission; only pending
entries are resolved on attachment. No timer advances while the region is inactive.

## Run policy, disclosure and limits

Adventure and arena are mutually exclusive policy variants. Arena running,
paused/stepping and stopped states are tagged states; policy hooks own action
limits, deadlines and elimination. The rules RNG and event log belong to the game,
separate from run policy. Elimination takes precedence over action and tick
limits, including deaths found while settling the final timestamp; action limits
take precedence when both limits coincide. Stopped arenas retain no step
permission, and invalid controls leave state unchanged. Rule, build, scheduling and persistence failures carry
specific error categories.

Ability `reach` and its supportive `range` are named tagged records; positional
arrays are not authoring aliases. Supportive targeting uses `self_or_ally` with a typed `range`: `touch` or
`sight` with a bounded cell range. Allies have the same nonempty faction,
neither authored directional faction hostility edge, and no engagement against
the target owned by the requester. The target's private engagement is never
consulted for the requester's menus or refusals. A neutral is not automatically
an ally.
Self-targeting bypasses ally geometry and relationship checks while still requiring
a living, available actor. Other targets use ordinary contact or illuminated-sight
geometry, including body cells, obstruction and rotated physical portals. Sight
range modifiers apply to supportive sight exactly as to ordinary sight.

Preparation freezes the selector and effective range. Start, resume and each
strike recheck the exact target's life, residency, disclosure, reach and ally
relation. Invalid targets are never replaced with another actor; existing unpaid
cancellation and paid-tail no-RNG miss rules apply. Disclosed actor facts include
only a server-computed `allied` boolean, never faction names.

Clients receive available abilities with IDs, names, effective reach and costs,
and construct menus and verbs from that list. Inspection discloses named sources
and generic modifiers; traces contain ID-keyed steps and the outcome. Hidden
actors, private state and unavailable abilities remain server-controlled.

Structural bounds live in `simulation/src/limits.rs`: 4,096 catalog entries,
128 modifiers per expanded source, 128 conditions, 256 hit dice, 32 templates,
64 cost holds and 32 damage components. Requirement depth and node count are
bounded independently. Tuning constants are content; these limits protect memory,
validation work and arithmetic and cannot be raised by a scenario.
The ordinary source-count summary removes per-HD observation work but does not
widen the current 256-HD or numeric envelopes. Continued deep scaling requires a
separately qualified widening of those bounds.

## Verification and scope

Regression coverage spans loader rejection, modifier ordering, equipment and
conditions, paid preparation/resume, observer equivalence, saved-rule restoration,
disclosure and clients, arena policy and ruleset search. Performance checks cover
single-actor lazy settlement and one perception scene per AI decision. Generated
certificates, save fixtures and wire samples must come from repository tooling.
All required local and CI gates in the testing policy must pass before merge.
The default dungeon scout, guardian and wisp use owned creature builds through
these same rules. The Rogue package remains exploration-only; shared creature
support does not establish Rogue parity or accepted adventure balance.
See [arena evaluation](../development/arena.md) for controlled experiments and
[project status](../status.md) for remaining acceptance work.

## Authoring creature builds and attacks

Creature definitions live in `creatures.species` and `creatures.templates`.
Character and actor `creature` builds name a species, optional templates, initial
attributes, Mana binding, faction, two distinct `starting_skills`, an explicit
`restoration` policy and ordered `hit_dice` choices. Each entry owns its source,
named-pool purchases and optional attribute increase. Actor archetypes
can provide a complete creature build. The arena uses an explicit `arcane`
template for magical capability rather than granting Mana merely because an
actor knows a spell. Shared validation rejects invalid or dormant initial
choices before any encounter starts.

A species declares natural `melee` skill, bonus, wind-up, recovery and authored
strikes. A strike may be checkless or have no damage; inspection does not invent
a damage component. For example:

```toml
melee = { skill = "light_weaponry", bonus = 2, wind_up = 90, recovery = 70, strikes = [{ offset = 0, check = { actor = "user", skill = { kind = "selected_weapon" }, against = { kind = "defense", defense = "physical" } }, on_success = [{ kind = "damage", damage = { kind = "components", components = [{ category = "energy", descriptor = "fire", amount = { kind = "rolled", dice = { count = 2, sides = 6, bonus = -1 } } }, { category = "keen", amount = { kind = "fixed", amount = 3 } }] } }], on_failure = [], descriptors = [], riders = [] }] }
```

Only Heavy/Light Weaponry are valid melee skills. The first authored damage
component is primary; the compiler retains that identity while canonicalizing
matching keys and bounding the bundle. Permanent melee modifiers change the
first primary damage payload independently in each selectable branch of a natural
strike, with extra dice only on a rolled primary. Both branches are frozen; only
the selected branch resolves. These permanent melee dice/flat adjustments never
leak into natural riders or secondary payloads. Generic authored ability damage
retains its existing modifier rules. A `SelectedWeapon` damage expression in
another authored ability uses only the first natural strike's first success-branch
damage payload (or the selected weapon's explicit bundle). It never falls back
to a failure branch, rider or later strike: the consuming ability has its own
check and branches. A failure-only natural damage source therefore remains valid
for basic melee but cannot supply that separate damage expression.
A `natural_melee` ability source has an explicit `additional_riders` list. It
imports the invoking species' complete natural sequence and appends those riders
to every strike after its existing species riders. It never substitutes an equipped
weapon. Within this source, source-selected checks, damage and timing use that
species' natural attack; named checks and target-owned selectors keep their ordinary
meaning. Ability timing may still be explicitly fixed. This source requires an
actor-target melee ability; ordinary contact push riders are allowed, selected
self-step effects are not.

Composition validates the complete source, including dormant rows, before a grant
can become active. The existing per-strike and aggregate bounds apply to the
combined result; failure riders still require a primary check. Invalid source
grants reject atomically. Frozen plans retain the source species, authored strike
ordinal and species-rider boundary; the plan's ability owns appended riders. Later
species, equipment, unlock or modifier changes affect future capture only. Primary
natural damage retains ordinary natural modifier rules, while appended riders
use ordinary frozen effects and the same 3d6/check-only/shared-edge pipeline.
This shared mechanism does not activate any particular species signature.

A later natural strike can declare `unlock = "talent_id"`; the first cannot be gated.

Weapon archetypes retain their explicit single damage bundle under `equipment.attack`, with
`equipment.slot = "weapon"`. For example, a fixed Keen weapon is:

```toml
[archetypes.blade]
class = "weapon"
name = "blade"
equipment = { slot = "weapon", attack = { skill = "heavy_weaponry", bonus = 4, wind_up = 60, recovery = 40, damage = { primary = { category = "keen" }, components = [{ category = "keen", amount = { type = "fixed", value = 6 } }] } } }
```

Declare archetypes as tables or as one inline table; TOML cannot extend an inline
table with a later table declaration. An equipped weapon supplies its own skill,
bonus, base phases and damage bundle. The creature supplies its current attributes,
training and permanent melee modifiers. Speed scales the selected attack's phases
once. Identified inventory items disclose their full base attack definition;
unidentified items retain the existing knowledge boundary. Saves retain the source
and reconstruct the same definition on restart.

Mob equipment choices compare their own check score and Speed-adjusted phases,
plus a damage estimate grouped by category and descriptor. Rolled damage uses a
clipped raw mean as a ranking heuristic. It does not predict each roll or include
an enemy's hidden protection. Replacing fire damage with cold is a tradeoff,
so it does not qualify as a straight upgrade under this conservative policy.


## Implementation and test entry points

The typed modifier operations are defined in [rules.rs](../../crates/simulation/src/rules.rs),
with structural limits in [limits.rs](../../crates/simulation/src/limits.rs).
[Derivation](../../crates/simulation/src/creatures/derive.rs),
[planning](../../crates/simulation/src/abilities/plan.rs),
[resolution](../../crates/simulation/src/abilities/resolve.rs) and
[execution](../../crates/simulation/src/abilities/execution.rs) own their respective
phases. Unit and [integration tests](../../crates/simulation/tests/it) cover the
mechanics; [scenario integration tests](../../crates/server/tests/it/scenario_packages.rs)
cover strict authoring and recovery. Generated samples are not hand-maintained.
