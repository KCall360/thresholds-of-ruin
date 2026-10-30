# Magic items and charged devices

This reference owns worn magical sources, device forms and execution, paid
identification, and scroll/device effect integration. [Items](items.md) owns
physical transfers and character knowledge; [equipment](equipment.md) owns
anatomy, armor and shared paid work. The [combat reference](combat.md) owns checks,
rays and damage kernels. [Named consumable and area-device effects](consumable-effects.md)
own Strength, Raise Level, Haste, detection, mapping, portable light and Drain
Life; the potion section here owns healing, poison and sensory potions.
Catalog availability belongs to the
[live Rogue policy](scenario-packages.md#live-rogue-content-policy).

## Worn magic and the finite Rogue ring catalog

Wearable effects use shared source modifiers, not item-name dispatch. Equipment
may author `potency_modifiers`: checked numeric templates multiplied by its
signed instance potency, alongside ordinary unscaled `modifiers`. The
`signed_potency` birth policy samples one value uniformly from its negative and
positive ranges. Rogue's numeric rings have equally likely −1, +1 and +2;
negative instances are cursed. Equipping does not identify the type, potency or
curse. Existing exact-instance knowledge and cursed-removal rules apply to both
PCs and NPCs; identified values are private to the entitled actor.

| Ring | Worn effect | Food surcharge in thirtieths |
| --- | --- | --- |
| Protection | Signed reduction against all six damage categories, including Psychic | 30 |
| Add strength | Signed Strength bonus, excluded from Physical Defense | 30 |
| Sustain strength | Prevents new restorable Strength loss; does not restore existing loss | 30 |
| See invisible | Source-owned see-invisible capability | 6 |
| Adornment | No mechanical benefit | 0 |
| Dexterity | Signed Speed bonus for checks and timing, excluded from Physical Defense | 10 |
| Increase damage | Signed bonus to every actual attack damage roll | 10 |
| Regeneration | Doubles Health recovery rate; multiple rings do not stack the benefit | 60 |
| Slow digestion | Reduces ordinary positive-reserve digestion | −15 |
| Maintain armor | Prevents corrosion while worn; does not repair or permanently protect armor | 30 |
| Searching | One free active Perception Search per 100 eligible actor-active ticks; multiple sources do not add scans | 10 |
| Teleportation | Each exact worn source independently attempts same-floor relocation with probability 1/50 every 100 actor-active ticks | 0 |
| Stealth | Continual opposed Stealth/Perception at awareness events, with one advantage; does not hide physically visible bodies | 30 |
| Aggravation | Periodic snapshot-location alerts and one Stealth disadvantage; generated cursed | 0 |

Protection is separate from the five-category magical armor ward. Signed
reductions compose with ordinary reduction and the strongest applicable talent
before the final zero floor; negative potency never creates vulnerability.
Add strength and dexterity exclude only their own attribute contributions from
Physical Defense at source folding, preserving other Strength/Speed modifiers,
initial attributes and losses. Their other attribute effects remain ordinary.

The attack bonus is captured at preparation and applies once to each canonical
rolled damage pool, including natural, melee, ranged, spell, device and damaging
attack-rider pools. It is not a per-die bonus and does not affect fixed damage,
hazards, healing, restoration, attribute-loss or resource-cost rolls. Homogeneous
merged components receive one bonus; distinct typed pools each receive it.

Regeneration halves the otherwise-derived Health period, rounded up with a
one-tick minimum, and retains quiet recovery and damage-reset behavior. It does
not accelerate Stamina, Focus or Mana recovery. Each worn ring still adds its own food
cost. Digestion uses a baseline of 30 thirtieths per ordinary unit interval
(100 active ticks in Rogue), plus the summed worn surcharges, floored at zero.
Fractions persist across equipment changes, freezes and source-free restart.
Two slow-digestion rings stop ordinary digestion while reserves remain positive;
empty-reserve starvation and nutrition spent by actual recovery are unchanged.

Searching accrues only while its source is active and the actor is awake and able
to see. Unequipping, sleep, freezing and re-equipping do not grant an immediate
scan or accrue suppressed time; the earned fraction survives. Each scan uses the
shared active Search check without spending next-check benefits or interrupting
paid work. See [discovery](../architecture/perception.md).

Stealth and Aggravation capabilities do not stack. Aggravation's authored pulse
trial is 1-in-3 every 500 wearer-active ticks. This cadence and Stealth's one
advantage are provisional TOR tuning, not exact classic Rogue probabilities.
See [awareness](../play/rogue.md#living-dungeon-pressure-and-awareness) for the
event, sensory-contact and active-time boundaries. Manual Sneak remains deferred.

The finite `rogue.rings` birth stage chooses one ring with weights
9/9/5/10/1/8/8/4/9/5/5/5/7/10 in the table's order, without reordering other stage streams.
Fourteen concealed appearances are shared by the catalog. The paid identify-ring-or-wand
scroll permits either class through one server-owned choice, without exposing
unidentified potency or curse state.

## Magical wand and staff forms

At the start of each run, every one of the fourteen Rogue device-effect identities
is assigned either a wand or a staff, following the
[pinned per-effect form choice](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/init.c#L319-L355). Every later instance of that effect keeps the
same form and concealed appearance in that run. Wand labels are shuffled metals;
staff labels are shuffled woods. A new seed can change both assignments. Physical
form selection does not change an effect's authored weight, charge range, discovery
policy or magical effect numbers. The original device stage retains its choices;
previously catalog-only devices now appear through
[supplemental live stock](scenario-packages.md#live-rogue-content-policy).

Both forms can be wielded as one-grip weapons with ordinary HeavyWeaponry melee,
60 wind-up / 40 recovery ticks: a wand deals fixed **1 Impact** and a staff **2d3
Impact**. These physical attacks neither Zap nor spend charges, and remain usable
when empty. Physical skill, damage category, grip and timing are explicit TOR
adaptations; the [pinned melee values](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/sticks.c#L25-L31)
distinguish staff 2d3 and wand 1d1. Neither form has a
Throw profile in this batch. Existing exact-item holds and cursed-equipment
restrictions apply. They start with neutral physical condition, rather than a new
random weapon-enchantment roll; the ordinary weapon-enchantment scroll still
selects the `weapon` class, not these magical implements.

The existing serialized `wand` class now means the shared **wands and staves**
identification category. Both retain the `/` glyph and shared device asset; this
change does not add a bespoke 3D staff mesh. The current worn-equipment projection
and 3D held model display only the `weapon` and `armor` classes, so wielded magical
implements are not yet rendered there; their class is not relabeled to bypass
that disclosure contract. Inventory, equipment, melee and Zap still work normally.
The identify-ring-or-wand scroll can identify either form without a new action or
wire category. Names and noun
matching describe the disclosed form, never its hidden effect.

## Reading, identification and charged devices

Read and Zap are explicit item capabilities, independent of an item's name or
physical class. `read <carried name or #id>` spends one scroll unit at completion.
`zap <carried name or #id> <direction>` reserves the exact nonstackable device
when preparation starts and spends one charge on resolution. Devices keep
quantity one; each instance owns its charge balance, sampled once on creation. Empty devices retain the same
unknown affordance and take the same time, but launch nothing and cannot underflow.
No target, a successful dodge or immunity still spends a charged launch. Pausing,
resuming, dropping, death, checkpoint restoration and transaction rollback reuse
the existing exact-item hold and preparation rules.

### Device aim and hidden delivery

Requested aim and actual device delivery are separate. An unknown device exposes
only the ordinary directional prompt, including when its hidden effect actually
applies to the user. A known device can disclose `direction`, `self_target` or
`direction_or_self` activation. Explicit self aim requires the actor to know the
identity and the definition to support it; incompatible or unknown self requests
reject rather than expose hidden capability. Once preparation captures the aim
and delivery, later identification does not rewrite that paid plan. Known
Polymorph supports both directional and self use; see [Polymorph](polymorph.md).

The strict input/capability types live in
[magic items](../../crates/simulation/src/magic_items.rs); the knowledge gate and
self-versus-ray plan capture live in
[device planning](../../crates/simulation/src/abilities/ray.rs).

### Zap timing

Ordinary item Zap uses `consume_turns × actor.turn_ticks` for preparation, then
applies the actor's action-duration scaling (Haste or Slow). The bundled rules use
`consume_turns = 1`: a 100-tick actor therefore takes 100 preparation ticks before
that scaling. **Zap adds no recovery phase.** This is actor-relative item timing,
not a universal 100-tick constant and not a Speed-scaled weapon wind-up.

Underlying device abilities may author a `100/100` preparation/recovery pair for
their ability definition; ordinary item execution supplies the item preparation
and zero recovery instead. This applies to self devices and directional devices,
including Polymorph, not just elemental rays. Item timing is frozen with the
prepared work. Empty devices still pay that same work time.

The implementation boundary is [item work timing](../../crates/simulation/src/interactions.rs),
[`Plan::recovery`](../../crates/simulation/src/abilities/plan.rs) and
[device ray planning](../../crates/simulation/src/abilities/ray.rs). For example,
[Room Light](../../crates/simulation/src/abilities/room_light_tests.rs) and
[Drain Life](../../crates/simulation/src/abilities/drain_life_tests.rs) witnesses
assert one 100-tick item turn for a 100-tick actor.

### Identification and private knowledge

An unknown identify scroll is consumed before its permitted class set appears.
Its owner gets one persisted choice among at most 64 current carried items in that
class set. `identify <name or #id>` resolves it; `identify cancel` declines without
refund. Both require the original opaque choice token. A stale, wrong-class or
other actor's target rejects atomically. Another Read cannot bank another choice;
death clears it. Declining is an explicit TOR usability adaptation: pinned Rogue
insists on an eligible selection. Reading while blind is permitted, as in that
source. ASCII uses `r` (also F6) to Read and `z` (also F12) to select a device and
direction. Escape before submission cancels freely; Escape in the paid choice
declines the spent scroll. `#release` gives up control and `Z` opens abilities.

Type identity and exact-instance charge knowledge are separate actor-owned facts.
Identifying one device learns its type for matching instances, but only that
selected device's charges. A charged elemental launch recognizes its type even
without a target, but does not reveal its remaining charges. A known owner observes
their own charge payment. Ownership transfer does not give the recipient another
actor's knowledge, and another actor's hidden use cannot refresh an old charge
fact. Spectator responses omit charge balances and identification choices. Queries
and client selection never identify anything.

Rogue's fire, cold and lightning devices use Intellect + Lore against each
recipient's Speed + Agility, using the existing opposed-ray executor. Rank zero
is usable. Activation has no separate check or Mana cost, and owning or using the
device never grants an ability to the actor. Definitions currently author six
clear-cell visits, 6d6 typed Energy damage and 3–7 initial charges. These are
explicit TOR check/timing adaptations on the pinned Rogue content; geometry,
reflection and repeat-check disadvantage remain the shared ray rules.

### Potion effects

The generic effects also support independent per-HD healing dice (the compiled
HD count is frozen at preparation), rolled actor-turn condition durations,
condition removal, and additive duration extension. A turn is converted through
the prepared actor's turn-tick value. Repeatable potion confusion has a separate
condition from the gaze's expiry-protected condition. Ordinary Rogue healing uses strict overflow: when its independent HDd4 sum
exceeds the prior injury, one point improves the earliest active owned HD with
acquisition-time headroom. Exact fill does not improve a roll. The original seed,
source, acquisition type/die and template witness stay fixed; the first HD is
already maximized. At the cap the potion simply heals. This bounded owned-roll
policy is an explicit TOR adaptation of Rogue's uncapped permanent maximum-HP
increase, not a loose permanent Health modifier.

Extra healing rolls independent HDd8 instead of HDd4 and also cures blindness.
It identifies on use. Strict positive overflow selects one improvement; overflow
greater than the frozen active-HD count plus one selects two. The improvements
follow the earliest eligible acquisition records, possibly using two points of
one row's headroom, and each stays bounded by that row's original die maximum.
If fewer credits remain, only that eligible prefix is applied. Exact fill or no
eligible headroom never invents an unbounded maximum-Health bonus. This preserves
the pinned source's one/two threshold while adapting permanent gains to TOR's
owned-HD ledger. It also removes exactly `rogue.hallucinating`, including while blind.

Improvement credit survives level drain only for that exact owned HD. Restoring
the same source/ordinal/seed restores its original credit; choosing a different
replacement retires that dormant row and cannot transfer or later resurrect its
credit. A smaller current form die or explicit Health override can temporarily
mask the benefit, but cannot enlarge the acquisition cap. Credit becomes effective
again when the masking form or override ends. Eligibility comes from compiled
permanent facts; ordinary healing does not scan the HD ledger, and permanent
recompilation occurs only when an owned record actually changes. Resolved
improvement claims also bind the current build epoch, so a later replacement
that repeats the same source and seed cannot reuse an old claim. The `scenarios/tests/magic-items` package and bounded process
witness exercise the ordinary drink/read/identify/zap path and source-free reopen.

Rogue poison identifies on use. Its descriptor-owned Athletics resistance can
negate the exposure; only failed resistance rolls `1d3` ordinary restorable
Strength loss. Poison immunity prevents both resistance and loss rolls.
The existing deficit bounds still apply; a restore-strength dose removes the loss.
See-invisible grants the shared perception capability for `1d85 + 807` source-base
turns (808–892), converted through the prepared actor's turn ticks. It also removes
`rogue.blindness`; repeated doses extend the same actor-active condition, which
expires without protection. Even a successful blindness cure or invisible-creature
reveal leaves the potion unidentified. Sight still needs the shared illuminated,
unobstructed eye witness; this does not grant monster detection or pierce a guise.
These bindings follow the pinned [potion cases](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/potions.c)
and the actual [spread expression](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/misc.c).
Sustain-strength uses the shared worn attribute-loss protection above. Poison does
not cure Hallucination in this accepted adaptation. The Hallucination potion
identifies on use, including blind and repeated use, and applies `1d85 + 807` base
turns of one pure actor-active condition. Its saved presentation phase shifts once
per captured base turn; extending the same generation preserves that cadence.
Extra Healing cures it; ordinary healing and see-invisible do not. Cosmetic names,
assets and glyph categories never replace actual action targets or physical item
classes. Only genuine visual disclosure is changed; ordinary Awareness remains
factual. Acquired map/public-plan appearances remain historical after cure.
All concealed potion identities have distinct catalog appearances. The existing finite magic-potion stage chooses equally
among healing, confusion, blindness, poison, see-invisible, extra healing and levitation for one unit;
restore-strength retains its separate stage. These are provisional TOR availability
weights, not the source distribution. These existing stage IDs, relative order
and attempt bounds are unchanged. Hallucination and other completed potion choices
are available through the appended
[supplemental stock stage](scenario-packages.md#live-rogue-content-policy), not
by replacing this seven-potion pool.

### Device delivery variants

Cancellation and Magic Missile wands also appear through supplemental stock and use first-body
UntilObstruction delivery, not the bounded Sight profile of the existing utility
wands. Both start with 3–7 charges and remain directional when identified.
Cancellation has no save or random roll and never discovers its own type;
Magic Missile discovers on charged use and has one Agility/Lore save with
1d4+1 Energy damage on failure through ordinary bonus/reduction rules. See
[the player behavior](../play/rogue.md#cancellation-and-magic-missile) and
[the source contract](creature-rules.md#shared-ability-rules).

Invisibility, haste-monster and slow-monster wands use a directional contact ray:
the first physically encountered creature gets one Agility-versus-Lore dodge,
and that contact ends the ray on either outcome. These rays never reflect and
never reach a second creature behind the contact. Six-cell reach and the opposed
check are explicit TOR adaptations of the
[source's utility branches](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/sticks.c),
which have no equivalent save. Their underlying abilities author 100/100 base
prepare/recovery metadata; actual device execution follows [Zap timing](#zap-timing).
They have 3–7 initial charges, pay exactly one
charge through ordinary Zap, and never identify on use or reveal an unknown
instance's remaining charges. Unknown sources also stay unnamed in combat
history, observed-ability knowledge, previews and ordinary spectator views.
Visible changes still produce neutral feedback and source-neutral status identities;
no event reveals a hidden first contact to the caster. Identification after a use
cannot rename its frozen history. Known sources and charged `on_use` devices may
name their ability only to their initiator; authoritative diagnostics remain wizard-only.
Haste/slow are hostile interactions; invisibility
is non-hostile, without adding pursuit solely for concealment.

All three utility conditions persist until explicitly removed and repeated use
keeps one application. Haste against persistent slow removes slow and leaves neutral;
slow against persistent haste removes haste and leaves neutral. A later application can
then establish its own effect. Haste halves ordinary action durations with upward
rounding and a one-tick minimum; slow doubles them. Neither changes Speed,
checks, defenses, cooldowns, regeneration or condition clocks. Invisibility uses
the shared perception capability. Existing linked-hold cleanup remains authoritative;
the source's incidental grouped-branch Flytrap release is not copied. Personal
status displays “until removed”, and saved conditions retain that lifetime.
The unchanged one-unit device stage now chooses equally among the three elemental
and five utility wands, with eight distinct concealed appearances. These weights
remain provisional TOR availability.

## Monster-control scrolls

Readable capabilities may carry exactly one `control` payload instead of immediate
self-effects or identification. The paid plan retains that payload and validates
it against the exact consumed item; retries and recovery cannot substitute a
more powerful scroll. Each completed read consumes one unit even if no target or
placement is available. The Rogue control-scroll stage makes one finite placement
attempt per first-entered floor; its five equal weights are provisional tuning.

| Scroll | Completed effect |
| --- | --- |
| Aggravate monster | One shared location-alert pulse. The position is a snapshot, not a disclosed identity or continuously tracked actor. |
| Create monster | One depth-appropriate prepared creature in a legal adjacent horizontal cell, without birth loot. Failed placement still consumes the scroll. Created creatures neither consume wandering slots nor obey the eight-live-wanderer cap. |
| Hold monster | Intellect + Lore against each engaged, disclosed hostile's Spiritual Defense within two horizontal cells and unobstructed sight. An ordinary actor-active condition denies voluntary actions and pauses preparations; a committed hostile attack attempt releases it whether the attempt hits or misses. |
| Monster confusion | Arms one next-successful-physical-hit charge. Misses retain it; the first successful natural, selected-melee or projectile primary consumes it, including when the mental application is resisted. Another read replaces rather than banks a charge. Delivery uses Intellect + Lore against Spiritual Defense and the same ordinary Confused movement condition as the PC's confusion potion. |
| Scare monster | Reading wastes one unit. Its useful capability belongs to an occupied ground scroll, described below. |

Authored spells, rays and gaze attacks are not physical-hit delivery. Multi-strike
sequences test each committed primary in order; riders do not spend another charge.
Confusion does not choose targets, change allegiance or invent random attacks.
The authored 2,000-active-tick confusion and 1,000-active-tick Hold/Scare durations
are provisional TOR balance settings, not exact classic Rogue durations.

An occupied ground Scare scroll automatically applies a finite AI Flee objective
to an approaching hostile AI creature within one horizontal square, including
diagonals. Walls and closed doors block the contact. There is **no resistance
roll**; ordinary mind-affecting immunity remains effective. The objective retains
the scroll location, not its occupant's identity. It neither cancels magical sleep
nor activates an intact stationary Xeroc ambush, and it does not lock player input.
The ordinary Flee policy prefers retreat but can defend when cornered.

An exact scroll/recipient approach triggers at most once. Remaining adjacent,
changing occupants, querying, reconnecting or restoring cannot reroll or refresh
it. Genuine departure and return rearms the approach. Removing the scroll or
moving its occupant does not cancel an already earned finite objective, and
inactive-floor time does not spend its condition timer.

Picking up non-cursed Scare units transfers those units and curses them. Picking
up already cursed units crumbles them instead of adding them to inventory. Curse
is homogeneous per-unit stack state: splitting, merging, dropping and save/restore
preserve it. Ordinary remove-curse can clear carried Scare curse, after which the
next ground pickup curses those units again. This is the selected adaptation's
explicit pickup rule, not a claim that every NetHack edition behaves identically.

The [living-dungeon process contracts](../../scripts/test_living_dungeon_process.py)
cover ordinary scroll behavior and source-free recovery. Test ownership is not a
current pass claim or complete-run balance qualification; see [status](../status.md)
and the [testing policy](../development/testing.md) for qualification boundaries.

## Trap avoidance and exposure resistance

Deliberate movement onto a trap already known privately to that actor can make
one Agility check against its separate authored avoidance DC. Newly discovering
the plate during entry is too late; forced movement, falling, standing still and
queries do not grant attempts. Success preserves contact identity without
triggering the payload, so remaining on the plate does not repeatedly roll.
An already-open trapdoor is a physical hole, not an avoidable trigger.

Payload descriptors may author one shared skill resistance policy. This belongs
to the explicitly tagged payload, not a condition name or an individual damage
component; a poisoned physical attack authors a separate delivered rider. Rogue poison
uses Athletics against DC 12, including venom riders, poison potions, poison
darts and explicitly poison-classified sleeping gas. Immunity prevents the
exposure without a resistance roll. A delivered exposure rolls once, with exact
available outgoing check-token ownership; a successful resistance cancels only
that payload. It does not erase the primary bite/dart hit or physical damage.
A missed delivery does not roll resistance. Magical sleep is not implicitly
classified as poison. Avoidance, resistance and concealed-feature discovery DCs
are independent authored values, even where provisional tuning is 12.

## Levitation and relocation

Levitation potions use ordinary paid Drink and exact-unit consumption. At successful
completion they grant 29–31 base turns (1d3 + 28), measured using the prepared
actor's base turn ticks rather than haste-scaled action duration. The sampled
actor-active duration extends on repeat use and pauses in frozen time. Overflow
rejects the enclosing transaction without partial consumption. The same completion
arrests descent; an actor must actually finish drinking before impact for rescue.
See [locomotion](creature-rules.md#locomotion-capabilities).

Teleport scrolls use paid Read. Random relocation enumerates eligible supported
cells on the current floor and draws once; temporary hover does not admit holes.
Standalone authored regions have a 65,536-cell candidate budget; larger unsupported
candidate domains fail cleanly rather than sampling a biased prefix. A blocked or
empty destination still consumes the completed scroll/charge, but does not
manufacture a displacement or identify a scroll from one.

Teleport-away and teleport-to wands use the ordinary six-clear-cell first-contact
utility ray, the shared actor-relative [Zap timing](#zap-timing), and 3–7 initial
charges. Recipient Speed + Agility opposes frozen caster Intellect + Lore; ties dodge, and contact
ends the ray on hit or dodge. Away uses same-floor random relocation. To uses
exactly the adjacent endpoint from the **frozen launch placement/direction**,
with live target body/frame and current geometry/occupancy; there is no nearby
fallback. Forced caster displacement interrupts unfinished launch work.

The teleport ring is generated cursed. Each exact worn item owns its partial
100-active-tick interval, including sleep but excluding frozen time. Duplicate
rings have independent 1/50 trials, in stable item order; both can trigger.
Detached/re-equipped sources retain their fraction. This is not the nonstacking
regeneration/searching policy. Successful forced relocation releases spatial
holds and retires unfinished preparation without refunding already-paid costs.
Destination contact runs once through ordinary trap/lifecycle authority.

## Implementation and test entry points

- [Item effects and knowledge](../../crates/simulation/src/magic_items.rs),
  [instance knowledge](../../crates/simulation/src/equipment_knowledge.rs) and
  [paid interactions](../../crates/simulation/src/interactions.rs) own use,
  discovery, payment and continuation state.
- [Magic-item simulation witnesses](../../crates/simulation/src/abilities/magic_item_tests.rs)
  and [actual magic-item processes](../../scripts/test_magic_items_process.py)
  exercise ordinary use, private instance facts and source-free restart.
- [Ray planning](../../crates/simulation/src/abilities/ray.rs) and
  [effect resolution](../../crates/simulation/src/abilities/resolve.rs) share the
  [combat](combat.md) kernels; item identities do not select alternate rules.
- [Polymorph](polymorph.md) owns the current body transition and its explicit
  distinction from proposed temporary-form return semantics.

All catalog frequencies and TOR/source comparisons here remain distinct from
full-run balance evidence. The [live policy](scenario-packages.md#live-rogue-content-policy)
owns which authored entries can currently spawn.
