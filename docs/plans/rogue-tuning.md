# Rogue tuning plan

This plan tunes the `rogue-exploration` scenario so a run is recognizably
Rogue: its pacing, food pressure, monster danger by depth and progression. It
builds on the functional content described in the [Rogue plan](rogue.md) and
[player guide](../play/rogue.md), and specializes
[progression phase 7](progression.md#phase-7-rogue-behavior-targets-and-arena-tooling).
Each milestone below needs the maintainer's approval before it starts, and the
milestones that change a format need approval of that format decision too.
Every milestone follows the [testing policy](../development/testing.md).

## Goal and boundary

The target is recognition, not executable parity. Only Rogue's mechanics are
active in the Rogue scenario, running on TOR's engine: ticks, 3d6 checks with
edge, eye-based sight, persistent floors and paired stairs stay as the
[adaptation decisions](rogue.md#adaptation-decisions) record. Targets compare
ratios of actions and per-floor quantities, which survive the change from Rogue
turns to TOR ticks. Rogue's RNG, AC and XP equations, and regenerated floors are
not targets.

**No mechanic is removed from the game.** A scenario declares which rules and
mechanics it enables or disables. Rogue disables what Rogue lacks and enables
what only Rogue needs; every other scenario keeps today's behavior.

## Starting point

The bundled package already selects all 26 species, wandering population,
traps and secret entrances, potions, scrolls, rings, wands and staves,
equipment, gold and treasure rooms, Rogue's nutrition constants, the Amulet and
surface escape. Almost every number that shapes a run is marked provisional:

| Area | Current value | Location |
| --- | --- | --- |
| Presets | Warrior and Mage; mace, ring mail, 3 bandages, 1 ration; one racial plus one class HD | `scenarios/rogue-exploration/scenario.toml` |
| Upgrades | Player chooses a purchase at each earned HD | same, `future_growth = { kind = "selected" }` |
| XP | Threshold `100 + 100 × (tier − 1)`; reward `25 + 5 × relative tier`, 1–1000 | `crates/simulation/data/default-ruleset.toml` `growth` |
| Threat | 70 per HD, 5 per rank, 10 per talent, … | same, `growth.threat`, plus Rogue overrides |
| Health recovery | Period `max(200, 1200 − 40 × (HD − 1))` ticks | same, `health_recovery` |
| Care | Universal Bind Wounds in `default_abilities`; bandages | same, `constants`, `bind_wounds` |
| Respec | 600-tick preparation, costs one ration | same, `advancement` |
| Nutrition | Rogue's 1300/2000/300/150/−852; regeneration also costs nutrition (rates 1/1) | `scenarios/rogue-exploration/rules.toml` |
| Floor stock | One attempt per stage: 2 bandages, 1 ration, restore strength, potion, scroll, device, equipment, equipment scroll, ammunition, ring, control scroll; room gold 1/2 | `scenario.toml` `floor_activation` |
| Population | Four initial actor attempts per floor; per-depth species bands | same, `rogue.eight_species_foes` |
| Birth stock | Ration-or-bandage pool on several species; Leprechaun gold | same, `carried_item_pools` |
| Wandering | 6,700–7,300 tick delay, then 1-in-6 every 400 ticks, cap 8 | same, `floor_activation.wandering` |
| Traps | 1–2 hidden traps per floor, DC 12, bear 300 ticks, sleep 200 ticks | same, `hidden_traps` |
| Species | HD, die, strikes, Physical defense, talents, timings | `catalog/creatures.toml`, `catalog/actors.toml` |

There is no food-drought guarantee. Nothing measures a whole run: phase 7's
targets, reference PC and species matrix are unbuilt, and the only autonomous
descent controller, the [earned-route pilot](../development/rogue-earned-route.md),
is qualified only to earned HD 2. Foe Physical defense bonuses also sit well
below the spread that Rogue's armor classes imply.

## Rogue mechanics inventory

Milestone 1 completes this classification for every mechanic the package
exposes, from the [Rogue rules](../research/rogue-5.4.4/rules.md) and
[catalog](../research/rogue-5.4.4/content.md):

| Class | Meaning | Known members |
| --- | --- | --- |
| Rogue | Enabled and tuned | Melee, launch and throw, nutrition and fainting, natural recovery, potions, scrolls, rings, wands and staves, traps, search, wandering, monster specials, Amulet |
| Engine adaptation | Kept as the accepted TOR form | Ticks, 3d6 checks and edge, sight, persistent floors, paired stairs, NPC stair policy |
| Not in Rogue | Disabled for Rogue, kept in the engine | Bandages and Bind Wounds, ration respec, free purchase choice, Mage class and paid techniques, corpses (Rogue monsters leave none), nutrition cost of regeneration |

The tuned scenario is a separate package, `scenarios/rogue`. The
`rogue-exploration` starter slice keeps its Warrior/Mage presets, bandages and
free choices for the tests and play that rely on them. The new package shares
its catalog by copy; [`test_rogue_profile_content.py`](../../scripts/test_rogue_profile_content.py)
lists every intentional difference, so later tuning diverges deliberately.

## Scenario mechanics profile

Milestone 1 is implemented. Scenarios switch shared mechanics through the
existing `[capabilities]` table; omitted switches keep the engine default, and
compiled and saved policy records every switch. The server enforces each one:

| Mechanic | Mechanism | Default | Rogue |
| --- | --- | --- | --- |
| Owner advancement (purchases, source and Intellect choices, respec) | `owner_advancement` capability | enabled | disabled |
| Corpses | `corpses` capability | enabled | disabled |
| Bind Wounds | existing `default_abilities` rules override | granted | not granted |
| Food-drought guarantee | capability, milestone 4 | disabled | enabled |

Respec is an owner replacement, so `owner_advancement` covers it rather than a
second switch. Where an existing validated rules override already expresses a
choice, the package uses it instead of a new switch; numeric knobs, such as the
nutrition cost of regeneration, stay numeric and are tuned in milestone 6.
The advancement overview discloses `locked`, so clients explain why choices are
unavailable instead of offering them.

## The reference PC and its fixed upgrade path

Rogue has one adventurer and no level-up choices, so `scenarios/rogue` has one
preset whose build advances automatically. That build is the reference every
other element is tuned against.

- **Start.** A human with one racial body HD and one Warrior HD, carrying
  Rogue's [starting kit](../research/rogue-5.4.4/rules.md#r01--run-goal-and-starting-state):
  mace, ring mail, shortbow, 32 arrows (the midpoint of Rogue's 25–39) and one
  ration, all known. The mace and ring mail keep the starter values; Rogue's +1
  bow, 12 HP and Strength 16 mappings are milestone 6 tuning.
- **Each level.** One Warrior HD (d10, like Rogue's +1d10) plus the next row of
  `growth_schedule`. Seventeen rows buy one skill rank each, interleaving melee,
  ranged, Athletics and Discipline ranks to the cap of five; three later rows
  add Health only, for 20 rows matching Rogue's experience table.
- **The lock.** `restoration = "authored"`, `future_growth = { kind =
  "authored" }` and `owner_advancement = false`. The schedule is checked at
  compilation as if every row were owned. A drained row regrows with its
  scheduled purchase; an exhausted schedule holds earned XP.
- **Tests.** Simulation unit tests cover scheduled growth, saved records with
  grown and ungrown rows, drain and regrowth, inadmissible schedules, corpses and
  the lock; protocol tests cover `locked`; actual-process tests cover a locked
  kill that grows its scheduled row and leaves no corpse, the Rogue package's
  start, and compiler rejections.

Because the build is fixed, the reference PC for a depth is simply the scheduled
build at the level Rogue's player is expected to have reached there.

## Rogue reference model

Implemented as [`rogue_reference_model.py`](../../scripts/rogue_reference_model.py).
It evaluates Rogue's own rules from the research documents, exactly where they
are closed-form, without running Rogue:

- **Population.** 7.5 present rooms at 52.5% gives about 3.9 initial monsters per
  floor; species probabilities come from the population index, including its
  low and deep redistribution and the wandering exclusions.
- **Progression.** Expected kill XP, including depth and Health adjustments, and
  the resulting level on arrival at each depth from the threshold table.
- **Supplies.** Nine 36% attempts with category weights; food is a renewal chain
  over the drought counter, giving about 0.60 food items per new floor, with
  the fourth consecutive foodless floor forced (about one floor in ten).
- **Pressure.** One wanderer per 94 turns on average, trap counts by depth, and
  the `doctor` healing rate by level.

Time per floor and the share of monsters killed are play assumptions, stated as
parameters. With 900 turns per floor the model expects level 4 by depth 3,
7 by depth 8, 10 by depth 16 and 14 by depth 24.
[`test_rogue_reference_model.py`](../../scripts/test_rogue_reference_model.py)
checks each part against hand-computed values.

## Layer 1: encounter targets

[`rogue_reference_targets.py`](../../scripts/rogue_reference_targets.py) derives,
for each species at its typical depth against the reference adventurer: hit
chance both ways from `swing()`, damage per action, exact expected attacks to
kill both ways (kills are granular, so not Health divided by mean damage) and
the per-action rate of the signature effect after saving throws.

[`rogue_encounter_study.py`](../../scripts/rogue_encounter_study.py) measures the
same ratios with `tor-arena`: the locked adventurer, grown along its own schedule
to the reference level, against one ordinary actor of the species, in both
orientations over paired seeds. The study copy makes the adventurer engage first,
so passive species are measured too. Arena damage excludes overkill, so the
adventurer's damage per action is reported but not judged; the other ratios are
judged against a ±35% band.

[`rogue_species_tuning.py`](../../scripts/rogue_species_tuning.py) then changes
only authored values in `scenarios/rogue/catalog/creatures.toml`: a Health
modifier toward Rogue's `level d8` mean, the Physical defense bonus toward the
adventurer's hit chance, accuracy toward the foe's hit chance, and a damage-dice
bonus toward its damage per action. Natural-attack checks take attribute plus
skill rank, not the melee bonus, so their accuracy is an attack-attribute
modifier that does not raise defense; selected-weapon checks use the melee
bonus. Defense and accuracy move one point per pass, two beyond ×1.4; damage
moves one point; Health moves straight to its target and backs off if any birth
is rejected. Species that attack fewer than 20 times in a study are left alone,
because their ratios are noise. Mechanics are not changed: armor keeps its damage reduction, so small
dice gain a flat bonus instead.

Study-only arrangements let every species be measured without changing the
scenario: against the invisible phantom the adventurer wears see invisible, as
a Rogue player would; the revealed xeroc engages first; and the orc, flytrap and
xeroc get four times the seeds. The orc needs no edit: it is greedy, as below.
The arena itself now counts an admitted autonomous intention that fails at
execution as a lost turn, ending the encounter as a stalemate after eight in a
row, instead of rejecting the encounter.

Orc acquisition (fixed): the study once forced orcs to engage, which hid a
behavior difference. The package authored orcs as passive with Gold interest,
so they never attacked unless provoked; in Rogue a greedy monster that notices
the player heads for the room's gold, or for the player when there is none.
With the maintainer's approval TOR gained a `greedy` disposition: a greedy
species collects known Gold it can safely reach under its Gold interest and
otherwise acquires a visible hostile as a mean species does. Rogue's orcs use
it; the starter package keeps its passive orcs. The value is authored and saved,
within this branch's unreleased format bump. Measured without any study edit,
the orc attacked 166 times over seeds 0–15 with every judged ratio in band.

The baseline had 72 of 130 judged ratios in band. TOR foes had far more Health
than Rogue's (a full first die), dealt little damage through ring mail, and were
easier to hit at shallow depths. After eleven passes, 114 of 116 judged ratios
are in band, with all 26 species measured. The emu's 1d2 and the leprechaun's
fixed 1 damage were absorbed by ring mail's reduction; as tuned values, their
dice become 1d3+2 and 1d3+1, which deliver Rogue's expected damage through the
armor. With those, all 116 judged ratios are within the ratified ±35%. The
final study over seeds 0–15 judged 117 ratios, all within ±35%, with at least
20 measured attacks for every species.

Signature effects (30 October 2026). Arena reports count the effects each
participant applied, and the study judges each species' signature per foe action.
Each encounter stands on a quiet floor of the species' depth (an arena `floor`),
so floor rules such as theft apply, and a ratio is judged only when its side made
at least 20 attack checks; species that seldom attack get four times the seeds.
Rogue 5.4.4 monsters never flee, so the package disables the
`low_health_flight` capability rather than editing species.

Final study over seeds 0–15: 128 judged ratios, all within ±35%.
medusa's confusion.

| Species | Signature | Target | Measured | Foe checks | Within ±35% |
| --- | --- | --- | --- | --- | --- |
| I ice monster | freeze | 0.35 | 0.12 | 16 | Not judged |
| R rattlesnake | Strength poison | 0.20 | 0.15 | 54 | Yes |
| L leprechaun | steal gold | 0.27 | 0.31 | 83 | Yes |
| A aquator | rust armor | 0.80 | 0.85 | 118 | Yes |
| N nymph | steal magic item | 0.45 | 0.42 | 74 | Yes |
| F venus flytrap | hold | 0.70 | 0.69 | 826 | Yes |
| W wraith | level drain | 0.08 | 0.08 | 212 | Yes |
| M medusa | gaze confusion | 0.49 | 0.56 | 330 | Yes |
| V vampire | maximum-Health drain | 0.21 | 0.18 | 582 | Yes |

Values chosen by measurement: the wraith's drain rider uses Lore against Physical
defense and the vampire's Spellcasting against Cognitive; the flytrap gains one
point of accuracy; the thieves gain accuracy and their theft riders use natural
attack against Spiritual (a theft rider must keep a check, so Rogue's certain
nymph theft becomes the easiest pairing); the leprechaun deals 1d5 so its damage
survives ring mail as Rogue's does; the medusa confuses with its first strike, as
in Rogue, instead of spending turns on a separate gaze; the ice monster and bat
gain defense and Health, and the bat accuracy and damage, so they live to strike
back. A per-action [timeline](../development/arena.md#offline-evaluation) showed
why they never did: a provoked passive monster's next decision falls at the tick
of the swing that provoked it, so it waits a step, and a monster that dies to one
blow never acts.

Open limits: the dragon's breath is a ranged action in Rogue, not a hit rider, so
the per-hit target does not judge it. The ice monster attacks too rarely (16
checks) for its own ratios to be judged, as in Rogue. Rogue lengthens confusion that lands while the
player is already confused and grants no immunity afterwards, so `rogue.confused`
extends and has no expiry protection; with `keep` and 400 protected ticks, the
medusa confused at about half Rogue's rate.

Open engine defect: a held adventurer's AI basic-melee plan against an adjacent
flytrap fails execution with `InvalidPlan` when the flytrap's Clutch resolves
first, and the rollback makes the same choice repeat. The arena behavior above
keeps flytrap encounters measurable; the AI admission/execution ordering needs
its own fix.

## Layer 2: run-shape targets

**Run summary.** [`rogue_campaign.py`](../../scripts/rogue_campaign.py) writes a
versioned summary per run: outcome, death depth, level on arrival at each depth,
ticks and commands per floor, share of time hungry, meals, rest episodes, final
level, Health and earned XP. Wanderer arrivals, kills by species, traps
triggered and food found are listed as unavailable from ordinary disclosures.

**Campaign harness.** Each seed starts a real server on `scenarios/rogue` and
drives the locked adventurer with the earned-route policy through the headless
client, settling paused work with disclosed permission. There are no purchase
decisions. The campaign aggregates death depths, mean level on arrival and the
hungry share. It is a fixed policy, not a model of human play: apart from
drinking a known healing potion at low Health it uses no potions, scrolls or
wands, and it eats only rations, so its deaths overstate Rogue's danger.

Searching when the known route is exhausted (stairs behind secret doors) and
waiting out refused steps let runs from depth 1 reach depths 3 to 5. Deep starts
(`--start-depth`) begin at a floor's up stairs with the model's level for that
depth, the median best armor from Rogue's expected armor finds, half the
expected healing potions and a spare ration; the bot drinks a known healing
potion below 40% Health. From depth 1 the adventurer reaches level 4 by depth 3
and 5 by depth 4 or 5, matching the model. Deep starts survive with that gear,
but the fixed bot earns little XP within its command budgets, so deep pacing is
not yet measured.

Two player habits keep the bot from dying of its own policy: when Weak or
fainting with no food it stops resting and heads for known down stairs, and it
keeps fighting an adjacent foe at low Health instead of walking away (escaping
only up stairs). Each run records the foes, phases and hunger over its last 30
samples and the hunger at death. `--work-root` puts saves and process logs on a
fast volume, and the headless client's output spool rolls over at 64 MiB, since
disk stalls on a slow volume blocked the harness past its reply deadline.

The campaign also reads the saved first-entry receipts and counts the items
that actually landed. [`rogue_generation_probe.py`](../../scripts/rogue_generation_probe.py)
generates 120 first floors: 0.56 food per floor against Rogue's 0.52 ordinary
rate (z = 0.6) and 3.7 items per floor, so generation matches the source.

**Targets.** Ratified by the maintainer on 29 October 2026: level on arrival
within ±1 of the model at depths 1–10 and ±2 at 11–26; food found against spent
within ×1.5 of the model per five-floor band; starvation at most 10% of deaths;
at least 75% of runs reach depth 4; every species with at least 20 measured
attacks within ±35% of its encounter targets.

## Food and the drought guarantee

Implemented. Food comes only from Rogue's sources: the starting ration, floor
items and monster carry chances. Two `chance_items` stages place nine attempts
each, with chances of 36% times their categories' share, so every item type's
expected count per floor equals Rogue's nine 36% attempts; a 64-choice band
limit is why there are two. Missiles keep Rogue's groups of 8 to 15.
[`rogue_item_table.py`](../../scripts/rogue_item_table.py) generates these
stages from the research tables.

The `food_drought` stage places one ration (or, one time in ten, fruit) when
three consecutive earlier first-entry floors and this floor's earlier stages
placed no food (`threshold = 2`: more than two earlier floors). Rogue increments
`no_food` before placing a new level's items and forces food when it exceeds 3,
so its fourth foodless level is the forced one; the package first used 3, one
floor late, until campaign starvation exposed it. The count is derived from the retained entry records, so no new
counter is saved; revisits, preparation and rollback cannot change it, and the
receipt audit recomputes it. See the
[authoring reference](../reference/scenario-packages.md#first-entry-population).

## Healing

With care disabled, healing is Rogue's: natural recovery, healing potions and
level-up Health. The recovery override (base 2300 ticks, −200 per HD, floor 150)
gives about 21 turns per HP at level 1 and 9 at level 7, close to the `doctor`
rate of 19 and 7. Regeneration's nutrition cost is zero, as in Rogue.

## Population, wandering, items and traps

- **Population.** Four initial attempts already match Rogue's 3.9 initial
  monsters per floor. A treasure room appears on 1 in 20 floors, with monsters
  drawn one level deeper; its items omit rings and wands to fit one band.
- **Wandering.** The existing cadence (6,700–7,300 ticks, then 1 in 6 every 400)
  matches Rogue's one wanderer per 94 turns.
- **Items.** Rogue's counts and category/subtype weights, as above.
- **Traps.** Hidden traps stay at one or two per floor, averaging Rogue's count
  over depths 1–26; generation has no depth-scaled trap count.

## Progression pacing

Deep floors are too slow and dangerous for the fixed bot to measure pacing, so
[`rogue_pacing_model.py`](../../scripts/rogue_pacing_model.py) predicts it from
the tuned values: each foe's threat tier from its authored Hit Dice, ranks,
talents and sources, TOR's kill reward and threshold rules, and kills per floor
from the four initial attempts, Rogue's wanderer rate and treasure rooms. Like
the reference model, it assumes every monster is killed.

With the shared thresholds (100 + 100 per Hit Die) or the first early fit
(60 + 40), the adventurer falls behind deep: level 10 at depth 26 where Rogue's
model expects 14. Kill rewards shrink as foes fall below the adventurer's Hit
Dice, which already plays the part of Rogue's doubling thresholds, so the Rogue
rules use a flat 85 XP per Hit Die. That keeps every depth from 1 to 26 within
one level of the model (mean error 0.42 levels), slightly ahead at depths 2–5.
Seeded campaigns confirm the early floors; deeper confirmation through play
needs a bot that kills more efficiently.

## Milestones

| Milestone | Status | Format change |
| --- | --- | --- |
| 1 | Implemented: capability switches, growth schedules and the `scenarios/rogue` package | Protocol 65, save 64, scenario 36 |
| 2 | Implemented: reference model, targets, encounter study, run summary and campaign harness | No |
| 3 | Implemented: value tuning of all 26 species; all 117 judged ratios within ±35%; greedy orc acquisition | `greedy` disposition, within the unreleased 64/36 bump |
| 4 | Implemented: Rogue item table, grouped missiles and the drought guarantee | Within the unreleased 64/36 bump |
| 5 | Implemented: treasure rooms; wandering and initial counts already match | No |
| 6 | Implemented: recovery, regeneration cost and XP thresholds fitted at every depth by the pacing model; tolerances ratified | No |

Package edits regenerate certificates and run
`python scripts/test_scenario_references.py`. Each milestone updates the
[player guide](../play/rogue.md), the [Rogue plan](rogue.md),
[status](../status.md) and the references it changes.

## Acceptance

- Every species is inside its encounter tolerances against the reference build.
- Campaign metrics are inside the ratified run-shape tolerances on a fixed seed
  set.
- The drought guarantee works across rollback, restart and replay.
- The Rogue profile leaves no non-Rogue mechanic enabled, and each disabled
  mechanic still works in a scenario that enables it.
- Engine adaptations are recorded as deliberate differences, not tuned away.

### Acceptance results (30 October 2026)

[`rogue_acceptance.py`](../../scripts/rogue_acceptance.py) checks the ratified
tolerances against a sixteen-seed campaign from depth 1 (seeds 0–15, 3,000
commands per floor, 15,000 per run) and the final encounter study, with the
orc measured under its own greedy acquisition.

| Tolerance | Result | Evidence |
| --- | --- | --- |
| Level on arrival, pacing model | Pass | Depths 1–26 within one level |
| Level on arrival, campaign | Pass | Mean 1, 3.0, 4.6, 5.5, 6.0, 6.2, 6.6, 6.8 and 7.0 at depths 1–9 against 1, 3, 4, 5, 5, 6, 6, 7 and 7 |
| Encounter ratios | Pass | 26 species, 117 judged ratios, all within ±35% |
| Food found against spent | Pass | 0.93 of the model's ratio at depths 1–5 and 1.03 at 6–10, at the bot's pace |
| Runs reaching depth 4 | Pass | 13 of 16 |
| Starvation share | Waived: bot-limited | 5 of 9 deaths |

The maintainer accepted food as matching Rogue and waived the starvation share
as a limit of the bot, not the tuned values (30 October 2026):

- Generation matches the source: 0.56 food per first floor over 120 floors
  against Rogue's 0.52 ordinary rate, and the drought guarantee now forces food
  on the fourth foodless floor, as Rogue's `no_food` counter does.
- At Rogue's rates a floor yields about 780 turns of food. The bot spends about
  980 turns per floor at depths 1–5, much of it resting to full Health, so long
  runs starve by arithmetic; four of the five starved runs ate all or nearly
  all the food they picked up. One rested for 900 turns and then descended past
  uncollected food.

Fixes made while measuring:

- Levels on arrival compared ticks as text, crediting later level-ups to
  shallow floors; earlier campaign level results were unreliable.
- The starvation check judged any empty spell during a run; it now uses the
  server's named cause of death, falling back to the hunger at death for older
  runs.
- A failed closing save no longer discards a finished run. Three concurrent
  debug servers with 35–70 MB saves fell behind on saving ("Saving is behind
  schedule") until an explicit save reply timed out; this is recorded as a
  possible persistence performance issue, not investigated here.
