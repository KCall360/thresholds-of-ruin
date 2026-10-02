"""Shared validation and summaries for the benchmark workload reports.

Each `*_performance_report.py` script validates one workload's raw JSON-lines
samples and prints a compact summary; `perf_compare.py` runs them. This module
holds what they share: failures that are real errors (not `assert`, which
`python -O` removes), duration checks, percentile summaries, and the
matrix-of-cases validator used by the combat, items and physics workloads.
"""
import json
import math
from pathlib import Path


class InvalidWorkload(ValueError):
    """Samples that are incomplete, from another workload version, or not measurements."""


def require(condition, message):
    if not condition:
        raise InvalidWorkload(message)


def is_duration(value):
    """A finite, non-negative number of milliseconds (not a boolean)."""
    return not isinstance(value, bool) and isinstance(value, (int, float)) and math.isfinite(value) and value >= 0


def percentiles(values):
    """Nearest-rank p50 and p95, the maximum, and the sample count."""
    values = sorted(values)
    require(values, "no samples to summarize")
    return dict(n=len(values), p50=values[math.ceil(len(values) * .5) - 1],
                p95=values[math.ceil(len(values) * .95) - 1], maximum=values[-1])


def read_rows(path):
    """The JSON object lines of a raw samples file."""
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8-sig").splitlines() if line.startswith("{")]


def samples(value):
    return value if isinstance(value, list) else [value]


def summarize_matrix(rows, *, matches, key_fields, cases, check_row, sample_ids, timings, counts, extend=None):
    """Validate a complete matrix of cases and summarize each case's samples.

    `matches(row)` identifies the workload and version; every row's case
    (`key_fields`) must be in `cases`, and every case must have exactly the
    samples `sample_ids`. `check_row(row)` raises for a malformed row. Each
    `timings` field is summarized as percentiles over every sample's values and
    each `counts` field as its range; `extend(report, samples)` adds extra fields.
    """
    groups = {}
    for row in rows:
        require(matches(row), f"row from another workload or version: {row.get('workload', row.get('version'))}")
        key = tuple(row[f] for f in key_fields)
        require(key in cases, f"unknown case {key}")
        check_row(row)
        groups.setdefault(key, []).append(row)
    require(set(groups) == set(cases), f"missing cases {sorted(set(cases) - set(groups))}")
    reports = []
    for key, group in sorted(groups.items()):
        require(len(group) == len(sample_ids) and {s["sample"] for s in group} == set(sample_ids),
                f"case {key} needs samples {sorted(sample_ids)}")
        report = dict(zip(key_fields, key))
        for field in timings:
            values = [v for s in group for v in samples(s[field])]
            require(all(is_duration(v) for v in values), f"{field} has a value that is not a duration")
            report[field] = percentiles(values)
        for field in counts:
            report[field] = dict(min=min(s[field] for s in group), max=max(s[field] for s in group))
        if extend:
            extend(report, group)
        reports.append(report)
    return reports


def positive_counts(row, fields):
    for field in fields:
        require(isinstance(row[field], int) and not isinstance(row[field], bool) and row[field] > 0,
                f"{field} must be a positive count")


def main(summarize):
    """Command-line entry point: validate the samples file and print the summary."""
    import sys
    print(json.dumps(summarize(read_rows(sys.argv[1])), indent=2))
