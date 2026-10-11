# Creature implementation plan

This tracks the accepted creature/combat/arena milestone. It is a design and
completion checklist, not a claim that all listed behavior is available.

## Current implementation

The simulation rule foundation includes bounded dice and skill checks, creature
composition, source-owned grants, template priorities, advancement ownership,
twenty-seven one-time talent cards and dormant choices. Canonical damage,
protection, split-cost reservations, injury-preserving health, resource recovery
and source-specific fear state are implemented as deterministic rule modules.
Neutral combat checks retain the published d20 sequence.

An independent strict build record now preserves species/template definitions,
individual attributes, HD sources and health seeds, Mana binding and owned
advancement choices. Restoring it validates the complete build and recomputes
derived values; records do not contain combat caches. Catalog IDs are explicit,
nullable choice fields remain required, collection sizes are bounded during
decoding and duplicate set/map entries reject. Creature-backed game actors now
use this boundary in checkpoints. Scenario authoring and saved scenario sources
now require owned builds rather than flat combat profiles. Runtime combat requires an owned creature build and reconstructs its derived
cache on restore. The final coupled format audit remains open before publication.

Scenario manifests now accept a `creatures` catalog containing `species` and
`templates`, and characters, actor archetypes and inline actors accept a
`creature` build. Builds name a species, identity, faction and Mana binding;
identity labels use UTF-8 byte limits shared by authoring, spawning and checkpoint
decoding (60 bytes for names and 80 for factions). Validation checks unused
archetypes too, and exact-limit UTF-8 identities survive real-server restart.
Each build's ordered `hit_dice` entries own training, talent and attribute choices.
The compiler checks unused catalog definitions as well as selected builds,
rejects conflicting templates and ineligible initial talents, and resolves
recipes before spawning. Objectives require explicit builds for all starting
characters; they do not invent default combat attributes. Each actor receives an independent Health stream
derived from the scenario seed and actor identity, without consuming combat
randomness. The flat `combat` field is rejected; a creature build cannot also specify
standalone anatomy because its species owns anatomy. Physical body geometry remains a separate field.
Bundled combat definitions and authoring/saved-source schemas now use owned
recipes. Runtime raw-profile construction and checkpoint decoding have been removed;
final format verification remains pending. An actual-process test validates an authored package, starts the
server, checks disclosed creature stats, saves and restarts, and verifies the
same stats through the Text client without advancing time. Further process
tests execute Power Strike, Magic Bolt and Fear through headless requests,
interactive Text commands and a presented native ASCII window. They check
preparation, both cost halves, reservation settlement, recovery and unchanged
player position. The practice target uses ordinary neutral AI; an externally
controlled actor waiting for input would deliberately stop time advancement.
Further arena controls and diagnostics remain in the acceptance scope below.

The simulation now has an explicit, checkpointed arena run mode. It records
selected-character death while allowing surviving AI to continue through normal
admitted actions. Death preserves any stop condition that is already set. Setup
requires a fresh run with living, loaded creature-backed participants, and an
existing run cannot change this policy. The mode is required when decoding a
checkpoint; unknown modes and an adventure with nonterminal selected death reject.
Simulation tests cover continued combat after death and restore, unchanged
adventure termination, rejected setup and immutable run policy. This is the run
control foundation. Bounded termination now runs through the simulation's normal
action and deadline scheduling: encounters stop at the configured active tick
limit, committed-action limit, or team elimination. Defaults and maximums are
100,000 ticks and 10,000 actions. Admission does not count as execution. Stopping
clears queued work and unpaid preparation reservations while retaining charges
already paid. Checkpoints preserve limits, counters and the stop reason, and
reject inconsistent evidence. Integration tests cover exact limits, elimination,
reservation cleanup and restoration. Scenario manifests now accept arena
settings with explicit participant IDs, manual or all-AI control, and tick/action
limits. Participant lists must be unique, include the selected character, contain
at most 256 spawned creature-backed actors, and have no adventure objective.
The ordinary streaming engine loads participant regions before establishing the
run and keeps them active through persistent actor reference points. Authoring
and zero-radius streaming tests cover all-AI execution, manual input, remote
participants and invalid settings. The session explicitly permits unattended
arena AI while preserving the living-controller requirement for adventures.
A real-server acceptance test reaches its exact tick limit without player input
and restores the stopped state after restart. Additional real-server acceptance kills
the manual selected character with an ordinary consumable, verifies surviving
AI reaches the bound, and restores both death and the terminal encounter after
restart. The bundled `scenarios/mob-arena` supplies a three-HD two-team encounter
with Warrior, Mage and mixed builds; its process test executes the selected
character's paid fear ability. The eleven core creature/arena process tests pass,
and the bundled arena passes the every-package retry, replay, restart and rewind
invariants plus fresh-checkout certificate checks. Wizard arena pause/resume and
bounded committed-action stepping are durable. Granting execution again resumes
the ordinary timer loop up to the next decision, including when every participant
is waiting for a future tick; the permission change does not consume an action. Initial paused all-AI runs are
supported through `start_paused`; pause preserves queued work and paid resource
holds, and restart preserves active AI preparation without another start charge.
Simulation tests cover permission bounds, exact stepping, strict checkpoint fields
and discarded permission at encounter termination. Server tests cover private
receipts, authority, retry, checkpoint and rewind; the real-server process test
covers stepping, unchanged snapshots, restart and exactly-once finish payment.
Wizard latest-HD removal now uses the shared atomic rebuild path, including
owned choice removal, grant-loss preparation cancellation, retained start charges
and ordinary zero-HD death cleanup. It pauses active arenas and rejects mutations
after their recorded stop. Integration coverage verifies authorization, unknown
actors, empty ledgers, exact retry, restart, rewind and stopped-result immutability.
Protocol and server regressions validate zero-HD dead snapshots and deltas while
rejecting living zero-HD and zero-maximum positive-HD or legacy combat records.
The process test exercises preparation interruption, three removals, restart and
rewind through the Text client and real server. Named transformations, private
inspection and numerical diagnostics are covered below; final verification remains
required.
Named wizard template application/removal now resolves definitions from the
existing prepared catalog rather than accepting client-supplied grants or
recompiling the catalog per edit. Templates and latest-HD removal share one
arena-aware rebuild boundary. Backend tests cover cached authored definitions,
unknown/absent templates, original receipts on retry, resource capacity changes,
class grants surviving template removal, conflicting primary types, restart and
rewind. A real-server Text/headless test removes a template during its granted
Fear preparation, retains the start charge, releases the finish hold, preserves
advancement, restarts, reapplies without free resource refill, and rewinds to the
original build. A second real-process case transforms an injured humanoid to
undead: its first racial die changes d8 to d12 while its Mage die remains unchanged,
injury and inventory persist through restart, removal restores the original maximum
without healing, and rewind restores the original build and Health. The private
inspection and numerical diagnostic interfaces are described below.
Wizard advancement now appends racial/Warrior/Mage HD and applies training,
attribute increases and talent selections to explicitly numbered, one-based HD
owners. Append preserves retained seeds/choices, derives only the new ordinal from
the original actor health stream, and rejects the 256-HD bound atomically. It
uses the shared arena-aware rebuild path. Domain tests cover retained records,
training ownership, stable re-addition and the bound; parser tests cover named
catalog values, owner bounds and forged seeds. Backend tests cover authority before
target lookup, exact receipt retries, invalid/overspent/occupied choices, restart,
removal and rewind. Four real-process cases cover injured append/removal/re-addition
with restart/rewind, owned training/attribute/talent removal, non-revival after
zero-HD death, and older owned talents becoming dormant and reactivating as their
Mage requirement is lost and restored. These advancement tests do not certify
private inspection or the final compatibility migration.

The trusted simulation inspection query now reads a loaded creature in one
lookup, returning its identity, borrowed definition/choice/derived state and
current personal stats. It does not serialize simulation state or evaluate
perception. Integration tests verify unchanged paid preparation and reservations,
owned HD choices, class grant provenance, unknown targets, transformation-induced
Fear dormancy/reactivation and zero-HD persistent death. The independent privileged
report schema now carries explicit species/template operations, owned HD seeds and
choices, effective source grants and source-specific Fear durations. Protocol tests
cover exact large-integer round trips, inconsistent ownership/Health/source records,
and Fear immunity conflicts while allowing reductions and unrelated immunities.
The backend maps these records explicitly and requires wizard-enabled authority
before target lookup. Its integration test verifies unchanged paid preparation,
reservations and state, unknown targets, inspection of another actor, and exact
report reconstruction after restart. The wizard command now routes through a
read-only session query and a dedicated contextual private response. Session tests
cover role authority before target lookup, observer/target separation, unchanged
state/history, stale input/context/branch and unavailable targets. Client
connections validate both the report and recipient authority and discard query
contents while unsynchronized. Shared Text/ASCII rows show exact owned seeds,
choices, source grants and resources; ASCII provides an F7 wizard command editor
and a scrollable inspection panel cleared by resets or disconnects. Client tests
cover authority, request correlation, malformed reports, exact display and native
model scrolling/reset behavior. A dedicated recovery test verifies that stale
inspection contents are discarded before a fresh snapshot and report. Native
inspection rows are validated, formatted and wrapped once on arrival, then
borrowed by rendering; an invalid replacement leaves the displayed report intact.
Three real-process cases verify Text inspection during paid Fear preparation,
headless private source export with exact restart reconstruction, and the native
F7 editor/report using OS key and text events plus scrolling and closing. The
existing native pickup/close acceptance also passes after sharing window lookup
in the test harness. A real server recording now includes the independent private
response and all protocol samples round-trip. Full numerical combat diagnostics
remain a separate unfinished requirement.

The numerical resolver now offers an optional observer for actual check operands,
raw/kept dice, allocated and unused edge, random-state boundaries, immunity and
ordered descriptor/category reductions. Fear records immunity before any
resistance roll and distinguishes resistance from application. Normal resolution
uses a no-op observer; retaining observers explicitly copy the outcome's dice.
Owned traces are bounded to 128 steps and mark truncation rather than presenting
incomplete records as complete. Rules tests compare observed and ordinary outcomes
and random state, verify exact trace replay, retained dice lifetime, misses and
immunity short-circuiting, and bounded retention.

Trusted backend tooling can now opt into capture at Game's shared creature combat
completion boundaries. Records carry tick, actor/target, current receipt and
original payment owner, preparation/resume timing, planned split costs and the
actual applied result. Health, resource balances/reservations/recovery phase and
source-specific Fear are captured immediately before the completion payment and
after the effect, including recorded injury rather than inferring it from death.
Queued, preparing or cancelled work produces no fabricated
resolution. A window retains the latest 64 resolutions and reports discarded
records. Windows and immutable records use shared storage so Game/rewind clones
do not copy retained dice; subsequent mutation keeps forks independent.

Capture is disabled by default, excluded from checkpoints and absent from normal
observations. Re-enabling preserves an existing window; disabling discards it.
Restored games start with capture disabled. Tests compare capture with ordinary
execution for melee, Power Strike, Bolt and Fear, verify original/resumed owners,
cancellation, fork isolation and identical traces after replaying a checkpointed
preparation. All runtime combat actors now use the owned build and shared
resolution kernel.

Independent protocol DTOs now describe numerical traces and pages of up to eight
records from the retained window. A page states total captures, dropped history,
retained count and its inclusive sequence boundary, allowing older retained pages
to be exported without confusing pagination with data loss. Wide random states,
ticks, receipt identities, sequences and signed edge/total values use canonical
decimal strings. The trace validator checks governing attributes and Mana
bindings, d20 retention, homogeneous damage-pool retention, edge consumption,
random-state continuity, component order, nested protection order and aggregate
arithmetic. Fear includes the caster's attribute/rank/bonus and duration bonus,
so difficulty and duration can be checked as well as the defender's resistance.

Report validation connects the trace to action identity, timing, actual injury,
split completion payment and source-specific Fear; lethal damage distinguishes
protected damage from Health actually lost. Tests cover contradictory values,
misses, immunity, advantage/disadvantage, maximum pools, illegal ordering, fixed
component randomness, large indices, paid owners, expired pages and disabled
capture.

The backend now projects captured records explicitly through shared catalog
mappings; it does not serialize simulation state, load actors or advance time.
Trusted Engine capture controls and numerical page queries require wizard-enabled
authority before checking page availability. Queries and capture toggles leave
ordinary observations, revisions and journal history unchanged. Capture policy
lives outside rewind/save state: in-memory rewinds retain the current on/off
setting, restore the appropriate historical window and resume collection without
resurrecting disabled data. Reopened saves start with capture disabled and require
wizard authority to inspect or enable it. Backend tests exercise real seeded arena
resolutions, backwards pages, repeated queries/enablement, rejected commands,
rewind equivalence and restart isolation. Fresh intention owners after rewind do
not change numerical outcomes.

Session commands `wizard combat capture on|off` and `wizard combat inspect
[through]` now expose runtime capture and bounded pages through a dedicated private
`combat_diagnostics` reply. Account/global wizard authorization precedes decoding
and retries; stale context, branch or revision cannot read a page or toggle capture.
These requests need no control and never become journal commands. Text and ASCII
share validated rows containing actual rolls, costs, timing, damage protection,
Fear and before/after states; ASCII caches and wraps the private rows once. Headless
exports the structured response. Client tests cover role restrictions, semantic
validation, request correlation, stream repair and atomic panel replacement.

A process regression exposed a missing stop notification when an arena step budget
was exhausted. The session now respects output headroom and announces the stopped
scheduler before blocking; the regression covers exactly 32 committed actions.
Real-process acceptance verifies paid seeded resolutions, backwards paging, private
headless reports, Text numerical output, native ASCII capture presentation, unchanged
simulation state across queries and disabled/empty capture after restart. The recorded
wire fixture includes the new response and all protocol samples round-trip.
The local diagnostic export helper assembles every retained page from one unchanged
private reply context, rejects gaps/conflicting pages, preserves raw receipts and
hashes the complete window. Numerical comparison normalizes only payment-owner IDs
while preserving their presence. Actual-process acceptance exports all pages,
rewinds to the initial arena, repeats the same 32 committed actions and verifies
identical numerical hashes with different raw owner hashes. Reopening the save
still starts capture disabled. Eviction and trace truncation remain explicit;
exports cannot recover records already evicted.

The final coupled save/ruleset/validator migration
must include the required run mode, nullable arena state and pause/step fields.

Health, resources, cost reservations and fear also have validated reconstruction
boundaries. Living health must remain above injury; persisted death remains dead.
Resource balances and fractional recovery must fit freshly derived capacities
and their recovery periods, with no stored progress in full pools. Restoring
reservations validates unique IDs and funding without charging the start cost
again. Fear restores unique causers and remaining durations without refreshing.
The unified creature state reconciles those primitives during actor rebuilds.
Creature state, definitions and derived values share storage across retained
actor versions; editing one creature copies only that creature's mutable state.
Injury, pools, reservations and fear remain separate from derivation. Ordinary
damage and healing use one injury balance, zero-HD death runs existing corpse and
inventory cleanup once, and increasing maximum Health never revives a dead actor.
Game checkpoints reconstruct the combat cache from identity and creature records.

The shared melee-definition core now validates Heavy/Light Weaponry skill,
check bonus, base preparation/recovery bounds and an explicit primary damage
component. It uses the existing canonical damage kernel for mixed fixed and
rolled components. Signed melee modifiers and Power Strike bonuses combine
before zero clamping; added dice modify rolled primaries, while fixed amounts
have no dice to extend or reroll. Source definitions stay immutable. Simulation
unit/integration coverage and strict lint checks pass. Source records now retain
skill, timing, primary key, descriptors and fixed/rolled expressions. They bound
component allocation, reject unknown nested fields and reconstruct through the
same validated constructors. Round trips preserve damage results, protection and
RNG state. Species now own the full immutable natural attack, and actor source
records retain it. The immutable attack value also supports validated source
serialization and deterministic ordering for equipment persistence and stack
keys: equivalent canonical component declarations compare equally, and decoding
rejects invalid mechanics through the same bounded record constructors.
Derivation keeps permanent modifier totals alongside that source so signed fixed
penalties and Power Strike combine before clamping.
Natural planning and execution use the declared category, descriptor, components,
skill, bonus and base phases, with Speed scaling physical timing. Independent
privileged DTOs map each source field without serializing simulation records;
Text and ASCII inspection show the full bundle and primary designation. The
bundled arena and generated matrix packages use the new shape. Focused Windows
unit/integration checks cover authoring, source restoration, canonical DTO
validation, signed modifiers, edge allocation and presentation. Real-server
checks capture Light Weaponry, rolled Fire/Energy and fixed Keen resolution,
preserve the source after restart, and retain inspection privacy. Native ASCII
inspection passes. All 268 matrix packages compile and their encounters replay
exactly. Equipment now owns the same immutable bounded source definition.
Normal melee and Power Strike use the selected weapon's skill, check bonus,
components and descriptors, with creature-owned attributes, training and modifiers.
Independent attack DTOs serve both privileged natural inspection and identified
inventory disclosure. Focused tests cover Speed-adjusted preparation, checkpoint
restoration, public validation and identification boundaries, and conservative
mob weapon choices. A real-server test executes a Light Weaponry Fire/Energy
weapon with fixed Keen damage through Text, captures both normal and paid
resolution, and verifies its disclosed source after restart. The default five-chamber dungeon now declares four owned builds and a separate
starting greatsword. A four-HD Warrior supplies the player's training and talents;
the scout, Construct guardian and Fire Elemental wisp use racial builds. Source
health/protection/timing replace the four flat profiles. Engine completion and
native completion, source inspection/restart and Text combat/corpse narration
pass; the ASCII health assertion checks authoritative derived health rather than
the removed constant. All fourteen bundled flat-profile fixture declarations now use synthetic
one-HD builds. Fixed attacks and source grants keep test hit probability, health
and phase lengths controlled, and species own the fixture anatomy. Humanoid/Animal
first-HD Health has an 8-point floor; the former 4/5-point tiny targets now use
that derived floor. The cave rat remains an Animal with no equipment slots.
The all-fixture recipe regression and package validation pass. Runtime raw-profile
construction, serialization and fallback paths have been removed; full milestone,
release, CI and deployment gates still apply.

Creature-backed basic melee runs through existing queued preparation/execution
and the shared seeded check/damage kernel. Natural melee phases use Speed and
misses skip damage randomness. Bundled scenarios and fixtures now use owned
builds, and equipped weapons declare their skill and damage components. Every
runtime combat state owns its creature build; melee and AI equipment evaluation
have no flat-profile fallback.

Backend ability queries now derive melee/power-strike timing from current combat
equipment, fixed bolt/fear timing, split costs, reach and talent-modified effects.
They reject dead actors and ungranted techniques without changing actor state.
Melee plans contain canonical damage components. Power strike adds three to
the primary Impact component before damage clamps to zero, preserving its
descriptor, dice draws and edge allocation. A weapon without Impact receives
a separate fixed Impact component, subject to ordinary protection. Natural
and equipped melee combine signed modifiers before clamping consistently.
Pure bolt resolution uses permanent-binding Spellcasting and the shared seeded
check/damage bundle. Pure fear resolution uses Discipline against the caster's
Intimidation difficulty; Fear/Mind-Affecting immunity skips the resistance roll.
Its edge belongs to the defender's resistance bundle. Derivation includes the
base 300 ticks when checking the maximum fear duration, so an accepted build
cannot advertise a duration the effect state rejects. Queued power strike,
bolt and fear now use those kernels through ordinary preparation and resolution.
Fear applies or refreshes the source-specific condition on failed resistance;
resistance and immunity still pay the resolution cost. Protocol commands and
qualitative ability events now map through the server and shared client narration.
The simulation now projects the observer's own creature type/subtypes, ordered
HD sources, attributes, skill ranks, defenses, permanent Mana binding, active and
dormant talents, granted abilities and resource balances. Resource inspection
distinguishes current, maximum, available and reserved amounts without exposing
reservation identities. Personal projections omit health seeds and private grant
provenance, remain read-only, rederive after transformations and reconstruct after
checkpoint restoration. Enemy combat observations remain qualitative. Independent
wire DTOs and exhaustive server mappings now carry personal stats in full views
and combat deltas, with named catalog values and structural validation for complete
skill/resource lists, duplicate choices and inconsistent balances. Shared inspection
prose now supports the Text `stats` command in both modes and an ASCII `@` panel
with wrapping, scrolling and spectator access. Shared ability affordances now
support Text `power strike`, `magic bolt` and `fear` commands (with `powerstrike`
and `bolt` aliases) and an ASCII `z` ability/target menu. They use disclosed
personal grants and visible opaque targets, omit basic melee from the paid menu,
and never infer affordability from a snapshot. Text techniques submit one action
from the current position without implicit travel. Native selections are free,
deduplicate portal occurrences and clear on cancellation, state revision changes
or control loss. Server preparation remains responsible for grants, physical
reach and funding. Creature-backed actual-process checks now cover all three
paid techniques through Text, native ASCII and headless clients.

Creature-backed AI now chooses paid techniques through the same admitted action
path. Its default policy tries Fear at full Focus, Power Strike in melee when
funded, basic melee otherwise, and Magic Bolt outside melee reach. Healing and
retreat retain priority. The policy consults current visible targets and personal
grants/resources, never enemy defenses, immunity or condition state. An advertised
Bolt without magical capability and Mana cannot qualify. Remembered locations
support pursuit but cannot authorize casting at an unseen actor. Full work
validation excludes actors without a creature build before selection.
Interrupted funded preparation against the selected visible target takes priority
over a new paid technique, preserving its original cost owner and start payment.
AI decisions are queued without spending; selection occurs at execution and normal
preparation repeats validity checks. The anonymous synchronous AI shortcut was
removed so paid autonomous work always has an issued admission identity. Unit and
integration checks cover funding, occlusion, cancellation, checkpoint reconstruction
and interrupted resume; real-process cases observe start and resolution charges
for each technique using short player waits as explicit decision boundaries.

Focused client inspection checks cover all displayed fields, dormant choices,
reserved balances, both Text parsers and spectator-safe ASCII scrolling. Real
Text and native-window process checks verify the diagnostic fallback, presentation
and unchanged tick/state during inspection. Creature-backed process tests also
cover personal stats, save/restart, free native ability/target selection and
ordinary execution through the shared combat model. Broader transformation,
privacy and arena acceptance remain part of the final milestone verification.

Backend target queries now check current grants, living active actors and
physical reach without paying or reserving. Melee uses existing occupied-cell
reach. Bolt/fear range uses the shared Manhattan sight metric, six cells from
the caster's eye to any physically visible occupied target cell, including
rotated portal projections. Illumination and current occlusion apply. Abstract
stair landing disclosures do not create a casting path. Queries reconstruct
after checkpoints; action execution must call them again at preparation and
completion rather than retaining their results as permissions.

Resource recovery and fear expiry follow scheduled changes on active simulation
time in both fast-forward and moving physics. Relative progress stops while
frozen/detached and resumes without catch-up. A derived actor index tracks only
living creatures with resource deficits or fear. Its ordered deadlines are
maintained by actor edits and reconstructed on restore, never persisted. Timers
settle elapsed time at deadlines, collisions, lifecycle changes and decision
boundaries, rather than on every physics tick. Expiry and recovery precede checks
at the same tick, including fear expiry at an attack's resolution.

An internal settlement cursor avoids editing creature state between boundaries;
checkpoint creation requires settled state and restore initializes the cursor
from the saved tick. This cursor carries no additional saved rules or format.
A real-server streaming case expands source-owned capacities while retaining
balances and applies Fear before moving the controlled character away. Authored
long rooms keep sight and reach from pinning the subject active. Both a loaded
frozen region and a detached region preserve the subject through 4,000 global
ticks and a restart. On thaw, each resource follows its recovery period without
catch-up, saturates at capacity, and Fear resumes its retained active duration.

Resources and fear expose the interval to their next balance change or expiry;
creature state combines them into one earliest wake-up interval. These intervals
retain fractional recovery, disappear at full capacity or death, and recompute
after fear refresh or immunity. The loop now selects the earliest creature
deadline alongside actor readiness and attack completion. Frozen and detached
actors cannot wake it, and deadlines reconstruct from checkpointed relative
progress. A timer wake while every actor is preparing continues through attack
completion rather than returning without a decision. Deadline lookup takes the
first active entry in the ordered index, skipping frozen actors. Arbitrary delayed
action submission is not yet exposed; future scheduled actions must enter normal
validation, preparation and cost handling rather than bypassing those boundaries.

Preparation now retains its original admission identity separately from the
latest resume identity. A new admission can resume retained progress without
changing the future reservation owner; receipts and execution events continue
to use the latest identity. Checkpoints require the explicit nullable origin and
reject unissued, future or duplicate origins and conflicting live ownership.
Preparation also snapshots the original wind-up duration and recovery. Changes
to Speed while work is active or paused affect subsequent work; resuming and
restoring the underway work retain its phases. Checkpoints require both timing
fields, validate remaining progress against the original duration, and check
recovery bounds and overflow independently of the current combat definition.
Validating cost ownership against execution receipts remains part of ability
integration. This extends the
unpublished preparation record; the required save/ruleset migration must cover
it before publication. Preparation removal now shares one cost settlement path:
cancellation releases only the unpaid hold, while completion spends the remainder
against the original owner before applying effects. Teleport, invalid targets,
movement and replacement work use this cleanup; pause/resume preserves the hold.
Paid ability starts now reserve the full cost and snapshot their charge. Queueing
is free; affordability and current grants/reach are checked again at execution.
Paid actions require an admission identity, including resumed work. Grant loss
immediately cancels preparation and releases its hold. Replacement validation
accounts for the unpaid hold it releases in the same commit. Checkpoints bind the
one hold to its original owner and exact charge, rejecting orphan, mismatched and
unissued holds, missing charge fields and invalid fixed casting phases.
The server's private journal recognizes ability starts and resolution lifecycle
events. Explicit action/storage adapters preserve named techniques and lossless
numeric targets in private saves while wire commands use observer-scoped opaque
targets. Foreign and undisclosed tokens produce the same rejection. Disclosed
ability outcomes contain no rolls, damage amounts, immunity reasons or balances;
participants not currently seen remain absent, and uninvolved observers must see
both participants. Text/native history and narration consume the qualitative
events. Recorded protocol-32 samples include all three techniques and private
inspection/diagnostic replies. Save format 25, ruleset `interactions-v26` and
validator `tor-scenario-11` cover the coupled owned-build migration; guides and
scenario references are checked against those versions.
The ledger also exposes a read-only start gate using the same affordability,
ownership, reservation-limit and resume checks as payment. Creature validation
additionally rejects dead actors. These checks do not allocate holds or change
balances, so action preparation can validate the whole start before committing.

The historical receipt audit now includes all three paid ability preparations.
The journal adapter classifies every backend action exhaustively. A real-process
regression forces pending Fear into an actual checkpoint with no replay tail,
then verifies restart, the original reservation, and exactly-once finish payment;
server integration additionally verifies retry and rewind from that checkpoint.
Final acceptance review, performance evidence, full Windows/Linux debug/release
verification and desktop verification remain required. Earlier broad checks do not certify subsequent changes.

During iteration, run checks directly affected by each change. The maintainer
authorized reserving the full suite for PR preparation. This changes when broad
checks run; required behavior coverage and publication gates remain in scope.

## Contracts

One TOR ruleset governs players, mobs, items and environmental effects. TOR owns
simulation time, queued actions, physics, perception, authority and persistence.
Plotweaver-inspired attributes/checks, D&D-inspired composition and NetHack-style
knowledge are mechanisms within that ruleset, not alternate scenario rulesets.

Definitions, advancement choices, derived state and mutable state remain separate.
Authoring, simulation, saved DTOs and disclosed protocol DTOs are independent.
Recompute benefits from current composition and recorded choices; never reverse a
transformed build by subtracting historical numerical deltas.

## Required behavior

| ID | Requirement | Status |
| --- | --- | --- |
| C1 | Shared species, primary type, subtypes, classes, templates and source-owned grants | Shared build/actor state, authored catalogs and personal client inspection implemented |
| C2 | Core 3.5 type/subtype catalog with explicitly implemented/adapted/deferred traits | Catalog, explicit trait derivation and authored species/template validation implemented |
| C3 | Template priorities; conflicting equal-priority structural changes reject atomically | Named authored templates and shared wizard rebuilds implemented; conflict/retry/restart/rewind and grant-loss process coverage, including injury-preserving racial/class type changes |
| C4 | Six attributes, eighteen reference skills and Mana-bound Spellcasting; skill ranks 0-5 | All reference skill mappings, rank bounds and Mana bindings covered by shared rule tests; real processes cover distinct defenses, weapon skills, passive resistance and personal inspection |
| C5 | Racial/class HD ledger, fixed first-HD maximum, independent stable per-HD randomness | Stable actor HD streams, owned authored choices, checkpoints and wizard append implemented; retained seeds, stable re-addition, bound, restart and rewind covered |
| C6 | Current-type racial health; class health independent of type; reversible derivation | Actor rebuilds, personal inspection and wizard type/template edits implemented; racial/class derivation covered by rules, injury-preserving racial/class type changes, restart, removal and rewind covered by a real-process case |
| C7 | One racial/two class training points per HD; one attribute increase per four total HD, ordinary cap 5 | Rules, authoring validation and owned wizard training/attribute controls implemented; invalid/overspent/occupied choices and ownership removal covered |
| C8 | One talent slot per HD, whole eligible build pool, distinct one-time talents and dormant choices | Rules, authored choices and owned wizard talent controls implemented; older-slot selection, dormancy, reactivation and ownership removal covered through real processes |
| C9 | Source-local prerequisite levels, acyclic talent dependencies, no self-enabling or temporary-bonus eligibility | Source-local eligibility and dormant choices integrated with derivation, rebuilds and wizard controls; class-loss/restoration lifecycle covered through a real process |
| C10 | Latest-HD removal, ownership removal and zero-HD persistent death | Shared rebuild and wizard removal implemented; authorization, grant-loss cleanup, retry/restart/rewind and process coverage |
| C11 | Injury-preserving maximum Health changes and healing without revival | Actor damage/healing/rebuilds implemented; injured transformations/advancement and persistent death covered through processes; autonomous known-potion healing and consumed-stack persistence verified through a real server restart |
| R1 | Shared bounded d20 and homogeneous damage pool primitives | Shared dice/check rules connected to authored melee and techniques; real-process technique execution covered |
| R2 | Applicable edge cancellation, check-first allocation and separate resolution bundles | Cancellation and check-first bundles connected to melee, Bolt and source-specific Fear; real-server traces verify Fear gives AI attacks and resistance checks one reroll and keeps the lower die; shared-kernel tests cover positive edges, one-for-one cancellation without extra randomness, check-first multi-die allocation and bounded work for excessive counts |
| R3 | Fixed and NdS+bonus damage, primary component, canonical components and miss skipping | Canonical natural/equipped melee and Bolt resolution implemented; full weapon source authoring, disclosure and restart covered; raw actor profiles and melee fallback paths removed |
| R4 | Damage categories plus descriptors; immunity before once-per-group reduction | Category/descriptor protection and authored grants connected to techniques; real-server mixed melee traces verify canonical component merging, descriptor immunity before reductions, source/equipment capacity addition, once-per-group descriptor/category reductions, complete immunity with no injury, and unchanged protection after restart |
| R5 | Three paired defenses; natural 1/20 have no automatic result | All three paired defenses derive and disclose independently; current melee and Bolt use Physical defense, and Fear uses a passive Discipline check against the caster's difficulty. Real-server traces verify distinct defense values, a natural 1 hit with sufficient modifiers, a natural 20 miss with insufficient modifiers, and no damage bundle on the miss |
| R6 | Stamina, Focus and Mana maxima/recovery, capacity saturation and frozen timers | Active/frozen/detached actor timers and checkpoint equivalence covered; real-server streaming tests preserve all three balances and source-specific Fear through 4,000 global ticks, save/restart and thaw, then verify recovery periods, capacity saturation and active-time expiry |
| R7 | Split start/resolution costs, reservations, resumable progress and retry accounting | Paid player/AI preparation and process charges covered; cancellation, checkpoint ownership and AI resume covered; historical receipt/action/lifecycle audit completed; all three techniques reconstruct prepared work, and checkpoint-without-tail restart preserves Fear preparation, reservations and original charges; retry/rewind covered in server integration |
| R8 | Basic melee, power strike, magic bolt, fear and passive resistance | Queued techniques, authoring and client controls implemented; paid techniques exercised through Text/native/headless processes; real-server Fear immunity/resistance verifies resolution payment; mixed source/equipment protection, descriptor/category immunity and restart covered by numerical real-server traces |
| R9 | Source-specific fear affects actions and resistance; refresh/no-stack; immunity clears | Source-specific Fear, refresh/expiry, immunity clearing and technique resistance implemented; real-server checks cover descriptor immunity without resistance draws, paid resisted casts, AI disadvantage with the lower check die, restart, non-additive refresh, ordinary active-time expiry, and permanent clearing on immunity gain |
| R10 | Ordinary queued execution for player and AI, with perception-limited decisions | Player/AI techniques share admitted execution; visibility/funding/resume tested, and each AI technique plus arena selected-death continuation covered through real servers |
| U1 | Own build/resource/talent disclosure, opaque targets and preserved enemy privacy | Simulation/wire disclosure and Text/ASCII inspection implemented; creature stats/save/restart/native presentation covered by processes; unauthorized target inspection rejects, ordinary transcripts omit health seeds and source grants, and read-only inspection preserves paid preparation |
| U2 | Text listing/invocation, native ASCII selection, headless requests and basic bump attacks | All three paid techniques executed through interactive Text, presented native selection and headless processes; real native Right-arrow input resolves exactly one owned basic melee attack without moving or paying a technique cost |
| A1 | Validated arena recipes; duels, manual control and two-team groups | Manifest settings validated; bundled three-HD two-team encounter and paid fear pass through a real server; manual/all-AI and selected-death continuation tested |
| A2 | Normal engine execution, all-AI continuation, loading pins and bounded termination | Streaming engine loads pinned participants, including remote actors at zero radius; unattended session execution and exact tick stop/restart pass through real processes; selected-death continuation/restart now pass through real processes; real-server disconnected-room coverage preserves exactly one non-observing zero-radius mob pin through a checkpoint without replay tail, restart and future-tick stepping |
| A3 | Wizard-only inspection, pause/advance, transformations and replayable full diagnostics | Durable pause/resume and bounded committed-action stepping implemented, including retry, rewind, restart and process coverage; latest-HD removal, named templates, owned advancement and persistent death covered through the wizard interface; privileged source inspection implemented and covered through Text, headless and native-input processes with restart/privacy/preparation invariants; opt-in bounded creature resolution capture and checkpoint replay covered in simulation; independent bounded numerical report schema, semantic validation and authorized trusted backend projection/capture controls implemented; session/private responses and Text/ASCII numerical reports plus headless structured export implemented; real-process numerical query/paging/privacy/restart acceptance and wire recording covered; complete retained-window export and numerical replay comparison covered through a real server/headless/helper process; final full-suite and cross-platform gates remain pending |
| A4 | Paired baseline/candidate seeds and mirrored positions with explicit failure/stalemate reports | Ordinary Engine offline runner and tor-arena CLI implemented with deterministic hashed inputs, health/resource/combat metrics and explicit elimination/cap/stalemate/failure reports; paired/mirrored batch harness implemented with retained input/binary fingerprints, completed-record-preserving interrupted continuation, replay, explicit censored observations and roster checks; curated 67-case 1/2/4/8/16-HD matrix generator implemented with ordinary compiler validation, explicit one-HD mixed/dip exclusions and paired plans; all 67 cases and both orientations executed/replayed through the ordinary Engine in a real-process test, including fully partitioned capped encounters; final cross-platform/release gates pending |
| A5 | Sequential seeded Bayesian search, constraints, independent acceptance and candidate export | Wheel-hash-pinned Optuna environment, constrained seeded TPE primitive, public-API history reconstruction, source/environment integrity and disjoint 20/20/200 seed sets implemented with real-process verification; bounded scalar parameter application to preserved mirrored candidate snapshots implemented with ordinary compiler validation, explicit invalid-candidate evidence and real Engine/replay coverage; full sequential campaign, three-finalist screening, acceptance, verified continuation and manual-review export implemented; default 50/20/three-20/200 campaign audited: 42 feasible/eight compiler-invalid trials, 4400 ordinary Engine encounters without failures/censoring, independent acceptance rejected Strength 4 at -0.16; completed continuation preserved all 6293 files and 800 acceptance reports matched exact replay; final cross-platform/release gates pending |
| V1 | Behavior tests at every touched layer and real-process/native-input acceptance | Rules, simulation, protocol, server, clients and tools have affected coverage; real-server, Text, headless and native-input acceptance exercised; final full-suite and cross-platform gates pending |
| V2 | Save/replay/restart/rewind equivalence and bounded strict decoding | Game checkpoint/melee equivalence and server restart/rewind exercised through process tests, including retained Fear and strict source reconstruction; strict legacy raw-combat checkpoint rejection implemented; full validation gates pending |
| V3 | Updated versions, fixtures, certificates, guides and documentation index | Protocol 32, save 25, ruleset interactions-v26 and validator tor-scenario-11 updated together; wire/source fixtures, bundled certificates, guides and index updated; documentation and scenario-reference checks pass; final acceptance audit pending |
| V4 | Quick/push gates, Windows/Linux debug/release CI, targeted release performance | Pending |
| V5 | Three desktop launchers, real frames, fresh/preserved saves and owned-process cleanup | Pending |

## Initial balance definitions

Attributes: STR, SPD, INT, WIL, AWA, PRE. Physical/Cognitive/Spiritual defenses are
10 plus STR+SPD, INT+WIL and AWA+PRE respectively. Current Health is maximum Health
minus injury, clamped to zero. Maximum is STR plus effective HD health and grants.

Species definitions provide starting defaults; an individual build records its
own initial attributes separately. Arena candidates can redistribute those values
while sharing one species definition. Initial values and owned ordinary upgrades
are checked together against the cap before template adjustments apply.

The eighteen reference skills use the mapping on the
[publisher's reference sheet](https://content.demiplane.com/nexus/stormlightrpg/character/pdf/60733be0-2e8e-432f-bf6b-89f92ec485b3-default.pdf):

| Attribute | Skills |
| --- | --- |
| STR | Athletics, Heavy Weaponry |
| SPD | Agility, Light Weaponry, Stealth, Thievery |
| INT | Crafting, Deduction, Lore, Medicine |
| WIL | Discipline, Intimidation |
| AWA | Insight, Perception, Survival |
| PRE | Deception, Leadership, Persuasion |

Spellcasting is a nineteenth TOR skill using the build's permanent Mana binding
(INT, WIL, AWA or PRE). Neither a temporary attribute boost nor equipment changes
that binding. Attribute values of zero and untrained checks are valid. Ordinary
attributes and skill ranks range from zero to five; template-derived attributes
can exceed the ordinary advancement cap. Initial checks use d20 plus attribute,
rank and explicit modifiers, with success on a total equal to or above the
threshold. Natural 1 and 20 have no separate automatic outcome. Real-server acceptance
uses fixed first-combat-roll seeds to verify both boundary dice against Physical
defense; the numerical trace must agree with the total, applied result and actual
injury. Cognitive and Spiritual defenses remain derived stats for future effects;
the initial techniques do not implicitly substitute them for Physical defense or
Discipline resistance.

Racial dice: Fey d6; Aberration, Animal, Elemental, Giant, Humanoid, Monstrous
Humanoid, Outsider, Plant and Vermin d8; Construct, Magical Beast and Ooze d10;
Dragon and Undead d12. Warrior d10; Mage d6. Zero HD kills a living combatant.

Stamina maximum 2+STR, Focus 2+WIL, Mana 2+bound attribute (zero without magical
capability). Recover one point per 100/300/1000 active ticks respectively; discard
excess at capacity. Preserve injury through Health maximum changes and current
balances through other maximum changes. Pause recovery and effect timers while
frozen. Designated physical durations use ceil(base*5/(5+SPD)), at least one tick.

Natural melee defaults: emu 1d4 impact, hobgoblin 1d6 impact, zombie 1d8 impact.
Power strike adds 3 primary impact damage and costs 2 Stamina. Magic bolt uses
1d6+1 energy, range 6, Spellcasting versus Physical defense, and costs 2 Mana.
Fear uses Discipline versus 10+caster Intimidation, gives one source-specific
disadvantage for 300 ticks and costs 2 Focus. Bolt/fear preparation and recovery
are each 100 ticks. Costs split 1+1 with full reservation at preparation start.

Technique learning consumes a talent slot. Power strike requires Warrior 1 or
Animal/Humanoid racial HD 2 and relevant training. Bolt requires Mage 1 and
Spellcasting 1; fear requires Mage 1 and Intimidation 1 and is unavailable while
mindless. Other accepted talent cards are health/stamina/guard upgrade chains,
melee flat/die upgrades, category wards, Resolve, Arcane Reserve, Potent Bolt,
Empowered/Greater/Master Bolt and Fear Mastery. Distinct upgrades are one-time
choices with predecessor and total/class-level prerequisites, not talent ranks.

Zombified changes primary type to Undead, preserves anatomy, grants mindlessness,
and applies STR+2, SPD-1 (floor zero), effective INT zero. Independent grants
preserve fear/mind-affecting immunity when another source is removed.

## Catalog adaptation and kernel limits

The reference inventory is the
[3.5 SRD type/subtype index](https://www.d20srd.org/indexes/typesSubtypes.htm).
Catalog labels do not silently import that ruleset's other mechanics.

| Catalog group | Current rule-module behavior | Deferred mechanics |
| --- | --- | --- |
| All fifteen primary types | Racial health die from the table above | Other vision, survival, anatomy and combat traits except those below |
| Construct, Plant, Undead | Source-owned fear and mind-affecting immunity | Other 3.5 immunities and physiology |
| Ooze, Vermin | Explicit mindlessness with fear/mind-affecting immunity | Body topology, senses and other 3.5 traits |
| Fire, Cold subtypes | Matching descriptor immunity | Vulnerabilities and other elemental interactions |
| Goblinoid, Reptilian subtypes | Identity labels | Additional ancestry mechanics |
| Air, Angel, Aquatic, Archon, Augmented, Chaotic, Earth, Evil, Extraplanar, Good, Incorporeal, Lawful, Native, Shapechanger, Swarm, Water | Recognized catalog labels | Planar/alignment, movement, body and related mechanics |

Builds are bounded to 256 HD and 32 templates. Each species/template has at most
32 distinct grants. Unspent choices remain attached to their owning HD; every
fourth HD owns an attribute opportunity. Retained health streams are independent
of class/type choice and never borrow the combat random state. Same-priority
numeric adjustments aggregate before flooring, then attribute overrides apply.
Conflicting type, attribute override or subtype add/remove operations reject.

Damage definitions have at most 32 canonical components and at most one million
total possible raw damage. Homogeneous pools merge by category, descriptor and
die size; the explicit primary key leads the otherwise canonical roll order.
Each descriptor group must lie within one category, so descriptor reductions
precede broad category reductions without partially overlapping selector groups.
Protection totals are checked during build derivation.

Cost holds are keyed by intention identity and bounded to 64 per actor. Capacity
loss cancels newest unfunded holds first; paid costs stay paid. Completed retry
idempotence belongs to the engine's intention/receipt layer, not retained cost
tombstones. Fear holds at most 128 causer relations. Refresh preserves the longer
of remaining and newly applied duration, never adds durations. These bounds are
validation contracts enforced by shared rules and scheduler/persistence integration.

The current fixture migration covers source-owned combat, AI, queue, checkpoint,
equipment, collision and frozen-time subjects. Tests preserve authored timing and
health where compatible with the HD model. A one-health collision subject is an
injured eight-health creature; an immunity rebuild retains injury instead of
resetting health. AI technique selection continues to use personal capabilities
and disclosed geometry, without reading hidden enemy immunity. Real-server Fear acceptance additionally verifies immunity skips resistance draws,
resisted casts consume a roll, both outcomes pay the resolution cost, a feared AI
keeps the lower attack-check die against its causer, and restart retains the
remaining condition. A refreshed cast also gives the defender disadvantage on
its resistance check against the same causer. Refresh retains one relation and uses the longer duration
rather than adding durations; ordinary waits expire it at the recorded deadline.
Gaining immunity clears existing Fear, and removing immunity does not restore it.
Mixed-damage acceptance also exercises two authored Fire components that merge to
8 Energy damage, 6 Cold Energy damage, and 4 Keen damage. Fire reduction is 3;
Energy reduction combines a source grant of 4 with armor reduction of 2; Keen
reduction is 1. Private traces verify every stage, and a second ordinary attack
after restart retains both protection and prior injury:

| Immunity | Raw | After immunity | After descriptor reductions | Final injury |
| --- | --- | --- | --- | --- |
| None | 18 | 18 | 15 | 8 |
| Cold descriptor | 18 | 12 | 9 | 3 |
| Energy category | 18 | 4 | 4 | 3 |
| Energy and Keen categories | 18 | 0 | 0 | 0 |

Runtime construction accepts owned builds through `configure_creature`; the raw
`configure_combat` API has been removed. Combat checkpoints require exactly an
identity and an owned creature record, then reconstruct the derived cache. Legacy
`{spec, hp}` records reject without an importer. Unit coverage checks strict
decoding; an actual-server restart test checks rejection without advertising a
listener or rewriting the save. These checks do not replace the final full suite.

## Arena evidence and boundaries

Arena packages reuse ordinary character, actor, creature and faction definitions.
For example, a root manifest setting is:

```toml
arena = { participants = [1, 2], control = "all_ai", ticks = 100000, actions = 10000 }
```

`manual` keeps the selected character externally controlled; every other
participant must use AI. `all_ai` also uses the selected character's configured
AI profile, so that character must declare `unselected = "ai"` and an AI profile.
Faction identity determines teams. AI remains perception-limited and all actions
use ordinary admission, costs, timing and combat rules. Loading pins do not
disclose remote participants to clients.

Test builds at 1, 2, 4, 8 and 16 HD. Include racial/class/mixed progression,
STR/SPD and weapon-style tradeoffs, class dips, Mana bindings, resource sustain,
fear, resistance and varied terrain. Equal HD is not assumed to mean equal power.

Initial search defaults: 50 candidates, 20 training seeds, separate 20-seed
screening for three finalists, then independent 200-seed acceptance for a chosen
candidate and baseline. Mirror encounters and treat them as paired observations.
Per-run limits are 100000 ticks or 10000 actions. Save resolved inputs, hashes,
optimizer history, reports and replay inputs locally, not routine output in Git.
Bayesian TPE tooling is outside simulation; dependency review/pinning and verified
continuation are required. Export candidates for review; never automatically
promote search results to source definitions.

XP/CR/ECL formulas, ordinary leveling UI, playable drain/restoration, survival,
unsupported species mechanics and anatomy-changing templates remain outside this
milestone. Any further deferral needs a concrete maintainability/performance
reason, a documented limitation and evidence; it must not be hidden by a green
narrow test selection.
