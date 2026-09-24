# Symmetric shadowcasting

The current rules use symmetric shadowcasting and finite stone enclosure.
Protocol 11 exposes floor/ceiling surfaces; saves use format 3 and
`diagonal-v11`. Older rulesets are rejected. See [material volumes](material-volumes.md).

## Geometry and behavior

Horizontal sight uses the point-floor / diamond-wall model explained by
[Albert Ford](https://www.albertford.com/shadowcasting/), independently implemented
with exact integer slope comparisons. Closed doors and walls are equally opaque
for sight. Movement continues to treat them as full blocked cells.

- Floors are tested at their centers. Blockers are beveled for sight.
- An adjacent closed door remains visible; it hides the floor directly behind it.
- Diagonal floor next to that door can remain visible. Even two diagonally touching
  blockers can admit a sightline through their corner.
- Wall visibility includes exposed portions, so convex room walls remain visible.
- Floor visibility is reciprocal on ordinary grids at equal sight range. Rotated,
  bidirectionally joined partitions of the same space produce the same result.
  This is not a promise of reciprocity across one-way or ambiguous topology.
- Actors and items do not block sight. Only currently visible contents are sent
  to clients; remembered contents remain explicitly stale client knowledge.

Range remains eight Manhattan steps (world API capped at 16). Vertical sight
continues to follow explicit stair/shaft links and stop at opaque landings.
No automatic door opening, movement, sound, lighting, or targeting rules change.

## Implementation

Four quadrant scans split angular intervals at opaque cells. Slopes are integer
numerator/denominator pairs; cross-products and signed Euclidean rounding avoid
floating-point tolerances and directional rounding bias. Depth is bounded by the
radius, including on cyclic maps.

Topology resolution is separate from obstruction. Each sampled observer offset
is resolved once per scene. Cells inside the origin's rectangular region use a
direct orientation transform. Remote offsets use bounded integer geometry rays;
at exact corners both routes must agree on location and orientation. They ignore
opacity, which shadowcasting evaluates separately. Missing/ambiguous geometry
blocks sight but never appears as a disclosed wall. Physical cells can have
multiple occurrences, identified by observer offset rather than deduplicated.

The scan skips offsets outside Manhattan range. Ordinary-region sampling is
O(r²), plus output sorting and world lookups. Remote geometry resolution can
still cost O(r³) overall; the full portal pipeline is not claimed to be O(r²).
There is no persistent visibility cache to invalidate after a door edit or rewind.

## Verification and performance

`crates/world/tests/shadowcasting.rs` exhausts all 512 patterns of a 3x3 obstacle
area and tests every floor pair for reciprocity. It also checks door corners,
straight shadows, convex room walls, diagonal blockers, rotated split-room
invariance from both sides, and bounded repeated appearances through cycles.
Server tests verify current perception through save/resume and rewind. `scripts/scenarios/shadowcasting.json` and
`scripts/test_shadowcasting_process.py` drive real text, headless and native ASCII
clients and check disclosure, stale memory, ordinary door actions and resume.
These tests are included in Windows/Linux CI discovery for debug and release.

Run `cargo run -p tor-world --release --example fov_bench` to measure current
scene construction, including topology and sorting. The benchmark covers open
rooms, pillars, a broad rotated join, and a cycle at radii 8 and 16. Timings are
diagnostic, not pass/fail thresholds. Historical ray implementations and their
comparison benchmark have been removed.
