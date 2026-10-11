# Wizard mode

The foundation is implemented: permanent game marking, separately authenticated
wizard authority, ground-item and actor placement, teleportation, and bounded
rewind with retained futures. Scriptable commands are available through the
headless client's structured request interface, the text client's developer
console, and the graphical ASCII client's F7 wizard command editor. These clients
display the marker and follow setup/rewind snapshots. The headless client is the
preferred frontend for scripted wizard setup and scenario-driving. In ASCII,
enter the command without the leading `wizard`, then press Enter to submit or
Esc to cancel. For example, `creature inspect 2` opens a scrollable private report.
[Geometry setup](portal-geometry.md) now adds room placement, rotated passages,
wall terrain and explicit vertical links. Richer item properties, enemy archetypes,
and scalable history remain later milestones.

## Start or promote a wizard game

Set distinct credentials in the server's PowerShell session:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
$env:TOR_WIZARD_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --wizard --listen 127.0.0.1:4000 --seed 42 --save saves/wizard.db
```

`--wizard` is an explicit server administration operation: it creates a marked
game or permanently promotes the existing save at that path, even if no command
is used. The marker commits before the listener opens. Both the flag and a valid,
distinct `TOR_WIZARD_TOKEN` are required. An optional `TOR_SPECTATOR_TOKEN` must
also differ. Invalid credential configuration fails before opening the save.

Set `TOR_SERVER_TOKEN` to the **wizard credential** and connect with the
headless client or start the text client normally. The server grants role
`wizard`; a player token cannot gain wizard authority through control ownership
or a frontend label. For headless use, send a branch/revision-checked
`Command::Wizard` through a `request` input; the server applies the same
authorization and validation as it does for text commands.
Wizard authority covers the whole game, including all actors and rewind of the
whole simulation. It does not require ordinary control; ordinary actions still
do. Only grant the wizard credential to a trusted developer of this game.
Spectators remain read-only, including same-user retries of privileged requests.

Restart without `--wizard` and without `TOR_WIZARD_TOKEN` to disable privileged
access. The save remains a wizard game and both frontends continue showing it.
The trusted library equivalent is `Engine::enable_wizard()` before constructing
the service; session accounts must separately have `AccessRole::Wizard`.

## Creature inspection

`wizard creature inspect <actor>` returns a private report only to its requesting
wizard. Inspection needs no control ownership and can inspect another loaded
creature. It preserves queued work, paid preparations and resource reservations;
it does not load an absent actor, pause the arena, advance ticks, or add a journal
entry. Invalid or stale input is rejected before returning a report.

The report includes species/default and initial attributes, effective type and
subtypes, skills/defenses/resources, active and dormant talents, template operations,
and each numbered HD's retained health seed, base health contribution, training,
attribute increase and talent choice. Effective grants are grouped by species,
type, subtype, class, template or active talent. Fear lists each causer and its
remaining active duration. Text prints these rows, ASCII opens a scrollable panel,
and headless emits the structured `creature_inspection` response. Normal history
and actor observations do not contain the report or its seeds.

## Text commands

The client forwards developer text without interpreting its geometry; only the
server parses these commands. Coordinates in this developer console are
region-local integers, with north decreasing y. The examples below assume the
`scenarios/two-room` package: two 5 by 3 by 2 interiors and an adjoining hall
stored with region 1, with stone shells extending one cell beyond the interiors.
Supported commands are:

| Command | Behavior |
| --- | --- |
| `wizard door 1 0 0 0 closed 2` | Place a closed door two cells tall (height defaults to 1); use ordinary open/close to interact |
| `wizard item token 1 1 1 0` | Place a copper token on the ground |
| `wizard item tablet 1 1 1 0` | Place a stone tablet on the ground |
| `wizard actor 75 1 2 1 0` | Spawn an ordinary actor with base recovery 75 ticks |
| `wizard teleport 1 2 1 1 0` | Teleport actor 1 to region 2, position (1,1,0) |
| `wizard arena pause` | Freeze an active arena while retaining queued work and paid reservations |
| `wizard arena resume` | Continue an active arena through ordinary scheduling |
| `wizard arena step [count]` | Permit one committed action by default, or a bounded count, then pause |
| `wizard combat capture on` / `off` | Enable bounded runtime combat capture, or disable it and clear retained records |
| `wizard combat inspect [through]` | Read up to eight private numerical resolution records, ending at the optional inclusive sequence |
| `wizard creature inspect <actor>` | Read a loaded creature's build, owned choices, source grants, Health and Fear without advancing or editing the simulation |
| `wizard creature add-hd <actor> <racial|warrior|mage>` | Append a hit die with a stable health seed and empty owned advancement slots |
| `wizard creature train <actor> <hit-die> <skill>` | Spend a training point owned by the named hit die |
| `wizard creature attribute <actor> <hit-die> <attribute>` | Use that die's attribute opportunity, available at every fourth total hit die |
| `wizard creature talent <actor> <hit-die> <talent>` | Select an eligible distinct talent in that die's unspent slot |
| `wizard creature remove-hd <actor>` | Remove the latest hit die and its owned choices; zero hit dice cause persistent death |
| `wizard creature template <actor> <template> <on|off>` | Apply or remove a named authored template through the shared rebuild rules |
| `wizard rewind initial` | Fork from the initial scenario, while that boundary is retained |
| `wizard rewind <entry-id>` | Fork from the state immediately after a retained action/setup entry |
| `history [before-id]` | Current branch history, including this user's wizard summaries |
| `branch-history <branch-id> [before-id]` | Read permitted entries on an abandoned branch |

New-rule games also support `wizard room`, `wizard connect` and `wizard wall`;
see [geometry setup](portal-geometry.md#authorized-developer-setup) for arguments and examples.

Placement/teleportation consume no ordinary action time. Teleport preserves
recovery times and reveals the destination to the relocated actor. New actors
are ready at the current tick and participate in stable scheduling; they are not
autonomous mobs. A wizard client can reconnect with `--actor <id>` to control
them. Ordinary accounts retain their configured actor allowlists.
Item properties and containment other than ground placement are not supported.
Unknown item kinds, zero recovery durations, invalid coordinates, and forbidden
actor overlap are rejected atomically.

The arena commands require an active arena. Step counts committed actions,
including preparation starts and resumes, rather than elapsed ticks. A manual
participant can still require player input. Pause and resume preserve active AI
preparation and its reserved finish cost across restart. `start_paused = true` in
the arena manifest supports controlled all-AI startup. These controls publish
ordinary readiness updates and keep their wizard history private.

Latest-HD removal uses the ordinary creature rebuild rules: injury is retained,
owned training, attribute choices and talents are removed, and losing an ability
grant cancels its preparation without refunding charges already paid. The edit
pauses an active arena and rejects changes to a stopped encounter. Exact retries
do not remove another hit die. Rewind can restore the earlier build and death
state. See [creature arena authoring](scenario-packages.md#creature-arena) for the
bundled encounter, bounds and scenario setup.

Template controls use the current package's validated catalog; JSON commands cannot
supply replacement definitions or forged grants. Unknown templates, already-applied
or absent templates, and conflicting structural changes reject atomically. Removing
a template removes only its source-owned contributions: a Mage retains its separate
class-provided magical capability. A lost ability grant cancels its preparation;
capacity reductions release unfunded holds and clamp balances. Increasing capacity
again does not refill it. Template edits preserve injury, inventory and sockets,
pause active arenas, and reject stopped encounters. Exact retries, restart and
rewind preserve the same build and resource accounting.

Advancement commands number hit dice from **1**, in the order shown by `stats`.
Racial dice own one training point; class dice own two. Only dice 4, 8, 12 and so
on own an attribute opportunity, with the ordinary attribute cap of five. Every
die owns a talent slot, and an older unspent slot can select from the whole
currently eligible build pool. Temporary benefits cannot qualify a talent.
Unknown names, invalid owners, overspending, occupied slots and ineligible choices
reject atomically. Adding a die preserves retained health seeds and choices; its
new seed comes from the actor's original spawn stream, independently of combat
randomness. Removing and re-adding the same ordinal uses the same seed and creates
empty choice slots. The limit is 256 hit dice.

Advancement preserves injury, does not refill enlarged resource pools, and never
revives a dead actor. Removing a prerequisite class level leaves a retained talent
choice dormant; restoring the requirement reactivates it. These commands share the
arena pause, stopped-result protection, retry, restart and rewind behavior of
latest-HD removal and template changes.

The server retains the most recent **128 decision boundaries across the entire
chronological journal**, including abandoned branches. The initial state counts
as one boundary until evicted; notes do not consume boundaries. Targets are the
state after an accepted ordinary action or wizard setup/rewind command, not a
wall-clock interval. An entry target must be visible to the attached actor/user.
Rewind cannot remove the requesting actor. Other clients attached to removed
actors disconnect explicitly. The private journal result records old/new branch, restored tick,
and next scheduled actor. History remains stored after a boundary expires, but
rewinding to that expired state is rejected. Active travel is cancelled by rewind.

Every rewind creates a fresh branch; it never erases the abandoned future or
changes existing note anchors. Current-branch history starts at the fork; use
`branch-history` for previous branches. Wizard entries are private to their
authenticated author and actor. Authorized wire history contains only a
sanitized summary and rewind flag. Full parameters/results remain in the backend
journal.
Ordinary action/result disclosure and note audiences retain their normal rules.

The protocol includes role `wizard`, required `state.wizard_game`, and
`history_branch`. Developer input is opaque text parsed only by the server;
observations are backend-resolved scenes without internal geometry. Snapshots use
an empty request ID and establish an explicit stream boundary after setup/rewind;
they are not acknowledgements for an outstanding request. Clients rebuild their
state from these snapshots, including decreasing ticks/revisions after rewind.
ASCII clears old branch drafts, pickup choices, and history panels. Request IDs,
expected actor revisions, and branch checks prevent stale or duplicate mutations.
Denied roles are checked before receipt lookup. Exact authorized retries return
the original receipt without replaying the operation.

The save preserves the root branch, permanent marker, and chronological records
with authenticated receipts. Replaying the records reconstructs all branches and
the bounded decision cache; the final branch is determined by the rewind records.
Only the current save format and ruleset load. Rewind restores complete
simulation state, including scheduler, knowledge, inventory, ID allocation, and
the seeded combat random stream.

## Verification

Behavior tests cover atomic setup, duration/knowledge preservation, failed marker
commits, permanent promotion with no commands, copies, disabled restart, unsupported-save rejection,
corrupt replay, bounded targets, privacy, retained annotations, and deterministic
identity restoration. Raw WebSocket tests exercise every command with wizard,
player, and spectator roles, disabled mode, same-user retry bypasses, stale branch
requests, and snapshots. A slow-controller regression verifies rewind snapshots
precede new-branch control updates.

`scripts/fixtures/wizard-foundation.json` currently drives the actual server,
text client, text spectator, and native ASCII spectator in
`scripts/test_wizard_process.py`. The existing process tests are retained for
now; new scripted wizard scenarios should use `tor-client-headless` for
privileged setup and scenario-driving, reserving the text client for text
frontend coverage. The suite verifies ordinary pickup/wait actions, setup,
rewind, branch history, spawned actor scheduling, control transfer, denied
inputs, and marked save/resume with privileged access disabled. These tests run
in debug/release on Windows and Linux with the existing desktop/Xvfb CI
configuration.

## Design requirements and later extensions

The following requirements govern this foundation and its future extensions.

## Enablement and permanent game identity

Wizard mode must be explicitly enabled on the server, disabled by default. A
frontend option, command name, or claimed identity cannot enable it. Support
creating a wizard game and explicitly promoting an existing game through a
server administration operation; client access alone cannot promote a game.
The implemented interface is the startup administration flag described above.

Persist an irreversible wizard-game marker before acknowledging enablement or
accepting any privileged mutation. If persistence fails, enablement fails without
granting privileged access. Mark the game on enablement even if no wizard command
is ever used. Keep two separate concepts:

- Game identity: once marked, always a wizard game.
- Current permission: whether this server session enables privileged operations
  and whether the authenticated caller is authorized to use them.

The marker belongs to the whole game lineage, outside rewindable simulation
state. Saves, exports/copies, snapshots, replay, retained branches, and new forks
inherit it. Rewinding before enablement or loading an older branch cannot clear
it. Restarting without privileged access leaves the game marked. There is no
supported command or save migration that converts a wizard game back to normal.
As with normal permadeath, arbitrary external file editing is outside the local
server's integrity guarantee; an independent pre-enablement backup is not
retroactively rewritten.

Expose the marker in attachment/state metadata and show a persistent, conspicuous
wizard-game indicator in every frontend, including observer views. Any future
normal-play scores, achievements, or completion records must exclude wizard games
or identify them separately. Keep the marker independent of actor control and
the current connection's privileges.

## Command families and staged delivery

| Family | Intended capabilities | Delivery |
| --- | --- | --- |
| Objects | Place supported items at an explicit location or in supported containment; specify supported properties | Basic items in the foundation; richer properties with interactions |
| Mobs/actors | Spawn supported actor or creature archetypes at explicit locations with validated settings | Existing actor types first; enemy archetypes with combat |
| Rooms | Place bounded rooms, walls and rotated passages/stair links | Implemented with wide joins; validate bounds and topology atomically |
| Teleport | Relocate an explicit actor to a valid region-local position, including across rooms/elevations | Foundation for existing geometry; extend with geometry features |
| Turn rewind | Restore a recorded decision boundary and continue on a new branch while preserving the abandoned future | Bounded initial history in the foundation; scalable storage later |

Wizard commands may bypass ordinary reach, travel, or acquisition rules, but must
not create invalid geometry, duplicate identities, broken containment, overlapping
occupancy where forbidden, or inconsistent scheduler state. Reject unsupported
archetypes/properties and invalid destinations atomically. These tools are not
arbitrary code execution or raw edits to serialized world internals.

Implement versioned request envelopes and server-side capability checks before
adding frontend shortcuts. Geometry parameters stay inside opaque developer
text, with structured operations confined to backend parsing and replay. Define actor scope and control policy
explicitly for each operation; observing or controlling an actor does not itself
grant wizard authority. Server-granted spectator accounts must remain read-only,
including in wizard games: the existing request allowlist must reject all wizard
mutations. Test this through raw requests as well as frontend inputs.
Keep requests uniquely identified and revision/branch
checked so stale or duplicate placement commands cannot corrupt a scenario.

State-changing wizard operations are deterministic external inputs recorded in
the durable journal with authenticated provenance, parameters, and results.
They are not annotations. Define time and scheduler effects per command: setup
operations should not implicitly spend ordinary action turns, but must update
affected observation revisions and readiness consistently. IDs and any randomness
must be reproducible on replay. Commit a mutation before publishing its result.

## Rewind, history, and observation boundaries

A turn rewind targets an authoritative decision boundary, not a rendered frame
or a guessed wall-clock interval. Multiple actors and variable action durations
make “one turn” ambiguous; define the target in terms of recorded history and
show the selected actor, tick, and branch to the caller.

Restore world state, scheduler/readiness, RNG state, identity allocation, actor
knowledge, and all other deterministic inputs. Preserve the abandoned future as
history and create a distinct branch for continued actions. Preserve annotations
with their original branch, anchors, provenance, and audience; do not silently
retarget notes from the abandoned future to the new present. The permanent wizard
marker is never restored from an earlier, unmarked simulation snapshot.

Serialize rewind with action processing and stop any pending travel at an action
boundary. Invalidate stale requests from the old branch. Publish a new explicit
snapshot/stream boundary to connected clients so decreasing simulation time does
not enter the current monotonic stream as an ordinary update. Refresh disclosed
observations for affected actors after placement and teleportation as well.
Ordinary observers retain their normal visibility and private-note rules.

## Acceptance and verification

Write behavior tests before implementation. Cover:

- Normal games and unauthorized callers reject every privileged command without
  changes, partial placement, or hidden-information disclosure.
- Enabling mode commits the marker even with no commands; failed persistence does
  not grant access. Restart, rewind before enablement, branch selection, and save
  copy/resume all preserve wizard identity. Disabling commands does not clear it.
- Placement and teleportation preserve invariants; invalid, stale, and duplicate
  requests have defined atomic/idempotent outcomes and correct observations.
- Rewind restores deterministic state and produces a separate continuation while
  retaining the original future and correctly scoped annotations. Replaying the
  same commands reproduces outcomes, including scheduler and RNG behavior.
- Actual server/frontend processes expose the marker, perform representative
  commands, reconnect after rewind, transfer ordinary control, and resume saves.
  Test multiple observers and denied wizard access as well as successful use.

Run acceptance in debug and release on Windows and Linux. Each subsequent feature
should add small scripted wizard scenarios with assertions about authoritative
outcomes, alongside ordinary-play tests. Wizard shortcuts help arrange scenarios;
normal actions must still exercise the feature being verified. Versioned command
scripts and seeds should make failures reproducible without manual setup. Keep
unimplemented scenarios in this plan until their implementation begins.

Wizard-command scripts should often be the final integration acceptance test for
a new feature, driving actual server/frontend processes rather than only calling
internal setup APIs. Assert both authoritative results and the observations shown
to the client. These scenarios are part of the feature's completion criteria,
along with focused behavior tests and updated documentation; see
[testing policy](testing.md#scenario-packages-fixtures-and-wizard-commands).

[Unnamed place hints](place-hints.md) add perceived cell anchors without labels
or boundaries. Shared memory retains last-seen hints; ASCII doesn't render them;
text uses them as described in [the adventure interface](text-adventure.md).

[Backend travel](travel.md) supports known-cell destinations. The
[text adventure interface](text-adventure.md) supports travel and approach-then-pickup.

Use `wizard chamber <id> <width> <depth> <height> <name>` to author enclosed interiors; see [material volumes](material-volumes.md).

Combat capture starts disabled, retains the latest 64 creature combat completions,
and records actual dice, check operands, edge allocation, ordered damage protection,
Fear, preparation timing, costs and before/after states. Misses have no damage rolls.
The report identifies dropped records and truncated traces. To page backwards, use
`wizard combat inspect <first sequence minus one>` while that sequence is retained.
The query and capture controls require an attached wizard and current context,
branch and observation revision, but do not require control or advance time.
Reports are private to the requester; Text prints them, ASCII opens the scrollable
report panel, and headless preserves the structured response for export. Capture
policy survives in-memory rewind and resets to disabled when a save is reopened.

To export a diagnostic window, pause the arena, query `wizard combat inspect`,
then follow every older-page cursor printed by the report until all retained
records are collected. Preserve the headless client's JSON-lines output before
rewinding, reopening the save or changing capture. The local helper assembles the
last unchanged reply window and rejects missing pages, conflicting duplicates or
mixed contexts:

```powershell
python scripts/combat_diagnostics.py export HEADLESS_JSONL COMBAT_JSON
python scripts/combat_diagnostics.py compare BASELINE_JSON REPLAY_JSON
```

Replace the uppercase placeholders with your local input and output paths.
The export retains all original receipt IDs and includes integrity and numerical
SHA-256 hashes. Numerical comparison normalizes only the two payment-owner IDs,
retains whether each is present, and compares every other captured field. Thus
fresh owners after rewind can reproduce the same numerical result. Dropped records
and truncated traces stay explicit; this exports the retained window rather than
recovering records already evicted. The helper consumes protocol-validated headless
reports and adds window/integrity checks; hashes do not authenticate their author.
Exports and transcripts are local analysis artifacts and stay outside Git.

For a fresh offline arena evaluation, run the `tor-arena` binary with
`--scenario PATH --seed N --all-ai`. It resolves all-AI/unpaused control in memory,
revalidates that resolved package and uses ordinary Engine admission/execution.
The authored source and existing saves are untouched. Without `--all-ai`, the
package must already declare unpaused all-AI control.

The JSON report includes the seed, selected character, resolved model hash,
streaming settings, combined input hash, ticks/actions, explicit elimination or
limit/stalemate reason, participant health/resources before and after, attack
checks/hits and ability resolution/application counts. Damage counters count
combat-resolution events; environmental changes are reflected in health states.
Wide counters use decimal strings. Invalid input or execution fails with exit 2
and a structured failure report.

For paired batches, use `python scripts/arena_evaluation.py --binary TOR_ARENA run
PLAN_JSON OUTPUT_DIRECTORY`. The JSON plan supplies `baseline` and `candidate`,
each with `forward` and `mirrored` package paths, a `seeds` array of canonical
unsigned decimal strings, and the `faction` whose win rate is compared. Paths are
relative to the plan file. Optional `timeout_seconds` defaults to 60 per run.
For example:

```json
{
  "baseline": {"forward": "baseline", "mirrored": "baseline-mirrored"},
  "candidate": {"forward": "candidate", "mirrored": "candidate-mirrored"},
  "seeds": ["42", "43"],
  "faction": "blue"
}
```

Author the mirrored packages explicitly: exchange starting sides while retaining
actor IDs, factions, builds, and encounter conditions. The harness requires
separate forward/mirrored inputs and matching actor/faction rosters; it does not
infer a geometric reflection through arbitrary portals or terrain. Review those
placements as part of the encounter. Baseline and candidate use identical seeds
in both orientations, preserving actor-specific seeded health draws.

The new output directory retains exact package snapshots, file hashes, executable
SHA-256, individual run records, and `batch.json` with all reports and the paired
summary. It refuses to overwrite an existing directory. Interrupted batches retain
completed records and a write-ahead checkpoint. Continue them with
`python scripts/arena_evaluation.py --binary TOR_ARENA resume OUTPUT_DIRECTORY`.
Resume validates the executable, retained package snapshots, record identities
and chained checksums before executing unfinished encounters. It preserves
completed failures and censored results, recovers a pending publication without
reexecuting it, and accepts an already completed batch without running it again.
Changes to the original authoring packages do not affect the snapshots. Older
partial inventories without the recovery checkpoint cannot be resumed. Checksums
detect corruption; they do not authenticate artifacts. Artifacts remain local and
are not committed or promoted to authored source definitions.

Each seed contributes to the win difference only if all four runs end in
elimination. Mutual elimination counts as half a win. Orientation differences are
averaged within each seed. Failure, stalemate and cap counts remain explicit, and
excluded seeds are listed; the mean is null when no complete pairs exist. The mean
is descriptive evidence, without a significance or acceptance claim. Exit 1 means
at least one recorded run failed; exit 2 means invalid batch inputs/artifacts.

Use `python scripts/arena_evaluation.py --binary TOR_ARENA replay OUTPUT_DIRECTORY`
to verify fingerprints and rerun every retained encounter, comparing complete
reports (including recorded failures). Changed inputs, executable fingerprints or
run inventories are rejected. Exit 1 means a replay differs. The curated HD/build
matrix and sequential campaign below provide additional evaluation coverage.


Generate the curated encounter matrix with `python scripts/arena_matrix.py
--compiler TOR_SCENARIO --output NEW_DIRECTORY --seeds 42 43`. It creates four
validated packages per case and a paired `plan.json` suitable for the batch
runner above. `matrix.json` records the compiler fingerprint, seed list, cases,
and explicit exclusions. The output must be new; failed or interrupted generation
retains partial evidence without publishing a complete matrix inventory.

The full matrix contains 67 cases (268 packages) across 1, 2, 4, 8 and 16 HD:
racial builds, STR-heavy and SPD-light natural melee styles, all four Mage Mana
bindings, alternating classes, both class dips, resource sustain, Fear, passive
Fear immunity/Energy reduction, and fully partitioned terrain. One-HD mixed/dip builds are
excluded because two HD sources need at least two HD. `--families` and `--hd` can
select a subset. Defaults use the bundled arena's current ruleset; `--ruleset`
provides an explicit override, which the ordinary compiler still validates.

Each candidate faces the same opponent and uses the same HD as its baseline.
The baseline is a STR-heavy Warrior; the opponent is the same build except in
the resistance cases, where it is a Fear-capable Mage. Initial attributes sum to
ten, and each recipe spends its owned training and attribute opportunities within
the ordinary caps. Talent choices retain their prerequisite chains. Mirroring
reflects starting positions across the nine-cell arena while preserving IDs,
factions, builds and terrain. An equal HD count does not assert equal power.
These recipes exercise natural melee styles; authored item weapon migration
remains separate work.

Execute each inventory entry's plan with the paired batch runner, storing reports
in a separate new directory, then verify it with `replay`. Partitioned encounters
end at a cap or stalemate; these are deliberate observations rather than
substitute wins. Matrix generation validates packages and prepares plans; it does
not run battles, perform optimizer screening, or promote balance changes.


The search sampler requires a separate Python environment. Create one with
`python -m venv SEARCH_ENV`, then use its Python to run
`-m pip install --require-hashes -r .github/requirements-arena-search.txt`.
Use that interpreter for repository Python tests and search tools. CI installs
the same wheel-only, exact-version environment for both platform/profile jobs.
Simulation, server and clients acquire no optimizer runtime dependency.

The reviewed environment pins Optuna 5.0.0 and its transitive dependencies with
published wheel hashes. Optuna, Alembic, Colorlog, SQLAlchemy, Mako and PyYAML use
MIT licenses; NumPy's distribution includes BSD/MIT/0BSD/Zlib/CC0 notices,
Packaging uses Apache-2.0 or BSD-2-Clause, tqdm uses MPL-2.0 and MIT, MarkupSafe and
Colorama use BSD licenses, and typing-extensions uses PSF-2.0. Preserve the
upstream distribution notices. Dependency updates require repeating the
continuation and constraint checks, then updating the pins and fingerprints.

`arena_search_sampling.py` provides the campaign's constrained TPE sampling
primitive. It recreates a seeded sampler for each trial ordinal and restores the
completed history with public Optuna APIs. Ten startup trials precede TPE
sampling; parameters are visited in canonical name order. Named constraints are
feasible when their values are zero or less. Objective/constraint values must be
finite. Histories retain input-space, interpreter/platform, dependency and sampler
source fingerprints, plus record integrity hashes. Changed environments or
modified evidence are rejected rather than silently continuing another search.
Hashes detect accidental changes; they do not authenticate an author.

Run `python scripts/arena_search_sampling.py SAMPLING_JSON` with the pinned
interpreter to verify every recorded proposal and print the next proposal. This
command is read-only. The primitive derives separate, disjoint default seed sets
of 20 training, 20 screening and 200 acceptance seeds. The complete campaign
command below connects those proposals to reviewed parameters, evaluates trials,
screens three finalists, evaluates the winner independently and exports it for
review. Sampling/history verification alone does not establish candidate quality.


Define reviewed candidate parameters with `tor-arena-search-parameters-v1` JSON.
Each named parameter supplies a sampler `space` and one or more existing scalar
`targets`. For example:

```json
{
  "format": "tor-arena-search-parameters-v1",
  "parameters": {
    "strength": {
      "space": {"type": "integer", "low": 1, "high": 5},
      "targets": [{"file": "scenario.toml", "path": ["characters", 0, "creature", "attributes", "strength"]}]
    }
  }
}
```

Use `python scripts/arena_search_parameters.py --compiler TOR_SCENARIO
--plan PAIRED_PLAN --spec PARAMETER_SPEC --values PARAMETER_VALUES --output
NEW_DIRECTORY`. Values are a JSON object such as `{"strength": 4}`. Optional
`--seeds` selects the evaluation seeds; otherwise the source plan's list is kept.

The tool snapshots both baseline and candidate packages in both orientations.
It applies the same assignments to both candidate orientations, preserving the
baseline. Targets must be existing scalars in creature catalog/build declarations
or AI profile settings; scalar type must stay unchanged. Direct region actor
creature declarations are supported. Overlapping targets, missing paths and edits
to placement, actor IDs/names/factions, bodies, control, participants or limits
are rejected. This defines a constrained parameter space; additional budgets and
selection criteria belong to the campaign configuration.

Every resolved package passes through the ordinary scenario compiler. A legal
candidate publishes `plan.json` for the paired runner and `candidate.json` with
parameters, definition hash, compiler fingerprint, original file hashes and
resolved file hashes. Invalid shared-rule builds retain their resolved files and
compiler errors, return exit 1, and receive no runnable plan. Bad configuration,
missing files or an existing output directory return exit 2. No source package or
save is overwritten.

To resume candidate preparation, repeat the materialization command with
`--resume` and the same plan, specification, parameter values, seed overrides and
output directory. `materialization.json` records the input and compiler hashes
and each completed package validation. Resume verifies all completed packages
before invoking the compiler, preserves their files and validation failures, and
rebuilds only unfinished packages. It rejects changed source content, compiler,
configuration or published evidence. Keep source packages available and unchanged
until preparation finishes. Interrupted publication of a valid candidate's
`plan.json` can be recovered from its completed checkpoint. Recovery hashes detect
corruption and are not authentication. Older materializations without this
checkpoint cannot resume.

Run the complete sequential campaign with
`python scripts/arena_search.py --binary TOR_ARENA --compiler TOR_SCENARIO run
CONFIG_JSON NEW_OUTPUT_DIRECTORY`. The configuration format is
`tor-arena-search-v1`. Required fields are `plan`, `parameter_spec` (paths relative
to the configuration file) and `seed` (canonical unsigned decimal text).
Defaults are 50 candidate trials, 20 training seeds, three finalists screened on
20 independent seeds, and one winner evaluated with its baseline on 200 further
independent seeds. Optional `candidates` is 3..999; `training_seeds`,
`screening_seeds` and `acceptance_seeds` are 1..1000.

Optional `constraints` is a list of named linear budgets, each containing `name`,
`coefficients` (integer parameter names mapped to bounded integer weights) and
`maximum`. Each budget requires the weighted sum to be at most its maximum.
Compiler-invalid candidates, candidate Engine failures and insufficient paired
coverage are recorded as infeasible optimizer observations. Baseline compiler or
Engine failures stop the campaign as configuration errors. Defaults require all
seed pairs to finish elimination; `maximum_excluded_fraction` can explicitly
permit some censored seeds, but at least one complete pair remains required.
No failure or cap is silently scored as a loss.

Training ranks feasible observations by mean paired win difference, breaking ties
by trial ordinal, and selects three distinct parameter assignments. Fewer than
three ends with `insufficient_feasible_candidates`. Screening selects its winner
using only screening results. Acceptance requires feasibility and a mean at least
`minimum_acceptance_difference` (default zero, range -1..1). This is a configured
descriptive threshold; it is not a claim of statistical significance or balance.
The campaign can end with `screening_infeasible` or `acceptance_failed`. Exit 1
reports those unsuccessful outcomes; exit 2 reports invalid configuration,
corrupted artifacts or tool failures.

Continue with `python scripts/arena_search.py --binary TOR_ARENA --compiler
TOR_SCENARIO resume OUTPUT_DIRECTORY`. Input snapshots, tool and implementation
hashes, dependency environment, optimizer history and per-stage records are
verified. Original authoring paths are unnecessary once snapshots are ready.
Recovery reconciles a completed trial whose history publication was interrupted,
continues partial encounter batches, and preserves completed results. Changed
retained evidence is rejected; hashes detect corruption, not authentication.
A completed campaign verifies its evidence without rerunning completed encounters.

The output retains `inputs.json`, immutable input packages, `history.json`, each
trial's compiled candidate, paired reports and result, screening and acceptance
reports, and `campaign.json`. The `review` directory exports both candidate
orientations and baselines, their runnable plan, parameters, fingerprints and
acceptance evidence. Failed acceptance can still be reviewed. Export never
updates authored scenario definitions.

The default campaign was executed on a curated four-HD strength-heavy encounter
with seed 42, strength values 3..6 and a strength budget of at most 5. It completed
all 50 trials: 42 feasible and eight compiler-invalid. Its 3,360 training,
240 screening and 800 acceptance encounters all ended in elimination, without
failed or censored encounters. The 240 training/screening/acceptance seeds were
disjoint. Screening selected Strength 4; its acceptance mean was -0.16, below the
configured zero threshold, so the outcome was `acceptance_failed`. The candidate
was retained for review and no authored definitions changed. This result
illustrates why screening winners still require independent acceptance.

The retained inputs, all stage reports, selection, rejection evidence and review
export were audited. Completed continuation left all 6,293 retained files and
modification times unchanged, and all 800 acceptance reports matched exact replay.
Reduced process tests also cover interrupted history publication and artifact
corruption. This is evidence for the campaign workflow and this bounded parameter
space; it does not establish balance across the creature roster. Final
cross-platform/release checks remain required. Preserve matching executables,
source modules and the pinned dependency lock with local campaign evidence when
later builds change the implementation; continuation deliberately rejects changed
tools. A preserved copy of this campaign and its tools also verified completed
continuation with all 6,293 campaign files unchanged.
