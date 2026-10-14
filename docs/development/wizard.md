# Wizard mode

Wizard mode provides permanent game marking, separately authenticated
wizard authority, ground-item and actor placement, teleportation, and bounded
rewind with retained futures. Scriptable commands are available through the
headless client's structured request interface, the text client's developer
console, and the graphical ASCII client's F7 wizard command editor. These clients
display the marker and follow setup/rewind snapshots. The headless client is the
preferred frontend for scripted wizard setup and scenario-driving. In ASCII,
enter the command without the leading `wizard`, then press Enter to submit or
Esc to cancel. For example, `creature inspect 2` opens a scrollable private report.
[Geometry setup](../architecture/geometry.md) includes room placement, rotated passages,
wall terrain and explicit vertical links. See [arena tools](arena.md) for controlled encounters, combat capture,
paired evaluation and search. Minimal `item` placement still supports only its
listed kinds and ground placement; it is not a general item-property editor.

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
and each numbered HD's retained health seed, base health contribution, named-pool
point balances, owned purchases and attribute increase. The two permanent starting
ranks, explicit restoration policy and dormant talent reasons are disclosed
separately. Effective grants are grouped by species,
type, subtype, class, template or active talent. Fear lists each causer and its
remaining active duration. Text prints these rows, ASCII opens a scrollable panel,
and headless emits the structured `creature_inspection` response. Normal history
and actor observations do not contain the report or its seeds.

## Developer command directory

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
| `wizard creature add-hd <actor> <racial|warrior|mage>` | Append a stable-seeded die; an intended authored ordinal restores its saved choices, while choose-again and extra ordinals stay empty |
| `wizard creature train <actor> <hit-die> <skill>` | Spend one compatible point owned by the named hit die on a skill rank |
| `wizard creature attribute <actor> <hit-die> <attribute>` | Use that die's attribute opportunity, available at every fourth total hit die |
| `wizard creature talent <actor> <hit-die> <talent>` | Spend one compatible point owned by that die on an eligible distinct talent |
| `wizard creature replace <actor> <hit-die> <purchase> refund` | Refund the selected purchase and its unsupported owned-parent subtree |
| `wizard creature replace <actor> <hit-die> <purchase> <pool> skill <skill>` | Replace one purchase with a rank paid from the named pool, refunding unsupported descendants |
| `wizard creature replace <actor> <hit-die> <purchase> <pool> talent <talent>` | Replace one purchase with an eligible talent paid from the named pool |
| `wizard creature remove-hd <actor>` | Remove the latest hit die and its owned choices; zero hit dice cause persistent death |
| `wizard creature template <actor> <template> <on|off>` | Apply or remove a named authored template through the shared rebuild rules |
| `wizard rewind initial` | Fork from the initial scenario, while that boundary is retained |
| `wizard rewind <entry-id>` | Fork from the state immediately after a retained action/setup entry |
| `history [before-id]` | Current branch history, including this user's wizard summaries |
| `branch-history <branch-id> [before-id]` | Read permitted entries on an abandoned branch |

The wizard `talent` and talent `replace` selectors accept only choice-free
talents; they have no form-choice argument. Acquire Mage roots through the
ordinary [advancement preview and confirmation](../play/advancement.md), which
requires the exact root and form IDs even for singleton Falter.

HD and purchase positions are one-based. Replacement is one atomic, journaled
privileged edit. It uses the [shared ownership transaction](../reference/creature-rules.md#advancement-points-and-starting-ranks):
dormant owned OR alternatives retain their branch, while losing the final owned
parent route refunds descendants exactly once. The edit preserves starting ranks,
HD seeds, attribute choices, authored restoration intent and runtime injury.
This immediate developer command does not implement the ordinary timed,
safe-place, ration-consuming respec action.

The geometry command family also supports `wizard room`, `wizard connect` and `wizard wall`;
see [geometry setup](../architecture/geometry.md#developer-authoring) for arguments and examples.

Placement/teleportation consume no ordinary action time. Teleport preserves
recovery times and reveals the destination to the relocated actor. New actors
are ready at the current tick and participate in stable scheduling; they are not
autonomous mobs. A wizard client can reconnect with `--actor <id>` to control
them. Ordinary accounts retain their configured actor allowlists.
The minimal `wizard item` command cannot set richer item properties or place
items into containment other than the ground; ordinary equipment/consumable
gameplay is supported separately.
Unknown item kinds, zero recovery durations, invalid coordinates, and forbidden
actor overlap are rejected atomically.

Arena control and creature-edit semantics, including resource accounting,
advancement limits and deterministic seeds, are defined in the
[arena guide](arena.md#controlled-editing).

## Rewind and retained branches

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
simulation state, including scheduler, knowledge, inventory and the seeded combat
random stream. Allocation high-water marks from the abandoned future are retained
so new actors, items, doors, records and intentions never reuse identities; see
[persistence](../architecture/persistence.md#capture-and-scheduling).

## Integrity and extension constraints

The permanent marker belongs to the entire lineage, outside rewindable simulation
state. Existing copies preserve it; an independent pre-enablement backup is not
retroactively rewritten. Arbitrary external file editing is outside the local
server's integrity guarantee. Future scores, achievements and normal completion
records must exclude marked games or identify them separately.

Wizard commands bypass gameplay reach or acquisition rules, never structural
validity. They must not create duplicate identities, invalid geometry, forbidden
overlap, broken containment or inconsistent scheduling. These are bounded,
server-parsed deterministic inputs, not arbitrary code execution or raw serialized
world edits. State-changing operations retain authenticated provenance, parameters
and results in the private journal before publishing their effects. Inspection
and runtime capture controls are explicitly read-only/session exceptions.

New operations need server-side capabilities, explicit actor/control scope,
revision/branch/retry behavior and tests before frontend shortcuts. Geometry stays
in opaque developer text. Ordinary observations and private-note audiences keep
their existing disclosure rules. Future extension requirements live in the
[requirements plan](../plans/requirements.md) and [scripting plan](../plans/scripting.md).

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
`scripts/test_wizard_process.py`. New scripted wizard scenarios should use `tor-client-headless` for
privileged setup and scenario-driving, reserving the text client for text
frontend coverage. The suite verifies ordinary pickup/wait actions, setup,
rewind, branch history, spawned actor scheduling, control transfer, denied
inputs, and marked save/resume with privileged access disabled. These tests run
in debug/release on Windows and Linux with the existing desktop/Xvfb CI
configuration.
