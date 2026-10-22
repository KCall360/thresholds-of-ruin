# Rogue creature models: plan for the missing 3D models

This plan covers giving every Rogue monster in `scenarios/rogue-exploration` a
3D model of its own, working from the maintainer's concept sheets. It extends
the [procedural 3D art plan](3d-art-continuation.md) and follows the
[art direction](../art/3d-art-direction.md); it is not a gameplay milestone and
needs no server or protocol change.

## Status

Steps 1 to 8 are implemented on `feat/rogue-creature-models`: all 16 models,
their idles, antics, deaths and signature motions, with routing, rig, support
and motion tests and an actual-process line-up test. Step 9's visual review
used offline line-ups and bestiary captures; real-speed motion review and
crowd cost measurement remain open, as for the rest of the cast.

The original 16-model implementation scope below is historical. Ongoing
full-cast refinement uses the maintainer's
[2026-10-20 reference set](../art/rogue-concepts/bold-storybook-2026-10-20/README.md):
all 26 Rogue monsters plus the approved humanoid body anchor. Ice Monster's
refinement is deferred while the maintainer reconsiders its character sheet.
The authoritative
[Bold Storybook definition](../art/3d-art-direction.md#proportions-bold-storybook)
governs this work, including mature anatomy and the styling of adult women's
clothing, armor and accessories. This updates visual direction, not gameplay.

The 2026-10-21 sharing checkpoint consolidates reusable garments and accessories,
connected lined cloaks, constructed boots, leaner female thigh contours and
shared non-humanoid components. Compact bird, beast and equine supports now
follow their mesh and joint dimensions together. Rattlesnake rests extended;
Ice Monster refinement remains deferred. See the
[sharing checkpoint and remaining work](3d-art-continuation.md#sharing-checkpoint--2026-10-21)
for the review limits and the current local verification scope. Shared equipment
disclosure and a general external monster-authoring format remain future work.

## Where things stood

Before this work the scenario named 26 creature assets. The 3D client resolves each one through
its palette table (`crates/client-3d/model/src/appearance.rs`), falling back to
the nearest dotted prefix and finally to `CreatureModel::Placeholder`.

| Status | Monsters |
| --- | --- |
| Own model | Emu, Snake, Hobgoblin, Zombie, Quagga, Centaur, Yeti, Black Unicorn, Ice Monster, Vampire |
| Borrows a sibling's model | Troll (draws as Hobgoblin), Rattlesnake (draws as Snake) |
| Borrows an unrelated model | Orc (`creature.humanoid` → generic Common biped), Kestrel (`creature.bird` → Emu) |
| **Placeholder marker** | Aquator, Bat, Dragon, Griffin, Jabberwock, Leprechaun, Medusa, Nymph, Phantom, Wraith, Xeroc, Venus Flytrap |

**Original implementation scope:** the 12 placeholders, Orc and Kestrel (which looked like a
different creature), and Troll and Rattlesnake (which drew as a relative
but have their own sheets: the troll's claws, padded feet and yellow-olive
hide; the rattlesnake's diamond pattern and rattle). That is 16 new models;
the last two are small variations of existing recipes.

**Historical exclusion:** Ice Monster already had a floater model, so its
crystal-core and shard-arm redesign was outside the missing-model pass. It is
retained in the current reference set; its refinement is currently deferred.

## Concept sheets

The sheets this plan uses are in
[`docs/art/rogue-concepts/`](../art/rogue-concepts/README.md), indexed by
monster and letter, alongside the existing art references.
The current dated index includes all 26 monsters across 24 sheets and the
approved humanoid anchor; the older WebPs remain historical. These are
aspirational targets. Follow the authoritative Bold Storybook definition above
when image proportions or incidental annotations disagree with it. Do not
infer new gameplay or server capabilities from reference labels or poses.

What the sheets contribute beyond appearance:

- **Two sexes for people-like monsters** (Orc, Leprechaun, Nymph, Medusa,
  Phantom, Wraith). The sheets differ by
  body shape, hair and costume cut, which the existing `Variant::woman` and
  build choices already express.
- **One shared design for beasts and aberrations** (Bat, Kestrel, Griffin,
  Dragon, Jabberwock, Aquator, Xeroc, Venus Flytrap). Male and female sheets are
  near-identical, so shade/pattern variation is enough.
- **Joint landmarks.** The Bat/Kestrel sheet marks shoulder, elbow, wrist and
  hip; the Aquator sheet gives its tendrils three or four bones. Use these as
  rig landmarks, not as decoration.
- **Signature poses** (claw strike, reach/filch, drain, rending, gaze). These
  become motions only where the server already discloses a matching ability
  (see step 7).

## Constraints

- **Appearance comes only from what the server discloses.** `ActorView` carries
  an asset name and an appearance seed, nothing about flight, invisibility, sex
  or state. A model may look like a flier or a ghost because its asset is a
  flier or a ghost; it must not show disguise, invisibility or flight changes
  the observation does not report.
- **Xeroc's disguise is already server-owned.** While disguised the server
  discloses it as an item (`rogue.object_guise`, currently a mace), and the
  client draws that item. The 3D work is the true form only. The sheet's chest
  disguise and reveal sequence are not a client feature.
- **Phantom visibility.** The server sends a phantom only when the observer can
  perceive it. Its translucent "visibility study" look is its normal
  appearance (maintainer decision); it is never used to suggest a hidden
  phantom.
- **Fliers.** Bat and Kestrel are always drawn in flight, including at rest
  (maintainer decision). This is part of their look, not a disclosed state.
- **Reuse before adding.** New recipes compose existing body plans, anatomy,
  `robe.rs` cloth, `serpent` segment chains and `bird` feathers. Add a new
  shared part only when at least two models need it.
- **Cost.** Keep the mesh cache's identity and idle budgets. Wings and segment
  chains multiply part counts; measure first-use generation for the new models
  against existing ones (step 8) rather than assuming.

## Shared parts to build first

| Part | Used by | Built from |
| --- | --- | --- |
| Palette entries and `CreatureModel` variants | all 16 | `appearance.rs` table; one variant per asset |
| Wing rig: shoulder, elbow, wrist joints plus finger or feather fan | Bat, Kestrel, Griffin, Dragon | New `J` joints (`WingL/R`, `WingForeL/R`, `WingHandL/R`); `JOINTS` grows accordingly |
| Membrane skin stretched between supports | Bat, Dragon, Aquator toe webs | `membrane_parts::MembraneFan`: curved scallops, shallow tension, closed thickness and explicit outward facing |
| Feathered wing surface | Kestrel, Griffin | Shared closed feather vanes on the wing rig |
| Hooked raptor mandibles | Kestrel, Griffin | `raptor_parts::HookedBeak`: closed broad upper/lower shells with a ridge, downturned hook and mouth gap |
| Tail and spine chains | Dragon, Jabberwock, Griffin (tufted tail) | Existing serpent segment chain, parented to `J::Tail` |
| Segmented tentacle/stalk chain | Medusa hair snakes, Aquator tendrils, Flytrap stem | Same segment chain at small scale, with a head or tip part |
| Tattered cloth hem | Phantom, Wraith | `robe.rs` continuous robe with a jagged, partly transparent free edge |
| Horn, crest and frill attachments | Dragon, Jabberwock | New small sculpt helpers on `J::Head` |

## Composing future non-humanoid monsters

Start with the supporting skeleton and the few masses that identify the animal.
Author joint landmarks, segment lengths and foot or hand roles together. A
grasping arm must stay free during travel; a supporting leg must reach its sole
and retain contact through turning, actions and recovery. Reuse a body plan only
when those contracts match, rather than copying another species' stance.

The small construction pieces in
[`beast_parts.rs`](../../crates/client-3d/src/creatures/geometry/beast_parts.rs)
provide oriented thick shell plates, staggerable fang rows in a local mouth
frame, and closed fleshy leaf blades with a raised middle and rolled rim.
Recipes choose placement, dimensions, overlap, pigment and bite direction;
the shared pieces supply closed volume. Keep the existing shared eye sockets,
feather vanes, wing chains and profiled limb construction as the other building
blocks. The shared hooked beak in
[`raptor_parts.rs`](../../crates/client-3d/src/creatures/geometry/raptor_parts.rs)
provides broad ridged upper and lower mandibles with a downturned hook. Recipes
choose attachment, breadth, depth, length, hook drop and pigment. Griffin and
Kestrel use different dimensions while keeping the same closed construction.
The closed skin in
[`membrane_parts.rs`](../../crates/client-3d/src/creatures/geometry/membrane_parts.rs)
spans an ordered fan of supports with curved scallops, shallow tension, distinct
outer/inner faces and a sealed perimeter. Recipes choose facing, thickness,
scallop depth, camber and support bow. Keep the rim ordered around the center
with consistent projected sector facing; fit the bowed attachment rays inside
the supporting fingers or curve those supports with the same profile. Bat and
Dragon wings and Aquator toe webs use this construction at different scales.
Broad grounded pads serve both webbed and blunt crawling feet; their
sole ring stays planar at the authored contact depth. Bent supporting chains
retain all three components of each segment offset in walking and local action
poses, so splaying a creature's feet does not collapse them back below its hips.
Aquator and Xeroc exercise these shared rules with distinct muscle and toe
shapes. Profiled segments terminate in rounded flesh where a bent joint
exposes their ends; Xeroc keeps skin-colored shanks and pads with separate
pale nails. The flytrap reuses fang rows for its opposing cilia and leaf blades
for both the rosette and stalk growths. Recipes choose leaf breadth, arch, droop
and edge color independently. Promote another component when real recipes share a construction rule;
species identity still belongs in the recipe. There is no external monster
format or general creature editor in this work.

Rendered mass scales belong to mesh descendants; they do not automatically
move child joint pivots. The shared `ReptileHead` in
[`anatomy.rs`](../../crates/client-3d/src/creatures/anatomy.rs) resolves anatomical
head scale and muzzle length, and applies the complete rendered head scale to
both the lower-jaw mesh and its hinge. Dragon and Jabberwock use the same rule.
Keep a new articulated mouth's dimensions correlated this way, rather than
repairing its pivot and lower jaw with independent offsets. Measure narrow
surface cross-sections by intersecting emitted triangles; vertex-only slices
can miss the surface between longitudinal mesh rows.

Dragon's shorter bent supports and connected shoulder/haunch masses reuse the
profiled limb construction. Dragon and Jabberwock claw feet share broad planar
palms, substantial splayed toes and curved nails. Keep the palm's sole at the
same contact depth the support solver uses, independently of decorative claws.

Footwear also separates anatomy from the worn shape. `BootLast` in
[`anatomy.rs`](../../crates/client-3d/src/creatures/anatomy.rs) resolves a leather
upper, layered welt/outsole and an optional tapered heel in ankle-local space.
[`boot_parts.rs`](../../crates/client-3d/src/creatures/geometry/boot_parts.rs)
emits closed material volumes from those sections. Heeled choices raise the
arch while the heel tip and forefoot keep the common sole plane; flat choices
retain a continuous planted midsole. The selected outfit controls this shape
for native and replacement boots. Bare, stone and furry feet keep their own
recipes. The same last supplies conservative contact ellipsoids to the moving
robe, with complete emitted triangles checked inside convex contact bounds.
Do not substitute anatomical bare-foot bounds for a constructed shoe.

The ground-bird recipe resolves hip, upper/lower support vectors, ankle height
and body lift together in `BirdBody`; generation and both local and travelling
support use those dimensions. Its substantial hock, tapered shank and broad
three-toed sole retain separate foot articulation. Serpents share `SerpentHead`
dimensions for their larger skulls, lower-jaw hinge, tongue and floor envelope.
Their paired fangs fold along the palate as the jaw closes; the existing
independently placed body segments still own travel, coils and falls. Keep
attachments and their animation in the same recipe rather than resizing meshes
without moving their pivots.

For each new monster, test emitted meshes and joint/support behavior, extend
the native lineup witness, then review front, profile, rear and action views at
gameplay scale. A passing geometry test does not approve a silhouette. Shape,
rig and animation changes belong in the same refinement.

## Per-model recipes

| Monster | Plan | Style / recipe | Key features from the sheet |
| --- | --- | --- | --- |
| Orc | Biped | New `Orc`: heavy humanoid like Hobgoblin, broader | Olive skin, lower tusks, topknot, red scarf and loincloth, leather harness, bracers, boots |
| Leprechaun | Biped, scale ≈0.75 | New `Leprechaun` | Red hair (man: beard; woman: long braid), green frock coat with gold trim, purple waistcoat, buckled belt and boots |
| Nymph | Biped | New `Nymph`, barefoot | Leaf skirt and bodice, leaf crown in the hair, wrapped ankle straps, bare arms with cuffs |
| Medusa | Biped | New `Medusa` | Scaled green skin, clawed hands, 6–9 hair snakes from the stalk chain, plum robe-skirt with gold trim, bronze bracers and boots |
| Phantom | Biped | New `Phantom` on the robe | Pale blue-grey skin, white hair, tattered violet robe and scarf, translucent hem, glowing eyes |
| Wraith | Biped | New `Wraith` on the robe | Deep hood, tattered teal-grey layered cloak, claw-fingered hands, cyan eyes; distinct from Phantom by hood, darker palette, longer claws |
| Bat | Bird (or new Flier) | New `Bat` | Large ears, fox-like muzzle, fur ruff, membrane wings, clawed hind feet; drawn airborne |
| Kestrel | Bird | New `Kestrel` | Hooked yellow beak, grey cap and moustache stripe, spotted buff breast, rufous back, banded tail; drawn airborne |
| Griffin | Quadruped | New `Griffin` | Eagle head and taloned forelegs, lion hindquarters, folded teal-and-cream feathered wings, tufted tail |
| Dragon | Quadruped, large | New `Dragon` | Red scales and cream belly plates, swept-back horns, dorsal spines along neck and tail, membrane wings |
| Jabberwock | Beast rig, reared on hind support | New `Jabberwock` | Raised chest, free long grasping arms, bent hind legs, antler-like horns, orange feather-scale frills, green scales and a balancing tail; must not read as a recoloured dragon |
| Aquator | Quadruped, low | New `Aquator` | Squat frog-like body, teal skin with orange mottling, dorsal gill fan, two tendrils (3–4 segments) with rusty mineral tips, webbed feet |
| Xeroc | Quadruped, low | New `Xeroc` | Turtle-like shell plates, oversized fanged mouth, yellow slit eyes, short stout legs |
| Venus Flytrap | New `Rooted` plan | New `Flytrap` | Rooted base of leaves and roots, segmented stem, toothed two-lobed trap head that opens and closes; no locomotion |
| Troll | Biped | New `Troll`, split from Hobgoblin | Taller and heavier, yellow-olive hide, long dark claws, padded clawed feet, red nose, leather harness and ragged olive kilt |
| Rattlesnake | Serpent | New `Rattlesnake`, split from Snake | Extended S rest pose with raised head and rattle, olive-and-tan diamond pattern, broader triangular head, segmented rattle at the tail tip |

Use one shared plumage, hide or scale treatment per family and let
`Variant::shade` and `pattern` give individuals small differences.

## Order of work

The original implementation sequence was:

1. **Commit the reference sheets.** Copy the original scope's sheets to
   `docs/art/rogue-concepts/`, with a short index of monster, letter and file.
2. **Route every asset.** Add the 16 `CreatureModel` variants and palette rows,
   each starting from its nearest existing recipe so nothing is a marker any
   more. Add the test below before the variants.
3. **People-like monsters** (Orc, Troll, Leprechaun, Nymph, Medusa): biped styles and
   costumes, both sexes and all three builds. Medusa's hair snakes introduce
   the segment-chain attachment.
4. **Robed undead** (Phantom, Wraith): tattered hem on the existing robe, plus
   the transparency material.
5. **Fliers** (Bat, Kestrel): the wing rig, membrane and feather surfaces, and
   an airborne idle and travel posture that never lands.
6. **Large beasts** (Griffin, Dragon, Jabberwock): wings on a quadruped, tail
   and spine chains, horn and frill attachments.
7. **Aberrations, the plant and the rattlesnake** (Aquator, Xeroc, Venus
   Flytrap, Rattlesnake): low body proportions, tendrils, the `Rooted` plan,
   and the rattle and coil on the serpent recipe.
8. **Signature motions.** Map only abilities the scenario already discloses:
   `rogue.breath_weapon` (Dragon), `rogue.confusing_gaze` (Medusa),
   `rogue.corrode` (Aquator), `rogue.energy_drain` (Wraith),
   `rogue.ambush_from_guise` (Xeroc), and `rogue.venomous_bite`
   (Rattlesnake). Plain melee keeps the existing strike motions. Leprechaun and Nymph
   theft and the leprechaun's vanish are not separate disclosed abilities today,
   so they get no new motion.
9. **Review and measure.** Run the offline bestiary, portraits, side
   (`--azimuth 90`) and rear views and a travel study for every new model next
   to existing controls; then compare first-use mesh generation and frame cost
   for a mixed crowd against the current cast.

Steps 3–7 are independent once step 2 lands and could run in parallel branches,
but they share `geometry.rs` and `creatures.rs`, so they should land one at a
time.

## Tests and acceptance

- **Routing test (fails first):** every `creature.*` asset in the
  rogue-exploration scenario resolves to a model other than `Placeholder`,
  `Humanoid` or another species' model. It originally failed for all 16.
- **Art contract tests** (`creatures/art_contract_tests.rs`): each new rig has
  its landmarks (wing joints, tail chain, tendrils), finite closed meshes, and
  stays within its body plan's height and footprint.
- **Determinism:** the same seed gives the same look for every observer, and
  new visual choices are part of the cache key.
- **Reaction and motion tests** cover hurt, death and the mapped signature
  moves for each new family.
- **Actual-process acceptance:** launch the release client against the
  rogue-exploration scenario and capture each of the 16 in the bestiary and in
  live play, recording the source revision.
- **Visual acceptance** is the maintainer's review of those captures against
  the sheets. Passing tests do not establish it.
- Verification follows the 3D client's own gate and CI matrix as well as the
  repository's [testing policy](../development/testing.md).

## Decisions

Decided by the maintainer: Bat and Kestrel are always drawn in flight; the
translucent Phantom is its normal look; Troll and Rattlesnake get their own
models in the original pass. Ice Monster's floater remains unchanged while its
new character-sheet direction is reconsidered.
