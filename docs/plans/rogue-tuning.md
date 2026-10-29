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

`scripts/rogue_reference_model.py` computes Rogue's own expectations from the
research documents, without running Rogue:

- **Population.** Initial monsters per floor (25% per room, 80% with gold),
  species from the population index, monster Health and depth scaling.
- **Progression.** Expected kill XP per floor and the resulting level at each
  depth, from the XP thresholds and HP adjustments in
  [R05](../research/rogue-5.4.4/rules.md#r05--combat-statistics-and-advancement)
  and [R08](../research/rogue-5.4.4/rules.md#r08--monsters-ai-spawning-and-special-effects).
- **Supplies.** Nine item attempts at 36%, category weights (food 16%, of which
  90% rations), gold, drought frequency and expected food units per floor.
- **Pressure.** Wanderer arrivals per 1,000 turns from the wandering daemon,
  trap counts by depth, natural healing from the `doctor` formula.

Unit tests check each output against hand-computed examples.

## Layer 1: encounter targets

`scripts/rogue_reference_targets.py` derives, for each of the 26 species against
the reference PC at its depth band: hit chance both ways, damage per action,
actions to kill both ways, and signature-effect frequency (freezing, drain,
theft, rust, poison, gaze, breath, holding). A new arena matrix family pits each
species against the scheduled build, extending `scripts/rogue_roster_study.py`
and `scripts/arena_recipes.json`, and reports target, measured value and
tolerance. A species is tuned when every ratio is inside tolerance over the
paired seeds, as phase 7 defines.

Tuning follows the species waves in the [progression plan](progression.md).
Scalar values go through `scripts/arena_search.py` parameter specifications;
structural changes, such as a talent or strike order, are compared as candidates
with `scripts/arena_evaluation.py`. The first lead is Physical defense against
Rogue's armor classes.

## Layer 2: run-shape targets

**Run summary.** A versioned per-run summary record: death
depth and cause, level at each depth, ticks and commands per floor, food found
and eaten, time Hungry, Weak and fainting, potions and scrolls used, kills by
species, wanderer arrivals, traps triggered, Amulet reached and escape. It is
built from disclosed observations plus the private
[survival-flow diagnostics](../development/performance.md#private-committed-survival-flows).

**Campaign harness.** The earned-route controller grows into a deep-descent
controller: no purchase decisions (the build is locked), eat when Hungry, use
known healing, flee at low Health, descend after exploring. Many seeds run
through real processes, with bounded parallelism that respects free memory.
It is a fixed policy and is labelled as one; its results are evidence for the
tolerances below, not a claim about human play.

**Targets.** Level at depth against the model; food found against food spent
per depth band; share of deaths by starvation; wanderers per floor; and the
shape of the death-depth distribution. Tolerances are proposed from the model
and **ratified by the maintainer before tuning starts**.

## Food and the drought guarantee

Food comes only from Rogue's sources: the starting ration, floor items at the
model's expected rate with the ration and fruit split, and monster carry
chances. A chance-based food stage replaces the flat one-ration stage. Stage
identities stay append-only and existing streams stay untouched.

The drought guarantee is a profile mechanic. A run-scoped counter records
consecutive newly generated floors that produced no food. When it exceeds the
authored threshold (Rogue's is 3), the next first entry places one food item and
resets it. Revisits, speculative preparation, queries and frozen time never
change it, and a rolled-back entry restores it. This adds saved state and a
scenario field, so it needs a format decision. Tests: unit counter rules;
integration for rollback, discarded preparation and restart; an actual-process
run on a seed that reaches the guarantee.

## Healing

With care disabled, healing is Rogue's: natural recovery, healing and extra
healing potions, and level-up Health. The recovery period table is tuned to the
`doctor` rate at each level of the reference build. Regeneration's nutrition
cost is set to zero unless the run metrics show Rogue's food pressure needs it.

## Population, wandering, items and traps

- **Population.** Compare the four initial attempts and the per-depth bands with
  Rogue's per-room monster chance and population index; birth stock holds only
  items Rogue monsters carry.
- **Wandering.** Keep the source-shaped cadence and tune delay, trial and cap
  against the model's arrivals and the run metrics.
- **Items.** Replace one attempt per category with Rogue's item count and
  category and subtype weights.
- **Traps.** Tune DCs and durations against Rogue's saving-throw odds and trap
  counts by depth.

## Progression pacing

Fit the XP threshold and reward and the threat weights so the reference build's
level at depth tracks the model. Treat the Health roll per HD and the recovery
period as part of the same fit.

## Milestones

| Milestone | Delivers | Format change |
| --- | --- | --- |
| 1 | Implemented: capability switches, growth schedules and the `scenarios/rogue` package | Yes: protocol 65, save 64, scenario 36 |
| 2 | Reference model, targets script, run summary and campaign harness | No |
| 3 | Species encounter tuning, all 26, in waves | No |
| 4 | Food distribution and drought guarantee | Yes |
| 5 | Population, wandering, items and traps | No |
| 6 | Progression and recovery pacing; campaign acceptance | No |

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
