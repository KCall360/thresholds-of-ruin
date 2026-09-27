# Items and character knowledge

Milestone 4b adds quantities, pickup/drop, compatible stacking and character-owned
identity knowledge. Equipment, capacity, containers, item use, and ordinary
identification gameplay remain deferred. NetHack informs the interaction style;
these are this project's explicit rules, not a claim of exact NetHack behavior.

## Ordinary play

After setting the server token as described in the [README](../README.md), run
`cargo run -p tor-server -- --scenario scenarios/tests/items --character 1 --save saves/items-demo.db`
and connect either client normally. This ordinary package demonstrates stacks and
concealed potions without wizard mode. `take 3 #10`, `take #11`, and `drop 2 #23`
exercise a split, merge, and partial drop in a fresh game.

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

## Authoring and disclosure

Packages declare `quantity` (default 1), optional `stackable` overrides, and
string-valued `properties` on item placements. Archetypes default to non-stackable
and may declare `identity`, `appearance_pool`, `stackable`, and `properties`.
Instance properties override archetype properties by key. Non-stackable items
must have quantity 1; specific objective item instances must be non-stackable.
Properties are inert stack-compatibility metadata here; they do not activate item
effects, equipment, weight or capacity mechanics.

`appearance_pools.<key>` contains `appearances` and optional `confounding` (default
false). Archetypes sharing an identity must agree on the identity name and pool.
Sorted identity keys receive appearances shuffled deterministically from the game
seed, pool key, and appearance index with SHA-256. Ordinary pools require enough
distinct appearances. Explicit confounding pools permit reuse. Assignment covers
the manifest's archetypes, independently of character selection or current items.
Resolved appearances persist with authoritative item state and pinned inputs.

An item with an appearance pool conceals its identity until learned; other items
start known. Characters may declare `known_identities`. Learning an identity
identifies matching effects for that character, including other instances, without
identifying a different effect merely because it shares the appearance. Knowledge
survives dropping and persists independently of item ownership and existence.

The privileged `wizard identify <actor> <item>` command exercises knowledge
updates through the ordinary journal and rewind path. It requires existing wizard
authorization and marks the lineage through the existing wizard rules. No normal
item-use or identification action is introduced.

Clients receive only disclosed `name`, `description`, `appearance`, `identified`,
quantity and instance ID. Hidden archetype/identity keys and properties are never
wire fields. History and wizard summaries do not serialize authoritative specs.
Protocol 15, save format/SQLite version 10, ruleset `physics-v15`, and validator
`tor-scenario-3` replace the previous versions without migration readers.

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

### Recorded results (2026-09-27, Windows)

Verification passed: 257 Rust tests in each of debug and release, 108 debug
Python checks, 73 release process checks, all-target Clippy, rustdoc with warnings
denied, and all 22 package validations. Text, ASCII, and Text + ASCII Spectator
desktop launchers connected successfully, retained fresh saves, and cleaned up
their owned processes. Linux verification remains a CI requirement.

The [measurement summary](measurements/items-2026-09-27/summary.json) records
p50/p95/maximum, sample counts and operation/byte counts. Its
[manifest](measurements/items-2026-09-27/manifest.json) identifies source, binary
and raw artifact hashes. The baseline is the local unpublished 4a implementation.

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
