# 3D art direction and animation philosophy

This is the working direction for improving the existing procedural 3D client.
Targets are aspirational renders, not screenshots of implemented improvements.
The [continuation plan](../plans/3d-art-continuation.md) records implementation,
review status, current challenges and the next shared modeling work.
The visual goal is a living, hand-carved dark-fairytale diorama: deliberate
sculptural forms, convincing weight, expressive color, and readable action.
Characters and usable objects should be brightly readable and have visual pop;
the setting and background carry much of the atmosphere.

## Philosophy

Every shape must communicate character, material, or motion. Broad forms carry
identity at gameplay distance; secondary shapes reward a closer view. Fine detail
comes last. Preserve the handcrafted charm while replacing accidental primitive
shapes with authored anatomy. Quality means consistent decisions across the cast,
not a more detailed hero surrounded by unfinished creatures.

The player must understand a creature before reading its name. Body mass,
stance, silhouette and movement distinguish families. Cosmetic variations remain
stable for each individual and use only disclosed appearance data. Presentation
must never imply an undisclosed weapon, condition, creature identity or result.

## Proportions: Bold Storybook

Bold Storybook is the selected working direction from the
[proportion comparison](proportion-study-v1.png). The former five-foot-cube
reference does not constrain the art. Favor a stronger character presence
relative to the floor, a more compact body impression, larger expressive heads,
broader hands and feet, and bolder garment and weapon masses. These proportions
should carry personality and action at the actual gameplay camera.

Apply the direction across the cast. People and undead use distinct head, hand
and body shapes; armored figures have stronger mass and readable protective
layers; birds exaggerate the beak, head and feather silhouette; equines emphasize
the skull, chest and hooves; serpents emphasize the head and flowing body rhythm;
floaters rely on strong crystal or flame shapes. Preserve family differences
rather than applying one cartoon head to every creature.

Adult female fantasy silhouettes remain part of this direction: defined waists,
fuller figures, strong legs, fitted clothing and shaped armor. Stylized proportions
and mature facial structure should work together. Keep slender, athletic and
sturdy variants individually recognizable.

World cells still describe movement, support and occupied space. Visual scale and
proportion are artistic choices; review adjacent creatures, narrow passages,
weapon handling, targeting and corpses so the presentation remains understandable.
The generated comparison illustrates the direction; its suggested scale changes
and layouts are exploratory references rather than measured runtime specifications.

## Current priority: improve the whole cast

Prioritize the larger procedural generation model over isolated blemishes,
even when those blemishes will eventually need correction. Choose improvements
that make whole families and new body/outfit combinations coherent. A repeated
close-up defect is useful evidence when it reveals a shared construction problem;
it should not displace work on body shape, garment construction or motion rules.

Choose work by its visible benefit across the supported cast. First give models
and objects enough light for their color, shape and materials to read, then improve
shared form construction, material definition, connections between body parts,
and the movement of weight through a grounded rig. Review people, armored
figures, birds, quadrupeds, serpents and floaters together after each broad pass.
A better rendering of weak anatomy still needs stronger forms; a better hero
alone does not establish a high-quality cast.

Apply common craft standards through family-level recipes and reusable helpers.
Keep distinctions in mass, stance, color and movement rather than making every
creature share one body or one gait. Fine seams, buckles, facial ornament and
individual pose polish follow the improvements that change the whole scene.
The female fantasy direction is part of the broad figure and outfit system,
with variations for each role and build.

## Procedural authoring and equipment

This work serves a reusable procedural generation system. A creature recipe
selects a body plan, dimensions and shape parameters, material palette, garments,
attachments and compatible motion. New mobs should compose or extend those
shared mechanisms. Introduce a new rig or geometry generator when a genuinely
new body family needs one; avoid fixing one named mob with offsets that cannot
follow the family's other builds. Deterministic appearance variation remains
stable for each individual.

Fitted ribbons, garment surfaces, cloth clearance, limb proportions and joint
attachments should take their shape from the wearer. These are foundations for
interchangeable equipped armor: equipment needs a visual slot/attachment mapping,
layer order, body coverage and clearance rules, compatible deformation, and cache
identity that includes the equipped appearance. Show only equipment information
disclosed to the client.

Biped recipes now resolve one body description shared by mesh construction,
joint placement, garment fitting and animation support. The same limb and foot
shapes feed geometry and fitting, and animation reads the resolved build instead
of inferring it from joint spacing. Guardian and Yeti exceptions remain explicit;
equine and centaur dimensions remain separate. This preserves the accepted
average family shapes while providing correlated body variation. Organic biped
builds link shoulder, waist and hip ratios with upper-arm, forearm and thigh
fullness. Lateral shoulder and hip roots follow those widths; vertical bone
lengths, hands, feet and distal taper retain their established values. Garment
widths and cuffs follow the same profile. This gives slender and sturdy bodies
different mass distributions without adding independent random shape controls.

Existing outer-torso recipes now distinguish cloth, leather and plate construction:
rib and back ease varies by material, the waist stays fitted, and structured
fronts bridge the chest differently. Family necklines, armholes and hems retain
their cuts, with decoration fitted to the resulting surface. This is an initial
shared construction rule; sleeves, full garment patterns and interchangeable
layers still need further work.

Garments also need their own construction rules: cloth has supported regions,
ease and free edges; leather uses structured panels; armor uses continuous
protective shells and deliberate overlap. These layers should follow the resolved
wearer without copying every anatomical bulge. Body morphology, outfit selection
and motion constraints should remain distinct parts of the recipe. Keep adult
female fantasy silhouettes and broad, shallow male chest planes throughout.

The present implementation is mostly Rust body-family and outfit recipes.
Gameplay already supplies equipment-slot data, and the client displays an
equipped weapon. A general data-driven mob/garment authoring interface and
interchangeable fitted armor layers are still future integration work. The
current art pass develops and reviews the shared geometry and motion mechanisms;
it does not establish arbitrary equipment compatibility or choose a scripting
language.

## Baseline diagnosis

The existing Warrior photo shows disconnected spherical shoulder masses,
long nearly uniform arms, weak elbow landmarks, thin wrists, and cloth that
reads as soft molded material. The face has useful personality but needs clearer
brow, socket, cheek and jaw planes. The gameplay capture shows that small limb
and facial details disappear at the normal camera distance. Large silhouette and
value changes therefore take priority over surface decoration.

These observations come from actual captures of the integration baseline and
the isolated art preview. Still images cannot establish animation quality.
The current implementation uses cached
procedural parts, hierarchical joints and code-driven poses; retain that approach
for the first iteration rather than assume an external asset pipeline is needed.

## Shape and material language

People: coherent ribcage and pelvis, readable neck insertion, shoulders integrated
under sleeves or armor, tapered upper arms and forearms, elbows placed at a
believable waist level. Use slight knee flexion and a clear supporting leg.
Keep hands large enough to read and make grips close around the weapon handle.

Male chests use a broad, shallow pectoral plane continuous with the ribcage.
Male cuirasses, jerkins and coats follow that plane; chest strength comes from
width, shoulder placement and controlled planes. Preserve this distinction
across slender, average and sturdy builds and the centaur's human torso.

Cloth: thick hems and a few directional folds anchored at shoulders, belt and
knees. Leather: structured panels, edge thickness and restrained seams. Metal:
controlled planar bevels, sharper highlights and darker broad faces. Hair:
interlocking sculpted clumps with a continuous hairline. Skin: warm broad planes
with restrained highlights. Avoid equal gloss across all materials.

Beasts: establish skull, ribcage, pelvis and species-specific limb placement
before fur, feathers or scales. Birds need a convincing hip-to-hock chain;
quadrupeds need readable shoulder and hindquarter masses. Serpents need a
continuous body rhythm and grounded contact. Floaters need intentional suspension
and restrained acceleration rather than arbitrary vertical bobbing.

## Female figures, clothing and armor

Lean a little into classic high-fantasy tropes for adult female PCs and mobs.
Their figures and outfits should have a confident, appealing, intentionally
feminine silhouette: a clearly shaped waist, fuller hips and bust, long strong
legs, and a deliberate contrast between shoulder and hip lines. Keep anatomy
coherent in motion. Allow slender, athletic and sturdy builds to express that
direction differently, preserving stable individual variation.

Clothing and armor should follow the figure. Use fitted bodices and cuirasses,
shaped seams and lacing, defined belts, short split tunics or layered hip panels,
and fitted boots where they suit the role. Armor can have elegant fantasy curves,
sculpted surfaces and ornamental edges, with convincing thickness and overlapping
pieces. The underlying body should remain legible through the outfit's larger
forms. Avoid a uniform padded tube or a scaled copy of the male torso.

Give each role its own interpretation. Warriors and sentinels have a strong,
athletic stance, a defined armored waist and layered protection around the hips.
Scouts have a nimble silhouette with fitted leather and split cloth that allows
movement. Mages can use a fitted upper robe, a cinched waist, a long flowing lower
silhouette and expressive sleeves. Vampires can lean further into an elegant,
dramatic figure, shaped bodice, high boots and sweeping cloak. Creature families
retain their own anatomy and character alongside these styling choices.

The fantasy emphasis should read at the normal gameplay camera as well as in a
portrait. Review front, side and back silhouettes, bust/waist/hip transitions,
cloth and armor clearance during actions, and the supporting stance. Maintain
consistent craftsmanship across sexes and builds. Apply the same care to mobs
and PCs; cosmetic styling does not change their mechanical capabilities or
invent undisclosed equipment.

## Color philosophy: expressive color with a clear hierarchy

Color should make the world inviting, strange and memorable. Dark fairytale
means mystery and consequence; it does not require a uniformly brown or muted
image. Rich dyes, warm skin, colored minerals, unusual creatures and magical
light have a real place here. Saturation is a compositional tool, not something
to minimize everywhere. A scene may be colorful and still feel ancient or dangerous.

Let characters carry substantial blocks of color: a jewel-teal cloak, a strong
blue or red sentinel, an ochre feather mass. Give the eye an identifiable color
shape at gameplay distance rather than reserve all vivid color for tiny trim.
Use secondary warm/cool relationships within those masses: teal cloth against
cognac leather, cool steel against warm brass, sienna feathers against blue-gray
neck skin. Preserve individual skin, hair and coat variation.

Keep the broad floor and most stone less saturated than the characters so they
support navigation and silhouettes. Quiet areas still have hue: blue-violet
shadow planes, warm mineral light, moss and weathering can enrich the cave.
Local pockets of intense crystal color and magical light provide stronger peaks.
More colorful regions can have their own dominant hue and contrast structure;
do not flatten every dungeon into the same neutral cave palette.

Distribute visual emphasis deliberately. Large character color blocks establish
identity, smaller complementary accents provide sparkle, and rare emissive
areas draw focus. A whole scene need not compete at maximum saturation. Material
contrast also supplies pop: a clean sword glint or a warm leather edge against
matte cloth can be effective without adding glow. Contact shadows ground those
colors and keep neighboring forms distinct.

Color must work alongside value, shape and motion. Red clothing remains ordinary
clothing; an attack indication uses the existing interaction vocabulary and
additional shape cues. Preserve UI semantics and reserve emphasis around the
current action. Do not imply hidden identity, hostility, loot value or conditions
through new cosmetic colors, eyes or auras. All variations remain within the
server-disclosed appearance and the approved asset/palette vocabulary.

Judge a richer palette in a crowded gameplay view as well as a portrait. Review
warm, cool and dim disclosed lighting, visibility of nearby creatures, material
separation, UI competition and whether accents still feel special. Compare its
light/dark structure visually as well as its color. Increase selective chroma
and warm/cool contrast before simply raising exposure or global saturation.
Concept lighting is not proof of gameplay visibility.

The early targets establish anatomy and sculptural form; their relatively muted
color is not a ceiling on the finished game. The richer color study explores
this broader direction. Keep the parts that add life and readability, and
revise any hues or contrast that obscure the actual play. No numerical saturation
quota or universal palette lock is intended.

## Lighting and atmosphere

The playable cast should feel inviting to look at. Illuminate faces, body planes,
clothing, equipment and usable props strongly enough for rich colors and material
differences to read at the normal camera. Brighter light is part of the selected
direction. Do not preserve a dim foreground merely to signal a mysterious cave.

Build atmosphere through the setting: enclosing silhouettes, layered background
shapes, weathered stone, unusual vegetation and minerals, distant darkness,
localized mist and a deliberate warm/cool palette. Broad background masses may
be quieter and darker while the cast remains clearly lit. Avoid a uniform brown
wash, crushed dark costumes, muddy shadow faces and a global gray veil.

Use a clear key to describe form, generous softer fill to reveal shadow-side
color, and selective rim or reflected color to separate silhouettes. Keep
contact shadows and local occlusion so brighter figures still belong on the
floor. Selective metal glints and crystal accents add sparkle; stronger glow
is not a substitute for light on ordinary surfaces. Check the result against
the richest color target, not only against a dimmer previous screenshot.

Artistic presentation light may favor characters and props over the background.
It must respect disclosed illumination and memory: unknown or remembered actors
and undisclosed equipment cannot become visible through a lighting treatment.
Review lit, dim, crowded and back-facing spaces together, judging the cast's hue and
readability alongside the intended mood of the set.

### Ceilings and overhead geometry

Do not render ceilings in the current client. The ceiling toggle and ceiling
preview option are removed. Keep the existing overhead terrain omission and
camera-facing wall cutaway while the presentation is developed. Covered-roof
captures are rejected experiments, not an accepted lighting or enclosure target.

Reintroducing overhead geometry needs a deliberate design. Work through stacked
floors, bridges and overhangs, shafts and upward views, meaningful overhead
objects or interactions, and camera or focus transitions. Decide what each view
must communicate before choosing hiding, fading or cutaway rules. Preserve clear
actors and usable objects, spatial understanding, and the boundaries of disclosed
terrain and memory. These cases remain unresolved; the current omission does not
establish a future classification policy.

## Motion philosophy

Motion communicates intention, effort and consequence. Start with the supporting
foot and center of mass, then pelvis, chest, head and hands. Secondary motion
follows that chain with smaller amplitude and a short delay. Let stillness exist.
Breathing should not make every body part oscillate together.

Walking: measure displacement-driven phase against stride length. Separate
stance and swing, keep the support foot stable in world space, lift the swing
toe, and roll heel to toe on landing. Knees bend forward coherently. Pelvis
height varies modestly; chest counter-rotation supports balance. Blend smoothly
into rest and preserve natural joint bends. Teleports and observation resets
must clear obsolete movement without manufacturing intermediate travel.

Actions: make anticipation, exertion/contact and recovery distinct. The body
supplies force before the weapon finishes its arc. Guard hands and shields have
purpose. Ground feet during planted attacks; only step when placement supports
it. Damage and death cues follow disclosed events. Local animation cannot delay
input readiness, invent hits or continue an obsolete action after a reset.

Family differences: heavy creatures accelerate deliberately and settle with
weight; light creatures take quicker, smaller steps; undead move with constrained
coordination; birds stabilize the head against body motion. Personality comes
from timing and posture before exaggerated amplitude. Reserve playful antics for
appropriate idle contexts and blend back without a visible pose snap.

## Quality bar and review

Review each representative creature at the normal gameplay camera and in a
three-quarter portrait, front, side and back. A good portrait must also read in
the actual game. Check silhouette in flat value, anatomy at joint extremes,
weapon grip, cape/body intersections, foot contact and material separation.

Inspect motion at real speed and slow playback: idle, start, sustained walk,
stop, turn, signature action, interrupted action, hurt and death. Compare at
multiple frame rates once verification is authorized. A still target render
establishes shape and lighting direction only; it cannot validate gait or timing.

Work in broad passes: shared surface/material quality, continuous body forms
and joint connections, then grounded family-specific motion and coherent outfit
silhouettes. Review the full cast, including corpses, variants, equipment and
fallback markers, at each pass. Use representative portraits to diagnose a
shared problem and carry its solution through the relevant recipes before
spending time on individual decoration. Preserve cached mesh reuse and bounded
per-frame work. Measure cost before accepting additional geometry or pose
complexity.

Launch responsiveness is part of presentation quality. Review the time from
starting the client to the first complete, correctly lit scene, including model
construction and renderer pipeline preparation. A long blank or partially lit
opening is a defect even if later captures look good. Remove duplicate work
before trading away shape, color or lighting; capture warmup is not a fix for
the player's launch experience.

## Target renders and implementation status

Generated images establish the visual ambition. Actual captures establish what
has been implemented. Judge the gap between them at gameplay distance before
polishing portrait details. The selected proportions are Bold Storybook; earlier
concepts remain useful for anatomy, materials and the female fantasy direction.

### Current comparison

| View | Actual art preview | Generated direction |
| --- | --- | --- |
| Gameplay forms | [Working capture, ceilings omitted](storybook-gameplay-working-v15.png) | [Bold Storybook target](storybook-gameplay-target-v2.png) |
| Brighter lighting | [Working capture](storybook-gameplay-working-v15.png) | [Bright foreground study](gameplay-bright-lighting-target-v1.png) |
| Whole cast | [Working cast](storybook-cast-working-v11.png) | [Selected proportion comparison](proportion-study-v1.png), right panel |
| Adult female Warrior | [Working capture](female-warrior-working-v11.png) | [Figure and outfit target](female-warrior-target-v1.png) |
| Male and female Warrior builds | [Working lineup](warrior-builds-working-v14.png), [male profile](male-warrior-profile-working-v14.png) | Broad shallow male chest; preserve the female figure direction |

The current preview has stronger character scale, enlarged family-specific heads,
hands and feet, richer teal cloth, more distinct matte/satin/metal materials,
contact shading, a brighter neutral-warm key and generous cool-neutral fill.
Atmosphere is primarily a responsibility of the set and background rather than
dim character lighting. Authored pigment colors
are decoded from sRGB before linear lighting, preserving their intended chroma
and value; dark palettes need individual readability review. Thin garment shells,
fitted waists and hips, moving shoulder/upper-arm forms, wrist-mounted hands
and articulated biped feet apply through shared recipes. Torsos now connect
ribcage, waist and shoulder planes, with fitted cuirasses, split tunics and
garment panels seated on those forms. Equine bodies use connected sculptural masses; birds
have narrower flowing plumage; serpents have a continuous skull and taper;
ice uses shaded facets with selective glowing tips.

Motion uses separate stance and swing paths for all bipeds, birds and equines,
with family-specific support timing and pickup. Biped ankles can keep soles
level, upper-body motion follows contact phase, and walking suppresses idle
fidgets. Birds stabilize their upper body,
equines change gait and coordinate their limbs, and centaurs counterbalance the
human torso. Family attack poses, serpent spacing and settled corpses follow
the same weight and support philosophy. These implementations still need
motion review; a source change or a good still does not establish the quality bar.
Standing action support places a lead step before impact and bounds pelvis
height by leg reach. Heavy/Hook use a staggered base; Step places and recovers
a retreating foot; Mend supports its kneel on the forward sole and rear knee/toe.
Bird toes and equine hooves have terminal joints that preserve level support.
The caster posture pass keeps hems above the floor during an upright healing
gesture and a shallower supported crouch. The earlier 21-frame Heavy, Guard,
Step and Mend review confirmed those clearances and coherent returns to rest.
The subsequent v16 contact pass retains world-space support anchors and settles
swing feet before moving the standing support. Sampled slow curves and stops
look more grounded across people, birds and equines. These samples do not yet
establish fast gait, sharp-turn or interrupted-action quality. They also exposed
a separate defect: legs visibly pass through the caster's rigid lower robe.

The integrated release preview builds. Manual reviews include 420 motion frames,
all 18 portraits, open menagerie sessions, and a subsequent complete catalog of
18 families with five appearance variants each on an RTX 2060 through DX12.
The full catalog produced 54 lineup and close-view captures without a renderer
error. Male chest planes now read as continuous across leather, cloth, plate
and centaur builds; the female figure and outfit direction remains intact.
The current gameplay capture includes the anatomy and posture changes and omits
ceilings. The whole-cast and female portrait comparisons are earlier views.
They demonstrate progress in silhouettes,
material contrast and ground shadows. They do not establish final animation
quality, performance, general gameplay correctness or PR readiness.

Ceiling rendering is removed following review of the covered-roof experiments.
Use open gameplay views for the current lighting direction. The bright foreground
study remains the lighting reference: it illustrates more luminous color and
clearer shadow faces, while remaining a generated paintover.

Actual walk recordings: [bipeds](people-walk-working-v7.webp) and
[beasts](beasts-walk-working-v7.webp). These short loops preserve capture timing
and show the movement before the subsequent supported-action and dark-palette
pass. The loop boundary jumps back to the start; it is not an authored idle or
animation transition. Current movement review found coherent support through
representative walking, retreat, Heavy/Hook and kneeling phases, including level
animal soles. The old loops still show the earlier sliding contacts. The v16
sampled review is the newer slow-travel evidence; faster travel, starts/stops
under interruption, sharp turns and combinations with recoil remain open.

### Launch responsiveness

The launch investigation separated procedural construction from shader startup.
Surface-net vertices now reuse their normals, pigment and occlusion rather than
recomputing them for every triangle corner; fitted garments reuse surface queries.
The Windows preview selects a matching DXC compiler pair beside the executable
using absolute paths. This substantially shortened the blank or partially drawn
shader-startup interval in local manual comparisons without reducing geometry,
contact shading or light quality. A missing pair retains FXC, and explicit
compiler choices remain authoritative. Compiler files are runtime dependencies,
not committed art assets.

Opt-in phase timings retain the remaining construction cost for investigation.
These are exploratory frontend observations, not accepted performance-harness
results or a cold/warm launch guarantee. Repeatable startup measurements and
renderer coverage remain part of PR preparation.

The full appearance catalog initially failed a DX12 mesh-buffer allocation.
Exact final vertex reuse now shares identical position, normal and linear-color
records while retaining triangle order, sharp edges and pigment boundaries.
The successful full catalog retained about 6.36 million vertices from 35.55
million triangle corners, reducing its calculated vertex-plus-index payload
from about 1,492 MiB to 378 MiB without lowering surface resolution. These are
payload counts, not measured total GPU residency. The v21 cache now retains all
live models and bounds idle reuse to 128 MiB of raw mesh payload and 1,024 keys,
with periodic pruning. Four completed sessions cycling the families stayed inside the byte
budget while evicting unused entries, with no missing models in the reviewed
lineups. They did not reach the key-count limit. This bounds idle cache retention,
not total renderer memory or GPU residency; resource measurements remain open.

### Remaining priorities

The v16 preview has now been visually reviewed before further changes. Shared
neck, elbow and cuff transitions are cleaner; the accepted male chest planes,
female figure/outfit direction, bright foreground and omitted ceilings remain.
Its captures exposed robe/leg intersections, chest decoration sinking into
curved clothing, an overly flared emu body and weak vampire/wisp separation.

After rebasing onto PR #161, the v17 preview produced 54 catalog captures,
495 motion captures and three live menagerie captures without a logged renderer
error. Inspection confirms more continuous fitted chest trim, sentinel borders
and heraldry, tapered hair tips, an ovoid emu body and clearer vampire sleeves.
The attempted rigid articulated robe is rejected: it widens the resting outline,
creates hard knee ridges and opens gaps between panels. Successful capture does
not approve that design. The v18 connected cloth replacement removed the rigid gaps and knee seams.
The v19 pass also removed neutral thigh intersections across all ten caster
variants, made curved chest ribbons continuous, and separated shaded orange
wisp folds from the luminous inner flame. Close travel views still expose a
pelvis opening, localized cuff/foot intersections and rigid sash tails crossing
the moving cloth. The centaur death pose also needs floor-support correction.
The v20 preview confirms covered caster waists, continuous moving sash tails,
and centaur upper bodies resting above the floor. Its remaining lower robe is
too tightly fitted around advancing feet and still has a few small penetrations.
Fallen equines expose capped knee/hock joins. The humanoid lower face retains a
projecting chin shelf and rough recessed mouth band. No real-speed motion or
dynamic mesh-upload performance approval is implied by sampled frames.

The v21 shared face attempt was rejected for excessive chin recession and a
faint mouth line. The v22/v23 replacement retains an adult forward chin plane,
blends it continuously into the lower lip and uses narrow contours fitted to the
actual face surface. The scoped face pass is visually accepted across 48 close
views from three angles and 40 catalog variants. Healthy and undead identities
remain distinct; shallow expression creases remain without broken mouth edges.
Separate family checks confirm the guardian's visible smile, the yeti's joined
nose/muzzle and properly placed mouth/tusks, the zombie's fitted open mouth,
hobgoblin nose clearance and vampire fang attachment. These are reusable generated
head components; they do not introduce per-frame facial animation.

The shared equine overlap also closes exposed knee/hock endcaps in the reviewed
fallen quagga, unicorn and centaur variants without visibly enlarged joints.
Static visual acceptance does not approve real-speed gait, resolve remaining
cloth penetrations, or replace the code tests deferred until PR preparation.

1. Establish a shared body description for geometry, joint landmarks, garment
   fit and motion support. Preserve accepted defaults, then introduce correlated
   shape variation that produces distinct silhouettes across whole families.
2. Improve garment construction and material layers across Hero, Scout, Caster
   and Vampire recipes. Separate wearer shape from clothing ease, protective
   shells, attachment surfaces and motion constraints. General equipped-armor
   integration remains a separate scope; do not infer equipment from prose.
3. Review faster movement, sharp turns, abrupt stops and interrupted actions
   across grounded families. Use the existing travel study to find failures in
   shared support and motion rules. Defer isolated frame blemishes unless they
   reveal a broader problem or obstruct a family-level assessment.
4. Preserve the accepted face/mouth improvements and family identities. Retain
   mature proportions, bright color, fitted chest decoration, shaded wisp folds
   and the corrected equine joints while improving the shared generator.
5. Review brighter foreground lighting in crowded, dim and back-facing gameplay.
   Keep ceilings omitted and preserve the black unicorn's readable charcoal coat.
6. Finish resource-lifetime and startup review. Establish repeatable cold/warm
   measurements and verify the bounded idle cache without reducing quality.
   Dynamic cloth also needs measured update and mesh-upload cost before approval.

### Supporting studies

| Study | Actual reference | Generated target |
| --- | --- | --- |
| Warrior anatomy | [Baseline](warrior-baseline.png) | [Sculptural target](warrior-target-v1.png) |
| Early gameplay forms | [Baseline](gameplay-baseline.png) | [Cast target](gameplay-target-v1.png) |
| Expressive color | [Gameplay baseline](gameplay-baseline.png) | [Richer color study](gameplay-color-study-v1.png) |
| Bird construction | [Emu baseline](emu-baseline.png) | [Emu target](emu-target-v1.png) |
| Female fantasy silhouette | [Baseline](female-warrior-baseline.png) | [Fitted figure and outfit](female-warrior-target-v1.png) |

Targets may alter framing, labels, cosmetic identity, equipment or small anatomy
despite preservation instructions. They are art references, not camera, UI,
mechanical or anatomical specifications. Preserve disclosed appearances in the
implementation. The latest gameplay target also changes the player's weapon
and some neighboring costumes; use its connected masses, material hierarchy and
color relationships rather than copying those changes. The richer study's broad
amber pool must not overpower neighboring actors. The bird concept's toes and
joints require independent construction rather than literal copying.

The latest reference round explores [adult female adventurers](female-adventurers-target-v2.png),
[interchangeable female armor](female-armor-layering-target-v1.png),
[male adventurers](male-adventurers-target-v1.png), and
[creature families](creature-families-target-v1.png). Both female prompts explicitly
request adult women. Use the references for mature character appeal, fitted
layering, materials and coherent species forms. Their taller humanoid proportions
do not replace the selected compact Bold Storybook target. The armor study is an
aspirational visual layering example; it does not establish equipped-armor support.

[Generation prompts](target-prompts.md) record the reference instructions in
publication wording.
Generated images are not deployable model assets. Their fine ornament is
subordinate to the larger sculptural forms and coherent motion.

### Verification boundary

Art exploration used release builds, source review and manual renderer captures.
PR preparation adds focused body/fitting, cache, renderer and native-process
checks; the continuation plan records the acceptance boundary. Broad Windows/Linux
debug/release verification belongs to the independent 3D CI matrix. Neither a
passing test suite nor a captured frame establishes completed art quality,
real-speed motion approval or general equipment compatibility.
