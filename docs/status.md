# Current capabilities and next work

This page owns current scope, sequencing and supported format identities.
References describe exact behavior; [future requirements](plans/requirements.md)
record accepted work that is not yet complete. “Implemented” describes code in
this tree, not a promise that every related product or performance goal is closed.
Every change still requires the [testing policy](development/testing.md).

**Current formats:** protocol **41**, save format **36**, scenario format **12**, ruleset
**`interactions-v36`**, scenario validator **`tor-scenario-23`**. Unsupported
protocol/save/rules identities are rejected rather than migrated. Other current
guides refer here instead of copying these identities.

The independent compiled inputs use RulesInput **9**, ContentPackage **9** and
RegionInput **5**. Physical TORB/TORJ/TORR framing stays at **6** and TORC at **1**;
rooms **2**, grid-rooms **1**, world-family **1** and the existing generator and
arena envelopes are unchanged. Current payloads require explicit actor nutrition,
condition semantics and personal status alongside invocation and reply-context
shapes, source-owned form choices, fall provenance and durable floor-activation
records. Prior payload identities remain rejection-only.

Checkpoint population receipt validation binds historical identities to pinned
authored reservations and the selected starting kit, including after abandoned
branch boundaries expire; legitimate removed population entities remain valid.
Equal complete population state is pooled across retained checkpoint boundaries,
with boundary-specific tick and identity validation preserved. The bounded
[size regression](development/performance.md#population-checkpoint-size-regression)
fits the existing codec limit; this does not resolve the documented long
journal-only restart timeout. See [persistence](architecture/persistence.md).

## Implemented scope

| Area | Current capability | Authoritative detail |
| --- | --- | --- |
| Default adventure | Authored five-chamber retrieval/escape dungeon, persistent death, shared text and ASCII play | [Gameplay](play/gameplay.md) |
| Rules and actors | Shared general rules imported by scenarios, validated overrides, pinned resolved catalogs, tracked creature builds and advancement, generic modifiers/conditions/abilities, deterministic 3d6 checks, permanent requirement predicates, deduplicated edge families, preparation-owned one-use tokens, active-time Health recovery, finite supply costs and cooldowns, universal Bind Wounds, ordered strikes and explicit-branch riders, anatomy equipment, timed consumption and explicitly authored nutrition/recovery policies | [Creature rules](reference/creature-rules.md), [items](reference/items.md) |
| Actions | Simulation-owned admission/execution, deterministic scheduling, paid preparation/resume, self-or-ally targeting, immutable strike plans with live execution validation, atomic cancel-and-step with retained commitment, player/AI/travel lifecycle recovery | [Simulation](architecture/simulation.md) |
| World | Region-local 3D geometry, all 24 portal rotations, multi-cell bodies, gravity, finite material volumes, cardinal/diagonal movement, doors and paired stairs | [Geometry](architecture/geometry.md), [movement](reference/movement.md) |
| Perception | Range-16 illuminated eye-based 3D sight, body-local awareness, opaque disclosed targets, stale client memory and durable place knowledge | [Perception](architecture/perception.md), [clients](architecture/clients.md) |
| Sessions and storage | Loopback authenticated WebSockets, control transfer and spectators, background journaling, explicit durability barriers, checkpoints, replay and bounded wizard rewind | [Protocol](reference/protocol.md), [persistence](architecture/persistence.md) |
| Region lifecycle | Lazy authored/generated content, active/frozen/detached regions, deterministic preparation, independent persistence and asset palettes | [Streaming](architecture/streaming.md) |
| Authoring | Strict modular TOML packages, one compiler, source provenance, immutable compiled bundles, shared rules imports/overrides | [Packages](reference/scenario-packages.md) |
| Procedural worlds | Compact seeded finite/open grid-chain descriptors, random stable ports, authored graph composition, lazy materialization and bounded validation | [Packages](reference/scenario-packages.md), [streaming](architecture/streaming.md) |
| Rogue starter slice | Persistent generated floors, explicit Warrior/Mage first purchases and fixed Mage forms, equipped starting kit, ordinary emu/snake combat and finite care, selected-character first-entry population and NPC stair policy | [Rogue play](play/rogue.md), [generation](reference/generation.md), [authoring](reference/scenario-packages.md#first-entry-population) |
| Tools | JSON-lines headless client with opt-in omission of diagnostic map memory, privileged inspection/setup/rewind, bounded context-bound strike detail pages, shared-kernel arena evaluation, matrices and bounded search campaigns | [Headless](reference/headless.md), [wizard](development/wizard.md), [arena](development/arena.md) |

The architecture and creature-rules refactors are implemented and published.
They are current architecture, not active refactor plans. This does not close
all performance work or implement the remaining mechanics below.

Abilities can select negative owner-outgoing edge contributions before the
existing family/talent caps, with distinct roll roles, independent check/damage
budgets and preparation-owned token qualification. The authored Measured Cut
talent uses that shared policy; its initial values still require paired tuning;
see [edge selection](reference/creature-rules.md#checks-and-edge-allocation).

The shared advancement ledger supports rules-defined point pools and purchase
kinds, two immutable starting ranks, owned skill/talent purchases, explicit
choose-again/authored restoration, disclosed dormancy reasons and atomic
replacement previews with exact subtree refunds. Immediate wizard editing uses
the same replacement kernel. The default one-point-per-HD economy is provisional.
The simulation supports exact ordinary purchase operations and timed,
ration-consuming replacements. Strict owner query/action/page wire types are
defined. Owner-authorized host queries/actions, independent save adapters,
private history filtering, durable receipts/replay and atomic Store rollback are
implemented. Text, headless and ASCII clients share exact preview/confirmation
and bounded private pages, with ordinary paid replacement, cancellation and exact
resume. The [earned-XP foundation](reference/creature-rules.md#earned-xp-and-future-hit-dice)
adds direct fatal-effect awards, explicit future-source controls, immediate HD
growth and paid regrowth after drain. Directly lethal push/fall attribution uses
one persisted causal episode and the same atomic death/award boundary, including
journal and checkpoint recovery. Poison ticks still require their shared adapter.
Source, timing and XP values remain provisional; complete Warrior/Mage HD 1–6
play remains a separate content and acceptance requirement. See
[creature rules](reference/creature-rules.md#advancement-points-and-starting-ranks).

Finite supplies and actor-active cooldowns now share ordinary preparation,
resolution, cancellation and replay. Item-owned holds conserve physical stock;
clients disclose known supply capability and physical quantity without exposing
claims or another actor's preparations. Cooldowns start once at successful
preparation and survive cancellation, grant changes and rebuilds. Universal
Bind Wounds uses the ordinary Heal pipeline, one finite bandage and self-or-ally
touch targeting. Its configurable values remain provisional: the documented
HD 1/6/11 rest/hostile-pressure samples are scoped examples, not final balance.
See [finite costs and care](reference/creature-rules.md#finite-supplies-and-universal-care).

Autonomous actors can select narrowly recognized harmless self-condition defense
against currently sensed hostiles. Selection compares their own effective family
edge and strongest category protection, preserves healing/retreat and paid work,
and uses ordinary immutable plans and finite costs. This is a shared decision
mechanism, not an arena-balance acceptance result; see
[autonomous self-defense](reference/creature-rules.md#autonomous-self-defense).

Shared nutrition exposes independent Hungry, Weak and Empty recovery policies,
including neutral Normal recovery in every band. Hunger labels alone impose no
recovery penalty. Rogue explicitly enrolls its selected character; numerical
balance remains separate acceptance work. See
[the nutrition contract](reference/creature-rules.md#shared-nutrition-eating-and-starvation).

Shared starter mechanisms include owner-outgoing negative-edge selection,
strongest-category timed talent protection, perception-conditioned actor-active
Health recovery, and immutable source-owned form acquisition. Owner catalogs
keep form options nested under talent roots; player, authoring, inspection and
save operands retain the exact root/form pair. The authored catalog now contains
seven Warrior nodes, three independent Mage roots and two shared recovery nodes,
with six acquired forms. The former 27 production talents are retired; purpose-
authored generic test fixtures remain local. Named rank-heavy, talent-heavy and
mixed allocation recipes cover HD 1, 6 and 11, with older diagnostic bands retained.
The separate starter-study generator prepares all 27 allocation matchups and 54
class-relative comparisons with bounded paired plans, exact input fingerprints
and explicit pilot/evidence status. It validates packages without running battles.
The shared mechanisms and catalog are implemented; complete Warrior/Mage HD 1–6
play, pressured escape and numerical calibration remain open. See
[fixed acquisition forms](reference/creature-rules.md#fixed-acquisition-forms) and
[starter evaluation](development/arena.md#starter-allocation-study).

The shared nutrition foundation adds explicit policy/creature enrollment,
actor-active reserve deadlines, hunger-controlled Health recovery, exact-unit
Eat with frozen positive fixed/rolled nourishment and ordinary starvation cleanup.
Required nutrition keeps its exact policy through runtime-preserving rebuilds;
standalone cross-policy replacements reject atomically.
The current format tuple includes these nutrition operands and the shared status
clock/protection records. The shared bundled policy remains disabled, while
Rogue explicitly enrolls Warrior/Mage, keeps emus/snakes exempt and shares its
finite ration between eating and respec. Authored bounded empty-depletion triggers
now use ordinary shared status effects and indexed actor deadlines; personal
condition facts and events reach ordinary Text/ASCII views. Status restrictions,
paid-work pauses and follow-up protection use the same typed condition lifecycle.
Authored Held/Frozen encounters,
final Rogue constants and survival balance remain open. See
[nutrition](reference/creature-rules.md#shared-nutrition-eating-and-starvation).

The internal received-Health-loss timer preserves a rules-authored actor-active
quiet interval independently of healing, maximum changes and dealing damage.
Purchase and replacement plans retain exact operations and actor/build-epoch
provenance, revalidate execution, and reuse runtime-preserving rebuilds. Fresh
work cannot resume an old preparation; explicit resume retains its original
identity and remaining active work. Respec reserves one finite ration and spends
it only at atomic replacement commitment. Perceived danger, actual received
Health loss or permanent-build changes discard the attempt. The configurable
advancement timings remain provisional.
See [received-loss quiet time](reference/creature-rules.md#received-health-loss-and-quiet-time).
Public-plan observation has typed source-recipient effects,
foe-qualified outgoing tokens, immutable paid observations, committed observer
recognition and shared Text/ASCII presentation, reconciled with owner advancement.
Paid history remains immutable through earned growth and reconnect, with XP
restricted to owner-authorized advancement pages. Paid Warrior and Mage techniques
use shared one-cell movement, forced displacement, fixed acquisition forms and
ordinary client/save adapters. This implements the starter mechanisms, not phase
6's full play and balance acceptance or the complete Rogue scenario; see
[paid observations](reference/creature-rules.md#paid-public-plan-observations).

The playable Rogue package exposes those foundations through selected-character
presets and ordinary purchases. Focused actual-process coverage includes Mage
form/future-source retention, both starting kits through source-deleted journal
and checkpoint restart, Rattling Blow and Cold Lance against ordinary foes,
combat-injury Bind Wounds and its cooldown, and source-deleted first entry to an
unvisited floor. Direct fatal-push/fall recovery has its own actual-process case.
The ordinary four-foe walkthrough covers earned HD 2, spending its new point,
finite floor stock, descent/return without restocking, care, ration respec and
saved restart. Broad integrated/native verification and final Windows/Linux CI
remain required gates. This bounded coverage does not establish HD 1–6 pacing,
pressured escape or calibrated builds. See the
[playable scope and verification](play/rogue.md#scope-and-verification).

## Important limitations

- Typed voluntary one-cell steps, contact-derived forced pushes and generic
  autonomous retreat are implemented with the W04/W06 starter techniques. These
  shared mechanisms do not establish useful escape options at every tested HD
  band or complete phase 6's paired balance acceptance. See
  [one-cell translation](architecture/simulation.md#one-cell-ability-translation).

- Clients must be relaunched to reconnect. Active travel does not automatically
  resume after server restart; valid preparation recovery requires fresh input.
- The server accepts loopback connections only. Packaged distribution and automatic
  local-server launch are not provided.
- Wizard rewind retains a 128-boundary window. It is unavailable in normal play
  and enabling wizard mode permanently marks the game lineage.
- The 3d6 kernel keeps the existing initial defense/bonus values; this is not
  a claim of balanced Rogue builds. The point economy, Health recovery and the
  initial authored talent values still require content calibration.
- The Rogue starter slice has only emu/snake population and finite starting/floor
  supplies, authored hunger/fainting/starvation and ration eating. The full roster,
  calibrated food pressure, complete item catalog, traps, wandering population,
  calibrated depth curve, Amulet victory and complete run remain unfinished.
  Rations share one finite stock between eating and respec. Numerical calibration
  remains open, and publication requires full verification.
  A successful seed is not balance evidence. The explicit operational depth limit
  is not a gameplay endpoint; persistent floors and paired returns deliberately
  differ from Rogue's floor lifecycle.
- Recipe generation supports the declared stages, not an arbitrary scripting
  language. Runtime scripting and its language remain undecided.
- Shared rule/effect infrastructure does not imply complete survival, carrying
  capacity, curses, enchantment, rust, identification, ranged projectile/ray,
  secret-search or hallucination gameplay. See the current references and
  [remaining requirements](plans/requirements.md) for their boundaries.
- Asset palettes do not yet drive the ASCII glyph table. There is no 3D frontend.
- Saves can still require matching compiled bundles for unvisited authored inputs.
  Portable saved-game export/import and historical migration are not implemented.

## Next work

### Performance and scalable persistence

Performance closure remains deferred and open; it does not block all feature
milestones. Existing targets still apply to every change. Resolve save/restore
and client timing tails, explain longer multi-client workloads, and measure
remaining history/actor scaling costs. Do not call the work complete from lower
operation counts or one favorable timing sample. The
[performance guide](development/performance.md) owns targets, workload boundaries,
reproduction, evidence and remaining acceptance criteria.

### Ordered strike acceptance boundary

The shared sequence kernel, independent source/save/wire adapters and client
controls are implemented. Primary and rider checks use one deterministic pipeline;
source snapshots remain immutable across continuation and replay. Private authored
source detail uses bounded wizard-only pages rather than eager expansion of every
ability. The existing main inspection's unrelated ledger/source-row expansion is
still a separate limitation.

Sequence scheduling still visits actor/preparation candidates; zero plan recaptures
or zero per-rider Game clones do not imply constant-world execution cost. The
[performance guide](development/performance.md#ordered-strike-workload-v1) separates
ordinary-combat comparisons from head-only sequence workloads and reports actual
executed rows, loaded actor/region density and checkpoint versus journal-tail work.
Local native input acceptance requires a usable display; exact waived local methods
remain required by unwaived Windows/Linux CI. These boundaries do not close the
remaining nutrition and Rogue content milestones. Finite supply/cooldown operation counts are measured separately;
unchanged scene-build counts alone do not establish per-fragment lookup cost.

### Rogue adaptation and shared mechanics

Build on the generated-floor foundation and finite starter content rather than
recreate them. Its
ongoing performance acceptance, including persistence tails, is distinct from
whether floor generation exists. Add omitted rooms, mazes, secrets and traps as
shared support arrives. Extend shared survival, items, ranged/perception and
species behavior; extend the initial emu/snake roster and finish the objective/run.
The [Rogue plan](plans/rogue.md) defines the adaptation and dependency order.
Rogue species are built through the [progression plan](plans/progression.md)'s
advancement ledger, edge rules, regeneration and strike sequences; each of its
phases still needs separate approval.
Do not equate available framework types with all content being playable.

### Other scoped extensions

- Implement a disclosed-view bot framework with uncertain memory and ordinary
  action/travel boundaries; bots must not inspect server world state.
- Design [script extensions](plans/scripting.md) separately from declarative
  generation, including authority, budgets and deterministic saved state.
- Reconcile suspend/resume file handling, portable export, stronger recovery,
  larger retained history, packaging and client launch workflows.
- Treat remote authentication/encrypted deployment, multiplayer input policy and
  an immersive 3D client as separate later milestones.

Planning approval does not authorize starting an implementation milestone or
choosing a compatibility-breaking format. Follow [development practices](../CONTRIBUTING.md)
and obtain the maintainer's required approval.

## Updating status

Update this page with the relevant guide when scope changes. Remove fulfilled
items from plans, replace superseded behavior, and keep historical test counts,
CI gates and rollout narratives in Git history. Keep a necessary benchmark or
rejected-alternative record only with its scope, limitations and durable evidence.
The [documentation checks](../scripts/test_documentation.py) compare format
identities against code; they do not certify semantic accuracy or fresh CI.
