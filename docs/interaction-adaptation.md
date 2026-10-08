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
client item checks. AI known healing, gear replacement and safe nearby looting
are implemented.

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

AI now prioritizes a carried known restorative at half health or below, using the
AI actor's own identity knowledge and accepting only wholly positive healing
sequences. Five focused AI tests pass, including human/AI knowledge isolation,
mixed harmful effects and healing before flight. A checkpoint integration test
passes for active autonomous consumption and one-unit completion. Two real-server
process tests pass for known healing/restart and leaving an unknown potion
untouched when only the human knows it. The new `ai-interactions` fixture has a
current validation certificate.

AI chooses known gear that strictly improves its combat attributes without
trading away attack speed, damage kinds or typed protection. It uses the actor's
anatomy, fills free matching sockets before replacing occupied ones, and removes
old gear through ordinary timed work before recomputing an upgrade. It avoids
gear work while fleeing or adjacent to a visible hostile. Eight AI decision tests
pass for healing and gear, including duplicate rings and unknown equipped items.
Three real-server process cases pass, including completed armor replacement,
duplicate ring sockets, unidentified gear and equipment persistence after restart.
A simulation checkpoint test passes midway through autonomous armor removal,
then completes removal and equips the upgrade after restoration. The existing
many-target test still verifies one topology search per AI decision.

Nearby looting considers only visible, known useful gear or a known restorative
when none is carried. It chooses a safe candidate within three remembered route
steps, ordered by distance and item ID, and checks both the destination and next
step against visible hostile reach. It takes one unit and reuses the decision's
existing route search. Five real-server AI process tests pass, including travel
to ground armor followed by timed replacement and one-unit healing pickup with
the remaining stack intact. Focused simulation tests cover the range, unsafe
first-step alternatives, own-actor knowledge and one-search invariant; a
checkpoint case preserves split quantities and avoids collecting spare healing.

Next: finish performance/desktop/CI closeout.
Active item work and equipped gear already pass frozen and detached checkpoint
round trips, including completion after reattachment.

The first PR CI run exposed certificates generated from local CRLF manifest
bytes. Source hashing now automatically normalizes CRLF to LF in validation,
lazy region integrity checks and in-memory construction; other source edits
remain significant. All 35 package certificates have been regenerated. The new
CRLF/LF regression failed before the fix and passes afterward. All 44 package
unit tests, seven package integration tests, 17 package process tests, three
repository certificate/reference checks and seven documentation checks pass.
The process regression validates CRLF sources, starts from an LF checkout and
restarts a saved game with CRLF sources. Generated package text remains LF.

## Closeout evidence

The release comparison against PR #87 (`28c5932`) measured feature commit
`98bc012` in three interleaved baseline/current rounds, with all 30 runs passing
their respective validators. Item and client display tables were expanded from
the SHA-verified retained samples to include their actual measured groups.
The machine fingerprint is `6a1878811f37`: i7-9750H, 15.8 GiB RAM, Windows 11
build 26200, NTFS on ST1000LM035 HDD, Rust 1.98.1. Timings are diagnostic;
each row below uses the same machine and release profile. Intervals are ms.

| Case and interval | n per side | Baseline p50 / p95 / max | Adapted p50 / p95 / max |
| --- | ---: | --- | --- |
| 8 regions, 1 actor: authoritative turn | 915 | 0.542 / 0.791 / 0.921 | 0.541 / 0.782 / 2.084 |
| 64 regions, 8 actors: authoritative turn | 7,500 | 0.030 / 2.544 / 5.619 | 0.031 / 2.573 / 6.601 |
| Combat, 8 actors, 1,000 history: AI decision | 576 | 0.0005 / 0.564 / 1.161 | 0.0004 / 0.558 / 0.813 |
| 1,000 items, 256 identities: transfer | 1,200 | 0.402 / 0.531 / 1.083 | 0.394 / 0.527 / 1.053 |
| Client, 20,956 cells, burst 64: render | 60 | 2.413 / 2.783 / 3.019 | 2.361 / 2.839 / 3.049 |

Operation counts are unchanged. Wire/save byte counts change with physical
classes, preparation and interaction data: ordinary sent-envelope totals rise
by 11,625 bytes per round in each case; the large item case's disclosed total
rises from 5,008,820 to 5,348,820 bytes and its saved total from 18,513,920 to
21,053,440 bytes. Combat save totals also increase, while the small item case's
saved total decreases. No timings or byte-size targets are relaxed. Raw samples
remain outside Git pending maintainer approval for measurement publication.

Release binaries for the server, scenario utility, text, ASCII and headless
clients were rebuilt at `4d44627`. The existing three desktop shortcuts were
updated from the old checkout to the current workspace. All three pass actual
connection and output checks; ASCII reports presented frames, and the combined
launcher confirms a read-only spectator with a separate credential. Each launch
creates and retains a fresh save, preserves prior save hashes and cleans up its
own processes. Credentials, saves and launcher helper scripts remain outside Git.
The focused 16-region memory streaming comparison at `4d44627` also passes all
six runs in three interleaved rounds. For 2,100 authoritative intervals per side,
baseline p50/p95/max is 0.221/0.378/0.623 ms and adapted is
0.217/0.376/0.602 ms. Operation counts are unchanged; sent-envelope totals rise
from 1,250,079 to 1,252,254 bytes per round. An earlier unsupported 8-region
invocation was rejected on both sides before measurements; its six failed runs
are retained separately and excluded from these findings.
Final CI is still pending in
[PR #88](https://github.com/KCall360/thresholds-of-ruin/pull/88).
The release CI run also identified an obsolete version-22 positive save-schema
fixture. Current version-24 metadata is now captured from the production writer,
including interaction and AI packages, and checked against its fixed golden
fixture. The old package records remain rejection cases. All 251 server unit
tests pass locally after this update; no compatibility reader was introduced.
Ledger preparation exposed a conversion bug for the streaming workload's
`streaming-v1` identifier. The import now separates its name and numeric version
without relaxing ledger validation. All 44 focused comparison/ledger tooling
tests pass, including an actual CLI regression and malformed identifier checks.
