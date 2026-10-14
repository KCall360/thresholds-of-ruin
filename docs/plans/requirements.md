# Remaining shared requirements

This page records accepted future requirements and open decisions. It is not a
second description of implemented behavior. [Current status](../status.md) owns
scope and sequence; [architecture](../architecture/overview.md) and the references
own current contracts. A requirement does not authorize a new milestone or select
an undecided numerical formula, API or scripting runtime.

## Content, generation and rules distribution

The implemented package model already supports authored/generated regions,
validation certificates, pinned sources, rule imports/overrides, recipes and floor
groups. Preserve those [authoring contracts](../reference/scenario-packages.md)
when extending them; do not replace them with wizard commands or a monolithic
Rogue-specific generator.

Remaining work includes:

- Extend reusable, fine-grained stages for omitted rooms, mazes, secrets, traps,
  population and loot. Keep geometry/population/loot streams independent and pin
  stage identity/version so changing content does not silently reshape a floor.
- Evaluate generation scripting only if declarations become cumbersome. It is
  separate from runtime hooks and must preserve validation and determinism.
- Define discovery-aware connectivity guarantees. A required route may contain
  secrets; do not silently require an always-visible alternative. Searching and
  ordinary perception should share supported discovery semantics.
- Retain broad stone-filled region joins for future excavation. Region boundaries
  need not follow room walls; movement, sight and physics must agree at joins.
- Decide more elaborate theme inheritance only when needed. World defaults and
  zone replacement are current behavior, not a general layering framework.
- Add future exact-version dependency selection and multiple installed
  scenario/ruleset/generator versions deliberately. Missing dependencies must fail
  clearly. Historical-save migration is optional later work, not permission to
  weaken today's strict current-format rejection.
- Author versions are `major.minor`; content hashes, not version names alone,
  establish identity. Requiring a human version bump for every edit remains a
  separate policy decision.

## Creature progression and combat extensions

PCs and mobs share one tracked build/rules kernel. Existing attributes, sources,
advancement, generic effects, timed actions and equipment are described in
[creature rules](../reference/creature-rules.md) and [items](../reference/items.md).
Do not reintroduce parallel scenario-specific combat engines.

The [progression design](progression-design.md) and its
[implementation plan](progression.md) own the class, talent, advancement-ledger,
regeneration and constructed-creature direction. Further design and content must
address:

- Full type/subtype/species/template/class catalogs and balance. External RPGs
  inspire concepts; their equations and defaults are not automatically TOR rules.
- Calibration of CR assessment, template contributions and recipient-relative XP
  rewards; damaging-condition credit beyond the implemented direct-effect and
  direct-push/fall adapters. Distinguish diagnostic arena scoring from ordinary
  progression and preserve the shared award boundary.
- Level-drain restoration, dependent abilities and respec. The accepted direction
  reverses latest recorded HD/class advancement without stripping innate
  types/subtypes/templates; removing all HD causes ordinary death. Confirm the
  exact supported lifecycle operation in the current reference before extending it.
- Additional effect families and their targeting, duration, stacking, immunity,
  save, resistance and ordering rules. Reuse the existing bounded generic effect
  pipeline; framework availability does not define a new effect's gameplay.
- Throwing, ammunition launch and wand/ray actions, including configurable
  bouncing, obstruction, range, cost, resource recovery and deterministic effect
  order. Do not assume a ray/projectile system from a targeted ability alone.
- Collision/fall damage balance and hazards such as spikes. Physics provides
  impact hooks; quantities and momentum-transfer mechanics require scoped design.
- Richer species behaviors, tactics and memory policy. All decisions must obey
  the actor's perception/knowledge, including pursuit of known items or gold.

Prepared effect values remain frozen at preparation; execution revalidates live
conditions without retroactively recalculating those operands. New timed actions
must define their progress/resume/invalidation policies, partial effects and
cancellation behavior explicitly. Damage alone need not erase valid progress;
waiting can preserve it, while another action or displacement can invalidate it.
Time already spent remains spent. Do not infer a universal cancellation policy
from travel's separate cancellation command.

## Survival, inventory and identification

- Extend and calibrate food distribution and survival content on the implemented
  shared nutrition, eating, hunger and starvation foundation, integrated with
  actor-active Health recovery. Scenarios author nutrition/recovery policies and
  food availability; TOR owns active-time execution rather than copying Rogue's
  turn counters.
- Add carrying weight/capacity influenced by strength/build. Preserve stack and
  transfer semantics; do not substitute Rogue's item-count limit.
- Extend anatomy-specific slots and content for armor, weapons, shields and rings.
  Existing data-defined equipment support is the foundation, not future work.
- Design curses/uncursing, enchantment, rust/protection, charges and hidden
  per-instance properties, then implement each usable item category with normal
  PC/AI actions and recovery tests.
- Extend current character-owned type knowledge with per-instance knowledge,
  naming unknown types without identifying them, and learning through use or
  identification. Confounding appearances must not identify unrelated effects.
- Containers, locks, keys and richer item interactions are separate increments.
  Specify their timing, capacity, interruption and actual-client acceptance cases.

## Perception, anatomy and population

Current ambient cell lighting and eye-based 3D perception are implemented. Future
extensions include:

- Secrets/search, blindness, invisibility, disguise and observer hallucination,
  with identical disclosure rules for every frontend.
- Persistent mistaken identities per observer, shifting occasionally in simulation
  time rather than every action. Define identity keys and shift distribution;
  persist beliefs and schedule state without transmitting hidden true identity.
- Sound propagation and richer semantic events when interactions require them.
  Ambient lighting does not imply physical light-source propagation.
- Crouching, ducking and other posture changes. Current whole-body clearance and
  portal frames remain authoritative; facing is not implicitly a gameplay rule.
- Wandering populations on active floors, including revisits. Frozen regions do
  not accrue catch-up time or spawning. Later spawning is distinct from initial
  generation and must persist its state/RNG.
- An immersive 3D frontend only after text and ASCII validate the shared model.
  Asset dependency loading remains client-owned; palette membership conveys broad
  possibilities, not entity-instance knowledge or guaranteed availability.

## Runtime hooks and objectives

Select runtime scripting independently from generation scripting. The accepted
direction is a general event/query/outcome design rather than a built-in victory
variant for every scenario. A tutorial hook may react to a PC actually perceiving
a mob; an escape hook may query carried Amulet possession and invoke victory.

Specify authority, observer/region scope, ordering, once/repeat semantics,
permitted mutation, resource budgets, failure atomicity and persistent state
before choosing a language. Hooks must use the same candidate-state, action and
save boundaries as ordinary gameplay. See [script extension design](scripting.md).
These decisions do not block using the existing exploration-only dungeon.

## Recovery, history and distribution

- Reconcile normal-play suspend/resume and permanent death with safe file
  replacement, live journals and crash recovery. Manual filenames should default
  to the character name without save slots or reusable ordinary rollback saves.
  Never consume/delete the only recoverable state during resumption.
- Keep the server independently startable. Package clients with appropriate
  automatic server launch/attachment and frontend-specific presentations; preserve
  independently started server workflows.
- Extend wizard rewind/retained branches beyond the current bounded window,
  replay checksums and interrupted-write recovery when scoped. Retain receipt and
  identity high-water marks and do not silently discard history.
- Keep ordinary checkpoints loadable through the same version/wizard rules as
  ordinary saves. Caches must invalidate with fixtures/dependencies.
- Resolve client timing tails, large-world/history scaling and allocation
  profiling using the [performance criteria](../development/performance.md).
  Process-restart testing does not establish hardware power-loss qualification.
- Design remote authentication, encrypted deployment and multiplayer input policy
  separately. Local loopback support is not a remote security architecture.

## Portable save export: unimplemented design

Portable export is separate from live storage and ordinary lazy recovery. It
should capture a self-contained, consistent durable boundary with a versioned
manifest of exact content identities/digests. Preserve checkpoint, replay tail,
history/branches, receipts, suspended intentions and identity high-water marks
together. Obtain durability explicitly; do not silently acknowledge/discard an
unsaved tail. Package a consistent read snapshot without holding the engine lock.

The manifest must include pinned definitions, index and every source required for
future activation, including unbuilt regions. Explicitly acquire/verify missing
sources; fail without changing the original save if resources are unavailable or
mismatched. Paths are locators, not portable identities. Exclude credentials,
observer stream contexts, derived topology indexes and runtime caches.

Import should independently validate envelope/save/rules/authoring identities,
bound encoded and expanded resources, reject duplicate identities and unsafe
paths, verify digests and apply ordinary strict recovery invariants. Create the
new destination atomically, preserving existing saves. Container layout,
compression and export/import commands remain undecided.

## Acceptance for each increment

Add tests at each affected layer, actual-client/process acceptance, actor-specific
disclosure, deterministic save/replay/rewind and relevant release performance
comparisons. Ordinary scenario packages are the test setup format; wizard edits
are for intentional runtime mutation or wizard behavior. Record unresolved design
choices instead of silently implementing speculative defaults. Keep current
references and status synchronized in the same change.
