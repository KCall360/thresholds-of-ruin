# Performance harness completion plan and session handoff

Status: planned, 2026-09-23. This plan describes the remaining work to complete
Phase A of [milestone 3p](performance-persistence.md); it does not authorize Phase B.
The user requested both focused measurements and a mixed workload, with a
256-region ASCII spectator demo showing the same actions that are profiled.

## Current implementation and gaps

The current `crates/server/examples/latency_bench.rs` constructs connected worlds
of 1, 8, 64, and 256 regions, with 1 or 8 actors. Its timed simulation actions
are waits followed by navigation refresh. Static perception queries and merely
placing walls, a door, or two z-levels do not establish that a workload crosses
portals, changes LOS, operates doors, or uses stairs. Door placement failure is
currently ignored, and two levels have no explicit vertical links.

History cases use a separate two-room scenario. They seed waits, time 100 memory
commands, then perform one standalone archive save. Thus they neither cover the
combined world/history matrix nor produce distributions for ordinary durable
actions. The actual retained count at that save is the stated history plus 100.

Measurement corrections required before drawing further conclusions:

- `revision_detection` currently includes after-action perception already counted
  in `perception`. Make phases exclusive, or explicitly label nested inclusive
  totals; never sum overlapping values.
- Time navigation refresh separately. It is combined with simulation in world
  samples and absent from server phase reporting.
- Retain individual phase samples, not only phase means. Report mean, p50, p95,
  maximum, and sample count by action type and for the mixed trace.
- Measure complete durable commands, plus serialization, actual write/flush,
  file sync, replacement, and publication independently. The current sync label
  also contains file replacement. Do not change the writer to simplify profiling:
  Phase A previously replaced buffered streaming serialization with `to_vec`;
  review that alteration and measure the intended production path consistently.
- Exclude harness-owned client clones/update construction from application timing.
  Include growing disclosed memory and sequential updates in client samples.
- Verify fixture-seeded histories against normal execution/replay; keep seeding
  outside timed regions. Diagnostic helpers must not bypass save locking or allow
  an attached engine to publish undurable state.

The local demo currently sends 24 east moves and 24 waits without pacing, starts
its bot before the spectator, and does not validate outcomes. Its actor starts
at y=1 while passages are at y=8, so those east moves do not cross regions. The
server's `--regions` world is also a separate constructor without the benchmark
door. Starting three processes is insufficient verification of visible behavior.

## Shared deterministic workloads

Define one versioned fixture/trace specification consumed by the benchmark and
the actual-client driver. Keep orchestration in test support/examples/scripts;
simulation remains free of clocks, filesystem access, and client dependencies.
Use headless clients for scripted player actions and authorized wizard setup.
If wizard setup is required, retain the permanent wizard marker and separate
credentials; setup is excluded from timed ordinary actions.

Each step specifies a stable label, actor, ordinary action, expected outcome,
and observable assertions. Server-side tests may additionally inspect internal
locations and topology. Client driving must resolve door IDs and destinations
from disclosed observations and honor actor readiness, branch, and revision.
Unexpected rejection or blocked movement fails the run; deliberate blocked
actions are separately labeled and assert no tick/state advance.

| Focused workload | Required evidence |
| --- | --- |
| `wait_same_region` | Accepted wait, expected tick/scheduler change, stable geometry |
| `move_same_region` | Successful cardinal/diagonal displacement and correct movement cost |
| `cross_region_boundary` | Forward and reverse crossings; internal region change and valid disclosed offsets |
| `cross_boundary_with_los_change` | Crossing reveals and hides specified cells/entities, including a rotated join case |
| `move_near_obstacle` | Route around an occluder changes visibility; separately test a blocked attempt |
| `open_or_close_door` | Both door state transitions succeed and affect LOS/reach; include passage through the open doorway |
| `change_elevation` | Explicit up/down links are traversed and the observer scene changes |
| `multi_actor_visibility_change` | Scheduled actors enter/leave another actor's LOS and appropriate observations/revisions change |

Build a repeating mixed trace from successful interior movement, boundary
crossing, obstacle/LOS changes, door operations, elevation changes, and waits,
with scheduled turns for multiple actors. Return to a known repeatable geometry
and door configuration, then continue into another region. Record the exact
ordered steps and action mix; don't silently substitute waits for unsupported
behaviors. Run enough complete cycles to populate per-action percentiles and
measure accumulated navigation knowledge, history, and client memory.

## Matrix and output

Run applicable workloads across 1/8/64/256 connected regions, 1/8 actors, and
0/100/1,000/10,000 retained actions in memory and durable modes. Label boundary
workloads as not applicable for one region and multi-actor workloads as not
applicable for one actor. Provide the applicable interior/door/elevation mixed
trace for those cases. Keep the active local geometry/action mix comparable
when increasing distant world size, and separately exercise a traversal trace
whose discovered area grows. Do not claim flat scaling solely from stationary
actors in an otherwise large world.

Record starting and ending history lengths, seed, trace version, commit/build
profile, platform, region/actor counts, warmup, sample count, and storage mode.
For each action retain exclusive simulation, navigation, perception/LOS,
revision comparison, candidate capture, rewind snapshot, serialization,
write/flush, sync, replacement/publication, client application, and rendering
measurements where applicable. Also record total authoritative command latency
and actual-client request-to-ack/presentation latency; these measure different
boundaries. Account for remaining overhead instead of presenting phase sums as
complete end-to-end measurements.

Report operation counts at the real call sites (transitions, perception calls,
comparisons, clones/snapshots, records encoded, write/sync calls), bytes written
per action versus final save size, bounded rewind count, and restart/replay time.
Add allocation accounting only if it can be measured without materially changing
the workload. Preserve raw machine-readable samples so findings are reproducible.

## Stable verification and acceptance

Use test-driven development. First write failing behavior tests demonstrating
each promised crossing, LOS change, door transition, elevation transition, and
multi-actor scheduling outcome. Add a mixed-trace replay test comparing the
authoritative result and disclosed observations with ordinary execution.

CI should enforce exact coverage/counts, deterministic results, byte accounting,
and bounded ratios between comparable small/large cases. Distinguish documented
current defects (full-history encoding/cloning, all-actor observation) from
desired later-phase bounds; tests must allow improvements rather than require
the inefficiency forever. Cover every scale value with inexpensive structural
checks; run representative traces in ordinary CI and the full timing matrix as
a diagnostic release run. No absolute CI latency thresholds.

Recovery work also remains open: `recovery_fixtures.rs` is a toy in-memory scanner
using a payload-only FNV-like checksum, not CRC32C, with no sequence enforcement.
Its checkpoint test is a value-selection model, not an interrupted file test.
Add fault injection around actual current storage/publication boundaries and
prove failure preserves state/receipt identity and restart retry behavior. Specify
future append/checkpoint fault schedules without implementing Phase B storage.
Unit/process tests do not themselves establish power-loss durability; review the
required file and directory durability barriers on Windows and Linux explicitly.

Before closing Phase A, rerun release workloads and publish corrected bottleneck
measurements. Review the proposed frame/checkpoint design against failure-after-
write, failure-after-sync, acknowledgement loss, corruption, retry, and restart.
Stop at the Phase A findings and reviewed Phase B proposal unless further
implementation is explicitly authorized.

## Observable 256-region run and desktop maintenance

The fourth shortcut should run the same mixed trace at a human-visible pace,
with the real server, a hidden headless driver, and ASCII authenticated as a
server-enforced spectator. Wait for a confirmed spectator snapshot/presented
frame before beginning; process existence alone is not readiness. Await each
headless `ready` result and check errors, then pace outside measured action time.
Make active workload/step and progress visible through disclosed diagnostic
presentation or a status display, without leaking hidden world state.

Verify changing revisions, successful crossings, door/elevation changes, and
several complete cycles in the actual ASCII client. Separate this paced
demonstration from throughput measurements. Surface launch/bot failures, retain
logs and fresh saves, and clean up the server/bot when the window closes or
startup fails. Test the complete shortcut path including helpers and binaries.

After a build update, rebuild and verify the binaries used by all four desktop
shortcuts: Text, ASCII, Text + ASCII Spectator, and 256 Region Spectator. Text
controls the paired game; ASCII has a separate spectator credential. Preserve
fresh-save-per-launch behavior and prior saves. Re-read links, launcher/helper
paths, and icon targets. `cargo check` does not rebuild launchable executables;
explicitly build every required binary. See [development practices](../CONTRIBUTING.md).

## Next-session handoff (2026-09-23)

- Start with this plan, [the governing milestone](performance-persistence.md),
  and [the roadmap](milestones.md). Implement the Phase A corrections using TDD,
  then update measured findings; mixed workloads are still unimplemented.
- Source inspection was against commit `56fa06b` on
  `codex/remove-backward-compatibility`. GitHub confirms
  [PR #21](https://github.com/KCall360/thresholds-of-ruin/pull/21) merged on
  2026-09-22 at 14:13:07 UTC, with Windows and Ubuntu checks successful on that
  head. Those checks do not establish representative workload coverage.
- This plan is committed separately on `codex/performance-harness-plan`. Fetch
  current main and inspect ancestry before continuing/publication; do not try to
  update the already merged PR. The user authorized this planning commit and a
  handoff; no new chat or PR is needed just to save it.
- Inspect `git status` first. The headless/wizard documentation changes mentioned
  earlier are now committed in `56fa06b`. Untracked `target-phase-a/` and
  `target-playtest/` are generated build directories; do not commit them.
- Machine-local helpers in `F:\Codex\Roguelike\.local` are ignored and are not
  supplied by this commit. Existing three launchers mostly use `target\debug`;
  `run-text-player.ps1` still points at `target\doors\debug`. The fourth launcher
  uses `target-playtest\debug`; the shortcut installer still picks icons from
  `target\debug`. Reconcile these when rebuilding instead of assuming the
  earlier shortcut-verification claims cover every helper.
- The user declared the servers then running to be orphaned and authorized their
  cleanup. Check current PIDs and ownership before stopping newly encountered
  processes; don't assume future games are orphaned. No processes were launched
  or stopped for this planning update.
- Review additional discovered gaps during implementation: server `--regions`
  lacks a documented upper bound/help entry and maps one region to two; the
  architecture checker currently exempts *all* server dev-dependencies. Prefer
  test-support orchestration or explicit reviewed edges, with boundary tests.
- Run the documentation checks for plan edits. For implementation, run applicable
  behavior/process tests, formatting, Clippy, architecture checks, and release
  benchmarks; require Windows/Linux CI before a subsequent merge. Don't repeat
  the earlier claim that Phase A is complete until the acceptance above passes.
