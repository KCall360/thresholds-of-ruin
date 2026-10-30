# Equipment, armor and physical projectiles

This reference owns anatomy sockets and capacity, equipment selection and item
condition, the finite equipment catalog, and shared paid item work. Inventory
transfers and actor-owned knowledge are in [items](items.md); charged devices and
worn magical effects are in [magic items](magic-items.md). [Combat](combat.md)
owns attack execution, checks, damage ordering and interruptions.

## Independent projectile definitions

The development schema gives ammunition an independent projectile capability:
exact launcher-family ID, the shared typed base-damage expression with a required
primary component, and deterministic hit recovery. A launcher's effective weapon
profile owns its family and short/long range. Payload and launcher families must
match exactly. Both source compilation and direct placement/restoration validate
damage descriptor references. A dual equipment/projectile definition validates
both capabilities and remains subject to equipment nonstackability.

Ammunition supports homogeneous birth, curse and signed hit/damage enchantments;
plain ammunition cannot acquire armor penalties, armor enchantment, corrosion or
protection. Ordinary Launch and Throw use the shared physical-projectile kernel. The Rogue
arrow consumes one unit on a successful hit and permits miss recovery. A weapon
profile may separately author `thrown_payload = { damage = ..., recovery = ... }`
without becoming ammunition or requiring a launcher. Throw replaces only the
weapon base damage; instance enchantments and general attack bonuses apply once.
The captured profile retains the payload through paid preparation and reopening.
Profiles without that payload retain their melee damage and recoverable-hit
behavior. No percentage breakage rule is implied.

## Timed equipment and effects

The server accepts `equip` (carried item plus anatomy slot index), `unequip`
(carried equipped item) `drink` (carried potion) and `eat` (carried explicitly edible item) through ordinary intention
admission. Targets are opaque observer-scoped inventory references. Text accepts
`equip`/`wear`/`wield`, `remove`/`unequip`, `drink`/`quaff`, and `eat` with a carried
item name or opaque target. Adventure resolves disclosed nouns with its existing
choice rules. ASCII uses `w` to wield a weapon, `W` to wear armor, `P` to put on an
accessory, `T` to take off armor, and `R` to remove an accessory. `#equip` and
`#remove` include other equipment such as tools. `q` drinks and `e` eats; Up/Down
and Enter choose among items, and Escape cancels without spending a turn. These commands
act on one item at a time and have no quantity editor. Clients choose the first free
matching anatomy socket; replacing gear requires separate removal. Both clients
use the shared `client-common` item checks, while the simulation validates the
submitted action against authoritative state. Direct text offers opaque choices
for ambiguous names; adventure uses disclosed order for indistinguishable stacks.
Selection clears on cancellation, changed observations, control loss and disconnect.
Completion narration belongs to the owning actor, handles noncombat equipment
and the last consumed unit, and does not repeat stale observations.

Anatomy is an ordered slot list; duplicate kinds provide distinct sockets, such
as two rings. An authored anatomy supplies `slots`, named `capacities` and an
explicit `primary_attack_socket` (`none` or a valid attack-capable socket index).
Character declarations supply their own anatomy. Item archetypes optionally
provide `equipment`, `consumable`, or a typed catalog `supply` identity; placements use `equipped_slot` with
`carried_by` for starting gear. Invalid classes, counts, slots, conflicting gear
and effect definitions are rejected. Multiple matching sockets may hold weapons.
The anatomy's designated **primary attack socket** supplies the default weapon;
if it is empty or absent, the natural attack remains the default even when a
secondary socket holds a weapon. Other equipped items still contribute their
modifiers. An invocation can explicitly select another eligible equipped weapon;
that secondary selection adds a resolution cost of Focus 1 with the captured
Offhand trait, otherwise Focus 2. This selects one weapon, not a simultaneous
dual attack. See [combat](combat.md) for capture and live availability checks.

### Shared equipment capacity

Anatomy authors named resource capacities, such as grips, independently of its
ordered sockets. Slot definitions can demand units from one shared resource;
a weapon profile multiplies those units by its grip demand. There is no implicit
humanoid grip pool. Equipment capacity is not inventory weight or a carrying limit.

The sum of minimum possible demands must fit the authored resource, or the
assignment is structurally invalid. Current demands can exceed that structural
minimum after expertise becomes inactive. Such a deficit does **not** delete or
automatically unequip items: it is derived from current anatomy, equipment and
active expertise. A fresh selection cannot attack using a weapon whose shared
pool is deficient, and new equipment cannot be added into a deficient pool.
Removing equipment remains an ordinary way to restore usable capacity. Already
prepared work retains its captured profile and follows its explicit live checks;
a new sheet is not substituted into a paid plan.

The owners are [capacity validation](../../crates/simulation/src/equipment_capacity.rs),
[loadout admission](../../crates/simulation/src/interactions.rs),
[derived weapon selection](../../crates/simulation/src/creatures/derive.rs) and
[invocation weapon capture](../../crates/simulation/src/abilities/weapon_selection.rs).
[Capacity witnesses](../../crates/simulation/src/equipment_capacity_tests.rs)
cover aggregate minimum rejection, expertise-loss deficits and independent pools.

## Armor and corrosion

Ordinary armor uses an explicit equipment capability, for example
`armor = { physical_reduction = 2, material = "metal", rustproof = false }`.
Its base Physical protection is additive damage reduction for Impact and Keen,
rather than a Physical-defense bonus. A magical armor ward adds its own reduction
to an explicit allowlist: Energy, Impact, Keen, Spirit and Vital. Psychic is a
separate damage category and receives no armor ward. Future categories do not
join this allowlist automatically. Explicit Psychic reductions and immunity still
work through the ordinary damage kernel. Damage reduction does not prevent
separate effects such as confusion, attribute loss or hit-die drain.

An item definition is shared and immutable. Each actual item owns eight explicit
condition fields: corrosion, curse, permanent protection, physical armor penalty,
magical armor ward, signed weapon hit enchantment, signed weapon damage
enchantment and signed wearable potency. Physical protection is
`max(base - penalty - corrosion, 0)`; the nonnegative ward is added separately. Penalty and corrosion are each bounded by
the original armor contribution; the total effective reduction is checked against
the numerical limit. No negative magical ward or hidden enchantment debt exists.
Nonmetal, intrinsically rustproof and non-armor items cannot carry corrosion.
Runtime protection does allow existing corrosion: it prevents future loss without
repairing it. Physical penalty and curse are independent; uncursing repairs neither
penalty nor corrosion. Identified displays distinguish these facts rather than
pretend they form one signed armor bonus.

The exact item state survives transfer, dropping, death, recovery and re-equipping.
Two copies of one definition can differ. Equip, restoration and actor-known
previews use one effective-item fold. A change folds the existing permanent
compiled sheet without walking hit-die rows. Signed weapon damage stays signed
until final damage modifiers are combined, so negative values are not prematurely
clamped before ordinary skill bonuses.

The generic `corrode_armor` effect names an equipment-slot kind and a positive
bounded amount. It selects the first equipped socket of that kind and reduces
remaining rustable physical plating by that amount, floored at zero. The magical
ward remains intact.
Empty, nonmetal, intrinsically rustproof, protected and physically exhausted armor
are no-ops. Corrosion is an isolated effect payload: the exact item witness and
actor sheet publish in the enclosing transaction. Later strikes see the new
protection. No hidden inventory identity appears in its public result. CO1 uses
one attempt per source occurrence after a successful natural contact, including
zero-damage contacts.

Generic `alter_equipment` effects freeze the selected equipped item IDs, sockets
and condition witnesses at paid preparation. An empty selection remains empty;
a replacement or independently changed target cannot inherit the old effect.
Multiple alterations in one payload validate original witnesses once and then
apply sequentially to the candidate. Weapon enchantment chooses hit or damage
with equal probability only when an eligible weapon was selected. Armor enchantment
adds one ward point and removes curse; weapon enchantment adds one point on its
selected axis and removes curse. Protection prevents later rust without repair or
uncursing. Remove-curse affects equipped items only, preserving negative weapon
bonuses and physical armor losses. All changes, randomness, payment and knowledge
share the enclosing rollback boundary.

These four scrolls consume their unit and action even with no eligible equipment,
and use explicit `discovery = "never"`: neither success nor a no-op learns the
scroll type. Paid armor/weapon identification records exact-instance details;
knowing a type does not reveal every copy's condition. Equipping armor observes
its numeric contribution, while a paid failed cursed removal learns only that
item's curse. Failed removal keeps the authored removal timing (three turns
for body armor under shared rules, one for weapons), an explicit TOR adaptation
from Rogue's shorter failed armor command. Hidden curse never changes admission
or visible preparation timing. Facts belong to the observing actor and may become stale; hidden
changes and transfers do not refresh them. AI cannot repeatedly attempt a removal
it already knows is cursed or inspect hidden bonuses while comparing equipment.

Authoring may omit `armor` and defaults intrinsic `rustproof` to false. Current
owned records require explicit nullable armor, all armor and condition fields;
unknown fields, positional aliases and impossible loss reject. There is no legacy
condition reader.

## Equipment birth and the finite Rogue catalog

The optional `equipment_generation` capability selects a generic birth policy;
class, identity and scenario name never implicitly select one. A generated weapon
can declare `{ kind = "weapon", curse_percent = 10, enchant_percent = 5,
minimum = 1, maximum = 3 }`; armor uses the corresponding `armor` kind. The two
percentages describe disjoint intervals of one percentage roll, not independent
chances. Each percentage is nonnegative and their sum cannot exceed 100. The
inclusive magnitude range must satisfy `1 <= minimum <= maximum <= 1000000`.
Validation also checks the equipment class and capability and every possible
result against the attack/protection limits before sampling.

Creation samples condition once in the existing item-birth transaction. The item,
allocator and rules RNG publish together; a failed placement leaves them unchanged.
A checkpoint, replay, transfer or equip does not reroll existing condition or
modify the shared equipment definition. Omitting the policy initializes ordinary
zero condition without a random draw. Explicit `kind = "authored"` also bypasses
the roll, with checked fixed `hit_enchantment` and `damage_enchantment` operands.
These operands default to zero only in authoring; canonical runtime and stored
policy records require both fields, and item definitions require an explicit
nullable policy field.

Rogue's generated weapons have a 10% curse chance with −1 to −3 hit, the next 5%
have +1 to +3 hit, and the remaining 85% are ordinary. Birth generation does not
change weapon damage. Armor has a 20% curse chance with a 1–3 physical penalty,
the next 8% receive a 1–3 magical ward, and the remaining 72% are ordinary.
These probabilities and ranges follow the pinned
[Rogue generation branch](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/things.c#L250-L269).
TOR clamps a generated armor penalty to its base physical reduction; the curse
remains even when that clamp produces zero. Favorable armor rolls add a nonnegative
five-category ward, rather than changing Rogue's AC statistic.

The production finite catalog contains these five nonlauncher melee weapons,
using the [pinned weapon dice](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/weapons.c):
mace 2d4, long sword 3d4, dagger 1d6, two handed sword 4d4 and spear 2d3.
All currently use 60 wind-up / 40 recovery ticks. This common timing and the TOR
skill/damage-category mapping are conservative provisional tuning, not Rogue
attack-speed claims. The dagger's typed profile adds Thrown 20/60 feet and
Offhand only while Knife expertise is active; its existing LightWeaponry 1d6 Keen
melee math and 60/40 timing are unchanged. A thrown dagger remains recoverable
on a successful hit. The spear likewise gains expert-only Throw after an ordinary
Spear expertise purchase: its explicit thrown payload is 1d6 Keen, while melee
remains HeavyWeaponry 2d3 Keen. Its 20/60-foot range, 60/40 timing and recoverable
hit policy are TOR handling choices. It is a singleton weapon, not ammunition or
a stack; existing exact-item holds, curse restrictions and landing rules apply.
Neither spear availability nor starting expertise is changed.

The `rogue.dart` and `rogue.shuriken` items are standalone stackable weapons,
not ammunition. Dart melee is fixed 1 Keen (not an invalid one-sided die) and its
throw is 1d3 Keen; shuriken melee is 1d2 Keen and its throw is 2d4 Keen. Both use
ordinary Knife expertise to enable Throw, 20/60-foot ranges and 60/40 timing.
These handling choices and damage categories are TOR adaptations; expertise is
not free. A successful hit consumes the thrown unit. Misses use ordinary bounded
floor recovery, including loss when no legal landing exists. Birth conditions,
exact-unit holds, condition-aware splitting/merging and conservative owner
knowledge use the same inventory mechanisms as other instance-sensitive stacks.
Both can appear in supplemental floor stock and treasure rooms, one unit per
successful item attempt under the
[provisional live policy](scenario-packages.md#live-rogue-content-policy).
Starting kits and existing weapon mechanics remain unchanged. Authored quantity
remains explicit; no source birth-stack range is silently adopted.

The shortbow uses LightWeaponry, a base TwoHanded requirement and expert-only
Quickdraw. Its launcher range is 80/320 feet; incidental melee deals fixed
1 Impact, without a damage die. The separate stackable arrow supplies 2d3 Keen
from the pinned Rogue launched payload, replacing the bow's incidental melee
base damage. Arrows have no equipment socket and begin mundane without a new
birth-condition roll. One arrow is consumed on a successful hit; misses use the
shared recoverable-landing policy, which may lose a unit when no legal landing
exists. Shortbow birth conditions use the ordinary generated-weapon policy.

Traits and short/long ranges follow the selected
[Plotweaver Open Alpha v0.3](https://drive.google.com/file/d/11v0ujMTuXNdwkHEPAcPZqY29I3ZdmlJn/view)
and [Stormlight Starter Rules](https://www.cosmererpg.com/s/SL015_Stormlight_StarterRules_Digital.pdf)
mapping. Shared 60/40 timing, TOR checks, damage-category mapping and the
combination with Rogue damage remain explicit adaptations, not source RAW.
Discreet is unsupported because no noncombat consumer exists; no item authors it
as a functional trait.

Knife and Shortbow are equipment expertise choices. Either can use the ordinary
HD advancement point or an earned permanent-Intellect entitlement; neither is
freely granted to the player, a species or an NPC. Existing NPC purchases remain
explicit and contain no new expertise choices. Their combat-benefit assessment
is 10 points each, provisional TOR tuning over the shared zero expertise baseline.
No pure-grant talent is added or counted twice.

Rogue defaults grant actual Throw and Launch abilities, tagged Attack and Ranged,
with exact projectile delivery and selected-weapon timing. Both are non-basic
attacks and add no base resource or supply cost. The shared secondary-weapon
Focus cost still applies when selecting that socket. Ordinary item ownership,
profile eligibility, expertise and available capacity still determine whether a
particular action can be prepared.

The eight armor identities are leather armor, ring mail, studded leather armor,
scale mail, chain mail, splint mail, banded mail and plate mail. Their base physical
reductions are respectively 1, 2, 2, 3, 4, 5, 5 and 6. This small monotone ladder
preserves the [source armor ordering and ties](../research/rogue-5.4.4/content.md#armor-8)
and the existing ring-mail value while limiting the initial spread of additive DR;
it is provisional TOR balance, not an equivalence between AC and damage reduction.
Only leather is nonmetal; the other seven are rustable metal absent protection.

Both starting kits use explicit authored policies. The mace's original +1 hit
and +1 damage now live on that exact instance; its immutable neutral 2d4 definition
and identity match the generated mace. Effective starter attack values and 60/40
timing are unchanged, and ring mail retains 2 base physical DR with zero condition.
Starting equipment consumes no birth randomness. Starting and generated archetypes
may share an identity while differing in birth policy; their underlying equipment
behavior must still agree exactly.

The equipment stages retain their existing relative order: `rogue.equipment`,
`rogue.equipment_scrolls`, `rogue.ammunition`, then `rogue.rings`. The complete
current builder, including supplemental stock and treasure stages, is specified
by the [live Rogue content policy](scenario-packages.md#live-rogue-content-policy);
this equipment catalog is not a second stage-count registry.
Each makes one placement attempt per first-entered floor from depth 1, with no
replenishment or retry after a recorded skip. Equipment and scroll attempts have
quantity one; ammunition attempts place one finite six-arrow stack. The gear
stage has fourteen equally weighted choices including shortbow; the scroll stage
has enchant armor, protect armor, enchant weapon, remove curse, identify weapon
and identify armor, also equally weighted.
These finite rates and equal subtype weights are provisional TOR tuning, not
Rogue's complete item probability table. Shortbow changes the existing equipment
pool's choices and its resulting outcomes; it is not a guaranteed bow per floor.
Earlier stage IDs, order and choices remain unchanged, preserving their
independent generation streams. The four alteration scrolls use `discovery = "never"`; the two class-identification scrolls use
`discovery = "on_use"` and no direct effects.

## Paid item work and finite supply claims

One unit of a supply stack provides one unit of its named supply. Supply identity
participates in physical stack equality, cloning and saved item definitions.
Equipment cannot also be supply content, and unknown supply references reject
at package compilation and item validation. Authoring may omit `supply`; owned
item records require the field with explicit null when it is absent.

Finite ability costs, drinking and eating use the same item-owned authority. Starting
preparation holds exact item IDs under its original issued intention; merely
admitting an intention to the queue does not reserve them. A stack's
physical quantity is authoritative; preparations retain references, not a second
quantity balance. The bounds are 8 distinct cost rows, 32 selected fragments per
action and 64 holds per stack. Ordinary transfer can move only unheld units.
Forced physical movement, lost access, invalidated work and death retire the
whole unpaid claim. A valid first resolution spends once even when the effect
misses, is immune or would heal an already-full target. Paid strike tails keep
their historical selection without requiring deleted sources to remain loaded.

Selection defaults to known carried supplies. An explicit selection may instead
name disclosed ground stacks within body contact when the authored cost permits
it. It cannot reach another inventory or substitute a different same-kind item.
Views expose known supply identity and physical quantity, never holds, free-unit
counts, claimant identities or private preparation state. Ground supply contact
is separate from pickup reach. Frozen/detached records retain reciprocal
actor/item claims; incomplete or contradictory restored relationships reject.

Under the shared default rules, body armor changes take three actor turns;
other equipment changes, drinking, eating, reading and zapping take one.
Ordinary Zap uses [item timing](magic-items.md#zap-timing), not its underlying
ability definition's preparation/recovery pair. Equip/remove multipliers belong
to each rules-defined slot, and consume timing is a ruleset constant; validated
scenario overrides can change these defaults. Effects apply only on completion. Equipped armor protects during
removal, occupied slots require removal before replacement, and equipped items
cannot be dropped. Positive received Health loss or a newly visible hostile
pauses work, preserving
progress and the original intention identity. A resume accepts the current threat baseline.
Waiting preserves progress; a different successful action can discard it. Explicit
cancellation of item preparation is rejected. Death clears equipment and drops each
remaining carried stack once alongside the corpse.

Consumables contain a sequence of the same `heal`, typed `damage`, restorable
attribute/maximum-Health loss or restoration, condition
application and resource-restoration effects used by abilities. Definitions are validated before mutation. Healing caps at maximum
health; damage uses effective reductions and immunities. A lethal effect ends the
sequence. Exactly one unit is consumed on completion. Observable health effects
teach identity to the actor, even when the consumed stack disappears; an
unobservable effect does not identify it.

The optional observation `interactions` contains the controlled actor's slots,
preparation and carried-item affordances, independently of combat attributes.
Equipment statistics are disclosed only for known identities. Item effects and
other actors' inventory are absent. This state reconstructs exactly through full
observations, deltas, checkpoints and replay. The validated
`scenarios/tests/interactions` package and `test_interactions_process.py` cover
starting gear, timing, disclosure, consumption and checkpoint restart.

## Implementation and test entry points

- [Interactions](../../crates/simulation/src/interactions.rs): exact item work,
  equipment admission, completion and private affordances.
- [Armor and corrosion](../../crates/simulation/src/armor.rs),
  [equipment birth](../../crates/simulation/src/equipment_generation.rs) and
  [magic alterations](../../crates/simulation/src/equipment_magic.rs): instance
  condition, atomic mutation and immutable definitions.
- [Equipment modifier tests](../../crates/simulation/tests/it/equipment_modifiers.rs),
  [capacity tests](../../crates/simulation/src/equipment_capacity_tests.rs) and
  [equipment alteration tests](../../crates/simulation/src/abilities/equipment_magic_tests.rs):
  independent rule and boundary witnesses.
- [Physical projectile planning](../../crates/simulation/src/abilities/projectile.rs),
  [release](../../crates/simulation/src/abilities/projectile_release.rs) and
  [execution](../../crates/simulation/src/abilities/projectile_execution.rs):
  exact-unit, frozen-profile and live-collision authority. Their lifecycle is
  described in [combat](combat.md), rather than duplicated here.

These are coverage entry points, not a claim that tests have been run for the
current documentation edit. Current qualification belongs to [status](../status.md)
and the [testing policy](../development/testing.md).
