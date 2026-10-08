# Project documentation

Guides describe what exists now. The roadmap and design plan describe what's
planned. When the code and a guide disagree, the code and its tests are
authoritative until the guide is corrected.

## Start here

| I want to… | Read |
| --- | --- |
| Find out what the game is and play it | [Repository README](../README.md) |
| Look up server and client options | [Command-line reference](server-options.md) |
| See what works and what comes next | [Project status and roadmap](milestones.md) |
| Contribute a change | [Development practices](../CONTRIBUTING.md) and the [testing policy](testing.md) |
| Understand the system's boundaries and why | [Architecture](architecture.md) |
| Work on the repo as an AI agent | [AGENTS.md](../AGENTS.md) |

## Playing

| Topic | Guide |
| --- | --- |
| Terminal play | [Text client](text-client.md), [adventure commands](text-adventure.md) |
| Windowed play | [Graphical ASCII client](ascii-client.md), [remembered map](ascii-memory.md) |
| Combat, AI, victory, and death | [Dungeon gameplay](dungeon.md) |
| Items and identification | [Items and character knowledge](items.md) |
| Recovery of unpublished interactions | [Interaction adaptation](interaction-adaptation.md) |
| Travel | [Backend travel](travel.md) |
| Place names | [Durable place knowledge](place-knowledge.md) |

## How the game works

| Area | Guides |
| --- | --- |
| Rules and time | [Simulation](simulation-slice.md), [diagonal movement](diagonal-movement.md), [doors](doors.md) |
| Geometry and physics | [Portal geometry](portal-geometry.md), [material volumes](material-volumes.md), [bodies, portals, and gravity](physics.md), [region streaming](region-streaming.md) |
| Perception | [2D shadowcasting comparison baseline](shadowcasting.md), [place hints](place-hints.md), [narration and stream recovery](narration-and-recovery.md), [three-dimensional sight](sight-3d.md) |
| Server and protocol | [Protocol and annotations](protocol.md), [headless client](headless-client.md), [running play until it needs input](run-until-blocked.md) |
| Saving | [Background saving](background-saving.md), [checkpoints](checkpoints.md) |
| Content | [Scenario packages](scenario-packages.md), [scenario scripting (planned)](scripting.md) |

## Development

| Topic | Guide |
| --- | --- |
| Documentation review scope and corrections | [Documentation review](documentation-review.md) |
| Testing requirements and how to run the checks | [Testing and verification](testing.md) |
| Performance targets, results, and open work | [Performance and scalable persistence](performance-persistence.md) |
| Benchmarks, before-and-after comparisons, and the performance ledger | [Performance harness](performance-harness.md) |
| Privileged setup for testing | [Wizard mode](wizard-mode.md) |
| Accepted future requirements | [Game design plan](game-design-plan.md) |
| Accepted refactor scope and implementation status | [Refactor guide](refactoring.md) |
| Text engine and parser internals | [IF engine](if-engine.md), [IF parser](if-parser-architecture.md), [spatial narrative summary](spatial-narrative-architecture.md) |
| Build the Rogue adaptation in stages | [Rogue scenario plan](rogue-scenario-plan.md) |
| Compare the researched Rogue rules and content | [Reference specifications](game-specifications/README.md), [Rogue overview](game-specifications/rogue-5.4.4/README.md), [rules](game-specifications/rogue-5.4.4/rules.md), [catalog](game-specifications/rogue-5.4.4/content.md), [TOR capability assessment](game-specifications/rogue-5.4.4/implementation.md) |

## Historical evidence

Current guidance is above. [Refactor increments](history/refactoring-increments.md)
and the [original spatial narrative diagnosis](history/spatial-narrative-architecture.md)
preserve superseded decisions and checkpoint measurements; their pending states
and proposed APIs do not describe current support.

Historical findings, closeout audits, session handoffs, and raw measurements
from before 2026-09-28 are preserved in the
[`docs-history-2026-09` archive](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs).

## Maintaining the docs

When behavior changes:

1. Update the relevant guide with user-visible behavior, limitations, and how
   it's verified. Replace outdated statements; don't leave history beside
   current behavior.
2. Update the [roadmap](milestones.md) if scope or status changed. It's the only
   place that states the current protocol, save format, ruleset, and validator
   versions. Elsewhere, say "current".
3. Update the [architecture](architecture.md) only when a boundary or durable
   design decision changed. It isn't a changelog.
4. Update the repository README only when the player-facing summary or quick
   start changed.
5. Add every new guide to a table on this page.
6. Current guides summarize project verification instead of depending on PR
   numbers, CI runs or local-only logs. Historical records and external reference
   specifications retain their dated provenance and pinned source identities.

`scripts/test_documentation.py` checks local links, this index, and the stated
versions. Nested reference and historical documents are included in file and
heading-link checks; historical checkpoint versions are excluded from
current-format assertions.
