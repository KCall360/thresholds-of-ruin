"""Boundary and completeness checks for executed structural producer reports."""
import copy
import unittest
from structural_performance_report import SCENARIO_TIMINGS, validate_horizon, validate_scenario
from workload_report import InvalidWorkload


class StructuralReports(unittest.TestCase):
    def scenario(self):
        return [dict(workload_version=1, regions=2, sample=0, source_bytes=100,
                     saved_bytes=200, legacy_construction_ms=.1,
                     **{key: .2 for key in SCENARIO_TIMINGS})]

    def horizon(self):
        return [dict(workload='structural-horizon-v1', regions=5, samples=16, hops=2,
                     catalog_build_ns=1, p50_ns=1, p95_ns=2, max_ns=3,
                     required=[1, 2, 3], expanded_regions=2, examined_links=3)]

    def test_smoke_requires_complete_selected_case_and_normal_runs_keep_full_matrix(self):
        for validate, rows in [(validate_scenario, self.scenario()), (validate_horizon, self.horizon())]:
            with self.subTest(validator=validate.__name__):
                self.assertTrue(validate(rows, smoke=True))
                for broken in [[], rows + rows]:
                    with self.assertRaises(InvalidWorkload):
                        validate(broken, smoke=True)
                with self.assertRaises(InvalidWorkload):
                    validate(rows)

    def test_scenario_rejects_invalid_measurements_and_missing_terminal_sample(self):
        for key, value in [('sample', True), ('saved_bytes', 0), ('action_ms', float('nan')),
                           ('resume_ms', -1), ('legacy_construction_ms', None), ('regions', 256)]:
            rows = self.scenario()
            rows[0][key] = value
            with self.subTest(key=key), self.assertRaises(InvalidWorkload):
                validate_scenario(rows, smoke=True)

    def test_horizon_rejects_false_counters_nonfinite_timings_and_vacuous_or_unordered_regions(self):
        for key, value in [('samples', 15), ('hops', True), ('expanded_regions', True),
                           ('p50_ns', float('inf')), ('p95_ns', 4), ('required', [1]),
                           ('required', [2, 1]), ('required', [1, 1]), ('required', [1, 6])]:
            rows = self.horizon()
            rows[0][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(InvalidWorkload):
                validate_horizon(rows, smoke=True)

    def test_full_horizon_reports_preserve_work_across_unrelated_scale(self):
        rows = [dict(copy.deepcopy(self.horizon()[0]), regions=scale, samples=10000) for scale in [5, 256, 8192]]
        self.assertEqual(validate_horizon(rows), rows)
        rows[-1]['examined_links'] += 1
        with self.assertRaises(InvalidWorkload):
            validate_horizon(rows)


if __name__ == '__main__':
    unittest.main()
