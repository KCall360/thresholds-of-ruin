# Generation recipes and groups

The exploratory Rogue package uses one generation group per floor. Each group
owns nine independently streamed regions. Generation prepares a complete floor;
publishing it creates ordinary detached region records in stable member order.
Only the settled loading horizon and simulation pins attach members. Membership
does not extend either horizon or keep terrain resident.

Recipes are versioned, typed declarations with six ordered, uniquely named
stages: stone fill, grid partition, rooms, connected graph, corridors, and
stairs. Stage streams derive from the game seed, group identity, stage identity
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
and has no upward exit on floor 1 or downward stair on floor 26. Creatures, items,
survival mechanics, and victory are reserved for later content increments.

## Verification and diagnostics

Generation validates connected carving, a solid exterior, reciprocal face mappings,
and two-cell clearance before accepting a floor. Tests cover independent region
eviction with terrain edits, journal-only and checkpoint recovery, all 26 floors
in both directions, generation rewind within the ordinary retained window, and
authored entrances linked to generated anchors. Actual clients exercise headless
exploration, adventure text, native ASCII stairs, spectators, and reconnects.

Run `cargo run -p tor-server --release --example rogue_bench -- 5` for JSONL
diagnostics. The workload measures pure generation of all floors, then the same
first-floor expedition with synchronous demand, settled background preparation,
and racing background preparation, each in memory and with durable storage.
Acquisition profiles separate group waiting and synchronous generation inside the
ordinary transition timing. A committed group costs exactly nine records; the
preparation budget remains 32 record equivalents. Residency follows ordinary
horizons and pins, including both ends of a stair. Timing samples are diagnostic;
the report validator and process test enforce completeness and work-count bounds.

The Windows release diagnostic run used machine fingerprint `6a1878811f37`
(Core i7-9750H, Windows 11, NTFS HDD) and five seeds (130 floor generations)
and five expeditions per acquisition/storage mode. Pure generation measured
0.549/0.685/1.900 ms p50/p95/max. The complete transition publishing a demanded
floor measured 6.886/8.091/8.091 ms in memory and 6.894/8.210/8.210 ms with durable
storage. With settled preparation those transitions measured
0.436/1.419/1.419 ms and 0.467/1.456/1.456 ms, respectively; each distribution has
five samples. Prepared acquisition itself measured 0.006 ms median, with no
synchronous floor build. Racing preparation also completed before demand in
these runs. Every expedition published one additional nine-record group; maximum
loaded residency was nine and maximum resident record count was 27.

The ordinary streaming comparison used two interleaved rounds, 280 commands per
side and storage mode, against the paired-stairs baseline. Memory command time
changed from 0.230/0.382/0.554 to 0.237/0.396/0.588 ms p50/p95/max. Durable command
time changed from 0.247/0.474/1.241 to 0.253/0.481/1.156 ms. Deterministic work
counts and observation bytes matched, and no runs failed. These short-run timing
observations are diagnostic; they are not
acceptance thresholds or published headline measurements. Raw samples remain
local pending authorization to publish release assets.

Durable flush barriers, including queued journal and region-row persistence,
measured the following diagnostics after each expedition:

| Acquisition mode | n | p50 ms | p95 ms | max ms |
| --- | ---: | ---: | ---: | ---: |
| Synchronous demand | 5 | 112.984 | 415.784 | 415.784 |
| Settled preparation | 5 | 112.466 | 147.399 | 147.399 |
| Racing preparation | 5 | 145.783 | 188.322 | 188.322 |
