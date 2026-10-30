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

On Windows the client compiles shaders with DXC whenever it can find a
`dxcompiler.dll` and `dxil.dll` pair: first beside the actual client executable,
then in the newest installed Windows SDK's `bin/<version>/x64` directory (the
SDK that Rust's MSVC toolchain already requires). FXC compiles the shadowed
lighting shader into much slower GPU code: on an RTX 2060, a 25-creature crowd's
opaque pass took about 10 ms with FXC and 2 ms with DXC. DXC also avoids the
position-dependent black speckles observed on skinned models with FXC and depth
prepasses on the Windows review host; the native pixel regression covers the first
Ranger. The client selects the pair by absolute path, independently of the
launcher's working directory. Only without any pair does it fall back to the
built-in FXC compiler. Explicit
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
For scripted compositions, deterministic captures and one-window looping playback,
see [scripted model review](review-scripts/README.md).

Offline review modes run without a server. Create the destination directory
before using `--capture DIR`; the screenshot writer expects it to exist.

Live startup presents a software-rendered splash before starting the private
world or initializing the game's graphics device. It embeds the maintainer's
[startup artwork](assets/startup-loading.png), so it also works when launched
outside the checkout. Its progress bar counts completed startup milestones:
connection setup, graphics initialization, the first dungeon view, and entry
into play. Status text and a camera-control tip accompany the bar; it does not
invent an estimated time or count unfinished creature models as a startup block.
The game window opens after its first scene frames are prepared. You can then
move while remaining creatures use their egg stand-ins. Escape or closing the
splash cancels startup and cleans up its owned processes. Small and high-DPI
displays retain the artwork's aspect ratio within the monitor bounds.

With live `--capture DIR`, `DIR/startup/` contains the software splash's actual
presented milestone images. `startup_splash` JSON diagnostics identify those
software presentations separately from the game's GPU screenshot evidence.
Offline reviews and model-workload benchmarks retain their existing startup.

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
  `--women` lines up only women among people, standing closer and framed as
  a figure study; `--jump` has each individual jump on the spot in turn;
  `--lively` keeps their idle stances and fidgets; `--antic N` has them all
  play one antic from the start (2 is the women's flirt), held long enough to
  see it through; `--turn DEGREES` turns everyone from their usual facing
  (90 for profiles); `--capture-every SECS` adds timed motion frames to the
  line-up captures. `--outfit NAME` dresses humanoids from the wardrobe:
  `commoner`, `ranger`, `knight`, `courtier`, `leotard`, `sarong`, `tavern`,
  `queen`, `pharaoh`, `goddess`, `juliet`, `jungle`, `genie`, `princess`, `royal-princess`, `sylvan-duelist`,
  `pirate`, `maiden` and `witch`. Character-study recipes add `elven-ranger`,
  `rogue`, `paladin`, `barbarian`, `valkyrie`, `cleric`, `druid`, `bard`,
  `vampire-noble`, `necromancer` and `desert`. `adventurer` is the Rogue armor
  style guide's unarmored base: a green short-sleeved shirt with split tails,
  brown trousers and cuffed tall boots. Every body armor preview is drawn over it
  in `review-scripts/rogue-armor.json`. These paired adult costumes
  select wearer-fitted necklines, independent skirt layers, embroidery and
  shared fur, leaf or feather trim. Female cuts emphasize fitted waists,
  exposed cleavage and split skirts; male openings are authored separately.
  For a focused review, use `--lineups --family Warrior --outfit elven-ranger
  --capture DIR --capture-warmup 3`; repeat with `--turn 90` or `--turn 180`
  for side and rear views, and `--jump --capture-every 0.5` for motion.
  These are initial study interpretations: accessory completeness and visual
  fidelity still require review against the supplied studies.
  Lineup capture deadlines use real time and wait for each GPU readback.
  Slow frames stop at each camera and family boundary so no view is skipped;
  creature animation retains its capped timestep.
  Automatic wardrobe cycling follows that same presented lineup clock, so slow
  model rebuilds and readbacks do not skip through outfits between captures.
- Independent equipment preview: repeat `--wear helm`, `--wear boots`, and
  `--wear cloak`, and one body armour, with `--bestiary` or `--lineups`. Body
  armours follow the Rogue style guide: `leather`, `studded-leather`, `ring-mail`,
  `scale-mail`, `chain-mail`, `splint-mail`, `banded-mail` and `breastplate`.
  In live play, the server's disclosed `worn` list selects the same pieces:
  each Rogue armour asset maps to its own torso recipe. For example,
  `--lineups --family Warrior,Mage --outfit royal-princess --wear helm --wear boots
  --wear cloak --wear breastplate --capture DIR` combines the preset with a steel
  helm, leather boots, a travel cloak and a fitted breastplate. These pieces override only their own regions and
  remain equipped while O or `--wardrobe-cycle` changes the outfit. A closed
  helm hides the preset crown/hood, loose hair and covered earrings; boots replace
  footwear and its attached trim. Clearing a visual equipment selection in the
  composition API restores the currently selected preset's piece; omit `--wear`
  to review that preset alone. Native mage robes retain their pose-aware
  deformation with accessory equipment. This first preview supports dressable
  humanoids; unsupported bodies keep their existing recipes. Breastplates fit
  over the selected torso layers without replacing sleeves or skirts. The female
  variant has a contoured front and an opening fitted from the same bust landmarks
  as garment necklines; the male variant retains a broad chest plate. Existing
  cuirasses participate in the fitting surface when worn underneath. Covered
  bodice ornaments are suppressed; straps and pendants fit the resulting exterior.
  Cloaks use the same armor surface for resting and posed upper-back clearance.
  Additional armor regions and slots, general cloth collision and gameplay
  integration remain future work. `--wear` is rejected for live connections and the item gallery.
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
- `scenarios/crowd` (in this crate) is the smaller rendering diagnostic: 24
  creatures and the player, using the menagerie's palette. Use
  `--scenario crates/client-3d/scenarios/crowd --distance 16` to frame all 25.
  The default distance only framed 15 in the local camera-count check.
- `scenarios/crowd-hundred` repeats the same creature mix with 100 NPCs and the
  player in a spaced layout. Use
  `--scenario crates/client-3d/scenarios/crowd-hundred --distance 34` for the
  target of 100+ visible creatures at 60 FPS (16.7 ms/frame). Its placement respects the server's Manhattan
  sight limit, including ground beneath the bodies. Local native acceptance
  verified 101 body-bearing actors in the camera throughout the measured window;
  this establishes the workload, not a frame-rate guarantee. The current
  standalone release measurements remain below the 60 FPS target.
- `--frame-stats SECS` (1 to 600) times live play. Once the world appears it
  waits until five seconds pass without a frame over 250 ms, noting the longest
  frame meanwhile (`worst_settling_ms`, where creature construction stalls show),
  then times every frame for SECS with presentation unsynchronized, prints one
  `3d_frame_stats` JSON line and exits. The report gives the mean, nearest-rank
  p50/p95/p99 and maximum frame times; instantiated creatures, mesh entities,
  skinned meshes, point and shadowed lights, posed rigs; per-frame animation work
  on the main thread (pose, robes, drapes, hair) and hair's own stages; and each
  render pass's recent GPU and CPU time where the device can time passes.
  `frustum_creatures` reports the minimum/maximum distinct actors with generated
  body geometry in the sole active 3D camera over all timed frames. It excludes
  held props, decorations and deliberately oversized hair/cloth culling bounds.
  `sampled_frames` must equal `frames`; missing or ambiguous camera data increments
  `unavailable_frames` and makes both extrema null. Counts run after current-frame
  culling. The original `creatures` field still counts instantiated actors.
  Frustum presence does not prove unoccluded pixels; inspect a captured view
  separately. Avoid screenshot readbacks during performance measurements.
  For example, `--scenario crates/client-3d/scenarios/crowd-hundred --distance 34 --seed 7 --frame-stats 10`.
- In live play, fitting loose hair against the body runs on background threads:
  each wearer's hair keeps its last fitted shape between solves, and free workers
  go to the wearers that have waited longest, sooner the nearer they are to the
  camera. Offline reviews, tests and benchmarks fit hair inline every frame, so
  their results stay exact and deterministic.
- `TOR_3D_CURSORS=DIR` saves the cursor icons, enlarged, for inspection
- `TOR_3D_PROFILE_BUILD=1` enables local startup timings for model cache misses
  and scene construction phases, plus live/idle model-cache payload and evictions;
  ordinary runs keep this profiling disabled
- CPU construction comparisons use `python scripts/perf_compare.py BASE --case models`
  from the repository root. Both revisions must contain the versioned `model_bench`
  example and its validator. Three fixed compositions each produce three cold/warm
  spawn pairs; cold starts with an empty model cache and warm reuses that cache while
  retaining the first model. World/rig creation and queued entity insertion are timed;
  material setup and mesh accounting are outside the timers. Mesh byte counts cover
  CPU vertex/index buffers, not total process memory or GPU residency. The current workload
  also records 30 CPU pose-update timings per sample at 50 Hz, following two untimed
  initialization updates and a jump trigger. It runs the same pose, transform and
  soft-body schedule as review playback, including hair/cloth fitting. Schedule
  execution is timed; timestep updates and pose-change checks are outside the timer.
  This is not GPU rendering, total application frame time or display pacing. The
  standalone producer disables optional startup profiling during measurement. It
  enables a bounded latest-frame profile for pose/joints, robe fitting, drape
  updates and hair/contact fitting; ordinary playback leaves this disabled.
  Model workload version 4 additionally splits hair time into control fitting,
  mesh deformation, face/edge constraints, and mesh writes/normal reconstruction.
  Nested stage totals must fit inside hair time; they are not added again to the
  top-level total. These phase totals exclude scheduling, transform propagation
  and soft-body work. Compare only matching workload versions.
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
Process tests run through the shared test runner, which writes each outcome and
traceback to `process-tests-tests.jsonl` as it happens. A failed or timed-out
step prints the slowest tests and every recorded failure.

Process tests that generate and animate models (lineups, the bestiary, outfit
and equipment studies, scripted reviews, the live playlist and the model
benchmark) run locally only. GitHub's runners have no GPU and run them 10 to
17 times slower, so by the maintainer's decision CI doesn't gate on model
generation. CI still runs every gameplay, launch and input-rejection process
test, and `TOR_3D_MODEL_TESTS=1` runs the model tests there too. Run both
profiles locally before relying on a model change.

With explicit maintainer authorization for an unavailable local display, the
independent verifier also accepts `--waive-local-graphical MANIFEST` for either
profile. It reuses the [exact local graphical waiver contract](../../docs/development/testing.md#focused-iteration-and-local-mouse-exception): current discovered method IDs,
not class names or fixed counts, and an immutable validated manifest snapshot.
Non-graphical checks and both profiles remain required; CI rejects the option.
The pre-window invalid-appearance rejection is non-graphical and cannot be waived.
Receipt `tor-client-3d-verification-v3` separates `gate_passed` from
`clean_passed`; legacy `passed` remains equal to `clean_passed`. An authorized
waived gate may exit zero while both clean fields remain false. Missing or
mismatched actual waiver accounting and changed checked inputs fail the gate.
This mechanism grants no authorization itself and does not replace hosted native
acceptance or the unchanged default verification commands.

Core verification deliberately excludes this workspace. Core changes do not
require maintaining or checking the 3D client; its own workflow runs when its
files change or when manually requested. When resuming 3D maintenance, build and
verify against the current shared protocol and client libraries. Run matching
server and client binaries from the same checkout.

## Controls

WASD, arrows or numpad move relative to the camera (two keys make a diagonal;
holding repeats). Q/E turn the camera in 45-degree steps, the mouse wheel zooms.
Space waits, R/F (or `<`/`>`, PageUp/PageDown) climb and descend, M opens
the map and journal, Tab takes control, Esc closes
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
- I (or Z) opens the pack. Carried items offer Equip, Remove, Eat, Drink,
  Read, Zap and Drop, and items within reach offer Take. Choices come from the
  server's disclosed interactions through `tor_client_common::items`, as in the
  ASCII client.
- The normal inventory has an icon grid and a separate Within reach section.
  Click an item to inspect its description, quantity, equipped state and actions.
  All, Gear and Usable filter carried items; Usable follows disclosed use
  capabilities. Arrows select items locally. Ctrl+F or the search field captures
  typing for a name search of at most 64 Unicode characters. Enter or the first
  Esc leaves search; another Esc closes inventory. WASD still moves while browsing
  outside search. The wheel scrolls inventory under the pointer without zooming the
  world. Number-key item actions retain their original plan order through filters.
- F6 opens Equipment. Its slots come from the server's disclosed anatomy, with
  repeated slot kinds kept separate. Select carried gear to compare remembered
  properties with an equipped item; click a matching slot to choose the comparison.
  Unknown bonuses stay unknown. Remove and Equip use the same item actions as the
  pack; a full set of matching slots requires removing an item first.
  Item actions appear directly below the compact item header, before property
  text and descriptions, so long inspection details do not push them out of the
  opening comparison view.
  A large live character preview appears beside the equipment panel. Use its
  Turn left, Front and Turn right buttons, or `[` / `]`, to inspect the current
  appearance. Turning the preview consumes no turn and preserves the actor and
  exploration camera. It follows the live model, including its loading stand-in,
  and closes with Equipment. Narrow windows keep equipment usable and hide the
  preview when there is insufficient room.
  Tab switches Inventory/Equipment while the panel is open; outside the panel it
  retains actor-control transfer. Switching pages clears search and resets scrolling.
- A device zapped along a direction waits for one: the next movement key aims
  it, relative to the camera like a step, and Esc stops aiming. A device aimed
  at yourself is zapped at once. A device that allows either offers Zap, which
  then waits for a direction, and Zap self.
- When the server asks which item to identify, the pack opens on that choice:
  pick one of the offered items, or decline.
- Discovered traps lie on the floor as plates with a glowing seam: amber while
  their family is unknown, then coloured and fitted with the mechanism they
  spring. The mini-map marks them orange. Clicking a reachable unknown trap
  studies it; known traps are never offered as a travel destination.
- When something holds the character, the pack lists each hold by its holder
  (or as unseen) with Escape.
- The pack also lists granted abilities beyond the basic attack, labelled
  with their cooldowns and supply needs. Self-only abilities act at once.
  Rays and steps take the next movement key. Abilities aimed at a creature
  take a click on one (yourself or an ally for supportive abilities). Esc
  stops aiming.
- Projectile abilities ask the server for their choices. The pack shows each
  page: a thrown source or an equipped launcher, then compatible ammunition,
  with Previous, Next and Cancel. Once a delivery is chosen, the next movement
  key fires it. The session binds every query to its own reply and drops the
  choice when the state it was made from changes.
- F10 opens a right-side character sheet with disclosed attributes and the
  server's advancement pages. The shared live portrait appears beside it;
  Home / End turns the portrait when the command line is empty. Brackets remain
  available for typed commands. On narrow windows the sheet expands and the
  portrait hides when there is insufficient room. Buttons browse Overview, Point pools, Skills,
  Talents, Expertises, Intellect thresholds and Ledger; Tab cycles those sections.
  Page Up / Page Down request the previous / next server page. Scroll the
  page to read long content; the command line stays visible below it.
  Point pools and skill/talent/expertise catalogs have graphical choice cards,
  including each disclosed talent form. Up/Down selects a card; Enter chooses a
  funding pool or requests a purchase preview. A separate fixed-footer button
  (F7) names and confirms the exact retained proposal, even while browsing other
  pages; empty Enter never commits it. Preview cards show disclosed health,
  resources, injury and consequences, with an explicit warning if the result
  would leave the character dead. Balance/refund details remain available
  for review. Stale, unowned or
  unsynchronized pages cannot dispatch choices, and confirmation also requires
  current action readiness. Navigation and controls preserve nonempty drafts.
  Type any [advancement command](../../docs/play/advancement.md) for replacement,
  growth, Intellect expertise or exact supplies; supply pages keep their own
  visible content over retained catalogs. Esc or F10 closes; typing never moves
  the character or turns the camera. Attributes remain read-only disclosed facts;
  no allocation budget or undisclosed prerequisite tree is invented.
- C opens the read-only detection charts (monsters, food, magic, mapping) the
  character has acquired. A number key opens one; Page Up and Page Down page
  it; Tab returns to the list. The connection fetches and checks every page,
  and browsing never takes game time.
- M or the Map dock button opens a combined map and journal. The large north-up
  map fits the disclosed current-level terrain: bright cells are visible, dim
  cells remembered, and dark areas unexplored. The exploration mini-map remains.
  Places, History and Write note tabs share this screen; Page Up / Down and the
  mouse wheel scroll the journal. Narrow windows stack the two panes.
  Click a known tile or use Up/Down to select a destination; Enter or the
  Travel button submits its exact disclosed key. Selection is local, and stale
  views or unavailable action readiness cannot submit a journey. Closed doors,
  known traps, occupied tiles and other levels are excluded. The server chooses
  the route and interruption policy.
- F2 shows the latest retained history, oldest first, beside the map.
- F4 opens a note anchored to the current issued snapshot. It starts private; Tab makes it
  visible to everyone who observes the actor. Enter saves it. F4 also opens
  note editing from Places or History. M stays part of a note or rename draft.
- F5 lists the places the character knows and where each name came from. A
  number key picks one, then type its new name and press Enter.
- F8 resumes and F9 cancels paused or ongoing work, when the server offers
  either; the status line says when it does.
- Number keys 1 to 9, then 0, take the pack's first ten actions in order.
  Action buttons show available numeric shortcuts; filters do not change that
  mapping, so the pack can be played without the mouse.

Compared with the [ASCII client](../../docs/play/ascii.md), the 3D client
does not yet have the personal stats screen, the victory or death summary,
the message log, help, releasing control, running, or paging back past the
latest 40 history entries.

## Live interface

The live interface uses dark stone panels, brass borders and a persistent bottom
dock. A red health globe shows only the server's disclosed health, with the exact
value printed over it; unavailable health reads `— / —`. Other resource pools,
conditions, attributes and objectives retain the shared authoritative status rows.
The top badge distinguishes Ready, Waiting, Working, Observing, Fallen and
Disconnected. The four most recent messages sit above the dock.

The dock opens Inventory, Character (the existing advancement pages), Map, Places
and Journal (retained history). Wait submits the existing wait action only when
the controlled actor can act. Inventory-specific bindings are listed under
[Controls](#controls). Framed UI consumes
pointer input, including the spaces between buttons, so interacting with the dock
does not also move or attack in the world. The north-up map sits at the top right.
Opening inventory eases the camera toward the clear area beside it, keeping the
player visible; closing it returns the usual framing. This neither moves the
actor nor consumes a turn. The message panel stays hidden until there is a message.

Inventory selections follow disclosed item identities and are cleared when an
item disappears, becomes unreachable, is hidden by the filter, or the observation
switches actor, branch or stream. Filtering and selection do not send game actions.
Identification and projectile choices keep their priority prompt and original
number-key actions. Item symbols and framed stone/brass textures share bounded
procedural atlases, using disclosed models when available and category symbols
for unknown models. The bundled serif font has its [credits and license](assets/ui/NOTICE.txt).

The supplied six UI concepts guide further work. A graphical progression tree,
combined map and journal, and
pause/accessibility settings remain unfinished.

## How it works

- `portrait.rs` renders the existing displayed actor into one adaptive texture,
  capped at 768 by 1024 pixels. It shares mesh handles and animation, restores
  original render layers on close or actor changes, and includes newly prepared
  meshes without requiring another observation. The offscreen camera is inactive
  when closed. Exploration picking, following, cave cutaway, hair scheduling and
  frustum diagnostics exclude this camera. Camera fitting uses shaped bounds;
  loose hair/cloth culling envelopes remain rendered but do not shrink the figure.

- `net.rs` owns `tor_client_common::Connection` on a worker thread and builds
  requests exactly as the ASCII client does, sending the window a plain `View`
  (visible cells, aligned map memory, creatures, items, narration, HP and the
  shared status rows). The HUD shows those rows as the ASCII client does:
  attributes, hit dice, place and objective, then health, resource pools,
  gold, hunger, time, holds and combat state, including Dead and Victorious.
  Projectile choices and advancement pages also come from the worker, which
  binds each query to its reply. `model::advancement` projects bounded typed pages
  and the exact confirmation record; `progression.rs` builds graphical choices
  from those records without parsing prose. The shared reducer remains the final
  admission boundary for every preview and confirmation.
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
  foundations for equipment composition. The offline wardrobe swaps fitted
  clothing and plate, including garment-owned necklines, tunic and peplum cuts,
  pigments and trim. Outer peplums attach to the selected torso shell's resolved waist
  section and fit over the transformed tunic surface,
  accounting for folds and thickness while retaining the fitted waist.
  Reusable surface-fitted baldrics have open buckle hardware. Closed front,
  back, shoulder and waist patches follow the combined tunic/outer-garment
  support and clear plate, tabard and heraldry. Female cuts retain
  their shaped neckline and fitted figure across mob bodies; zombie decay survives outfit changes.
  Torso ornaments, sewn edging, collar/gorget colors and hip pouches follow the selected outfit;
  native harnesses, jewelry and sashes no longer reappear on replacement clothes.
  The Ranger recipe supplies its own pouch across wearer families.
  Sequential outfit replacements preserve body identity and reuse unchanged
  head/hair assets; selecting Native restores the original clothing and key.
  Shared tubes sample their cubic paths by model-space error and radius,
  preserving authored knots and paint spacing instead of applying eight
  subdivisions to every smoothed segment. Tiny rings avoid unnecessary side
  doubling while keeping the authored minimum and existing maximum.
  Closed organic arms and legs use radius-aware surface-net sampling with
  constrained projection and bounded conforming refinement. Fitted shells and
  painted cut edges retain their authored sampling. Rounded cloth gathers share
  their source field with fitting, including the sleeve's underside mass.
  Limb bounds include offset muscles and smooth-union growth before padding.
  Upper-arm recipes resolve their narrower clavicle and elbow features on a
  finer initial grid. A 0.75 mm local field-error allowance is checked against
  independent one-millimeter physical-distance and silhouette requirements.
  Scalar field queries and analytic value/gradient queries share primitive
  expressions and ordered field composition. Projection and refinement normally
  obtain both values in one traversal; scalar-only queries omit derivative work.
  A rejected projected surface gets one symmetric-difference retry on the same
  grid before falling back to fine sampling, preserving the density at hard seams.
  Sculpted vertex normals retain their existing normal calculation.
  Live creature and found-corpse recipes prepare on a background worker,
  including fitting, loose hair and the articulated rig. A shared egg-shaped
  stand-in remains visible and selectable while preparation runs; movement,
  input and disclosed state continue updating. Completed results replace only
  the stand-in, preserving the actor's current animation and position. Obsolete
  results are discarded after disappearance or appearance replacement. Each
  ready figure replaces its egg on the next update, without waiting for the
  remaining queue. One job runs at a time; the worker and render caches each
  retain the existing idle bounds and collect dropped assets. A failed job
  retains its egg and reports an error. Offline art reviews and the model
  workload retain synchronous construction for deterministic evidence. Frame
  timing waits for all live models to finish and settle; eggs and failed jobs
  cannot qualify a finished-model crowd measurement.
  Independent shape, silhouette, topology and volume checks cover the sampler;
  its conservative fine-grid fallback remains available for unresolved curvature.
  Cached contact and hair sources retain CPU coordinates once; warm fitting
  reads them without publishing spurious mesh modifications to the renderer.
  A resolved head component names headwear, neck adornments, earrings and neck
  coverage. The same values drive its cache identity and mesh construction, so
  a new outfit cannot inherit an earlier outfit's choker or collar.
  Mage and RedMage join the swappable humanoids. Replacement outfits remove
  native robes, sash knots and sleeve bells; Native restores them. Casting
  identity remains independent of the worn outfit.
  Bipeds and centaur riders share organic upper-arm, sleeve and forearm
  surfaces. Rider build changes arm volume and shoulder spacing while preserving
  reach; shoulder caps move with the arm instead of remaining on the torso.
  Yeti body masses use a shared surface-coat component: continuous underfur
  with staggered, closed clumps combed along the body curve. Torso, hip and limb
  recipes supply dimensions and flow; shins carry fur instead of generic boot shafts.
  Shared shoulder armor uses a hollow faceted crown and an overlapping lower
  plate, fitted over the resolved arm rather than scaled from build alone.
  Modeling follows the [Bold Storybook definition](../../docs/art/3d-art-direction.md#proportions-bold-storybook):
  compact lengths, large anatomical heads and substantial hands/feet, adult faces,
  lean female bodies with moderate hips and tapered thighs, and alluring
  adult female clothing and accessories. Shared female pelvic breadth places
  the leg roots independently of thigh muscle volume; leg and garment generation
  use the same tapered shape across builds. Existing geometry still needs review
  against that definition across front, side, rear and action silhouettes.
  General gameplay worn-equipment integration and data-driven creature
  authoring remain future work.
- The Rogue bestiary: every monster in `scenarios/rogue-exploration` has a
  recipe of its own, drawn from the maintainer's concept sheets in
  [`docs/art/rogue-concepts`](../../docs/art/rogue-concepts/README.md). The orc,
  troll, leprechaun, nymph, medusa, phantom and wraith are dressed on the
  shared humanoid body, as women and men; the medusa's hair is a crown of
  weaving serpents and the phantom is drawn translucent as its normal look.
  The bat and kestrel are always drawn on the wing, beating membrane or
  feathered wings, and never plant feet. Kestrel and Griffin share
  closed feather vanes with tapered points, a narrow central ridge and thickness
  proportional to their width. Staggered short feather rows cover the wing arms
  and overlap the long flight feathers. Griffin, Dragon, Aquator and Xeroc
  retain four supporting legs. Jabberwock rears above bent hind legs with
  free grasping forelimbs, a connected chest and a counterbalancing tail.
  Support and grasp roles are explicit in the rig, so another reared creature
  can reuse support handling without inheriting a quadruped's planted hands.
  Profiled segments with rounded flesh terminations and claw palms supply
  reusable limb volumes. Xeroc's
  disclosed true form has a large articulated maw and thick overlapping shell
  plates; both jaws share an oriented fang-row component. Plates seat on the
  supporting convex body, with their own closed backs and thickness. Species
  recipes retain their proportions and placement choices. Aquator has connected
  shoulder and haunch masses, a broad cheeked skull and seated dark eyes.
  Aquator and Xeroc share splayed supporting limb profiles and broad planar
  soles, with webbed or blunt crawling toes. Walking and local action support
  retain authored lateral as well as fore/aft foot offsets. See the
  [composition procedure](../../docs/plans/rogue-creature-models.md#composing-future-non-humanoid-monsters)
  for adding monsters with these pieces. Ice Monster refinement is deferred
  while its character-sheet direction is reconsidered.
  The Emu uses a compact shared ground-bird stance, substantial hocks and
  tapered shanks, broad three-toed feet, a breast-rooted throat and a larger
  anatomical skull. Its mesh and support solver resolve the same landmarks.
  Both snakes have broad sculpted skulls, separate hinged lower jaws and paired
  fangs that fold inside the closed mouth. Jaw, tongue and head-floor placement
  share anatomy dimensions, including the head's support when fallen sideways.
  The Venus flytrap is rooted, swaying on a two-jointed
  stem supporting deep cupped trap lobes, staggered cilia and a broad fleshy
  leaf rosette. Rosette and stalk growths share a closed leaf blade whose
  breadth, arch, droop and rim color belong to the recipe. The rattlesnake rests in an extended S with its head and rattle raised,
  its rattle buzzing. A disguised xeroc remains the item the server discloses.
- Quagga, Unicorn and Centaur share compact equine support dimensions: shorter
  cannons, distinct fore-knee and hind-hock landmarks, and a standing height
  derived from their broad planar hoof soles. Ribcage and haunch masses balance
  those shorter supports. A shared closed hoof wedge has a defined heel and
  sloped toe. Horse and Unicorn muzzles join their nasal bridge and lower jaw
  in one sculpt, with the muzzle pigment applied to the surface. Centaur side-fall
  body height and arm support follow the barrel width rather than a fixed offset.
- Griffin has a larger anatomical skull beneath its raised crest, a sculpted
  feathered neck with surface-fitted groups, connected eagle shoulders and
  shorter bent supports above broad talons. Lion haunches, hocks and paws remain
  distinct. Griffin and Kestrel share closed ridged hooked mandibles, with each
  recipe choosing their size and pigment.
- Bat and Dragon wings share closed tensioned skin with curved scalloped edges,
  shallow inter-finger camber and a sealed thick perimeter. Explicit outward
  facing keeps the same pigments outside both mirrored wings. Aquator toe webs
  reuse the membrane fan at foot scale; Bat wing supports are more substantial.
- Dragon uses shorter bent supports, connected shoulder/haunch masses, broad
  four-toed claw feet, a larger anatomical skull and a sculpted neck. Dragon and
  Jabberwock share rendered head/jaw dimensions so the lower jaw and hinge stay
  correlated with the skull; both reuse the substantial claw-foot construction.
- Worn boots share a shaped leather upper and layered welt/outsole. Heeled
  choices add a real raised arch and tapered block heel, with heel and forefoot
  on the common sole plane. Flat choices retain a planted midsole. The same
  outfit-controlled last supplies robe contact bounds; bare, stone and furry
  foot recipes remain separate.
- Shoulder cloaks share a connected two-sided sheet with a closed, thick perimeter,
  gathered upper-back attachment and separate exterior/lining pigments. Clean and
  torn hems use the same construction. Native Vampire and Wraith cloaks and Ranger
  and Courtier replacements fit the resolved wearer; each cloak owns its moving
  mesh while reusing the material. Upper rows stay attached as the lower cloth
  follows the existing eased travel/turn swing. This is bounded procedural drape,
  without general cloth collision or simulation.
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
  Adult female eyes share an almond opening and socket-fitted globe; iris and
  pupil surfaces clip to that opening, and continuous lids carry a visible upper
  lash line. Ordinary pupils are round, while Medusa retains her slit. Glow
  changes only the iris material. Lower-lid rims and adult cheek/jaw planes still
  need style work.
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
  hood up or down. Adult women use a defined waist, balanced bust, moderate hips,
  naturally tapered athletic thighs and clear knees, with alluring fitted
  garments and shaped fantasy armor appropriate to each role.
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


The original blue `princess` dress remains available. `royal-princess` selects
the purple and gold reference gown: a low sweetheart bodice, detached sleeve
swags, lavender lining, two thigh openings around a central panel, and a jeweled
crown. On male figures it selects the courtier outfit. For the female review use
`--women --family Warrior,orc,nymph --outfit royal-princess`.

Gathered sleeve puffs can expose a contrasting lining between raised fabric strips.
The Bard uses red strips over cream lining on both body cuts, with female bell
sleeves and the separate male short-sleeve cut. Plain puffs remain available.

Brocade uses fitted thread curves on torso garments and skirts. Torso stitches
stop at neckline openings, bare shoulders, cropped hems, and native skin patches.
Fine motifs remain independent of the body mesh shading resolution.

Bell sleeves use a hollow cloth shell with a visible inner lining and thin hem.
Their folds grow toward the wrist, preserving the shared forearm fit; fitted
bracers retain their separate construction.

Belt pouches fit the combined waist and pelvic surface, including outer torso
layers. Their curved backs, rounded fronts and belt loops share that fit across
wearers instead of relying on a fixed depth.

Sleeve compression folds follow the inner elbow and taper at their ends, leaving
the outside of the joint smooth. The upper and lower cloth retain their eased,
overlapping supports while bending.

Fitted torso belts and buckles follow the torso during breathing and action poses,
so that a moving waist cannot pass through a band held on the separate hip joint.
Caster belts continue to share the dedicated robe attachment.


### Sylvan Duelist

`sylvan-duelist` adds the green-and-gold forest outfit alongside both princess
presets. Female wearers have two embroidered shoulder-to-waist panels, an open
center and torso sides, dark trousers, tall boots and hanging green cloth.
Male wearers use the separate Ranger recipe. Review with
`--women --family Warrior,Mage,orc --outfit sylvan-duelist`; add `--jump` and
`--capture-every 0.5` for motion captures.

The reusable `Straps::Panels` cut resolves its width and path from the bust apex,
bust width, shoulder breadth and waist. Coverage, embroidery clipping and edge
binding share that cut; the renderer does not branch on the outfit name.


For fixed-step offline wardrobe reviews, add `--review-step-hz 50` to a
`--lineups` command. Each rendered update advances presentation by 20 ms;
screenshot readbacks hold the pose. Rates 20-240 Hz are supported. Wall-clock
watchdogs still run normally. This is a clock option for existing lineups, not
the proposed scene-script runner or a guarantee of identical pixels.
