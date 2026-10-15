# Play the Rogue starter slice

The [`rogue-exploration` package](../../scenarios/rogue-exploration/scenario.toml)
authors Warrior and Mage presets, ordinary emus and snakes, equipped starting
gear, and finite bandage/ration supplies on persistent generated floors. Its
historical package name is unchanged. This is an incomplete Rogue adaptation:
there is no Amulet, escape victory, complete roster or calibrated
depth curve. The default launch still selects the separate
[first dungeon](gameplay.md).

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
or detached floors do not digest or catch up on missed faint rolls. Emus and snakes
are explicitly exempt. These values and food distribution are provisional; see
[nutrition](../reference/creature-rules.md#authored-empty-depletion-conditions).

## Floors keep their history

Each first entry by the selected character evaluates a finite profile: two emu
attempts, two snake attempts, one two-bandage stack and one ration. These are
provisional counts used uniformly at eligible depths, without difficulty scaling.
Foe placement excludes your current sight, body occupancy and protected arrival
cells. A stage can record a failed placement when legal space is unavailable;
there is no refill or repeated placement attempt on return.

Preparing nearby geometry does not populate it. Successful first entry does,
before subsequent simulation time advances. Supplies must be reached and taken
through normal actions. Slain foes remain dead and picked-up or consumed supplies
remain gone on revisits. Floors persist, and the return stair pairs with its
original endpoint. Rogue's NPCs cannot voluntarily use the abstract stairs;
physical pushes, falls and physical portals retain the shared physics rules.

The first floor has no upward exit. The generator has an explicit 1,000,000-floor
address limit. Other operational bounds can stop admission earlier: the retained
population ledger permits 1,000,000 decisions, and this profile records ten per
first floor entry. These resource limits are not a final floor or victory condition. Frozen regions do not receive catch-up turns. Saves retain the selected
build, inventory, floor decisions and ordinary actor/item state. Death remains terminal and
persistent. See [generation](../reference/generation.md) and
[persistence](../architecture/persistence.md).

## Scope and verification

All starting stats, equipment bonuses, foe timing, quantities, XP policy and the
uniform population are provisional TOR tuning. A successful seeded walkthrough
is behavioral coverage, not evidence of balanced Warrior/Mage runs. Bow/arrows,
most species, traps, wandering populations, calibrated food pressure and the retrieval/escape
objective remain in the [Rogue plan](../plans/rogue.md).

Shared engine support and playable content have different acceptance boundaries.
The presets, first purchases, ordinary foes and finite care are playable content;
they do not establish a complete survival loop or useful builds throughout HD 1–6.

The authored content contracts live in
[`test_rogue_playable_content.py`](../../scripts/test_rogue_playable_content.py).
Focused actual-process coverage includes:

- Exact selected-Mage form and future-source choices retained after source-deleted
  restart, plus first entry to an unvisited floor using saved content, in
  [`test_rogue_playable_process.py`](../../scripts/test_rogue_playable_process.py).
- Both presets' once-only equipped starting kits through journal-only and
  checkpoint recovery in
  [`test_floor_activation_process.py`](../../scripts/test_floor_activation_process.py).
- Purchased Rattling Blow and Cold Lance against ordinary emus/snakes, and
  Bind Wounds healing real combat injury while spending one bandage and respecting
  its cooldown, in
  [`test_rogue_techniques_process.py`](../../scripts/test_rogue_techniques_process.py).
- The ordinary four-foe walkthrough in `test_rogue_playable_process.py`: earned
  HD 2 and spending its new point, finite floor supplies, descent/return without
  new stock or population decisions, bandage care, ration respec and saved restart.

Broad integrated/native verification and final Windows/Linux CI remain required
gates. These bounded cases do not close HD 1–6 play, pressured escape or numerical
calibration. See [status](../status.md) and the
[testing policy](../development/testing.md).

## Reconstructed roster and depth source checkpoint

The catalog additionally authors hobgoblin, zombie, quagga, black unicorn, ice
monster and vampire candidate recipes, plus incomplete centaur/yeti combat
prototypes. Their actual per-HD records, rolled natural strikes and purchased second/third strike
unlocks use the existing shared compiler and frozen combat pipeline. Centaur/yeti
carry behavior and remaining signature species remain unfinished. Catalog
presence is not complete-species acceptance, production admission or measured balance.

The ice monster purchases CH1 Freezing Touch on its first racial HD. Its `natural_melee` ability source imports the actual species-owned checked
contact, then its checked additional rider applies the shared Frozen condition for 200
actor-active ticks, then 400 ticks of target-wide protection. Keeping the current
application prevents refresh and chaining; the shared lifecycle pauses paid work,
preserves its prefix/operands/holds and requires human Resume after thaw. Nutrition
and recovery retain their ordinary active-time behavior. No ice-only scheduler or
fixed-damage replacement attack is introduced.

The vampire candidate owns eight actual racial-HD rows: DR1 Drain Vitality on
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
priority 20 are provisional tuning inputs. The eight-species candidate alone
selects V's provisional 20% birth-stock policy: one ordinary ration or clean
bandage with equal weights. This restricted pool is not Rogue's full item pool;
it enables no equipment, care or loot tactics. Production selection and earned
survival qualification remain pending.

Production still makes two emu and two snake attempts, followed by the existing
bandage and ration stages in their original order. Every existing pool is now one
open depth band with a singleton weight of one. No newly authored candidate is
selected. Depth 26 is not terminal, and no HD cap is a designed late-floor policy.
Actual kit fights, earned advancement/escape routes and coupled food/recovery
measurements are still required before activating a candidate or selecting its
numbers. All food, recovery, ability and stat costs remain provisional.

Each species explicitly owns its acquisition disposition: E/S/H/Z/Q/U/V are
mean, while I/C/Y and the player species are passive. Idle waiting and the
1000-tick memory bound are provisional. CH1 explicitly declares hostile contact,
independent of its AI priority; its strike and rider come from the shared natural
attack pipeline. Care, equipment and loot tactics remain explicit profile opt-ins.
Complete roster, deep stat scaling and broad integrated acceptance remain open.
