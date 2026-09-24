# Session handoff — 2026-09-24

Phase A and its pre-release cleanup were merged in
[PR #22](https://github.com/KCall360/thresholds-of-ruin/pull/22), after Windows and
Linux CI passed. Main's merge is `5745e0f219d90900b7d8260ceb2ef40ce6c0d5a0`.
The user then authorized Phase B implementation with a revised saving contract:
performance takes priority over making each acknowledgement durable. Recent
acknowledged play may be lost on a crash. Save timing must be configurable and
opportunistic. The [roadmap](milestones.md) remains the status source of truth.

## Current work

Workspace: `F:\Codex\Roguelike`. Branch: `codex/background-save-journal`.
This is the publication checkpoint. The user authorized committing and pushing
all completed work, then merging only after Windows/Linux CI pass on the final
PR head. Check the live PR for this branch for publication and merge status;
do not infer that it still needs merging from this checkpoint document.

Read [background saving](background-saving.md) for the implemented contract:

- Protocol 12, save format 4, ruleset `diagonal-v11`. Old saves are rejected;
  no importer, historical rules behavior, or compatibility defaults were added.
- Ordinary commands publish after bounded in-memory journal admission. Only
  their new record is encoded. A worker owns SQLite batch I/O outside the
  engine/session lock; timing uses target age, idle opportunity, maximum age,
  and queue pressure.
- Explicit save, normal player-client exit, graceful server shutdown, and wizard
  enablement retain durability barriers. Spectators cannot save. Saving does not
  block other clients from acting.
- SQLite supplies atomic transactions and interrupted-batch recovery. Versioned,
  checksummed rows add application schema and save-identity validation. Failed
  saves retain pending data, warn clients, reject further mutation, and allow
  an explicit retry. Committed corruption fails closed.
- Full replay, retained history, and state-copy costs remain. Application
  checkpoints, rotation, and compaction are **not implemented** (Phase C).

The [Phase A review](persistence-review.md) is historical evidence. Its proposed
synchronous acknowledgement and separate-file installation scheme is superseded.
SQLite barriers depend on filesystem/device behavior; process-kill tests do not
prove power-loss durability, and new parent-directory durability is not separately
established by the application.

## Verification and measurements

Phase A raw results remain unchanged in `docs/measurements/phase-a-2026-09-24/`.
Phase B uses the [focused subset](performance-persistence.md#phase-b-verification-and-measurement):
eight-region cases at 100/10,000 actions and one/eight actors, matched memory
cases, one 256-region/eight-actor/10,000-action saved case, and an actual-client run.
The worker's application byte counts are not physical SQLite I/O measurements.
See [Phase B findings](phase-b-findings.md) and the retained manifest/raw samples.

Storage and process tests cover unsaved-tail rollback, explicit save, shutdown,
queue admission, failed saves/retry, strict frame validation, immutable prior rows,
receipts, private notes, retained branches, and permanent wizard marking.
Ordinary restart test helpers now save explicitly; dedicated crash tests bypass
that helper and kill without saving.

Local logs and launcher helpers remain ignored under `.local/`. Existing
`target-phase-a/`, `target-playtest/`, and prior saves remain untouched. Do not
commit credentials or local playtest output. Follow `CONTRIBUTING.md` for final
checks, launcher verification, and Windows/Linux CI before any future merge.

## Completed local checks

- All 200 Rust tests pass in debug and release.
- Workspace Clippy, formatting, architecture boundaries, warning-free rustdoc,
  documentation links/index, and diff whitespace checks pass.
- The full debug Python suite passed 67 of 68 tests before the final reconnect
  regression was added; all six background-save process tests then passed on the
  final implementation. The release frontend suite passed 57 of 58 tests,
  including the new reconnect regression.
- Both Python suite failures are the unchanged native mouse test: the sandbox
  denies `SetCursorPos`. An elevated debug rerun reached that API but another
  window covered the click target (`WindowFromPoint` mismatch). This is not
  recorded as a pass; Windows/Linux CI must validate it before any merge.
- All four desktop launchers passed real connection, fresh-save retention, and
  owned-process cleanup checks. The 256-region demonstration completed at least
  three cycles before its spectator window was closed and cleanup verified.
- The focused release benchmark validated nine cases and 13,798 ordered attempts.
  The separate actual-client run completed three cycles and 205 accepted actions.

## Next work

Check the branch PR and fetch `main` before starting more work. If the PR is
already merged, continue from updated `main`; do not repeat publication or merge.
If it is open, the user has authorized resolving CI failures and merging after
Windows/Linux CI pass on its final head. Stop before Phase C unless the user
broadens scope. Local credentials, saves, and diagnostic logs remain ignored.
