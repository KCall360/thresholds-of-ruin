# Rogue 5.4.4 content catalog

This catalog covers all baseline species and ordinary item subtypes, not merely
examples. Numerical tables come from `extern.c`, weapon dice from `weapons.c`,
and behavior from the corresponding effect files in the
[pinned source](README.md). See [rules](rules.md) for interaction, timing,
generation, knowledge, and [implementation](implementation.md) for priorities.

Weights are percentages **conditional on selecting that category**, before
food-drought overrides. Base worth is a scoring input, not a shop price; there
are no shops. Ring stone values additionally modify base worth. Slash-separated
damage entries mean separately rolled attacks. A zero-damage hit can trigger a
special effect. Content names identify the reference; adapting names/art assets
does not remove the need for the corresponding mechanics.

## Monsters (26)

Level and AC are base values before depth scaling. XP is base, before depth and
rolled-HP adjustments. Carry is the percent chance of receiving an item when
allowed. Flags: mean = wake/aggression, flying = extra pursuit opportunity,
regen = source regeneration flag (runtime behavior remains to be verified),
greedy = gold targeting, invisible = ordinary sight concealment.

| Glyph | Species | Level | AC | Base XP | Carry % | Damage | Flags / distinguishing behavior |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| A | aquator | 5 | 2 | 20 | 0 | 0d0 / 0d0 | Mean; rusts armor on hit |
| B | bat | 1 | 3 | 1 | 0 | 1d2 | Flying; erratic movement |
| C | centaur | 4 | 4 | 17 | 15 | 1d2 / 1d5 / 1d5 | Three attack components |
| D | dragon | 10 | −1 | 5000 | 100 | 1d8 / 1d8 / 3d10 | Mean; ranged flame |
| E | emu | 1 | 7 | 2 | 0 | 1d2 | Mean |
| F | venus flytrap | 8 | 3 | 80 | 0 | Special | Mean; stationary, holds, escalating damage; Gallery fix |
| G | griffin | 13 | 2 | 2000 | 20 | 4d3 / 3d5 | Mean, flying, regen flag |
| H | hobgoblin | 1 | 5 | 3 | 0 | 1d8 | Mean |
| I | ice monster | 1 | 9 | 5 | 0 | 0d0 | Freezes on hit for 2–3 additional iterations; prolonged freezing can kill |
| J | jabberwock | 15 | 6 | 3000 | 70 | 2d12 / 2d4 | Heavy damage |
| K | kestrel | 1 | 7 | 1 | 0 | 1d4 | Mean, flying |
| L | leprechaun | 3 | 8 | 10 | 0 | 1d1 | Steals gold, then disappears; kill can yield gold |
| M | medusa | 8 | 2 | 200 | 40 | 3d4 / 3d4 / 2d5 | Mean; first qualifying gaze can confuse after failed magic save |
| N | nymph | 3 | 9 | 37 | 100 | 0d0 | Steals eligible unequipped magic item, then disappears |
| O | orc | 1 | 6 | 5 | 15 | 1d8 | Greedy |
| P | phantom | 8 | 3 | 120 | 0 | 4d4 | Invisible; somewhat erratic |
| Q | quagga | 3 | 3 | 15 | 0 | 1d5 / 1d5 | Mean |
| R | rattlesnake | 2 | 3 | 9 | 0 | 1d6 | Mean; failed poison save reduces strength unless sustained |
| S | snake | 1 | 5 | 2 | 0 | 1d3 | Mean |
| T | troll | 6 | 4 | 120 | 50 | 1d8 / 1d8 / 2d6 | Mean, regen flag |
| U | black unicorn | 7 | −2 | 190 | 0 | 1d9 / 1d9 / 2d9 | Mean; strong AC |
| V | vampire | 8 | 1 | 350 | 20 | 1d10 | Mean, regen flag; 30% drain test after hit, loses 1d3 max/current HP |
| W | wraith | 5 | 4 | 55 | 0 | 1d6 | 15% drain test after hit; XP/level loss plus 1d10 max/current HP loss |
| X | xeroc | 7 | 7 | 100 | 30 | 4d4 | Appears as an object until revealed |
| Y | yeti | 4 | 6 | 50 | 30 | 1d6 / 1d6 | Two attack components |
| Z | zombie | 2 | 8 | 6 | 0 | 1d8 | Mean |

Population order: `K E B S H I R O Z L C Q A N Y F T W P X U M V G J D`.
Wandering selection excludes I, L, N, F, X, D by replacing their entries with
zero and rerolling. Do not select uniformly from A–Z for normal generation.
Polymorph uses a different, uniform A–Z selection. Sources: `monsters.c`, `sticks.c`.

## Item-category distribution

| Category | Weight % | Ordinary subtypes |
| --- | ---: | ---: |
| Potion | 26 | 14 |
| Scroll | 36 | 18 |
| Food | 16 | 2 |
| Weapon | 7 | 9 |
| Armor | 7 | 8 |
| Ring | 4 | 14 |
| Wand/staff effect | 4 | 14 |

Gold and Amulet use separate generation paths. Wands versus staffs are randomized
appearance/construction forms of the same 14 effect identities, not 28 effects.

## Weapons (9)

| Weapon | Weight % | Base worth | Melee | Thrown/launched | Notes |
| --- | ---: | ---: | --- | --- | --- |
| Mace | 11 | 8 | 2d4 | 1d3 | Starting +1 hit/+1 damage |
| Long sword | 11 | 15 | 3d4 | 1d2 | |
| Short bow | 12 | 15 | 1d1 | 1d1 | Launcher; starting +1 hit |
| Arrow | 12 | 1 | 1d1 | 2d3 with bow | Grouped ammunition; without bow uses melee dice |
| Dagger | 8 | 3 | 1d6 | 1d4 | Throwable |
| Two handed sword | 10 | 75 | 4d4 | 1d2 | No separate shield restriction needed: Rogue has no shields |
| Dart | 12 | 2 | 1d1 | 1d3 | Grouped missiles |
| Shuriken | 12 | 5 | 1d2 | 2d4 | Grouped missiles |
| Spear | 12 | 5 | 2d3 | 1d6 | Throwable |

Generated weapons: 10% are cursed with −1 to −3 hit; the next 5% have +1 to +3
hit; otherwise ordinary. Starting gear bypasses that roll. Missile groups receive
separate group IDs. Sources: `things.c:new_thing`, `weapons.c:init_weapon`, `fight.c`.

## Armor (8)

| Armor | Weight % | Base worth | Base AC |
| --- | ---: | ---: | ---: |
| Leather armor | 20 | 20 | 8 |
| Ring mail | 15 | 25 | 7 |
| Studded leather armor | 15 | 20 | 7 |
| Scale mail | 13 | 30 | 6 |
| Chain mail | 12 | 75 | 5 |
| Splint mail | 10 | 80 | 4 |
| Banded mail | 10 | 90 | 4 |
| Plate mail | 5 | 150 | 3 |

Generated armor: 20% cursed with AC worsened by 1–3; the next 8% improve AC by
1–3; otherwise base AC. Lower is better. Enchant armor improves AC by one and
uncurses it. Rust worsens eligible armor by one until AC 9. Leather, permanently
protected armor, and a maintain-armor ring prevent rust. Sources: `extern.c`,
`things.c`, `scrolls.c`, `move.c:rust_armor`.

## Potions (14)

All consume one unit. Identification is effect-specific; descriptions below are
behavior summaries, not a replacement for `potions.c`'s conditional branches.

| Effect | Weight % | Base worth | Behavior |
| --- | ---: | ---: | --- |
| Confusion | 7 | 5 | Randomizes movement, nominal duration 20; repeated doses extend; recognition affected by hallucination |
| Hallucination | 8 | 5 | Distorts perceived items/monsters and messages, nominal 850; extends |
| Poison | 8 | 5 | Lose 1–3 strength unless sustained; can end hallucination; identifies |
| Gain strength | 13 | 150 | +1 strength, updates remembered maximum; identifies |
| See invisible | 3 | 100 | Nominal 850; cures blindness; not automatically type-identified just by taste |
| Healing | 13 | 130 | Heal experience-level d4; exceeding max raises max HP by one; cures blindness; identifies |
| Monster detection | 6 | 130 | Reveals monsters on current floor for nominal 20; recognition depends on newly detected monsters |
| Magic detection | 6 | 105 | Displays magic-object locations, including carried monster loot under source conditions; target-dependent identification |
| Raise level | 2 | 250 | Advances XP past next threshold; normal level HP gain; identifies |
| Extra healing | 5 | 200 | Heal experience-level d8; overflow raises max HP by one or two; cures blindness/hallucination; identifies |
| Haste self | 5 | 190 | Extra player action per cycle, 4–7 cycles; another dose causes exhaustion; special use timing; identifies |
| Restore strength | 13 | 130 | Restore remembered base maximum, account for strength rings; not universally auto-identified |
| Blindness | 5 | 5 | Suppresses sight, nominal 850; extends; identifies |
| Levitation | 6 | 75 | Avoid ground traps; blocks pickup and stair use, nominal 30; extends; identifies |

## Scrolls (18)

Read consumes one unit. The five identify subtypes are separate effects and
generation entries, not one unrestricted identify scroll.

| Effect | Weight % | Base worth | Behavior |
| --- | ---: | ---: | --- |
| Monster confusion | 7 | 140 | Arms the next successful player hit to confuse its target |
| Magic mapping | 4 | 150 | Reveals layout, stairs, secret doors/passages and traps |
| Hold monster | 2 | 180 | Holds running monsters within the centered 5×5 neighborhood |
| Sleep | 3 | 5 | Player forced inaction; duration uses `rnd(SLEEPTIME)+4` |
| Enchant armor | 7 | 160 | Improve worn AC by one, remove its curse |
| Identify potion | 10 | 80 | Identify selected potion |
| Identify scroll | 10 | 80 | Identify selected scroll |
| Identify weapon | 6 | 80 | Reveal weapon instance details |
| Identify armor | 7 | 100 | Reveal armor instance details |
| Identify ring, wand or staff | 10 | 115 | Reveal chosen ring/device identity and instance details |
| Scare monster | 3 | 200 | Ground movement barrier; reading wastes it; repickup can destroy it |
| Food detection | 2 | 60 | Display floor-food locations; target-dependent identification |
| Teleportation | 5 | 165 | Relocate player on current floor; recognition depends on room change |
| Enchant weapon | 8 | 150 | Randomly +1 hit or +1 damage on wielded weapon, remove its curse |
| Create monster | 4 | 75 | Spawn on eligible adjacent cell if available |
| Remove curse | 7 | 105 | Uncurse currently equipped items |
| Aggravate monsters | 3 | 20 | Activate pursuit on current floor |
| Protect armor | 2 | 250 | Permanent rust protection on worn armor |

Sources: `scrolls.c`, `command.c:whatis`, `pack.c`, `chase.c`.

## Rings (14)

Wear at most two. Food cost is per stomach update per worn ring; fractions mean
an independent chance to charge one unit, not accumulated fractional debt.
Slow digestion has a chance to subtract one from digestion.

| Effect | Weight % | Base worth before stone | Food cost | Behavior |
| --- | ---: | ---: | --- | --- |
| Protection | 9 | 400 | +1 | Bonus improves AC and player's magic saves |
| Add strength | 9 | 400 | +1 | Modify current strength while worn |
| Sustain strength | 5 | 280 | +1 | Prevent covered poison strength losses |
| Searching | 10 | 420 | +1 with probability 1/3 | Automatic search after command cycle |
| See invisible | 10 | 310 | +1 with probability 1/5 | Disclose invisible monsters while active |
| Adornment | 1 | 10 | 0 | No mechanical benefit |
| Aggravate monster | 10 | 10 | 0 | Cursed; aggravates when put on |
| Dexterity | 8 | 440 | +1 with probability 1/3 | Modify eligible wielded-weapon accuracy |
| Increase damage | 8 | 400 | +1 with probability 1/3 | Modify eligible wielded-weapon damage |
| Regeneration | 4 | 460 | +2 | +1 HP per doctor update per ring |
| Slow digestion | 9 | 240 | −1 with probability 1/2 | Reduce digestion |
| Teleportation | 5 | 30 | 0 | Cursed; 1/50 teleport check after command cycle per ring |
| Stealth | 7 | 470 | +1 | Suppress ordinary mean-monster wake check |
| Maintain armor | 5 | 380 | +1 | Prevent rust |

Protection/add-strength/dexterity/increase-damage bonuses are −1, +1, or +2
(equiprobable); −1 is cursed. Aggravation and teleportation rings are always
cursed on normal generation. Sources: `things.c`, `rings.c`, `fight.c`, `command.c`.
Multiple-source see-invisible removal is an edge case to check before parity.

## Wands/staffs (14 effects)

Both forms share identities; a staff's melee damage is 2d3, a wand's 1d1, and
both have 1d1 thrown damage. Light starts with 10–19 charges; all other effects
3–7. Effect use, recognition, obstruction, and saving throws differ by subtype.

| Effect | Weight % | Base worth | Behavior |
| --- | ---: | ---: | --- |
| Light | 12 | 250 | Permanently light current room; corridor use only gives feedback |
| Invisibility | 6 | 5 | Make targeted monster invisible |
| Lightning | 3 | 330 | Bouncing elemental bolt |
| Fire | 3 | 330 | Bouncing flame bolt; dragon exception |
| Cold | 3 | 330 | Bouncing ice bolt |
| Polymorph | 15 | 310 | Replace target species and rolled stats; retain carried pack |
| Magic missile | 10 | 170 | Directional projectile, magic save, base 1d4 with +1 device damage bonus plus shared combat adjustments |
| Haste monster | 10 | 5 | Remove slow or grant haste; provoke pursuit |
| Slow monster | 11 | 350 | Remove haste or grant slow; provoke pursuit |
| Drain life | 9 | 300 | If eligible targets exist, halve player's HP and distribute resulting HP amount as damage across room targets |
| Nothing | 1 | 5 | Consume charge without effect |
| Teleport away | 6 | 340 | Relocate targeted monster to another eligible floor position |
| Teleport to | 6 | 50 | Bring targeted monster next to player in chosen direction |
| Cancellation | 5 | 280 | Suppress monster special abilities, remove invisibility/disguise; release relevant hold |

Source: `sticks.c`. Catalog descriptions do not imply that every effect identifies
on use or that every ray passes through ordinary doors.

## Food, gold, Amulet, and traps

Food generated within its category is a ration 90% of the time and configurable
fruit 10%; both supply 1,100–1,499 food units up to a 2,000 stomach cap. Food sale
value is two gold per unit. Gold is purse currency, not ordinary pack inventory.
The Amulet is the ascent gate and victory item, worth 1,000 at victory; ordinary
item generation never rolls it. No spellbooks, shields, wearable boots/helmets,
shops, crafting, corpse nutrition, or intrinsic spell system belong to this
edition. Sources: `things.c`, `misc.c`, `new_level.c`, `rip.c`.

| Trap | Core effect |
| --- | --- |
| Trapdoor | Advance depth and generate new floor |
| Arrow | Hit test, 1d6 damage; misses can leave an arrow |
| Sleeping gas | Forced sleep using nominal duration 5 |
| Bear trap | Movement lock using nominal duration 3 |
| Teleport | Relocate within floor |
| Poison dart | Hit test, 1d4 damage, possible strength −1 |
| Rust | Degrade eligible worn armor |
| Mysterious | One of eleven flavor messages |

## Victory valuation

Exact scoring is optional for a recognizable adaptation. For a reference profile,
use these formulas from `rip.c:total_winner`, with integer truncation and a
minimum of zero per item. Knowledge is evaluated before the victory reveal.

| Category | Added gold |
| --- | --- |
| Food | `2 * quantity` |
| Weapon | `base_worth * (3*(hit_bonus + damage_bonus) + quantity)` |
| Armor | `base_worth + 100*(9-current_AC) + 10*(base_AC-current_AC)` |
| Scroll/potion | `base_worth * quantity`, halved if type unknown |
| Ring | Base worth including stone; numerical positive bonus adds `100*bonus`, nonpositive numerical bonus replaces worth with 10; halve if instance details unknown |
| Wand/staff | `base_worth + 20*charges`, halved if instance details unknown |
| Amulet | 1,000 |

Stone names and their 26 values are in `init.c:stones` (5–350 gold). For exact
valuation, extract that appearance table alongside the shuffled stone assignment;
the base ring worth above alone is insufficient.
