# Items and character knowledge

Milestone 4b adds quantities, pickup/drop, compatible stacking and character-owned
identity knowledge. The interaction adaptation adds anatomy-based equipment and
timed potion effects to the simulation, server and clients. Capacity, containers and
ordinary identification actions remain deferred. NetHack informs the interaction style;
these are this project's explicit rules, not a claim of exact NetHack behavior.

## Ordinary play

After setting the server token as described in the [README](../README.md), run
`cargo run -p tor-server -- --scenario scenarios/tests/items --character 1 --save saves/items-demo.db`
and connect either client normally. This ordinary package demonstrates stacks and
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
no destination stack. Retired IDs are not reused on the same timeline. History
records source ID, result ID and transferred quantity. Rewind restores allocation.

## Timed equipment and initial effects

The server accepts `equip` (carried item plus anatomy slot index), `unequip`
(carried equipped item) and `drink` (carried potion) through ordinary intention
admission. Targets are opaque observer-scoped inventory references. Text accepts
`equip`/`wear`/`wield`, `remove`/`unequip`, and `drink`/`quaff` with a carried
item name or opaque target. Adventure resolves disclosed nouns with its existing
choice rules. ASCII uses W to equip, T to remove and Q to drink; Up/Down and Enter
choose among items, and Escape cancels without spending a turn. These commands
act on one item at a time and have no quantity editor. Clients choose the first free
matching anatomy socket; replacing gear requires separate removal. Both clients
use the shared `client-common` item checks, while the simulation validates the
submitted action against authoritative state.

Anatomy is an ordered slot list; duplicate kinds provide distinct sockets, such
as two rings. Archetypes and actors can declare `anatomy = { slots = [...] }`.
Character declarations supply their own anatomy. Item archetypes optionally
provide `equipment` or `consumable`; placements use `equipped_slot` with
`carried_by` for starting gear. Invalid classes, counts, slots, conflicting gear
and effect definitions are rejected. The initial weapon model equips one weapon;
dual wielding remains outside this slice.

Body armor changes take three actor turns; other equipment changes and drinking
take one. Effects apply only on completion. Equipped armor protects during
removal, occupied slots require removal before replacement, and equipped items
cannot be dropped. Damage or a newly visible hostile pauses work, preserving
progress and the original admission. A resume accepts the current threat baseline.
Waiting preserves progress; a different successful action can discard it. Explicit
cancellation of item preparation is rejected. Death clears equipment and drops each
remaining carried stack once alongside the corpse.

Consumables contain a sequence of shared `heal` and typed `damage` effect
primitives. Definitions are validated before mutation. Healing caps at maximum
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

## Authoring and disclosure

Archetypes declare an optional physical `class` (default `misc`); item placements
may override it for ordinary items. Concealed items retain their archetype class,
and appearance pools share one class and the same equipment-slot/drink affordances
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
and may declare `identity`, `appearance_pool`, `stackable`, and `properties`.
Instance properties override archetype properties by key. Non-stackable items
must have quantity 1; specific objective item instances must be non-stackable.
Properties are inert stack-compatibility metadata here; they do not activate item
effects, equipment, weight or capacity mechanics.

`appearance_pools.<key>` contains `appearances` and optional `confounding` (default
false). Archetypes sharing an identity must agree on its name, pool, physical
class, equipment and effects.
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
identification actions remain deferred.

Clients receive only disclosed `name`, `description`, `appearance`, `identified`,
quantity and instance ID. Hidden archetype/identity keys and properties are never
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
New item mechanics have no pre-4b behavior baseline. Neither workload closes the
deferred 3p long-run findings or replaces real-client acceptance tests.

### Recorded results

Measured 2026-09-27 on the maintainer's Windows machine, against the 4a
implementation as the baseline. The
[archived summary and manifest](https://github.com/KCall360/thresholds-of-ruin/tree/docs-history-2026-09/docs/measurements/items-2026-09-27) record all distributions,
operation and byte counts, and source and binary hashes.

| Items / identities | Transfer p95 / max (ms) | Client apply p95 (ms) | Canvas p95 (ms) | Save p95 (ms) | Restart p95 (ms) |
| --- | --- | --- | --- | --- | --- |
| 16 / 8 | 0.144 / 0.225 | 0.162 | 1.111 | 46.971 | 9.762 |
| 1,000 / 256 | 1.265 / 1.805 | 0.894 | 1.368 | 89.390 | 47.446 |

Transfers use an in-memory engine; these numbers do not establish durable-action
latency or native-window/network latency. The large workload examines 10,010
merge candidates across 20 transfers. Non-stackable transfers skip the merge
scan, enforced by an operation-count regression test.

Scenario workload v1 retained its timing boundaries. At 256 regions, action p95
was 0.0103 ms before and 0.0121 ms after; two repeat runs measured 0.0122 and
0.0116 ms. Restart median increased from 10.6101 ms to 11.7136 ms, with repeat
medians 11.6870 and 11.6986 ms. Restart p95 was 15.4556 ms before and
19.4469/26.1246/23.8392 ms afterward. Save-barrier p95 also varied across runs
(38.7462 ms before; 36.8009/67.2715/42.7563 ms afterward).

The investigation corrected a permission-profile mismatch, repeated the same
workload twice without source changes, and confirmed unchanged large-scenario
save size (139,264 bytes). Initial desktop-permission runs are retained separately
and excluded from matching comparisons. These checks show stable action cost and
a repeatable roughly 1.1 ms restart-median increase; they do not isolate its cause
or attribute all tails to disk I/O. This measured restart overhead and variable
persistence tails remain limitations, with no restart target or deferred 3p
criterion declared satisfied. No latency targets were relaxed.
