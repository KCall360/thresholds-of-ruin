# Architecture and ownership

Thresholds of Ruin has one authoritative backend and interchangeable text,
graphical ASCII and headless clients, plus an optional procedural 3D prototype.
A game can continue across client switches.
Actor identity and control ownership are explicit: there is no global player.
Gameplay is single-player; multiplayer input policy remains future work.
The 3D frontend is maintained in a separate workspace with its own verification.
Windows is the primary platform and Linux is continuously
tested. [Project status](../status.md) owns supported formats and implementation
scope; this page explains boundaries and why they exist.

## Workspace boundaries

| Crate | Owns |
| --- | --- |
| `tor-world` | Region-local geometry, entities, containment and portals |
| `tor-simulation` | Rules, deterministic transitions, scheduler, perception and actor knowledge |
| `tor-protocol` | Versioned commands, disclosed observations and semantic events |
| `tor-server` | Sessions, authority, networking, travel policy, persistence and package compilation; the `tor-scenario` tool |
| `tor-client-common` | Connections and validated disclosed-state models |
| `tor-client-ascii` | Native graphical ASCII input and presentation |
| `tor-client-text` | Deterministic language parsing, clarification and prose |
| `tor-client-headless` | JSON-lines scripted play and disclosed-state inspection |
| `tor-test-support` | Shared fixtures and cross-crate test orchestration |

World and simulation perform no filesystem/network I/O, rendering or wall-clock
reads. Randomness is explicit, seeded and persisted; unordered iteration cannot
resolve game outcomes. This is an ownership rule, not a ban on all external
libraries: simulation uses Serde for deterministic records. Clients cannot depend
on world or simulation. The [architecture checker](../../scripts/check_architecture.py)
checks all internal dependency kinds; server diagnostic development edges do not
permit runtime/build dependencies on clients. External libraries still need review.
Thread-local work counters read no clock, enter no saves and never influence rules.

Protocol, authoring, compiled content, simulation, journal and save types are
independent. Exhaustive typed adapters connect them. A wire encoding change must
not silently change a save schema; a package schema is not a wizard command parser.
There is no historical format importer or alternate rules implementation. See
[contribution policy](../../CONTRIBUTING.md) for coordinated changes and approval.

The [scenario compilation boundary](scenario-compilation.md) separates authoring,
canonical content inputs, lazy world descriptions and mutable save state.

## Authority, intentions and publication

The server's wire adapter preserves unresolved typed request facts. Decoding does
not authorize an operation, resolve a live target or mutate a game. Session
permission checks and durable receipt lookup precede fresh actor context, branch
and revision validation. Observer-scoped opaque entity references derive from a
saved privacy salt, observer and entity kind. Retry comparison can reconstruct
them without consulting whether the target still exists.

Gameplay has two explicit publication boundaries:

1. Validate a fresh request and prepare an intention in private candidate state.
   Admit its journal record and required content to persistence, then publish
   acceptance and queue state.
2. Simulation selects due work, revalidates live conditions and executes it in a
   private candidate. Reconcile scheduling, regions and observations; admit the
   execution record and content, then publish authoritative effects in order.

Acceptance means queued, not completed. Readiness comes from the simulation
stream. Queries, annotations and session metadata have their own immediate paths.
Human, AI and travel work use the same simulation-owned queue; future scripts must
use the same boundary rather than inventing another action executor. AI admission
chooses no action and advances neither time nor RNG; execution decides once.
Private AI/travel records have no authenticated RPC receipt. Retrying or reopening
queued work preserves the original intention identity.

Journal admission normally does not wait for a disk flush. Explicit save and other
durability barriers do. Never describe in-memory publication as durable commit.
See [simulation and scheduling](simulation.md) and
[persistence](persistence.md) for rollback, replay and recovery contracts.

## Content and creature rules

The server resolves shared general-rule imports and identity-keyed overrides,
validates the complete result and pins authored, effective and dependency hashes.
Simulation receives immutable shared rules and catalog content, not source paths
or an import loader. Restore uses the saved resolution rather than changed rules.
Uncopied authored inputs resolve from immutable compiled artifacts; original
source files are not a recovery dependency. Pure procedural descriptors synthesize
only a requested bounded structural neighborhood and generation unit. Named
logical ports compose authored maps with generated families before coordinates
exist; the simulation still owns traversal authority and mutable region state.

Actors own content references, advancement choices and mutable state. One ordered
modifier fold derives equipment, talents, conditions and protection. Abilities,
including basic melee, use owned plans captured when execution starts timed
preparation, not when the action enters the queue. Preparation freezes check/effect
operands and costs; live completion checks remain explicit.
Pure resolution, execution orchestration and actor-local lifecycle application
are distinct. Items and physics share effect/damage kernels; diagnostics observe
the same outcome rather than rerunning rules. Tagged adventure/arena policies own
run limits and death policy, not RNG. See [creature rules](../reference/creature-rules.md).

Content additions built from existing mechanisms are data changes. New Rust should
introduce a reusable mechanism, not a special case for one spell or creature.
[Rules review principles](simulation.md#rules-design-and-review) make this
boundary concrete. Runtime scripting remains [deferred design](../plans/scripting.md).

## Geometry, perception and knowledge

Regions are bounded rectangular 3D volumes with integer local coordinates and
five-foot cells. Finite stone volumes contain carved interiors, floors and
ceilings; unallocated space is neither empty terrain nor implicit stone. Storage
regions are not player-facing rooms. Portals use translation and the 24 proper
cube rotations, including vertical apertures. The world need not have a consistent
global embedding. Stairs are abstract traversal links, not physical apertures;
doors are independent entities, not portal topology.

Rigid bodies retain a body frame across joins. Region-local gravity and sparse
overrides act through deterministic fixed-point motion, support and collision
rules. Backend spatial indexes use portal-resolved region-local locations and
body cells, not assumed global distance. Mutation is centralized and indexes are
rebuilt from authoritative state, never serialized to clients.
See [geometry and physics](geometry.md).

The backend owns sight, appearance and hidden facts. Three-dimensional eye-based
sight traverses physical apertures and discloses relative occurrences, opaque cell
keys and visible solids. One entity may have several projected occurrences.
Stair landing awareness is distinct from physical rays. Exact sound propagation
remains undecided. See [perception](perception.md).

A place hint is an optional authored point, not room membership, a boundary or an
exploration claim. Names and remembered navigation belong to characters and are
learned only at authoritative perception boundaries. Clients may organize places
from disclosed geometry and retain stale last-seen cells; neither presentation
caches nor names reconstruct undisclosed topology. Actor-owned last-seen map
appearances persist with navigation and names. Same-branch snapshots retain the
bounded client cache; branch resets clear it before the server rehydrates the
selected branch's nearby explored cells. See [movement](../reference/movement.md)
and [client architecture](clients.md).

## Observation and client boundary

The server builds an actor-specific observation at a committed boundary and reuses
it for revision comparison, navigation refresh and publication. Immutable shared
views may be reused only for the same actor and branch. Each reader retains its
own cursor, control state, reset context and history audience. Private annotations
remain independently filtered by actor/user. Region transitions invalidate stale
candidate views; caches are absent from saves and rewind boundaries.

Snapshots and ordered deltas never serialize the world. Entity names, affordances,
errors, assets and semantic events must not disclose hidden facts. Current health
and preparation detail are personal; visible enemies disclose qualitative injury.
The client validates a complete candidate before replacing its current view.
Intermediate observations still refresh memory even when presentation skips ahead.

Both frontends can observe an actor; one connection controls it at a time. Control
transfer is explicit and atomic. Server-granted spectator credentials restrict
mutation; a player's `--observe` startup preference is not that security boundary.
Wizard authority also has its own credential and does not widen normal disclosure.

Clients may animate, summarize or fast-forward already committed progress. Window
size, redraw rate and network delay never advance game time. Slow output is bounded
and requires resynchronization rather than silently dropping state. Host output
limits, retained-state limits, deadlines and exact recovery envelopes belong in the
[protocol reference](../reference/protocol.md); they are not simulation tuning constants.

Asset palettes forecast possible top-level theme assets without revealing
instances. Their revision stream is independent of actor observations on the same
connection; clients resolve dependencies and fallbacks. Palette caches rebuild
after load and are not authoritative gameplay state.

## Travel, items and language

Travel routes use actor-known connections. Session owns routing, control and
hazard policy; simulation owns saved movement context, effects and admitted steps.
Only committed execution advances a journey. Threats, damage, obstruction,
displacement and control loss can stop it; unseen terrain cannot guide routing.
Restored pending steps are settled before control acquisition, and journey jobs
never resume automatically after restart. An explicit save does not cancel travel.

Text resolves disclosed nouns into structured opaque targets, asking about
ambiguity before spending time. The backend still validates advertised actions
against current state. Items support quantities, identity knowledge, timed equipment
and consumables through the shared rule system. Capacity, containers, locks/keys
and broader use/identification mechanics remain future work; an appearance class
alone does not implement food, wands or spellbooks. See [items](../reference/items.md).

## Persistence, packages and development authority

The storage layer owns a journal lease shared with its worker. Final ownership
release is explicit, including inherited file-descriptor cases; shutdown joins the
worker before returning. Framed records add versioning, checksums and identity to
SQLite transactions. Save types remain independent of wire serializers. Strict
restore uses the same validating constructors as live rules.

Checkpoints restore the current game and retained rewind boundaries with shared
immutable data; derived indexes rebuild afterward. History and receipts remain
available according to the persistence contract. Identity allocation, privacy,
RNG, preparation, velocity and knowledge must recover coherently. See
[persistence](persistence.md) for exact stored state and limits.

Scenario compilation owns TOML, provenance, validation certificates and source
pinning. Diagnostics acquire source only on failure; unavailable or stale text
never replaces the semantic error with guessed coordinates. Certificates describe
bounded checks and sampled generated seeds, not winnability or trust. Independent
regions and recipe-backed groups are implemented. Group publication is atomic,
but member residency follows ordinary horizons and pins. Frozen regions accrue
no active time; reactivation does not simulate a catch-up interval. See
[packages](../reference/scenario-packages.md), [generation](../reference/generation.md) and
[region lifecycle](streaming.md).

Wizard mode permanently marks the entire lineage before privileged mutation can
execute. Disabling it, copying a save or rewinding cannot restore normal-game
status. Scenario validation status is separate: a structural edit may invalidate
it, while enabling authority alone does not. Public history remains sanitized;
private structured operations support exact replay. Source packages are never
silently rewritten. See [wizard tools](../development/wizard.md).

Performance claims need measured boundaries, not just an architectural change.
Queued acknowledgement, completed action, durable flush and native presentation
are different intervals. The [performance guide](../development/performance.md) owns
open tails and evidence; future scope belongs in [remaining requirements](../plans/requirements.md).
