# Milestone 3p explored-world checkpoint reduction

This focused change removes the explored-world checkpoint-size blocker without
raising the 64 MiB cap or disabling checkpoints. It adds no interactions or travel
features. The remaining closure disposition is recorded below.

## Publication and compatibility

PR #28 merged at `eff107ce95b7c0f6c8f813597d0fcb94f70fefa5` after Windows and Linux
CI succeeded on final head `61372ee1958346a90fe423eb75b6e16f182951cc`; GitHub was
queried before implementation. The user explicitly chose save format **6**, with
format-5 rejection under the existing pre-release policy, before implementation.
Protocol 12 and ruleset `diagonal-v11` remain unchanged. No compatibility reader,
migration, checkpoint bypass, replay-validation relaxation, or cap change is added.
Old saves remain on disk but require their old executable; new launchers still
create fresh saves.

## Representation and regression

Format 5 stored each distinct whole navigation map in full. Retained exploring
boundaries share most knowledge but differ locally, so whole-map equality does
not remove overlap. Format 6 pools equal cell and edge maps by source region,
then stores ordered region references for each navigation instance. Deduplication
runs on the existing storage worker; capture, queue admission, publication and
transaction boundaries are unchanged. Decoding shares immutable region maps,
with copy-on-write updates preserving each boundary's exact knowledge, including
changed cells and removed edges. Clients receive the same disclosed protocol data.

The full saved-discovery regression first failed on the old representation at
765,021,723 bytes. It now requires both eight and 256 completely traversed regions
to fit below 16 MiB, writes a real checkpoint, compares counting/writer bytes and
restarts. Additional tests compare complete game/navigation and revisions at every
retained rewind boundary, validate shared ownership and deletions, and reject bad
indices, duplicate/out-of-order references, malformed region tables and fields.
Existing rewind, branching, retry, corruption, queue and crash-recovery suites
remain required. This is a bound on the existing fixture, not arbitrary-world
streaming or a bound on every intermediate allocation.

## Targeted release comparisons

Runs use the same Windows machine and F: NTFS HDD as the prior closeout. The old
release executable is retained separately before rebuilding. Fixture version 1,
profiling version 2, action ordering and validators are maintained. Measurements
run sequentially without competing builds/tests; ordinary desktop activity and
OS scheduling are uncontrolled. Save policy is 10/50/1 ms target/maximum/idle.
Raw samples, binary hashes and commands accompany the final measurement manifest.

Final-state checkpoint JSON falls from 7,457,174 to **1,051,196 bytes** at eight
regions and from 765,021,723 to **9,920,494 bytes** at 256 regions (98.7% reduction).
The count includes all retained rewind boundaries. It is an offline diagnostic,
not necessarily the last asynchronously selected checkpoint.

| 256-region saved traversal | p50 / p95 / max action ms | Selected checkpoint / tail records | Flush / restart ms |
| --- | --- | --- | --- |
| Before, checkpoints disabled | 1.053 / 1.782 / 5.505 | none / 2,805 | 874 / 3,222 |
| After, checkpoints disabled (comparison only) | 1.045 / 1.749 / 3.265 | none / 2,805 | 498 / 3,178 |
| After, interval 64 | 1.040 / 1.736 / 3.075 | 2,752 / 53 | 1,189 / 1,737 |
| After, interval 1024 | 1.050 / 1.781 / 3.847 | 2,048 / 757 | 442 / 2,178 |

All completed rows contain 2,805 actions and 20,956 disclosed cells. Both enabled
runs restore exactly. The final selected checkpoint is 9,806,500 bytes at interval
64 and 7,840,164 bytes at interval 1024, with last worker encodings of 58/49 ms.
The respective database files are 21,630,976/14,524,416 bytes; physical database
size includes reusable pages and retained history. Eight-region interval-64 restart
falls from 431 to 90 ms, with checkpoint size 5,268,464 to 890,141 bytes. Neither
hardware power-loss durability nor constant-time history loading is claimed.

The fresh old-binary interval-64 run again fails during large exploration and is
retained as a rejected partial run. It must not pass the completion validator.
The representative eight-actor/10,000-history mixed trace has 1,499 accepted actions
per run. p95 is 1.881/1.883 ms before/after; a 9.404 ms after-sample occurs in
navigation (5.555 ms) and perception (3.723 ms), with zero checkpoint capture and
command-path I/O. The focused repetition and native results are recorded below.

## Closure and next step

Checkpoint-enabled complete exploration, exact recovery and bounded tail replay
are now directly measurable rather than blocked by the old representation.
Final native acceptance and verification results must be recorded before
publication. Historical acknowledgement spikes are not proved fixed by a size
reduction; non-reproduction alone does not close that separate evidence item.
Milestone 3 feature expansion remains paused until the explicit 3p closure
assessment resolves that item. Then resume the documented shared resumable-action
extension points, durable place knowledge, semantic narration/interruption and
slow-client resynchronization acceptance for interactions and travel.
