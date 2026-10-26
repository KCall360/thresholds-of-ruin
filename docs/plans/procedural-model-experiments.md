# Procedural model refactor experiment history

These are preserved investigation checkpoints, including superseded approaches and
known failures at those revisions. They are not acceptance claims for the current
implementation. Start with the [current handoff](procedural-model-handoff.md),
particularly its [independent-lock checkpoint](procedural-model-handoff.md#independent-lock-checkpoint)
and subsequent bounded hair witness. The [art continuation guide](3d-art-continuation.md)
contains additional visual findings.

The sections below retain their original chronology and evidence. References to
then-current code describe those checkpoints; later implementation may have moved
or replaced it. No historical failure is waived by moving its record here.

## Historical blended-surface prototype

[Surface bindings](../../crates/client-3d/src/creatures/deformation.rs) retain
weighted authored source parameters separately from mutable motion state. At this
historical checkpoint, hair compiled contributions using the same capsule distances,
ordering and smooth-union width as its generated surface. Production long hair now
uses explicit independent-lock ownership; see the [independent-lock checkpoint](procedural-model-handoff.md#independent-lock-checkpoint). Cloaks use the same evaluator for authored row flexibility.
This is a shared binding layer, not yet a shared simulation or fitting system.

The continuity regression failed with exclusive nearest-source ownership and
passes with blended contributions. Four binding tests, ten SDF tests and five
cloak checks pass. The binding-only checkpoint passed nine hair checks but retained the model jump
failure at tick 15 with nine front witnesses (the contract requires more than
twenty). The subsequent timing change described below worsens that witness to zero.
Do not claim this prototype fixes jump motion or the visible neck bend.

Attachment targets, immutable fitted rest geometry and free-control history now
have separate roles. The initial contact-fitted chain and its frame are retained
in torso support coordinates. Pinned targets follow the posed head/socket frame;
free recovery targets retain their support-frame orientation and follow the mean
translation of pinned controls. Previous free controls are no longer rigidly
carried by head rotation. This is a torso-frame hanging policy, not a world-space
gravity or inertial simulation. Multiple and absent attachments have explicit
regression coverage.

Recovery receives elapsed time from animation and predicts exponential attraction
once before length/contact projection. Constraint iterations no longer advance
recovery time. Projection uses immutable rest lengths rather than distances
between changing recovery targets. A stationary attachment continues settling;
unchanged frames are cached only after control movement falls below the
normalized-space tolerance. Zero-duration updates preserve an existing fit;
initial construction still fits geometry.

The rotation and rest-length regressions failed before this change: free fall
rotated with the socket, and a changed recovery target doubled a unit segment.
Both now pass. Six shared deformation tests and twelve hair checks pass. The
full-model jump at that pre-rest-fitting checkpoint failed at tick 15 with one front witness; the timing-only
checkpoint had zero and the binding-only checkpoint had nine. The unchanged
contract requires more than twenty. None of these counts establishes acceptance.
The control trace still shows substantial upward motion as the jump progresses;
removing rigid head carry did not resolve it. Posed-arm contact and its missing
motion history remain unresolved; do not tune recovery or weaken coverage to
hide the failure.

The earlier time-contract regressions also remain: equal elapsed free recovery
at 20, 50, 60 and 120 Hz agrees, zero time preserves controls, and the real wearer
fixture verifies stationary settling and per-instance mesh ownership. Constrained
motion with moving contacts is not yet shown update-rate independent. A shared
solver must earn reuse with both hair and cloth; the common binding/target layer
alone is not a completed shared simulation.

Binding evaluation allocates nothing per vertex per update, but compilation stores
a vector of contributions per vertex. The broader hair update still allocates
working arrays and performs expensive mesh contact correction. Construction memory,
control simulation and final mesh deformation need separate measurements before
accepting a performance claim. The rebuilt native client completed all eight deterministic comparison captures.
Resting and jump side views still show the neck/shoulder bend; successful capture
execution is not visual acceptance. Release all-target Clippy, the release build
and repository quick check pass. The full debug/release matrix remains outstanding.

## Earlier renderer verification checkpoint

The live renderer checkpoint passed 11 focused script tests, eight robe/cloak
checks, three native live/capture process tests, release build/all-target Clippy,
architecture and repository quick checks. The launcher completed all four
40-second chapters in one window with 12 preloaded models. Documentation-only
line-spacing cleanup subsequently passed 18 documentation tests.

This evidence does not establish full procedural-model acceptance. The full 3D
debug/release matrix remains incomplete. Earlier hair failures below are historical;
the independent-lock checkpoint records the current focused acceptance. Native mouse tests were waived by the
maintainer for this work. Nothing has been pushed or merged by this checkpoint.

Before semantic rest fitting, the attachment-frame experiment retained this failure:
`contact_survives_jump_head_pose` fails at tick 15 with one front witness. Six
shared deformation tests and twelve other hair checks pass. Release all-target
Clippy, the release binary build and repository quick check pass. The native
fixture completed all eight captures; inspected resting and tick-13 side views
retain the neck/shoulder fold. This is not visual approval. The full 3D
Windows/Linux debug/release matrix remains outstanding.

## Staged diagnosis of the initial neck fold

The preserved `contact_survives_jump_head_pose` fixture now supports opt-in
`TOR_HAIR_DIAGNOSTICS=1`. Its `HAIR_FIT_STAGES` JSON report contains authored
controls, body-only fits, full-contact fits, current fitted controls, and authored
and final surface points, all in normalized torso support coordinates. It runs
before the jump and does not alter fitting or acceptance assertions. Ordinary
test runs omit this large diagnostic report. Use the existing release test command
with this environment variable and `-- --nocapture`, redirect output locally,
then unset the variable. The diagnostic is test-only, not a server or renderer
protocol. The existing jump failure remains at tick 15 with one front witness.

Here, body-only means the static composed torso support, including the breastplate,
with posed-arm envelopes excluded. The initial-state report separates two effects.
Body-only fitting moves the
front locks laterally; full posed-arm contact then raises some controls above
their preceding pinned controls, creating a reversed segment before animation.
The source has four front locks (indices 11 through 14 in this recipe); these
indices are evidence for this authored recipe, not a proposed semantic API.

| Front lock | Preceding pinned y | First free authored y | Body-only fitted y | Full-contact fitted y |
| --- | --- | --- | --- | --- |
| 11 | 0.6059 | 0.5068 | 0.5479 | 0.5916 |
| 12 | 0.6228 | 0.5321 | 0.5821 | 0.6822 |
| 13 | 0.6059 | 0.5068 | 0.5490 | 0.5944 |
| 14 | 0.6228 | 0.5321 | 0.5854 | 0.6831 |

For lock 12, the tip moves from authored y=0.2601 to body-only y=0.3416 and
full-contact y=0.4217; its lateral coordinate moves from x=-0.1453 to -0.2307
in the body-only fit. The right-side counterpart behaves similarly. Thus missing
motion history cannot explain this initial fold on its own. The current arm
profiles are mesh-derived cylindrical envelopes; their conservatism may contribute,
but these results do not establish how much is proxy error versus genuine overlap.

That diagnosis motivated the semantic rest-fitting checkpoint below, preserving authored
support intent: front locks fit over the chest, back locks over the back, and scalp
attachments remain fixed. Reuse the existing torso-surface clearance queries,
rather than infer the intended side from a nearest-surface escape. Keep this
separate from motion-time contact and rest-length constraints: a procedural rest
shape must first adapt to the composed wearer and equipment. Preserve front/back
intent as recipe metadata, not hard-coded strand indices. Test both support sides,
clearance, pinned controls, ray misses and already-clear authored points, followed
by the unchanged full-model witness and native capture comparison.

This diagnostic-only checkpoint passes release all-target Clippy and repository
quick verification. Production fitting is unchanged, so the preceding native
captures remain the visual baseline; the diagnostic run reproduces the same
full-model failure rather than claiming new acceptance.

## Semantic rest fitting checkpoint

[Control curves](../../crates/client-3d/src/creatures/deformation.rs) now retain
an optional typed `SupportSide` with their authored points. The hair recipe labels
front locks, back locks and the unconstrained brow lock; runtime fitting does not
infer semantics from strand indices. `fit_surface_controls` reserves the control
radius against a caller-supplied composed surface query, moves only outward along
the selected front/back direction, and preserves lateral/vertical pattern
coordinates. Pinned controls, missed rays and already-clear authored detail remain
unchanged. Hair uses the existing torso clearance queries, including the equipped
breastplate, before establishing its contact-fitted rest geometry and lengths.
This remains a torso-frame fitting policy, not a universal anatomical landmark API.

The intended clearance regression failed before projection was implemented. Seven
shared tests and one recipe test now pass; twelve hair checks pass. Opt-in stage
reports are now version 2 and include `surface_fit` and `support_side` before the
body-only/full-contact stages. The first free controls of the two previously
reversed front locks now fit near y=0.561, below their pinned y=0.623; their tips
remain near y=0.273 instead of y=0.422. The rest fit no longer creates the measured
initial control reversal.

The full-model jump passes the former tick-15 coverage failure and retains 135
front witnesses at tick 19, but fails an unchanged edge-stretch bound at tick 20:
source edge 0.007435487, fitted edge 0.020314265. One endpoint has zero mobility and
is inside a posed contact shape (distance about -0.00562); its neighbor has mobility
about 0.00372. Keeping that point fixed conflicts with clearing the obstacle. Do
not weaken the stretch limit or silently unpin arbitrary vertices to hide this.
The initial rest-fitting improvement is not full motion acceptance.

This diagnostic-enabled suite took about 513 seconds; that is not a controlled
performance comparison. The final surface solver still measures stretch against
unfitted source edges and performs many collision/relaxation sweeps. Performance
qualification remains open. The release build passes. The new resting side capture
shows front locks extending farther down the chest, but a collar-like band remains
near the neck. Repository quick verification passes, but the native run fails its existing
180-second watchdog after only the two resting captures (`scene or GPU readback
timed out`). Six captures remain incomplete. This is a current acceptance failure,
not a waived check; do not raise the timeout as a substitute for resolving the
expensive/conflicting constraints.

The next attachment experiment should preserve attachment intent in the authored
control recipe and derive surface mobility from the same bindings, instead of
using global height thresholds independently for pins and mesh mobility. First
verify the intended scalp region against the actual geometry: the failing fixed
vertex must not be relabeled just to satisfy this one test. Preserve exact scalp
attachment, continuous transition to free hair, composed contact clearance and
the existing strain checks. Distinguish an infeasible fixed attachment from a
moving-contact or conservative-envelope error in diagnostics.

## Authored attachment checkpoint

`ControlCurve` now carries explicit attachment flags alongside points and support
side. The recipe reuses its existing scalp/fall boundary in authoring coordinates;
it does not recompute attachment from the wearer's torso height. The brow lock is
fully attached. This is authored recipe ownership, not yet a complete named body
landmark system. The duplicate height-derived runtime pin table has been removed.
Surface mobility is interpolated through the same immutable source bindings and
smoothed before both skinning and contact/strain correction consume it.

The first-red regression showed an authored attachment incorrectly becoming free
at torso height 0.4. Ownership is now independent of body translation/height. The
existing exact-attachment and continuity witnesses remain, and an independent
crown-region check covers real source vertices rather than defining the scalp
solely from whichever vertices happen to have zero mobility. Recipe checks also
protect attached roots/crown and unattached jaw/neck fall.

Seven shared tests, one recipe test and thirteen hair checks pass, including crown
protection and per-instance mesh ownership. The full jump passes the prior tick-20
attachment failure and retains 637 front witnesses through tick 25. At tick 26 it
fails the unchanged stretch bound on a different edge: source length 0.007283831,
fitted length 0.022842944. Both endpoints have full mobility and bind primarily to
neighboring back locks 8, 9 and 10. This is no longer the fixed-anchor conflict.
The diagnostic run took about 889 seconds; it is not a controlled performance
benchmark. The release build passes, but native replay fails the existing 180-second watchdog. Quick qualification remains pending for this revision.

If dense surface repair remains too slow or overconstrained, evaluate generating
independent hanging locks from the existing parametric curve/tube machinery while
retaining a common attached cap. The current smooth-union surface connects locks
whose controls can move independently. The latest failing edge makes that a
representation worth testing, not a proven root cause. Preserve actual emitted
surface clearance, scalp attachment, strain checks and visual review. Do not simply
raise iteration budgets, widen strain limits or increase the renderer watchdog.

## Scope correction: solve the representation before tuning constraints

The current hair work has become too focused on local constraint adjustments.
Passing small solver tests has not delivered native process acceptance or acceptable
runtime cost. Treat the remaining strain failure and native timeout as evidence
against continuing to increase dense-mesh repair work.

The next bounded experiment is independent parametric hanging locks, attached to a
shared scalp cap, with immutable source bindings to a small control representation.
Fit that representation against the composed body and equipment surface, then deform
the render mesh from it. Do not fuse independently moving locks into shared triangles.
This is a hypothesis to validate, not a claim that independent locks automatically
prevent penetration. Verify emitted triangles as well as controls.

Generalize the contracts, not one universal simulation algorithm: components declare
attachment ownership, rest geometry, contact requirements and render bindings. Hair,
cloth and rigid armor can use different motion solvers behind those contracts. Keep
rest fitting separate from per-frame motion, cache composition-dependent work, and
bound per-frame work. Preserve deterministic capture independently of live playback.

Use focused acceptance for known conflicts plus seeded composition sampling. The
experiment must retain scalp attachment, surface clearance, strain checks and visual
quality, and complete native capture within the existing watchdog. Measure CPU frame
timing separately from GPU/display behavior. Do not expand the composition matrix
until this representative difficult case works. A full named landmark system and
wire-format changes remain separate design decisions.
