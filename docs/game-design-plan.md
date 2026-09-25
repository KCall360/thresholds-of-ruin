# Game design requirements and architectural considerations

This is the forward-looking project plan consolidated from the September 2026
design discussion. It describes intended behavior, not implemented features.
[The roadmap](milestones.md) owns sequencing and status; [architecture](architecture.md)
owns system boundaries. Settled requirements below guide implementation without
requiring every later feature in the first playable milestone. Open details are
deliberately retained as considerations, not silently selected defaults.

## Scenarios, content, and validation

- Ship ordinary, self-contained scenario packages. Start with authored scenarios
  and fully authored mob placements; support generated and hybrid worlds later.
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

- Extend aperture rotations beyond the current z-axis-only transform restrictions,
  including z-facing joins. Stairs are separate geometry/traversal concepts.
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
- Before implementation, resolve aggregate normalization/mass, deterministic
  integration and diagonal collision ordering, transformed velocity and occupied
  footprints across portals, support, and the interaction of multi-cell bodies
  with activation boundaries. The lack of entity orientation does not remove the
  need to map occupied geometry consistently through a rotated connection.
- Clarify whether the proposed momentum-damage hook applies only to impacts or
  also ordinary acceleration: applying damage to every change of momentum would
  also damage freely falling bodies. This remains open, as do terminal-speed
  values and detailed physical balance.

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
  components. Exact policies, thresholds, criticals, range, numbers, and effect
  ordering belong to the combat milestone. The discussion did not settle how a
  combined multi-type amount chooses a resistance.
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
  the same appearance; exact NetHack-inspired rules remain revisable.
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

Exact file syntax, package layout, wire fields, validation heuristics, physics
constants, damage formulas, animation, advanced AI, region-boundary effects,
equipment slots, identification actions, sound, hunger, ranged combat, multiplayer,
3D renderer selection, and historical-save migration await their milestones.
These extension points guide architecture; they are not permission to expand the
first implementation into all future systems.
