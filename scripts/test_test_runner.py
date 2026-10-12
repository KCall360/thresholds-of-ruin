"""The timed runner preserves unittest behavior and records failed attempts."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class TimedRunnerContract(unittest.TestCase):
    def run_fixture(self, source, *args, graphical_waivers=None, ci=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'test_sample.py').write_text(source, encoding='utf-8')
            output = root / 'timings.jsonl'
            command = [sys.executable, str(Path(__file__).with_name('test_runner.py')), *args]
            if graphical_waivers is not None:
                manifest = root / 'waiver.json'
                manifest.write_text(json.dumps({'reason': 'synthetic display fixture',
                                                'tests': graphical_waivers}), encoding='utf-8')
                command.extend(['--waive-local-graphical', str(manifest)])
            environment = {**os.environ, 'TOR_TEST_TIMINGS': str(output)}
            if ci is not None:
                # This controls only the toy runner fixture, never repository tests.
                environment['CI'] = ci
            result = subprocess.run(command, cwd=root,
                                    env=environment,
                                    capture_output=True, text=True, timeout=20)
            records = [json.loads(line) for line in output.read_text().splitlines()] if output.exists() else []
            return result, records

    def test_explicit_graphical_waiver_is_named_while_new_headless_failure_remains(self):
        source = '''import unittest
class Mixed(unittest.TestCase):
 graphical = True
 def test_window(self): self.fail('native body must not start')
 def test_new_headless(self): self.fail('new headless sentinel')
'''
        name = 'test_sample.Mixed.test_window'
        result, records = self.run_fixture(source, 'discover', '-s', '.', '-v',
                                          graphical_waivers=[name], ci='')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('new headless sentinel', result.stderr)
        self.assertNotIn('native body must not start', result.stderr)
        self.assertIn('LOCAL GRAPHICAL WAIVERS (not run): ' + name, result.stderr)
        self.assertEqual({row['test_id']: row['outcome'] for row in records if row['kind'] == 'test'},
                         {'test_sample.Mixed.test_new_headless': 'failed', name: 'waived'})
        self.assertEqual(records[-1]['waived_tests'], [name])

    def test_actual_runner_rejects_ci_and_unknown_graphical_waivers(self):
        source = '''import unittest
class Native(unittest.TestCase):
 graphical = True
 def test_window(self): self.fail('body must not start')
'''
        for ci, name, message in [('true', 'test_sample.Native.test_window', 'local only'),
                                  ('', 'test_sample.Native.test_missing', 'not selected tests')]:
            with self.subTest(ci=ci, name=name):
                result, records = self.run_fixture(source, 'discover', '-s', '.',
                                                  graphical_waivers=[name], ci=ci)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(message, result.stderr)
                self.assertFalse(any(row['kind'] == 'test' for row in records))

    def test_discovery_keeps_success_failure_error_and_skip_visible(self):
        result, records = self.run_fixture('''import unittest
class Sample(unittest.TestCase):
 def test_pass(self): self.assertEqual(2, 2)
 def test_fail(self): self.fail('sentinel failure')
 def test_error(self): raise ValueError('sentinel error')
 @unittest.skip('not supported in fixture')
 def test_skip(self): pass
''', 'discover', '-s', '.', '-p', 'test_sample.py', '-v')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('FAILED (failures=1, errors=1, skipped=1)', result.stderr)
        tests = {row['test_id'].rsplit('.', 1)[-1]: row for row in records if row['kind'] == 'test'}
        self.assertEqual({name: row['outcome'] for name, row in tests.items()},
                         {'test_pass': 'passed', 'test_fail': 'failed', 'test_error': 'error', 'test_skip': 'skipped'})
        self.assertTrue(all(row['seconds'] >= 0 for row in records))
        self.assertEqual(records[-1]['kind'], 'suite')
        self.assertEqual(records[-1]['tests_run'], 4)
        self.assertFalse(records[-1]['successful'])

    def test_named_selection_subtests_and_expected_failures_keep_exit_semantics(self):
        result, records = self.run_fixture('''import unittest
class Sample(unittest.TestCase):
 def test_cases(self):
  for value in [0, 1]:
   with self.subTest(value=value): self.assertEqual(value, 0)
 @unittest.expectedFailure
 def test_expected(self): self.fail('known fixture')
 @unittest.expectedFailure
 def test_unexpected(self): pass
 def test_excluded(self): raise RuntimeError('must not run')
''', '-v', 'test_sample.Sample.test_cases', 'test_sample.Sample.test_expected',
                                          'test_sample.Sample.test_unexpected')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn('must not run', result.stderr)
        tests = [row for row in records if row['kind'] == 'test']
        self.assertEqual([row['outcome'] for row in tests], ['failed', 'expected_failure', 'unexpected_success'])
        self.assertEqual(tests[0]['subtests_failed'], 1)
        self.assertEqual(records[-1]['tests_run'], 3)

    def test_passing_named_test_returns_success(self):
        result, records = self.run_fixture('''import unittest
class Sample(unittest.TestCase):
 def test_pass(self): self.assertTrue(True)
''', 'test_sample.Sample.test_pass', *(['--durations', '1'] if sys.version_info >= (3, 12) else []))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(records[-1]['tests_run'], 1)
        self.assertTrue(records[-1]['successful'])

    def test_later_skip_does_not_relabel_an_earlier_subtest_failure(self):
        result, records = self.run_fixture("""import unittest
class Sample(unittest.TestCase):
 def test_mixed(self):
  with self.subTest(case='bad'): self.fail('first failure')
  self.skipTest('later skip')
""", 'test_sample.Sample.test_mixed')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(records[0]['failures'], 1)
        self.assertEqual(records[0]['outcome'], 'failed')
        self.assertFalse(records[-1]['successful'])

    def test_class_setup_error_is_not_hidden_by_zero_started_tests(self):
        result, records = self.run_fixture("""import unittest
class Sample(unittest.TestCase):
 @classmethod
 def setUpClass(cls): raise RuntimeError('setup sentinel')
 def test_never_started(self): pass
""", 'discover', '-s', '.')
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn('setup sentinel', result.stderr)
        self.assertEqual(records[-1]['errors'], 1)
        self.assertEqual(records[-1]['tests_run'], 0)
        self.assertFalse(records[-1]['successful'])

    def test_runner_without_timing_destination_preserves_success(self):
        with tempfile.TemporaryDirectory() as directory:
            Path(directory, 'test_sample.py').write_text(
                'import unittest\nclass Sample(unittest.TestCase):\n def test_ok(self): pass\n',
                encoding='utf-8')
            environment = {key: value for key, value in os.environ.items() if key != 'TOR_TEST_TIMINGS'}
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('test_runner.py')),
                                     'discover', '-s', '.'], cwd=directory, env=environment,
                                    capture_output=True, text=True, timeout=20)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Ran 1 test', result.stderr)

    def test_failfast_still_stops_after_first_failed_test(self):
        result, records = self.run_fixture('''import unittest
class Sample(unittest.TestCase):
 def test_a(self): self.fail('first')
 def test_b(self): self.fail('must not execute')
''', 'discover', '-s', '.', '-f')
        self.assertEqual(result.returncode, 1)
        self.assertNotIn('must not execute', result.stderr)
        self.assertEqual(records[-1]['tests_run'], 1)


if __name__ == '__main__':
    unittest.main()
