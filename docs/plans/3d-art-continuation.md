# Procedural 3D art: implementation and continuation plan

This plan records the current procedural-art foundation, its review limits and
the next useful work. The [art direction](../art/3d-art-direction.md) owns the
visual principles; the [3D client guide](../../crates/client-3d/README.md) owns
launch and diagnostic commands. This remains an independently verified frontend
prototype, not a completed art pipeline or a new gameplay milestone.

## Intended outcome

New creature recipes should compose body plans, correlated dimensions, materials,
garments, attachments and compatible motion. Improve shared construction rules
before polishing individual ornaments or isolated frames. Preserve compact Bold
Storybook proportions, expressive color, bright foreground lighting, adult
character silhouettes, a broad shallow male chest and open ceilings.

The existing implementation is mostly Rust recipes. Shared fitting data makes
future equipment composition more practical, but there is no general external
mob-authoring interface or interchangeable worn-armor renderer yet. The client
must continue to render only appearance and equipment disclosed by the server.

## What is implemented

| Foundation | Implementation | Current boundary |
| --- | --- | --- |
| Body dimensions | `creatures/anatomy.rs` resolves biped dimensions, joint landmarks, limb/foot shapes and Storybook mesh scales once per rig | Guardian and Yeti retain distinct recipes; equines and centaurs retain their own dimensions |
| Build variation | Existing slender/average/sturdy choices link shoulders, waist, pelvis and muscle fullness | Average proportions and vertical bone lengths are anchors; this is not unrestricted morphology |
| Shared fitting | Geometry, robe clearance and animation support use the resolved body; lateral joint roots and garment widths follow it | Arbitrary equipment fitting and compatibility are not implemented |
| Torso layers | Cloth, leather and plate use material-specific rib/back ease; structured fronts bridge the chest with a fitted waist | Existing family cuts remain; sleeves and complete garment patterns need more work |
| Faces | Joined lower-face/chin forms, surface-fitted mouth contours and family-specific nose/fang/tusk clearance | Expression comes from sculpted geometry; no facial-animation system |
| Cloth | Each caster owns a continuous double-sided robe and bound sash, fitted to the final supported pose | Approximate body proxies and bounded contact correction cannot guarantee every contact is clear |
| Motion | Grounded support/swing paths, turning, interrupted actions, supported falls and family-specific locomotion | Still-image review does not establish real-speed timing or absence of foot sliding |
| Model resources | Live meshes remain referenced; idle reuse is bounded by payload and key budgets | The idle budget is not a bound on total CPU memory, live assets or VRAM |
| Shape and rendering | Connected organic joints, fitted trim, feather/floater forms, foreground lighting and material separation | Some forms still read as rounded primitives or a coating over anatomy |
| Review tools | Offline lineups, fallen lineups, travel studies, appearance controls, camera azimuth and capture warmup | These are diagnostic views, not gameplay authority or automatic visual-quality scores |

The renderer retains remote-main's synchronous material-pipeline setup and CPU
DX12 frame pacing alongside adjacent shader-compiler discovery. Explicit renderer
and compiler overrides remain available. Live play and diagnostic views share
the camera shading setup.

## What is working, and what that evidence establishes

Before the current remote-main rebase, the shared body/profile pass was reviewed
across all 18 families and five individuals per family in ordinary and side views.
Sampled travel, actions and fallen poses preserved broad support and family
identity. Adult figure relationships, the male chest correction and accepted
human/humanoid mouth work survived that pass. These findings establish a useful
visual baseline; they do not automatically qualify every later source revision.

The first torso-material preview had a narrower review: Warrior, Sentinel and
Vampire variants retained their adult silhouettes and fitted waists. The clearest
gain was the Sentinel's continuous chest shell. Leather and cloth changes were
subtler and did not resolve the overall garment-construction problem.

The rebased PR snapshot was captured again in normal and side lineups. Ordinary
views were reviewed for Warrior, Mage, Sentinel, Scout, Hobgoblin and Vampire,
with Guardian, Yeti, Quagga and Centaur controls; the six torso families were also
reviewed from the side. No broad static proportion or outfit-fit regression was
identified. This extends the torso review, but does not approve real-speed motion
or every cloth contact. The following image is actual rendering, not a generated
target:

![Current procedural humanoid lineups](../art/procedural-humanoids-current.jpg)

Focused PR checks exercise cache eviction without losing visible geometry,
correlated dimensions and rig landmarks, closed finite neutral robe geometry,
structured torso surfaces and a launched native renderer. Existing appearance
identity, disclosed combat reaction and native attack witnesses own their
unchanged boundaries. Full supported-platform/profile coverage belongs to the
independent 3D CI matrix; focused local checks are not a substitute for that matrix.

## Current challenges

1. **Garment construction remains the largest modeling gap.** Torso layers now
   have a little material-dependent room, but many sleeves and costumes still
   inherit the body's rounded form. Cloth needs supported regions, elbow and
   underarm ease, free edges and coherent folds; leather needs panel structure;
   plate needs protective shells and deliberate articulation/overlap.
2. **Body, outfit and motion recipes remain partly coupled.** `Style` still
   combines species, outfit and behavior choices. The resolved body is a useful
   seam, not complete separation of these concerns.
3. **Continuous cloth has bounded, approximate contacts.** A small intermittent
   angular dark patch remains documented from earlier travel review. Its exact
   surface identity was unresolved. Keep it behind broad construction work unless
   it exposes a general failure; repeated tiny crop/fix loops are not the priority.
4. **Real-speed motion is not approved.** Review foot plants, weight transfer,
   turn/stop/restart transitions, action interruptions and revival in motion.
   Do not infer timing quality from a contact sheet.
5. **Resource and update cost need controlled measurements.** Existing runtime
   observations are not isolated cloth, mesh-upload or GPU measurements. Preserve
   live references and bounded idle reuse while measuring representative crowds.
6. **References are aspirational.** The generated humans are taller than the
   selected compact in-game direction. The armor study exaggerates chest shaping;
   use coherent protective shells rather than copying separate chest cups.

## Continue in this order

### 1. Establish the exact reviewed baseline

Build the rebased frontend with its matching backend. Use fixed seeds, camera,
scale and lighting for comparisons. Finish the latest torso-layer review in
ordinary and side views, with a few travel/action poses and live gameplay.
Record the source revision and distinguish visual acceptance from a successful
capture or test. Reuse existing diagnostic commands rather than adding a second
review framework.

### 2. Give garments their own reusable shape rules

Extend the current torso-material seam into supported/eased/free regions. Start
with Hero, Scout, Caster, Vampire and Sentinel recipes so one rule improves
several roles. Keep the anatomical support surface separate from the garment's
visible surface, and seat trim and straps on the final garment. Preserve fitted
adult curves and neckline identity; a continuous front does not require a flat
torso or a loose barrel. Give sleeves their own taper and articulation clearance.

Acceptance: the layer reads as cloth, leather or plate at gameplay distance;
waist/shoulder/hip relationships survive all supported builds; arm movement does
not expose broad disconnected surfaces. Review both sexes and all three builds
collectively, with unchanged families as controls.

Progress: limbs now have the first reusable cloth rule. A sculpt can be
inflated, so a sleeve or trouser leg is the anatomy beneath it grown by an
ease (filling the muscle hollows), with soft rolls gathered at the elbow and
below the knee; cuffs and bracers sit over that ease. Every clothed person
uses it, the Rogue folk included, and contract tests require the cloth to
clear the limb all round for every build. Draped skirts, tunic hems, coat
tails and the caster robe now hang free: their folds deepen toward the edge
and the hem drops where each fold swings out, while short edges such as cowls
stay neat. Cloaks, capes, tunic skirts, coat tails and rags now hang on
their own drape pivots: a cloak flares back as its wearer travels and trails
through turns, skirts sway with the stride, and cloth at rest barely stirs.
This is a lagged swing, not cloth simulation, so contact with legs is not
checked. Sentinel plate now articulates: overlapping lames form the
rerebrace under each pauldron and the cuisses over the thigh, each band tucked
inside the one above so bending cannot open a gap. Torso garments now have
all three regions: supported at the shoulders, eased over the ribs, and free
below the belt, where leather jerkins and vests hang as a short swinging
peplum over the tunic skirt. Coats and robes keep their own skirts.

### 3. Separate composition concerns without a new content format

Keep body morphology, outfit selection, material roles and motion constraints
explicit in internal recipe data. Use shared surface/attachment coordinates for
belts, shoulder layers, held items and future worn equipment. Preserve deterministic
appearance identity and include every future independent visual choice in cache
identity. Do not infer anatomy or equipment slots from display prose.

Progress: outfit selection is now internal recipe data apart from the body. An
outfit names what each slot wears: arms (bare, sleeved or plate), legs (fitted,
trousered or plate), boots (height, heel, buckles, cuff), hands, cuffs, an outer
layer (a jerkin, vest or coat, or a cuirass with tabard and gorget), the tunic
skirt and a cloak. Each clothed kind's own outfit reproduces its concept-sheet
garments exactly; mesh fingerprints of every humanoid kind, sex, build and shade
were unchanged by the extraction. A kind's own trimmings (lacing, baldrics,
cowls, loincloths, rags, charms) come only with its own outfit, while what its
body grows, such as the medusa's serpents, stays whatever it wears. Skirts are
cut for the body: a waistband seated on the pelvis, a hip rounding over where
the thighs join it, and a hem flared just enough to contain the thighs, with
leprechaun coats cut away at the front as on their sheet.

An offline wardrobe (commoner, ranger, knight, courtier) dresses any humanoid
that wears separable clothes; stone, fur and robed casters do not. Each dressed
look has its own cache identity. The bestiary's `--outfit NAME` dresses its
humanoids, and O, or `--wardrobe-cycle SECONDS`, redresses them mid-stride,
replacing only the model while motion carries on. This is presentation for
review only: live creatures still wear their kind's outfit, as observations
carry no equipment list (see 5).

### 4. Review motion families at real speed

Review people, heavy humanoids, birds, equines/centaurs, serpents and floaters
together. Check travel, sharp turns, stopping, action interruption, falls and
revival. Fix common support/weight-transfer rules before individual pose garnish.
Retain different family rhythms and the current disclosed ability-selection rules.

Progress: a real-speed walk review now runs as a test. It walks seventeen
families at a steady pace through the actual animation and support systems
and measures how far each planted foot slides between frames. Every planted
foot's median slide is zero; it found the shortened griffin and dragon legs
dragging through their stance (a horse's stride on shorter legs), now fixed by
scaling each beast's stride to its leg length. The same measurement covers a
steady turn and a stop-and-restart for people, birds, equines and beasts:
planted feet yield at most about 2 mm a frame on tight turns, as the solver
intends, and settle without skating. A third measurement follows a walk cut
by an action, a death and a revival: interruptions move no faster than the
action's own lunge, and revival, which used to snap a body upright in one
frame, now rises out of the death pose over most of a second.

### 5. Measure cost, then expand equipment integration separately

Measure first-use mesh generation, warm reuse and cloth/mesh-update costs with
controlled inputs. Keep raw timing claims separate from correctness tests.
Interchangeable worn armor needs disclosed visual recipes, anatomical attachment
mapping, coverage/layer ordering, clearance/deformation rules and loadout-aware
cache identity. The client side of that now exists for review (see 3): plate
lames are fitted to the wearer's own thigh and upper arm and stay nested, for
every build and figure, and sculpts evaluate only the steps that can shape
each block of their grid, exactly near the surface, which roughly halves
first-use mesh generation for a dressed person. NPC equipment cannot be invented when its observation has no
equipment list. Protocol/content-format changes require their own agreed scope.

## Review and verification discipline

Keep the original dirty development checkout protected. Use the isolated art
branch and preserve unfinished work before rebasing. Never edit checked Rust
inputs during a build or run competing owners against the same target directory.

For this PR, the maintainer requested focused local tests and broad CI coverage.
Run the targeted art-contract tests, the renderer checks affected by rebase,
documentation/link checks and the native offline lineup witness. Live-backend
and combat process coverage remains with CI. Record failures
and limits honestly. The independent 3D workflow runs Windows/Linux debug/release
verification; its full result is required before merge. Do not weaken tests or
claim manual visual approval from a passing automated suite.

New body/outfit behavior should extend the narrow owning regression tests and
actual-process witness. Existing interaction, appearance identity and authority
tests remain relevant without duplicating their assertions in art-specific tests.

### Contract witnesses

| Contract | Owning witness | Limit |
| --- | --- | --- |
| Bounded idle reuse while visible meshes survive | [Cache regressions](../../crates/client-3d/src/props_art_tests.rs), including key and byte boundaries | Caller-owned insertions become idle when dropped and are accounted for by periodic pruning |
| Correlated bodies and actual rig landmarks | [Body contract tests](../../crates/client-3d/src/creatures/art_contract_tests.rs) | Does not grade anatomy or character appeal |
| Continuous structured torso and fitted waist | `geometry::art_contract_tests` in [geometry](../../crates/client-3d/src/creatures/geometry.rs) | Sampled surface properties, not full garment/contact approval |
| Finite closed neutral cloth and independent per-wearer deformation | `robe::art_contract_tests` in [robe](../../crates/client-3d/src/creatures/robe.rs) | Topology and mesh ownership are checked; full contact clearance and real-speed continuity remain manual review |
| Default backend, single-sample cameras and software frame pacing | `graphics::tests` in [renderer setup](../../crates/client-3d/src/graphics.rs) | Native process evidence supplies actual renderer startup/readback |
| Observer-independent disclosed appearance | `scene::tests::observer_role_and_interaction_target_do_not_change_the_rendered_appearance` in [scene tests](../../crates/client-3d/src/scene/tests.rs) | Existing identity boundary retained |
| Generated models presented by a real native window | `Native3DProcesses.test_art_lineup_presents_generated_bodies_and_accepts_native_close` in [process tests](../../crates/client-3d/scripts/test_client_process.py) | Screenshot integrity and native window lifecycle, not automated aesthetic judgment |
| Disclosed combat still dispatches real client motion | `Native3DProcesses.test_native_click_attacks_the_selected_actor_and_plays_its_motion` in [process tests](../../crates/client-3d/scripts/test_client_process.py) | Existing server/transport/native-input witness retained |
