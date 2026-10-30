# Rogue encounters, equipment and survival

For launch, presets, first purchases, persistent floors and the objective, start
with the [Rogue quickstart](rogue.md). This page is the player-facing mechanics
companion, not a second ruleset. [Combat](../reference/combat.md),
[progression](../reference/progression.md), [items](../reference/items.md),
[magic items](../reference/magic-items.md) and [perception](../architecture/perception.md)
own the exact contracts. Live stock follows the [package policy](../reference/scenario-packages.md#live-rogue-content-policy).

## Species idle policy

These are provisional TOR choices for all 27 authored species (26 monsters plus human), including
the player species. They are not a claim of original Rogue behavioral parity or
balanced encounter pressure. Human defaults apply only when a human is explicitly
AI-controlled; they do not automate the selected player's turns.

| Species | Idle | Doors | Care | Equipment | Useful loot | Gold interest |
| --- | --- | --- | --- | --- | --- | --- |
| Human, hobgoblin, centaur, troll, vampire, medusa | Explore | Yes | Yes | Yes | Yes | No |
| Orc | Explore | Yes | Yes | Yes | Yes | Yes |
| Leprechaun | Explore | Yes | No | No | No | Yes |
| Nymph | Explore | Yes | No | No | No | No |
| Bat, kestrel, griffin, emu, snake, rattlesnake, quagga, black unicorn, zombie, wraith, jabberwock, phantom, aquator, dragon | Explore | No | No | No | No | No |
| Yeti | Explore | No | Yes | No | Yes | No |
| Ice monster, Venus flytrap, xeroc | Wait | No | No | No | No | No |

All keep their authored acquisition disposition and 1,000-tick actor-active
memory. The shared `rogue_hostile` profile overrides only the flee threshold to
zero; species defaults use 25%. It cannot flatten the species' distinct tools or
idle choices. Explicit Wait prevents every idle move, including a detour to loot,
while still permitting useful underfoot actions if independently enabled.

Care and looting use ordinary finite items and the creature's own identity
knowledge. Useful pickup is bounded by known usable equipment, one healing
reserve and supplies for one payable use of a safe owned healing ability. Only
actors with required nutrition keep a food reserve; the current foe recipes are
nutrition-exempt. No hidden item effect becomes a useful-loot goal and excess
supplies are not collected. Leprechaun and orc separately seek known reachable
Gold, after self-preservation and retaliation and before ordinary idle behavior. Birth stock alone does not teach an item's identity or grant
a tactic. See [the shared policy](../reference/scenario-packages.md#behavior-defaults-and-ai-overrides)
for fallback and override authoring.


## Equipment, combat and care

Each selected character starts with an equipped mace and ring mail, three known
clean bandages and one known food ration. Inventory and the ordinary equipment
screen show the actual items. The kit is installed once for the run; reconnecting,
changing control, revisiting floors and reopening a save do not grant another kit.

Basic melee and Bind Wounds are available immediately. Use the usual
[text](text.md) or [ASCII](ascii.md) controls to explore, attack and choose abilities.
Foes use the same creature build, rolled strikes, damage and readiness rules as
you. Fatal effects earn XP and eligible future HD automatically; the new points
remain yours to spend. Future-source changes affect later growth and do not
replace the class or form already purchased on an existing HD.

Bind Wounds consumes one finite bandage through ordinary preparation, recovery
and cooldown. In text, `bind wounds me using clean bandage` selects the known
carried supply. Positive Health loss can interrupt preparation. Health also
recovers under the shared actor-active recovery rules; safe waiting and a
bandage are different actions. See [finite care](../reference/creature-rules.md#finite-supplies-and-universal-care).

Your ration is one finite item: eat it for nourishment or spend it on an ordinary
safe-place respec. Text uses `eat food ration`; ASCII uses **e**, then selects the
exact carried item. Fresh purchases and future-source selection do not consume it.
Replacement requires its ordinary quiet interval, finite ration and paid work,
and can be interrupted; see [respec](advancement.md#rations-work-and-interruptions).

Warrior and Mage start with 1300 reserve and digest one unit every 100 active ticks.
Each Health, Stamina, Focus or Mana point actually regained by periodic recovery
also consumes one reserve unit. Full pools cost nothing extra; direct healing and
restoration from abilities or consumables have no regeneration tax. Recovery
intervals and these costs are provisional and will need tuning together.
Stats show your reserve and hunger. Below 300 is Hungry; below 150 is Weak; at zero
or below it is Empty. Health recovery remains Normal in every band. Empty depletion
can cause Fainted, temporarily preventing voluntary action and pausing paid work;
status rows distinguish active conditions from any later protection. When a faint
ends, explicitly resume paused work or cancel eligible work under the ordinary
commitment cost rules. Paused Eat requires Resume or a successful ordinary
replacement; it has no explicit cancellation. Eating still needs time, and reaching
-852 causes persistent death.
A ration adds 1100–1499 reserve after clearing negative debt, capped at 2000. Frozen
or detached floors do not digest or catch up on missed faint rolls. All 26
selected foe recipes are explicitly exempt. These values and food distribution
are provisional; see
[nutrition](../reference/creature-rules.md#authored-empty-depletion-conditions).


## Strength loss and ordinary restoration

Rattlesnake owns Venomous Bite on its first of two racial HD; the second point
remains unspent. Its mean acquisition and 1d6 natural bite use the ordinary
combat rules. Only a successful contact delivers its separate poison exposure.
Poison immunity blocks the exposure; otherwise Strength + Athletics against its
authored DC can resist it. Failed resistance records at most one actual Strength
loss, without undoing the bite's damage. The same descriptor-owned resistance
applies to poison from traps, monster attacks and items; the source's display name
does not decide whether it is poison. Body, attributes and timing initially reuse
snake values as provisional tuning.

An unidentified potion has a neutral appearance. Pick it up and drink through
ordinary Drink/Quaff/Q controls. A potion of restore strength removes only recorded
Strength deficit, using a fixed 1000 operand equal to the canonical deficit bound.
It never adds bonus Strength or heals injury. Restored Strength may increase derived
maximum Health and therefore displayed current Health while injury stays unchanged;
ordinary paid time can separately allow natural recovery. Resource balances are not
refilled by the effect. Waiting, food, healing and respec do not erase the deficit.
A dose spends one unit. Positive actual restoration teaches its identity; drinking
at zero deficit consumes it without learning. Character-owned knowledge persists
after the last unit and through save/replay. See [item knowledge](../reference/items.md).

Poison potions identify when drunk. Poison immunity blocks their exposure;
otherwise failed Athletics resistance permits 1–3 restorable Strength loss.
See-invisible potions cure blindness and reveal invisible creatures through normal
illuminated sight for 808–892 actor turns. A second dose extends the remaining
duration. Effective use does not identify this potion. Both can occur in the
existing one-unit random potion stock. Sustain-strength rings prevent new
restorable Strength loss. The Hallucination potion identifies on use, lasts
808–892 base turns and changes observer-specific visual appearance each base turn.
Repeating it extends the same cadence; real interaction targets and previously
recognized stairs remain authoritative. Extra healing
rolls independent d8s per active HD, cures blindness and Hallucination even while
blind, and identifies on use; its
one/two-point overflow gain is limited by existing owned-HD headroom. See the
[exact potion mapping](../reference/items.md#reading-identification-and-charged-devices).

Utility wands of invisibility, haste monster and slow monster stop at the first
creature along the chosen direction, even if it dodges. They do not bounce.
A successful effect lasts until removed, and another copy does not stack.
Haste halves action time; slow doubles it. Applying the opposite wand removes
the old rate and leaves the target at normal speed until a later application.
These effects do not change the Speed attribute. Utility use does not identify
the wand. Status screens show these persistent effects as “until removed”.


## Spear throwing

A spear can be thrown after purchasing **Spear** equipment expertise through
ordinary advancement. Select the carried or wielded spear in Throw; it deals
1d6 Keen instead of its 2d3 melee payload and remains recoverable after a hit.
Its range is 20/60 feet. Curses, occupied grips, paid preparation and floor landing
rules still apply; this adds neither free expertise nor extra starter stock.


## Live content and layout policy

The live package appends `rogue.expanded_stock` and `rogue.treasure_room` after
all existing floor stages. Existing stage identities, relative order, weights,
starting kits and rules stay intact; the new stages supply additional finite
stock, not replacements. Frequencies are explicitly provisional conservative
activation choices, not a completed balance campaign or exact Rogue parity.
The [population reference](../reference/scenario-packages.md#live-rogue-content-policy)
owns the exact item choices, weights and counts. Failed placement remains a saved
skip; return visits and restarts never reroll or replenish these stages.

The dungeon also enables zero or one omitted room slot per floor and a 1-in-15
maze trial for each eligible dark room. Protected stairs, trap capacity and the
existing lighting policy still constrain eligibility. These stages change realized
geometry and may change legal population placements even for an old seed; preserved
stage streams do not mean seed-identical floors. See the
[generation reference](../reference/generation.md#optional-room-omissions).

Portable torches remain catalog-only. Polymorph can now appear as loot, but its
existing emu/snake/troll destination pool is not expanded. Warrior/Mage presets,
ordinary expertise, care, respec, nutrition, traps, sight and persistent paired
stairs retain their deliberate TOR behavior. The
[frozen earned-route comparator](../development/rogue-earned-route.md) is not a
balance or survival qualification for these newly enabled floors.


## Fruit, throwing weapons and treasure rooms

Fruit, darts and shuriken can appear in supplemental floor stock and treasure
rooms, but are not added to starting kits. Fruit is named
ordinary food: take it and `eat fruit` with the same one-unit nutrition rules as
a ration. Darts and shuriken are real weapons; acquire Knife expertise ordinarily
and select a carried stack in Throw to spend exactly one free unit. Wielding
requires a singleton, but throwing does not require wielding. Hits consume the
unit; misses may leave recoverable stock under ordinary landing rules. Dagger
recovery and arrow behavior are unchanged.

A rare treasure room adds loot and hostile births in one selected present room,
using next-depth actor context. Its foes use the ordinary species bands, AI and
growth rules, but need not be outside your sight. Treasure stock is additional to
ordinary floor stock, with independent bounded item and actor counts rather than
Rogue's coupled counts. The separate
[inert authoring example](../../scenarios/rogue-exploration/examples/treasure-stage.toml)
is still not imported; its larger counts and illustrative Hobgoblin/Orc bands are
not the live policy.
See [items](../reference/items.md) and the
[population schema](../reference/scenario-packages.md#first-entry-population).


## Magical staves and wands

Each magical effect chooses one physical form for the run: a metal-looking wand
or a wood-looking staff. The same appearance keeps the same effect within that
run; identification reveals its name. Both use the usual **Zap** action and
charges. Wield either for ordinary physical attacks: staves deal 2d3 Impact,
wands fixed 1 Impact, with no charge cost—even when empty. They occupy one grip
and cannot be thrown. Cursed wielded implements obey the same removal restrictions
as other equipment. The inventory category “wands and staves” and identify-wand
scroll include both; physical form selection does not change the magical effect
or its authored loot weight.


## Nothing, Light and Drain Life wands

Exact payment, topology, rounding, refusal and disclosure rules live in the
[current effect reference](../reference/consumable-effects.md#nothing-room-light-and-drain-life-devices).

`rogue.wand_nothing`, `rogue.wand_light` and `rogue.wand_drain_life` can appear
through supplemental floor stock. They do not replace choices in the original
eight-device stage and are not added to starting kits.

All three use the normal paid Zap action. An unidentified wand still asks for a
direction, so its hidden self-targeting behavior is not disclosed by the prompt.
Once its type is known, the client can use it on yourself without that direction
step. Remaining charges are not revealed merely by identifying the type.

Nothing starts with 3–7 charges. A charged use spends one charge and ordinary
action time, but applies no effect and does not identify the wand. Light starts
with 10–19 charges and identifies on charged use, including when nothing changes.
It illuminates the caster's actual room, including its canonical wall, ceiling,
floor and door surfaces across region boundaries—not every cell of a storage
region or every neighboring room. A passage or generated maze is a paid no-op;
an already-lit room also spends the charge without further change. Loading remote
surfaces does not give frozen creatures turns. Illumination persists in the save;
it is not a map-reveal, creature-detection or targeting command.

Drain Life starts with 3–7 charges and never identifies itself automatically.
With less than 2 current HP, the attempt spends ordinary action time but refuses
without spending a charge. With no eligible other creatures it spends a charge
without changing HP. Otherwise, for starting HP `H` and `N` eligible recipients,
you retain `floor(H/2)` HP and each recipient loses `floor(floor(H/2)/N)` HP.
The recipient set is fixed before applying losses; fractional loss rounds down,
including to zero, without suppressing attack-contact consequences.

Eligibility follows canonical topology, not sight, direction or distance. Room
interiors reach their own room; a doorway retains those room targets and adds
its affiliated passage-interior targets, not other rooms' doorways. A passage
interior reaches that passage and its affiliated doorways, not neighboring room
interiors. Direct Health loss bypasses armor and resistance. Living creatures
with Health are eligible regardless of allegiance; bodies without Health and
the caster are excluded. Frozen recipients do not receive turns. Hidden recipients and their count are not listed in ordinary output.
Paid results and current Health persist through source-free checkpoint reopening.


## Cancellation and Magic Missile

`rogue.wand_cancellation` and `rogue.wand_magic_missile` can also appear through
supplemental floor stock, each with 3–7 charges. The original eight-device stage
and starting kits remain unchanged. Both use ordinary paid Zap and require a
direction even after their type is identified.

These two wands follow physical geometry until the first living body or an
obstruction. They do not bounce or use sight as their range. A body without a
combat sheet still intercepts the ray; it cannot be skipped to hit someone
behind it. Loading distant geometry does not give frozen creatures turns.
A bounded work guard rejects an over-budget traversal rather than silently
turning it into a shorter hit or miss; this is not a gameplay range limit.

Cancellation suppresses explicitly marked special abilities and contributions,
clears current pure invisibility conditions and disguise, and never identifies
its own type. Innate invisibility is suppressed only when its contributor is
explicitly marked, as with the Rogue Phantom.
It performs no save or random roll and does not provoke pursuit merely by
cancelling the target. Ordinary attacks, owned advancement, defenses, anatomy,
flight, erratic movement and regeneration are not blanket-disabled. It removes
only holds whose sustaining special was lost, not unrelated paralysis.
Later independent invisibility can work again; another Cancellation can clear
that new application. There is no global veto on future invisibility.

Magic Missile identifies on charged use, even if no creature is damaged. At the
first combat recipient it makes exactly one Agility-versus-Lore opposed save,
not an additional launch attack followed by a second save. A failed save takes
1d4+1 Energy damage through the ordinary universal damage-bonus and reduction
rules. A successful save stops the missile without damage to a later body.


## Flight and linked holds

Bat and kestrel each own one animal racial HD, with 1d2 and 1d4 natural damage.
Bat is passive and has an authored 1/2 erratic-movement chance; kestrel is mean.
Both fly and carry no birth stock. Griffin owns thirteen magical-beast racial
HD, RG1 on HD 1 and Second Strike on HD 2, with 4d3/3d5 natural damage. It is
mean, flies and has a 20% finite birth-stock chance from the shared supply pool.
Flight permits unsupported paid movement but preserves clearance, imposed
velocity and Rogue's NPC abstract-stair prohibition.

Venus flytrap owns eight plant racial HD, remains mean and stationary, and buys
Clutch on HD 1 and Tightening Hold on HD 6. The creating contact deals zero
damage. Later qualifying Clutch or basic-melee primaries against that exact live
link increment its damage channel, including misses; damage still requires a hit.
Escape has base 100 preparation/100 recovery scaled by Speed and 400 actor-active
ticks of reapplication protection against that holder. A movement command while
held instead attempts paid Escape from the oldest active hold, without moving
or queuing movement after success. Explicit Escape can choose a different hold.
Forced displacement can
break the link; stationary does not anchor F against pushes. It carries no stock.
See [movement capabilities](../reference/creature-rules.md#locomotion-capabilities).


## Gold and theft

Gold is a finite ordinary inventory stack. Take and drop it with the same commands
and quantity controls as other items. Each present room gets one 50% first-entry
Gold chance, and each successfully placed leprechaun is born carrying finite Gold. Amounts use the
shared positive quantity range, from 2 through `51 + 10 * floor depth` inclusive;
birth amounts use the birth floor. There is no separate purse or repeat death roll.
A successful room chance can still skip placement when no legal cell is available.
Revisiting a floor or restarting does not replace spent or stolen stock.

Leprechauns (L) appear at depths 6–15. Their three actual fey HD own Filch Gold
(TR1) on HD 1 and Vanish (TR3) on HD 2, with one point unspent and a natural 1d1
strike, encoded as fixed one damage because a one-sided die has no variation.
The separate Cognitive-checked theft rider takes one rolled amount capped
by all free carried Gold. Its range is frozen from the current floor when the
attack is prepared. Nymphs (N), at depths 10–19, likewise own three fey HD,
Filch Charm (TR2) on HD 1 and Vanish on HD 2, with one unspent point and zero
natural damage. Their checked rider can steal one free unequipped magical unit.
L and N retain passive acquisition until ordinarily provoked.

Both successful thieves may relocate to a legal cell on the same floor after an
actual transfer. Failed checks, no eligible stock or fully held units cannot
trigger Vanish. Held reservations remain protected, and theft cannot take
equipped items. The concealed restore-strength potion and the Amulet of Yendor
are magical; ordinary ring mail is not. Theft does not identify a concealed item
for either actor. Stolen items and Gold remain ordinary inventory, recoverable
through normal death drops. The Amulet remains the same unique objective item.

Orcs (O) appear at depths 4–13 with one racial body HD and one personal
Warrior HD (two total), purchased W02
Measured Cut, natural 1d8 damage and passive acquisition until provoked. Their explicit
Gold interest does not grant theft. These are provisional TOR builds and
behaviors, not a calibrated survival curve or original Rogue timing. See
[shared theft and Gold](../reference/items.md#theft-and-gold) for exact inventory
and preparation contracts.


## Gaze, invisibility and object guise

Medusa (M) enters at depth 18 with eight monstrous-humanoid racial HD and the
reptilian subtype. GZ1 Confusing Gaze is bought on HD 1; NA1 and NA2 on HD 2
and HD 4 unlock its separate 3d4/3d4/2d5 natural strikes. Its existing
Spellcasting skill binds to Presence. The gaze prepares and recovers for 100
ticks each, requires mutual sight within four cells, and checks Spiritual
with the ordinary 3d6 kernel. Success applies Confused for 300 active ticks,
followed by 400 ticks of target-wide protection. A 700-tick caster cooldown
starts at preparation. M is mean and has a 40% finite birth-stock chance.

Phantom (P), at depths 15–24, has eight undead racial HD, the incorporeal
subtype, one natural 4d4 strike, typed invisibility and the existing 1/5
erratic movement chance. It retains passive acquisition and carries no birth
stock. The incorporeal label grants neither immunity nor wall passing.

Xeroc (X), at depths 16–25, has seven aberration racial HD and the shapechanger
subtype. It buys MI1 Object Guise on HD 1 and MI2 Ambush From Guise on HD 6,
with one natural 4d4 strike and a 30% finite birth-stock chance. Its initial
stationary guise looks like an ordinary mace. Search and passive discovery use
checked private recognition; they do not globally break the physical guise.
Actual touch can trigger its readied ambush, not mere adjacency. Attacking or
moving breaks the shared guise; private recognition and physical release are
separate facts. The purchased MI2 supplies its qualifying guise attack advantage.
See [guise and recognition](../architecture/perception.md#object-guise-and-private-recognition)
for the exact receipts and attack boundary. Search does not reveal an invisible
phantom merely by querying nearby cells. Shapechanger itself grants no additional mechanics.

M/X stock uses the same provisional equal ration/bandage pool as other carriers.
Bodies, attributes, ranks, timing and AI priorities are provisional TOR tuning;
these source contracts do not establish encounter or survival balance.


## Rust and bouncing breath

Aquator has five racial HD and one zero-damage contact. A successful CO1 contact
reduces the equipped armor item's own physical damage reduction by one, to zero.
Ring mail starts with two reduction against Impact and Keen; armor does not
increase Physical defense. Nonmetal or rustproof armor is exempt. Item identity
and corrosion persist through unequipping, transfer and save/reopen.

Dragon has ten racial HD, fire immunity, purchased second/third natural strikes
(1d8/1d8/3d10) and Breath Weapon. Its Willpower 3 plus Breath rank 2 gives one
3d6 + 5 source roll for the ray. Each encountered creature rolls Speed + Agility;
a tie or better dodges with zero damage and lets the ray continue. Failure takes
6d6 Energy damage with the fire descriptor and intercepts the ray, even when
ordinary immunity reduces that damage to zero.

The ray visits at most six clear cells, reverses at walls and closed doors,
and passes open doors. Physical joins can rotate it; abstract stairs do not
carry it. Leaving and re-entering a creature's body requires another dodge with
one additional disadvantage for every earlier check against that same ray.
A contiguous multi-cell pass checks once. Positive edge cancels those penalties
before the ordinary three-extra-dice cap; a new ray resets the counter.
Preparation/recovery are provisionally 100/100 ticks, with a 2000-tick cooldown.
Later explicit Breath purchases at racial HD 15/20/25 raise the rank to 3/4/5;
these spacing values are provisional tuning. They use ordinary owned points
and the rank-five cap, without a separate HD-derived roll bonus.


## Living-dungeon pressure and awareness

An active floor has one wandering clock, even when its rooms cross region
boundaries. Its provisional delay is 6,700–7,300 active ticks, followed by one
1-in-6 trial every 400 active ticks. A winning trial starts another long delay,
including when the floor is full or no legal placement exists; no backlog of
monsters accumulates. At most eight **living wandering-born** monsters occupy a
floor. Initial inhabitants and Create Monster births do not count, and deaths
free wandering slots. Wandering excludes the source's I/L/N/F/X/D species,
places outside your current room and sight, grants ordinary kill XP, and creates
no carried birth loot. Its first location lead is a finite snapshot, not tracking.
Leaving an inactive floor freezes the clock; saving/restarting does not reroll it
or charge wall-clock time.
On a partly resident floor, an unknown placement candidate costs one bounded
missed trial; wandering does not preload remote rooms or favor resident cells.
A legal adjacent Create Monster placement does not require remote rooms loaded.
Living wanderers in detached rooms still count toward the floor cap.

The Stealth ring attempts stealth continually: **3d6 + Speed + Stealth** opposed
by **3d6 + Awareness + Perception**, with Perception winning ties. Its authored
one advantage is provisional tuning. Committed approach, adjacency and battle
cues own attempts; observing, planning, reconnecting and restoring do not reroll
an unchanged encounter. Automatic awareness neither borrows nor spends next-check
preparation tokens; persistent opponent-scoped modifiers still apply. Stealth
gates wake and AI acquisition, not physical visibility: this is not invisibility.
After sensory contact breaks, a renewed encounter may be contested.
An attack reveals its source to actual observers; listeners receive only a
snapshot of the source or impact location they heard. No manual Sneak command or stance is implemented.

Aggravation has a provisional 1-in-3 pulse trial every 500 wearer-active ticks.
A pulse gives eligible floor monsters the wearer's location **at that instant**,
not an identity or a continually updated target. It adds exactly one disadvantage
to Stealth, not attacks, Lore, Perception or Search. Duplicate Stealth/Aggravation
rings do not stack these capabilities; one advantage and one disadvantage cancel
without stopping pulses. Accrued pulse progress survives unequip/re-equip and
restart. Neither sound nor aggravation cancels magical sleep, identifies a hidden
actor, or activates an intact stationary Xeroc. Ordinary alerts expire under the
listener's authored memory, currently 1,000 active ticks.

These cadence/duration values are authored balance defaults, not a claim of
classic-Rogue timing or a calibrated safe survival curve.


## Searching, concealed entrances and traps

Perception uses Awareness and the shared 3d6 check. Concealed features retain
separate first-sight and first-adjacency passive opportunities, with their authored
distance/edge rules. Leaving, reconnecting and reloading do not reset spent
receipts. Sleep or incapacity does not spend an ineligible opportunity. The
[discovery contract](../architecture/perception.md#concealed-physical-features)
owns exact eligibility and distance behavior.

Use **Search** repeatedly when needed: each attempt pays its normal 50 ticks of
preparation and 50 ticks of recovery, including ordinary time and hunger costs.
Guise recognition is checked and private; Search never globally releases a guise. A worn ring of
searching runs the active discovery check free once per 100 actor-active ticks;
two searching rings do not double that cadence.

Finding a secret entrance reveals it only to that character. **Open** changes the
shared physical world; until opened, the entrance blocks sight and movement.
Ordinary room entrances remain doorless.

Finding a trap reveals its presence, not its mechanism or a safe route across it.
Text **examine trap**, or selecting a discovered adjacent trap with ASCII **;**
then **Enter**, learns its family without spending time. This explicit action is
unavailable during pending work; merely viewing the map never examines it.
Discovery does not disarm a trap. Deliberately moving onto a trap whose presence
that actor already knows gives one Speed + Agility check against its separate
authored avoidance DC (currently 12). Success avoids triggering it for that entry,
not all future visits. The attempt is part of the paid movement, never a free
retry; standing, viewing and reloading do not roll again. Another character's
knowledge does not help, and being pushed, falling or teleporting onto it gives no
avoidance check. A trap's actual activation can reveal its family to witnesses.

The eight families use ordinary shared mechanics:

- **Rust** corrodes susceptible worn armor. Nonmetal, rustproof and protected armor
  retain their ordinary protection; there is no additional resistance roll.
- **Sleeping gas** is explicitly a poison exposure. Poison immunity blocks it;
  otherwise Strength + Athletics against the authored DC (currently 12) can resist
  it. Failed resistance causes 200 actor-active ticks of Sleep. Magical sleep is
  separate and is not made poison merely because it uses the Sleep condition.
- **Arrow** and **poison dart** first use ordinary attacks against Physical defense
  and normal armor. A dart must hit before its separate poison exposure can cause
  one restorable Strength loss; Athletics and poison immunity apply to that
  exposure, while sustain Strength still prevents the loss. A missed arrow leaves
  recoverable ordinary ammunition. Traps remain reusable, so arrow farming is
  possible.
- **Bear traps** restrict movement for 300 actor-active ticks, not all actions.
  Actually leaving the trap, including by teleport, frees the actor; hovering in
  place does not.
- **Teleport traps** use the same-floor relocation rules below.
- **Trapdoors** open permanently only after the destination floor is ready.
  Actors and loose items then pass through a real aperture under gravity, not
  teleportation. An occupied landing uses normal collision, never an alternate
  destination. An already-open hole cannot be defeated with an avoidance check:
  it requires an actual crossing or hovering capability.
- **Mysterious traps** have no invented harmful payload.

Flight and levitation avoid ordinary ground contact, not collisions imposed by
physics. Placement, discovery and avoidance DCs, resistance DCs and effect
strengths/durations are provisional authored choices, not a balance target.


## Teleportation and levitation

Teleportation scrolls and teleport-away effects choose a valid destination on the
same floor, using the complete floor rather than a loaded subset. A teleport-to
wand places its hit recipient in the exact cell beside the captured launch
position in the captured firing direction; blocked placement never searches nearby. A completed use still
spends its scroll or wand charge if placement fails. Forced teleport interrupts
unfinished preparation without refunding already-paid costs.

Teleport wands use directional, non-bouncing rays rather than selecting an unseen
target. The source uses Lore and each encountered creature can dodge with Agility;
the ray follows ordinary geometry and interception rules. Each worn teleportation
ring independently has a 1-in-50 chance every 100 actor-active ticks. Two rings can
both trigger. Sleeping time counts; frozen or detached time does not catch up.

A potion of levitation grants timed horizontal hovering, not flight's up/down
control. Repeated doses extend the condition. Ground pickup and stairs are
unavailable while levitating. A successfully completed drink can arrest an actual
fall before impact, preserving tangential velocity; it never undoes an impact that
already happened. Gravity resumes on expiry, so expiring over a hole means falling.
Normal levitation does not rise uncontrollably to the ceiling.


## Reconstructed roster and depth source checkpoint

The selected 26 species recipes use the shared compiler and character-sheet
pipeline. Centaur owns one racial body HD plus four personal Warrior HD (five
total). Second Strike belongs to its second personal class row (HD 3 overall),
and Third Strike to its fourth (HD 5 overall); its natural sequence is 1d2/1d5/1d5. Yeti owns four
actual racial HD, purchases Second Strike on HD 2 and uses 1d6/1d6; its cold
subtype retains descriptor immunity. Both explore while retaining passive
acquisition until actual hostile provocation. Their 15%/30% birth carry chances
follow the pinned reference, while the equal ration/bandage pool is provisional TOR scenario tuning, not original
loot equivalence. Birth creates one ordinary finite item, which drops with the
same identity on death and never restocks on revisit or restart. Tool permissions
come from the [species policy](#species-idle-policy), independently of birth stock.
Troll adds six actual racial HD with RG1 Regeneration on HD 1, Second Strike on HD 2 and Third Strike on HD 4; its other points remain
unspent. Its separately checked 1d8/1d8/2d6 sequence follows ordinary mean
acquisition, with exploration and 1,000-tick memory. Its reference 50% carry chance
uses that same provisional pool. The regenerative flag only permits RG1, which
supplies the existing −50% Health-recovery-period modifier; no RG2 is selected.
Troll's giant attributes (2/1/0/1/1/0), Physical defense +3, two-cell body of mass
250 and 100/100-tick melee reuse provisional authoring choices, without a
balance or earned-kit claim. Complete encounter and survival balance remain unfinished.

Jabberwock owns fifteen actual aberration racial-HD rows, with Second Strike on
HD 2 and Rending Pair on HD 8; thirteen points remain unspent. It has a heavy
2d12/2d4 natural sequence and passive acquisition. Rending Pair is a shared
purchased ability that imports that complete sequence and its natural timing,
then attempts one unchecked keen 1d6 rider immediately after the second qualifying
hit on the same target in that preparation. Protected zero-damage contact can
qualify; misses, whole-payload immunity and rider checks cannot. A lethal primary
skips the rider. The signature does not augment every other natural ability.
The generic talent requires Second Strike, eight total HD and the species'
`natural_sequence` eligibility flag. Draining its HD 8 owner removes the grant;
authored restoration restores the exact row, seed and purchase.

J has no flying or regeneration flag. Its 70% birth chance uses the same finite
provisional ration/bandage pool. Keen damage categories without descriptors,
attributes 2/1/0/1/1/0, Physical defense +3, large two-cell mass-250 body,
Heavy Weaponry/Athletics starting ranks, 100/100 natural timing and signature
AI priority 20 are provisional TOR inputs, not original Rogue damage typing or
a balance claim. All checks retain the shared 3d6 rules.

The ice monster purchases CH1 Freezing Touch on its first racial HD. Its `natural_melee` ability source imports the actual species-owned checked
contact, then its checked additional rider applies the shared Frozen condition for 200
actor-active ticks, then 400 ticks of target-wide protection. Keeping the current
application prevents refresh and chaining; the shared lifecycle pauses paid work,
preserves its prefix/operands/holds and requires human Resume after thaw. Nutrition
and recovery retain their ordinary active-time behavior. No ice-only scheduler or
fixed-damage replacement attack is introduced.

The wraith owns five actual undead racial-HD rows. DR2 Energy Drain costs the
HD 1 point, has no DR1 parent, and leaves four points unspent. W explores but
retains passive acquisition until ordinary hostile provocation. Its single keen
1d6 contact is followed by a separate natural-attack check against Spiritual that removes the victim's latest
actual HD and its purchases. There is no extra 1d10 loss, percentage proc, Feed,
flight, incorporeality or regeneration. W has zero carried-loot chance and appears
only at depths 14–23. Its standard undead d12 Health and immunities, attributes
2/2/1/2/1/1, natural Physical defense +3, mass 70, two-cell body, 100/100 timing and
signature priority 20 are provisional TOR inputs, not an AC/XP conversion.

A living victim regrows through ordinary kill-earned XP and its selected future
class source. Existing spent XP is never reused. ChooseAgain restores an empty
purchase slot with that ordinal's original Health seed; it does not restore its
old purchases for free. Rest, bandages and restore-strength potions cannot restore
an HD. Losing the final HD is permanent death, and preserved injury can make an
earlier drain fatal too. Existing controlled-recovery and autonomous-survived-drain witnesses are distinct:
one is not evidence of an autonomous full recovery or balanced starter descent.
The [historical roster checkpoint](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/play/rogue.md#reconstructed-roster-and-depth-source-checkpoint)
preserves the exact fixture boundaries, budgets and past qualification record.
Current restoration ownership is in [progression](../reference/progression.md).

The vampire base recipe owns eight actual racial-HD rows: DR1 Drain Vitality on
ordinal 1 and RG1 Regeneration on ordinal 2. DR1 is a purchased natural-melee
signature: the species contributes its single keen 1d10 strike, and successful
contact permits a separate natural-attack check against Spiritual for restorable
maximum-Health loss of 1d3. There is no Feed or drain-to-heal effect. Injury is
preserved and ordinary death/drop/XP handling applies. RG1 requires the species'
regenerative flag and halves the ordinary Health recovery period; it grants no
unconditional combat heal. The original Rogue regeneration flag's behavior is
still unverified, so this is the [accepted TOR adaptation](../plans/progression-design.md#drain-tree),
not [reference parity](../research/rogue-5.4.4/rules.md).

Vampire attributes (Strength/Speed/Intellect/Willpower/Awareness/Presence
2/2/1/2/1/1), natural Physical defense +7, two-cell humanoid body of mass 70,
heavy-weaponry/discipline starting ranks, 100/100 action timing and signature AI
priority 20 are provisional tuning inputs. The selected population attaches V's
reference 20% birth carry chance: one ordinary ration or clean
bandage with equal weights. This restricted pool is not Rogue's full item pool;
the vampire's separately authored species policy enables its useful tool actions.
Revisit and restart do not restock.
Earned-survival qualification remains open.
Depth 26 is not terminal, and no HD cap is a designed late-floor policy. Actual-kit
fights, earned advancement/escape routes and coupled food/recovery measurements
remain necessary to assess these provisional food, recovery, ability and stat costs.

Each species explicitly owns its acquisition disposition: K/E/S/H/R/Z/Q/F/T/U/V/G
are mean, while B/I/C/Y/W/J and the player species are passive.
The [idle policy matrix](#species-idle-policy) and 1,000-tick memory bound are
provisional. CH1 explicitly declares hostile contact, independent of its AI
priority; its strike and rider come from the shared natural attack pipeline.
Tool use follows species defaults with only explicitly supplied profile overrides.
The full 26-species roster is implemented. Scaling beyond current build/storage
capacity and broad integrated acceptance remain open.



## Monster-control scrolls

Use **Read** for Aggravate monster, Create monster, Hold monster and Monster
confusion. A read costs its ordinary action and consumes a scroll even when
placement or targeting has no effect. Aggravation alerts creatures to a location
snapshot; it does not reveal an invisible wearer. Creation can exceed the wandering
monster cap. Hold checks Lore against nearby engaged foes' Spiritual Defense and
ends when they are attacked, hit or miss. Monster confusion arms your next
successful natural, melee or projectile hit; a miss does not waste the charge,
and repeated reads do not accumulate charges. Confused foes use the same movement
rules as a confused PC, not random targeting or changed allegiance.

Stand on a dropped **Scare monster** scroll to make approaching hostile AI
creatures flee briefly, without a resistance roll. Mind-affecting immunity still
applies, and cornered creatures can defend themselves. Standing in range does not
keep refreshing the fear. The scroll's recorded location is the threat; its
occupant is not continuously tracked. Reading this scroll merely wastes it.

The first pickup of non-cursed Scare units makes them cursed. Picking up cursed
units crumbles them instead of collecting them. Stacks preserve this curse through
splitting, dropping and saves; remove-curse can clear carried Scare units.

Confusion's 2,000 active ticks and Hold/Scare's 1,000 active ticks remain provisional
balance choices. Existing actual-process and source-free recovery witnesses cover these mechanics.
They do not establish balance or qualify a later changed tree; follow the
[testing policy](../development/testing.md).
