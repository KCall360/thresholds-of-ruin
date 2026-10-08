# Reference game specifications

This directory collects researched specifications for games that could inspire
scenario packages or rulesets in Thresholds of Ruin. These are reference and gap
analyses, not claims that the games are implemented or additions to the accepted
roadmap. [The roadmap](../milestones.md) remains authoritative for project scope.

The subsequent Rogue interview has an accepted
[scenario implementation plan](../rogue-scenario-plan.md), separate from the
historical reference specification and using shared TOR mechanics.

| Reference | Research status | Documents |
| --- | --- | --- |
| UNIX Rogue 5.4.4, with RLGallery differences | Source-reviewed; exact Gallery revision and executable parity remain unverified | [Overview and sources](rogue-5.4.4/README.md), [rules](rogue-5.4.4/rules.md), [content catalog](rogue-5.4.4/content.md), [implementation requirements](rogue-5.4.4/implementation.md) |

## Organization for future specifications

Give each game/version its own directory. Keep these four concerns separate:

1. **Overview and sources:** exact edition, pinned evidence, research method,
   confidence, known variants, and unresolved questions.
2. **Rules:** observable behavior, formulas, timing, progression, information,
   world lifecycle, and end conditions. Distinguish verified rules from inference.
3. **Content:** bounded inventories of monsters, items, effects, generation
   weights, and numerical parameters, with source references.
4. **Implementation:** capability gaps against a dated repository snapshot,
   adaptation choices, dependencies, and testable acceptance requirements.

Use stable requirement IDs within a specification. Describe a recognizable
adaptation separately from numerical/behavioral fidelity and exact executable
parity. Cite primary manuals and fixed source revisions where available. Record
uncertainty instead of filling gaps from related games or later ports.

Shared mechanics can become reusable engine capabilities once implementation is
approved. Game-specific balance, symbols, monster behavior, timing, and generation
policy belong to the selected reference profile. A specification does not by
itself require support for plug-in rulesets or historical save compatibility.
