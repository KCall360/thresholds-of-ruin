# Documentation and architecture review after PR 206

Reviewed baseline: [`dacbc215`](https://github.com/KCall360/thresholds-of-ruin/commit/dacbc215e12aaa13ef3cf89900836a296127f074).
This is a source-backed review/navigation record, **not a new mechanics
specification, benchmark or fresh runtime qualification**. Implemented fixes are
documentation only; engineering recommendations remain proposals requiring their
own scope, tests and approval. The [coverage map](documentation-coverage.md) gives
whole-project coverage and engineering reading paths.

The original local review was completed but unpublished. Its working files later
became unavailable in the selected execution and cloud-desktop environments. This
version reconstructs the reviewed intent from the same verified Git source and
retained findings, with fresh documentation checks. It is **not claimed to be a
byte-identical restoration**, and old local pass receipts are not reused as proof
of these reconstructed files.

## Integration after PR 207

Before publication, the documentation was integrated onto merged PR 207,
[`b758868`](https://github.com/KCall360/thresholds-of-ruin/commit/b758868a6c790788d2ad46597a8932eac1f954d4).
This bounded follow-up checks the shared advancement pending-query cursor fix and
the now-merged asynchronous model/startup and live 3D interface behavior. It does
not turn the original review into a comprehensive new 3D visual audit or adopt
unmerged feature work. Exact controls and existing native evidence remain owned
by the [3D guide](../../crates/client-3d/README.md) and
[art continuation record](../plans/3d-art-continuation.md).

An outstanding advancement request can survive cursor-only ordered progress when
its actor, branch, picture and input authority are unchanged. That request-tracking
rule does not relax the complete current ReplyContext validation or make an
already displayed page fresh. The client and protocol lifecycle references now
explain that distinction. The 3D guide's old statements that combined map/journal
and advancement/history/notes screens were absent were corrected; unfinished
polish and the unqualified 100+ visible-creature 60 FPS target remain explicit.

## Inventory, audiences and canonical owners

The prior documentation had roughly 33,600 lines. Its principal problem was not
lack of an index: current guides, completed plans and checkpoint diaries disagreed
despite explicit single-owner goals. File size alone is not a reason to split an
otherwise coherent test, performance or art evidence owner.

| Audience/task | Canonical owner | Disposition |
| --- | --- | --- |
| New player | Root README and play guides | Startup capabilities, controls, first purchase, discovery and save-bundle wording corrected |
| Current scope/compatibility | [Status](../status.md) | Compact format/capability/limit/next-work map instead of duplicated mechanics and checkpoint diaries |
| Contributor/agent | [Contributing](../../CONTRIBUTING.md), [testing](testing.md), [AGENTS](../../AGENTS.md) | Existing risk-based policy remains authoritative; stale README mandatory-push/full assertions removed |
| Backend/client integrator | [Architecture](../architecture/overview.md), [protocol](../reference/protocol.md) | Causal/display separation, readiness, checkpoint state and bounded recovery reconciled; joined lifecycle added |
| Mechanics author | [Combat](../reference/combat.md), [creature rules](../reference/creature-rules.md), [progression](../reference/progression.md) | Frozen/live order, body/personal ownership, expertise, drain/restoration and timing corrections |
| Content author | [Packages](../reference/scenario-packages.md), [generation](../reference/generation.md) | Preserve exact schema/live-policy ownership and distinguish registry/activation/retention/operational limits |
| Design reviewer | [Plan map](../plans/README.md) | Implemented rationale separated from proposals; checkpoint chronology retained through immutable links |
| Test/performance investigator | [Performance](performance.md), [objectives](test-objectives.md), [effectiveness](test-effectiveness.md) | Negative/scoped measurements preserved; no new runtime or benchmark claim |
| 3D/art contributor | [3D guide](../../crates/client-3d/README.md), [art continuation](../plans/3d-art-continuation.md) | Separate workspace and visual-review limits; no pending branch treated as current baseline |
| External researcher | [Pinned Rogue research](../research/rogue-5.4.4/README.md) | Fixed-source findings stay research, not another TOR ruleset |

## Old-to-new navigation

| Prior location | Current owner / preservation |
| --- | --- |
| `status.md` repeated mechanics/art/checkpoints | Concise status with exact-owner links; [prior history](https://github.com/KCall360/thresholds-of-ruin/blob/dacbc215e12aaa13ef3cf89900836a296127f074/docs/status.md) |
| `reference/items.md` physical equipment/armor/birth/projectiles | [Equipment](../reference/equipment.md), original headings retained as forwarding anchors |
| `reference/items.md` rings/potions/devices/identification/control | [Magic items](../reference/magic-items.md), preserving all unique healing/overflow/Hallucination contracts and old headings |
| `reference/creature-rules.md` owned advancement/drain/forms/restoration/replacement/XP | [Progression](../reference/progression.md), all original moved headings preserved |
| `play/rogue.md` encounter/item/survival detail | [Rogue mechanics](../play/rogue-mechanics.md); launch/first choice/floors/objective remain quickstart; forwarding headings preserved |
| `plans/baleful-polymorph.md` implemented behavior | [Polymorph](../reference/polymorph.md); plan retains future modes/consent/roster and history |
| Spatial reconstruction plan | [Spatial architecture](../architecture/spatial-registry.md); syntax stays packages; rationale/capacity history retained |
| Consumables plan current mechanics | [Effect reference](../reference/consumable-effects.md) plus magic items, not only an immutable-history link |
| Consumables plan upstream findings | [Fixed-source adaptation research](../research/rogue-5.4.4/consumable-adaptations.md) |
| Completed Rogue/progression sequences | Labeled decisions/acceptance records linked to current owners; detailed checkpoint diaries in immutable prior tree |
| Unclassified plans | [Plan status map](../plans/README.md) |

Meaningful moved reference anchors remain valid for existing readers/source
comments. Current links should target canonical owners; checkpoint-specific history
retains its historical scope and is not maintained as current instructions.

## Added engineering depth

Existing action/atomicity, wire and rule-kernel contracts were already detailed.
The review adds joined explanations rather than falsely describing them as absent:

- [Simulation cycle](../architecture/simulation-cycle.md): ingress→authority→queue→
  actor/input-boundary selection→timers/physics/preparation→region handoff→save
  admission→causal/readiness publication, with failure matrix and worked sequences.
- [Protocol lifecycle](../reference/protocol-lifecycle.md): enum-compared family
  index, independent connection/input/stream states, retry order, pages and recovery;
  newly explicit projectile-choice and ability/weapon-preview integration.
- [Combat](../reference/combat.md): frozen/live table, check/immunity/resistance/
  damage/rider order, deliveries, costs/holds/debt, death/form interactions and five
  worked examples. Detailed modifier/condition subcontracts keep their owners.
- [AI](../reference/ai.md): pure choice versus committed state, sixteen priority
  branches, knowledge limits, retained work, care/items/defense and narrow fallbacks.
- [Effects](../reference/consumable-effects.md): Strength caps/provenance, banked
  Raise Level, completion-state Haste overdose, detection/mapping receipts,
  torch clocks/optics and fixed-census area Health loss.

Independent bounded source/editorial checks covered cycle/wire, combat kernels,
AI ordering and consumable edge cases. Source/test links are evidence of the
review, not reports that those behavioral witnesses were run here.

## Corrected claims and evidence

| Priority | Correction | Implementation/witness |
| --- | --- | --- |
| High | First purchasable live class owner is HD 2 after locked body HD | [Presets](../../scenarios/rogue-exploration/scenario.toml), [parser](../../crates/client-common/src/advancement.rs), [process witness](../../scripts/test_rogue_playable_process.py) |
| High | Body/personal ownership, exact drained rows and expertise replace blanket watermark-only descriptions | [Body](../../crates/simulation/src/creatures/body.rs), [advancement](../../crates/simulation/src/creatures/advancement.rs), [learning](../../crates/simulation/src/creatures/expertise_learning.rs), [body tests](../../crates/simulation/src/creatures/body_tests.rs) |
| High | Checked Search recognizes Xeroc's mace guise privately; actual touch differs from adjacency; discovery has first-sight/first-adjacency receipts | [Discovery](../../crates/simulation/src/discovery.rs), [tests](../../crates/simulation/src/discovery_tests.rs), [content](../../scripts/test_rogue_perception_content.py) |
| High | Ordered outcomes own status/narration; completion does not require a newer picture; hidden-only work need not emit updates | [Session](../../crates/server/src/session.rs), [state](../../crates/client-common/src/state.rs), [Text settlement](../../crates/client-text/src/engine/turn.rs) |
| Medium | Checkpoint has no attachment display revision map; snapshot multi-frame batches have distinct atomic budget admission | [Checkpoint](../../crates/server/src/checkpoint.rs), [outbound](../../crates/server/src/outbound.rs) |
| Medium | Ordinary Zap uses captured item preparation and zero additional recovery, not ability metadata 100/100; reservation starts at preparation | [Interactions](../../crates/simulation/src/interactions.rs), [plan timing](../../crates/simulation/src/abilities/plan.rs), [device capture](../../crates/simulation/src/abilities/ray.rs) |
| Medium | Explicit primary socket selects default weapon; grip handling is not inventory carrying capacity | [Derivation](../../crates/simulation/src/creatures/derive.rs), [weapon tests](../../crates/simulation/src/abilities/weapon_selection_tests.rs) |
| Medium | Mean acquisition has no independent 2-in-3 wake lottery; normal contact differs from continual Stealth | [AI](../../crates/simulation/src/ai.rs), [awareness](../../crates/simulation/src/awareness.rs) |
| Medium | Live roster/survival/objective/stock and baleful Polymorph are not exploration-only or catalog-only foundations | [Package](../../scenarios/rogue-exploration/scenario.toml), [items](../../scenarios/rogue-exploration/catalog/items.toml) |
| Medium | Unique retrieval independently caps eligible-entry receipts at 4,096, including while carried; refusal is not victory | [Retrieval](../../crates/simulation/src/unique_retrieval.rs) |
| Medium | Current NetHack-shaped controls and risk-based assurance replace stale A/G/D and mandatory local push/full claims | [ASCII controls](../../crates/client-ascii/src/model/controls.rs), [testing](testing.md) |

## Recommended engineering work

The documentation review did not implement these proposals. Subsequent
correctness/hardening status is recorded below; remaining performance hypotheses
still require measurement rather than a blanket rewrite.

### 1. Retreat metric across regions — confirmed correctness fix

The pinned review identified a **source-supported candidate, not a runtime
reproduction**: current remembered route length was compared with same-region
Manhattan distance or `current + 1` outside the threat's region. Subsequent
[regressions](../../crates/simulation/src/ai_flee_tests.rs) confirmed the defect
before the implementation changed.

The exact witness is a six-cell corridor split into regions A and B, three cells
each, no gravity, no competing care/erratic/self-step action and no retained
preparation. The engaged actor is A0, visible opponent B2, flee threshold 100,
relevant cells/edges remembered, and East to A1 its only legal move. The old code
selected East despite reducing the five-step route to four; the unsplit control
waited. The positive rotated-portal escape control also passed before the fix.

Current [retreat scoring](../reference/ai.md#retreat-distance-and-limits) compares
hop counts of the same minimum-duration remembered-route policy from the actual
current/candidate frames. Full CI caught a follow-on regression: this initially
made a retained navigation map mandatory even for currently visible local retreat.
The corrected policy retains a single-metric local fallback only in an unwrapped
disclosed chart within the actor/threat region, without teaching topology. Regressions also retain one-way
escapes, blocked/unknown-route behavior, same-region folded-chart rejection,
preview purity and checkpointed AI execution. Additional
candidate searches are an explicit correctness cost, not a measured speedup.
The approved deterministic correction uses rules identity `interactions-v57`.
All saves pinned to the previous identity are rejected, not only saves containing
the affected AI state; scenario certificates must be regenerated. Wire and stored
schema versions are unchanged. Serializer shape compatibility alone does not
establish old-tail replay, and this change provides no migration.

### 2. Save-owned note-anchor leaves — implemented hardening

[Journal](../../crates/server/src/journal.rs) `RecordedAnchor::State` and
[save schema](../../crates/server/src/save_schema.rs) now use save-owned snapshot
and optional-entry adapters instead of protocol leaf serializers. Independent
fixtures preserve existing string/null shapes, required fields, byte-length and
control-character validation, escaped/Unicode strings and private causal anchors.
The stored format is unchanged. This resolves the identified coupling risk;
it is **not a repair of established data corruption**, an importer, or evidence
that a separate deterministic gameplay change preserves historical replay.

### 3. Adaptation and route work — measured counts, timing still open

[Compact outcome adaptation](../../crates/server/src/adapt/outcomes.rs) now lazily
groups already-projected entity rows by location within one call. The existing
location-and-frame disclosure gate and scene iteration remain authoritative;
groups retain list order, repeated identities and portal occurrences. No global
cache or additional disclosure is introduced.

The [count witnesses](../../crates/server/src/adapt/outcomes_work_tests.rs) use
actual four-location corridor/portal scenes with synthetic disclosed entity
rows. With 64 items, 64 apparent objects and 64 actors, four ordinary occurrences
previously inspected 768 rows; 33 portal occurrences inspected 6,336. Both now
inspect 192 input rows once, plus 12 or 99 location lookups respectively. Matched
source-row emissions remain 192 and 1,584; these counts exclude observer aliases
and extra underfoot representations. When no scene occurrence is disclosed,
row work remains zero. Typed projections and eventful serialized facts match the independent
full adapter, including frame privacy, cosmetic names and multi-cell identities.

These are work counts, **not a measured latency improvement**. Grouping adds
`O(N log L)` construction and `O(N + L)` transient references/maps; one/few-cell,
many-location and mostly unmatched workloads can trade scans for extra work.
Release timing and allocation measurements remain open.

Known routes still materialize paths for ranking. A scalar alternative must
preserve the hop count of the selected minimum-duration route, not substitute
the route's duration cost. Body-transition count witnesses likewise find four
permanent compiles on successful away-form changes, with 50/316/1,228 raw HD
visits in the direct 2/16/64-HD fixture family. This is measurement only: no body
optimization is implemented. Identical replacement compilation may be reusable,
but original-body reward/Intellect projection and actual gear/condition folds
must remain distinct; paid and direct fixtures are not interchangeable timings.

### 4. Presentation/history retention — medium capacity risk

[Presentation history](../../crates/server/src/presentation_history.rs) capabilities
retain Game boundaries **even though older full pictures are released**. A
32-capability connection bound and default 16 connections permit many retained
references; COW sharing makes multiplying full Game size misleading. Quantify
actual retained roots/bytes and behavior beyond the rewind window before changing
lifetime semantics. [History indexing](../../crates/server/src/history_index.rs)
also means a checkpoint alone does not bound every startup validation/history cost.

### 5. Checkpoint failure diagnosis — medium conditional robustness

[Storage](../../crates/server/src/storage.rs) should distinguish deterministic
encoding/capacity failures from retryable I/O where retry policy depends on cause.
This review did not demonstrate a production checkpoint crossing its limit or a
corrupt save. Add a bounded failure-classification witness before changing retries.

### 6. Cache locality and scheduling — lower, measure first

Broad upper-cache invalidation, route materialization, bounded identity eviction
and count-bounded mailbox fairness are investigation targets, not proof of bad
latency. Preserve independent exact geometry oracles and typed invalidation
witnesses. A maximum mailbox message count is not a CPU-time fairness guarantee.

### 7. Mechanics acceptance — medium calibration / future scope

Full-run food/recovery/loot/XP pressure and later-depth behavior remain unqualified.
Current 256 actual-HD, materialization depth and retrieval-receipt guards are
operational; neither open selectors nor wide scalars remove them. Temporary forms,
wider Polymorph eligibility/consent and deferred trap feedback need explicit design,
not assertions that existing infrastructure already implements them.

### 8. Drift prevention — maintenance

Update current owner, player summary and joined sequence together when behavior
changes. The source-only Search comment in
[authored rules](../../scenarios/rogue-exploration/rules.toml) still describes old
guaranteed physical discovery despite checked private recognition; correct that
comment in a separately scoped source change, not by changing rules data here.
Keep status out of chronological CI diaries, preserve useful rationale before
trimming plans, and maintain tested examples without duplicating normative fields.

## Strengths and evidence limits

Preserve one authoritative mutation owner, private candidate rollback and storage
admission before publication; acknowledgement is not durable flush. Role/actor
authorization precedes receipt lookup; authorized exact retries precede fresh
context/control-lease checks. Opaque observer references and ordered outcomes
remain independent of presentation revisions. Remembered routing, local frames,
exact geometry oracles, frozen/no-catch-up time, strict restore and bounded complete
output are valuable boundaries. No authorization bypass, disclosure leak or save
corruption was established here; this is not a security certification.

Earlier compact-capture smoke measurements of roughly **3.026–3.235 moves/s versus
a 5 Hz target** remain negative evidence, not a current-baseline benchmark or pass.
[Retained measurements](server-performance-review.md) and
[performance policy](performance.md) preserve workload/machine/tail qualifications.

The static review spans server/protocol/persistence, deterministic simulation,
AI/perception/geometry, progression/equipment/content/generation and shared client
contracts. Optional 3D ownership/scope was inspected, not rebuilt or visually
requalified; not every art/research/historical sentence was re-proven.

## Local validation

Fresh reconstructed-tree validation: all 19 documentation tests passed, along with
Ruff, actionlint and whitespace checks. The change is Markdown-only, and all nine
tagged real-decoder wire examples (plus the three other JSON fences) remain
byte-identical to the reviewed baseline. These are fresh local results, not copied
from the missing earlier workspace.
No Cargo/build, real-decoder, gameplay/process/native or performance runs are part
of this local documentation checkpoint. Publication qualification belongs to the
PR and its exact-head hosted checks, not to these static local receipts.

`docs/reference/protocol.md` remains an executable-documentation input under the
canonical classifier, even for prose corrections. Byte-identical tested envelopes
do not turn the whole change into prose-only CI or waive final-head required gates.
The merged 3D guide corrections also trigger the independent client workflow.
