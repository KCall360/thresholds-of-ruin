"""The optional measurement must never replace or hide its full baseline."""
import copy
import json
from pathlib import Path
import tempfile
from unittest import mock
import unittest

import ci_shadow


class ShadowComparison(unittest.TestCase):
    def baseline(self, profile='debug'):
        return {'scope': 'ci-partition', 'tier': 'full', 'ci_profile': profile,
                'ci_partition': 'python', 'clean_passed': True, 'gate_passed': True,
                'waived_steps': [], 'not_run_steps': [], 'jobs': 2,
                'steps': [{'step': 'python-debug' if profile == 'debug' else 'process-release',
                           'status': 'passed', 'code': 0, 'seconds': 30.0,
                           'command': ['python', 'scripts/test_runner.py', '--workers', '2', 'discover']}]}

    def test_workflow_runs_example_parser_tests_in_both_rust_profiles(self):
        workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/ci.yml').read_text()
        self.assertIn('Verify physics example argument contract', workflow)
        self.assertIn('cargo test -p tor-server --example physics_bench --locked', workflow)
        self.assertIn('args+=(--release)', workflow)

    def test_workflow_collects_observations_without_enabling_reuse(self):
        workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/ci.yml').read_text()
        self.assertIn('python scripts/ci_observation.py --phase before', workflow)
        self.assertIn('python scripts/ci_observation.py --phase after', workflow)
        self.assertNotIn('scripts/ci_evidence.py', workflow)

    def test_reuses_exact_baseline_worker_count(self):
        self.assertEqual(ci_shadow.baseline_settings(self.baseline(), 'debug'), (2, 2, 30.0))

    def test_single_worker_default_is_preserved(self):
        data = self.baseline()
        data['steps'][0]['command'] = ['python', 'scripts/test_runner.py', 'discover']
        self.assertEqual(ci_shadow.baseline_settings(data, 'debug')[0], 1)

    def test_failed_waived_missing_or_wrong_profile_baseline_rejected(self):
        for key, value in [('clean_passed', False), ('gate_passed', False),
                           ('waived_steps', ['python-debug']), ('not_run_steps', ['build']),
                           ('ci_profile', 'release'), ('ci_partition', 'rust'), ('scope', 'local')]:
            with self.subTest(key=key):
                data = self.baseline()
                data[key] = value
                with self.assertRaises(ValueError):
                    ci_shadow.baseline_settings(data, 'debug')

    def test_ambiguous_or_invalid_command_and_timing_rejected(self):
        for change in [{'command': ['python', '--workers', '0']},
                       {'command': ['python', '--workers', '2', '--workers', '3']},
                       {'seconds': float('nan')}, {'seconds': -1}, {'code': 1}]:
            data = self.baseline()
            data['steps'][0].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                ci_shadow.baseline_settings(data, 'debug')
        data = self.baseline()
        data['steps'] += copy.deepcopy(data['steps'])
        with self.assertRaises(ValueError):
            ci_shadow.baseline_settings(data, 'debug')

    def test_measurement_runs_existing_runner_and_keeps_raw_evidence(self):
        import ci_policy
        name = 'test_ci_shadow.ShadowComparison.test_single_worker_default_is_preserved'
        inventory = {'sha256': 'a' * 64, 'tests': [{'test_id': name, 'kind': 'tool', 'discovery': 'default'}]}
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'measurement'
            with (mock.patch.object(ci_shadow.sys, 'platform', 'win32'),
                  mock.patch.object(ci_policy, 'read_json', side_effect=[self.baseline(), {'audit': {'sources': {}}}]),
                  mock.patch.object(ci_policy, 'discover', return_value=inventory),
                  mock.patch.object(ci_policy, 'reviewed_candidate', return_value=[name])):
                result = ci_shadow.main(['--profile', 'debug', '--baseline', 'unused.json',
                                         '--output-dir', str(output)])
            self.assertEqual(result, 0)
            report = json.loads((output / 'comparison.json').read_text())
            self.assertTrue(report['passed'])
            self.assertEqual(report['selected'], [name])
            self.assertEqual(report['workers'], 2)
            self.assertTrue((output / 'shadow-process-tests.jsonl').is_file())

    def test_exact_successful_timing_inventory_required(self):
        row = {'kind': 'test', 'test_id': 'a.A.test_one', 'profile': 'debug',
               'outcome': 'passed', 'failures': 0, 'errors': 0,
               'subtests_failed': 0, 'subtests_skipped': 0}
        ci_shadow.validate_results([row], ['a.A.test_one'], 'debug')
        for rows in [[], [row, row], [{**row, 'outcome': 'skipped'}],
                     [{**row, 'subtests_skipped': 1}], [{**row, 'profile': 'release'}],
                     [{'kind': 'local_graphical_waiver'}]]:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                ci_shadow.validate_results(rows, ['a.A.test_one'], 'debug')


if __name__ == '__main__':
    unittest.main()
