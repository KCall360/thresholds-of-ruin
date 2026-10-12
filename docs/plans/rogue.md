# Rogue adaptation: remaining work

The goal is Rogue's generated-dungeon, survival and descent/retrieval/ascent
experience using **shared TOR mechanics**. The [pinned Rogue reference](../research/rogue-5.4.4/README.md)
is evidence for content and behavior intent, not a second combat/timing engine or
an executable-parity commitment. [Current status](../status.md) owns scope;
[shared requirements](requirements.md) own reusable systems.

## Adaptation decisions

- Descent has no preset final floor, as in the pinned reference. Compact lazy
  descriptors use an explicit operational depth limit for safe execution; that
  limit is not a Rogue victory condition or a seed-selected dungeon length.
- Floors persist on revisits; no regeneration or ascent-specific loot suppression.
- Nine regions form a 3×3 floor, stone-filled with broad portal joins; fine-grained
  declarative stages carve rooms/corridors. Region boundaries need not be walls.
- Generate a complete floor group within the preparation/loading horizon before
  traversal requires it; distinguish speculative preparation, publication and
  active residency. Use independent stage streams and pinned versions.
- Named paired stairs connect corresponding endpoints. Backtracking is generally
  allowed; gates require separate scenario design. Falling through a portal is
  physics, not a stair action. Invalid arrivals consume no movement time.
  Stair locations and the initial entry vary deterministically with the seed.
  Returning to a paired stair deliberately differs from Rogue's separately
  selected arrival cell after regenerating each floor.
- Reproduce nine room slots, connecting corridors, omissions, darkness, secrets
  and mazes as their shared stages become supported. Required routes may contain
  discoverable secrets. Ordinary room entrances have no usable doors.
- Preserve Rogue's 26 species, item catalog and distinctive capabilities as
  standard TOR content, with scenario-restricted generation pools. Species are
  constructed from types, subtypes, HD and eligibility-gated talent trees and
  tuned in the arena for comparable behavior, not copied XP or combat equations;
  see the [progression design](progression-design.md#rogue-roster-as-constructed-creatures).
  Do not label fixed-damage placeholders complete species.
- TOR owns timing, progression, capacity, AI knowledge, physics and spatial sight.
  Exact Rogue combat numbers, action cycles, RNG, regenerated floors, room-wide
  revelation, capacity and scoring are not required parity targets.
- Use shared nutrition/healing, anatomy equipment, NetHack-style character
  knowledge, reusable conditions and ranged actions. Per-item unusual rules,
  including scare-monster scrolls, need individual decisions.
- Hallucinated identities persist per observer with occasional timed shifts.
  Population advances on active floors, including revisits, without frozen-time
  catch-up. Gold/item pursuit requires actor knowledge.
- Place the Amulet on newly generated floors at depth 26 or deeper while it is
  not carried, then win by carrying it out. Depth 26 is an eligibility threshold,
  not the final floor. This objective remains unimplemented; decide the non-winning
  exit behavior and presentation/scoring explicitly.

## Existing exploration foundation

The `rogue-exploration` package describes an open dungeon with one compact
grid-chain family. Each lazily materialized floor has nine 26×7×2 regions,
deterministic group preparation/publication and independent region persistence.
Initial entry and stair ownership/locations are seed-selected; the initial entry
is a safe room cell distinct from the stairs. Floor 1 has no upward exit.
Depth 26 has an ordinary onward connection.

The package's operational limit is 1,000,000 materializable floors. No floor
stubs, index entries or geometry are created for unvisited depths. Structural
addresses continue beyond the operational limit; attempting to cross it fails
without advancing the game or creating a terminal objective. Existing generic
per-group, graph-query, validation and preparation budgets still apply.

Room lighting is depth-dependent and corridors are dark; the graph may include
extra connections. There are no mobs, items, survival mechanics or victory.
Persistent floors and paired return stairs are intentional TOR adaptations,
not exact replication of Rogue's regenerated floors and independent arrivals.

[Generation reference](../reference/generation.md) describes supported recipes,
validation and actual-client coverage. This foundation does not need to wait for
all creature/item mechanics. Its remaining performance acceptance, particularly
save/flush tails, belongs to the [performance guide](../development/performance.md).
Do not mistake “implemented” for closure of those measurements.

## Next increments

### Layout, perception and discovery

Add omitted slots, mazes, secrets and timed search through reusable stages and
shared discovery rules. Lighting already exists; extend it without replacing TOR
LOS with whole-room revelation. Add blindness/invisibility/disguise/hallucination
when their shared behavior is ready.

Acceptance: connectivity after discovery, no invented visible-route guarantee,
consistent movement/sight across carved joins, actor-specific discovery/memory,
recovery of mistaken identities and shifts, and unchanged stage-stream isolation.

### Complete shared builds/effects and survival

Use the implemented creature builder, attributes, sources, advancement, generic
effects, equipment and paid preparation. Extend the missing tables/mechanics,
CR/XP policy, level-drain restoration/dependencies, nutrition/healing and capacity
under [shared requirements](requirements.md). Do not reimplement the completed
rules refactor as a prerequisite.

Acceptance: equivalent PC/mob rules; deterministic progression/CR/XP; recorded
advancement reversal and zero-HD death; retained innate species properties;
source-granted defenses, stacking and active-time recovery across freeze/replay.

### Items and ranged actions

Introduce food and item-generation stages as each category becomes usable.
Complete curses/enchantment/rust/charges, type/instance knowledge and the catalog's
specials. Add throw/launch/zap with configurable bouncing and resource/timing
rules. PCs and AI use the same ordinary actions with knowledge-limited decisions.

Acceptance: capacity/anatomy constraints, unknown-property disclosure, naming
without identification, allowed observed-use learning, resource consumption,
obstruction/bounces, starvation/healing, AI use and full recovery. Balance food
safeguards and treasure-room population only when prerequisites exist.

### Species, traps and pressure

Add each of the 26 species with its normal-play ability/behavior case, then include
it in depth/spawn tables. Complete trapdoor, arrow, sleeping gas, bear trap,
teleport, poison dart, rust and mysterious trap families. Physical trapdoor falls
must respect destination generation/loading and geometry. Add seeded wandering
population while floors are active.

Acceptance: generation restricts content correctly; depth increases pressure
through TOR mechanics; all specials work via ordinary actions/physics; inactive
floors neither spawn nor accumulate catch-up spawning.

### Objective and complete run

Design [runtime hooks](scripting.md) separately. An escape handler should query
inventory and trigger victory; tutorial hooks follow actual perception. Resolve
once/repeat ordering, authority, bounded execution and saved state before choosing
a runtime. Then add the Amulet and escape goal, retaining persistent death/victory.

Acceptance: no premature Amulet placement, carrying the goal out wins, absent-goal
escape cannot accidentally win, supported clients complete the run, and long
seeded descents/ascents validate food, population and difficulty. A recognition
claim is not exact executable parity; that would require separately pinned
binaries and differential traces.

## Decisions still needed

Generation-extension/scripting need; search/discovery rules; remaining creature,
CR/XP and survival balance; drain restoration; capacity and richer item details;
ranged collision/effect order; hallucination shift policy; per-item edge cases;
depth distributions/starting gear; non-winning exit and presentation/scoring.
Implemented recipe schema, floor dimensions, stair resolution and basic rule
imports are no longer undecided prerequisites. Every increment follows the
[testing policy](../development/testing.md) and updates its current references.
