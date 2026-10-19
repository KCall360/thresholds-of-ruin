# tor-client-3d (prototype)

A Bevy 0.16 client that draws the character's disclosed knowledge of the dungeon
with procedural scenery, creatures, and items. Its gameplay scope remains the
prototype's exploration, combat, and basic item interactions. It has its own
workspace, lockfile, tests, and verification gate.

Live play currently omits disclosed terrain two or more levels above the
character's feet, keeping the overhead view open. Nearby walls retain their
camera cutaway. There is no ceiling display toggle.

Windows defaults to Direct3D 12 for live play and both offline showcases.
All three modes use single-sample rendering and Bevy's built-in FXAA.
New shader pipelines compile before drawing, so a ready scene does not briefly
show only its HUD while world geometry is missing. The first use of a shader
variant can pause that frame while compilation completes.
On CPU Direct3D 12 adapters, rendering waits for each submitted frame to finish
before queuing another, bounding the software GPU's outstanding resources while
retaining the same models, materials, shadows, and resolution.
Other platforms retain Bevy's renderer defaults. The standard `WGPU_BACKEND`
environment variable can select another backend explicitly. Capture diagnostics
identify the actual adapter, backend and driver; verification receipts also
record renderer and compiler configuration without session credentials.

For faster Windows shader compilation, an optional matching `dxcompiler.dll`
and `dxil.dll` pair can be placed beside the actual client executable. The client
selects that pair by absolute path, independently of the launcher's working
directory. Without the pair it uses the built-in FXC compiler. Explicit
`WGPU_DX12_COMPILER` choices remain authoritative; `WGPU_DX12_COMPILER=fxc`
selects FXC even when the pair is present. Startup logs report the requested
compiler and selected paths. `RUST_LOG=info,wgpu_hal::dx12::instance=debug`
also reports which compiler the native renderer loaded.

## Run

Build the release server once from the repository root, then run the client:

```
cargo build --release -p tor-server
cargo run --release --manifest-path crates/client-3d/Cargo.toml -- --seed 42
```

Use an optimized client for art review and launch-time comparisons; procedural
sculpting is substantially more expensive in an unoptimized development build.
Offline review modes run without a server. Create the destination directory
before using `--capture DIR`; the screenshot writer expects it to exist.

With no `--connect`, the client starts a private `tor-server` (default scenario
`scenarios/rogue-exploration`) with a fresh save under `.local/client-3d/saves/`
and stops it on exit. Other options:

- `--scenario DIR`, `--save FILE`, `--save-root DIR`, `--seed N`, `--server-bin EXE`
- `--connect 127.0.0.1:PORT [--actor 1] [--observe]` joins a running server
  (token from `TOR_SERVER_TOKEN`)
- `--bestiary` shows every creature, one row at a time (people, beasts, the
  Rogue folk and the Rogue beasts), walking, then performing all nineteen moves
  on a schedule, then a close-up of each at rest and in its signature move
  (with `--capture DIR`, named screenshots of each moment); `--portraits`
  skips straight to the close-ups; `--idles` shows everyone standing at rest;
  `--antics` shows them amusing themselves as if the player had left;
  `--photos` (with `--capture DIR`) takes one picture of each creature alone,
  turned three-quarters to the camera
- `--bored` starts a game as though the player had been away nearly a minute
- `--appearance woman|man[:N]` chooses the offline bestiary's Warrior preview; `N`
  picks among that sex's hair, skin and build
- `--azimuth DEGREES` orbits offline bestiary/gallery cameras relative to their
  usual view: `90` and `270` show the flanks, `180` the rear. It applies to
  portraits, photos, lineups and the prop tour; live gameplay retains Q/E.
  The stage wall cuts away when needed for the selected review angle.
- `--capture-warmup SECS` delays the offline bestiary timeline, including lineups,
  by 0–60 seconds while the renderer warms up; `12` helps when early captures
  are blank
- `--capture-every SECS` also works in the offline bestiary for time-stamped
  motion frames; `--quit-after SECS` bounds its timeline after warm-up
- `--travel-study` runs the four 40-second offline groups through slow curves, a
  3.2-cell/s shuttle, sharp turns, abrupt stops/restarts, and action, hurt, dodge
  and death interruptions. Everyone shares an offset within 0.9 cells of home.
  Try `--travel-study --capture DIR --capture-warmup 12 --capture-every 0.1`,
  then repeat with `--azimuth 90` to inspect feet and profiles. The study clock
  slows during render stalls to preserve the animation's time step; capture
  timestamps and `--quit-after` use that clock. It exits after 160 study seconds
  and pending captures finish. This mode cannot combine with gallery, lineups,
  idles/antics or portraits/photos.
- `--lineups` (with `--capture DIR`) shows five individuals of each creature
  side by side, women and men alternating among people, with close-ups of
  the first two individuals. `--family NAME[,NAME...]` limits the line-ups to
  the named creatures, as the bestiary labels them (for example
  `--lineups --family "dragon,venus flytrap"`).
  Lineup capture deadlines use real time and wait for each GPU readback.
  Slow frames stop at each camera and family boundary so no view is skipped;
  creature animation retains its capped timestep.
- `--fallen-lineups` shows the same five generated individuals in settled
  death poses, with a full lineup and two close views framed around the fallen
  bodies, clothing and held props. For manual corpse review, use
  `--fallen-lineups --capture DIR --capture-warmup 12`; repeat with
  `--azimuth 90` for another angle. Captures have `fallen-` in their names.
  This mode cannot combine with gallery, travel-study, idles/antics or
  portraits/photos.
- `scenarios/menagerie` (in this crate) is a lit hall holding its supported
  creature recipes, wandering at peace except an emu and a snake that fight, with
  items on the floor: `--scenario crates/client-3d/scenarios/menagerie`
- `TOR_3D_CURSORS=DIR` saves the cursor icons, enlarged, for inspection
- `TOR_3D_PROFILE_BUILD=1` enables local startup timings for model cache misses
  and scene construction phases, plus live/idle model-cache payload and evictions;
  ordinary runs keep this profiling disabled
- `--demo-hunt` explores and fights whatever it finds
- `--gallery` shows every prop on a stretch of cave floor, no server needed
- `--demo-interact` runs a scripted pass over pick up, drop, equip, remove,
  eat, travel and doors against the real server
- `--demo` wanders automatically; `--capture DIR` saves a screenshot every four
  seconds; `--quit-after SECS` exits cleanly; `--distance N` sets the camera distance; `TOR_3D_DEBUG=1` logs views

Capture waits for each GPU readback before requesting another image, so a slow
renderer cannot accumulate screenshot targets and transfer buffers. Frame
reports include the number of captures in flight at submission. A readback that
does not finish by the existing quit deadline still produces a failure. Capture
intervals and `--quit-after` use elapsed real time, so animation-clock clamping
does not stretch these timers on a slow renderer.

## Missing models

Gameplay selects procedural recipes from disclosed, palette-approved asset IDs.
It tries the specific ID and then its dotted parents: a new
`creature.equine.authored` can use the generic equine recipe. Display
names do not select anatomy, equipment, or item identity.

When no creature family is supported, a neutral, visible marker stands in for
the model. Its disclosed name, description, and interactions remain available.
Items fall back to their disclosed class (a potion bottle, scroll, weapon, and
so on); an unclassified item uses a neutral object marker. Unidentified items
use only their disclosed appearance asset and class. Adding scenario content
therefore does not require adding a matching 3D model.

## Session ownership

Default launches atomically reserve a new directory under the save root and
retain previous sessions, including when launches share a clock timestamp.
`--save-root DIR` changes that root; `--save FILE` explicitly selects a saved
game. Each private backend receives separate owner and read-only spectator
credentials through its environment. Credentials are not written to logs,
save directories, or command-line arguments.

The client owns only the backend it starts. Startup errors, normal exit, and
unwinding stop and reap that process. Joining an existing server leaves its
process lifetime with its owner. Quitting uses the shared client's durable
save barrier before stopping a private backend; failed or unconfirmed closure
returns a failing process status.

Startup applies every handshake and control-acquisition message to shared state,
then presents one settled initial view. Status explanations follow that view
before input becomes ready; normal gameplay updates retain their original order.

The worker admits one command at a time. Its event queue holds at most 64
notifications, preserves their order, and applies at most one view per frame
so deferred scene entities are available before the next reconciliation.
Commands retain the shared input context from the view used to interpret them.
The worker refuses obsolete stream, branch, cursor, readiness, or observation
contexts instead of restamping them with newer authority. Stream and branch
resets clear local errands, inventory selection, old narration, and transient
state cues.

## Independent maintenance

The [continuation plan](../../docs/plans/3d-art-continuation.md) records the
implemented art foundations, reviewed behavior, remaining challenges and next
shared modeling work. Art targets do not establish runtime equipment support.

Run its checks explicitly from the repository root:

```
python crates/client-3d/scripts/verify.py --profile debug
python crates/client-3d/scripts/verify.py --profile release
```

The gate includes model tests, procedural geometry tests, matching server/client
builds, and native acceptance through a real window and OS keyboard input.
Linux needs a display (CI uses Xvfb) and the native libraries listed in the
dedicated `client-3d.yml` workflow. On Ubuntu this includes
`libxkbcommon-x11-0` for X11 keyboard support; see
[Bevy's Linux dependencies](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).
Logs and source/toolchain receipts stay under `.local/verify-3d/`.
Failed native processes retain their bounded diagnostics in that run's
`process-failures/` directory, which the independent workflow uploads with its logs.

Core verification deliberately excludes this workspace. Core changes do not
require maintaining or checking the 3D client; its own workflow runs when its
files change or when manually requested. When resuming 3D maintenance, build and
verify against the current shared protocol and client libraries. Run matching
server and client binaries from the same checkout.

## Controls

WASD, arrows or numpad move relative to the camera (two keys make a diagonal;
holding repeats). Q/E turn the camera in 45-degree steps, the mouse wheel zooms.
Space waits, R/F (or `<`/`>`, PageUp/PageDown) climb and descend, M shows or
hides the mini-map, Tab takes control, Esc closes
the pack or quits. Stepping into
a hostile creature attacks it, as in the ASCII client.

Objects:

- Hover any cell for its name, description and what a click does; a frame of
  light marks it (gold: interact, red: attack, blue: travel).
- The cursor shows what a click does: a grasping hand over things to pick
  up (or doors to work), a walking figure over floor to travel to, crossed
  swords over a creature to fight.
- Click a hostile creature anywhere in view to walk up to it and attack, or
  an item out of reach to walk over and pick it up. The server's own travel
  takes the PC there (asked again if something in view interrupts it); only
  if travel is refused does it fall back to steps over the known floor. Any
  key calls the errand off.
- Click: attack an adjacent hostile creature, pick up an item within reach,
  open or close an adjacent door, climb the stairs you stand on, or travel to
  any known floor cell.
- G (or `,`) picks up the nearest item within reach; X opens or closes the
  hovered or an adjacent door.
- I opens the pack: Equip, Remove, Eat, Drink and Drop for carried items, and
  Take for items within reach. Choices come from the server's disclosed
  interactions through `tor_client_common::items`, as in the ASCII client.

## How it works

- `net.rs` owns `tor_client_common::Connection` on a worker thread and builds
  requests exactly as the ASCII client does, sending the window a plain `View`
  (visible cells, aligned map memory, creatures, items, narration, HP).
- Observations are in an observer frame that moves with the character. The
  window matches cell keys that appear once in consecutive views to recover the
  shift, so the world stays still and the camera glides after the player.
- The cave motif (`cave.rs`, `cave.wgsl`): every exposed face of a known solid
  cell is subdivided and displaced by a world-space noise field, so shared
  edges move together and the rock is seamless across cells and 8x8x8 chunks.
  Only chunks whose cells changed are rebuilt (about 1 ms per view). Faces are
  flat shaded for a faceted look, with per-vertex occlusion in corners.
- The shader paints walls as stratified slate with mineral flecks and floors
  as umber earth with cracks, gravel, damp patches and faintly glowing moss.
  It dissolves rock between the camera and the player with a dither, and
  darkens wall tops cut off above head height. Remembered cells are dimmed.
- Floors grow crystal clusters (teal or violet, each casting light),
  stalagmites and rubble, chosen from each cell's identity so a place looks
  the same on every visit. Cells two or more above the feet are cut away.
- Creatures are reported by a body cell, so each is stood on the first solid
  cell beneath it. Hostile creatures get a red ring; a wound flashes the torch.

- Props (`props.rs`) share the cave's art direction: chunky, flat-faceted
  forms in one small palette (wood, leather, cloth, bone, iron, steel, bronze,
  gold). Painted mesh palettes use sRGB and convert to linear color at upload;
  overbright eye glints and material emissive energy keep their linear range.
  Sculpt occlusion multiplies decoded linear pigment, independently of its hue;
  runtime lights supply directional shade.
  A glowing accent appears only on magical or valuable things, in the
  crystals' teal, violet and amber. Models are built in code and cached:
  mace, greatsword and shorter blades, ring mail, bandage, ration, corpses
  tinted by creature, the dawn seal, copper/silver/iron tokens, the stone
  tablet, and fallbacks for every item class (potion, ring, amulet, scroll,
  spellbook, wand, gem, tool, pouch). Items are matched by name, then class.
  The shared model cache retains live handles and reuses idle models within a
  128 MiB raw-mesh-payload budget and 1,024-key limit. Periodic pruning releases
  the least recently used idle handles; these limits do not bound live geometry,
  renderer staging or total GPU residency.
- Doors are stone doorways two cells high with an amber lamp, turned to fit
  the walls beside them; the plank leaf swings when the door opens or closes.
  Stairs up spiral around a pillar inside a teal rune ring; stairs down are a
  dark shaft ringed by stone and a cold blue rune. The equipped weapon hangs at
  the player's side, and every item on the floor carries a small glint.

- Scale follows the selected [Bold Storybook art direction](../../docs/art/3d-art-direction.md):
  stronger character presence, expressive heads, hands and feet, and deliberate
  family silhouettes. Cells retain their movement and support meaning.
- Creatures (`creatures.rs`) are cached jointed figures with sculpted connected
  masses, thin draped garments, flowing feather clumps and inset expressive eyes.
  Painted patterns preserve individual variation, including quagga stripes.
  Seven body plans cover the scenarios: bipeds (the
  player, sentinels, arcanists and mages, the ruin scout, stone guardian,
  hobgoblin, zombie, vampire and yeti), a bird (emu), quadrupeds (quagga,
  black unicorn), the centaur, a serpent (snake) and floaters (ember wisp,
  ice monster). Gameplay recipes come from disclosed asset IDs and their
  supported parents. The player holds the disclosed equipped weapon.
  Biped dimensions and limb shapes resolve once in `creatures/anatomy.rs` and
  feed mesh generation, joint placement, garment fitting and support motion.
  Slender and sturdy organic builds link shoulder, waist, hip and muscle shape;
  average bodies, vertical bone lengths and terminal sizes remain the anchors.
  Guardian and Yeti retain their separate mass recipes. These fitting rules are
  foundations for equipment composition; interchangeable worn armor and general
  data-driven creature authoring remain future integration work.
- The Rogue bestiary: every monster in `scenarios/rogue-exploration` has a
  recipe of its own, drawn from the maintainer's concept sheets in
  [`docs/art/rogue-concepts`](../../docs/art/rogue-concepts/README.md). The orc,
  troll, leprechaun, nymph, medusa, phantom and wraith are dressed on the
  shared humanoid body, as women and men; the medusa's hair is a crown of
  weaving serpents and the phantom is drawn translucent as its normal look.
  The bat and kestrel are always drawn on the wing, beating membrane or
  feathered wings, and never plant feet. The griffin, dragon, jabberwock,
  aquator and xeroc keep the four-legged support rig on their own short, heavy
  legs and feet, with jaws, folded wings that mantle, tails and the aquator's
  segmented tendrils. The Venus flytrap is rooted, swaying on a two-jointed
  stem with a snapping trap, and the rattlesnake rests coiled, its rattle
  buzzing. A disguised xeroc remains the item the server discloses.
- Idles: at rest everyone breathes, shifts weight and looks about, with a
  fidget of their own every few seconds: the hero checks the blade, casters
  study a raised palm, sentinels settle the shield, the scout flips the dagger,
  the hobgoblin taps its club, the zombie sways, the yeti beats its chest, the
  emu pecks, horses graze and stamp, the snake rears to taste the air. The
  orc cracks its knuckles, the troll scratches its scalp, the leprechaun flips
  a coin, the nymph opens a welcoming hand, the medusa flexes her claws, the
  wraith reaches, the dragon snorts, the griffin preens and the xeroc gapes.
- Animation: walk cycles paced to the distance moved use separate support and
  swing paths. A zombie drags a leg, a guardian or yeti lumbers, the emu stabilizes
  its head, horses change from a four-beat walk to diagonal trot, the snake
  travels on a wave, floaters bob with orbiting
  motes), turning to face movement and targets. The server's combat events
  pick the move for each ability by name: a strike with wind-up and
  follow-through, a heavy overhead smash with a shockwave (Rattling Blow), a
  cast that launches a bolt in the spell's element (Spark Lance), a hex of
  drifting violet motes (Falter), and a guard or ward with rising rings (Brace
  for Impact, Shell). Rogue signature abilities have their own motions: the
  dragon's breath pours from its open jaw, the medusa's confusing gaze kindles
  her eyes as her serpents rear, the aquator sweeps its tendrils to corrode,
  the wraith drains, a xeroc lunges from ambush and the rattlesnake's bite
  carries venom. Targets flinch when hit and sidestep a miss, timed to
  the blow landing; beasts rear and stamp or peck, the centaur thrusts.
- Faces and torsos are sculpted (`src/sdf.rs`): signed-distance shapes for cranium, jaw,
  cheekbones, brow, nose, lips, chin and ears blend into one surface, with
  carved eye sockets, meshed by surface nets per individual. Narrow mouth
  contours follow the actual face surface as separate fitted accents so their
  edges remain legible below the face mesh's sampling scale.
- Lighting follows the art direction: a dominant shadow-casting warm key,
  weaker cool fill and rim, contact shading, and localized magical accents.
  Matte cloth, satin leather and metal have distinct material responses.
- Variety: every individual is rolled from the server's opaque shared
  appearance seed, independent of observer or interaction target. Corpses retain
  the captured creature recipe and seed across reloads. Live player appearance
  uses the same rule; player-selected traits await server-owned storage in
  [the presentation plan](../../docs/plans/presentation.md). People are
  women and men, of slender, average or sturdy build, with one of five skin
  tones, sculpted hair (men swept, cropped, tousled, slicked or tied back,
  some bearded; women long, ponytail, bun, braid or bob) in six colours and
  varied eye colour, their own clothing shades and, for casters and scouts,
  hood up or down. Adult women use a defined waist, fuller bust and hips, strong
  legs, fitted garments and shaped fantasy armor appropriate to each role.
  Sentinels bear one of four emblems (sun, star, crescent
  or flame) on tabard and shield. Helmeted sentinels show a braid; hooded casters show
  locks framing the face. Beasts vary in coat, stripes, bands, fur or stone,
  and size.
- Antics: after a minute with no keyboard or mouse input, everyone at rest
  starts to amuse themselves: the hero dances a jig or stretches and yawns,
  casters twirl the staff or float cross-legged, sentinels march and salute
  or polish the shield, the scout juggles the dagger or tiptoes, the guardian
  does the robot, the zombie's head falls off (it catches it), the vampire
  pirouettes, the emu and the quagga race in circles, horses rear and buck,
  the snake coils into a bouncing spring, wisps loop the loop. The leprechaun
  jigs or spins nearly out of sight, the nymph dances, the medusa sways like a
  snake charmer, the dead rise and drift or loom, fliers loop and roll, the
  dragon and griffin rear beating their wings, the jabberwock jabbers, the
  aquator hops, the xeroc spins on its shell and the flytrap dances or snaps
  at flies only it can see.
- Death: a creature seen to die collapses onto its back, limbs limp, head
  lolled (beasts on their side, legs slack); its glow fades to embers and any
  light it carried goes out. Its body stays as its corpse,
  turned to lie along open ground. Corpses found later are the creature's own
  model in that pose. With the same disclosed appearance and cell, a corpse keeps its rig and
  pose between views. Newly identified remains adopt their disclosed creature recipe.
- Light: each current cell carries server-owned illumination. Lit cells get
  warm room fill and decorative glow. Dark body awareness stays readable as
  muted unlit geometry; point lights and emissive accents are suppressed there.
  Remembered terrain stays dim. Shared map hydration restores terrain after
  reconnecting, without inventing live actors, item targets or door controls.
  The player's warm fill is artistic lighting, not a simulated portable torch.
- Walls down: rock on the camera's side of the character is cut to a
  knee-high stub across the view, so the near row of every room (stairs,
  items, creatures) stays visible; taller rock nearer the character dissolves
  in a dithered circle as before.
- Mini-map: a north-up circle in the lower right of the character's level:
  seen and (dimmer) remembered floor and walls, doors, stairs up and down,
  items, creatures (hostile in red) and a wedge for the camera's heading.

## Not done

Advancement, abilities, history, notes and the other screens; choosing a
quantity when taking or dropping part of a stack (the whole stack moves).
The independent verification gate covers the retained prototype scope.

The scene draws one representative rig per disclosed actor target. Repeated
body cells and portal images do not create additional rigs. Mouse selection
uses transformed model bounds and finite, disclosed floor surfaces; it does
not retain procedural triangle data on the CPU for exact mesh picking.

Known ability IDs select motion recipes; unfamiliar techniques use a neutral
gesture. Display captions do not choose combat effects. A hurt reaction needs
a disclosed HP decrease or worsening injury between comparable views. An
applied status effect alone does not make its target flinch; damage that stays
within another actor's disclosed injury band may have no flinch.
