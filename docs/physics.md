# Bodies, portals, and gravity

Milestone 4c introduces rigid occupied-cell bodies, all 24 proper cube rotations,
gravity, drift, and collision hooks. Crouching, ducking, turning, torque, pushing,
bounce, and crushing are deferred. A body must fit without changing
posture. Its cell-offset representation leaves room for later posture changes.

## Bodies and coordinates

Actor `body` declarations contain `cells` (distinct integer offsets including
`[0,0,0]`), a required `eye` (one of those cells, where every sight line
starts), and positive `mass`. Limits are 64 cells and offsets from -8 through 8
on each axis. Mass uses game units; inventory weight does not contribute yet.
The default playable character occupies `[0,0,0]` and `[0,0,1]` and sees from
`eye = [0,0,1]`, with mass 80. Omitted bodies in diagnostic packages are single
cells that see from that cell. See [three-dimensional sight](sight-3d.md). Loose item stacks occupy
one cell and have mass equal to quantity; carried items follow their carrier.
Items remain nonblocking and can share cells, preserving pickup/drop semantics.

The reference cell and persistent geometric frame define a body's offsets.
Portals transform that frame, which also defines velocity, fractional displacement,
and the player's view axes. No gameplay facing or automatic upright rotation is
introduced. A character can emerge horizontally through a sideways portal.

Bodies may straddle regions. Every monotone topology route to an occupied offset
must agree on location and frame; self-overlap, incomplete apertures, obstruction,
or conflicting transforms reject placement. Translation checks all occupied cells.
A walking diagonal needs one ordering of its two component steps to carry the
whole body clear, as for a single cell (both, if open, must agree); other
simultaneous diagonal translations, such as falling while sliding, check all
component orderings. Closing a door,
placing a wall, and teleportation respect the full body.

## Gravity and deterministic time

Region `gravity = [0,0,-1]` is a local-axis acceleration. A sparse
`gravity_overrides = [{ at = [x,y,z], vector = [0,0,0] }]` replaces the region
field absolutely. Each field is zero or one signed axis with strength at most
1024. Diagnostic regions may omit gravity to retain static traversal; explicit
zero gravity enables unsupported-movement restrictions and preserves drift.

Resolve each occupied cell's field into the body's common frame, sum, then divide
by occupied-cell count. Equal opposing fields cancel; partial exposure weakens
acceleration; perpendicular contributions produce a diagonal. Do not normalize
the average to unit length. Uniformly distributed mass cancels out of acceleration.

One simulation tick is one integration quantum. Position uses 65,536 units per
cell, velocity uses those units per tick, and one authored gravity strength adds
16 units of velocity per tick. Signed division remainders preserve weak fields.
Semi-implicit integration updates velocity before displacement. Integer square
root and conservative scaling cap total speed at 8,192 units per tick (12.5 cells
per 100-tick ordinary turn), including diagonals. These are initial game tuning
values rather than a claim of SI physical realism.

Each crossed cell is tested. Fields are sampled again at the next quantum after
translation. At equal timestamps physics precedes actor decisions; actors resolve
in actor-ID order, followed by loose items in item-ID order. Simultaneous axis
ties use persistent body x, y, z order if a combined move must slide. Other actors
retain their normal recovery times. Physics never advances while waiting for
player input. Inert populations skip unused integration quanta; removed support
is reconsidered on the next scheduled physics quantum.

## Support, actions, and impacts

An obstructed occupied cell can support the rigid body. Ordinary walking in a
gravity-enabled region requires support against at least one nonzero component
of the resultant field. Unsupported bodies can wait and manipulate inventory,
but cannot walk through air. Jumping, flight, and zero-gravity push-off controls
are not implemented. Walking preserves existing velocity. Explicit stairs remain
separate traversal actions and are never gravity conduits.

Collision removes only the obstructed velocity component and its fractional
displacement; tangential components survive. Resting support suppresses acceleration
into the obstruction without repeated impact events. There is no balance or torque:
a small ledge can support a large body. Stable ID ordering resolves competing
bodies, without simultaneous swaps or momentum transfer.

Backend impact records contain entity, tick, reference location, collision axis,
region-local normal, other actor when applicable, incoming normal velocity, and
mass. Ordinary gravity acceleration and portal coordinate changes never cause
impacts. Milestone 4d applies impact damage at contact, through the same HP,
resistance, interruption, and death rules used by combat. See
[dungeon gameplay](dungeon.md) for the threshold and typed-damage contract.

Dropped items inherit the carrier's motion and frame. Pickup absorbs that motion
without transferring momentum to the carrier. Moving stacks merge only when
their motion is compatible. Forced displacement or collision stops automatic
travel at its next action boundary. Routes still use learned reference-cell
topology; every actual step validates the complete current body.

## Authoring and disclosure

Use `kind = "portal"` for physical z-facing apertures; omitted kind retains the
existing vertical stair convention. Use `rotation = 0..23` for a cube transform,
or `turns = 0..3` for planar quarter turns. Nonzero `turns` cannot accompany
`rotation`. Transform 23 maps `(x,y,z)` to `(z,y,-x)`; transforms 0–3 retain the
original z-axis quarter turns. The complete stable table is in
[`rotation.rs`](../crates/world/src/rotation.rs). Reflections are excluded.

`scenarios/tests/physics` is a two-cell falling/landing fixture;
`scenarios/tests/physics-portal` joins a zero-gravity approach to a sideways shaft.
Actor and character declarations accept an initial fixed-point `velocity`.
Actor archetypes accept `body`, with an instance body replacing the archetype.
All these packages use the ordinary offline validator and normal startup path.

Wizard commands include `gravity <region> <gx> <gy> <gz>`,
`cell-gravity <region> <x> <y> <z> <gx> <gy> <gz>`, and
`velocity <actor> <vx> <vy> <vz>`. Structured wizard JSON additionally supports
`set_body` (with `cells`, `eye` and `mass`) and `connect_portal`. These remain privileged, journaled operations;
they permanently mark wizard lineage and participate in rewind.

Every observer uses [three-dimensional sight](sight-3d.md) from its eye cell.
The body frame carries the eye's axes across any portal the body straddles.
The backend resolves rotated occurrences and height slices; clients never
reconstruct topology. Abstract stair landings occupy
separate panels beyond physical sight slices. Only visible occupied actor cells
are disclosed, possibly with the same actor ID at several positions. Current
own-body motion and collision sensations are disclosed without gravity maps,
hidden bodies, or collision-target identities. Both playable clients use shared
motion narration; ASCII draws disclosed height panels.

Physics state is part of the current save format; older prerelease saves are
rejected. Gravity tables share world-geometry storage across actions and rewind boundaries.
Checkpoints retain body frames,
velocity, displacement, gravity, integration remainders, and boundary sensations.
Retries do not integrate twice; journal replay, restart, and rewind restore the
same state. Region streaming remains 4e work; all authored regions remain active.

## Verification and profiling

Behavior tests cover cancellation/aggregation, drift, support, low passages,
sideways and straddling crossings, item motion, concurrent actors, exact rotation
composition, save/checkpoint replay, idempotent retry, privileged edits, and rewind.
`scripts/test_physics_process.py` launches real text and native ASCII clients for
falling, landing, body-cell presentation, and saved continuation.

`cargo run --release --locked -p tor-server --example physics_bench` runs physics
workload v1: resting/falling populations, 1/8 actors, 1/128 items, and 2/8 body
cells, including client application/drawing, durable save barriers, and restart.
It records samples, integration/body-resolution counts, and disclosed/save bytes.
Existing latency workload versions remain unchanged for before/after comparison.
Only changed observer states are applied/drawn, matching live stream delivery;
client sample counts can therefore be lower than command counts in multi-actor
cases. Results are under [performance](#performance).

ASCII F6/F7 browse disclosed height slices without advancing time; spectators can use them too. Mouse selection follows the displayed slice.

## Performance

Measured 2026-09-27 in Windows release builds, sequentially on the same machine with other builds and tests stopped. The baseline is the preserved 4b release executable.

Existing workload versions and traces are unchanged: `r8-a1-h100-memory --quick --cycles 5` and `r64-a8-h100-memory --cycles 3`. These measure successful mixed-trace engine command calls, not native input-to-presentation latency; intentionally blocked probes retain separate labels in the raw reports.

| Case | Build | Commands | p50 ms | p95 ms | Maximum ms |
|---|---|---:|---:|---:|---:|
| small | 4b baseline | 305 | 0.316 | 0.831 | 1.440 |
| small | 4c | 305 | 0.353 | 0.968 | 1.779 |
| large | 4b baseline | 1503 | 0.013 | 2.175 | 3.374 |
| large | 4c | 1503 | 0.016 | 3.450 | 5.087 |

The large-case perception cost increased with body/frame-aware observation. Range indexing removes repeated surface scans, identity transforms have direct paths, and resting/same-tick waits avoid scene rebuilds. Both existing command cases remain below the provisional 8 ms p95 / 33 ms maximum targets; this does not claim 3p closure. The large comparison retains exactly 5,912 scene/perception calls and 1,503 simulation transitions in each build; memory cases write no journal bytes.

Client application and canvas rendering are timed separately (4c p95, small/large): 0.194/0.178 ms application, and 1.088/1.137 ms rendering. These exclude transport and native presentation.

Physics workload v1 uses three samples per row, eight turns per actor, 1/8 actors, 1/128 loose items, and 2/8 body cells. It uses SQLite saves with checkpoint interval eight, explicit durable barriers, and exact restart comparison.

| Actors / items / body cells | Falling | Commands | p50 ms | p95 ms | Maximum ms |
|---|---|---:|---:|---:|---:|
| 1 / 1 / 2 | no | 24 | 0.025 | 0.053 | 0.100 |
| 1 / 1 / 2 | yes | 24 | 0.102 | 4.469 | 7.174 |
| 1 / 1 / 8 | no | 24 | 0.030 | 0.052 | 0.063 |
| 1 / 1 / 8 | yes | 24 | 0.039 | 2.823 | 3.097 |
| 8 / 128 / 2 | no | 192 | 0.018 | 0.168 | 0.349 |
| 8 / 128 / 2 | yes | 192 | 0.017 | 22.534 | 26.537 |
| 8 / 128 / 8 | no | 192 | 0.018 | 0.206 | 0.218 |
| 8 / 128 / 8 | yes | 192 | 0.018 | 22.517 | 29.098 |

Dense eight-actor falling remains above the 8 ms p95 target. Its cost is concentrated in updates that advance physics and refresh all actors' observations; resting waits have no integration steps. This stress-case gap remains in deferred 3p, without changing the target.

| Actors / items / cells | Falling | Physics steps | Body-cell requests | Scene calls | Disclosed bytes | Saved bytes | Save p95 ms | Resume p95 ms |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| 1 / 1 / 2 | no | 0 | 232 | 8 | 77209 | 155648 | 73.712 | 21.320 |
| 1 / 1 / 2 | yes | 478 | 975 | 16 | 77209 | 798720 | 145.177 | 86.648 |
| 1 / 1 / 8 | no | 0 | 712 | 8 | 77845 | 159744 | 55.614 | 13.585 |
| 1 / 1 / 8 | yes | 478 | 2991 | 16 | 77845 | 798720 | 90.520 | 64.230 |
| 8 / 128 / 2 | no | 0 | 11392 | 64 | 84661 | 1536000 | 236.311 | 199.071 |
| 8 / 128 / 2 | yes | 32504 | 51320 | 160 | 84661 | 8462336 | 1241.604 | 844.305 |
| 8 / 128 / 8 | no | 0 | 17920 | 64 | 86441 | 1560576 | 135.353 | 181.485 |
| 8 / 128 / 8 | yes | 32504 | 111488 | 160 | 86441 | 8486912 | 1038.318 | 797.075 |

Counts and bytes are per sample, including harness disclosure reads outside timed commands. Save/resume distributions have only three samples each and remain diagnostic. Samples do not establish 10,000-action history scaling, network latency, or native responsiveness targets.

Raw samples are kept outside the repository. Reproduce physics reports with `python scripts/physics_performance_report.py <physics-jsonl>`.
