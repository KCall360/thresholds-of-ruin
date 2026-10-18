# Play the Rogue starter slice

The [`rogue-exploration` package](../../scenarios/rogue-exploration/scenario.toml)
authors Warrior and Mage presets, twenty-one depth-selected foe species, equipped
starting gear, and finite bandage/ration supplies on persistent generated floors.
The package selects the canonical K/E/B/S/H/I/R/Z/O/C/L/Q/Y/N/F/T/W/U/V/G/J population.
The L/N/O theft/gold additions still need the qualification described below.
Its historical package name is unchanged.
This is an incomplete Rogue adaptation:
five production species and the calibrated depth curve remain unfinished. The Amulet
and intentional surface escape provide the retrieval/ascent objective. The default launch still selects the separate
[first dungeon](gameplay.md).

Creatures follow explicit species defaults before engaging a foe, using only
their own senses and known surroundings. Most explore and prefer less-visited
cells; the ice monster and stationary flytrap wait. Their separate door, care,
equipment, useful-loot and Gold-interest permissions are listed below. Passive acquisition still requires ordinary hostile
provocation, and mean acquisition still uses the existing seeded wake check.
Idle policy does not change remembered pursuit or combat. Rogue's NPC stair
restriction remains in force.
Only active creatures take turns, and waiting for player input still pauses the
adventure. Frozen floors do not accumulate missed roaming turns.

## Species idle policy

These are provisional TOR choices for all twenty-two authored species, including
the player species. They are not a claim of original Rogue behavioral parity or
balanced encounter pressure. Human defaults apply only when a human is explicitly
AI-controlled; they do not automate the selected player's turns.

| Species | Idle | Doors | Care | Equipment | Useful loot | Gold interest |
| --- | --- | --- | --- | --- | --- | --- |
| Human, hobgoblin, centaur, troll, vampire | Explore | Yes | Yes | Yes | Yes | No |
| Orc | Explore | Yes | Yes | Yes | Yes | Yes |
| Leprechaun | Explore | Yes | No | No | No | Yes |
| Nymph | Explore | Yes | No | No | No | No |
| Bat, kestrel, griffin, emu, snake, rattlesnake, quagga, black unicorn, zombie, wraith, jabberwock | Explore | No | No | No | No | No |
| Yeti | Explore | No | Yes | No | Yes | No |
| Ice monster, Venus flytrap | Wait | No | No | No | No | No |

All keep their authored acquisition disposition and 1,000-tick actor-active
memory. The shared `rogue_hostile` profile overrides only the flee threshold to
zero; species defaults use 25%. It cannot flatten the species' distinct tools or
idle choices. Explicit Wait prevents every idle move, including a detour to loot,
while still permitting useful underfoot actions if independently enabled.

Care and looting use ordinary finite items and the creature's own identity
knowledge. Useful pickup is bounded by known usable equipment, one healing
reserve and supplies for one payable use of a safe owned healing ability. Only
actors with required nutrition keep a food reserve; the current foe recipes are
nutrition-exempt. No hidden item effect becomes a useful-loot goal and excess
supplies are not collected. Leprechaun and orc separately seek known reachable
Gold, after self-preservation and retaliation and before ordinary idle behavior. Birth stock alone does not teach an item's identity or grant
a tactic. See [the shared policy](../reference/scenario-packages.md#behavior-defaults-and-ai-overrides)
for fallback and override authoring.

## Choose a new-save preset

Complete [source setup and authentication](../../README.md#play-from-source).
Use a fresh save path; an existing save retains its original selected character.
The server's existing `--character` option chooses a complete preset. There is
no in-client class creation screen or freeform editing of starting attributes.

Warrior:

```sh
cargo run -p tor-server --bin tor-server -- --scenario scenarios/rogue-exploration --character 1 --seed 42 --save saves/rogue-warrior.db
cargo run -p tor-client-text -- --actor 1
```

Mage:

```sh
cargo run -p tor-server --bin tor-server -- --scenario scenarios/rogue-exploration --character 2 --seed 42 --save saves/rogue-mage.db
cargo run -p tor-client-text -- --actor 2
```

Run the server and client in separate terminals with the same private
`TOR_SERVER_TOKEN`. To use a graphical client, replace `tor-client-text` with
`tor-client-ascii`, retaining the matching `--actor` value. Seed 42 is an example,
not a guarantee of survival or a balance recommendation. Omission of
`--character` selects Warrior. The unselected preset and its inventory are absent.

Both characters are human with Strength, Speed, Intellect and Willpower 2,
Awareness and Presence 1, and Intellect binding. Warrior begins with one Warrior
HD and the immutable Heavy Weaponry/Discipline starting ranks. Mage begins with
one Mage HD and Spellcasting/Heavy Weaponry. Each explicitly selects its own class
for future earned HD, and begins with **one unspent advancement point**.

## Make your first choice

Open `advancement` in text or **F10** in ASCII. Browsing and previewing do not
spend the point. For example, Warrior can preview:

```text
purchase 1 advancement talent rattling_blow
confirm
```

Mage chooses an exact acquisition form, for example:

```text
purchase 1 advancement talent evocation_initiate form spark_lance_cold
confirm
```

In text, prefix each line with `advancement`; in ASCII, type it on the F10 screen.
The Mage begins without an automatically chosen spell. Its catalog also offers
Binding and Warding roots with their listed forms. A missing or incompatible form
is rejected without spending a point. Both characters can instead buy an eligible
skill rank. The [advancement guide](advancement.md) owns all command syntax,
previews, prerequisites, future-source selection and respec controls.

## Equipment, combat and care

Each selected character starts with an equipped mace and ring mail, three known
clean bandages and one known food ration. Inventory and the ordinary equipment
screen show the actual items. The kit is installed once for the run; reconnecting,
changing control, revisiting floors and reopening a save do not grant another kit.

Basic melee and Bind Wounds are available immediately. Use the usual
[text](text.md) or [ASCII](ascii.md) controls to explore, attack and choose abilities.
Foes use the same creature build, rolled strikes, damage and readiness rules as
you. Fatal effects earn XP and eligible future HD automatically; the new points
remain yours to spend. Future-source changes affect later growth and do not
replace the class or form already purchased on an existing HD.

Bind Wounds consumes one finite bandage through ordinary preparation, recovery
and cooldown. In text, `bind wounds me using clean bandage` selects the known
carried supply. Positive Health loss can interrupt preparation. Health also
recovers under the shared actor-active recovery rules; safe waiting and a
bandage are different actions. See [finite care](../reference/creature-rules.md#finite-supplies-and-universal-care).

Your ration is one finite item: eat it for nourishment or spend it on an ordinary
safe-place respec. Text uses `eat food ration`; ASCII uses **e**, then selects the
exact carried item. Fresh purchases and future-source selection do not consume it.
Replacement requires its ordinary quiet interval, finite ration and paid work,
and can be interrupted; see [respec](advancement.md#rations-work-and-interruptions).

Warrior and Mage start with 1300 reserve and digest one unit every 100 active ticks.
Each Health, Stamina, Focus or Mana point actually regained by periodic recovery
also consumes one reserve unit. Full pools cost nothing extra; direct healing and
restoration from abilities or consumables have no regeneration tax. Recovery
intervals and these costs are provisional and will need tuning together.
Stats show your reserve and hunger. Below 300 is Hungry; below 150 is Weak; at zero
or below it is Empty. Health recovery remains Normal in every band. Empty depletion
can cause Fainted, temporarily preventing voluntary action and pausing paid work;
status rows distinguish active conditions from any later protection. When a faint
ends, explicitly resume paused work or cancel eligible work under the ordinary
commitment cost rules. Paused Eat requires Resume or a successful ordinary
replacement; it has no explicit cancellation. Eating still needs time, and reaching
-852 causes persistent death.
A ration adds 1100–1499 reserve after clearing negative debt, capped at 2000. Frozen
or detached floors do not digest or catch up on missed faint rolls. All ten
selected foe recipes are explicitly exempt. These values and food distribution
are provisional; see
[nutrition](../reference/creature-rules.md#authored-empty-depletion-conditions).

## Strength loss and ordinary restoration

Rattlesnake owns Venomous Bite on its first of two racial HD; the second point
remains unspent. Its mean acquisition and 1d6 natural bite use the ordinary
combat rules. Only a successful contact attempts the separate Physical-checked
poison rider, which records at most one actual Strength loss. Poison immunity
suppresses that rider alone. Body, attributes and timing initially reuse snake
values as provisional tuning.

An unidentified potion has a neutral appearance. Pick it up and drink through
ordinary Drink/Quaff/Q controls. A potion of restore strength removes only recorded
Strength deficit, using a fixed 1000 operand equal to the canonical deficit bound.
It never adds bonus Strength or heals injury. Restored Strength may increase derived
maximum Health and therefore displayed current Health while injury stays unchanged;
ordinary paid time can separately allow natural recovery. Resource balances are not
refilled by the effect. Waiting, food, healing and respec do not erase the deficit.
A dose spends one unit. Positive actual restoration teaches its identity; drinking
at zero deficit consumes it without learning. Character-owned knowledge persists
after the last unit and through save/replay. See [item knowledge](../reference/items.md).

## Flight and linked holds

Bat and kestrel each own one animal racial HD, with 1d2 and 1d4 natural damage.
Bat is passive and has an authored 1/2 erratic-movement chance; kestrel is mean.
Both fly and carry no birth stock. Griffin owns thirteen magical-beast racial
HD, RG1 on HD 1 and Second Strike on HD 2, with 4d3/3d5 natural damage. It is
mean, flies and has a 20% finite birth-stock chance from the shared supply pool.
Flight permits unsupported paid movement but preserves clearance, imposed
velocity and Rogue's NPC abstract-stair prohibition.

Venus flytrap owns eight plant racial HD, remains mean and stationary, and buys
Clutch on HD 1 and Tightening Hold on HD 6. The creating contact deals zero
damage. Later qualifying Clutch or basic-melee primaries against that exact live
link increment its damage channel, including misses; damage still requires a hit.
Escape has base 100 preparation/100 recovery scaled by Speed and 400 actor-active
ticks of reapplication protection against that holder. A movement command while
held instead attempts paid Escape from the oldest active hold, without moving
or queuing movement after success. Explicit Escape can choose a different hold.
Forced displacement can
break the link; stationary does not anchor F against pushes. It carries no stock.
See [movement capabilities](../reference/creature-rules.md#locomotion-capabilities).

## Gold and theft

Gold is a finite ordinary inventory stack. Take and drop it with the same commands
and quantity controls as other items. Each present room gets one 50% first-entry
Gold chance, and each successfully placed leprechaun is born carrying finite Gold. Amounts use the
shared positive quantity range, from 2 through `51 + 10 * floor depth` inclusive;
birth amounts use the birth floor. There is no separate purse or repeat death roll.
A successful room chance can still skip placement when no legal cell is available.
Revisiting a floor or restarting does not replace spent or stolen stock.

Leprechauns (L) appear at depths 6–15. Their three actual fey HD own Filch Gold
(TR1) on HD 1 and Vanish (TR3) on HD 2, with one point unspent and a natural 1d1
strike. The separate Cognitive-checked theft rider takes one rolled amount capped
by all free carried Gold. Its range is frozen from the current floor when the
attack is prepared. Nymphs (N), at depths 10–19, likewise own three fey HD,
Filch Charm (TR2) on HD 1 and Vanish on HD 2, with one unspent point and zero
natural damage. Their checked rider can steal one free unequipped magical unit.
L and N retain passive acquisition until ordinarily provoked.

Both successful thieves may relocate to a legal cell on the same floor after an
actual transfer. Failed checks, no eligible stock or fully held units cannot
trigger Vanish. Held reservations remain protected, and theft cannot take
equipped items. The concealed restore-strength potion and the Amulet of Yendor
are magical; ordinary ring mail is not. Theft does not identify a concealed item
for either actor. Stolen items and Gold remain ordinary inventory, recoverable
through normal death drops. The Amulet remains the same unique objective item.

Orcs (O) appear at depths 4–13 with one humanoid Warrior HD, purchased W02
Measured Cut, natural 1d8 damage and passive acquisition until provoked. Their explicit
Gold interest does not grant theft. These are provisional TOR builds and
behaviors, not a calibrated survival curve or original Rogue timing. See
[shared theft and Gold](../reference/items.md#theft-and-gold) for exact inventory
and preparation contracts.

## Floors keep their history

Each first entry by the selected character evaluates five ordered stages:
`rogue.eight_species_foes` makes four weighted actor attempts, then the unchanged
`rogue.bandages` and `rogue.rations` stages each attempt one stack, containing two
bandages and one ration respectively. The appended `rogue.restore_strength` stage
attempts one ordinary potion unit on every floor from depth 1 onward. This is
provisional TOR availability, not Rogue's 13% subtype probability; legal placement
can still fail, so it does not guarantee a cure. The final independent
`rogue.room_gold` stage makes the per-room Gold decisions described above; the earlier stage order and
random streams are unchanged. The actor stage's authored depth bands
select K/E/B/S/H/I/R/Z/O/C/L/Q/Y/N/F/T/W/U/V/G/J. Kestrel (K) is eligible at depths
1–6, bat (B) at 1–8, orc at 4–13, leprechaun at 6–15, centaur at 7–16,
rattlesnake at 3–12, nymph at 10–19, yeti at 11–20,
Venus flytrap (F) at 12–21, troll at 13–22 and wraith at 14–23. Griffin (G)
enters at 20 and Jabberwock at 21. Depth 1 selects K/E/B/S/H equally; depth 27
onward selects V/G/J, equal except 7:12:12 at 29 and 8:8:13 at 30. The [package manifest](../../scenarios/rogue-exploration/scenario.toml)
owns the full band/weight table. The historical `rogue.eight_species_foes`
and `rogue.vampire_supplies` IDs stay unchanged to preserve unrelated deterministic
streams. Four-U, four-V, four-G and four-J draws are possible; these
provisional counts and weights do not establish a safe or calibrated depth curve.
Foe placement excludes your current sight, body occupancy and protected arrival
cells. A stage can record a failed placement when legal space is unavailable;
there is no refill or repeated placement attempt on return.

Preparing nearby geometry does not populate it. Successful first entry does,
before subsequent simulation time advances. Supplies must be reached and taken
through normal actions. Slain foes remain dead and picked-up or consumed supplies
remain gone on revisits. Floors persist, and the return stair pairs with its
original endpoint. Rogue's NPCs cannot voluntarily use the abstract stairs;
physical pushes, falls and physical portals retain the shared physics rules.

The actor stage uses `racial_after` depth 26: each birth appends one actual racial
HD per depth beyond 26, preserving its base rows, purchases and seeded per-HD
Health rolls. New rows keep their ordinary unspent grants. J has 15 HD at depth
26 and reaches 256 at 267; a sampled J requests 257 at 268 and refuses the whole
entry atomically, without partial population or time advance. G reaches 256 HD
at 269 and refuses at 270. All-V draws remain valid through depth 274, where V
reaches 256 HD; at 275 all three tail recipes exceed capacity. Admission follows the actual sampled recipes, without
clamping or replacing an over-cap choice. This is a current construction-capacity
limit, not a designed HD plateau.

The first floor has a surface exit on its upward stair. The generator has an
explicit 1,000,000-floor address limit. Other operational bounds can stop admission earlier, including the
population ledger's 1,000,000-decision limit and actor construction capacity.
These resource limits are not a final floor or victory condition.
Frozen regions do not receive catch-up turns. Saves retain the selected
build, inventory, floor decisions and ordinary actor/item state. Death remains terminal and
persistent. See [generation](../reference/generation.md) and
[persistence](../architecture/persistence.md).

## Find the Amulet and return

On your first entry to depth 26 or deeper, the Amulet of Yendor appears on a
reachable, supported ground cell. There is one ordinary item identity in the
played world. Every later entry to depth 26 or deeper, including a revisit,
relocates that same item onto the entered floor unless you are carrying it.
Leaving it in a pile or with another actor does not protect its old location.
The arrival cell, an existing pile and the same randomly reselected cell are
valid destinations. Queries, nearby floor preparation, NPC entry and restoring
a save do not move it.

Take it through the ordinary item command and return through the paired stairs.
You may backtrack between dungeon floors without it. At depth 1, deliberately
use **up** in text or **<** in ASCII at the surface exit while carrying the Amulet
to escape. An attempt without it is refused without advancing time. Standing on
the exit or being pushed or teleported there does not win. A successful escape
ends the run immediately without advancing world time or giving dungeon actors
another turn; internal stair travel still takes its ordinary time. Death and escape are
persistent terminal outcomes; restarting does not grant a new Amulet or victory.

## Scope and verification

All starting stats, equipment bonuses, foe timing, quantities, XP policy and the
depth-selected population are provisional TOR tuning. A successful seeded
walkthrough is behavioral coverage, not evidence of balanced Warrior/Mage runs. Bow/arrows,
the five remaining species, traps, wandering populations, calibrated food pressure and complete
run balancing remain in the [Rogue plan](../plans/rogue.md).

Shared engine support and playable content have different acceptance boundaries.
The presets, first purchases, ordinary foes and finite care are playable content;
they do not establish a complete survival loop or useful builds throughout HD 1–6.

The authored content contracts live in
[`test_rogue_playable_content.py`](../../scripts/test_rogue_playable_content.py).
The bounded [hobgoblin roaming tests](../../crates/server/src/hobgoblin_roaming_tests.rs)
compile the production recipe and policies into small rooms, checking committed
idle movement, hidden-player independence, doors, stair restrictions and personal
hostile acquisition, plus least-visited choice in a three-cell corridor. The
stair witness also enables traversal as a counterfactual, proving that the
production restriction is what prevents descent.

Roaming grows each actor’s visit history. Snapshots share immutable history
buckets by region; an idle update copies only the acting actor’s current region
bucket. Ordinary player waits copy no visit entries, including after mature
four-actor roaming. The actor/region directory still has a cloning cost, and the
active bucket’s update cost grows with its visited cells. The bounded structural
tests check those costs and retained sharing; they do not establish
world-independent latency or calibrated encounter pressure.
Focused actual-process contracts include:

- Exact selected-Mage form and future-source choices retained after source-deleted
  restart, plus first entry to an unvisited floor using saved content, in
  [`test_rogue_playable_process.py`](../../scripts/test_rogue_playable_process.py).
- Both presets' once-only equipped starting kits through journal-only and
  checkpoint recovery in
  [`test_floor_activation_process.py`](../../scripts/test_floor_activation_process.py).
- Purchased Rattling Blow and Cold Lance against eligible first-floor E/S/H foes,
  and Bind Wounds healing real combat injury while spending one bandage and respecting
  its cooldown, in
  [`test_rogue_techniques_process.py`](../../scripts/test_rogue_techniques_process.py).
- The ordinary combat walkthrough in `test_rogue_playable_process.py`: actual
  committed kills and earned HD advancement, spending its new point, finite floor
  supplies, descent/return without new stock or population decisions, bandage care,
  ration respec and saved restart.
- Ordinary Amulet relocation, carrying, source-free restart and intentional
  surface exit through Text and native ASCII in
  [`test_amulet_process.py`](../../scripts/test_amulet_process.py).
- Generated Centaur/Yeti passive provocation and Troll unprovoked mean acquisition,
  complete natural sequences, ordinary same-identity death drops and source-free recovery in
  [`test_rogue_carrier_process.py`](../../scripts/test_rogue_carrier_process.py). Its
  stronger authored HD16 player is a mechanism fixture, not earned-route or starter-kit evidence.
- Production R loss, reachable ordinary potion pickup/timed drink, finite stock,
  identification, injury preservation and source-free recovery in
  [`test_rogue_restoration_process.py`](../../scripts/test_rogue_restoration_process.py).
  This bounded controlled encounter retains the actual Warrior kit; it is not a
  generated-route or balance sample.
- Production stationary F movement rejection and ordinary unsupported B/K/G
  flight, with persisted G continuation, in
  [`test_rogue_locomotion_process.py`](../../scripts/test_rogue_locomotion_process.py).
  Production recipe combat mapping is asserted separately by the server roster
  integration tests, including F Clutch, later basic-melee GR2 damage and Escape.
- Production-origin depth-27 activation, actual V growth and carried stock through
  source-deleted recovery in
  [`test_rogue_activation_process.py`](../../scripts/test_rogue_activation_process.py).

These existing witnesses cover their named contracts; historical E/S walkthroughs
remain separate evidence. This L/N/O theft/gold batch has not yet established
focused unit, integration, Service, actual-process, source-free recovery or final
repository/Windows/Linux debug/release qualification. The added contracts need
coverage for free and held stock, one-roll cross-stack conservation, magical
selection and concealment, actual-transfer-gated same-floor relocation, depth
capture, room-stream isolation, death recovery and strict current-format restore.
Changing documentation or extending the roster does not make an older run evidence
for those contracts. No UI or 3D feature is added by this batch.
The [earned-route pilot](../development/rogue-earned-route.md) retains its frozen
eight-species inputs and adverse or censored outcomes; it is not twenty-one-species
acceptance. Reaching depth 27 records arrival, not
Vampire combat survival. Broad integrated/native verification and final
Windows/Linux CI remain required gates. These bounded cases do not close HD 1–6
play, pressured escape or numerical calibration. See [status](../status.md) and the
[testing policy](../development/testing.md).

## Reconstructed roster and depth source checkpoint

The selected K/E/B/S/H/I/R/Z/O/C/L/Q/Y/N/F/T/W/U/V/G/J recipes use the shared compiler and character-sheet
pipeline. Centaur owns four actual Warrior HD, with Second Strike purchased on
HD 2 and Third Strike on HD 4; its natural sequence is 1d2/1d5/1d5. Yeti owns four
actual racial HD, purchases Second Strike on HD 2 and uses 1d6/1d6; its cold
subtype retains descriptor immunity. Both explore while retaining passive
acquisition until actual hostile provocation. Their 15%/30% birth carry chances
follow the pinned reference, while the equal ration/bandage pool is provisional TOR scenario tuning, not original
loot equivalence. Birth creates one ordinary finite item, which drops with the
same identity on death and never restocks on revisit or restart. Tool permissions
come from the [species policy](#species-idle-policy), independently of birth stock.
Troll adds six actual racial HD with RG1 Regeneration on HD 1, Second Strike on HD 2 and Third Strike on HD 4; its other points remain
unspent. Its separately checked 1d8/1d8/2d6 sequence follows ordinary mean
acquisition, with exploration and 1,000-tick memory. Its reference 50% carry chance
uses that same provisional pool. The regenerative flag only permits RG1, which
supplies the existing −50% Health-recovery-period modifier; no RG2 is selected.
Troll's giant attributes (2/1/0/1/1/0), Physical defense +3, two-cell body of mass
250 and 100/100-tick melee reuse provisional authoring choices, without a
balance or earned-kit claim. Remaining signature species and balance are unfinished.

Jabberwock owns fifteen actual aberration racial-HD rows, with Second Strike on
HD 2 and Rending Pair on HD 8; thirteen points remain unspent. It has a heavy
2d12/2d4 natural sequence and passive acquisition. Rending Pair is a shared
purchased ability that imports that complete sequence and its natural timing,
then attempts one unchecked keen 1d6 rider immediately after the second qualifying
hit on the same target in that preparation. Protected zero-damage contact can
qualify; misses, whole-payload immunity and rider checks cannot. A lethal primary
skips the rider. The signature does not augment every other natural ability.
The generic talent requires Second Strike, eight total HD and the species'
`natural_sequence` eligibility flag. Draining its HD 8 owner removes the grant;
authored restoration restores the exact row, seed and purchase.

J has no flying or regeneration flag. Its 70% birth chance uses the same finite
provisional ration/bandage pool. Keen damage categories without descriptors,
attributes 2/1/0/1/1/0, Physical defense +3, large two-cell mass-250 body,
Heavy Weaponry/Athletics starting ranks, 100/100 natural timing and signature
AI priority 20 are provisional TOR inputs, not original Rogue damage typing or
a balance claim. All checks retain the shared 3d6 rules.

The ice monster purchases CH1 Freezing Touch on its first racial HD. Its `natural_melee` ability source imports the actual species-owned checked
contact, then its checked additional rider applies the shared Frozen condition for 200
actor-active ticks, then 400 ticks of target-wide protection. Keeping the current
application prevents refresh and chaining; the shared lifecycle pauses paid work,
preserves its prefix/operands/holds and requires human Resume after thaw. Nutrition
and recovery retain their ordinary active-time behavior. No ice-only scheduler or
fixed-damage replacement attack is introduced.

The wraith owns five actual undead racial-HD rows. DR2 Energy Drain costs the
HD 1 point, has no DR1 parent, and leaves four points unspent. W explores but
retains passive acquisition until ordinary hostile provocation. Its single keen
1d6 contact is followed by a separate natural-attack check against Spiritual that removes the victim's latest
actual HD and its purchases. There is no extra 1d10 loss, percentage proc, Feed,
flight, incorporeality or regeneration. W has zero carried-loot chance and appears
only at depths 14–23. Its standard undead d12 Health and immunities, attributes
2/2/1/2/1/1, natural Physical defense +3, mass 70, two-cell body, 100/100 timing and
signature priority 20 are provisional TOR inputs, not an AC/XP conversion.

A living victim regrows through ordinary kill-earned XP and its selected future
class source. Existing spent XP is never reused. ChooseAgain restores an empty
purchase slot with that ordinal's original Health seed; it does not restore its
old purchases for free. Rest, bandages and restore-strength potions cannot restore
an HD. Losing the final HD is permanent death, and preserved injury can make an
earlier drain fatal too. Required acceptance has two cooperating owners, both
using an explicitly authored HD2 Warrior and the unchanged kit. A controlled
persisted Service case uses canonical creature builds/strikes and ordinary
external-controller commands: eight emus earn 160, real survived W DR2 removes
paid Lore, W then waits while its ordinary kill brings 195, and a ninth emu brings
215 with 200 spent to regrow HD2 and 15 held. Source-free journal reopen at 195,
the original ordinal Health seed, an empty ChooseAgain row and ordinary Lore
repurchase are required. External control deliberately governs hostile timing;
this case does not exercise autonomous opposition.

The actual-process owner retains autonomous opponents, ordinary purchases and
eight emu kills, then stops at the first public survived 160-XP/HD1 drain. It
requires source-free checkpoint reopen with the exact paid-row loss and injury,
without another attack or a W-kill claim. Both the controlled recovery case and
autonomous survived-drain/source-free-reopen prefix have passed. Autonomous
W-kill/full recovery, starter survival, earned descent and complete-run balance
remain unproven. A production depth-14 witness also confirms W birth, owned rows,
zero carried stock and exact source-free checkpoint/receipt reopen.
The unchanged
89-cell fixture stops at 800 requests, 40,000 active ticks, 96 MiB of complete
process transcripts or 128 MiB of attempt disk. Output exhaustion is a censored
attempt, not evidence that all 800 requests fit. The one fixed-seed correctness
qualification has a separate 160 MiB evidence allowance; the closed 256 MiB
seed-study allowance and its failed attempts are retained unchanged. Qualification
preserves 1 GiB free disk and requires an additional 128 MiB before starting;
there is no automatic retry or seed search.

The vampire base recipe owns eight actual racial-HD rows: DR1 Drain Vitality on
ordinal 1 and RG1 Regeneration on ordinal 2. DR1 is a purchased natural-melee
signature: the species contributes its single keen 1d10 strike, and successful
contact permits a separate natural-attack check against Spiritual for restorable
maximum-Health loss of 1d3. There is no Feed or drain-to-heal effect. Injury is
preserved and ordinary death/drop/XP handling applies. RG1 requires the species'
regenerative flag and halves the ordinary Health recovery period; it grants no
unconditional combat heal. The original Rogue regeneration flag's behavior is
still unverified, so this is the [accepted TOR adaptation](../plans/progression-design.md#drain-tree),
not [reference parity](../research/rogue-5.4.4/rules.md).

Vampire attributes (Strength/Speed/Intellect/Willpower/Awareness/Presence
2/2/1/2/1/1), natural Physical defense +7, two-cell humanoid body of mass 70,
heavy-weaponry/discipline starting ranks, 100/100 action timing and signature AI
priority 20 are provisional tuning inputs. The selected population attaches V's
reference 20% birth carry chance: one ordinary ration or clean
bandage with equal weights. This restricted pool is not Rogue's full item pool;
the vampire's separately authored species policy enables its useful tool actions.
Revisit and restart do not restock.
Earned-survival qualification remains open.
Depth 26 is not terminal, and no HD cap is a designed late-floor policy. Actual-kit
fights, earned advancement/escape routes and coupled food/recovery measurements
remain necessary to assess these provisional food, recovery, ability and stat costs.

Each species explicitly owns its acquisition disposition: K/E/S/H/R/Z/Q/F/T/U/V/G
are mean, while B/I/C/Y/W/J and the player species are passive.
The [idle policy matrix](#species-idle-policy) and 1,000-tick memory bound are
provisional. CH1 explicitly declares hostile contact, independent of its AI
priority; its strike and rider come from the shared natural attack pipeline.
Tool use follows species defaults with only explicitly supplied profile overrides.
The full 26-species roster, scaling beyond current build/storage capacity and
broad integrated acceptance remain open.
