# Game design requirements and architectural considerations

This is the forward-looking project plan consolidated from the September 2026
design discussion and the Rogue interview of 2026-10-08. It records accepted
requirements; many are now implemented (milestones 4a–4e), and others are still
ahead. [The roadmap](milestones.md)
says which, and owns sequencing and status; [architecture](architecture.md)
owns system boundaries. Settled requirements below guide implementation without
requiring every later feature in the first playable milestone. Open details are
deliberately retained as considerations, not silently selected defaults.

## Scenarios, content, and validation

- Ship ordinary, self-contained scenario packages. Start with authored scenarios
  and fully authored mob placements; support authored/generated hybrid worlds
  through the current room generator.
  General procedural recipes and floor groups are planned below.
  Wizard commands are not the scenario authoring format.
- A world contains zones and regions. World theme pools provide defaults; a
  zone's theme replaces world pools. More elaborate layering is future scope.
- Regions define geometry, gravity defaults and sparse overrides, single-cell
  named anchors, local outgoing portal declarations, and local actor/item
  placements. Structural metadata can be derived with explicit overrides.
  Validate cross-region connections globally. Anchors are authoring references,
  not disclosed place names; named regions are a possible later convenience.
- Authored entities have stable IDs; generated entities have deterministic IDs.
  Archetypes support instance overrides. Placements always have locations, even
  when the containing region and its entities are loaded later. References to
  ungenerated regions can resolve through fixed structural metadata and anchors.
- All actors share one model. Controller assignment distinguishes human and AI
  control. One human controls one character; scenarios can offer multiple
  characters and specify whether the others are omitted or run scenario-selected
  server AI. Multiplayer input policy is not part of this scope.
- Scenarios specify character starting anchors and initial inventory, with room
  for equipment later. Initial victory means one player character occupies a
  named objective cell and, optionally, carries a particular authored item
  instance. Objective disclosure and ending versus continued play are configured
  by the scenario.
- Use a stable scenario ID and author-controlled `major.minor` version. Validation
  binds exact scenario content hashes, ruleset, generator, content dependencies,
  and validator identity. Every authored change requires revalidation; author
  versions alone cannot establish that contents match. Whether every edit must
  also bump the human version is not yet a requirement.
- A separate explicit validation utility emits a reusable artifact. Check
  structure, references, geometry, anchors, objectives, and deterministic
  generation for enumerable regions. Large-world generation checks can be bounded
  or sampled with their coverage recorded; they do not prove every possible game
  winnable. Runtime still validates generated results and fails safely.
- Startup does lightweight integrity/version checks rather than rerunning costly
  whole-world validation. Default policy refuses unvalidated or stale packages;
  runtime options permit development warnings or wizard/test restrictions.
  Diagnostics primarily serve server/launcher users and must explain the fault
  clearly, with enough structure for different client startup presentations.
- Running games pin immutable scenario inputs and exact dependencies. Future
  support should allow multiple installed ruleset/generator versions. Existing
  games keep their selected versions; new generation rules establish a different
  scenario/version combination. Missing exact dependencies fail clearly. Historical
  save migration and declared compatibility remain future work, not an immediate
  requirement to retain every pre-release implementation.

## Region activation and persistence

- Generate on demand in region/zone units, depending on neighbors only through
  already-fixed structural metadata. Activation occurs within the player's
  preload horizon, including areas the player might never visit. The precise
  horizon and reachability optimization remain implementation decisions.
- Once activated, initial layout and contents are persisted for the game's life.
  Gameplay may change them; later spawning is a separate future system. Do not
  replace regions by regenerating their initial contents.
- Distant regions freeze, including all actors and effects. Reactivation performs
  a deterministic batch of deferred updates, perception refresh, and actor
  decisions before ordinary scheduling resumes. This must not become elapsed-time
  catch-up simulation: frozen regions do not advance. Define batch boundaries and
  cross-region interactions when implementing streaming.
- Persist complete authoritative state: RNG, scheduling and action progress,
  bodies and velocities, AI state/memory, effects, inventories, identification,
  corpses, mutations, and activation state. Recovery must preserve subsequent
  gameplay outcomes; serialized bytes need not match.
- Unactivated regions need scenario/seed/version references, not materialized
  worlds. Activated but frozen regions can stay on disk. Restore the active
  preload set first; record that set in saves so loading need not scan or generate
  the whole world. Recompute client palettes from restored state.
- Save at internally consistent committed simulation boundaries. After recovery,
  require fresh player input rather than automatically restarting interrupted
  work. Preserve valid saved action progress. Timed actions and physics require
  an explicit design for intermediate committed events; do not assume every
  long action must finish before any recoverable state can exist.
- Retain the current background-save durability contract until deliberately
  revised: normal acknowledgements can precede persistence, explicit saves are
  durability barriers, and crashes roll back an unsaved suffix consistently.
- Normal play uses persistent permadeath. Manual save/restore follows NetHack's
  suspend/resume model, with arbitrary filenames defaulting to the character
  name and no save slots or reusable normal-play rollback saves. Safe replacement,
  consuming a resumed save, live journals, and crash recovery must be reconciled
  without deleting the only recoverable state. Physical file mechanics are deferred.
- The server is independently startable. Text, ASCII, and future 3D clients have
  appropriate launcher presentations; distribution determines their workflows.
  Restoring in place versus starting another server is an implementation choice.

## Portal geometry, bodies, and gravity

- Aperture rotations cover all 24 proper cube rotations, including z-facing
  joins. Stairs are separate geometry/traversal concepts.
  Preserve independent region coordinates and non-Euclidean connections.
- Each cell resolves gravity from an absolute sparse override or its region's
  default direction and strength. Directions initially use the six local grid
  axes. Discontinuities between adjacent cells and across portals are intentional.
- Bodies occupy discrete footprints/heights across multiple cells; a medium
  character normally occupies two vertically stacked cells. Do not introduce
  entity facing/orientation as a gameplay requirement.
- Aggregate contributions from all occupied cells, weighted by field strength.
  The resultant can be diagonal. Use acceleration and persistent velocity with
  a relatively high terminal speed. Zero resultant gravity preserves drift.
- Translate one cell at a time at simulation times determined by velocity while
  other actors continue on the normal scheduler. Reevaluate gravity as occupancy
  changes. This is simulation time, not wall-clock physics during player input.
- Collision zeroes the obstructed velocity component and preserves tangential
  components. Leave room for momentum transfer. Provide a damage hook for
  momentum changes; collision/fall damage normally uses impact, while hazards
  such as spikes may use keen. Damage quantities are not yet specified.
- The 4c implementation decisions are in [bodies and gravity](physics.md):
  average acceleration, fixed-point tick integration, persistent body frames,
  deterministic collisions, rigid support, and impact-only hooks. Crouching and
  ducking remain future posture changes. Numerical balance can evolve later.

## Asset palettes

- Palettes forecast top-level asset identifiers, tailored to the player's preload
  horizon. Environmental assets are authored; dynamic-entity possibilities derive
  from themes. A goblin/giant-ant theme advertises both even if no instance exists.
  Palette construction must not require spawning or revealing actual entities.
- Use broad zone/theme coverage and early updates so changes do not signal the
  contents of the next room. Extra plausible entries may obscure actual contents;
  this is not a secrecy guarantee. Human-readable asset IDs are sufficient.
- Clients resolve their own models, glyphs, sounds, and other dependencies. The
  wire entries need only identifiers, not renderer-specific dependency metadata.
- Deliver an immediate palette snapshot on attachment/reconnect, then versioned
  incremental updates whenever the effective palette changes. Use the existing
  authenticated connection, a separate message family/revision, and independently
  requestable full palette snapshots. Clients request resynchronization on gaps;
  no revision or readiness acknowledgements are required.
- A listed asset may be needed at any time. This is best effort: polymorph or
  other gameplay can introduce an unlisted asset. Clients use fallbacks and retry
  asynchronously. Retention/eviction is the client's decision; removals end the
  preload expectation rather than forcing deletion or forbidding later use.
- Palette availability conveys no entity-instance knowledge. Ordinary perception
  and unidentified-item disclosure restrictions still apply. Theme palettes can
  reveal broad possibilities without exposing hidden item-to-appearance mappings.

## Actors, combat, and AI

- Players and mobs share bodies, actions, scheduling, gravity, damage, and death.
  Attacks are timed actions resolved against current state. Use d20 plus attack
  bonus versus physical defense; any occupied target cell can be attacked with
  line of sight and valid reach. Simulation permits attacking any valid actor.
- Use hit points and the initial Cosmere-inspired type vocabulary: energy,
  impact, keen, spirit, vital. This selects categories, not the rest of that RPG's
  rules or balance. Support multi-type damage, immunity, and flat reductions that
  can reduce damage to zero.
- Leave resistance resolution flexible for separate, combined, or grouped damage
  components. Further thresholds, criticals, ranges, numbers and effect ordering
  require scoped combat design. Current combat resolves each component
  independently; see
  [dungeon gameplay](dungeon.md). Further resistance rules require explicit design.
- Death is persistent. Inventory drops separately at the base cell; corpses are
  ordinary items with descriptive/history information, using ordinary object
  blocking rules rather than the former living body's occupancy rules.
- Server AI is deterministic and limited to the actor's perception and remembered
  knowledge. Start with search, attack, and flee states, expiring last-known
  locations, and a transition lookup table for conditions such as impossible
  goals. Scenarios select behaviors, including for AI-controlled starting
  characters. Richer state graphs, memory durations, and tactics are deferred.

## Items, knowledge, and interrupted actions

- Implement items before equipment. Initial operations are pickup, drop, and
  inventory inspection, including individual/requested pickups. Multiple items
  or stacks can occupy a cell. Only explicitly stackable items with matching
  archetype and relevant properties merge.
- Keep identity and ownership extensible for later weight/capacity, equipment,
  containers, locks/keys, and item use. None of these is required in the first
  item core; their timing and interaction rules should not require its replacement.
- Separate actual identity from character knowledge from the beginning. Use
  deterministic per-game randomized appearance mappings. Identical descriptions
  normally imply identical effects, with room for confounding descriptions.
  Identification applies to matching effects, not blindly to everything with
  the same appearance; the accepted extensions below select the NetHack
  knowledge pattern.
- Knowledge belongs to the character, persists after transfer/consumption, and
  survives save/replay (rewind restores the knowledge of that earlier state).
  Never send undiscovered true identities or revealing properties to clients.
- Timed item use and equipment changes may be interrupted. All actors share the
  progress model. Threat changes or inability to complete can interrupt; action
  types may define their own policies and meaningful partial effects.
- Reattempting the same still-valid action resumes progress without a special
  notification. Another action discards progress, except waiting, which may
  preserve it over multiple turns. Voluntary movement and gravity displacement
  generally invalidate progress. Damage alone interrupts without discarding it;
  relevant target/world changes can invalidate it. Avoid treating every actor
  state change, such as reduced HP, as invalidation.
- Voluntary cancellation is generally unavailable for these actions. Existing
  travel cancellation remains its own policy. Time already spent remains spent;
  no extra interruption charge is the proposed default. Track architectural
  support now; defer detailed line-of-sight and partial-effect edge cases until
  an action requires them.

## Wizard mode and test strategy

- Wizard mode can start from a scenario or be enabled on a save, and may modify
  structure and gameplay, including regions, portals, terrain, and gravity.
  Enabling it retains the permanent wizard-lineage marker, independently of
  scenario validation. Only edits that break validation mark the running state
  unvalidated; do not automatically validate or rewrite the source package.
- Journal privileged edits and validation-status changes for deterministic replay.
  Continue allowing development runs under the selected runtime policy.
- Tests use ordinary scenario packages without a special test-only format/flag.
  Assertions and action sequences live in tests, referencing stable authored IDs
  and anchors. Validate those references and pin appropriate fixture versions to
  make incompatible changes fail clearly.
- Fast mechanics tests may run in process; process acceptance tests launch real
  server/client binaries. Use scenario packages for reproducible initial setup,
  and wizard commands for wizard behavior or intentional runtime mutations.
- Checkpoints are ordinary saves produced by reproducible setup sequences.
  Optional caching must invalidate on fixture/dependency changes. They remain
  loadable by ordinary clients subject to the usual version and wizard rules.

## Deferred details

The authored TOML syntax and package layout are now defined in
[scenario packages](scenario-packages.md). Future wire fields, generation
validation heuristics, physics
constants, damage formulas, animation, advanced AI, region-boundary effects,
exact anatomy slot tables, identification actions, sound, hunger balance, ranged
combat formulas, multiplayer,
3D renderer selection, and historical-save migration await their milestones.
These extension points guide architecture; they are not permission to expand the
first implementation into all future systems.

## Procedural recipes and connected region groups

Accepted extensions from the 2026-10-08 interview. Existing per-region
generation/streaming and semantic random streams are foundations; generalized
recipes and floor groups are not yet implemented. The
[Rogue scenario plan](rogue-scenario-plan.md) is the first consumer, not a separate
Rogue combat ruleset.

- Author procedural generation through declarative recipes composed of reusable,
  fine-grained stages: partitioning, room selection and carving, maze generation,
  connectivity selection, corridor carving, secrets, lighting, stairs, and
  population. Do not reduce this to a monolithic `rogue` generator selection.
  Reusable recipes may compose these stages; exact schema is still to be designed.
- Consider scripting for generation stages where declarations become cumbersome.
  Introducing a language or runtime is not yet decided and must not delay an
  ordinary declarative floor-generation scenario. Generation extensions must
  preserve seeded determinism, validation, and dependency pinning.
- Support a generation group spanning multiple connected regions. Generate and
  validate the whole group when it enters the region-loading horizon, before the
  PC traverses into it. A floor is one such group. Generation is not deferred
  until the actor uses its stairs. Loading and simulation activation policies
  must distinguish a materialized group from regions currently being simulated.
- Use independently derived random streams for generation stages, with stable
  group/stage identities and pinned versions. Loot-stage changes should not
  unexpectedly change room geometry. Persist generated state and relevant RNG
  state; do not reconstruct visited floors from recipes on return.
- Adopt stone-filled regions with carved rooms/corridors and broad connecting
  portals as a general pattern. Region boundaries need not follow room walls.
  Connections must support movement, perception and physics, with room for future
  mining across initially solid boundaries. Validate matching boundaries without
  treating solid stone as an impassable structural disconnection forever.
- Connectivity policies may require discoverable secret routes. Validate both
  structural connectivity after discovery and any separately requested visible
  connectivity guarantee. Search and ordinary perception can discover secrets.
- Standardize scenario stairs as explicit transport to a destination in another
  region. Existing links already support cross-region traversal; the new floor
  pattern uses paired destinations rather than local elevation changes. Support persistent paired stair anchors and
  destination clearance. Physical falling through a portal remains physics,
  distinct from using a stair. Paired stairs use ordinary movement recovery;
  blocked arrivals are refused without consuming time. Backtracking is generally
  allowed; scenario-specific gates remain a separate extension. Pairs may also
  connect positions in the same region.

## Shared creature builds, progression, and defenses

- PCs and mobs use the same tracked build system, broadly inspired by D&D 3.5's
  types, subtypes, racial Hit Dice, templates, and class levels. This is a TOR
  design direction, not adoption of all external RPG rules.
- The proposed standard attributes are STR, SPD, INT, WIL, AWA, PRE, inspired by
  the Cosmere RPG/Plotweaver system. Attribute ranges, modifiers and advancement
  formulas remain open; TOR retains its own combat and timed-action mechanics.
- Types determine racial HD and HD benefits; subtypes supply additional traits;
  templates can modify a build; class levels add progression. Record advancement
  choices so progression/regression work for every creature, including mobs
  created at higher levels. Species supply base anatomy and innate capabilities.
- Racial HD and class levels contribute to ECL. Determine CR from the creature's
  build; award kill XP as a function of the recipient's level and the mob's CR.
  Use standard TOR rewards and thresholds, not scenario-specific Rogue XP tables.
  CR derivation, template contributions to ECL/CR and XP attribution remain open.
- Provisionally, level drain reverses the latest HD/class advancement, preserving
  types, subtypes and templates. Racial HD may be removed. Zero remaining HD
  causes death. Record enough build history to reverse granted benefits; define
  restoration, dependent abilities and respec policies before implementation.
- Defenses, saves, resistances and immunities can derive from attributes, build
  components, equipment and effects. For example a Fire subtype can grant fire
  immunity. Numerical formulas and combination/precedence rules remain open.

## Shared effects, survival, equipment, and perception

- Define reusable effects usable by monster abilities, items and traps. Sources
  specify targeting, strength and activation. Effects use simulation time and
  declare repeat behavior: extend duration, increase intensity, replace, or apply
  independently. Persist active effects and their deterministic update state.
- Implement nutrition, food, hunger, starvation and passive healing as standard
  TOR systems. Scenarios configure food availability and item modifiers; formulas
  and thresholds belong to TOR balance rather than copying Rogue's turn counters.
- Anatomy determines equipment slots, including support for humanoid armor,
  weapons, shields and two rings. Take inspiration from NetHack and D&D 3.5 for
  anatomy-specific slot tables. Shared equipment supports curses/uncursing,
  enchantment, rust/protection and hidden per-instance details/charges.
- Carrying limits use TOR weight/capacity influenced by strength and build, rather
  than Rogue's item-count limit. PCs and mobs share equipment and consumable
  actions; AI choices respect actor knowledge and capabilities.
- Follow the NetHack knowledge pattern: consistent randomized appearances within
  a run, character-owned type identification, separate per-instance knowledge,
  naming unknown types without identifying them, and learning through use or
  identification. This refines the earlier item-core extension points.
- Shared ranged actions cover throwing, ammunition launch and wand/ray use with
  TOR targeting, range, collision, resources and timing. Bouncing is a configurable
  attack property. Resource recovery and exact collision/effect order remain open.
- Introduce lit cells and rooms using TOR spatial perception, not automatic
  whole-room revelation. Blindness, invisibility, hallucination and disguise are
  shared perception behaviors, disclosed consistently through every client.
- Hallucination tracks persistent mistaken identities per observer, with random
  shifts over time rather than each turn. Persist these beliefs and shift state.
  Identity keys and shift distribution remain open. Never transmit hidden true
  identities through hallucinated views.
- Species behavior extends standard TOR AI, perception, knowledge and memory.
  Item/gold pursuit requires actor knowledge, not access to hidden world contents.
  Wandering populations advance while their floor is active, including revisits;
  inactive regions freeze with no elapsed-time catch-up spawning.

## Scenario event hooks and scripted objectives

- Decide runtime scripting separately from generation scripting. Anticipate
  author-visible hooks into authoritative events, including a PC perceiving a
  particular mob, so a tutorial can present combat guidance.
- Prefer a general event/query/outcome design over adding a built-in objective
  variant for every scenario. A dungeon-escape handler could inspect the PC's
  inventory for the Amulet and trigger victory. Hook-driven victory is the
  intended extension direction; a scripting runtime is not selected yet.
- Open design includes hook ordering, actor scope, repeat/once semantics,
  permitted mutations, deterministic execution limits and save/replay state.
  Tutorial events must follow actual actor perception. These decisions belong
  to the runtime-hook design and are not prerequisites for exploration-only play.
