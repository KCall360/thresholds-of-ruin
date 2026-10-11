# Creature and combat refactor

Status: **implemented; final verification required before merge**. The approved
format decisions are bundled TOML rulesets, string IDs, generic diagnostic steps,
highest equipped socket weapon selection, and the coordinated current formats in
the [roadmap](milestones.md). Existing saves are rejected.
The [rules reference](creature-implementation.md) describes the implementation.

This guide applies the
[rules architecture principles](rules-architecture.md) (cited as P1–P11) to the
creature, combat and arena code merged in
[PR #99](https://github.com/KCall360/thresholds-of-ruin/pull/99). It lists what
to keep, what to reshape and what to delete, then orders the work into steps.
Each step deletes the path it replaces and rewrites that path's tests.

The work changes the protocol, save format, ruleset and scenario validator
together. The maintainer approved this milestone and these format changes (see
[AGENTS.md](../AGENTS.md#publishing)). The
[creature implementation plan](creature-implementation.md) remains the record of
the accepted behavior. This guide changes how that behavior is built, not the
requirements, except where a step below says otherwise.

## What to keep

These parts are sound and become the kernel of the new design. Reshape their
interfaces as needed, but don't rewrite their logic:

| Area | Files | Why it stays |
| --- | --- | --- |
| Dice and edge | `simulation/src/dice.rs` | Bounded pools, rejection sampling, explicit edge allocation, deterministic stream |
| Damage and protection kernel | `simulation/src/damage.rs` | Canonical components, immunity before reduction, observer-generic resolution |
| Hit-die ledger | `simulation/src/progression.rs` | Per-HD seeds, stable re-addition, independent Health stream |
| Health | `simulation/src/health.rs` | Injury-preserving maximum changes; no revival |
| Cost reservations | `simulation/src/costs.rs` | Split start/resolution payment, resume ownership, cancellation that releases only unpaid amounts |
| Resource pools | `simulation/src/resources.rs` | Saturating recovery with fractional progress (recovery periods move to data) |
| Template stage logic | `apply_templates` in `simulation/src/creatures/derive.rs` | Priority groups, conflict rejection, aggregate-then-floor |
| Validating restore | `CreatureState::from_recorded`, `CreatureBuild::from_recorded` | Decoding goes through the same constructors the runtime uses |
| Arena driver and search tooling | `server/src/arena_evaluation.rs`, `server/src/bin/tor-arena.rs`, `scripts/arena_*.py` | Sound experiment design; only the set of parameters it can change grows (step 8) |

## What to reshape or delete

| Today | Problem | Target | Principles |
| --- | --- | --- | --- |
| `CombatSpec`, `AttackSpec`, `profile()` in `actor_creatures.rs`, `effective_combat()` in `interactions.rs` | Legacy "compatibility view" running alongside the creature model. `attack.damage` is left empty, so the spec fails its own `CombatSpec::valid`. The attack-bonus formula exists three times | Delete. The combatant view is identity + creature + derived cache, with equipment folded in | P2, P3 |
| `CombatSpec::damage_taken`, `EffectSpec::Damage`, physics `apply_damage` | Second damage pipeline that only knows categories, so hazards and items ignore descriptor immunity and can't be "fire" | All damage is a `DamageSpec` resolved through `Protection` | P2 |
| `damage_protection()` in `creature_melee.rs` | Rebuilds `Protection` by converting through `CombatSpec` plus a hard-coded list of four descriptors | `Derived.protection` already includes equipment and conditions | P2, P3 |
| `EquipmentSpec { defense, reductions, attack }` | A third protection representation | Equipment is a modifier source; its attack is a weapon damage expression | P3 |
| `Action::Attack`, `Work::Attack`, `OutcomeKind::AttackStarted`, `CombatEvent::Resolved` alongside `UseAbility`, `AbilityStarted`, `AbilityResolved`, plus `Ability::BasicMelee` behind `unreachable!` | Two models of attacking | The basic attack is an ability; one action, one work variant, one event | P2 |
| `resolve_magic_bolt` (called only from tests) vs. the inline Bolt in `Game::resolve_paid_ability` | Production and tests use different Bolt code | One resolver for every ability | P2, P5 |
| `Grant::{BoltFlat, BoltDice, FearDifficulty, FearDuration, MeleeFlat, MeleeDice}` and the matching `DerivedCreature` fields | One-off fields per ability | `Stat::Ability(id, AbilityStat)` modifiers | P1, P3 |
| Inline constants in `abilities.rs` and `derive.rs`: +3, 100/100, range 6, 1+1 costs, 1d6+1 | Tuning hard-coded and repeated | `AbilityDef` data in the ruleset | P1 |
| `Talent` enum with `predecessor`, `minimum_hd`, `minimum_mage_level`, `grants` match tables | Content in code | `TalentDef` data: prerequisites, requirements and modifiers | P1, P8 |
| `apply_grants` ending in `_ => {}` | New grant variants silently ignored | Exhaustive fold over `Op` | P6 |
| Mindless adding immunities inside `add_source` | Hidden implied rule | Data on the Mindless flag definition | P1, P3 |
| `FearState`, `fear.rs`, `CreatureState::apply_fear`, `fear_immune()`, per-call `Edge::from_counts(0, disadvantages_against(..))` | One condition hard-wired through every layer. `TooManySources` reaches `.expect("validated fear effect")`, a reachable panic | Generic conditions; Fear is a `ConditionDef` with a source-scoped disadvantage modifier and defined eviction | P1, P7 |
| `CombatWorld { run_mode, arena: Option<ArenaRun>, objective, outcome, ... }` | Paired fields that must be cross-checked; run-mode branches inside `finish_death` | `RunMode::{Adventure(..), Arena(..)}` with policy hooks; split `Rng` and `EventLog` out | P6 |
| `work_duration`, `work_timing`, `ability_plan`, `ability_target_available` recomputed per decision, admission and execution | Repeated validation; sight scene recomputed per check | `Plan` stored in `Preparation`; one perception snapshot per AI decision | P4 |
| `(bool, u32)` returns; disclosure outcome rebuilt from `applied`/`damage` (`combat.rs`) | Untyped results | `Outcome` stored in the event | P6 |
| `GameError::InvalidLocation` used for most rule failures | No diagnosis | `RuleError`, `BuildError` and the others, wrapped by `GameError` | P7 |
| `creatures/record.rs` tuples `(u64, u8, u32, u32)` and `[u32; 3]`; `record/ids.rs` hand-numbered `u8` tables | Opaque, order-coupled save data | Named structs; string IDs; delete `ids.rs` | P6, P8 |
| `SpeciesRecord`/`TemplateRecord` embedded in every actor | Definitions copied into every actor | Catalog stored once per save; actors hold IDs | P9 |
| Mirrored enums: `creature_authoring.rs` `catalog!`, protocol `Ability`/`Technique`/`Talent`/`Skill`/`CreatureType`/`Inspection*`, `adapt.rs` ability mappings, client lexicon verbs | Each content addition touches about 15 files in 6 crates | Protocol carries IDs plus disclosed names; clients build verbs and menus from the disclosed list | P8 |
| `[i16; 6]` adjustments, `attribute as usize` indexing, authoring `Attributes`/`Adjustments`/`Overrides` structs | Positional coupling | `ByAttribute<T>` used everywhere, with serde support | P6 |
| Clone-then-derive repeated in `CreatureBuild::{train, increase_attribute, select_talent, set_templates, add_hit_die}`; `select_talent` derives twice | Repetition | One `fn edit(&mut self, f) -> Result<Derived, BuildError>` | — |
| Timer settlement walks every timed actor (`creature_time.rs`) | O(timed actors) per injury in large arenas | Lazy per-actor settlement plus a deadline index | P5 |
| `resolve`/`resolve_with_diagnostics` pairs; `CombatWorld::resolve_creature_attack`/`resolve_creature_fear` choosing by `Option<observer>` | Doubled API | A single entry point taking `&mut impl Observer` | P5 |
| Diagnostics stack: `resolution_diagnostics.rs`, `protocol/src/combat_trace.rs`, `protocol/src/combat_diagnostics.rs`, `server/src/combat_diagnostics_view.rs`, `client-common/src/combat_diagnostics.rs`, `scripts/combat_diagnostics.py` | Large for a wizard tool, with typed mirrors of every step | Keep the capability; the trace becomes observer steps keyed by IDs plus the `Outcome`, with one generic projection | P5, P8 |
| `docs/creature-implementation.md` | 8,000-word implementation diary | Rules reference (balance definitions, catalog adaptation, limits) plus architecture notes; the history moves to PR descriptions | P11 |

## Steps

Each step ends with every test passing on the new model and the replaced code
deleted. The steps build on each other; don't start a later step on top of a
path an earlier step was meant to remove.

### 1. Ruleset data and content IDs

- Add a `Ruleset`, loaded from a data file named by the scenario's `ruleset`
  field. It holds abilities, talents, conditions, creature types, subtypes,
  classes, resources and `Constants`. Gather the bounds into one `limits`
  module.
- Content uses interned string IDs. Delete the `Talent` and `Ability` enums,
  the `catalog!` mirrors in `creature_authoring.rs`, `record/ids.rs` and the
  ability mappings in `server/src/adapt.rs`.
- Save records use named structs and string IDs, and store the catalog once
  per save.
- **Tests:** ruleset loading and validation (unknown references, cycles in
  talent prerequisites, bounds) replace the catalog round-trip tests and the
  `adapt.rs` mapping tests. Add one `test_ruleset()` fixture and creature
  builders; convert the hand-built `Species`/`DamageSpec` literals in
  `simulation/src` and `simulation/tests/it`.
- **Done when:** adding a talent built from existing modifiers is a data-only
  change, verified by a test that loads such a talent from a fixture.

### 2. One modifier fold

- Introduce `Stat`, `Op`, `Scope`, `Modifier` and `SourceId` (see the
  [target architecture](rules-architecture.md#modifiers-and-derivation)).
  Talents, species, types, subtypes, classes, templates and **equipment** all
  emit modifiers.
- `derive` becomes the single fold. The cache is recomputed on rebuild, equip,
  unequip and condition changes.
- Delete `CombatSpec`, `AttackSpec`, `profile()`, `effective_combat()`,
  `damage_protection()`, `EquipmentSpec.defense/reductions`, and the
  ability-specific `Grant` variants and `DerivedCreature` fields.
- **Tests:** the derivation table tests rewrite to the fold, adding equipment
  and condition sources. Delete
  `raw_combat_profiles_are_rejected_by_the_current_checkpoint_format` along
  with the type it guards; strict-decoding tests for the new record replace it.
- **Done when:** `grep CombatSpec` finds nothing, and inspection lists every
  contributing source for defense, attack and protection.

### 3. One effect vocabulary and one damage path

- Add `AbilityDef`, `CheckDef`, `Against` and `Effect`. Power Strike, Magic
  Bolt and Fear become data. The basic attack becomes an ability whose damage
  is the selected weapon.
- Consumables and hazards use `Effect`, and physics collisions produce a
  `DamageSpec`. Delete `EffectSpec`, `CombatSpec::damage_taken`,
  `Action::Attack`, `Work::Attack`, `AttackStarted`, `CombatEvent::Resolved`
  and the separate disclosure branches.
- The weapon-selection rule (today "last equipped weapon by slot order",
  written twice) becomes one explicit function, or a rule in slot data.
- **Tests:** add a regression showing that a fire hazard does no damage to a
  creature with fire immunity. Rewrite `simulation/tests/it/abilities.rs`,
  `paid_abilities.rs`, `combat.rs` and `attack_definition.rs` against the
  single resolver; delete tests of removed entry points.
- **Done when:** exactly one function resolves every ability, item effect and
  hazard, and tests call it.

### 4. Plan, resolve, apply

- `Plan` captures cost, timing, check operands and effects at the start of
  preparation. `Preparation { plan, hold }` replaces the stored `duration`,
  `recovery` and `charge` fields and the recomputing helpers.
- `resolve(&Plan, &Snapshot, &mut Rng, &mut impl Observer) -> Outcome` is
  pure. `Game::apply(Outcome)` is the only place that changes injury,
  resources, conditions and death, and the only place that emits events.
- Diagnostics become the observer's trace plus the outcome. Collapse the
  doubled `*_with_diagnostics` APIs.
- AI takes one perception snapshot per decision and evaluates plans against
  it.
- **Tests:** property tests showing that resolution with and without an
  observer yields identical outcomes and random states; lifecycle tests for
  resume reusing the original plan. Rewrite `resolution_diagnostics.rs` tests
  to assert on the trace and outcome.
- **Done when:** no `.expect("validated …")` remains on rule paths, and an AI
  decision computes at most one sight scene.

### 5. Generic conditions and lazy timers

- Add `ConditionDef` with stacking, source scoping, descriptors and an
  eviction rule. Fear becomes data. Delete `fear.rs`, `FearState`, the fear
  fields in records, the protocol and the clients, and every per-call
  `Edge::from_counts(0, …)`.
- Each actor gets an `active_since` value and is settled lazily; a deadline
  index replaces the walk over all timed actors.
- **Tests:** regressions for capacity eviction with more than 128 sources (no
  panic), refresh and longest-duration behavior, immunity clearing, and
  frozen-time equivalence. Convert `simulation/tests/it/fear.rs` into
  condition tests.
- **Done when:** a second condition (for example a test-only "marked") works
  with no new Rust.

### 6. Run policy, errors and state types

- Replace `RunMode` plus `Option<ArenaRun>` with
  `RunMode::{Adventure(..), Arena(..)}` and the `on_death`,
  `on_action_committed` and `deadline` hooks. Move arena pause and step to the
  policy.
- Introduce `RuleError`, `BuildError`, `PersistError` and `ScheduleError`, and
  replace the generic `InvalidLocation` uses on rule paths.
- Replace positional arrays with `ByAttribute<T>` and `ByResource<T>`, and
  tuples with named structs. Add `CreatureBuild::edit`.
- **Tests:** arena tests (`arena_run.rs`, `server/tests/it/arena_evaluation.rs`)
  rewrite against the policy hooks. Error tests assert specific variants.

### 7. Protocol and clients from disclosed lists

- Observations disclose `{ id, name, cost, reach, target_kind }` for the
  viewer's own abilities. Requests carry `UseAbility { id, target }`.
  Inspection and diagnostics carry IDs with display names.
- Delete the protocol content enums (`Technique`, `Talent`, `Skill`,
  `CreatureType`, `CreatureSubtype`, the `Inspection*` mirrors) where a
  disclosed ID and name suffice. Delete the client's per-ability lexicon verbs
  and `client-common/src/abilities.rs` matching.
- **Tests:** client tests drive verbs and menus from a disclosed fixture list.
  Re-record the wire fixture. Rewrite the actual-process tests
  (`scripts/test_creature_*_process.py`) per user flow.

### 8. Mechanics pass

Once steps 1–7 are done, this is mostly data editing, validated in the arena:

- Give Cognitive and Spiritual defenses consumers, so Intellect, Willpower,
  Awareness and Presence matter. For example, spell checks could target
  Cognitive defense, and conditions could be resisted by Insight or
  Discipline.
- Apply Speed scaling consistently, or deliberately not at all, across every
  ability's timing (today only melee scales).
- Rebalance the Mana economy (capacity, recovery period and cost) and the
  talent values.
- Let the arena search change ruleset fields as well as scenario fields
  (`scripts/arena_search_parameters.py`).

## Format and documentation impact

- Protocol, save format, ruleset and validator versions change together in
  the step that first changes them. The current-version-only policy remains;
  no importers are written.
- Regenerate fixtures, scenario certificates and wire recordings with the
  repository tooling. Hand-edited generated files are not accepted.
- Update the [roadmap](milestones.md) versions and the
  [architecture](architecture.md) boundaries when a boundary changes.
- Replace `docs/creature-implementation.md` with a concise rules reference as
  part of step 1.

## Decisions for the maintainer

1. Approve starting this as a milestone, and the compatibility-breaking format
   changes.
2. Ruleset file location and format: a shared `rules/` package referenced by
   ID, or bundled with each scenario.
3. Whether the diagnostics wire report keeps typed per-step DTOs or moves to a
   generic ID-keyed step list (smaller; clients render generically).
4. The weapon-selection rule with more than one weapon-capable slot.
5. Whether the step order may be merged into fewer pull requests. Each step is
   designed to be independently reviewable and to pass CI on its own.
