# Items and character knowledge

Items support quantity-aware pickup/drop, compatible stacking, character-owned
identity knowledge, anatomy-based equipment, timed consumption with explicit edibility,
readable scrolls, charged directional devices, and ordinary currency/theft transfers.
Capacity and containers remain deferred. NetHack informs the interaction style;
these are this project's explicit rules, not a claim of exact NetHack behavior.

## Ordinary play

After setting the server token as described in the [README](../../README.md), run
`cargo run -p tor-server --bin tor-server -- --scenario scenarios/tests/items --character 1 --save saves/items-demo.db`
from the repository root with a fresh save path, then connect either client
as described in [text play](../play/text.md) or [ASCII play](../play/ascii.md). This ordinary package demonstrates stacks and
concealed potions without wizard mode. Use the opaque `#id` shown by the client
when selecting between alike stacks; these targets are scoped to the observer.

Text and adventure commands accept `take [quantity] <name or #id>` and
`drop [quantity] <name or #id>`. `get` aliases `take`. Omission (or `all` before
the noun) transfers the entire selected stack. Numeric quantities must be positive
and fit an unsigned 64-bit integer. Ambiguous names require selection; a requested
quantity stays attached through adventure clarification and walking to an item.

ASCII uses G for pickup and D for drop; stairs use `<` and `>`. A single one-unit
item transfers immediately. Otherwise Up/Down selects a stack, typed digits set
the quantity, Backspace edits it, Enter confirms, and Escape cancels. Blank means
the entire stack. Inventory and ground lists show counts. Inspection is free.

Pickup requires the actor's current ground cell; drop requires ownership and
places items at that cell. Each successful transfer costs half the actor's turn,
rounded upward, independent of quantity. Zero, excessive, overflowing, unavailable,
and unauthorized requests do not partially mutate state or spend simulation time.

Only explicitly stackable items merge. The complete immutable definition, exact
per-unit condition and charge state must match. Corpse provenance is never
interchangeable stack state. Equipment and devices remain nonstackable; independent
ammunition payloads can be stackable without acquiring equipment sockets.

Ordinary fungible stacks retain the lowest matching destination ID. Instance-sensitive
stacks, such as ammunition with hit/damage enchantments, instead retire the matching
destination and allocate a fresh result ID. Held or equipped instance-sensitive
destinations are skipped, preserving exact pending claims. A whole transfer without
a merge retains its ID; a partial transfer preserves the source remainder and its
holds. Only already-free units move. The ordinary transfer validates those exact
witnesses before cancelling the acting character's previous preparation and
releasing its claims, within the same atomic action.

Physical equality does not identify either input. On an entitled pickup/drop split,
only the acting character's stored partial instance facts can follow the new ID.
On a fresh merge, only fields remembered equally for both inputs survive in the
acting character's new record; unknown or conflicting fields remain unknown. Numeric
knowledge is currently one composite field. This compares memories, never hidden
condition values. Other observers and stale retired-ID memories are untouched;
ordinary owned transfers such as theft do not copy the victim's private facts.
If bounded memory is full, the new inherited record is omitted without blocking the
physical transfer. Explicit identification retains its existing full-memory behavior.

Retired IDs are not reused. History records source ID, actual result ID and
transferred quantity, rather than the merged stack's total. Rewind restores the
saved items with their existing identities, but keeps allocation high-water marks
beyond the abandoned future. New stacks cannot reuse IDs from that future; see
[persistence and rewind](../architecture/persistence.md).

### Independent projectile definitions

The development schema gives ammunition an independent projectile capability:
exact launcher-family ID, the shared typed base-damage expression with a required
primary component, and deterministic hit recovery. A launcher's effective weapon
profile owns its family and short/long range. Payload and launcher families must
match exactly. Both source compilation and direct placement/restoration validate
damage descriptor references. A dual equipment/projectile definition validates
both capabilities and remains subject to equipment nonstackability.

Ammunition supports homogeneous birth, curse and signed hit/damage enchantments;
plain ammunition cannot acquire armor penalties, armor enchantment, corrosion or
protection. This foundation does not yet expose ordinary launch/throw actions or
production ammunition content. The planned Rogue arrow policy consumes ammunition
on successful hits and permits miss recovery; durable thrown weapons remain
recoverable after hits. No percentage breakage rule is implied.

## Timed equipment and effects

The server accepts `equip` (carried item plus anatomy slot index), `unequip`
(carried equipped item) `drink` (carried potion) and `eat` (carried explicitly edible item) through ordinary intention
admission. Targets are opaque observer-scoped inventory references. Text accepts
`equip`/`wear`/`wield`, `remove`/`unequip`, `drink`/`quaff`, and `eat` with a carried
item name or opaque target. Adventure resolves disclosed nouns with its existing
choice rules. ASCII uses W to equip, T to remove and Q to drink and E to eat; Up/Down and Enter
choose among items, and Escape cancels without spending a turn. These commands
act on one item at a time and have no quantity editor. Clients choose the first free
matching anatomy socket; replacing gear requires separate removal. Both clients
use the shared `client-common` item checks, while the simulation validates the
submitted action against authoritative state. Direct text offers opaque choices
for ambiguous names; adventure uses disclosed order for indistinguishable stacks.
Selection clears on cancellation, changed observations, control loss and disconnect.
Completion narration belongs to the owning actor, handles noncombat equipment
and the last consumed unit, and does not repeat stale observations.

Anatomy is an ordered slot list; duplicate kinds provide distinct sockets, such
as two rings. Archetypes and actors can declare `anatomy = { slots = [...] }`.
Character declarations supply their own anatomy. Item archetypes optionally
provide `equipment`, `consumable`, or a typed catalog `supply` identity; placements use `equipped_slot` with
`carried_by` for starting gear. Invalid classes, counts, slots, conflicting gear
and effect definitions are rejected. Multiple matching sockets may hold weapons;
the highest equipped weapon-capable socket supplies the selected attack. Other
equipment still contributes its modifiers. Simultaneous dual attacks are not implemented.

### Armor and corrosion

Ordinary armor uses an explicit equipment capability, for example
`armor = { physical_reduction = 2, material = "metal", rustproof = false }`.
Its base Physical protection is additive damage reduction for Impact and Keen,
rather than a Physical-defense bonus. A magical armor ward adds its own reduction
to an explicit allowlist: Energy, Impact, Keen, Spirit and Vital. Psychic is a
separate damage category and receives no armor ward. Future categories do not
join this allowlist automatically. Explicit Psychic reductions and immunity still
work through the ordinary damage kernel. Damage reduction does not prevent
separate effects such as confusion, attribute loss or hit-die drain.

An item definition is shared and immutable. Each actual item owns seven explicit
condition fields: corrosion, curse, permanent protection, physical armor penalty,
magical armor ward, signed weapon hit enchantment and signed weapon damage
enchantment. Physical protection is `max(base - penalty - corrosion, 0)`; the
nonnegative ward is added separately. Penalty and corrosion are each bounded by
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
remaining rustable physical plating to zero. The magical ward remains intact.
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

### Equipment birth and the finite Rogue catalog

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
on a successful hit.

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

Production keeps `rogue.equipment` and `rogue.equipment_scrolls` after the eight
preceding population stages, then appends `rogue.ammunition` and the finite
`rogue.rings` stage, for twelve stages.
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

One unit of a supply stack provides one unit of its named supply. Supply identity
participates in physical stack equality, cloning and saved item definitions.
Equipment cannot also be supply content, and unknown supply references reject
at package compilation and item validation. Authoring may omit `supply`; owned
item records require the field with explicit null when it is absent.

Finite ability costs, drinking and eating use the same item-owned authority. Successful
preparation holds exact item IDs under its original issued intention. A stack's
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
other equipment changes, drinking, eating, reading and zapping take one. Equip/remove multipliers belong
to each rules-defined slot, and consume timing is a ruleset constant; validated
scenario overrides can change these defaults. Effects apply only on completion. Equipped armor protects during
removal, occupied slots require removal before replacement, and equipped items
cannot be dropped. Damage or a newly visible hostile pauses work, preserving
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
not accelerate Focus or Breath recovery. Each worn ring still adds its own food
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

## Reading, identification and charged devices

Read and Zap are explicit item capabilities, independent of an item's name or
physical class. `read <carried name or #id>` spends one scroll unit at completion.
`zap <carried name or #id> <direction>` reserves the exact nonstackable device and
spends one charge on resolution. Devices keep quantity one; each instance owns
its charge balance, sampled once on creation. Empty devices retain the same
unknown affordance and take the same time, but launch nothing and cannot underflow.
No target, a successful dodge or immunity still spends a charged launch. Pausing,
resuming, dropping, death, checkpoint restoration and transaction rollback reuse
the existing exact-item hold and preparation rules.

An unknown identify scroll is consumed before its permitted class set appears.
Its owner gets one persisted choice among at most 64 current carried items in that
class set. `identify <name or #id>` resolves it; `identify cancel` declines without
refund. Both require the original opaque choice token. A stale, wrong-class or
other actor's target rejects atomically. Another Read cannot bank another choice;
death clears it. Declining is an explicit TOR usability adaptation: pinned Rogue
insists on an eligible selection. Reading while blind is permitted, as in that
source. ASCII uses F6 to Read and F12 to select a device and direction. Escape
before submission cancels freely; Escape in the paid choice declines the spent
scroll. R still releases control and Z still opens abilities.

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
owned-HD ledger. The source's hallucination cure remains unsupported.

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
Sustain-strength uses the shared worn attribute-loss protection above. Poison's
hallucination cure and full source presentation remain unsupported; no bespoke
resolver substitutes for those remaining gaps. The seven concealed potion identities have
seven distinct appearances. The existing finite magic-potion stage chooses equally
among healing, confusion, blindness, poison, see-invisible and extra healing for one unit;
restore-strength retains its separate stage. These are provisional TOR availability
weights, not the source distribution. Stage IDs, order and attempt bounds are unchanged.

The opt-in Cancellation and Magic Missile wands use first-body
UntilObstruction delivery, not the bounded Sight profile of the existing utility
wands. Both start with 3–7 charges and remain directional when identified.
Cancellation has no save or random roll and never discovers its own type;
Magic Missile discovers on charged use and has one Agility/Lore save with
1d4+1 Energy damage on failure through ordinary bonus/reduction rules. See
[the player behavior](../play/rogue.md#opt-in-cancellation-and-magic-missile) and
[the source contract](creature-rules.md#shared-ability-rules).

Invisibility, haste-monster and slow-monster wands use a directional contact ray:
the first physically encountered creature gets one Agility-versus-Lore dodge,
and that contact ends the ray on either outcome. These rays never reflect and
never reach a second creature behind the contact. Six-cell reach, 100/100 base
prepare/recovery ticks and the opposed check are explicit TOR adaptations of the
[source's utility branches](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/sticks.c),
which have no equivalent save. They have 3–7 initial charges, pay exactly one
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

The eight new living-dungeon actual-process contracts have passed, including
source-free recovery. Complete required verification and final cross-platform CI
remain pending; these focused results do not establish balance or complete-run
qualification.

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
utility ray, the shared device timing of 100 preparation ticks with no additional
recovery, and 3–7 initial charges. Recipient
Speed + Agility opposes frozen caster Intellect + Lore; ties dodge, and contact
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

## Quantity expressions

Shared quantity operands describe positive item units, independently of bounded
combat dice. `QuantityExpr` has three strict tagged forms:

- `{ kind = "fixed", quantity = 1 }`
- `{ kind = "uniform", minimum = 2, maximum = 61 }`
- `{ kind = "depth_scaled_uniform", minimum = 2, maximum_base = 51, per_depth = 10 }`

The last form computes the inclusive upper bound as
`maximum_base + per_depth * depth`, with checked u64 multiplication/addition. A compiled `QuantityRange`
requires `1 <= minimum <= maximum`; reversed, zero or overflowing ranges reject.
Rogue's gold expression is therefore uniform over `2..=51 + 10 * depth`.
This is an item quantity, not a damage roll or a bounded dice-pool workaround.

The consumer supplies context explicitly. Birth stock resolves at the birth floor's
depth; theft resolves the current floor depth when preparation freezes its plan.
A later transfer, pause or restart does not recalculate the captured range.
Nongenerated contexts must supply an explicit fixed amount or bounded range;
there is no floor-one, zero-depth or global-depth fallback. Resolving the range
is separate from sampling it. A fixed range consumes no random word; random
sampling and state changes belong to the consumer's atomic transaction.
Population authoring also accepts an integer as fixed-quantity shorthand. That
source convenience is not a compatibility reader or an alias for strict compiled
and saved records. Ordinary materialized stacks retain one concrete u64 quantity.

## Theft and gold

`magical` and optional `currency` are explicit item capabilities.
`currency = "gold"` marks ordinary stackable Gold units; the coin appearance class alone
does not grant currency behavior. Gold uses normal pickup/drop, splitting,
merging, inventory and death-drop authority, with no purse or second quantity
ledger. Runtime magical status is true for explicit authored magic or nonzero
owned curse, protection, armor penalty/ward or weapon enchantment. Corrosion alone
does not confer magic. Class, name and identity knowledge do not grant it. Rogue's
restore-strength potion, Amulet of Yendor and authored +1/+1 starter mace are
magical; its ordinary ring mail is not. These capabilities participate in
compatible item definitions and remain private authoritative facts rather than
new item-view fields.

The shared theft effect is legal only as the sole effect of a checked rider.
Primary effects, failure branches and item-consumable effects cannot contain it.
Authoring may omit the optional relocation gate; canonical saved records require
its nullable field explicitly, without a compatibility default.
The source declares either a currency selector with an authored quantity
expression or a single magical-unit selector. The Rogue Trickster talents use
separate Cognitive checks:

- TR1 Filch Gold samples one source-sized amount, then caps the transfer by all
  eligible free carried Gold across the target's stacks. It does not multiply
  the roll by stack count or reroll once per stack.
- TR2 Filch Charm transfers one free unequipped magical unit, including the
  unique Amulet when carried by the victim. It does not need the thief to know
  the true identity and does not teach that identity after transfer. Concealment
  and each actor's prior identity knowledge retain their ordinary meanings.
- TR3 Vanish supplies the source's relocation gate. The compiled active flag is
  captured when theft is prepared; changing the flag later does not rewrite the
  frozen plan. Relocation is permitted only after a positive actual transfer and
  only to a legal cell on the same floor. A failed check, empty candidate set or
  zero transferred units cannot trigger it.

Live resolution rechecks carried ownership, equipment and exact free units.
Item-owned preparation reservations exclude held units from theft just as they do
from ordinary transfer. Candidate planning cannot expose the victim's inventory
or hidden item facts to the requesting actor. The transfer, any relocation,
identity allocation, random state and resulting lifecycle updates share one
atomic publication boundary; failure cannot leave a partial transfer or duplicate
an item. Stolen gold and magic items become ordinary finite inventory and drop
through normal death cleanup. There is no kill-time loot creation or restitution
ledger. The unique objective's independent entry-relocation helper is unchanged.

These contracts add no new command, client field, theft menu or 3D feature. See
[status](../status.md) for the theft/gold batch's outstanding validation; existing
item tests below do not by themselves qualify this new behavior.

## AI item decisions

AI healing uses the acting actor's own identity knowledge. At half health or
below, an AI prefers a carried, known potion containing only positive healing
effects before fleeing or attacking. It does not infer effects from an unknown
appearance, borrow a human's knowledge, drink known harmful mixtures, or spend a
potion at full health. This uses ordinary timed item work and autonomous intention
execution. If no suitable potion exists, ordinary AI can select a granted,
affordable self-targeted ability whose frozen effects contain only healing,
using its own known carried or disclosed touch-range supplies. Candidate previews
use physical quantity only. At actual execution, finite-item contention selects
an ordinary validated Wait through the same admission and replay path; rejected
supply preflight itself spends no time or resources. The `ai-interactions` package
and actual-process tests cover known healing, knowledge isolation, consumption
timing and restart. AI equipment
replacement and nearby looting use the same ordinary action path.

Known gear must strictly improve the actor’s derived combat capabilities or
reduce effective basal nutrition use, without losing attack speed, damage kinds
or typed protection. Nutrition savings count only for actors with Required
nutrition and use the effective rate floored at zero; exempt actors gain no
nutrition benefit. Extra food upkeep does not veto an otherwise strict combat
upgrade. Candidate evaluation uses
the production modifier fold on a private loadout snapshot. Free matching sockets
are considered before replacement; removal completes through ordinary timed work,
and the upgrade is recomputed afterward. AI avoids gear work while fleeing or
adjacent to a visible hostile. Attack comparison uses the actor’s check score,
Speed-adjusted phases and damage grouped by category/descriptor. A clipped raw
mean ranks rolled damage; it does not predict rolls or read an enemy’s hidden
protection. Replacing fire with cold is a tradeoff, not a straight upgrade.

Loot candidates must be visible and known: useful gear, or a positive restorative
when none is carried. They must lie within three remembered route steps. Distance
then item ID orders candidates; both the destination and first step avoid visible
hostile reach. Pickup takes one unit, preserves the remainder and reuses the
existing route search. Unknown appearances never authorize these useful-item
decisions. An independently authored `gold` interest permits known reachable Gold pursuit and ordinary pickup,
after self-preservation and retaliation and before idle exploration. It does not
infer hidden currency from an appearance, imply general looting permission or
change the actor's hostile acquisition and memory contracts. See
[behavior defaults](scenario-packages.md#behavior-defaults-and-ai-overrides).

Physical projectile AI first chooses one exact request from identified type
capabilities, active expertise, its own remembered curse facts, available grips,
known resources and disclosed target geometry. It considers at most 128 carried
rows and 16 launcher candidates; these are conservative NPC search limits, not
limits on the player's paginated chooser. It does not probe real attack numbers
or hidden occupants to choose another weapon. A paused paid projectile resumes
its exact frozen terms before fresh expertise, cooldown or funding gates.

Actual numeric capture happens once at execution. A valid launcher/ammunition
combination outside the engine's checked numeric budget produces a typed refusal
and the actor takes an ordinary positive-time Wait, without trying an alternative
weapon. Failed capture changes no items, payment or randomness. Successful Wait
publication increments an optional bounded diagnostic counter; it exposes no
hidden operands. Invariant failures remain errors. Ordinary journal replay uses
the same request and refusal boundary. Unknown curse is discovered only by the
paid blocked equipped throw; remembered cursed equipped weapons are then avoided.
Invisible intervening creatures still stop and resolve the actual projectile.

This supports appropriately equipped NPCs using their authored abilities and
expertise. The current Rogue roster receives no new ranged loadouts or free
expertise. Equipment acquisition still compares the existing melee utility;
it does not yet value a bow for its ranged option. General ranged-loot valuation
is a separate follow-up, so this does not imply new ranged behavior throughout
the Rogue roster.

## Authoring and disclosure

Archetypes declare an optional physical `class` (default `misc`); item placements
may override it for ordinary items. Concealed items retain their archetype class,
and appearance pools share one class and the same equipment-slot/drink/eat/read/zap affordances
so these facts cannot reveal a hidden effect.
Classes are appearance facts, independent of mechanics, and survive transfer,
identification, rewind and persistence. They participate in stack compatibility.
ASCII uses `)` weapon, `[` armor, `!` potion, `%` food/corpse, `(` misc/tool,
`"` amulet, `=` ring, `?` scroll, `+` spellbook, `/` wand, `$` coin and `*` gem.
An item pile draws the first disclosed item. Actors/terrain keep their existing
precedence, and remembered items keep the last disclosed class and grey color.
These classes do not implement food, spells, tools or other item-use mechanics.

Packages declare `quantity` (default 1), optional `stackable` overrides, and
string-valued `properties` on item placements. Archetypes default to non-stackable
and may declare `identity`, `appearance_pool`, `stackable`, `properties`,
`magical` and `currency`.
Instance properties override archetype properties by key. Non-stackable items
must have quantity 1; specific objective item instances must be non-stackable.
Properties are inert stack-compatibility metadata here; they do not activate item
effects, equipment, weight or capacity mechanics.

`appearance_pools.<key>` contains `appearances` and optional `confounding` (default
false). Archetypes sharing an identity must agree on its name, pool, physical
class, magical/currency capabilities, equipment and effects.
Sorted identity keys receive appearances shuffled deterministically from the game
seed, pool key, and appearance index with SHA-256. Ordinary pools require enough
distinct appearances. Explicit confounding pools permit reuse. Assignment covers
the manifest's archetypes, independently of character selection or current items.
Resolved appearances persist with authoritative item state and pinned inputs.

An item with an appearance pool conceals its identity until learned; other items
start known. Characters and other actors may declare `known_identities`. Learning an identity
identifies matching effects for that character, including other instances, without
identifying a different effect merely because it shares the appearance. Knowledge
survives dropping and persists independently of item ownership and existence.

The privileged `wizard identify <actor> <item>` command exercises knowledge
updates through the ordinary journal and rewind path. It requires existing wizard
authorization and marks the lineage through the existing wizard rules. Drinking
can teach identity through an observable self effect. Ordinary identification is
a single paid scroll continuation, described above; it is not a free identify endpoint. Rogue's concealed restore-strength potion
uses this same path: positive actual deficit removal teaches identity; a no-deficit
dose is consumed without learning. Its sole effect is bounded Strength restoration,
with no extra healing or resource refill; see [Rogue restoration](../play/rogue.md#strength-loss-and-ordinary-restoration).

An item view discloses its opaque `id`, `name`, `description`, physical `class`,
`quantity`, `appearance`, `identified` status and optional `asset`. Ground views
also locate disclosed occurrences and indicate reachability. Carried interaction
views provide the owner’s affordances and permitted known equipment details; see
the [wire reference](protocol.md). Hidden archetype/identity keys, properties and magical/currency capabilities are never
wire fields. History and wizard summaries do not serialize authoritative specs.
Items use the current protocol, save format, ruleset, and scenario validator;
older prerelease saves are rejected without migration.

## Verification and profiling

`scenarios/tests/items` is an ordinary validated scenario for actual text,
adventure, and native ASCII process tests. Simulation and server tests cover
conservation, atomic invalid actions, overflow, ownership, compatible/incompatible
stacks, confounding appearances, character selection, seeded assignment, exact
restart through journal/checkpoint paths, and rewind of knowledge.

Items workload v1 (`cargo run --release -p tor-server --example item_bench`) uses
16/1,000 items and 8/256 known identities, 20 samples each, 20 alternating partial
pickup/drop commands per sample. Each sample learns the final identity through a
timed wizard command, with wizard enablement outside the timer. It reports
construction, knowledge acquisition, transfer commands, explicit save
barrier and restart timing, real client-state application and ASCII canvas drawing
(without native-window presentation), disclosed/save bytes, observation/scene counts, item
candidates, stack candidates and knowledge checks. Authoring, validation, state
lookup between commands, and JSON reporting are outside command timing. Operation
counts include those untimed between-command state lookups. Client application and
drawing have separate timers; server observation extraction and JSON reporting
are outside those timers. Summarize with
`python scripts/item_performance_report.py <log>`; its validator rejects missing,
duplicate, malformed or nonfinite samples.

Retain scenario workload v1 unchanged for matching before/after comparisons.
Neither workload resolves the open long-run performance findings or replaces
real-client acceptance tests. Historical measurements found restart overhead and
variable persistence tails; their causes and targets remain open. See
[performance measurement and evidence](../development/performance.md) for dated results,
comparable workload boundaries and the retained adaptation release assets.


The implementation lives in [interactions.rs](../../crates/simulation/src/interactions.rs),
[items.rs](../../crates/simulation/src/items.rs) and
[AI item decisions](../../crates/simulation/src/ai_items.rs). Coverage includes
[AI regressions](../../crates/simulation/src/ai.rs) and
[actual interaction processes](../../scripts/test_interactions_process.py).
The non-graphical ring witness in
[magic-item processes](../../scripts/test_magic_items_process.py)
(`test_ring_wear_paid_union_choice_curse_and_source_free_restart`) covers ordinary
wear, paid ring-or-wand selection, exact-instance curse/knowledge isolation and
source-free restart; it does not depend on native-window tests.


## Explicit edibility

An item archetype may declare
`edible = { nutrition = { kind = "fixed", amount = 6 } }`, or use a bounded
rolled operand such as
`edible = { nutrition = { kind = "rolled", dice = { count = 1, sides = 400, bonus = 1099 } } }`.
The latter produces 1100–1499 through the shared neutral dice resolver; neither
example selects a production meal policy. Every possible result must be positive:
fixed amounts are 1–1000000, and rolled amounts use the shared bounded dice pool
with `count + bonus >= 1`. Each enclosing operand and nested dice record has one
strict named shape; scalar meals and positional records are rejected. Class,
name, appearance, identity and being a corpse never imply edibility. Equipment
and potion-consumable capabilities cannot coexist with edibility. An edible
item can also be an explicitly authored supply, sharing its one physical stock.
Appearance pools must share edible affordances so concealed identity cannot leak
through choices.

Eat freezes one declared fixed or rolled Nourish effect and the normal consumption
duration, holds exactly one unit of the selected carried stack, and spends that unit once at
resolution. It never chooses another stack or creates a second quantity ledger.
Cancellation, forced displacement and death release an unpaid hold; pausing
retains the existing preparation contract. A last unit may disappear while its
frozen effect and completion identity remain valid. Nourishment clears negative
reserve debt, adds the resolved amount, caps at the authored maximum and keeps
the digestion remainder. Rolled meals draw only at successful completion through
the persisted rules stream, after starvation and held-item validity checks. Queries,
admission, preparation, pause/resume, cancellation and checkpoint restoration do
not draw. Full or exempt creatures still consume the selected unit and resolve its
roll even when the applied gain is zero. Item spending, the roll and state changes
share one atomic transaction; replay applies the canonical gain without rerolling.
The disclosed edible affordance reveals no operand, dice, bounds, sample or RNG
state, including after identification. See
[nutrition](creature-rules.md#shared-nutrition-eating-and-starvation) for hunger,
deadline ordering, and persistent death.
