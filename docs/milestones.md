# Milestones

## 0: Foundation (complete)

- Public GitHub repository, GPL-3.0-only license, workspace and architecture.
- Windows and Linux formatting, lint, unit and cross-crate integration checks.
- Foundational region bounds and observation-stream ordering tests.
- No playable application or frontend launch-test claim at this stage.

## 1: Shared playable slice (complete)

Completed: deterministic in-memory simulation of two connected rooms, actor IDs,
movement/pickup/wait actions, variable recovery times, room-level observations,
and integration acceptance tests. See [implementation details](simulation-slice.md).

Completed: versioned JSON/WebSocket server, actor attachment and control transfer,
streamed observations, user/frontend/backend annotations, durable journal replay,
and real WebSocket/process integration tests. See [the protocol](protocol.md).

Completed: playable text client with deterministic command parsing, disclosed
item-name resolution, annotations, paginated history, live updates, and control
transfer. Actual server/text process tests cover play, two clients, and restart
persistence in debug and release on Windows and Linux. See [the client guide](text-client.md).

Completed: native graphical ASCII frontend with disclosed-room rendering,
keyboard movement/pickup, inventory, private/shared notes, history browsing, and
explicit control transfer. Actual process acceptance starts in text, picks up the
token, transfers control to the graphical window, traverses the passage, restarts
the server, and compares resumed state and history. Native keyboard-event and
window-launch tests run on Windows and Linux Xvfb. See [the ASCII guide](ascii-client.md).

The slice includes a seeded two-space scenario, an object, actor control,
streaming server, both frontends, and save/resume. This is not the full dungeon
gameplay milestone; reconnects currently require relaunching the clients.

Acceptance: start through text, pick up the object, transfer control to ASCII,
move through a doorway, save, restart the server, reconnect both clients, and
compare their disclosed state to the resumed authoritative state. Exercise the
actual client processes as well as shared adapters. Reconnection and stale or
duplicate commands must not corrupt the game.

## 1a: Wizard mode development foundation (planned)

Add server-enabled wizard mode early so later milestones can use it to construct
scenarios and verify behavior. This is planned work, not an available server flag
or client command. See [the wizard mode plan](wizard-mode.md) for requirements.

- Server-controlled enablement and authorization; permanent wizard-game identity
  across saves, restarts, replay, copies, and all history branches.
- Explicit privileged protocol commands and visible wizard-game indicators in
  every frontend, including for observers without command privileges.
- Initial placement of supported objects/actors, teleportation, and bounded turn
  rewind to recorded decision boundaries, with deterministic journaling and
  preservation of the abandoned future. Never expose rewind in normal games.
- Scriptable text commands and actual server/client process tests for reproducible
  development scenarios; extend other frontends as they become available.

Acceptance: explicitly enable wizard mode on the server; place an object,
teleport an actor, perform ordinary actions, rewind, and take a different action.
Restart the server and verify state, annotations, retained branches, and the
permanent wizard marker. Reject the same privileged requests in a normal game
and from an unauthorized observer without changing state or disclosing hidden
facts. Turning off privileged access must not remove the marker.

## 2: Geometry and perception

Stairs/elevations, rotated portal, portal-aware visibility, knowledge and memory.
Test an interior door unrelated to a portal, portal orientation transforms,
vertical movement, cycles, and hidden-information disclosure. Test one action
producing several ordered updates before the next player decision.
Extend wizard commands with room placement and explicit passage connections as
the geometry model supports them; use these to reproduce perception edge cases.

## 3: Interactions and travel

Doors, locks, keys, containers, clarification, named places and interrupted travel.
Both clients complete the same manipulation scenarios. Unknown map regions must
not influence travel. Ambiguity consumes no time; threats interrupt before another
automatic movement action. Test cancellation and slow-client resynchronization.
Extend wizard object placement with supported container, door, lock, and item
properties so interaction/travel failures can be reproduced without manual setup.

## 4: Dungeon gameplay

Equipment, melee, two enemy types, different action durations/speeds, death and
exit objective. Add seeded generation after hand-authored fixtures are reliable.
Test deterministic combat, victory and persistent permadeath through both clients.
Add wizard mob placement with explicit supported archetypes and behavior settings;
use placement, teleportation, and rewind to verify combat and death scenarios.

## 5: History and release preparation

Scale wizard rewind/branching beyond the initial bounded implementation; add
replay checksums, save-write recovery and packaged builds.
Verify retained branches, replay with compatible versions, and packaged client
launches that automatically start a local backend or attach to an existing one.

## CI growth

Every milestone adds its acceptance tests to CI. Keep unit tests fast and
deterministic. Protocol tests run a real server once transport exists; text tests
drive process input/output; ASCII tests combine input/presentation assertions with
actual window-launch tests. Configure graphical environments explicitly on both
platforms. Do not label model-only tests as graphical application tests.

Every new feature carries behavior tests and an integration acceptance scenario,
not only a milestone-wide test at the end. Once wizard mode exists, scripted
wizard commands should often provide the final integration test through actual
server/frontend processes. Use them to arrange and reproduce scenarios, then
exercise the feature through its intended commands and assert outcomes. Keep
normal-game coverage and update documentation as part of completing the feature.
See [development practices](../CONTRIBUTING.md) for the testing requirements.
