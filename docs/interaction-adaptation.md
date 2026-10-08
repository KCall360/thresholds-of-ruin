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
saved intention identity. Equipment, consumables, AI item actions and generalized
work variants are not implemented on this branch yet.

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

Next: generalize preparation within the current saved-intention and
region-lifecycle architecture, then add anatomy-based equipment and initial
shared consumable effects.
