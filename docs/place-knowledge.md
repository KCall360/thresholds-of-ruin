# Durable place knowledge

Characters learn an anchor when its hint first appears in their authoritative
perception. Seeing an anchor discovers that point, never a room extent, region,
connection, or surrounding contents. Repeated portal views identify one anchor.
Characters do not share knowledge or player-assigned names.

The backend assigns a deterministic mnemonic such as **Hollow Promise** or
**Quiet Reverie**, using the game seed and that character's discovery order.
These are personal, imagined names, not claims about physical properties or
authored region names. The 256 combinations receive numeric suffixes after the
first cycle. A player's rename may duplicate another name; selection uses a
listed number and the exact opaque identity, not name matching.

`places` lists names in the text client, with `in sight` or `remembered` beside
each. `name <place number> <new name>` renames an entry. ASCII uses **F5** for the
list, Up/Down to select, Enter to begin a new name, Enter to save, and Escape to
cancel/close. A visible floor cell with the same key establishes `in sight`;
remembered entries carry no current bearing, route, or reachability promise.
List numbers are local to the current list, not persistent identities.

Names accept 1–80 UTF-8 bytes, with no control characters or leading/trailing
whitespace. Renaming requires control, is revision-checked and journaled, and
does not advance time. Spectators can read the actor's names but cannot rename
them. Duplicate requests retain the original receipt. Naming cancels an active
travel request at the existing command boundary.

Learned places and names persist through disconnection, snapshots, checkpoints,
restart and strict replay. Rewind restores the chosen boundary's knowledge and
names; abandoned-future discoveries do not survive on the new branch. Removing a
hint, covering its cell or changing unseen geometry does not erase the learned
location or a player's name. This is historical knowledge, not live marker state.

This slice adds listing and renaming only. It does not restore all client map
memory after reconnect, infer places without hints, label room extents, or offer
offscreen travel destinations. Existing visible-anchor direction travel remains
unchanged. Future place travel can use remembered cells without new topology
disclosure.

## Boundaries and compatibility

The simulation stores remembered names alongside actor navigation, in shared
source-region maps. Refresh inspects only the perceived scene; unchanged
knowledge does not detach storage. Checkpoints deduplicate place maps across
rewind boundaries. Protocol 13 sends only opaque actor-specific cell keys and
names in `observation.places`; it sends no region coordinates or authored labels.
Save format 7 and `places-v12` reject prior versions under the pre-release policy.

## Verification

Simulation tests cover first sight, free reads, actor ownership, removal,
validation and checkpoints. Server tests cover hidden anchors, rename retries,
free timing, restart/replay and rewind. Client tests cover listing, renaming and
the absence of offscreen travel selection. The versioned place-hint process
scenario runs actual text, headless and native ASCII clients through discovery,
renaming, spectators, stale knowledge, save/reconnect and rewind.

Focused profiling includes matching existing movement cases and a versioned
place-knowledge workload; see the [performance harness](performance-harness.md).
Milestone 3p remains deferred and open.

The [verification findings](place-knowledge-findings.md) record local tests,
performance comparisons, diagnostic limitations and retained evidence.
