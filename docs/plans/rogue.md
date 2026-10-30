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
- There is exactly one Amulet of Yendor identity. On every actual player entry
  to depth 26 or deeper, including revisits, create it once if absent or relocate
  that same item to a random valid supported/reachable ground cell on the entered
  floor unless the player carries it. Existing piles, the entrant's cell and
  reselecting its current cell are valid outcomes. Queries, speculative preparation,
  NPC entry and frozen time never trigger placement. Depth 26 is an eligibility
  threshold, not the final floor. Intentional surface traversal wins only while
  carrying it; refusal without it costs no time. Internal backtracking never
  requires the Amulet.
  The objective and intentional surface escape are implemented; expanded
  presentation/scoring remain open.

## Implemented foundations and canonical owners

The generated persistent-floor foundation, complete 26-species selector, Warrior/
Mage starting kits and ordinary advancement, nutrition, discovery/traps, control
items, current loot and unique retrieval/ascent objective are implemented.
Mechanism availability does not establish a calibrated survival curve.

| Concern | Current owner |
| --- | --- |
| Launch, floors and objective | [Rogue play](../play/rogue.md) |
| Encounters, supplies, senses and survival | [Rogue mechanics](../play/rogue-mechanics.md) |
| Combat/conditions | [Combat](../reference/combat.md), [creature rules](../reference/creature-rules.md) |
| Builder and owned progression | [Progression](../reference/progression.md), [Polymorph](../reference/polymorph.md) |
| Physical/equipped/magical items | [Items](../reference/items.md), [equipment](../reference/equipment.md), [magic](../reference/magic-items.md) |
| Missing-consumables design decisions | [Design record](rogue-missing-consumables.md), [current effects](../reference/consumable-effects.md) |
| Population, stock and generated layouts | [Packages](../reference/scenario-packages.md#live-rogue-content-policy), [generation](../reference/generation.md) |
| Observation, discovery and tactics | [Perception](../architecture/perception.md), [AI](../reference/ai.md) |

## Remaining acceptance and design

- Calibrate coupled food, Health/resource recovery, XP, loot and depth-selected
  encounters against a locked reference build, following the [tuning plan](rogue-tuning.md).
- Establish useful ordinary earned builds and pressured descent/retrieval/ascent
  across both presets. A mechanism fixture, controlled opponent or one surviving
  prefix is not proof of autonomous full recovery or balanced starter survival.
- Preserve bounded comparator identities. Later content activation does not
  silently widen the frozen eighteen-foe earned-route study.
- Consider capacity/containers, richer item/presentation/scoring and runtime
  generation extensions only as separate approved increments. Do not relabel
  implemented Hallucination/discovery/items as absent because older plans did.
- Keep source-free persistence, client/native behavior, upper operational guards
  and measured scaling separate from content existence and numerical balance.

### Deferred deep-stat capacity

The current 256 actual-HD admission and detailed-inspection guards are accepted
for now and do not block functional Rogue. Exact counts, owners, aggregate Health,
threat and XP use checked 64-bit domains; ordinary summaries use compiled
per-source counts, while saved and detailed records retain every actual HD.
Weighted finite/open population intervals and wide scalar fields do not remove
the row guards or establish support beyond them.

Exact paged HD storage, archive and inspection work and eventual guard removal
are deferred to a separate approved increment. That work must preserve every
actual HD record, individual Health seed and earned choice through saves,
inspection, rollback and replay. A counted-HD suffix, averaged Health or silent
clamp cannot substitute for it. Continue to report current capacity refusal
atomically rather than inventing a terminal floor.

## Historical sequence

Completed stage diaries, detailed checkpoint counts and retained fixture budgets
remain in the [reviewed pre-reorganization plan](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/plans/rogue.md).
Useful design decisions remain above and exact current contracts have one owner.
