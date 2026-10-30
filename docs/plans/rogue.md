# Rogue adaptation: remaining work

The goal is Rogue's generated-dungeon, survival and descent/retrieval/ascent
experience using **shared TOR mechanics**. The [pinned Rogue reference](../research/rogue-5.4.4/README.md)
is evidence for content and behavior intent, not a second combat/timing engine or
an executable-parity commitment. [Current status](../status.md) owns scope;
[shared requirements](requirements.md) own reusable systems.

## Adaptation decisions

- Descent has no preset final floor, as in the pinned reference. Compact lazy
  descriptors use an explicit operational depth limit for safe execution; that
  limit is not a Rogue victory condition or a seed-selected dungeon length.
- Floors persist on revisits; no regeneration or ascent-specific loot suppression.
- Nine regions form a 3×3 floor, stone-filled with broad portal joins; fine-grained
  declarative stages carve rooms/corridors. Region boundaries need not be walls.
- Generate a complete floor group within the preparation/loading horizon before
  traversal requires it; distinguish speculative preparation, publication and
  active residency. Use independent stage streams and pinned versions.
- Named paired stairs connect corresponding endpoints. Backtracking is generally
  allowed; gates require separate scenario design. Falling through a portal is
  physics, not a stair action. Invalid arrivals consume no movement time.
  Stair locations and the initial entry vary deterministically with the seed.
  Returning to a paired stair deliberately differs from Rogue's separately
  selected arrival cell after regenerating each floor.
- Reproduce nine room slots, connecting corridors, omissions, darkness, secrets
  and mazes as their shared stages become supported. Required routes may contain
  discoverable secrets. Ordinary room entrances have no usable doors.
- Preserve Rogue's 26 species, item catalog and distinctive capabilities as
  standard TOR content, with scenario-restricted generation pools. Species are
  constructed from types, subtypes, HD and eligibility-gated talent trees and
  tuned in the arena for comparable behavior, not copied XP or combat equations;
  see the [progression design](progression-design.md#rogue-roster-as-constructed-creatures).
  Do not label fixed-damage placeholders complete species.
- TOR owns timing, progression, capacity, AI knowledge, physics and spatial sight.
  Exact Rogue combat numbers, action cycles, RNG, regenerated floors, room-wide
  revelation, capacity and scoring are not required parity targets.
- Use shared nutrition/healing, anatomy equipment, NetHack-style character
  knowledge, reusable conditions and ranged actions. Per-item unusual rules,
  including scare-monster scrolls, need individual decisions.
- Hallucinated identities persist per observer with occasional timed shifts.
  Population advances on active floors, including revisits, without frozen-time
  catch-up. Gold/item pursuit requires actor knowledge.
- There is exactly one Amulet of Yendor identity. On every actual player entry
  to depth 26 or deeper, including revisits, create it once if absent or relocate
  that same item to a random valid supported/reachable ground cell on the entered
  floor unless the player carries it. Existing piles, the entrant's cell and
  reselecting its current cell are valid outcomes. Queries, speculative preparation,
  NPC entry and frozen time never trigger placement. Depth 26 is an eligibility
  threshold, not the final floor. Intentional surface traversal wins only while
  carrying it; refusal without it costs no time. Internal backtracking never
  requires the Amulet.
  The objective and intentional surface escape are implemented; expanded
  presentation/scoring remain open.

## Existing generated-floor and starter foundation

The `rogue-exploration` package describes an open dungeon with one compact
grid-chain family. Each lazily materialized floor has nine 26×7×2 regions,
deterministic group preparation/publication and independent region persistence.
Initial entry and stair ownership/locations are seed-selected; the initial entry
is a safe room cell distinct from the stairs. Floor 1 has an intentional surface exit on its upward stair.
Depth 26 has an ordinary onward connection.

The package's operational limit is 1,000,000 materializable floors. No floor
stubs, index entries or geometry are created for unvisited depths. Structural
addresses continue beyond the operational limit; attempting to cross it fails
without advancing the game or creating a terminal objective. Existing generic
per-group, graph-query, validation and preparation budgets still apply.

Room lighting is depth-dependent and corridors are dark; the graph may include
extra connections. The [starter slice](../play/rogue.md) adds explicit Warrior/Mage
presets, an equipped mace and ring mail, finite bandages/rations, and ordinary
all 26 Rogue species selected by the complete authored population policy. The
initial HD point remains unspent and future growth explicitly
selects the matching class. Floor profiles are separate from geometry preparation:
first successful selected-character entry evaluates each finite stage once, and
returns retain entities, spent stock and recorded skips. NPC abstract-stair
traversal is disabled by typed scenario capability. Ordinary purchases expose
the starter talents and exact Mage forms; foes and Bind Wounds use shared combat,
finite supplies and cooldowns. Four weighted actor attempts use authored depth bands,
exact racial rows after depth 26 and finite C/Y/T/M/X/V/G/J birth stock at
15%/30%/50%/40%/30%/20%/20%/70%.
The equal ration/bandage pool is provisional TOR scenario tuning, not original
loot equivalence. Earlier population contracts have focused source-qualified
coverage; the L/N/O theft/gold additions still need their own qualification. Equipment/stat values
and weights remain provisional.
[Playable acceptance](../play/rogue.md#scope-and-verification)
and calibration retain their separate requirements.
W adds five undead racial rows with parentless DR2 paid on HD1, passive 1d6
contact and a separate Spiritual-checked actual-HD loss. Its depth 14–23 selection
has zero carry. Acceptance separates one controlled persisted Service composition
from an autonomous actual-process prefix. The former uses ordinary purchases,
eight emu kills, real survived W drain, W waiting under ordinary external control,
its ordinary kill, source-free journal reopen at 195 XP, then ninth-emu paid
regrowth and Lore repurchase. It retains the original ordinal Health seed and
empty ChooseAgain row without buffs, injected XP or a restoration consumable.
The autonomous process stops at the survived 160-XP/HD1 drain and requires exact
source-free checkpoint reopen. Both acceptance cases and the production depth-14
W birth/receipt/source-free checkpoint case have passed. Autonomous recovery,
starter and complete-run survival remain open.
Shared source-derived flight, stationary movement and authored erratic probability
now support B/K/F/G. The stationary plant F owns GR1 on HD1 and GR2 on HD6;
Clutch creates a shared linked hold and later qualifying attack-tag HoldDamage
primaries use its exact live-link increment. Ordinary paid Escape and per-holder
protection use the same action, lifecycle and persistence boundaries. B/K/G keep
their authored flight, natural damage and owned advancement rows. Focused unit,
Service, recipe-combat and nongraphical process witnesses cover these mechanisms;
native input and complete cross-platform qualification remain merge requirements.
They do not establish pressured escape or complete-run balance.
L/N/O add the shared [theft and Gold](../reference/items.md#theft-and-gold)
contracts: checked Cognitive currency/magical-item riders, preparation-frozen
same-floor relocation only after actual transfer, protected held units and
ordinary finite inventory/death recovery. L has three fey HD with TR1(1), TR3(2),
natural 1d1 (fixed one damage), depths 6–15 and finite birth Gold. N has three fey HD with TR2(1),
TR3(2), natural zero damage and depths 10–19. O has one humanoid Warrior HD,
W02(1), natural 1d8 and depths 4–13. Source-owned quantity expressions resolve
checked positive u64 ranges using explicit birth/preparation depth. The appended
50% per-present-room Gold stage preserves earlier stage ordering and random
streams. Together with M/P/X and A/D, production selects all 26 species. The [perception cohort](../play/rogue.md#gaze-invisibility-and-object-guise)
owns Presence-bound GZ1, typed invisibility and one initial purchased object
guise/ambush. M participates in the pinned upper-boundary redistribution and
remains eligible from depth 18 onward; P/X retain finite depth bands 15–24
and 16–25. M/P/X qualification landed with PR #168; the current A/D additions
retain their own focused checks and required complete cross-platform CI.
The unique objective helper is unchanged. Focused simulation, strict saved-input,
source-free load-only recovery, catalog, Service and ordinary-process witnesses
cover the mechanics. The process uses a labeled synthetic victim and ordinary
checked equipped recovery; it does not establish pursuit or survival balance.
This batch adds no UI or 3D features. Complete cross-platform CI remains a merge
requirement.
The retrieval/ascent victory is implemented; complete survival balancing remains open.
Persistent floors and paired return stairs are intentional TOR adaptations,
not exact replication of Rogue's regenerated floors and independent arrivals.

[Generation reference](../reference/generation.md) describes supported recipes,
validation and actual-client coverage. This foundation does not need to wait for
all creature/item mechanics. Its remaining performance acceptance, particularly
save/flush tails, belongs to the [performance guide](../development/performance.md).
Do not mistake “implemented” for closure of those measurements.

## Next increments

### Layout, perception and discovery

Reusable secret/discovery stages, optional strict room omissions and dark-room
mazes are implemented. The live Rogue package now selects omissions and mazes with
explicitly provisional conservative frequencies; the
[generation reference](../reference/generation.md#optional-room-omissions) owns
those values. Numerical calibration remains separate. Absent policies or no
realized omissions/mazes preserve their legacy geometry contracts; enabling these
policies does not promise seed-identical floors. Generation reserves the full
authored trap maximum and landing namespace before maze carving, preserves required stair/portal identities
and body clearance, and excludes maze cells from trap placement. See the
[generation reference](../reference/generation.md) for the owning authoring contract.

Ordinary and forced-maze process witnesses cover paid Search, paired-stair
traversal, source-free reopening and receipt-preserving revisits through public
observations. Their `omission_public_search` v1 wrapper preserves Controller
revision 7 and the cumulative 2,500-iteration navigation-loop cap. This bounded
public-search diagnostic policy establishes topology and persistence, not general
AI, survival or balance. The merged batch also provides adjacent
tactile Search and capped Awareness-based nonvisual disclosure. The latter does
not grant genuine sight or identify concealment/disguise; tactile Search still
requires its adjacent Perception check. Neither grants whole-room revelation. See
[perception](../architecture/perception.md) for their separate boundaries.
This completed live-content activation does not authorize the separate tuning
campaign or other implementation milestones. The frozen earned-route comparator
and its qualification boundary remain unchanged.

Lighting already exists; extend it without replacing TOR
LOS with whole-room revelation. Typed sight/invisibility, mutual-sight gaze,
single-use object guise and adjacent timed Search support the authored M/P/X
cohort. Hallucination is implemented; broader discovery acceptance and numerical
calibration remain open.

Acceptance: connectivity after discovery, no invented visible-route guarantee,
consistent movement/sight across carved joins, actor-specific discovery/memory,
recovery of mistaken identities and shifts, and unchanged stage-stream isolation.

### Complete shared builds/effects and survival

Use the implemented creature builder, attributes, sources, advancement, generic
effects, equipment, immutable ordered strikes and paid preparation. Natural
sequences and explicit success/failure riders use the same PC/mob pipeline;
Rogue species should author these shared sources rather than add a species-only
resolver. Earned XP, explicit future-source HD growth, paid regrowth after drain
and bounded direct-push/fall attribution are shared foundations. Add the
damaging-condition attribution adapter as those effects arrive, retaining the
same atomic award contract. Extend the missing content tables,
nutrition and capacity under [shared requirements](requirements.md), and measure
progression/recovery tuning with complete builds. Do not reimplement the completed
rules refactor as a prerequisite.

Acceptance: equivalent PC/mob rules; deterministic progression/CR/XP; recorded
advancement reversal and zero-HD death; retained innate species properties;
source-granted defenses, stacking and active-time recovery across freeze/replay.

#### Shared nutrition foundation

The shared foundation covers nutrition, eating, starvation and authored stochastic
condition triggers through the shared status gate. The Rogue package now explicitly
enrolls Warrior/Mage, keeps all 26 selected foe recipes exempt, and makes its
existing ration edible.
Its layered acceptance contracts do not establish complete-run survival,
production balance or final Rogue configuration. A creature requiring nutrition must use an authored shared rules
policy. Fixture numbers are deliberately small and have no balance claim.
The rules policy is explicitly tagged `kind = "disabled"` or `kind = "enabled"`
with the fields below; enabling it does not enroll any creature. Each creature
separately records `kind = "exempt"` or `kind = "required"`. Both declarations
have one strict current shape with no compatibility default.

- One nutrition point is an integer reserve unit. `unit_ticks` actor-active ticks
  consume one point; the creature retains one remainder below `unit_ticks`.
  Actual periodic Health/Stamina/Focus/Mana gains also pay explicit per-resource
  numerators over a shared denominator, with a separate saved fractional remainder.
  Full pools and direct healing/restoration pay no periodic cost. Rogue's all-one
  rates and the recovery intervals remain provisional coupled tuning parameters.
  `initial`, `maximum`, `hungry_below`, `weak_below` and `starve_at` are rules data.
  The initial reserve is nonnegative and at most the maximum; thresholds satisfy
  `0 < weak_below < hungry_below <= maximum`, and `starve_at <= 0`.
- Nourished nutrition uses normal Health recovery. Hungry, Weak and Empty
  bands each use an explicit authored recovery policy, independently selecting
  Normal, Slowed or Stopped; see the [current schema](../reference/creature-rules.md#shared-nutrition-eating-and-starvation).
  The mechanics fixture's slowdown and stops are not selected Rogue balance.
  Reserve at or below zero is empty; reaching `starve_at` is fatal. This is an
  explicit TOR deadline; the provisional Rogue mapping uses -852 to preserve its
  post-decrement comparison.
- Active intervals settle analytically and split only at bounded nutrition/recovery-
  policy crossings and existing source expiries. Earned Health work is settled under the old policy before rephasing. Large intervals,
  incremental intervals and restored intervals must agree. Only existing actor
  clocks/deadlines advance it; queries, frozen time and seams grant nothing.
- An explicit edible capability declares strictly positive fixed or rolled
  nourishment. Eat freezes the declared operand in the shared plan, holds one exact
  existing item unit, and spends it once at resolution. A rolled meal uses the
  existing neutral rules stream only at successful completion; queries, preparation,
  suspension, cancellation and restoration do not sample it. Nourishment clears
  negative reserve to zero, adds the resolved amount and caps at `maximum`; it
  grants no Health or XP directly.
  Item quantities and the established supply holds remain the only quantity
  authority. Both nutrition remainders are preserved. Existing interruption,
  cancellation and recovery contracts apply. An exact starvation/completion tie
  follows timer-before-action ordering: death releases the held unit unconsumed.
- Starvation uses ordinary persistent death and cleanup with no invented fatal
  owner or XP recipient. Strict records preserve reserve, both remainders and explicit
  exemption; tracked nutrition must agree with the saved Health policy. Personal
  disclosure must not expose another creature's hidden reserve.

The pinned Rogue source provides evidence for its own 1300 initial food,
300/150 thresholds, 1100–1499 meals, 2000 cap and post-decrement starvation.
The shared operand can express that exact meal range as `1d400 + 1099`; this
capability is now explicitly selected by Rogue's player-only enrollment and shared
ration stock. Fainting maps to the generic typed condition trigger. See the
[current authored mapping](../reference/creature-rules.md#authored-empty-depletion-conditions)
for strict schemas, timer ordering and bounded random work. Food distribution,
population pressure and complete-run balance remain separate acceptance work.

### Items and ranged actions

Extend the finite entry-time supply stages as more item categories become usable.
Existing rations share finite stock between ordinary eating and respec. The
appended ordinary restore-strength stage supplies one finite placement attempt per
floor from depth 1; this provisional rate is not a guaranteed cure or the complete
potion catalog. Effective observed restoration identifies the concealed potion.
The selected ordinary magic-item batch implements healing, confusion, blindness, poison,
see-invisible and extra healing, class-restricted identify and sleep, and
fire/cold/lightning plus invisibility/haste-monster/slow-monster charged devices.
Read and Zap are explicit capabilities; exact instance charges and owner-only
paid identification extend existing holds and knowledge without granting abilities.
The device attack uses Intellect + Lore versus Speed + Agility, with no activation
check or Mana cost. Healing overflow improves the earliest eligible active owned
HD by one, bounded by its immutable acquisition roll; exact restoration retains
that credit and a different-source replacement cannot inherit it. Smaller forms
can temporarily mask the benefit. See [item contracts](../reference/items.md#reading-identification-and-charged-devices).

Poison uses the shared restorable Strength deficit and on-use recognition.
See-invisible uses finite extending actor-active sight plus the blindness cure,
without automatic recognition. Sustain-strength now uses the shared worn
attribute-loss protection described below. Hallucination, detection and mapping
are implemented and selected by the supplemental live stock stage. The exact
Extra Healing cure has scoped runtime/consumer coverage; complete-run and balance
acceptance remain separate. Extra-healing overflow uses the source one/two
threshold on bounded owned HD records. Utility wands use persistent first-contact
rays and nonstacking haste/slow that cancel to neutral; the Lore dodge and bounded
reach are TOR adaptations. See [the bounded item mapping](../reference/items.md#reading-identification-and-charged-devices).

The current ring catalog implements protection, add strength, sustain strength,
see invisible, adornment, dexterity, increase damage, regeneration, slow digestion
and maintain armor through shared wearable modifiers, plus searching through the
shared active discovery resolver and teleportation through exact-source active clocks. Protection reduces all six
damage types; Strength and dexterity rings exclude only their own Physical Defense
contribution; attack damage-roll bonuses apply to canonical rolled attack pools.
Health regeneration benefits do not stack, while per-ring nutrition costs do. Signed
potency and curse knowledge stay instance-private; one paid identification choice
can permit both rings and wands. The finite ring stage follows ammunition; independent
secret-entrance and trap stages extend generation without changing geometry or
stair streams. Stealth and Aggravation complete the fourteen identities through
event-owned opposed awareness and periodic snapshot-location pulses. Manual
Sneak is explicitly deferred. See
[worn magic contracts](../reference/items.md#worn-magic-and-the-finite-rogue-ring-catalog).
These mechanics do not establish calibrated food pressure or complete Rogue parity.

The equipment risk/recovery batch adds instance curses, signed weapon enchantment,
physical armor penalties, five-category magical armor wards and rust protection.
Psychic damage is deliberately excluded from the ward. The finite authored gear
catalog contains five nonlauncher weapons (mace, long sword, dagger, two handed
sword and spear) and all eight source armor identities. Spear Throw now has an
explicit 1d6 payload and ordinary paid Spear expertise; its melee remains 2d3.
Each charged-effect identity selects a run-stable wand or staff physical form,
with separate shuffled metal/wood appearances and one-grip charge-free melee.
This does not change magical effect numbers or add staff Throw. Weapon melee dice follow
the [pinned weapon source](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/weapons.c);
common 60/40 wind-up/recovery timing remains provisional TOR tuning. Armor physical
DR 1/2/2/3/4/5/5/6 preserves source ordering and ties while retaining 2 DR ring mail;
this conservative additive-protection ladder is not a direct AC conversion.

Explicit bounded birth policies preserve the
[pinned generation probabilities](https://github.com/phs/rogue/blob/c7c119f893bd2f8255f2ad78d58a32664ddb9a97/things.c#L250-L269):
weapons have 10% cursed −1–3 hit, next 5% +1–3 hit; armor has 20% cursed 1–3
physical penalty, next 8% 1–3 ward. Remaining outcomes are ordinary. TOR clamps
physical penalties at base DR without clearing curse. The neutral shared mace
archetype remains 2d4; an explicit authored +1 hit/+1 damage instance policy keeps
the starting mace's effective stats unchanged and bypasses birth randomness.
The starting ring-mail policy likewise bypasses random condition.

The equipment batch appended `rogue.equipment` and `rogue.equipment_scrolls`,
bringing that checkpoint to ten stages while preserving the original eight stage IDs,
order and choices. Each makes one finite quantity-one attempt per first entered
floor from depth 1; gear's thirteen choices and scrolls' six choices have equal
provisional weights. The scroll pool adds enchant armor, protect armor, enchant
weapon, remove curse, identify weapon and identify armor. The four equipped-item
alterations retain no-auto-identification and consume on no eligible gear;
class-identification uses on-use discovery without direct effects. See
[equipment birth contracts](../reference/items.md#equipment-birth-and-the-finite-rogue-catalog)
for bounded policy schemas, immutable definitions, atomic sampling and adaptations.
Remaining catalog specials stay separate. The current branch implements physical
throw and launch, adopting weapon expertise and short/long
ranges from the [Plotweaver Open Alpha v0.3](https://drive.google.com/file/d/11v0ujMTuXNdwkHEPAcPZqY29I3ZdmlJn/view)
and [Stormlight Starter Rules](https://www.cosmererpg.com/s/SL015_Stormlight_StarterRules_Digital.pdf),
while retaining TOR's existing attack checks and damage rules. Durable thrown
weapons are recoverable after hits. The dagger's ranged profile requires knife
expertise; source ranges are 20/60 feet. Thrown, Offhand, Quickdraw and Two-Handed
have shared behavior. Discreet and other unsupported published traits remain
explicit gaps; this is not full tabletop weapon parity.

Expertise acquisition is a third ordinary HD point purchase competing with skill
ranks and talents. Permanent talents may also grant expertise. The shared
foundation tracks independent owned sources, deduplicates active expertise, and
keeps dormant talent grants distinct from refunded or removed purchases. Paid HD
expertise follows the existing drain/restoration choice policy; this does not
invent permanently retained paid choices. Intellect-derived choices retain one
choice per earned permanent-Intellect threshold: a loss makes higher thresholds
dormant, restoration fills the same choices, and a genuinely new maximum earns
new thresholds once. Recombining old sources above the previous maximum is new
capacity; there is no lifetime source-consumption counter. Creation includes its
immutable birth composition. Later acquisition receipts retain deduplicated exact
HD choice versions and canonical source witnesses; snapshots check source
consistency while replay establishes historical chronology. Timed modifiers,
equipment and restorable attribute damage neither grant nor disable these choices.
Privileged baseline rewrites are rejected once choices or later acquisition
receipts exist; before learning, the unchosen baseline is reconciled explicitly.
The ordinary bonus-choice path uses a distinct threshold/expertise
operation with existing admission, preview, stale-basis checks and replay. Compact
paginated threshold rows preserve dormant choices; Text and ASCII can browse,
preview and confirm the entitlement without spending an HD point. Owning tests
cover source-free reopening and idempotence with focused local qualification.
The broader published trait catalog and full required CI remain pending. Growth assessment
uses an explicit zero expertise baseline with authored combat-benefit overrides;
pure-grant talent contributions must not price the same unlock twice. These
assessment values remain TOR tuning, not imported tabletop rules. PCs and AI use the same ordinary
actions with knowledge-limited decisions; the conservative device AI fallback
requires an identified type and explicitly known positive instance charges.

The current development branch also implements explicit primary/secondary weapon
selection through one frozen invocation context. Empty primary sockets use the
natural attack; selecting a secondary weapon costs Focus (two normally, one with
Offhand), added to the ability's existing resource bundle. Admission validates the
plan; preparation start atomically funds and reserves the entire bundle. First
strike pays resolution costs once. Exact item/socket ownership remains live,
while captured traits, costs and timing survive later expertise changes.

Text and ASCII request an exact-action server preview before confirming an
explicit weapon selection. Its displayed costs, time to next strike, recovery,
and fresh/resuming/paid-prefix status come from the same read-only planner that
captures or resumes the action. The preview is revision-bound and does not promise
funding. It exposes no private target checks or hidden weapon facts.

Quickdraw makes an ordinary draw into an empty compatible socket take zero
preparation time. Pickup, replacement, sheathing and attacks retain their normal
rules. A paid draw captures the relevant resource pool's complete allocation and
legal profiles. Expertise loss may increase current demand without dropping gear
or invalidating that paid allocation; changed occupancy or capacity refuses the
completion. Unrelated pools do not interfere. Resulting deficits block new
weapon actions using the deficient pool, while natural and unrelated actions
remain available. Restore checks minimum legal aggregate demand and captured
source consistency; replay establishes actual historical ownership.

Independent ammunition definitions now carry exact launcher family, shared typed
base damage and deterministic hit recovery without fake equipment sockets.
Condition-aware splits and fresh-identity merges preserve exact held units and
conservative owner-only remembered facts; matching hidden values never identify
the result. Public transfer identity, persistence and ordinary projectile release
are separate qualification boundaries. Ordinary authored Throw/Launch work now
owns one exact-unit reservation and releases it only at paid completion.

The physical projectile kernel has real authored Throw/Launch definitions,
closed exact delivery selectors, profile-derived range bands, and frozen source
capture. Launcher and ammunition enchantments combine once while ammunition
replaces the launcher base damage. Qualified next-attack candidates retain their
captured condition instance identity; they are conditional collision-time
resources, not promised bonuses or advance reservations. Actor-local persisted
condition generations prevent expired tokens from being mistaken for later
reapplications. Generation exhaustion rejects the whole effect transaction,
including earlier effects, payment and random draws. Ordinary condition capacity
rejection retains its existing behavior.

Capture uses only perceived ordinary melee geometry for the threatened ranged
penalty. Extended hostile reach needs a separate knowledge-safe representation
before long-reach content can claim support. Physical flight reuses normal TOR
geometry: open doors pass and closed doors stop, with no reflections. The first
actual live creature stops travel even on a miss. Durable successful throws
recover at the collision cell, including loaded air and returning self-hits;
ordinary ammunition may explicitly consume on a successful hit. Misses choose
among legal horizontal 3-by-3 cells around the final clear cell, excluding the
shooter's body; no eligible miss landing loses the unit without a random draw.
Extraction, payment, equipment refresh, collision and landing share one rollback
boundary. Hidden cursed equipped throws are paid blocked attempts and retain the
weapon. Carried throws use a real free compatible socket without equipping it.
Paid Throw/Launch session tests cover pending and completed source-free reopening,
replay, explicit resume and idempotent exact-unit settlement. Actual Text script
and adventure process owners also cover ordinary expertise acquisition, preview
cancellation and exact confirmation, payment, recovery, finite ammunition and
source-free restart. Native ASCII acceptance is registered in CI; its graphical
run is waived locally. Existing sight-cell range adjustments are
rejected for projectile definitions rather than silently interpreted as feet.
Authored projectile bands are bounded to 4,096 five-foot steps (20,480 feet)
per release; over-budget definitions reject rather than silently truncate.
Launcher and ammunition hit contributions use the wide compiled check lane, not
the narrower per-weapon authored bonus bound. The existing shared final check
and total damage budgets remain 1,000,000; extreme compositions beyond those
budgets reject atomically without saturation or disclosing numeric operands.

Population compilation and validation reuse successful, exactly equal actor
recipes within one immutable content-bound invocation, retaining at most 64
recipes. Labels alone never authorize reuse, failures are not cached, and all
band, weight, turn, carried-policy and character/seed/depth witnesses remain
checked. Regression owners count repeated-recipe validation work and reject
changed inputs across fresh contexts. The original candidate-validation deadline
remains unchanged; deterministic work reduction does not replace final-binary
timing or the required platform CI checks.

These foundations have focused local metadata and owning-test coverage.
Server-authored revision-scoped source/ammunition menus now drive Text and ASCII
direction selection and exact cost/timing preview before explicit confirmation.
Their strict protocol, shared client, Text and ASCII model owners have focused
local coverage. Authored, equipped NPC projectile selection also has owning tests;
bounded knowledge-safe launcher/ammunition acquisition now preserves conservative
melee defaults unless an owned Launch ability has explicitly higher priority.
The six-shot reserve changes no stock Rogue builds or birth loadouts; dedicated
archer content and broader weapon trade-off tuning remain separate work. Final
formats are frozen and current-writer fixtures generated; complete required CI
is still required before merge. Production dagger/shortbow/arrow definitions
now preserve the existing dagger damage, use published 20/60 and 80/320 foot
bands, and supply arrows with the pinned Rogue 2d3 payload. Shortbows join the
ordinary equipment pool; a finite six-arrow stack is attempted once per newly
entered floor. Timing, stock and combat-expertise threat prices remain provisional
TOR tuning. Knife, shortbow and spear expertise require ordinary HD or Intellect choices,
with no free grant. Discreet has no noncombat
consumer yet and is explicitly unsupported rather than an inert displayed trait.

Acceptance: capacity/anatomy constraints, unknown-property disclosure, naming
without identification, allowed observed-use learning, resource consumption,
obstruction/bounces, starvation/healing, AI use and full recovery. The bounded
treasure-room mechanism, ordinary fruit and standalone stackable darts/shuriken
use those shared foundations and are now selected by provisional live policy.
Seventeen completed catalog items enter through supplemental finite stock, while
rare treasure rooms add bounded loot and next-depth foes without replacing the
existing stages. Their exact contracts and choices are in the
[population schema](../reference/scenario-packages.md#live-rogue-content-policy)
and [item reference](../reference/items.md). Torches remain catalog-only and the
Polymorph destination roster is unchanged. Food safeguards, calibrated frequencies
and overall balance remain separate decisions, not implied by activation.

### Species, traps and pressure

Extend the selected 24-species population to all 26 species, each with its
normal-play ability/behavior case. Complete earned-survival qualification and
calibrate the depth/spawn tables.
Centaur and yeti are selected at depths 7–16 and 11–20 respectively, with
ordinary finite birth stock, passive provocation and their purchased natural
strike sequences. Troll is selected at depths 13–22 with six racial HD,
RG1(1), NA1(2), NA2(4), mean acquisition, separate 1d8/1d8/2d6 strikes and 50%
finite birth stock from the same provisional pool. Rattlesnake is selected at depths 3–12, with two racial HD, VN1(1),
one unspent point and a 1d6 Physical-checked bite. Successful contact delivers a
separate poison exposure resisted with Athletics against authored DC 12. Failed
resistance records Strength−1; ordinary restoration removes recorded Strength deficit. Jabberwock enters
at depth 21 with fifteen aberration racial HD, NA1(2), NA3(8), a heavy 2d12/2d4
sequence, passive acquisition and 70% finite birth stock. Its purchased shared
Rending Pair attempts one 1d6 rider after two qualifying hits in that sequence.
Aquator selects at depths 9–18 with five actual racial HD, CO1 on HD 1,
one zero-damage contact and no stock. Its shared corrosion reduces the selected
armor item's Impact/Keen damage reduction by one to zero; it does not change
Physical defense. Dragon selects from depth 22 onward with ten actual racial HD,
NA1(2), NA2(4), BR1(6), fire immunity, 1d8/1d8/3d10 natural strikes and one
finite birth-pool item. Its separate HD 4/8 attribute choices raise baseline
Willpower 1 to 3; starting Breath rank 1 plus the HD 8 skill purchase gives the
approved +5 check. The bounded authored birth schedule purchases Breath ranks
3/4/5 at HD 15/20/25; that spacing is provisional tuning, with the ordinary cap
of five and no runtime HD-derived bonus.

Shared directional rays support one frozen opposed source roll, live recipient
Speed + Agility checks, zero damage and continued travel on a dodge (including
ties), and cumulative repeat-encounter disadvantage after leaving/re-entering a
body. BR1 deals 6d6 Energy/fire damage on a failed dodge and intercepts there;
its provisional unscaled preparation/recovery is 100/100, cooldown 2000.
The full population preserves exact pinned redistribution: depth 31 has
M/V/G/J weight 9 each and D weight 14; the uniform five-species tail starts at 32.
Focused A/D mechanism, owned-progression and source-free process checks pass;
complete cross-platform CI remains required before merging this checkpoint.
The hazards/escape implementation extends trapdoor, arrow, sleeping gas, bear trap,
teleport, poison dart, rust and mysterious trap families. Physical trapdoor falls
use permanent apertures and preloaded destination geometry; authored gravity is
explicit. Escape adds exact-target and same-floor relocation plus ordinary
levitation, whose completed dose can arrest descent but grants no vertical flight.
Runtime and current-format qualification must cover these shared boundaries. Existing hobgoblins
roam using personal visible surroundings and visit counts, including reachable
door opening; NPC stair traversal remains disabled. This is separate from the
seeded wandering population while floors are active; its cap, cadence and
placement rules are described in the [player guide](../play/rogue.md#living-dungeon-pressure-and-awareness). Other species'
richer idle interests remain future work. Current species-owned useful-item and
Gold-interest policies remain independent of the shared profile's flee override;
carrying birth stock alone never grants a tactic.

Acceptance: generation restricts content correctly; depth increases pressure
through TOR mechanics; all specials work via ordinary actions/physics; inactive
floors neither spawn nor accumulate catch-up spawning.

Difficulty continues beyond the goal's eligibility depth within the current
256-HD capacity, which is accepted for functional Rogue. The following boundaries
are effective actor birth depths: the ordinary actor stage uses floor depth,
while treasure uses floor depth + 1 and can therefore refuse entry one floor
earlier. The selected mixed M/V/G/J/D tail reaches J's limit at depth 267;
a sampled J at 268 or deeper refuses the
whole entry atomically. G reaches its limit at 269 and refuses at 270; D reaches its limit at 272
and refuses at 273; all-M/V draws can remain valid through 274. Admission
checks the sampled recipe and never clamps or replaces it. This explicit
operational refusal is not a terminal objective. Work beyond that capacity is
deferred below. Preserve every actual per-HD record and its seeded Health for both PCs and monsters. Compile
the permanent build when its relevant inputs change, then use the compiled
character sheet for bounded live folds and ordinary observations; detailed
inspection and persistence retain all rows.

### Objective and complete run

Design [runtime hooks](scripting.md) separately; choosing a scripting runtime is
not a prerequisite for ordinary typed objective/entry transactions. The Amulet
uses one exact identity and ordinary atomic item movement, with a saved
identity binding and load-only source ownership reconciliation. Relocation must reconcile
the previous owner/ground location without duplication. Only committed player
entry triggers it, with rollback preserving state and RNG on failure. Retain
persistent death/victory and the actual player identity across controller changes.

Mechanism acceptance: no premature placement; repeat eligible entry relocates the same
identity unless carried by the player; save/replay and discarded preload agree;
carrying it through intentional surface exit wins. Standing, pushing, teleporting
or merely querying the exit cannot win, and missing-item refusal advances no time.
Successful escape is immediate and terminal, with no extra world ticks or
post-escape dungeon actor work; internal stairs retain ordinary traversal costs.
Supported clients exercise the terminal objective. Complete pressured runs and long
seeded descents/ascents still need to validate food, population and difficulty. A recognition
claim is not exact executable parity; that would require separately pinned
binaries and differential traces.

## Decisions still needed

Generation-extension/scripting need; creature CR/XP and survival balance;
capacity and richer item details; hallucination shift policy; per-item edge cases;
depth distributions and starting-gear tuning; presentation/scoring.
The proposed [tuning plan](rogue-tuning.md) covers CR/XP, survival, depth and
starting-gear tuning against a locked reference build.
Implemented recipe schema, floor dimensions, stair resolution and basic rule
imports, shared search/discovery and ranged collision/effect ordering are no
longer undecided prerequisites. Every increment follows the
[testing policy](../development/testing.md) and updates its current references.


### Deferred deep-stat capacity

The current 256 actual-HD admission and detailed-inspection guards are accepted
for now and do not block functional Rogue. Exact counts, owners, aggregate Health,
threat and XP use checked 64-bit domains; ordinary summaries use compiled
per-source counts, while saved and detailed records retain every actual HD.
Weighted finite/open population intervals and wide scalar fields do not remove
the row guards or establish support beyond them.

Exact paged HD storage, archive and inspection work and eventual guard removal
are deferred to a separate approved increment. That work must preserve every
actual HD record, individual Health seed and earned choice through saves,
inspection, rollback and replay. A counted-HD suffix, averaged Health or silent
clamp cannot substitute for it. Continue to report current capacity refusal
atomically rather than inventing a terminal floor.
