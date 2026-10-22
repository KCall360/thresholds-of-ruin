# CI execution-cost baseline

Measured **2026-10-22**, before the risk-based testing-regime change. This is a
purpose-selected recent sample, not a repository-wide average or billing report.

## How much CI does a PR consume?

| PR | Scope | PR runner-hours | Postmerge runner-hours | Total |
|---|---|---:|---:|---:|
| [#183](https://github.com/KCall360/thresholds-of-ruin/pull/183) | Core room omissions, four revisions | 16.984 | 4.557 | **21.541** |
| [#184](https://github.com/KCall360/thresholds-of-ruin/pull/184) | Windows CI sharding | 4.641 | 4.421 | **9.062** |
| [#186](https://github.com/KCall360/thresholds-of-ruin/pull/186) | Core dark mazes, three revisions | 13.868 | 4.865 | **18.733** |
| [#187](https://github.com/KCall360/thresholds-of-ruin/pull/187) | Prose documentation | ≥0.010 | 0.014 | **≥0.024** |
| [#185](https://github.com/KCall360/thresholds-of-ruin/pull/185) | Open 3D change | 6.351 | not merged | **6.351 so far** |

The open 3D PR separates **4.455 hours core CI** (canceled) and **1.896 hours
independent 3D** (failed). Do not compare it as a finished PR. A full recent core
workflow consumes roughly **4–4.8 runner-hours**, even when parallel execution
returns in **36–48 minutes**. Failed revisions and postmerge repetition multiply
that cost. The documentation total is a **lower bound**, not an exact 1.47 minutes:
one source-identity job has no completion timestamp despite workflow success.

### Accounting method

Sum completed minus started timestamps for distinct executed jobs, including
failed and canceled work, across every revision and attempt. Skipped jobs cost
zero. No OS billing multipliers, minute rounding, CPU utilization or invoice data
are inferred. Windows and Linux runner time can be compared separately.

All 15 runs matched PR branches or exact merge SHAs in a 100-run repository page
extending before these PRs were created. All job pages fit within 100 records;
`filter=all` included previous attempts. On partial reruns GitHub can return new
job IDs carrying forward old successful jobs with **identical name/start/end**.
The #186 postmerge retry had 14 such records: charging them twice would falsely
report 9.414 rather than 4.865 hours. Distinct later execution remains charged.

Creation-to-final-executed-job time is a wall-clock span, not runner-hours or a
pure queue measure. It includes workflow dependencies and any retry gap. The
GitHub terminal timestamp is separate: docs jobs finished in 26–27 seconds, but
delayed skipped records extended terminal metadata to 5.2–5.5 minutes. Incomplete
job intervals are omitted and explicitly make totals lower bounds; neither
ongoing work nor missing records are guessed.

The compact [measured run inventory](../../scripts/testdata/ci-cost/baseline-20261022.json)
contains run URLs, results, attempts, hours and spans without raw API payloads.

## Where the time goes

Final #186 PR [run 39731755709](https://github.com/KCall360/thresholds-of-ruin/actions/runs/39731755709)
used **4.3925 runner-hours**, **36.02 minutes elapsed**, and started its first job
2 seconds after creation. Windows consumed 2.6781 hours (61%).

| Step category | Summed runner seconds |
|---|---:|
| Verification wrapper: build, lint and tests | 15,080 |
| Setup, installs and checkout | 441 |
| Cache restore/save | 120 |
| Evidence observations, prior-head preparation and uploads | 33 |
| Other gates, scope and dependency steps | 94 |
| Unallocated job overhead | 45 |

The ten small verification artifacts (~1.3 MB total) report 15,065.57 seconds of
verifier stages. Cargo `Finished ... target(s)` lines account for **7,534.83 seconds
(2.09 hours)**, approximately half the verifier time. Remaining time includes
tests and wrappers, not exclusively test CPU time. Step-name categorization is
approximate; package installation is setup, project compilation is verification.

The critical [Windows release Rust job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/39731755709/job/119668230256)
took 35.57 minutes, including **31m30s compilation** in its 34.16-minute verifier.
Python Windows sharding independently rebuilt process binaries: debug shards
418.5/402.6 seconds and release shards 325.0/525.8 seconds. Sharding improves
latency but need not reduce total runner-hours.

Python test stages still cost 1,614/1,528 seconds on Linux debug/release;
Windows debug shards cost 857/1,237 seconds and release shards 608/1,031 seconds.
Shard 1 contains only 21 tests, but a single test accounts for about 854 seconds
debug and 607 seconds release. Balance by observed duration and contract
ownership, not test counts.

### Cache and evidence are different

No postmerge adoption was observed in this sample. An exact dependency-cache hit
in [Windows postmerge release Rust](https://github.com/KCall360/thresholds-of-ruin/actions/runs/39770714421/job/119797339132)
still led to 2,840 seconds of fresh verification. All representative final-PR
receipts identify fresh evidence. Successful cache/preparation steps or artifact
existence alone do not prove complete, valid test qualification.

## Quantitative evaluation targets, not savings promises

Use the 4.3925-hour, 36.02-minute final #186 run as a diagnostic reference, not an
unbiased before/after control. Repeat measurements on comparable source,
toolchains, runner images and cache conditions before claiming improvement.

- **Compilation scenario:** eliminating 25% of its measured Cargo build time
  would save about 0.523 runner-hours, producing approximately **3.87 hours**
  if every other cost were unchanged. Eliminating 50% would yield approximately
  **3.35 hours**. These are arithmetic scenarios, not demonstrated cache benefits.
- **Build-sharing ceiling:** removing one duplicate process build per Windows
  profile could save at most about **0.202–0.262 runner-hours** in this run,
  before artifact transfer, validation and layout costs. This overlaps the
  compilation scenario and must not be added to it.
- **Long-test scenario:** halving only the observed ~854-second debug and
  ~607-second release shard-1 witnesses would save about **0.203 hours**.
  It would not shorten this run's critical path, dominated by Windows Rust.
  Removing those witnesses is not justified by their duration.
- **Postmerge scenario:** avoiding redundant postmerge work has a theoretical
  upper bound of **4.421 hours for #184** or **4.865 hours for #186**. Safe
  exact-state qualification, merge-specific checks and failed setup recovery
  still cost time. The measured sample demonstrates no adoption savings.
- **Measurement targets:** report runner-hours and wall span for every evaluated
  regime; seek at least **20% less core runner time** (≤3.51 hours against this
  reference) without losing mapped contract witnesses, and diagnose the
  compilation bottleneck before targeting **≤30 minutes** wall span. These are
  engineering acceptance targets subject to controlled validation, not forecasts.

Do not optimize 33 seconds of evidence machinery while overlooking hours of
compilation or repeated matrices. Preserve the inexpensive documentation path.

## Reusable offline measurement

`scripts/ci_cost_report.py` reads saved JSON only; it needs no network or token.
Supply each workflow run and its combined, paginated all-attempt jobs:

```json
{
  "runs": [{
    "run": {"id": 123, "status": "completed", "run_attempt": 2},
    "jobs_scope": "all",
    "total_count": 17,
    "jobs": []
  }]
}
```

The illustration omits real timestamps/jobs; it intentionally cannot establish
complete cost. Preserve `created_at`, `updated_at`, and each job's `id`, `name`,
`labels`, `started_at`, `completed_at`, `conclusion` and `run_attempt`. Include
`repository.full_name` when combining repositories. Fetch jobs with `filter=all`,
combine every page, retain `total_count`, then declare `jobs_scope: "all"`.
Otherwise the report flags a lower bound. This declaration is a caller assertion,
not independently authenticated API provenance.

```sh
python scripts/ci_cost_report.py --input saved-ci-jobs.json \
  --json-output ci-cost.json --text-output ci-cost.txt
```

Repeat `--input` to combine files. Same-job-ID progress snapshots are reconciled
first: compatible completed timestamps supersede null fields regardless of input
order; conflicting non-null identities or timestamps fail closed with an error
instead of being double charged. Then carried-forward new IDs with identical
name/start/end intervals are deduplicated within runs. An empty input sample is
uncertain, not exact zero. An explicitly completed run with `jobs_scope: "all"`,
`total_count: 0`, and no jobs is a verified empty run for this offline accounting
contract; missing count or attempt-completeness declarations make a lower bound. Optional repeated `--summary` accepts verifier stage summaries, projecting
only stage names and durations. Stage durations are informational and never added
to runner totals. Their provenance/uniqueness remains the caller's responsibility.
No metric here changes qualification or waives tests.

Tests use independently hand-authored fixtures for parallel wall/runner time,
carried-forward new IDs, real later retries, canceled/failed jobs, skipped invalid
intervals, null completion times, pagination limits, OS breakdown, overlapping
input files, progress snapshots, conflicting job identities, verified-empty versus
missing samples, optional summaries and CLI output. Run:

```sh
python -m unittest discover -s scripts -p test_ci_cost_report.py -v
```
