# Scenario scripting

**Status: desired feature; runtime and implementation deferred.** This note records the direction agreed in
discussion. Nothing is implemented, and the scenario package format is
unchanged. Adding scripts to packages is a compatibility-breaking format
decision that needs the maintainer's authorization before work starts. Existing [three-dimensional sight](../architecture/perception.md) and
[region lifecycle](../architecture/streaming.md) provide foundations; their implementation
does not authorize a scripting milestone.

## Goal

Runtime selection and implementation are deferred. Luau remains a candidate,
not an accepted dependency. The [architecture](../architecture/overview.md) requires
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

Existing combat, items, and objective support stay in Rust. Future scenario
hooks may compose authoritative queries and the victory operation, rather than
requiring a built-in objective variant for every scenario. The Rogue plan uses
an escape event plus a carried-Amulet check; tutorials motivate actor-specific
perception hooks. These APIs are planned, not executable package fields. The engine grows a
new primitive only when many scenarios need it or the rule belongs to the
simulation.

## Required architectural boundaries

The following boundaries constrain any future design. They are not claims that a
handler API, timer schema or runtime already exists.

- **Scoped authority.** Controller handlers query disclosed observations and
  remembered navigation. Scenario-world handlers may need broader, explicitly
  granted regional authority for hidden encounters. Neither grants clients private
  state. Region-local queries distinguish unknown, unloaded and absent data; they
  must not eagerly generate everything referenced. Activation or mutation of another
  region is a separate validated effect under ordinary lifecycle policy.
- **Typed transactions.** Handlers propose bounded effects or intentions, never
  receive mutable engine/store/socket/world references. Validate against private
  candidate state; revalidate queued intentions when executed. Admit required
  inputs and the operation/result to persistence before publishing observations.
  Failure discards the enclosing proposed state/effect batch, not just its last
  command. Clients invoke authorized opaque interactions, not arbitrary functions.
- **Deterministic execution.** Use named hooks at documented boundaries, explicit
  ordering and total budgets for recursive events. Explicit named random streams
  survive save/replay/rewind. Work, effects and persistent state are bounded.
  A host wall-clock timeout may protect availability but cannot pick a different
  successful simulation result or publish half an invocation.
- **Pinned, value-only state.** Persistent state has a bounded typed schema keyed
  by stable content and owner identities. Save values, scheduled intentions and
  content/API identities, never interpreter stacks, closures or handles. Restore
  builds a fresh runtime from pinned artifacts and rejects unsupported identities.
  Regional ownership controls clock/lifetime and freeze/thaw. Lazy regions do not
  allocate handler state before activation. Rewind restores pending work with state;
  recovery retains fresh-human-input requirements. Historical runtime migration
  requires its own compatibility decision.

The sections below explore a possible event and command design. Exact event
names, handler order, timer record fields, stacking rules, package syntax and
Luau library choices remain proposals to validate before implementation.

## Events

Scripts never run between an action's validation and its effects. The action
path has no callback, yield, or external mutation at that point, and scripting
keeps that rule. Handlers run after effects are applied, at the points where the
objective check runs today (the end of an action and physics settlement).

Candidate events come from facts the simulation already produces:

- action outcomes: moved, door changed, item taken or dropped
- combat events: attack resolved, interrupted, died
- a character's first visit to a region
- the objective being reached or a character escaping a dungeon
- an actor perceiving a particular mob, with first/repeat behavior explicitly defined
- a timer firing
- an archetype's own hooks, such as `on_use`

One proposed deterministic order for handlers at the same point is: due
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

Commands are checked like player actions. A rejected command needs a deterministic outcome and journal policy rather
than behavior that depends on host timing; the transaction rule above forbids
partially publishing a failed batch. The
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

If adopted, these values must be integrated into simulation snapshots,
checkpoints, rewind and replay with explicit validating schemas and tests. The script runtime itself holds no
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

Timers in inactive regions are frozen. They are stored as time remaining and
resume without catch-up when the region reactivates. This follows the implemented regional freeze/no-catch-up contract;
exact timer ownership and transfer rules still need design.

Ongoing effects, such as regeneration, use an engine primitive with a
script-chosen rate, or a timer that re-arms itself at a coarse interval. No
script runs every tick.

## Generation versus runtime scripting

Procedural world authoring starts with declarative recipes. Consider generation
script extensions only where reusable stages cannot express the requirement
clearly. Generation and runtime scripting are separate decisions; neither a VM
nor event hooks are prerequisites for the live Rogue scenario.
See the [procedural requirements](requirements.md).

## Language selection (deferred)

Luau is a candidate for later evaluation. Selection needs evidence about
determinism, isolation, resource accounting, author tooling, and cross-platform
builds under representative scenario workloads. No runtime dependency or
comparison spike is currently authorized by this design note.

## Sandbox and budgets

Every package must be treated as untrusted. For a Luau-like candidate, the
proposed sandbox would expose only these capabilities:

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

## Exploratory package and language sketch

The following is not valid current package input. Unknown script and handler
fields are rejected today. Script files would sit beside region files and be listed in the manifest.
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
- In the proposed format, script text is included in the package's content and model hashes, so a save
  is bound to the exact scripts it was played with. The package size limit
  covers scripts.
- The proposed validator compiles every script and checks that every handler named in
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

## Verification needed before adoption

A runtime decision needs representative Windows/Linux determinism and isolation
checks, bounded-work and memory failure cases, exact full-width ID/tick handling,
source-pinning and strict restore tests, freeze/thaw and rewind equivalence, and
actual-process disclosure/interaction tests. Prove that a failed cascade changes
neither authoritative state nor successful RNG history. Verify module state cannot
leak between calls rather than relying on replay mismatch to detect it later.
Existing content primitives and declarative recipes remain the default until
these questions are settled; scripts are not a prerequisite for generated Rogue
exploration. [Project status](../status.md) and [remaining requirements](requirements.md)
own sequencing and scope approval.
