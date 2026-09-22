# Diagonal movement and reach

Games use **diagonal-v11**, with protocol **11** and save format **3**. Previous
rulesets and save formats are unsupported; start a fresh game after an upgrade.

## Rules

Eight horizontal directions are available. A diagonal is one atomic move, with
recovery `ceil(base ticks × √2)`: 100 ticks becomes 142, 75 becomes 107.
The computation uses checked integer arithmetic, without floating point or
accumulated fractional time. Overflow rejects the action before mutation.
Up/down still requires an explicit vertical connection.

The destination must be walkable and unoccupied. At least one of the two side
cells must also be clear: northeast needs north **or** east. Walls, closed doors,
and actors obstruct sides; ground items do not. There is no intermediate occupied
position, extra action, or intermediate observation. Rejections consume no time.

Across portals, each side route uses real cardinal connections and composes their
quarter-turn rotations. If both usable routes disagree on destination or final
orientation, the diagonal is unavailable. Sight-only material rim projections
cannot create a movement connection. Wizard joins still use cardinal/vertical
faces; there is no diagonal portal-authoring command.

Open/close actions gain diagonal reach with the same side-clearance rule, while
retaining the normal manipulation duration. Closed target doors can be opened;
side doors must be open. Existing occupancy restrictions on closing still apply.
Pickup remains at the actor's feet. Visibility and its range remain unchanged.

## Travel and clients

Travel minimizes total movement ticks using a deterministic weighted search.
It composes diagonal routes only from remembered disclosed connections and clear
side cells. It never queries hidden current geometry to find a route. Ordinary
movement revalidates every executed step; stale obstacles interrupt travel.
Equal-cost routes preserve stable discovery order: N/E/S/W/up/down, then
NE/SE/SW/NW.

ASCII movement:

```text
Y K U
H . L
B J N
```

Arrows remain cardinal movement. `<` ascends; `>` descends, with D retained as
a descend alias. F4 opens notes. Period/Space wait; shifted `>` never also waits.
O/C followed by any horizontal movement key manipulates a door. Destination
selection accepts diagonals too.

Text accepts northeast/southeast/southwest/northwest and ne/se/sw/nw. `step ne`
makes one move; `ne` or `go northeast` requests travel to a visible destination.
Descriptions use eight compass bearings. Script mode retains immediate movement.
Headless actions use `north_east`, `south_east`, `south_west`, `north_west`.

## Verification

World and simulation tests exercise corner masks, occupancy, exact timing and
overflow, rotated and ambiguous connections, normal-cost diagonal door actions,
hidden corner approach disclosure, stale navigation, and a route where seven steps cost less than six diagonals.
Server tests cover durable retries, restart, and rejection of obsolete rulesets.
Client tests cover parsing, bearings, key mappings and free cursor movement.

`scripts/scenarios/diagonal.json` and `scripts/test_diagonal_process.py` exercise
real server/text/headless/native ASCII processes, native Y/U/B/N, F4 and `<`/`>` input,
diagonal door input, one-clear-side travel, spectator denial and agreement,
rotated movement axes, save/resume and rewind. The existing Windows/Linux CI
discovery runs these in debug and release.
