# ASCII remembered map

The ASCII client renders currently visible cells in their normal colors and
previously seen cells in grey. Remembered terrain, doors, stairs, and ground items
retain their last disclosed state. Actors disappear when they leave current sight.
Seeing a cell again refreshes its contents, including removing items that are gone.
The `GREY: LAST SEEN` legend distinguishes memory from current sight; the inventory
and `IN SIGHT` list still show current observations only.

This is entirely client-side. No server, protocol, save-format, or gameplay-rules
change is involved. Hidden changes do not refresh memory.
Mouse travel and destination selection still target currently visible cells only;
mouse hit testing uses the same expanded layout as painting.

## Alignment and lifetime

The shared client processes every received observation, including intermediate
travel updates, before the native window presents it. A separate map cache aligns
old cells to the current actor-relative scene by matching opaque disclosed cell
keys that occur once in both successive views. All such matches must agree on
one translation. Ordinary movement, elevation changes, and rotated crossings that
preserve the displayed axes can therefore retain the map without learning region
identities or connection transforms.

Some non-Euclidean views cannot form one consistent map. If overlap is absent,
ambiguous, or conflicting, the spatial chart starts again with the current view
rather than guessing old cells' locations. This can occur after teleportation or
in cyclic layouts. The original historical last-seen memory remains separate and
unchanged; its stored offsets still describe past sightings.

Map memory lasts for the connection. Consistently aligned same-branch snapshots
retain it; rewind clears it. Restarting a client begins with its current snapshot,
without reconstructing past views from history or the save. Remembered item
positions may be stale if another actor moved them out of sight.

The spatial cache retains at most 4096 occurrences, preferring nearby cells.
Rendering includes remembered offsets inside the map window (31 cells either
side in x, 16 in y) and from two cells below the feet to three above, merged
into one map as described in [the ASCII client](ascii-client.md#the-map).
Cells are a fixed size; the map is never rescaled to fit. Cells outside the
window can remain cached; unseen cells are never filled in.

## Known limitations

- **3D views fill the cache sooner.** With
  [three-dimensional sight](sight-3d.md), a view also discloses floors,
  ceilings and empty headroom, and the cache keeps all of them. The 4096-cell
  bound therefore covers less explored area than it did with plane views,
  especially for tall observers or in tall rooms, so the remembered map may lose
  distant cells sooner. The rendered window alone spans about 20,800 positions,
  so the bound, not the window, limits what can be shown. This hasn't been
  measured. If it matters in play, options include raising the bound or not
  caching empty headroom, though the single map uses headroom to tell low
  walls from full walls, so dropping it would change what it shows.
- **A conflicting view discards the whole chart.** A single disagreeing anchor,
  such as a cell seen through a rotated portal, resets the chart as described
  above. 3D sight shows more cells through portals, including vertical ones, so
  this may happen more often than before.

## Verification

Shared-client tests cover translation across successive updates, item refresh,
actor exclusion, elevation, snapshots, rewind, ambiguous views, overflow, bounded
storage, and atomic rejection of invalid updates. ASCII tests check glyphs, grey
pixels, remembered heights merged into the single map, clipping, and mouse
targets.

`scenarios/tests/ascii-memory-*` and `scripts/test_ascii_memory_process.py`
exercise real server/headless/text/native ASCII processes: normal door occlusion,
movement, pickup and revisiting, a rotated crossing, hidden actors and item changes,
spectators, rewind, and save/resume with fresh client memory. They assert both
presentation diagnostics and pixels from the actual native framebuffer. Existing
CI discovery includes these tests on Windows and Linux in debug and release.
