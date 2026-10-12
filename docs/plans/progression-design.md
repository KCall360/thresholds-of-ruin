# Progression and creature construction design

This is the accepted design direction for classes, talents and constructed
creatures. The first check, condition and permanent-requirement foundations are
implemented; the remaining progression and creature content is unfinished.
The [implementation plan](progression.md) orders the work;
[creature rules](../reference/creature-rules.md) remain the
authority for what exists today; [status](../status.md) owns scope.

The catalog here starts from the maintainer's *Thresholds of Ruin Simulation
Talent Design* proposal (Revision 2, 12 October 2026). That proposal is kept
outside the repository. This page keeps its structure, timing contracts and
creature trees, and applies the review decisions recorded below. Every numeric
value is provisional until the [arena](../development/arena.md) measures it.

## Review decisions

The proposal was reviewed against the current rules. These decisions change it:

1. **Edge is the main tactical currency.** Temporary and tactical effects use
   advantage and disadvantage. Flat check bonuses are reserved for permanent
   progression. The proposal priced edge like a small flat bonus; it is not.
2. **Natural Health regeneration exists.** Talents, species and supplies can
   improve it. The proposal's no-regeneration survival profile is dropped.
3. **Rogue monsters are constructed creatures.** Each is a species with a type,
   subtypes and racial or class HD whose abilities are bought from eligibility-
   gated trees. The proposal's creature trees stay; Rogue-driven trees are added.
4. **Multiple attacks are strike sequences.** One ability holds an ordered list
   of strikes, each with its own check, damage and time offset.
5. **The arena tunes creatures to behavior targets** derived from the Rogue
   reference, rather than copying Rogue's numbers.
6. **Advancement grants are data.** Point pools and what they buy are ruleset
   content, so the economy can change without an engine refactor.
7. **Duplicates are merged.** Same-effect conditions become one family and
   duplicate nodes are replaced (see [catalog changes](#catalog-changes-from-the-proposal)).

## Principles

1. **One rules kernel.** PCs and creatures use the same builder, advancement
   ledger, abilities, resources, items, timing and AI action admission. A
   monster has no power a PC could not, in principle, construct.
2. **Edge first.** Positioning, setups, conditions and talents grant advantage or
   disadvantage. Skill ranks, weapons and enchantment provide flat accuracy.
3. **Scarce points, meaningful choices.** A point buys a talent or a skill rank.
   Because ranks are the only permanent flat accuracy, ranks and talents compete.
4. **Useful from HD 1.** Every root grants a working action. Solo play never
   requires a companion, a negotiating enemy or a rare item.
5. **Visible commitment and counterplay.** Preparation is a tell, values freeze
   at preparation, and nobody gets an extra turn or a reaction chosen after
   seeing a private roll.
6. **Finite and deterministic.** Cooldowns, charges, supplies and failed
   knowledge attempts persist across save, replay, freeze and thaw. No rule needs
   a game master's judgment.
7. **Ownership is per HD.** Each owned choice belongs to the HD that bought it.
   Level drain, restoration and respec work identically for PCs and monsters.
8. **Baseline traits are free but counted.** Type, subtype and species traits
   need no points, but they count toward a creature's threat. Anything unusual
   is purchased.
9. **Tune behavior, not formulas.** A constructed Rogue monster should play like
   its Rogue counterpart under TOR's timing, sight and physics.

## Patterns rejected

- Game-master decisions about credible bargains, convenient teachers or safe camps.
- Scene or encounter resets without a machine-defined event.
- Free ally turns, retroactive defense rewrites, or reactions chosen after a roll.
- Progression locked behind loot with no reachable fallback.
- Generic stacking damage-die or accuracy chains bought every few levels.

## Advancement model

Each HD carries one owned advancement entry. Its source is racial or a class.
Ruleset data defines the point pools each HD grants and what each pool may buy.
The recommended default is one shared point per HD, spent on one talent or one
skill rank, plus two distinct rank-1 starting skills granted once at creation.

| HD | Current grants (pure class) | Recommended shared points | Starting ranks |
| ---: | --- | ---: | ---: |
| 1 | 1 talent + 2 training ranks | 1 | 2 |
| 10 | 10 talents + 20 ranks | 10 | 2 |
| 30 | 30 talents + 60 ranks | 30 | 2 |

Rules that hold whatever the pool definitions are:

- Purchased talents, purchased ranks and unspent points equal the granted points.
- Starting ranks are recorded separately and cannot be claimed again through
  multiclassing, templates, drain or reconstruction. All ranks cap at 5.
- Requirements use permanent build facts only: total, racial and class HD, owned
  talents, skill ranks, attributes, type, subtype and permanent flags. Equipment
  and temporary conditions never satisfy them.
- OR parents use `any` requirement groups over owned talents; functional bases
  use `all`.
- Attribute opportunities every 4 HD remain separate from points.

**Scarcity check.** With one point per HD and three nodes per band, a single-
class character can own its entire class map by HD 26. That is acceptable only
if rank-heavy and shared-tree builds stay competitive. The arena's choice-
concentration test decides; if they do not, add genuine skill gates per row of
each branch.

### Drain, restoration and respec

- Level drain removes the latest HD and the choice it owns. Requirements are then
  rechecked; nodes whose HD, class or skill gates fail become dormant even if an
  earlier HD bought them. Type, subtype and species traits are never drained.
  Removing the last HD is ordinary death.
- A creature's authored build lists its intended choice per HD. Restoring or
  regrowing an HD refills that choice from the build. A PC chooses again, with a
  preview, for an empty restored slot.
- Purchase order is authored data. A signature bought on HD 1 is lost last; a
  late or high-gated purchase is lost first.
- Respec happens at a disclosed safe place: no sensed hostile, no active hostile
  plan and no recent Health loss. It replaces one purchase and its dependent
  subtree in one transaction, takes time and consumes a ration, and preserves
  injury, cooldowns, charges, item identities and failed-knowledge locks.

## Edge model

The current kernel still uses d20 checks: any non-zero allocated check edge
rolls a second d20 and keeps the better or worse result. The following is the
accepted **next implementation**, not a description of the current kernel:

- Checks roll 3d6. Positive and negative edge sources cancel one-for-one.
- Up to three remaining edge units affect the check: roll that many extra d6
  and keep the highest three for advantage or lowest three for disadvantage.
- Excess edge is allocated to damage under the existing allocation policy.
  Damage keeps its existing rolled pools; damage edge adds dice and keeps the
  best or worst as before.
- Natural totals of 3 and 18 do not automatically fail or succeed.
- Meeting a difficulty still succeeds; opposed checks retain the current tie
  rule, with both participants using the same 3d6 kernel.
- Defenses and bonuses are retuned through arena measurements. Do not copy d20
  hit probabilities through a mechanical numeric conversion.

Check-only sources are confined to checks. Freeze the combined check edge and
the user's shared check/damage budget separately. When the user rolls a check,
spend up to three shared units independently of check-only sources; only that
shared remainder reaches damage. A target-only resistance check does not spend
the user's budget, while an opposed user roll does. For example, shared +5 and
check-only −6 give check −1 and damage +2. An accuracy penalty neither changes
damage nor refunds shared units. Condition-family and talent caps apply per
check, not to an independent shared damage budget. Canonical component ordering
and these source scopes must survive preparation, cancellation and save/replay.

Design rules:

- Each condition family contributes at most one edge, however many instances or
  sources apply. Talent-granted edge on one check is limited to +1 and −1 before
  other sources combine.
- A temporary effect that would have been "+N defense" becomes "checks against
  you have disadvantage". A temporary "+N check" becomes advantage.
- Talents never add flat check bonuses. Flat bonuses come from ranks, weapons,
  enchantment and attributes.
- Edge on damage is used sparingly, as a damage-pool edge rather than an extra
  rider die, so it cannot stack with riders.

### Condition families

| Family | Effect | Descriptor | Replaces |
| --- | --- | --- | --- |
| Fear | Disadvantage on checks against the source | `fear`, `mind_affecting` | Fear, Hesitant, Unsettled |
| Rattled | Disadvantage on checks against the source | `pressure` | Rattled |
| Braced | Physical checks against the holder have disadvantage | — | +2 Physical defense |
| Readied (token) | Holder's next qualifying check has advantage | — | Readied |
| Exposed (token) | Next check against the holder has advantage | — | Exposed, Distracted |
| Distracted (token) | Holder's next check has disadvantage | — | −1 Cognitive defense |
| Tempo (token) | Next preparation shortened (timing, not edge) | — | Tempo |
| Held | Cannot move; other actions allowed; escape action | — | Rogue hold, bear trap |
| Frozen | Cannot start new preparations; followed by a thaw window | `cold` | Rogue freeze |
| Confused | Movement intents may be randomized | `mind_affecting` | Rogue confusion |

Tokens are consumed when a qualifying preparation starts, even on a later miss or
cancellation. A reserved token cannot be renewed or rewritten. A token never
benefits the action that created it. Action-denying conditions (Held, Frozen,
Confused) need a counter and must not chain into renewable loss of all actions;
each is followed by an immunity window.

## Health regeneration

Health becomes a recovering pool alongside Stamina, Focus and Mana:

- Recovery period is a derived stat. It shortens with HD, following the shape of
  Rogue's level-scaled healing; modifiers from talents, species and items change it.
- Taking or dealing damage resets accrued recovery progress ("quiet"). Walking
  does not.
- Regeneration uses actor-active time. Frozen regions accrue nothing; crossing a
  region seam restores nothing.
- Hunger can slow or stop recovery once nutrition exists; the
  [Rogue plan](rogue.md) owns nutrition.
- Bandages and potions remain faster, finite healing.

Talents that enhance regeneration include Steady Recovery (U03), Hardy Recovery
(U07), Field Dressing's recovery boost (E01) and the creature
[Regeneration tree](#regeneration-tree).

## Timing, attacks and resolution

The proposal's timing contracts are retained:

- Phases scale with Speed by `ceil(base×5/(5+Speed))`, minimum 1.
- Printed costs pay 1 at preparation and the rest at resolution, including on a
  failed check. Item costs are reserved at preparation and consumed at resolution.
- Movement routes run as scheduled cell steps with rechecks at each step.
- Cooldowns use actor-active time, start at preparation and survive cancellation.

### Strike sequences

An attack ability is an ordered list of one or more strikes. Each strike declares
its check, damage components, on-hit riders and offset from the first strike.

- Preparation happens once and freezes every strike's operands.
- Each strike resolves at its own tick. Later strikes recheck the target's life
  and reach, so stepping away during a sequence makes the remaining strikes miss.
- Recovery starts after the last strike. Damage that suspends the plan keeps
  strikes already resolved.
- An ordinary attack is a one-strike sequence. Offsets of zero model simultaneous
  attacks.
- Conditions apply to every strike. A one-use token applies to the first strike
  only, unless its definition says the whole sequence.
- A species authors all strikes of its natural attack. Strikes beyond the first
  stay dormant until a [Natural Weapons](#natural-weapons-tree) node unlocks them,
  so drain can remove them.

### Hit resolution order

For each strike that hits:

1. Resolve the frozen check and damage payload.
2. Apply live immunity, then reduction.
3. Apply at most one prevention or redirection (reserved slot; filled by later
   reaction policies).
4. Commit Health and death.
5. Apply on-hit riders. A rider may carry its own check, rolled by the attacker
   against the target's stated defense; this is how Rogue's saves are expressed.
6. Dispatch after-hit hooks.

### Reactions and fields

Committed reaction policies, persistent fields and forced displacement keep the
proposal's contracts: one armed policy, a reserved trigger cost, recovery debt,
a global trigger lock, region-owned field clocks, one field per family per owner,
and body-size limits on pushes. They are later phases.

## Class trees

Each class has three branches and six rows. A row needs total HD 1, 6, 11, 16, 21
or 26 and class HD 1, 3, 6, 9, 12 or 15. Roots need rank 1 in the listed skill.
Later nodes need a parent from the adjacent branches (`/` means OR) plus any
listed functional base. The **Needs** column names the capabilities from the
[capability table](#capabilities); **C** means existing core effects.

### Warrior (d10)

Sentinel controls contact, Duelist exploits timing, Breaker moves enemies.

| ID | Node | Parents | Effect | Needs |
| --- | --- | --- | --- | --- |
| W01 | Brace for Impact | Discipline 1 | 100/100, S2: Braced for 300 ticks | C |
| W02 | Measured Cut | weapon skill 1 | Weapon attack, S2, that ignores your negative edge sources | C |
| W03 | Rattling Blow | weapon skill 1 | Weapon attack, S2; hit applies Rattled for 200 ticks | C |
| W04 | Guarded Step | W01/W02 | Step one cell and gain Braced for 200 ticks | X3 |
| W05 | Read the Rhythm | W02/W03 | Reveal a visible foe's public plan; Readied against it | X1 X2 |
| W06 | Dislocating Strike | W01/W03 | Weapon hit pushes one cell | X3 |
| W07 | Catch the Impact | W04/W05 | Policy: prevent up to 3 from a hit of 4 or more | X1 X4 |
| W08 | Answer the Miss | W05/W06 | Policy: a melee miss against you grants Readied and Tempo | X1 X4 |
| W09 | Open the Guard | W04/W06 | Weapon hit applies Exposed | X1 |
| W10 | Hold the Passage | W07/W08 | Mark two cells; hostile crossings take +100 ticks | X3 |
| W11 | Break Their Tempo | W08/W09 | Weapon hit adds 100 ticks to the target's preparation | X2 |
| W12 | Sweeping Exchange | W07/W09 | Two-strike sequence on two targets at 75% damage | X9 |
| W13 | Shelter Under Pressure | W10/W11 | Policy: prevent up to 6 and clear an unreserved Exposed | X1 X4 |
| W14 | Commitment Punisher | W11/W12 | Advantaged weapon attack on a recovering target; Braced on hit | X2 |
| W15 | Breach Line | W10/W12 | Move up to 3 cells, then strike and push sideways | X3 |
| W16 | Last Safe Ground | W13/W14 | Field: you and two allies Braced and immune to pushes | X1 X3 |
| W17 | Perfect Exchange | W14/W15 | Advantaged attack; on hit retreat 2 cells or grant Tempo | X1 X3 |
| W18 | Fault Line | W13/W15 | Telegraphed 3-cell line; strike each target and push | X3 X5 |

### Mage (d6)

Evocation, Binding and Warding are independent roots. Each root grants a form;
mastery profiles (M04, M07, M10, M13) are mutually exclusive modifiers of one
known evocation form. Advanced forms are discovered through a seeded registry
with a guaranteed non-random route by their band.

| ID | Node | Parents | Effect | Needs |
| --- | --- | --- | --- | --- |
| M01 | Evocation Initiate | Spellcasting 1 | Spark Lance: 1d6+1 fire or cold energy vs Cognitive, M2 | C X8 |
| M02 | Binding Initiate | Spellcasting 1 | Falter: Spellcasting vs Spiritual applies Fear for 300 ticks | C |
| M03 | Warding Initiate | Spellcasting 1 | Shell: reduction 2 in a chosen category for 400 ticks | X7 |
| M04 | Extended Projection | (M01/M02) + M01 | Profile: +2 range | X8 |
| M05 | Anchor Sigil | (M02/M03) + M02 | One-use sigil stops the first entrant's move on a failed Agility check | X3 |
| M06 | Shared Shell | (M01/M03) + M03 | Shell on one recipient for 500 ticks | X1 |
| M07 | Forked Projection | (M04/M05) + M01 | Profile: two-target sequence at 70% | X9 X8 |
| M08 | Bind the Moment | (M05/M06) + M02 | Add 100 ticks to a preparation, or delay the next one | X2 |
| M09 | Cleanse Pressure | (M04/M06) + M03 | Remove one Fear, Rattled, Exposed or Distracted | X6 X1 |
| M10 | Piercing Projection | (M07/M08) + M01 | Profile: ignore up to 2 reduction | X7 X8 |
| M11 | Linked Anchors | (M08/M09) + M05 + M02 | Two sigils sharing one trigger | X3 |
| M12 | Shelter Weave | (M07/M09) + M03 | Shell to up to three recipients, or solo buffer 3 | X1 X5 |
| M13 | Dispersal Projection | (M10/M11) + M01 | Profile: telegraphed radius-1 burst, up to 3 targets at 70% | X3 X5 X8 |
| M14 | Quiet the Pattern | (M11/M12) + M02 | Suppress one temporary defense or reduction for 300 ticks | X6 |
| M15 | Spell Shelter | (M10/M12) + M03 | Policy: prevent up to 6 energy damage | X1 X4 |
| M16 | Horizon Aperture | (M13/M14) + M01 | Linked gates with three transits | X3 |
| M17 | Seal the Working | (M14/M15) + M02 | Block an observed ability for 300 ticks | X2 X6 |
| M18 | Refuge Geometry | (M13/M15) + M03 | Field: Cognitive and Spiritual checks against recipients have disadvantage; Shell | X3 X7 |

Form tiers keep the proposal's ceilings: novice 1d6+1 (HD 1, Spellcasting 1),
adept 2d6+1 (HD 11, Mage 6, Spellcasting 2), master 3d6+1 (HD 21, Mage 12,
Spellcasting 3). A form is an immutable definition; rarity never exceeds its tier.

### Scout (d8, new class)

Stalker shoots, Pathfinder moves, Saboteur prepares ground. The starting kit
supplies the chosen root's weapon, projectiles or kits.

| ID | Node | Parents | Effect | Needs |
| --- | --- | --- | --- | --- |
| S01 | Patient Shot | Light Weaponry 1 | Ranged attack with longer phases and advantage; consumes a projectile | X8 X13 |
| S02 | Trail Measure | Survival 1 | Mark a route; ignore 100 terrain ticks; advantage on its first hazard check | X2 |
| S03 | Distraction Toss | Thievery 1 | Dust flask; Thievery vs Cognitive applies Distracted | X8 |
| S04 | Read the Prey | S01/S02 | Learn a target's state and one demonstrated protection; Readied | X2 |
| S05 | Sure Route | S02/S03 | Move up to 3 cells ignoring 200 terrain ticks | X3 |
| S06 | Snare Line | S01/S03 | One-use snare: failed Agility adds 100 ticks to the move | X3 X8 |
| S07 | Wait for Exposure | (S04/S05) + S01 | Patient Shot on a recovering target adds a 1d6 rider | X4 |
| S08 | Shared Footing | S05/S06 | A recipient's next route ignores 200 terrain ticks and one field delay | X3 X1 |
| S09 | Selective Trigger | (S04/S06) + S06 | Snare filter: hostiles only, or all but you | X3 |
| S10 | Break Contact | S07/S08 | Policy: after a miss against you, step to a declared cell | X3 X4 |
| S11 | Rope and Reach | S08/S09 | Rope traversal link across a gap up to 3 | X3 X8 |
| S12 | Spoil the Mechanism | S07/S09, Crafting 2 | Delay a construct's or manufactured weapon's plan by 100 ticks | X2 |
| S13 | Window of Opportunity | (S10/S11) + S01 | Patient Shot on a recovering target with damage-pool edge; refund 1 Focus | X4 |
| S14 | Leave No One Behind | S11/S12 | Two-cell route alone or dragging a consenting ally | X3 X1 |
| S15 | Chain of Obstacles | (S10/S12) + S06 | Two linked snares, one trigger | X3 X8 |
| S16 | Expose the Plan | S13/S14 | Reveal up to three foes' public plans; Readied | X2 X5 |
| S17 | Impossible Route | S14/S15 | Six-cell route crossing one gap and one ally | X3 |
| S18 | Collapse the Killing Ground | S13/S15 | Two temporary low barricades | X3 X8 |

### Envoy (d8, new class)

Medic sustains, Tactician prepares, Parley changes explicitly modeled intent.
Every node has a solo mode; Leadership and companion commands are not required.

| ID | Node | Parents | Effect | Needs |
| --- | --- | --- | --- | --- |
| E01 | Field Dressing | Medicine 1 | Bandage heals 1d6 + Medicine; recipient's Health recovers twice as fast for 600 ticks | X1 X8 X10 |
| E02 | Prepare a Response | Deduction 1 | Deduction vs 12 grants Readied to self or ally | X1 |
| E03 | Read Intent | Insight 1 | Reveal intent category and parley flag; target's next check against you has disadvantage | X2 X6 |
| E04 | Efficient Dressing | (E01/E02) + E01 | Field Dressing heals 2 more | X8 |
| E05 | Watch the Opening | E02/E03 | That foe's next attack against the recipient has disadvantage | X2 X1 |
| E06 | Test Their Resolve | E01/E03 | Persuasion vs Spiritual applies Fear | C |
| E07 | Clear the Head | E04/E05 | Remove one Fear, Rattled, Distracted or Confused | X6 X1 |
| E08 | Self Briefing | E05/E06 | Solo: Readied and a disadvantage token for the next check against you; team: Readied to two allies | X1 |
| E09 | Demand Space | E04/E06, Persuasion 1 | Parley-capable foe below half Health withdraws for 200 ticks | X6 X8 |
| E10 | Triage Priority | E07/E08 | Policy: prevent up to 5 from a hit leaving a quarter Health or less | X1 X4 |
| E11 | Set the Pace | E08/E09 | Tempo and a 3-point buffer | X1 X4 |
| E12 | Offer an Exit | E07/E09, Persuasion 2 | Foe below a quarter Health withdraws for 500 ticks | X6 X8 |
| E13 | Recovery Without Panic | (E10/E11) + E01 | Field Dressing and one cleanse in one action | X6 X8 X1 |
| E14 | Coordinated Pressure | E11/E12 | Readied against one foe for self, or for two allies | X1 |
| E15 | Read the Retreat | E10/E12 | Reveal nearest exit, move 2 cells toward it | X2 X3 |
| E16 | Keep Standing | E13/E14 | Solo 8-point buffer, or 4-point buffers for three allies | X1 |
| E17 | One Shared Opening | E14/E15 | Readied or Tempo for each recipient; solo adds Braced | X1 X4 |
| E18 | Terms Worth Taking | E13/E15, Persuasion 3 | Nonaggression and withdrawal for 600 ticks | X6 X8 |

### Shared trees

Optional specializations gated by skill rank 1, 2 and 3 at HD 1, 11 and 21.

| Route | HD 1 | HD 11 | HD 21 |
| --- | --- | --- | --- |
| Careful Search (Perception) | U01 Careful Search: reveal a hidden mechanism in a cell within 2 | U05 Diagnostic Sweep: search three cells at once | U09 Cautious Advance: search, then move only if clear |
| Field Assembly (Crafting) | U02 Field Assembly: workbench recipe for bandage or snare kit | U06 Salvage Carefully: fixed finite salvage | U10 Reliable Assembly: cheaper recipes |
| Wayfarer (Survival) | U03 Steady Recovery: Health recovery period −25% while no hostile is sensed | U07 Hardy Recovery: quiet reset after damage is halved | U11 Expedition Cache: anchored supply cache |
| Recall a Pattern (Lore) | U04 Recall a Pattern: learn one cued property | U08 Test a Sample: consume to learn a property | U12 Evidence Ledger: two cues reveal a property |

Every knowledge attempt is locked per subject, permanent rank and evidence set.
Crafting needs a reachable disclosed workbench; recipes never profit-cycle.

### Culminations at HD 30

Optional; require two owned row-6 class nodes; at most one owned.

| ID | Node | Effect |
| --- | --- | --- |
| P01 | Deliberate Readiness | Gain Readied and Tempo for one later action |
| P02 | Practice Across Traditions | Own one foreign class node of HD 16 or lower, without its graph and class gates |
| P03 | Make a Safe Passage | Marked route: reduced terrain cost and no forced displacement while on it |
| P04 | Vanishing Exit | Telegraphed escape route that leaves a smoke cell behind |

### Catalog changes from the proposal

| Proposal | Change | Reason |
| --- | --- | --- |
| Hesitant, Unsettled | Merged into Fear | Same effect and descriptor |
| Distracted (−1 Cognitive) | Disadvantage token | Edge first |
| Braced (+2 Physical) | Disadvantage on Physical checks against the holder | Edge first |
| W02 Measured Cut (+1 check) | Ignores your negative edge | Talents add no flat bonus |
| W08 Answer the Miss (1d6 rider) | Readied and Tempo | Edge first |
| S01 Patient Shot (+1 check) | Advantage for longer phases | Edge first |
| S13 Window of Opportunity | Damage-pool edge | Distinct from S01 |
| U03 Trail Markers | Replaced by Steady Recovery | Duplicated S02 |
| U07 Secure a Crossing | Replaced by Hardy Recovery | Duplicated S11 |
| P04 Prepared Escape | Renamed Vanishing Exit | Name clashed with A04 |
| Satiety numbers, no regeneration | Dropped | Rogue plan owns nutrition; regeneration exists |

### Legacy talents

The current 27 talents remain during the foundation checkpoints and retire with
the phase-6 playable Warrior and Mage slice:

| Current | Replacement |
| --- | --- |
| Hardiness → Indomitable (Health) | HD Health only; regeneration talents |
| Endurance chain, Resolve, Arcane Reserve (pools) | None; pools derive from attributes |
| Guard chain (Physical defense) | Braced (W01) |
| Heavy Blows chain (damage dice) | Weapons, forms and conditional riders |
| Impact, Keen and Energy Ward | Shell (M03) for PCs; species baselines for creatures |
| Power Strike | Rattling Blow (W03) |
| Magic Bolt chain | Spark Lance (M01) and form tiers |
| Fear, Fear Mastery | Falter (M02), Test Their Resolve (E06), Dread Touch (D01) |

The arena encounter matrix's Power Strike, Fear and resource-sustain families are
regenerated against the replacements.

## Creature construction

A creature combines layers. Eligibility unlocks a menu; it grants nothing.

| Layer | Provides | Points |
| --- | --- | --- |
| Species | Anatomy, senses, natural strike sequence, attributes, behavior flags, permanent catalog flags | Free, counted in threat |
| Type | Health die, baseline immunities and flags | Free, counted in threat |
| Subtypes | Descriptors, elemental affinity, incorporeality | Free, counted in threat |
| Racial or class HD | Advancement points | One per HD |
| Template | Encounter role allowance (elite, boss) or a depth adjustment | No points |

A creature has two starting ranks like a PC. Remaining points may stay unspent.
AI uses the same admission, cost, supply, route and cooldown checks as a PC and
knows only what it perceives.

**Behavior flags** describe AI disposition, not abilities: `mean` (wakes and
pursues), `greedy` (pursues gold it knows of), `erratic` (randomized movement
probability), `stationary`, `flying` (anatomy), `invisible` (concealment).

**Complexity ceiling.** A minion has a basic attack and one signature; a standard
creature three; elite and boss creatures four, with at most two policies and one
armed. Encounter allowance numbers wait for the CR and XP policy in
[requirements](requirements.md#creature-progression-and-combat-extensions).

**Depth scaling.** Rogue's deep-level adjustments become additional racial HD
(with authored choices) and a hasted template, not special formulas.

### Creature trees from the proposal

Retained with edge-first wording. Each node costs one point plus its encounter
allowance. Final nodes need HD 26 unless a Rogue species needs an earlier gate,
in which case the tree is rebased when that species is built.

| Tree | Eligibility | Nodes |
| --- | --- | --- |
| Pack Hunter | `pack_hunter` flag | K01 Circle Prey: advantage on a melee check when a pack ally is adjacent to the target. K02 Cut Off Retreat. K03 Coordinated Lunge: advantage after an ally's attack on the same target. K04 Alpha Pressure: Readied for self and two allies |
| Heavy Body | giant or dragon type, or `heavy_body` | B01 Shoulder Check (push). B02 Brace the Bulk (Braced). B03 Sweeping Mass (two-target sequence). B04 Ruinous Charge |
| Elemental Furnace | elemental or dragon type with affinity | F01 Ember Projection. F02 Heat Warning (zone). F03 Furnace Vent (burst). F04 Cooling Cycle |
| Restless Dead | undead type | D01 Dread Touch (Fear). D02 Anchor Reliance. D03 Hollow Passage. D04 Break the Anchor |
| Construct Sentinel | construct type, `guardian` flag | C01 Watch Sector. C02 Locking Pulse (Rattled). C03 Shared Shield (policy). C04 Reconfigure |
| Swarm and Growth | swarm subtype, or `distributed_body` | G01 Enveloping Reach: checks against both overlapping actors have advantage. G02 Diffuse Shape. G03 Spread the Pressure (two-target sequence). G04 Divide to Escape |
| Ambush | `ambusher` flag | A01 Camouflage Stillness (advantage on Stealth). A02 Tell Before the Leap. A03 False Retreat. A04 Prepared Escape |
| Elite and Boss Template | elite or boss template | T01 Visible Resolve. T02 Declared Threat. T03 Change the Fight. T04 Final Commitment |

### Rogue-driven creature trees

Added so the Rogue roster can be constructed. Codes use two letters to avoid
clashing with the proposal's trees. HD is the minimum total HD.

#### Natural Weapons tree

Eligibility: a species-authored natural strike sequence with more than one strike.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| NA1 | Second Strike | 2 | — | Unlock the sequence's second authored strike | X9 |
| NA2 | Third Strike | 4 | NA1 | Unlock the third authored strike | X9 |
| NA3 | Rending Pair | 8 | NA1 | Two hits on one target in one sequence add a 1d6 rider once | X9 |
| NA4 | Pressing Assault | 12 | NA2 | A miss gives the next strike in the same sequence advantage | X9 |

#### Regeneration tree

Eligibility: `regenerative` flag.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| RG1 | Regeneration | 1 | — | Health recovery period halved | X10 |
| RG2 | Unquiet Mending | 6 | RG1 | Recovery continues at base rate while fighting | X10 |
| RG3 | Knit Flesh | 11 | RG2 | 150/100, S2: restore 1d6 + a quarter of HD; cooldown 1200 | X10 |

#### Corrosive tree

Eligibility: `corrosive` flag.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| CO1 | Corrode | 1 | — | Rider: worn metal armor loses 1 protection; rustproof and non-metal armor are exempt | X9 X12 |
| CO2 | Corrosive Body | 6 | CO1 | A metal weapon that hits this creature tests for rust | X12 |
| CO3 | Seeping Grip | 11 | CO1 | Corrode also affects a held shield or wielded metal weapon | X9 X12 |
| CO4 | Dissolution | 16 | CO2 + CO3 | Telegraphed touch strips 2 protection; long recovery | X9 X12 |

#### Venom tree

Eligibility: `venomous` flag.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| VN1 | Venomous Bite | 1 | — | Rider check vs Physical: Strength −1, restorable; `poison` descriptor | X9 X14 |
| VN2 | Lingering Venom | 6 | VN1 | Rider applies Poisoned: 1 damage per 400 ticks for 1600 ticks | X9 X6 |
| VN3 | Paralytic Venom | 11 | VN1 | Rider applies Held for 200 ticks | X9 X6 |

#### Grasp tree

Eligibility: `grasping` flag (plants, constrictors).

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| GR1 | Clutch | 1 | — | Rider applies Held while adjacent; escape is an Athletics action vs the holder | X9 X6 |
| GR2 | Tightening Hold | 6 | GR1 | Each later strike on a held target gains +1 damage, cumulative while held | X9 |
| GR3 | Drag Down | 16 | GR2 | Held target cannot use movement abilities | X6 |

#### Chill tree

Eligibility: `cold` subtype.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| CH1 | Freezing Touch | 1 | — | Rider check vs Physical: Frozen for 200 ticks, then a 400-tick thaw immunity | X9 X6 |
| CH2 | Numbing Cold | 6 | CH1 | Rider applies Distracted | X9 |
| CH3 | Rime Ground | 11 | CH1 | Zone that adds 100 ticks to the first crossing | X3 |

#### Drain tree

Eligibility: undead type.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| DR1 | Drain Vitality | 1 | — | Rider check vs Spiritual: maximum Health −1d3, restorable | X9 X14 |
| DR2 | Energy Drain | 1 | — | Rider check vs Spiritual: drain the target's latest HD and its Health | X9 X14 |
| DR3 | Feed | 6 | DR1/DR2 | A successful drain restores the drainer's Health by the amount drained | X9 X14 |

#### Trickster tree

Eligibility: fey type.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| TR1 | Filch Gold | 1 | — | Rider check vs Cognitive: take carried gold | X9 X12 |
| TR2 | Filch Charm | 1 | — | Rider check vs Cognitive: take one unequipped magic item | X9 X12 |
| TR3 | Vanish | 2 | TR1/TR2 | After a successful theft, relocate to a random legal cell on the floor | X11 |

#### Breath tree

Eligibility: dragon type with an elemental subtype.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| BR1 | Breath Weapon | 6 | — | Telegraphed ray of subtype energy, 6d6; target's Physical check halves; cooldown 2000 | X13 |
| BR2 | Lingering Breath | 12 | BR1 | Ray leaves a short-lived hazard zone | X13 X3 |

#### Gaze tree

Eligibility: `gaze` flag.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| GZ1 | Confusing Gaze | 1 | — | 100/100, sight 4: Presence-based check vs Spiritual applies Confused for 300 ticks; targets that cannot see are immune | X6 X12 |
| GZ2 | Transfixing Gaze | 11 | GZ1 | Gaze applies Held for 200 ticks instead | X6 |

#### Mimic tree

Eligibility: shapechanger subtype.

| ID | Node | HD | Parents | Effect | Needs |
| --- | --- | ---: | --- | --- | --- |
| MI1 | Object Guise | 1 | — | While stationary, presents as an authored object; revealed on touch, search or attack | X12 |
| MI2 | Ambush From Guise | 6 | MI1 | First attack from guise has advantage | X12 |

## Rogue roster as constructed creatures

Racial HD equals the Rogue level. Behavior flags and baseline traits are free;
nodes are bought on the listed HD so drain removes them in a deliberate order.
Remaining points buy skill ranks, chosen during arena tuning. Reference values
are in the [Rogue catalog](../research/rogue-5.4.4/content.md#monsters-26).

| Rogue | Type, subtypes | HD | Flags and baseline | Purchases (HD bought) |
| --- | --- | ---: | --- | --- |
| A aquator | aberration, aquatic | 5 | mean, corrosive; strike does no damage | CO1 (1) |
| B bat | animal | 1 | flying, erratic | — |
| C centaur | monstrous humanoid; Warrior HD | 4 | three-strike sequence | NA1 (2), NA2 (4) |
| D dragon | dragon, fire | 10 | mean, fire immunity, three-strike sequence | NA1 (2), NA2 (4), BR1 (6) |
| E emu | animal | 1 | mean | — |
| F venus flytrap | plant | 8 | mean, stationary, grasping | GR1 (1), GR2 (6) |
| G griffin | magical beast | 13 | mean, flying, regenerative | RG1 (1), NA1 (2) |
| H hobgoblin | humanoid, goblinoid; Warrior HD | 1 | mean | W03 (1) |
| I ice monster | elemental, cold | 1 | cold immunity; strike does no damage | CH1 (1) |
| J jabberwock | aberration | 15 | heavy two-strike sequence | NA1 (2), NA3 (8) |
| K kestrel | animal | 1 | mean, flying | — |
| L leprechaun | fey | 3 | greedy | TR1 (1), TR3 (2) |
| M medusa | monstrous humanoid, reptilian | 8 | mean, gaze, three-strike sequence | GZ1 (1), NA1 (2), NA2 (4) |
| N nymph | fey | 3 | — | TR2 (1), TR3 (2) |
| O orc | humanoid; Warrior HD | 1 | greedy | W02 (1) |
| P phantom | undead, incorporeal | 8 | invisible, erratic | — |
| Q quagga | animal | 3 | mean, two-strike sequence | NA1 (2) |
| R rattlesnake | animal, reptilian | 2 | mean, venomous | VN1 (1) |
| S snake | animal, reptilian | 1 | mean | — |
| T troll | giant | 6 | mean, regenerative, three-strike sequence | RG1 (1), NA1 (2), NA2 (4) |
| U black unicorn | magical beast | 7 | mean, natural armor, three-strike sequence | NA1 (2), NA2 (4) |
| V vampire | undead | 8 | mean, regenerative | DR1 (1), RG1 (2) |
| W wraith | undead | 5 | — | DR2 (1) |
| X xeroc | aberration, shapechanger | 7 | — | MI1 (1), MI2 (6) |
| Y yeti | giant, cold | 4 | cold immunity, two-strike sequence | NA1 (2) |
| Z zombie | undead | 2 | mean, mindless | — |

These assignments are first drafts for the arena to confirm. Rogue's damage
dice become each species' authored strike components. Rogue armor class maps
to Physical defense through species natural armor.

### Arena behavior targets

Each species is tuned toward targets derived from the Rogue reference at its
typical depth, against a reference PC for that depth:

- the species' hit chance against the reference PC, and the PC's against it;
- expected damage per reference action cycle;
- reference actions to kill the PC, and for the PC to kill it;
- signature effect frequency per encounter (rust, freeze, theft, drain);
- pursuit and escape behavior (wakes, chases, flees, holds position).

The targets are ratios of actions, so they survive the conversion from Rogue
turns to TOR ticks. Tuning changes species attributes, strike components and
skill-rank purchases; it does not change the shared engine or trees for one
species.

## Capabilities

| Code | Capability |
| --- | --- |
| C | Existing damage, healing, restoration and condition effects on one target |
| X0 | Advancement ledger, data-defined point pools, talent and subtype requirements |
| X1 | Self-or-ally selectors and one-use tokens |
| X2 | Observation records and plan interaction (delays) |
| X3 | Scheduled routes, fields and forced displacement |
| X4 | Committed reaction policies and recovery debt |
| X5 | Bounded multi-target resolution |
| X6 | Condition lifecycle: families, removal, suppression, action denial, AI intents |
| X7 | Protection families and bounded bypass |
| X8 | Typed item and world records: supply costs, projectiles, form registry, recipes, knowledge |
| X9 | Strike sequences and on-hit riders with rider checks |
| X10 | Health recovery pool and regeneration modifiers |
| X11 | Creature behavior flags and self-relocation |
| X12 | Item-state effects and concealment: rust, theft, disguise, sight-dependent effects |
| X13 | Rays and projectiles with obstruction |
| X14 | Attribute damage, maximum-Health drain, level drain and restoration |

## Open decisions

- Final point economy: confirm one shared point per HD after arena measurement.
- Whether Rogue's freeze-lock lethality is preserved or capped by thaw immunity.
- Health recovery base period and its HD scaling.
- Reference PC per depth for arena targets.
- Whether a token applies to a whole strike sequence for specific abilities.
- Encounter allowance numbers, after CR and XP policy.

## Influences

The proposal drew patterns, not rules, from public Cosmere RPG starter material,
Plotweaver open-alpha design commentary and NetHack 5.0's skill system, and the
[pinned Rogue research](../research/rogue-5.4.4/README.md). No external catalog,
text or tree image is reproduced. Draft playtest packets that carry
confidentiality footers are not linked or quoted here.
