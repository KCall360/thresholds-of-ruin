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
| Terminal play | [Text client](text-client.md), [adventure commands](text-adventure.md), [IF engine](if-engine.md), [IF parser architecture](if-parser-architecture.md), [spatial narrative architecture](spatial-narrative-architecture.md) |
| Windowed play | [Graphical ASCII client](ascii-client.md), [remembered map](ascii-memory.md) |
| Combat, AI, victory, and death | [Dungeon gameplay](dungeon.md) |
| Items and identification | [Items and character knowledge](items.md) |
| Travel | [Backend travel](travel.md) |
| Place names | [Durable place knowledge](place-knowledge.md) |

## How the game works

| Area | Guides |
| --- | --- |
| Rules and time | [Simulation](simulation-slice.md), [diagonal movement](diagonal-movement.md), [doors](doors.md) |
| Geometry and physics | [Portal geometry](portal-geometry.md), [material volumes](material-volumes.md), [bodies, portals, and gravity](physics.md), [region streaming](region-streaming.md) |
| Perception | [Shadowcasting](shadowcasting.md), [place hints](place-hints.md), [narration and stream recovery](narration-and-recovery.md), [three-dimensional sight](sight-3d.md) |
| Server and protocol | [Protocol and annotations](protocol.md), [headless client](headless-client.md), [running play until it needs input](run-until-blocked.md) |
| Saving | [Background saving](background-saving.md), [checkpoints](checkpoints.md) |
| Content | [Scenario packages](scenario-packages.md), [scenario scripting (planned)](scripting.md) |

## Development

| Topic | Guide |
| --- | --- |
| Testing requirements and how to run the checks | [Testing and verification](testing.md) |
| Performance targets, results, and open work | [Performance and scalable persistence](performance-persistence.md) |
| Benchmarks, before-and-after comparisons, and the performance ledger | [Performance harness](performance-harness.md) |
| Privileged setup for testing | [Wizard mode](wizard-mode.md) |
| Accepted future requirements | [Game design plan](game-design-plan.md) |
| Accepted refactor scope and implementation status | [Refactor plan](refactoring.md) |

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
6. Don't cite PR numbers, commit hashes, CI runs, or local-only files (such as
   `.local/` logs). Summarize the evidence instead.

`scripts/test_documentation.py` checks local links, this index, and the stated
versions.
