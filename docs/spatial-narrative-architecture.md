# Spatial Narrative & Environment Synthesis Architecture

This document records the design discussion, structural diagnosis, and architectural plan for spatial narrative synthesis, client-spawned place hints, geometry-derived exits, and deterministic description permanence.

> **Status:** superseded by the [IF engine](if-engine.md#places-and-ways).
> Place extent, geometry-derived exits and unified actors are built there.
> Pillar 3's atmospheric and themed prose was dropped: the engine states only
> disclosed facts, so it no longer describes smells, draughts or epithets.
> This page is kept as the record of the diagnosis.

---

## 1. Problem Statement & Diagnosis

Testing the interactive text client (`tor-client-text`) in authored scenarios like `first-dungeon` uncovered four interrelated shortcomings in room presentation and spatial navigation:

### A. The Multi-Cell Actor Volumetric Bug
- **Symptom**: In room descriptions, the text client reported: `"You see yourself above you."`
- **Root Cause**: The 3D volumetric world engine models characters with multi-cell vertical extents (e.g. eye cell at $z = 1$, root body cell at $z = 0$). In `tor-server::adapt`, every visible cell occupied by an actor is included in `visible_actors`. In `adventure.rs`, the client iterated through `visible_actors` and treated the separate vertical cell offsets as if they were independent entities occupying the room.
- **Correction**: An actor's volume is a single unified presence. The observing character represents the player's subjective narrative perspective and must never be listed as an external entity. Non-player volumetric actors must be described as a single cohesive presence regardless of how many grid cells their body spans.

### B. Anchor Dependency & Exit Blindness
- **Symptom**: In `first-dungeon` Room 1 (*Threshold*), the room description listed no exits (`ways` was empty), and typing `east` or `go east` produced: `"You can't see a way east."` despite an open portal archway 5 tiles east.
- **Root Cause**: `destinations()` in `adventure.rs` required `c.place_hint == true` on visible cells.
  1. *Blindness to unhinted openings*: If the next room's anchor is beyond sight radius (Room 2's center is 9 cells away; sight limit is 8), open corridors and passages are completely invisible to high-level travel planning.
  2. *Voronoi anchor suppression*: `destinations()` computed the nearest Manhattan anchor to the player (`place_at(origin)`). When a room has only one anchor, `current.is_none_or(|key| key != c.key)` filtered out that room's own anchor as "the place you are already in", even if the player stood 2 tiles away at the entrance threshold.
- **Correction**: Authored anchors (`place_hint`) are optional semantic naming hints, not mandatory prerequisites for physical movement. The client must derive exits and travel targets directly from disclosed perceptual geometry (wall perimeter breaks, archways, corridors, and frontier cells).

### C. Flat, Disjointed Surroundings
- **Symptom**: Room presentation read as a sterile checklist of isolated database fields:
  ```text
  You stand in a space with a stone floor.
  You can see walls of stone.
  A quiet chill lingers among the stones...
  ```
- **Root Cause**: `describe_with` treated floor material, wall material, and sensory atmosphere as separate, independent lines without synthesizing them into cohesive interactive fiction prose that reflects room scale, architecture, and structural features.
- **Correction**: Synthesize room proportions (hall, chamber, passage, alcove), masonry texture, ceiling clearance, and architectural openings into unified prose paragraphs.

### D. The In-Memory Cache Flaw vs. Deterministic Permanence
- **Proposed Flawed Solution**: Storing generated room profiles in a client-side in-memory map (`BTreeMap<PlaceKey, RoomProfile>`).
- **Flaw**: In-memory client sidecar state violates the core principles of the engine:
  - It does not persist across save/load cycles.
  - It does not transfer between clients (e.g. switching between Text, ASCII, and Spectator).
  - It does not survive checkpoints, retries, or server rewinds.
- **Correction**: Deterministic procedural synthesis:
  $$\text{Description} = f(\text{PlaceKey}, \text{Geometry}, \text{Materials}, \text{Authored Hints})$$
  Because the cell `key`, geometry, and scenario hints are permanent and durable in the world state, a pure mathematical derivation produces byte-for-byte identical descriptions across all clients, saves, and re-entries with zero sidecar state.

---

## 2. Core Architectural Pillars

### Pillar 1: Client-Spawned Place Hints
When an author has not provided a `place_hint` in a room or corridor:
1. The client analyzes the disclosed walkable floor and deterministically selects a focal anchor cell (e.g. the centroid or primary junction).
2. This designates a **client-spawned place hint**.
3. The cell's stable opaque `key` acts as:
   - The permanent anchor for deterministic description synthesis.
   - The recipient for player naming (`name room <Name>`) and notes.
   - The target destination for travel from adjacent spaces.

### Pillar 2: Geometry-Derived Navigation & Exits
The client computes travel options across three cascading tiers:
1. **Tier 1 (Authored Anchors)**: Disclosed `place_hint` cells that are not the player's immediate standing cell.
2. **Tier 2 (Doors)**: Closed or open doors along the bearing.
3. **Tier 3 (Perimeter Openings & Frontiers)**: Breaks in the wall boundary where walkable floor continues outward. The furthest visible walkable cell along that bearing provides the target key and label (e.g. *"an open passage to the east"*).

### Pillar 3: Rich Architectural Synthesis with Authoring & Procedural Variety
To avoid repetitive "mad-lib" prose, narrative generation blends multiple dimensions of variety:
- **Authoring Hints**:
  - Scenario `themes` (e.g. `["ruined-stone", "scavengers"]`, `["flooded", "catacombs"]`).
  - Region `zones` (e.g. `entry`, `sanctum`, `mines`, `barracks`).
  - Authored region names (`"Threshold"`, `"Broken gallery"`, `"Reliquary"`).
  - Materials and asset palette styles.
- **Geometric Classification**:
  - *Passage / Corridor*: $L \gg W$ (aspect ratio $\ge 3:1$).
  - *Chamber*: Compact rectangular space ($L \approx W$).
  - *Hall*: Expansive floor area ($> 14$ walkable tiles).
  - *Alcove / Niche*: Tight space ($< 5$ walkable tiles).
  - *Vaulted Space*: Ceiling clearance $\ge 3$ tiles.
- **Syntactic Rotation**:
  - Using a deterministic hash of the stable place key (`key_hash(key)`), rotate among distinct phrasing archetypes:
    - *Spatial focus*: *"A broad, quiet chamber of quarried stone opens before you..."*
    - *Architectural focus*: *"Heavy stone masonry encloses this rectangular hall..."*
    - *Atmospheric focus*: *"Shadows pool across the cold stone flags of this vaulted room..."*
    - *Threshold focus*: *"You emerge onto the chilled flagging of a wide gallery..."*

### Pillar 4: Unified Volumetric Actor Observation
- Actors are presented as unified entities, not disconnected cells.
- The observer character (`actor.id == observation.actor`) is filtered out of third-person room entity descriptions.
- Non-player actors spanning multiple cells are rendered by their primary entity name and root offset, with size adjectives derived from their extent (e.g., *"a towering stone guardian"*).

---

## 3. Implementation Roadmap

1. **Parser & Docs Hotfix (CI unblocking)**:
   - Fix unclosed HTML tag warnings in `grammar.rs` doc comments to restore green CI.
2. **Perceptual Geometry & Exit Discovery (`tor-client-text::adventure`)**:
   - Implement perimeter wall-break detection.
   - Refactor `destinations()` to use multi-tiered discovery (anchors &rarr; doors &rarr; wall openings).
   - Ensure `ways` reflects all physical openings even in unhinted spaces.
3. **Deterministic Spatial Synthesis (`tor-client-text::narrative`)**:
   - Implement geometric form classification and perimeter opening narration.
   - Implement deterministic template rotation seeded by `key_hash(place_key)`.
   - Synthesize room shape, materials, and openings into unified descriptive paragraphs.
4. **Actor Observation Deduplication**:
   - Filter observer actor from `describe_with`.
   - Aggregate multi-cell figures into single entity statements.
5. **Testing & Verification**:
   - Unit tests for geometry classification, perimeter detection, and deterministic description stability.
   - Integration tests in `crates/client-text/tests/it/adventure.rs`.
   - Process acceptance tests verifying that `first-dungeon` Room 1 immediately discloses exits and responds to `east`.
