# Session handoff — milestone 4c complete locally

Updated 2026-09-27. The user approved the physics proposals and requested all of
4c before 4d. The implementation is on `codex/milestone-4c-physics`, ready for
review and Windows/Linux CI. No merge is authorized yet. The [roadmap](milestones.md)
remains the status source of truth.

## Delivered

See [physics](physics.md): rigid cell bodies, 24 cube rotations, portal straddling,
averaged local gravity, fixed-point drift and terminal speed, support/sliding,
actor/item integration, inherited dropped-item motion, impact hooks, authoring and
wizard inputs, own-body disclosure, height browsing (ASCII F6/F7), and exact
checkpoint/replay/retry/rewind state. The main plane retains beveled doorway sight;
height slices use conservative voxel rays. Frame-aware surfaces and stair landings
remain correctly oriented. Resting waits and same-tick handoffs avoid scene work.

The default playable character is two cells tall. Crouching/ducking and posture
changes remain deferred. No HP damage, combat, AI, death, or victory is added; 4d
owns the first complete dungeon loop. Protocol 15, save/SQLite format 10, ruleset
`physics-v15`, and validator `tor-scenario-3` intentionally reject older prerelease
formats. All 24 authored packages validate, including physics and sideways-portal
acceptance packages. Gravity tables share world geometry across action/rewind state.

## Verified locally

- Rust: 276 tests each in debug and release, no failures.
- Python: 114 tests in the full debug suite; 76 release process tests, all passed.
- Formatting, all-target Clippy, private rustdoc with warnings denied, architecture
  boundaries, documentation links, and all authored-package validation passed.
- Real text/ASCII clients cover falling, landing, rotated crossings, body cells,
  height browsing, blocked movement, saved continuation, and existing interactions.
- All three desktop launchers connected with fresh saves and cleaned up only owned
  processes. Final native captures were visually inspected.

Final debug tests used `CARGO_BUILD_JOBS=2` and `CARGO_PROFILE_DEV_DEBUG=0` to avoid
Windows linker/paging pressure; debug assertions remained enabled. Release used
the ordinary optimized profile. Earlier failed logs retain build contention,
native access/startup failures, and the fixed doorway and benchmark regressions;
final application suites ran sequentially after builds completed.

Local evidence is retained under `.local/`: `4c-rust-{debug,release}-publish.log`,
`4c-python-{debug,release}-publish.log`, `4c-clippy-publish.log`,
`4c-rustdoc-publish.log`, `4c-fmt-publish.log`, `4c-architecture-publish.log`,
`4c-docs-publish.log`, `4c-desktop-publish.log`, and `4c-visual-qa/`.
Normalized implementation hashes are in `4c-source-hashes.json` and are verified
before publication. Machine-local saves, tokens, logs, and captures stay out of Git.

## Performance and next steps

The [physics guide](physics.md#local-performance-evidence--2026-09-27) records
reproduction commands, sample counts, p50/p95/max, work/byte counts, and save/resume
measurements. Matched existing command p95 is 0.968/3.450 ms (small/large), versus
0.831/2.175 ms baseline. Body/frame-aware perception accounts for the remaining
increase; scene counts are unchanged. Both existing cases remain below the 8 ms
p95 target. Dense eight-actor falling still exceeds it and remains a documented
3p stress-case gap; no target was relaxed and full 3p closure is not claimed.

Raw final measurements are `.local/4c-final-*`; failed and earlier optimization
runs are retained separately. The preserved 4b binary hash and measurement
boundaries are recorded in the guide. Timing excludes transport/native presentation
and does not establish long-history scaling.

The merged 4a/4b main commit `17bab2f` has the same source tree as `818f2f4`;
publication rebases only the new 4c commit onto that merged main baseline. Review
the feature PR, require Windows and Linux CI before merging, and obtain user merge
authorization. Then proceed to 4d. Deferred 3p work remains open.
