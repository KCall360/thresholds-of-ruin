# TOR support for the Rogue scenario

Reviewed against published main on 2026-10-08, including the completed streaming,
architecture refactor and three-dimensional sight work. The
[accepted scenario plan](../../rogue-scenario-plan.md) owns sequencing and
adaptations; the [TOR design plan](../../game-design-plan.md) owns shared systems.
This assessment is not a playable package or a behavioral-parity commitment.

## Available foundations and missing support

| Capability | Current published support | Planned addition |
| --- | --- | --- |
| Authoring and validation | Ordinary format-2 TOML packages, per-region sources, structural catalog, validation certificates and pinned inputs | Fine-grained declarative recipe stages and multi-region generation groups |
| Generation | Per-region `rooms` generator, semantic seeds and independent geometry/population/loot streams | Rogue 3x3 floor composition, room omissions, mazes, secrets, depth-dependent content |
| Loading and persistence | Horizon-driven active/loaded sets, reference points/pins, frozen regions, disk detachment and background preload | Coordinated whole-floor generation before traversal, with all nine regions validated together |
| Geometry and stairs | Finite carved stone, broad rotated portals, bodies/gravity and explicit traversal links | Scenario pattern with paired cross-region stairs; physical trapdoor falls through portals |
| Actions and combat | Shared queued player/AI/travel execution, timed melee, preparation/resume, typed damage and death | Ranged actions, configurable bouncing and reusable status/effect families |
| Builds/progression | Actor combat profiles and body declarations | Six attributes, types/subtypes, racial HD, templates/classes, tracked advancement/regression, ECL/CR/XP |
| Survival | HP and ordinary death | Nutrition, food, starvation and passive healing |
| Items/knowledge | Quantities, stacks, actor-owned identity knowledge and randomized appearances | Anatomy slots/capacity, equipment/curses/enchantment/rust, charges and instance details, consumables and richer identification |
| Perception | 3D eye-based sight, disclosed solids/entities, stale memory and durable place names | Lighting, probabilistic secrets/search, blindness/invisibility/disguise and persistent observer hallucinations |
| AI/population | Deterministic perception-limited search/attack/flee and saved memory | All 26 species' abilities/preferences, depth tables and wandering spawning |
| Objectives | Authored anchor with optional exact-item requirement and persistent terminal results | Generated Amulet at depth >=26, surface escape and event-hook victory composition |
| Script extensions | Language-independent future contracts only | Generation extensions if needed; runtime event hooks designed separately |
| Clients/recovery | Text/adventure, native ASCII and headless clients, save/replay/rewind and disclosure checks | New actions/statuses/prompts as shared mechanics arrive |

Published equipment/item-use support is not implied by source in another checkout.
Likewise, the existing single-region generator is not the accepted nine-region
floor recipe. No general scripting language or new recipe fields are valid today.
Current runtime versions live only in the [roadmap](../../milestones.md#current-implementation).

## Adaptation contract

Preserve Rogue's content and generation intent from [rules](rules.md) and
[the catalog](content.md), but implement it through TOR rules. Floors persist;
backtracking is provisionally permitted; stairs form persistent pairs. Required
routes may contain discoverable secrets. Ordinary doorways contain no usable
doors. There is no ascent-specific loot suppression. TOR controls combat/action
timing, capacity, progression, AI knowledge, physics and spatial perception.
Hallucinated identities persist per observer and shift occasionally, not each turn.

All 26 species and the item catalog become standard TOR content. Scenario pools
restrict spawning to that content; special item rules are decided individually.
Species builds use shared racial HD and innate abilities, tuned to approximate
Rogue pressure rather than copying its combat/XP equations. Anatomy determines
slots, and PCs/mobs use the same equipment and consumable actions.

## Implementation and acceptance

1. Deliver exploration-only nine-room floors, corridors and paired stairs first.
   Generate the full group at the loading horizon, preserve it on revisits and
   resume, and verify coherent movement/perception across carved region joins.
2. Add layout features as supported: omitted rooms, mazes, secrets and lighting.
   Validate reachability after discovery, not an invented visible-route guarantee.
3. Implement shared tracked builds/progression/defenses and reusable effects.
   Test reversible HD/class advancement, zero-HD death and source-granted immunity.
4. Add shared survival, equipment/knowledge and ranged systems, then each usable
   item category and species. Exercise every special through ordinary play.
5. Complete all eight trap families, depth population and active-floor wandering.
   Trapdoors are physics crossings; inactive regions freeze without catch-up.
6. Complete Amulet placement and escape victory through the separately designed
   hook/query/outcome facility. Test absent-goal escape and terminal recovery.

The [scenario plan](../../rogue-scenario-plan.md) contains detailed stages,
acceptance gates and remaining design decisions. Each increment requires ordinary
scenario/process coverage, actor-specific disclosure, deterministic save/replay
and relevant performance checks under the [testing policy](../../testing.md).

## Reference-only fidelity work

Exact Rogue combat numbers, floor regeneration, action-cycle ordering, room-wide
revelation, historical capacity/scoring and executable RNG parity are comparative
reference topics, not requirements for this adaptation. Any future parity claim
would require a pinned executable and differential traces, including edition
patches and edge cases. The research catalog does not replace that validation.
