"""Validate the complete scenario and structural-horizon producer matrices.

Smoke invocations select a small authored scale; they do not redefine the normal
versioned workloads. No elapsed-time threshold is a correctness condition.
"""
from workload_report import is_duration, positive_counts, require, summarize_matrix


SCENARIO_TIMINGS = ['validation_ms', 'integrity_ms', 'construction_ms', 'action_ms',
                    'create_save_ms', 'save_barrier_ms', 'resume_ms']


def validate_scenario(rows, *, smoke=False):
    fields = {'workload_version', 'regions', 'sample', 'source_bytes', 'saved_bytes', 'legacy_construction_ms', *SCENARIO_TIMINGS}
    require(all(isinstance(row, dict) and fields <= row.keys() for row in rows), 'missing scenario report fields')
    def check(row):
        positive_counts(row, ['regions', 'source_bytes', 'saved_bytes'])
        legacy = row['legacy_construction_ms']
        require(is_duration(legacy) if row['regions'] == 2 else legacy is None,
                'legacy baseline belongs only to the two-region case')
        require(type(row['sample']) is int, 'sample identity must be an integer')
    return summarize_matrix(
        rows, matches=lambda row: type(row.get('workload_version')) is int and row['workload_version'] == 1,
        key_fields=['regions'], cases={(2,)} if smoke else {(2,), (256,)},
        check_row=check, sample_ids=range(1 if smoke else 20),
        timings=SCENARIO_TIMINGS, counts=['source_bytes', 'saved_bytes'])


def validate_horizon(rows, *, smoke=False):
    fields = {'workload', 'regions', 'samples', 'hops', 'catalog_build_ns', 'p50_ns', 'p95_ns', 'max_ns', 'required', 'expanded_regions', 'examined_links'}
    require(all(isinstance(row, dict) and fields <= row.keys() for row in rows), 'missing horizon report fields')
    scales = [5] if smoke else [5, 256, 8192]
    require(len(rows) == len(scales), 'missing or duplicate structural-horizon report')
    require([row.get('regions') for row in rows] == scales, 'incorrect structural-horizon scales/order')
    expected_work = None
    for row in rows:
        require(row.get('workload') == 'structural-horizon-v1', 'unknown horizon workload')
        require(type(row['samples']) is int and row['samples'] == (16 if smoke else 10000), 'incomplete horizon sample count')
        require(type(row['hops']) is int and row['hops'] == 2, 'incorrect horizon radius')
        positive_counts(row, ['regions', 'expanded_regions', 'examined_links'])
        require(all(is_duration(row[key]) for key in ['catalog_build_ns', 'p50_ns', 'p95_ns', 'max_ns']), 'invalid horizon measurement')
        require(row['p50_ns'] <= row['p95_ns'] <= row['max_ns'], 'unordered horizon percentiles')
        required = row['required']
        require(isinstance(required, list) and all(type(v) is int and 1 <= v <= 5 for v in required), 'horizon must contain only connected authored regions')
        require(required == sorted(set(required)) and 1 in required and len(required) > 1, 'invalid or vacuous required regions')
        work = (required, row['expanded_regions'], row['examined_links'])
        require(expected_work is None or work == expected_work, 'unrelated regions changed horizon work')
        expected_work = work
    return rows
