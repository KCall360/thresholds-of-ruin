# Geometry and observer scenes

Regions and portals are backend storage and topology. They are not rooms or
objects presented to a player, and clients do not reconstruct their geometry.
The backend resolves everything currently visible into one actor-relative scene.
A space split across a broad join must look like the same space stored in one
region. The ASCII renderer fits all disclosed cells, including remote cells,
into that scene; text describes visible contents at relative offsets.

## Joins and visibility

New games use `travel-v5`, with deterministic integer cell-centre rays
within eight Manhattan steps. Clockwise quarter turns around z and translation
map a ray across a join. Crossing consumes distance, including self-links and
cycles. A physical cell may have multiple visible occurrences in non-Euclidean
geometry. The scene preserves those occurrences at distinct offsets. The world
API caps sight radius at 16; the current game fixes it at eight.

A rectangular join glues an entire aperture with one affine transform. Every
constituent crossing is validated before any mutation commits. Horizontal joins
can cover both width and height. Several adjacent joins can cover irregular
areas. Reverse connections are explicit and separately validated.

Walls are visible and stop sight and movement. At a diagonal corner both routes
must be clear and resolve to the same destination and orientation. This preserves
continuous sight across broad joins while preventing blocked corner cuts.
Movement retains the observer's axes across rotated joins: repeated north input
continues toward what appeared north in the view, even if backend axes rotate.

Actors and items do not block sight in this slice. There is no lighting or sound
propagation. Visibility samples cell centres rather than continuous surfaces;
narrow corner views are conservatively hidden. Up/down sight follows explicit
stair links and reveals their landings, not an entire destination floor. Separate
visible heights are displayed in adjacent ASCII panels. Arbitrary gravity,
falling/support physics, and continuous stair meshes remain future work.

## Authorized developer setup

Use a separate wizard save and the [wizard credential](wizard-mode.md). The text
client forwards opaque commands; the server alone interprets them. Example:

```text
wizard room 3 5 3 1 West space
wizard room 4 5 3 1 East space
wizard join 3 4 0 0 east 4 0 0 0 0 3 1
wizard join 4 0 0 0 west 3 4 0 0 0 3 1
wizard item tablet 4 2 0 0
wizard teleport 1 3 2 1 0
```

`join` takes source region/x/y/z, direction, destination region/x/y/z,
quarter turns (0–3), width, and height. Width follows +y on east/west faces and
+x on north/south faces; height follows +z. For up/down joins the aperture lies
in x/y. Destination offsets rotate with the join. Extents must be positive,
contain at most 1024 cells, and fit clear source/destination cells. Horizontal
sources must exit a region boundary. Duplicate exits are rejected atomically.
`connect` takes the same first arguments without width/height for a one-cell link.
Vertical links currently require zero rotation and may begin inside a region.

`room` takes ID, width, depth, height, and a developer name. IDs must be nonzero
and unique; width/depth are 1–32, height 1–8. Names are 1–80 UTF-8 bytes without
control characters. These names never label the player's scene.

`wall <region> <x> <y> <z> <closed|open>` sets opaque terrain or clears it.
Placement cannot cover an actor or ground item. Walls can obstruct joins without
removing them. This is terrain setup, not a door interaction: independent door
entities remain pending and will not be tied to portal locations.

Setup consumes no action time. Authorization, revisions, idempotent receipts,
branch checks, durable replay, and rewind apply to entire joins. The private
journal retains full commands/results. Public wizard history exposes only a
summary, so it cannot leak internal geometry even to an ordinary client using
the same identity. Wide joins require the new ruleset.

## Protocol and memory

Protocol **8** sends positions as relative x/y/z offsets, with the actor at zero.
Each visible cell carries an opaque key, position, wall flag, and semantic stair
flags. Items carry `reachable`; sight does not grant pickup reach. Movement
history reports the chosen direction. Observations and history contain no region
IDs, names, dimensions, portal links, coordinate transforms, or visited-region
list. Repeated appearances of one physical cell share a key.

Clients store last-seen contents by opaque key. The backend derives keys from a
private per-save random salt, actor identity, and internal cell identity; the
salt and physical coordinates never leave the backend. Keys remain stable across
movement and save/resume. A stored relative offset describes the last sighting,
not a current global map. If a cell has several appearances in one observation,
memory retains the last occurrence in deterministic scene order.

Only disclosed cells refresh memory, including clearing absent items on a
visible cell. Unseen cells retain stale contents. Memory is connection-local,
resets on branch changes, and cannot be reconstructed from history. ASCII and
text show current sight; headless output also exposes remembered sightings.

## Saves and verification

Save format **3** adds the private view-identity salt. Formats 1 and 2 migrate on
successful open while retaining their ruleset. `two-room-v1` keeps whole-room
perception and its old movement rules; `portal-sight-v2` keeps its four-step
visibility and original input directions for deterministic journal replay.
No save silently changes its rules. The new protocol presents these older
observations through relative views as well. Existing normal saves are never
implicitly promoted to wizard mode.

Tests compare one region against the same space split by a wide join, including
all visible offsets. They cover rotations, consistent movement, height offsets,
wall occlusion, corners, cycles, atomic rejection, stable opaque keys, sanitized
history, reach, legacy replay, and rewind. `scripts/scenarios/wide-join.json` and
`portal-geometry.json` drive the actual server, text, headless, and native ASCII
clients. They test continuous sight, movement/pickup, hidden changes, stale
memory, stairs, restart, and rewind on Windows/Linux in debug and release.

[Unnamed place hints](place-hints.md) add perceived cell anchors in protocol 6.
They carry no labels or boundaries. Shared memory retains last-seen hints; ASCII does not render them; text now uses them as described in
[the adventure slice](text-adventure.md). New saves use
`travel-v5`; earlier saves retain their original rules.

[Backend travel](travel.md) adds protocol 7 and `travel-v5` for new games.
Earlier rules retain their behavior. The [text adventure interface](text-adventure.md)
now adds text travel and approach-then-pickup.
