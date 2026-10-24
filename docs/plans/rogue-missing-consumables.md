# Rogue missing consumables, devices and portable light

**Status: reconstruction authorized; fresh implementation and qualification pending.**
The previous local implementation and its test artifacts are unavailable. The
maintainer directed a redo on 2026-10-24. This checkout starts from published main
`511ea466e14892075af030a78d254f1101e7c813`; no lost commit or historical test receipt
qualifies this source. [Current status](../status.md) owns implemented scope.

## Reconstruction decisions and safeguards

The following later maintainer decisions supersede the historical unapproved
candidate profile below. They are preserved decisions, not recovered code or
new test evidence. Recheck pinned source branches while writing original code.

- Keep Rogue 5.4.4 behavior by default, translating existing TOR progression,
  timing, privacy and persistence rather than inventing a second combat system.
  Polymorph is separate. Do not retune or append to existing live weighted pools.
- Unknown devices always request direction. Identified non-directional devices
  skip that prompt; directional devices still require aim. Public affordances
  use the same observer knowledge as item names.
- Room Light is a wand/staff effect, permanent in the actual authored room;
  passage and maze use identifies and spends without illuminating a room.
  Portable torches are a separate addition: one hand, four-cell configurable
  radius, 100,000 active simulation ticks of configurable fuel (1,000 standard
  base turns). Drop preserves flame, stow extinguishes, no frozen catch-up.
  Ignite/extinguish use ordinary paid item work, not automatic equip ignition.
- Cancellation has no save when its contact ray reaches a target. Explicitly
  marked abilities/contributions remain suppressed without deleting ownership,
  purchases or HD; remove invisibility, disguise and exact relevant hold links.
  Later independent invisibility can apply; another cancellation clears it again.
  No new restoration mechanic or global undo of previous conditions.
- Drain Life uses original room/passage/doorway membership and direct Health
  loss, without resistance, armor, LOS or an arbitrary corridor radius. Below
  two Health is a paid refused attempt without charge; no recipients spends a
  charge without Health. Otherwise caster remaining Health is floor(H/2), each
  recipient loses floor(remaining/count). Snapshot recipients before kills.
  Preserve wide Health arithmetic and ordinary hostility/lifecycle contracts.
- Magic Missile uses one Lore-versus-Agility check, 1d4+1 plus the ordinary
  universal attack-damage bonus, no second hit roll, no bounce or fixed short
  ray cap. Nothing spends ordinary time/charge without identification or RNG.
- Gain Strength restores one existing deficit before granting a permanent point
  beyond the unringed remembered maximum, within the normal cap of five.
  No latent boon through poison, refundable acquisition, or new respec UI.
  Raise Level funds only the normal current progression obligation; do not skip
  funded-but-blocked tiers, reduce banked XP or bypass drained-HD regrowth rules.
- Haste uses the shared hasted condition. A potion while hasted clears all its
  actual condition sources and causes the original forced rest; no potion-only
  exception. Persistent wand and finite potion lifetimes need explicit strict
  support, not globally relaxed condition validation.
- Hallucination uses stable observer-specific identities and saved periodic
  shifts (one base turn), not query/redraw RNG. Distort genuine visual appearance
  and the explicit monster-sense presentation exception, not nonvisual Awareness.
  Preserve genuinely recognized stairs and true interaction authority. Extra
  healing cures; no additional poison-cure adaptation is approved here.
- Monster detection refreshes one expiry to 20 base turns per dose, without
  stacking or stale timers. It identifies only if the actual bounded chart is
  available and nonempty at drinking completion. Empty/unavailable results do
  not identify; later revelation never retroactively identifies. Page queries
  remain pure. This intentionally differs from Rogue's never-identify behavior.
- Food and Magic detection are positive-only historical snapshots. Include
  monster-carried magic independently of ground stock; exclude corrosion alone,
  preserve magical contributions even if their numeric effects offset. No
  universal redefinition of magical/theft flags. No live carrier tracking.
- Mapping discloses private topology, stairs, secrets and trap presence, not
  trap family, occupants, items or material. Ordinary room interiors and
  floor/ceiling shells are not magically explored. Preserve richer memory.
  Disconnected or ambiguous components must not acquire fabricated alignment.
- Historical and live charts use bounded ordinary protocol delivery without
  granting sight, navigation or targeting handles. Presentation overflow is
  explicit unavailable, not a truncated or stale chart labelled current.
  Scope metadata and source-free validation must support actual reachable maps;
  never substitute region identity or LOS for a room.

Current-only protocol/save changes and gameplay PR publication/description updates
are authorized; merging still requires the repository's exact-head gates. Local
graphical deferrals are task-specific only, must be re-audited against actual
methods, and do not waive hosted native tests or other local checks. Existing
format numbers remain unchanged until corresponding implementation lands.

Reconstruct in coherent phases: spatial/device/presentation foundations; paid
mechanics and source provenance; observer disclosure and clients; content and
real-caller/persistence acceptance; final performance and full qualification.
Keep one build owner, source-first causal tests, positive fixture controls and
whole affected older contract suites. No historical passing count substitutes
for a fresh result. Preserve release/debug, native/headless and unit/Service/
actual-process distinctions.

Before substantive implementation, establish a verified off-machine checkpoint
containing a Git bundle, decision notes, explicit validation status and a restore
recipe. Private backup authorization does not waive push gates or make an
unfinished checkpoint PR-ready. Never include credentials, saves, caches or
binaries. Back up each coherent local checkpoint and verify restoration identity.
The obsolete cleanup proposals targeted the unavailable environment and must not
be executed against this reconstruction.

## Preservation and evidence

This note was reconstructed on 2026-10-22 from retained design discussion and
source-research findings after the earlier local planning file became unavailable.
It is **conversation-backed reconstruction, not byte-identical file recovery**.
The numeric suggestions were proposed for review, not accepted by the maintainer.
No implementation, build or behavior qualification is established by preserving
this note. References to shared code below were checked against the preservation
checkout; source-behavior findings retain their pinned upstream attribution.

The [Rogue 5.4.4 reference](../research/rogue-5.4.4/README.md) pins upstream commit
`c7c119f893bd2f8255f2ad78d58a32664ddb9a97`. The earlier investigation read the actual
files through the GitHub connector, not just their catalog summaries. Recheck the
relevant branches before implementing edge cases. TOR's shared rules take
precedence over source combat equations, redraw behavior and historical bugs.

## Accepted scope and exclusions

The maintainer selected missing-feature completion instead of an expedition and
balance milestone. The batch includes:

- Original room-Light device **and portable lights**, with scenarios choosing
  which items are available.
- Magic mapping; monster, magic and food detection.
- Gain strength, raise level, haste self, hallucination and its missing cures.
- Magic missile, Drain Life, cancellation and Nothing devices.

**Polymorph is excluded and reserved for a separate PR.** Existing food, loot and
monster distributions are not to be retuned. Live maze/omission enablement,
manual Sneak, complete-run balance, scoring and deep-HD capacity work are not
added to this batch. Do not reopen completed roster or objective foundations.

The maintainer accepted these specific concepts on 2026-10-21:

1. Implement both room Light and portable lights; scenario authors select item
   availability.
2. Hallucination gives each affected observer persistent false appearances that
   change periodically, without changing actual creatures/items or their true
   identified properties.
3. Abilities are explicitly marked cancellable; cancellation removes
   invisibility/disguise as in Rogue. Preserve the shared rules and existing
   ownership/advancement boundaries, rather than deleting HD or purchases.

Cancellation was presented as lasting rather than a short stun. Its exact
delivery, resistance, lifetime/removal and reapplication contracts still need a
frozen decision; do not infer the full profile below from agreement to marked
abilities. **No numerical profile approval was received.**

### Scenario availability

Use existing item catalogs, initial kits and floor-activation/spawn references
where possible. A scenario can expose room Light, portable light, both or neither
by selecting items; defining an archetype need not spawn it. Do not add a global
light-feature switch or a parallel availability system solely for this choice.

Adding new choices to an existing weighted pool changes its distribution even
when old weights are unchanged. Live Rogue placement/stock therefore needs an
explicit content decision consistent with the no-retuning constraint. Feature
fixtures can demonstrate all availability combinations without changing existing
production probabilities. Appending a new finite stage also changes stock and
must not be treated as a distribution-free shortcut.

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

## Historical proposed rules profile — superseded where specified above

Every value and adaptation in this section is a **candidate for review**, not
approved production tuning or an implemented field. Freeze the selected profile
before implementation. Low-level reversible representation choices can remain
with the implementer; the gameplay choices below cannot be silently inferred.

### Light and perception candidates

| Feature | Candidate profile | Basis |
| --- | --- | --- |
| Portable torch | 20-foot/four-cell bounded, occluded radius; 100,000 active ticks of fuel; one grip; one ordinary action to ignite/extinguish | New TOR proposal, not Rogue source behavior |
| Torch movement/storage | Dropping leaves it burning; stowing extinguishes while retaining fuel; no refilling/container simulation in this batch | New TOR proposal |
| Torch clocks | Burns while its location is active, including owner sleep; no frozen catch-up; no haste scaling | Shared active-time adaptation; exact ownership/location handoff must be defined |
| Room Light | Permanent authored-room-cell illumination, 10–19 charges, recognition on use; corridor/already-lit use still spends | Source-inspired, with TOR room metadata and LOS |
| Hallucination | 808–892 base turns; repeats extend; shifts every 500 actor-active ticks | Source duration; shift interval is an unapproved TOR proposal |
| False appearances | Scenario creature pool for creatures, scenario item pool for ground items; no cross-class creature/item substitutions | Unapproved conservative presentation choice |
| Hallucination scope | Distort world names/assets/descriptions, including detected creatures; retain terrain/stairs, usable owner inventory controls and true knowledge | Unapproved scope choice, narrower than source presentation |
| Monster detection | 20 base turns, repeated doses extend, follows observer's current floor, species/location through separate disclosure; no on-use identification | Source duration/identity intent; repeat/floor/channel rules are adaptations |
| Magic/food detection | One-shot snapshots; positive-result recognition; carried magic without ground-item bug, ground food only | Source-inspired with explicit bug correction |
| Mapping | Actor-owned topology/secrets/traps, not occupants/current sight | Source intent with shared knowledge adaptation |

Radius geometry/falloff, equipment placement, dropped-light origin, fuel expiry
and ignition timing are material choices, not implied by the word “torch.”
Current ambient lighting must compose with emitters; moving a torch must not
permanently paint cells. Overlapping emitters, room Light, portal joins, heights
and extinguishing must invalidate the correct sight caches without hidden-state
leaks. Physical illumination may help any observer, not just its owner.

False identities must be stable across queries, redraws, clients and reopening
until an active-time shift. Queries, spectator count and frozen time sample no
randomness. Preserve saved identity/schedule state and bounded work. False
appearances must not leak true names through descriptions, assets, menus, errors,
targeting metadata or renamed history; neither invent hostility nor change
physical action authority. Cure reconciles currently observable information,
not all unseen subjects or stale memory globally.

Proposed cure adaptation: extra healing always clears hallucination; poison
clears it after accepted unsustained exposure, even if further loss is bounded.
Resisted, immune or sustained poison does not. This is not yet approved.

### Cancellation and device candidates

- **Cancellation delivery:** six-cell first-contact ray, Intellect + Lore versus
  Speed + Agility, ties dodge; 100/100 preparation/recovery, one charge, no
  automatic identification. This adopts existing utility-device conventions
  rather than source no-save delivery.
- **Marked abilities:** cancel until explicitly restored, without an automatic
  timer. The mark must cover that ability's rider, passive-trigger and hold
  maintenance paths as well as explicit invocation. Use shared capability
  identity, not species-only rules or a blanket voluntary-action prohibition.
  The same marked capability behaves identically on PCs and NPCs.
- **Reapplication:** reacquiring/reapplying the same canceled ability does not
  bypass suppression. Proposed distinction: remove current invisibility/guise
  sources, but allow a genuinely new independent application later. An ongoing
  veto and one-time removal are different contracts; this candidate distinction
  still requires selection and precise saved source identity.
- **Paid work:** interrupt a wholly canceled pending ability, retaining spent
  costs and releasing unspent reservations. If only a rider is canceled, retain
  ordinary attack damage without that rider. Release exact dependent holds, not
  another holder's link. Never undo previous poison, drains or theft.
- **Preservation:** keep owned grants, HD, purchases, ordinary damage/movement,
  flight and regeneration unless explicitly selected as cancellable. Do not
  destroy progression sources to simulate suppression.
- **Magic missile:** six-cell first-contact Lore/Agility delivery, 1d4+1 Energy
  damage, no bounce, ends on dodge, identifies on use. Payload/check/type are
  proposed TOR adaptations, not approved combat tuning.
- **Nothing:** ordinary paid charge-consuming no-op with no recognition. Do not
  optimize away timing/holds or rewrite existing empty-device admission merely
  to copy another source branch.

**Drain Life candidate:** same authored room plus unobstructed physical line,
independent of illumination; in corridors, two-cell range with obstruction.
Select all other eligible creatures, not just foes. “Living” eligibility must be
defined explicitly rather than silently excluding an undead creature that still
owns Health. Doorway ownership and maze membership need concrete witnesses.
This spatial policy is an adaptation of source room/passage selection.

Retain source half-Health/equal-share rounding, with sacrifice as an unresisted
cost and recipients taking ordinary **Vital** damage with resistance/immunity,
no extra save. Snapshot live Health and targets at atomic resolution. Proposed
100/100 timing, no recognition; no targets spends charge without Health, and
below 2 Health spends neither charge nor Health while elapsed preparation is
not refunded. At admission and at completion, define this consistently without
exposing hidden target counts. Charge, cost, damage, RNG, deaths, growth and hold
cleanup share rollback and ordinary causal XP attribution. More targets than
half-Health correctly produce zero shares. Geometry, Vital resistance and these
paid-resolution details remain unapproved.

### Permanent growth and haste candidates

**Gain strength:** +1 permanent Strength through a provenance-backed acquisition,
not an edit to birth stats or a condition masquerading as permanent ownership.
Candidate cap is the ruleset's permanent attribute cap, currently 5, not its
1,000 derived-value guard. Temporary/ring bonuses do not consume the proposed
allowance; existing poison deficit remains independent. At cap, the proposed
completed dose consumes/identifies but grants nothing. Preserve prior HD choices,
seeds and ordinary blocked-growth rules; define interaction with later purchases,
drain/regrowth and respec before implementing. Cap and paid-no-op are proposals.

**Raise level:** at resolution credit only the positive XP shortfall to the next
ordinary threshold, then use normal Game-owned growth finalization. Preserve
selected future source, seeded Health, owned purchase entitlement and restoration
policy; do not insert an HD or replace earned XP. Candidate: bank credited XP if
growth is blocked; when already funded, credit zero rather than letting repeated
blocked doses stockpile thresholds. Completed use consumes/identifies even at cap
or when blocked. Account for intervening earned XP during preparation and checked
wide arithmetic. These payment/threshold/cap choices are not approved.

**Haste self:** 4–7 base turns, ordinary paid drink and existing nonstacking
action-duration rules. Candidate repetition policy: while this potion's haste
remains active, another dose removes that potion haste and imposes 0–7 base turns
of forced rest; unrelated haste sources remain owned. This source-specific repeat
policy deliberately differs from source's general hasted flag. Settle slow
interaction, zero exhaustion, expiry during paid work and condition identity.
Already-captured action timing remains captured; nutrition, light fuel and expiry
clocks do not accelerate.

## Shared implementation anchors and boundaries

These are reuse points, not claims that the new capabilities already exist:

- [Ambient lighting](../../crates/world/src/lighting.rs) owns cell/region light and
  sight invalidation; it is not portable emitter propagation.
- [Prepared floor groups](../../crates/simulation/src/floor_activation.rs) retain
  bounded room cells. A whole region is not interchangeable with one room.
- [Perception](../../crates/simulation/src/perception.rs) owns source-composed
  sight/invisibility/guise and actor disclosure. Detection and hallucination need
  explicit observer authority, not client decoration or hidden true identity.
- [Rules](../../crates/simulation/src/rules.rs) already have damage, restoration,
  conditions and holds; ordinary movement/action restrictions are insufficient
  for selective cancellation.
- [Action duration](../../crates/simulation/src/action_duration.rs) already folds
  nonstacking haste/slow and opposing rates. Use it rather than changing Speed.
- [Source identities](../../crates/simulation/src/creatures/catalog.rs) and
  [creature state](../../crates/simulation/src/creatures/state.rs) need proper
  permanent-consumable provenance; equipment/condition extra sources are not an
  acquisition ledger.
- [Earned growth](../../crates/simulation/src/earned_growth.rs) retains
  earned/held/spent XP and ordinary thresholds. The reviewed ledger uses u64
  while the award operand uses u32; do not silently narrow a threshold shortfall.
  Keep the existing actual-HD guard and bank/refusal boundaries.
- [Linked holds](../../crates/simulation/src/linked_hold.rs) provide exact-link
  lifecycle cleanup, not permission to release all holds.
- The [default rules](../../crates/simulation/data/default-ruleset.toml) provide
  attribute cap 5, derived guard 1,000, one-turn consumption and two grip units.
  Current Rogue actors use 100 ticks per base turn. These explain the candidate
  profile's units; they do not approve its values. Existing weapon sockets
  require a weapon attack, so ordinary held-tool compatibility needs real support
  without inventing a free hand or fake attack.

## Implementation sequence after resumption and decisions

1. Review one concise rules profile, freeze material choices and independent
   acceptance cases. Request any compatibility decisions explicitly; do not
   invent version numbers. Begin later in an isolated branch based on the
   qualified dependency state, not frozen inputs of another qualification run.
2. Implement room Light and portable emitter composition, item activation,
   equipment/fuel and persistence under one geometry/lighting owner.
3. Implement mapping/detection authority, then hallucination/cures and shared
   Text/ASCII/event disclosure. Agree shared schemas before parallel edits.
4. Implement permanent boon/XP provenance and haste/exhaustion, preserving
   acquisition and ordinary active-time semantics.
5. Implement cancellable abilities across invocation/rider/passive paths and
   exact lifecycle cleanup; finish missile, Drain Life and Nothing executors.
6. Integrate catalogs and scenario-selected availability, then one consolidated
   schema/current-writer/certificate/wire-recording checkpoint and combined
   qualification. One integrator owns shared Effect/disclosure changes and one
   owner each build target; checked inputs stay frozen during verification.

This sequencing preserves a substantial coherent feature batch without combining
it with the rejected food/loot/pressure calibration work. Portable emitters,
hallucination and permanent grants are real shared-system extensions, not merely
new item names. No source-only definition should be called completed gameplay.

## Acceptance criteria

Follow the [testing policy](../development/testing.md), including intended
first-red witnesses, affected runtime/Service/process layers and final required
cross-platform checks. Preserve these distinct boundaries:

- Strict authoring/schema bounds and independent expected values; ordinary PC
  and NPC consumers, no species-only resolver.
- Every effect: paid preparation, pause/cancel/resume, exact-once units/charges,
  atomic rollback, nonmutating queries, freeze, replay and source-free reopening.
- Information: full current floor versus loaded subset, no next-floor activation,
  two observers and spectators, doorway/maze/room bounds, no hidden identity,
  affiliation, targeting or charge leakage; moving-carrier snapshot staleness.
- Portable light: move/equip/unequip/drop/transfer/stow/extinguish/deplete;
  overlapping emitters/ambient room Light; walls, portals and heights; active and
  frozen fuel settlement; no trailing cached light; same PC/NPC rule. Scenarios
  demonstrate room-only, portable-only, both and neither through ordinary content.
- Hallucination: stable redraws, timed shifts, blind/invisible/guise/detection
  combinations, blind cure, transfer/stack/death identities, false names/assets/
  events, preserved true knowledge and independent observers.
- Cancellation: each tagged special independently; unchanged ordinary attacks,
  HD, purchases and unmarked capabilities; two hold owners; source removal versus
  persistent suppression; later reapplication and paid-plan boundaries; no undo
  of historical theft or deficits.
- Growth: repeated doses, intervening XP, funded blocked growth, cap/overflow,
  disabled future source, drain/regrowth, exact seeds/purchase entitlement, no
  replay duplication; Strength with rings, poison, sustain and respec.
- Haste: same/different-source repetition, slow conflict, zero exhaustion,
  expiry during paid work and unchanged nutrition clocks.
- Drain Life: Health 0/1/2/3, zero/many targets and zero shares, hidden/immune
  recipients, maze/doorway/corridor membership, odd rounding, cost versus damage,
  attribution/growth and complete rollback.
- Actual Text/ASCII ordinary selection, preview/confirmation and use; owner-only
  choices and spectator privacy. Synthetic fixtures alone do not close runtime
  or platform acceptance. No complete-run survival or balance claim is implied.
