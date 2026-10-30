# Items and character knowledge

Items support quantity-aware pickup/drop, compatible stacking, character-owned
identity knowledge, [anatomy-based equipment](equipment.md), timed consumption
with explicit edibility, [readable scrolls and charged devices](magic-items.md),
and ordinary currency/theft transfers.
Inventory carrying-weight limits and containers remain deferred; authored
[shared equipment capacity](equipment.md#shared-equipment-capacity) is implemented. NetHack informs the interaction style;
these are this project's explicit rules, not a claim of exact NetHack behavior.

## Ordinary play

After setting the server token as described in the [README](../../README.md), run
`cargo run -p tor-server --bin tor-server -- --scenario scenarios/tests/items --character 1 --save saves/items-demo.db`
from the repository root with a fresh save path, then connect either client
as described in [text play](../play/text.md) or [ASCII play](../play/ascii.md). This ordinary package demonstrates stacks and
concealed potions without wizard mode. Use the opaque `#id` shown by the client
when selecting between alike stacks; these targets are scoped to the observer.

Text and adventure commands accept `take [quantity] <name or #id>` and
`drop [quantity] <name or #id>`. `get` aliases `take`. Omission (or `all` before
the noun) transfers the entire selected stack. Numeric quantities must be positive
and fit an unsigned 64-bit integer. Ambiguous names require selection; a requested
quantity stays attached through adventure clarification and walking to an item.

ASCII uses `,` for pickup and `d` for drop; stairs use `<` and `>`. A single
reachable stack is picked up immediately. Otherwise Up/Down selects a stack, typed digits set
the quantity, Backspace edits it, Enter confirms, and Escape cancels. Blank means
the entire stack. Inventory and ground lists show counts. Inspection is free.

Pickup requires the actor's current ground cell; drop requires ownership and
places items at that cell. Each successful transfer costs half the actor's turn,
rounded upward, independent of quantity. Zero, excessive, overflowing, unavailable,
and unauthorized requests do not partially mutate state or spend simulation time.

Only explicitly stackable items merge. The complete immutable definition, exact
per-unit condition and charge state must match. Corpse provenance is never
interchangeable stack state. Devices, armor, rings and other equipment remain
nonstackable. Plain weapon equipment with an explicit standalone thrown payload may be a homogeneous stack;
independent ammunition remains stackable without acquiring equipment sockets.
A weapon stack must be split to exactly one unit before wielding: drop the
whole stack, take one, equip that singleton, then take the remainder. Taking a
unit back before equipping would merge it with an identical carried stack.
Carried Throw
uses one free unit directly, without equipping the stack, multiplying modifiers
or reserving extra sockets. Equipped units never merge with a carried stack.

Ordinary fungible stacks retain the lowest matching destination ID. Instance-sensitive
stacks, such as ammunition with hit/damage enchantments, instead retire the matching
destination and allocate a fresh result ID. Held or equipped instance-sensitive
destinations are skipped, preserving exact pending claims. A whole transfer without
a merge retains its ID; a partial transfer preserves the source remainder and its
holds. Only already-free units move. The ordinary transfer validates those exact
witnesses before cancelling the acting character's previous preparation and
releasing its claims, within the same atomic action.

Physical equality does not identify either input. On an entitled pickup/drop split,
only the acting character's stored partial instance facts can follow the new ID.
On a fresh merge, only fields remembered equally for both inputs survive in the
acting character's new record; unknown or conflicting fields remain unknown. Numeric
knowledge is currently one composite field. This compares memories, never hidden
condition values. Other observers and stale retired-ID memories are untouched;
ordinary owned transfers such as theft do not copy the victim's private facts.
If bounded memory is full, the new inherited record is omitted without blocking the
physical transfer. Explicit identification retains its existing full-memory behavior.

Retired IDs are not reused. History records source ID, actual result ID and
transferred quantity, rather than the merged stack's total. Rewind restores the
saved items with their existing identities, but keeps allocation high-water marks
beyond the abandoned future. New stacks cannot reuse IDs from that future; see
[persistence and rewind](../architecture/persistence.md).

### Independent projectile definitions

See [independent projectile definitions](equipment.md#independent-projectile-definitions)
for ammunition, launcher families, standalone thrown payloads and recovery policy.


### Ordinary fruit

The `rogue.fruit` item is ordinary named Food with a generic food appearance,
not a ration reskin or a special consumable effect. It is stackable, has exactly
the ration's 1d400+1099 nutrition, and uses the same paid one-unit Eat, reserve cap,
debt settlement and food detection. Its `ration` supply tag deliberately means
interchangeable food supply, not literal identity: fruit never merges with a
ration stack. The authored `name` supplies its label through ordinary validation.
There is no mutable global fruit preference, extra effect, taste XP or starter
addition. Fruit can appear in supplemental floor stock and treasure rooms under
the [provisional live policy](scenario-packages.md#live-rogue-content-policy);
the existing ration stage and carried-supply pools are unchanged.

## Timed equipment and effects

See [timed equipment and effects](equipment.md#timed-equipment-and-effects) for
commands, anatomy and weapon selection, and
[paid item work and finite supply claims](equipment.md#paid-item-work-and-finite-supply-claims)
for exact holds, timing, completion, interruptions and actor-owned affordances.

### Armor and corrosion

See [armor and corrosion](equipment.md#armor-and-corrosion) for instance condition,
physical reduction, magical wards, corrosion, alteration and cursed removal.

### Equipment birth and the finite Rogue catalog

See [equipment birth and the finite Rogue catalog](equipment.md#equipment-birth-and-the-finite-rogue-catalog)
for generated condition, weapons, ammunition, armor and starting gear.


## Worn magic and the finite Rogue ring catalog

See [worn magic and the finite rogue ring catalog](magic-items.md#worn-magic-and-the-finite-rogue-ring-catalog) for the current contract.

## Magical wand and staff forms

See [magical wand and staff forms](magic-items.md#magical-wand-and-staff-forms) for the current contract.

## Reading, identification and charged devices

See [reading, identification and charged devices](magic-items.md#reading-identification-and-charged-devices) for the current contract.

## Monster-control scrolls

See [monster-control scrolls](magic-items.md#monster-control-scrolls) for the current contract.

## Trap avoidance and exposure resistance

See [trap avoidance and exposure resistance](magic-items.md#trap-avoidance-and-exposure-resistance) for the current contract.

## Levitation and relocation

See [levitation and relocation](magic-items.md#levitation-and-relocation) for the current contract.


## Quantity expressions

Shared quantity operands describe positive item units, independently of bounded
combat dice. `QuantityExpr` has three strict tagged forms:

- `{ kind = "fixed", quantity = 1 }`
- `{ kind = "uniform", minimum = 2, maximum = 61 }`
- `{ kind = "depth_scaled_uniform", minimum = 2, maximum_base = 51, per_depth = 10 }`

The last form computes the inclusive upper bound as
`maximum_base + per_depth * depth`, with checked u64 multiplication/addition. A compiled `QuantityRange`
requires `1 <= minimum <= maximum`; reversed, zero or overflowing ranges reject.
Rogue's gold expression is therefore uniform over `2..=51 + 10 * depth`.
This is an item quantity, not a damage roll or a bounded dice-pool workaround.

The consumer supplies context explicitly. Birth stock resolves at the birth floor's
depth; theft resolves the current floor depth when preparation freezes its plan.
A later transfer, pause or restart does not recalculate the captured range.
Nongenerated contexts must supply an explicit fixed amount or bounded range;
there is no floor-one, zero-depth or global-depth fallback. Resolving the range
is separate from sampling it. A fixed range consumes no random word; random
sampling and state changes belong to the consumer's atomic transaction.
Population authoring also accepts an integer as fixed-quantity shorthand. That
source convenience is not a compatibility reader or an alias for strict compiled
and saved records. Ordinary materialized stacks retain one concrete u64 quantity.

## Theft and gold

`magical` and optional `currency` are explicit item capabilities.
`currency = "gold"` marks ordinary stackable Gold units; the coin appearance class alone
does not grant currency behavior. Gold uses normal pickup/drop, splitting,
merging, inventory and death-drop authority, with no purse or second quantity
ledger. Runtime magical status is true for explicit authored magic or nonzero
owned curse, protection, armor penalty/ward or weapon enchantment. Corrosion alone
does not confer magic. Class, name and identity knowledge do not grant it. Rogue's
restore-strength potion, Amulet of Yendor and authored +1/+1 starter mace are
magical; its ordinary ring mail is not. These capabilities participate in
compatible item definitions and remain private authoritative facts rather than
new item-view fields.

The shared theft effect is legal only as the sole effect of a checked rider.
Primary effects, failure branches and item-consumable effects cannot contain it.
Authoring may omit the optional relocation gate; canonical saved records require
its nullable field explicitly, without a compatibility default.
The source declares either a currency selector with an authored quantity
expression or a single magical-unit selector. The Rogue Trickster talents use
separate Cognitive checks:

- TR1 Filch Gold samples one source-sized amount, then caps the transfer by all
  eligible free carried Gold across the target's stacks. It does not multiply
  the roll by stack count or reroll once per stack.
- TR2 Filch Charm transfers one free unequipped magical unit, including the
  unique Amulet when carried by the victim. It does not need the thief to know
  the true identity and does not teach that identity after transfer. Concealment
  and each actor's prior identity knowledge retain their ordinary meanings.
- TR3 Vanish supplies the source's relocation gate. The compiled active flag is
  captured when theft is prepared; changing the flag later does not rewrite the
  frozen plan. Relocation is permitted only after a positive actual transfer and
  only to a legal cell on the same floor. A failed check, empty candidate set or
  zero transferred units cannot trigger it.

Live resolution rechecks carried ownership, equipment and exact free units.
Item-owned preparation reservations exclude held units from theft just as they do
from ordinary transfer. Candidate planning cannot expose the victim's inventory
or hidden item facts to the requesting actor. The transfer, any relocation,
identity allocation, random state and resulting lifecycle updates share one
atomic publication boundary; failure cannot leave a partial transfer or duplicate
an item. Stolen gold and magic items become ordinary finite inventory and drop
through normal death cleanup. There is no kill-time loot creation or restitution
ledger. The unique objective's independent entry-relocation helper is unchanged.

These contracts add no new command, client field, theft menu or 3D feature.
[Status](../status.md) owns current qualification and outstanding work; the
coverage entry points below are not a claim of a fresh test run or full-run balance.

## AI item decisions

AI healing uses the acting actor's own identity knowledge. At half health or
below, an AI prefers a carried, known potion containing only positive healing
effects before fleeing or attacking. It does not infer effects from an unknown
appearance, borrow a human's knowledge, drink known harmful mixtures, or spend a
potion at full health. This uses ordinary timed item work and autonomous intention
execution. If no suitable potion exists, ordinary AI can select a granted,
affordable self-targeted ability whose frozen effects contain only healing,
using its own known carried or disclosed touch-range supplies. Candidate previews
use physical quantity only. At actual execution, finite-item contention selects
an ordinary validated Wait through the same admission and replay path; rejected
supply preflight itself spends no time or resources. The `ai-interactions` package
and actual-process tests cover known healing, knowledge isolation, consumption
timing and restart. AI equipment
replacement and nearby looting use the same ordinary action path.

Known gear must strictly improve the actor’s derived combat capabilities or
reduce effective basal nutrition use, without losing attack speed, damage kinds
or typed protection. Nutrition savings count only for actors with Required
nutrition and use the effective rate floored at zero; exempt actors gain no
nutrition benefit. Extra food upkeep does not veto an otherwise strict combat
upgrade. Candidate evaluation uses
the production modifier fold on a private loadout snapshot. Free matching sockets
are considered before replacement; removal completes through ordinary timed work,
and the upgrade is recomputed afterward. AI avoids gear work while fleeing or
adjacent to a visible hostile. Attack comparison uses the actor’s check score,
Speed-adjusted phases and damage grouped by category/descriptor. A clipped raw
mean ranks rolled damage; it does not predict rolls or read an enemy’s hidden
protection. Replacing fire with cold is a tradeoff, not a straight upgrade.

Loot candidates must be visible and known: useful gear, or a positive restorative
when none is carried. They must lie within three remembered route steps. Distance
then item ID orders candidates; both the destination and first step avoid visible
hostile reach. Pickup takes one unit, preserves the remainder and reuses the
existing route search. Unknown appearances never authorize these useful-item
decisions. An independently authored `gold` interest permits known reachable Gold pursuit and ordinary pickup,
after self-preservation and retaliation and before idle exploration. It does not
infer hidden currency from an appearance, imply general looting permission or
change the actor's hostile acquisition and memory contracts. See
[behavior defaults](scenario-packages.md#behavior-defaults-and-ai-overrides).

Physical projectile AI first chooses one exact request from identified type
capabilities, active expertise, its own remembered curse facts, available grips,
known resources and disclosed target geometry. It considers at most 128 carried
rows and 16 launcher candidates; these are conservative NPC search limits, not
limits on the player's paginated chooser. It does not probe real attack numbers
or hidden occupants to choose another weapon. A paused paid projectile resumes
its exact frozen terms before fresh expertise, cooldown or funding gates.

Actual numeric capture happens once at execution. A valid launcher/ammunition
combination outside the engine's checked numeric budget produces a typed refusal
and the actor takes an ordinary positive-time Wait, without trying an alternative
weapon. Failed capture changes no items, payment or randomness. Successful Wait
publication increments an optional bounded diagnostic counter; it exposes no
hidden operands. Invariant failures remain errors. Ordinary journal replay uses
the same request and refusal boundary. Unknown curse is discovered only by the
paid blocked equipped throw; remembered cursed equipped weapons are then avoided.
Invisible intervening creatures still stop and resolve the actual projectile.

This supports appropriately equipped NPCs using their authored abilities and
expertise. The current Rogue roster receives no new ranged loadouts or free
expertise. Equipment acquisition still compares the existing melee utility;
it does not yet value a bow for its ranged option. General ranged-loot valuation
is a separate follow-up, so this does not imply new ranged behavior throughout
the Rogue roster.

## Authoring and disclosure

Archetypes declare an optional physical `class` (default `misc`); item placements
may override it for ordinary items. Concealed items retain their archetype class,
and appearance pools share one class and the same equipment-slot/drink/eat/read/zap affordances
so these facts cannot reveal a hidden effect.
Classes are appearance facts, independent of mechanics, and survive transfer,
identification, rewind and persistence. They participate in stack compatibility.
ASCII uses `)` weapon, `[` armor, `!` potion, `%` food/corpse, `(` misc/tool,
`"` amulet, `=` ring, `?` scroll, `+` spellbook, `/` wand or staff, `$` coin and `*` gem.
An item pile draws the first disclosed item. Actors/terrain keep their existing
precedence, and remembered items keep the last disclosed class and grey color.
These classes do not implement food, spells, tools or other item-use mechanics.

Packages declare `quantity` (default 1), optional `stackable` overrides, and
string-valued `properties` on item placements. Archetypes default to non-stackable
and may declare `identity`, `appearance_pool`, `stackable`, `properties`,
`magical` and `currency`.
Instance properties override archetype properties by key. Non-stackable items
must have quantity 1; specific objective item instances must be non-stackable.
Properties are inert stack-compatibility metadata here; they do not activate item
effects, equipment, weight or capacity mechanics.

`appearance_pools.<key>` contains `appearances` and optional `confounding` (default
false). Archetypes sharing an identity must agree on its name, pool, physical
class, magical/currency capabilities, equipment and effects.
Sorted identity keys receive appearances shuffled deterministically from the game
seed, pool key, and appearance index with SHA-256. Ordinary pools require enough
distinct appearances. Explicit confounding pools permit reuse. Assignment covers
the manifest's archetypes, independently of character selection or current items.
Resolved appearances persist with authoritative item state and pinned inputs.

An item with an appearance pool conceals its identity until learned; other items
start known. Characters and other actors may declare `known_identities`. Learning an identity
identifies matching effects for that character, including other instances, without
identifying a different effect merely because it shares the appearance. Knowledge
survives dropping and persists independently of item ownership and existence.

The privileged `wizard identify <actor> <item>` command exercises knowledge
updates through the ordinary journal and rewind path. It requires existing wizard
authorization and marks the lineage through the existing wizard rules. Drinking
can teach identity through an observable self effect. Ordinary identification is
a [single paid scroll continuation](magic-items.md#identification-and-private-knowledge); it is not a free identify endpoint. Rogue's concealed restore-strength potion
uses this same path: positive actual deficit removal teaches identity; a no-deficit
dose is consumed without learning. Its sole effect is bounded Strength restoration,
with no extra healing or resource refill; see [Rogue restoration](../play/rogue.md#strength-loss-and-ordinary-restoration).

An item view discloses its opaque `id`, `name`, `description`, physical `class`,
`quantity`, `appearance`, `identified` status and optional `asset`. Ground views
also locate disclosed occurrences and indicate reachability. Carried interaction
views provide the owner’s affordances and permitted known equipment details; see
the [wire reference](protocol.md). Hidden archetype/identity keys, properties and magical/currency capabilities are never
wire fields. History and wizard summaries do not serialize authoritative specs.
Items use the current protocol, save format, ruleset, and scenario validator;
older prerelease saves are rejected without migration.

## Verification and profiling

`scenarios/tests/items` is an ordinary validated scenario for actual text,
adventure, and native ASCII process tests. Simulation and server tests cover
conservation, atomic invalid actions, overflow, ownership, compatible/incompatible
stacks, confounding appearances, character selection, seeded assignment, exact
restart through journal/checkpoint paths, and rewind of knowledge.

Items workload v1 (`cargo run --release -p tor-server --example item_bench`) uses
16/1,000 items and 8/256 known identities, 20 samples each, 20 alternating partial
pickup/drop commands per sample. Each sample learns the final identity through a
timed wizard command, with wizard enablement outside the timer. It reports
construction, knowledge acquisition, transfer commands, explicit save
barrier and restart timing, real client-state application and ASCII canvas drawing
(without native-window presentation), disclosed/save bytes, observation/scene counts, item
candidates, stack candidates and knowledge checks. Authoring, validation, state
lookup between commands, and JSON reporting are outside command timing. Operation
counts include those untimed between-command state lookups. Client application and
drawing have separate timers; server observation extraction and JSON reporting
are outside those timers. Summarize with
`python scripts/item_performance_report.py <log>`; its validator rejects missing,
duplicate, malformed or nonfinite samples.

Retain scenario workload v1 unchanged for matching before/after comparisons.
Neither workload resolves the open long-run performance findings or replaces
real-client acceptance tests. Historical measurements found restart overhead and
variable persistence tails; their causes and targets remain open. See
[performance measurement and evidence](../development/performance.md) for dated results,
comparable workload boundaries and the retained adaptation release assets.


The implementation lives in [interactions.rs](../../crates/simulation/src/interactions.rs),
[items.rs](../../crates/simulation/src/items.rs) and
[AI item decisions](../../crates/simulation/src/ai_items.rs). Coverage includes
[AI regressions](../../crates/simulation/src/ai.rs) and
[actual interaction processes](../../scripts/test_interactions_process.py).
The non-graphical ring witness in
[magic-item processes](../../scripts/test_magic_items_process.py)
(`test_ring_wear_paid_union_choice_curse_and_source_free_restart`) covers ordinary
wear, paid ring-or-wand selection, exact-instance curse/knowledge isolation and
source-free restart; it does not depend on native-window tests.


## Explicit edibility

An item archetype may declare
`edible = { nutrition = { kind = "fixed", amount = 6 } }`, or use a bounded
rolled operand such as
`edible = { nutrition = { kind = "rolled", dice = { count = 1, sides = 400, bonus = 1099 } } }`.
The latter produces 1100–1499 through the shared neutral dice resolver; neither
example selects a production meal policy. Every possible result must be positive:
fixed amounts are 1–1000000, and rolled amounts use the shared bounded dice pool
with `count + bonus >= 1`. Each enclosing operand and nested dice record has one
strict named shape; scalar meals and positional records are rejected. Class,
name, appearance, identity and being a corpse never imply edibility. Equipment
and potion-consumable capabilities cannot coexist with edibility. An edible
item can also be an explicitly authored supply, sharing its one physical stock.
Appearance pools must share edible affordances so concealed identity cannot leak
through choices.

Eat freezes one declared fixed or rolled Nourish effect and the normal consumption
duration, holds exactly one unit of the selected carried stack, and spends that unit once at
resolution. It never chooses another stack or creates a second quantity ledger.
Cancellation, forced displacement and death release an unpaid hold; pausing
retains the existing preparation contract. A last unit may disappear while its
frozen effect and completion identity remain valid. Nourishment clears negative
reserve debt, adds the resolved amount, caps at the authored maximum and keeps
the digestion remainder. Rolled meals draw only at successful completion through
the persisted rules stream, after starvation and held-item validity checks. Queries,
admission, preparation, pause/resume, cancellation and checkpoint restoration do
not draw. Full or exempt creatures still consume the selected unit and resolve its
roll even when the applied gain is zero. Item spending, the roll and state changes
share one atomic transaction; replay applies the canonical gain without rerolling.
The disclosed edible affordance reveals no operand, dice, bounds, sample or RNG
state, including after identification. See
[nutrition](creature-rules.md#shared-nutrition-eating-and-starvation) for hunger,
deadline ordering, and persistent death.
