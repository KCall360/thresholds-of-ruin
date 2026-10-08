# Adapting the unpublished interaction work

The retained interaction features use the accepted shared TOR architecture.
The complete original checkout is preserved on the local archival branch
`codex/milestone-4f-interactions`. It contains obsolete formats, fixtures and
documentation and is retained as reference material.

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

## Retained behavior and verification

Physical item classes and ASCII glyphs use disclosed data and stale item memory.
Preparation belongs to the actor, preserving attack timing, saved intention
identity and recovery lineage while supporting equipment and consumption.
Anatomy defines matching sockets, including duplicate rings. Starting gear and
completed equipment affect combat without mutating base rules; armor protects
until removal completes, and death drops carried and equipped items once.
Shared healing/damage effects run at consumption completion, consume one unit
and identify it only when effects are observable. See [items and knowledge](items.md).

Text, adventure and ASCII share carried-item affordance checks, candidate choices
and anatomy validation through `client-common`. They choose free matching sockets
from disclosed data and require removal before replacement. Direct text commands
offer opaque choices for ambiguous items; adventure retains disclosed-order
choices for indistinguishable stacks. ASCII uses W/T/Q for equip/remove/drink.
Selection clears on cancellation, changed observations, control loss and
disconnect. Shared completion narration handles noncombat equipment and the last
consumed unit, belongs to the owning actor and does not repeat stale observations.

AI uses its own identity knowledge. At half health or below it prioritizes known
restoratives whose effects are entirely positive healing. Known gear must strictly
improve combat attributes without losing attack speed, damage kinds or typed
protection. AI fills free matching sockets before replacing gear, removes old
gear through ordinary timed work and recomputes the upgrade afterward. It avoids
gear work while fleeing or adjacent to a visible hostile. Nearby looting considers
visible, known useful gear or a restorative when none is carried, within three
remembered route steps. Candidates are ordered by distance and item ID; both the
destination and first step must avoid visible hostile reach. AI takes one unit
and reuses its existing route search.

Local tests cover preparation identity, interruption and resumption; concealed
statistics, observable identification and atomic effect sequences; duplicate
sockets, effective combat and death drops; action admission; and frozen/detached
checkpoint restoration. Affected integration suites pass 99 simulation,
51 protocol, 87 text and 37 ASCII tests at the client-completion checkpoint.
Subsequent AI checks pass 24 focused unit tests and 14 affected integration tests,
including the one-search-per-decision invariant.

Actual text, headless and native ASCII process tests cover equipment timing,
removal, opaque choices, cancellation, final-unit consumption, completion
narration, presented frames and checkpoint restart. Five AI process cases cover
known healing, actor-specific knowledge, armor replacement, duplicate rings and
safe one-unit pickup with the remaining stack intact. Current wire samples were
recorded through real processes.

Package source hashing automatically normalizes CRLF to LF during validation,
lazy region checks and in-memory construction. Other edits, including comments,
still invalidate certificates; generated text uses LF. All 35 packages have
current certificates. The regression validates CRLF sources, starts from an LF
checkout and restarts a saved game with CRLF sources. Local package checks pass
44 unit, seven integration, 17 process and three certificate/reference tests.

## Closeout evidence

The release comparison against the accepted architecture baseline used three
interleaved baseline/adapted rounds, with all 30 runs passing
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
are published as [performance release assets](https://github.com/KCall360/thresholds-of-ruin/releases/tag/perf-4f-adaptation-20261008).
Twelve baseline/adapted headline records are in the
[performance ledger](performance-harness.md#performance-ledger), with exact asset
hashes verified against downloaded copies. Raw samples remain outside Git.

Release binaries for the server, scenario utility, text, ASCII and headless
clients were rebuilt for the current implementation. The existing three desktop shortcuts were
updated from the old checkout to the current workspace. All three pass actual
connection and output checks; ASCII reports presented frames, and the combined
launcher confirms a read-only spectator with a separate credential. Each launch
creates and retains a fresh save, preserves prior save hashes and cleans up its
own processes. Credentials, saves and launcher helper scripts remain outside Git.
The focused 16-region memory streaming comparison after source-hash normalization
also passes all
six runs in three interleaved rounds. For 2,100 authoritative intervals per side,
baseline p50/p95/max is 0.221/0.378/0.623 ms and adapted is
0.217/0.376/0.602 ms. Operation counts are unchanged; sent-envelope totals rise
from 1,250,079 to 1,252,254 bytes per round. An earlier unsupported 8-region
invocation was rejected on both sides before measurements; its six failed runs
are retained separately and excluded from these findings.
Final Windows/Linux debug/release CI remains pending. Synthetic relay item
fixtures include the current physical-class field, so recovery tests reach their
intended quantity, uniqueness and retained-size checks. The focused text-client
process regression failed before this correction; all 20 affected stream recovery
process and relay tests pass afterward, with their repair assertions intact.
Current saved scenario metadata is captured from the production writer,
including interaction and AI packages, and checked against its fixed golden
fixture. The old package records remain rejection cases. All 251 server unit
tests pass locally after this update; no compatibility reader was introduced.
Ledger preparation exposed a conversion bug for the streaming workload's
`streaming-v1` identifier. The import now separates its name and numeric version
without relaxing ledger validation. All 44 focused comparison/ledger tooling
tests pass, including an actual CLI regression and malformed identifier checks.
