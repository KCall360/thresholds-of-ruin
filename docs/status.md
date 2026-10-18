# Current capabilities and next work

This page owns current scope, sequencing and supported format identities.
References describe exact behavior; [future requirements](plans/requirements.md)
record accepted work that is not yet complete. “Implemented” describes code in
this tree, not a promise that every related product or performance goal is closed.
Every change still requires the [testing policy](development/testing.md).

**Current formats:** protocol **49**, save format **47**, scenario format **20**, ruleset
**`interactions-v42`**, scenario validator **`tor-scenario-32`**. Unsupported
protocol/save/rules identities are rejected rather than migrated. Other current
guides refer here instead of copying these identities.

The independent compiled inputs use RulesInput **15**, ContentPackage **17** and
RegionInput **5**. Physical TORB/TORJ/TORR framing stays at **6** and TORC at **1**;
rooms **2**, grid-rooms **1** and world-family **1** are unchanged. The independent
arena run report is **`tor-arena-run-v5`**; its enclosing batch/matrix/search
formats remain unchanged. Current payloads require explicit nullable rider
conditions, bounded sequence-hit/attempt progress, actor nutrition, condition
semantics, exact periodic-regeneration nutrition rates/fractions and personal
status alongside invocation and reply-context shapes, source-owned form choices,
fall provenance, effective locomotion operands, learned flight requirements,
linked-hold identities, frozen Escape plans, exact private Move-to-Escape
admission bindings, durable floor-activation records,
explicit cell illumination and shared creature/corpse appearance. Current payloads
also retain explicit item magical/currency capability, checked quantity ranges and
frozen theft/relocation plans. Prior payload identities remain rejection-only.
The theft/gold change leaves protocol, RegionInput and physical framing unchanged.

Characters retain last-seen terrain, doors, stairs and item appearances in the
saved game. Returning to an explored neighborhood or opening a fresh client
hydrates a bounded complete map window. Trusted movement retains certified
coverage and requests only uncovered face regions; uncertain alignment falls
back to a full bounded refresh. Unseen changes and creature occupants are not
refreshed or invented. See [remembered maps](play/ascii.md#remembered-map)
and [the read-only transfer](reference/protocol.md#explored-map-pages). This
current-format change does not reconstruct visual history absent from older
saves; retain an older save with its matching build when finishing that run.
The measured map-compaction change compacts exact appearances and plain
neighbor evidence, and pools equal owner directories across snapshots; its
navigation encoding and retained workload were unchanged. The current locomotion
batch adds explicit learned flight requirements, covered by focused persistence
and capability-loss tests. The queue fixture has an
approved 2 MiB logical budget, with its 1 MiB compressed budget and exploration's
16 MiB logical budget unchanged. The corrected production writer passes both
capacity/reopen tests: 1,541,092 logical bytes for the queue fixture and 5,247,096
for 256 explored regions. The [capacity evidence](development/performance.md#explored-map-checkpoint-capacity)
separates those results from failed-first/model sizes and single correctness-run
timings. The 256-region reopen took 3.06 seconds including three replayed records;
passing capacity checks does not establish general responsiveness or release
performance parity.

Wizard tools can list and spawn named scenario-authored creature recipes with
explicit population context, destination/explicit depth, shared exact-HD
construction and ordinary finite birth stock. Standalone archetypes disclose their
external control and lack of population stock. Discovery stays private and
read-only; invalid/ambiguous/beyond-capacity requests leave state unchanged.
See [named creature spawning](development/wizard.md#named-creature-spawning).

Actor population choices can carry zero or one finite ordinary stack at birth
from explicit source-owned pools. Independent deterministic decisions, complete
entry rollback, nested historical receipt audits and current-only pinned save
adapters implement the mechanism. Inventory and death drops use ordinary item state;
AI tactics follow species defaults and explicit profile overrides rather than
inventory ownership. Rogue selects a provisional Vampire
ration/bandage policy with focused activation coverage; earned survival,
deep-scale storage and numerical balance remain open. See
[birth-stock authoring](reference/scenario-packages.md#first-entry-population).

Checkpoint population receipt validation binds historical identities to pinned
authored reservations and the selected starting kit, including after abandoned
branch boundaries expire; legitimate removed population entities remain valid.
Equal complete population state is pooled across retained checkpoint boundaries,
with boundary-specific tick and identity validation preserved. The bounded
[size regression](development/performance.md#population-checkpoint-size-regression)
fits the existing codec limit; this does not resolve the documented long
journal-only restart timeout. See [persistence](architecture/persistence.md).

Opt-in private survival-flow diagnostics retain exact gross committed settlement,
resource payments, unpaid holds and consumed units without changing gameplay,
ordinary disclosure or saved authority. Measurement requires a loss-free final
save barrier bound to independent durable history; stock acquisition/drop/transfer
flows remain unavailable. See [measurement limits](development/performance.md#private-committed-survival-flows).

## Implemented scope

| Area | Current capability | Authoritative detail |
| --- | --- | --- |
| Default adventure | Authored five-chamber retrieval/escape dungeon, persistent death, shared text and ASCII play | [Gameplay](play/gameplay.md) |
| Rules and actors | Shared general rules imported by scenarios, validated overrides, pinned resolved catalogs, tracked creature builds and advancement, generic modifiers/conditions/abilities, deterministic 3d6 checks, permanent requirement predicates, deduplicated edge families, preparation-owned one-use tokens, active-time Health recovery, finite supply costs and cooldowns, universal Bind Wounds, ordered strikes and explicit-branch riders, anatomy equipment, timed consumption and explicitly authored nutrition/recovery policies | [Creature rules](reference/creature-rules.md), [items](reference/items.md) |
| Actions | Simulation-owned admission/execution, deterministic scheduling, paid preparation/resume, self-or-ally targeting, immutable strike plans with live execution validation, atomic cancel-and-step with retained commitment, player/AI/travel lifecycle recovery | [Simulation](architecture/simulation.md) |
| World | Region-local 3D geometry, all 24 portal rotations, multi-cell bodies, gravity, source-derived flight/stationary/erratic movement, finite material volumes, cardinal/diagonal movement, doors and paired stairs | [Geometry](architecture/geometry.md), [movement](reference/movement.md) |
| Perception | Range-16 illuminated eye-based 3D sight, body-local awareness, opaque disclosed targets, stale client memory and durable place knowledge | [Perception](architecture/perception.md), [clients](architecture/clients.md) |
| Sessions and storage | Loopback authenticated WebSockets, control transfer and spectators, background journaling, explicit durability barriers, checkpoints, replay and bounded wizard rewind | [Protocol](reference/protocol.md), [persistence](architecture/persistence.md) |
| Region lifecycle | Lazy authored/generated content, active/frozen/detached regions, deterministic preparation, independent persistence and asset palettes | [Streaming](architecture/streaming.md) |
| Authoring | Strict modular TOML packages, one compiler, source provenance, immutable compiled bundles, shared rules imports/overrides | [Packages](reference/scenario-packages.md) |
| Procedural worlds | Compact seeded finite/open grid-chain descriptors, random stable ports, authored graph composition, bounded declared-depth character starts, lazy materialization and bounded validation | [Packages](reference/scenario-packages.md), [streaming](architecture/streaming.md) |
| Rogue starter slice | Persistent generated floors, explicit Warrior/Mage first purchases and fixed Mage forms, equipped starting kit, twenty-four-species depth-selected population, finite care and Centaur/Yeti/Troll/Medusa/Xeroc/Vampire/Griffin/Jabberwock birth stock and finite Leprechaun gold, selected-character first-entry population, unique Amulet relocation, intentional surface escape and NPC stair policy | [Rogue play](play/rogue.md), [generation](reference/generation.md), [authoring](reference/scenario-packages.md#first-entry-population) |
| Tools | JSON-lines headless client with opt-in omission of diagnostic map memory, privileged inspection/setup/rewind, bounded context-bound strike detail pages, shared-kernel arena evaluation, matrices and bounded search campaigns | [Headless](reference/headless.md), [wizard](development/wizard.md), [arena](development/arena.md) |

Development supports [fast local Rust feedback](development/testing.md#fast-local-rust-feedback)
for single-target iteration and [Python-authored catalog contracts](development/service-contracts.md)
against the production Service. Focused feedback does not qualify quick, push,
full or CI, and Service contracts do not replace unique real transport, client
or persistence witnesses. The conservative
[prose-only documentation path](development/testing.md#prose-only-documentation-changes)
runs documentation/link and tooling checks; executable documentation inputs and
every non-prose change retain full Windows/Linux debug/release CI.

Creature state now has a shared permanent compilation beneath its effective
sheet. Equipment, conditions and restorable losses reuse exact per-HD Health
provenance and a checked sum without rescanning advancement. All raw seeds and
choices remain authoritative and survive save/restore. This source-stage cache
boundary has focused regression coverage; integrated runtime and performance
qualification remain required. See [derivation](reference/creature-rules.md#derivation-and-sources).
Ordinary character sheets now project a strict total/per-source HD summary from
that compilation. Detailed inspection retains exact ordered records, while
ordinary observations and clients visit only distinct sources. Counts/owners and
aggregate Health, injury, points, threat and XP prices use checked 64-bit domains;
wire values are canonical decimal strings. Exact analytical XP projection does
not loop through untrusted saved tiers. The current 256 actual-HD admission
and inspection-row guards remain, as do bounded per-HD operands, ranks,
attributes, resources and purchase indices. Generic population recipes can append
exact racial rows after an authored depth threshold, with a recoverable atomic
capacity refusal instead of truncated actors. This intermediate policy preserves
base class rows and exact Health/restoration provenance. The 256-HD limit is
accepted for the current functional Rogue scope. Lossless chunked persistence,
detailed disclosure paging and guard removal are deferred; this scalar
prerequisite does not establish support for more than 256 actual HD.

Rogue roster study tools retain their eighteen-foe exact-kit Warrior/Mage selection
with explicit per-arm identities, pinned inputs and a disclosed generic AI proxy.
They have not been expanded or qualified for L/N/O theft/gold or M/P/X perception additions. The
bundled package selects the canonical K/E/B/S/H/I/R/Z/O/C/L/Q/Y/N/F/T/W/P/X/U/M/V/G/J policy:
one four-attempt actor stage with authored depth bands, exact `racial_after`
depth-26 growth, unchanged finite bandage/ration stages and an appended ordinary
restore-strength potion stage, followed by an independent per-room gold stage.
Earlier source-qualified checks cover their specific population contracts;
the theft/gold batch still requires qualification as described below.
Centaur is eligible at depths 7–16, Yeti at 11–20; they keep four actual Warrior/racial HD respectively,
passive provocation and three/two purchased natural strikes. Their 15%/30%
birth chances share V's provisional equal ration/bandage pool; Y retains cold immunity.
Rattlesnake is eligible at depths 3–12 with two actual racial HD, owned VN1 on
HD 1 and one unspent point. Its 1d6 natural bite adds a separately Physical-checked
poison Strength−1 rider. Ordinary concealed restore-strength potions remove only
recorded Strength loss; one finite placement attempt per floor is provisional.
Wraith is eligible at depths 14–23 with five actual undead racial rows and zero
carry chance. Parentless DR2 is paid at HD1; four points remain unspent. Its
passive 1d6 contact adds a separate Spiritual-checked latest-HD drain. Ordinary
kill-earned regrowth preserves the ordinal Health seed and leaves a ChooseAgain
purchase slot empty. The controlled authored-HD2 Service composition requires
ordinary attributed kills earning 160→195→215 XP, real W drain followed by
external-controller Wait, one 200 debit/15 held, source-free journal reopen and
ordinary Lore repurchase. The autonomous process owner instead ends at the first
survived 160-XP/HD1 drain and requires source-free checkpoint reopen. Both cases
have passed, as has the production depth-14 W birth/owned-row/zero-stock receipt
and source-free checkpoint case. Autonomous full recovery, starter survival and
an earned route remain open.
No recovery potion or new engine mechanic is introduced. Fatal loss and preserved
injury remain governed by the shared HD rules.
Jabberwock enters at depth 21 with fifteen actual aberration racial rows,
Second Strike on HD 2 and Rending Pair on HD 8. The shared purchased signature
imports its natural 2d12/2d4 sequence/timing and adds one conditional keen 1d6
rider after two qualifying primary hits in the same preparation. Its passive
acquisition, exact owned rows and 70% finite birth stock use ordinary content;
no flight or regeneration is granted. Damage categories and sheet/body operands
are provisional TOR tuning, with no survival or balance claim.
J's production depth-21 receipt/restart and controlled authored-HD16
signature/stock/death/restart cases still require actual-process qualification.
The latter is advanced controlled acceptance, not starter survival or earned
depth-21 reachability.
Troll is eligible at depths 13–22 with six actual racial HD: RG1 on HD 1,
Second Strike on HD 2 and Third Strike on HD 4, other points unspent. Its mean
acquisition and separate 1d8/1d8/2d6 strikes reuse shared behavior/combat; its 50%
carry chance uses the same provisional pool. RG1 halves ordinary Health recovery;
the regenerative flag is eligibility only. Vampire content uses purchased
Spiritual-checked maximum-Health drain and ordinary halved-period
recovery; its provisional birth-stock pool contains one ration or bandage. The
population's depth-27+ tail selects V/J. J reaches 256 HD at depth 267 and 257
at 268; any sampled over-cap recipe refuses the whole entry atomically. All-V
draws can remain valid through 274. No choice is clamped or replaced, and this
operational capacity boundary is not a terminal floor. No-contact, failed and
censored outcomes remain explicit; prepared inputs do not establish combat survival or
balance. Four-U, four-V, four-G and four-J draws remain possible. The frozen
[earned-route pilot](development/rogue-earned-route.md) keeps its independent E/S
arm named `production`; it is not relabeled or regenerated from the selected
population.
No pilot outcome or numerical qualification is implied by activation. Earned
growth routes and coupled food/recovery measurements retain their separate
acceptance requirements. See
[roster study inputs](development/arena.md#rogue-roster-study-and-bounded-candidate-inputs).

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
the same replacement kernel. The default grants one shared talent-or-skill point per HD, alongside the two
authored starting ranks.
The simulation supports exact ordinary purchase operations and timed,
ration-consuming replacements. Strict owner query/action/page wire types are
defined. Owner-authorized host queries/actions, independent save adapters,
private history filtering, durable receipts/replay and atomic Store rollback are
implemented. Text, headless and ASCII clients share exact preview/confirmation
and bounded private pages, with ordinary paid replacement, cancellation and exact
resume. The [earned-XP foundation](reference/creature-rules.md#earned-xp-and-future-hit-dice)
adds direct fatal-effect awards, explicit future-source controls, immediate HD
growth and paid regrowth after drain. Authored restoration ownership conflicts
and current numeric fold obstructions retain XP for exact retry; operational
capacity and invariant failures reject the complete growth candidate. The
256-HD guard, exact ledgers and current formats remain unchanged. Directly lethal push/fall attribution uses
one persisted causal episode and the same atomic death/award boundary, including
journal and checkpoint recovery. Poison ticks still require their shared adapter.
Source, timing and XP values remain provisional; complete Warrior/Mage HD 1–6
play remains a separate content and acceptance requirement. See
[creature rules](reference/creature-rules.md#advancement-points-and-starting-ranks).

Shared restorable attribute and maximum-Health deficits use ordinary fallible
effects in abilities, riders and consumables. Fixed creature-owned aggregates
survive rebuilds, saved restart and paid preparation; restoration preserves injury
and never revives. Personal observations and shared Text/ASCII stats disclose
these losses without exposing opponent state. Ordinary latest-HD drain removes
the actual latest ordinal and its purchases; draining the last HD reaches normal
death. Restoration is limited to previously applied growth and preserves the
original seeded Health and retained XP progress. Owner views disclose a missing
restoration source without inventing one. Production restoration loot remains
separate work. See
[restorable losses](reference/creature-rules.md#restorable-attribute-and-maximum-health-losses).

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
Rogue explicitly enrolls Warrior/Mage, keeps its twenty-four selected foe recipes
exempt and shares its finite ration between eating and respec. Authored bounded empty-depletion triggers
now use ordinary shared status effects and indexed actor deadlines; personal
condition facts and events reach ordinary Text/ASCII views. Status restrictions,
paid-work pauses and follow-up protection use the same typed condition lifecycle.
Production Ice Monster Frozen and Venus flytrap linked-Held encounters now
use shared rules. Final Rogue constants and survival balance remain open. See
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

Typed creature behavior is a new reconstruction of the agreed mechanism, not
an exact restoration of the unavailable implementation. Species own acquisition,
idle, door, care, equipment, loot and flee defaults alongside memory policy;
actor-owned engagement and deterministic RNG survive controller changes. Explicit
AI enrollment can use those defaults without a named profile. Profiles partially
override supplied tactical fields, and omitted species behavior falls back to
passive exploration without tools. Population stages with no `ai` remain externally
controlled. Source-owned interaction semantics license provocation only
from a committed strike and ordinary disclosed evidence. Non-hostile grants
reject current modifier-induced harm, and frozen admission checks captured effects
against source-owned intent. Idle actions use only the actor's senses and known
routes/items, with bounded useful equipment and supply collection. Explicit Wait
blocks all idle movement, including loot detours, while permitting independently
enabled useful underfoot actions; acquisition, pursuit and combat stay separate.
The bundled packages author provisional species distinctions, including Wait-idle
ice monsters and stone constructs. This does not activate the full Rogue roster,
add a food economy or establish balance; see
[behavior and provocation](reference/creature-rules.md#creature-behavior-and-provocation)
and [authored idle policy](reference/scenario-packages.md#behavior-defaults-and-ai-overrides).

Shared locomotion now derives stationary, flying and explicitly bounded erratic
capabilities from active sources. Genuine flight permits paid unsupported and
open-vertical movement with ordinary multicell/portal clearance and NPC stair
policy, preserves imposed velocity and resumes gravity on capability loss.
Learned flight-only routes retain their disclosed requirements through persistence.
B/K/F/G use the shared locomotion sources; F's shared Clutch and
Tightening Hold use exact live relationships, while ordinary Escape has paid
preparation/recovery and holder-specific protection. Focused unit, Service,
recipe-combat and nongraphical actual-process witnesses cover these mechanics,
including source-free recovery of paid Escape work. Native input and the full
cross-platform matrix remain merge requirements; these witnesses do not establish
numerical balance. See
[locomotion](reference/creature-rules.md#locomotion-capabilities) and
[the production roster](play/rogue.md#flight-and-linked-holds).

Shared theft transfers ordinary inventory through checked Cognitive riders.
TR1 takes one source-sized gold roll, capped by all free carried gold; TR2 takes
one free unequipped magical unit without granting identification. TR3 freezes its
compiled flag at preparation and permits same-floor relocation only after an
actual transfer. Held units remain protected, all effects publish atomically,
and stolen stock remains recoverable through ordinary death drops. Gold is an
ordinary stack capability, not a separate purse. Quantity expressions resolve to
checked positive u64 ranges using the relevant birth or preparation depth, with
no guessed depth in nongenerated contexts. See [theft and gold](reference/items.md#theft-and-gold).

Rogue now selects L/N/O and M/P/X alongside the earlier eighteen foes, for 24
production species and two remaining. L uses three fey HD, TR1(1), TR3(2), natural 1d1,
depths 6–15 and finite birth gold. N uses three fey HD, TR2(1), TR3(2), zero
natural damage and depths 10–19. O uses one humanoid Warrior HD, W02(1), natural
1d8 and depths 4–13. Gold-interested AI prioritizes self-preservation and
retaliation before known reachable gold, then ordinary idle behavior. The
appended room stage makes a 50% gold decision for each present room, without
reordering existing stages or their streams. Restore-strength potions and the
Amulet are explicitly magical; ordinary ring mail is not. The unique objective
helper and its relocation policy are unchanged. This batch adds no UI or 3D
feature. Focused simulation, strict current-save, load-only source-free recovery,
catalog integration, Service and ordinary-process witnesses cover these contracts.
The process fixture uses a labeled durable victim, harmless natural provocation
and strengthened equipped recovery damage; production L/N/O builds and AI are
unchanged. This proves ownership, disclosure and recoverability, not pursuit or
survival balance. Complete Windows/Linux debug/release CI remains required before
merge.

The M/P/X cohort adds Medusa's eight monstrous-humanoid/reptilian
HD with GZ1(1), NA1(2), NA2(4), Presence-bound Spellcasting and 3d4/3d4/2d5
natural strikes; Phantom's eight undead/incorporeal HD with typed invisibility,
1/5 erratic movement and 4d4 natural damage; and Xeroc's seven aberration/
shapechanger HD with MI1(1), MI2(6), one mundane mace guise and 4d4 natural
damage. M remains eligible at every depth from 18 through the pinned selector's
upper-boundary redistribution, with shared exact racial growth beyond 26.
P/X bands are 15–24 and 16–25; M/P/X birth chances are 40%/0%/30%.
The subtype labels grant no immunity, wall passing or extra shapechanging.
See [perception cohort](play/rogue.md#gaze-invisibility-and-object-guise).
Focused simulation, encoded Service, source-free persistence and bounded
ordinary-process witnesses pass for these contracts. The five-arm process
witness retains production M/P/X builds and real checks, with explicitly
synthetic room geometry, external foe control and a durable living observer;
it establishes mechanisms, not autonomous pursuit or survival balance.
Current scenario certificates, save fixtures and actual wire recordings are
regenerated, including the separate 3D menagerie certificate. These focused
checks do not replace complete Windows/Linux debug/release CI before merge.

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
- The Rogue starter slice selects twenty-four-species depth-selected population,
  finite starting/floor supplies and Centaur/Yeti/Troll/Medusa/Xeroc/Vampire/Griffin/Jabberwock birth stock and finite Leprechaun gold, alongside
  authored hunger/fainting/starvation and ration eating. The full 26-species roster,
  calibrated food pressure, complete item catalog, traps, wandering population,
  calibrated depth curve and complete run balancing remain unfinished. The Amulet
  relocates on each eligible player entry unless carried; intentional surface Up
  while carrying it is the persistent escape condition.
  Rations share one finite stock between eating and respec. Numerical calibration
  remains open; focused activation coverage does not replace the repository
  verification and exact-head cross-platform CI gates.
  A successful seed is not balance evidence. The explicit operational depth limit
  is not a gameplay endpoint; persistent floors and paired returns deliberately
  differ from Rogue's floor lifecycle.
- Recipe generation supports the declared stages, not an arbitrary scripting
  language. Runtime scripting and its language remain undecided.
- Shared rule/effect infrastructure does not imply complete survival, carrying
  capacity, curses, enchantment, rust, identification, ranged projectile/ray,
  secret-search or hallucination gameplay. See the current references and
  [remaining requirements](plans/requirements.md) for their boundaries.
- Rogue actor/player/item appearances use existing asset fields and structural
  palette forecasts. Text and ASCII share a minimal keyed English vocabulary;
  ASCII map/look/inventory redraw on palette changes and recover safely.
  Shared creature seeds and captured corpse appearance support the optional,
  independently maintained [3D prototype](../crates/client-3d/README.md).
  The prototype adds shared body/fitting dimensions, correlated build silhouettes,
  fitted garment surfaces and family-specific support motion. See the
  [art continuation plan](plans/3d-art-continuation.md) for visual-review limits
  and the unfinished garment/equipment authoring boundary.
  Scenario overrides (including Rogue letters), general trait specifications
  and server-stored player choices remain deferred. See the
  [presentation plan](plans/presentation.md).
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

The reconstructed shared `natural_melee` source imports each invoking species'
ordered natural strikes and appends explicit ability riders through the ordinary
compiler. Full-source grant validation includes dormant rows; immutable plans and
bounded private inspection retain species/ability rider provenance. Species
activation and integrated save/scenario/content qualification remain separate.
This is new reconstruction of missing work, not an exact historical restoration.


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
species behavior; extend the selected twenty-four-species roster to all 26 species and
qualify complete pressured runs. Work beyond the accepted 256-HD guard, including
exact-row deep storage and disclosure paging, is deferred rather than a
prerequisite for functional Rogue.
The [Rogue plan](plans/rogue.md) defines the adaptation and dependency order.
Rogue species are built through the [progression plan](plans/progression.md)'s
advancement ledger, edge rules, regeneration and strike sequences; each of its
phases still needs separate approval.
Do not equate available framework types with all content being playable.

### Other scoped extensions

- The text client reads idioms, modifiers, questions, follow-up answers and
  confirmations, fights and retraces routes, and words combat from disclosed
  facts. Repeated fight/follow goals preserve inter-round stop, resync and input
  permission boundaries; combat prose avoids undisclosed source/effect claims,
  and end accounts explicitly cover this client session. See
  [playing in text](play/text.md). Still open, because they cross the
  client boundary: authored descriptions, synonyms and place prose in scenario
  packages (the client invents none), disclosing which item the objective names,
  and any optional model-backed paraphrase layer for unrecognized input.
- Implement a disclosed-view bot framework with uncertain memory and ordinary
  action/travel boundaries; bots must not inspect server world state.
- Design [script extensions](plans/scripting.md) separately from declarative
  generation, including authority, budgets and deterministic saved state.
- Reconcile suspend/resume file handling, portable export, stronger recovery,
  larger retained history, packaging and client launch workflows.
- Treat remote authentication/encrypted deployment, multiplayer input policy and
  expansion of the 3D prototype beyond its accepted scope as separate later milestones.

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
