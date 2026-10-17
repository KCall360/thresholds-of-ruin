# tor-client-3d (prototype)

A Bevy 0.16 client that draws the character's disclosed knowledge of the dungeon
with procedural scenery, creatures, and items. Its gameplay scope remains the
prototype's exploration, combat, and basic item interactions. It has its own
workspace, lockfile, tests, and verification gate.

Windows defaults to Direct3D 12 for live play and both offline showcases.
All three modes use single-sample rendering and Bevy's built-in FXAA.
On CPU Direct3D 12 adapters, rendering waits for each submitted frame to finish
before queuing another, bounding the software GPU's outstanding resources while
retaining the same models, materials, shadows, and resolution.
Other platforms retain Bevy's renderer defaults. The standard `WGPU_BACKEND`
environment variable can select another backend explicitly. Capture diagnostics
identify the actual adapter, backend and driver; verification receipts also
record renderer and compiler configuration without session credentials.

## Run

Build the release server once from the repository root, then run the client:

```
cargo build --release -p tor-server
cargo run --manifest-path crates/client-3d/Cargo.toml -- --seed 42
```

With no `--connect`, the client starts a private `tor-server` (default scenario
`scenarios/rogue-exploration`) with a fresh save under `.local/client-3d/saves/`
and stops it on exit. Other options:

- `--scenario DIR`, `--save FILE`, `--save-root DIR`, `--seed N`, `--server-bin EXE`
- `--connect 127.0.0.1:PORT [--actor 1] [--observe]` joins a running server
  (token from `TOR_SERVER_TOKEN`)
- `--bestiary` shows every creature walking, then performing all sixteen moves
  on a schedule, then a close-up of each at rest and in its signature move
  (with `--capture DIR`, named screenshots of each moment); `--portraits`
  skips straight to the close-ups; `--idles` shows everyone standing at rest;
  `--antics` shows them amusing themselves as if the player had left;
  `--photos` (with `--capture DIR`) takes one picture of each creature alone,
  turned three-quarters to the camera
- `--bored` starts a game as though the player had been away nearly a minute
- `--appearance woman|man[:N]` chooses the offline bestiary's Warrior preview; `N`
  picks among that sex's hair, skin and build
- `--lineups` (with `--capture DIR`) shows five individuals of each creature
  side by side, women and men alternating among people, with a close-up
- `scenarios/menagerie` (in this crate) is a lit hall holding its supported
  creature recipes, wandering at peace except an emu and a snake that fight, with
  items on the floor: `--scenario crates/client-3d/scenarios/menagerie`
- `TOR_3D_CURSORS=DIR` saves the cursor icons, enlarged, for inspection
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
Space waits, R/F (or `<`/`>`, PageUp/PageDown) climb and descend, C toggles the
ceiling cutaway, M shows or hides the mini-map, Tab takes control, Esc closes
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
  gold), with a glowing accent only on magical or valuable things, in the
  crystals' teal, violet and amber. Models are built in code and cached:
  mace, greatsword and shorter blades, ring mail, bandage, ration, corpses
  tinted by creature, the dawn seal, copper/silver/iron tokens, the stone
  tablet, and fallbacks for every item class (potion, ring, amulet, scroll,
  spellbook, wand, gem, tool, pouch). Items are matched by name, then class.
- Doors are stone doorways two cells high with an amber lamp, turned to fit
  the walls beside them; the plank leaf swings when the door opens or closes.
  Stairs up spiral around a pillar inside a teal rune ring; stairs down are a
  dark shaft ringed by stone and a cold blue rune. The equipped weapon hangs at
  the player's side, and every item on the floor carries a small glint.

- Scale uses cells as an art-direction reference. Proportions may be stylized
  to improve silhouettes, readability, and visual appeal at gameplay distance.
- Creatures (`creatures.rs`) are jointed figures with soft shading: smooth
  swept tubes for torsos, necks, limbs, tails and mane locks; painted coat
  patterns (the quagga's stripes); leaf-shaped layered feathers; cartoon eyes
  with whites, iris, pupil and catchlight. They were previously faceted in the
  cave's style,
  posed in code every frame. Five body plans cover the scenarios: bipeds (the
  player, sentinels, arcanists and mages, the ruin scout, stone guardian,
  hobgoblin, zombie, vampire and yeti), a bird (emu), quadrupeds (quagga,
  black unicorn), the centaur, a serpent (snake) and floaters (ember wisp,
  ice monster). Gameplay recipes come from disclosed asset IDs and their
  supported parents. The player holds the disclosed equipped weapon.
- Idles: at rest everyone breathes, shifts weight and looks about, with a
  fidget of their own every few seconds: the hero checks the blade, casters
  study a raised palm, sentinels settle the shield, the scout flips the dagger,
  the hobgoblin taps its club, the zombie sways, the yeti beats its chest, the
  emu pecks, horses graze and stamp, the snake rears to taste the air.
- Animation: walk cycles paced to the distance moved (a
  zombie drags a leg, a guardian or yeti lumbers, the emu's head pumps, horses
  move diagonal pairs, the snake travels on a wave, floaters bob with orbiting
  motes), turning to face movement and targets. The server's combat events
  pick the move for each ability by name: a strike with wind-up and
  follow-through, a heavy overhead smash with a shockwave (Rattling Blow), a
  cast that launches a bolt in the spell's element (Spark Lance), a hex of
  drifting violet motes (Falter), and a guard or ward with rising rings (Brace
  for Impact, Shell). Targets flinch when hit and sidestep a miss, timed to
  the blow landing; beasts rear and stamp or peck, the centaur thrusts.
- Faces and torsos are sculpted (`src/sdf.rs`): signed-distance shapes for cranium, jaw,
  cheekbones, brow, nose, lips, chin and ears blend into one surface, with
  carved eye sockets and mouth line, meshed by surface nets per individual.
- Lighting follows the art direction: a warm key from the upper left, a cool
  blue-violet fill from the right and a soft back light that rims figures.
- Variety: every individual is rolled from the server's opaque shared
  appearance seed, independent of observer or interaction target. Corpses retain
  the captured creature recipe and seed across reloads. Live player appearance
  uses the same rule; player-selected traits await server-owned storage in
  [the presentation plan](../../docs/plans/presentation.md). People are
  women and men, of slender, average or sturdy build, with one of five skin
  tones, sculpted hair (men swept, cropped, tousled, slicked or tied back,
  some bearded; women long, ponytail, bun, braid or bob) in six colours and
  varied eye colour, their own clothing shades and, for casters and scouts,
  hood up or down. Women are shaped differently: narrower shoulders and
  waist, fuller hips. Sentinels bear one of four emblems (sun, star, crescent
  or flame) on tabard and shield. Helmeted sentinels show a braid; hooded casters show
  locks framing the face. Beasts vary in coat, stripes, bands, fur or stone,
  and size.
- Antics: after a minute with no keyboard or mouse input, everyone at rest
  starts to amuse themselves: the hero dances a jig or stretches and yawns,
  casters twirl the staff or float cross-legged, sentinels march and salute
  or polish the shield, the scout juggles the dagger or tiptoes, the guardian
  does the robot, the zombie's head falls off (it catches it), the vampire
  pirouettes, the emu and the quagga race in circles, horses rear and buck,
  the snake coils into a bouncing spring, wisps loop the loop.
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
