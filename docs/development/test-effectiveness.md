# Test-effectiveness ledger

Keep a small, evidence-backed record of what tests actually catch and what
maintenance they cost. This is not a pass-count leaderboard, a new merge gate or
a requirement to audit every run. Update it at a coherent fix checkpoint when a
useful finding is established. The [testing policy](testing.md) owns coverage,
execution cadence and qualification.

## Counting and classification

- **Production defect:** implementation violates the supported contract; retain
  the intended failing assertion, correction and green regression evidence.
  Say whether it was caught during development or in released/published code.
- **Documentation defect:** a correct test catches wrong documentation. Do not
  call this an outdated test merely because the repair changes prose.
- **Stale/unupdated test:** an approved contract changed but a fixture, mock,
  driver or expected value did not. Establish the intended contract before
  changing the test; never assume a red assertion is stale because code changed.
- **Harness/infrastructure:** setup, build, driver, environment or evidence
  collection prevented/invalidated the intended check. This is not proof of a
  later gameplay failure. A method may have both stale-fixture and harness
  failures; flag that overlap instead of forcing disjoint totals.
- **Unresolved:** insufficient evidence to decide. Keep it visible but outside
  confirmed defect/stale counts until diagnosed. Deliberate new-feature first-red
  work and mutation sensitivity are separate from naturally detected defects.

Count **distinct root causes**, **distinct failing test methods** (fully qualified
owner plus method), and **failed method executions** separately. One root can
break many methods; rerunning a method or running it on another OS/profile adds
executions, not bugs or distinct methods. Multiple assertions/subtests within a
method do not multiply its method count. A suite/job failure is not another test
failure. Record errors, cancellations, waivers and not-run checks separately.
Define the evidence window and deduplicate before adding totals; never extrapolate
a repository-lifetime count from a partial set of logs.

## Seed audit: Polymorph development, 26 October 2026

Reviewed on 27 October, covering the pre-correction library snapshot, local
`full02` Python stage, initial four hosted Rust lanes and the later inspected
Linux-debug Rust repair lane for [PR #198](https://github.com/KCall360/thresholds-of-ruin/pull/198).
The production list below also uses its bounded focused red/green witnesses.
These are **development defects**, not claims that broken behavior reached main.

**Confirmed in this window: 5 production bug roots, 1 documentation defect,
and 49 distinct stale/unupdated test methods.** The stale methods account for
**61 observed failed/error method executions** in the enumerated snapshots (10 + 32 +
16 + 3). That is a lower bound for the development effort, not a lifetime total
or all PR attempts. One stale method also had a harness error. A global stale
root-cause count is not established; maintenance categories below are not bugs.
No production-versus-stale percentage is inferred from this selected sample.

### Genuine defects caught

| ID | Defect and observed failure | Retained regression and correction |
| --- | --- | --- |
| P1 | Paid Polymorph used a provisional outcome after publication finalized the receipt; the visible changed-form case failed with `Rule(InvalidPlan)` after its positive saved-outcome case. | `abilities::polymorph_tests::paid_polymorph_identifies_only_immediate_visible_changed_form` in [Polymorph ability tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/abilities/polymorph_tests.rs). [Execution](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/abilities/execution.rs) now consumes the authoritative committed receipt, retaining the equality guard. Exact regression subsequently passed. |
| P2 | ChooseAgain restoration resurrected a drained Lore purchase instead of restoring an empty choice slot. The ordinary paid purchase/drain/earned-restoration witness reached its intended assertion and found Lore still present. | External red/green witness `OwnedChooseAgainFirstRed.test_owned_choose_again_discards_drained_lore`; permanent `creatures::body_tests::owned_choose_again_preserves_health_and_funding_not_removed_choices` in [body tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/body_tests.rs). Preserve funded identity, Health and purchase high-water marks, not removed choices. Both ordinary witness and retained regression passed. |
| P3 | Authored restoration refused a lawful sixth retained ordinary attribute receipt despite effective Strength cap 5; valid setup/drain/allocation succeeded, but total HD stayed 24 instead of restoring 25. | External red/green `AuthoredRestoreCapFirstRed.test_retained_authored_sixth_receipt_restores_with_effective_five`; permanent `creatures::body_tests::authored_restoration_reunites_six_lawful_physical_receipts_without_forging_birth` in [body tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/body_tests.rs). Retain raw ownership and cap its effective contribution. This proves a privileged-builder boundary, not ordinary advancement UI acceptance. Both witnesses subsequently passed. |
| P4 | Strict ChooseAgain decoding accepted extra fields; the malformed restoration payload unexpectedly deserialized. | `creatures::record::body::tests::personal_restoration_records_require_policy_and_exact_stable_birth_owners` in [record tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/record/body.rs). An empty struct variant replaces the permissive unit variant without changing valid JSON shape; exact focused regression passed. |
| P5 | AdvancementGrowth validation rejected valid owned zero-personal-HD/body-loss states: Wraith owner-page retrieval failed, and the Strength restoration witness received “Advancement is unavailable.” | `session::advancement_tests::canonical_wraith_earned_regrowth_survives_source_free_service_reopen` in [advancement tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/server/src/advancement_session_tests.rs), and `session::magic_item_tests::self_devices::growth_potions::imported_strength_restores_exclusively_then_reaches_combined_cap_independent_of_ring` in [growth tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/server/src/growth_potion_session_tests.rs). [DTO validation](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/protocol/src/advancement.rs) accounts for full authored body capacity while retaining strict bounds; both exact regressions and added capacity checks passed. Count this as one root, not two bugs. |
| D1 | The documentation test correctly detected a stale ruleset identity in a historical consumables checkpoint. | `test_documentation.DocumentationTests.test_documents_do_not_mention_other_versions`; corrected the historical prose rather than weakening the current-guide check. The repaired method passed. This is not a stale-test failure. |

P1's red was observed in a diagnostic build with unrelated reviewed production
reversals; it was additional to those intended mutations. Its traced receipt
mismatch, narrow correction and subsequent green support the classification;
the diagnostic is not an unmodified-baseline full run. The ten intentional core
mutation witnesses are not counted as ten additional naturally detected bugs.
Other feature/mutation investigations are outside this seed count; unverified
candidates remain pending rather than being promoted from notes alone.

### Stale-test counts and evidence boundaries

| Audited snapshot | Distinct stale methods | Observed stale-affected failed/error executions | Classification detail |
| --- | ---: | ---: | --- |
| Pre-correction simulation/server library run | 10 | 10 | Of 13 failed methods, 3 exercised P4/P5 (2 roots); the other 10 had stale fixtures/oracles. |
| Local `full02` Python stage | 32 | 32 | 30 of 31 failed methods were stale; D1 accounts for the other. Two of the 3 error methods also had stale setup, including the mixed spawn case below. |
| [Initial hosted run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40407638850), Windows/Linux debug/release Rust | 4 | 16 | Same 4 methods on each of 4 lanes; 3 fixture causes, not 16 bugs. |
| [Repair run, Linux-debug Rust](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40413523424/job/121890062812) | 3 | 3 | 3 later methods/fixture causes. Other repair lanes are not included in this execution count. |
| **Deduplicated total within these snapshots** | **49** | **61** | Different snapshots above contain different stale methods; additional reruns are not silently added. |

Library maintenance categories were: unselected restoration career, obsolete
compiled format, old starter HD total, four synthetic perception fixtures,
missing authoring fields, incomplete wand inventory, and obsolete retained-magic
rejection (10 methods).

For `full02`, categories were: old region format (3), missing supported-body
catalog closure (3), missing copied Chill body module (1), obsolete purchase
metadata (7), old starter HD expectations (15), Vampire prefix length (1), old
Extra Healing sole-HD oracle (1), and incomplete compact mock (1): **32 methods**.
The run itself reported **2,139 passed, 31 failed methods, 3 error methods and
100 waived**, with **33 failure assertions**. These are different units, not
contradictory totals and not a successful local full run.

The compact mock errored because its fixture omitted `hit_dice`. The spawn method
had two stale-catalog subtest assertions and a secondary `FileExistsError`; count
that stale method once and record its harness overlap. The other infrastructure
finding was a benchmark helper's 600-second cold Cargo-build timeout **before
benchmark execution**, not a performance assertion. Thus infrastructure affected
two methods in this stage, one overlapping the stale category; do not sum these
categories into an invented outcome total.

Hosted methods and corrections:

- `rogue_chill::rogue_chill_is_a_purchased_checked_status_signature_with_no_fake_damage`:
  author the unpaid variant's matching immutable package, retaining rejection
  under the original package.
- `rogue_roster::production_flying_roster_executes_authored_basic_melee_primaries_and_damage`
  and `rogue_roster::production_flytrap_uses_ordinary_clutch_tightening_and_single_escape`:
  install each newborn's canonical geometry before owned creature admission.
- `scenario_packages::rogue_projectile_content_is_default_available_and_expertise_is_an_ordinary_choice`:
  buy expertise from personal HD, not locked body HD. These four methods were
  fixed in [the server fixture correction](https://github.com/KCall360/thresholds-of-ruin/commit/a202ca6b12377d9bfb945585c2801701c12d471d).
- `advancement_ledger::saved_current_and_authored_purchases_reject_the_257th_owned_talent`:
  supply explicit nullable purchase identity and validate the positive roundtrip.
- `progression::latest_removal_preserves_retained_health_randomness_and_exposes_zero_hd`:
  expect the bound Health policy while retaining exact source/seed assertions.
- `supply_lifecycle::phase5_container_checkpoint_rejects_positional_creature_record`:
  include the new pending-return field before testing positional rejection.
  These three were fixed in [the simulation fixture correction](https://github.com/KCall360/thresholds-of-ruin/commit/5d1f3d90236cbad8e912f6dbd8e172012511e38f).

Both hosted corrections changed fixtures only and their exact four/three local
regressions passed; they did not justify relaxing production admission or strict
rejection. The later local broad run stopped at 626 passes/zero failures by
maintainer direction, not completion. Four repaired methods passed there;
the remaining 30 failed/error methods plus one affected Service-helper witness
passed in a 31-method selection. Sixteen focused Rust boundary checks also
passed. These scoped results are not an exhaustive local pass; final hosted
qualification remains separate, as recorded in PR #198.

**Evidence limits:** this seed was classified from the original local failure
and green receipts plus the specified hosted logs, not only PR prose. Local
raw logs are intentionally uncommitted and are not presented as reader-accessible
links. The linked PR/CI and immutable merged sources preserve public context,
regression assertions and hosted correction diffs; the local occurrence counts
above are the audit summary, not a claim that those links contain every local
receipt. No lifetime failure rate, complete CI-attempt total, or measured speedup
is established. Remaining unreviewed historical failures are **unclassified /
pending**, not zero.

## Dependency integration: SQLAlchemy 2.1.5

Reviewed 27 October 2026 for [PR #194](https://github.com/KCall360/thresholds-of-ruin/pull/194),
from its dependency-update head `aa70a74a46f819384009313894832c93aaa745fe`
and the bounded local correction receipts. **One production integration defect
root**, not a stale test: the hash-pinned requirements selected SQLAlchemy 2.1.5
while the sampler's runtime `PINNED` allowlist still required 2.1.4. A correctly
installed locked environment was therefore rejected before sampling. This was
caught in development; the evidence does not establish a broken main release.

The existing `test_arena_search_sampling.SearchSamplingTests.test_dependency_allowlist_matches_hash_pinned_requirements`
failed at its independent lock-versus-runtime assertion, then passed after
synchronizing the runtime allowlist without changing or weakening the dependency
hashes. This local window contributes **1 naturally failing method / 1 observed
failed execution / 0 stale-test roots or methods**. Broader hosted errors from
the same mismatch are not independently added to these bounded counts.

Two explicit regressions,
`test_environment_rejects_old_sqlalchemy_before_loading_the_sampler` and
`test_history_rejects_old_sqlalchemy_even_with_valid_integrity_hash` in the same
class, each failed before the correction and passed after it. Those are
**2 deliberate diagnostic red executions**, not additional discovered defects or
stale tests. The retained-history case repairs the integrity hash before testing
semantic environment rejection and also proves the rejected state is unchanged.
All seven selected dependency/history tests passed locally; mocked package
metadata acceptance, exact sampler-source digest, missing-dependency rejection,
syntax and unchanged-lock checks passed separately. Installed Optuna/process and
platform qualification belongs to the exact-head hosted run, not these local
mocked checks.

The follow-through is to update lock pins and their strict runtime allowlist in
one dependency checkpoint, with this cheap independent contract selected before
expensive sampling/process owners. Do not loosen history/environment validation
to make a dependency update pass.

## Rogue content batch: bounded iteration findings

The local catalog checkpoint on 27 October 2026 exposed **1 stale-test root,
2 distinct methods and 2 failed method executions**: after the approved dart and
shuriken identities were installed, two closed catalog-inventory expectations
still omitted them. The intended update script had stopped on a source-anchor
assertion after adding the definitions, before updating those inventories.
`test_rogue_equipment_content.RogueEquipmentContent.test_generated_gear_keeps_source_distributions_and_nonlauncher_damage`
and
`test_rogue_playable_content.RoguePlayableContent.test_selected_character_loadout_declares_equipment_and_finite_known_stock`
therefore failed for the two extra identities. Adding those identities to the
independent expected sets retained all unchanged weapon/armor values, starter
kits and live-pool assertions. Both passed in the subsequent 23-method focused
catalog/fixture checkpoint. This is test-maintenance cost, not a gameplay defect.

The same incomplete-edit run had one setup error in the new treasure example
owner because its deliberately new file was not installed yet; classify that as
incomplete new-feature/harness setup, not another stale root or production bug.
Intentional missing-payload, missing-room-cluster and missing-content first-red
checks are separately retained and excluded from discovered-defect counts.

### Compiler boundary and new acceptance drivers

The real current compiler then exposed **1 production integration defect root**
before publication: the server's authoring validation still rejected every
stackable equipment archetype, even though the runtime's narrow standalone-weapon
contract supported darts and shuriken. One natural compiler invocation rejected
`rogue.dart` in the production catalog. An intentional confirmation using
`test_rogue_throwing_process.RogueThrowingProcesses.test_text_carried_dart_and_shuriken_preserve_exact_pending_payload_after_source_deletion`
failed **1 method once** at that same compiler boundary. That confirmation is not
a second root or an additional naturally discovered test failure.

The repair shares the closed equipment-stack allowlist between authoring and
runtime, retains the outer item-capability exclusions, and adds a full authored
singleton/stack positive with invalid-capability contrasts. It does not change
weapon values or permit stacked armor, launchers or passive modifiers. All **44
current packages**, including the external 3D package, subsequently passed the
real compiler; the complete throwing Text process also passed with source-deleted
reopening and exact hit/miss unit accounting. These are local boundary results,
not an exhaustive or hosted qualification claim.

The first completed pair of new process drivers exposed **2 new-harness roots,
2 distinct methods and 2 failed executions**, not stale existing oracles or
runtime defects:

- The throwing method above attempted a third Focus-cost-2 throw after spending
  its finite pool of 4. The retained response was `Action: Failed.` and no unit
  was released. Ordinary bounded player waits now recover the previewed cost;
  assertions still require the actual admission, one-unit consumption and damage.
- `test_rogue_treasure_process.RogueTreasureProcesses.test_generated_cluster_shares_one_room_uses_next_depth_and_reopens_without_authoring_sources`
  attempted to control a newly born NPC not in the account's initial actor
  allowlist. The fixture now uses an explicit neutral, idle-AI control policy;
  production authorization, catalogs and the selected character remain intact.

Both methods passed after those driver repairs. The throwing method overlaps the
compiler-confirmation method above, so summing category method counts would
inflate a distinct-method total. The genuine native throwing witness remains
**not run locally / required in hosted CI**. Compile/lint preflights and
pre-execution review fixes are not included in these test-failure counts.

### Expired-history fixture premise

The focused server checkpoint reported **373 passed / 1 failed / 0 ignored**.
The failure was
`engine::floor_activation_tests::room_cluster::expired_abandoned_cluster_still_binds_selected_room_area_and_full_footprint`:
its initial assertion assumed floor 2 was absent from the live activation scopes.
Streaming legitimately prepares linked neighbors even with zero configured
radii; the reach frontier can activate floor 2 and prepare floor 3. This is
**1 additional new-harness root / 1 distinct method / 1 failed execution**, not a
production defect or stale existing test. Together with the two process-driver
causes above, this bounded new-harness window has **3 roots / 3 methods / 3
failed executions**; the earlier incomplete-file setup remains separate.

The fixture now targets floor 4, traversing three ordinary descents through
unpopulated earlier floors before rewinding. Distinct start/up/down anchors and
the three-step reach bound keep that target outside the initial frontier. Both
initial and expired-history absent-scope assertions, plus every room-area and
full-footprint corruption rejection, are retained. This is a fixture-only
correction; **its runtime rerun remains pending required exact-head hosted CI**.
A no-link preflight is not a replacement for that witness. The other selected
server owners, including the genuine current save writer/reader and encoded
standalone-stack storage-rejection/retry contract, passed in the same local run.

### First hosted content-batch checkpoint: cosmetic palette oracle

The initial [PR #200 run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40462237905)
at head `ade368b78bfeb0658636ccb9fe8b13db4f5ad8bb` exposed **1 stale-test root**:
current Hallucination expectations still described the 81-object catalog rather
than the 84-object catalog after opt-in dart, fruit and shuriken definitions.
The unchanged presentation adapter deliberately includes named catalog entries,
not only live loot. Independent source-tuple projection and the documented
unsigned selector yield `scroll titled NEM AL` at phase zero and
`scroll titled IV MEM` at phase one; creature samples are unchanged. Neither
the selector nor gameplay behavior needed correction.

This bounded six-job inspection covers Linux debug/release Rust jobs
`122049377146` and `122049377204`, Linux debug/release Python jobs
`122049377227` and `122049377124`, and Windows debug/release Python shard 2 jobs
`122052230753` and `122052230786`: **7 distinct stale methods / 18 failed
executions**, not eighteen bugs. Four Service methods failed in each Linux Rust
lane at their shared exact cosmetic assertion (8 executions); two actual-process
methods failed in each of the four Python lanes (8 executions); the independent
roster-vector method failed in both debug Python lanes (2 executions):

- `session::magic_item_tests::self_devices::hallucination::acquired_map_and_paid_public_plan_keep_historical_cosmetics_after_door_cure_and_source_free_reopen`
- `session::magic_item_tests::self_devices::hallucination::paid_production_dose_selects_exact_visual_cosmetics_without_query_rng_or_authority_changes`
- `session::magic_item_tests::self_devices::hallucination::peer_driven_exact_base_period_publishes_unsolicited_owner_and_followed_visual_updates`
- `session::magic_item_tests::self_devices::hallucination::pending_paid_dose_and_active_palette_reopen_interval_one_without_sources_before_service_pause`
- `test_magic_items_process.MagicItemProcesses.test_imported_hallucination_acquired_map_and_paid_history_survive_door_cure_reopen`
- `test_magic_items_process.MagicItemProcesses.test_imported_hallucination_text_visual_cadence_and_source_free_reopen`
- `test_rogue_presentation_vectors.RoguePresentationVectors.test_complete_authored_rosters_keep_visual_and_monster_sense_channels_distinct`

The repair updates only fixed expected tuples and their palette count across
the current Rust/Python owners, Text assertion, Service adapter, recording
validator and its pure fixtures. Exact identity, privacy, cadence, historical
memory, source-free recovery and negative checks remain intact; expectations
are not computed by the production selector under test. Both actual processes
and ten independent-vector/recording-validator checks passed locally using
unchanged qualified applications (**12 methods total**). Runtime reruns of the
Rust Service owners and hosted partitions remain required; no local relink or
broad rerun is claimed. The still-running Windows Rust lanes are outside this occurrence count.
Historical wire-format examples stay unchanged and valid. The previously
corrected depth-four expired-history owner passed both inspected Linux Rust
lanes; those lanes still failed overall on the cosmetic expectations above.

### Second hosted content-batch checkpoint: private damage publication

The [next PR #200 run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40466465054)
at head `30ec322b0e5c3308a09c2f74e3b3d60ee99d0f5e` passed all 1,213 server
library and 269 server integration methods in both inspected Linux Rust lanes,
including the repaired cosmetic owners. Each lane then reported **2,154 passed /
1 failed** in the simulation library. The failure was
`floor_activation::tests::room_cluster_retains_birth_evidence_after_actor_death_and_rejects_duplicate_ids`.
Its birth, duplicate-identity rejection and death checks succeeded, but final
checkpoint restoration correctly rejected unsealed participant-event receipts
as `RunState`.

This is **1 additional new-harness root / 1 distinct method / 2 failed
executions** in Linux debug/release jobs `122063447022` and `122063447005`, not a
production restore defect or stale existing oracle. The test called private
`apply_damage`, an inner-transaction effect, without completing the outer
publication boundary before checkpointing. The correction follows the existing
gold-population fixture: seal participant events, positively assert receipt
validity, then retain all existing birth/death/identity checks and exact restored
state equality. No production validator is relaxed and no events are discarded.

The repair receives a narrow no-link preflight; **the corrected simulation
method's runtime rerun remains required in exact-head hosted CI**. A source audit
of the other new direct-mutation/checkpoint witnesses is a bounded setup review,
not an additional runtime pass. Other profile executions are outside these two
inspected-job counts.

## Spear and magical-form development, 27 October 2026

This bounded local window covers the first linked affected-contract checkpoint
and its five actual-process checks, before publication. **No production defect
was established.** It found **1 stale-test root / 1 distinct method / 1 failed
execution**, plus **2 new-test harness-premise roots / 2 distinct methods /
2 failed executions**. These categories do not overlap. Deliberate feature
first-red witnesses and source-reviewed corrections made before runtime are
excluded from those counts.

- Stale schema owner: `scenario_package::tests::phase5_container_archetype_rejects_positional_fields`
  still enumerated 23 fields after the approved named `forms` field made 24.
  Add `forms` in declaration order; retain the positive named roundtrip, exact
  field-count equality and rejection of the complete positional representation.
- New ordinary-actor fixture: `abilities::magic_item_tests::physical_devices::ordinary_actor_can_hold_physical_device_without_gaining_creature_only_zap`
  tried to equip into a nonexistent socket: `spawn_actor` intentionally starts
  with empty anatomy. Configure normal rules anatomy without creature statistics;
  retain the equipment, forbidden Zap, charge, hold and checkpoint assertions.
  This setup rejection is not evidence of a physical-device production failure.
- New Adventure fixture: `test_rogue_staves_process.RogueStavesProcesses.test_text_staff_physical_hit_charge_exhaustion_and_pending_source_free_reopen`
  expected `examine wand` to resolve uniquely while carrying both a wand and a
  scroll whose disclosed name includes “wand”. Preserve the ordinary ambiguity
  prompt, choose the disclosed wand through clarification, then keep the exact
  wand/staff noun and unchanged-checkpoint assertions. Earlier physics, charges,
  exhaustion and source-free pending-work assertions passed in the failed method;
  that does not make the whole method a pass.

A separate local evidence-driver failure expected exactly one Rust summary line,
although a passing simulation integration method launches a nested test process.
The integration binary itself passed all 815 registered methods; this was
**1 harness root / 0 failed test methods / 1 aborted evidence-collection stage**,
not another gameplay defect or failed method execution. Preserve its complete
output and distinguish the outer summary when collecting evidence. Remaining
selected simulation-library owners were then run separately rather than repeating
already completed integration targets.

The two Rust fixture corrections passed the no-link server/simulation preflight;
their corrected runtime checks remain required in exact-head hosted CI. The
corrected staff process then passed its exact local rerun, including ordinary
clarification and unchanged simulation state. This is risk-based local evidence,
not a full-suite or hosted qualification claim.

## 3D crowd performance PR, 28 October 2026

Hosted [Windows debug 3D run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40618173786/job/122585477848)
on PR #202 head `9fc7d8a4`: **1 production-defect root / 1 distinct method /
1 failed execution**, caught in published `main` code by a PR run; the PR's own
changes did not cause it.

- `test_client_process.Native3DProcesses.test_mouse_picks_the_presented_item_and_inventory_keys_do_not_exit_early`
  hovered the relic under the player, then on the frame the left button went
  down the hover reported a zombie two cells away. The press selected it, and
  the relic never reached the inventory before the output deadline. Hover
  picking intersected the deliberately oversized culling boxes of deforming hair
  (a 3 m cube), cloaks and robes, so a distant figure's animated box could cross
  the cursor ray. The software renderer's roughly 7 s debug frames only made the
  window easier to land in; local software-adapter runs showed this branch's
  frames are no slower than FXC's.
- Fix: those meshes carry `picking::LooseBounds` and picking ignores them,
  finding the wearer by its body. Intended red:
  `picking::tests::culling_only_bounds_never_capture_the_cursor` failed at its
  loose-box assertion with the positive control passing; it passes with the fix,
  as does `creatures::rogue_tests::every_culling_sized_box_is_marked_loose_and_no_body_box_is`.
- The native mouse methods could not run locally (another window covered the
  client: a harness/environment limit, not a result). Exact-head hosted CI
  remains their qualification.

## Server performance review: bounded local findings, 27 October 2026

This window covers the new active-mob harness, five diagnostic-accounting
regressions, and the subsequent ten-method simulation selection on harness base
`9fe8b6d818cc8c0a3364251e99924237d32b3303` plus the recorded local test changes.
It includes the coherent corrected simulation/server checkpoints described below.
The two initial red selections reused one source-qualified library binary; green
selections reused their separately built binary. Inspected command receipts
report unchanged inputs.
These are local development findings, not a claim that gameplay defects shipped.

### Existing diagnostic contracts

Five deliberately added regressions established **3 pre-existing diagnostic
accounting roots / 5 distinct failing methods / 5 failed method executions**.
Group roots by the missing accounting rule, not by each affected write site:

| Root | Methods and actual red evidence | Correction / qualification |
| --- | --- | --- |
| Shared actor-table mutation accounting omitted insert/replacement and removal. | `actor_store::tests::actor_table_copy_counter_includes_shared_insert_and_replacement` and `actor_store::tests::actor_table_copy_counter_includes_shared_remove_and_edit`: each observed 0 copied actors where the independent two-actor snapshot required 2. | Route table mutations through one copy-accounting seam. Two methods, two failed executions; not four bugs for the subcases. Both methods subsequently passed in the selected local opt-level 0 checkpoint. |
| A weak-only `Arc` identity change was counted as an actor payload clone. | `actor_store::tests::actor_table_copy_counter_excludes_weak_only_disassociation`: observed 1 instead of 0. | Distinguish strong shared ownership from weak disassociation. One method, one failed execution; the corrected method subsequently passed in the selected local opt-level 0 checkpoint. |
| Scene-array clone accounting omitted the sight array on installation and both arrays on a warm read. | `observation::tests::scene_cache_copy_counter_includes_both_arrays_at_install`: 143 instead of 279; `observation::tests::scene_cache_copy_counter_includes_warm_snapshot_arrays`: 0 instead of 279. | Count the arrays actually copied; subsequent shared-ownership optimization must still count any retained public `Vec` copies. Two methods, two failed executions; both corrected methods subsequently passed in the selected local opt-level 0 checkpoint. |

These are instrumented-work contract defects, not five gameplay defects or five
naturally encountered failures. Their explicit diagnostic first-red executions
are retained separately from new performance acceptance below.

### New harness and fixture maintenance

The causal harness selection exposed **3 new-harness roots / 3 distinct failing
methods / 3 failed method executions**:

- `test_active_mob_driver.ActivityEvidence.test_run_preserves_causal_failure_and_records_terminal_cleanup`:
  terminal cleanup evidence was absent, and cleanup could replace the original
  movement failure. Count the shared causal-failure/cleanup publication rule as
  one root; its two subtest failures are one method execution.
- `test_active_mob_performance_report.ActiveMobReportTests.test_wait_only_actor_cannot_hide_behind_active_region_totals`:
  active region totals could conceal an individual AI that only waited.
- `test_active_mob_performance_report.ActiveMobReportTests.test_every_crossing_matches_corridor_geometry`:
  the validator accepted crossing counts inconsistent with the exact corridor
  route. Seven negative subcase failures are one method execution, not seven bugs.

The log's **10 assertion failure blocks** therefore do not mean ten methods or
executions. All three corrected methods subsequently passed in the recorded
63-method focused Python checkpoint (exit 0, unchanged inputs). This is evidence
for the driver/validator contracts, not evidence of meeting a latency target.

One additional **new test-fixture root / 1 method / 1 failed execution** occurred
in `perception::contract_tests::reused_projection_indexes_keep_rotated_portal_sight_distinct_from_stair_disclosure`.
Its abstract stair and hidden-disclosure assertions passed, but its later East
physical portal started at interior x=2 in a width-9 region. The boundary-exit
validator correctly returned `NotBoundaryExit`, surfaced as `InvalidLocation`.
The correction leaves the stair assertions intact and moves the observer to
boundary x=8 before adding the rotated physical portal. No production validator
or privacy assertion was weakened; the corrected fixture passed in the same
147-method simulation checkpoint. This is a new fixture error, not a stale approved oracle
or a production topology defect.

### Intentional performance first-red and preflight boundaries

Six further methods reached their intended operation-count assertions once each:

- `perception::contract_tests::one_observation_builds_each_projection_location_index_once`;
- `observation::tests::warm_observation_reuses_its_geometry_projection_indexes`;
- `awareness::tests::awareness_reconciliation_projects_actor_rows_without_full_observations`;
- `awareness::tests::attack_awareness_capture_and_application_do_not_build_full_observations`;
- `awareness::tests::unchanged_attack_listener_reuses_physical_hearing_geometry`;
- `combat::contact::tests::warm_attack_available_reuses_scene_and_revalidates_contact`.

These **6 methods / 6 intentional red executions** establish new optimization
acceptance, not six additional discovered bugs. In that ten-method selection,
three semantic controls passed and the separate portal fixture above failed.
All six passed after the refactor in the 147-method simulation checkpoint.
These behavioral checks deliberately used a task-local opt-level 0 override with
debug assertions and overflow checks retained. They are not release performance
evidence or default-profile hosted qualification.

Three independent preflight causes prevented later checks, with **0 test methods /
0 failed method executions** each:

1. The new benchmark attempted `LowerHex` formatting on an unsupported digest
   array at three source sites. One compilation root and one failed Cargo check,
   not three bugs. Explicit byte formatting corrected it; the matching later
   example check passed (exit 0, unchanged inputs), as did the baseline release
   build. Those build results are not additional test-method passes.
2. The shared-scene refactor left an unused `adapt::observation` wrapper, rejected
   by `-D warnings`. One development preflight root and one failed Clippy command;
   one slice adapter replaced the wrapper and all callers were updated. The
   combined simulation/server test-target Clippy retry passed.
3. The new fixture used manual `% 2 == 0` parity, rejected by current Clippy.
   Equivalent `is_multiple_of(2)` corrected the example-only lint failure; the
   targeted example check passed. No behavioral test ran in this failed step.


The sustained-activity follow-up has separate reconstructed causality evidence:
two new `test_active_mob_driver.ActivityEvidence` methods reject a hidden middle
gap and missing late/canonical/all-AI coverage. An isolated exact pre-sustained
driver passed the positive boundary fixtures but failed both intended rejection
assertions; the current driver passed the same methods and the complete 18-method
driver owner. This establishes a new harness acceptance-gap correction, not a
game defect or an originally recorded checked-tree first-red. The corresponding
three offline Rust proof tests passed in the selected server library checkpoint.

One executor transport replacement interrupted a local behavioral compilation.
No terminal receipt or test result existed; it is infrastructure-only, not a
failed test method. After checking unchanged source, available Cargo locks and
no compiler/linker process, the exact command retry passed. A separate local
cohort wrapper initially named a nonexistent receipt: zero tests executed; the
corrected invocation reused the verified binary and all 147 selected methods
passed. Neither interruption nor invocation error adds a production defect.

The first simulation feedback wrapper said "Cargo failed", but its underlying
build finished successfully and five tests then failed at the accounting
assertions above. Do not count that wrapper status as an additional compiler root.
Documentation preflight caught two maintenance roots in two methods (one failed
execution each): the new versioned workload was missing from the explicit
workload-identifier registry, and new command examples used development-local
paths forbidden by the guide policy. The registry now names only the actual
workload format; examples use an explicitly chosen output-directory variable.
No version or local-evidence check is disabled. The corrected 19-method documentation
checkpoint passed with unchanged inputs.

Raw local logs remain uncommitted; the bounded counts here summarize inspected
receipts, not repository-lifetime failure rates or exhaustive CI qualification.


### Raw World-fact cache follow-up

At the first-wave head `472d9f680417e9e10f1b4e29523e2c70aa72a310`, one new
operation-count method,
`observation::tests::warm_observation_reuses_raw_world_facts_but_projects_fresh_entities`,
passed its nonempty fixture and fresh-projection controls, then failed because the
second unchanged observation rebuilt raw facts (2 builds versus 1 expected).
This is **1 intentional optimization first-red method / 1 execution**, not an
additional discovered gameplay defect. The bounded raw-fact cache keeps complete
World identity and exact scene/emitter evidence while projecting disclosure,
contact, live door clearance, terminal policy and hallucination fresh.

A preflight caught **1 internal test-helper compatibility root / 0 executed test
methods**: removing the old concealment wrapper missed an independent awareness
witness. A test-only wrapper delegates to the same canonical predicate; the
existing assertion was not changed. The coherent formatting, simulation test-target
and affected benchmark Clippy retry passed with unchanged inputs. No claim of a
production behavior failure follows from that compile-only result.

A separate executor interruption left the first no-link session without a terminal
receipt. Source and Cargo locks were checked before exact-command recovery;
that retry passed. This infrastructure interruption is not a failed test method.

The green checkpoint passed all six new witnesses plus one existing raw-world
hazard method, then 151 remaining affected contracts from the same verified
executable: **158 unique methods, 158 passing executions**. The retained tests
cover portal/body/contact disclosure, fresh hallucination, actor/item privacy and
branch isolation; eviction now also checks release of raw cached payloads. These
checks use the task-local opt-level 0 profile with assertions and overflow checks,
not release timing or default-profile hosted qualification. Server/integration
and real-process final-head qualification remain pending; their earlier passes
are not silently relabeled as coverage of this changed simulation dependency.

The first-wave real headless smoke remains failed: throughput was 0.472 Hz and
source-free restart exceeded its unchanged 30-second limit after successful save
and inspection. A protected-copy diagnostic later restarted in 8.664 seconds;
the earlier failure remains unreproduced and unresolved, not waived. Ordinary
saved-action p95 passed its 8 ms budget, but the larger case exceeded the 33 ms
maximum at both baseline and head. These negative acceptance results are not
counted as discovered implementation roots without a causal diagnosis.

### Exact decision-geometry reuse follow-up

At head `da492247cfb57a6043a2a39e826d983bc57b371a`,
`observation::tests::decision_reuses_witnessed_geometry_but_refreshes_disclosure`
passed its nonempty, identical-view, exact-sight and newly placed item controls,
then failed at the intended work assertion: three scene builds versus one.
This is **1 intentional optimization first-red method / 1 execution**, not a
new discovered gameplay defect. The fix shares the ordinary exact-witness
geometry lookup while projecting current entities, knowledge and effects anew.

The bounded review updated existing operation-count fixtures to explicitly
measure either a cold construction or a warmed lookup. Action, route, tactile
privacy and precommit-plan assertions remain exact. These were proactive
contract updates, not observed stale-test failures; no failed-test count is
invented for them. The geometry invalidation fixture additionally compares
fresh decision sight and observations after chart, body, orientation and death
changes against the uncached computation.

The green checkpoint passed the new regression and 160 affected methods from
the same verified executable: **161 unique methods / 161 passing executions**.
The integration checkpoint passed the warmed finite-supply boundary plus 20
affected public integration methods (21 unique methods). A local selection
preparation typo caused one driver failure before any method executed; correcting
the selection reused the same binary and did not change assertions.

This task-local opt-level 0 evidence preserves assertions and overflow checks;
it is neither release timing nor exhaustive/default-profile hosted qualification.

### Shared cell-array follow-up

The first new copy-count test exposed **1 new-fixture setup root / 1 method /
1 failed execution**, not a production defect: the standard two-room fixture
contains a door and therefore failed its own feature-free positive precondition.
The existing bare-chamber fixture is the correct positive control. No copy-saving
proof was claimed from that setup failure. A prematurely launched green no-link
check was allowed to finish with frozen inputs; original instrumented Vec
production was then restored for the corrected causal red. This avoidable extra
preflight is recorded rather than presented as useful regression evidence.

The corrected positive fixture passed its full-view, fresh actor/item and
stable geometry controls, then failed at the intended copy assertion: 190
`CellView` rows copied versus zero. This is **1 intentional optimization
first-red method / 1 execution**, separate from the setup failure above. The
new regression then passed with shared arrays, independent caller edits and
exactly-once projection detachment accounting.

The passing no-link preflight covers simulation/server test targets and the
benchmark. Its production and consumer input hashes exactly match the green
implementation; the sole subsequent difference is the corrected existing fixture
call in the new test, compiled in the causal red and green runs. That bounded
reuse avoids another identical production lint build; it is not mislabeled as
an exact-tree Clippy pass. The existing public integration tests passed at the
separate geometry-only checkpoint; they are not relabeled as fresh COW evidence.

The final focused library checkpoint passed **162 unique methods / 162 passing
executions** from one verified executable, including live door/trap and
hallucinated-stair detachment with unchanged retained snapshots. The next
integration evidence is the combined release workload and real headless process;
final-main affected integration/server qualification remains required. These
selected local checks are not exhaustive hosted assurance.

## Immutable disclosure and boundary work: local checkpoint, 27 October 2026

The instrumented pre-change checkpoint preserves production behavior at
`45c723dc435d21f0e46b9dbe4d1138f42c8b8e77`. Four new regressions reached their
intended assertions after positive state/privacy/replay controls:

- `control_scrolls::tests::absent_ground_scare_clears_only_eligible_receipts_and_preserves_earned_flee`: eight unnecessary neighborhood probes, expected zero.
- `control_scrolls::tests::unchanged_present_scare_contact_does_not_copy_shared_actors`: two actor payload copies for an unchanged contact transition, expected zero.
- `creature_view::tests::fresh_observations_do_not_copy_owned_personal_inspection_collections`: four owned stat expansions for four fresh observations, expected zero; demanded wire materialization remains positive.
- `engine::observation_reuse_tests::action_projection_reuses_settled_readiness_without_revising_disclosure`: eight observer-local scheduler scans after one settled action, expected zero; full observations, tie ordering and exact replay controls passed.

These are four deliberately introduced performance first-red methods/executions,
not newly discovered independent correctness defects. Two additional new tests
failed setup because their shared fixture supplies starting equipment, contrary
to a new empty-item-store assertion. That is one new-fixture assumption root /
two methods / two failed executions, not stale existing coverage. Their correction
asserts the actual no-ground-scare precondition without deleting equipment or
relaxing state/error assertions. The same absent-feature cause already has the
first-red evidence above, so no redundant old-source link was added.

The first green no-link preflight caught one missed existing consumer migration:
`deep_population_atomicity_tests::assert_original_admission` still accessed
owned `hit_dice` fields directly on the new immutable facts type. Two E0609
diagnostics belong to one stale-test API-migration root / one failed preflight /
zero executed tests. Explicit demand materialization preserves both existing
251-HD assertions. No production behavior was weakened.

The green checkpoint passed **59 unique methods / 59 executions**: 25 simulation
library, 26 server library and 8 integration owners. Both corrected fixture methods
and all four intended-red methods passed. No-link Clippy (simulation/server
libraries, tests and active workload example), formatting and architecture checks
passed. The first architecture invocation had an unset tooling PATH and never ran
Cargo metadata; the correctly configured retry passed. This is a setup error, not
a production or stale-test defect. Release timing, real-process throughput and
final-main/hosted qualification remain pending; no 5 Hz success is claimed.

The documentation check then caught one genuine portability error in this new
review: a citation to ignored, machine-local receipts.
`test_guides_do_not_cite_local_only_evidence` failed once; the report now states
source identities and aggregate evidence without presenting private local paths
as repository references. This is one documentation defect / one method / one
failed execution, not a stale-test failure. The assertion was unchanged.


## 2026-10-27: preparation and witnessed projection components

Three intended first-red performance regressions exposed unnecessary work, not
new gameplay defects: non-due preparation checks made four owned snapshots,
identical streaming geometry made eight body queries, and unchanged full/reference
views repeated two actor classifications. Due-only snapshots and exact existing
scene/visibility witnesses remove that work while preserving dynamic disclosure.
The focused checkpoint covers preparation interruption/overflow/ordered riders,
streaming witnesses, rejected branches, event proofs and server revision privacy.

The initial combined run completed all 61 simulation methods before its server
compile was interrupted without a terminal receipt. The resumed driver first
encountered the abandoned verifier lock; no test ran on that attempt. Free Cargo
locks, unchanged source hashes and the unavailable prior session established the
interruption; the stale lock was preserved and only the remaining owners resumed.
These are infrastructure failures, not production or stale-test roots.
The completed checkpoint passed 78 unique methods: 61 simulation library,
13 server library and 4 simulation integration methods. Clippy preflight passed.
These are focused local checks, not hosted or exhaustive qualification; release
measurement of this batch remains pending.

## 2026-10-28: canonical reach geometry and empty ground work

Two intended first-red performance contracts failed on the unchanged implementation:
`combat::contact::tests::warm_contact_endpoints_reuse_source_geometry_but_resolve_live_target`
resolved 12 body cells rather than four fresh target cells, and
`item_store::tests::empty_ground_index_skips_scene_probes_without_losing_carried_items`
performed eight ground lookups rather than zero. Both positive fixtures passed
before their work-count assertions. One linked simulation artifact qualified both
reds. These are planned optimization regressions, not two newly discovered gameplay
defects or stale-test failures. The cheap green preflight caught one new implementation API mistake: the existing
`RegionMap` has no `is_empty` method (one E0599 diagnostic, zero executed tests).
The fix checks its first indexed entry without adding an index or changing behavior.
This is a compile-time implementation defect, not stale coverage.
Canonical contact geometry remains the only route
algorithm; cold-oracle/live-state and carried/ground mutation witnesses are retained.
The completed focused checkpoint passed 52 unique methods: 35 simulation library,
13 server library and four simulation integration methods. Both intended reds and
the live combat/death/rejected-world witness passed. No-link Clippy passed after
the API correction. These are focused local results, not hosted/full qualification;
release measurement of this batch is pending.


## 2026-10-28: per-actor snapshot ownership

One deliberate first-red performance contract,
`actor_store::tests::shared_actor_edit_copies_only_touched_payload_and_retains_snapshot`,
observed three actor payload copies for one edit in a retained three-actor snapshot,
where the intended bound is one. Positive original/candidate value assertions
passed before that count assertion. This is planned optimization work, not a
new gameplay defect or a stale-test failure. Two existing copy-accounting tests
are deliberately updated to distinguish copied actor payloads from copied shared
map handles; their isolation and owned return-value assertions remain strict.
Weak-only disassociation, unique removal/replacement, indexed mutation, rollback,
visibility and checkpoint witnesses are included in the focused checkpoint.

One preflight invocation repeated `--lib` and Cargo rejected the command before
compilation or tests. This is one driver error, zero test methods and zero stale
fixture failures; the corrected command removes the duplicate argument.
The focused checkpoint passed 29 unique methods: 19 simulation library, six
server library and four simulation integration methods. Clippy preflight passed.
The later release smoke measured 2.115 moves/sec, still below 5/sec; no full or
hosted pass is claimed.

## 2026-10-28: change-driven commit facts

The deliberate first-red
`event_history::tests::commit_event_facts_match_rich_authority_without_materialization`
passed positive identity, receipt, restore and candidate-isolation assertions,
then observed one rich Observation where the intended commit-only path needs
zero. This is a planned performance contract, not a new gameplay defect.

One new fixture/oracle error was caught in
`event_history::tests::commit_event_facts_keep_hidden_identity_and_reject_stale_or_wrong_owner`:
the adjacent invisible source correctly retains tactile Contact authority for
its victim, while the new assertion incorrectly expected no caster identity.
The corrected witness requires exactly Contact (not Creature) and the exact
source ID, while the third-party proof stays empty. Count: one fixture root,
one method, one failed execution; zero stale existing-test failures and zero
production defects. A second new fixture error requested 100 actors from the
small diagnostic constructor, which accepts at most eight; setup failed before
any behavior assertion. The corrected fixture explicitly distributes five
validated positions over each of twenty rooms. This is one additional fixture
root/method/failed execution, still zero stale existing-test failures. The
interrupted server link hit the disk guard; a subsequent retry found its abandoned
verification lock. Both are infrastructure outcomes before tests, not product
failures; obsolete debug executables were removed and the proven-abandoned lock
cleared before retry. The no-link check also caught a new test's private Engine
field access before any test ran; it now uses the existing checkpoint encoder
and simulation restore instead of widening production visibility.

The simulation checkpoint passed 40 event/receipt/boundary methods plus three
reused exact preparation, map-memory and historical-plan witnesses. The new
awareness work-count test passed semantic Game/contact/RNG equality and proved
a real visibility-triggered rescan, then deliberately failed at one unnecessary
actor payload copy versus zero. The source-local guard skips only an identical
contact assignment; it does not skip scans or contests. Its green and the server
integration checkpoint passed. The corrected server first-red then
passed full observation/revision/replay equivalence and failed at exactly 100
rich observations for one local move across the 100-actor fixture, above its
bounded affected-observer budget. The actual server map-adapter fixed-point
witness passed in the same qualified binary.

The wired checkpoint passed all 13 selected server methods and 54 of 55
simulation methods. One existing awareness work-count assertion still expected
two expensive classifications after exact classification sharing had reduced
that to one; its identity/contact/alert/RNG and zero-rich-view assertions passed.
The fixture now explicitly starts its scene cache cold and requires exactly one
classification with an unchanged visibility witness. This is **one stale existing
test root, one method and one failed execution**, not a product defect. Its
focused rerun passed; no unrelated production change was made for it. A no-link
preflight caught private cache-field access in the new cold-cache setup; the
fixture now resets through the existing Default implementation. That compile
error was not an executed test failure. The combined checkpoint qualifies 68
unique methods (13 server, 55 simulation), reusing the 67 earlier passes across
only the test-body/prose correction. Clippy passed on the production changes.
Release measurements subsequently reached 2.937 moves/sec (still below 5/sec);
hosted CI is not claimed.

One additional stale-validator root was caught by all four head-side ordinary
benchmark validations: `validate_work` still required exactly two rich views
per actor, contradicting the intentional boundary-proof fast path. The fix
requires all proof fields together, strict integer counts, exact observer
coverage and bounded fallback/map materialization; absent proof fields retain
the legacy contract. Its positive proof fixture reproduced the intended red,
39 Python tests passed, and all eight retained base/head raw runs revalidated.
Count: one validator root, four failed validations, no product defect. A first
revalidation command incorrectly selected the phase-D case subset; that was a
driver flag error and is retained separately, not counted as a stale test.
No benchmark or Rust build was repeated.

## 2026-10-28: independent input authority and admitted operands

Two deliberate first-red witnesses establish the changed contracts. The wire
fixture first validates its InputContext, then fails because the old Act schema
requires `expected_revision`. The human-operand test admits a natural attack,
equips a different weapon and reaches the intended failure: the queued Default
attack selected that new item. These are explicit semantic changes, not newly
discovered violations of the previous contract. The three causal regressions now pass in focused green qualification. A third causal witness commits a real origin
change and fails because the unconnected runtime registry still holds the old
basis; per-commit publication hooks are now enabled for green validation.

Cheap preflights caught two newly authored InputBasis mistakes (an interior doc
comment and wrapping an already typed ItemId), then passed after correction.
The coherent server preflight caught 23 remaining compile errors in integration
fixtures still using the removed wire field or old decoded shape. These are
migration completeness errors before any affected test executed, not 23 failing
tests or product defects. A later all-target preflight found one additional
map-return fixture with the obsolete wire field; that was corrected along with
an unused fixture actor parameter, and the server preflight passed. A read audit also caught a historical wire fixture from revision 52
being edited instead of the current v64 source; the historical fixture was
restored exactly. The provisional current fixture was subsequently replaced
by genuine final-server process recording; historical fixtures remain unchanged.

The first focused green run exposed two stale assertions after the approved
contract change: `engine::wire_request::tests::durable_wire_retry_resolves_before_a_taken_item_or_revision_is_checked`
still expected `StaleRevision` when resolving an unavailable item as a new
request, although the wire action no longer carries a display revision. Its
same-request durable retry and fresh unavailable-target assertions both passed;
the final expectation is now `InvalidAction`, with no removed safeguards.
`wire::current_wire_protocol_revision_is_explicitly_pinned` still pinned 64
rather than the new 65. Count for this checkpoint: two stale roots, two distinct
methods, two failed executions; both targeted green reruns passed. The other 24
selected server methods, 11 selected simulation methods, and 290 protocol
integration methods passed. These counts do not constitute whole-package or CI
qualification.

The local multi-target runner initially omitted client-common's required
`test-support` feature; compilation failed before any common test executed.
The corrected owner passed all five selected methods, and four ASCII socket
methods passed. The server integration link twice crossed the disk headroom
guard; the owned run was stopped, obsolete archived build artifacts were
removed with manifests preserved, and only the unfinished target was resumed.
Stale wrapper lock refusals performed no additional builds or test executions.
The three selected integration methods then passed. These are harness/resource
interruptions, not product defects or stale test assertions.

The documentation check also found the Hello example and several appearance
handoff references still using the prior wire revision. The example is updated,
while historical handoff prose now describes its original introduction without
presenting an obsolete revision as current. A historical fixture filename in
this ledger was rephrased to avoid the guide's reserved ruleset-name syntax.
This is one documentation maintenance finding, one failing method
(`DocumentationTests.test_documents_do_not_mention_other_versions`), one
execution; not a production defect or a stale test assertion.

Actual-process coverage found one genuine implementation defect in this local
input-authority change: the new private operand/control generations also
participated in spectator readiness numbering. Spectators have no command
permissions, so owner operand/control changes must not become spectator
authority-numbering signals. The input basis already excludes private charges
and reservations; no private-item-value leak was established. The existing regression
`IntentionProcesses.test_readiness_is_authoritative_across_control_changes_and_simulation_execution`
failed at its exact spectator invariant (revision 4 instead of 0); the other 11
selected process methods passed. This is one development production-defect root,
one failing method, one failed execution, not a stale expected value. The fix
excludes spectators from those private generations while retaining controller
ABA protection; the focused real-inventory/control-ABA regression, three related input-context
methods and two existing owner authority/exhaustion regressions pass. The exact
previously failing actual-process case now passes against the rebuilt server,
and the final real wire recording succeeds. The unchanged client/bootstrap
artifacts retain their explicit earlier-build provenance. Pre-fix performance/recording evidence is retained as pre-fix, not
relabeled final-head qualification.

The later frozen-baseline ordinary comparison encountered three additional
executions of the already recorded old boundary-accounting validator failure;
this is the same validator root, not three new defects. Seven complete raw runs
validated under the current oracle without rerunning, and only the final run
interrupted by the disk guard was replaced. The replacement is not temporally
adjacent to its paired head sample. All eight complete runs validate structurally,
but the ordinary maximum-latency budget still fails; validation is not a claim
that the performance target passed.

### Typed dependency evidence checkpoint (2026-10-28)

The first cheap preflight caught two implementation/lint issues: the local
Rust toolchain deprecated `AtomicU64::fetch_update`, and an item before-location
fact was not yet consumed. The checked allocator now uses a compatible CAS loop;
old/new carrier dependencies are explicitly invalidated. A read audit also
removed strong global identity handles that would unnecessarily retain COW
payloads; these were review findings, not behavioral test discoveries.

A new internal region-index invariant test was accidentally inserted twice by
the coordinator and helper before the preflight source snapshot. The compiler
rejected the duplicate name. The exact duplicate was removed, and sole source
application ownership clarified. This is one new-test setup/coordination error,
not a stale existing expectation or a product failure. No broad tier was run.

Four exact causal witnesses failed at their intended assertions before enabling
the preparation/region/observer paths and fixing empty-source bucket lifetime.
All four now pass. The bucket regression is one new implementation invariant
defect discovered in review and confirmed by an internal two-item-row test, not
an observed valid published-game failure. Explicit contributor counts fix it.
The unchanged red binary also passed 28 store/ancestry/weak-root/cold-index
invariants; final selected green passed 159 simulation and 17 server methods.

The first coherent green run passed all four causal tests and 154 other selected
simulation methods. One existing awareness test still required a full perception
rescan after any global visibility change. That is the implementation detail
this batch intentionally removes: remote-disjoint changes can now carry a
certified unchanged classification. The test retains exact Game/contact/RNG and
no-COW assertions, and now requires every eligible observer to be covered by
one rebuilt or certified classification and the newly recorded awareness
boundary to contain all eligible observers. This is one stale-test-maintenance
root, one failing method, one failed execution. It is not counted as a product
bug or a waived failure. The updated test passes, as do all other selected methods.

The selected server runner initially named the spectator test under the generic
module rather than its actual nested owner. One first server test passed before
selector resolution stopped the runner. Resolving the exact registered names
then reused the same verified binary for all 17 selected methods; no rebuild.
This was a selector-driver error, not a test or production failure.

### Simplifying the dependency experiment

The broader typed-dependency cohort passed its focused contracts but did not
establish a matched throughput gain: active-cycle medians were 316.458 ms before
and 317.027 ms after, with 16 samples per side. The real headless run completed
3.029 moves/second, below the unchanged five-per-second goal. Region pin spans
improved, while availability and observer proof work did not demonstrate enough
benefit to justify their complexity. Instrumented scope timings are diagnostic,
not additive estimates or a reason to discard noisy samples.

The next cohort removes positive availability-proof caching and general observer
delta reuse, keeping region contribution ownership and exhaustive empty-residency
classification. Four availability-proof-only tests and two observer-delta-only
tests are removed with those mechanisms, not counted as lost gameplay contracts;
existing canonical availability, target/payment/lifecycle, perception/privacy,
rollback and replay tests remain. The awareness test again uses its original
canonical-rescan assertion because that implementation is restored. The prior
stale-test failure remains recorded above rather than erased from the ledger.
The genuine empty-source bucket lifetime regression remains mandatory.
The simplified cohort passes 153 selected simulation and 17 server methods.
A disk-headroom guard interrupted the server test link before any server test
ran. Existing simulation evidence was retained; obsolete debug CLI executables
were removed with hashes recorded, and only the unfinished server target was
resumed. No release baseline, library cache or source was removed, and this
resource interruption is not a gameplay defect or stale-test failure.

## Add the next useful finding

One compact row or paragraph per diagnosed cause is enough:

> Date / PR / source state; category and supported contract; exact failing test
> IDs and intended red; fix and green evidence (link when available); distinct
> roots / methods / observed failed executions with window and overlaps;
> unresolved or CI-only coverage. Update an existing cause instead of recounting
> it when another OS, profile or rerun exposes the same failure.

Use the findings to improve fixture update lists, cheap preflights and affected
owner selection. They are not a reason to remove strict assertions, security
coverage, unique boundary witnesses or required native CI.

### Canonical map projection and socket latency checkpoint

Focused development distinguishes intentional causal failures from new test or
runner setup mistakes. Restoring legacy rich-map work, old-side classification,
and TCP batching makes the corresponding new work/socket assertions fail; these
are deliberate first-red witnesses, not newly discovered gameplay defects.

Two setup roots were corrected: the all-target client lint command initially
omitted the existing `test-support` feature, and the new map oracle requested two
units of a non-stackable ordinary item. The latter failed with `InvalidQuantity`
before its intended assertion and is not causal optimization evidence. One
runner initially summarized that setup failure too broadly as an intended red;
its exact assertion evidence was subsequently checked and that one method was
rerun with quantity one. Neither case is counted as a stale existing test or a
production defect; successful independent evidence was preserved.

Two existing observation-reuse methods still assumed map acquisition eagerly
created a rich view after every action. They failed at zero-additional-view count
assertions, while all selected map/disclosure/replay/rollback checks passed.
The revised witnesses count exactly the missing views, require each to materialize
once, and continue requiring repeated reads to reuse the identical boundary.
This is one stale-oracle root, two distinct failing methods and two observed
failed executions, separate from the new fixture setup error above.

### Boundary derivation follow-up accounting

A source audit found that the newly separated intention-derivation duration was
serialized in command profiles but omitted from their exclusive-duration sum.
The ordinary latency report therefore left that duration in its unattributed
bucket. This did not change total latency, game behavior or acceptance budgets;
the retained 123.724 ms tail contained only 0.002283 ms of intention derivation.
The sum and report fields now include each narrow boundary phase, with a focused
unit assertion for exact once-only accounting. This is an audit-found diagnostic
defect, not a defect credited to a previously failing test. Existing timing
receipts remain unchanged; no new stale-test failure is inferred from this audit.

### Borrowed growth-retry ownership follow-up

The new shared/unique allocation-identity witness passed and demonstrated the
old mutable no-op detachment versus retained ownership after a borrowed retry
decision. Thirteen other selected simulation methods also passed. This is causal
refactor evidence, not a claimed historical first-red production defect.

One newly authored pending-recovery-context method failed its positive fixture:
the creature had no contextual recovery source, so canonical normalization
correctly retained `Known { sensed_hostile: false }` instead of the test's expected
`true`. The corrected fixture provides and asserts that capability and the actual
recovery period before exercising context success and error ordering; its
focused rerun passed. This is one new-fixture root, one distinct
method and one failed execution, not a stale test following an approved contract
change and not a production defect. Successful independent evidence is retained;
only the corrected method and unfinished targets need new execution.

A disk guard later interrupted only the unfinished server test compilation.
All successful simulation evidence was preserved. Inactive duplicate artifacts
and obsolete library variants were cleaned with recorded hashes; the server-only
resume passed all six selected methods. This is an infrastructure interruption,
not an additional failed test method or a production/stale-test defect.


### Causal/display separation: preflight accounting

The protocol66/archive64 migration deliberately changes display revision ownership,
snapshot identities and event delivery. Static preflight found unupdated test and
benchmark constructors (required snapshot fields, formerly-Copy bases, private
causal guards and removed revision-map internals). These are compile-time fixture
maintenance, not observed failed gameplay methods. The initial package-selection
error was a driver/workspace setup issue; helper visibility, a stale annotation
argument and one Clippy signature warning were development integration errors.
No stale-method execution count is inferred from these compiler diagnostics.

Source review also identified two semantic development defects before execution:
legacy tick/value narration gates could duplicate old receipts or omit identical
same-tick new receipts, and post-commit outcome projection failure could leave a
live stream without its committed event. Causal batch/suffix capture and explicit
stream disconnection address those respectively. Focused tests retain legacy
counterexamples and exact expected outcomes; these are audit-found issues, not
credited as historical failures caught by an already-running test suite.
The 68 recorder-driver unit methods passed; this does not qualify Rust behavior
or the performance goal.

The first focused Rust pass passed both protocol methods and 27 common-client
methods. Three newly authored outcome-stream methods failed before their intended
assertions because their positive fixture read the not-yet-recorded wire66 golden.
The fixture is now independently constructed and validated, removing that circular
qualification dependency. This is one new-fixture/setup root, three distinct
methods and three failed executions, not three gameplay defects or stale existing
tests. The 29 successful methods are preserved; only these three and unfinished
targets are selected for the resume.

### Causal/display separation: ordered text exchange resume

The focused green-resume pass had **one failing method execution**:
`turns::an_exchange_of_blows_is_told_in_order_and_a_death_ends_it`.
Its actual passage stopped at preparation instead of the expected interrupted
exchange and HP loss. Diagnosis established two overlapping roots, not two
executions:

- **One stale existing-script root / one distinct method:** the script sent an
  Immediate Ack for admitted Act work and changed only visual `ready`, leaving
  authoritative Readiness open. Settlement correctly trusted that purported
  completed immediate request. The script now sends Admitted, Started, closed
  Readiness, its unchanged ordered outcomes, Resolved, then reopened Readiness.
  No expected prose or production permission checks were weakened.
- **One development production narration regression:** `Chronicler::outcome`
  ignored the disclosed `OwnAction.basic_attack` flag, rendering generic ability
  preparation instead of the existing basic-attack phrase. It now uses that
  flag, not an ability ID/name heuristic. The new focused method
  `engine::chronicle::tests::outcome_preparation_prose_uses_disclosed_basic_flag_not_ability_name`
  compares new and legacy beats for both flag values with a renamed ability.

Both issues were exposed by the same original execution. Fixes are staged;
no green rerun is claimed here, and the newly added witness has not yet run.

The new basic-flag unit witness subsequently **passed** and is preserved without
rerunning. The existing exchange method had a **second failed execution**, this
time while validating its reconstructed queued stream before any prose assertion.
The first fixture correction had omitted the Queued intention update: an Admitted
receipt does not populate `ClientState.intentions`, so a first-seen Started update
correctly failed `InconsistentState`. The script now emits Queued before Started,
with the same exact intention/entry identity. This is an incomplete correction of
the same stale-script root (one distinct existing method, now two failed
executions), not an additional production defect or another failure of the
successful basic-flag witness. No further rerun result is claimed here.

The first focused server pass preserved 22 successful methods and exposed two
new/migrated test-setup errors (two methods, two failed executions): the new
frozen-creature inspection fixture used a bare two-room actor without inspectable
creature state, and the migrated broadcast ownership assertion compared a retained
server Arc to an independently decoded wire Arc. The inspection witness now uses
the existing authored mob-arena fixture and still requires actual live damage;
the ownership witness retains the original server Arc and checks both pointer
stability and equality with the received picture. Neither correction changes
production behavior or weakens frozen-picture semantics. Reruns pending.

The corrected frozen-inspection and broadcast-ownership witnesses and original
text exchange subsequently passed; 69 unique focused Rust methods were green
before pre-PR cleanup. An environment interruption killed one remainder compile;
its orphaned verify lock caused a setup-only retry failure and was preserved and
cleared after confirming the owned tool session was gone. Neither is a failed
behavioral method.

Independent review then found a genuine pending-pause ownership regression:
fallible observer capture after `mem::take` could lose the current and unvisited
actors' required suspensions. The fix retains each obligation until suspension
and pause succeed, with a two-actor capture-failure/retry witness. This is an
audit-found defect, not credited as an already-executed test catch. The same
review found Observe flush could precede live reads, foreign outcome self-target
was not checked, and live client state could masquerade as a reset Snapshot.
Targeted causal fault witnesses accompany that coherent cleanup; no green rerun
is claimed until its focused receipt completes.

The first cleanup behavioral pass preserved 34 successful common-client methods,
including blocked-flush ordered delivery and foreign-self rejection. The newly
authored sequence-cancellation witness failed setup validation because it set
active preparation without its required remaining duration. Its positive fixture
now supplies that duration and validates OutcomeFacts before reduction. This is
one new-fixture method/one failed execution, not a production behavior defect.

The cleanup resume passed the corrected sequence fixture, seven text methods,
and 19 of 21 selected server methods. Two remaining setup/expectation failures
were not gameplay defects: a migrated cold test indexed a view cache that is now
legitimately lazy, and a new round-trip test expected Wizard shorthand spelling
instead of its established canonical JSON encoding. The former still requires
no stale disclosed plan in any retained cache and exact final knowledge/privacy;
the latter checks requested display basis and exact decoded saved operands.
These are two failed methods/executions, recorded separately from production bugs.

The two remaining cleanup expectation corrections passed. Across preserved
cleanup receipts, all 63 selected methods pass. The 69 earlier phase21 successes
remain separate evidence; overlapping methods are not double-counted as new
coverage. No release or performance qualification is implied by these checks.

### Demand-driven picture harness and compact semantic follow-up

The first real headless run exposed one stale driver contract: it treated a new
ready notification as evidence that the retained picture had also refreshed.
With separate outcomes and pictures, this compared the old center to the
completed move's destination. Count one stale driver root and one failed
actual-process execution, not a failed movement. The corrected driver waits for
both next readiness and a newer picture, then retains the exact destination
assertion. A fresh but wrong destination still fails. Restart equality compares
all disclosed contents and branch/history while excluding only attachment-local
picture identity and generation. Twenty pure driver tests passed; the second
actual-process run completed with save/restart equality. Its measured throughput
still missed the target and is not presented as a test success for performance.

The compact semantic-input follow-up passed four protocol methods and thirteen
server methods. Its independent review found an ownership regression before
release: canonical visual fallback was repeated per recipient rather than
shared per observing actor. This was audit-found, not credited as a test-caught
bug. The correction uses one transient per-actor picture pair; the existing
one/eight/thirty-two-watcher witness now checks zero adaptations for unchanged
pictures and exactly two for changed pictures, regardless of watcher count.
The corrected thirteen server methods and scoped Clippy passed. The protocol
methods were unaffected and reused, not rerun or counted twice.

One selector-registration error stopped a remainder dispatcher before running
methods; it was corrected from the built binary's actual test listing and reused
that binary. An interrupted executor and a later disk guard interrupted compiler
owners, not gameplay tests. Their incomplete receipts were retained; completed
checks were reused. Exact obsolete artifacts were reclaimed, with protected
binaries and hashes retained, before resuming only the unfinished target. These
are harness/infrastructure incidents, not additional stale gameplay test methods.

The documentation gate caught one genuine documentation defect in this report:
a guide cited a local-only receipt path. The published prose now contains the
measurements and limitations directly, rather than a nonportable evidence link.
This was one failed documentation method/execution, not a stale test failure.

### Current-main integration of causal presentation

The integrated no-link preflight caught one stale test assertion introduced by
combining the staff feature with the input-authority migration: the text staff
command test still constructed `Command::Act.expected_revision`, a removed field.
The text parser now returns the unchanged exact item/aim action; the connection
owns input authority. Removing only that obsolete expected field preserves all
staff capability, charge-independence and aim assertions. This is one stale-test
root/method and one failed preflight, not a production staff defect.

The recorder audit also exposed two remaining stale harness contracts from
separating causal outcomes and pictures. `act` could return a new lifecycle or
readiness frame with the old picture; a focused fake-stream first-red failed
at the intended old-versus-new picture assertions (two methods, five subtest
assertions). The harness now tracks the actual ordered visual-dirty signal and
finishes only already-demanded picture work; causal-only outcomes do not wait
for a picture or issue reset requests. Lifecycle and paid-pause selection remain
unchanged. Positive tests cover picture-before-lifecycle and no-queued replies.
Cross-attachment discovery/amulet comparisons now exclude only picture identity
and generation; all disclosed mechanics stay exact, with a negative comparator
witness for changed tick/health. These are stale harness expectations, not new
production movement or objective defects; actual recorder qualification follows.

The first integrated wire recording captured a real fragmented snapshot but
failed its required explicit-save cleanup under an artificial 512-byte frame
budget. Current contextual control replies can exceed that old budget (captured
admission envelopes were 554–557 bytes); deliberately undersized non-snapshot
frames disconnect by contract. Fresh and resumed-save probes both failed at512
and passed at1024. The recorder now uses1024, still requires an actual snapshot
part and asserts its encoded size stays within that limit. Production budgets,
backpressure behavior and cleanup requirements are unchanged. This is one stale
fixture-budget root and one failed recorder execution, with the separate bounded
probes retained—not evidence of a successfully recorded fixture yet.

The next recorder pass reached advancement and exposed one more caller of the
same stale-picture completion assumption: it explicitly waited for queued work
to leave admission, then immediately issued picture-bound inspection. It now
finishes already-demanded picture delivery before that inspection; the original
active-respec assertion and query remain unchanged. This is one additional stale
caller/method execution under the existing causal/picture harness root.

Recording then reached a Text device case whose pickup assertion expected the
old debug event label `Taken`, although committed outcomes now render canonical
prose (`You pick up ...`). The downstream exact inventory, tick, charge and RNG
assertions remain. A source audit batched the same obsolete pickup/wait labels
in affected process tests rather than discovering each through a separate run.
One actual recorder execution failed at this stale Text-label root; source-only
corrections elsewhere are not counted as additional test failures.

The recorder source audit found anonymous utility-event selectors still reading
visual deltas rather than committed Outcome combat facts. The focused pure
first-red had five intended assertion failures across ten selected methods;
the corrected selection passed all ten. Capture now retains the actual ordered
Outcome; identity, target, applied-result and hidden-ability rejection assertions
are unchanged, and visual payloads are explicitly rejected as event sources.
No production wire sample was synthesized. This is a stale event-source fixture
root detected before reaching that branch of the actual recorder.

The fourth recorder run passed earlier cases and reached growth diagnostics.
Its second inspection reused the wizard's old frozen picture: private growth
credit need not dirty that picture, so the report correctly lacked the newer
record. New diagnostic inspections now explicitly request a fresh wizard picture;
continuation paging is untouched. Exact effect, build, held-XP and RNG assertions
remain. The same four recorder diagnostic entry points were corrected together.
One actual growth-case execution failed at this stale inspection-basis root.

The fifth recorder pass reached Detection but the unchanged 8 GiB disk guard
stopped it after repeated background map pages produced roughly540 MiB of full
headless diagnostic-state logs. This is an infrastructure interruption, not a
failed gameplay method. The Detection clients now opt into existing compact
background diagnostics: raw messages and hydration status remain exact, while
repeated derived-state copies are omitted. After hydration, a local Inspect
provides the full memory for the unchanged content assertion, without a server
Snapshot/reset. A focused helper witness verifies both compact and already-full
paths and forbids an unnecessary request on the latter. Failed logs are retained
in hash-verified compressed archives; no production budget or workload is reduced.

A second recording attempt reached the same disk guard despite compact background
diagnostics; full demanded pictures still made the large Detection lifecycle log
substantial. Both interruptions are one resource-headroom root cause, not two
gameplay failures. Completed release-example executables and failed logs were
hash-verified into compressed archives to provide sufficient recording headroom.

The final recorder completed after headroom recovery; no production thresholds
were changed. The archive writer's initial local selector used a source-file
module name rather than its registered `storage::schema::scenario` path. The
nonempty-test guard rejected the zero-test run. The compiled binary was reused
with its hash and actual `--list` paths verified; seven exact checks then passed.
This is a local driver-selection error, not a production defect or a passing test.
Read-only audit also found two benchmark examples selecting an older wire fixture;
both now select the current fixture and their existing contract tests pass.

### Performance PR hosted 3D formatting gate

The first independent 3D CI run rejected rustfmt layout in two migrated UI
files before compiling or executing behavior tests. This is one formatting
root cause, not a gameplay defect or stale behavioral oracle; platform/profile
repeats are not additional defects. Rustfmt corrected only whitespace in
`interact.rs` and `screen.rs`. Both workspace formatting checks passed locally;
behavioral evidence is reused for identical non-whitespace source, while hosted
native 3D qualification must still run on the corrected head.

### Performance PR causal/display migration: hosted feedback

The first behavioral matrix for [PR #203](https://github.com/KCall360/thresholds-of-ruin/pull/203)
exposed both real client defects and incomplete fixture migration. This is an
ongoing correction window, not a green final-head qualification. Repeated
Windows/Linux and debug/release failures are not additional root causes.

Confirmed client defects include automatic ASCII running choosing its next step
before character readiness, terminal ASCII input being admitted between the
ordered terminal outcome and the later readiness update, and scripted Text
commands returning at causal completion before their demanded picture arrived.
The last case reproduced as successful pickup narration followed by an empty
inventory. An explicit Observe process check also received its picture but timed
out waiting for a correlated reply: picture-only completion was not recognized
by the generic pending-request path. These four roots remain distinct from test
maintenance; their exact safety assertions and process witnesses are retained.

Fixture maintenance includes state annotations still encoding a revision instead
of a snapshot capability, cross-attachment comparisons treating local picture
identities as global identities, and callers expecting a causal-only Wait to
produce an automatic picture. Payload comparisons preserve every disclosed field
and omit only the top-level snapshot/revision identities across independent
attachments. Independent review restored an exact same-picture supply-picker
assertion. The 220 audited comparison sites are source sites, **not** 220 observed
failed tests. A helper requiring fresh post-action inspection requests an ordinary
Observe; raw command delivery and transport lifecycle tests keep their distinct
asynchronous contract.

Maintained direct-Engine benchmark producers also used an internal unissued
picture revision as a change signal: physics emitted zero apply/draw samples for
864 commands across 24 rows; combat did so for 192 commands across three rows.
Item/place/latency additionally retained the old combined event/picture envelope,
and direct place setup incorrectly invoked a display-capability resolver without
Service validation. These are five producer subcases in three failing Python
methods, not five methods. Producer migration must retain report bounds and
separate outcome timing from picture timing rather than rename their sum.

The earned-route driver inspected advancement using a pre-settlement picture.
Its corrected accounting boundary requests one nonresetting Observe, with pure
regressions preserving paid-action counts and error propagation. A subsequent
local process attempt timed out during scenario validation before route execution;
that setup failure is not evidence of either a route regression or a passing fix.
Final correction results and additional diagnosed roots will be recorded when
this bounded batch has completed.

Reusing the immutable, already-built Text and protocol test binaries exposed
later failures that the hosted Cargo run had not reached after ASCII failed.
Text reported 178/199 passing and protocol integration 274/293; protocol library
58/58 passed. These are historical-snapshot diagnostic results, not qualification
of the corrections. The 19 protocol failures have three stale fixture roots:
17 failing methods shared two reply-context constructors that omitted the required
snapshot, one field-list oracle
still counted five fields, and one malformed-delta test accidentally changed its
snapshot base before reaching the intended payload-validation failure.

The Text failures revealed two further genuine roots: converting a noticed actor
to generic discovery lost typed newcomer identity/direction (and therefore
inter-leg caution), and starting play without a welcome call left the first HP
loss without an initialized accounting baseline. Event-time disclosed positions
are retained rather than reconstructed from a later picture; the exact newcomer
stop and two-loss overflow-safe accounting assertions remain. Autonomous Text
chains also confused queue admission with character readiness, the same semantic
readiness root as ASCII autorun above. A shared causal readiness value follows
ordered pacing facts independently of an older picture. Other Text fixtures
still combined events with pictures or authored obsolete snapshot identities;
those are corrected separately from the preserved production safety assertions.

The bounded correction run passed all 160 ASCII tests and all 294 protocol
integration plus 60 protocol-library tests. Common-client tests reached 177/181:
two synthetic `Snapshot.state` constructors omitted `StateView.snapshot`, affecting
four methods (not four production roots). Text reached 197/199: the remaining
two exact total-request counts included a legitimate read-only Observe. Their
revised oracles distinguish the precise picture demand from the unchanged paid
journey/attack/pickup sequence and preserve refusal/no-retry assertions.

The historical server diagnostic was interrupted after 245 passing and one
failing test to avoid spending the local checkpoint on unrelated expensive
floor/recovery cases; the remaining 1,022 were not run. Its one failure was rerun
alone: the roaming-work oracle expected one scene construction, while the retained
optimization performs zero. The corrected oracle still checks the exact actor and
move, no route search, and bounded personal-history copying. The partial suite is
not a pass. A later local lint failure caught an unnecessary unwrap in the migrated
latency producer before application linking; the direct pattern match replaced it.

The existing genuine wire recording contained no Noticed fact. A bounded ordinary
door-reveal process captured the new event-time position from the actual server
in two seconds; the prior 73 samples remain unchanged and this one raw sample was
appended. This is a targeted additional recording, not a claim that all recording
scenarios ran again. The new strict protocol sample regression passes.

The completed focused client/protocol correction owners now pass 894 tests
(181 common, 160 ASCII, 199 Text, 294 protocol integration and 60 protocol library).
All five repaired producer smoke/selected cases execute and pass their existing
report validators: physics 8 rows, combat 3, items 20, places 101 and latency 236.
These development-profile checks qualify report/behavior contracts, not new
performance claims. Three of seven actual-process methods passed immediately;
the four remaining methods exposed narrower stale caller expectations: an opaque
unknown snapshot expects StaleContext, a measured attack must demand its picture
before comparing a later annotation response, place spectators must receive exact
public history rather than wizard-private setup entries, and Text Wait narration
uses its canonical sentence rather than the old debug event name. Both place
variants are subcases of one method. The subsequent corrections keep exact
history, causal timing, paid-action and payload assertions.

A read-only audit also found additional server fixture omissions before another
hosted round: three required snapshot contexts, five comparisons between separately
issued pictures, one comparison mixing private causal and display revisions, and
one equipment-projection check still expecting an unsolicited picture. These are
source-identified corrections, not additional observed failed-method executions.
The equipment check explicitly demands its picture and retains private-curse
redaction checks on both causal facts and displayed fields; it does not require an
otherwise empty private event to be broadcast.


The final selected server checks passed 13 methods across the bounded correction
runs. One exposed a genuine chart-ownership regression: first-page preparation
repeated the already-owned current-boundary census. Reusing the exact current
Game/branch view restores the original Arc-identity oracle without another cache.
A historical first transfer reconstructs from its frozen boundary and owns its
continuation; the regression also checks exact historical charts and unchanged
checkpoint/RNG/knowledge state after the world advances.

All seven targeted process methods above now pass. The saved-exploration retry
also exposed a stale execution-time oracle: a Wait history entry records its
execution/start tick, not its next-ready tick. The replacement binds a new own
Wait receipt by actor, branch, event and exact execution tick, rejects old history,
and preserves exact restart payload/history equality and Text narration order.
This is one further stale driver root, not a production regression. The final
process checks used the previously frozen application binaries; the later pure
chart-ownership fix was qualified by its rebuilt server test owner, not relabeled
as part of those application artifacts. Hosted CI builds the exact final tree.

Resource-guard interruptions during local linking/freezing are infrastructure
failures, not test defects or passing checkpoints. Completed application hashes
were verified after bounded reversible archive cleanup; successful evidence was
preserved rather than rerunning unaffected owners. No guard threshold was lowered.

### PR203 second hosted correction cohort (2026-10-29)

The first correction head's exhaustive Linux server owner reported 1,222 passes
and 50 failures. A bounded local correction run subsequently passed 57 selected
methods and left 18 failures; those successful methods are retained rather than
repeated wholesale. Four of the remaining failures exposed a genuine missing
wrong-branch guard on private queries. The guard belongs after exact retry
recognition and before query dispatch; its existing negative tests and the
exact-once retry regression qualify that ordering. Thirteen remaining methods
still supplied causal revisions to a display-bound inspection target, and one
used a retained valid picture as an invalid-picture fixture. Those are two stale
fixture roots, not fourteen production bugs.

The full process partition reported 105 failures and three errors across 709
executed methods. These are execution counts, not independent defect counts.
Additional genuine client roots identified by those traces are: opening a new
private advancement Overview from a retained picture rather than explicitly
refreshing it; routine own movement being narrated as a caution and stopping
ASCII directional running; and an unnamed newly noticed figure producing empty
Text narration. The correction retains exact attachment/control binding during
Overview refresh and does not refresh already pinned pages or confirmations.

Stale driver roots include comparing newly issued picture capabilities across
restart, reading combat outcomes only from pictures, expecting old debug event
labels instead of semantic narration, and treating causal completion as a fresh
picture. Native input completion now checks the client's disclosed picture-current
flag and authoritative readiness, including frames arriving during key release.
The earned-route restart check excludes only the new attachment's snapshot and
display-generation identities; payload, tick and history remain exact, with
negative mutation witnesses. Dedicated resource probes explicitly hold an
external actor boundary to inspect paid preparation. The shipped autonomous arena
is not changed: its intermediate Falter payment oracle remains in the existing
paused, same-build preparation test, while ordinary execution retains its final
payment and outcome checks.

These corrections are pending final rebuilt process qualification; source review
and pure helper passes do not constitute a green application run. Native physical
key qualification also requires the xdotool dependency; an unavailable dependency
is an infrastructure/setup failure, not a behavior pass or a new waiver.

The final remaining diagnostic case was initially attributed to its old picture,
but failed again after an explicit Observe. That correction disproved the fixture
hypothesis: runtime capture settings/records were projected from a frozen gameplay
boundary even though capture configuration is deliberately not journaled gameplay.
The fix validates the issued capability but reads the current private diagnostic
owner, matching capture controls. The existing regression retains the same picture
across capture/inspect, checks invalid metadata and unauthorized recipients, and
asserts unchanged gameplay state/history. This is one genuine production root,
not another stale-test count. The affected client owner checks passed 642 tests;
this final server correction still requires its own rebuilt witness.

The runtime diagnostic regression then passed, and the ordinary applications were
rebuilt from that corrected source. Eighteen of nineteen representative process
methods now pass, including the paid resource/fear/Falter witnesses, owner
advancement refresh, cancellation, ordered strikes, vampire loot, fruit, and the
Amulet route within its unchanged output budget. Three narrow correction mistakes
were caught and fixed without rebuilding: diagnostic memory revisions are numeric
rather than wire decimal strings; the diagnostic observation-base repeats the
independently issued picture capability; and consuming the last fruit retains the
legacy generic start label followed by its exact named completion. A vampire
purchase-preview equality also needed the same issued-picture distinction.
These are stale fixture/driver corrections, not additional production defects.
The remaining earned-route local run stops in candidate validation at the existing
30-second setup limit with the development opt0 scenario tool. No route behavior
was qualified by that timeout, and no limit was increased.

The next hosted run passed the server library and reached the previously blocked
integration owner: 234 methods passed and 36 failed. Source triage found additional
fixture migrations, not 36 new production defects: trusted direct-Engine helpers
did not bind the new display-operation variant to a causal boundary; transport
fixtures expected eager pictures instead of ordered Outcomes; reset comparisons
included issued picture identities; a note retry reconstructed a different anchor;
and direct performance drivers used the deliberately unissued display sentinel as
an advance counter. Corrections preserve exact event ordering, attachment-local
delta bases, original retry anchors, payload, hidden-observer disclosure, payment,
and causal advancement. Physics now separately asserts no eager picture work and
actual disclosure on demand. These integration corrections are pending their
focused owner run; the prior passing library/client/application evidence remains
valid for unchanged production code.

The next process triage identified additional genuine narration regressions:
ordinary ASCII HP/preparation updates became caution messages; Text retained its
HP/name baseline across a rewind Snapshot; Text no longer used legacy noun
fallbacks and classified refusal sentences as conditions; and observer door facts
omitted their already-authorized event-time direction. The semantic projection
audit also found missing disclosed actor assets (used by Text's palette fallback)
and coalesced ASCII underfoot prose. These are production roots, not stale-test
counts; the correction preserves the disclosed event-time facts rather than
looking them up in a later picture. Focused regressions and rebuilt qualification
are pending for this coherent correction.

Windows release's earned-route execution reached its final comparison after
1,257 seconds and differed only in the independently issued snapshot capability;
Linux likewise differed in picture identity fields. The comparison now excludes
only top-level snapshot/display-generation identities, retaining exact payload,
causal history, gross flows and source-free restore. Sixty-one pure comparison
checks pass; this does not substitute for the next final-head hosted route run.

The focused correction checkpoint passed all 96 selected protocol/client methods
and 269 of 270 server integration methods. The remaining integration failure was
one stale palette expectation: it required one combined delta where event-time
asset dependencies now require additions, the ordered Outcome, then removals.
The replacement checks both exact deltas and the intervening event for player and
spectator, rather than ignoring extra messages. A newly authored palette test
also used an untyped TOML mutation on a typed Archetype, caught at compilation;
this is test-implementation maintenance, not a gameplay defect. Its corrected
fixture uses the existing concealed dual-use archetype rule: globally forecast
item-pool assets do not imply that the actor's regional asset is already present.

The completed correction checkpoint qualified all 270 server integration methods,
the compact/full projection oracle, both palette Service fault/order cases, and
the 96 selected protocol/client methods. The genuine wire-66 recording was
regenerated with the corrected event-time fields. Four additional stale process
assertions were corrected: one restart compared a session-local display revision,
one wizard setup command lacked a fresh observation, one required two transport
frames for two correctly ordered strikes in a single Outcome, and one expected
obsolete readiness prose. Their four actual-process regressions now pass.

Local lossless segmented capture exposed a harness qualification limitation:
raw-file byte budgets do not count archived segments. Such runs establish behavior,
not those byte budgets. The wraith witness was rerun with ordinary raw capture and
its existing compact-background diagnostic option (all wire messages and gameplay
frames retained). It passed the unchanged 96 MiB output bound with 85,917,443 bytes,
249 requests and 10,828 actor ticks, preserving the survived drain and source-free
restart. Hosted ordinary-capture checks remain required for the other budgeted
process cases. Storage-guard interruptions and a mistyped pure-test selector were
setup failures, not production defects or passing behavioral checks.

The regenerated recording's existing Noticed golden test also required updating:
the new foreign-door witness legitimately records a second reveal at the same
exact disclosed offset. Its replacement checks both reveals and the exact new
wooden-door name, open state and observer-relative position; it does not drop the
cardinality or position assertion. The other 17 recorded-sample tests passed
before this isolated expectation correction.

The next hosted Linux debug Rust run passed 2,264 simulation unit tests and exposed
two additional stale work-counter assertions. One still interpreted the historical
`actor_table_copies` field as whole-map detaches rather than per-actor payload
copies; its replacement checks both payload copies and ordered table-entry copies,
while retaining old-snapshot, injury and unrelated-creature assertions. The other
expected repeated classification on an unchanged warm observation; its replacement
checks exact cold counts, zero additional warm classifications, identical disclosed
views, and renewed classification after an actor is added. These are two stale-test
roots/methods, not evidence of gameplay or privacy regressions.
The same two failures reproduced in hosted Linux release (2,264 other simulation
unit tests passed). Both corrected counter contracts passed the focused local
owner; the unchanged production binaries and previously qualified behavior
checks were not rebuilt or relabeled.

The same hosted correction exposed three additional pure-fixture roots: the owned-HD
wizard double still accepted only Snapshot where setup now explicitly requires
Observe; the active-mob scripted binary bundle omitted Windows executable suffixes;
and the combat driver double put completion receipts in picture events rather
than ordered Outcomes. These account for six existing failed methods (one wizard,
one driver and four combat methods), with repeated platform executions and combat
subtests not counted again. Nine focused wizard/driver checks and eight combat
checks pass, including both platform path branches and a negative picture-only
completion witness. Production behavior and hash/identity/deadline checks were
not relaxed.

The real ASCII return-journey assertion also read a picture before its refresh:
authoritative arrival correctly preceded the new displayed tick. Its replacement
uses the outbound journey's existing current-picture barrier and retains the exact
return tick. The door rewind witness likewise relied on display pace to keep
travel unfinished; its replacement holds the Text client's arrival receipt while
an independent spectator verifies completed travel and a still-closed door, then
rewinds before releasing the pending door goal. These are two stale process-test
methods, pending focused actual-process qualification; no additional production
root is established by these timing failures.
Both actual-process replacements now pass on the unchanged qualified applications.
The first travel rerun additionally exposed a fixture-owned transport deadlock:
it held the command acknowledgement, while automation intentionally waits for
acknowledgement before reading another key. The relay now delivers Active and the
acknowledgement before holding later steps, preserving the Skip control, seven
paid steps, ordered history and source-free restart checks. This is maintenance
within the same travel method, not a production defect or another failed method.
The door witness drains each completed prompt explicitly before its query barrier.

The following hosted Linux debug run passed all 1,274 server unit, 270 server
integration, 2,266 simulation unit and 815 simulation integration tests, then
reached twelve failing test-support performance methods. All twelve stopped while
decoding one shared synthetic view that omitted the now-required snapshot identity;
this is one stale fixture root affecting twelve methods, not twelve engine bugs.
The repaired fixture adds the identity and a required-field rejection witness.
The forbidden-acceptance mutation also advances its revision so it reaches the
intended acceptance guard rather than the independent progress guard first.
All thirteen focused performance-oracle tests pass, including their exact expected
failure reasons. No production oracle or gameplay logic changed.

The same hosted round's Linux release Python lane exposed one further stale
private-report caller: the helper refreshed before creature inspection but not
combat inspection after causal-only actions. It now explicitly observes before
either private query, without retrying a rejected command. The failing fear,
disadvantage, immunity and source-free restart witness and the outgoing-policy
restart sibling both pass on unchanged qualified applications. Count one further
stale method/root; none of its RNG, payment, trace, privacy or restart assertions
were removed. Windows release Python shard 2 completed successfully on the
published test-correction head, including the preceding travel and combat fixtures.

The Linux debug Python lane also exhausted its 40-minute job budget without a
failed test: its exact receipt records 2,049 passed of 2,395 selected, leaving 346
unexecuted. This is incomplete infrastructure qualification, not a test pass or a
new gameplay defect. The two longest completed witnesses took 1,251 and 714
seconds. Linux debug now uses two existing module-owned shards, placing those
witnesses in separate owners while retaining the same full inventory, original
per-test limits and 40-minute job cap. The aggregate independently checks exact
coverage, source/run/attempt bindings and both compiler identities; Windows and
Linux release coverage are unchanged.

All 161 owning orchestration methods qualified across focused receipts. One stale
workflow-name expectation was updated for the explicit shard owners. Five CLI
plan subcases initially lacked the actual local Rust toolchain environment; the
two affected methods passed after supplying it, without compilation. These setup
failures do not count as production detections. Independent review caught and
corrected two new orchestration defects before publication: the seal's strict
output-key set needed the shard-attempt fields, and each shard needed a distinct
compiler role instead of a last-writer matrix output. Negative regressions retain
both safeguards. Actionlint, Ruff and the focused coverage/attempt/seal tests pass;
final hosted exhaustive qualification remains required.

Once the full Rust suites passed, their downstream production Service catalog
owner exposed seven stale methods across three contract migrations: two still
mutated removed action display revisions instead of input authority generations;
four compared newly issued picture identities/generations as immutable payload;
and one expected an unsolicited hallucination picture instead of causal visual
invalidation followed by display demand. These are three fixture roots affecting
seven methods, not gameplay defects. All 29 Service methods now pass against the
exact Cargo-produced current host, including exact stale/wrong-branch errors,
hidden-hold retry/no-retarget behavior, complete disclosed payload comparisons,
private discovery and exact hallucination phase samples for owner and follower.
No production code or privacy/replay assertion was relaxed.

Linux release's projectile process exposed one additional stale decision fixture:
its headless peer had settled equipment, but Text still held the older picture
when a new launch menu began. Retained stdout proves the client correctly refused
the changed selection without sending an action; the fixture's shortbow substring
accidentally matched preceding equipment narration. An explicit sync before the
new menu verifies one real snapshot and its complete disclosed payload, then keeps
the exact page count, expertise, payment, recovery and source-free restart checks.
The full affected process passes. This adds one stale method/root, not a transport
or gameplay defect. Both new Linux debug Python shard jobs also completed
successfully; aggregate exact-coverage proof and final-head CI remain separate gates.

Windows debug's native bump fixture reached and verified native input, recovery
and unchanged resource balances, then failed its raw private `combat inspect`
query with `StaleContext`. The fixture now uses the existing explicit pre-query
Observe helper, without retrying an action or weakening the exact owned-melee
trace and narration checks. A direct-caller audit found four equivalent new-report
openings in natural-attack, equipped-weapon, replay and data-only ruleset tests;
those now refresh too, while backward paging and deliberate stale-capability
queries retain their existing basis. This is one repeated stale-fixture root,
one observed failing method and four proactively corrected methods, not a new
gameplay defect. All four affected non-native processes pass (22.816 seconds),
as does the helper's refresh-once/no-retry regression. The exact native case
still requires hosted qualification: local X11 lacks xdotool; no local native
pass or waiver is claimed. Production source and binaries are unchanged.

The next hosted run exposed two timing-dependent stale fixtures. Generated-caves
spectating waited for the final move's intermediate started-picture tick, which
an independent native client may coalesce. The helper now waits for a stable
ready/no-pending-action boundary before exact spectator, rewind and restart
observation comparisons. The data-only condition test queried private creature
state after a peer action without refreshing; it now uses the existing fresh-query
helper. These are two observed failing methods/fixture roots, not gameplay errors.
A broader audit also corrected four analogous inspection openings (one unavailable
target negative, two Text inspections and one native inspection after external
expiry); pinned report paging and intentional stale-context negatives are intact.

The affected generated-caves graphical process, data-only condition/restart,
frozen/detached unavailable-target, private paid-preparation and status-protection
processes pass locally. One combined run preserved three successful methods before
the disk guard interrupted its fourth; only that unfinished method was rerun after
hash-verified reversible compression of two superseded server rlib cache objects
reclaimed 659,116,032 physical bytes. This resource interruption is not a test
pass or production detection. The native physical-key status case remains a hosted
gate because local xdotool is absent. No production source or time limits changed.
