# Growth, sensing and light effects

This is the current contract for Gain Strength, Raise Level, Haste potions,
detection/mapping, portable light and Nothing/Room Light/Drain Life devices.
[Magic items](magic-items.md) owns shared payment/identification, healing,
Hallucination and other device families; [progression](progression.md) owns
advancement. [Spatial registry](../architecture/spatial-registry.md) owns canonical
room/floor membership. Catalog support does not imply [live stock](scenario-packages.md#live-rogue-content-policy).

## Shared lifecycle and failure boundaries

These are ordinary paid actions. **Preparation start**, not queue admission,
reserves the exact item. Completion settles actor time and executes retained
effects on the ordinary candidate transaction. Pause retains operands and claims;
a query/selection does not consume, identify, roll or run effects. Hard validation,
arithmetic or publication failure rejects the enclosing candidate atomically.
A legitimate paid no-op can instead spend time/stock without changing the world.
See [combat](combat.md) and the [simulation cycle](../architecture/simulation-cycle.md).

| Effect | Bundled discovery | Nonpositive boundary |
| --- | --- | --- |
| Gain Strength, Raise Level, Haste | `on_use` | A valid capped Strength dose still identifies |
| Monster/Food/Magic Detection | `self_effect` plus final admitted-chart receipt | Empty/unavailable chart does not identify |
| Magic Mapping | `on_use` | Supported admitted use does not require detection's positive-only predicate |
| Nothing | `never` | Charged no-op never identifies |
| Room Light | `on_use` | Passage or already-lit room still pays and identifies |
| Drain Life | `never` | Weak refusal/empty census never identifies |

## Gain Strength

At completion:

1. If actual restorable Strength loss is positive, restore **one** point and stop.
   Do not also acquire a permanent point, even if permanent headroom exists.
2. Otherwise attempt one nonrefundable permanent acquisition.
3. At the cap, publish `Unchanged`; do not create a refund, ordinary attribute
   opportunity or transferable purchase.

Under bundled rules, ordinary initial/advancement Strength is capped at **5**.
Ordinary contribution plus consumable-acquired Strength caps at **7**; innate
source adjustments fold afterward. These are not limits on every final displayed
species/template/equipment/condition contribution. Ordinary purchases respect both
their own and the combined cap.

**Example:** ordinary Strength 4 plus one acquired point permits one ordinary
increase to 5 (combined 6), not a second ordinary increase to 6. Ordinary 4 plus
three acquired points is already at the combined cap and cannot buy another.

Acquired points retain nonrefundable provenance separate from HD-owned attribute
choices. Refunding/draining an ordinary choice removes that contribution, not the
acquired points, and cannot turn magic into fresh ordinary opportunities. Initial
attribute rewriting cannot rebase acquisition history. Body-enrolled forms retain
gains they cannot currently express; [body saturation and custom caps](polymorph.md#builder-ownership)
own that exception. Rebuild and eligibility use the ordinary checked path,
including retained Intellect-learning rules.

Source: [`stage_gain_strength`](../../crates/simulation/src/creatures/state.rs),
[`acquire_strength`](../../crates/simulation/src/creatures/mod.rs),
[folding](../../crates/simulation/src/creatures/derive.rs).
Witnesses: [caps/provenance](../../crates/simulation/src/creatures/growth_dose_tests.rs),
[paid doses/rollback](../../crates/simulation/src/abilities/growth_dose_tests.rs).

## Raise Level

A dose grants **the exact XP credit to fund one additional ordinary obligation
beyond everything already funded by banked XP**. It does not directly append an
HD, award a fixed amount, or repeatedly top up the first visible threshold while
several levels are already funded but blocked.

The calculation uses surviving personal HD, highest applied personal tier,
retained XP, restoration obligations and configured thresholds. Already-funded
regrowth is accounted before forward tiers. Checked wide arithmetic and bounded
threshold-sum search avoid iteration once per projected level. Exhausted
representation is an error, not a wrapped award or silent plateau.

The owning Game finalizer applies eligible growth. A lawful selected source uses
ordinary ownership, Health streams, attribute opportunities and pools. Disabled
growth, unavailable restoration source or the current actual-row guard can leave
credit banked. Repeated doses fund successive obligations even while application
is blocked; later selecting a lawful source spends the retained credit. Dead
actors do not gain an alternate HD-append path.

**Example:** retained XP fully funds two unapplied obligations and part of a third.
One dose supplies the third's remainder. A second funds the fourth, even if future
growth remains disabled; it does not award the same first-threshold remainder.

Source: [`next_unfunded_credit` / `projected_tier`](../../crates/simulation/src/earned_growth.rs),
[growth staging/finalization](../../crates/simulation/src/creatures/state.rs).
Witness: [paid, blocked and reenabled growth](../../crates/simulation/src/abilities/growth_dose_tests.rs).
[Progression](progression.md) distinguishes personal obligations from exact lost
body-package rows and free explicit restoration.

## Haste potion

The potion shares the named `rogue.hasted` condition with the device, but adds a
finite dose and overdose rule. Choose the branch from **actual condition presence
at completion**, after time settlement and earlier ordered self effects—not net
speed/duration multiplier at admission.

| Completion state | One rules-RNG draw | Result |
| --- | --- | --- |
| No active named Haste | `1d4 + 3` base turns (4–7) | Apply finite Haste |
| Any active named Haste | `1d8 - 1` base turns (0–7) | Remove all named Haste applications; apply forced rest only if positive |

The prepared source's base-turn ticks convert the draw to actor-active duration.
Haste does not accelerate its own clock. Expiry is exact; frozen time does not
catch up. Haste expiring during drinking therefore does not cause overdose;
Slow-masked Haste still does, because named presence—not the net multiplier—is
checked. Removal clears timed and until-removed applications from every source,
not unrelated innate/equipment Speed modifiers. Slow remains.

Positive overdose rest blocks voluntary actions and pauses preparation under the
shared condition contract. Zero rest creates **no zero-duration timer**. The dose
still consumes/identifies under `on_use`. Shared Haste/Slow scaling/cancellation
belongs to [magic items](magic-items.md).

Source: [closed receipt and condition shapes](../../crates/simulation/src/abilities/haste_dose.rs),
[resolution/application](../../crates/simulation/src/abilities/resolve.rs).
Witness: [duration, masked overdose, ordered effects, save/expiry](../../crates/simulation/src/abilities/haste_dose_tests.rs).

## Detection and Magic Mapping

Charts are private information, **not** sight, explored-map cells, targeting
handles, navigation or pursuit authority. Acquisition uses the actor's canonical
floor, not just resident regions. Live Monster Detection follows the current
floor; one-shot historical charts keep acquisition samples after travel.
Complete scope preparation is load-only and gives frozen members no turns.

| Effect | Content | Lifetime |
| --- | --- | --- |
| Monster Detection | Living other creatures with combat/Health, regardless of allegiance; not self, dead creatures or noncreature bodies | Live census while active; 20 prepared base turns; refresh replaces remaining lifetime rather than adding |
| Food Detection | True ground food, not carried stock or every nutritious item | Historical acquisition |
| Magic Detection | Ground and other actors' carried magical stock; excludes owner's pack | Historical acquisition |
| Magic Mapping | Sparse certified room surfaces, passages, doors, stairs, secret doors and trap presence | Historical acquisition |

Monster sense bypasses ordinary eyes/guise, not personal-identity privacy. Its
own observer cosmetic channel may reflect Hallucination. Magic detection tests
authored magic, curse, permanent armor protection or nonzero armor/hit/damage
enchantment. Corrosion/armor penalty alone is not detection magic; offsetting
enchantments do not cancel positivity. This deliberately differs from older
`Item::magical`/theft classification; potency alone is not the predicate.

### Completion, unavailable results and later queries

Traversal, allocation, sorting, page envelopes and total delivery are budgeted
before complete installation. Missing projection or exceeded budget gives explicit
**unavailable**, never a successful resident-only or positive truncated prefix.
A paid unavailable Monster Detection still starts its full lifetime. Valid dose
payment is not refunded because a chart is unavailable.

Detection identification needs the paid completion's complete admitted **positive**
chart. Starting/refreshing its marker, changing internal state, or a creature
arriving later is insufficient. Later queries are pure and never retroactively
identify the dose. Food and Magic Detection retain an older positive historical
sample after an empty/unavailable attempt, with an explicit latest-attempt status;
the old sample does not make the new attempt positive. Monster Detection instead
returns its live result without historical fallback.

Mapping's source certification is checked before paid admission. Supported admitted
use identifies through `on_use`, not detection's positive-only rule. It does not
open/discover secret terrain in ordinary map knowledge or disclose trap families.
A historical sample can remain readable after current support changes.

The [wire chart contract](protocol.md#detection-and-mapping-charts) owns summary
states, snapshot/context checks, bounded pages and atomic client installation.
Chart rows are not actionable world coordinates.

Sources: [census](../../crates/simulation/src/detection.rs),
[lifetime/receipt](../../crates/simulation/src/detection_lifecycle.rs),
[magic predicate](../../crates/simulation/src/detection_magic.rs),
[mapping](../../crates/simulation/src/mapping_chart.rs).
Witnesses: [census/privacy](../../crates/simulation/src/detection_tests.rs),
[refresh/freeze/identification](../../crates/simulation/src/detection_lifecycle_tests.rs),
[topology/budgets](../../crates/simulation/src/mapping_chart_tests.rs).

## Portable torch

The bundled torch is a nonstackable tool, one real held-light grip, radius **4**,
with **100,000 fuel ticks** (1,000 ordinary 100-tick base turns). It is catalog-only,
not default live stock. Other authored lights use their own validated policy.

Ignite/Extinguish are paid exact-item actions. A living owner must carry/equip the
item, with positive fuel and an actual state change. Equip alone does not ignite;
queries do not spend fuel. Failed grip admission, cursed **dropping** and invalid
ignition reject atomically. Ordinary paid cursed Unequip is different: its blocked
completion can teach the curse while spending time; see [equipment](equipment.md).

| State/transition | Result |
| --- | --- |
| Equipped, lit, active region | Burns elapsed simulation ticks |
| Lit on active ground | Emits/burns independently of former carrier |
| Drop lit equipped torch | Preserves flame and remaining fuel |
| Pickup into inventory or stow/unequip | Extinguishes without refill |
| Frozen region or nonemitting carried inventory | No burn or later catch-up |
| Fuel reaches zero | Exact extinction; cannot reignite without fuel |

The elapsed interval burns at the old physical location before same-tick physics
moves. Expiry is a scheduler boundary, not a client redraw guess. If fuel expires
during paid Extinguish, the now-unavailable exact item interrupts work rather
than resurrecting flame/fuel.

Illumination uses the emitter's frame-aware unobstructed eye scene, combined with
ambient illumination; it does not mutate ambient World light. Door/source/observer
motion, capabilities, portals and extinction invalidate the optical witness;
unchanged positive fuel alone need not rebuild an unchanged map. Host optical
handoff settles required regions before subsequent observation/time progression.
Radius does not permit shining through walls or revealing concealed identities.
Exact fuel/lit state survives checkpoint reopening.

Source: [fuel, scenes and handoff](../../crates/simulation/src/portable_light.rs).
Witness: [paid transitions, grip, freeze, expiry and restore](../../crates/simulation/src/abilities/portable_light_tests.rs).

## Nothing, Room Light and Drain Life devices

All three use ordinary charged Zap and knowledge-gated aim. Unknown types still
ask for a direction without revealing hidden self targeting; known self-targeting
types can omit that choice. Type knowledge does not reveal every instance's charges.
Actual [Zap timing](magic-items.md#zap-timing) is one captured actor base turn under
bundled rules, with ordinary duration scaling and **no additional recovery**.
The referenced ability's authored 100/100 timing does not replace item timing.

**Nothing** starts with 3–7 charges. Charged completion spends one charge/time with
no effect or automatic identification.

**Room Light** starts with 10–19 charges. Charged completion identifies and lights
the canonical room's owned floor/wall/ceiling/door surfaces, including across region
boundaries—not all region cells or neighboring rooms. Passage/maze/already-lit room
is a paid no-op. Complete surface validation precedes mutation; remote load grants
no turns. Ambient light persists independently of fuel and is not map/detection.

**Drain Life** starts with 3–7 charges and never automatically identifies. Use
current settled caster Health at completion:

1. Below 2 HP, publish paid `TooWeak` refusal: spend ordinary attempt time but
   release the reserved charge without payment.
2. Census complete canonical membership for living **other** combat/Health
   creatures. Allegiance, sight, direction and distance are not the criteria.
3. No recipients: spend a charge, change no Health.
4. Otherwise freeze starting Health `H` and recipient count `N`. Caster retains
   `floor(H/2)`; apply direct loss `floor(floor(H/2)/N)` to each recipient, capped
   by that recipient's current Health.
5. Apply through ordinary death/growth cleanup, without recalculating the divisor
   after an early recipient dies.

For `H=9`, `N=2`, caster loses 5 and keeps 4; each target loses up to 2, dying if it
has at most 2 HP. For `H=2`, `N=2`, caster keeps 1 and target loss is 0. Zero share
is not rounded up, but surviving target contact consequences still release holds/
guise and can give eligible AI an anonymous location alert. No visible identity
is invented. Direct loss bypasses armor/resistance, not lifecycle cleanup; shared
damage-activity accounting prevents repeated quiet-time resets within the batch.

Membership is topological: room interior reaches its room; doorway adds its
associated passage interior; passage reaches that passage and affiliated doors,
not neighboring room interiors. Frozen recipients can lose Health without turns.
Hidden recipients/count are not enumerated to the caster. Missing loaded members
are an error, never a smaller successful census.

Sources: [room surfaces](../../crates/simulation/src/spatial_scope/room_light.rs),
[Drain census/arithmetic](../../crates/simulation/src/spatial_scope/drain_life.rs),
[abilities](../../scenarios/rogue-exploration/rules.toml),
[item policies](../../scenarios/rogue-exploration/catalog/items.toml).
Witnesses: [Room Light](../../crates/simulation/src/abilities/room_light_tests.rs),
[Drain weak/empty/zero/death/rollback](../../crates/simulation/src/abilities/drain_life_tests.rs).
