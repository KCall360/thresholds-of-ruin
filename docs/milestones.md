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

Completed: server-enforced spectator credentials for both frontends, live
actor-perspective actions/results, read-only snapshots/history, and unchanged
annotation privacy. Raw WebSocket tests reject forged mutation requests and
receipt retries; actual text/ASCII process tests verify live observation, denied
inputs, and save/resume. See [spectator access](protocol.md#read-only-spectators).

The slice includes a seeded two-space scenario, an object, actor control,
streaming server, both frontends, and save/resume. This is not the full dungeon
gameplay milestone; reconnects currently require relaunching the clients.

Acceptance: start through text, pick up the object, transfer control to ASCII,
move through a doorway, save, restart the server, reconnect both clients, and
compare their disclosed state to the resumed authoritative state. Exercise the
actual client processes as well as shared adapters. Reconnection and stale or
duplicate commands must not corrupt the game.

## 1a: Wizard mode development foundation (complete)

Implemented server-enabled wizard mode with separate developer credentials and
a durable permanent marker. Text commands provide item/actor placement, teleport,
and rewind across the last 128 recorded decision boundaries. Both frontends
follow fresh snapshots and display wizard status. See [wizard mode](wizard-mode.md)
for commands, authorization, format migration, limits, and acceptance coverage.

- Server-controlled enablement and authorization; permanent wizard-game identity
  across saves, restarts, replay, copies, and all history branches.
- Explicit privileged protocol commands and visible wizard-game indicators in
  every frontend, including for observers without command privileges.
- Initial placement of supported objects/actors, teleportation, and bounded turn
  rewind to recorded decision boundaries, with deterministic journaling and
  preservation of the abandoned future. Never expose rewind in normal games.
- Scriptable text commands and actual server/client process tests for reproducible
  development scenarios; ASCII currently uses the text client alongside it for
  privileged commands.

Acceptance: explicitly enable wizard mode on the server; place an object,
teleport an actor, perform ordinary actions, rewind, and take a different action.
Restart the server and verify state, annotations, retained branches, and the
permanent wizard marker. Reject the same privileged requests in a normal game
and from an unauthorized observer without changing state or disclosing hidden
facts. Turning off privileged access must not remove the marker.

## 2: Geometry and perception

First slice complete (PR #10): a JSON-lines headless frontend uses the shared
connection and exposes current disclosed state separately from last-seen room
memory. Same-branch snapshots preserve memory; rewind resets it. Actual process
tests cover player/spectator access, stale hidden-room contents, revisit refresh,
and save/resume. See [the headless guide](headless-client.md). Later slices extend geometry and
perception below; this foundation does not complete milestone 2.

Completed (PR #11): [portal geometry](portal-geometry.md) adds bounded
cell visibility through rotated passages, elevation offsets, wall occlusion,
explicit stair links, and wizard room/link/terrain setup. Memory now refreshes
individual visible cells by opaque keys. The backend resolves one actor-relative
scene; clients never receive region or portal geometry. Rectangular multi-cell
joins are atomic, and orientation stays consistent through crossings. That slice
introduced protocol 5, save format 3, and observer-scene-v3 while preserving older
rules during replay.
Actual server/text/headless/native ASCII acceptance covers sight, stale memory,
movement, pickup, stairs, save/resume and rewind. Interactive doors independent
of portals and richer multi-event perception remain pending.

Implemented: [unnamed place hints](place-hints.md), authored cell anchors
independent of regions and portals. Protocol 6 discloses hints only with perceived
cells; shared memory retains potentially stale values. Wizard set/clear supports
dynamic maps, restart and rewind. That slice introduced place-hints-v4; older rules remain
unchanged. Actual text/headless/native ASCII acceptance covers disclosure and
memory. Client location grouping and navigation are not implemented by this slice.

Stairs/elevations, rotated portal, portal-aware visibility, knowledge and memory.
Test an interior door unrelated to a portal, portal orientation transforms,
vertical movement, cycles, and hidden-information disclosure. Test one action
producing several ordered updates before the next player decision.
Extend wizard commands with room placement and explicit passage connections as
the geometry model supports them; use these to reproduce perception edge cases.

Headless-client requirements (foundation and portal coverage implemented):

Add a thin headless client built on `tor-client-common`. It must connect as a
player or authorized spectator, issue scripted actions where permitted, and
expose only its received disclosed state to tests. Use it to test current
visibility separately from retained client memory, including portal views and
hidden-information boundaries.

## 3: Interactions and travel

Implemented first travel slice: [backend travel to known cells](travel.md), ASCII
`_` selection and mouse clicks, ordered completed-step updates, cancellation,
potential-hazard/obstruction/control interruptions, replayed navigation knowledge, and
restart/rewind behavior. Protocol 7 and new rules `travel-v5` preserve legacy
saves. Actual server/text observer/headless/native ASCII acceptance includes
native underscore and mouse input, normal play, wizard scenarios and persistence.
Text exposes no travel command yet; higher-level intentions, location grouping,
and automatic approach-then-manipulate sequences remain future work. Doors,
locks, containers, richer threat/damage interruptions and the remaining work below are
still pending.


Doors, locks, keys, containers, clarification, named places and interrupted travel.
Both clients complete the same manipulation scenarios. Unknown map regions must
not influence travel. Travel is server-managed: it resolves ordinary movement
steps without disclosing unresolved route steps or future outcomes. Ambiguity
consumes no time; threats interrupt before another automatic movement action.
Player cancellation takes effect at an action boundary. Test cancellation,
slow-client resynchronization, and clients that present already-completed travel
updates more slowly than the simulation.
Extend wizard object placement with supported container, door, lock, and item
properties so interaction/travel failures can be reproduced without manual setup.

## 4: Dungeon gameplay

Equipment, melee, two enemy types, different action durations/speeds, death and
exit objective. Player actors and mobs interact through the same action and world
interfaces; distinct player, AI, and future bot controllers supply their intents.
Add scenario and world-schema inputs before making seeded generation the default:
support fully authored scenarios, fully generated scenarios, and authored
scenarios with marked procedural-generation regions. Test deterministic combat,
victory, persistent permadeath, and each scenario form through both clients.
Add wizard mob placement with explicit supported archetypes and behavior settings;
use placement, teleportation, and rewind to verify combat and death scenarios.

## 5: Rogue-o-matic bot framework

Build a rogue-o-matic client framework on the ordinary disclosed client view. It
tracks received state, retained map memory, inventory and equipment, messages, and
explicitly uncertain or inferred knowledge; it cannot access server world state.
Provide a small action interface so multiple bot policies can be implemented on
top, starting with deterministic exploration and scenario-driven test bots. Test
that bots obey the same visibility, travel, and action boundaries as a human
client.

## 6: History and release preparation

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
