# Procedural model refactor handoff

This is an implementation map and acceptance checklist, not a completed-refactor
claim. Superseded refactor experiments remain in the
[experiment history](procedural-model-experiments.md); detailed visual findings remain in the
[art continuation guide](3d-art-continuation.md). Preserve existing outfits,
sex-specific fitting, intrinsic creature anatomy and the original dirty checkout.

## Start here

This branch contains working local wardrobe/equipment previews and a scripted
renderer, alongside refactor experiments. It does **not** yet carry generic worn
item composition from the server to every client. Start with the current boundaries
below, then use these implementation checkpoints:

- [Resolved equipment](#resolved-equipment-boundary): one local result for clothing
  replacements and the typed independent body layer.
- [Cloak cut and exact composition](#cloak-cut-ownership): garment-owned length and
  the checked `Look::composed` entry point, alongside retained interactive fallbacks.
  The [Wraith cloak](#recipe-owned-wraith-cloak) now uses the same recipe and
  replacement path, including its ragged hem.
- [Coat openings](#recipe-owned-coat-openings) and [sewn chest details](#shared-sewn-chest-details):
  recipes retain their cuts and decorations when copied; equipment coverage leaves
  the base recipe intact. [Continuous robes](#recipe-owned-continuous-robes) now carry
  their dynamic garment, sash and cuff behavior across recipe copies.
  [Attached garment details](#portable-attached-garment-details) retain hip,
  shoulder and sleeve decorations independently.
- [Attachment migration](#attachment-migration-map): current coordinate owners and
  a bounded first consumer for a future landmark interface.
- [Shared renderer execution](#shared-renderer-event-and-animation-execution): event
  timestamps, pose refresh, persistent live playback and deterministic capture.
- [Independent locks](#independent-lock-checkpoint) and
  [contact collection](#contact-collection-allocation-cleanup): current hair/contact
  representation; superseded approaches and their failures remain in the
  [experiment history](procedural-model-experiments.md).
- [Appearance proposal](appearance-composition-contract.md): cross-client disclosure,
  lifecycle and fallback requirements, with four explicit decisions still open.

For cache work, preserve the separation in
[geometry construction](../../crates/client-3d/src/creatures/geometry.rs): the full
body-part composition key (including explicit recipe values), head key including
headwear/cuirass/earrings, and base hair key
have different reuse scopes. [Rig](../../crates/client-3d/src/creatures.rs) owns
mutable hair, robe and cloak state; resetting presentation invalidates that state.
Fully attached hair components reuse immutable source meshes and move only through
their joint transform. Only components with nonzero deformation weights receive
private mesh storage and per-frame vertex/normal updates.
The current debug-formatted local keys are implementation details, not stable
content identifiers or a wire-format proposal. Replacing them with semantic keys
must retain selection-order independence, exact preset restoration, and per-instance
motion isolation. New visible components need an explicit cache dependency and
contact invalidation rule. The rendering boundary now includes direct recipe values in body-part identity;
see the [shared-cache boundary](#direct-recipe-cache-boundary) for its regression
and the remaining public-API limits.

Before declaring handoff acceptance, use the [work queue](#next-developers-work-queue)
and the outstanding work/evidence list below
and distinguish local focused checks from the still-required publishing/CI gates.
Do not rerun every historical experiment: select checks for the contracts being
changed and retain their source mapping. A native screenshot proves a rendered
case, not exhaustive fitting or server-side disclosure correctness.

## Latest handoff state

This branch is a work-in-progress handoff snapshot published at the maintainer's
explicit request without completing a new pre-push gate. That gate was started
and then stopped at the maintainer's direction after formatting passed. The
verified checkpoints below remain scoped evidence, not a claim that this branch
is merge-ready. Continue from the work queue and preserve the outstanding
platform/profile and publishing requirements before any merge.

The branch was later rebased without conflicts onto main at `511ea466`
(#191). The rebased tree equals a merge of the two. The checkpoints
below were recorded on the earlier base (#171); their source fingerprints describe
that base, not the rebased commits.

The branch was rebased again onto main at `feae51f2` (#195), which had
already taken its own protocol bump for hallucinated presentations. Conflicts
were resolved as follows:

- **Worn equipment:** the worn list now bumps the protocol to 63.
- **Hallucination:** a hallucinated figure carries no worn list, so its true
  gear cannot betray it.
- **Palette discovery:** a ground item with a presentation contributes only
  that presentation's asset, never its true corpse body asset.

The local evidence listed below was recorded before this second rebase.

The worn-equipment slice and Rogue armour recipes followed on that
base. Local evidence for those commits:

- **3D release suite:** 395 renderer-library, 2 model-library and 31 model
  integration tests pass, with one ignored subprocess fixture. Focused armour
  tests passed again after the shoulder-cap and tasset change.
- **`quick` tier (Windows):** formatting, Clippy, architecture, build tools, the
  isolated simulation check and the debug Rust suite pass.
- **Arena-search sampling:** two tests failed only because the shared pinned
  Python environment had numpy 2.5.3 against the branch's 2.5.4 pin. With the
  maintainer's approval, the environment was updated from the hash-pinned
  requirements, and all 7 sampling tests now pass.
- **Debug process tests:** five performance tests and one earned-route test
  first hit the run's 600-second build and wall-clock limits. One performance
  test also exposed a missing `subprocess` import, which is now fixed. All 18
  performance tests and the earned-route test pass when rerun individually with
  prebuilt examples.
- **Native mouse test:** the standing local mouse-click test is waived, not
  passed.
- **Hair timing diagnostic:** a single workload-4 `model_bench` run after the
  query reuse, on the same machine, gave the following, with 90 frames per case.
  Earlier figures are from [exact fixed points](#exact-control-solver-fixed-points).

  | Case | Full update p50/p95/max | Earlier |
  | --- | --- | --- |
  | Human | 9.69/10.91/11.90 ms | 9.623/9.701/10.458 ms |
  | Nymph | 39.98/46.11/47.61 ms | 43.526/58.041/58.275 ms |
  | Nymph control fitting | 11.93/19.25/20.07 ms | 15.142/25.194/25.416 ms |
  | Nymph face/edge constraints | 26.67/27.98/28.33 ms | — |

  Face constraints now dominate. This is not a controlled base/head
  comparison, and the long-hair case still misses the 20 ms budget.
- **Not run:** a controlled `perf_compare` measurement of the hair query
  changes, the 3D client's own `verify.py`, push and full gates, and both-platform
  CI. The query reductions are proven only by exact query-count tests.


The consolidated Windows release contract check passes 387 renderer-library
checks, one model-library check and 31 model integration checks, plus tooling,
showcase-source validation, formatting, all-target lint and architecture checks.
The one ignored entry is `launch::tests::malformed_ready_fixture`, invoked as a
child by `malformed_startup_reaps_its_owned_server`, which passed. The source
fingerprint matched before and after the run. The later wearer-pigment migration has separate focused evidence below; it does
not automatically inherit this complete Rust-suite result.

Native evidence remains the recent affected fixtures: equipment replacement with
Warrior/Mage/Wraith, and the 12-family women lineup covering the moved garment
details. These are combined with the earlier selected-process checkpoint for
unchanged paths, not represented as a new all-process or full-platform gate.
Debug-profile qualification, final Windows/Linux CI and publishing gates remain
outstanding. The maintainer's exact three mouse-test waivers remain local.

The latest measured demanding Nymph composition is 43.526/58.041/58.275 ms
p50/p95/max per CPU update, with 90 frames in the diagnostic. It still misses a
20 ms update budget. See [exact fixed points](#exact-control-solver-fixed-points)
for the measured source state and limits; older timing tables below are historical
snapshots, not controlled before/after results or current FPS measurements.

For an immediate visual review, use the existing
[scripted model review instructions](../../crates/client-3d/review-scripts/README.md),
including `showcase-loop.ps1` for one-window live playback and `--scene-script`
for deterministic captures. Do not use capture timing to judge live smoothness.

Common exposed-skin pigments now retain wearer references as described below.
Audit the remaining native boot/anatomy pigments and validated recipe admission
before public native-recipe transfer. Then complete the shared fitting/attachment interface and
item capability/coverage rules. Server-driven appearance still requires the four
explicit decisions in the appearance proposal; no generic wearable payload or
all-client implementation is claimed by this checkpoint.

## Next developer's work queue

These are remaining work packages, not claims that the local checkpoints complete
the refactor. Preserve the broader objective while using focused tests for each
changed boundary. The tables and historical timings below describe different
recorded states; the latest local source/binary fingerprints and logs remain in
the ignored evidence directory, not in portable repository documentation.

| Priority and boundary | Concrete next change | Evidence needed to close it |
| --- | --- | --- |
| Garment ownership | Audit remaining native/style-dependent details before exposing arbitrary native recipes as interchangeable outfits. Continuous robe/waist/cuff ownership is now recipe-driven; qualify additional bodies and independent subpiece coverage before generalizing it. | Preserve native versus explicit recipe witnesses, cross-body fitting, equip/remove restoration and moving-legs captures; add fixed conflict cases rather than a broad Cartesian expansion |
| Recipe identity | Resolve the [wearer-owned pigment boundary](#wearer-owned-pigment-migration) before general native transfer. Preserve the renderer-boundary recipe key and introduce a validated recipe-assignment API before allowing arbitrary native-recipe transfer through scripts or gameplay. Define canonical semantic identity if replacing the current local debug-formatted keys. | Keep the shared-library distinct-recipe, head/hair reuse, repeat-selection and native-restoration witness; extend it for new dependency boundaries rather than relying on fresh-library fitting tests |
| Shared fitting | Extract a fitting description from the existing bust, torso, hip and attachment consumers. Use semantic landmarks for placement and actual supporting surfaces for clearance; neither replaces the other. | Existing asymmetric bust/hip and waist-frame witnesses remain valid; an actual garment and an equipped item consume the same description |
| Hair motion and cost | Prioritize the measured long-hair case. Separate support-query/constraint cost from mesh writes and normals before choosing a representation change; qualify temporal continuity as well as endpoint clearance. | Retain scalp pins, strain and full-mesh contact; add a focused moving-obstacle/history witness, then a controlled same-workload release comparison |
| Generic visible items | Define required body capabilities and coverage per item family using the proposal's capability matrix. Do not generalize the current four preview slots into a universal anatomy model. | Explicit unsupported-combination errors plus fixed cases for repeated limbs, layered garments and multi-part equipment; correct removal/restoration |
| Server and all clients | Obtain the compatibility/content decisions described in the appearance proposal, then implement one observer-filtered projection and shared interpretation across text, ASCII, 3D and headless clients. | Hidden-state indistinguishability, atomic full/delta equivalence, self/corpse carriers, reload/redisclosure, bounded malformed inputs and actual-process checks for each client |
| Qualification | Freeze the final inputs and run the applicable publishing gates and platform/profile CI. Keep the explicit local mouse waiver visible. | Receipts identify the exact tested state, exclusions and failures; later focused fixes do not silently inherit a full-gate pass |

Two distinctions are especially important during implementation:

- `Composition::resolve` currently combines local preview choices. It does not
  decide gameplay permission or which equipment an observer may know about.
  Moving its Rust structs into the wire protocol would carry renderer choices
  across the wrong boundary. Use the proposed public semantic description instead.
- `Look::composed` rejects unsupported exact preview selections; interactive
  wardrobe browsing deliberately retains its documented fallback policy. A future
  server-supplied appearance must not silently take that browsing fallback or
  restore a covered default item merely because a component is absent.

The existing persistent renderer and deterministic capture modes are useful tools
for these packages. Reuse their focused fixtures; do not rebuild the renderer or
repeat its full historical test suite solely to update a recipe or this guide.

## Current boundaries

| Concern | Authoritative implementation | Current limitation |
| --- | --- | --- |
| Disclosed identity | [CreatureAppearance](../../crates/protocol/src/wire.rs), [server adaptation](../../crates/server/src/adapt.rs) | Seed plus optional asset identifier; no general wearable composition payload |
| Client model interpretation | [3D appearance adapter](../../crates/client-3d/model/src/appearance.rs), [scene construction](../../crates/client-3d/src/scene.rs) | Scene creates `Look::of(model, individual(seed))`; review-script overrides are not a server-driven wardrobe |
| Body and resolved look | [Look and Variant](../../crates/client-3d/src/creatures.rs) | Renderer-specific proportions, materials, rig and cache identity remain coupled |
| Clothing vocabulary/composition API | [Wardrobe and Outfit](../../crates/client-3d/src/creatures/outfit.rs) | Shared types and public resolution entry points; reusable pieces still need explicit coverage/ownership contracts |
| Authored recipes | [Wardrobe presets](../../crates/client-3d/src/creatures/outfit/presets.rs), [native outfits](../../crates/client-3d/src/creatures/outfit/native.rs), plus existing royal/sylvan/studies modules | Recipe selection remains separate from fitting geometry; native details and anatomy restrictions are not a generalized capability system |
| Independent visible equipment | [Equipment](../../crates/client-3d/src/creatures/outfit/equipment.rs) | Review fixtures support helm, boots, cloak and breastplate; this is not the full future item catalog |
| Scripted verification | [capture runner](../../crates/client-3d/src/review_script.rs), [live runner](../../crates/client-3d/src/review_script/live.rs) | Fixed-tick capture and wall-clock playback have deliberately different clock contracts |
| Hair contact | [hair fitting](../../crates/client-3d/src/creatures/hair_contact.rs) | Independent parametric locks pass the focused jump and attachment suite; broader composition coverage, visual refinement and controlled performance qualification remain open |

The preset and native method bodies were extracted unchanged into dedicated recipe
modules, retaining `Outfit::from_wardrobe` and `Outfit::native` as the existing entry
points. The extraction checked exact body text after indentation removal; authored
constants, sex-specific fallback rules and recipe calls were preserved. All six
focused composition tests, release all-target Clippy, the executable rebuild,
and the native equipment conflict/restoration fixture pass.
This is a responsibility split, not a
new recipe format or claimed rendering/performance change.

## Verification entry points

Use the smallest relevant set during iteration; these are focused checks, not a
replacement for the repository's publishing gate. Run from the repository root
with the Windows environment and log redirection required by `AGENTS.md`.

| Change | Focused entry point | Escalate when |
| --- | --- | --- |
| Exact composition admission | Same Cargo invocation below with `checked_composition`, then affected `review_script` tests | Native rejection must identify the actor and create no output; retain a valid replay witness |
| Outfit selection/replacement | `cargo test --manifest-path crates/client-3d/Cargo.toml --release --lib creatures::outfit::equipment::` | Changed geometry also needs the native equipment conflict/restoration fixture |
| Contact collection | Same Cargo invocation with `creatures::visual_contact::tests` | Contact/geometry semantics change: retain emitted-mesh hair witnesses |
| Hair fitting | Same Cargo invocation with `contact_survives_` | Add a fixed witness for a newly found failure; do not expand the Cartesian catalog |
| Mesh frame conversion | Same Cargo invocation with `reparented_mesh_preserves_placement` and the existing waist-owner witness | Changed mounting behavior needs an animated native composition witness |
| Renderer event/clock behavior | Existing `review_script` tests plus the affected timestamp/live-loop native test | Do not substitute screenshot existence for event or timing assertions |
| Prose/source-map changes | `python -m unittest discover -s scripts -p test_documentation.py` | No renderer rerun solely for prose |

Native fixtures are in `crates/client-3d/scripts/test_client_process.py`; set
`TOR_3D_TEST_BINARY` to the newly built executable and `TOR_TEST_BINARIES` to the
matching core binaries. Use unittest's `-k` filter to select the affected fixture,
and retain logs/artifacts under the ignored local evidence directory. A successful
Cargo check does not update the executable used by these tests.

For the frozen standalone 3D qualification, use
`python crates/client-3d/scripts/verify.py --profile debug` and the corresponding
`--profile release` invocation when required by the accepted gate. These run broad
workspace/process coverage; they are not edit-loop commands. The core
`scripts/verify.py` is a separate entry point, not a substitute for 3D verification.
Freeze all repository inputs during fingerprinted gates. Preserve the existing
maintainer mouse-test waiver for this work without weakening the tests themselves.
Full final-commit platform/profile CI and applicable pre-push gates remain required.

### Windows release checkpoint

A frozen-source Windows release checkpoint passed tooling, showcase validation,
formatting, all-target lint, architecture checks, Rust workspace tests, executable
builds, scenario validation and all 31 selected native/process tests. The library
result was 377 passed and one intentionally ignored subprocess fixture, invoked by
its owning lifecycle test; the model crate added one library and 31 integration
passes. Start/end repository fingerprints matched. The receipt and selected-test
inventory are retained locally with the logs.

Exactly three native mouse tests were excluded under the maintainer's explicit
waiver: item picking/inventory keys, movement/doors/fetch, and clicked attacks.
No committed test or CI command was weakened. This is a passed selected-check
release checkpoint, not the unmodified full gate or final cross-platform CI.
The native checks took about 814 seconds; the Rust test command, including test
compilation, took about 559 seconds. Reuse this unchanged evidence rather than
rerunning the broad suite after each focused edit. Subsequent changes still need
checks appropriate to their affected contracts and recorded source state.

A subsequent test-only cleanup keeps the idle hip-tilt witness's 2,000 frames per
sex and both tilt assertions, but selects the female bob and male cropped hairstyle.
A setup guard verifies that this pose fixture has no loose-hair solver. The first
attempt selected the loose hairstyle and correctly failed that guard; the corrected
focused test passes in 1.61 seconds of test execution. Full-mesh hair jump, attachment,
isolation and native wardrobe coverage remain unchanged. Production code did not
change, so the preceding release checkpoint is retained for those unchanged paths;
this focused result qualifies the modified test rather than triggering another
broad native run.

## Target separation

1. **Disclosed appearance description.** The server owns what an observer may
   know about a body and its visible items. Describe semantic identity and
   appearance, not Bevy entities, meshes, fitting vertices or renderer cache keys.
   Preserve the same cosmetic identity for living actors and their remains.
2. **Client interpretation.** Text, ASCII and 3D clients consume that same
   disclosed description using their own presentation. Define unknown-recipe
   fallback and content-version policy before relying on seeded reproduction.
   Hidden inventory, undisclosed item identity and observer-local interaction
   handles must never become cosmetic seeds or leak through visual records.
3. **Resolved composition.** Resolve body traits, outfit defaults and explicit
   worn items into a deterministic layer plan. Specify replacement, coverage and
   conflicts once. A visible item should contribute its own recipe while fitting
   the current wearer; it must not silently inherit another species or sex.
4. **Body fitting description.** Named sockets and landmarks carry their owning
   coordinate frame, side and anatomical capability. Surface/contact support
   supplies the shape between anchors. Include non-humanoid wearers; avoid a
   universal humanoid slot assumption or a landmark-only clipping solution.
5. **Presentation instance.** Mutable cloth, hair and soft-body history belongs
   to an instance. Immutable recipe data and suitable source geometry can be
   shared. Recomposition, teleport, corpse conversion and live-loop reset need
   explicit invalidation/initialization contracts.

These are proposed contracts. A new wire format, compatibility break or new
implementation milestone still needs maintainer authorization; this document
makes no such decision.

## Proposed visual disclosure contract

The concrete [appearance composition proposal](appearance-composition-contract.md)
records current source boundaries, recommended shared-record semantics, disclosure
examples, lifecycle rules, cross-client responsibilities and implementation gates.
Its [review defaults](appearance-composition-contract.md#concrete-review-defaults-and-migration-boundary)
now spell out repeated-anatomy roles, hidden-ring and corpse transitions, bounded
admission, atomic replacement, and pending catalog dependencies. Structural wire
validation and asset-dependent presentation validation have separate responsibilities.
It remains a proposal; no wire-format decision has been made.

Backend equipment already exists: `EquipmentSlot` and `ItemInteractionView` in
[the wire types](../../crates/protocol/src/wire.rs) describe slots, equipped
positions and known mechanics. They are not an observer-facing model composition.
The existing `equipment_only_action_broadcast_redacts_spectator_wire_and_retained_state`
regression in [server session tests](../../crates/server/src/magic_item_session_tests.rs)
protects private equipment knowledge. Preserve it when adding visible equipment.

Propose a bounded semantic appearance description attached to a disclosed actor:
body recipe identity, stable cosmetic seed and an ordered collection of visibly
worn components. Each component needs a public visual recipe, a semantic attachment
role and only disclosed cosmetic variations. Content definitions map these roles
to species capabilities and local fitting surfaces; wire data must not contain
bone indices, mesh geometry, physics controls or renderer-specific transforms.
An attachment role must distinguish repeated limbs without assuming every wearer
has human hands or feet. Its exact representation remains a design decision.

Resolve outfit defaults and explicit item overrides before generating geometry.
Keep recipe identity separate from gameplay item identity and interaction handles.
A hidden enchantment, curse or inventory operation must not change an observer's
appearance payload or cosmetic seed unless it has an intentionally visible effect.
Text can describe the same visible garments, ASCII can choose a silhouette/color
summary, and 3D can resolve local geometry. Clients must not infer absent facts.

Start with whole bounded appearance replacement in the existing observation update
model; add component deltas only if measurement justifies the complexity. Define
removal, redisclosure and actor-to-remains continuity explicitly. Preserve current
format rejection policy rather than introducing an accidental compatibility layer.
Unknown visual content should have a deliberately specified client fallback;
malformed protocol data should still be rejected. No schema or compatibility
change is implemented or authorized by this proposal.

Acceptance must compare owner and observer snapshots and updates, equip/unequip,
conceal/reveal, save/reload, remains, unsupported visual content, and all clients.
Use a fixture in which two mechanically different items have identical disclosed
appearance: private changes must produce identical public appearance records.

## Work and evidence needed before handoff acceptance

- Hair clearance has two fixed jump witnesses: Human/Sylvan Duelist/breastplate
  and Nymph/Royal Princess/cloak. Broader coverage remains open. A local scalp
  boundary test checks attachment influence continuity, but full emitted-mesh
  continuity and moving-obstacle history still need independent qualification:
  clear endpoints alone do not prove a safe swept path. Retain scalp attachment,
  strain bounds and full emitted-mesh clearance witnesses; per-frame displacement
  logging is diagnostic, not a continuity assertion.
- Inventory garment-specific measurements before extracting a shared fitting
  API. Keep sex-specific bust, waist and hip witnesses, including asymmetric
  bodies; a generic API that recreates shelves or armor mismatch is not progress.
- Review and implement the proposed [equipment capability matrix](appearance-composition-contract.md#equipment-capability-planning-matrix)
  for armor, weapons, shields, headwear, gloves, footwear, drapes, jewelry and
  underlayers. It now identifies current support and proposed rules; generic
  capability resolution is not implemented. Diagnose unavailable combinations
  explicitly instead of silently substituting.
- Establish backend-to-wire-to-each-client contracts for visible item changes,
  actor/corpse identity, save/reload, deltas, redisclosure, unsupported recipes,
  bounded inputs and old/new content behavior before calling transport complete.
- Keep focused permanent regression witnesses plus seeded composition samples.
  Record seeds and failing combinations; do not multiply every body, outfit,
  item and animation into an exhaustive routine gate.
- Measure model construction, cache reuse, animation/contact cost and memory
  separately. Live CPU frame intervals are useful pacing evidence, not GPU
  timings or a count of displayed frames lost. Use the repository performance
  harness for release before/after comparisons. Its current `perf_compare.py`
  `models` workload now measures CPU cold/cached construction and mesh-buffer
  storage for three fixed compositions through the separate 3D Cargo workspace.
  Current workload version 4 also times the review renderer's shared pose/transform/
  soft-body schedule over 30 fixed jump frames and separately reports pose/joints,
  robe fitting, drape updates and hair fitting. Hair subphases now separate control
  fitting, mesh deformation, face/edge constraints and mesh writes/normals. Total
  process/GPU memory and deeper attribution within the constraint solver remain open.
  The historical v2 measurements below are retained as evidence, not current schema
  documentation. A controlled
  base/head comparison still requires the same versioned producer and validator on
  both revisions; no speedup has been established. Do not hand-build a base executable
  or treat live playback diagnostics as equivalent evidence.
- Preserve deterministic capture replay and actual-process live-loop acceptance.
  The renderer currently preloads up to 128 compositions; it is not a general
  gameplay streaming cache. Its separate stage construction is a future sharing
  opportunity, not a reason to merge the two time models.

## Historical motion diagnostics

A single release run of model workload version 2 on machine fingerprint `6a1878811f37` identified
an expensive composition. This is a historical diagnostic, not an interleaved
base/head comparison or a speedup result. Each case has three construction samples
and 90 measured pose frames (three fixed 30-frame jumps); there were no failed runs
or competing build processes detected. Raw samples and binary/source fingerprints
are retained locally, not committed as routine output.

| Composition | Pose n | p50 / p95 / max CPU ms | Cold / cached spawn p50 ms | First / added second-instance mesh bytes |
| --- | --- | --- | --- | --- |
| Human, Sylvan Duelist, breastplate | 90 | 9.628 / 9.684 / 9.797 | 1679.300 / 29.490 | 39,981,252 / 131,664 |
| Nymph, Royal Princess, cloak | 90 | 104.790 / 116.995 / 118.154 | 1917.789 / 130.462 | 51,148,204 / 662,584 |
| Mage, native, boots | 90 | 2.462 / 2.546 / 2.586 | 390.772 / 7.016 | 7,453,464 / 155,680 |

Mesh bytes count CPU vertex/index buffers, not total process or GPU memory. Pose
values time the full shared CPU schedule, not an isolated fitting phase or displayed
frame duration. The small construction sample count is diagnostic only. Counts
were repeatable within each case; the second-instance count includes private
mutable meshes despite shared cached geometry.

A subsequent current-tree run with model workload version 3 enables optional
phase attribution: timing separate operations within the shared animation update.
Each phase retains only the latest frame's aggregate; ordinary rendering does not
enable it. The same machine and three compositions produced these 90-frame results:

| Composition | Full update p50 / p95 / max ms | Dominant phase | Phase p50 / p95 / max ms |
| --- | --- | --- | --- |
| Human, Sylvan Duelist, breastplate | 9.669 / 9.764 / 9.932 | Drapes/segments | 9.603 / 9.662 / 9.823 |
| Nymph, Royal Princess, cloak | 105.176 / 117.373 / 117.931 | Hair/contact fitting | 105.001 / 117.203 / 117.761 |
| Mage, native, boots | 2.482 / 2.575 / 2.615 | Robe fitting | 2.425 / 2.518 / 2.548 |

No failed samples or competing builds were detected; construction and mesh counts
remain validated, and raw timings, machine details and binary/source hashes are
retained locally. This is attribution, not a before/after speedup comparison.
The phase totals exclude scheduling, transform propagation and soft-body work.
The Human fixture has no active fitted hair, so its zero hair time is expected;
it does not establish the cost of a long-haired Human composition.

Optimize the Nymph hair path first. Its face-contact relaxation performs bounded
repeated support queries, but this measurement does not yet separate query cost,
iteration count, control fitting, mesh writes and normal reconstruction. Consider
reducing repeated invariant work and accelerating queries before reducing mesh
resolution or constraint iterations. The validator requires complete finite phase
arrays whose sums fit inside their enclosing update, alongside actual pose changes.
Retain full emitted-face clearance and strain witnesses when optimizing: a faster
result obtained by dropping constraints would not satisfy the fitting contract.
Use `perf_compare.py` for a subsequent before/after result with matching workload
versions on both revisions; this single run does not establish such a baseline.

## Prepared contact geometry

`visual_contact::Cylinder` now prepares its center, normalized axis and half-length
when the contact profile is built. Animation changes only its coordinate transform
and distance scale. This removes repeated endpoint subtraction, normalization and
length calculations from each hair/cloak support query, while retaining the same
signed-distance arithmetic and contact shape. Fields are private so a caller cannot
change an endpoint without rebuilding its derived geometry.

A focused reference witness passes before and after the representation change and
requires bit-identical distances for straight, tilted and degenerate cylinders under
identity and nonuniform rotated/transformed query frames. Existing missing-joint and
long-face coverage tests pass. Both fixed full-mesh hair jump witnesses also pass
without changing contact, scalp attachment or stretch thresholds. Release all-target
Clippy passes. A matched before/after performance comparison remains outstanding;
removing repeated source operations alone is not evidence of a measured speedup.

The rebuilt benchmark process and native equipment conflict/restoration fixture
pass. A diagnostic run at the contact-pruning checkpoint, on machine
`6a1878811f37` without competing builds,
retains repeatable counts and valid phase totals (90 frames per case, zero failed
samples): Human full update p50/p95/max 9.674/9.719/9.929 ms; Nymph
79.028/89.262/91.307 ms; Mage 2.475/2.571/2.940 ms. Nymph hair alone is
78.926/89.089/91.139 ms. These absolute CPU schedule timings still identify hair as
the main bottleneck and are not display-frame timings or a controlled speedup claim.


Contact queries also prepare a conservative enclosing box. Its largest signed slab
distance bounds the cylinder from below, so a contributor whose bound exceeds the
current closest distance can be skipped. The check uses the same transformed point
as the exact query, includes a scale-aware floating-point margin, and falls through
to exact evaluation for near ties or unordered values. It does not change constraint
iterations, contact tolerance or mesh resolution. The reference witness compares
pruned and exhaustive minima bit-for-bit over transformed, degenerate, interior,
near-tie and large-coordinate cases; a separate assertion proves distant contributors
are actually skipped. Both full-mesh hair fixtures pass with unchanged thresholds.

Keep immutable shape preparation separate from runtime pose updates when adding
worn-item contact contributors. This is shared visual contact geometry, not gameplay
collision, anatomical capability validation or a server-visible equipment format.

## Cross-client appearance integration map

- [Shared presentation](../../crates/client-common/src/presentation.rs) supplies
  palette-gated cosmetic nouns, glyphs and colors; disclosed names remain the
  authority for identity and commands. Extend that boundary instead of letting
  renderer recipe names become prose or gameplay facts.
- [Text figures](../../crates/client-text/src/adventure.rs) consolidate actor cells
  and prefer the disclosed actor name, falling back to a presentation noun only
  when unnamed. Visible apparel descriptions must preserve that naming authority.
- [ASCII map appearance](../../crates/client-ascii/src/map.rs) resolves actor glyph
  and color through the palette and preserves player/terrain state cues. A worn
  component should not replace those gameplay cues or require 3D mesh semantics.
- [Asset discovery](../../crates/client-common/src/palette.rs) currently walks the
  flat observation assets; [connection handling](../../crates/client-common/src/connection.rs)
  uses that walk for palette requests and also has a remembered-map-page path.
  A nested wearable payload needs bounded traversal and tests at these boundaries;
  audit self appearance and corpse appearance carriers explicitly rather than
  assuming the existing flat walk covers them.

Before selecting a wire representation, decide how repeated/multi-cell actor
observations share appearance records and how partial visibility affects component
disclosure. A whole outfit record must not accidentally reveal unseen or covered
items. Renderer-only suppression (for example earrings under a helm) cannot be the
privacy boundary: the server must decide which semantic appearance facts are
public before sending them. These are design requirements, not implemented wire
changes or a compatibility decision.

The required [comparison harness](../../scripts/perf_compare.py) includes the
`models` workload and its strict validator. It separates cold construction, cached
reuse and a fixed jump's shared animation schedule, with optional phase attribution.
It does not yet isolate redressing/refitting cost or qualify GPU frame timing.
Both compared revisions must contain the same workload version before publishing
qualified optimization tables.
Native watchdog outcomes remain useful process acceptance evidence, not a substitute
for controlled release benchmarks or GPU frame measurements.


## Independent-lock checkpoint

The first-red real-model regression found one rendered triangle influenced by five
independently moving locks. Long loose hair now emits one parametric tube per
explicitly owned control curve, with the existing head cap covering its roots.
Triangles no longer bridge locks. Linear longitudinal subdivision avoids unbounded
spline overshoot beyond the control envelope. Tube resolution is explicit, so static
and animated construction produce the same geometry regardless of builder defaults.
The obsolete analytic sculpt has been removed; nape coverage and separated tips are
checked against emitted triangles.

Each lock is cached under the same hair identity plus its component index. Phantom
hair retains its solid material policy. Mutable contact meshes remain per-wearer;
redressing tests compare immutable source handles, while the existing isolation test
checks that animation changes neither cached geometry nor another wearer. Fully
attached locks now share the cached source directly; their positions and normals
never enter the per-frame deformation path. The extended ownership witness first
failed because the rigid brow lock had a private copy, then passed after the split.
It requires rigid source reuse, private deforming meshes, actual free-hair movement,
and no mutation of a cached source or another wearer. All 16 contact/chain tests and
release all-target Clippy pass for this ownership change. The rebuilt benchmark
process and native equipment conflict/restoration fixture also pass. No performance
speedup is inferred from sharing alone. Vertex
witnesses follow actual multi-mesh scene traversal order. Attachment verification
checks all declared fixed vertices plus an independent spatial crown witness, not
a global height threshold that includes movable jaw/neck hair on another body.

The expanded focused release suite passes 19 hair tests, including the armored jump,
walking/turning, source reuse, independent ownership, emitted silhouette, exact
attachments, clearance and unchanged strain bounds. Seven shared deformation tests
and one authored-recipe test passed during this change. Release all-target Clippy
passes. The experimental native run completed all eight front/side captures under
the unchanged watchdog. The resting front and jumping side views show chest locks
without the previous collar-like fold; shoulder bends remain visibly angular.
The final release binary repeated all eight captures successfully. Repository quick
verification, all 18 documentation tests and whitespace checks pass. This is focused
local evidence, not the full 3D debug/release or both-platform CI matrix; no push or
merge has been performed.

The 15-test focused contact run took about 9 seconds, compared with about 889 seconds
and a failure at the preceding checkpoint. These are diagnostic suite durations,
not a controlled before/after benchmark or GPU frame-rate measurement. The new
representation adds per-lock draw calls. Dense final-surface correction and per-frame
allocations still exist; do not describe this as a finished low-cost simulation.

Remaining refactor work: preserve authored longitudinal parameters directly when
emitting vertices (current bindings select a segment only within their explicit
owner); batch draws without reconnecting independently moving topology; improve
bend appearance without invalidating collision envelopes; add seeded body/equipment
coverage and a controlled 3D performance workload. Shared fitting contracts and the
server-disclosed appearance proposal remain separate unfinished work. Earlier
smooth-union binding experiments remain test-only reference code and historical
evidence; they no longer define production long-hair ownership.


## Equipment-aware sampling checkpoint

The bounded composition planner now includes independent equipment. It retains
all 47 previous fixed witnesses and adds five: a female human duelist with a
breastplate and exposed hair; a nymph princess with a helm; a male mage with a
cloak; a female red mage with boots; and a female troll courtier with all four
slots. The 32 seeded cases independently select equipment and choose only outfits
valid for the selected body, avoiding implicit male fallbacks for female-only cuts.
Total case count is 84, not a Cartesian product. Fixed witnesses survive seed
changes; the random body/outfit/equipment tail changes reproducibly.

The generated-model test applies the actual equipment before construction, checks
helm coverage and native-robe cloak generation, and requires metallic torso
geometry when a breastplate is selected. This last check is not a complete material
or plate-fit proof; dedicated fitting regressions remain necessary. Existing finite
geometry, anatomy preservation, garment-layer and limb-material witnesses remain.
Failures print the full composition, batch seed and index. `TOR_COMPOSITION_CASE`
replays one index and rejects an out-of-range selection. Replay is tied to the
source revision; changing a planner is allowed to change a seed's random cases.

The reusable [equipment-conflict fixture](../../crates/client-3d/review-scripts/equipment-conflicts.json)
and its actual-process acceptance cover four models in one renderer process,
capturing equipped, unequipped and restored states during a jump. Checks include
all requested capture ticks, camera-independent actor snapshots, restored body and
outfit identities, changed/restored resolved keys, no accumulating root rigs,
animated joints and complete PNGs. Visual inspection confirms crown/hair restoration
and the two mage presentations. This front-view fixture is not full surface-clearance
or all-angle visual approval.

Six focused composition tests pass, including generation of all 84 cases. The new
native acceptance passes with the existing release binary (this checkpoint changes
test planning/fixtures, not production Rust). A separate retained replay completed
all 12 captures for visual review. Single-case replay ran exactly one witness, and
index 84 was rejected for being outside the plan. Follow the [review instructions](../../crates/client-3d/review-scripts/README.md)
for reproduction. Repository quick, all 18 documentation checks, the independent
3D verification-tooling tests and whitespace checks pass. The full 3D debug/release
matrix and both-platform CI remain unrun; nothing has been pushed or merged.


## Shared nested-appearance discovery fix

The first-red unit regression demonstrated that `observation_assets` returned the
visible actor and corpse-item assets but omitted self appearance, ground-corpse body
appearance and carried-corpse body appearance. The shared iterator now visits those
already-disclosed nested assets without allocation or protocol changes. Missing
appearance/assets remain absent; this does not infer private equipment or new facts.

All 135 client-common unit tests and 95 integration tests pass. The scripted
connection regression exercises each nested carrier through missing-palette repair,
then verifies that a still-unavailable asset does not trigger repeated requests.
A real server/native ASCII process fixture removes only the player's nested body
asset from the first palette and verifies a repair reply containing it, followed by
a presented frame using that repaired revision. The player has no duplicate visible
actor cell carrying the asset. Native acceptance passes. Its first attempts exposed
and corrected test assumptions about frame diagnostics (revision/stale only, and a
numeric revision versus the wire's string encoding); no runtime workaround was added.

Core debug server, scenario, Text, ASCII and headless binaries have been built.
The dependent 3D release executable rebuild, release Clippy, one model-library
test and 31 model integration tests pass. The broader core checkpoint gate finished with 554 process passes
and three failures: a 600-second physics-example build timeout and two Rogue
route frozen-input failures caused by editing the 3D Python test during the run.
The latter are invalidated evidence, not product failures. All three targeted
reruns now pass on frozen inputs (1,294 seconds total), after the physics example
was built successfully with the existing timeout unchanged. The original gate
remains a failed run; these corrective results cover its three failures without
repeating the 554 unaffected process cases. Future frozen-input runs must freeze
all repository inputs, including the separate 3D workspace and documentation.
The full cross-platform/profile matrix and server-driven composition proposal
remain open.

The Windows Text, ASCII and Text + ASCII Spectator desktop launchers now use a
standalone release bundle outside the isolated checkout. All required release
executables were built and the installed scenario validated. Two Text launches
connected and presented room output, then quit normally; ASCII presented a player
frame and closed normally. The paired launcher presented Text output and an ASCII
spectator frame with no player control. Terminating only its owned Text client
exited the wrapper while retaining the server for the spectator; closing ASCII
then cleaned up the owned server. Each run produced a fresh nonempty save, with
prior saves unchanged. Separate player/spectator credential creation was checked
in the unchanged launcher helpers. The three old shortcuts were backed up before
updating and rereading their targets; the old release and saves were preserved.
This is local desktop qualification, not publishing or full CI qualification.

## Shared renderer event and animation execution

Live playback and deterministic capture share `apply_operation` for actions,
velocity, teleport, clothing and equipment. Identical outfit/loadout requests do
not request a rebuild. Rendering policies still choose preloaded or rebuilt
geometry; neither owns separate event semantics.

Both modes use `AdvanceReview` for animation, transform propagation and the
soft-body step, and `RefreshReview` for explicit pose evaluation and transform
propagation without elapsed time. `PoseRefresh` is an actor request consumed
once. Capture advances the preceding interval before applying boundary events,
then refreshes changed actors. Live playback uses the same refresh path and
handles events at exact frame endpoints before presenting them. Screenshot waits
request neither advancement nor refresh; GPU readiness/readback and real-time
watchdogs remain separate policies.

Two first-red native regressions exposed premature integration: a velocity change
moved capture one tick early, and a new/restarted action was already one step old
at its timestamp. Both now pass. The older replay fixture's position assertion
was corrected to nine intervals between tick 1 and tick 10, rather than ten.
Action starts/restarts now report phase zero, and the following tick advances
exactly one interval. Teleport placement, ordered same-tick velocity changes,
stopping and cancellation retain explicit witnesses.

Nine focused animation/support tests and 14 renderer tests pass, as do release
Clippy and the executable build. The real-model refresh test verifies changed
geometry without aging animation, followed by unchanged joint transforms and
hair/cloth mesh positions across held frames. Four native acceptance tests pass:
action timestamps, movement timestamps, deterministic replay with composition
changes, and persistent-window looping with diagnostics. These results do not
claim identical soft-body poses across differing live/capture integration steps,
a GPU/display frame-drop measurement, or full cross-platform qualification.

Keep iteration checks focused on changed contracts. Use bounded conflict fixtures
and seeded composition samples; reuse unchanged evidence with its source mapping.
Finish implementation and documentation before frozen-input gates. The prior
core gate's three failures passed targeted stable-tree reruns; do not repeat its
554 passing process cases merely because the renderer changed. Publishing still
requires the repository's applicable gate and CI evidence.

Capture-name validation now rejects ASCII case collisions before output creation.
A real-process first-red test reproduced `front`/`FRONT` leaving only one PNG
for two requested captures on Windows. The fix preserves authored spelling and
permits distinct uppercase names. All 10 focused renderer unit tests, release
all-target Clippy/build, and three targeted native checks pass (collision rejection,
existing invalid-input rejection, and valid deterministic replay).

Live parsing now has a separate timeline policy: shared composition/camera/resource
validation remains, but screenshot records no longer bound live events or need to
be present. The playlist still validates events against its actual chapter duration
and a 600-second ceiling. Capture retains its required images and 120-second limit.
The native live-loop first-red failed on the missing `captures` field; its artifact
retention also encountered a Windows rename error, recorded separately from the
product assertion. All 16 renderer unit tests pass, including a 150-second live
event without screenshot requests, unchanged capture limits, and rejection above
the live ceiling. Release all-target Clippy/build and five targeted native checks
pass: capture-free live looping, invalid live playlists including an event exactly
at the chapter end, capture-name collision rejection, general invalid capture input,
and valid deterministic replay. The process checks completed in 29.43 seconds.

## Resolved equipment boundary

`outfit::Composition` carries the resolved outfit and the typed independent body
item together. `Outfit::worn` delegates to this resolver, while biped generation
consumes its result and exhaustively matches the body recipe. Previously geometry
read raw equipment presence separately and treated any body item as a steel
breastplate. Adding another body-item variant must now explicitly choose its
geometry, rather than silently generating the existing armor.

This extraction preserves preset restoration and existing wearer-specific shell
fitting. It is a renderer-local seam, not a generalized inventory, body capability
registry or wire format. Helm, footwear and cloak still resolve into outfit
fields; the independent body layer still uses the existing fitted surface chain.
A future component plan should carry recipe ownership and coverage consistently
for all of these regions, with named body-relative attachment frames and contact
surfaces supplied by the wearer. Do not serialize this intermediate Rust type as
the proposed cross-client appearance contract.

The focused contract witness pairs the selected breastplate recipe with suppression
of its covered torso ornament, retains the underlying bodice/skirt, and restores
the exact original composition on removal. Existing native equipment conflict
coverage exercises generated geometry and restoration through the script runner.
All six focused equipment composition tests and the 18 documentation checks pass.
The release executable rebuild passes, as does the existing native equipment
conflict/restoration fixture (21 seconds). No rendering policy or fitting shape was
changed by this extraction. Release all-target Clippy passes after replacing the
outer optional-item match with `Option::map` around the same exhaustive recipe
match. The unit/native results above precede that semantics-preserving lint
cleanup; full CI qualification remains outstanding.

## Attachment migration map

The next fitting extraction should adapt existing anatomy and contact results,
not build a second table of approximate coordinates. These are the present owners:

| Existing source | Meaning to preserve | Migration boundary |
| --- | --- | --- |
| `HumanoidBody::shoulder`, `leg_root`, `elbow`, `knee`, `ankle` in [anatomy](../../crates/client-3d/src/creatures/anatomy.rs) | Offsets in the relevant parent joint, not interchangeable world positions | A resolved attachment includes its parent frame as well as local placement |
| `Bust::apex` and torso shell construction | Anatomical contour landmark; clothing clearance is a separate fitted surface | Keep anatomy landmarks distinct from the exterior of clothing or armor |
| `HumanoidBody::feet` and `BootLast` | Ankle-local foot primitives and footwear dimensions | Reuse the wearer shape; do not substitute a humanoid foot for an unsupported creature |
| `Waist::attachment` | Sashes/belts follow torso; low jewels/coins follow hips | Authored attachment ownership survives refitting and animation |
| [Visual contact](../../crates/client-3d/src/creatures/visual_contact.rs) | Composed mesh profiles transformed along a joint path | Contact queries use the resolved exterior, including worn layers |
| [Hair contact](../../crates/client-3d/src/creatures/hair_contact.rs) | Explicit mesh/head-or-drape/torso/support transforms and authored pin weights | Preserve ownership and reset per-instance history when presentation changes |

For an initial renderer-local attachment contract, return a frame identity, local
position/orientation, and a capability result. Missing capabilities should remain
explicit; do not invent a fallback origin or silently attach to the nearest joint.
A later repeated-limb role maps to a specific occurrence in the body recipe, rather
than assuming every body has left/right human hands. Keep these renderer frame
identities out of the observer-facing wire description.

Migrate one real consumer first, such as waist ornaments, and compare its generated
placement and animation with the current recipe. Only expand the interface after
that consumer works. Surface fitting remains a separate operation: choose the
underlying resolved layer, transform into its frame, apply clearance, and retain
the intended support side. An anchor alone cannot establish bust coverage or stop
hair, cloth, and armor intersecting between anchors.

Keep validation bounded during that migration: one torso-owned and one hips-owned
waist fixture, an unsupported-body capability case, a nonidentity parent transform,
and equip/remove restoration. Reuse the existing native conflict fixture for the
process boundary. Add a repeated-limb witness when such a body actually consumes
the contract. These are migration recommendations, not an implemented landmark
registry or authorization for a protocol format change.

## Contact collection allocation cleanup

`visual_contact::Part::posed` now yields transformed cylinders through an iterator.
The scene collector no longer constructs and discards a temporary vector per part
on every animated pose. It still owns one combined vector. The inverse joint-chain
transform and scale conversion are computed once per contributor, outside its
cylinder loop. This is a structural allocation reduction, not a measured frame-time
or throughput claim; compiler optimization may already have removed some repeated
transform arithmetic in the previous implementation.

The complete joint chain is resolved before any cylinder is yielded. A missing
joint omits that contributor while preserving other parts; it must not emit contacts
in a partially composed frame. Focused coverage checks that rule and deterministic
collection, alongside the existing rotated/translated sparse-face coverage witness.
Both focused contact tests, all 18 documentation checks, release all-target lint,
and the executable rebuild pass. The existing native equipment/contact fixture
also passes on this build, covering the composition lint cleanup as well. Controlled
performance measurement has not been repeated for this cleanup.

## Initial garment fitting inventory

These are extraction candidates and existing policy owners, not a complete catalog
of every garment constant. Distinguish authored cut, anatomical measurements,
coordinate conversion and contact clearance before moving values into a shared API.

| Owner | Current inputs and policy | Invariant to preserve |
| --- | --- | --- |
| [Panel span](../../crates/client-3d/src/creatures/geometry/panel_parts.rs) | Each side uses its own bust apex and mass radius; authored torso-height interval and waist/shoulder convergence control the cut | Coverage and edge binding share the same span; asymmetric sides never borrow the opposite side's width |
| [Neckline edge](../../crates/client-3d/src/creatures/geometry/neckline_parts.rs) | Continuous radial surface solve, bounded edge-height solve, surface-normal clearance | Front/back continuity and the intended exposed neckline survive changes in bust shape |
| [Waist pieces](../../crates/client-3d/src/creatures/geometry/waist_parts.rs) | Authored hip-frame height converts to torso support height; signed-distance clearance rather than a fixed radial offset | Sloping hips retain thickness without shelves; mounting frame agrees with animation ownership |
| [Robe belt](../../crates/client-3d/src/creatures/robe.rs) | Axis measurements plus diagonal clearance checks against the selected body/outer shell | A convenient elliptical band must still enclose asymmetric hip sections |
| [Independent armor](../../crates/client-3d/src/creatures/geometry/armor_parts.rs) | Wearer anatomy, underlying garment/plate union, ornament relief allowance, female opening cutter | Armor encloses its retained underlayers and shares its opening with trim and coverage checks |
| [Cloak recipe](../../crates/client-3d/src/creatures/cloak.rs) | Body shoulders/hips, actual equipped shell, rear accessory depth and lining thickness | Both cloth faces clear the resolved exterior; motion uses instance-owned geometry |

Do not collapse these into a single radius or offset parameter. In particular,
cloth thickness, clearance, underlayer relief and an authored neckline depth have
different meanings even when all are represented by `f32`. Start by making the
frame and support-surface dependency explicit at a real consumer, then move common
queries only when they preserve the existing surface and coverage witnesses.

### Cloak cut ownership

`Outfit::cloak` now carries a typed recipe with exterior color, lining color and
length. Native, preset and independently equipped cloaks use the same description.
The native vampire recipe owns its longer cut; geometry no longer infers cloak
length from the wearer species or whether an outfit was explicitly supplied.
Consequently, transferring the native recipe preserves its cut, and removing an
equipped travel cloak restores the underlying recipe. Shoulder/hip fitting and
armor/back-layer clearance remain owned by `cloak::Recipe`.

The emitted-mesh regression first failed when an otherwise identical vampire look
received its native outfit explicitly: that path shortened the cloak. It compares
all cloak vertices before/after recipe transfer and equipment removal. Existing
connected-sheet and ghost-material witnesses cover the adjacent construction
contracts. This is one concrete extraction, not a general creature capability
system. Exact requests now use `Look::composed`, which checks body/outfit support
before applying the outfit and equipment together; the script parser only adds
actor context to its diagnostic. It rejects unsupported anatomy and female-only
recipes on male bodies. Native-with-no-equipment remains valid for other creatures.
The older `Look::dressed` and `Look::equipped` methods retain the interactive
wardrobe's fallback behavior; new exact callers should use the checked entry point.
These checks still describe the current humanoid support boundary, not a generic
per-item capability resolver or server gameplay permission.

`Wardrobe::recipe_for` owns the body-specific preview mapping in one place:
Princess and Royal Princess use Courtier on male bodies, while Sylvan Duelist uses
Ranger. Recipe generation applies that mapping; exact admission rejects a request
if the mapping would substitute another recipe, and seeded sampling excludes those
substitutions. Individual recipe builders no longer choose their own fallback.
The catalog witness checks all 31 entries against the explicit expected mapping,
recipe equality and exact admission, without constructing a geometry matrix. The
existing sampling witnesses retain independent female-only assertions and the same
84-case budget. This prevents a new recipe from accidentally drifting between
preview, script validation and test selection, while preserving existing behavior.


### Neck-drape ownership

`Outfit::neck_drape` now owns a cowl/scarf's pigment, hanging-tail choice and whether
an equipped helm covers it. Native recipes preserve the existing Hero, Mage,
Scout, Orc and Phantom choices. Equipment resolution suppresses hood-base cowls
when replacing their headwear with a helm, while ordinary scarves remain. Geometry
fits the resolved drape to the wearer and no longer selects it from species or
from whether the outfit was supplied explicitly.

The first-red Orc witness found 412 marked scarf-tail faces in its native form and
zero after assigning an explicit copy of that outfit. The regression checks that
the tail survives this transfer and also appears on a Human body. Separate fitting
witnesses retain structural body/shell clearance checks. Their baselines now remove
neck drapes explicitly rather than depending on the former accidental omission.
Helm composition tests preserve ordinary cowls/scarves and restore the original
hood-base cowl after removing a helm. The existing animated equipment fixture now
includes a helm with the Mage's cloak, covering hood replacement and restoration
within the same scene and capture count. This does not add a separately selectable
scarf item or new wire fields; it makes an existing garment part transferable.

### Emitted-mesh visibility fixtures

The Medusa sewn-edge witness isolates trim by changing only its pigment, asserting
identical triangle count, vertex positions and material assignment, then excluding
color-changing faces from the structural support mesh. It retains the real open
neckline. Closing that neckline is not a valid suppression fixture: the coverage
partition splits body triangles at the skin/cloth boundary, so closing it changes
the triangulation even when the anatomical surface is unchanged. This caused the
broader release checkpoint's structural-baseline guard to fail before it could
measure clearance.

The corrected witness still checks actual emitted triangle interiors for positive
clearance, bilateral support, winding and overlapping strips across three builds.
It does not reuse the fitting solver as its clearance oracle. This is a test-only
correction; runtime appearance is unchanged. Keep such fixed problem witnesses
alongside seeded composition sampling, rather than expanding all body/outfit/item
combinations. Broader release library coverage is not native or cross-platform
qualification, and a failed aggregate run must remain recorded as failed even
when its affected tests subsequently pass.

## Shared mesh-frame conversion

The first waist-frame cleanup reuses the existing bird-foot reparenting helper,
now named `reparent_mesh_transform`, for torso-owned waist pieces. It converts
parent-authored mesh coordinates into the child joint's mesh coordinates while
accounting for both joints' baked proportions. The pivot is expressed in the
parent joint frame, independently of those mesh scales. The existing waist result
is preserved because torso and hips currently share their proportion scale.

This avoids introducing a parallel landmark/transform framework merely to name
an existing translation. The same helper now serves an attachment and a terminal
body part. It is deliberately limited to rest-frame conversion: runtime joint
rotation, authored attachment ownership, surface clearance and capability checking
remain separate concerns. Native caster waist pieces retain their hip/robe owner.
A new focused witness checks equal and unequal mesh proportions under a rotated,
translated parent and passes. The existing waist-owner test, release all-target
Clippy, executable rebuild and native equipment conflict/restoration fixture also
pass. Documentation/link checks pass; full CI qualification remains outstanding.

## Bounded additional hair witness

The existing full-mesh jump-contact assertions are now shared by the original
asymmetric Human/Sylvan Duelist/breastplate case and one additional fixed case:
Nymph, seed 7, female, build 2, bust 4, royal princess gown, travel cloak. This
adds a different body/garment silhouette without multiplying the wardrobe catalog.
Both cases retain the same 30-frame scalp pinning, edge strain, face/vertex contact
and minimum front-witness requirements. The per-frame displacement diagnostics
remain diagnostics, not a separate temporal-continuity assertion. The focused
run passes both cases in 8.83 seconds with unchanged thresholds. This expands
contact evidence to one additional fixed composition, not the whole catalog.
Only test code changed; the preceding native-renderer evidence is retained rather
than rerunning unaffected process coverage.

## Recipe-owned coat openings

`Outer::Shell` now carries a `ShellOpening`: closed, shaped neckline, plunging
coat, or open coat. These describe the garment's authored cut. The receiving
`HumanoidBody` supplies its dimensions; the existing neckline surface cutter
fits that cut to the body. `outfit_shell` no longer accepts `Look`, so it cannot
choose an opening from the wearer's species/style or from whether the outfit
was selected explicitly. Native and preset constructors preserve their previous
cuts and dimensions.

The first-red regression copied the native female Vampire recipe into an
explicit outfit and found 13,056 shell vertices instead of the native 13,654.
The permanent witness compares actual emitted shell positions and topology for
Vampire, Wraith and Leprechaun before/after explicit transfer. It passes after
the fix, as do the nine equipment-composition tests. Release all-target Clippy
passes after correcting a misplaced existing enum-size annotation.

This is one ownership correction, not complete native-outfit portability.
The caller still chooses `trim_neck` from native/style state. Native caster
robe/waist ownership, cuffs and chest ornaments were subsequently migrated as
described below.
Those need separate behavior witnesses before being moved into recipes. The
shell witness isolates the opening with a fixed neck-trim policy and must not
be cited as proof that an entire transferred model is identical. No protocol,
save format or public appearance schema changed.

The release executable rebuild and expanded existing female-lineup acceptance
check pass: Warrior, Vampire, Orc, Leprechaun, Nymph and Wraith complete all 18
GPU captures and individual bust assertions in one native process. This checks
rendering and completion, not pixel equivalence. The affected garment regression
module also passes. Documentation checks (18) and whitespace validation pass.
The prior broad release checkpoint covers its recorded earlier state; this
focused follow-up is not a new full gate or cross-platform qualification.
Native mouse automation remains omitted at the maintainer's request.

## Shared sewn chest details

The remaining five native chest-detail branches now resolve into the existing
`TorsoOrnament` recipe: laced front, robe placket, riveted vest, stitched front,
and high collar with buttons. Their pigment values belong to the recipe,
including the collar lining, shoulder pads, cravat and buttons. The receiving
body still determines dimensions and whether the cravat is appropriate; female
wearers keep the open neckline. Plackets receive the resolved neckline so they
end beneath its point rather than covering the plunge.

The shared emitter now lives in
[ornament_parts.rs](../../crates/client-3d/src/creatures/geometry/ornament_parts.rs),
not in the Rogue species module. The existing leaf primitive moved with it and
is reused by crowns and plant models without changing its geometry. Existing
ornaments use the same emitter. Geometry no longer branches on native selection
or wearer style for these sewn chest details. The existing equipment resolver
suppresses `torso_ornament` beneath a breastplate, leaving the base recipe intact;
removing the item restores it. This preserves the existing whole-ornament
coverage behavior, including the high collar; separate collar-versus-chest
coverage remains a future refinement rather than a new implicit layering rule.

The first-red mesh regression observed zero copied Vampire button faces versus
1,536 native faces. It now passes for both sexes, copying the native recipe,
transferring it to a Human, covering it with a breastplate, and restoring it on
removal. A bounded recipe test covers the five new variants and verifies that
coverage changes only the ornament field. Ten equipment tests pass. An initial
extraction compile failure identified the leaf helper dependency; moving the
unchanged shared helper resolved it. The later continuous-robe checkpoint below
migrates robe/waist animation ownership and cuffs; neither checkpoint asserts that
all native outfits are completely interchangeable. No wire/save format changed.

The completed focused checkpoint passes 14 garment tests, ten equipment tests,
release all-target Clippy and the native executable rebuild. Two native process
checks pass: the expanded female lineup (nine families, 27 captures) and the
animated equipment-conflict/restoration script. These exercise the shared emitter
and existing female fitting, not arbitrary native-outfit transfer through the
script schema. The exact transfer regression remains the emitted-mesh witness.
Documentation checks (18) and whitespace validation pass. No mouse automation
was run; full final-commit platform/profile qualification remains outstanding.

## Recipe-owned continuous robes

`Outfit::robe` now optionally holds `FittedRobe`: cloth pigment, trim pigment and
an authored fold seed. Native Mage and RedMage recipes select it; replacement
presets leave it absent. The component owns the existing continuous lower robe,
surface-following sash tails, knot and bell-cuff trim. Its waist details retain
the hip frame used by the robe animation instead of silently moving to the torso
when a native recipe is copied.

The [robe generator](../../crates/client-3d/src/creatures/robe.rs) receives the
recipe rather than `Look`. The receiving body's resolved anatomy, feet and final
leg pose still supply fitting/contact geometry. Mesh ownership, contact iteration
rules, topology and shared material remain unchanged. The neutral geometry and
vertex palette are identical for a native mage and an explicitly copied recipe;
fold variation no longer changes to match the recipient's cosmetic seed.

The first-red regression failed because the explicit copy had no robe rig. The
new permanent witness now passes that equality check and transfers the garment
to a female Orc with a different build/seed. It verifies retained robe pigments,
actual leg-driven deformation, finite positions and unchanged topology. This is
a motion/ownership witness, not a full clearance proof for every recipient body.
The existing robe envelope and owned-mesh tests remain the clearance/isolation
checks; additional non-mage conflicts should add bounded emitted-mesh witnesses.

This is one authored continuous-robe component, not a generic cloth solver or a
new public wardrobe entry. Its lower cut and sash layout retain existing authored
constants, and bell cuffs remain part of this component rather than independently
swappable recipes. There is no new wire/save format or arbitrary native-recipe
selection in review-script JSON. The local equipment resolver retains the robe
when overlaying items; replacing the whole outfit removes it as before.

The focused release checkpoint passes all four robe contract tests, the existing
Mage/RedMage wardrobe replacement/restoration test, ten equipment tests, strict
all-target Clippy and the executable rebuild. Both native checks pass: live mage
redressing and the animated equipment-conflict/restoration script. Runtime
animation dispatch already updates any attached robe rig from the final pose;
there is no remaining caster-style gate in that dispatch. These native scripts
do not expose arbitrary copied native recipes, so the cross-body claim remains
bounded to the direct emitted-mesh regression. Documentation checks (18) and
whitespace validation pass; no mouse automation or new full platform/profile gate
was run for this focused change.

## Direct recipe cache boundary

The initial audit found that recipe transfer tests assigned `Look::outfit`
directly and constructed each look through `rogue_tests::spawn_one`, which
allocates a fresh `PropLibrary`. Catalog entry points `Look::dressed` and
`Look::equipped` refresh their key; direct assignment did not. `Kit::key` copied
that catalog key, so distinct raw recipes on the same body could share the same
`creature/{key}/{part}` lookup. Dynamic robe ownership did not protect static
cuffs, knots or torso parts from this collision.

The new first-red test reproduced it in one shared library: two differently
colored recipes returned the same three torso mesh handles. At the rendering
boundary, `Kit::key` now includes the complete explicit `Outfit` value alongside
the existing look key; native looks retain their existing key. The full local
value is compared as part of the string key, not reduced to a truncated digest.
There is no second identity table, global cache flush or change to cache eviction.
The head key retains its narrower headwear/cuirass/earring dependencies, and
immutable hair retains its body key.

The permanent shared-library witness now passes distinct torso handles and
pigments, unchanged head/hair source reuse, repeated-recipe reuse and restoration
of the original native geometry. This closes the observed body-part alias for
current authored recipes. Earlier fresh-library opening, ornament and robe
witnesses retain their geometry/motion scope; they do not replace this cache test.

These are debug-formatted renderer-local values, not persistent content IDs or
network identifiers. They cost more key storage for explicit compositions;
existing idle model-count and mesh-payload bounds still govern eviction, but the
mesh-payload diagnostic is not a total-memory measurement. No speedup or total
memory improvement is claimed. A future compact semantic representation must
retain all geometry dependencies and equality checks rather than assume a digest
cannot collide.

Arbitrary recipe assignment is still internal: there is no new script selector,
public validation API, generic capability resolver or changed wire format. Before
exposing it, validate body compatibility and fitting inputs and preserve the
shared-cache witness. Adding recipe identity does not establish that every
combination fits, that non-finite arbitrary inputs are supported, or that all
native/style dependencies have been removed.

The focused release checkpoint passes the new shared-cache regression and the
existing catalog head/hair reuse test, all-target Clippy, and rebuilt native and
benchmark executables. The real benchmark-process contract passes construction,
cache reuse and motion/mesh-count validation; it is not a controlled speedup
measurement. The native animated equipment-conflict/restoration fixture also
passes. Documentation checks (18) and whitespace validation pass. Native mouse
automation and a new full platform/profile gate were not run for this change.

## Shared contact query frames

Posed mesh profiles usually contribute several cylinders in one joint frame.
`Scene` now records consecutive ranges sharing the same query transform, and
transforms each sample once per range. Each cylinder retains its own center,
shape, distance scale and conservative bound. Cylinder order is unchanged, as is
the reduction against the static body field. Changed poses rebuild the ranges;
unchanged poses still report no contact invalidation, and the range vector reuses
its allocation. No fitting iterations, mesh resolution or clearance thresholds
were reduced.

The new reference test compares signed distances bit-for-bit with exhaustive
per-cylinder evaluation on a fixed grid, including nonuniform transformed frames,
different per-cylinder scales, reordered contacts and an empty contact set. Its
six-cylinder fixture uses three query transforms. All five contact tests pass.
This verifies query semantics and the repeated-work reduction in that fixture;
it does not establish a renderer-wide speedup or animation continuity.

The focused release checkpoint passes both emitted-mesh jump-contact witnesses,
all-target Clippy, rebuilt native/benchmark executables and the native animated
equipment fixture. One validated model workload version 3 run followed after tests and builds
finished; the runner rejected competing builds and preserved separate raw output,
source/binary hashes and machine metadata. On fingerprint `6a1878811f37`:

| Case | n (frames) | Full CPU update p50 / p95 / max ms |
| --- | --- | --- |
| Human, Sylvan Duelist, breastplate | 90 | 9.630 / 9.695 / 9.980 |
| Nymph, Royal Princess, cloak | 90 | 76.651 / 85.810 / 97.637 |
| Mage, native, boots | 90 | 2.404 / 3.475 / 3.580 |

All nine producer records passed construction, reuse, motion and count validation;
there were no failed runs. Nymph hair fitting alone was 76.469 / 85.615 / 97.440 ms.
This is a current-state diagnostic, not a controlled comparison with earlier
checkpoints or a GPU/presented-frame measurement. The 50-Hz long-hair workload
still exceeds its 20-ms CPU step budget substantially. More attribution inside
the contact/constraint solver remains higher priority than claiming a performance
closeout. Documentation checks (18) and whitespace validation pass; no mouse
automation or new full platform/profile gate was run for this scoped refactor.

## Nested hair timing

The optional latest-step `MotionProfile` now contains four nested hair durations:
control-curve fitting and deformer preparation; mesh deformation and rest-edge
preparation; face-contact/edge-stretch constraints; and mesh writes with normal
reconstruction. The intervals are disjoint. Attachment-history bookkeeping and
contact collection remain inside the enclosing hair phase without being assigned
to one of these subphases. No solver arithmetic, iteration count or threshold
changed, and ordinary placement does not read clocks or retain timing history.

The model benchmark emits these arrays as diagnostic workload version 4. The
validator requires 30 finite nonnegative values per stage, sums bounded by the
enclosing hair duration, and nonzero stage totals in the known active-hair Nymph
fixture. Top-level phases are checked against full update time without adding
the nested hair values twice. Earlier version 3 evidence remains historical;
a controlled comparison requires matching producers and validators on both
revisions. This changes the developer diagnostic schema, not game protocol,
save data or review-script format.

The first-red contract test rejected the new stage fields. After updating the
producer/validator together, all six validator tests and 31 performance-harness
tests pass, including missing/invalid durations, over-counted nested time and
missing active-stage timing. The existing held-frame profile-reset witness now
starts with nonzero nested timings too, so a paused frame must clear them.

The completed focused checkpoint passes the held-frame reset/optional-profile
witness, both emitted-mesh jump witnesses, release all-target Clippy, rebuilt
native/benchmark executables, the actual producer-process contract and the native
animated equipment scene. A separate isolated version 4 diagnostic on machine
`6a1878811f37` produced nine validated records with no failed runs. Full CPU update
p50/p95/max values for 90 frames per case were Human 9.623/9.705/9.887 ms,
Nymph 75.841/84.038/84.696 ms, and Mage 2.476/2.604/3.252 ms.

For the active-hair Nymph, the stage attribution was:

| Hair stage | n | p50 / p95 / max ms | Share of pooled hair time |
| --- | --- | --- | --- |
| Controls and deformer preparation | 90 | 46.878 / 51.015 / 51.386 | 60.88% |
| Mesh deformation/rest edges | 90 | 0.499 / 0.539 / 0.561 | 0.65% |
| Face/edge constraints | 90 | 28.398 / 32.620 / 33.288 | 38.24% |
| Mesh writes/normals | 90 | 0.167 / 0.181 / 0.205 | 0.22% |

Shares use summed durations over all samples, not ratios of medians. The enclosing
hair phase was 75.676/83.878/84.523 ms; minor bookkeeping lies outside the nested
intervals. This changes the investigation priority: inspect control fitting and
deformer preparation first, then face/edge relaxation. Rewriting normal generation
or moving it to the GPU would address very little of this measured CPU cost.
The broad control stage still needs source-level attribution before choosing an
algorithm change; do not assume all of it is contact-query time. Retain attachment,
strain, full-mesh clearance and temporal-history requirements when changing either
solver. This single-fixture diagnostic is not a controlled speedup comparison or
proof of a renderer-wide distribution. Documentation/whitespace checks pass; no
mouse automation or new full platform/profile gate was run for this checkpoint.


## Exact control-solver fixed points

The control solver previously ran all 48 passes even when a complete pass left
all coordinates unchanged. It now retains a scratch copy of the controls and
stops after an exactly unchanged pass, comparing floating-point bits including
signed zero. This is a fixed point: with an unchanged distance field, repeating
the same operations cannot produce a different result. Callers must supply a
stable field for the duration of a solve. The remaining iteration limit, swept
contact, segment sampling, rest lengths and recovery rate are unchanged.

This deliberately avoids a tolerance-based convergence rule, which could stop
while clearance or strain is still improving. It adds one temporary control-sized
buffer per solve; further allocation changes should be driven by profiling.
Stationary points still receive a complete pass, including overlap correction.
The new settled-chain query-count witness failed first at 384 queries; its paired
overlap witness checks that a stationary embedded control is pushed clear.
All 18 hair-contact tests pass, including swept movement, fixed sockets,
rest-length recovery, and both full emitted-mesh jump witnesses. Release all-target
Clippy, rebuilt renderer/benchmark executables and the actual native equipment
scene pass. That process witness checks rendered captures, worn-item selections
and wardrobe changes; it is not a quantitative clearance or timing assertion.
The emitted-mesh witnesses own clearance, while the new query-count test owns
redundant-work termination. Documentation and whitespace checks pass.

An isolated diagnostic on machine `6a1878811f37` produced nine valid records,
90 frames per case, with no failed runs. Full CPU update p50/p95/max was
Human 9.623/9.701/10.458 ms, Nymph 43.526/58.041/58.275 ms, and
Mage 2.467/2.575/2.745 ms. Nymph control fitting was
15.142/25.194/25.416 ms. This is current-state evidence, not a controlled
before/after result or an FPS claim. The long-hair case still exceeds the
20 ms update budget; exact fixed-point termination does not close performance
or temporal-continuity requirements. No new full gate or final platform/profile
CI was run, and the three native mouse tests remain explicitly waived locally.


## Recipe-owned Wraith cloak

The native Wraith cloak previously bypassed `Outfit::cloak`: the native-only
costume helper emitted it directly. This broke two composition boundaries:
explicit recipe copies lost the cloak, and equipping a travel cloak emitted two
cloaks instead of replacing the native one. A first-red mesh/rig test observed
two cloaks for the equipped Wraith.

`Cloak` now carries its ragged hem flag alongside length, exterior and lining.
The Wraith native recipe owns the original 1.2 length and pigments; ordinary
cloak constructors default to an intact hem. The shared resolved-cloak path
emits every cloak, so back-item replacement/restoration, breastplate clearance
and rear-strap support apply consistently. The duplicate Wraith generator is
removed. This remains a renderer-local garment recipe, not server equipment
permission or a wire-format change.

The regression covers native, equipped, restored, explicit-copy and cross-body
states. Transfer retains pigments and animated deformation on a different body.
The native equipment process fixture adds Wraith to its existing Warrior/Mage
cases; avoid expanding this into every species/outfit/item permutation. The [attached-detail extraction](#portable-attached-garment-details) also moves
loincloths, leaf skirts, tatters and shoulder discs into recipes; remaining
native-only geometry is listed there.
Focused qualification passes four cloak tests, five equipment tests, all-target
release Clippy, rebuilt renderer/benchmark executables, and the expanded actual
native equipment fixture (three families, two wardrobe modes, three camera views
per family). Documentation and whitespace checks pass. The first-red duplicate
cloak failure remains recorded locally. These checks do not constitute a new
full gate, final platform/profile CI, or proof that all native accessories now
transfer; the three local native mouse tests remain waived.


## Portable attached garment details

`GarmentDetails` separates hip decorations, shoulder discs and sleeve tatters.
`HipDecoration` describes a loincloth, ragged apron, buckle/pouch, leaf layers,
or torn hem with authored pigments and cut parameters. Native recipe selection
chooses these pieces; `geometry/garment_details.rs` fits and attaches them using
resolved body dimensions and hip/arm/forearm joints, without species or native
outfit checks. The former `rogue::biped_extras` path is removed. The existing
strip/tatter helper moved with its consumers; geometry constants, seeds and drape
motion coefficients are retained.

The first-red test found the Orc recipe copy missing its loincloth. Fixed copy
witnesses cover Orc, Troll, Leprechaun, Nymph, Medusa, Phantom and Wraith, comparing
attached mesh topology and placement. Cross-body cases retain attachments on a
differently built Human. A mixed recipe combines Nymph hip leaves, Medusa discs
and Phantom sleeve tatters, then removes each group independently. The existing
native women-lineup fixture adds Troll, Medusa and Phantom to cover the newly
moved generators without a Cartesian composition matrix.

These pieces are still renderer-local authored groups, not a universal equipment
slot system. Shoulder discs and sleeve tatters retain existing coverage behavior;
future arm armor needs an explicit coverage decision. Zombie hip rags and the
native neck-trimming decision remain in the main biped generator. Arbitrary
recipe admission, all-body clearance and visible-equipment disclosure remain
separate unfinished boundaries. The later [pigment migration](#wearer-owned-pigment-migration) fixes the common
bare-hand/leg/foot roles. Remaining native boot/anatomy pigment choices still need
an explicit audit before unrestricted transfer. The accessory witnesses above
prove attachment retention and finite geometry, not every material role.

Focused qualification passes the copy/transfer/mixing witness, the existing
84-case fixed-plus-seeded composition batch, five equipment tests, all-target
release Clippy, rebuilt renderer/benchmark executables and the actual 12-family
native lineup (36 required captures). The first native run rendered every family
but failed its filename expectation because captures use catalog order, not
filter-list order. The corrected expectation passed without removing captures or
assertions. Extraction-time compilation failures were corrected before this
qualification; local failed logs remain available. Final documentation/whitespace
checks pass. No new full gate, final Windows/Linux profile matrix or native mouse
automation was run for this focused checkpoint.


## Wearer-owned pigment migration

Copied recipes previously baked the donor's skin into hands, fitted/bare legs
and bare feet. A first-red actual-mesh witness transferred a green-skinned Nymph
recipe to a Human with a different skin pigment; the hand contained zero vertices
with the recipient pigment.

`Pigment::Fixed(Vec3)` now represents authored color and `Pigment::Skin(shade)`
represents a wearer-owned skin reference with a multiplicative shade. Outfit
hands, fitted/bare leg colors and bare feet retain that meaning until the
geometry consumer resolves them against the recipient. `pants`, `seat` and
`boot` explicitly take the wearer's skin. Coverage and garment shape variants
are unchanged, including the distinction between fitted legs and a high-cut bare
leg opening. Existing bare-arm queries already used the wearer's skin and remain
unchanged. Recipe/debug cache keys include the pigment variant, so fixed color
and a skin reference are not conflated even when their current RGB values match.

Native/base/preset constructors preserve original-wearer colors. Troll bare feet
retain their 0.88 shade. Fixed stockings, gloves, trousers and armor retain their
authored pigments; no RGB equality heuristic or donor-color replacement is used.
The mesh witness checks recipient-colored hands, legs and feet, fixed trousers
whose pigment deliberately equals the donor's skin, and boot equip/remove
restoration. A recipe-level witness covers original-wearer equivalence, shaded
feet and explicitly fixed pigments in each migrated slot.

This does not finish arbitrary native transfer. Native boot representations for
Zombie, Guardian and Yeti still carry resolved pigments; the latter two are not
ordinary dressable bodies. Audit those representations and intrinsic anatomy
before broadening admission. Arbitrary user-supplied pigment values also require
validation before a public recipe API. This is renderer-local material meaning,
not an approved content/wire representation; server appearance must still use
observer-filtered semantic catalog data.

At handoff, the corrected nested-mesh regression, six equipment tests and the
shared-cache recipe-identity witness passed. The initial mesh helper only visited
direct children and missed the nested hand meshes; that was corrected, its
nonempty sample asserted, and the old fixed-donor-color behavior rerun to establish
the intended failure before restoring skin bindings. These focused results do
not replace the earlier full Rust-suite checkpoint or final platform/profile CI.
