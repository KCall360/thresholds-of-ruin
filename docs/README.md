# Documentation

Choose a task below. Current references describe this tree; plans describe
unfinished work; the Rogue research describes a pinned external game. When code,
tests and a guide disagree, investigate the contract and correct the discrepancy
rather than treating a historical plan as current behavior.

## Start here

| I want to… | Read |
| --- | --- |
| Start playing | [Repository README](../README.md), then [text](play/text.md) or [ASCII](play/ascii.md) |
| Play the Rogue starter slice | [Rogue presets and launch](play/rogue.md) |
| Spend earned points or respec | [Player advancement](play/advancement.md) |
| Understand the current game and limitations | [Gameplay](play/gameplay.md), [current status](status.md) |
| Set up a development machine | [Setup](development/setup.md), [contributing](../CONTRIBUTING.md), [testing](development/testing.md) |
| Understand ownership and design | [Architecture overview](architecture/overview.md), [decisions and evidence](architecture/decisions.md) |
| Author a scenario or rules override | [Scenario packages](reference/scenario-packages.md), [generation](reference/generation.md), [creature rules](reference/creature-rules.md) |
| Integrate a client or automation | [Protocol](reference/protocol.md), [headless client](reference/headless.md), [CLI](reference/cli.md) |
| Work as an AI coding agent | [AGENTS.md](../AGENTS.md), plus the ordinary contributor/testing policy |

## Current reference map

| Concern | Authoritative guide |
| --- | --- |
| Actions, preparation, rules execution and scheduling | [Simulation](architecture/simulation.md) |
| Regions, portals, materials, bodies and gravity | [Geometry](architecture/geometry.md) |
| Eye-based sight, illumination, awareness and reference algorithms | [Perception](architecture/perception.md) |
| Active/frozen/detached state and content preparation | [Region lifecycle](architecture/streaming.md) |
| Durability, journals, checkpoints, recovery and history | [Persistence](architecture/persistence.md) |
| Disclosed client state, narrative, parser and memory | [Client architecture](architecture/clients.md) |
| Creature mechanics and shared catalog | [Creature rules](reference/creature-rules.md) |
| Stacks, equipment, consumption and knowledge | [Items](reference/items.md) |
| Steps, doors, stairs, travel and durable places | [Movement and interaction](reference/movement.md) |
| Package structure, validation, pinning and overrides | [Scenario packages](reference/scenario-packages.md) |
| Declarative recipes and generation groups | [Generation](reference/generation.md) |
| Authentication, envelopes, serialization and stream recovery | [Wire protocol](reference/protocol.md) |
| Command-line arguments and environment | [CLI](reference/cli.md) |
| JSON-lines automation workflow | [Headless client](reference/headless.md) |

## Development procedures

- [Setup](development/setup.md): tools, platform/display prerequisites and environments
- [Testing](development/testing.md): required layers, local tiers, CI and evidence
- [Performance](development/performance.md): targets, workloads, comparisons and evidence
- [Arena](development/arena.md): combat diagnostics, evaluation, matrices and search
- [Wizard tools](development/wizard.md): privilege, inspection, setup and rewind

## Plans and external research

- [Remaining shared requirements](plans/requirements.md), including portable export
- [Rogue adaptation](plans/rogue.md): decisions and remaining content/survival work
- [Script extension design](plans/scripting.md): unimplemented runtime and open choices
- [Progression and creature construction design](plans/progression-design.md):
  edge-first talents, regeneration and constructed Rogue monsters, with its
  [implementation plan](plans/progression.md)
- [Rogue 5.4.4 research](research/rogue-5.4.4/README.md): pinned edition, rules and catalog

[Status](status.md) owns project sequencing. Research is neither an alternate TOR
ruleset nor a claim of executable parity. Useful rationale and historical evidence
are linked from [decisions](architecture/decisions.md) and the performance guide;
completed implementation diaries are not maintained as current references.

## Maintaining this documentation

1. Put each contract in one authoritative home. Other pages summarize briefly and
   link, rather than copying controls, schema fields, current status or policies.
2. Update behavior, limitations and verification coverage together. Replace old
   statements; do not append a new paragraph that contradicts an earlier one.
3. Update [status](status.md) when scope changes. It is the only current-format
   registry. Keep source-pinned research separate from current version assertions.
4. Keep architecture about boundaries and rationale. Put repeatable procedures in
   development guides, exact contracts in references and unfinished requirements
   in plans. Delete fulfilled plan items after preserving useful rationale.
5. Preserve meaningful negative results, measurement boundaries and uncertain
   findings with immutable evidence links. Routine test counts, rollout status,
   local logs and session/deployment diaries belong outside maintained guides.
6. Check a new reader's path. Every Markdown page must be reachable from this
   index, directly or through another indexed page. Moves must update local links,
   heading anchors, source comments, help pointers and script references.
7. Mark fixed-edition research with `<!-- documentation-role: fixed-source -->`.
   It remains subject to link and reachability checks; only current-format
   comparisons exclude it. Do not use the marker to hide stale current guidance.
8. Complete wire envelopes use the `json client-message` or `json server-message`
   fenced-block tags. The protocol integration tests decode them through the real
   bounded protocol decoders. Label fragments explicitly; valid JSON alone does
   not establish a valid request. Keep target values and shell prerequisites clear.
9. Run documentation checks and relevant example tests, plus the verification
   required by [testing policy](development/testing.md). Structural checks cannot
   prove semantic accuracy; inspect code/test contracts and conflicting pages.

The checks live in [test_documentation.py](../scripts/test_documentation.py) and
[protocol documentation tests](../crates/protocol/tests/it/documentation.rs).
Source-adjacent Rustdoc owns API-local invariants; avoid copying Rust type layouts
into prose unless the example is tested and explains something additional.
