# Consumables and devices: design record and remaining work

**Status:** reconstructed mechanics are implemented. This is retained rationale,
not an active implementation checklist or a current CI certificate. Approved
completed items now participate in [live stock](../reference/scenario-packages.md#live-rogue-content-policy).
Portable torches remain catalog-only; current [Polymorph](../reference/polymorph.md)
was implemented separately.

## Current behavior and canonical owners

| Concern | Owner |
| --- | --- |
| Read/Zap, identification, charges and knowledge-gated aim | [Magic items](../reference/magic-items.md) |
| Growth/Haste, sensing/mapping, finite light and area devices | [Effect reference](../reference/consumable-effects.md) |
| Body/personal ownership, progression and expertise | [Progression](../reference/progression.md) |
| Conditions, timing and effect ordering | [Creature rules](../reference/creature-rules.md), [combat](../reference/combat.md) |
| Private observer presentation and charts | [Perception](../architecture/perception.md), [protocol](../reference/protocol.md) |
| Canonical rooms/passages and retained validation | [Spatial registry](../architecture/spatial-registry.md), [authoring](../reference/scenario-packages.md#canonical-room-and-passage-metadata) |
| Live availability | [Scenario policy](../reference/scenario-packages.md#live-rogue-content-policy) |

## Accepted scope and exclusions

The original batch completed missing features rather than balancing a complete
run: room/portable light, mapping/detection, Strength/Raise/Haste/Hallucination,
Missile, Drain Life, Cancellation and Nothing. Its exclusions of Polymorph and
live-pool changes were batch boundaries, not permanent prohibitions. Later changes
implemented baleful Polymorph and live stock. Manual Sneak, scoring, full balance,
deep-HD storage and temporary forms were not silently included.

## Reconstruction decisions and safeguards

- Reuse TOR timing, progression, privacy, damage and action ownership; do not add
  a second Rogue combat engine or copy source implementation bugs.
- Catalog support is not live selection. Adding weighted choices/stages changes
  distribution even if old weights remain unchanged.
- Payment, attempted effect, identification and disclosure are separate outcomes.
  Unknown devices must not leak their hidden self-targeting behavior through UI.
- Room membership is canonical source metadata, not a region, radius or guessed
  rectangle. Portable illumination and persistent ambient light are separate.
- Hallucination changes saved observer presentation, never target/navigation
  authority, physical item class or factual nonvisual awareness.
- Charts are complete bounded disclosures, not sight or handles. Overflow is
  explicit unavailable, never an apparently complete positive prefix.
- Growth retains exact ownership. Doses do not bypass caps, duplicate funding,
  refund permanent acquisitions or reroll saved Health.
- Cancellation suppresses supported owned sources rather than deleting ownership
  or undoing all later independent effects.
- Pure kernel, actual-process, source-free and native witnesses remain distinct.
  Old pass counts/waivers do not qualify a changed tree.

The [pinned source findings](../research/rogue-5.4.4/consumable-adaptations.md)
preserve deliberate differences in detection, identification, damage and cadence.
Exact current values belong to the references above.

## Remaining work

Numerical/full-run calibration, portable-light live availability, temporary form
modes/consent/roster, broader presentation and measured/native acceptance require
separate decisions and evidence. The frozen earned-route comparator was not
expanded by subsequent live stock. Follow the [testing policy](../development/testing.md).

## Preservation and evidence

The original missing-work reconstruction was not byte-identical recovery of lost
drafts. Its checkpoint sequence, failed attempts, fixture corrections and local
graphical deferrals remain in the
[immutable pre-reorganization plan](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/plans/rogue-missing-consumables.md).
Those historical receipts keep their original scope; they do not authorize new
publication or count as current final-head verification.
