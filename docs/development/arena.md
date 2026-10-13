# Arena development and evaluation

`scenarios/mob-arena` provides a repeatable encounter using ordinary creature
builds, AI, abilities and combat. All four participants have three hit dice: the
selected blue adept combines Warrior and Mage HD, its blue sentinel ally
is a Warrior, and the red team has a Warrior and a Mage. The adept starts with
Spellcasting and Intimidation, buys a Heavy Weaponry rank, Magic Bolt and Fear;
`stats` shows its skills, talents and resource pools.
Use the normal Text ability commands or the native ASCII ability menu to play.

```sh
cargo run -p tor-server --bin tor-server -- --scenario scenarios/mob-arena --seed 42 --save saves/mob-arena-42.db
```

Start with [development setup](setup.md). Build `tor-server`,
`tor-scenario`, `tor-arena` and any clients you use; choose release binaries for
measurements. Uppercase command arguments below are local paths to replace, not
literal filenames. Authentication and irreversible wizard marking follow the
[wizard guide](wizard.md).

Use a fresh save for each changed definition or seed: resume retains pinned
inputs. Revalidate packages after source edits. Authoring creature builds, melee
and equipment belongs to [scenario packages](../reference/scenario-packages.md),
[creature rules](../reference/creature-rules.md) and [items](../reference/items.md).

## Health rest-to-full baseline

The focused `health_rest_to_full_baseline_measures_seeded_warrior_hd_bands` test
measures the production recovery kernel with unmodified Warrior builds at
HD 1, 6, 11, 16, 21 and 26. Each band uses seeds 0–31, all six attributes set
to 1, no equipment/talents/conditions, normal nutrition, and starts at 1 Health.
It advances to each real recovery deadline and records total active ticks to full
Health. It reports maximum-Health median and tick p50/p95/max (nearest-rank
reporting for the 32 samples).

```sh
cargo test -p tor-simulation --test it health_rest_to_full_baseline -- --nocapture
```

The initial measured candidate gives these active-tick distributions (32 seeds
per row). The first Warrior HD is maximized by the ordinary Health rules, hence
HD 1 has no seed variation. These are gameplay tick counts, not wall-clock timings.

| HD | Period | Maximum Health p50 | Rest p50 | Rest p95 | Rest max |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 1200 | 11 | 12000 | 12000 | 12000 |
| 6 | 1000 | 37 | 36000 | 49000 | 49000 |
| 11 | 800 | 67 | 52800 | 64000 | 67200 |
| 16 | 600 | 89 | 52800 | 66000 | 68400 |
| 21 | 400 | 120 | 47600 | 55200 | 56400 |
| 26 | 200 | 149 | 29600 | 34400 | 34800 |

Although points recover faster with HD, increasing maximum Health makes total
rest longest in the middle bands. This baseline does not yet justify changing
any one curve constant; ordinary encounter and nutrition evidence are still needed.

The candidate period curve is provisional. This microbaseline isolates recovery;
it does not establish encounter balance, food pressure, or numerical parity with
another game's combat distribution. Keep arena encounter evidence separate.

Arena participant summaries include authoritative initial/final injury,
`damage_received`/`damage_dealt` for all resolved effects, explicit
`healing_received`, and natural `health_recovered`. Natural recovery is derived
from injury conservation, without persistent gameplay counters:
`initial injury + damage = final injury + explicit healing + natural recovery`.
Maximum-Health changes cannot count as healing. Counts use applied effects,
excluding immunity and overkill; item/environmental outcomes count even without
an ability identity. Final Health remains an outcome measure, not a proxy for
cumulative damage.

### Controlled ordinary recovery encounters

A controlled 512-encounter sample on the integrated 3d6/point-ledger rules used
the `strength_heavy` and `mage_intellect` recipes at HD 1, 4, 8 and 16, seeds
0–15 and both orientations. Both variants used each family's candidate recipe,
against the same Warrior opponent. Unlike the stock matrix's Warrior baseline,
this isolates recovery rather than changing the Mage build at the same time.

Only the three Health-curve scalars changed: the control used base/minimum
1000000 and reduction 0, above the 100000-tick encounter bound; the candidate
used base 1200, reduction 40 and minimum 200. Each encounter's two actors shared
its variant. Defense 10, attributes, point purchases, abilities, AI, equipment
and action timings were identical between variants. All 32 packages passed the
ordinary compiler; all 256 initial participant-state pairs matched exactly.

Every encounter ended by elimination, with no failures, caps or stalemates.
All 512 full reports matched exact replay and satisfied injury conservation.
Both variants recovered **zero Health in combat** in this sample. Their paired
win difference was 0 at every band; ticks, actions and applied-damage totals
were identical. The following distributions therefore describe either variant;
each row has 32 observations (16 seeds × 2 orientations), with nearest-rank
quantiles. Orientations are not independent seed replicates.

| Family | HD | Active ticks p50 / p95 / max | Actions p50 / p95 / max |
| --- | ---: | ---: | ---: |
| strength_heavy | 1 | 502 / 602 / 602 | 13 / 15 / 15 |
| strength_heavy | 4 | 747 / 1683 / 1683 | 23 / 52 / 52 |
| strength_heavy | 8 | 805 / 1409 / 1409 | 33 / 59 / 59 |
| strength_heavy | 16 | 1180 / 2180 / 2180 | 48 / 90 / 90 |
| mage_intellect | 1 | 450 / 534 / 534 | 9 / 12 / 12 |
| mage_intellect | 4 | 587 / 731 / 731 | 13 / 18 / 18 |
| mage_intellect | 8 | 564 / 690 / 690 | 14 / 19 / 19 |
| mage_intellect | 16 | 580 / 680 / 680 | 15 / 19 / 19 |

This negative observation does not justify changing the curve or claim it is
balanced. Combat duration and ordinary damage resets did not expose recovery;
the rest-to-full baseline above still demonstrates substantial between-fight
cost. Nutrition, interrupted rest, new talents and full-floor resource pressure
need separate calibration. The one-tick mechanism fixture proves accounting,
not balance. This is neither a d20 comparison nor a PC-only recovery buff.

To reproduce the control, generate the family/HD subset with the curated matrix
below and seeds 0–15. For each case, copy each candidate-orientation package to
its baseline orientation, preserving the family build and placements. Change
only the baseline resolved `health_recovery` scalars to the control values above
and revalidate all four packages. Run and replay the existing paired plan.
Retain the exact compiler/binary fingerprints, resolved-rule differences and
matching initial states with the reports. The original stock Mage baseline
cannot substitute for this same-build control.

## Live encounter controls

The manifest's `arena` setting names participant IDs, `control = "manual"` or
`"all_ai"`, and tick/action limits. Manual control waits for the selected actor's
input; surviving arena AI continues after its death. All-AI control also uses
the selected character's declared AI profile. This package declares that profile
already, so changing `control` to `"all_ai"` and revalidating enables unattended
play. Team elimination can stop an encounter before its bounds. Limits cannot
exceed 100000 ticks or 10000 committed actions; free admissions do not count.

For controlled experiments, set `start_paused = true` in the manifest's `arena`
section and revalidate. With server wizard mode enabled, a wizard can issue
`arena pause`, `arena resume`, or `arena step [count]`. The default step count is
one; counts must be between 1 and 10000 and fit the encounter's remaining action
budget. Players and spectators cannot issue these controls.

A step permits committed actions through the ordinary simulation queue. It counts
an action's start or resume, rather than elapsed ticks or effect resolutions.
After the last permitted action, the arena freezes before advancing to another
decision. A manual participant can still require input before a step completes.
Pause preserves queued intentions and paid preparations, including their reserved
finish costs; snapshots do not advance time. Restart preserves the arena pause
state and active AI preparations without charging another start cost. Resume
continues ordinary scheduling until the encounter stops or is paused again.
Control commands retain private wizard receipts while clients receive ordinary
readiness updates.


## Controlled editing

Latest-HD removal uses the ordinary creature rebuild rules: injury is retained,
owned point purchases and attribute choices are removed, and losing an ability
grant cancels its preparation without refunding charges already paid. The edit
pauses an active arena and rejects changes to a stopped encounter. Exact retries
do not remove another hit die. Rewind can restore the earlier build and death
state. See [creature arena authoring](../reference/scenario-packages.md#creature-builds-and-arenas) for the
bundled encounter, bounds and scenario setup.

Template controls use the current package's validated catalog; JSON commands cannot
supply replacement definitions or forged grants. Unknown templates, already-applied
or absent templates, and conflicting structural changes reject atomically. Removing
a template removes only its source-owned contributions: a Mage retains its separate
class-provided magical capability. A lost ability grant cancels its preparation;
capacity reductions release unfunded holds and clamp balances. Increasing capacity
again does not refill it. Template edits preserve injury, inventory and sockets,
pause active arenas, and reject stopped encounters. Exact retries, restart and
rewind preserve the same build and resource accounting.

Advancement commands number hit dice from **1**, in the order shown by `stats`.
Racial and class dice each provisionally grant one shared advancement point in
the default rules; a rank and a talent compete for that point. Creation records
two distinct starting ranks separately. Only dice 4, 8, 12 and so on own an
attribute opportunity, with the ordinary attribute cap of five. An older unspent
point can select from the currently eligible build pool. Temporary benefits
cannot qualify a talent. Unknown names, invalid owners, overspending and
ineligible purchases reject atomically. Adding a die preserves retained Health
seeds and choices; its seed comes from the original spawn stream, independently
of combat randomness. Regrowing a drained ordinal uses its original seed and
follows the explicit `choose_again` or `authored` restoration policy. Starting
ranks survive drain and are never granted again. See the authoritative
[point ledger](../reference/creature-rules.md#advancement-points-and-starting-ranks).

Advancement preserves injury, does not refill enlarged resource pools, and never
revives a dead actor. Removing a prerequisite class level leaves a retained talent
choice dormant; restoring the requirement reactivates it. These commands share the
arena pause, stopped-result protection, retry, restart and rewind behavior of
latest-HD removal and template changes.

## Private combat capture and export

Combat capture starts disabled, retains the latest 64 creature combat completions,
and records actual dice, check operands, edge allocation, ordered damage protection,
Fear, preparation timing, costs and before/after states. Misses have no damage rolls.
The report identifies dropped records and truncated traces. To page backwards, use
`wizard combat inspect <first sequence minus one>` while that sequence is retained.
The query and capture controls require an attached wizard and current context,
branch and observation revision, but do not require control or advance time.
Reports are private to the requester; Text prints them, ASCII opens the scrollable
report panel, and headless preserves the structured response for export. Capture
policy survives in-memory rewind and resets to disabled when a save is reopened.

To export a diagnostic window, pause the arena, query `wizard combat inspect`,
then follow every older-page cursor printed by the report until all retained
records are collected. Preserve the headless client's JSON-lines output before
rewinding, reopening the save or changing capture. The local helper assembles the
last unchanged reply window and rejects missing pages, conflicting duplicates or
mixed contexts:

```powershell
python scripts/combat_diagnostics.py export HEADLESS_JSONL COMBAT_JSON
python scripts/combat_diagnostics.py compare BASELINE_JSON REPLAY_JSON
```

Replace the uppercase placeholders with your local input and output paths.
The export retains all original receipt IDs and includes integrity and numerical
SHA-256 hashes. Numerical comparison normalizes only the two payment-owner IDs,
retains whether each is present, and compares every other captured field. Thus
fresh owners after rewind can reproduce the same numerical result. Dropped records
and truncated traces stay explicit; this exports the retained window rather than
recovering records already evicted. The helper consumes protocol-validated headless
reports and adds window/integrity checks; hashes do not authenticate their author.
Exports and transcripts are local analysis artifacts and stay outside Git.

## Offline evaluation

For a fresh offline arena evaluation, run the `tor-arena` binary (default seed 42) with
`--scenario PATH --seed N --all-ai`. It resolves all-AI/unpaused control in memory,
revalidates that resolved package and uses ordinary Engine admission/execution.
The authored source and existing saves are untouched. Without `--all-ai`, the
package must already declare unpaused all-AI control.

The JSON report includes the seed, selected character, resolved model hash,
streaming settings, combined input hash, ticks/actions, explicit elimination or
limit/stalemate reason, participant health/resources before and after, attack
checks/hits and ability resolution/application counts. `actions` counts committed
AI decisions. Each ability's `resolutions` counts primary-strike outcomes, including
a no-roll unavailable-target miss, and `applications` counts those primaries that
applied a change. A three-strike ability therefore has one admission but up to
three resolutions; its riders add neither ability resolutions nor applications.
`attack_checks` counts actual primary attacker-side checks and `hits` counts their
successful rolls. Target-side checks and rider checks are excluded from that
primary denominator. Rider damage and healing still contribute to effect totals;
sequence-terminal events add no outcome counts. Damage counters count
combat-resolution events; environmental changes are reflected in health states.
Paired matrix/search scoring remains based on encounter outcomes and seeds, not
ability-use or strike counts.
Wide counters use decimal strings. Invalid input or execution fails with exit 2
and a structured failure report.

## Paired batches and replay

For paired batches, use `python scripts/arena_evaluation.py --binary TOR_ARENA run
PLAN_JSON OUTPUT_DIRECTORY`. The JSON plan supplies `baseline` and `candidate`,
each with `forward` and `mirrored` package paths, a `seeds` array of canonical
unsigned decimal strings, and the `faction` whose win rate is compared. Paths are
relative to the plan file. Optional `timeout_seconds` defaults to 60 per run.
For example:

```json
{
  "baseline": {"forward": "baseline", "mirrored": "baseline-mirrored"},
  "candidate": {"forward": "candidate", "mirrored": "candidate-mirrored"},
  "seeds": ["42", "43"],
  "faction": "blue"
}
```

Author the mirrored packages explicitly: exchange starting sides while retaining
actor IDs, factions, builds, and encounter conditions. The harness requires
separate forward/mirrored inputs and matching actor/faction rosters; it does not
infer a geometric reflection through arbitrary portals or terrain. Review those
placements as part of the encounter. Baseline and candidate use identical seeds
in both orientations, preserving actor-specific seeded health draws.

The new output directory retains exact package snapshots, file hashes, executable
SHA-256, individual run records, and `batch.json` with all reports and the paired
summary. It refuses to overwrite an existing directory. Interrupted batches retain
completed records and a write-ahead checkpoint. Continue them with
`python scripts/arena_evaluation.py --binary TOR_ARENA resume OUTPUT_DIRECTORY`.
Resume validates the executable, retained package snapshots, record identities
and chained checksums before executing unfinished encounters. It preserves
completed failures and censored results, recovers a pending publication without
reexecuting it, and accepts an already completed batch without running it again.
Successful new and retained reports must use the current run-report revision;
previous-revision successes are rejected before unfinished work or replay runs.
The Python reader checks that revision, seed and nonempty participant inventory,
plus the record and termination constraints. It does not independently validate
the complete Rust report schema. Nonzero child exits always remain failures:
parsed objects supply only a bounded diagnostic message and exit code, even for
obsolete failure output. Such diagnostics are never accepted as success evidence.
The per-run report read limit remains 2 MiB and the retained batch bound is 64 MiB.
Changes to the original authoring packages do not affect the snapshots. Older
partial inventories without the recovery checkpoint cannot be resumed. Checksums
detect corruption; they do not authenticate artifacts. Artifacts remain local and
are not committed or promoted to authored source definitions.

Each seed contributes to the win difference only if all four runs end in
elimination. Mutual elimination counts as half a win. Orientation differences are
averaged within each seed. Failure, stalemate and cap counts remain explicit, and
excluded seeds are listed; the mean is null when no complete pairs exist. The mean
is descriptive evidence, without a significance or acceptance claim. Exit 1 means
at least one recorded run failed; exit 2 means invalid batch inputs/artifacts.

Use `python scripts/arena_evaluation.py --binary TOR_ARENA replay OUTPUT_DIRECTORY`
to verify fingerprints and rerun every retained encounter, comparing complete
reports (including recorded failures). Changed inputs, executable fingerprints or
run inventories are rejected. Exit 1 means a replay differs. The curated HD/build
matrix and sequential campaign below provide additional evaluation coverage.


## Curated encounter matrix

Generate the curated encounter matrix with `python scripts/arena_matrix.py
--compiler TOR_SCENARIO --output NEW_DIRECTORY --seeds 42 43`. It creates four
validated packages per case and a paired `plan.json` suitable for the batch
runner above. `matrix.json` records the compiler fingerprint, seed list, cases,
and explicit exclusions. The output must be new; failed or interrupted generation
retains partial evidence without publishing a complete matrix inventory.

The full matrix contains 67 cases (268 packages) across 1, 2, 4, 8 and 16 HD:
racial builds, STR-heavy and SPD-light natural melee styles, all four Mage Mana
bindings, alternating classes, both class dips, resource sustain, Fear, passive
Fear immunity/Energy reduction, and fully partitioned terrain. One-HD mixed/dip builds are
excluded because two HD sources need at least two HD. `--families` and `--hd` can
select a subset. Defaults use the bundled arena's current ruleset; `--ruleset`
provides an explicit override, which the ordinary compiler still validates.

Each candidate faces the same opponent and uses the same HD as its baseline.
The baseline is a STR-heavy Warrior; the opponent is the same build except in
the resistance cases, where it is a Fear-capable Mage. Initial attributes sum to
ten, and each recipe spends its named-pool points and attribute opportunities
within the ordinary caps, with two explicit starting ranks. Talent choices retain their prerequisite chains. Mirroring
reflects starting positions across the nine-cell arena while preserving IDs,
factions, builds and terrain. An equal HD count does not assert equal power.
These recipes exercise natural melee styles; do not treat them as coverage of
all authored equipment builds.

Execute each inventory entry's plan with the paired batch runner, storing reports
in a separate new directory, then verify it with `replay`. Partitioned encounters
end at a cap or stalemate; these are deliberate observations rather than
substitute wins. Matrix generation validates packages and prepares plans; it does
not run battles, perform optimizer screening, or promote balance changes.

### First 3d6 foundation baseline

On 12 October 2026, the unchanged legacy `strength_heavy` and `mage_intellect`
recipes were measured at HD 1, 4, 8 and 16 using seeds 0 through 15. Each case
contains baseline/candidate and forward/mirrored observations: 64 encounters,
16 paired seeds and 32 candidate observations. All 512 encounters terminated
by elimination; none failed or were censored, and all reports matched exact
replay. The same-build Warrior control
had exactly zero paired candidate/baseline difference at every band.

| HD | Warrior candidate hits/checks | Mage candidate hits/checks | Mage paired win difference |
| --- | --- | --- | --- |
| 1 | 48/68 | 56/78 | −0.5000 |
| 4 | 128/146 | 68/126 | −0.1875 |
| 8 | 466/584 | 66/158 | −0.4375 |
| 16 | 212/758 | 82/150 | −0.4375 |

Hits/checks aggregate the candidate actor's actual attack checks, including its
chosen abilities; repeated checks within an encounter are not independent seed
samples. Win differences use the paired estimator above, relative to the
STR-heavy baseline. These debug-binary observations measure gameplay outcomes,
not wall-clock performance. They do not compare 3d6 against d20 or establish
causation for the class difference.

The first foundation keeps base defense 10 and the existing paired attributes
and flat modifiers. These legacy recipes are plainly not a balanced final
Rogue roster: observed Warrior accuracy falls outside the proposed routine-hit
band at several HD levels, and this Mage recipe underperforms its control.
Retuning follows the point economy and playable talent slice, with new paired
evidence; the baseline is not permission to copy d20 probabilities or to claim
that either class is ready.

Reproduce with `arena_matrix.py --families strength_heavy mage_intellect
--hd 1 4 8 16 --seeds 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15`, supplying the
compiler/output arguments above, then execute and replay each inventory plan
with the paired batch runner. Retain its compiler/binary fingerprints and
snapshotted inputs alongside the reports.


## Pinned search environment and sampler

The search sampler requires a separate Python environment. Create one with
`python -m venv SEARCH_ENV`, then use its Python to run
`-m pip install --require-hashes -r .github/requirements-arena-search.txt`.
Use that interpreter for repository Python tests and search tools. CI installs
the same wheel-only, exact-version environment for both platform/profile jobs.
Simulation, server and clients acquire no optimizer runtime dependency.

The reviewed environment pins Optuna 5.0.0 and its transitive dependencies with
published wheel hashes. Optuna, Alembic, Colorlog, SQLAlchemy, Mako and PyYAML use
MIT licenses; NumPy's distribution includes BSD/MIT/0BSD/Zlib/CC0 notices,
Packaging uses Apache-2.0 or BSD-2-Clause, tqdm uses MPL-2.0 and MIT, MarkupSafe and
Colorama use BSD licenses, and typing-extensions uses PSF-2.0. Preserve the
upstream distribution notices. Dependency updates require repeating the
continuation and constraint checks, then updating the pins and fingerprints.

`arena_search_sampling.py` provides the campaign's constrained TPE sampling
primitive. It recreates a seeded sampler for each trial ordinal and restores the
completed history with public Optuna APIs. Ten startup trials precede TPE
sampling; parameters are visited in canonical name order. Named constraints are
feasible when their values are zero or less. Objective/constraint values must be
finite. Histories retain input-space, interpreter/platform, dependency and sampler
source fingerprints, plus record integrity hashes. Changed environments or
modified evidence are rejected rather than silently continuing another search.
Hashes detect accidental changes; they do not authenticate an author.

Run `python scripts/arena_search_sampling.py SAMPLING_JSON` with the pinned
interpreter to verify every recorded proposal and print the next proposal. This
command is read-only. The primitive derives separate, disjoint default seed sets
of 20 training, 20 screening and 200 acceptance seeds. The complete campaign
command below connects those proposals to reviewed parameters, evaluates trials,
screens three finalists, evaluates the winner independently and exports it for
review. Sampling/history verification alone does not establish candidate quality.


## Candidate materialization

Define reviewed candidate parameters with `tor-arena-search-parameters-v2` JSON.
Each named parameter supplies a sampler `space` and one or more existing scalar
`targets`. For example:

```json
{
  "format": "tor-arena-search-parameters-v2",
  "parameters": {
    "strength": {
      "space": {"type": "integer", "low": 1, "high": 5},
      "targets": [{"file": "scenario.toml", "path": ["characters", 1, "creature", "attributes", "strength"]}]
    }
  }
}
```

Use `python scripts/arena_search_parameters.py --compiler TOR_SCENARIO
--plan PAIRED_PLAN --spec PARAMETER_SPEC --values PARAMETER_VALUES --output
NEW_DIRECTORY`. Values are a JSON object such as `{"strength": 4}`. Optional
`--seeds` selects the evaluation seeds; otherwise the source plan's list is kept.

The tool snapshots both baseline and candidate packages in both orientations.
It applies the same assignments to both candidate orientations, preserving the
baseline. Targets must be existing scalars in creature catalog/build declarations
or AI profile settings; scalar type must stay unchanged. Direct region actor
creature declarations are supported. `rules.toml` targets can tune constants,
resource recovery/capacity, the Health recovery curve, ability costs/timing/range/check thresholds and defenses,
effect amounts/dice/durations, AI priorities and resource gates, and numeric
modifier values. Per-HD point amounts are addressed by pool identity:
`["constants", "racial_point_grants", "advancement", "points"]` and
`["classes", "warrior", "point_grants", "advancement", "points"]`.
Pool identities and allowed purchase kinds stay fixed during a numeric sweep;
the ordinary compiler rejects zero or over-budget grants. Content IDs and
expression kinds cannot be changed by parameters. For example, `["resources", "mana", "recovery_period"]` targets the resource
whose stable identity is `mana`, independent of declaration order. Character and
region actor segments select their numeric `id`, never their array position.
Only non-identity arrays, such as effects and modifiers, use positional indices. Each arena
report uses `tor-arena-run-v4` and includes the actual ruleset ID and exact bundled
source SHA-256, also included in its input fingerprint. Overlapping targets, missing paths and edits
to placement, actor IDs/names/factions, bodies, control, participants or limits
are rejected. This defines a constrained parameter space; additional budgets and
selection criteria belong to the campaign configuration.

Rules imports and overrides resolve through `tor-scenario resolve-rules`, the
same Rust resolver used by package compilation. Arena tools pass their selected
compiler explicitly; the Python helper never builds a compiler or implements
import semantics itself. Direct helper use defaults to `target/debug/tor-scenario`
(or the `TOR_SCENARIO_COMPILER` override).

Every resolved package passes through the ordinary scenario compiler. A legal
candidate publishes `plan.json` for the paired runner and `candidate.json` with
parameters, definition hash, compiler fingerprint, original file hashes and
resolved file hashes. Invalid shared-rule builds retain their resolved files and
compiler errors, return exit 1, and receive no runnable plan. Bad configuration,
missing files or an existing output directory return exit 2. No source package or
save is overwritten.

To resume candidate preparation, repeat the materialization command with
`--resume` and the same plan, specification, parameter values, seed overrides and
output directory. `materialization.json` records the input and compiler hashes
and each completed package validation. Resume verifies all completed packages
before invoking the compiler, preserves their files and validation failures, and
rebuilds only unfinished packages. It rejects changed source content, compiler,
configuration or published evidence. Keep source packages available and unchanged
until preparation finishes. Interrupted publication of a valid candidate's
`plan.json` can be recovered from its completed checkpoint. Recovery hashes detect
corruption and are not authentication. Older materializations without this
checkpoint cannot resume.

## Sequential search campaigns

Run the complete sequential campaign with
`python scripts/arena_search.py --binary TOR_ARENA --compiler TOR_SCENARIO run
CONFIG_JSON NEW_OUTPUT_DIRECTORY`. The configuration format is
`tor-arena-search-v1`. Required fields are `plan`, `parameter_spec` (paths relative
to the configuration file) and `seed` (canonical unsigned decimal text).
Defaults are 50 candidate trials, 20 training seeds, three finalists screened on
20 independent seeds, and one winner evaluated with its baseline on 200 further
independent seeds. Optional `candidates` is 3..999; `training_seeds`,
`screening_seeds` and `acceptance_seeds` are 1..1000.

Optional `constraints` is a list of named linear budgets, each containing `name`,
`coefficients` (integer parameter names mapped to bounded integer weights) and
`maximum`. Each budget requires the weighted sum to be at most its maximum.
Compiler-invalid candidates, candidate Engine failures and insufficient paired
coverage are recorded as infeasible optimizer observations. Baseline compiler or
Engine failures stop the campaign as configuration errors. Defaults require all
seed pairs to finish elimination; `maximum_excluded_fraction` can explicitly
permit some censored seeds, but at least one complete pair remains required.
No failure or cap is silently scored as a loss.

Training ranks feasible observations by mean paired win difference, breaking ties
by trial ordinal, and selects three distinct parameter assignments. Fewer than
three ends with `insufficient_feasible_candidates`. Screening selects its winner
using only screening results. Acceptance requires feasibility and a mean at least
`minimum_acceptance_difference` (default zero, range -1..1). This is a configured
descriptive threshold; it is not a claim of statistical significance or balance.
The campaign can end with `screening_infeasible` or `acceptance_failed`. Exit 1
reports those unsuccessful outcomes; exit 2 reports invalid configuration,
corrupted artifacts or tool failures.

Continue with `python scripts/arena_search.py --binary TOR_ARENA --compiler
TOR_SCENARIO resume OUTPUT_DIRECTORY`. Input snapshots, tool and implementation
hashes, dependency environment, optimizer history and per-stage records are
verified. Original authoring paths are unnecessary once snapshots are ready.
Recovery reconciles a completed trial whose history publication was interrupted,
continues partial encounter batches, and preserves completed results. Changed
retained evidence is rejected; hashes detect corruption, not authentication.
A completed campaign verifies its evidence without rerunning completed encounters.

The output retains `inputs.json`, immutable input packages, `history.json`, each
trial's compiled candidate, paired reports and result, screening and acceptance
reports, and `campaign.json`. The `review` directory exports both candidate
orientations and baselines, their runnable plan, parameters, fingerprints and
acceptance evidence. Failed acceptance can still be reviewed. Export never
updates authored scenario definitions.

## Interpreting campaign evidence

A screening winner is not automatically acceptable. A historical four-HD campaign
with seed 42, Strength 3..6 and a budget at most 5 completed 50 trials (42 feasible,
eight compiler-invalid), with 3,360 training, 240 screening and 800 acceptance
encounters and 240 disjoint seeds. All encounters ended in elimination. Strength 4
won screening but scored -0.16 against a zero acceptance threshold, producing
`acceptance_failed`; no authored definitions changed. Completed continuation
preserved 6,293 files and all 800 acceptance reports replayed exactly. This is
bounded workflow evidence, not general roster balance or current-build verification.
See the [immutable source record](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/wizard-mode.md#L607-L627).

Keep matching executables, source modules and the dependency lock with campaign
evidence. Continuation deliberately rejects changed tools. Hashes detect corruption,
not authorship or trust; local exports and transcripts remain outside Git.
