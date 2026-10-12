# Creature rules reference

The simulation evaluates validated scenario content through one modifier fold and
one plan/resolve/apply pipeline. The [architecture](../architecture/overview.md),
[rules principles](../architecture/simulation.md#rules-design-and-review), [scenario format](scenario-packages.md)
and [testing policy](../development/testing.md) describe the surrounding contracts.

## Content and identity

Each scenario manifest names a rules import with optional scenario overrides, or
an explicit standalone rules document. Its validated definitions
include abilities, talents, conditions, creature types, subtypes, classes,
descriptors, ability tags, equipment slots, flags, resources and tuning constants. Species and
templates form the scenario catalog. Content IDs are bounded strings; display
names are separate, non-authoritative text. Unknown IDs, duplicate definitions,
cycles, invalid modifiers and out-of-bound values fail loading.

A game owns one immutable shared ruleset and catalog. A build references that
content, its species and templates by ID, and retains its owned hit-die ledger
and advancement choices. Saves store the content once, its ruleset identity and
effective hash, imported source hashes, and each actor's mutable state and IDs.
Imported rules are resolved once and saved as immutable content; restore does not
re-import changed general rules. Restoring these rules and catalog definitions
does not require their original source directory. A streamed save may still need
uncopied region sources from the matching package; see
[package recovery](scenario-packages.md#saves-and-the-package). Only the current coordinated formats in the
[roadmap](../status.md) are accepted; there is no compatibility importer.

## Derivation and sources

Species, type, subtype, class, template, active talent, equipment and condition
sources feed the same exhaustive modifier vocabulary. The fold handles additions,
sets, protection, flags, abilities and ability-specific values. Flags expand their
explicit data-defined modifiers. Template priorities retain aggregate-then-floor
semantics, reject conflicting sets, and do not depend on map insertion order.

Talent prerequisites form a validated graph. Requirements use permanent build
facts; temporary equipment and conditions cannot unlock owned advancement.
Requirements support subtype membership, species-owned catalog flags, and owned
talents inside `all`/`any` groups. A species flag must originate in that species'
expanded source; a class, template, talent, item or condition cannot substitute.
Parent talents must be active. Draining a parent's HD or invalidating its gates
therefore makes dependent choices dormant; an eligible alternative in an `any`
group can keep a child active. The loader rejects missing references and cycles,
including cycles hidden inside OR alternatives. Declaration order has no effect.
Unsupported choices remain dormant rather than being silently deleted. Build edits
clone once, validate once, derive once, then commit atomically. Rebuilds preserve
injury and reconcile resource balances, reservations, conditions and death.

Equipment slots are ruleset content IDs. Each slot declares its display name,
equipped phrase, compatible item classes, attack capability, and equip/remove
turn multipliers. Rules data also declares the default anatomy. An actor’s
numeric socket index identifies a particular position, including repeated slots;
clients receive the slot ID and disclosed presentation/capability data. Unknown
slot references are rejected in authored content and restored state. Attributes
and resources use named fields. Health rolls remain seeded per hit die
and independent of the rules RNG. The hit-die ledger retains its per-ordinal
seeds: removing and re-adding an ordinal yields the same Health seed and empty
owned choice slots. Maximum-Health changes preserve injury and cannot revive a
dead creature. Pool recovery saturates while retaining fractional active-time
progress; capacity increases do not refill pools. Dice pools are bounded and
use rejection sampling rather than biased modulo selection. Damage uses bounded canonical components with
optional descriptor IDs. Descriptor and category immunity precede reductions.
Equipment contributes modifiers directly; the highest ordered equipped
weapon-capable slot supplies the selected weapon, otherwise the natural attack is
used. Equip and unequip timing comes from the selected slot definition; consume
timing is a ruleset constant. Body armor’s longer timing is authored data. There
is no compatibility combat profile or category-only hazard path.

Inspection reads cached health die sizes and rolls, selected natural/equipped
attack provenance, check operands, effective ability timings and reach. It does
not re-roll health or re-derive the inspected build. Natural attack and selected
weapon are disclosed separately, including item and numeric socket attribution.

## Checks and edge allocation

A check rolls three d6, then adds its frozen attribute, skill rank and permanent
flat modifiers. Meeting the difficulty succeeds; an opposed check succeeds on a
tied total, preserving the same comparator. Natural totals of 3 and 18 have no
automatic failure or success. Both sides of an opposed check use this kernel.
The initial defense baseline remains the ruleset's base plus its paired
attributes; arena measurements, rather than a numeric d20-probability conversion,
provide evidence for tuning.

Positive and negative sources cancel one-for-one. Up to three net check-edge
units add that many d6: keep the highest three for advantage, or the lowest three
for disadvantage. The check is bounded to six draws and allocates no heap memory.
Diagnostics record actual draw order, retained dice, their sum, modifiers and the
inclusive threshold; they do not reroll the result.

Shared effect edge and check-only edge have separate frozen roles. A source that
specifies only a check (including a next-check token or incoming-check condition)
changes the combined check roll, never a damage pool. For a user-owned check,
three units at most are debited from the user's shared edge independently of the
check-only contributions; only the shared remainder reaches later damage. Thus
shared +3 with Readied rolls at most six dice and leaves no damage edge; shared
+5 with Readied leaves +2 damage edge. Check-only penalties likewise cannot
refund shared edge into damage. With shared +5 and check-only −6, the check has
−1 edge while damage retains +2: an accuracy-only penalty does not alter damage
edge. Family deduplication and talent-source limits apply to the check fold, not
the independent shared effect budget. For example, a talent's shared +5 is
capped to +1 on the check, while its shared budget still spends three units and
leaves +2 for damage. `CheckEdge`, ability check edge and incoming check edge
never enter that shared budget. Check traces distinguish unused combined check
edge from the shared budget spent and the remainder reserved for damage.
A target-only resistance check does not spend
the caster's shared edge; an opposed caster check does. Damage keeps its existing
canonical rolled pools, keep-best/keep-worst policy and protection order. A miss
never rolls an absent success damage branch, and unused edge is not banked.

The unmodified 3d6 mean is 10.5. Exact enumeration gives 108/216 success for a
natural target of 11; one, two and three advantage dice give 947/1296,
6688/7776 and 43305/46656 respectively. These are kernel probabilities, not
claims that a class or species is balanced. Pinned streams and full integer
outcome histograms are checked by [dice tests](../../crates/simulation/tests/it/dice.rs)
and the [kernel's exhaustive tests](../../crates/simulation/src/dice.rs).

## Abilities and effects

An ability defines cost, timing, reach, check, success/failure effects, descriptors,
validated ability tags and AI hints. Tags are named ruleset content IDs with
preindexed membership. Modifiers may target one ability or every ability carrying
a tag. The weapon talent chain targets `weapon_attack`, so an added attack inherits
those benefits by declaring the tag. The derived cache keeps the original tagged
modifier and its source for inspection; resolution consumes the per-ability values. Basic melee is an ordinary disclosed ability. Effects are damage,
healing, resource restoration and condition application; items and physics use the
same resolution machinery. Fixed amounts do not consume RNG; rolled amounts use
the deterministic bounded dice kernel. Canonical tuning lives in
[default-ruleset.toml](../../crates/simulation/data/default-ruleset.toml); scenarios import it and can declare
strict, identity-keyed overrides. Ability tags select families of abilities for
modifiers without enumerating each technique ID.

The following values describe the shared default rules, not hard-coded engine
constraints. A validated scenario override can change them.

Canonical Magic Bolt checks Spellcasting against Cognitive defense. Fear checks
caster Intimidation against Spiritual defense and applies on success. All ability
phases scale with Speed using ceiling division and a one-tick minimum.

Magical creatures have Mana capacity `2 + binding`, with another 2 from Arcane
Reserve. Magic Bolt spends 1 Mana at preparation and 1 at resolution. Mana recovers
one point per 800 active ticks, so a two-point cast needs 1,600 ticks of regeneration
in steady state after its initial reserve is spent. The split cost preserves a
meaningful interruption cost; capacity increases extend the initial reserve
without reducing the ongoing price of casting.

Canonical talent increments are:

- Health: +2/+4/+6/+8 through Hardiness, Toughness, Unyielding and Indomitable.
- Stamina, Focus and Mana capacity: +2 per applicable talent, one additional
  two-point ability use.
- Physical defense: +1 per Guard tier. Category wards reduce matching damage by 2.
- Weapon and Bolt damage: +1 flat from the initial damage talent, then one
  additional die per higher damage tier.
- Fear Mastery: advantage on its check and +100 ticks to its condition duration.
  Its edge is ability-scoped and cannot affect another ability. Legacy talent IDs
  remain until the new class slice replaces their catalog.

Single-point and single-die steps preserve granular progression; graduated Health
bonuses and fixed two-point resource increments keep their distinct roles.
Recovery is conservative enough to retain resource scarcity during sustained
casting. These are bounded shared-rules defaults: arena sensitivity checks do not
establish general build parity or multi-encounter adventure pacing. Full-Focus-only
AI also cannot establish the player-controlled burst value of Resolve.
Mental-attribute timing and AI learning from Fear outcomes are not implemented;
the current timing mechanism scales with Speed and AI uses declared hints.

Starting timed preparation during execution captures a plan containing the selected
weapon expression, check operands, edge, costs, timing and effects. Earlier queue
admission stores the requested action, not this frozen plan. Preparations retain
that plan and the
reservation owner. Resume reuses the original plan rather than reinterpreting
changed stats. Start costs are paid once, completion pays the reserved remainder,
and terminal cancellation releases only unpaid costs. Suspension retains the
valid preparation and its reservation so resuming does not pay the start twice.
Check bonuses, thresholds, reach, costs, durations, effect values and dice
expressions stay frozen from preparation. Random rolls occur only at resolution.
Completion rechecks live life, target existence, reach and funding without rebuilding
the captured operands. The short-lived action validator and these saved plans
are distinct; see [action execution](../architecture/simulation.md#actions-and-atomicity).

Pure resolution consumes the plan, a target snapshot and explicit RNG, and emits
an outcome plus optional ID-keyed observer steps. One edge budget is consumed
across caster checks and rolled effects; fixed effects preserve unused edge.
Application validates actor-local candidate states before committing, so failed
effects cannot leave partially mutated actors. Outcomes
record actual changes, including damage limited to remaining health. Death does
not permit later healing to revive the actor. Diagnostics and ordinary execution
use the same resolver; observing cannot change rolls or outcomes.

All creature lifecycle changes produce actor-local outcomes. One application
boundary commits them, reconciles preparations, performs death cleanup and emits
combat events. Equip/rebuild, resource recovery, condition expiry and item effects
use that boundary as well as completed abilities. Resolver preview and commit
share one effect-state kernel; this adds no whole-game clone to an action.

## Conditions and active time

Conditions define a family, lifecycle, refresh/longest/bounded stacking behavior,
source scope, descriptors and a capacity policy. Each family contributes at most
one signed edge to a check, after selecting applicable causers. Fear and Rattled
penalize checks against their source; Braced penalizes incoming Physical checks.
Multiple instances and aliases in one family cannot multiply that edge. Talent
edge contributes at most +1 and −1 before other sources combine. Ability-scoped
check edge is selected before that clamp. The loader rejects flat check-bonus
modifiers on talents and conditions, including through implied flags. Permanent
species/equipment modifiers retain the ordinary flat-accuracy vocabulary.

Readied grants the holder's next check advantage, Distracted gives that check
disadvantage, and Exposed grants the next check against the holder advantage.
They reserve at preparation start, never at queue admission. A frozen plan owns
bounded token identities; competing preparations cannot share one. Reservation
spends the token even if the check misses or work is cancelled. Pause and resume
retain the original operands and reservation. Reserved tokens cannot refresh,
stack, be evicted, or be rewritten, even after their remaining duration expires.
Completion or discard removes their noncontributing reservation records. Those
records and their owners are private simulation state, not disclosed conditions.
A token applied by an effect cannot benefit that effect's already-prepared check.
`edge` is the shared check/effect budget. Global `check_edge`, ability-scoped
`check_edge`, `incoming_edge` and one-use tokens affect checks only. Fear and
Rattled use global check-only edge with `versus_source` scope. Check-only sources never spill
into damage or free shared edge for damage. Plans freeze shared effect edge
separately from the combined check edge. A caster-owned check independently
spends up to the dice kernel's check-edge limit from the shared budget, whatever
check-only sources add or cancel; only that shared remainder reaches effects.
The talent/family check caps do not cap an authored shared damage budget: a talent
granting shared +5 gives the check at most +1 from talents, while a three-unit
check allocation leaves +2 shared edge for damage. Ordinary overrides remain
separate by stat and participant before the applicable contributions combine.

Other temporary stats use ordinary unconditional sources. Unsupported scoped
operations fail validation rather than being silently ignored.

Condition sources are bounded. An oldest-eviction policy deterministically removes
the oldest instance at capacity; rejection is explicit where selected. Permanent
and equipment sources must also derive without temporary conditions. If expiry or
immunity removal leaves an invalid combination of residual temporary sources,
oldest remaining conditions are evicted until derivation succeeds. This bounded
policy prevents expired conditions from persisting through stale caches.

Actors store an active-time baseline. Reads project only the requested actor;
mutations settle that actor, and an active-only deadline index finds future condition
events. Freezing removes deadline entries; thawing rebases and restores them, so
expired frozen deadlines are never scanned by active scheduling. Frozen regions
accrue no active time. Saves retain raw state and its baseline,
so restoration neither double-advances timers nor loses fractional recovery.

## Run policy, disclosure and limits

Adventure and arena are mutually exclusive policy variants. Arena running,
paused/stepping and stopped states are tagged states; policy hooks own action
limits, deadlines and elimination. The rules RNG and event log belong to the game,
separate from run policy. Elimination takes precedence over action and tick
limits, including deaths found while settling the final timestamp; action limits
take precedence when both limits coincide. Stopped arenas retain no step
permission, and invalid controls leave state unchanged. Rule, build, scheduling and persistence failures carry
specific error categories.

Clients receive available abilities with IDs, names, effective reach and costs,
and construct menus and verbs from that list. Inspection discloses named sources
and generic modifiers; traces contain ID-keyed steps and the outcome. Hidden
actors, private state and unavailable abilities remain server-controlled.

Structural bounds live in `simulation/src/limits.rs`: 4,096 catalog entries,
128 modifiers per expanded source, 128 conditions, 256 hit dice, 32 templates,
64 cost holds and 32 damage components. Requirement depth and node count are
bounded independently. Tuning constants are content; these limits protect memory,
validation work and arithmetic and cannot be raised by a scenario.

## Verification and scope

Regression coverage spans loader rejection, modifier ordering, equipment and
conditions, paid preparation/resume, observer equivalence, saved-rule restoration,
disclosure and clients, arena policy and ruleset search. Performance checks cover
single-actor lazy settlement and one perception scene per AI decision. Generated
certificates, save fixtures and wire samples must come from repository tooling.
All required local and CI gates in the testing policy must pass before merge.
The default dungeon scout, guardian and wisp use owned creature builds through
these same rules. The Rogue package remains exploration-only; shared creature
support does not establish Rogue parity or accepted adventure balance.
See [arena evaluation](../development/arena.md) for controlled experiments and
[project status](../status.md) for remaining acceptance work.

## Authoring creature builds and attacks

Creature definitions live in `creatures.species` and `creatures.templates`.
Character and actor `creature` builds name a species, optional templates, initial
attributes, Mana binding, faction and ordered `hit_dice` choices. Each entry owns
its source, training, talent and optional attribute increase. Actor archetypes
can provide a complete creature build. The arena uses an explicit `arcane`
template for magical capability rather than granting Mana merely because an
actor knows a spell. Shared validation rejects invalid or dormant initial
choices before any encounter starts.

A species declares its natural `melee` attack with a skill, check bonus, base
wind-up and recovery, and a damage bundle. For example:

```toml
melee = { skill = "light_weaponry", bonus = 2, wind_up = 90, recovery = 70, damage = { primary = { category = "energy", descriptor = "fire", sides = 6 }, components = [{ category = "energy", descriptor = "fire", amount = { type = "rolled", count = 2, sides = 6, bonus = -1 } }, { category = "keen", amount = { type = "fixed", value = 3 } }] } }
```

Only Heavy/Light Weaponry are valid melee skills. The primary key identifies a
component by category, optional descriptor and die size; omit `sides` for a fixed
primary. Rolled amounts declare `count`, `sides` and signed `bonus`; fixed amounts
declare `value`. The compiler canonicalizes and bounds the bundle. Speed scales
physical phase durations. Permanent melee modifiers and Power Strike combine
before zero clamping; extra dice apply only to a rolled primary. Source inspection
shows the base definition, including every component and its primary designation.

Weapon archetypes use the same attack bundle under `equipment.attack`, with
`equipment.slot = "weapon"`. For example, a fixed Keen weapon is:

```toml
[archetypes.blade]
class = "weapon"
name = "blade"
equipment = { slot = "weapon", attack = { skill = "heavy_weaponry", bonus = 4, wind_up = 60, recovery = 40, damage = { primary = { category = "keen" }, components = [{ category = "keen", amount = { type = "fixed", value = 6 } }] } } }
```

Declare archetypes as tables or as one inline table; TOML cannot extend an inline
table with a later table declaration. An equipped weapon supplies its own skill,
bonus, base phases and damage bundle. The creature supplies its current attributes,
training and permanent melee modifiers. Speed scales the selected attack's phases
once. Identified inventory items disclose their full base attack definition;
unidentified items retain the existing knowledge boundary. Saves retain the source
and reconstruct the same definition on restart.

Mob equipment choices compare their own check score and Speed-adjusted phases,
plus a damage estimate grouped by category and descriptor. Rolled damage uses a
clipped raw mean as a ranking heuristic. It does not predict each roll or include
an enemy's hidden protection. Replacing fire damage with cold is a tradeoff,
so it does not qualify as a straight upgrade under this conservative policy.


## Implementation and test entry points

The typed modifier operations are defined in [rules.rs](../../crates/simulation/src/rules.rs),
with structural limits in [limits.rs](../../crates/simulation/src/limits.rs).
[Derivation](../../crates/simulation/src/creatures/derive.rs),
[planning](../../crates/simulation/src/abilities/plan.rs),
[resolution](../../crates/simulation/src/abilities/resolve.rs) and
[execution](../../crates/simulation/src/abilities/execution.rs) own their respective
phases. Unit and [integration tests](../../crates/simulation/tests/it) cover the
mechanics; [scenario integration tests](../../crates/server/tests/it/scenario_packages.rs)
cover strict authoring and recovery. Generated samples are not hand-maintained.
