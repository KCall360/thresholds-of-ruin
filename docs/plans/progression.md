# Progression and constructed creatures: implementation plan

This plan orders the work for the [progression design](progression-design.md).
Phase 1 mechanics, the core phase-2 point ledger and permanent requirement
predicates, and the phase-3 active-time Health recovery kernel are implemented.
The point economy and recovery curve remain provisional. Ordinary player
advancement, nutrition gameplay, strike sequences and content waves are unfinished;
the ledger's wizard editing does not close normal-play progression acceptance.
Each phase is a separate milestone: it needs the maintainer's explicit approval before it starts, and phases that change the
ruleset or save format need approval of that format decision too. Every phase
follows the [testing policy](../development/testing.md), updates the current
references it changes, and updates [status](../status.md) when scope changes.

The phases build the architecture first, prove it with a small Warrior and Mage
slice, then construct the Rogue roster in waves that track the
[Rogue plan](rogue.md)'s item, ranged and perception stages.

## Phase overview

| Phase | Delivers | Capabilities | Depends on |
| --- | --- | --- | --- |
| 1 | 3d6 checks, edge rules, condition families, one-use tokens | X1 (tokens), X6 (families) | — |
| 2 | Core advancement ledger and requirement kinds | X0 | 1 |
| 3 | Health recovery and regeneration | X10 | 2 |
| 4 | Strike sequences, hit order and on-hit riders | X9 | 1–2 |
| 5 | Self targeting, cooldowns, item costs and respec | X1, X8 (supply costs) | 2, 4 |
| 6 | Warrior and Mage slice; legacy talents retired | Minimum X2, X3, X7, X8 needed by the slice | 1–5 |
| 7 | Rogue behavior targets and arena tooling | — | 6 |
| 8 | Rogue wave 1: plain, multi-strike and venom species | X14 (attribute damage) | 7 |
| 9 | Rogue wave 2: behavior flags and action denial | X6 (action denial), X11 | 8 |
| 10 | Rogue wave 3: item, perception, drain and ray species | X12, X13, X14 | 9 and Rogue plan stages |
| Later | Remaining classes, reactions, fields, forms and creature trees | X2–X8 beyond the phase-6 minimum | 6 |

Phases 1–3 change rules data and saved state. They ship as reviewable foundation
checkpoints: checks/conditions/requirements first, then the point ledger and
Health recovery. Each changed current format receives its own explicit revision.
There is no migration between these rulesets’ characters; a fresh game is required, as the
[status](../status.md) format policy already states.

## Phase 1: edge rules and condition families

**Scope**

- Replace d20 checks with the accepted [3d6 edge contract](progression-design.md#edge-model):
  cancellation one-for-one, at most three extra check dice keeping three,
  excess edge allocated to existing rolled damage, and no automatic success
  or failure on natural 3 or 18. Keep initial defenses and bonuses explicit;
  retune them only from paired arena evidence as playable builds become available.
- Condition families: a family contributes at most one edge per check, whatever
  its instance count. Existing Fear becomes the first family; add Rattled and
  Braced.
- One-use tokens: Readied, Exposed and Distracted, with reservation at
  preparation start, consumption even on a miss or cancellation, no renewal
  while reserved, and no benefit to the creating action.
- Talent-granted edge clamp of +1 and −1 per check before other sources combine.
- Validation rejects flat check bonuses and edge `Set` overrides on talent and
  condition sources, including implied flags. Capped edge sources author signed
  additive grants; ordinary uncapped source overrides retain typed stat semantics.

**Tests**

- Unit: check bounds, keep-high/low behavior, cancellation, excess-edge
  allocation, natural 3/18 outcomes, family deduplication, token reservation
  and consumption races, clamp order, and loader rejection of flat talent bonuses.
- Scope regression: mixed-sign shared/check-only sources cannot spill into or
  refund damage edge; cover no user check, opposed checks, canonical components
  and the first-check-only boundary when strike sequences arrive.
- Integration: a token reserved by one preparation and raced by another; save,
  replay, freeze and thaw preserve reservations and expiry.
- Actual process: a headless client applies Braced and observes the disclosed
  disadvantage on an enemy attack; ordinary one-use consumables are reserved,
  saved, resumed and consumed without reuse or private reservation disclosure.
  Permanent requirement predicates load through the real compiler, enforce
  ownership/dormancy and survive source-independent restart.

**Acceptance:** the dice kernel and documentation implement the accepted 3d6
contract, arena measurements record the provisional defense/bonus baseline
without claiming final Rogue balance, and no source contributes twice for one family.

## Phase 2: core advancement ledger

**Scope**

- Ruleset-defined point pools per HD source; the default grants one shared
  point per HD, spent on a talent or a skill rank.
- Two distinct starting ranks recorded once at creation and never reclaimable.
- New requirement kinds: owned talent (for OR parents inside `any`), subtype
  membership, and species catalog flag.
- Level drain removes the latest HD's choice and makes failing nodes dormant.
- Restoration refills a creature's choice from its authored build; a PC
  chooses again with a preview.
- Atomic purchase/subtree replacement and preview in the ledger. The playable
  respec action waits for phase 5's supply reservation and consumption mechanics.
- Build inspection shows the point ledger, dormant nodes and why they are dormant.

**Tests**

- Unit: ledger invariants for every HD count and pool definition; requirement
  evaluation for each new kind; drain, restore and ledger rebuilds preserving injury,
  cooldowns, charges and failed-knowledge locks.
- Integration: rebuilds through save and replay; arena matrix regenerated with
  the new economy.
- Actual process: a text and headless client level up, spend a point, see the
  preview, and observe a dormant node after scripted drain.

**Acceptance:** point count, starting-rank uniqueness, rank cap and permanent
eligibility stay invariant across advance, drain, restore, ledger rebuilds and reload.

## Phase 3: Health recovery and regeneration

**Scope**

- Health as a recovering pool with a derived recovery-period stat that shortens
  with HD; modifiers from talents, species and items.
- Quiet reset when the actor deals or takes damage.
- Active-time accounting across freeze, thaw and replay; no recovery from
  crossing region seams.
- A hook for nutrition to slow or stop recovery, filled by the Rogue plan.

**Tests**

- Unit: period derivation, modifier fold, quiet reset, saturation at maximum.
- Integration: identical recovery across pause, reconnect, freeze and replay.
- Actual process: a client rests after combat and sees Health return at the
  disclosed rate; combat interrupts it.
- Arena: rest-to-full time per HD band recorded as a baseline.

**Acceptance:** recovery is deterministic and never accrues in frozen regions.

## Phase 4: strike sequences and hit order

**Scope**

- An attack ability holds an ordered strike list: check, damage components,
  riders and offset per strike. Existing attacks become one-strike sequences.
- Preparation freezes all strikes; each resolves at its tick with live life
  and reach checks; recovery follows the last strike.
- Dormant strikes unlocked by owned nodes, so drain can remove them.
- The hit resolution order, with the prevention step reserved but empty.
- On-hit riders with optional rider checks against a stated defense.
- Species natural attacks author their full strike sequence.

**Tests**

- Unit: offsets and Speed scaling; target leaving reach mid-sequence; damage
  suspension keeping resolved strikes; rider check ordering; token applying to
  the first strike only.
- Integration: a three-strike sequence through save and replay; inspection
  disclosing each strike's provenance.
- Actual process: an ASCII client steps away during a sequence and sees the
  later strikes miss.
- Performance: scheduler event counts at 16 and 256 regions show sequences add
  per-strike work only, with no small-case fast path.

**Acceptance:** one-strike sequences reproduce current attack outcomes exactly
for identical seeds.

## Phase 5: targeting, cooldowns and item costs

**Scope**

- Self and self-or-ally reach selectors.
- Persisted cooldowns on actor-active time, started at preparation.
- Supply costs: reserve at preparation, consume at the stated stage, release on
  abandonment. Bandages become a universal Bind Wounds action.
- Respec uses phase 2's atomic ledger replacement as a timed, cancellable
  safe-place action consuming a ration. Validate its safety conditions again
  at the appropriate preparation and completion boundaries.

**Tests**

- Unit: competing plans for the last bandage; cooldown survival through
  cancellation and reload.
- Integration: supply reservations through save and replay.
- Respec: cancellation and unsafe-place rejection, one ration consumed only
  on the committed replacement, and preservation of injury, cooldowns,
  charges, item identities and failed-knowledge locks across save and replay.
- Actual process: a client binds wounds with its last bandage while an AI
  competes for the same stack.

## Phase 6: Warrior and Mage slice

**Scope**

- Warrior W01–W06 and W09, Mage M01–M03, culmination rules deferred.
- Regeneration talents U03 and U07.
- Implement the minimum reusable mechanisms required by those nodes here:
  disclosed public-plan observation (X2, W05); one-cell movement and forced
  displacement with ordinary geometry checks (X3, W04/W06); temporary
  category-specific protection (X7, M03); and the fixed starter spell-form
  records and selection needed by M01 (X8). These are prerequisites of the
  slice, not features postponed until after it.
- Retire the current 27 talents as mapped in the design's legacy table; update
  the arena package, matrix families and scenario builds that name them.
- Update [creature rules](../reference/creature-rules.md) and the
  [arena guide](../development/arena.md).

**Tests**

- Unit and integration for each node's effect and its minimum new mechanisms,
  including observation disclosure, blocked displacement, protection expiry
  and deterministic starter-form selection.
- Actual process: play a Warrior and a Mage to HD 6 through real processes.
- Arena: rank-heavy, talent-heavy and mixed builds at HD 1, 6 and 11; record
  choice concentration and routine hit chance (target band roughly 45–75%).

**Acceptance:** each class has two useful options and an escape at every
tested band, and neither class dominates the paired matrix.

## Phase 7: Rogue behavior targets and arena tooling

**Scope**

- Choose and record a reference PC per depth band: level, attributes, gear.
- A script that derives each species' targets from the
  [Rogue catalog](../research/rogue-5.4.4/content.md#monsters-26) and the
  [Rogue combat rules](../research/rogue-5.4.4/rules.md#r05--combat-statistics-and-advancement):
  hit chances both ways, damage per action, actions to kill both ways, and
  signature-effect frequency.
- An arena matrix family that pits each constructed species against its
  reference PC and reports the same ratios.
- A comparison report: target, measured value and tolerance per species.

**Tests**

- Unit: target derivation against hand-computed Rogue examples.
- Integration: the matrix family generates validated packages.

**Acceptance:** a species is "tuned" when every ratio is inside its recorded
tolerance over the paired seeds.

## Phase 8: Rogue wave 1

Species: emu, snake, hobgoblin, orc, zombie, quagga, rattlesnake.

**Scope**

- Natural Weapons NA1 and NA2; Venom VN1 with Strength damage and restoration.
- Warrior class HD on hobgoblin and orc.
- Species packages in a Rogue roster catalog that the Rogue scenario imports.

**Acceptance:** each species meets its targets; drain removes NA strikes in the
authored order; Strength damage is restorable.

## Phase 9: Rogue wave 2

Species: bat, kestrel, centaur, yeti, troll, black unicorn, venus flytrap,
ice monster.

**Scope**

- Behavior flags: mean, flying, erratic, stationary.
- Regeneration tree RG1; Grasp GR1 and GR2; Chill CH1.
- Action-denying conditions Held and Frozen with escape actions and immunity
  windows; resolve the freeze-lock decision first.

**Acceptance:** targets met; no renewable loss of all meaningful actions for
more than one ordinary cycle unless the freeze decision explicitly allows it.

## Phase 10: Rogue wave 3

Species: aquator, leprechaun, nymph, phantom, xeroc, medusa, vampire, wraith,
griffin, jabberwock, dragon.

Each species waits for the Rogue plan stage that provides its prerequisite:

| Species | Needs | Rogue plan stage |
| --- | --- | --- |
| aquator | per-item rust and protection | Items and ranged actions |
| leprechaun, nymph | theft, self-relocation | Items and ranged actions |
| phantom | invisibility | Layout, perception and discovery |
| xeroc | object disguise | Layout, perception and discovery |
| medusa | confusion, sight-dependent immunity | Layout, perception and discovery |
| vampire, wraith | maximum-Health drain, level drain | Complete shared builds/effects and survival |
| dragon | rays with obstruction | Items and ranged actions |
| griffin, jabberwock | flying, regeneration, Rending Pair | Phase 9 |

**Acceptance:** every species meets its targets, and the Rogue roster acceptance
in the [Rogue plan](rogue.md) can proceed.

## Later work

Scheduled after phase 6 as separate milestones, in an order to be decided:

This extends phase 6's minimum mechanisms rather than supplying prerequisites
that its completed Warrior and Mage slice already needed.

- Scout and Envoy classes; remaining Warrior and Mage rows.
- Reaction policies (X4) filling the prevention step.
- Fields, routes and forced displacement (X3); multi-target areas (X5).
- Spell form registry, mastery profiles and shared trees beyond Wayfarer.
- Parley and AI intents; crafting and recipes.
- Culminations; the proposal's creature trees and elite and boss templates.

## Open decisions

The design's [open decisions](progression-design.md#open-decisions) must be
settled at the start of the phase that first needs them.
