# Creature rules reference

The simulation evaluates validated scenario content through one modifier fold and
one plan/resolve/apply pipeline. The [architecture](../architecture/overview.md),
[rules principles](../architecture/simulation.md#rules-design-and-review), [scenario format](scenario-packages.md)
and [testing policy](../development/testing.md) describe the surrounding contracts.

## Content and identity

Each scenario manifest names a rules import with optional scenario overrides, or
an explicit standalone rules document. Its validated definitions
include abilities, talents, conditions, creature types, subtypes, classes,
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
Context-gated talents and their content are separate work.

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
or stopped recovery. Nutrition gameplay is still separate work. Before a policy
or derived-period change, settle time under the old policy. Rephase earned
progress as `floor(old_elapsed × new_period / old_period)`; stopped recovery
retains its fraction using the base period. Changing or repeatedly toggling
policies never heals or increases that fraction. Save records store only elapsed
progress and the nutrition policy, deriving periods on restore. Invalid policy,
progress at/above the period, or nonzero progress on full/dead Health rejects.
Personal stats and privileged inspection disclose period, progress, stopped state
and reset percentage; enemies still disclose only qualitative injury.

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
enforce the same limit before producing an observation. Together with nineteen
skills capped at rank five and two permanent starting ranks, this bounds paid
purchases at 349 and per-HD pool disclosures at 4,096. Attribute opportunities remain
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
hit_dice = [
  { source = "mage", purchases = [{ pool = "advancement", selection = { kind = "talent", talent = "magic_bolt" } }] },
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
an empty purchased-choice slot. Existing ordinary previews spend those points.
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
Temporary equipment-fold conflicts or derived bounds defer the HD without vetoing the fatal effect.
Automatic growth also defers rather than evicting an otherwise valid timed condition to fit a combined derived bound or override. Genuine immunity removals and explicit rebuild/respec reconciliation retain their normal semantics.
New XP or authoritative source-fold changes run one bounded growth pass at actor finalization;
ordinary damage, unchanged timers and reads do not retry an unchanged obstruction.
Recorded-only Health ledgers also retain XP, but cannot automatically add HD without a genuine saved root.
A bounded projection of that balance advances the recipient's comparison tier
even before its missing source is supplied. Actual HD growth uses the same
threshold ordering, without a separate credit bank or refund economy. The
structural HD cap also retains XP. Earned, spent and held XP are checked `u64`
amounts with `earned = spent + held`; restored applied/comparison tiers must also satisfy their bounded minimum spent/earned totals. Extra paid regrowth remains valid. Exhaustion rejects the whole transaction,
including its fatal effect, rather than saturating or discarding value.
This representational limit is 18,446,744,073,709,551,615 XP, distinct from the
ordinary HD cap. Rules bound one award to at most 1,000,000 XP, so exhaustion
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
and the combined ordinary Warrior/Mage HD 1–6 acceptance remain required.

Typed condition and forced-motion causal origins are extension hooks only.
Damaging condition content and a persisted direct-push motion episode are not
implemented here. Their adapters must retain the actual source/episode, clear
unrelated later motion, and materialize an off-floor owner for an award without
thawing that floor or granting catch-up activity. Missing owners fail atomically;
there is no silent award loss, unbounded pin set or deferred-death ledger.

## Abilities and effects

An ability defines cost, timing, reach, check, success/failure effects, descriptors,
validated ability tags and AI hints. Tags are named ruleset content IDs with
preindexed membership. Modifiers may target one ability or every ability carrying
a tag. The weapon talent chain targets `weapon_attack`, so an added attack inherits
those benefits by declaring the tag. The derived cache keeps the original tagged
modifier and its source for inspection; resolution consumes the per-ability values. Basic melee is an ordinary disclosed ability. Effects are damage,
healing, resource restoration and condition application; items and physics use the
same resolution machinery. Fixed amounts do not consume RNG; rolled amounts use
the deterministic bounded dice kernel. Canonical tuning lives in
[default-ruleset.toml](../../crates/simulation/data/default-ruleset.toml); scenarios import it and can declare
strict, identity-keyed overrides. Ability tags select families of abilities for
modifiers without enumerating each technique ID.

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
attack against that opponent within 300 active ticks. This tree's shared effect
and client support does not yet register its final default talent prerequisite
chain; that content belongs to the combined Warrior slice. A successful paid
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

Canonical Magic Bolt checks Spellcasting against Cognitive defense. Fear checks
caster Intimidation against Spiritual defense and applies on success. All ability
phases with an authored Speed scaling attribute use ceiling division and a
one-tick minimum. Bind Wounds uses fixed unscaled timing.

Magical creatures have Mana capacity `2 + binding`, with another 2 from Arcane
Reserve. Magic Bolt spends 1 Mana at preparation and 1 at resolution. Mana recovers
one point per 800 active ticks, so a two-point cast needs 1,600 ticks of regeneration
in steady state after its initial reserve is spent. The split cost preserves a
meaningful interruption cost; capacity increases extend the initial reserve
without reducing the ongoing price of casting.

Canonical talent increments are:

- Health: +2/+4/+6/+8 through Hardiness, Toughness, Unyielding and Indomitable.
- Stamina, Focus and Mana capacity: +2 per applicable talent, one additional
  two-point ability use.
- Physical defense: +1 per Guard tier. Category wards reduce matching damage by 2.
- Weapon and Bolt damage: +1 flat from the initial damage talent, then one
  additional die per higher damage tier.
- Fear Mastery: advantage on its check and +100 ticks to its condition duration.
  Its edge is ability-scoped and cannot affect another ability. Legacy talent IDs
  remain until the new class slice replaces their catalog.

Single-point and single-die steps preserve granular progression; graduated Health
bonuses and fixed two-point resource increments keep their distinct roles.
Recovery is conservative enough to retain resource scarcity during sustained
casting. These are bounded shared-rules defaults: arena sensitivity checks do not
establish general build parity or multi-encounter adventure pacing. Full-Focus-only
AI also cannot establish the player-controlled burst value of Resolve.
Mental-attribute timing and AI learning from Fear outcomes are not implemented;
the current timing mechanism scales with Speed and AI uses declared hints.

Starting timed preparation freezes one bounded ordered strike plan. Earlier
queue admission stores the request, not the plan. Each ability authors strikes
or selects the current melee source. Both authored abilities and natural sources
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

Conditions define a family, lifecycle, refresh/longest/bounded stacking behavior,
source scope, descriptors and a capacity policy. Each family contributes at most
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

Other temporary stats use ordinary unconditional sources. Unsupported scoped
operations fail validation rather than being silently ignored.

Condition sources are bounded. An oldest-eviction policy deterministically removes
the oldest instance at capacity; rejection is explicit where selected. Permanent
and equipment sources must also derive without temporary conditions. If expiry or
immunity removal leaves an invalid combination of residual temporary sources,
oldest remaining conditions are evicted until derivation succeeds. This bounded
policy prevents expired conditions from persisting through stale caches.

Actors store an active-time baseline. Reads project only the requested actor;
mutations settle that actor, and an active-only deadline index finds future Health,
resource, condition, received-loss and internal cooldown events. Freezing removes
deadline entries; thawing rebases and restores them, so
expired frozen deadlines are never scanned by active scheduling. Frozen regions
accrue no active time. Saves retain raw state and its baseline,
so restoration neither double-advances timers nor loses fractional recovery.

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
`sight` with a bounded cell range. Allies have the same nonempty faction and
neither directional hostility edge. A neutral is not automatically an ally.
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
