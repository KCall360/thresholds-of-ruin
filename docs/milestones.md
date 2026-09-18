# Milestones

## 0: Foundation (complete)

- Public GitHub repository, GPL-3.0-only license, workspace and architecture.
- Windows and Linux formatting, lint, unit and cross-crate integration checks.
- Foundational region bounds and observation-stream ordering tests.
- No playable application or frontend launch-test claim at this stage.

## 1: Shared playable slice (in progress)

Completed: deterministic in-memory simulation of two connected rooms, actor IDs,
movement/pickup/wait actions, variable recovery times, room-level observations,
and integration acceptance tests. See [implementation details](simulation-slice.md).

Remaining PRs: network protocol/server and streamed observations; the text client;
the graphical ASCII client and control transfer; persistent save/resume. The
simulation tests do not stand in for future server or frontend process tests.

Write the acceptance scenario first, then implement a seeded two-space scenario,
an object, actor control, streaming server, text client, graphical ASCII client,
and save/resume.

Acceptance: start through text, pick up the object, transfer control to ASCII,
move through a doorway, save, restart the server, reconnect both clients, and
compare their disclosed state to the resumed authoritative state. Exercise the
actual client processes as well as shared adapters. Reconnection and stale or
duplicate commands must not corrupt the game.

## 2: Geometry and perception

Stairs/elevations, rotated portal, portal-aware visibility, knowledge and memory.
Test an interior door unrelated to a portal, portal orientation transforms,
vertical movement, cycles, and hidden-information disclosure. Test one action
producing several ordered updates before the next player decision.

## 3: Interactions and travel

Doors, locks, keys, containers, clarification, named places and interrupted travel.
Both clients complete the same manipulation scenarios. Unknown map regions must
not influence travel. Ambiguity consumes no time; threats interrupt before another
automatic movement action. Test cancellation and slow-client resynchronization.

## 4: Dungeon gameplay

Equipment, melee, two enemy types, different action durations/speeds, death and
exit objective. Add seeded generation after hand-authored fixtures are reliable.
Test deterministic combat, victory and persistent permadeath through both clients.

## 5: History and release preparation

Developer undo/branching, replay checksums, save-write recovery and packaged builds.
Verify retained branches, replay with compatible versions, and packaged client
launches that automatically start a local backend or attach to an existing one.

## CI growth

Every milestone adds its acceptance tests to CI. Keep unit tests fast and
deterministic. Protocol tests run a real server once transport exists; text tests
drive process input/output; ASCII tests combine input/presentation assertions with
actual window-launch tests. Configure graphical environments explicitly on both
platforms. Do not label model-only tests as graphical application tests.
