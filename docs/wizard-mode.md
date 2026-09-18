# Wizard mode

The foundation is implemented: permanent game marking, separately authenticated
wizard authority, ground-item and actor placement, teleportation, and bounded
rewind with retained futures. Scriptable commands are available in the text
client. Both frontends display the marker and follow setup/rewind snapshots;
the graphical ASCII client currently uses text alongside it for wizard commands.
[Geometry setup](portal-geometry.md) now adds room placement, rotated passages,
wall terrain and explicit vertical links. Richer item properties, enemy archetypes,
and scalable history remain later milestones.

## Start or promote a wizard game

Set distinct credentials in the server's PowerShell session:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
$env:TOR_WIZARD_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --wizard --listen 127.0.0.1:4000 --seed 42 --save saves/wizard.json
```

`--wizard` is an explicit server administration operation: it creates a marked
game or permanently promotes the existing save at that path, even if no command
is used. The marker commits before the listener opens. Both the flag and a valid,
distinct `TOR_WIZARD_TOKEN` are required. An optional `TOR_SPECTATOR_TOKEN` must
also differ. Invalid credential configuration fails before opening the save.

In the text client's terminal, set `TOR_SERVER_TOKEN` to the **wizard credential**
and start the client normally. The server grants role `wizard`; a player token
cannot gain wizard authority through control ownership or a frontend label.
Wizard authority covers the whole game, including all actors and rewind of the
whole simulation. It does not require ordinary control; ordinary actions still
do. Only grant the wizard credential to a trusted developer of this game.
Spectators remain read-only, including same-user retries of privileged requests.

Restart without `--wizard` and without `TOR_WIZARD_TOKEN` to disable privileged
access. The save remains a wizard game and both frontends continue showing it.
The trusted library equivalent is `Engine::enable_wizard()` before constructing
the service; session accounts must separately have `AccessRole::Wizard`.

## Text commands

The client forwards developer text without interpreting its geometry; only the
server parses these commands. Coordinates in this developer console are region-local integers, with north decreasing y. The fixture has
regions 1 and 2, each 5 by 3 by 1. Supported commands are:

| Command | Behavior |
| --- | --- |
| `wizard item token 1 1 1 0` | Place a copper token on the ground |
| `wizard item tablet 1 1 1 0` | Place a stone tablet on the ground |
| `wizard actor 75 1 2 1 0` | Spawn an ordinary actor with base recovery 75 ticks |
| `wizard teleport 1 2 1 1 0` | Teleport actor 1 to region 2, position (1,1,0) |
| `wizard rewind initial` | Fork from the initial scenario, while that boundary is retained |
| `wizard rewind <entry-id>` | Fork from the state immediately after a retained action/setup entry |
| `history [before-id]` | Current branch history, including this user's wizard summaries |
| `branch-history <branch-id> [before-id]` | Read permitted entries on an abandoned branch |

New-rule games also support `wizard room`, `wizard connect` and `wizard wall`;
see [geometry setup](portal-geometry.md#geometry-setup) for arguments and examples.

Placement/teleportation consume no ordinary action time. Teleport preserves
recovery times and reveals the destination to the relocated actor. New actors
are ready at the current tick and participate in stable scheduling; they are not
autonomous mobs. A wizard client can reconnect with `--actor <id>` to control
them. Ordinary accounts retain their configured actor allowlists.
Item properties and containment other than ground placement are not supported.
Unknown item kinds, zero recovery durations, invalid coordinates, and forbidden
actor overlap are rejected atomically.

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
authenticated author and actor, and public history contains only a sanitized
summary and rewind flag. Full parameters/results remain in the backend journal.
Ordinary action/result disclosure and note audiences retain their normal rules.

Protocol version **8** retains role `wizard`, required `state.wizard_game`, and
`history_branch`. Developer input is opaque text parsed only by the server;
observations are backend-resolved scenes without internal geometry. Snapshots use
an empty request ID and establish an explicit stream boundary after setup/rewind;
they are not acknowledgements for an outstanding request. Clients rebuild their
state from these snapshots, including decreasing ticks/revisions after rewind.
ASCII clears old branch drafts, pickup choices, and history panels. Request IDs,
expected actor revisions, and branch checks prevent stale or duplicate mutations.
Denied roles are checked before receipt lookup. Exact authorized retries return
the original receipt without replaying the operation.

Save format **3** preserves the root branch, permanent marker, and chronological
records with authenticated receipts. Replaying the records reconstructs all
branches and the bounded decision cache; the final branch is determined by the
rewind records. Normal format-1 saves migrate on successful open while retaining
`two-room-v1`. New games use `travel-v5`; wide joins also remain supported
in `observer-scene-v3`.
Legacy rules remain unchanged. Older servers cannot load the new ruleset. Rewind restores complete
simulation state, including scheduler, knowledge, inventory, and ID allocation.
The fixture has no evolving RNG; future RNG state belongs in these boundaries.

## Verification

Behavior tests cover atomic setup, duration/knowledge preservation, failed marker
commits, permanent promotion with no commands, copies, disabled restart, migration,
corrupt replay, bounded targets, privacy, retained annotations, and deterministic
identity restoration. Raw WebSocket tests exercise every command with wizard,
player, and spectator roles, disabled mode, same-user retry bypasses, stale branch
requests, and snapshots. A slow-controller regression verifies rewind snapshots
precede new-branch control updates.

`scripts/scenarios/wizard-foundation.json` drives the actual server, text client,
text spectator, and native ASCII spectator in `scripts/test_wizard_process.py`.
It verifies ordinary pickup/wait actions, setup, rewind, branch history, spawned
actor scheduling, control transfer, denied inputs, and marked save/resume with
privileged access disabled. These tests run in debug/release on Windows and Linux
with the existing desktop/Xvfb CI configuration.

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
| Rooms | Place bounded rooms, walls and rotated passages/stair links | Implemented with observer-scene-v3 wide joins; validate bounds and topology atomically |
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
[development practices](../CONTRIBUTING.md). Until wizard mode is available,
existing fixtures and process tests must still verify each new feature.

[Unnamed place hints](place-hints.md) add perceived cell anchors in protocol 7.
They carry no labels or boundaries. Shared memory retains last-seen hints; ASCII does not render them; text now uses them as described in
[the adventure slice](text-adventure.md). New saves use
`travel-v5`; earlier saves retain their original rules.

[Backend travel](travel.md) adds protocol 7 and `travel-v5` for new games.
Earlier rules retain their behavior. The [text adventure interface](text-adventure.md)
now adds text travel and approach-then-pickup.
