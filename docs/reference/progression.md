# Progression, ownership and restoration

This is the current permanent-build contract: who owns each Hit Die (HD), which
choices survive body changes and drain, and how earned XP reaches the builder.
[Creature rules](creature-rules.md) owns derivation, temporary conditions and
recovery; [Polymorph](polymorph.md) owns the runtime body-transition transaction;
[player advancement](../play/advancement.md) owns commands and confirmation.

## Body and personal ownership

An ordinary build without an enrolled body package treats all actual HD as
personal for progression. An explicitly enrolled build instead composes the
active body package's rows followed by retained personal rows. Ownership is not
inferred from racial versus class source: a later earned racial HD is personal.
The live Rogue Warrior and Mage each start with **one racial body HD and one
personal class HD**. Only the personal class point is available for an ordinary
player choice; the body package's unused innate budget is not a free player point.

| Record | Stable owner and responsibility |
| --- | --- |
| Body HD | `{ form, ordinal }` in one immutable authored package; racial source, body-owned purchases and Health acquisition |
| Personal HD | Monotonically allocated personal identity; class or racial source, purchased training and personal attribute opportunities |
| Purchase | Its HD owner plus a non-reused serial; effective row position is not funding identity |
| Inactive body snapshot | That visited package's exact Health rows and improvements, retained for a later return |
| Drained receipt | The actual removed owner, seed and choices; not replaced by borrowing another form's HD |

The package supplies species, body geometry, innate racial HD, anatomy and
body-owned grants. A body replacement changes base Strength and Speed to that
form's values (or the saved original custom physical base on return); the
personal mental base remains. Retained personal attribute increases, starting
skills, learned expertise, class HD and purchases still pass through ordinary
validation and eligibility. An owned talent whose current permanent gates fail
is **dormant**, not forgotten or refunded. Returning to a qualifying body can
reactivate it. Ownership preservation is not a promise that every learned ability
works with every body's prerequisites, anatomy or equipment.

Authored original-body-funded personal Willpower/Breath grants retain an explicit
partition and donor proof. They share the original pool budget, not a second pool;
visitor-body packages do not create equivalent personal grants. Loss of the
funding body row suppresses that grant until its exact receipt is restored.
Ordinary player purchase/replacement never edits a package row. Actionable owner
balance pages and `unspent_points` exclude package pools.

The default attribute opportunity is every fourth **personal** HD in an enrolled
build, not every fourth effective body-plus-personal row. For an unenrolled build,
all HD count. Body package rows cannot author an ordinary attribute increase.
Personal identities, acquisition species and purchase high-water marks survive
recomposition and restoration. New purchases cannot reuse retired serials.

Sources: [body composition](../../crates/simulation/src/creatures/body.rs),
[stable owner types](../../crates/simulation/src/creatures/composition.rs),
[purchase admission](../../crates/simulation/src/creatures/advancement.rs),
[actionable balance projection](../../crates/server/src/advancement_query.rs),
[Rogue starter builds](../../scenarios/rogue-exploration/scenario.toml).

## Health ownership and recomposition

Each actual HD retains a Health seed and acquisition-time interpretation.
Racial die size comes from creature type; class die size comes from its class.
Health sampling does not use combat RNG. In a body-enrolled build only the first
body HD is maximized; a personal HD does not become maximized merely because a
new composition puts it at another effective index. Unenrolled ledgers retain
their original ordinal policy. Current maximum Health folds the resulting HD
contributions, effective Strength and ordinary Health modifiers, then restorable
maximum-Health loss. Injury remains the authoritative current-Health state.

A newly visited body uses a stable actor/form Health stream. Returning to a
visited package restores its snapshot, including lawful per-HD Health
improvements. Personal rows keep their own streams and acquisition origins.
Changing body size, earning HD or reopening a save cannot reroll retained rows
or reinterpret their owned improvement budget. Restoration retains the Health
stream; an explicit ChooseAgain source change uses that same seed with the new
source's legitimate acquisition interpretation, rather than promising the old
source's die or improvement credits. Maximum changes
preserve injury; they are neither healing nor resurrection and can cause ordinary
sticky death when the new maximum no longer supports the injury.

Sources: [HD ledger and acquisition policy](../../crates/simulation/src/progression.rs),
[body snapshot/stream composition](../../crates/simulation/src/creatures/body.rs),
[maximum-Health derivation](../../crates/simulation/src/creatures/derive.rs).

## Latest-Hit-Die drain and restoration

`drain_latest_hit_die` removes one actual latest HD and its owned purchases.
`restore_hit_die` restores at most one previously applied HD. Both effects have
no authored operands: source, Health seed, ordinal and purchases come from the
ordinary advancement ledger. They use the same abilities, riders, consumables,
frozen plans and transactional publication as the other loss effects.

The exact restorable count is:

- **Unenrolled build:** highest applied growth tier minus current actual HD.
- **Body-enrolled build:** highest applied personal growth tier minus current
  personal HD, **plus outstanding body-package HD losses**.

A funded but unapplied earned tier never becomes free restoration. Restoration
first restores the latest outstanding package receipt, with its exact original
owner, source, Health seed and choices. This branch needs neither a selected
future source nor newly spent XP. Only after package losses are gone does
restoration use the personal branch. Cross-package body changes are a paid
no-change while package losses remain; changing form cannot replace that loss.

For personal/unowned restoration, authored recipes restore their original source
and choices. ChooseAgain restores the stable row/seed with empty purchased
choices and requires an explicitly selected future racial or class source.
When that policy lawfully changes the source, it rebinds the acquisition
interpretation for the new source without rerolling the seed or changing the
personal owner. Authored plans beyond their saved recipes also restore an empty
row. Original Health root and identity are required; missing source/root rejects
new admission atomically, with a reason in owner advancement inspection. A
previously committed plan that becomes ineligible retains its normal costs and
debt and resolves unaffected.

Drain and restoration re-evaluate permanent gates and dormancy through the
ordinary rebuild. They preserve injury, cooldowns, paid balances, quiet-time
progress and valid frozen plans. Genuine grant loss cancels the unresolved tail
with its full commitment/recovery debt; restoring a grant does not replay the
prefix or resurrect cancelled work. Removing the last HD causes ordinary sticky
death, inventory drop and once-only fatal-owner XP. Restoration never revives.

The applied and earned growth watermarks remain distinct saved facts. Unenrolled
restoration uses its existing ledger/ordinal policy; enrolled builds additionally
retain the exact removed ownership receipts, including package losses. These are
not a second XP bank or an inferred substitute-body history. Owner stats disclose
the computed count, and advancement discloses the applied ceiling and missing-source/root reason.
Restoration never spends or refunds XP. Drain may unblock previously earned,
unspent credit for ordinary paid regrowth; already spent credit is never reused.
See [earned growth accounting](#earned-xp-and-future-hit-dice).

A newly restored contextual recovery source uses the ordinary lazy growth
context boundary. Frozen actors preserve a pending denominator until thaw;
noncontextual drain never asks a frozen actor to perceive the world.

Sources: [restorable count and branch order](../../crates/simulation/src/creatures/state.rs),
[owned receipt removal/restoration](../../crates/simulation/src/creatures/body.rs)
and [body-change loss gate](../../crates/simulation/src/polymorph.rs).

## Advancement points and starting ranks

Rules define named `point_pools`, each with a display name and allowed purchase
kinds (`skill_rank`, `talent`, `expertise`). `constants.racial_point_grants` and each class's
`point_grants` declare `{ pool, points }` grants per hit die. A source may grant
multiple restricted or shared pools, or no points. Unknown pool references,
duplicate grants, empty allowed-kind lists, duplicate allowed kinds and grants
over the structural bounds reject before a game starts. Pool definitions are
identity-keyed: declaration order does not affect compiled semantic hashes. Grant
order is preserved because convenience purchase commands select the first
compatible unspent pool.

The default campaign provisionally grants one shared `advancement` point per HD.
One point buys one talent (including a selected fixed form), one skill rank or
one expertise where that pool permits it. This economy has not been accepted
as final balance. Purchased ranks, talents, expertise and unspent points always
sum to each pool's grant; a purchase cannot borrow another pool's balance. There
are at most 16 grants and 256 total points per HD. Independent ownership bounds
are 256 purchased talents (active and dormant combined) and 256 distinct owned
expertise IDs across their sources. Extra grants can remain unspent; granting
points does not expand either ownership bound.
Construction, authoring, purchase/replacement and saved current/authored builds
enforce these limits before producing an observation. The 256 actual-HD admission
guard and sixteen pools per HD bound the full
per-HD pool projection at 4,096 rows. There is no separately asserted combined
skill/talent/expertise purchase count here: each independently validated bound
remains authoritative. Attribute opportunities are separate, every four personal
HD for enrolled builds and every four HD otherwise in the default rules.

Creation explicitly records exactly two distinct rank-1 starting skills, outside
the HD-owned purchases. They cannot be reclaimed by drain, multiclassing or
restoration. Starting and purchased ranks together cap at five. A purchase may
belong to an older unspent HD and uses current permanent eligibility. Existing
owned talents whose requirements cease to hold remain dormant; they spend their
owned points until their HD is removed. Inspection discloses each pool's grant,
spent and unspent balance, every purchase, the two starting skills, restoration
policy, and failed parent/requirement descriptions for dormant talents. Large
requirement descriptions are bounded summaries with an explicit omission marker.

## Expertise and Intellect learning

Expertise is distinct from a skill rank: Heavy/Light Weaponry training supplies
check rank, while equipment expertise unlocks the weapon's authored expert
profile. Expertise IDs have cultural, equipment, utility or specialist categories.
The effective ownership set combines three sources:

1. A named-pool paid HD purchase, removed/refunded through that HD's ordinary
   ownership and replacement rules.
2. An active talent's fixed expertise grant. A dormant talent retains ownership
   but does not activate its grant; another active source can still provide it.
3. An Intellect-earned choice at one retained numbered threshold. This consumes
   no HD point. Specialist expertise cannot be chosen through this bonus channel;
   ordinary HD purchase uses its own pool permission.

New paid/bonus choices cannot acquire an expertise already owned, including a
dormant grant. Fixed talent grants can overlap other legitimate origins; the
runtime expertise is active when at least one origin is active, not multiplied by
source count. Ownership and active status are therefore separate inspection facts.

The actor records its eligible permanent Intellect at birth and genuinely new
higher capacity reached at explicit permanent advancement boundaries. Each
first-earned threshold owns at most one immutable choice. A drop in current
eligible capacity makes higher-threshold choices dormant; restoration/reactivation
uses the same choice. Returning to a previously reached capacity allocates no new
slot and does not allow choosing again. The empty retained slot can be chosen
only while its threshold is currently eligible. This is a retained learning
entitlement, not a temporary bonus proportional to today's effective sheet.

Eligible Intellect is reconstructed through the ordinary validated permanent
source fold, including Set masking and the recorded source/choice prerequisites.
The birth witness retains its initial attributes and birth-template identities;
for an enrolled build the personal-learning projection uses the original body,
not a visiting body's Intellect. Equipment, temporary conditions, attribute
deficits and a later body change cannot mint new thresholds. Legitimate personal
Intellect advancement earned while in another form can earn a new threshold.
Saved acquisition witnesses must prove every first-earned increase; arbitrary
baseline rewrites after learning reject instead of forging entitlement.

Ordinary owner previews and confirmation use the same build-epoch and runtime
rebuild boundary as other advancement. Commands live in
[player advancement](../play/advancement.md); equipment effects live in
[equipment](equipment.md), rather than redefining learning there.

Sources: [expertise source fold](../../crates/simulation/src/expertise.rs),
[threshold receipts and selection](../../crates/simulation/src/creatures/expertise_learning.rs),
[eligible permanent Intellect](../../crates/simulation/src/creatures/intellect.rs)
and [ordinary purchase admission](../../crates/simulation/src/creatures/advancement.rs).

## Fixed acquisition forms

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

## Restoration policy

`restoration = "choose_again"` restores an empty purchased-choice slot so a
character can choose again. `restoration = "authored"` saves the complete intended source and
purchase list per ordinal, independently of player/control ownership. Regrowing
an intended ordinal requires its original source and refills only that ordinal;
retained choices are never overwritten. An ordinal beyond the fixed authored
blueprint is empty when regrown. Saved current and intended sources must agree
for every overlapping ordinal, even when retained purchases have been edited.
For body-enrolled builds, these personal recipes bind stable personal IDs rather
than treating today's effective position as the original ordinal; body-package
receipts follow their separate exact-restore branch above. Neither path grants
starting skills again or consumes the combat RNG. Rebuilds retain injury, resource balances and persistent
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
HD owner, named pool and skill/talent/expertise choice together with actor, build
epoch and opaque host branch/stream/reset provenance. Admission requires an active, ready
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

## Timed ordinary replacement

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

## Earned XP and future Hit Dice

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

Drain retains the current XP balance and highest earned tier. For the ordinary
unenrolled example, HD 6 drained to 5 with 360 of a provisional 600 XP keeps that
360; 240 new XP regrows HD 6. Explicit restoration first leaves the 360 toward
HD 7. Enrolled growth uses personal tiers, not effective body-plus-personal HD. A completed threshold is spent
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
or ordinary body reshaping retire the episode so unrelated later falls earn no
credit. The explicit Polymorph transition is an exception: it preserves the
existing causal fall episode across the body change, rather than treating that
change as an unrelated new fall.
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

## Implementation and test entry points

The following are source witnesses, not a claim that a particular verification
run has just passed. See [testing policy](../development/testing.md) for execution
and evidence requirements.

| Contract | Implementation | Boundary witnesses |
| --- | --- | --- |
| Stable body/personal ownership, repeat visits, physical caps and restoration receipts | [body.rs](../../crates/simulation/src/creatures/body.rs), [composition.rs](../../crates/simulation/src/creatures/composition.rs) | [body_tests.rs](../../crates/simulation/src/creatures/body_tests.rs): retained personal grants, repeat-visit Health, ChooseAgain/authored restore, original custom physical base |
| Actual loss versus funded tier; restore without XP refund | [state.rs](../../crates/simulation/src/creatures/state.rs), [earned_growth.rs](../../crates/simulation/src/earned_growth.rs) | [hit_die_effect_tests.rs](../../crates/simulation/src/creatures/hit_die_effect_tests.rs), [earned growth integration](../../crates/simulation/tests/it/earned_growth.rs) |
| Point conservation, forms, limits and dependency refunds | [advancement.rs](../../crates/simulation/src/creatures/advancement.rs), [limits.rs](../../crates/simulation/src/limits.rs) | [ledger](../../crates/simulation/tests/it/advancement_ledger.rs), [replacement](../../crates/simulation/tests/it/advancement_replacement.rs) |
| Intellect birth/history and dormant retained choices | [expertise_learning.rs](../../crates/simulation/src/creatures/expertise_learning.rs), [intellect.rs](../../crates/simulation/src/creatures/intellect.rs) | [learning tests](../../crates/simulation/src/creatures/expertise_learning_tests.rs), [source-witness tests](../../crates/simulation/src/creatures/intellect_tests.rs) |
| Paid owner action, safety, holds and epoch | [player advancement integration](../../crates/simulation/tests/it/player_advancement.rs) | Same integration owner includes runtime-preserving previews and confirmation boundaries; [server projection](../../crates/server/src/advancement_query.rs) owns bounded actionable pages |
