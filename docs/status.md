# Current capabilities and next work

This page owns current scope, sequencing and supported format identities.
References own exact mechanics; [plans](plans/README.md) distinguish unfinished
work from implemented design records. “Implemented” does not mean balanced,
fully optimized or qualified for every workload. Follow the
[testing policy](development/testing.md).

**Current formats:** protocol **66**, save format **64**, scenario format **35**,
ruleset **`interactions-v57`**, scenario validator **`tor-scenario-47`**.
Unsupported identities are rejected rather than migrated. Rules-identity
changes invalidate all saves pinned to the previous identity,
not only saves containing affected AI. Wire and physical save formats need not
change with a deterministic rules correction. Current private
creature records include explicit Health-roll policy, stable HD/purchase ownership,
body snapshots and pending-return state; authored content can register body forms.

Independent compiled inputs are RulesInput **26**, ContentPackage **32** and
RegionInput **9**. Physical TORB/TORJ/TORR framing is **6**, TORC **1**; rooms **2**,
grid-rooms **1** and world-family **1** are unchanged. The arena run report is
**`tor-arena-run-v5`**. Other guides link here instead of duplicating version lists.

## Implemented scope

| Area | Current capability | Exact owner |
| --- | --- | --- |
| Default adventure | Five-chamber retrieval/escape dungeon, persistent death, shared Text/ASCII play | [Gameplay](play/gameplay.md) |
| Clients | Text parsing/clarification, graphical ASCII with NetHack-shaped controls, headless JSON-lines automation, spectators and explicit control transfer | [Clients](architecture/clients.md), [Text](play/text.md), [ASCII](play/ascii.md), [headless](reference/headless.md) |
| Simulation | One mutation owner, admitted intentions, deterministic scheduling, private candidates, paid immutable plans, pause/resume/cancel and partial-sequence debt | [Cycle](architecture/simulation-cycle.md), [actions](architecture/simulation.md) |
| Combat | Shared 3d6/edge, source/modifier folds, ordered primary/rider effects, finite supplies/resources, cooldowns, protection, death and causal fall credit | [Combat](reference/combat.md), [creature rules](reference/creature-rules.md) |
| Advancement | Ordinary purchases/respec, fixed choices, expertise and retained Intellect learning, earned XP, body/personal ownership, drain/restoration | [Progression](reference/progression.md), [player controls](play/advancement.md) |
| World | Region-local 3D geometry, 24 portal rotations, finite volumes, multi-cell bodies, gravity, cardinal/diagonal movement, doors and paired stairs | [Geometry](architecture/geometry.md), [movement](reference/movement.md) |
| Perception and AI | Illuminated eye-based sight, awareness/contact, checked private discovery/guise recognition, remembered navigation and bounded knowledge-limited tactics | [Perception](architecture/perception.md), [AI](reference/ai.md) |
| Regions | Lazy authored/generated content, active/frozen/detached state, no frozen-time catch-up, load-only spatial preparation and first-entry activation | [Streaming](architecture/streaming.md), [spatial registry](architecture/spatial-registry.md) |
| Sessions and storage | Authenticated loopback WebSockets, ordered causal outcomes separate from demand-driven pictures, journaling, durability barriers, checkpoints/replay and bounded wizard rewind | [Wire lifecycle](reference/protocol-lifecycle.md), [protocol](reference/protocol.md), [persistence](architecture/persistence.md) |
| Authoring and generation | Strict modular TOML, canonical compiled bundles, shared rules imports/overrides, deterministic independent stages and compact lazy world families | [Packages](reference/scenario-packages.md), [generation](reference/generation.md) |
| Rogue | Warrior/Mage presets, 26-species depth selector, nutrition/fainting/starvation, finite care, persistent floors, unique Amulet relocation and intentional surface escape | [Quickstart](play/rogue.md), [mechanics](play/rogue-mechanics.md) |
| Current Rogue stock | Supplemental completed catalog items, rare treasure rooms, room omissions and dark-room maze trials, without replacing starter kits or original stage weights | [Live policy](reference/scenario-packages.md#live-rogue-content-policy) |
| Equipment and magic | Explicit primary attack socket, grip capacity, armor/corrosion, curses/enchantment, identification, thrown/projectile/ray attacks, control effects, sensing/mapping and finite portable lights | [Items](reference/items.md), [equipment](reference/equipment.md), [magic](reference/magic-items.md), [effect details](reference/consumable-effects.md) |
| Forms | Baleful Polymorph, retained personal progression, compatible carried gear, cancellation and pending return | [Polymorph](reference/polymorph.md) |
| Diagnostics | Privileged inspection/setup/rewind, exact bounded pages, shared-kernel arenas, matrices and search | [Wizard](development/wizard.md), [arena](development/arena.md) |
| Optional 3D | Independent procedural client with responsive startup, asynchronous model stand-ins, inventory/equipment/portrait, advancement preview/confirmation and combined map/journal; separately qualified from the core workspace | [3D guide](../crates/client-3d/README.md), [art continuation](plans/3d-art-continuation.md) |

Gameplay submissions bind to attachment input authority, not picture revision.
Changed control/origin/operands can invalidate input; a redraw cannot restore it.
Queries, travel and wizard operations retain their own exact context rules.
Acknowledgement, execution, visual freshness and durability are separate facts.

## Important limitations

- A working mechanism or successful seed is not a balanced run. Food pressure,
  XP/recovery economics, later-depth curves and useful earned builds need separate
  acceptance. The frozen earned-route comparator is not expanded by later loot.
- The **256 actual-HD** admission/detailed-record guard remains. Wide counts,
  aggregate Health/XP and analytical funding do not bypass exact row storage.
  Lossless paged HD storage/inspection is deferred; refusal must be atomic.
- The live dungeon has an operational **1,000,000-floor** materialization limit,
  not a designed terminal floor. Unique retrieval separately has **4,096 eligible
  entry receipts**, even while the Amulet is carried. These independent limits
  can refuse an entry; they do not grant victory or imply deep-run qualification.
- Shared grip handling is implemented; general inventory carrying/containers and
  richer item presentation remain separate work. Portable torches are supported
  catalog content but not default live stock.
- Only baleful Polymorph and the explicit emu/snake/troll destinations are current.
  Temporary forms, expiry, broader form eligibility and cooperative consent are
  future decisions, not implied by body enrollment.
- Automatic travel stops for its current actor/Health/displacement hazards; a
  complete newly discovered trap/dangerous-terrain classifier remains deferred.
- Clients require relaunch to reconnect. Travel does not silently resume after a
  restart; retained preparation requires fresh authorized input.
- Server networking is loopback-only. Installer/distribution, automatic core
  client server launch, remote authentication/encryption and multiplayer input
  policy are not implemented merely because alternate clients exist.
- Wizard rewind retains a 128-boundary window, is unavailable in normal play, and
  permanently marks the game lineage. This is not ordinary save scumming support.
- Saves retain canonical content and procedural sources, but uncopied unvisited
  authored inputs may still require their exact immutable compiled bundle.
  Portable export/import and historical-format migration are not implemented.
- Runtime scripting and language selection remain undecided. Declarative recipe
  support is not an executable script runtime.
- Performance closure remains open. Sharing and lower operation counts do not
  prove constant-world latency; history/actor scaling, persistence tails and
  presentation-held state need measured acceptance. Preserve negative evidence.
- Optional 3D/model capabilities and native visual qualification have their own
  owner. Responsive startup and nonblocking model preparation are implemented;
  the 100+ visible-creature 60 FPS target is not yet qualified. Its current
  guide/art record defines exact controls, unfinished fitting/animation,
  accessibility and richer HUD polish, and visual-review limitations.

## Next work

### Performance and scalable persistence

Measure unresolved save/restore/client tails, long histories, actor-density and
multi-client behavior against [performance targets](development/performance.md).
The [server review](development/server-performance-review.md) preserves existing
measurements; [documentation review findings](development/documentation-review.md#recommended-engineering-work)
separate source-supported questions from demonstrated runtime defects.

### Ordered strike acceptance boundary

The shared sequence kernel, adapters, ordinary controls and natural-melee riders
are implemented. Immutable captures and correct replay do not establish cheap
world-independent scheduling or complete native/content acceptance. Retain
separate ordinary-combat and sequence workloads and their actual executed rows.

### Rogue adaptation and shared mechanics

Calibrate the implemented roster, survival, stock and depth curves rather than
restarting completed feature foundations. See the [Rogue design record](plans/rogue.md)
and proposed [tuning sequence](plans/rogue-tuning.md). Broader HD storage, full-run
balance and pressured earned routes remain separate increments. Topology/recovery
witnesses are not survival evidence; graphical waivers are never passes.

### Other scoped extensions

- Authored language/description/places and optional disclosed-view bots, without
  giving clients hidden world authority.
- Portable export, recovery/distribution and future remote/multiplayer policy.
- [Scripting](plans/scripting.md), [presentation](plans/presentation.md), optional
  3D/art work and temporary form decisions, each with explicit scope and approval.

Planning approval does not authorize a new implementation milestone or a breaking
format decision. Follow [development practices](../CONTRIBUTING.md).

## Updating status

Keep this page a current scope/format/next-work map. Exact mechanisms belong to
references and architecture; proposals to plans; measurements to their evidence
owners. The prior implementation/checkpoint chronology remains in the
[post-206 tree](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/status.md)
and [merged 3D integration tree](https://github.com/KCall360/thresholds-of-ruin/blob/b758868a6c790788d2ad46597a8932eac1f954d4/docs/status.md),
not reusable qualification of changed code.
