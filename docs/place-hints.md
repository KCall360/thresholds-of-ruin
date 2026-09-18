# Unnamed place hints

A place hint is a boolean attribute of a map cell: this point may help a client
organize perceived space into locations. It has no name, description, extent,
room membership, or prescribed presentation. It does not assert that the actor
has explored the surrounding area. Clients may combine hints with perceived
geometry and contents, infer locations without hints, or ignore them entirely.

Map authors and future generators choose anchors explicitly. Hints are independent
of regions and portals: a region can contain several hints or none, and a perceived
space can span internal joins. The current authored two-space scenario places a
hint at each space's center. There is no procedural generator yet.

## Disclosure and memory

Protocol **7** retains the protocol-6 requirement for `place_hint` on each disclosed `visible_cells` entry.
Only perceived cells carry hints; there is no dungeon-wide marker list. The
existing opaque cell key identifies the location, and the existing relative
position locates each visible occurrence. Repeated views of the same cell through
non-Euclidean geometry carry the same key and hint, with their respective offsets.
No region identities, transforms, labels, or unseen connections are disclosed.

Shared client memory retains the last perceived value. An unseen removal stays
stale until that cell is perceived again. A fresh observation replaces the value,
including `false`; rewind clears abandoned-future memory. Headless JSON exposes
current hints and remembered hints. Text and ASCII receive and retain them without
rendering markers or changing navigation. Text grouping and location-based travel
are future work; this slice establishes their input.

## Dynamic authoring

Backend map setup can set or clear a hint without consuming action time or
changing movement, sight, or topology. Coordinates must identify an existing cell.
Hints can coexist with actors and objects. Making a cell solid retains its
authored hint but suppresses disclosure; clearing the wall reveals it again.
Terrain edits do not automatically infer new anchors. Future digging or generator
logic may explicitly add or remove them.

In a separate authorized wizard game:

```text
wizard place 1 2 1 0 on
wizard place 1 2 1 0 off
```

Arguments are region/x/y/z followed by `on` or `off`. The client forwards opaque
developer text; only the server interprets the coordinates. Commands obey existing
authorization, revisions, branch checks, durable retries, restart, and rewind.
Public wizard history retains sanitized summaries, not hidden marker locations.

## Compatibility and verification

New games use `travel-v5`. Save format remains **3**: the new rules version
selects the authored hints and supports journaled hint edits. Older servers reject
the unknown rules version. Existing `two-room-v1`, `portal-sight-v2`, and
`observer-scene-v3` games retain their original fixture, replay, and rules, expose
`place_hint: false`, and reject wizard hint edits. All connected binaries must
use protocol 7. Normal games are never implicitly promoted to wizard mode.

Tests cover topology-independent authoring, visibility, terrain changes, stale
memory and refresh, permissions, invalid setup, replay, and rewind. The versioned
`scripts/scenarios/place-hints.json` scenario drives actual server/text/headless/
native ASCII processes. Existing Windows/Linux debug and release CI discovery
includes these tests.

[Backend travel](travel.md) adds protocol 7 and `travel-v5` for new games.
Earlier rules retain their behavior; text has no travel commands in this slice.
