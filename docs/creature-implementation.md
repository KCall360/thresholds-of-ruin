# Creature rules reference

The simulation evaluates validated scenario content through one modifier fold and
one plan/resolve/apply pipeline. The [architecture](architecture.md),
[rules principles](rules-architecture.md), [scenario format](scenario-packages.md)
and [testing policy](testing.md) describe the surrounding contracts.

## Content and identity

Each scenario manifest names a bundled TOML ruleset. Its validated definitions
include abilities, talents, conditions, creature types, subtypes, classes,
descriptors, flags, resources and tuning constants. Species and
templates form the scenario catalog. Content IDs are bounded strings; display
names are separate, non-authoritative text. Unknown IDs, duplicate definitions,
cycles, invalid modifiers and out-of-bound values fail loading.

A game owns one immutable shared ruleset and catalog. A build references that
content, its species and templates by ID, and retains its owned hit-die ledger
and advancement choices. Saves store the content once, its ruleset identity and
source hash, and each actor's mutable state and IDs. Loading requires no source
scenario directory. Only the current coordinated formats in the
[roadmap](milestones.md) are accepted; there is no compatibility importer.

## Derivation and sources

Species, type, subtype, class, template, active talent, equipment and condition
sources feed the same exhaustive modifier vocabulary. The fold handles additions,
sets, protection, flags, abilities and ability-specific values. Flags expand their
explicit data-defined modifiers. Template priorities retain aggregate-then-floor
semantics, reject conflicting sets, and do not depend on map insertion order.

Talent prerequisites form a validated graph. Requirements use permanent build
facts; temporary equipment and conditions cannot unlock owned advancement.
Unsupported choices remain dormant rather than being silently deleted. Build edits
clone once, validate once, derive once, then commit atomically. Rebuilds preserve
injury and reconcile resource balances, reservations, conditions and death.

Equipment slot kinds remain engine mechanisms; an actor’s socket index identifies
a particular slot, including repeated rings. Attributes and resources use named fields. Health rolls remain seeded per hit die
and independent of the rules RNG. Damage uses bounded canonical components with
optional descriptor IDs. Descriptor and category immunity precede reductions.
Equipment contributes modifiers directly; the highest ordered equipped
weapon-capable slot supplies the selected weapon, otherwise the natural attack is
used. Equip, unequip and consume turn multipliers are validated ruleset constants,
including the separate body-armor timing. There is no compatibility combat profile or category-only hazard path.

## Abilities and effects

An ability defines cost, timing, reach, check, success/failure effects, descriptors
and AI hints. Basic melee is an ordinary disclosed ability. Effects are damage,
healing, resource restoration and condition application; items and physics use the
same resolution machinery. Fixed amounts do not consume RNG; rolled amounts use
the deterministic bounded dice kernel. Canonical tuning lives in
`crates/simulation/data/default-ruleset.toml`, with explicit copies in scenarios.

Canonical Magic Bolt checks Spellcasting against Cognitive defense. Fear checks
caster Intimidation against Spiritual defense and applies on success. All ability
phases scale with Speed using ceiling division and a one-tick minimum. Mana
recovers one point per 600 active ticks; a two-point cast therefore needs 1,200
ticks of regeneration in steady state after its initial reserve is spent.

A bounded sustain comparison used 80 runs (HD 4/8, ten seeds, both orientations,
600/1,000-tick recovery), each lasting 10,000 ticks against a high-health opponent
across a sight-clear, impassable waist-high barrier. No melee occurred. Reducing
recovery from 1,000 to 600 increased completed bolts from a mean of 6.5 to 10 per
run while retaining low/depleted Mana. Ordinary AI movement and interruption also
contribute to observed downtime. This supports the recovery choice, not a claim
of general win-rate balance; short melee-closing encounters showed no win-rate
change. Talent numeric values are retained.

Admission captures a plan containing the selected weapon expression, check
operands, edge, costs, timing and effects. Preparations retain that plan and the
reservation owner. Resume reuses the original plan rather than reinterpreting
changed stats. Start costs are paid once, completion pays the reserved remainder,
and interruption releases only unpaid costs.

Pure resolution consumes the plan, a target snapshot and explicit RNG, and emits
an outcome plus optional ID-keyed observer steps. One edge budget is consumed
across caster checks and rolled effects; fixed effects preserve unused edge. Application commits on a
candidate game so failed effects cannot leave partially mutated actors. Outcomes
record actual changes, including damage limited to remaining health. Death does
not permit later healing to revive the actor. Diagnostics and ordinary execution
use the same resolver; observing cannot change rolls or outcomes.

## Conditions and active time

Conditions define refresh, longest-duration or bounded stacking behavior, source
scope, descriptors and a capacity policy. Fear is ordinary content. Source-scoped
edge modifiers affect only checks against the recorded source. Other temporary
stats use ordinary unconditional sources. Unsupported scoped operations fail
validation rather than being silently ignored.

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
separate from run policy. Rule, build, scheduling and persistence failures carry
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

## Integration scope and acceptance

Regression coverage spans loader rejection, modifier ordering, equipment and
conditions, paid preparation/resume, observer equivalence, source-free saves,
disclosure and clients, arena policy and ruleset search. Performance checks cover
single-actor lazy settlement and one perception scene per AI decision. Generated
certificates, save fixtures and wire samples must come from repository tooling.
All required local and CI gates in the testing policy must pass before merge.
