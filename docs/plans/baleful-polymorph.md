# Baleful Polymorph: remaining work and rationale

The implemented contract moved to [Polymorph and body transitions](../reference/polymorph.md).
The original phase was opt-in; after PR #206 the device is in supplemental live
stock, while its destination roster remains emu/snake/troll. Historical phase
notes must not be read as current loot availability or new verification evidence.

## Builder ownership

See [current builder ownership](../reference/polymorph.md#builder-ownership) and
[progression](../reference/progression.md). Stable ownership was chosen so changing
body does not import an NPC's personal class, reroll paid Health, mint points or
launder drained choices. Retained training can be dormant rather than deleted.

## Atomic transition and disclosure

See [current atomic transition](../reference/polymorph.md#atomic-transition-and-disclosure).
The accepted baleful contract is indefinite and death is final. Curses do not block
body reconciliation. Cancellation dismisses/suppresses the transformation, with a
persisted return pending when a living base body cannot yet fit.

Future **temporary** forms remain a separate accepted design: restore the original
HP snapshot on reversion, do not transfer overflow damage, and treat a failed reversion fit
as death rather than protective survival. These are not current runtime
rules. Expiry, replacement between modes and their precise integration/acceptance
still require explicit implementation work and review.

Cooperative target-owned consent is also deferred. Known self-use currently waives
only the user's own save; friendly non-self targets still resist normally.

## Rogue body classification

See [current classification](../reference/polymorph.md#rogue-body-classification).
All NPC packages are enrolled, but a wider weighted form roster, unique/boss
metadata, mixed-template classification and final numeric tuning remain future
work. Explicit author attestation is not an inference from phasing or package
existence. Broader live roster/item balance needs its own approval and evidence.

## Versioned boundaries and verification

[Status](../status.md) owns supported versions and the
[current reference](../reference/polymorph.md#implementation-and-contract-witnesses)
owns implementation/test entry points. Old saves and old clients are rejected;
there is no compatibility importer or silent ownership default.

Historical checkpoint `3ad2ee66` introduced the separately approved protocol bump
for changed-only diagnostics and DirectionOrSelf consumers. The original phase
followed consumables PR #195 (`feae51f`). Its six-case save-61 writer, 44-package
validation, controlled Wraith route (earned XP215/spent XP200), independently
checked 26-creature/81-object Hallucination roster, and task-scoped 100-method
graphical deferral were checkpoint-specific evidence, not a qualification of a
later tree. The Wraith witness restored the exact Health identity with empty
ChooseAgain choices and retained purchase high-water marks; the separate
privileged-builder witness restored six retained ordinary receipts at effective
Strength five. Preserve those different acceptance boundaries when extending tests.

Historical graphical cases were **deferred, not passed**. They do not create a
standing waiver. Future publication must follow the current
[testing policy](../development/testing.md); documentation source inspection is not
fresh runtime, native-client or hosted-CI verification.
