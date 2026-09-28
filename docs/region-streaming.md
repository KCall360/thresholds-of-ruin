# Region streaming foundations

Milestone 4e is in progress. The first implemented slice is a backend structural
catalog and preload-horizon planner. Runtime games still construct and activate
every authored region. Freezing, disk eviction, on-demand generation, deferred
reactivation, and client asset-palette delivery are not implemented yet.

## Inspect a horizon

```powershell
cargo run -p tor-server --bin tor-scenario -- horizon scenarios/first-dungeon 1 2
```

The read-only authoring command requires a current validation certificate. It
prints JSON containing sorted `required`, `activate`, and `deactivate` region IDs,
plus `expanded_regions` and `examined_links` operation counts. It uses an empty
initial active set, so `activate` equals `required` and `deactivate` is empty.
Errors use the utility's existing JSON error envelope and nonzero exit status.
Package parsing and integrity checks still read the complete bounded authored
package; the command does not claim bounded startup for a streamed world.

## Structural contract

`RegionCatalog::from_package` indexes bounds, named anchors, zone/theme metadata,
and directed outgoing region links. It does not create a world, inspect actors
or items, consume RNG, or mutate the package. Anchor resolution works without
loading the destination region. Unknown zones, duplicate/zero region IDs, invalid
bounds/anchors, and missing portal destinations fail catalog construction.
Full portal geometry and entity validation remain the scenario validator's job.

`RegionCatalog::plan` takes a nonempty set of root regions, a portal-hop radius,
and the previous active set. Radius zero includes only the roots; higher radii
include the union of directed neighborhoods. Reverse links must be authored;
closed doors and runtime occupants do not change structural preloading. Multiple
links to one region are deduplicated. Cycles terminate, outputs are stable across
source ordering, and unknown root/active IDs reject the entire query.

The result describes transition candidates, not mutations. Future streaming must
include body/effect dependencies in its roots or otherwise pin them before
applying transitions at committed simulation boundaries. A region outside this
graph neighborhood is not automatically safe to freeze. No fixed runtime radius
has been selected by this authoring interface.

Catalog construction scales with structural metadata. Subsequent queries expand
only reached regions below the radius, with ordered-map lookup costs. Computing
transition sets also costs work proportional to the supplied active/required
sets. The catalog is backend-only; sending it to clients would disclose unseen
topology and is forbidden. Theme metadata does not constitute an asset palette
or reveal item appearance mappings.

## Verification and performance

The `region_horizon` Rust integration suite exercises all checked-in scenario
catalogs, directed cycles, multiple roots, anchor resolution, zone replacement
and inheritance, invalid inputs, and the actual authoring executable. A stable
operation-count regression adds unrelated regions up to 8,192 and checks that a
one-hop query still expands one region and examines one link.

```powershell
cargo test -p tor-server --test region_horizon --locked
cargo test -p tor-server --test region_horizon --release --locked
cargo run -p tor-server --example horizon-profile --release --locked
```

The `structural-horizon-v1` workload reports 10,000 queries after 100 warmups for
5, 256, and 8,192 structural regions. Each case preserves the same five-region
dungeon neighborhood and adds disconnected metadata. It reports catalog-build
time separately from query p50/p95/maximum and operation counts; file parsing,
JSON reporting, and correctness assertions are outside query timing. This is a
synthetic catalog scalability check, not support for runtime packages exceeding
the current 256-region limit. There is no earlier query implementation for a
before/after comparison; the small/large cases establish the initial baseline.

Initial Windows release measurements (2026-09-27, 10,000 samples per case):

| Structural regions | Catalog build | Query p50 | Query p95 | Query maximum |
| --- | --- | --- | --- | --- |
| 5 | 0.036 ms | 0.6 us | 0.6 us | 60.8 us |
| 256 | 0.291 ms | 0.6 us | 0.8 us | 41.0 us |
| 8,192 | 8.221 ms | 0.6 us | 0.7 us | 28.6 us |

All cases expand two regions and examine four directed links. These sub-microsecond
query measurements include timer overhead and host scheduling noise; they are
diagnostic, not timing assertions. The operation-count test is the stable scaling
gate.

The planner changes no formats or scenario certificates. Ordinary simulation, scheduling, persistence, and client behavior do
not use the planner yet.
