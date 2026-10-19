# Items and character knowledge

Items support quantity-aware pickup/drop, compatible stacking, character-owned
identity knowledge, anatomy-based equipment, timed consumption with explicit edibility,
and ordinary currency/theft transfers. Capacity, containers and
ordinary identification actions remain deferred. NetHack informs the interaction style;
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

ASCII uses G for pickup and D for drop; stairs use `<` and `>`. A single one-unit
item transfers immediately. Otherwise Up/Down selects a stack, typed digits set
the quantity, Backspace edits it, Enter confirms, and Escape cancels. Blank means
the entire stack. Inventory and ground lists show counts. Inspection is free.

Pickup requires the actor's current ground cell; drop requires ownership and
places items at that cell. Each successful transfer costs half the actor's turn,
rounded upward, independent of quantity. Zero, excessive, overflowing, unavailable,
and unauthorized requests do not partially mutate state or spend simulation time.

Only explicitly stackable items merge. Archetype, true identity, name, appearance,
concealment and authored instance properties must match. Character knowledge is
not a physical stack property. If several destination stacks match, the lowest
ID survives. A whole transfer retains its ID when it does not merge. Partial
transfers retain the source ID and allocate a new monotonic ID only when there is
no destination stack. Retired IDs are not reused. History records source ID, result ID and
transferred quantity. Rewind restores the saved items with their existing
identities, but keeps allocation high-water marks beyond the abandoned future.
New stacks cannot reuse IDs from that future; see
[persistence and rewind](../architecture/persistence.md).

## Timed equipment and effects

The server accepts `equip` (carried item plus anatomy slot index), `unequip`
(carried equipped item) `drink` (carried potion) and `eat` (carried explicitly edible item) through ordinary intention
admission. Targets are opaque observer-scoped inventory references. Text accepts
`equip`/`wear`/`wield`, `remove`/`unequip`, `drink`/`quaff`, and `eat` with a carried
item name or opaque target. Adventure resolves disclosed nouns with its existing
choice rules. ASCII uses W to equip, T to remove and Q to drink and E to eat; Up/Down and Enter
choose among items, and Escape cancels without spending a turn. These commands
act on one item at a time and have no quantity editor. Clients choose the first free
matching anatomy socket; replacing gear requires separate removal. Both clients
use the shared `client-common` item checks, while the simulation validates the
submitted action against authoritative state. Direct text offers opaque choices
for ambiguous names; adventure uses disclosed order for indistinguishable stacks.
Selection clears on cancellation, changed observations, control loss and disconnect.
Completion narration belongs to the owning actor, handles noncombat equipment
and the last consumed unit, and does not repeat stale observations.

Anatomy is an ordered slot list; duplicate kinds provide distinct sockets, such
as two rings. Archetypes and actors can declare `anatomy = { slots = [...] }`.
Character declarations supply their own anatomy. Item archetypes optionally
provide `equipment`, `consumable`, or a typed catalog `supply` identity; placements use `equipped_slot` with
`carried_by` for starting gear. Invalid classes, counts, slots, conflicting gear
and effect definitions are rejected. Multiple matching sockets may hold weapons;
the highest equipped weapon-capable socket supplies the selected attack. Other
equipment still contributes its modifiers. Simultaneous dual attacks are not implemented.

### Armor and corrosion

Ordinary armor uses an explicit equipment capability, for example
`armor = { physical_reduction = 2, material = "metal", rustproof = false }`.
Its Physical protection is additive damage reduction for Impact and Keen, rather
than a Physical-defense bonus. Energy, Spirit and Vital damage and unrelated
modifiers retain their ordinary rules. `material` is the closed `metal`/`nonmetal`
choice; string-valued item properties never grant armor or rust protection.

An item definition is shared and immutable. Each actual item owns its persisted
`condition = { corrosion = 0 }`; loss cannot exceed that item's original armor
contribution. Nonmetal, rustproof and non-armor items cannot carry corrosion.
The item ID and loss survive transfer, dropping, death, recovery and re-equipping.
Two items sharing a definition can have different losses. Equip, AI comparison,
restoration and identified equipment views use the same effective contribution.
A change folds the existing permanent compiled sheet, without walking HD rows.

The generic `corrode_armor` effect names an equipment-slot kind and a positive
bounded amount. It selects the first currently equipped socket of that kind,
reduces rustable item-owned Physical armor protection by that amount, and floors
at zero. Empty, nonmetal, rustproof and already exhausted armor are no-ops.
Corrosion is an isolated effect payload: the exact item witness and actor sheet
publish in the enclosing action transaction; later strikes see the new protection.
No hidden inventory identity appears in its public result. CO1 uses one attempt
per source occurrence after at least one successful natural contact, even when
that contact deals zero damage.

Authoring may omit `armor` and defaults `rustproof` to false. Current owned
records require the explicit nullable armor field, all armor fields and the
item condition; unknown fields, positional aliases and impossible loss reject.

One unit of a supply stack provides one unit of its named supply. Supply identity
participates in physical stack equality, cloning and saved item definitions.
Equipment cannot also be supply content, and unknown supply references reject
at package compilation and item validation. Authoring may omit `supply`; owned
item records require the field with explicit null when it is absent.

Finite ability costs, drinking and eating use the same item-owned authority. Successful
preparation holds exact item IDs under its original issued intention. A stack's
physical quantity is authoritative; preparations retain references, not a second
quantity balance. The bounds are 8 distinct cost rows, 32 selected fragments per
action and 64 holds per stack. Ordinary transfer can move only unheld units.
Forced physical movement, lost access, invalidated work and death retire the
whole unpaid claim. A valid first resolution spends once even when the effect
misses, is immune or would heal an already-full target. Paid strike tails keep
their historical selection without requiring deleted sources to remain loaded.

Selection defaults to known carried supplies. An explicit selection may instead
name disclosed ground stacks within body contact when the authored cost permits
it. It cannot reach another inventory or substitute a different same-kind item.
Views expose known supply identity and physical quantity, never holds, free-unit
counts, claimant identities or private preparation state. Ground supply contact
is separate from pickup reach. Frozen/detached records retain reciprocal
actor/item claims; incomplete or contradictory restored relationships reject.

Under the shared default rules, body armor changes take three actor turns;
other equipment changes, drinking and eating take one. Equip/remove multipliers belong
to each rules-defined slot, and consume timing is a ruleset constant; validated
scenario overrides can change these defaults. Effects apply only on completion. Equipped armor protects during
removal, occupied slots require removal before replacement, and equipped items
cannot be dropped. Damage or a newly visible hostile pauses work, preserving
progress and the original intention identity. A resume accepts the current threat baseline.
Waiting preserves progress; a different successful action can discard it. Explicit
cancellation of item preparation is rejected. Death clears equipment and drops each
remaining carried stack once alongside the corpse.

Consumables contain a sequence of the same `heal`, typed `damage`, restorable
attribute/maximum-Health loss or restoration, condition
application and resource-restoration effects used by abilities. Definitions are validated before mutation. Healing caps at maximum
health; damage uses effective reductions and immunities. A lethal effect ends the
sequence. Exactly one unit is consumed on completion. Observable health effects
teach identity to the actor, even when the consumed stack disappears; an
unobservable effect does not identify it.

The optional observation `interactions` contains the controlled actor's slots,
preparation and carried-item affordances, independently of combat attributes.
Equipment statistics are disclosed only for known identities. Item effects and
other actors' inventory are absent. This state reconstructs exactly through full
observations, deltas, checkpoints and replay. The validated
`scenarios/tests/interactions` package and `test_interactions_process.py` cover
starting gear, timing, disclosure, consumption and checkpoint restart.

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
ledger. Magical status is authored independently of class, name, modifiers and
identity knowledge. Rogue's restore-strength potion and Amulet of Yendor are
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

These contracts add no new command, client field, theft menu or 3D feature. See
[status](../status.md) for the theft/gold batch's outstanding validation; existing
item tests below do not by themselves qualify this new behavior.

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

Known gear must strictly improve the actor’s derived combat attributes without
losing attack speed, damage kinds or typed protection. Candidate evaluation uses
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

## Authoring and disclosure

Archetypes declare an optional physical `class` (default `misc`); item placements
may override it for ordinary items. Concealed items retain their archetype class,
and appearance pools share one class and the same equipment-slot/drink/eat affordances
so these facts cannot reveal a hidden effect.
Classes are appearance facts, independent of mechanics, and survive transfer,
identification, rewind and persistence. They participate in stack compatibility.
ASCII uses `)` weapon, `[` armor, `!` potion, `%` food/corpse, `(` misc/tool,
`"` amulet, `=` ring, `?` scroll, `+` spellbook, `/` wand, `$` coin and `*` gem.
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
can teach identity through an observable effect; standalone ordinary
identification actions remain deferred. Rogue's concealed restore-strength potion
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
