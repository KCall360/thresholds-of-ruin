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

Add omitted slots, mazes and secrets through reusable stages and
shared discovery rules. Lighting already exists; extend it without replacing TOR
LOS with whole-room revelation. Typed sight/invisibility, mutual-sight gaze,
single-use object guise and adjacent timed Search support the authored M/P/X
cohort. Hallucination and broader secret/discovery systems remain open.

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
The selected ordinary magic-item batch implements healing/confusion/blindness,
class-restricted identify and sleep, and fire/cold/lightning charged devices.
Read and Zap are explicit capabilities; exact instance charges and owner-only
paid identification extend existing holds and knowledge without granting abilities.
The device attack uses Intellect + Lore versus Speed + Agility, with no activation
check or Mana cost. Healing overflow improves the earliest eligible active owned
HD by one, bounded by its immutable acquisition roll; exact restoration retains
that credit and a different-source replacement cannot inherit it. Smaller forms
can temporarily mask the benefit. See [item contracts](../reference/items.md#reading-identification-and-charged-devices).

Complete curses/enchantment and the remaining catalog specials separately. Throw
and launch still need their own projectile work. PCs and AI use the same ordinary
actions with knowledge-limited decisions; the conservative device AI fallback
requires an identified type and explicitly known positive instance charges.

Acceptance: capacity/anatomy constraints, unknown-property disclosure, naming
without identification, allowed observed-use learning, resource consumption,
obstruction/bounces, starvation/healing, AI use and full recovery. Balance food
safeguards and treasure-room population only when prerequisites exist.

### Species, traps and pressure

Extend the selected 24-species population to all 26 species, each with its
normal-play ability/behavior case. Complete earned-survival qualification and
calibrate the depth/spawn tables.
Centaur and yeti are selected at depths 7–16 and 11–20 respectively, with
ordinary finite birth stock, passive provocation and their purchased natural
strike sequences. Troll is selected at depths 13–22 with six racial HD,
RG1(1), NA1(2), NA2(4), mean acquisition, separate 1d8/1d8/2d6 strikes and 50%
finite birth stock from the same provisional pool. Rattlesnake is selected at depths 3–12, with two racial HD, VN1(1),
one unspent point, a 1d6 bite and a separate Physical-checked poison Strength−1
rider; ordinary restoration removes recorded Strength deficit. Jabberwock enters
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
complete cross-platform CI remains required before merging this checkpoint. Complete trapdoor, arrow,
sleeping gas, bear trap, teleport, poison dart, rust and mysterious trap families. Physical trapdoor falls
must respect destination generation/loading and geometry. Existing hobgoblins
roam using personal visible surroundings and visit counts, including reachable
door opening; NPC stair traversal remains disabled. This is separate from the
unfinished seeded wandering population while floors are active. Other species'
richer idle interests remain future work. Current species-owned useful-item and
Gold-interest policies remain independent of the shared profile's flee override;
carrying birth stock alone never grants a tactic.

Acceptance: generation restricts content correctly; depth increases pressure
through TOR mechanics; all specials work via ordinary actions/physics; inactive
floors neither spawn nor accumulate catch-up spawning.

Difficulty continues beyond the goal's eligibility depth within the current
256-HD capacity, which is accepted for functional Rogue. The selected mixed M/V/G/J/D
tail reaches J's limit at depth 267; a sampled J at 268 or deeper refuses the
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

Generation-extension/scripting need; search/discovery rules; remaining creature,
CR/XP and survival balance; capacity and richer item details;
ranged collision/effect order; hallucination shift policy; per-item edge cases;
depth distributions and starting-gear tuning; presentation/scoring.
Implemented recipe schema, floor dimensions, stair resolution and basic rule
imports are no longer undecided prerequisites. Every increment follows the
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
