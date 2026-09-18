# Wizard mode development plan

Status: planned, not implemented. Introduce the foundation after the shared
playable slice and before the geometry and interaction milestones. Grow commands
alongside the systems they exercise. Wizard mode should make it easy to construct
small scenarios, reproduce failures, and verify future features through the same
authoritative backend used for normal play.

## Enablement and permanent game identity

Wizard mode must be explicitly enabled on the server, disabled by default. A
frontend option, command name, or claimed identity cannot enable it. Support
creating a wizard game and explicitly promoting an existing game through a
server administration operation; client access alone cannot promote a game.
The precise configuration and administration interface will be chosen during
implementation, not added to the current protocol implicitly.

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
| Rooms | Place room geometry and explicitly connect passages/portals | Geometry milestone; validate bounds and topology atomically |
| Teleport | Relocate an explicit actor to a valid region-local position, including across rooms/elevations | Foundation for existing geometry; extend with geometry features |
| Turn rewind | Restore a recorded decision boundary and continue on a new branch while preserving the abandoned future | Bounded initial history in the foundation; scalable storage later |

Wizard commands may bypass ordinary reach, travel, or acquisition rules, but must
not create invalid geometry, duplicate identities, broken containment, overlapping
occupancy where forbidden, or inconsistent scheduler state. Reject unsupported
archetypes/properties and invalid destinations atomically. These tools are not
arbitrary code execution or raw edits to serialized world internals.

Implement structured, versioned protocol requests and server-side capability
checks before adding frontend shortcuts. Define actor scope and control policy
explicitly for each operation; observing or controlling an actor does not itself
grant wizard authority. Keep requests uniquely identified and revision/branch
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
