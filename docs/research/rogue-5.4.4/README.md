# Rogue 5.4.4 reference specification

<!-- documentation-role: fixed-source -->

This is a pinned external-game reference researched on **2026-10-08**.
The accepted [TOR adaptation](../../plans/rogue.md) takes precedence wherever it
chooses different behavior. Its exploration foundation is implemented; this
reference is neither TOR's current mechanics nor a claim of executable parity.

Rogue's defining loop is a single adventurer exploring generated floors, managing
food and uncertain equipment, fighting progressively dangerous monsters,
recovering the Amulet of Yendor at depth 26 or below, and climbing back to escape.
Survival depends on limited information, consumables, positioning, and attrition.

Read [rules](rules.md) for behavior and [content](content.md) for the bounded
monster/item catalog. [Current TOR support](../../status.md) and the
[remaining adaptation plan](../../plans/rogue.md) own project capability and
implementation priorities; no separate historical gap table is maintained here.

## Edition and evidence

The requested [RLGallery Rogue V5 page](https://rlgallery.org/about/rogue5.html)
identifies its game as UNIX Rogue 5.4, restored as 5.4.4 by the Roguelike
Restoration Project and subsequently maintained by RLGallery. The page is a
history and edition reference, not a complete rules manual. DOS/Epyx Rogue,
Rogue 3.6/5.2, NetHack, and newer 5.4.5 forks are outside this baseline.

The primary rules evidence is the
[5.4.4 source mirror at commit c7c119f893bd2f8255f2ad78d58a32664ddb9a97](https://github.com/phs/rogue/tree/c7c119f893bd2f8255f2ad78d58a32664ddb9a97).
Its commit message describes an import from rogue.rogueforge.net; `vers.c`
declares release `5.4.4` and version string `rogue (rogueforge) 09/05/07`.
This is a pinned restoration baseline, **not a verified copy of today's Gallery
executable**. The bundled authors' guide supplies player-facing intent; source
branches take precedence for the baseline's actual behavior.

| Evidence | Used for |
| --- | --- |
| [Authors' guide, rogue.me.in](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/rogue.me.in) | Goal, controls, player model, equipment, information |
| [rogue.h](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/rogue.h), [extern.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/extern.c), [init.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/init.c) | Constants, complete content tables, generation weights, starting state and appearances |
| [command.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/command.c), [move.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/move.c), [main.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/main.c) | Actions, turn boundaries, search, stairs, traps, update order |
| [rooms.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/rooms.c), [passages.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/passages.c), [new_level.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/new_level.c) | Floor lifecycle, connected generation, lighting, treasure rooms, placement |
| [fight.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/fight.c), [monsters.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/monsters.c), [chase.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/chase.c) | Combat, saves, spawning, wake/pursuit, disguise, ranged breath |
| [misc.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/misc.c), [daemons.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/daemons.c), [daemon.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/daemon.c) | Healing, hunger, XP, timed effects and repeated updates |
| [things.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/things.c), [pack.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/pack.c), [armor.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/armor.c), [weapons.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/weapons.c) | Capacity, stacks, curses, equipment, thrown objects |
| [potions.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/potions.c), [scrolls.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/scrolls.c), [rings.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/rings.c), [sticks.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/sticks.c) | Effects and conditional identification |
| [save.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/save.c), [rip.c](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/rip.c) | Suspend/resume, terminal outcomes, scoring |
| [RLGallery flytrap fix, 2016-05-19](https://rlgallery.org/notes/flytraps.html) | Confirmed later behavioral difference: escalating flytrap damage on hits as well as misses |

Source file names in the companion documents refer to this fixed revision unless
explicitly labeled Gallery. The pinned source links above are the durable evidence references. Research used source inspection, not a compiled or played reference
binary. No claim of seed-for-seed or executable parity is made.

The research compared the 26 monster identities and ordinary combat/XP/loot
entries, plus the six equipment/magic subtype tables, against the pinned source.
That evidence concerns the reference tables, not a playable TOR clone. Source
inspection did not establish executable parity or resolve every conditional edge
case; retain the open research below when implementing a corresponding effect.

## Fidelity levels

- **Recognizable clone:** preserve the descent/retrieval/ascent loop, generated
  connected floors, food pressure, unidentified consumables, equipment risk,
  turn-based combat, and the distinctive monster/effect families. Different
  numbers, controls, or persistent floors must be labeled adaptations.
- **Behavioral reference profile:** match the documented baseline tables,
  probabilities, combat math, action/update order, knowledge, and regenerated
  floors. Apply Gallery differences only as explicitly selected overrides.
- **Exact executable parity:** additionally pin the actual target distribution,
  all patches, platform RNG, random-call order, terminal behavior, and edge cases,
  then compare reference and clone traces. This remains unverified.

## Open research before a parity claim

1. Obtain and pin Gallery's current Rogue source/build and enumerate all changes
   from the restoration baseline. Only its linked flytrap change is confirmed
   here; there may be others.
2. Differentially exercise command cancellation/failure timing, haste/fuse
   expiry, armor changes, ray bounces, hidden-memory behavior, and floor changes.
   Function names and branches below provide starting points, not exhaustive
   traces for every edge case.
3. Resolve whether to preserve baseline bugs. In particular, flytrap damage data
   and the unsigned strength-update boundary deserve executable checks.
4. Decide whether floor regeneration, Rogue glyphs, exact scoring, and old
   command aliases are required or documented scenario adaptations.
5. Extract/check every source-dependent edge case when implementing the relevant
   effect. Catalog summaries identify required mechanics but do not replace the
   reference for all conditionals or presentation messages.

## Consumable adaptation findings

[Retained information, light, growth and device findings](consumable-adaptations.md)
preserve the pinned source comparison. They are not current TOR mechanics; those
belong to the linked current references.
