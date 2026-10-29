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
character silhouettes, a broad shallow male chest and open ceilings. Follow the
[Bold Storybook definition](../art/3d-art-direction.md#proportions-bold-storybook):
large anatomical heads, substantial hands/feet and shorter lengths establish
compactness; female figures remain lean and lightly athletic with moderate hips,
tapered thighs and clear knees. Adult women, their clothes and accessories should
be alluring while preserving balanced anatomy and species identity.
The [updated cast sheets](../art/rogue-concepts/README.md) guide species and
costume choices; their approved humanoid reference anchors the body review.

The existing implementation is mostly Rust recipes. Shared fitting data makes
future equipment composition more practical, but there is no general external
mob-authoring interface or interchangeable worn-armor renderer yet. The client
must continue to render only appearance and equipment disclosed by the server.

## What is implemented

### Sharing checkpoint — 2026-10-21

The `feat/rogue-creature-models` branch collects the current full-cast refinement
for review. Shared body dimensions, garment-owned cuts and ornaments, constructed
boots, layered armor, fitted accessories and connected lined cloaks improve
composition across native and replacement outfits. Zombie damage and Phantom
transparency survive outfit replacement. Small reusable leaf, feather, membrane,
beak, claw, shell and hoof components support further creature recipes.

The broader shape pass covers the winged beasts and birds, Aquator, Xeroc,
Flytrap, serpents and equines. Rattlesnake's extended resting pose supersedes
the coiled pose in its reference sheet. Ice Monster refinement stays deferred.
The dated reference index retains the supplied full cast and approved humanoid
anchor, with filenames, sizes and hashes checked against the originals.

This is a reviewable modeling checkpoint. Adult face/neck transitions, hair and
hood overlap, general moving-cloth collision, whole-cast motion review and
controlled crowd-cost measurements remain open. The following construction
summary and continuation order distinguish these limits from implemented work.

| Foundation | Implementation | Current boundary |
| --- | --- | --- |
| Body dimensions | `creatures/anatomy.rs` resolves biped dimensions, joint landmarks, limb/foot shapes and Storybook mesh scales once per rig | Guardian and Yeti retain distinct recipes; equines and centaurs retain their own dimensions |
| Build variation | Existing slender/average/sturdy choices link shoulders, waist, pelvis and muscle fullness | Average proportions and vertical bone lengths are anchors; this is not unrestricted morphology |
| Shared fitting | Geometry, robe clearance and animation support use the resolved body; lateral joint roots and garment widths follow it | Arbitrary equipment fitting and compatibility are not implemented |
| Torso layers | Cloth, leather and plate use material-specific rib/back ease; structured fronts bridge the chest with a fitted waist | Existing family cuts remain; sleeves and complete garment patterns need more work |
| Figures | Each woman rolls her own figure, leaning toward her kind's typical one; the chest form is sculpted on a parameterized chest wall (bipeds and the centaur rider); garments name their support | Shapes are tuned by eye; no scaled or furred woman yet exercises a flatter species chest |
| Necklines and openings | Garments own split, deep and court plunge cuts resolved against the wearer; outer shells and a woman's low-cut plate open along the same cut; armholes close under the arm; bare-legged garments are cut high | Edges are painted plus fitted bindings, so very coarse meshes can still show a stepped paint edge beside the binding |
| Soft tissue | Soft regions are skinned to spring-driven bones that answer the body's acceleration in any motion; plate holds them, structured garments damp them | One spring per side; no cloth-on-skin interaction or per-move tuning |
| Pelvis | One sculpted pelvis inside the fitted envelope; skirts measure it at the belt line and widen only where the thighs press | The envelope radii still bound fits elsewhere |
| Faces | Joined lower-face/chin forms, surface-fitted mouth contours and family-specific nose/fang/tusk clearance | Expression comes from sculpted geometry; no facial-animation system |
| Cloth | Each caster owns a continuous double-sided robe and bound sash, fitted to the final supported pose | Approximate body proxies and bounded contact correction cannot guarantee every contact is clear |
| Motion | Grounded support/swing paths, turning, interrupted actions, supported falls and family-specific locomotion | Still-image review does not establish real-speed timing or absence of foot sliding |
| Model resources | Live meshes remain referenced; idle reuse is bounded by payload and key budgets | The idle budget is not a bound on total CPU memory, live assets or VRAM |
| Shape and rendering | Connected organic joints, fitted trim, feather/floater forms, foreground lighting and material separation | Some forms still read as rounded primitives or a coating over anatomy |
| Non-humanoid composition | Explicit support/grasp limb roles, authored bend landmarks, profiled limbs and claw palms; oriented thick scutes and fang arcs | Species proportions and placement remain Rust recipes; there is no external monster format |
| Adult eyes and brows | Substantial fitted female brows and controlled arches; one almond eye boundary shared by clipped iris/pupil surfaces, fitted lids and a visible upper lash line; iris-only glow and Medusa slit pupils | Lower-lid rims, species expression and adult cheek/jaw planes still need style work |
| Review tools | Offline lineups (with `--women`, `--jump`, `--turn` and timed motion frames), fallen lineups, travel studies, appearance controls, camera azimuth and capture warmup | These are diagnostic views, not gameplay authority or automatic visual-quality scores |

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

The updated Bold Storybook definition requires a fresh proportion and costume
review. Earlier construction checks and accepted snapshots do not prove that
anatomical skull size, compact lengths, adult faces, lean female anatomy or
alluring costume design meet this target.

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

Reassess representative anatomy and outfits against the updated Bold Storybook
definition before treating older proportion anchors as accepted. Compare the
anatomical skull, ribcage, pelvis, hands and feet separately from costume mass;
review front, side, rear and action silhouettes at gameplay and portrait scales.
Keep adult female appeal explicit while correcting inflated hip/thigh masses
through shared body dimensions and garment fitting.

Build the rebased frontend with its matching backend. Use fixed seeds, camera,
scale and lighting for comparisons. Finish the latest torso-layer review in
ordinary and side views, with a few travel/action poses and live gameplay.
Record the source revision and distinguish visual acceptance from a successful
capture or test. Extend the existing diagnostic renderer with the
[scripted scene fixture](#scripted-scene-fixture-design) described below; keep
existing lineup commands and reuse their composition and rendering paths.

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
Cloaks now use one connected exterior and lining, sealed around a thick perimeter,
with clean or connected torn hems. Their gathered top fits the resolved upper back;
body build scales the shoulder and hem widths. The fitted upper drape reserves
selected rear strap layers so a baldric stays underneath it. Each wearer owns its cloth mesh,
while materials remain shared, including Phantom transparency. Two pinned rows
hold the upper attachment as the existing eased swing blends toward the hem.
Skirts and hair still follow their existing drape pivots. This is procedural drape,
not cloth simulation; general moving-body and floor collision remain unchecked.
Sentinel plate now articulates: overlapping lames form the
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
skirt and a cloak. Neckline, lower-panel split, hem trim and cutaway construction
belong to the outfit, including the outer jerkin's free peplum. Its split,
pigment and hem trim travel with the garment, independently of the wearer's
native costume. Open feminine bodices retain their shaped neckline on
sentinels, undead and other wearers. Plate coverage depends on the recipe;
the independent female breastplate now has an anatomy-resolved front opening.
Skirts are cut for the resolved pelvis and thighs, preserving fitted waists,
balanced hip contours and enough clearance for the wearer's legs. A shared cloth surface
also fits the jerkin's peplum over the actual tunic cut, including its placement,
Storybook scale, fold phase, raised trim and shell thickness. The outer waist
now attaches to the selected torso shell's actual waist section, including its
Storybook scale and front/back centre. Shell emission and seam fitting share
one resolved sculpt with the same garment cuts. The hip and hem still expand
around the lower layer. An emitted-triangle regression checks the attachment
around front, sides and back across native and Ranger wearers; the independent
lower-layer regression checks clearance around all four sides across
native and Ranger outfits, both sexes and all three builds. It also bounds
excess stand-off: a clear shell can still form an oversized shelf. The free
hip/hem dimensions are authored independently from the narrow sewn waist, so
fitting the lower layer does not inflate an already adequate hem. Clearance
is resolved on the cloth's radial vertex grid at matching physical heights and
angles. A lifted split edge expands its supporting panels without widening the
entire garment. Closed front edges remain coincident under asymmetric support;
open split edges keep their separation. This establishes resting clearance;
independently moving drapes still need contact review.

Baldrics are reusable accessories: the warrior, scout, ranger and knight use
one assembly fitted over their torso layers. Independently clipped outer
sculpts combine with the tunic beneath them; a short vest does not cut away
the shoulder support. The cuirass resolves once for rendering and attachment,
and accessory clearance includes raised heraldry. A grown support envelope is
traced before emitting the strap, avoiding normal-offset folds at tight waists.
Front and back strips share seam rows with shoulder and waist turns, forming
one closed loop with inner surfaces and real thickness. The shoulder and belt
landmarks set its span; buckle placement follows a height along the path.
Open buckle frames and tongues follow the strap direction and surface normal.

Shared shoulder armor now uses a broad faceted crown and a thinner overlapping
lower plate. Inner surfaces fit the resolved arm, including muscle fullness
and the clavicle insertion; turned edges expose the metal thickness. This
replaces the two rounded shoulder pads without changing the arm joints.

Harnesses, stocks and buttons, leaf bodices, neck bands and discs, and knotted
sashes are explicit outfit ornament recipes. Their materials and optional
shoulder straps or scarf travel with the selected ornament; the renderer no
longer looks up those costumes from the wearer's native kind. Mesh witnesses
transfer each recipe onto an unrelated wearer and remove it again. Neckline
and armhole edging, collar binding and armor gorget pigment also belong to the
outfit. Native and Courtier retain their authored trim; Commoner and Ranger use
their own cloth binding, and Knight uses its armor gilt. Ranger selects its own hip pouch
on every wearer; Native keeps the warrior's and scout's pouches.

Torso attachments trace the combined body and selected structural garment, so
an opening or short shell does not remove their underlying support. Sewn edges,
harnesses and scarf tails retain their authored pattern-plane coordinates while
their depth fits a grown support surface. Complete rows end where their full
width loses support, instead of clamping individual vertices toward the neck or
waist.
The native female vampire keeps her open bodice without the male cravat; both
retain their rows of gold buttons. Emitted-mesh witnesses cover native Medusa
bindings, Orc harness/scarf, transferred Orc layers and the sex-specific cravat.
Binding shading, neckline construction, painted edges and action-pose clearance
remain unfinished. A detailed ribbon-shading experiment is shelved while the
rougher Rogue creature families receive anatomy and silhouette refinement.
It has not earned visual acceptance and is not part of the current renderer.
Oblique visibility alone cannot prove
self-intersection: legitimate cap exposure can lie ahead of a more distant part
of the same curved strip. Static checks do not establish full costume approval.

Other native costume trimmings (lacing, cowls and loincloths) remain
recipe-specific. Body traits, including the medusa's serpents, and the zombie's
decayed, torn-shirt treatment survive wardrobe changes. Native leprechaun coats
retain their front cutaway. These construction rules preserve creature identity
while an exchanged garment carries its own design. Static mesh/topology and
clearance checks do not establish full motion/contact or visual acceptance.

The offline wardrobe dresses 18 humanoid model identifiers, including Mage and
RedMage; stone guardians and yetis retain specialized body recipes. Casters
exchange their native robes, sash knots and sleeve bells for the selected outfit
and restore them with Native, while retaining body identity and casting poses. Each dressed
look has its own cache identity. The bestiary's `--outfit NAME` dresses its
humanoids, and O, or `--wardrobe-cycle SECONDS`, redresses them mid-stride,
replacing only the model while motion carries on. Repeated recipe-level outfit
changes retain the individual's original cache identity: selecting an outfit
again reuses its model, and Native restores the original costume. Unchanged
head and hair assets are reused across sequential replacements. Offline selections
remain presentation for review; live creatures use their disclosed visual loadout
and cannot inherit offline overrides.

The [resolved head component](../../crates/client-3d/src/creatures/geometry/head.rs)
owns its headwear, neck adornments, earrings and neck-coverage dependencies.
One resolved value drives both mesh construction and cache identity. Unrelated
garment changes can still reuse the head, while a changed choker or standing
collar receives its own geometry. Fresh-versus-cached geometry tests exercise
both outfit orders on Human, Nymph and Phantom. Native close-ups compare the
same explicitly refreshed pose in both orders; a separate positive witness
requires the neck ornament to be visible in the selected pixel region.

Outfits also carry headwear: bare, a hood up or pushed back, or a closed helm.
A hood or helm covers hair and what is worn in it, and pointed ears fold inside
it; what the body grows, such as serpents, still shows. Redressing reuses the
head and hanging hair when the outfit leaves them alone.

Contact and hair fitting share a retained-source accessor. Initial fitting
keeps source coordinates available after render extraction; warm fitting reads
without mutable asset access. An application asset-event regression observes a
real mutation first, then requires zero modifications to existing meshes during
warm composition while private deforming meshes are still created. Existing
hair isolation and posed-contact tests retain their geometry and motion coverage.
This removes unnecessary invalidation; it is not a measured timing improvement.

Yeti torso, hips and limbs compose the shared `fur_parts::Coat` surface
component rather than rings of scattered tufts. Each patch supplies its body
ellipsoid, comb direction, clump length, color and seed. Staggered clumps
overlap continuous underfur; row heights vary to break up bands, and their crests and tips project along the body
curve using destination normals. The same closed lock emitter serves garment
caps and cuffs. Limb coats resolve their spans from the same elbow, wrist,
knee and ankle attachments as the rig, with small overlaps at either end.
Density has a bounded row/column count, and each complete Yeti
build is covered by a 35,000-vertex / 42,000-triangle interim ceiling and zero additional warm-spawn meshes.
The local debug models currently contain roughly 31,000–33,000 vertices and 39,000–41,000 triangles across
both sexes and three builds. The coat supplies its own continuous backing; furred limbs do not also build
covered skin or tube cores. Bent-joint witnesses cover the backing after removal.
These ceilings are regression guards, not accepted crowd-rendering targets.
The performance target is **100 or more visible creatures at 60 FPS**
(16.7 ms/frame). The separate
`crowd-hundred` live scenario repeats the existing crowd palette with 100 NPCs
and the player; distance 34 frames its spaced layout within Manhattan sight
range. A native functional run retained 101 body-bearing actors in the camera
throughout all 90 measured frames; the smaller crowd at distance 16 retained
25 throughout 176 frames. These runs captured screenshots to inspect placement
and are not performance measurements. The additive `frustum_creatures` report
tracks extrema over every timed frame after culling, excluding held props,
decorations and oversized hair/cloth bounds; any unavailable count makes the
extrema null. The original actor counter remains an instantiated-actor count.
The old smaller fixture at default distance had only 15 bodies in the camera
despite reporting 25 instantiated actors. Camera-frustum presence alone does
not establish unoccluded pixels. Three subsequent release-client runs without
screenshots retained all 101 bodies throughout their measurement windows and
recorded matching physical viewports, but remain below the frame-rate target.
The dominant reported GPU passes include opaque rendering, depth and shadows;
shared torso/head meshes lead the source geometry inventory. Cache-miss source
counts do not equal per-visible-instance work. Further density work and measured
comparison remain required; offline CPU composition timings do not qualify
this rendering target.

Closed arm and leg surfaces now use radius-aware sampling, constrained projection
and bounded conforming refinement. Their independent one-millimeter physical
error and radial silhouette witnesses cover small/large capsules, gathered cloth,
resolved limbs across sex/build variants and shared arm recipes. Rounded gathers
and the sleeve underside use the same fields for generation and fitting. Actual
offset muscle bounds include smooth-union growth before caller padding.
The combined surface/evaluator checkpoint passed 469 methods (zero failures, one intentionally
ignored subprocess fixture exercised by its lifecycle owner), and all four
native composition methods passed. Matched native captures cover the same 26
monster appearances on both sides: 1,861,887 to 1,721,261 source vertices and
3,280,784 to 2,999,532 triangles, retaining 565 meshes. The final cast used one
symmetric retry and no fine-grid fallback. Reviewed Centaur, Hobgoblin, Nymph,
Vampire, Troll and Mage views retain
their silhouettes. These are fixed-cast source counts, not crowd-instance or
frame-time comparisons. Fresh live saves deliberately vary cosmetic identity
through their persistent private salt, even at the same world seed; compare
fixed appearances when attributing geometry changes. The exact benchmark Mage
also retained its reviewed silhouette without fallback, using 94,525 source
vertices and 148,136 triangles against 100,187 and 159,460 before this work.
Upper-arm recipes resolve narrower clavicle/elbow features on their initial
grid. The local field estimate uses a 0.75 mm allowance within independently
tested one-millimeter physical error; guards cover 96 arm cases at 20,000
triangles each and the benchmark Mage forearm at 12,000. Focused optimized
checks passed all 22 SDF and eleven surface methods. Scalar and analytic
value/gradient queries share primitive expressions and ordered composition;
projection normally obtains both values in one traversal. Rejected surfaces get
one symmetric-difference retry on the same grid. This preserves the exact Troll
upper arm at 4,720 source vertices instead of the rejected candidate's 75,118.
Its regression retains independent physical-distance/silhouette controls and a
12,000-triangle budget. A fresh 101-body source inventory found six symmetric
retries and zero fine fallbacks across 1,847 cache-miss parts: 7,321,211 source
vertices and 13,405,296 triangles, which are not rendered-instance totals.
Three fresh release runs after this correction at 1800 by 1125 pixels retained
101 camera-visible bodies in every measured frame (474/474/461 frames, zero
unavailable counts). Each run identified the NVIDIA GeForce RTX 2060 on DX12.
They recorded 47.50/47.43/46.18 FPS; median frame times were
20.95/21.08/21.19 ms, p95 23.78/23.52/24.24 ms, and maxima
30.18/25.67/210.44 ms. The third run's hitch remains unresolved. The requested
60 FPS remains unmet. These current operating
points are not a controlled before/after comparison or evidence that projection
improved frame time. Head and torso density remain the leading shared geometry
follow-up. The controlled model workload comparison (version 4) used three balanced rounds and
validated all six runs without failure on the same machine. Human and Nymph
cold/warm construction and mesh storage improved against main. The Mage memory
spike was removed, but its construction regression remains: cold p50/p95/max
384.0/406.7/406.7 ms on main versus 482.5/538.6/538.6 ms here (nine samples per
side); warm 5.609/6.158/6.158 versus 7.457/10.111/10.111 ms. Its pose median
improves from 2.410 to 2.297 ms, but p95 rises from 2.700 to 3.419 ms (270
samples per side). Its cold mesh bytes
fell from 22,360,392 to 18,683,520, summed over the three cold samples in each
round. The shared field-evaluation cost remains a separate optimization target;
lower source counts do not establish uniformly faster construction.

Live creature and found-corpse preparation now runs off the update thread,
including fitting, hair and the articulated rig. Egg-shaped stand-ins retain
movement and selection while work runs; each completed model replaces its egg
on the next update without waiting for the remaining queue or a new server view.
Obsolete results are discarded, current movement/death state is preserved, and
weapon attachment reconciles when a hand becomes ready. One job bounds background
contention and completed payloads; both source caches retain their idle bounds,
with worker asset bookkeeping collected through Bevy's maintenance systems.
Native debug/release client checks cover movement while loading, replacement and
101 finished camera bodies. Quitting during unfinished work also passes locally.
Frame timing waits for finished models before settling; this responsiveness work
does not establish the still-open 60 FPS target. Offline art reviews and the
controlled model workload retain synchronous construction.

Live startup presents the embedded concept artwork in a software window before
starting the world or graphics device. Its bar and information follow completed
startup milestones, then hand over to the prepared game scene while creature
models continue behind their eggs. The illustration and controls fit the display;
startup can be cancelled without leaving its owned world running. Software
milestone captures remain separate from gameplay GPU captures. This improves
startup presentation without changing the offline review timelines or qualifying
the frame-rate target. The [3D guide](../../crates/client-3d/README.md) owns its
current behavior and capture locations.

Shared tube paths now refine by a one-millimeter model-space target,
Bezier control-hull flatness and radius-aware tangent variation. Authored knots,
radii and explicit low-poly settings remain intact; paint spans stay short or
retain the old larger-segment spacing. Bounded subdivision work and authored
angular limits keep refinement finite. Tiny tubes avoid unnecessary side
doubling. Independent dense S-bend references, closed-surface tests and the full
3D library cover the shared change. Remaining head/hand density still needs review. These are geometry counts, not release timings.
`yeti-coat.json` supplies fixed three-angle rest, walk and heavy-action review.
The current body-coat finish is not visually accepted: its close surface pile
still reads as tiled scales. Next work is dimensional stand-off/loft with visible
free tips, retaining root overlap, curved flow, closed topology, resize and mesh
budget coverage. Head fur, hand/claw integration and further silhouette refinement
also remain open.

The [shared organic arm surfaces](../../crates/client-3d/src/creatures/geometry/arm.rs)
serve bipeds and centaur riders. Rider dimensions resolve width, muscle and
shoulder spacing from the build profile while retaining elbow and hand reach.
Deltoid caps belong to the animated arm; a compact root bridges the clavicle
without raising the shoulder crown into fitted armor. Mesh tests cover build
volume, grip placement and joint coverage through supported bends. The native
`centaur-builds.json` review covers both sexes and all builds at rest and walking.

The current people recipes place the head on a short neck and slope shoulders
into the arms. Female pelvic breadth and thigh muscle volume are now independent;
moderate hips place the leg roots, while the build profile shapes a leaner upper
leg and clear knee taper. Emitted-mesh cross-sections cover six humanoid families,
both sexes and all three builds, retaining calf recovery, fine ankles, pelvis
depth and the existing knee/ankle landmarks. Garments use the same revised body
support. These geometric contracts do not establish full style acceptance.
Organic female thighs now share four blended anatomical masses: upper
attachment, quadriceps, distal taper and knee. Width and depth change
independently along the limb. Longitudinal overlap joins the masses without
intermediate pinches, and the upper-shin cap fits the knee's narrower contour.
Build choices retain their muscle differences. The
sculpt, trouser and plate fitting, mesh bounds and posed robe contacts consume
that same description; the robe's analytical rays and contact proxies include
the accumulated blend allowance. Emitted front/side sections check contour
variation and bilateral agreement and the combined knee envelope without moving either joint. Native
front, side, rear and action views remain the artistic acceptance evidence;
these geometric checks alone do not establish the intended silhouette. The
other limb recipes and the separate stone/fur body constructions retain their
existing forms. Medial/lateral planes and the combined knee/shin silhouette
still need continued review against the approved body reference.
Continue reviewing the anatomical skull independently of hair, adult
brow/cheek/nose/jaw structure, connected torso masses and action silhouettes. Long loose hair is one sculpted
fall from the crown, and cloaks, skirts and hair ease into their swings.

Kestrel flight and tail feathers and the Griffin wing feathers share
one closed vane generator. Broad faces narrow progressively into a point, with
a slim central ridge and thickness proportional to width. Family palettes and
Kestrel barring remain recipe choices. Staggered rows of short coverts follow
both spread and folded wing planes, overlapping the longer secondaries. Their
bowed outlines and raised rows keep the short feathers readable as separate
shapes. Folded primary fans mirror their feather planes across the body. Broad
body/wing contact and motion review remain separate work; improved feather
construction does not establish those.

Griffin now has a larger anatomical cranium, cheek and brow beneath a raised
crest, a broad sculpted neck with surface-fitted feather groups, and shoulder
masses that continue into shorter bent forelegs. Wider talons retain planar
support, while the lion hindquarters keep distinct hocks and paws. Griffin and
Kestrel share closed ridged hooked mandibles; their cere and palette remain
recipe choices. Focused geometry and motion tests and native front/side/rear
lineups support this proportion pass. Feather/wing joins, full action/fall
surface clearance and overall finish remain open.

Bat and Dragon skin panels now share a closed tensioned membrane fan with
curved scalloped edges, shallow inter-finger camber, smooth surface normals and
physical perimeter thickness. Explicit outward facing preserves outer/inner
pigments on both mirrored wings. Aquator toe webs reuse the same skin at foot
scale; Bat's wing bones are more substantial. Focused closure, anchor and
mirroring tests and native front/side/rear views support this construction pass.
Complete finger/body contact through every wing pose and the wider creature
anatomy remain visual review work.

Dragon now uses shorter bent fore/hind support vectors, connected shoulder and
haunch masses and broader grounded four-toed claw feet. Its anatomical skull is
larger, and its neck is a continuous sculpt rather than a narrow tube. Shared
`ReptileHead` dimensions align Dragon/Jabberwock lower-jaw geometry and hinges
with the rendered skull scale; their lower-jaw profile is broader. Both recipes
reuse the claw-foot construction. A settled Dragon barrel contract measures its
actual surface against the floor. These contracts do not establish full limb,
wing, neck or jaw surface clearance through every action and fall.

Standard worn boots now use a constructed last: a shaped leather upper above a
layered welt/outsole, plus a tapered block heel for the outfit choices that call
for one. Their raised arch is real geometry; heel and forefoot share a support
plane while flat boots retain their planted midsole. Native and replacement
boots resolve from the same last, and the moving robe receives its conservative
contact envelope. Bare, stone and furry foot recipes remain separate. Shaft,
cuff and buckle construction and complete foot/garment motion appearance remain
continued refinement work.

### 4. Review motion families at real speed

The current broad Rogue pass targets Jabberwock, Aquator, Xeroc, Venus flytrap, Emu and the two snakes. Jabberwock's raised
chest, bent hind support and free grasping arms replace its former four-post
stance; its rig roles drive support selection independently of species names.
Xeroc's large articulated maw and actual overlapping armor masses replace its
small flat mouth and painted shell seams. Aquator's shoulder girdle, haunches,
cheeked skull and fitted eyes establish a stronger amphibian body. Aquator and
Xeroc use short splayed limb profiles and broad grounded pads; walking and local
action support both retain the limbs' lateral offsets. Shared profiled limbs
end in rounded flesh, and Xeroc retains purple shanks and pads with pale nails.
The flytrap has broad fleshy leaves, a substantial stalk and deep cupped lobes
that open to show the red interior and close on contact. Its opposing cilia
reuse the mouth-frame fang rows; rosette and stalk growths share a leaf blade.
Reusable components and the
[future-monster composition procedure](rogue-creature-models.md#composing-future-non-humanoid-monsters)
keep those construction improvements available to later mobs. Whole-form review
still owns visual quality, including shell attachment, open-jaw contact and
silhouette weight. Emu's compact support vectors and body lift are resolved
jointly; substantial hocks, shanks and three-toed soles support its feather mass.
The throat reaches into the breast and the skull itself is larger. Both snakes
have larger broad skulls, hinged lower jaws and fangs that fold as they close.
Their head, jaw, tongue and floor envelope share dimensions. Rattlesnake
default rest follows the maintainer's extended-body direction, with raised head
and rattle. Whole-motion body
and attachment clearance still need review; tests and snapshots do not establish
whole-cast style approval. Ice Monster refinement is deferred while the maintainer
reconsiders its sheet.

Quagga, Unicorn and Centaur now share compact equine support dimensions. The
standing height resolves from the upper link, shorter cannon and actual hoof
sole; fore knees and hind hocks keep different offsets. Ribcage and haunch mass
balance the shorter supports. Broad closed hoof wedges supply a sloped toe,
defined heel and planar sole. Horse and Unicorn nasal bridge, muzzle and lower
jaw share one sculpt. The existing short-leg stride rule keeps the smaller
reach from dragging planted feet through turns. General whole-motion surface
clearance remains visual work, beyond these shape/support contracts. Centaur's
settled side fall uses the same barrel width for body and human-arm support; an
emitted-mesh regression checks all three equine families and builds against the
floor.

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

The first offline composition slice keeps the wardrobe preset separate from an
independent visual loadout. Steel helms, leather boots and travel cloaks resolve
over the preset's head, feet and back pieces; covered earrings and replaced boot
trim are suppressed in the resolved result rather than deleted from the preset.
Selection order does not change cache identity, redressing retains equipment,
and clearing equipment restores the selected preset. The native mage robe still
owns its pose-aware deformation when accessories are equipped. Review these with
`--wear helm --wear boots --wear cloak`; live connections reject the override.
The next torso layer adds an independently selected steel breastplate, fitted
around the body and selected outer shell while retaining sleeves, skirts and
other compatible pieces. Covered bodice ornaments are suppressed in the resolved
outfit; straps and pendants use the combined exterior support. Cloaks reserve
that same plate surface and correct upper-back clearance after their eased
swing, including room for the lining. This remains bounded torso/back fitting,
not general cloth collision. The female front uses the shared anatomy-resolved
neckline cutter and contoured chest supports; the male front keeps its broad
plate. An underlying cuirass is resolved once for rendering and outer-item fitting.
Review armor explicitly with `--wear breastplate --wear cloak`; omit `--wear`
for the princess gown review.

These layers support the existing dressable humanoid fitting recipes. Arbitrary
anatomy compatibility, additional armor regions and general equipment collision
solving remain further composition stages.

The planned equipment system must support compatible creatures beyond the
humanoid wardrobe group. Visible items include armor, weapons, shields, helms,
gloves/gauntlets, shoes/boots, capes/cloaks/robes, amulets, rings and
shirts/undershirts. Current wardrobe presets are combinations of visual parts,
not the eventual equipment-slot boundary.

Keep body identity and anatomical attachment/grasp roles separate from native
costume. Compose independently equipped parts with explicit coverage, inner-to-outer
layer order, fitting and motion clearance: shirts beneath armor, cloaks over
armor, hands holding weapons or shields, and jewelry attached to compatible
anatomy. Finger/ring and grasp compatibility must be explicit rather than inferred
from a humanoid model whitelist. Cache identity must include the actual visible
loadout. These are future requirements, not implemented gameplay equipment
support or approval of a protocol/content-format change; only server-disclosed
items may be shown.

## Living refactor notes: anatomy, attachments and fitting

These notes record implementation findings and intended direction as of
2026-10-23. Update them alongside meaningful generation/composition changes:
record what actually changed, the source of truth, observed limitations and the
next architectural implication. Replace obsolete claims rather than accumulating
contradictory progress notes. Proposed interfaces below are design intentions,
not implemented APIs or approval for a new format or implementation milestone.

### Current implementation

- [Anatomy](../../crates/client-3d/src/creatures/anatomy.rs) resolves
  `HumanoidBody`: dimensions, joint-root positions, limb/foot shapes and optional
  bust anatomy. Bust data already includes useful landmarks such as the fold,
  upper slope and apex. This is shared fitting data, but not a complete named
  attachment/landmark registry for every creature.
- [Creature rigs and looks](../../crates/client-3d/src/creatures.rs) supply named
  joints (`J`), attachment hierarchy and wardrobe/loadout identity. A joint is
  not automatically a garment landmark: the torso joint does not describe the
  waist perimeter, and a hand joint does not fully specify a weapon grip.
- [Garment geometry](../../crates/client-3d/src/creatures/geometry.rs) combines
  body measurements, local recipe constants and fitted surfaces. `NecklineCut`
  ([cloth](../../crates/client-3d/src/creatures/geometry/cloth.rs)) derives
  openings from anatomy; [torso](../../crates/client-3d/src/creatures/geometry/fitting.rs)
  and [pelvis](../../crates/client-3d/src/creatures/geometry/torso.rs) queries
  locate actual surfaces.
  Other heights, offsets and trim paths still live inside individual generators.
  There is no universal request such as “resolve right-hand grip” that all
  species and equipment currently implement.
- [Equipment composition](../../crates/client-3d/src/creatures/outfit/equipment.rs)
  is an offline visual loadout layered over a wardrobe preset. Region overrides
  and suppression rules are explicit but currently item-specific. The
  [breastplate](../../crates/client-3d/src/creatures/geometry/armor_parts.rs)
  uses body/underlayer surfaces and the shared neckline cutter. An existing
  cuirass is resolved once for rendering and fitting another item over it.
  This is not yet a general layer dependency resolver.
- [Cloaks](../../crates/client-3d/src/creatures/cloak.rs) keep anchored shoulder
  rows and use the actual armor surface for bounded upper-back clearance during
  swing. [Robes](../../crates/client-3d/src/creatures/robe.rs) have their own
  pose-aware leg-clearance behavior. These are useful working constraints to
  preserve, not a unified cloth solver or proof of arbitrary-item collision.

### Intended shared description

Distinguish three things rather than treating all placement data as points:

| Concept | Responsibility | Example |
| --- | --- | --- |
| Anatomical landmark | A meaningful position or region resolved from morphology | Waist, neckline, bust apex, shoulder root |
| Attachment frame | Position, orientation and owning animated joint, with explicit coordinate space | Right-hand grip, shield grip, neck pendant, cloak shoulder |
| Fitting surface and motion constraint | Shape between landmarks, layer clearance and behavior while posed | Belt perimeter, armor over shirt, cloak over armor, robe around moving legs |

Each body recipe should provide a shared attachment and fitting description.
Items request semantic roles; the anatomy resolves where and whether those roles
exist. Resolve side and multiplicity explicitly: creatures may have several
hands, unusual feet, wings or no compatible neck. Missing compatibility must be
an explicit result, not a guessed humanoid fallback. Rings need compatible digits;
weapons and shields need grasp orientation as well as a hand position.

Keep body morphology separate from garment design. Shared landmarks do not mean
identical silhouettes: female/male cuts, neckline styles, coverage, ease and
material behavior remain recipe choices fitted to those landmarks. Selecting a
female body must not merely stretch the male garment. Conversely, an equipment
item must not silently change the selected body or replace unrelated outfit parts.

Resolve layers from inner to outer with declared coverage and ownership. An outer
item should fit the actual visible support beneath it, decide which covered
ornaments are suppressed, and expose its resulting surface to the next layer.
Use the same resolved data for mesh emission, trim, attachments and deformation;
independent approximate copies caused the cuirass fitting failure. Anchors alone
cannot prevent clipping between anchors or during animation.

### Open panel garments: first shared cut

The additional `SylvanDuelist` outfit uses `Straps::Panels { width }` rather than
an outfit-specific branch in the renderer. Its recipe is in
[sylvan.rs](../../crates/client-3d/src/creatures/outfit/sylvan.rs); the reusable
[panel cut](../../crates/client-3d/src/creatures/geometry/panel_parts.rs) resolves
two shoulder-to-waist strips from bust apex, lateral bust radius, shoulder
breadth and build. The strips converge toward the waist while exposing the
sternum, lateral bust and torso sides. Their coverage predicate, embroidery
clipping and fitted binding use the same center/half-width function.

This is still the existing fitted-bodice construction: cloth color is carried by
the torso surface, with raised binding and embroidery, not an independently
simulated fabric shell. Free lower panels, lower-arm cloth and the cloak use the
existing separate drape mechanisms. Shoulder/waist endpoint heights remain
recipe coordinates; they should become semantic landmarks in a later migration.
Do not describe this as the complete attachment registry or a cloth solver.

Tests distinguish covered bust apices from intentionally open center/side areas,
including multiple builds, volumes and species. Male wearers explicitly resolve
the separate Ranger recipe; both existing princess recipes remain on the rail.
The initial long cutaway used a lateral split larger than its fitted radius;
front sections reversed their angular order, leaving broken-looking cloth and
floating trim in the native capture. A regression checks angular ordering along
every sampled row, and this recipe now uses a smaller opening. This is a recipe
constraint, not a general solution for arbitrarily large split values. A future
angular opening parameterization should preserve panel orientation by construction.

`SleeveCut::Draped` supplies opaque lower-arm cloth while preserving the existing
sheer `Gauntlet` cut. `Waist::CrossedBelts` uses the shared fitted band generator
with opposite slopes and separate surface clearances, rather than reusing
jewelry cords. The same band function still serves straight sashes unchanged.
`Skirt::waistline` optionally selects a level seam: Sylvan uses the lower belt
line, while existing paired princess panels keep their raised waist. The initial
inherited high seam intersected the crossing belts in close-up review. This is
still an authored height, not a shared semantic waist-attachment service.

Verification for this addition: 48 shared garment-part tests, the two focused
Sylvan generation/coverage checks, five dressing checks and four redressing
checks passed. Release all-target Clippy, the release client build, repository
quick gate and documentation checks passed. The actual-process test rendered
human, mage and orc women in motion; reviewed captures confirm the corrected
continuous lower panels and lower waist seam. An initial harness setup attempt
timed out rebuilding unrelated workspace helpers; rerunning with the existing
release helper directory passed with unchanged source during capture. Full
workspace debug/release suites and native mouse tests were not run for this
checkpoint. These are local, uncommitted changes; no publication or launcher
deployment was performed.

The reference's hip weapon is not an invented gameplay loadout: general sheathed
weapon attachment and disclosed equipment remain separate from clothing fitting.

### Inspection finding: held items need two frames

The first attachment inventory found a concrete duplication to remove during
incremental migration. Both
[`scene::update_held`](../../crates/client-3d/src/scene.rs) and the
[bestiary wearer spawn](../../crates/client-3d/src/bestiary.rs) construct the
same sword rotation and hand-local translation independently. The live path
used a different forward offset for the short sword; the preview used the
long-sword offset. This duplication is now replaced by `props::held_transform`,
which both paths call. Sword placement aligns the authored handle midpoint to
the existing hand-local grasp convention instead of estimating forward offsets.

The [prop generator](../../crates/client-3d/src/props.rs) authors the sword lying
flat. Its handle shaft runs from `(-0.235, 0.03, 0)` to `(-0.4, 0.03, 0)` before
item scale; the short sword scales the entire model by `0.62`. Handle geometry
and handle-frame metadata now share their endpoints and scale. A regression
measuring the emitted handle found the old greatsword midpoint 0.0275 local units
behind the target grasp. The new mapping removes that discrepancy for both sword
sizes and preserves their 0.62 size ratio. Those coordinates describe the existing
asset, not a proposed universal grip position.

The humanoid hand is itself a child of the forearm, translated by `body.hand`
and scaled by `1.22`. Its finger geometry also depends on body build. Hand
animation is applied to that joint. Centaur hands have a separately authored
rest offset, and their native spear has its own attachment rotation. A blanket
replacement of every hand attachment would therefore change working poses or
misapply a humanoid fist assumption to other anatomy.

The intended relationship is **body grasp frame × inverse item handle frame**:
first locate/orient the grasp in the animated hand's local coordinates, then
align the item's authored handle to it. Define the scale policy explicitly;
parent scale must not accidentally resize equipment merely because a hand is
larger. A hand-local grasp and a prop-local handle need documented axes and
units, and neither is the hand joint origin by definition. Preserve item size
separately from matching the grip unless the item recipe explicitly permits
resizing.

Before changing these consumers, establish witnesses for the actual handle
axis passing through the grasp, blade direction, short/long weapon size,
nonuniform/parent scale, and attachment stability through wrist animation and
redressing. Check preview and live presentation against the same resolved
attachment. Keep the missing-creature/no-invented-hand contract. Determine which
non-humanoid grasp roles can supply a compatible frame before expanding support;
a joint called `HandR` is not sufficient evidence of a usable grip.

This is an item-side migration, not a completed anatomy attachment API. The
hand target remains the legacy local convention `(-0.03, -0.02, 0)`; it is not
yet resolved from each body's finger geometry. Existing parent-hand scaling and
non-sword presentation are preserved. Missing sword-handle metadata returns an
explicit absence, but the consumer still retains its old fallback for other
items. Replacing that fallback, resolving body grasp frames and defining equipment
scale independently of parent scale remain outstanding. Emitted-handle tests
cover alignment, blade direction, item size and wrist-frame composition; they
do not prove arbitrary hand fit or nonuniform scale compensation.
The live-scene integration witness covers equip, short/long weapon replacement,
unchanged-entity reuse and unequip cleanup. Native bestiary captures and a warrior
portrait show the held sword; five-person wardrobe lineups deliberately omit
weapons and cannot establish this rendering contract. A whole-bestiary wardrobe
cycling attempt timed out while repeatedly redressing 16 humanoids, with pending
capture files. Its cause is not established, and it is not passing stress evidence.
The fixed-outfit native case passed; broad cycling/readback responsiveness remains
an investigation item separate from handle alignment.

### Migration and evidence to preserve

Inventory embedded coordinates and their spaces before moving them. Introduce
named accessors around existing resolved anatomy first; migrate one shared
consumer at a time with before/after rendering evidence. Avoid moving constants
into a new file and calling that a semantic landmark system. Keep unresolved
questions explicit: surface ownership/lifetime, rest versus posed frames,
multiple attachment roles, layer conflict policy and cache invalidation.

Preserve deterministic appearance and loadout-aware cache keys, reversible
redressing, anatomy compatibility, feminine/masculine garment choices, and current
robe/cloak clearance. Validate across species, builds and poses using resolved
landmarks, emitted triangles and native captures. Numeric clearance checks do
not establish visual quality: the current breastplate neckline still has visible
edge faceting despite passing coverage checks. Keep gown-only captures separate
from equipment studies so optional armor cannot conceal the outfit under review.

Gameplay item disclosure, protocol changes and external content formats remain
separate decisions. The refactor should support the planned armor, weapons,
shields, headwear, handwear, footwear, outerwear, jewelry and underlayers without
claiming those slots or arbitrary creature compatibility are already implemented.

## Review and verification discipline

Keep the original dirty development checkout protected. Use the isolated art
branch and preserve unfinished work before rebasing. Never edit checked Rust
inputs during a build or run competing owners against the same target directory.

During model refinement, the maintainer requested simplified testing for rapid
iteration: run focused model/motion regressions and native previews, and set
aside mouse tests for these passes. For the 2026-10-21 sharing checkpoint, the
maintainer explicitly authorized pushing this feature branch without running
the push gate. This task-specific adjustment does not change the repository's
default gates or the complete Windows/Linux debug/release CI required before
merge.
Run the targeted art-contract tests, the renderer checks affected by rebase,
documentation/link checks and the native offline lineup witness. Live-backend
and combat process coverage remains with CI. Record failures
and limits honestly. The independent 3D workflow runs Windows/Linux debug/release
verification; its full result is required before merge. Do not weaken tests or
claim manual visual approval from a passing automated suite.

For the sharing checkpoint, `cargo test --manifest-path
crates/client-3d/Cargo.toml --workspace --lib --locked` passed 206 renderer
library tests and the model-routing library test. One subprocess fixture is
intentionally ignored for direct invocation and exercised by its owning
lifecycle test. The 18 documentation tests passed. Formatting and warning-free
Clippy passed on the same code. Both focused native process owners passed:
Rogue family lineups reached captured GPU frames, and the live wardrobe test
cycled replacement outfits across nine wearer families. The latest native
binary and its recorded source fingerprint match the shared code. Earlier
shape passes reviewed front, side, rear, action and settled-fall captures;
the latest footwear pass adds native and replacement-outfit views.
These results do not qualify the current tree in release or establish a full
verification-gate pass. Earlier broad renderer checks predate the final equine
and footwear changes; earlier native mouse failures remain recorded as
failures, not passes.

New body/outfit behavior should extend the narrow owning regression tests and
actual-process witness. Existing interaction, appearance identity and authority
tests remain relevant without duplicating their assertions in art-specific tests.

### Contract witnesses

| Contract | Owning witness | Limit |
| --- | --- | --- |
| Bounded idle reuse while visible meshes survive | [Cache regressions](../../crates/client-3d/src/props_art_tests.rs), including key and byte boundaries | Caller-owned insertions become idle when dropped and are accounted for by periodic pruning |
| Correlated bodies and actual rig landmarks | [Body contract tests](../../crates/client-3d/src/creatures/art_contract_tests.rs) | Does not grade anatomy or character appeal |
| Continuous structured torso and fitted waist | `geometry::art_contract_tests` in [geometry](../../crates/client-3d/src/creatures/geometry.rs) | Sampled surface properties, not full garment/contact approval |
| Native/swapped torso binding, harness/scarf support and sex-specific Vampire cravat | [Emitted garment contracts](../../crates/client-3d/src/creatures/geometry/garment_tests.rs) and native Rogue/wardrobe process owners | Static contact, coverage and recipe checks; binding shading, neckline construction, painted edges and motion clearance remain open |
| Connected lined cloaks, supported yokes, pinned motion and rear strap layering | [Cloak contracts](../../crates/client-3d/src/creatures/cloak_tests.rs), [owned sheet tests](../../crates/client-3d/src/creatures/cloak.rs) and emitted garment contracts | Native/replacement selection and mesh construction are checked; general body/floor collision and whole-motion appearance remain open |
| Finite closed neutral cloth and independent per-wearer deformation | `robe::art_contract_tests` in [robe](../../crates/client-3d/src/creatures/robe.rs) | Topology and mesh ownership are checked; full contact clearance and real-speed continuity remain manual review |
| Default backend, single-sample cameras and software frame pacing | `graphics::tests` in [renderer setup](../../crates/client-3d/src/graphics.rs) | Native process evidence supplies actual renderer startup/readback |
| Observer-independent disclosed appearance | `scene::tests::observer_role_and_interaction_target_do_not_change_the_rendered_appearance` in [scene tests](../../crates/client-3d/src/scene/tests.rs) | Existing identity boundary retained |
| Generated models presented by a real native window | `Native3DProcesses.test_art_lineup_presents_generated_bodies_and_accepts_native_close` in [process tests](../../crates/client-3d/scripts/test_client_process.py) | Screenshot integrity and native window lifecycle, not automated aesthetic judgment |
| Disclosed combat still dispatches real client motion | `Native3DProcesses.test_native_click_attacks_the_selected_actor_and_plays_its_motion` in [process tests](../../crates/client-3d/scripts/test_client_process.py) | Existing server/transport/native-input witness retained |


## Scripted scene fixture design

Status: proposed extension to the existing offline client, not implemented. The
user requested repeatable scripts for composition, placement, cameras, animation
triggers and screenshots. This is review infrastructure within the model-quality
work; it does not choose the game's future runtime scripting language or change
protocol, save or scenario formats. Keep existing lineups as shortcuts.

### Existing mechanisms and the actual gap

- [Bestiary](../../crates/client-3d/src/bestiary.rs) builds seeded bodies, applies
  wardrobes and equipment, schedules actions and moves the review camera. Its
  `lineup` uses `Time<Real>`, named shot deadlines and pending-readback checks.
  Other directors use the local `clock` function, including travel-study time.
- [Animation](../../crates/client-3d/src/creatures/animation.rs) advances `Anim`
  using a capped frame delta, derives walking from root displacement and retains
  contact state. Sampling only an action timer cannot reproduce the full pose.
  Cloth and smoothing retain history too; they must advance with the same clock.
- [Diagnostics](../../crates/client-3d/src/diagnostics.rs) already observes GPU
  screenshot completion, bounds in-flight readbacks and reports renderer and
  actor evidence. Reuse that mechanism rather than a separate image renderer.
- [Client setup](../../crates/client-3d/src/lib.rs) orders animation and related
  presentation systems. Explicit ordering through transform propagation and
  render extraction is needed before a scripted screenshot is requested.

A wall-clock wrapper around existing CLI switches cannot guarantee a selected
pose. The fixture must drive production composition and animation systems with
controlled steps, including garment motion, without creating another poser.

### Capture hold foundation

The offline bestiary now installs
[ReviewClockPlugin](../../crates/client-3d/src/review_clock.rs). Before Bevy updates
its time resources, a pending screenshot pauses virtual presentation time; the
clock resumes after the last pending request disappears. Real time keeps moving
for deadlines. The barrier releases only a pause it acquired, preserving a
preexisting pause. Animation, cloth and soft-body smoothing consume the common
virtual clock. The plugin is installed only in the offline bestiary, leaving
the live client's clock source unchanged. Animation now skips zero-duration
updates in both paths: recomputing a pose without advancing time could still
change contact-dependent joint transforms.

The regression uses a generated character performing a jump and checks that its
action phase and joint/cloth transforms remain unchanged across delayed readback
updates, then advance after completion. Previously only the lineup's camera and
family were held. This is a capture barrier, not yet fixed-step script execution:
script parsing, integer tick advancement, scene/event controls, readiness checks
and per-capture manifests remain to be implemented and verified.

The native acceptance test now checks hold/resume timestamps in the actual client
and validates the named rendered images. Unit coverage also preserves real-time
watchdog progression and a preexisting pause. The generated-body regression,
existing animation/bestiary checks and the wardrobe composition matrix pass for
this foundation. This evidence establishes the capture hold, not deterministic
fixed-tick replay or completeness of the proposed fixture runner.

### Script contract

Proposed entry point: `--scene-script PATH --capture OUTPUT`. This spelling is a
proposal, not a supported command. Use strict, versioned declarative TOML with
unknown fields rejected. The initial vocabulary should cover the following:

| Record | Required behavior |
| --- | --- |
| Scene | Schema identity, deterministic seed, fixed step rate, viewport dimensions, lighting, background and ground |
| Actor | Unique ID, model identity, explicit body variation/seed, wardrobe, equipment, world position, orientation and scale |
| Camera | Named perspective or orthographic camera, explicit position and target, up direction, clipping range, field of view or orthographic extent |
| Event | Integer tick, stable sequence order, actor/camera target and typed operation |
| Composition change | Redress or equip/unequip through existing composition APIs; preserve unrelated actor identity and item slots |
| Placement/movement | Explicit teleport versus motion segment; movement segments advance root position per tick so locomotion receives displacement |
| Animation | Typed existing action, with explicit start/restart or cancellation semantics; unsupported actions fail validation |
| Capture | Unique safe name, tick and camera; wait for the corresponding completed GPU readback |

Use integer ticks at a declared fixed rate as the authoritative timeline; display
seconds only as a derived value. Validate rates against animation's delta cap so
one requested step is not silently shortened. Tick zero is the initialized pose.
For subsequent ticks, apply ordered state-changing events, advance one step,
propagate transforms and render, then service captures. A zero-age action capture
at tick zero needs an explicit initialization path, not a fake movement step.
Multiple cameras at one tick render sequentially while model state stays frozen.
Reject nonfinite values, invalid projection ranges, coincident camera/target,
duplicate IDs or capture names, unknown references, conflicting modes and output
paths escaping the chosen run directory. Bound actors, events, duration and
readback timeout so malformed scripts cannot run indefinitely. Unsupported
anatomy/item combinations must fail explicitly or record an explicitly requested
fallback; silent Ranger substitution would conceal a fixture authoring error.

### Execution and capture barriers

Use a director state machine: validate, compose, prepare, advance, render/capture,
finish or fail. Real time is for watchdogs and rendering readiness only. Warmup
must not age animations, cloth, procedural lighting or camera motion. Readiness
should include scene creation and relevant render preparation; a fixed sleep is
not evidence that pipelines are ready. Put a bounded failure deadline around it.

All presentation consumers affecting a fixture must share the controlled clock:
actor/root motion, action timers, gait/contact state, smoothing, robe and cloak
motion, effects, procedural lighting and camera interpolation. Audit their time
sources before claiming deterministic capture. Preserve ordinary live-client time
behavior. Advance from a fresh initial state through every fixed step; arbitrary
seeking must reset and replay rather than overwrite timers on an evolved rig.

At a capture tick, hold state until transforms and render extraction represent
that tick. Do not advance while its readback is in flight. Record completion only
after a valid image has been written, using a temporary file and final rename.
Do not infer success from a placeholder file or a queued screenshot entity.
Finish successfully only when every declared capture has a matching completion
record; missing/failed captures produce a nonzero exit and an incomplete manifest.

### Evidence and acceptance

Each run gets a fresh output directory and manifest containing script bytes/hash,
resolved configuration and actor variants, source revision plus dirty-input hash,
executable identity, viewport and camera state, fixed-step rate, event order,
capture tick, resolved action/phase, renderer/driver details, image dimensions and
hashes. Distinguish the pose requested from the pose actually submitted for
capture. Keep routine output machine-local. A test runner supplies build identity;
the renderer must not fabricate provenance it cannot establish.

Repeatability means matching resolved state and tick progression on the same
inputs, with an explicitly scoped visual comparison. Do not promise bit-identical
pixels across GPU vendors or drivers. Pixel checks need controlled render settings
and tolerances; geometry, transforms and semantic state remain separate evidence.
A valid PNG alone never proves correct fit or animation.

Implementation acceptance requires:

1. Parser/validation tests for every supported record and rejection boundary,
   including invalid references, duplicate output names and path containment.
2. Director tests proving stable same-tick event order, delayed readback without
   pose advancement, timeout/error propagation and complete-capture accounting.
3. Composition integration tests for multiple bodies, wardrobes and visible gear,
   preserving identity and unaffected slots across scripted changes.
4. Timing integration tests for walking, actions and cloth, comparing state at
   capture ticks despite different wall-clock/render delays. Retain existing live
   animation and lineup behavior tests.
5. Actual-client process tests that load a fixture, render named front/side/back
   and moving views, validate their manifest/image agreement, repeat the run and
   exercise a failing script/capture. Use the client executable, not a mock.
6. Human visual review of the captures. Start with Sylvan bust/build variations,
   both mage bodies, a broader nonhuman body, and outfit-plus-equipment layers.
   Keep an ordinary body/outfit fixture as a control against regressions.

Implement the shared clock and capture barrier first, then wire strict scripts to
composition, placement, cameras and events. This sequencing is not a reduced
scope: all requested controls and actual-process evidence are needed before the
scripted fixture is described as implemented. Keep the general model-quality goal
open after this infrastructure lands.

## Bounded composition coverage

The expensive wardrobe cross-product is now split into cheap exhaustive recipe
checks and bounded generated-body checks. The former still visits all 682
model/body/outfit combinations in its existing domain to verify dressability,
native identity restoration, recipe resolution and distinct wardrobe cache keys.
It does not construct meshes.

The generated-body test uses 47 permanent witnesses plus 32 seeded Monte Carlo
cases. Permanent witnesses cover native and Knight outfits on each of the 11
model identities and both body variants, the female Sentinel's loose hair in
Courtier clothing, and Medusa's serpents in Commoner clothing on both variants.
Random cases vary model, outfit, individual seed, body build, bust volume, contour,
asymmetry and figure. Their fixed count does not grow with the outfit catalog.

Coverage ownership from the former exhaustive geometry test is explicit:

| Original assertion | Current owner |
| --- | --- |
| Dressability, native identity, distinct cache keys, recipe resolution | Cheap exhaustive recipe test |
| Helmet hides hair and contains pointed ears | Permanent Knight witnesses for every previously covered model/body pair |
| Female Sentinel shows loose hair outside her helm | Permanent female Sentinel/Courtier witness |
| Medusa retains intrinsic serpents | Permanent Native/Knight/Commoner witnesses and sampled compositions |
| Finite mesh vertices, hair state, cloak/skirt/peplum/layer counts, limb metal matching recipe | Same assertion helper on every permanent and sampled generated case |
| Ghost material exception | Preserved in the shared helper; permanent native/Knight ghost witnesses |

The expensive test no longer proves the entire cross-product. The dedicated bust,
hip, neckline, layered-skirt, mage-redressing, equipment-clearance and animation
regressions remain unchanged; random cases do not replace known failure witnesses.
The earlier exhaustive result remains historical evidence for its tested inputs,
not a claim about future combinations.

`TOR_COMPOSITION_SEED` selects a decimal unsigned 64-bit batch seed; the default
is stable. SplitMix64 is specified in the test helper to avoid changing replay
when a random-number library changes. Before each generated case the test emits
its batch seed, index and resolved model/outfit/body/individual seed into captured
test output, so a failure can be replayed with the same seed and source revision.
Successful output stays in local test logs. Invalid seed text fails explicitly.
The script fixture runner is not implemented yet, so these are replay records,
not executable scene-fixture files.

Future CI can rotate recorded seeds and weight extra samples toward changed
parts and known conflict classes. That rotation and weighting are not automated
by this first bounded sampler. Keep permanent regressions, budgeted random cases
and actual rendered/animation evidence distinct in reports. Promote a newly
understood random failure to a focused permanent regression rather than enlarging
the full Cartesian product.


### Optional fixed-step lineup clock

`--lineups --review-step-hz 50` selects a fixed presentation increment per
rendered update. Rates from 20 through 240 Hz are accepted; lower rates would
exceed the production animation delta cap. The review clock snapshots virtual
time before Bevy's wall-clock update and restores it with one selected step
afterward. The generic presentation clock receives that same value, so animation,
cloth and smoothing share the increment. Real time remains untouched for process
deadlines. Pending screenshots advance by zero and retain pause ownership
semantics. Existing variable-time lineups remain the default.

In this mode lineup deadlines also consume presentation deltas. The option is
restricted to offline lineups and rejected for live connections or galleries.
This makes frame progression repeatable; it does not establish deterministic
GPU pixels, shader readiness or a complete scripted timeline. The rate is rounded
to a nanosecond duration; authoritative integer script ticks, explicit events,
cameras, scene manifests and initialization remain pending. In particular,
existing lineup view deadlines may clamp their local schedule independently of
the animation step; they are not yet exact scripted capture ticks.


### Fitted waist attachment ownership

The fixed-step review exposed a mismatch between fitting and animation ownership:
ordinary belts followed the torso, but crossed leather bands and sashes were
emitted on the independent hip joint despite being fitted to the torso surface.
A generated-body jump regression reproduced loss of the crossed band's intended
clearance within two updates. Increasing mesh clearance would only mask that
frame mismatch.

`Waist::attachment()` now declares the owning joint for shared waist components:
sashes, crossed cords and crossed leather bands use the torso; jeweled skirt
frameworks and low coin ornaments use the hips. Generation transforms fitted
components from their authored hip coordinates into the torso attachment frame,
using the same path as the ordinary belt. Native caster accessories preserve
their existing coupled hip/robe frame. Geometry, colors and outfit recipes remain
unchanged.

This is an explicit component-level attachment rule, not the future complete
landmark registry. A later fitting description should carry the owning frame with
surface/anchor results so emitters cannot silently fit in one frame and animate
in another. Hanging tails may ultimately need separate secondary attachments;
this change keeps each current accessory rigidly with its declared parent. It
does not claim to eliminate all hip shelves, layered-cloth gaps or bust-panel
issues visible in the review.


### Paired panels resolve each side independently

The shared open-bodice panel fitter previously mirrored the right-side apex and
used the unscaled base bust radius for both halves. The anatomy already resolves
independent mass radii and apex heights for asymmetric bodies. A regression at
the resolved lateral panel edges showed the fuller left side losing its intended
coverage; the reverse asymmetry is covered by the same test.

Panel span resolution now accepts a side and uses that side's mass radius and
apex height. Coverage and the raised binding both call that resolver. Symmetric
bodies keep matching spans, while the shared shoulder and waist endpoints remain
unchanged. This is fitting behavior of the reusable panel component; it does not
replace the outfit or alter the authored neckline/coverage fraction. Tests cover
both asymmetry directions, symmetry preservation and binding centers staying on
the corresponding panel.

For the later fitting refactor, paired landmarks must remain side-specific from
anatomy through pattern coverage and decorative geometry. Mirroring a resolved
right-side fitting result silently discards asymmetry even when the underlying
body is correctly generated. Full panel shape refinement against the reference,
and a complete named landmark registry, remain separate unfinished work.


### Asymmetric fitting of centered necklines

Sweetheart/strapless top edges previously mirrored the right-side mass, while
plunge clearance used only the right apex and the unscaled base radius. Focused
regressions reproduced a left peak missing its resolved slope and a left-side
plunge exceeding its intended clearance on an asymmetric small-volume body.

Sweetheart peaks now use each side's resolved mass. Both halves share the lower
of their fitted center dips and a common level back, keeping the center
continuous rather than introducing a seam when the fit switches sides. Plunge
openings remain centered and symmetric, but their width is constrained at both
resolved apex heights using the corresponding mass radius. Coverage and raised
neckline binding already share these evaluators, so the corrected fit reaches
both paths without a second outline implementation.

The tests preserve center continuity and exercise both asymmetry directions,
including small and large volumes for plunge clearance. This extends the panel
fitting correction to shared sweetheart and V-cut geometry. It is not a claim
that every garment resolver is side-aware: armhole, halter, one-shoulder and
centaur-wrap paths still need their own fit/continuity review before a complete
landmark abstraction replaces the remaining local calculations.


Visual follow-up: the post-change royal-princess close-up shows small jagged
cloth-color patches beyond portions of the gold neckline binding. The numerical
edge/clearance regressions do not prove a clean rasterized boundary. Investigate
how mesh tessellation and vertex-color coverage meet the fitted ribbon; preserve
this distinction when assessing the mathematical fit versus rendered quality.


### Hard garment coverage boundaries in sculpted meshes

The close-up's cloth-color spill had a mesh-level cause: a surface-net triangle
could contain both skin and cloth vertex pigments. Raster interpolation then
blended the two across a fitted neckline even though the coverage evaluator and
binding path agreed mathematically. An emitted-mesh regression reproduces this
with a hard two-color cut on a sculpted sphere.

`Sculpt::mesh_partitioned` accepts an optional binary coverage evaluator. The
ordinary mesh path remains available. Crossing faces are locally subdivided and
clipped against that evaluator; the two regions share geometric intersection
positions but sample their pigments just inside their own side. Surface position,
winding, interpolated normals and baked occlusion are retained. Interior faces
keep their original cached attributes. Corner, midpoint and center samples catch
cuts turning through a face whose corners agree, without increasing the entire
sculpt grid's resolution. Body-painted bodices use this for their garment/skin
coverage, including neckline openings and bikini coverage.

This is a bounded piecewise-linear boundary approximation, not a replacement for
true separate cloth shells or per-pixel material masks. Features smaller than its
sampling scale can still be missed, and detail-pattern or wound-color boundaries
are not classified by this garment mask. The tests check mixed-pigment removal
in the final mesh, area and winding preservation, normals/shading, unchanged
interior handling and a region inside otherwise matching corners. The native
renderer and close-up review are still needed to assess the visible result.


The matching post-change princess close-up shows a cleaner central neckline,
with the stair-stepped cloth-color spill removed there. Cloth still rises above
the binding toward the underarm. That remaining front/back outline disagreement
needs a separate fitting investigation; successful hard-boundary mesh tests do
not establish that every binding follows the complete three-dimensional cut.


### Closed upper-edge binding on the fitted surface

After hard pigment boundaries were corrected, a separate underarm mismatch
remained: the binding used independent front/back projected paths, while the
bodice used a continuous front-to-back coverage blend. The emitted-binding
regression found a side vertex roughly 0.034 model units below its actual cut.

Sweetheart and strapless binding now trace a closed curve around the torso.
At each angular sample a bounded height solve finds the upper edge on the actual
sculpted support, evaluating the same coverage height and shared `bodice_front`
blend. A narrow band follows that surface with outward-normal clearance. This
replaces the disconnected front/back path assumption and its side extrapolation;
it does not change the authored garment cut. The new `neckline_parts` component
owns the surface tracing and emission, separate from outfit recipes.

Tests compare emitted trim with coverage, check that the resolved curve closes,
and check surface distance and edge-height agreement around Human, Mage and
Nymph bodies for both supported cuts. The tracer assumes a single upper edge
within its supplied height bracket and a radial torso surface. Arbitrary cutouts,
compound necklines and non-radial creature surfaces still need explicit tracing
contracts; this is not yet a universal attachment/landmark registry.


The first surface-traced implementation failed the whole-torso edge-agreement
test because the general-purpose sphere marcher's coarse hit tolerance made its
returned radial point jump during the outer height solve. This path now uses a
bracketed radial solve with tighter precision; the original edge-agreement
tolerance was retained and passes. The general sculpt marcher is unchanged.
The retained fixed-step princess close-up confirms the gold binding now continues
around the visible underarm curve; the previous large cloth-over-trim mismatch
is removed there. A narrow purple rim and faceting remain visible near parts of
the edge, so this is not a claim of pixel-perfect alignment between the sampled
paint boundary and offset trim. Garment regressions, sampled compositions and
the native princess motion check pass. The full scripted fixture and broader
surface/attachment contracts remain future work.

### Binding clearance and visible cut overlap

The closed trace initially solved the body edge and then lifted its trim along
the surface normal. That final lift changes height and lateral coordinates on
sloped surfaces, so an edge aligned before clearance can leave a visible cloth
rim afterwards. An emitted-strip regression checks that each patch overlaps
both sides of the coverage boundary for strapless and sweetheart cuts. It fails
on the previous implementation. The fitting solve now includes the normal lift,
and resolves the two edges at equal signed height margins about the cut. This
keeps the six-millimeter band centered on the visible boundary rather than
entirely below it. Rendered inspection and tests are required before claiming
that the remaining visual rim is resolved.

The matching fixed-step princess capture shows a more continuous front binding,
but a thin dark/purple line is still visible near the underarm. The clearance
regression proves overlap in the coverage field, not that the raised strip
perfectly follows the triangulated visible surface at every viewing angle.
Further geometry/occlusion work is deferred while the scripted review runner is
implemented. Garment tests, focused/seeded compositions and the native princess
motion process check pass for this change.

### Script runner implementation checkpoint (in progress)

The first executable runner uses strict JSON, reusing the client's JSON stack,
instead of adding the TOML dependency proposed above. The proposed broader
contract remains the destination. `--scene-script PATH --capture NEW_DIRECTORY`
is the entry point; other review/live options are rejected in this mode.
`crates/client-3d/review-scripts/wardrobe-motion.json` is a front/side/back and
motion example. These files are offline review inputs, not gameplay scripting
or a persistence-format commitment.

The initial schema supports exact model identities, explicit seeds and body
variations, wardrobe and the four currently composable equipment choices,
positions/yaw, perspective cameras, ordered actor events and tick/camera capture
requests. Events support action restart/cancel, velocity, teleport, dressing and
replacement of the explicit equipment loadout. Dressing preserves equipment;
equipment replacement preserves wardrobe. Teleport resets animation state and
retains velocity. Same-tick composition operations are applied in source order
and rebuild each actor once. All actors use the existing generator and rig.

Tick zero is the generated rest pose; action starts at tick zero are explicitly
rejected pending an initialization-without-advancement API. Later ticks apply
events, move roots, then advance the existing animation by one bounded step.
Camera switches and pending GPU readbacks hold the presentation clock. Output
requires a new directory; image writes use temporary PNGs and rename. A manifest
records the script, resolved composition keys, actor root/action state, camera,
tick, presentation time, renderer and image dimensions/fingerprints. FNV-1a
fingerprints are labeled as such and are not cryptographic provenance. Source
revision, dirty-input identity, executable digest, full joint/cloth evidence,
orthographic cameras, configurable scene lighting, and richer motion/attachment
operations remain unfinished. No claim of full fixture acceptance is made yet.

The first native process acceptance passes: it runs the same script in two fresh
processes, compares captured root/action and selected joint transforms, verifies
two cameras at tick zero hold identical actor state, and checks a same-tick dress
plus equipment change leaves no extra rig. A separate process test rejects bad
input before output creation. Clock/unit tests cover held phases, one-step
advancement, schema rejection and timeout failure. Render readiness waits for
tracked mesh uploads and an empty pipeline queue; all reported pipeline errors
currently fail the run, including potentially transient shader-loading errors.
That transient-versus-terminal classification still needs refinement.

The retained example captures show the duelist jumping and the native mage's
robe deforming as she walks. Initial actor positions were corrected to the fixed
stage ground at Y=-0.5 after visual inspection. The armored jump exposes small
skin-colored breaks near the lower sides of the fitted breastplate, plus the
already known waist discontinuities. These are concrete fitting regression
candidates; a successful script capture does not mean its models are free of
clipping. The example is now a reusable input for that investigation.

### Hair contact explains the apparent breastplate holes

The closer script capture and a hairstyle-only control correct the earlier
interpretation: the brown patches are long front hair locks emerging through
the armor, not skin escaping its fitted envelope. The added lateral torso test
passes both with all metal triangles and with front-facing triangles only.
Keeping the body, outfit, equipment and camera fixed while selecting a bun
removes the marks. A separate emitted-hair regression fails on the original
loose-hair mesh: one front vertex lies at Z=0.0074 while the plate at that XY
location is at Z=0.1800. The body-only clearance checks could not cover this.

Loose hair previously shared a cached mesh and only rotated as a whole beneath
the head. It had no contact contract with torso layers. The implementation in
progress keeps cached source hair but clones instance meshes for fitted contact.
Its support combines the actual body, selected garment shells and equipped
armor. Head and drape transforms are converted into torso/support coordinates
before contact correction; the scalp attachment stays fixed. The deformer is
called after head pose and eased hair swing, and rebuilding composition creates
new support without mutating another wearer's shared source mesh. This is a
contact projection, not a hair physics simulation or a universal collision
system. Motion, cache isolation and visual quality still need verification.

The first contact attempt passed its vertex tests but failed two stronger checks.
A triangle-center witness still crossed the torso (about -0.032 field distance),
and the native jump capture reverted to apparent holes. The latter exposed an
asset-lifetime mismatch: ordinary Builder meshes are render-only and release CPU
data after extraction, while the no-render unit harness retained it. Editable
hair now retains MAIN_WORLD as well as RENDER_WORLD, including the cached source
needed to redress an already rendered wearer. Mesh positions remain per-instance.

The fitting revision uses the same authored lock centerlines as hair generation,
reserving each lock's radius against a clearance isosurface. It translates the
lock section instead of flattening its individual vertices onto the body; this
also avoids classifying opposite faces of one lock on opposite body sides.
The existing torso ray query now delegates to a clearance-aware version with
zero clearance, preserving its prior contract for other callers. Triangle,
vertex, upload-retention and native-motion checks remain necessary evidence.

A continuous radial contact trial passed the animated triangle and vertex
regression and the static emitted-hair check. A native scripted jump also kept
hair outside the plate after renderer upload, confirming the CPU asset retention
fix. It is not accepted as the final shape: the loose-hair bounds regression
failed because a nymph's half-width grew from about 0.20 to 0.34, and the close
capture visibly flared the front locks sideways. The existing bounds assertion
is retained while contact direction is refined. The fitting must preserve a
plausible drape as well as clear the body; passing collision tests alone is not
sufficient. Full affected tests and the quick gate must be rerun once this
geometric iteration is stable.

The follow-up directional trials are also incomplete. Biasing the radial ray
from the torso center toward front/back narrowed the direction but left a face
penetration of -0.0027 at rest. Casting instead from each current lock center
avoids relocating whole sections across the chest, but the animated regression
still finds a triangle at tick 1 near (0.1961, 0.3987, 0.0194), with field distance
-0.00668. This latter trial is the current implementation. No test tolerance was
relaxed. Adjacent vertices can choose different authored centerline segments;
independent section projections therefore do not guarantee that their connecting
faces remain outside the support. The next refinement needs contact constraints
on connected mesh faces, with fixed scalp vertices and preserved source geometry,
rather than another arbitrary clearance increase. Cache-isolation coverage and
the full stable-checkpoint gate remain outstanding. Documentation and whitespace
checks passed after the radial-trial notes; later trial notes require rechecking.

A connected-face contact pass now follows the authored-lock projection. It
samples triangle centers and edge midpoints in support coordinates and distributes
outward corrections through the shared vertices using their barycentric weights;
scalp vertices have zero mobility. The bounded iteration fixes the retained
animated face witness and the static emitted-hair regression. Native rest and
jump captures confirm the post-upload deformation remains active. This is still
an experimental fitter, not accepted model quality: the native captures show
broad stretched shoulder wings, and the original nymph hair-bounds test still
fails (half-width about 0.33 versus about 0.20). The test has not been loosened.
Contact projection needs a strain/shape-preservation contract that distributes
motion through connected hair, or a better authored deformation representation;
clearance constraints alone cannot provide that. The current pass is neither
continuous collision detection nor proof against intersections between its
sample locations. Runtime cost also needs profiling before acceptance.

A separate isolation regression composes two wearers through the same asset
library, snapshots existing mesh positions, then deforms only the first wearer.
It checks that composition leaves existing meshes intact and animation changes
neither cached sources nor the other wearer. This covers the intended ownership
contract independently of whether the contact shape is aesthetically acceptable.

The next trial adds source-edge stretch constraints interleaved with contact.
The same Human/Sylvan/breastplate rest and jump views no longer show the broad
shoulder wings; an additional Nymph/native-outfit capture also shows a plausible
shoulder drape. This is stronger visual evidence but not sufficient acceptance:
a new animated edge-strain assertion fails on the first trial (source length
0.00593 becomes 0.01699). More edge-relaxation sweeps between support sweeps are
being tested without relaxing that assertion. Support-query cost is substantial
and remains an optimization requirement; the bounded solver is experimental.

The old loose-hair test's exact unfitted-bounds comparison is now applied to the
actual cached source coordinates, transformed by the same head scale and crown
pivot. Its rendered crown positions are separately compared to those coordinates.
The existing walking/turning swing and cloth-motion assertions are retained.
This replaces only the obsolete expectation that collision-fitted lower hair
must keep every bound of the intersecting source shape. Animated face/vertex
clearance and edge-strain assertions in hair_contact cover the fitted fall;
instance-isolation coverage protects both the cached source and other wearers.
The replacement motion test and stronger contact test must both pass before
claiming the revised contract is verified.

The eight-relaxation-sweeps trial still fails the unchanged strain bound at tick
zero: an edge of length 0.00830 becomes 0.02140. The source-placement/crown and
walking/turning/cloth-motion test passes in full with its retained motion
assertions. Its runtime is too high for accepting the current deformation loop.
The improved Human and Nymph captures above came from the preceding one-sweep
trial; do not claim they validate the later eight-sweep runtime. Mesh isolation
also passes in the latest trial. Convergence, current-runtime visual checks,
performance, affected suites and the quick gate remain outstanding. Future
optimization should consider reusing the prior fitted state and accelerating
exact support queries, while testing relaxation when contact is removed so a
previous obstruction cannot leave a permanent hair bulge.

### Control chains for sliding hair contact

The maintainer's direction is to deflect whole hair locks around torso, armor,
posed arms and other visible equipment, with sliding contact and secondary
motion analogous to soft-body bones. Individual mesh-vertex projection is not
the intended final representation. Major authored locks already have centerline
paths, so the implementation now starts fitting those short chains instead.
Each chain retains scalp membership, authored segment lengths and thickness;
contact is sampled at controls and quarter/mid/three-quarter segment positions.
The mesh follows the translation and rotation of its bound source segment.
The old face-contact/stretch pass remains as a temporary corrective pass and
its regression witnesses remain enabled. The new chain solver owns no materials,
render assets or species-specific anatomy; callers supply a distance field.

This is the initial posed-chain fitting stage, not yet a physical bone simulation.
It rebuilds from posed targets, while the existing eased drape drives secondary
motion. Damped persistent control state, restoration when obstacles move away,
posed arm/forearm collision shapes, and smooth blending across adjacent lock
bindings still need implementation/verification. Actual GPU skinning is an
optional representation choice; CPU control points already provide the same
small deformation control structure. A plane-contact unit test covers fixed
socket placement, thickness, segment length, tangential sliding and removal of
contact. Whole-mesh tests and native captures must still establish whether the
chain representation resolves the existing clipping/strain witnesses and improves
runtime. The initial compile exposed a Rust indexing-borrow error, corrected
before the current test build; no successful chain-runtime claim is made yet.

The first chain-based fitting checkpoint passes the plane sliding/release test,
instance isolation, the complete animated clipping-and-strain regression, static
emitted-hair clearance, the retained walking/turning and cloth-motion regression,
and the bounded focused/seeded composition batch. A rebuilt native client also
passes scripted pose replay with an equipment change during animation. Direct
rest/jump captures show front locks outside the breastplate without the former
broad shoulder wings. Some shoulder bends remain abrupt, consistent with the
current single-segment binding; this is not final hair quality. Release build,
all-target release Clippy and the repository quick gate pass. Full platform CI,
desktop deployment and native mouse checks were not run for this checkpoint
(mouse checks remain waived by the maintainer).

The successful mesh regressions include the temporary face-contact/stretch
correction. They do not prove that chains alone can replace it, or that all hair
surfaces clear every collider. The moving-arm collider, smooth segment/lock
weights, damped persistent state and runtime optimization remain open. Source
placement and fixed scalp assertions remain separate from fitted-fall clearance;
no former clipping or strain tolerance was relaxed to obtain this checkpoint.

### Continuous skinning through control joints

The first chain binding used a single rigid rotation for each source segment.
A retained synthetic surface regression demonstrates the defect independently
of collision fitting: two points 0.00002 apart on opposite sides of a bending
control joint separated by 0.02528 after deformation. The regression also checks
that a uniform rigid bend preserves the surface, guarding against a fix that
simply shrinks the radial offsets.

The revised deformer precomputes one orientation at every control, averaging the
rotations of its neighboring segments, then interpolates those unit orientations
along each segment. Both sides of a shared control therefore use the same frame,
while a point keeps its radial distance from the interpolated centerline. The
contact solver and its clearance/strain tolerances are unchanged. This addresses
within-lock segment boundaries; bindings between distinct sculpted locks still
need smooth weights, and chain centerline bends themselves remain piecewise
linear. The failing witness is retained; whole-mesh and native checks are required
before treating the revision as verified.

The continuous-joint revision passes its first-red regression, the animated
clipping/strain and isolation checks, static emitted-hair clearance, retained
walking/turning/cloth motion, and the bounded focused/seeded composition batch.
The rebuilt real client passes scripted replay with a mid-motion equipment
change. Native rest/jump captures preserve clearance; the visual difference in
this fixture is subtle, and larger shoulder angles remain. Release build,
all-target release Clippy and the quick checkpoint gate pass. Quick initially
deferred while Clippy was active and was rerun only after that process finished;
its concurrency guard was not bypassed. Full platform CI remains outstanding.

For the future moving-arm contact layer, animation already has current arm and
forearm transforms before hair fitting executes. The composed body and clothing
should contribute visual contact shapes attached to those joints, with sleeve
and armor thickness accounted for by their generators. Hair should consume the
posed shapes through the shared contact interface, not guess radii from species
names. These are visual fitting proxies, not new authoritative gameplay hitboxes.
This is a design note, not an implemented arm-collision feature.

### Exact regional contact-field programs

Hair repeatedly evaluates the same composed torso field during chain and mesh
constraints. The new ContactField reuses Sculpt's conservative operation bounds
to retain only operations that can affect a query cell. It preserves operation
order and recursively localizes union children. It caches field programs, not
sampled distances: there is no interpolation grid or relaxed collision margin.
Cells are 0.1 body-space units, bounded to 256 entries per immutable composed
surface, with rounding allowance around their bounding spheres. Queries beyond
the cache limit or coordinate domain evaluate the original program. Equipment
recomposition constructs a fresh source/cache. A lock protects the cache because
rig components must remain thread-safe; query order does not change field values.

Tests compare exact float bits against the original field for nested blends,
carvings, inflation, cell boundaries, large coordinates and capacity fallback.
Fresh caches are used for each boundary neighborhood so early cache saturation
cannot hide the culled path. A separate witness requires fewer evaluated source
operations near a body surrounded by distant layers. The animated humanoid
regression also compares cached and original distances on its emitted hair
vertices. These are correctness/work-count checks, not a benchmark result.
Native capture equivalence and the existing fitting regressions remain necessary.

Future moving arms should be separate posed contact contributions alongside the
static cached torso field. Do not insert moving geometry into an immutable cache
without an explicit revision/invalidation contract. A reusable contact scene may
combine static body programs with local-space attachment fields or analytic
capsules transformed by current joint poses. No moving-arm implementation or
quantified runtime improvement is claimed by this initial cache change.

The regional-field checkpoint passes three field-equivalence/budget regressions,
all four hair-contact tests (including the new full-field oracle), static emitted
hair clearance, walking/turning/cloth motion, the bounded composition sample and
real-client scripted replay. Both retained native PNGs—rest and jump—are byte
identical to the preceding continuous-joint build for the same input scene.
Release build, all-target release Clippy and the quick gate pass. This establishes
exactness for the exercised fields and captures, not universal proof or a measured
speedup. The current perf_compare workloads do not include the independent 3D
contact solver; a suitable harness case is needed for a formal timing comparison.
Full platform CI and the previously listed animation/contact features remain open.

Another optimization candidate is convergence: the chain currently performs a
fixed 48 sweeps, while the mesh pass can keep correcting tiny residuals near its
target clearance until its iteration cap. Any early-out criterion should check
both contact and length residuals, retain the existing clipping/strain witnesses,
and report work counts separately from formal performance measurements. This is
an investigation note; this checkpoint changes field evaluation only.

### Deferred posed arm contact experiment

An experimental implementation derived six conservative capsule envelopes per upper-arm
and forearm mesh from the composed geometry, including clothing in those meshes.
Faces are clipped to each axial slab before fitting its radial envelope; using
whole crossing triangles overestimates shoulder size. Profiles follow the current
local joint transforms and join the static torso field through a contact scene.
Its invalidation contract required refitting when only an arm moved, even when
the head stayed unchanged.
Cached source meshes retain CPU positions so profiles can be rebuilt on redressing.
This costs memory; reusable profile metadata beside the mesh cache is a future
alternative. These visual envelopes are not gameplay hitboxes.

The first animated regression exposed an arm intersection before integration.
The first integrated version then failed the existing hair edge-strain bound at
the shoulder. Tightening the envelope, doubling mesh relaxation sweeps, and replacing rounded
ends with capped cylinders each still failed the existing edge-strain bound.
The experiment was therefore removed from the active source; it is not a
validated moving-arm solution. The existing strain and scalp-attachment bounds
remain in force. Conservative capsules do not describe concave garments exactly,
and separately attached sleeve drapes are not yet contributors. Current posing
assumes rigid joint transforms; arbitrary joint scaling needs explicit handling.

The hair controls currently solve a posed geometric fit, not persistent physical
motion. Head/drape motion supplies the animation target. Future inertia, damping,
and tangential friction belong in a reusable chain solver, with collision shapes
provided by body/equipment composition. Keep fixed sockets, thickness, length,
and contact constraints separate; a single hair bone cannot wrap a lock around
several independently moving surfaces. Continuous collision detection and contact
history are also future work, not guarantees of the current sampled constraints.

The active Rust and Cargo inputs were restored byte-for-byte to the preceding
regional-field checkpoint, verified against its recorded SHA-256 inventory.
The experiment and first-red/iteration logs are retained locally for diagnosis.
The earlier passing build, fitting, native replay/capture, Clippy and quick
results therefore apply to the restored source. No new arm-contact acceptance
claim is made. A subsequent implementation needs to resolve competing shoulder
contact, fixed-socket and mesh-strain constraints together; increasing iteration
counts alone did not resolve the observed failure. Preserve the pending arm
clearance regression when reintroducing the feature, alongside all existing
scalp, strain, body-contact and per-wearer isolation checks.

A diagnostic rerun of the capped-cylinder experiment narrowed the first failure:
both edge endpoints were movable and bound to the same curve/segment/t value.
Their fitted contact gaps were positive (approximately 0.0077 and 0.0087), while
the local edge grew from 0.00567 to 0.01632. This particular failure is therefore
not explained by a fixed scalp socket or switching between hair controls. The
solver currently relaxes edges before face contact and exits after contact;
its final face corrections can leave unrelaxed edge strain. A balanced ending
length pass is being investigated, with contact assertions retained to detect
whether length restoration pushes the surface back into the obstacle. This is
a diagnosis and hypothesis, not yet an accepted fix.

Follow-up experiments confirmed that the constraints compete. Eight final edge
sweeps moved the first strain failure from jump tick 0 to tick 3. Up to 64 sweeps
with early exit removed that strain failure but produced an arm intersection at
tick 3. Projecting near-contact edge corrections into the surface tangent plane
then progressed beyond those failures, but the test failed its independent
requirement for more than 20 front-torso hair witnesses. The failure did not
report its tick, so no all-pose clearance claim is justified. This indicates a
coverage/shape-retention problem requiring rendered diagnosis, not permission to
remove the witness. The tangent version also introduced many normal queries into
edge relaxation; no performance improvement is claimed.

The next solver design should explicitly balance authored-shape retention with
contact and length constraints, rather than allowing mesh corrections to choose
an unconstrained route around the shoulder. Consider control-chain targets and
bounded displacement/curvature together before relying on expensive mesh-level
corrections. The experiments and diagnostic logs are preserved locally. Active
Rust/Cargo inputs were again restored exactly to the validated regional-field
checkpoint; neither the arm experiment nor its diagnostics are enabled in the
normal client. Existing regression witnesses remain unchanged.

Rendered follow-up established that the coverage loss is a real route change,
not merely a ray-sampling artifact. The diagnostic 16 ms jump test had 208
front-torso witnesses at tick 20 and zero at tick 21. Native 50 Hz captures around
the corresponding time show front locks still present at ticks 16 and 17, then
moved behind the shoulders at tick 18 as the arms swing forward. The time steps
are different, so these are neighboring-pose evidence rather than exact frame
identity. Both capture runs completed; front and side views are retained locally.

The stateless fitter restarts each control chain from its authored target every
pose. This permits a different collision-avoidance route to be chosen abruptly.
A per-wearer history experiment now seeds the next fit from the previous fitted
chain transformed into the current head/drape frame, with a small authored-target
pull during relaxation. Recomposition starts fresh history, and arm-only changes
must invalidate placement without discarding the prior fit frame. This is under
validation; it is not a claim of inertia, continuous collision detection, or a
frame-rate-independent physical model. Acceptance still requires shape retention,
obstacle release, isolation, contact and strain checks, plus rendered motion.

The history experiment did not pass acceptance: it retained 374 front witnesses
at jump tick 21 but fell to zero by tick 24. A second experiment used the initial
fitted chain, transformed into hair-local space, as the subsequent resting target
instead of the raw authored curves inside the body. It retained 245 witnesses at
tick 21 but only 18 at tick 22, below the unchanged required count. Neither
variant is accepted. Their source, failed tests and completed native captures
are retained locally; active Rust/Cargo inputs are restored to the prior verified
checkpoint. Shape retention and history alone did not establish a stable route.

Future investigation should inspect the continuous path between fitted poses,
not only endpoint signed distances. Discrete node and segment samples can select
a new side of an obstacle without maintaining the prior drape. Per-frame vertex
and control displacement diagnostics in scripted review would expose such jumps
directly, alongside existing joint-pose evidence. A swept-contact or constrained
path method needs its own continuity, contact-release and frame-step tests before
integration. The conservative arm profiles also still need a geometric accuracy
review; the present evidence does not prove that collision-envelope error is
irrelevant. No production moving-arm fix or performance result is claimed.

### Current work in progress: posed contact coordinates

The posed-arm prototype is now retained in the isolated working tree for further
iteration; it is not an accepted runtime checkpoint. The normal client executable
remains the last validated baseline. Do not infer that source and executable now
contain the same experimental feature, and rebuild explicitly before new native
captures. Nothing has been published or deployed.

The contact-profile regression failed on an end-boundary face point even before
animation. Its correction pads both axial ends by the same 0.003 allowance used
radially. Posed cylinders now retain their joint-local geometry and evaluate
queries through the inverse joint transform and support-coordinate scaling.
Transforming endpoints and reconstructing a round cylinder after nonuniform
support scaling did not preserve that geometry. Distance values use the minimum
support-space scale for the current rigid joint-transform contract; arbitrary
joint scaling still needs a general distance-bound treatment. The sparse-face
coverage and composed-joint pose regression passes with this correction.

The corrected prototype also passes the chain and per-wearer isolation checks,
but still fails the jump test's front-hair coverage requirement. The route-change
problem is therefore unresolved; correct contact coordinates alone are not its
solution. The failing acceptance test remains intact. The prototype currently
uses stateless chains with tangent-constrained mesh length corrections, not the
rejected history/rest-target variants. Those variants remain archived locally.

When restoring experimental source files, refresh their modification times to
force Cargo to reconsider them; copying historical timestamps can otherwise
reuse a prior artifact. The current corrected experiment was compiled after
explicitly refreshing all restored Rust inputs. Prior native restoration was
independently checked by byte-identical rendered output, not source hashes alone.

### Work in progress: control correction paths

A focused regression demonstrates that an unconstrained control correction can
jump across a spherical obstacle while both endpoints are clear. The prototype
now advances correction paths by signed-distance bounds, stops near contact and
projects remaining movement into its tangent plane. The loop is bounded (four
slide passes, at most 512 advance queries each); exhausting the budget returns
the last reached point rather than an unverified destination. Initial overlaps
use a separate bounded depenetration step. This is a static-field method; it is
not continuous collision detection for moving arm contributors, and guarantees
also depend on the supplied field's distance-bound accuracy.

The crossing regression failed before this implementation and now passes,
including a wall-sliding case and unchanged unobstructed motion. Existing chain
socket/thickness/length and shared-joint continuity tests pass, as does per-wearer
mesh isolation. Full humanoid acceptance remains failing: with this correction
wired into chain relaxation, the jump fails an existing edge-strain bound at
tick 3 (rest length about 0.00952, fitted length about 0.02628). No strain or
clearance limit was relaxed. This implementation is still part of the isolated
prototype, not a replacement for the validated normal executable.

Initial fitting and motion of an already valid fit need separate contracts.
The authored curves may start inside the composed body and require substantial
fitting; treating those corrections as motion from a valid collision-free state
changes the solution. A follow-up should establish and validate the initial fit,
then apply path constraints with pose history and moving-contact handling.
The primitive success alone does not validate that integration. Runtime cost,
query-budget exhaustion, contact release, motion continuity, and native rendered
acceptance remain required before accepting this work.

### Work in progress: separate initial fit and advancement

The chain module now exposes `fit` for initial overlap resolution and `advance`
for continuation from prior fitted controls. Both use one length/contact solver;
only advancement applies path-checked corrections and a small target-restoring
step. A new regression failed with stateless advancement, which jumped a control
from one side of a sphere to the other. It now passes, including recovery toward
the target after the obstacle is removed. All four focused chain tests and
per-wearer mesh isolation pass.

HairRig retains previous fitted controls and their frame independently of the
unchanged-pose cache. Arm-only pose invalidation clears that cache without
throwing away history. The initial fitted chain is stored in hair-local space as
the resting target; model recomposition creates fresh state. This is kinematic
fitting with solver-iteration target restoration, not an inertial or generally
frame-rate-independent physical simulation.

The full jump test no longer stops at the preceding tick-3 strain failure, but
still fails its unchanged requirement for more than 20 front-hair witnesses.
Acceptance is therefore incomplete. The normal executable is still the validated
baseline; the isolated source is a prototype with a failing integration test.
A diagnostic comparison of control displacement against final mesh displacement
is needed next: the chain retains history, while face/edge correction still
restarts from the skinned source mesh each pose. Either stage could be responsible
for the observed route change. Do not assume which stage is responsible without
measuring both. Moving-contact sweeps, runtime cost and native acceptance remain
outstanding.

### Investigation: jump contact displacement and arm timing

The stateful prototype's 16 ms jump diagnostic localizes the route change to
control fitting, before mesh correction: maximum control displacement rises
from about 0.00735 at tick 20 to 0.08266 at tick 21 and 0.14788 at tick 22.
Maximum final mesh displacement is about 0.08519 and 0.13822 at those last two
ticks. Front-hair witnesses fall from 158 to 4 at tick 22, failing the unchanged
coverage requirement. These are support-space distances for this fixture,
not a general performance or physical-accuracy result.

The shared jump arm pose derives lift from the square root of the parabolic
body height. Although position is continuous at takeoff and landing, angular
velocity is unbounded at those boundaries. A regression is being added for
bounded arm travel at supported review rates and near both phase boundaries.
This is an animation defect candidate independent of hair; fixing it does not
by itself prove that moving-arm contact or hair route retention is correct.

The new actual-client regression reproduces the defect using the previously
validated executable, a male Human in Knight clothing, and 50 Hz captures.
Across ticks 17 to 18, the left hand moves 0.42261 world units in 20 ms; across
ticks 39 to 40 it moves 0.31241. The right hand shows the same spikes. The
regression fails its 0.1-unit per-frame limit. This isolates the visible arm
motion issue from the long-hair solver. The first harness invocation did not
reach the test because an unrelated workspace build failed with invalid core
metadata; the successful reproduction used the explicit existing 3D executable
and renderer-only binary-directory override. The focused Rust arm-speed test
was still compiling when this evidence was recorded; no animation fix or
acceptance pass is claimed yet.

The focused Rust regression also failed before the animation change (20 Hz,
phase 0.26087, arm angular speed 14.30225 radians/second against a 14-radian
bound). Shared biped jump arms now use smooth anticipation, overhead lift and
recovery envelopes instead of square-root body height and direct crouch factors.
The arm swing begins before takeoff and recovers through landing, preserving
the overhead pose while removing singular boundary speeds. Body lift, leg
contacts, torso and head timing are unchanged. Validation of this change is
pending; the native executable still predates it. The new process regression
checks both hands around takeoff and landing, with actual rendered captures.

After the curve change, the focused arm regression passes at 20, 50, 60, 120
and 240 Hz, including fine samples across both flight boundaries and the
retained overhead pose. Python process-test syntax, documentation checks and
diff whitespace checks pass. The full armored-hair jump still fails: front
witnesses fall to 7 at tick 19, when anticipation has brought the arms forward
earlier. Maximum control displacement on that frame is about 0.07969 and mesh
displacement about 0.09005 support-space units. Thus smoother arm animation
addresses an independently reproduced defect but does not establish hair
contact-side retention. Remaining jump regressions and the client rebuild were
still active at this checkpoint; native validation of the new executable,
Clippy and quick verification have not yet been completed for this change.

### Next contact boundary: moving contributors and attachment transport

The current `Scene::set_pose` replaces all arm contributors with their final
pose, retaining no trajectory. `chain::advance` checks control corrections
against that final static field. It therefore cannot distinguish an arm that
crossed a lock between frames from an arm already on the other side. In
addition, HairRig transports all prior controls through the head-frame change
before asking the solver to check correction paths. Those two unchecked motions
remain separate from the now-bounded jump arm curve.

The next focused regression should move an obstacle through an initially clear
control while leaving both endpoint configurations clear; stationary-obstacle
correction tests cannot cover that case. A shared solution needs previous and
current contributor transforms with stable part identity, and should advance
socket motion and obstacle motion together. Prefer rigid translation/rotation
interpolation and a movement bound based on contact thickness over blindly
interpolating affine matrices or adding an outfit-specific offset. Shape changes
from recomposition need an explicit fresh-fit/reset contract. Frame-rate
comparison and query cost must accompany integration; existing full-mesh strain,
clearance and front-coverage assertions remain required. This is a proposed
contract, not an implemented moving-contact guarantee.

Native validation rejected the first smooth curve despite its passing angular
unit test: the left hand still moved 0.13408 world units at tick 17, above the
unchanged 0.1 bound. Lift is now spread over more of the jump, with a shorter,
smaller backswing; both regressions are being rerun without changing their
limits. The broad six-test jump run was deliberately stopped after reproducing
the hair failure and the native motion failure, before editing its Rust inputs.
Its unfinished belt/other checks are incomplete, not passes. The queued build
then completed, and no Rust inputs were edited until all prior build/test
processes ended. The normal executable has now been rebuilt from the prototype;
it is no longer the older validated baseline and must not be presented as an
accepted full hair/contact result.

The wider arm curve passed the angular test but native hand displacement still
failed at 0.10379 units on tick 18. Inspecting all captured transforms rather
than just the first assertion exposed a second singularity: at landing tick 41,
the head moved roughly (0, -0.08510, 0.07510) and the left hand roughly
(0.00174, -0.18097, 0.07371). `jump_profile` used fractional powers 0.7 and 0.45
for crouch/landing phase, with unbounded phase derivatives at their starts.
These now use the shared smooth easing function before the sine, preserving
peak crouch depths and flight timing. A focused boundary-speed regression also
checks retained crouch and landing depth. Native hand-displacement limits remain
unchanged and supply the pre-fix actual-process failure for this correction.
The revised shared body curve is awaiting unit/native validation; earlier hair
failure results remain evidence for earlier inputs, not a final result for this
new curve. Broader belt/contact checks still need a complete rerun.

With both arm and crouch curves corrected, both focused Rust regressions pass
and the unchanged real-client hand-displacement test passes across the captured
takeoff and landing boundaries. The release executable builds, and documentation
and whitespace checks pass. This confirms the shared jump-motion correction for
those witnesses; it does not validate the experimental hair solver. A fresh
armored long-hair regression and nearby native front/side captures are running
against this new body curve. Broader interrupted jump tests, Clippy and the quick
checkpoint remain outstanding; no push, publication or acceptance is claimed.

Final evidence for this jump-curve iteration: the fresh armored-hair test fails
its unchanged front-coverage condition at tick 15 (9 witnesses; more than 20
required). Native capture completes at 50 Hz for ticks 16/17/18, front and side.
Visual inspection shows shortened/retracted front locks and an abrupt hair-shape
break around the neck/shoulder; the hip-layer edges also remain conspicuous.
The native capture rate differs from the regression's 16 ms step, so equal tick
numbers do not identify equal poses. These images support keeping hair fitting
experimental; they do not negate the independently passing jump-motion tests.
Do not accept this as a full model-quality checkpoint. Next work remains moving
contact history/transport, then full mesh and visual acceptance, runtime cost,
and completion of the interrupted broader verification.

The quick checkpoint completed successfully (format, architecture, build tools,
Python tools); its selection included no core Rust packages or core process
tests. Release all-target Clippy also passes. These checks do not cover or
supersede the recorded failing 3D hair acceptance test.

A separate attachment-boundary concern emerged from the side capture:
`hair_contact::spawn` assigns a Boolean movable mask using posed vertex height
0.58, and `place` either applies the chain deformation or leaves the vertex
fixed. A chain segment can interpolate a nonzero displacement at that height,
so the switch has no continuity guarantee. The current shared-control skin test
covers continuity between two segments of one chain, not this fixed/deformed
boundary or bindings between different locks. A dedicated regression should
measure neighboring surface points across the attachment boundary under a bend.
A continuous attachment influence would need consistent weighting in skinning
and contact/strain corrections while retaining exact scalp attachment. This is
a separate hypothesis for the visible neck break, not proof of the front-lock
route failure or authorization to relax the existing tests.

## Live renderer playback and deterministic capture

The live playlist runner uses the validated composition/event schema with separate
camera holds and eased transitions. One window persists. Composition variants
preload once and reset on scene entry; hair, robe and cloak caches are invalidated.
This bounded review cache is not yet a gameplay inventory streaming system.
Wall-clock animation uses bounded substeps and reports discarded long-stall time.
CPU update/render intervals measure pacing; actual GPU/display timing remains
future work. Fixed-tick capture retains screenshot barriers. Future refactoring
should share stage/lifecycle code while preserving both clock contracts and their
actual-process tests.

### Attachment transport contact boundary

The head-frame advection path now sends free controls through the shared static
contact sweep before relaxation. Pinned socket controls still follow their
exact authored destination. A clear-endpoint sphere regression first reproduced
tunneling under the previous direct transport, then passed with the sweep; an
unobstructed-motion witness verifies exact destination recovery. All six chain
checks and release all-target Clippy pass for this change.

The full armored Sylvan Duelist jump witness remains failing at tick 15 with
9 front witnesses. The similar failing pose after constraining transport means
this correction is not a solution for the observed front-lock retraction.
Moving-arm history, attachment-boundary continuity and deformation/length behavior
remain separate unresolved contracts. In particular, current contact queries
still use the final posed arm field, not a temporally swept contributor field.
Native visual acceptance and full model-quality acceptance are not claimed.

Per-wearer mesh/source isolation passes after the transport correction. A rebuilt
native client completed eight front/side captures at 50 Hz (ticks 0, 12, 13 and
18). The side views show a conspicuous neck/shoulder discontinuity already at
rest; this is not solely a jump-pose defect. The existing Boolean fixed/deformed
vertex boundary therefore remains a concrete independent regression target.
These images do not establish that boundary as the only cause, and the 50 Hz
captures are not tick-equivalent to the 16 ms integration witness.

### Continuous scalp attachment influence

Hair mesh attachment now stores a neutral-frame scalar influence instead of a
Boolean movable mask. The existing fixed scalp boundary at height 0.58 is
preserved, with smooth influence increasing to one at 0.50. Skinning, face-contact
mobility and edge-strain mobility consume the same value. Animated height does
not reclassify attachment, and zero-influence vertices retain their exact source
position. These measurements remain prototype authoring data; a shared body
fitting description should eventually provide the attachment region.

The focused continuity regression first reproduced a 0.14730959-unit split
between neighboring samples across the former cutoff. It and the exact-scalp/
free-region witness pass with continuous influence. Chain and per-wearer source
ownership witnesses also pass. This proves the tested skinning boundary, not
all final-mesh continuity or visual quality. Cross-lock binding transitions and
contact corrections still require emitted-mesh and visual review.

The completed jump rerun still fails at tick 15 with 9 front witnesses. No
coverage, clearance or strain limits were relaxed. Both the transport and
attachment corrections leave that full-motion acceptance issue unresolved;
rest-target orientation and moving contact history remain separate candidates.

The rebuilt native capture completed all eight requested views with this scalar
influence. Resting and posed side views still show the collar-like neck bend, so
this change is not accepted as a visual solution for that defect. Source review
confirms that authored strands are smoothly unioned into the hair surface, then
vertices are bound by nearest segment after meshing. Preserving/blending those
source influences is the next structural boundary to test; a single nearest
segment can switch between differently deformed locks. Mesh normals are already
recomputed after deformation. This finding does not establish cross-lock binding
as the sole cause of the remaining visual defect.
