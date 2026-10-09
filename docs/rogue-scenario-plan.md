# Rogue scenario implementation plan

Accepted planning direction from the 2026-10-08 interview. This document plans
implementation; it does not describe a playable package or supported new TOML
fields. [The roadmap](milestones.md) owns status and sequencing; the
[TOR design plan](game-design-plan.md) owns shared feature requirements.

## Target and boundaries

Reproduce Rogue's floor-generation patterns, 26 species, item catalog and
distinctive capabilities using standard TOR mechanics. Use the
[Rogue 5.4.4 reference](game-specifications/rogue-5.4.4/README.md) for layout,
content and behavior intent, not an alternate combat/timing ruleset. TOR's
actions, physics, combat, progression, capacity, perception and AI remain shared.
Tune creature builds to approximate Rogue's difficulty curve within TOR.

The [earlier implementation inventory](game-specifications/rogue-5.4.4/implementation.md)
is a dated gap/reference analysis. Its recommended sequence and fidelity cases
are superseded by this accepted adaptation plan wherever they differ. Exact
Rogue combat math, action cycles, RNG parity, capacity, regenerated floors and
whole-room revelation are not goals of this scenario.

## Scenario decisions

| Area | Accepted behavior |
| --- | --- |
| Authoring | Declarative recipes built from fine-grained procedural stages; consider generation scripting separately |
| Floor structure | Nine regions in a 3x3 arrangement, initially filled with stone, connected across broad portal boundaries; carve rooms/corridors into them |
| Layout | Reproduce Rogue's nine room slots, connecting corridors, omitted rooms, depth-dependent darkness/secrets and occasional mazes as support arrives |
| Generation trigger | Generate the complete floor group when it enters the region-loading horizon, before stair traversal |
| Randomness | Independently derived stage streams, pinned inputs/versions and deterministic generated IDs/state |
| Persistence | Preserve terrain, entities and changes on revisits; no floor regeneration |
| Stairs | Use persistent named pairs with authored or independently generated endpoints, arriving at the corresponding stair; backtracking is generally allowed, with scenario-specific gates designed separately |
| Secrets | Required routes may include hidden doors/passages; timed search and ordinary perception can discover them |
| Doors | Ordinary entrances have no usable doors, matching Rogue; secret entrances require discovery |
| Geometry | Generally reproduce the flat layout through TOR geometry; use 3D where appropriate, including physical trapdoor falls through portals |
| Goal | Amulet available at depth 26 or deeper; carrying it out of the dungeon wins |
| Return journey | Preserve existing loot; no ascent-specific suppression of new loot |
| Population | Rogue-inspired depth tables and wandering spawns, restricted to the Rogue roster; spawn on active floors, including revisits; freeze inactive regions |
| Creature content | All 26 species are standard TOR mobs with distinctive capabilities and behaviors |
| AI | Standard TOR AI with species rules; pursuit uses perception and memory, including known gold/items |
| Items | Full Rogue catalog implemented as standard TOR content; this scenario's generation restricts content to that catalog |
| Special items | Review distinctive rules, including scare-monster scroll behavior, individually when implementing them |
| Survival | New shared TOR nutrition, starvation and passive healing; scenario configures Rogue-inspired food availability and modifiers |
| Builds | Shared types/subtypes, racial HD, templates and classes; ordinary Rogue species primarily use racial HD and innate abilities |
| Progression | TOR XP/thresholds, CR-derived rewards and six attributes; actual level drain provisionally reverses advancement and kills at zero HD |
| Equipment/knowledge | Anatomy-based slots, TOR capacity, curses/enchantment/rust/charges and NetHack-style character knowledge |
| Ranged/effects | Standard TOR actions and shared effect definitions; configurable bouncing attacks |
| Perception | TOR lighting/visibility and shared conditions; observer-specific hallucinated identities persist with occasional random shifts |
| Objectives/hooks | Anticipate escape event plus inventory query and victory operation; choose runtime scripting separately |

## Existing foundations

Current TOR already supports on-demand authored/generated regions, freezing and
disk detachment, asset palettes, semantic generator seeds and independent geometry,
population and loot streams. The existing `rooms` generator operates per region;
it does not implement composable recipes or atomic nine-region floor groups.
Use [scenario packages](scenario-packages.md#generated-regions) and
[region streaming](region-streaming.md) as the implementation starting point.
Equipment, item-use effects and the broader shared systems below remain planned
on the published baseline; unpublished work in other checkouts is not a release.

## Stage A — Playable exploration-only generated dungeon

This is the first deliverable. It must be playable before mob, item and survival
systems are complete. No placeholder mobs or items. Start with **nine rooms,
connecting corridors and stairs**; omitted rooms, mazes, darkness, secrets,
traps, population and other features are added only when supported.

Shared prerequisites:

1. Define recipe composition, a generation-group identity and structural metadata
   for horizon planning. Implement reusable stone fill, 3x3 partition, bounded
   room carving, connected graph/corridor carving and stair placement stages.
2. Connect region boundaries broadly, including solid portions needed for future
   excavation. Carve coherent corridors across joins and keep ordinary movement,
   perception and physics consistent at those joins. Existing region bounds may
   be usable through this division; validate dimensions/body clearance explicitly.
3. Integrate group generation with region loading: encountering a floor in the
   loading horizon generates all nine regions together. Do not generate the
   entire dungeon eagerly or wait until the PC descends. Persist committed group
   state and pin its recipe/dependencies.
4. Integrate paired stairs with generated floor groups, destination validation,
   persistence and supported-client traversal. Use ordinary movement timing;
   reject blocked arrivals without consuming time. Keep stair transport separate
   from falling; any scenario-specific backtracking gates need their own design.
5. Author and validate the ordinary exploration scenario. The accepted foundation
   uses 26 floors, each with a 3×3 grid of 26×7×2 regions; northwest owns the
   upward/entry anchor and southeast owns the downward anchor.
   This stage has no Amulet victory, hunger or combat requirement.

Acceptance: same seed and pinned recipe produce the same floor; stage streams
are isolated; all nine rooms and stairs are connected; horizon loading generates
the next floor before descent; stair round trips return to matching anchors;
revisits and save/resume retain generated state. Text/adventure, ASCII and
headless clients can explore and traverse without receiving undisclosed floors.
Measure generation/loading latency and persistence cost at representative scale.

The isolated implementation follows [generation recipes and groups](generation-recipes.md).
Group stairs retain named destinations until the destination floor is published;
preparing one floor does not partially generate another. Verification and
performance acceptance remain required before this stage is declared complete.

## Stage B — Complete layout and perception features incrementally

Add configurable omitted slots, optional extra connections, maze carving,
depth-dependent lighting and secrets through shared generation stages. Implement
lit cells/rooms and ordinary-perception discovery plus timed search. Add shared
blindness/invisibility/disguise/hallucination as their effect support becomes
available; do not delay earlier usable layout increments for all conditions.

Acceptance: carved topology remains connected after discovering secrets; secrets
may lie on required routes; generation does not require a visible alternative.
Lighting obeys TOR LOS across region joins. Discovery/memory are actor-specific.
Hallucinated identities remain consistent across clients and resume, shifting at
the declared simulation-time events rather than every action.

## Stage C — Shared builds, progression and effects

Implement the common tracked creature builder and six attributes, types/subtypes,
racial HD, templates, class advancement, CR assessment and TOR XP progression.
Resolve formulas and dependency rules in a focused design before implementation.
Record advancement history for initially high-HD mobs as well as advancing PCs.

Implement reusable effects, defenses/saves, source-granted immunity/resistance,
duration/stacking and persistence. Sources include abilities, items and traps.
Level drain reverses latest HD/class benefits without removing types/templates;
zero HD causes ordinary persistent death. Restoration remains to be specified.

Acceptance: equivalent PC/mob builds receive equivalent benefits; build-derived
CR and XP calculations are reproducible; drain reverses recorded advancement,
including racial HD; innate species features survive drain; subtype defenses and
effect stacking work through normal actions and survive recovery.

## Stage D — Survival, equipment, items and ranged actions

Implement shared nutrition/starvation/healing, capacity and anatomy-based slots;
then curses, enchantment, rust/protection, charges, type/instance knowledge and
the full catalog's effects. Implement throw/launch/zap and configurable bouncing.
PCs and AI use the same ordinary actions; knowledge governs AI decisions.

Add food and item-generation stages to the exploration scenario as each supported
category becomes usable. Reproduce Rogue-inspired placement/availability while
using TOR balance. Review special item rules individually rather than assuming
every historical edge case is accepted. Food safeguards and treasure-room
population can be added once their prerequisites exist.

Acceptance: carrying capacity and anatomy constrain equipment; unknown curses
and details disclose correctly; naming does not identify; observed use teaches
only the allowed facts. Test resource consumption, ranged obstruction/bounces,
nutrition/starvation/healing, AI uses and full state recovery.

## Stage E — Species, traps and dungeon pressure

Add each species to standard TOR content with its build, innate abilities and
species behavior rules. Add it to this scenario's depth/spawn tables when usable.
Complete all 26 species and all eight traps: trapdoor, arrow, sleeping gas, bear
trap, teleport, poison dart, rust and mysterious. A trapdoor uses physical
falling through a region portal; arrival floors must respect loading/generation
requirements and valid geometry.

Implement wandering spawn policy and seeded population stages. Respect active
simulation boundaries and frozen state. Standard AI pursues only perceived or
remembered goals. Do not substitute fixed-damage actors for missing special
capabilities and call the species complete.

Acceptance: every species has a normal-play capability/behavior case; generation
spawns only permitted species/items; depth changes pressure without switching to
Rogue formulas; all trap mechanisms function through TOR actions and physics;
inactive floors neither spawn nor accumulate catch-up spawns.

## Stage F — Objective, hooks and complete run

Decide runtime hook design separately. The intended objective is an escape event
whose handler checks carried Amulet possession and invokes victory. Tutorial
hooks, such as a PC seeing a particular mob, motivate the same general facility.
Define once/repeat behavior, ordering, authority, execution limits and saved hook
state before choosing a scripting implementation.

Configure Amulet generation at depth >=26 and an exit at the surface. Backtracking
is allowed provisionally. No ascent-specific loot suppression is selected; that
is a later configurable policy, not a prerequisite architectural subsystem.

Acceptance: no premature Amulet placement, carried-goal escape wins, absent-goal
escape does not accidentally win, death/victory persist, and ordinary supported
clients can complete the run. Decide the non-winning escape behavior explicitly.
Validate long seeded descent/ascent runs for population, food and difficulty.

## Remaining design work

These are focused implementation decisions, not reasons to postpone Stage A:

- Recipe schema/stage interfaces, group commit/failure behavior, generator version
  storage, concrete boundary geometry and loading/activation coordination.
- Initial exploration depth/size, stair cost/arrival obstruction and named anchors.
- Generation scripting need/language; runtime hooks separately, including mutation,
  permissions, ordering, replay and saved once-only tutorial state.
- Attribute/build formulas, type/subtype/template tables, CR assessment, XP credit,
  advancement choices, drain restoration and dependent abilities.
- Nutrition/healing numbers, anatomy slots, carrying limits, defenses, targeting,
  effect ordering and hallucination shift policy.
- Per-item special cases, exact depth distributions/balance, starting build/gear,
  non-winning exit behavior and presentation/scoring choices.

Each stage must add behavior tests, ordinary package/process acceptance, save and
disclosure checks, current guides and relevant performance evidence under
[development practices](../CONTRIBUTING.md). No runtime implementation or
performance claim is made by this planning update.
