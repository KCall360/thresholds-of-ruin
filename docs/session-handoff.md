# Session handoff — milestone 4d complete locally

Updated 2026-09-27. The user authorized implementing 4d to completion.
Branch: `codex/milestone-4d-dungeon`, based on merged 4c PR #34 (`c8efdd5`).
Implementation is complete and committed locally. Publishing the branch and draft
PR is awaiting explicit user approval after automatic approval review rejected the
push as external source/history publication. No 4d PR exists yet. Windows/Linux CI
and review must pass before merge; neither is claimed by local verification.

The implementation and rule contract are in [dungeon gameplay](dungeon.md).
The ordinary launch default is `scenarios/first-dungeon`. Protocol 16, save 11,
ruleset `dungeon-v16`, validator `tor-scenario-4`; no prerelease save migration.
All 28 authored packages have LF source files and valid regenerated certificates.

## Verification and evidence

The full Rust workspace passed 307 tests in each debug/release profile, none
ignored. Formatting, all-target Clippy, private rustdoc with warnings denied,
architecture, documentation, and all package certificates passed. Evidence:
`.local/4d-workspace-{debug,release}-complete.log`,
`.local/4d-clippy-final-complete.log`, `.local/4d-rustdoc-final.log`,
`.local/4d-packages-lf.log`, `.local/4d-architecture-final.log`, and
`.local/4d-docs-final.log`.

Python debug discovery ran 120 tests: 116 passed, and four hit host temp/mouse
permission restrictions. The affected modules passed with a workspace-local temp
directory and desktop access (`.local/4d-python-debug-retry.log`). Full release
process coverage passed 80 tests (`.local/4d-python-release-qualified.log`) before
the late checkpoint/performance fixes; full Rust and targeted dungeon acceptance
were rerun after those fixes.

The final dungeon acceptance suite passed all four scenarios in both profiles
(`.local/4d-dungeon-{debug,release}-complete.log`), exercising the full default
dungeon, text retrieval/escape and saved victory, native combat, and saved death.
One preceding debug run timed out on the final text save. It passed in isolation
and in both subsequent complete runs; no definitive cause was established. The
test now preserves server diagnostics on a failed save barrier. The same run
exposed a misleading terminal `look` wait hint, which was fixed; text-client tests
and all-target lint were rerun (`.local/4d-text-closeout-checks.log`).

Native target-selection, victory, death, and control-hint screenshots were
inspected (`.local/4d-visual-qa/`). All three existing desktop launchers (ASCII,
Text, Text + ASCII Spectator) connect to the final release build, preserve fresh
saves, and clean up owned processes (`.local/4d-desktop-final.log`).

## Performance and next milestone

Long-history profiling caught an AI checkpoint JSON-key bug. Ordered visit entries
with duplicate rejection fix it; the forced-checkpoint restart regression first
failed, then passed. Attack visibility avoids unrelated observation fields,
unchanged ticks avoid duplicate validation, and unchanged scenes avoid navigation
refresh. Stable operation-count regressions pass. Accepted combat evidence is
`.local/4d-combat-navigation.jsonl` and its `-summary.json`.
Two-actor p95 is below 4 ms; eight-actor p95 remains about 11.7 ms, dominated by
all-actor before/after observations. This is recorded in deferred 3p, with the
8 ms target unchanged. Full measurements and limitations are in the dungeon guide.
The matching 4c/4d ordinary workloads also passed: command p95 changed from
0.946 to 0.964 ms (small) and 3.389 to 3.410 ms (large). Evidence is
`.local/4d-complete-comparison.json` and the matching JSONL files. Failed and
pre-optimization evidence remains local; the save timeout excerpt is preserved in
`.local/4d-save-timeout-investigation.md`.

Next milestone is 4e. Its plan must account for frozen attack progress/recovery,
AI memory and visit locations, motion and pending effects, stable item/objective
references, and deterministic reactivation. Current checkpoint validation assumes
referenced locations exist in the loaded world; define how inactive regions retain
those references before changing that invariant. Preserve the ordinary 4d dungeon,
checkpoint/retry/rewind, disclosure, and native-client acceptance fixtures.

Use `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, UTF-8 Python, and workspace-local
temp files on this Windows host. Avoid editing Rust inputs during builds. Native
mouse tests need desktop permission. System Python lacks Pillow; the bundled
runtime Python runs `.local/4d-visual-qa.py`. Local saves, logs, and screenshots
remain outside Git.
