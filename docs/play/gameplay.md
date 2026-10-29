# The first dungeon

The ordinary launch default is the authored
[`scenarios/first-dungeon`](../../scenarios/first-dungeon/scenario.toml) adventure:
explore five chambers, survive their inhabitants, recover the **dawn seal**, and
return to the entrance. The main route has a shortcut between the Broken gallery
and Ember hall. The objective is disclosed, and this scenario ends on victory.

Choose [text](text.md) or [ASCII](ascii.md); both control the same backend rules
and can hand control to one another. No client controls simulation time by its
redraw rate or presentation pacing.

For the separate generated-floor Warrior/Mage scenario, see the
[Rogue launch and play guide](rogue.md).

## Your starting character

You start as a two-cell-tall human delver, seeing from the upper body cell, with
an equipped iron greatsword. The build starts with Heavy Weaponry and Discipline. Its four paid
advancement choices buy Rattling Blow, Brace for Impact, another Heavy Weaponry
rank and Measured Cut. `stats` in text or
Ctrl-X in ASCII shows the derived character and available abilities; inventory
shows the carried weapon. These are current scenario choices, not hard-coded
requirements for every character.

The greatsword's authored base damage is rolled keen damage, modified by the
character's current rules and build. The ruin scout, stone guardian and ember
wisp likewise use species/build-defined attacks and defenses. The guardian has
impact reduction; the wisp's attack has fire energy damage. Timing, defense and
effect details belong to the [creature-rules reference](../reference/creature-rules.md).
The game no longer uses the early fixed-damage, single-authored-attack model.

There are no healing potions placed in this default dungeon. That is a content
choice: the engine supports equipment, potion use, typed effects and granted
abilities. Mage roots can grant one explicitly selected Spark Lance or Shell
form, or the fixed Falter form, but this starting build does not grant them. A capability available in another scenario or arena is not
a promise that this adventure contains it.

## Explore and fight

- Inspect before acting: text `look`, `examine <creature>` and `stats`, or ASCII
  `;` and Ctrl-X, do not advance time.
- Text `east` chooses a way onward and may walk several steps; `step east` is
  one move. ASCII movement keys step once; shifted directions run.
- Text `attack <creature>` approaches when needed. ASCII `#attack` selects a visible
  target, and moving into a hostile creature attacks by default. This is a client
  interpretation; backend movement itself never becomes an attack.
- Granted techniques are selected separately: text `rattling blow <creature>`,
  `measured cut <creature>` or `brace for impact`, or
  ASCII `Z`. They validate reach and funding at execution; text techniques do not
  approach for you.
- Travel uses only known routes. Newly seen actors, positive HP loss and stale
  obstacles can stop it; travel never attacks or opens a door automatically.
- Pick up the seal and return to the starting entrance. Text `go to exit` uses
  the disclosed objective exit. Completing the objective is checked at committed
  action/physics boundaries and need not advance the clock.

Preparation and recovery matter. Attacks prepare, recheck their target at
resolution, then recover. Damage that actually reduces HP can interrupt
preparation; fully resisted damage does not. Repeat a still-valid interrupted
attack to resume its progress. Waiting preserves preparation, while changing
action can discard it. After a saved recovery is restored, text `wait` or ASCII
Space continues the scheduler instead of inventing a second action.

The backend schedules by readiness and stable identity, with an explicit human
input boundary taking priority over autonomous actors at the same tick. AI uses
its own perception and actor-owned remembered targets. Species defaults choose
passive or mean acquisition, waiting or exploring while idle, useful tool use,
and an actor-active memory interval. Named profiles override only their supplied
settings. Mean acquisition uses committed sensory events: ordinary visible
approaches alert directly, while continual Stealth opposes Perception once per
new encounter or movement cue. Observation and planning never reroll a cue. Passive creatures react to an attributed committed hostile
strike, including a miss or harmless contact. A visible body and a disclosed event
caster are both required; unseen preparations reveal nothing. Faction hostility
remains directed scenario data; one actor's private grudge cannot change another's
menus.

The dungeon's humans and ruin scavengers can explore, open reachable doors, care
for themselves, improve known equipment and collect useful supplies. The stone
construct waits without those tools; the fire wisp explores without them. The
shared tactical profile sets only the 25% Health flee threshold, preserving those
species distinctions. Waiting forbids idle movement, including loot detours, but
can still allow useful actions underfoot. It does not stop ordinary acquisition,
pursuit or combat.

Loot decisions use the creature's own knowledge and bounded needs: useful gear,
one healing reserve and enough supplies for one use of a safe owned healing
ability, plus one food reserve only for a creature that requires nutrition.
Excess stock, unknown effects and unusable items are not collection goals. These
are provisional policies using existing items and actions, not an added economy.
See [behavior defaults and overrides](../reference/scenario-packages.md#behavior-defaults-and-ai-overrides).

## What you can know

You see your own exact health and action progress; enemies have qualitative
injury descriptions. Hidden enemy statistics, AI memory, RNG state, identities
and locations are not disclosed. Inspecting an object uses its current known
appearance, not all of its hidden properties. See [items and knowledge](../reference/items.md).

Sight is three-dimensional, with ambient illumination and immediate awareness
around the body. A low obstacle or short door can hide one creature while a taller
one sees over it. Remembered map contents can be stale. Ordinary upward walking
requires an explicit stair or link; visible headroom alone is not an ascent.
See [movement and doors](../reference/movement.md).

## Death, victory and saves

Death drops inventory and a non-stackable corpse at the actor's base cell,
removes the actor from living occupancy/scheduling, and retains its identity for
history. Death of the selected human character ends the adventure and freezes
simulation time. The final disclosed view, history and save remain available.
A living starting character may satisfy a scenario objective, including an
optional AI-controlled starting companion; ordinary monsters cannot win.
Scenarios can choose continued play after victory, though this dungeon does not.

Accepted actions are saved asynchronously. Use explicit save or normal client
exit for a durability barrier. A crash may lose recent acknowledged play;
restart resumes the last saved prefix. Active travel is not restarted, and
control loss/recovery suspends human preparation. See
[persistence](../architecture/persistence.md) for the exact boundary.

## Content, mechanics and verification

The [scenario reference](../reference/scenario-packages.md) explains how other packages
choose characters, rules, factions and objectives. The [status page](../status.md)
separates implemented capabilities from future content and mechanics. This page
is a guide to the default adventure, not an inventory of every engine feature.

[`test_dungeon_process.py`](../../scripts/test_dungeon_process.py) covers the
complete native ASCII dungeon, text retrieval and saved victory, bump-attack
retrieval/escape, and persistent death. Creature, item and server suites cover
the shared mechanics beneath those runs. Historical timing figures are preserved
in the [original dungeon measurement record](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/dungeon.md#performance);
they are September 2026 Windows release measurements of older workloads, not
current latency promises. Use the [performance guide](../development/performance.md)
for comparable new measurements.
