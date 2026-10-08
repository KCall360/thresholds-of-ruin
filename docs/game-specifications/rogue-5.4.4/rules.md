# Rogue 5.4.4 rules

These describe the pinned baseline in [sources and scope](README.md). Requirement
IDs connect rules to the [implementation analysis](implementation.md). Complete
bounded content tables appear in [the catalog](content.md). A Gallery-specific
difference is explicitly marked; proposed adaptations are not reference facts.

## R01 — Run, goal, and starting state

One player, one adventurer, no class/race selection, companions, towns, shops,
quests, or spell-learning system. Start at dungeon depth 1, experience level 1,
0 XP, strength 16, 12/12 HP, unarmored AC 10, and 1,300 food units. Unarmed damage
is 1d4. Strength has separate current and remembered maximum values.

Starting inventory is one food ration, worn +1 ring mail (effective AC 6), wielded
mace with +1 hit/+1 damage, short bow with +1 hit, and 25–39 arrows. Starting gear
is known. Ring mail is armor, not an equipped ring. Sources: `extern.c`
`INIT_STATS`, `init.c:init_player`.

The Amulet is placed on newly generated floors at depth >=26 while it is not
carried. A successful upward command requires standing on the same stair cell
used for descent and carrying the Amulet; without it ascent is blocked. Climbing
from depth 1 to depth 0 wins. Levitation blocks both stair commands. Dropping the
Amulet clears possession and its associated ascent/hunger benefits. Death ends
the run; quitting is a separate scored outcome. Sources: `new_level.c:put_things`,
`command.c:d_level/u_level`, `things.c:drop`, `rip.c`.

## R02 — Floor generation and lifecycle

The logical display is 80 columns by 24 lines, with rows 1–22 for play. Nine room
slots partition the floor into a 3×3 arrangement. Generation omits 0–3 rooms,
connects the slots by passages and may add extra connections. Actual rooms have
random position/size within their slots. Deeper floors increasingly have dark
rooms, secret doors/passages, and occasional maze rooms. Ordinary doors are
walk-through boundaries; there is no player open/close/lockpick command.

Generation details that affect balance:

- Room darkness test is `rnd(10) < depth - 1`; selected dark rooms become mazes
  with probability 1/15. (`rnd(n)` is zero-based; see R05.)
- Each present room has a 1/2 gold chance, subject to ascent restrictions below.
  Gold amount is `rnd(50 + 10*depth) + 2`.
- Each room has an 80% initial-monster chance if it contains gold, otherwise 25%.
- Ordinary item placement makes nine independent attempts, each with 36%
  success. Item-category weights are in the catalog. A food drought forces food
  when the `no_food` counter exceeds 3; creating food resets that counter.
- A treasure room has a 1/20 chance when item placement is allowed. It adds
  several items and a dense group of monsters selected as if one depth deeper;
  counts are bounded by room area and the source's treasure limits.
- Traps are considered when `rnd(10) < depth`. Their count is
  `min(10, rnd(depth/4) + 1)`; types are uniformly selected among eight. Traps use
  room-floor placement, excluding maze passages. At depth <4 the baseline's
  `rnd(0)` behavior yields zero.
- Stairs and the arrival cell are separately selected free floor positions.
  Arrival is not necessarily on the stair cell.

**Every floor transition discards the previous floor's terrain, monsters, and
uncollected objects and generates a new floor.** Revisiting depth 25 after depth
26 does not restore an old map. The deepest reached depth is tracked separately.
While carrying the Amulet on a floor shallower than that maximum, ordinary item
and treasure-room placement is skipped, room gold is suppressed, and monsters
are not assigned normal carried loot. Monsters and traps still exist. Descending
to the deepest reached depth or deeper permits loot again. Sources: `rooms.c`,
`passages.c`, `new_level.c`, `monsters.c:give_pack`.

This differs from this project's intended persistent activated-region model;
choosing persistent floors makes a clone an adaptation.

## R03 — Actions, time, movement, and automation

Play advances on commands, not wall-clock time. `command()` runs BEFORE daemons
and fuses, accepts one time-consuming action (two while hasted), then runs AFTER
daemons and fuses. Initial AFTER daemon registration order is monster runners,
healing, then hunger; wandering startup is a fuse. Free commands remain inside
the input loop instead of granting another full world update. Armor changes call
`waste_time()` for an extra daemon/fuse update before the normal command ends.
Stair commands explicitly set `after = FALSE` and replace the floor without an
ordinary action's AFTER update. These are observable timing differences, not
equivalent to this engine's prepared-work/recovery schedule.

| Action family | Baseline behavior |
| --- | --- |
| Eight-direction movement | One step; no corner cutting: both side cells of a diagonal must be passable |
| Move into a monster | Attack from the existing cell, including an invisible monster; do not move into its occupied cell |
| Move onto an item | Automatic pickup after movement, unless the move-without-pickup prefix was used |
| Wait/rest, search, explicit pickup, drop | Normally consume an action; waiting permits healing, hunger, effects, and monsters to advance |
| Quaff, read, eat, wield, ring on/off, throw, zap | Normally consume an action; effects resolve within the command |
| Armor on/off | Extra update cycle; old equipment timing must be traced, not assumed to equal the current three-turn armor preparation |
| Inventory, help, call/nickname, discoveries, options, symbol/trap explanation, redraw, statistics | Free information commands |
| Invalid step into wall/out of bounds/illegal diagonal | Stops running; no ordinary AFTER advancement |
| Cancel item/direction prompt | Explicit cancellation paths are free; not every failed action is free |
| Immobilized movement or an empty wand zap | Can consume time even without the intended result |

Sleep/paralysis uses forced rest iterations; bear traps block movement attempts
while other actions remain possible. Held-by-flytrap behavior differs from a
bear trap. Combat/attacks reset the quiet-healing counter and stop repeated
actions. Numeric repeats are capped at 255. Running, corridor following, and
auto-fighting must submit/check one action at a time. `f` stops near danger using
observed damage; `F` continues until one combatant dies. See `command.c`, `move.c`,
`armor.c`, `main.c`; edge-case parity remains subject to reference traces.

## R04 — Perception, hidden terrain, and knowledge

Lit rooms expose their contents while the player is in the room. Darkness and
corridors restrict perception to the local lamp neighborhood. The `cansee` test
uses squared distance `<3` (the surrounding eight cells), room membership,
darkness, and corridor corner rules. This is room/lamp logic, not generic
radius-3 shadowcasting. Explored room outlines and corridors provide map memory;
dark-room lamp contents are erased as the player leaves their neighborhood.
Blindness suppresses visual perception. Invisibility hides monsters unless an
appropriate potion/ring or monster-detection effect discloses them. Detection
can reveal entities beyond ordinary sight without revealing all terrain.

Secret doors look like walls, hidden passages like blank space, and undiscovered
traps like floor. Search checks adjacent cells, excluding the player's cell.
Discovery chances per eligible cell are 1/5 for secret doors, 1/3 for hidden
passages, and 1/2 for traps. Add 3 to these denominators while hallucinating and
2 while blind. Discovery stops repetition. Mapping also reveals hidden terrain
and traps. Walking onto an unrevealed trap discovers and activates it, except
while levitating. Seeing a trap is separate from learning its type; `^` explains
a nearby revealed trap. Sources: `misc.c:look/erase_lamp`, `chase.c:cansee/see_monst`,
`command.c:search`, `move.c`, `scrolls.c`.

Potion colors, ring stones, and wand/staff materials are shuffled per run, with
distinct assignments within their pools. Scroll titles are generated from
syllables; uniqueness is not explicitly enforced. Knowledge has two layers:
type identity (what this appearance means) and instance details (bonuses/charges).
Calling an unknown type assigns a guess/name without proving its identity.
Discoveries include learned and called types. Identification scrolls restrict
their selection to a particular item category. Wearing armor reveals its
instance details; wielding a weapon does not universally identify its bonuses.

Use identification is conditional and effect-specific. Healing/poison/strength
potions explicitly identify themselves, even where the current engine's
HP-change-only test would not. Some no-target or unobservable effects leave
identity unknown. Never substitute a generic “any use identifies” rule.
Hallucination changes perceived symbols/names and some identification feedback;
it does not replace authoritative entities. Xerocs masquerade as objects until
revealed. Sources: `init.c`, `things.c`, `potions.c`, `scrolls.c`, `fight.c`.

## R05 — Combat, statistics, and advancement

Notation: `rnd(n)` returns 0 through n−1 for positive n; the baseline explicitly
returns 0 for n=0. Negative ranges are not a supported dice contract. `NdS` is N independent dice with faces 1 through S. Integer
division truncates. In `swing`, a hit occurs when:

`rnd(20) + hit_bonus >= 20 - attacker_level - defender_AC`

The roll is **0–19**, not 1–20. Lower AC is better; there is no special natural
1/20 rule. Weapon hit bonus and a strength lookup modify accuracy. The +4 bonus
against a defender without `ISRUN` is applied in `roll_em`; player attacks also
call `runto` before rolling, so do not infer that every initially sleeping
monster receives that bonus. Each slash-separated monster attack component gets
its own hit and damage roll. Successful component damage is
`max(0, dice_damage + weapon_damage_bonus + strength_damage_bonus)`.

Body armor replaces unarmored AC; protection rings subtract their bonuses from
that AC. There is no shield slot or independent armor damage-reduction system.
Launcher bonuses add to compatible thrown missile attacks. Monster attacks can
land with zero HP damage and still apply special effects (aquators, ice monsters,
nymphs). Sources: `fight.c:swing/roll_em/fight/attack`.

| Strength index | Hit adjustment | Damage adjustment |
| --- | --- | --- |
| 0–6 | −7 through −1 | −7 through −1 |
| 7–15 | 0 | 0 |
| 16 | 0 | +1 |
| 17 | +1 | +1 |
| 18 | +1 | +2 |
| 19–20 | +1 | +3 |
| 21–22 | +2 | +4 |
| 23–30 | +2 | +5 |
| 31 | +3 | +6 |

Normal strength changes intend bounds 3–31, with restoration remembering the
best underlying strength and excluding ring bonuses. The source uses unsigned
`str_t`; extreme subtraction behavior needs a parity check rather than silently
reproducing or correcting a suspected underflow. Sources: `misc.c:chg_str/add_str`.

Saving throws succeed when `1d20 >= 14 + category - floor(level/2)`.
Category is 0 for poison/paralysis/death, 2 for breath, 3 for magic. Protection
ring bonuses reduce the player's magic-save target. Individual effects decide
whether and which save occurs; there is no universal status resistance.

XP thresholds for successive levels starting at level 2 are:
`10, 20, 40, 80, 160, 320, 640, 1300, 2600, 5200, 13000, 26000, 50000,
100000, 200000, 400000, 800000, 2000000, 4000000, 8000000`.
Each gained level adds 1d10 to both current and maximum HP. Kills award base
monster XP plus depth and rolled-HP adjustments; raise-level potions advance XP.
Wraith drain changes level/XP and HP; vampire drain reduces maximum HP. Dungeon
depth and experience level are distinct. Sources: `extern.c:e_levels`,
`misc.c:check_level`, `monsters.c:new_monster/exp_add`, `fight.c`.

## R06 — Hunger, recovery, and timed effects

Food starts at 1,300; ordinary digestion subtracts one per stomach update plus
both rings' costs, minus one while carrying the Amulet. Ring costs may be
probabilistic or negative. Below 300 units the player becomes hungry, below 150
weak. At <=0 food continues falling by one per update; with no existing forced
inaction there is a 1/5 chance of fainting for 4–11 iterations. Starvation death
uses the exact `food_left-- < -850` test. Hunger-state changes stop automation.

Eating first clamps negative food to zero, then adds 1,100–1,499 units, capped at
2,000, and resets hunger state. Rations and the configurable fruit have the same
nutrition; a ration sometimes grants one XP. There is no corpse-eating mechanic.

Natural healing increments `quiet` each doctor update. Below experience level 8,
heal one HP when `quiet + 2*level > 20`; at level >=8 and quiet >=3, heal
`rnd(level-7)+1`. Each regeneration ring adds one HP per doctor update. Healing
caps at maximum HP and resets quiet whenever HP changed. Fighting interrupts
quiet; walking does not inherently do so. There is no requirement to stand still
for all natural healing. Sources: `daemons.c:stomach/doctor`, `misc.c:eat`.

Timed statuses include confusion, hallucination, blindness, see-invisible,
levitation, haste, detection, sleep, and movement holds. Their expiry/extension
rules are effect-specific. The baseline's `spread(n)` expression is
`n - floor(n/20) + rnd(floor(n/10))`; its nearby comment is inaccurate. Confusion
uses nominal 20, hallucination/blindness/see-invisible 850, and levitation 30.
Repeated haste instead causes exhaustion, clears haste, and adds 0–7 forced-rest
iterations. Initial haste duration is 4–7 AFTER cycles. Sources: `rogue.h`,
`misc.c:add_haste/spread`, `potions.c:do_pot`, `daemon.c`, `command.c`.

## R07 — Inventory, equipment, and object interactions

Inventory letters select items; capacity is `MAXPACK=23` accounting units,
**not simply 23 visible inventory rows**. Potions, scrolls, and food can appear
as stacks while consuming capacity per unit. Grouped missiles share capacity by
group; distinct groups of identical missiles can remain separate. Gold enters
the purse and does not consume pack capacity. Ground cells do not support
arbitrary object piles; dropping requires a bare floor/passage, and falling
missiles/loot seek nearby space or disappear. Sources: `pack.c`, `things.c`,
`weapons.c:fall/fallpos`.

Equipment consists of one wielded object, one suit of armor, and two rings (left
and right). Cursed equipped objects cannot be removed/replaced/dropped normally;
carried unequipped cursed objects may be dropped. Curse removal affects currently
equipped armor, weapon, and rings. Enchantment can improve gear and remove its
curse. Protected armor and maintain-armor rings prevent rust; leather and already
sufficiently degraded armor have additional rust exceptions in `rust_armor`.

Arrows require a wielded bow for their proper missile damage and launcher
bonuses. Throwing separates one missile/unit; hits can consume the object and
misses may leave recoverable objects nearby. A wand/staff is a charged item;
zapping consumes a charge even on many ineffective uses. Ordinary initial charge
range is 3–7; light uses 10–19. Empty devices do nothing and still cost an action.
Sources: `weapons.c:missile/do_motion`, `fight.c:roll_em`, `sticks.c:fix_stick/do_zap`.

A scare-monster scroll works **on the ground** as a monster movement barrier.
Reading it wastes it. After it has been picked up, placed down, and picked up
again it turns to dust. This lifecycle requires instance memory, not just an
on-read effect. Sources: `pack.c:add_pack`, `chase.c:chase/find_dest`, `scrolls.c`.

## R08 — Monsters, AI, spawning, and special effects

All 26 A–Z species and their stats are in the catalog. Initial HP is
`monster_level d8`. At depths >26 add `depth-26` to monster level, subtract it
from AC, and add ten times it to base XP. HP-based XP adjustment is
`floor(maxHP/8)` at level 1, otherwise `floor(maxHP/6)`, multiplied by 4 at levels
7–9 or by 20 above 9. At depth >29 all new monsters are hasted.

Species selection uses the ordered population table, index
`depth + rnd(10) - 6`; negative indices become `rnd(5)`, indices >25 become
`21+rnd(5)`. Wandering monsters use a different table that excludes selected
species and rerolls zero entries. Carry chance is species-specific and assigned
only where ascent loot policy permits.

Mean monsters can wake when noticed (2/3 test), unless held, suppressed by
stealth, or the player is levitating. Attacking provokes pursuit. Orcs can target
room gold. Pursuers route toward room exits, then choose neighboring steps toward
their destination with randomized ties. Item pursuit/collection exists, but the
baseline does not use this project's healing/equipment-upgrade/flee AI. Bats
randomize movement with 1/2 probability, phantoms with 1/5, and confused monsters
with 4/5. Confused random movement can clear confusion with 1/20 probability.
Flytraps do not travel toward distant targets. Flying species can gain another
movement attempt while squared distance from the player is >=3. Haste/slow affect
chase attempts separately. Regeneration flags appear in the species table;
their intended behavior should be checked against actual update code before
claiming an implemented monster-regeneration rule.

Wandering startup uses nominal 70 with `spread`; after startup it rolls every
four BEFORE updates, spawning on a 1/6 result. A spawned monster is placed
outside the player's room and starts pursuing; the spawning daemon is removed
and the startup delay rearmed. Long rest/search is therefore not safe. Sources:
`monsters.c`, `chase.c`, `daemons.c:swander/rollwand`.

Special attacks include armor rust, freezing, gold or magic-item theft,
strength poison, experience drain, maximum-HP drain, gaze confusion, invisible
pursuit, object disguise, stationary holding/escalating damage, and dragon breath.
Killing monsters drops eligible carried objects, grants XP, and clears a
flytrap's hold. Ordinary monsters do not create food corpses. Sources: `fight.c`,
`monsters.c:wake_monster`, `chase.c:do_chase`.

**Gallery override:** its 2016 flytrap fix restores increasing damage on hits
as well as misses. The pinned restoration source mutates shared flytrap damage
data while individual attacks use copied data. Treat that baseline defect and
Gallery's correction as separate behaviors. See the [Gallery note](https://rlgallery.org/notes/flytraps.html).

## R09 — Trap, magic, ray, and terrain effects

The eight traps are trapdoor, arrow, sleeping gas, bear trap, teleport, poison
dart, rust, and mysterious. Trapdoor generates the next floor. Teleport relocates
within the current floor. Arrow/dart traps have hit rolls and damage; poison dart
can additionally reduce strength. Rust attacks armor, sleep suppresses commands,
and bear traps suppress movement. Mysterious traps give randomized flavor
messages rather than a new lasting mechanic. Levitation avoids ground traps
and blocks ground pickup/stairs. Sources: `move.c:be_trapped`, `command.c`.

The item catalog enumerates all magic effects. Required effect families are
healing/max-HP change, stat change/restoration/drain, conditions and timed expiry,
knowledge/mapping/detection, gear enchantment/curse/rust protection, entity
creation/transformation/cancellation, displacement, and directional attacks.

Elemental bolts can bounce from walls/doors and hit their originator; their
traversal budget is six steps. They use saves and nominal 6d6 damage. Dragons
have a 1/5 breath decision on an aligned same-room target within squared distance
36, subject to cancellation. Fire has a dragon-specific exception. Magic missile
is a different projectile with base 1d4 and device damage bonus +1, a magic save,
and additional shared combat adjustments (including strength). Targeted monster
wands, thrown objects, and bouncing bolts use different traversal semantics;
implementing one generic LOS hit does not reproduce them. Sources: `sticks.c`,
`chase.c:do_chase`, `weapons.c`.

## R10 — Terminal state, scoring, save/resume, and presentation

Death reduces purse by `floor(purse/10)` and records cause, depth, and whether the
Amulet was carried. Winning adds the sale value of carried possessions to gold;
the Amulet itself is worth 1,000. Values depend on subtype, quantities, bonuses,
charges, appearance stones, and pre-sale knowledge. Negative calculated values
are clamped to zero. Scoring is not XP. High-score storage and OS-user ranking
are presentation/service details rather than prerequisites for the core loop.
Sources: `rip.c:death/total_winner/score`, `extern.c`.

Save is suspension, not a reusable rollback slot. Restore checks version and
screen constraints, consumes the ordinary save file, rejects dead characters,
and has anti-multiple-restore checks. The restoration calls `srand` again on
restore; do not assume its sequence-preservation contract equals this project's
deterministic replay. A clone should specify its own suspend/resume and RNG
contract explicitly. Sources: `save.c`, `state.c`.

The status display includes depth, purse, current/max HP, current/max strength,
AC, experience level/XP, and hunger. Rogue's historical glyphs differ from the
current clients' NetHack conventions: stairs `%`, gold `*`, food `:`, armor `]`,
and Amulet `,`; player `@`, passages `#`, doors `+`, traps `^`, potions `!`,
scrolls `?`, weapons `)`, rings `=`, devices `/`, monsters A–Z. Symbol explanation
does not identify a magic item's hidden effect. Inventory, messages, discoveries,
safe run interruption, and direction/item prompts must work in every supported
client; literal historical key bindings are an optional presentation profile.
Source: `rogue.h`, `extern.c:helpstr`, authors' guide.
