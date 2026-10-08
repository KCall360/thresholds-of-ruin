# Adapting the unpublished interaction work

This work starts from PR #87 (`28c5932`), using its accepted shared TOR design.
The complete September interaction checkout is retained in local archival commit
`fc4f958` on `codex/milestone-4f-interactions`. That archive includes obsolete
formats, fixtures and documentation; it is reference material, not a release.

## Scope and sequence

1. Physical item classes, ASCII symbols and remembered-item disclosure.
2. Actor-owned preparation integrated with saved intentions, readiness, retries,
   streaming, interruption and recovery.
3. Anatomy-based equipment, starting gear, effective combat and death drops.
4. Shared initial healing/damage effects, timed consumption and knowledge.
5. Knowledge-limited AI healing, gear replacement and safe nearby looting.
6. Actual-client acceptance, targeted performance evidence and GitHub CI.

Only retained behavior is ported. The fixed three-slot model, raw numeric wire
targets, old save readers and two-variant consumable architecture are not adopted
as permanent foundations. Existing attack preparation identity and command
admission remain authoritative. Humanoid gear and two initial potion effects are
the first consumers of extensible anatomy and effects, not alternate Rogue rules.
The exploration-first [Rogue sequence](rogue-scenario-plan.md) remains unchanged.

## Verification strategy

The maintainer explicitly requested expedited iteration: affected compile checks,
focused behavior tests and representative actual-process acceptance locally;
the full Windows/Linux debug/release suite is deferred to GitHub CI. Preserve
recovery, disclosure and native-input assertions. Do not substitute historic
September benchmark results for measurements of the adapted implementation.

## Current progress

The original dirty work is archived and the adaptation branch is based on PR #87.
Physical classes and ASCII glyphs are implemented. Preparation now belongs to the
actor rather than its combat state, preserving existing attack behavior and
saved intention identity. Generalized preparation, anatomy-based equipment and
shared immediate healing/damage effects are now implemented in the simulation,
server, authoring and transport boundaries. Text and ASCII controls now use shared
client item checks. Knowledge-limited AI item actions remain outstanding.

Verified locally: workspace all-target compile check; 50 protocol integration
tests; 33 ASCII model tests followed by four focused memory tests after expanding
class coverage; five item simulation tests; two authoring/disclosure tests; a
region-detachment round trip; seven documentation checks; all 33 package
certificates; and four actual native ASCII process tests. Native acceptance covers
hidden potion identities, category glyphs, save/restart and existing remembered
map/rewind behavior. Current wire samples were recorded through real processes.
The concealed-placement regression failed before its restriction was added.
Full suites and targeted release profiling remain outstanding.

The preparation ownership refactor passes 25 simulation intention tests, 14
combat tests, 18 region-lifecycle tests and five server tests covering preparation
identity, persistence rejection, paused recovery, independent admissions and
rewind/recovery lineage. These are focused checks, not a full-suite claim.

The equipment/effect foundation has focused simulation and server tests for
completion timing, damage interruption, same-admission continuation, concealed
statistics, duplicate anatomy sockets, death drops, observable identification,
consumed objectives, replay and checkpoint restoration. The affected simulation
integration suite passes 99 tests, the protocol suite passes 51 tests, and focused
server checks cover preparation recovery, action/save schemas and appearance
pool affordances. Seven documentation checks pass. Anatomy definitions are
shared after checkpoint decoding, matching the existing refactor architecture.
A real server/headless-client process case passes for starting gear, equipment
changes, equipped-drop rejection, consumption and checkpoint restart. All 34
scenario certificates and current wire samples have been regenerated.

Text commands now support equip/wear/wield, remove/unequip and drink/quaff using
carried-item affordances and free matching anatomy sockets. Adventure commands
use the same decisions and narrate preparation. No hidden statistic chooses an
item or socket, and replacement requires a separate removal. The text integration
suite passes 87 tests, the shared socket-choice test passes, and three actual
server/client cases cover both text interfaces plus checkpoint restart. Adventure
retains its established disclosed-order choice for indistinguishable stacks;
direct commands provide opaque-target choices for ambiguity.

ASCII now uses W/T/Q for equip/remove/drink through the shared client item
choices and action validation. Equipment choices stay selectable when sockets
are occupied so selection explains the refusal. Selections preserve disclosed
order, ignore quantity text, and clear on cancellation, observation changes,
control loss and disconnect. All 37 ASCII integration tests, five renderer tests
and the native-key mapping test pass. A native keyboard process test verifies
equipment timing, removal, cancelled potion selection, one-unit consumption and
a presented framebuffer. The shared candidate/socket test also passes.

Completion is now exposed in the observer's interaction snapshot and narrated
through `client-common`, including noncombat equipment and consumed final units.
Only the owning actor receives these item completions. Consecutive unchanged
observations do not repeat narration. A lethal final-unit potion without anatomy
still reports completion, covered by a focused simulation test. Current wire
samples include an actual item-completion snapshot. The affected integration
suites pass (99 simulation, 51 protocol, 87 text and 37 ASCII), followed by all
11 focused interaction simulation cases after adding the lethal final-unit case.
Seven shared narration tests and two server interaction tests pass.
All four actual-client interaction process tests pass, including native ASCII
completion narration after the final potion unit disappears and checkpoint
restart through the server/headless client.

Next: add
knowledge-limited AI item decisions and final performance/desktop/CI closeout.
Active item work and equipped gear already pass frozen and detached checkpoint
round trips, including completion after reattachment.
