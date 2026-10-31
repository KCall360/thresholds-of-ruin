# Scripted model review

Run the built 3D client with a script and a **new** output directory:

```powershell
.local/target-3d/release/tor-client-3d.exe --scene-script crates/client-3d/review-scripts/wardrobe-motion.json --capture .local/reviews/wardrobe-001
```

This offline mode uses the real procedural generator, renderer and animation
systems. It does not connect to a game server. Other client/review arguments
cannot be combined with `--scene-script`, except `--capture` and the optional
`--offscreen` switch described below. Existing output directories are
rejected so review evidence is not overwritten.

Scripts are strict JSON with `version: 1`. Unknown fields fail validation.
The included example shows multiple compositions, three camera angles, a jump,
walking and a mid-animation equipment change. Each actor has an exact model name
(e.g. `Human`, `Mage`, `RedMage`), an explicit seed and a body object. Omitted body
fields use the documented `Variant` defaults; they are not randomly sampled.
Body values and resolved composition identities are expanded in the manifest.

Each camera specifies position, target and perspective field of view in degrees.
World Y is up. Actor yaw is in degrees. The initial stage has a fixed neutral
ground and directional/ambient lighting. Camera projection customization and
scene lighting controls remain planned.

Events must be ordered by integer `tick`. Operations for an actor at the same
tick run in source order. Movement reaches that timestamp using the previous
velocity; boundary events affect the following interval. A teleport is visible at
its exact requested position at that timestamp, with no extra movement step:

- `action`: `name` is `jump`, `strike`, `thrust`, `heavy`, `focus`, `gesture`, or
  `cancel`. A named action starts/restarts at phase zero at its timestamp;
  the following interval advances it. Action starts require tick 1 or later;
  tick zero is the generated rest pose. That pose does not yet present the
  actor's authored `yaw` (the manifest still records it), so capture turned
  actors at tick 1 or later.
- `velocity`: `value` is a world-space vector in units per second. It persists
  until changed; use `[0, 0, 0]` to stop. Root displacement drives the usual gait.
- `teleport`: `position` and `yaw` place the actor and reset animation state.
  Existing velocity is retained.
- `dress`: `outfit` selects a wardrobe while retaining explicit equipment.
  A capture after a `dress` or `equipment` change waits only until every mesh the
  scene now draws has reached the renderer; meshes the old outfit owned, such as
  a cloak, are released and not awaited.
- `equipment`: `items` replaces the explicit loadout while retaining wardrobe.
  Supported pieces are `helm`, `boots`, `cloak`, and one body armour:
  `leather`, `studded-leather`, `ring-mail`, `scale-mail`, `chain-mail`,
  `splint-mail`, `banded-mail` or `breastplate` (plate mail's torso). An empty list
  restores the wardrobe's own pieces. Unsupported anatomy is rejected.

Capture records name a tick and camera. Names contain only ASCII letters,
numbers, underscores or hyphens and must be unique ignoring ASCII case (so
`front` and `FRONT` cannot overwrite the same file on Windows). Authored spelling
is preserved in filenames and manifests. Records are ordered by tick;
multiple cameras at one tick hold exactly the same actor state. The timeline
advances only between ticks, never while changing cameras or waiting for capture
readback. One step is `1 / hz` seconds, with rates from 20 through 240. Rates that
do not divide a second exactly use a duration rounded to nanoseconds.

Successful runs write the original `script.json`, named PNG images, and a
`manifest.json` with status `complete`. Capture records include the tick, clock,
expanded composition, root and selected joint transforms, action phase, camera,
renderer details, image dimensions and labeled FNV-1a fingerprints. These hashes
are consistency checks, not cryptographic provenance. Source revision, dirty
input hash and executable digest are not yet recorded. A failed or interrupted
run must not be treated as complete. An overall 180-second watchdog bounds scene
preparation and capture work; it may reject long runs on slow renderers.

Replay guarantees currently target resolved state on the same executable;
cross-driver pixel identity is not promised. Full joint/cloth diagnostics,
orthographic cameras, action-at-zero initialization, richer motion tracks and
automatic seeded fixture generation remain unfinished. See the implementation
and future contracts in [the art continuation guide](../../../docs/plans/3d-art-continuation.md).

## Windowless capture

Add `--offscreen` to render the same scene into an image without opening a window
or startup splash. This option is only valid with `--scene-script` and `--capture`;
it is not a headless live-game client or a live-playlist mode. Omitting it retains
the existing windowed review behavior.

Build from the repository root, then run with a new output directory:

```sh
cargo build --manifest-path crates/client-3d/Cargo.toml --bin tor-client-3d --locked
crates/client-3d/target/debug/tor-client-3d --scene-script crates/client-3d/review-scripts/showcase/02-adventurers.json --capture .local/reviews/offscreen-001 --offscreen
```

The executable path above assumes the default target directory; use your configured
`CARGO_TARGET_DIR` instead if set. On Windows, add `.exe` to the executable name.
No display server is needed, but a working graphics adapter and the normal native
build dependencies still are. On Linux, Mesa's Vulkan software driver (Debian's
`mesa-vulkan-drivers`) can provide a CPU adapter when no hardware GPU is available.
`WGPU_BACKEND=vulkan` selects Vulkan explicitly; `vulkaninfo --summary` (from
`vulkan-tools`) can inspect its available adapters. Install packages through your
platform's normal package manager; this mode does not install drivers itself.

The camera and PNG readback share an offscreen target at the script's exact
viewport dimensions, including widths that are not aligned to GPU row padding.
The existing readiness checks, deterministic timeline and capture watchdog apply.
The manifest's `capture_mode` is `offscreen` (or `window` for windowed runs), and
its renderer details identify the actual adapter. A successful process must exit
zero and leave `manifest.json` with status `complete`; partial output is not proof
of a successful run. The in-app 180-second watchdog runs once the application is
updating, so an automated caller should also impose an outer process timeout
(for example 240 seconds) to cover graphics-driver initialization hangs.

Software-rendered captures can validate scene construction and image output, not
hardware GPU performance, native window presentation, mouse/keyboard input, audio,
or cross-driver pixel identity. Native client tests remain separate. Linux VM
validation produced inspected 960×540 and odd-sized 641×479 PNGs on Mesa llvmpipe,
with identical repeated same-state captures; this is not a full platform matrix
or a performance measurement.

## Looping showcase

From the isolated checkout, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File crates/client-3d/review-scripts/showcase-loop.ps1
```

Use `-Cycles 1` for one pass. The default repeats internally in one persistent
window until Escape or window close. Each of the four chapters lasts 40 seconds,
with 8–10 second camera holds and four-second eased transitions. Actors switch
at chapter boundaries; the camera continues smoothly. Compositions preload once
and are reused on each loop. Startup can therefore take longer than one scene.

The cast includes both princess outfits, adventurers, arcane characters and a
creature finale. Walking, gestures, strikes, thrusts, heavy attacks, focusing and
jumps run on wall-clock time. The live inputs are in `live-showcase/`, selected by
`live-showcase.json`. The original `showcase/` capture scripts remain available.

Direct invocation:

```powershell
.local/target-3d/release/tor-client-3d.exe --live-playlist crates/client-3d/review-scripts/live-showcase.json --cycles 1 --diagnostics .local/live-timing.json
```

The diagnostics path must be new and its parent must exist. The wrapper creates
unique run directories under `.local/reviews/showcase/`, preserving previous
results. Close normally to write the final report; forcibly terminating the
process can leave only its initial status. Optional `--proof-frame NEW.png` takes
one asynchronous screenshot without stopping the timeline.

A playlist has `version: 1` and `scenes`, each containing a relative `script` path
and ordered `shots`. Every shot names a scene camera and supplies positive
`hold_seconds` and `transition_seconds`. The last shot transitions to the next
chapter's first camera, including at the loop boundary. All chapters share one
viewport. Scene events retain their tick/hz timestamps and must occur before the
chapter ends (at most 600 seconds). Live scene files may omit `captures`; supplied
capture entries are ignored and do not limit the live timeline. Deterministic
capture still requires ordered captures within its 120-second bound, with events
no later than the final capture.

The title shows average main-frame time and frames exceeding 50 ms. The final
JSON includes main-update and render-cleanup interval distributions, animation
work, long frames, discarded stall time, scene entries, final actor positions/outfits
and composition counts.
These are CPU measurements, not GPU timings or confirmed display drops.
Animation substeps are capped at 50 ms; wall stalls over 250 ms are bounded and
the discarded time is reported. Timing excludes initial renderer warmup.

Deterministic testing still uses `--scene-script PATH --capture NEW_DIRECTORY`.
It advances fixed simulation ticks and holds the pose for screenshot readback;
use live playback when judging animation pacing.


## Yeti surface coat

`yeti-coat.json` captures front, side and rear views at refreshed rest, walking
and a heavy action. It reviews body-coat overlap and silhouette through the
same posed sequence. Native acceptance checks presented bodies and held-camera
poses; emitted-mesh tests own closed topology, embedded roots, surface flow,
mesh budgets and warm-cache reuse. Visual review remains necessary for art quality.

## Centaur rider builds

`centaur-builds.json` captures both sexes across slender, average and sturdy
builds at refreshed rest and during walking. It reviews the shared organic arm
surfaces, moving shoulder caps and spear grip with fixed cameras. Native
acceptance checks presentation, motion, build-dependent shoulder spacing and
preserved reach; mesh tests own arm volume and bent-joint coverage.

## Equipment conflicts and bounded sampling

`equipment-conflicts.json` reviews four fixed conflicts in one native process:
long hair with a breastplate, a princess crown under a helm, a mage robe with a
cloak, and a red mage robe with boots. It captures each model while equipped,
after removing the items during a jump, and after restoring them. Run it with a
new output directory:

```powershell
.local/target-3d/release/tor-client-3d.exe --scene-script crates/client-3d/review-scripts/equipment-conflicts.json --capture .local/reviews/equipment-001
```

The generated-model test uses 52 permanent witnesses and 32 seeded samples rather
than multiplying every body, outfit and item combination. It includes independent
equipment selection, valid sex-specific outfits, and an all-slot conflict. The
samples check generated parts, finite geometry and selected coverage rules; they
are not exhaustive collision or animation qualification.

Each failure log records the batch seed, case index and complete composition.
Replay one case against the same source revision (the plan can change across
revisions):

```powershell
$env:TOR_COMPOSITION_SEED = "1592642302"
$env:TOR_COMPOSITION_CASE = "47"
cargo test --manifest-path crates/client-3d/Cargo.toml --release --lib focused_and_seeded_compositions_generate_their_declared_parts -- --nocapture
Remove-Item Env:TOR_COMPOSITION_CASE
Remove-Item Env:TOR_COMPOSITION_SEED
```

Omit `TOR_COMPOSITION_CASE` for the entire bounded plan; change only the seed to
rotate the random tail while preserving the fixed conflicts. Invalid case indices
fail instead of silently running no cases. Use the standard Windows build settings
from the development guide and retain the command output with the tested revision.
The native process acceptance test also checks camera-independent state,
equip/unequip/re-equip identities, animated joints and complete PNG output.
