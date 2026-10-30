<!-- documentation-role: fixed-source -->
# Rogue consumables: retained source findings

These are fixed-edition findings and adaptation rationale from the reviewed
consumables design, not current TOR rules or implementation status. Current
contracts live in [magic items](../../reference/magic-items.md),
[growth/sensing/light effects](../../reference/consumable-effects.md) and
[Polymorph](../../reference/polymorph.md). The [research index](README.md) pins
the external reference; historical checkpoint chronology remains in the
[original design record](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/plans/rogue-missing-consumables.md).

## Source findings that affect the design

### Information and light

The original [Light device](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/sticks.c)
permanently clears the current room's darkness, starts with 10–19 charges and
identifies on use. Corridor use gives feedback without permanent illumination.
This is not a moving torch, nor permission to reveal a room through walls.

[Mapping and food detection](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/scrolls.c)
have different disclosures: mapping identifies and exposes topology, stairs,
secrets and traps, not occupants; food detection snapshots ground food locations
and identifies on a positive result. Source global secret mutation belongs to a
single-player implementation; TOR should preserve actor-owned discovery.

The [detection potion branches](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/potions.c)
require care beyond the catalog summary:

- Monster detection lasts exactly 20 AFTER cycles and displays species, subject
  to hallucination. The inspected branch has **no `oi_know` assignment**, even
  when it shows monsters. Repeated use schedules another fuse rather than a
  clean extension policy.
- Magic detection is a one-shot location display including monster-carried
  magic; positive results identify. Its carried-item scan is accidentally inside
  a ground-item-exists guard. Do not preserve that unrelated dependency by
  accident.

### Growth, haste and cures

The [potion branches](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/potions.c)
and [helpers](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/misc.c)
give these intent anchors, not TOR progression formulas:

- Gain strength adds one and updates the remembered unringed maximum; do not
  import source 3–31 limits or unsigned arithmetic.
- Raise level sets XP just beyond the next threshold and calls ordinary level
  checking; it does not justify directly inserting a TOR HD.
- Haste initially lasts 4–7 AFTER cycles; another dose while hasted clears it
  and causes 0–7 forced-rest iterations. Its special quaff turn handling must not
  silently become a free TOR action.
- Hallucination lasts 808–892 cycles and repeated doses extend. Extra healing
  always cures it. Poison cures only in the branch not protected by sustain
  strength; TOR's existing resistance/immunity adds a separate policy decision.
  Ordinary healing and see-invisible cure blindness, **not hallucination**.

The [cure/display code](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/daemons.c)
clears hallucination while blind without revealing the surroundings. Source
frequent visual rerolls are deliberately different from TOR's accepted persistent
observer identities and periodic shifts.

### Cancellation and damaging devices

The [device branches](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/sticks.c)
show:

- Cancellation affects the first encountered monster without a save, sets its
  lasting canceled state, clears invisibility/confusing touch and breaks guise.
  It does not identify on use. The shared contact branch also releases a
  flytrap hold; TOR must use exact dependent-link cleanup, not global release.
- Magic missile uses directional motion, a magic save and then source hit
  resolution with 1d4 payload, +1 damage and +100 hit. Importing those checks
  would create a second combat system; select a TOR delivery explicitly.
- Nothing spends a charge without an effect or on-use identification.
- Drain Life below 2 Health returns without spending a charge. Otherwise an
  empty target set spends a charge but no Health. With targets, caster Health
  becomes `floor(H / 2)` and each target receives
  `floor(floor(H / 2) / target_count)`; remainder is lost. Odd-Health sacrifice
  therefore rounds up. Actual target membership uses room/passage objects and
  doorway cases, not simply adjacency or LOS. It does not identify on use.

Cancellation's [successful-hit gates](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/fight.c#L187)
cover Aquator corrosion, ice-monster freeze, rattlesnake poison, Wraith/Vampire
drains, flytrap hold/escalation and Leprechaun/Nymph theft. Separate gates cover
[Dragon breath](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/chase.c#L168)
and [Medusa gaze](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/monsters.c#L175).
This is not evidence for deleting every talent, flight or TOR regeneration.
Source flytrap miss damage outside the gate is a historical artifact, not an
intended exception to TOR cancellation. Source regeneration flags alone do not
establish a cancellation rule for TOR's implemented recovery.
