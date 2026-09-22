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
Rendering includes remembered offsets within 24 cells horizontally in x, 12 in y,
and eight in z. The window shows up to five nearest elevation panels and crops
large charts to keep glyphs readable inside the map panel. Cells outside the
viewport can remain cached; unseen cells are never filled in.

## Verification

Shared-client tests cover translation across successive updates, item refresh,
actor exclusion, elevation, snapshots, rewind, ambiguous views, overflow, bounded
storage, and atomic rejection of invalid updates. ASCII tests check glyphs, grey
pixels, remembered elevation panels, clipping, and mouse targets.

`scripts/scenarios/ascii-memory.json` and `scripts/test_ascii_memory_process.py`
exercise real server/headless/text/native ASCII processes: normal door occlusion,
movement, pickup and revisiting, a rotated crossing, hidden actors and item changes,
spectators, rewind, and save/resume with fresh client memory. They assert both
presentation diagnostics and pixels from the actual native framebuffer. Existing
CI discovery includes these tests on Windows and Linux in debug and release.
