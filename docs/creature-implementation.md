# Creature rules reference

The simulation evaluates validated scenario content through one modifier fold and
one plan/resolve/apply pipeline. The [architecture](architecture.md),
[rules principles](rules-architecture.md), [scenario format](scenario-packages.md)
and [testing policy](testing.md) describe the surrounding contracts.

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
re-import changed general rules. Loading requires no source
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

Equipment slots are ruleset content IDs. Each slot declares its display name,
equipped phrase, compatible item classes, attack capability, and equip/remove
turn multipliers. Rules data also declares the default anatomy. An actor’s
numeric socket index identifies a particular position, including repeated slots;
clients receive the slot ID and disclosed presentation/capability data. Unknown
slot references are rejected in authored content and restored state. Attributes
and resources use named fields. Health rolls remain seeded per hit die
and independent of the rules RNG. Damage uses bounded canonical components with
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
`crates/simulation/data/default-ruleset.toml`; scenarios import it and can declare
strict, identity-keyed overrides. Ability tags select families of abilities for
modifiers without enumerating each technique ID.

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
- Fear Mastery: +1 to its check and +100 ticks to its condition duration.

Single-point and single-die steps preserve granular progression; graduated Health
bonuses and fixed two-point resource increments keep their distinct roles.
Recovery is conservative enough to retain resource scarcity during sustained
casting. These are bounded shared-rules defaults: arena sensitivity checks do not
establish general build parity or multi-encounter adventure pacing. Full-Focus-only
AI also cannot establish the player-controlled burst value of Resolve.

Admission captures a plan containing the selected weapon expression, check
operands, edge, costs, timing and effects. Preparations retain that plan and the
reservation owner. Resume reuses the original plan rather than reinterpreting
changed stats. Start costs are paid once, completion pays the reserved remainder,
and interruption releases only unpaid costs.

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
This tuning does not promote the three authored dungeon mobs, and Rogue remains
exploration-only.
