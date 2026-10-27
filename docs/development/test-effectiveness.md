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
