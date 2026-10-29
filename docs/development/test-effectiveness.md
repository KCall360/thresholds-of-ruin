# Test-effectiveness ledger

Keep a small, evidence-backed record of what tests actually catch and what
maintenance they cost. This is not a pass-count leaderboard, a new merge gate or
a requirement to audit every run. Update it at a coherent fix checkpoint when a
useful finding is established. The [testing policy](testing.md) owns coverage,
execution cadence and qualification.

## Counting and classification

- **Production defect:** implementation violates the supported contract; retain
  the intended failing assertion, correction and green regression evidence.
  Say whether it was caught during development or in released/published code.
- **Documentation defect:** a correct test catches wrong documentation. Do not
  call this an outdated test merely because the repair changes prose.
- **Stale/unupdated test:** an approved contract changed but a fixture, mock,
  driver or expected value did not. Establish the intended contract before
  changing the test; never assume a red assertion is stale because code changed.
- **Harness/infrastructure:** setup, build, driver, environment or evidence
  collection prevented/invalidated the intended check. This is not proof of a
  later gameplay failure. A method may have both stale-fixture and harness
  failures; flag that overlap instead of forcing disjoint totals.
- **Unresolved:** insufficient evidence to decide. Keep it visible but outside
  confirmed defect/stale counts until diagnosed. Deliberate new-feature first-red
  work and mutation sensitivity are separate from naturally detected defects.

Count **distinct root causes**, **distinct failing test methods** (fully qualified
owner plus method), and **failed method executions** separately. One root can
break many methods; rerunning a method or running it on another OS/profile adds
executions, not bugs or distinct methods. Multiple assertions/subtests within a
method do not multiply its method count. A suite/job failure is not another test
failure. Record errors, cancellations, waivers and not-run checks separately.
Define the evidence window and deduplicate before adding totals; never extrapolate
a repository-lifetime count from a partial set of logs.

## Projected limb surface investigation, 28 October 2026

Two existing source-contract defects were confirmed while developing the new
sampler. Its deliberate density first-reds and intermediate refinement failures
are development evidence, excluded from these defect counts.

- **Incomplete limb bounds:** the non-contour bounds omitted the belly offset,
  its deeper Z radius and smooth-union growth. The dedicated
  `creatures::geometry::surface_tests::offset_belly_bounds_include_primitive_extents_and_requested_padding`
  failed its independent extent assertion before the fix: one root, one distinct
  method and one failed execution in this dedicated first-red window. The
  ordinary fine-grid closure control passed; an existing published mesh hole is
  therefore not claimed. The coarse-grid matrix exposed the clipping. Bounds
  now include capsule endpoints, the offset ellipsoid and conservative blend
  growth, then padding. Both bounds controls and the complete limb matrix pass.
- **Incomplete sleeve fitting field:**
  `creatures::geometry::surface_tests::projected_arm_recipes_follow_their_shared_source_fields`
  initially failed against the emitted sleeve: one root, one distinct method
  and one initial failed execution. Generation appended underside mass after
  resolving the field used by fitting. That same addition now lives in the
  shared form, preserving its emission order. All 96 arm checks pass, including
  independent physical error and radial silhouette witnesses.

The combined library passed 459 methods with no failures; its one ignored
subprocess fixture is exercised by the passing lifecycle owner. Four native
composition methods passed with a freshly built client. The fixed-cast geometry
reduction does not establish release timing or the requested 60 FPS crowd target.

A subsequent controlled release comparison caught a **development regression**
in the unpublished projected-surface candidate, outside the fixed Rogue cast.
The Mage benchmark's cold mesh payload rose from 22,360,392 bytes on main to
27,734,784 bytes; the earlier tube-only candidate had used 19,770,624 bytes.
Exact-appearance profiling found both folded forearms falling back to a quarter
grid spacing. The dedicated
`creatures::geometry::surface_tests::resolved_mage_forearm_keeps_its_shape_without_excessive_triangles`
first passed independent physical error and radial silhouette controls, then
failed at 59,044 triangles against a 12,000 ceiling. This is one candidate-cost
root, one distinct regression method and one dedicated initial failed execution;
later diagnostic iterations are not additional production defects.
Fine sampling without projection failed the unchanged one-millimeter controls.
A broader matrix found 16 additional over-budget upper-arm recipes: parameter
witnesses of the same candidate-cost issue, not 16 new defects. The correction
resolves upper-arm features on a narrower starting grid and calibrates the local
field estimate to 0.75 mm within the independently tested one-millimeter physical
requirement. All ten surface methods pass, including the 12,000-triangle Mage
guard and 20,000-triangle guards across 96 arm cases. The complete library passes
463 methods with no failures and one intentional subprocess fixture. All four
fresh native composition methods and seventeen focused optimized methods pass.
Fixed-cast profiles cover all 26 Rogue appearances plus the exact benchmark
Mage without a fine-grid fallback. The controlled comparison validated six runs
over three balanced rounds without failures on the same machine. It confirms
the Mage cold-mesh correction (22,360,392 to 18,683,520 bytes, totals over three
cold samples per round). Its cold construction remains slower than main:
p50/p95/max 598.4/605.7/605.7 versus 355.5/358.4/358.4 ms, nine samples per side.
That startup regression remains open; a geometry-budget pass does not close it.
The fresh 101-body release runs at 1800 by 1125 pixels remain below 60 FPS,
with 20.35–20.88 ms median frames and 25.88–26.43 ms p95. Source density and
repeated field evaluations remain shared follow-up work.

The subsequent analytic field-evaluation candidate has five independent math
methods: known primitive values and axis derivatives, numerical slopes, ordered
and selected nested layers, singular/empty-field boundaries, and symmetric
hard-seam fallback slopes. The initial
zero-gradient feature stub passed scalar/finite controls and failed the three
derivative methods as intended; these deliberate feature first-reds are not
additional production defects. Before integration, sampled analytic values
matched the original scalar evaluator bit for bit. Primitive expressions and
field composition now serve both scalar-only and value/gradient queries, with
derivative work excluded from the scalar specialization. The integrated library
passed 467 methods with no failures and one intentional subprocess fixture
before the later Troll profiling correction below. The one-millimeter and
density guards remain unchanged; the bounded-retry checkpoint supplies the
subsequent native and optimized qualification.

Native profiling then exposed one **unpublished candidate geometry regression**
that the synthetic arm matrix had missed. Analytic placement rejected the exact
Troll's right upper arm for an inward face, expanding its source vertices from
4,720 to 75,118 in the fine-grid fallback. The dedicated
`creatures::geometry::surface_tests::resolved_troll_upper_arms_keep_fidelity_and_bounded_density`
passed independent distance and radial silhouette controls, then failed at
150,232 triangles against its initial 20,000 ceiling. This is one candidate
root, one distinct regression method and one initial failed execution in that
window. The correction retries symmetric differences once on the same aligned
grid, preserving the field and its error tolerances. Only a second rejection
uses the fine mesh; failed attempts emit no geometry. The focused Troll check
restores 4,720 vertices, and all 22 SDF and 11 surface methods pass. Its final
12,000-triangle guard also prevents two overlapping coarse attempts from fitting
under the previous ceiling. The generic 96-case 20,000 guard remains unchanged.
The bounded-retry correction passes the complete library: 469 methods, zero
failures and one intentional subprocess fixture exercised by its lifecycle owner.
All four fresh native composition methods and 33 focused optimized SDF/surface
methods pass. Fixed-cast profiling covers all 26 Rogue appearances plus the
benchmark Mage: one symmetric retry (Troll), zero fine-grid fallbacks, and no
density spikes. The cast totals 1,721,261 source vertices and 2,999,532 triangles
across 565 meshes. A fresh 101-body source inventory also found no fine-grid
fallbacks, with six symmetric retries across 1,847 cache-miss parts.

The subsequent controlled model comparison validates six runs over three
balanced rounds. Mage cold p50/p95/max is 482.5/538.6/538.6 ms against
384.0/406.7/406.7 ms on main (nine samples per side); warm is
7.457/10.111/10.111 versus 5.609/6.158/6.158 ms. Its pose median improves, but
p95 rises from 2.700 to 3.419 ms (270 samples per side). Human and Nymph
construction and pose measurements improve; these results do not establish a
uniform speedup. Three clean 101-body runs retain every body in every measured
frame at 1800 by 1125 pixels on the RTX 2060/DX12. They record
47.50/47.43/46.18 FPS, median 20.95/21.08/21.19 ms, p95
23.78/23.52/24.24 ms and maxima 30.18/25.67/210.44 ms. The third run's hitch
remains unresolved. The 60 FPS target remains unmet; these separate fresh-save
operating points are not controlled GPU before/after evidence. Hosted CI and
authorized raw-result publication remain open.

The asynchronous live-loading feature establishes its intended first red at
`creatures::loading::tests::a_cold_request_immediately_shows_the_egg_without_building_the_creature`:
the synchronous stub returns `Biped` instead of the required `Marker` stand-in.
This deliberate feature first red is not an additional production defect.
The loader owns background preparation, bounded work, stale-result rejection,
failure retention and render-reference transfer. Existing scene contracts now
explicitly await readiness in their fixture driver; production updates only poll.
The affected debug checkpoint passes ten loading, twenty scene, six frame
measurement, five capture and seven source-cache methods. The six existing
reaction methods also pass. Loading contracts include immediate eggs, a ready
result installed on the next poll while another figure remains queued, 101
pending requests without blocking updates, stale results, death, failure,
independent deformation assets and complete generated component inventories.
The transfer matrix includes loose hair, robes, cloaks, skinning, serpent
segments and carried lights. Worker collection also drains Bevy's asset change
and handle-drop bookkeeping while preserving cached sources.

Actual-client acceptance passes movement and inventory input while models are
pending, replacement without a subsequent server view, quitting with unfinished
jobs and private-backend cleanup, and 101 fully prepared camera bodies on every
timed frame. The movement run presents 23 initial eggs, moves the player two
cells west while loading continues, then reaches 25 ready models. Capture review
confirms eggs and their generated replacements. The native feature driver first
used an unsupported OS key spelling, then the wrong existing position field;
these are two driver errors in one method over two executions, corrected to the
existing key-tap helper and `world_feet`. Retaining the private backend handle
also makes failure cleanup independent of the client's graceful exit. These
failures do not establish production defects. Optimized checks pass all ten
loading, twenty scene and six frame-measurement methods. Both client profiles
build successfully. The actual optimized client also passes the native movement,
replacement and 101-ready-model sentinels against the unchanged debug backend;
these are client-profile checks, not a full release-server or platform matrix.
Hosted qualification remains open.
During implementation, the death-completion witness preserved death and snuff
controls, then failed because the completed corpse acquired a live hand entry.
Completion now registers a hand only for a current live actor in `Game.actors`.
This is one unpublished candidate defect, one distinct regression method and
one initial failed execution, separate from the deliberate feature first red.
The initial transfer fixture incorrectly read private payloads from the worker
cache after collection, then selected an updo instead of a loose-hair witness;
it now inspects exported payloads and uses the existing loose-hair positive
fixture. The timing fixture's one-second steps exceeded the real stall threshold;
100 ms steps preserve that threshold and reach the ready-scene positive control.
These fixture setup corrections do not establish production defects.

The live startup splash has an intended feature first red at
`startup::tests::splash_stays_until_world_and_visible_render_frames_are_ready`:
the no-splash stub returns `Finish` while the world is unavailable. The production
gate now requires world readiness and render-frame advancement before revealing
the game and finishing the splash. Independent bar-pixel and milestone tests
retain zero, half and completed progress, unchanged title art, the covered
concept controls, sticky errors and monotonic stage updates. The native first
checkpoint passes four methods: presentation before world startup, all milestone
frames and playable entry with eggs, cancellation/owned-process cleanup, existing
movement during loading, and two fresh-save launches that preserve prior saves.
The first startup run enters with 100 creature models still pending; it does not
qualify a frame-rate improvement.

A subsequent actual-CLI regression found one **unpublished candidate startup
defect**: missing connection credentials were checked after splash capture had
already created files. The rejection method first fails at that side effect,
before its diagnostic assertion. Credential lookup now belongs to pure launch
configuration before the splash; world initialization still follows its first
presentation. This is one candidate root, one distinct regression method and one
initial failed execution. The final checkpoint also adds a small/high-DPI display
fit witness for the illustration's layout. The complete debug 3D library passes
487 methods with no failures and one intentional subprocess fixture exercised
by its lifecycle owner. The refreshed six-method native checkpoint passes,
including the credential rejection and existing appearance-override rejection.
All six startup methods also pass optimized, both client profiles build, and
the actual release client passes three native presentation/cancellation/movement
sentinels against the unchanged debug backend. These are local Windows client
checks; hosted platform qualification remains open.

The investigation also confirmed a separate refiner boundary defect:
`sdf::refinement::tests::the_last_allowed_refinement_round_can_complete_the_surface`
failed on an independently defined sphere after the final permitted subdivision.
The loop never checked that final result. It now checks convergence without
adding a fifth subdivision; all six refinement methods pass, including the
pathological work cap. One root, one distinct method and one initial failed
execution in that dedicated window. Outward quad-diagonal repair and shorter
two-edge triangulation have separate topology controls; neither alone resolved
the Mage density regression.

## Rogue head composition, 28 October 2026

This focused local correction confirmed **one production defect in published
main, one distinct Rust regression method and one failing regression execution**.
Review found that head geometry used neck adornments which its cache key omitted.
The new `creatures::rogue_tests::shared_head_cache_matches_fresh_components_in_both_outfit_orders`
failed at the intended cache-history assertion for Commoner to Princess, after
independent fresh meshes established that the choker changes head geometry.
A resolved head component now supplies both generation and cache dependencies.
The regression then passed for Human, Nymph and Phantom in both outfit orders,
and the affected creature generation/animation suite passed.

The new native method
`test_client_process.Native3DProcesses.test_redressed_head_matches_a_cold_render_in_both_outfit_orders`
had **two fixture failures in two executions**, separate from the production
root: construction and refreshed idle poses initially differed, then the narrow
close-up did not meet its positive ornament-pixel witness. Joint-frame comparison
established the first cause; retained captures established the second. The fixture
now explicitly refreshes both poses, asserts identical joint frames and frames
the neck more closely. Neither pixel threshold was relaxed. Native acceptance
then passed with a visible positive witness and identical cached-versus-cold neck
pixels in both directions. These fixture failures are not stale existing tests,
additional production bugs, or platform/profile duplicates. This is local debug
geometry/native evidence, not release-performance or hosted CI qualification.

## Shared tube sampling, 28 October 2026

Review of geometry cost found that a straight 3 cm smoothed tube used 360
triangles because smooth shading unconditionally selected eight path
subdivisions and doubled ring sides.
`props::art_contract_tests::smooth_small_tubes_keep_closed_surfaces_without_blanket_subdivision`
confirmed enclosed outward volume and closed seams, then failed at the intended
density assertion in one regression execution. Adaptive cubic sampling now uses
control-hull and tangent/radius criteria with bounded work, retains authored
knots/radii, and bounds painted spans. Angular sampling honors authored limits.
The tube now uses 80 triangles with the topology and paint controls passing.
Twelve focused tests passed, then the 3D library passed 441 methods with no
failures. Its one ignored subprocess fixture was invoked successfully by
`launch::tests::malformed_startup_reaps_its_owned_server`; it is not a waived
behavioral test. Current Yeti variants contain 31,273–32,977 vertices and
39,380–40,756 triangles, retaining 18 meshes and zero warm mesh allocations.
These are geometry counts, not release timing or GPU performance claims.
All four native composition methods then passed. Matched native captures covered
all 26 monsters; inspected serpent bands, wing outlines, hair curves and fingers
showed no obvious regression. The initially clipped serpent views were reframed
for both binaries before reviewing their complete silhouettes. These views do
not accept the still-unfinished Yeti fur design. Two balanced three-round release
comparisons subsequently validated all twelve runs with no failures on matching
binaries/machine: cold construction and mesh storage improved substantially for
Human and Nymph, while warm spawning and animation were mixed. The first Nymph
pose p95 regression was concentrated in one head round; the repeat had similar
base/head p95 but higher head median/maximum. No animation-speedup or GPU claim
is qualified. Raw bundles remain local pending authorized publication.

## Camera-qualified crowds, 28 October 2026

Two distinct causes were evidenced by the first native run, one failed method
and one failed execution each. They are separate from the deliberate unit
first-red work for the new visibility feature:

- **Reporting/documentation defect:**
  `Native3DProcesses.test_frame_stats_report_the_crowd_benchmark_and_exit`
  confirmed 25 instantiated/posed actors but only 15 with body meshes in the
  camera on all 159 timed frames. Its retained image confirmed right-edge
  clipping. The old claim that the default view drew all 25 was false. Keep the
  instantiated count, add a full-window camera count and use documented distance
  16 for this diagnostic. The same strict native assertion passed with 25 on all
  176 frames after framing was corrected.
- **New fixture error:**
  `Native3DProcesses.test_frame_stats_measure_one_hundred_visible_creatures`
  observed only 97 instantiated and camera-qualified actors, rather than 101.
  The initial spaced square used a Euclidean assumption, while authoritative
  sight is Manhattan range 16. Four corners were undisclosed; other perimeter
  bodies lacked disclosed ground. A spaced diamond reserves sight distance for
  the floor and keeps all 100 NPCs. At distance 34, the unchanged strict native
  assertion passed with 101 throughout 90 frames; the retained image was reviewed.

The five frame-stat unit checks cover per-camera ownership, part deduplication,
off-camera actors, prop/loose-bound exclusions, 101 composed actors, complete
sample-window minima and missing/ambiguous camera data. A separate actual-model
owner confirms generated body meshes for all 26 Rogue recipes. Clippy and
scenario certificate/reference checks pass. Native runs include screenshot
readbacks and establish workload/correctness, not release timing; performance
and hosted qualification remain separate.

## Yeti geometry cost review, 28 October 2026

Maintainer review prompted a per-part debug geometry inventory. The representative
Yeti contained 49,861 vertices and 74,524 triangles; two forearms alone contained
24,032 triangles because detailed skin was still generated beneath the coat
backing. Removing covered forearm, upper-arm, thigh and pelvis cores reduced it
to 36,630 vertices and 48,636 triangles. All six sex/build variants now have
35,893–37,557 vertices and 48,060–49,340 triangles, with 18 meshes and no additional
warm-spawn meshes. These are static geometry counts, not release timing results.
The tighter 40,000-vertex / 50,000-triangle interim guard first failed at the
intended count and then passed; it is refinement first-red work rather than
an additional naturally detected test-defect total. New Yeti bent-pivot witnesses
and the existing centaur witnesses share unchanged angle, ray and minimum-flesh
assertions; all nine selected joint/model/fringe/motion owners passed.
The fresh native Yeti check passed, and its geometry inventory matched the unit
counts; front/rest and side/heavy inspection found no new joint gaps. The
remaining count is still high;
this is a measured reduction, not an accepted final gameplay budget.

## Surface fur development, 28 October 2026

The first surface-coat implementation had **one development geometry defect,
one distinct method and one failed execution**:
`creatures::geometry::fur_parts::tests::surface_coats_embed_roots_and_comb_tips_over_every_side`
found tips outside their required surface envelope. Projecting a tip with its
root normal could push it inside a tightly curved body. Recomputing the normal
at the destination corrected it without relaxing the envelope or flow assertions.
All six fur component tests then passed, along with seven selected existing/new
model, fur-support, profile and walking contracts. The complete-model witness
reports about 48,000–52,000 vertices, 18 meshes and zero warm mesh allocations
across both sexes and all builds. These counts are not timing measurements.
A separate deliberate resize first-red failed at the tip of a half-size patch;
its fixed millimeter lift now scales with the patch. This first-red contract work
is separate from the naturally caught projection defect counted above.
The first new native Yeti method had one fixture-oracle failure: its saturation
filter rejected pale fur (188 pixels against a 200-pixel witness), although the
retained frames showed the model. The corrected oracle compares both row-edge
background colors, requires over 10,000 foreground pixels and rejects an empty
background control above the cast shadow. It changes the measurement rather
than lowering the visibility requirement. This fixture failure does not add a
production geometry root.
The refinement witness
`creatures::rogue_tests::yeti_forearm_coat_keeps_volume_toward_the_resolved_wrist`
first established upper-forearm fur, then failed its distal width assertion. The
inherited fixed-length coat did not span Yeti's longer arm. A shared hanging-limb
constructor now uses resolved attachment reach and preserves fur volume toward
the wrist on both arms, both sexes and all builds. This refinement first-red is
not counted as an additional published defect above.
Preflight also caught a newly extracted test helper borrowing a consumed `Parts`;
its ownership was corrected before linking. That compile diagnostic is separate
from the evidenced behavioral defect and does not increase its method/root count.
The fresh resolved-span Yeti native method passed; three unchanged native
composition methods retain their earlier passing evidence. Visual review still
rejects the close-pile finish as scale-like: green topology/presentation tests
do not establish fur quality. Release/hosted qualification remains pending.

## Warm 3D mesh retention, 28 October 2026

A focused regression confirmed **one production performance defect in published
main, one distinct regression method and one failing execution**.
`creatures::rogue_tests::warm_spawn_does_not_publish_modifications_to_existing_meshes`
first validated its observer with a real mesh mutation, then a warm Nymph
composition emitted 24 modifications to existing meshes. Contact and hair
fitting called mutable asset access solely to reassert an already-set CPU
retention flag. A shared retained-source accessor now changes that flag only
when needed and otherwise reads immutably. The new regression and all 321
affected creature tests passed, including existing private-hair isolation and
posed-contact tests. Clippy passed. This proves removal of spurious asset events,
not a measured speedup; release timings and hosted CI remain pending.

## Shared organic arms, 28 October 2026

During centaur arm composition work, an initial shared root-cap change caused
**one development regression root, two distinct existing methods and two failed
executions**: `art_contract_deltoids_round_off_level_with_the_shoulder_line`
reported a raised crown, and
`geometry::art_contract_tests::art_contract_pauldrons_clear_the_resolved_arm_on_every_build`
reported insufficient plate clearance. A compact horizontal root and reduced
blend overlap restored both constraints without changing their assertions.
Both methods and all 320 creature tests then passed. Three native methods passed: the
new centaur build/motion acceptance, cached-versus-cold head rendering, and
equipment conflict restoration. The centaur tests' deliberate first-red runs
are feature-development evidence, not additional naturally detected defect roots.
This is local debug evidence; release measurements and hosted CI remain pending.

## Seed audit: Polymorph development, 26 October 2026

Reviewed on 27 October, covering the pre-correction library snapshot, local
`full02` Python stage, initial four hosted Rust lanes and the later inspected
Linux-debug Rust repair lane for [PR #198](https://github.com/KCall360/thresholds-of-ruin/pull/198).
The production list below also uses its bounded focused red/green witnesses.
These are **development defects**, not claims that broken behavior reached main.

**Confirmed in this window: 5 production bug roots, 1 documentation defect,
and 49 distinct stale/unupdated test methods.** The stale methods account for
**61 observed failed/error method executions** in the enumerated snapshots (10 + 32 +
16 + 3). That is a lower bound for the development effort, not a lifetime total
or all PR attempts. One stale method also had a harness error. A global stale
root-cause count is not established; maintenance categories below are not bugs.
No production-versus-stale percentage is inferred from this selected sample.

### Genuine defects caught

| ID | Defect and observed failure | Retained regression and correction |
| --- | --- | --- |
| P1 | Paid Polymorph used a provisional outcome after publication finalized the receipt; the visible changed-form case failed with `Rule(InvalidPlan)` after its positive saved-outcome case. | `abilities::polymorph_tests::paid_polymorph_identifies_only_immediate_visible_changed_form` in [Polymorph ability tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/abilities/polymorph_tests.rs). [Execution](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/abilities/execution.rs) now consumes the authoritative committed receipt, retaining the equality guard. Exact regression subsequently passed. |
| P2 | ChooseAgain restoration resurrected a drained Lore purchase instead of restoring an empty choice slot. The ordinary paid purchase/drain/earned-restoration witness reached its intended assertion and found Lore still present. | External red/green witness `OwnedChooseAgainFirstRed.test_owned_choose_again_discards_drained_lore`; permanent `creatures::body_tests::owned_choose_again_preserves_health_and_funding_not_removed_choices` in [body tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/body_tests.rs). Preserve funded identity, Health and purchase high-water marks, not removed choices. Both ordinary witness and retained regression passed. |
| P3 | Authored restoration refused a lawful sixth retained ordinary attribute receipt despite effective Strength cap 5; valid setup/drain/allocation succeeded, but total HD stayed 24 instead of restoring 25. | External red/green `AuthoredRestoreCapFirstRed.test_retained_authored_sixth_receipt_restores_with_effective_five`; permanent `creatures::body_tests::authored_restoration_reunites_six_lawful_physical_receipts_without_forging_birth` in [body tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/body_tests.rs). Retain raw ownership and cap its effective contribution. This proves a privileged-builder boundary, not ordinary advancement UI acceptance. Both witnesses subsequently passed. |
| P4 | Strict ChooseAgain decoding accepted extra fields; the malformed restoration payload unexpectedly deserialized. | `creatures::record::body::tests::personal_restoration_records_require_policy_and_exact_stable_birth_owners` in [record tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/record/body.rs). An empty struct variant replaces the permissive unit variant without changing valid JSON shape; exact focused regression passed. |
| P5 | AdvancementGrowth validation rejected valid owned zero-personal-HD/body-loss states: Wraith owner-page retrieval failed, and the Strength restoration witness received “Advancement is unavailable.” | `session::advancement_tests::canonical_wraith_earned_regrowth_survives_source_free_service_reopen` in [advancement tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/server/src/advancement_session_tests.rs), and `session::magic_item_tests::self_devices::growth_potions::imported_strength_restores_exclusively_then_reaches_combined_cap_independent_of_ring` in [growth tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/server/src/growth_potion_session_tests.rs). [DTO validation](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/protocol/src/advancement.rs) accounts for full authored body capacity while retaining strict bounds; both exact regressions and added capacity checks passed. Count this as one root, not two bugs. |
| D1 | The documentation test correctly detected a stale ruleset identity in a historical consumables checkpoint. | `test_documentation.DocumentationTests.test_documents_do_not_mention_other_versions`; corrected the historical prose rather than weakening the current-guide check. The repaired method passed. This is not a stale-test failure. |

P1's red was observed in a diagnostic build with unrelated reviewed production
reversals; it was additional to those intended mutations. Its traced receipt
mismatch, narrow correction and subsequent green support the classification;
the diagnostic is not an unmodified-baseline full run. The ten intentional core
mutation witnesses are not counted as ten additional naturally detected bugs.
Other feature/mutation investigations are outside this seed count; unverified
candidates remain pending rather than being promoted from notes alone.

### Stale-test counts and evidence boundaries

| Audited snapshot | Distinct stale methods | Observed stale-affected failed/error executions | Classification detail |
| --- | ---: | ---: | --- |
| Pre-correction simulation/server library run | 10 | 10 | Of 13 failed methods, 3 exercised P4/P5 (2 roots); the other 10 had stale fixtures/oracles. |
| Local `full02` Python stage | 32 | 32 | 30 of 31 failed methods were stale; D1 accounts for the other. Two of the 3 error methods also had stale setup, including the mixed spawn case below. |
| [Initial hosted run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40407638850), Windows/Linux debug/release Rust | 4 | 16 | Same 4 methods on each of 4 lanes; 3 fixture causes, not 16 bugs. |
| [Repair run, Linux-debug Rust](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40413523424/job/121890062812) | 3 | 3 | 3 later methods/fixture causes. Other repair lanes are not included in this execution count. |
| **Deduplicated total within these snapshots** | **49** | **61** | Different snapshots above contain different stale methods; additional reruns are not silently added. |

Library maintenance categories were: unselected restoration career, obsolete
compiled format, old starter HD total, four synthetic perception fixtures,
missing authoring fields, incomplete wand inventory, and obsolete retained-magic
rejection (10 methods).

For `full02`, categories were: old region format (3), missing supported-body
catalog closure (3), missing copied Chill body module (1), obsolete purchase
metadata (7), old starter HD expectations (15), Vampire prefix length (1), old
Extra Healing sole-HD oracle (1), and incomplete compact mock (1): **32 methods**.
The run itself reported **2,139 passed, 31 failed methods, 3 error methods and
100 waived**, with **33 failure assertions**. These are different units, not
contradictory totals and not a successful local full run.

The compact mock errored because its fixture omitted `hit_dice`. The spawn method
had two stale-catalog subtest assertions and a secondary `FileExistsError`; count
that stale method once and record its harness overlap. The other infrastructure
finding was a benchmark helper's 600-second cold Cargo-build timeout **before
benchmark execution**, not a performance assertion. Thus infrastructure affected
two methods in this stage, one overlapping the stale category; do not sum these
categories into an invented outcome total.

Hosted methods and corrections:

- `rogue_chill::rogue_chill_is_a_purchased_checked_status_signature_with_no_fake_damage`:
  author the unpaid variant's matching immutable package, retaining rejection
  under the original package.
- `rogue_roster::production_flying_roster_executes_authored_basic_melee_primaries_and_damage`
  and `rogue_roster::production_flytrap_uses_ordinary_clutch_tightening_and_single_escape`:
  install each newborn's canonical geometry before owned creature admission.
- `scenario_packages::rogue_projectile_content_is_default_available_and_expertise_is_an_ordinary_choice`:
  buy expertise from personal HD, not locked body HD. These four methods were
  fixed in [the server fixture correction](https://github.com/KCall360/thresholds-of-ruin/commit/a202ca6b12377d9bfb945585c2801701c12d471d).
- `advancement_ledger::saved_current_and_authored_purchases_reject_the_257th_owned_talent`:
  supply explicit nullable purchase identity and validate the positive roundtrip.
- `progression::latest_removal_preserves_retained_health_randomness_and_exposes_zero_hd`:
  expect the bound Health policy while retaining exact source/seed assertions.
- `supply_lifecycle::phase5_container_checkpoint_rejects_positional_creature_record`:
  include the new pending-return field before testing positional rejection.
  These three were fixed in [the simulation fixture correction](https://github.com/KCall360/thresholds-of-ruin/commit/5d1f3d90236cbad8e912f6dbd8e172012511e38f).

Both hosted corrections changed fixtures only and their exact four/three local
regressions passed; they did not justify relaxing production admission or strict
rejection. The later local broad run stopped at 626 passes/zero failures by
maintainer direction, not completion. Four repaired methods passed there;
the remaining 30 failed/error methods plus one affected Service-helper witness
passed in a 31-method selection. Sixteen focused Rust boundary checks also
passed. These scoped results are not an exhaustive local pass; final hosted
qualification remains separate, as recorded in PR #198.

**Evidence limits:** this seed was classified from the original local failure
and green receipts plus the specified hosted logs, not only PR prose. Local
raw logs are intentionally uncommitted and are not presented as reader-accessible
links. The linked PR/CI and immutable merged sources preserve public context,
regression assertions and hosted correction diffs; the local occurrence counts
above are the audit summary, not a claim that those links contain every local
receipt. No lifetime failure rate, complete CI-attempt total, or measured speedup
is established. Remaining unreviewed historical failures are **unclassified /
pending**, not zero.

## Dependency integration: SQLAlchemy 2.1.5

Reviewed 27 October 2026 for [PR #194](https://github.com/KCall360/thresholds-of-ruin/pull/194),
from its dependency-update head `aa70a74a46f819384009313894832c93aaa745fe`
and the bounded local correction receipts. **One production integration defect
root**, not a stale test: the hash-pinned requirements selected SQLAlchemy 2.1.5
while the sampler's runtime `PINNED` allowlist still required 2.1.4. A correctly
installed locked environment was therefore rejected before sampling. This was
caught in development; the evidence does not establish a broken main release.

The existing `test_arena_search_sampling.SearchSamplingTests.test_dependency_allowlist_matches_hash_pinned_requirements`
failed at its independent lock-versus-runtime assertion, then passed after
synchronizing the runtime allowlist without changing or weakening the dependency
hashes. This local window contributes **1 naturally failing method / 1 observed
failed execution / 0 stale-test roots or methods**. Broader hosted errors from
the same mismatch are not independently added to these bounded counts.

Two explicit regressions,
`test_environment_rejects_old_sqlalchemy_before_loading_the_sampler` and
`test_history_rejects_old_sqlalchemy_even_with_valid_integrity_hash` in the same
class, each failed before the correction and passed after it. Those are
**2 deliberate diagnostic red executions**, not additional discovered defects or
stale tests. The retained-history case repairs the integrity hash before testing
semantic environment rejection and also proves the rejected state is unchanged.
All seven selected dependency/history tests passed locally; mocked package
metadata acceptance, exact sampler-source digest, missing-dependency rejection,
syntax and unchanged-lock checks passed separately. Installed Optuna/process and
platform qualification belongs to the exact-head hosted run, not these local
mocked checks.

The follow-through is to update lock pins and their strict runtime allowlist in
one dependency checkpoint, with this cheap independent contract selected before
expensive sampling/process owners. Do not loosen history/environment validation
to make a dependency update pass.

## Rogue content batch: bounded iteration findings

The local catalog checkpoint on 27 October 2026 exposed **1 stale-test root,
2 distinct methods and 2 failed method executions**: after the approved dart and
shuriken identities were installed, two closed catalog-inventory expectations
still omitted them. The intended update script had stopped on a source-anchor
assertion after adding the definitions, before updating those inventories.
`test_rogue_equipment_content.RogueEquipmentContent.test_generated_gear_keeps_source_distributions_and_nonlauncher_damage`
and
`test_rogue_playable_content.RoguePlayableContent.test_selected_character_loadout_declares_equipment_and_finite_known_stock`
therefore failed for the two extra identities. Adding those identities to the
independent expected sets retained all unchanged weapon/armor values, starter
kits and live-pool assertions. Both passed in the subsequent 23-method focused
catalog/fixture checkpoint. This is test-maintenance cost, not a gameplay defect.

The same incomplete-edit run had one setup error in the new treasure example
owner because its deliberately new file was not installed yet; classify that as
incomplete new-feature/harness setup, not another stale root or production bug.
Intentional missing-payload, missing-room-cluster and missing-content first-red
checks are separately retained and excluded from discovered-defect counts.

### Compiler boundary and new acceptance drivers

The real current compiler then exposed **1 production integration defect root**
before publication: the server's authoring validation still rejected every
stackable equipment archetype, even though the runtime's narrow standalone-weapon
contract supported darts and shuriken. One natural compiler invocation rejected
`rogue.dart` in the production catalog. An intentional confirmation using
`test_rogue_throwing_process.RogueThrowingProcesses.test_text_carried_dart_and_shuriken_preserve_exact_pending_payload_after_source_deletion`
failed **1 method once** at that same compiler boundary. That confirmation is not
a second root or an additional naturally discovered test failure.

The repair shares the closed equipment-stack allowlist between authoring and
runtime, retains the outer item-capability exclusions, and adds a full authored
singleton/stack positive with invalid-capability contrasts. It does not change
weapon values or permit stacked armor, launchers or passive modifiers. All **44
current packages**, including the external 3D package, subsequently passed the
real compiler; the complete throwing Text process also passed with source-deleted
reopening and exact hit/miss unit accounting. These are local boundary results,
not an exhaustive or hosted qualification claim.

The first completed pair of new process drivers exposed **2 new-harness roots,
2 distinct methods and 2 failed executions**, not stale existing oracles or
runtime defects:

- The throwing method above attempted a third Focus-cost-2 throw after spending
  its finite pool of 4. The retained response was `Action: Failed.` and no unit
  was released. Ordinary bounded player waits now recover the previewed cost;
  assertions still require the actual admission, one-unit consumption and damage.
- `test_rogue_treasure_process.RogueTreasureProcesses.test_generated_cluster_shares_one_room_uses_next_depth_and_reopens_without_authoring_sources`
  attempted to control a newly born NPC not in the account's initial actor
  allowlist. The fixture now uses an explicit neutral, idle-AI control policy;
  production authorization, catalogs and the selected character remain intact.

Both methods passed after those driver repairs. The throwing method overlaps the
compiler-confirmation method above, so summing category method counts would
inflate a distinct-method total. The genuine native throwing witness remains
**not run locally / required in hosted CI**. Compile/lint preflights and
pre-execution review fixes are not included in these test-failure counts.

### Expired-history fixture premise

The focused server checkpoint reported **373 passed / 1 failed / 0 ignored**.
The failure was
`engine::floor_activation_tests::room_cluster::expired_abandoned_cluster_still_binds_selected_room_area_and_full_footprint`:
its initial assertion assumed floor 2 was absent from the live activation scopes.
Streaming legitimately prepares linked neighbors even with zero configured
radii; the reach frontier can activate floor 2 and prepare floor 3. This is
**1 additional new-harness root / 1 distinct method / 1 failed execution**, not a
production defect or stale existing test. Together with the two process-driver
causes above, this bounded new-harness window has **3 roots / 3 methods / 3
failed executions**; the earlier incomplete-file setup remains separate.

The fixture now targets floor 4, traversing three ordinary descents through
unpopulated earlier floors before rewinding. Distinct start/up/down anchors and
the three-step reach bound keep that target outside the initial frontier. Both
initial and expired-history absent-scope assertions, plus every room-area and
full-footprint corruption rejection, are retained. This is a fixture-only
correction; **its runtime rerun remains pending required exact-head hosted CI**.
A no-link preflight is not a replacement for that witness. The other selected
server owners, including the genuine current save writer/reader and encoded
standalone-stack storage-rejection/retry contract, passed in the same local run.

### First hosted content-batch checkpoint: cosmetic palette oracle

The initial [PR #200 run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40462237905)
at head `ade368b78bfeb0658636ccb9fe8b13db4f5ad8bb` exposed **1 stale-test root**:
current Hallucination expectations still described the 81-object catalog rather
than the 84-object catalog after opt-in dart, fruit and shuriken definitions.
The unchanged presentation adapter deliberately includes named catalog entries,
not only live loot. Independent source-tuple projection and the documented
unsigned selector yield `scroll titled NEM AL` at phase zero and
`scroll titled IV MEM` at phase one; creature samples are unchanged. Neither
the selector nor gameplay behavior needed correction.

This bounded six-job inspection covers Linux debug/release Rust jobs
`122049377146` and `122049377204`, Linux debug/release Python jobs
`122049377227` and `122049377124`, and Windows debug/release Python shard 2 jobs
`122052230753` and `122052230786`: **7 distinct stale methods / 18 failed
executions**, not eighteen bugs. Four Service methods failed in each Linux Rust
lane at their shared exact cosmetic assertion (8 executions); two actual-process
methods failed in each of the four Python lanes (8 executions); the independent
roster-vector method failed in both debug Python lanes (2 executions):

- `session::magic_item_tests::self_devices::hallucination::acquired_map_and_paid_public_plan_keep_historical_cosmetics_after_door_cure_and_source_free_reopen`
- `session::magic_item_tests::self_devices::hallucination::paid_production_dose_selects_exact_visual_cosmetics_without_query_rng_or_authority_changes`
- `session::magic_item_tests::self_devices::hallucination::peer_driven_exact_base_period_publishes_unsolicited_owner_and_followed_visual_updates`
- `session::magic_item_tests::self_devices::hallucination::pending_paid_dose_and_active_palette_reopen_interval_one_without_sources_before_service_pause`
- `test_magic_items_process.MagicItemProcesses.test_imported_hallucination_acquired_map_and_paid_history_survive_door_cure_reopen`
- `test_magic_items_process.MagicItemProcesses.test_imported_hallucination_text_visual_cadence_and_source_free_reopen`
- `test_rogue_presentation_vectors.RoguePresentationVectors.test_complete_authored_rosters_keep_visual_and_monster_sense_channels_distinct`

The repair updates only fixed expected tuples and their palette count across
the current Rust/Python owners, Text assertion, Service adapter, recording
validator and its pure fixtures. Exact identity, privacy, cadence, historical
memory, source-free recovery and negative checks remain intact; expectations
are not computed by the production selector under test. Both actual processes
and ten independent-vector/recording-validator checks passed locally using
unchanged qualified applications (**12 methods total**). Runtime reruns of the
Rust Service owners and hosted partitions remain required; no local relink or
broad rerun is claimed. The still-running Windows Rust lanes are outside this occurrence count.
Historical wire-format examples stay unchanged and valid. The previously
corrected depth-four expired-history owner passed both inspected Linux Rust
lanes; those lanes still failed overall on the cosmetic expectations above.

### Second hosted content-batch checkpoint: private damage publication

The [next PR #200 run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40466465054)
at head `30ec322b0e5c3308a09c2f74e3b3d60ee99d0f5e` passed all 1,213 server
library and 269 server integration methods in both inspected Linux Rust lanes,
including the repaired cosmetic owners. Each lane then reported **2,154 passed /
1 failed** in the simulation library. The failure was
`floor_activation::tests::room_cluster_retains_birth_evidence_after_actor_death_and_rejects_duplicate_ids`.
Its birth, duplicate-identity rejection and death checks succeeded, but final
checkpoint restoration correctly rejected unsealed participant-event receipts
as `RunState`.

This is **1 additional new-harness root / 1 distinct method / 2 failed
executions** in Linux debug/release jobs `122063447022` and `122063447005`, not a
production restore defect or stale existing oracle. The test called private
`apply_damage`, an inner-transaction effect, without completing the outer
publication boundary before checkpointing. The correction follows the existing
gold-population fixture: seal participant events, positively assert receipt
validity, then retain all existing birth/death/identity checks and exact restored
state equality. No production validator is relaxed and no events are discarded.

The repair receives a narrow no-link preflight; **the corrected simulation
method's runtime rerun remains required in exact-head hosted CI**. A source audit
of the other new direct-mutation/checkpoint witnesses is a bounded setup review,
not an additional runtime pass. Other profile executions are outside these two
inspected-job counts.

## Spear and magical-form development, 27 October 2026

This bounded local window covers the first linked affected-contract checkpoint
and its five actual-process checks, before publication. **No production defect
was established.** It found **1 stale-test root / 1 distinct method / 1 failed
execution**, plus **2 new-test harness-premise roots / 2 distinct methods /
2 failed executions**. These categories do not overlap. Deliberate feature
first-red witnesses and source-reviewed corrections made before runtime are
excluded from those counts.

- Stale schema owner: `scenario_package::tests::phase5_container_archetype_rejects_positional_fields`
  still enumerated 23 fields after the approved named `forms` field made 24.
  Add `forms` in declaration order; retain the positive named roundtrip, exact
  field-count equality and rejection of the complete positional representation.
- New ordinary-actor fixture: `abilities::magic_item_tests::physical_devices::ordinary_actor_can_hold_physical_device_without_gaining_creature_only_zap`
  tried to equip into a nonexistent socket: `spawn_actor` intentionally starts
  with empty anatomy. Configure normal rules anatomy without creature statistics;
  retain the equipment, forbidden Zap, charge, hold and checkpoint assertions.
  This setup rejection is not evidence of a physical-device production failure.
- New Adventure fixture: `test_rogue_staves_process.RogueStavesProcesses.test_text_staff_physical_hit_charge_exhaustion_and_pending_source_free_reopen`
  expected `examine wand` to resolve uniquely while carrying both a wand and a
  scroll whose disclosed name includes “wand”. Preserve the ordinary ambiguity
  prompt, choose the disclosed wand through clarification, then keep the exact
  wand/staff noun and unchanged-checkpoint assertions. Earlier physics, charges,
  exhaustion and source-free pending-work assertions passed in the failed method;
  that does not make the whole method a pass.

A separate local evidence-driver failure expected exactly one Rust summary line,
although a passing simulation integration method launches a nested test process.
The integration binary itself passed all 815 registered methods; this was
**1 harness root / 0 failed test methods / 1 aborted evidence-collection stage**,
not another gameplay defect or failed method execution. Preserve its complete
output and distinguish the outer summary when collecting evidence. Remaining
selected simulation-library owners were then run separately rather than repeating
already completed integration targets.

The two Rust fixture corrections passed the no-link server/simulation preflight;
their corrected runtime checks remain required in exact-head hosted CI. The
corrected staff process then passed its exact local rerun, including ordinary
clarification and unchanged simulation state. This is risk-based local evidence,
not a full-suite or hosted qualification claim.

## 3D crowd performance PR, 28 October 2026

Hosted [Windows debug 3D run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40618173786/job/122585477848)
on PR #202 head `9fc7d8a4`: **1 production-defect root / 1 distinct method /
1 failed execution**, caught in published `main` code by a PR run; the PR's own
changes did not cause it.

- `test_client_process.Native3DProcesses.test_mouse_picks_the_presented_item_and_inventory_keys_do_not_exit_early`
  hovered the relic under the player, then on the frame the left button went
  down the hover reported a zombie two cells away. The press selected it, and
  the relic never reached the inventory before the output deadline. Hover
  picking intersected the deliberately oversized culling boxes of deforming hair
  (a 3 m cube), cloaks and robes, so a distant figure's animated box could cross
  the cursor ray. The software renderer's roughly 7 s debug frames only made the
  window easier to land in; local software-adapter runs showed this branch's
  frames are no slower than FXC's.
- Fix: those meshes carry `picking::LooseBounds` and picking ignores them,
  finding the wearer by its body. Intended red:
  `picking::tests::culling_only_bounds_never_capture_the_cursor` failed at its
  loose-box assertion with the positive control passing; it passes with the fix,
  as does `creatures::rogue_tests::every_culling_sized_box_is_marked_loose_and_no_body_box_is`.
- The native mouse methods could not run locally (another window covered the
  client: a harness/environment limit, not a result). Exact-head hosted CI
  remains their qualification.

## Live interface shell checkpoint, 29 October 2026

The new native dock test first failed at the intended missing-HUD assertion
against the previous client. This is deliberate feature first-red evidence,
not a production defect. The first local selected Rust checkpoint passed three
HUD, one pack, four screen, five capture and two map methods. The map owner is
`minimap::cosmetic_tests`; an earlier zero-test `minimap::tests` selection supplies
no coverage.

The first five-method native run exposed two infrastructure roots: Windows'
Default Lock Screen intercepted the new dock and existing world-picking mouse
methods (two methods, two failed executions), and the selected binary directory
lacked the headless client used for cleanup saves (two screen methods, two cleanup
errors). Mouse readback identified the lock-screen window, rather than a UI
action refusal. No mouse assertion was weakened or waived. The asynchronous
movement/egg replacement method passed in that run.

Building the real headless helper corrected the missing executable. A two-method
keyboard retry passed the journal/place/note/history method including its cleanup
save, but advancement timed out before its first game capture. That startup
timeout remains unresolved, rather than being counted as another confirmed UI
defect or a pass. An isolated advancement rerun after the owned compilation
finished passed in 42.825 seconds; the cause of the earlier timeout is still
unknown. The expanded HUD owner then has eight passing methods, covering all six
dock actions, disclosure and bounded globe assets. Together with the twelve
unchanged pack/screen/capture/map methods above, these are twenty selected Rust
methods, not a full library run. These runs do not qualify native dock input,
optimized behavior or hosted CI. The complete interface redesign remains unfinished.

## Inventory browsing and camera framing, 29 October 2026

The independent model's Gear-filter test first returned sword, potion and token
instead of only the sword. This was deliberate feature first-red work; the full
model integration owner then passed 56 methods, including six new browsing
contracts for filtering, Unicode search, stable selection and reachable targets.
The first renderer checkpoint passed 30 selected methods and five genuine native
keyboard methods, covering inventory search/selection, identification and
directional zap, advancement, journal/place/note/history, and movement while
models replace their eggs. These qualify that initial candidate, not later edits.

Visual review caught one **unpublished UI candidate defect**: opening inventory
left the player at screen center under the panel. The native regression
`Native3DProcesses.test_inventory_reframes_the_player_without_moving_the_actor`
first passed its initial centered-camera fixture check, then failed at
`0.5 < 0.4` after opening inventory. This window has one root, one distinct method
and one failed execution. Camera-only framing now uses the actual panel extent;
the regression retains unchanged actor-world and revision assertions. The same
visual checkpoint prompted model-specific item symbols, tiled stone grain and a
hidden empty message panel. Final correction qualification is separate from the
initial candidate's passes. Real mouse input remains subject to the locked-desktop
limitation above; no waiver is inferred and the full interface goal stays open.

The framing/art correction passed 34 selected renderer methods and six native
keyboard methods in debug, plus 13 targeted release methods and three native
methods using the release client with unchanged debug backend helpers. The model's
56-method evidence remains applicable to its unchanged browsing contracts. These
are selected local results, not the full platform/profile matrix or hosted CI.

A subsequent input-routing regression exposed a second unpublished candidate
defect: `minimap::cosmetic_tests::typing_fields_do_not_toggle_the_map_and_the_shortcut_returns_afterwards`
first passed ordinary map toggling, then failed because typing M in inventory
search changed the map from Inherited to Hidden. This is one keyboard-routing
root, one distinct method and one initial failed execution, separate from the
camera defect. The fix captures search and full-screen input before the global
map shortcut; the native inventory witness also checks that typed M preserves
the map state. Its correction qualification is recorded separately.

The routing correction passed all-target debug Clippy, 14 affected unit methods
(three map, six inventory, five capture), and two native debug-client methods:
inventory search/selection including M, and place/note/history typing. The actual
debug client was rebuilt; documentation's 19 methods, Python lint and whitespace
checks passed. Earlier release results cover the unchanged inventory/framing
contracts, not this later map-routing correction. Mouse acceptance and hosted CI
remain unqualified.

## Equipment disclosure and browsing, 29 October 2026

The new presentation-model projection first returned zero sockets where the
fixture disclosed two positions of the same kind. This was intentional feature
first-red work, not a production defect. The projection preserves separate socket
positions and carried-item identities, remembers only disclosed properties, and
compares compatibility by socket identity rather than labels. The model owner
passed 59 methods after implementation.

The initial native equipment method stopped in the Windows driver with
`KeyError: 'F6'`, before delivering the new shortcut. This is one harness root,
one distinct method and one failed execution, not a client behavior failure. The
existing inventory, magic-prompt and asynchronous egg/input methods passed in
that run. Adding the missing F6 virtual-key mapping let the unchanged equipment
method pass in 15.281 seconds: browsing preserved position/revision, Remove
emptied the disclosed socket, Equip restored the same item, and Tab stayed local
to the panel. Later modifier/dice and full-slot presentation witnesses have
separate correction qualification. Mouse and hosted-CI limitations remain open.

The first complete candidate passed four native debug methods, but visual review
showed twelve slot tiles pushing comparison cards below the opening view. The
same equipment native method gained an independent viewport-budget assertion;
its twelve control rectangles were present before it failed at grid bottom
`0.7066666411823697 < 0.57`. This is one unpublished layout root, one distinct
method and one failed execution, separate from the earlier driver error. Compact
slot rows reserve the lower opening view for comparison; drawn empty-slot
indicators also replace the unsupported font symbol seen during review. The
in-progress release pipeline was deliberately stopped before layout changes;
that interrupted run is not qualification and its warm cache was retained.

The corrected layout passed all-target Clippy in debug and release, all 59 model
methods in both profiles, and ten fresh equipment-filtered renderer methods
(four UI and six unchanged outfit methods) in both profiles. Thirty unchanged
debug methods for inventory input, dock actions, pack order, capture, map input
and framing retain their earlier owner evidence, making 40 selected debug
renderer methods. Release additionally passed eight inventory and ten HUD
methods, making 28 selected release renderer methods. Both actual clients were
rebuilt. Four native methods passed in each profile: equipment, inventory search,
magic prompts and asynchronous movement/egg replacement (59.525 seconds debug,
56.528 seconds release). Release-client runs use the unchanged debug backend
helpers, not a full optimized backend matrix.

The corrected capture places the slot-grid bottom at 0.5146666607591841 of the
frame and shows both comparison cards beginning in the opening view. Further
details and actions remain scrollable. A fresh read-only desktop probe still
found Windows Default Lock Screen under the pointer; mouse acceptance stays
unverified and unwaived. Formatting, Python lint, architecture and documentation
checks passed. Hosted CI and the full interface redesign remain unfinished.

## Equipment action priority follow-up, 29 October 2026

The opening-layout root above needed a further correction: item actions still
followed property text and descriptions. The existing renderer method
`equipment::tests::equipment_renders_declared_sockets_unknown_properties_and_original_action_numbers`
first passed its socket, unknown-property and original-number fixtures, then
failed because the selected subtree ordered properties and description before
`1 Remove`. This extends the same opening-layout root to two distinct methods
(the native slot-budget witness and this renderer witness) and two observed
behavioral failures, not another root. An earlier authoring preflight caught a
redundant iterator `copied()` in the new hierarchy walk; no behavioral test ran
in that failed compilation, so it is excluded from those failure counters.

Compact horizontal item headers and actions before properties corrected the
ordering. Native acceptance now checks the actual selected Remove button's
nonzero rectangle lies wholly inside the panel's scroll viewport, alongside the
existing complete slot set, local browsing and real Remove/Equip cycle. The
corrected debug capture places Remove's bottom at 0.6684444546699524 and the
viewport bottom at 0.753777801990509 of frame height; both cards fit in the opening
view for this fixture. Ten equipment-filtered methods and five capture methods
passed, then all four native debug methods passed in 64.027 seconds. The unchanged
59-method model evidence and other unchanged owners retain their prior mappings;
these results are selected local coverage. Optimized correction qualification is
separate. The desktop probe still identified Windows Default Lock Screen, so
mouse acceptance remains unverified and unwaived.

The action-priority correction also passed release all-target Clippy, ten
equipment-filtered and five capture methods, then four native methods in 56.938
seconds using the rebuilt release client and unchanged debug backend helpers.
Both profiles retain the original equipment action/identity assertions and the
new complete-button visibility check. Formatting, Python lint, documentation and
whitespace checks passed. This is selected local UI qualification; it does not
qualify the hosted matrix, unlocked mouse input, a larger character preview or
the remaining interface work.

## Live equipment portrait, 29 October 2026

The feature's camera-boundary regression first passed a one-body world-camera
fixture, then adding a tagged portrait camera made
`frame_stats::tests::portrait_camera_does_not_change_the_exploration_frustum_count`
return None instead of Some(1). This was deliberate feature first-red work, not
a production defect count. Exploration camera readers now exclude the offscreen
portrait while retaining rejection of ambiguous ordinary world views. Camera
tests also preserve actor following and the world camera's cave cutaway with a
second camera present. The portrait shares live meshes and animation; lifecycle
tests cover ready meshes, detached parts, missing entities, actor changes and
original layer restoration without mesh duplication.

The first candidate passed 44 selected renderer methods and five native debug
methods in 77.560 seconds. Visual review still found two **unpublished portrait
candidate defects**: broad hair/cloth deformation envelopes made the figure too
small, and intrinsic image sizing squeezed out the heading. The existing native
equipment method gained two independent subtest witnesses. Its real rendered
figure occupied 0.46022727309013856 of image height, failing the unchanged 0.52
minimum; only 11.000001523643732 pixels remained for the heading, failing the
20-pixel minimum. This is two roots, one distinct method, one failed execution
with two failed subtests; subtests do not inflate method or execution counts.
Existing nonempty-render, identity, viewport and equipment-action assertions
remain intact. Failed authoring preflights for interface visibility/type aliases
and a deprecated handle constructor had no behavioral test executions and are
excluded from those counters.

Camera fitting now excludes loose culling envelopes while still rendering their
meshes; an independent AABB witness verifies both facts. Nonshrinking labels and
an image with zero flex basis reserve caption space. The corrected debug
checkpoint has 45 selected methods: six fresh portrait methods and unchanged
owners for frustum counts (seven), camera (five), capture (five), equipment (ten,
including six outfit methods), inventory (eight) and picking (four). All-target
Clippy and the rebuilt actual client passed. Five native methods then passed in
75.937 seconds, including real portrait pixels/scale/heading, local rotation both
ways, actor reuse, equipment actions, exploration framing, search, magic prompts,
and asynchronous movement/egg replacement. Optimized correction qualification is
separate; the broader interface goal remains unfinished.

The corrected optimized checkpoint passed the same 45 selected renderer methods
and all-target Clippy, rebuilt the actual release client, and passed five native
methods in 75.354 seconds. Native release-client runs use unchanged debug backend
helpers; this is not a full optimized backend or hosted platform matrix. The
retained corrected capture measures a 0.7379518042645412 figure fraction and
50.999998301267624 pixels of heading space. These are UI acceptance observations,
not frame-rate qualification. The unchanged model's 59-method results retain
their earlier source mapping.

The maintainer then explicitly authorized: "Mouse tests are waived." A local
exact-method manifest audits the five mouse-dependent methods on
`Native3DProcesses`: dock opening, filtered inventory/drop, world item picking,
movement/doors/mouse fetch, and selected-actor clicking. The supported direct
runner recorded exactly those five IDs as `waived` in each profile-labelled
selection, with no mouse executions or passes. Other keyboard/rendering and
non-graphical checks remain required, and hosted CI remains unchanged. This
authorization supersedes the earlier unwaived local mouse limitation for this
task; the waiver records are not a clean full-suite gate.

Waiver audit also found one stale oracle by source inspection: the dock test's
expected catalog still omitted the added Equipment action. Its independent
expected set now includes all seven controls, preserving the existing click and
no-world-action assertions. This is one stale-oracle cause and one method, with
zero observed failed executions because the method is waived. The already
captured real frame contains the seven expected controls. An AST comparison
excluding only the five waived method bodies preserves the unchanged context
and bodies of the five already-passing native methods, supporting their reuse
after this isolated oracle correction. The full interface goal remains open.

### Character-sheet navigation fixture authoring (2026-10-29)

The new renderer test
`character::tests::browsing_uses_shared_commands_and_preserves_drafts_and_hidden_screen`
failed on its second independent request: the fixture reset `Game.busy` without
acknowledging the transport's one-request ready gate. A fresh ready test channel
per independent case fixes the fixture while preserving all command and
rejection assertions. The same correction was applied to the new screen test
before it ran. This is one fixture-authoring cause, one observed failing method
and one failed execution, not a production defect. The intentional first-red
for previously absent Page Up/Down navigation is feature work and excluded from
those counts. Compiler authoring errors are also excluded. The fresh character
owner passes two methods, including the independent free-command catalog,
redaction and page-scroll assertions; five screen and ten model-projection
methods pass. Two actual debug-client methods also pass, covering catalog
navigation, typed ledger commands and other full-screen keyboard workflows.
A hash-checked assertion-only delta preserves the production inputs of those
native runs. Release and hosted CI for this slice remain unrun; the explicit
five-method local mouse waiver remains separate from these passing checks.

### Graphical advancement choices and confirmation (2026-10-29)

Five defects were exposed in the new adapter while building graphical purchases:

- `progression::tests::hover_exit_restores_the_card_color_without_erasing_keyboard_selection`
  found that a card stayed highlighted after pointer exit. Feedback now restores
  the base color while retaining a keyboard-selected card's highlight.
- The 3D model integration method
  `advancement::preview_balance_details_are_read_only_and_do_not_offer_hidden_pool_shortcuts`
  found selectable funding choices in read-only preview details. Options and
  dispatch now require an actual Balances or Catalog query, respectively.
- The same integration owner's
  `advancement::supply_display_mode_never_exposes_the_retained_catalog_as_hidden_shortcuts`
  found that a typed supply chooser could be replaced by its retained catalog.
  A typed display-mode flag preserves the shared supply prose and removes hidden
  shortcuts. A renderer visibility assertion and the real-client workflow retain
  the cross-feature witness.
- Visual review found Confirm below the scroll viewport despite a passing native
  keyboard workflow. The owning regression
  `progression::tests::confirmation_stays_outside_the_scrolling_preview_body`
  failed before moving the action into a fixed footer. Native acceptance now
  checks its entire button rectangle against Bevy's actual calculated clip.
- `progression::tests::confirmation_names_the_exact_retained_proposal_while_browsing_other_pages`
  found a generic confirmation label when browsing away from the preview. The
  footer now names the immutable proposal using the shared formatter. Native
  acceptance reads the actual child Text and checks its subject both in the
  preview and while browsing the ledger.

These are five candidate-production causes, five distinct failing methods and
four failed executions (the two model methods failed in one execution). The
intentional first-red for the new freshness API and the initially absent
native graphical choices is feature work and excluded. Profile repeats do not
add causes or failures.

Two fixture causes are separate: the existing
`net::tests::an_aimed_device_takes_the_next_direction_instead_of_a_step` app lacked
its camera input system's Panel resource; adding that resource preserves its
original aiming and movement assertions. The new common method
`advancement::tests::supply_display_mode_is_disclosed_separately_from_the_retained_page`
initially expected the overlay to close on request. It now witnesses the real
boundary: retain the display while pending, then close it on a validated reply.
These are two fixture causes, two methods and two failed executions, not
production defects. Compiler authoring errors and the initial missing
`test-support` preflight feature are not behavioral regression evidence.

The final local risk checkpoint passes 86 selected methods in each profile:
27 shared advancement, seven renderer-independent model, and 52 renderer
(progression nine, character two, screen five, transport eleven, scene twenty,
diagnostics five). Direct pointer-state dispatch through the generated cards
and footer is covered independently of keyboard dispatch. Four native methods
pass with each actual client profile: exact-once ordinary purchase after an
uncommitted preview; preserved typed supplies/ledger, other screens and
confirmation visibility/subject; and movement plus egg replacement during
asynchronous generation. Both native selections use unchanged debug backend
helpers; this is not an optimized-backend or full platform matrix qualification.
The final source is freshly checked after the allocation-free freshness predicate
and explicit confirmation label. An earlier release run was deliberately
cancelled after the clipping defect; incomplete phases do not qualify anything.
The five audited native mouse methods remain explicitly waived locally and
required in CI. Hosted CI and the full UI redesign remain open.

## Character portrait and structured preview composition (2026-10-29)

The new keyboard-only
`Native3DProcesses.test_character_screen_shares_the_live_portrait_and_keeps_command_keys`
initially timed out because its post-release predicate required Home to remain
pressed. The shared `tap` helper already witnesses the pressed frame, then waits
for release. The actual transcript showed the preserved draft and unchanged
portrait angle. This is one new test-oracle defect, one distinct failing method
and one failed execution, not a production defect or a mouse-waiver case. The
corrected predicate checks the preserved draft after release; rotation, revision,
world position, real portrait pixels and pane non-overlap assertions remain.
The focused debug rerun passes. The two intentional first-red feature failures
for absent character portrait activation and result cards are excluded from
production-defect counts.

The selected renderer owners cover portrait activation and shared-rig lifecycle,
command draft precedence, actual-pane camera framing, responsive sheet widths,
structured health/resource/consequence cards, explicit death warnings, supply
page priority, and retained exact confirmation behavior. Visual review confirms
the two panes fit and the confirmation stays outside the scrolling results.
The final local checkpoint passes 83 fresh renderer methods in each profile;
34 unchanged shared/model advancement methods retain their previous debug and
release qualification after input-hash comparison. Seven selected native
behaviors pass with each actual client profile (six original debug passes plus
the corrected character rerun; all seven together in release). These include
movement and egg replacement while models prepare. Both native selections use
unchanged debug backend helpers; this is functional UI qualification, not an
optimized-backend matrix or performance measurement. Clippy, formatting, Ruff,
architecture, 19 documentation checks and whitespace checks pass.
The full interface redesign and hosted CI remain open; the five audited native
mouse methods remain waived locally, never recorded as passes.

## Map/journal composition and bundled-font coverage (2026-10-29)

Visual inspection of the new map/journal found a north-arrow glyph rendered as
an empty box. The static labels used a character absent from the bundled Crimson
Pro font. `map_journal::tests::static_journal_labels_are_supported_by_the_bundled_font`
then failed against the actual rendered label strings and bundled font cmap,
identifying only `↑`. The label now says “NORTH UP”; the regression checks all
static journal labels so a similar unsupported symbol cannot silently recur.
This is one unpublished UI defect, one distinct failing method and one failed
regression execution, originally found by visual review. It is separate from
the two intentional first-red map-opening feature failures. Compiler authoring
errors are excluded from defect counts.

The first debug checkpoint passed 100 selected renderer methods and six actual
keyboard-driven client methods, including enlarged-map pixels, preserved
rename/note behavior, search draft precedence, character portrait, exact purchase
confirmation and asynchronous egg replacement. Visual review also corrected the
place-origin and keyboard-scroll labels before final qualification. An attempted
`interact::` unit filter selected zero methods and is explicitly unqualified;
actual input boundaries are witnessed by the inventory owner and native tests.
The final corrected source passes 101 fresh selected renderer methods in each
profile (map/journal eight, minimap four, HUD ten, screen five, diagnostics five,
character three, inventory nine, portrait seven, progression eleven, camera five,
network fourteen and scene twenty). Thirty-four unchanged shared/model
advancement methods retain their previous debug/release qualification after
comparison of 88 registered input hashes. All six native methods pass again
with each final actual client profile, including the rendered map, preserved
rename/note and draft controls, exact confirmation, portrait, and movement plus
egg replacement during asynchronous generation. Both native selections use
unchanged debug backend helpers; these are functional checks, not a full
optimized-backend/platform matrix or comparative performance measurement.
Final visual review confirms the supported heading, place origins, map aspect
and scrolling help. Clippy, formatting, Ruff, architecture, 19 documentation
checks and whitespace checks pass.
The five audited native mouse methods remain waived locally, not passed; hosted
CI and the full UI redesign remain open.

## Add the next useful finding

One compact row or paragraph per diagnosed cause is enough:

> Date / PR / source state; category and supported contract; exact failing test
> IDs and intended red; fix and green evidence (link when available); distinct
> roots / methods / observed failed executions with window and overlaps;
> unresolved or CI-only coverage. Update an existing cause instead of recounting
> it when another OS, profile or rerun exposes the same failure.

Use the findings to improve fixture update lists, cheap preflights and affected
owner selection. They are not a reason to remove strict assertions, security
coverage, unique boundary witnesses or required native CI.
