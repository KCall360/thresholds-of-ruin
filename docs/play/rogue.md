# Play the Rogue starter slice

The [`rogue-exploration` package](../../scenarios/rogue-exploration/scenario.toml)
authors Warrior and Mage presets, all 26 depth-selected foe species, equipped
starting gear, and finite supplies on persistent generated floors, including
[supplemental loot and rare treasure rooms](rogue-mechanics.md#live-content-and-layout-policy).
The package selects the canonical K/E/B/S/H/I/R/Z/O/C/L/Q/Y/N/F/T/W/P/X/U/M/V/G/J/A/D population.
Its historical package name is unchanged.
This is an incomplete Rogue adaptation:
The calibrated depth curve and full survival loop remain unfinished. The Amulet
and intentional surface escape provide the retrieval/ascent objective. The default launch still selects the separate
[first dungeon](gameplay.md).

## Rogue-only scenario

The separate [`rogue` package](../../scenarios/rogue/scenario.toml) is the
scenario being tuned toward Rogue's experience; see the
[tuning plan](../plans/rogue-tuning.md). It shares this package's dungeon,
roster and items but keeps only Rogue's mechanics active. You play one
adventurer with Rogue's kit (mace, ring mail, shortbow, 32 arrows, one ration)
and no bandages. Each new Hit Die applies the next row of the authored build
automatically; the advancement screen shows the build as locked, and purchases,
source choices and respec are refused. Bind Wounds is not granted, and the dead
leave no corpses, though what they carried still drops. Load it with
`--scenario scenarios/rogue`.

Floors follow Rogue's item generation: nine chances at 36% for an item, drawn
by Rogue's category and type weights, arrows and darts in groups of 8 to 15, and
a treasure room on about one floor in twenty. If three new floors in a row
produced no food, the next new floor places a ration. Natural healing
follows Rogue's level-based rate, and healing does not cost food. Monster
Health, accuracy, defense and damage are tuned so each species' fights resemble
Rogue's at its usual depth; see the [tuning plan](../plans/rogue-tuning.md) for
what is calibrated and what remains open. As in Rogue, orcs are greedy: they
collect Gold they can safely reach and otherwise come after you. No monster flees
because it is wounded, though a scare effect still drives monsters off. The
medusa confuses with its claws rather than a separate gaze, and confusion that
lands while you are confused lasts longer.

Creatures follow explicit species defaults before engaging a foe, using only
their own senses and known surroundings. Most explore and prefer less-visited
cells; the ice monster, stationary flytrap and guised xeroc wait. Their separate door, care,
equipment, useful-loot and Gold-interest permissions are in the [mechanics guide](rogue-mechanics.md#species-idle-policy). Passive
acquisition requires ordinary hostile provocation, except Xeroc’s readied actual-touch
guise ambush. Mean acquisition is direct for an eligible visible opponent;
continual Stealth still gates awareness. See [AI decision order](../reference/ai.md).
Idle policy does not change remembered pursuit or combat. Rogue's NPC stair
restriction remains in force.
Only active creatures take turns, and waiting for player input still pauses the
adventure. Frozen floors do not accumulate missed roaming turns.

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
Awareness and Presence 1, and Intellect binding. Each preset has one locked racial body HD, then one personal class HD. Warrior
has immutable Heavy Weaponry/Discipline starting ranks; Mage has
Spellcasting/Heavy Weaponry. Each explicitly selects its own class
for future earned HD, and begins with **one unspent advancement point**.


## Make your first choice

Open `advancement` in text or **F10** in ASCII. Browsing and previewing do not
spend the point. For example, Warrior can preview:

```text
purchase 2 advancement talent rattling_blow
confirm
```

Mage chooses an exact acquisition form, for example:

```text
purchase 2 advancement talent evocation_initiate form spark_lance_cold
confirm
```

In text, prefix each line with `advancement`; in ASCII, type it on the F10 screen.
The Mage begins without an automatically chosen spell. Its catalog also offers
Binding and Warding roots with their listed forms. A missing or incompatible form
is rejected without spending a point. Both characters can instead buy an eligible
skill rank or eligible expertise. The first purchasable class owner is HD **2**;
HD 1 belongs to the authored body and is not an ordinary purchase pool.
The [advancement guide](advancement.md) owns all command syntax,
previews, prerequisites, future-source selection and respec controls.


## Floors keep their history

Each first entry by the selected character evaluates the ordered floor stages.
The original `rogue.eight_species_foes` makes four weighted actor attempts, then
`rogue.bandages` and `rogue.rations` stages each attempt one stack, containing two
bandages and one ration respectively. The appended `rogue.restore_strength` stage
attempts one ordinary potion unit on every floor from depth 1 onward. This is
provisional TOR availability, not Rogue's 13% subtype probability; legal placement
can still fail, so it does not guarantee a cure. The independent
`rogue.room_gold` stage then makes per-room Gold decisions, followed by the magic,
equipment, ammunition, ring and control-scroll stages. The supplemental and
rare treasure stages follow these existing stages; see the
[complete live-content policy](../reference/scenario-packages.md#live-rogue-content-policy).
The original actor stage's authored depth bands
select all 26 species. Kestrel (K) is eligible at depths
1–6, bat (B) at 1–8, orc at 4–13, leprechaun at 6–15, centaur at 7–16,
rattlesnake at 3–12, aquator at 9–18, nymph at 10–19, yeti at 11–20,
Venus flytrap (F) at 12–21, troll at 13–22, wraith at 14–23, phantom at 15–24
and xeroc at 16–25. Medusa enters at 18, Griffin (G) at 20, Jabberwock at 21
and Dragon at 22. Dragon always receives one finite birth-pool item.
Depth 1 selects K/E/B/S/H equally. The pinned upper-boundary redistribution
keeps M eligible at every depth from 18 onward. Depth 27 onward selects
M/V/G/J/D, equal at 27, then 6:11:11:11:11 at 28, 7:7:12:12:12 at 29,
8:8:8:13:13 at 30 and 9:9:9:9:14 at 31. The uniform tail begins at 32. The [package manifest](../../scenarios/rogue-exploration/scenario.toml)
owns the full band/weight table. The historical `rogue.eight_species_foes`
and `rogue.vampire_supplies` IDs stay unchanged to preserve unrelated deterministic
streams. Four-U, four-M, four-V, four-G, four-J and four-D draws are possible; these
provisional counts and weights do not establish a safe or calibrated depth curve.
Ordinary foe placement excludes your current sight, body occupancy and protected
arrival cells. Treasure-room foes retain body and protected-cell exclusions but
may be in sight. A stage can record a failed placement when legal space is
unavailable; there is no refill or repeated placement attempt on return.

Preparing nearby geometry does not populate it. Successful first entry does,
before subsequent simulation time advances. Supplies must be reached and taken
through normal actions. Slain foes remain dead and picked-up or consumed supplies
remain gone on revisits. Floors persist, and the return stair pairs with its
original endpoint. Rogue's NPCs cannot voluntarily use the abstract stairs;
physical pushes, falls and physical portals retain the shared physics rules.

Ordinary and treasure actors use `racial_after` depth 26: each birth appends one
actual racial HD per effective birth depth beyond 26, preserving its base rows,
purchases and seeded per-HD Health rolls. All capacity depths below are effective
actor birth depths: the ordinary stage uses floor depth, while treasure uses
floor depth + 1 and can therefore refuse entry one floor earlier. New rows keep
ordinary grants, with explicitly authored future
purchases spending their points at the specified ordinal. J has 15 HD at depth
26 and reaches 256 at 267; a sampled J requests 257 at 268 and refuses the whole
entry atomically, without partial population or time advance. G reaches 256 HD
at 269 and refuses at 270; D reaches 256 at 272 and refuses at 273. All-M/V draws remain valid through depth 274, where each
reaches 256 HD; at 275 all five tail recipes exceed capacity. Admission follows the actual sampled recipes, without
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

The roster, survival mechanisms, current loot and retrieval/ascent objective are
implemented. Numbers and distributions remain provisional; complete-run balance,
food pressure, useful earned builds and later-depth performance are not established
by one successful seed. [Current status](../status.md) owns limitations; the
[mechanics guide](rogue-mechanics.md) describes encounters, items and survival.

Existing ordinary-process, source-free recovery and content witnesses are linked
from that guide and the [content policy](../reference/scenario-packages.md#live-rogue-content-policy).
They are not fresh qualification of a later tree. Follow the
[testing policy](../development/testing.md); this guide is not a CI status ledger.

## Moved mechanics and encounter details

The headings below preserve existing links. Detailed player explanations now live
in the mechanics guide; exact contracts remain in the linked references.

### Species idle policy

See [Rogue mechanics](rogue-mechanics.md#species-idle-policy).

### Equipment, combat and care

See [Rogue mechanics](rogue-mechanics.md#equipment-combat-and-care).

### Strength loss and ordinary restoration

See [Rogue mechanics](rogue-mechanics.md#strength-loss-and-ordinary-restoration).

### Spear throwing

See [Rogue mechanics](rogue-mechanics.md#spear-throwing).

### Live content and layout policy

See [Rogue mechanics](rogue-mechanics.md#live-content-and-layout-policy).

### Fruit, throwing weapons and treasure rooms

See [Rogue mechanics](rogue-mechanics.md#fruit-throwing-weapons-and-treasure-rooms).

### Magical staves and wands

See [Rogue mechanics](rogue-mechanics.md#magical-staves-and-wands).

### Nothing, Light and Drain Life wands

See [Rogue mechanics](rogue-mechanics.md#nothing-light-and-drain-life-wands).

### Cancellation and Magic Missile

See [Rogue mechanics](rogue-mechanics.md#cancellation-and-magic-missile).

### Flight and linked holds

See [Rogue mechanics](rogue-mechanics.md#flight-and-linked-holds).

### Gold and theft

See [Rogue mechanics](rogue-mechanics.md#gold-and-theft).

### Gaze, invisibility and object guise

See [Rogue mechanics](rogue-mechanics.md#gaze-invisibility-and-object-guise).

### Rust and bouncing breath

See [Rogue mechanics](rogue-mechanics.md#rust-and-bouncing-breath).

### Living-dungeon pressure and awareness

See [Rogue mechanics](rogue-mechanics.md#living-dungeon-pressure-and-awareness).

### Searching, concealed entrances and traps

See [Rogue mechanics](rogue-mechanics.md#searching-concealed-entrances-and-traps).

### Teleportation and levitation

See [Rogue mechanics](rogue-mechanics.md#teleportation-and-levitation).

### Reconstructed roster and depth source checkpoint

See [Rogue mechanics](rogue-mechanics.md#reconstructed-roster-and-depth-source-checkpoint).

### Monster-control scrolls

See [Rogue mechanics](rogue-mechanics.md#monster-control-scrolls).
