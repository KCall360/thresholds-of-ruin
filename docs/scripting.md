# Scenario scripting

**Status: desired feature; runtime and implementation deferred.** This note records the direction agreed in
discussion. Nothing is implemented, and the scenario package format is
unchanged. Adding scripts to packages is a compatibility-breaking format
decision that needs the maintainer's authorization before work starts. It is
sequenced after [three-dimensional sight](sight-3d.md) and after the 4e runtime
transition contract (see [region streaming](region-streaming.md)).

## Goal

Runtime selection and implementation are deferred. Luau remains a candidate,
not an accepted dependency. The current [refactor plan](refactoring.md) uses
language-independent contracts for scoped queries, validated effects, scheduling,
persisted values, and pinned handler identities. The runtime-specific sketches
below are exploratory and must be re-evaluated before any scripting work starts.

Scenarios, maps, and objects need small pieces of specific behaviour: a lever
that opens a far door, a message when the objective item reaches the exit, an
egg that hatches, a potion whose effect ends with a side effect. Supporting each
of these as its own engine feature doesn't scale. Scripts are the home for this
kind of niche, content-specific logic.

The engine stays the home for the game's rules. When in doubt, put it in the
engine:

| Engine (Rust) | Scripts |
| --- | --- |
| Movement, doors, physics, combat, sight, AI decisions | Scenario glue: puzzles, story beats, one-off reactions |
| Timers, timed effects, damage, healing, spawning, material changes | Specific spells, potions, and item behaviours composed from those primitives |
| Anything that runs every tick, scans many cells, or searches | Short handlers that run when something happens |
| Validation of every change to the world | Requests for changes, as commands |

Existing combat, items, and the objective rule stay in Rust. The engine grows a
new primitive only when many scenarios need it or the rule belongs to the
simulation.

## Events

Scripts never run between an action's validation and its effects. The action
path has no callback, yield, or external mutation at that point, and scripting
keeps that rule. Handlers run after effects are applied, at the points where the
objective check runs today (the end of an action and physics settlement).

Candidate events come from facts the simulation already produces:

- action outcomes: moved, door changed, item taken or dropped
- combat events: attack resolved, interrupted, died
- a character's first visit to a region
- the objective being reached
- a timer firing
- an archetype's own hooks, such as `on_use`

When several handlers fire at the same point, they run in a fixed order: due
tick, then creation sequence, then stable identifiers. A handler that affects
many targets, such as an area effect, gets one call with all targets, not one
call per target.

## Commands and queries

A handler receives a context object. Through it, the handler can:

- **Query** state: who is at an anchor or cell, what an actor carries, whether a
  door is open, the value of a named variable, an effect's remaining time.
- **Issue commands**: open, close, or lock a door; spawn an archetype; move or
  remove an item; apply or remove a timed effect; set a named variable; start or
  cancel a timer; show a message; end the run.
- **Adjust AI inputs**: set a target, switch an AI profile, make a creature
  flee or become calm. Scripts never choose an AI action. The AI still decides,
  and actions are still checked against its choice.

Commands are checked like player actions. A rejected command is a deterministic
outcome recorded in the journal, not an error that depends on timing. The
command set starts small and grows as scenarios need it.

Script messages go through the same disclosure rules as other messages, so a
script can't reveal state a client isn't allowed to see.

## State

Scripts may use temporary variables during a call. Anything that must persist
lives in game state through the API:

- **Named variables**: typed flags and counters, scoped to the scenario, a
  region, or an entity.
- **Timers**: see below.
- **Timed effects**: engine data with a kind, target, remaining time, strength,
  and stacking rule. Because the engine understands them, it can apply
  resistance, dispelling, and disclosure to them, and clients can show them.

All of this is part of the simulation snapshot, so checkpoints, rewind, and
replay cover it without special handling. The script runtime itself holds no
game state, and nothing inside it is saved.

## Timers

Timers follow the model NetHack uses for eggs, corpses, and burning lights. A
timer is plain data:

- a kind, which maps to a handler name such as `eggs.hatch`
- a target: an item, an actor, a cell, or nothing
- a due tick
- a small integer argument

Saves store the handler name, never a function. Timers follow their owner when
it is carried, dropped, or contained. When the owner is destroyed, its timers
are removed or passed on by rule. Items stack only when their timers match.

Timers in inactive regions are frozen. Like NetHack's timers on other levels,
they are stored as time remaining and resume without catch-up when the region
reactivates. This matches the 4e rule for scheduler and AI time, and depends on
that contract being defined first.

Ongoing effects, such as regeneration, use an engine primitive with a
script-chosen rate, or a timer that re-arms itself at a coarse interval. No
script runs every tick.

## Language selection (deferred)

Luau is a candidate for later evaluation. Selection needs evidence about
determinism, isolation, resource accounting, author tooling, and cross-platform
builds under representative scenario workloads. No runtime dependency or
comparison spike is part of the current refactor.

## Sandbox and budgets

Every package is treated as untrusted:

- Only safe standard libraries are opened: string, table, and a trimmed math
  library. There is no file, OS, clock, network, or debug access.
- `math.random` is replaced by a function that draws from the game's seeded
  random stream.
- Each call needs a deterministic work budget, and the runtime needs a memory
  limit. Exhausting either rejects the handler deterministically,
  and none of its commands are applied.
- Validated artifacts are pinned with the package and available before the
  handler's region activates. Play never reads mutable script source files.

To keep script cost small:

- Heavy work, such as finding actors in an area, line of sight, or pathfinding,
  is done by engine functions that scripts call.
- Handlers receive numeric identifiers and query what they need, rather than
  receiving copies of world data.
- Multi-target events are batched into one call.

## Package format

Script files would sit beside region files and be listed in the manifest.
Handlers are named `module.function`, where the module is the table a script
file returns:

```toml
# scenario.toml (sketch)
scripts = ["scripts/eggs.luau", "scripts/doors.luau"]

[archetypes.egg]
timers = { hatch = { handler = "eggs.hatch", after = 3000 } }

[archetypes.lever]
on_use = "doors.pull_lever"
```

```lua
-- scripts/eggs.luau
local eggs = {}

function eggs.hatch(ctx, egg)
    local cell = ctx:location_of(egg)
    ctx:remove_item(egg)
    ctx:spawn("hatchling", cell)
    ctx:message_near(cell, "The egg cracks open.")
end

return eggs
```

- The manifest rejects unknown fields, so `scripts` and handler fields need a
  schema change. That means a format, ruleset, and validator change.
- Script text is included in the package's content and model hashes, so a save
  is bound to the exact scripts it was played with. The package size limit
  covers scripts.
- The validator compiles every script and checks that every handler named in
  the manifest exists. A package with a broken script fails validation.

## Runtime boundary

The future runtime lives outside `Game` and checkpoints. It receives scoped
queries and proposes typed effects; simulation owns validation and scheduling.
Persisted state contains values and stable handler identities, never VM objects,
closures, or function pointers. Loading, restoring, and replay construct a fresh
runtime from pinned artifacts. Handler failure discards proposed effects and state
changes together. Nested effects and randomness have deterministic ordering and
budgets. The simulation crate must remain independent of runtime implementation.

## Determinism risks

Replay requires every recomputed journal entry to equal the recorded one. The
risks for scripts are:

- **Leftover state.** Sandbox mode stops writes to shared globals, but a local
  variable declared at the top of a script file keeps its value between calls.
  Freezing each module's returned table helps. How to prevent this fully is an
  open question. The replay check would catch it, but only after the fact.
- **Table iteration order.** Any runtime's iteration order must be explicitly
  controlled and verified across platforms, rather than assumed stable.
- **Numbers.** The API must represent the full range of identifiers and ticks
  without precision loss. Merely using a numeric type does not establish this.
- **Randomness.** Only the seeded game stream is available.

## Open questions

- How to enforce "no state between calls" rather than only detect it.
- How wizard-mode operations interact with script state and timers.
- How script failures are reported to scenario authors.
- The exact first command and query set, and the scope of AI-input commands.
- Test fixtures under `scenarios/tests/`. The first proof would be an egg-style
  hatch timer and a timed buff.
