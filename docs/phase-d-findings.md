# Phase D findings — 2026-09-25

Phase D removes retained-history copies from ordinary command transactions and
shares immutable state across rewind boundaries. It preserves protocol 12, save
format 5, ruleset `diagonal-v11`, deterministic outcomes and queue-before-publication
semantics. These are backend ownership and observation changes, not new game rules.

## Implementation and operation contracts

A candidate owns game state, revisions, the current branch and at most 128 shared
rewind boundaries. It cannot own a journal, receipt index or storage handle. Only
the new record enters the save queue; successful admission precedes publication
and receipt insertion. Queue rejection leaves state, revisions, history, branches
and request identity unchanged. Backend annotations use the same admission path.

World maps and items use copy-on-write storage; doors detach independently of
terrain and topology. Actor navigation shares maps by source region. Updating a
local view checks visible connections rather than scanning all remembered edges,
and copies only changed regions. The outer region map and actor metadata still
have scale-dependent work; this is not a claim of constant-time game operations.
Ordered iteration and the strict flat checkpoint encoding are unchanged.

Waits affect only time and readiness and construct no observations, scenes or
navigation. Other actions conservatively compare all actors. Each actual
observation constructs one scene reused for surfaces, door approaches, revision
comparison, protocol adaptation and navigation where applicable. No long-lived
observation cache or speculative client state is introduced. Exhaustive action
impact matches require new actions to make a perception/navigation decision.

## Measurement method

The focused release run uses the unchanged version-1 fixture at seed 42, three
mixed cycles, no warmup, checkpoint interval 1,024 and save target/max/idle ages
10/50/1 ms. Nine cases cover memory/durable mode, one/eight actors, 100/10,000
retained actions at eight regions, plus 256 regions/eight actors/10,000 actions in
durable mode. Every case validates ordered attempts, save accounting and normal
restart disclosure. Separate eight- and 256-region traversals grow remembered
maps through 77 and 2,805 accepted actions respectively.

The reference is the merged Phase C engine at `cea088c`, built with only the new
benchmark selectors/metadata copied into its harness. Reference and final release
measurements ran on the same Windows machine and F: NTFS HDD with no competing
build or test suites. Normal desktop activity was not controlled. Machine timing
is diagnostic; operation counts and behavior assertions are the CI contracts.

The initial development baseline overlapped compilation and is retained for
transparency, not used for the final comparison. An initial optimized run exposed
whole-navigation copying on discovery; its samples are also retained. The final
implementation shares navigation by region to address that measured cost.

## Matched release results

Each latency cell is **p50 / p95 / maximum**, in milliseconds. The mixed set contains
8,289 ordered attempts per pass (including deliberate blocked attempts); successful
sample counts appear in the table. Discovery adds 2,882 successful actions.

| Case | Successful actions | Phase C reference (ms) | Phase D final (ms) |
| --- | ---: | ---: | ---: |
| r8-a1-h100-memory | 183 | 0.595 / 1.197 / 1.726 | 0.330 / 0.836 / 1.062 |
| r8-a1-h100-durable | 183 | 0.594 / 1.253 / 1.957 | 0.322 / 0.847 / 1.358 |
| r8-a1-h10000-memory | 183 | 9.042 / 10.213 / 18.080 | 0.312 / 0.845 / 0.966 |
| r8-a1-h10000-durable | 183 | 9.041 / 10.153 / 12.285 | 0.330 / 0.829 / 0.995 |
| r8-a8-h100-memory | 1503 | 2.657 / 3.584 / 4.562 | 0.012 / 1.918 / 2.946 |
| r8-a8-h100-durable | 1503 | 2.608 / 3.683 / 6.522 | 0.018 / 1.924 / 3.912 |
| r8-a8-h10000-memory | 1499 | 11.307 / 13.001 / 24.867 | 0.015 / 1.922 / 3.576 |
| r8-a8-h10000-durable | 1499 | 11.236 / 12.310 / 20.677 | 0.018 / 1.898 / 5.015 |
| r256-a8-h10000-durable | 1499 | 12.670 / 14.246 / 23.772 | 0.017 / 2.547 / 3.385 |
| traversal-r8 | 77 | 0.960 / 1.697 / 2.815 | 0.673 / 1.237 / 1.554 |
| traversal-r256 | 2805 | 6.892 / 11.772 / 19.114 | 1.055 / 1.771 / 3.166 |

The largest saved case improves from **14.246 to 2.547 ms p95**. Every selected
case stayed below the provisional 8 ms p95 and 33 ms maximum targets in this run.
There was no material upward trend from 100 to 10,000 retained actions. This is
evidence for the selected workloads, not a machine-independent latency guarantee.
The very low eight-actor medians reflect scheduled waits; per-action distributions
remain in the retained summaries.

Full discovery reaches 20,956 remembered cells. Before region sharing, its final
110 actions had median navigation work of 1.888 ms and publication/disposal of
0.992 ms; the final run removes that whole-knowledge copy. The initial optimized
256-region traversal had 4.252 ms p95; the final traversal has 1.771 ms p95.

| Saved case | Reference restart (s) | Final restart (s) | Final loaded / replayed |
| --- | ---: | ---: | ---: |
| r8-a1-h100-durable | 0.156 | 0.098 | 283 / 283 |
| r8-a1-h10000-durable | 1.888 | 0.305 | 10183 / 182 |
| r8-a8-h100-durable | 1.861 | 0.366 | 1603 / 579 |
| r8-a8-h10000-durable | 5.884 | 0.509 | 11499 / 474 |
| r256-a8-h10000-durable | 6.968 | 0.657 | 11499 / 474 |

Ordinary commands still report zero filesystem writes/syncs. Saved commands
serialize one new record regardless of retained history. Waits report zero
scene/observation/navigation calls; other observations report one scene per call.
Record and checkpoint bytes, all exclusive phases, client application/rendering,
restart counters and individual outliers are retained in the raw data.

## Verification and limits

Local workspace verification passes formatting, Clippy with warnings denied,
215 Rust tests in each of debug and release, warning-free rustdoc including private
items, architecture checks and documentation checks. Full Python discovery passes
77 tests, including the Windows native mouse test with desktop access. All 61
release actual-process tests also pass. The four desktop shortcuts passed actual
connection, fresh-save retention and owned-process cleanup checks, including three
completed cycles of the 256-region spectator demonstration.

Full-view tests compare optimized revisions and navigation with conservative
recomputation across the mixed trace, multiple actors and 256-region geometry.
A separate navigation oracle retains the original full-map scan and vector lookup,
checking doors, stale knowledge, geometry edits and route equivalence. Structural
sharing tests verify old snapshots remain isolated and unrelated regions/geometry
retain their storage. Rejection tests check state, receipts and rewind boundaries.
Existing checkpoint, retry, rewind, privacy, crash-recovery and actual-client
process suites continue to apply; see [checkpoints](checkpoints.md) and
[background saving](background-saving.md).

Non-wait actions still compare every actor. Item inspection, visited-place facts,
travel search, history queries and startup loading can still scale with their
inputs. Checkpoint encoding/size limits remain unchanged; the growing-discovery
trace is detached and does not establish bounded checkpoint size for an arbitrarily
large explored world. Client application and rendering still retain their own
costs; Phase E is next. Process tests do not prove hardware power-loss durability.

Reproduce with the [focused Phase D commands](performance-harness.md#focused-phase-d-and-growing-discovery).

## Retained artifacts

The [manifest](measurements/phase-d-2026-09-25/manifest.json) records hardware,
save volume, commands, source and binary hashes, and artifact hashes. The
[summary](measurements/phase-d-2026-09-25/matrix-summary.json) includes all phases,
labels, sample counts and recovery/save metrics.

- [Clean Phase C reference](measurements/phase-d-2026-09-25/reference.jsonl.gz).
- [Final Phase D run](measurements/phase-d-2026-09-25/final.jsonl.gz).
- [Initial optimized run before region sharing](measurements/phase-d-2026-09-25/optimized-initial.jsonl.gz).
- [Initial development baseline with competing compilation](measurements/phase-d-2026-09-25/baseline.jsonl.gz).

The [release actual-client run](measurements/phase-d-2026-09-25/actual-client.json)
uses 256 regions, eight scheduled headless clients and a native ASCII spectator.
It completed one full version-1 cycle and verified actor-specific frames and
server-enforced spectator behavior. Client acknowledgement/presentation timings
include transport and native processing; they are separate from authoritative
command timing and are not a completed Phase E responsiveness study.

### Actual-client tail investigation

The initial current-server run recorded a 738.439 ms presentation sample after a
16.818 ms acknowledgement. Consecutive [Phase C server](measurements/phase-d-2026-09-25/actual-reference.json)
and [current-server repeat](measurements/phase-d-2026-09-25/actual-repeat.json) runs
used the same frontend binaries, fixture and driver with no competing builds or
tests. Both completed 495 accepted actions, including 61 timed actor-1 presentations.

| Driver metric | Phase C p50 / p95 / max (ms) | Phase D repeat p50 / p95 / max (ms) |
| --- | ---: | ---: |
| Request to acknowledgement | 10.253 / 19.407 / 772.576 | 6.288 / 17.838 / 21.837 |
| Request to presented-frame report | 67.358 / 81.246 / 92.444 | 60.443 / 81.084 / 672.510 |

The repeat's presentation outlier occurred at a different move and had an 18.734 ms
acknowledgement. The reference also had an isolated 772.576 ms acknowledgement tail
on a secondary actor's wait. Presentation p95 did not materially regress, but the
current presentation maxima remain unfavorable and are not dismissed or claimed
fixed. The driver receives frame reports only after native presentation, diagnostic
PPM writing/flushing and JSON output; it cannot identify the exact stall origin.
The authoritative-only samples do not reproduce these tails. Isolating client,
diagnostic I/O and scheduling stalls belongs to Phase E; these measurements do not
establish its responsiveness gate or prove a particular cause for the outliers.
