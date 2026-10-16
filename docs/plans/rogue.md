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
E/S/H/I/Z/C/Q/Y/T/U/V builds selected by the eleven-species population policy. The
initial HD point remains unspent and future growth explicitly
selects the matching class. Floor profiles are separate from geometry preparation:
first successful selected-character entry evaluates each finite stage once, and
returns retain entities, spent stock and recorded skips. NPC abstract-stair
traversal is disabled by typed scenario capability. Ordinary purchases expose
the starter talents and exact Mage forms; foes and Bind Wounds use shared combat,
finite supplies and cooldowns. Four weighted actor attempts use authored depth bands,
exact racial rows after depth 26 and finite C/Y/T/V birth stock at 15%/30%/50%/20%.
The equal ration/bandage pool is provisional TOR scenario tuning, not original
loot equivalence. The population has focused source-qualified activation coverage; its
equipment/stat values and weights remain provisional.
[Playable acceptance](../play/rogue.md#scope-and-verification)
and calibration retain their separate requirements.
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

Add omitted slots, mazes, secrets and timed search through reusable stages and
shared discovery rules. Lighting already exists; extend it without replacing TOR
LOS with whole-room revelation. Add blindness/invisibility/disguise/hallucination
when their shared behavior is ready.

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
enrolls Warrior/Mage, keeps all eleven selected foe recipes exempt, and makes its
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
Existing rations share finite stock between ordinary eating and respec.
Complete curses/enchantment/rust/charges, type/instance knowledge and the catalog's
specials. Add throw/launch/zap with configurable bouncing and resource/timing
rules. PCs and AI use the same ordinary actions with knowledge-limited decisions.

Acceptance: capacity/anatomy constraints, unknown-property disclosure, naming
without identification, allowed observed-use learning, resource consumption,
obstruction/bounces, starvation/healing, AI use and full recovery. Balance food
safeguards and treasure-room population only when prerequisites exist.

### Species, traps and pressure

Extend the selected E/S/H/I/Z/C/Q/Y/T/U/V population to all 26 species, each with its
normal-play ability/behavior case. Complete earned-survival qualification and
calibrate the depth/spawn tables.
Centaur and yeti are selected at depths 7–16 and 11–20 respectively, with
ordinary finite birth stock, passive provocation and their purchased natural
strike sequences. Troll is selected at depths 13–22 with six racial HD,
RG1(1), NA1(2), NA2(4), mean acquisition, separate 1d8/1d8/2d6 strikes and 50%
finite birth stock from the same provisional pool. The remaining 15 species are unfinished. Complete trapdoor, arrow,
sleeping gas, bear trap, teleport, poison dart, rust and mysterious trap families. Physical trapdoor falls
must respect destination generation/loading and geometry. Add seeded wandering
population while floors are active.

Acceptance: generation restricts content correctly; depth increases pressure
through TOR mechanics; all specials work via ordinary actions/physics; inactive
floors neither spawn nor accumulate catch-up spawning.

Difficulty continues beyond the goal's eligibility depth within the current
256-HD capacity, which is accepted for functional Rogue. The selected V-only
tail reaches that limit at depth 274; depth 275 refuses the whole entry
atomically. This explicit operational refusal is neither a terminal objective
nor a silent clamp. Work beyond that capacity is deferred below. Preserve every
actual per-HD record and its seeded Health for both PCs and monsters. Compile
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
