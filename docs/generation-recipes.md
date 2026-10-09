# Generation recipes and groups

The exploratory Rogue package uses one generation group per floor. Each group
owns nine independently streamed regions. Generation prepares a complete floor;
publishing it creates ordinary detached region records in stable member order.
Only the settled loading horizon and simulation pins attach members. Membership
does not extend either horizon or keep terrain resident.

Recipes are versioned, typed declarations. Version 1 has six ordered, uniquely
named stages: stone fill, grid partition, rooms, connected graph, corridors, and
stairs. Version 2 appends room lighting with configurable `darkness_roll`
(1–10,000) and `darkness_start` (depth offset). Earlier geometry stages keep their
version-1 random streams. Stage streams derive from the game seed, group identity, stage identity
and version, and semantic inputs. Generation performs no I/O and allocates no
runtime record identities. Worker scheduling does not affect generated output.

Each floor has nine 26×7×2 regions in a 3×3 grid. Rooms retain a one-cell slot
margin. A random spanning tree connects orthogonal neighbors, with up to two
extra edges. Corridors have one-cell width and two-cell clearance. Full reciprocal
boundary joins preserve stone: carving determines where bodies can cross.

## Preparation and publication

The ordinary preparation horizon requests a group when any member enters it.
One worker deduplicates requests and prioritizes demand. The 32-record preparation
budget charges nine records per floor. Obsolete speculative results are dropped;
demand can displace speculation. A demand waits for its specific result. Worker
startup failure and recovery use the same deterministic synchronous generator.

Prepared output remains speculative until a member enters the actual loading
horizon at a candidate transition. All nine validated records publish together.
Generated records and exact source dependencies persist in the journal transaction,
including transactions without a checkpoint. Ordinary lifecycle records are the
only terrain state; there is no original generated-terrain store.

## Named stair destinations

A group-generated stair stores its destination region and anchor name. It is
visible and contributes its structural edge before destination coordinates exist.
Preparing the current floor reads only its own nine sources; it does not inspect
or partially generate the next floor.

Publication of the destination floor registers its immutable named anchor
coordinates. Movement resolves the logical reference through that structural
directory, while ordinary body clearance, occupancy, and movement timing decide
whether traversal succeeds. Undisclosed destination terrain remains private.
Coordinates survive region detachment, so resolving a destination does not require
rewriting, attaching, or retaining the source floor. The source's named link travels
with its ordinary region record.

The `rogue-exploration` package contains 26 reversible floors, starts on floor 1,
and has no upward exit on floor 1 or downward stair on floor 26. Room lighting uses `darkness_roll = 10` and `darkness_start = 1`: a room is dark
when a zero-based roll is below depth minus one. Corridors outside room footprints
remain dark. Lit rooms include both open body-height cells and their enclosing
solid surfaces. A lighting-only edit does not change corridors or stair positions.
Creatures, items, survival mechanics, and victory are reserved for later increments.

## Verification and diagnostics

Generation validates connected carving, a solid exterior, reciprocal face mappings,
and two-cell clearance before accepting a floor. Tests cover independent region
eviction with terrain edits, journal-only and checkpoint recovery, all 26 floors
in both directions, generation rewind within the ordinary retained window, and
authored entrances linked to generated anchors. Actual clients exercise headless
exploration, adventure text, native ASCII stairs, spectators, and reconnects.
Synchronous demand, settled preparation, and racing preparation produce identical
observations, lifecycle membership, and saved region rows, with journal-only and
per-command checkpoint saves. Player and spectator process responses keep
generation metadata and undisclosed floor identifiers private.

Run `cargo run -p tor-server --release --example rogue_bench -- 5` for JSONL
diagnostics. The workload measures pure generation of all floors, then the same
first-floor expedition with synchronous demand, settled background preparation,
and racing background preparation, each in memory and with durable storage.
Acquisition profiles separate group waiting and synchronous generation inside the
ordinary transition timing. A committed group costs exactly nine records; the
preparation budget remains 32 record equivalents. Residency follows ordinary
horizons and pins, including both ends of a stair. Timing samples are diagnostic;
the report validator and process test enforce completeness and work-count bounds.

The lighting release diagnostic used machine fingerprint `6a1878811f37`
(Core i7-9750H, Windows 11, NTFS HDD), five seeds and 130 floor generations.
Pure generation measured 0.592/1.269/3.499 ms p50/p95/max. The complete
range-16 expedition, including cold acquisition, measured:

| Acquisition mode | Storage | n | p50 ms | p95 ms | max ms |
| --- | --- | ---: | ---: | ---: | ---: |
| Synchronous demand | Memory | 348 | 3.135 | 6.107 | 21.920 |
| Synchronous demand | Durable | 348 | 3.095 | 6.224 | 15.903 |
| Settled preparation | Memory | 348 | 3.065 | 5.621 | 9.688 |
| Settled preparation | Durable | 348 | 3.143 | 6.438 | 12.787 |
| Racing preparation | Memory | 348 | 3.063 | 5.465 | 9.523 |
| Racing preparation | Durable | 348 | 3.105 | 6.314 | 11.165 |

Every expedition committed exactly one additional nine-record group. Maximum
loaded residency was 11 regions, maximum resident record count was 26, and
encoded observations ranged from 12,499 to 100,880 bytes. Cold demand is
included in the action distributions. Racing preparation completed before
demand in these runs.
These are current-workload diagnostics, not a matched prior-version comparison;
ordinary interleaved release comparisons are described in the
[performance harness](performance-harness.md#lighting-and-range-diagnostics).
Raw measurements remain local; release asset publication requires separate
authorization.

Durable flush barriers, including queued journal and region-row persistence,
measured the following diagnostics after each expedition:

| Acquisition mode | n | p50 ms | p95 ms | max ms |
| --- | ---: | ---: | ---: | ---: |
| Synchronous demand | 5 | 120.777 | 125.409 | 125.409 |
| Settled preparation | 5 | 117.923 | 261.244 | 261.244 |
| Racing preparation | 5 | 120.721 | 163.750 | 163.750 |

These short flush distributions do not resolve the existing save-tail findings.
