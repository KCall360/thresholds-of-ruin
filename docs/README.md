# Project documentation

This directory separates what exists from what is intended:

- [Project status and roadmap](milestones.md) is the source of truth for completed,
  active, and planned work.
- [Architecture](architecture.md) records the system boundaries and the reasons
  behind them. Statements about future work are explicitly identified there.
- The implementation guides below describe behavior that exists in the current
  tree, its compatibility constraints, and its verification.
- [Development practices](../CONTRIBUTING.md) defines how changes are designed,
  tested, documented, and published.

When implementation and documentation disagree, the code and automated tests are
authoritative until the documentation is corrected. A feature is not complete
until its guide and the roadmap are updated together.

## Start here

| Need | Document |
| --- | --- |
| Understand the project and run checks | [Repository README](../README.md) |
| See what works and what comes next | [Project status and roadmap](milestones.md) |
| Resume implementation work | [Session handoff](session-handoff.md) |
| Understand boundaries and design decisions | [Architecture](architecture.md) |
| Review background-save measurements | [Phase B findings](phase-b-findings.md) |
| Configure saves and understand crash recovery | [Background saving](background-saving.md) |
| Run the server or integrate a client | [Protocol and persistence](protocol.md) |
| Play in a terminal | [Text client](text-client.md) |
| Play in a native window | [Graphical ASCII client](ascii-client.md) |
| Drive scripted acceptance scenarios | [Headless client](headless-client.md) |

## Implemented behavior

These pages describe current behavior rather than proposals.

| Area | Documents |
| --- | --- |
| Simulation | [Simulation slice](simulation-slice.md), [diagonal movement](diagonal-movement.md), [doors](doors.md) |
| Geometry and perception | [Portal geometry](portal-geometry.md), [shadowcasting](shadowcasting.md), [material volumes](material-volumes.md), [place hints](place-hints.md) |
| Client knowledge and presentation | [ASCII memory](ascii-memory.md), [text adventure](text-adventure.md) |
| Navigation | [Backend travel](travel.md) |
| Engineering plans | [Performance and scalable persistence](performance-persistence.md), [performance harness](performance-harness.md), [Phase A findings](phase-a-findings.md), [persistence design review](persistence-review.md) |
| Development tools | [Wizard mode](wizard-mode.md) |

Feature guides describe current behavior. Runtime support is limited to the current
protocol, save format, and ruleset listed in
[the roadmap](milestones.md#current-implementation). Pre-release revisions need not
preserve save compatibility or historical rules implementations.

## Documentation maintenance

For every behavior change:

1. Update the relevant implementation guide with user-visible behavior,
   limitations, compatibility, and verification.
2. Update [the roadmap](milestones.md) if scope or status changed.
3. Update [architecture](architecture.md) only when a boundary or durable design
   decision changed; do not use it as a feature changelog.
4. Update the repository README only when the quick start or project-wide summary
   changed.
5. Link every new document from this index. Remove superseded prose instead of
   leaving an unlabelled historical plan beside current guidance.
