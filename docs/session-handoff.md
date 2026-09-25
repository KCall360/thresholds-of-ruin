# Session handoff — 2026-09-25 milestone 3p closeout

## Starting publication state

Phase E merged in [PR #27](https://github.com/KCall360/thresholds-of-ruin/pull/27)
at `3783326ab3ba730b3c9994aedbff6cfd147f8799`, after Windows and Linux CI passed on
final head `9e059354b93b2b599d16ed00b7b7676c5e4ffc5d`. Verified through GitHub.
The closeout branch is `codex/milestone-3p-closeout`. Consult its live PR for final
publication and CI state; merge only after Windows/Linux CI pass on its final head.

Read [the closeout audit](3p-closeout.md), [milestones](milestones.md),
[the harness](performance-harness.md), [checkpoints](checkpoints.md),
[the performance plan](performance-persistence.md), [architecture](architecture.md)
and [development practices](../CONTRIBUTING.md) before continuing.

## Findings and next work

**Milestone 3p is not ready to close.** Two native runs cover 3,350 acknowledgements;
maxima are 26.421/28.172 ms and the previous acknowledgement spikes did not recur.
Their cause is not proved fixed. Presentation spikes accompany 300/436 ms optional
diagnostic-report stalls; phase timing separates these from small application/draw work.

Genuine saved 256-region exploration completes 2,805 actions and discloses 20,956
cells with checkpoints disabled; the 2.32 MB journal reloads exactly. Its final
checkpoint representation is 765,021,723 bytes, versus the unchanged 64 MiB cap.
Enabled traversal fails background saving under both default and stress checkpoint
intervals. Whole-navigation-map deduplication repeats overlapping knowledge across
rewind boundaries. This is a blocker in the existing fixture, not deferred streaming.
The audit records raw successful and failed samples, metadata, acceptance evidence,
limits, and the next step toward milestone 3 interactions/travel.

## Implementation and contracts

Opt-in `latency_bench --saved-discovery` attaches before movement, records save and
reload metrics, and validates complete saved-prefix/replay accounting. An offline
counting diagnostic measures current format-5 checkpoint JSON without allocating
its encoded payload. Tests compare the count to actual writer bytes and reload a
fully explored eight-region checkpoint. Failed runs remain rejected by the validator.
Existing workloads/profiling and Phase E timing instrumentation remain intact.

Protocol 12, format 5 and `diagonal-v11` are unchanged, as are all runtime mutation,
queue, resynchronization, disclosure, rewind and save contracts. No cap increase,
checkpoint disablement in ordinary play or format change is introduced. Credentials,
logs and fresh saves remain outside Git. The full matrix was not warranted for
this diagnostic/audit change; targeted enabled/disabled saved discovery is retained.

## Verification and publication

Local verification passes: 220 Rust tests in each of debug/release, all 87 Python
debug checks, all 64 release process tests, formatting, Clippy, architecture and
documentation checks, and warning-free private-item rustdoc. Debug/release binaries
were explicitly rebuilt. All four desktop launchers passed real connection, fresh
save retention and owned-process cleanup checks, including three completed
256-region demo cycles. Their helpers still target the rebuilt debug binaries.
See the closeout PR for Windows/Linux CI on its final commit. Prior Phase E verification and measurements
remain in [its findings](phase-e-findings.md); they do not establish large explored
checkpoint bounds. Do not mark 3p complete just because the closeout PR passes CI.

The next focused change must reduce checkpoint growth while preserving exact
recovery, rewind and supported saves, then pass complete saved exploration and
native input/save/restart checks with checkpoints enabled. Settle any compatibility
decision explicitly if the chosen representation requires it. Only then reopen
milestone 3 feature expansion. Query scaling, streaming, linear history loading,
optional diagnostic I/O and hardware power-loss qualification have separate,
explicitly deferred dispositions in the audit.
