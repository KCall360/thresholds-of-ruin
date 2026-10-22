"""The timed runner preserves unittest behavior and records failed attempts."""
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_runner import TimedRunner


class ImmediateFailureDiagnostics(unittest.TestCase):
    def test_failure_and_error_diagnostics_precede_cleanup_without_duplicate_outcomes(self):
        for error, subtest in [(AssertionError, False), (ValueError, False),
                               (AssertionError, True), (ValueError, True)]:
            with self.subTest(error=error.__name__, subtest=subtest):
                records, before_cleanup = [], []
                class Fixture(unittest.TestCase):
                    def runTest(inner):
                        if subtest:
                            with inner.subTest(value="original"):
                                raise error("original failure sentinel")
                        else:
                            raise error("original failure sentinel")
                    def tearDown(inner):
                        before_cleanup.extend(dict(row) for row in records)
                        raise OSError("cleanup failure sentinel")
                fixture = Fixture()
                result = TimedRunner(stream=io.StringIO(), sink=lambda row: records.append(dict(row))).run(fixture)
                self.assertFalse(result.wasSuccessful())
                initial = [row for row in before_cleanup if row['kind'] == 'diagnostic']
                self.assertEqual(len(initial), 1)
                self.assertEqual(initial[0]['test_id'], fixture.id())
                self.assertIn('original failure sentinel', initial[0]['traceback'])
                self.assertEqual(initial[0]['exception_type'], error.__name__)
                self.assertGreaterEqual(initial[0]['seconds'], 0)
                self.assertEqual('subtest_id' in initial[0], subtest)
                self.assertNotIn('outcome', initial[0])
                diagnostics = [row for row in records if row['kind'] == 'diagnostic']
                self.assertEqual(len(diagnostics), 2)
                self.assertIn('cleanup failure sentinel', diagnostics[1]['traceback'])
                outcomes = [row for row in records if row['kind'] == 'test']
                self.assertEqual(len(outcomes), 1)
                self.assertEqual(outcomes[0]['outcome'], 'error')
                self.assertEqual(outcomes[0]['failures'], int(error is AssertionError))
                self.assertEqual(outcomes[0]['errors'], 1 + int(error is ValueError))
                self.assertEqual(records[-1]['kind'], 'suite')

    def test_failure_diagnostics_are_bounded_and_explicitly_mark_truncation(self):
        records = []
        class Fixture(unittest.TestCase):
            def runTest(inner):
                raise AssertionError('x' * 200000 + 'original failure tail')
        result = TimedRunner(stream=io.StringIO(), sink=lambda row: records.append(dict(row))).run(Fixture())
        self.assertFalse(result.wasSuccessful())
        diagnostics = [row for row in records if row['kind'] == 'diagnostic']
        self.assertEqual(len(diagnostics), 1)
        self.assertLessEqual(len(diagnostics[0]['traceback']), 65536)
        self.assertTrue(diagnostics[0]['traceback_truncated'])
        self.assertIn('original failure tail', diagnostics[0]['traceback'])

    def test_class_setup_error_has_immediate_evidence_without_invented_test_outcome(self):
        records = []
        class Fixture(unittest.TestCase):
            @classmethod
            def setUpClass(cls):
                raise OSError('class setup failure sentinel')
            def runTest(inner):
                raise AssertionError('must not run')
        result = TimedRunner(stream=io.StringIO(), sink=lambda row: records.append(dict(row))).run(
            unittest.TestSuite([Fixture()]))
        self.assertEqual(result.testsRun, 0)
        diagnostics = [row for row in records if row['kind'] == 'diagnostic']
        self.assertEqual(len(diagnostics), 1)
        self.assertIn('setUpClass', diagnostics[0]['test_id'])
        self.assertIn('class setup failure sentinel', diagnostics[0]['traceback'])
        self.assertGreaterEqual(diagnostics[0]['seconds'], 0)
        self.assertFalse(any(row['kind'] == 'test' for row in records))
        self.assertEqual(records[-1]['errors'], 1)


class TimedRunnerContract(unittest.TestCase):
    def test_serial_and_parallel_receipts_record_only_the_original_selection(self):
        source = '''import unittest
class Sample(unittest.TestCase):
 def test_a(self): pass
 def test_b(self): pass
'''
        for workers in (1, 2):
            with self.subTest(workers=workers):
                result, records = self.run_fixture(source, '--workers', str(workers), 'discover', '-s', '.')
                self.assertEqual(result.returncode, 0, result.stderr)
                selections = [row for row in records if row['kind'] == 'selection']
                self.assertEqual(len(selections), 1)
                self.assertEqual(selections[0]['test_ids'], ['test_sample.Sample.test_a', 'test_sample.Sample.test_b'])
                self.assertEqual(records[0]['kind'], 'selection')
                self.assertEqual(records[-1]['kind'], 'suite')

    def test_interruption_preserves_prior_subtest_failure_and_error_counts(self):
        for statement, field in [("self.fail('failure sentinel')", 'failures'),
                                 ("raise ValueError('error sentinel')", 'errors')]:
            with self.subTest(field=field):
                source = f'''import unittest
class Sample(unittest.TestCase):
 def test_a(self):
  with self.subTest(value=0): {statement}
  raise KeyboardInterrupt('after subtest')
'''
                result, records = self.run_fixture(source, 'discover', '-s', '.')
                self.assertNotEqual(result.returncode, 0)
                outcome = next(row for row in records if row['kind'] == 'test')
                self.assertEqual(outcome['outcome'], 'interrupted')
                self.assertEqual(outcome[field], 1)
                self.assertEqual(outcome['subtests_failed'], 1)
                self.assertFalse(any(row['kind'] == 'suite' for row in records))

    def test_class_teardown_interruption_does_not_relabel_completed_test(self):
        result, records = self.run_fixture('''import unittest
class Sample(unittest.TestCase):
 def test_a(self): self.assertTrue(True)
 @classmethod
 def tearDownClass(cls): raise KeyboardInterrupt('class cleanup interrupted')
''', 'discover', '-s', '.')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual([(row['test_id'].rsplit('.', 1)[-1], row['outcome'])
                          for row in records if row['kind'] == 'test'], [('test_a', 'passed')])
        self.assertFalse(any(row['kind'] == 'suite' for row in records))

    def test_skipped_subtests_finish_normally_but_cannot_hide_interruption(self):
        source = '''import unittest
class Sample(unittest.TestCase):
 def test_skip(self):
  with self.subTest(value=0): self.skipTest('skip sentinel')
 def test_mixed(self):
  with self.subTest(value=0): self.skipTest('skip sentinel')
  with self.subTest(value=1): self.assertTrue(True)
 def test_interrupt(self):
  with self.subTest(value=0): self.skipTest('skip sentinel')
  raise KeyboardInterrupt('interrupt after skip')
'''
        for method in ['test_skip', 'test_mixed', 'test_interrupt']:
            with self.subTest(method=method):
                result, records = self.run_fixture(source, f'test_sample.Sample.{method}')
                interrupted = method == 'test_interrupt'
                self.assertEqual(result.returncode == 0, not interrupted, result.stderr)
                outcome = next(row for row in records if row['kind'] == 'test')
                self.assertEqual(outcome['outcome'], 'interrupted' if interrupted else 'passed')
                self.assertEqual(outcome['subtests_skipped'], 1)
                self.assertEqual(any(row['kind'] == 'suite' for row in records), not interrupted)

    def test_interrupted_method_is_never_recorded_as_passed(self):
        result, records = self.run_fixture('''import unittest
class Sample(unittest.TestCase):
 def test_a(self): self.assertTrue(True)
 def test_b(self): raise KeyboardInterrupt('interrupted sentinel')
 def test_c(self): self.fail('must not run')
''', 'discover', '-s', '.')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('interrupted sentinel', result.stderr)
        self.assertEqual([(row['test_id'].rsplit('.', 1)[-1], row['outcome'])
                          for row in records if row['kind'] == 'test'],
                         [('test_a', 'passed'), ('test_b', 'interrupted')])
        self.assertFalse(any(row['kind'] == 'suite' for row in records))

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
        outcome = next(row for row in records if row['kind'] == 'test')
        self.assertEqual(outcome['failures'], 1)
        self.assertEqual(outcome['outcome'], 'failed')
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




class CiShardRunnerContract(unittest.TestCase):
    def execute(self, index, *, failing=False, profile="release"):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / 'scripts'
            scripts.mkdir()
            body = "import unittest\nclass Owner(unittest.TestCase):\n    def test_owner(self):\n        %s\n"
            (scripts / 'test_rogue_earned_route_process.py').write_text(
                body % ("self.fail('paid owner failed')" if failing else 'pass'), encoding='utf-8')
            (scripts / 'test_future_process.py').write_text(body % 'pass', encoding='utf-8')
            (scripts / 'test_future_pure.py').write_text(body % 'pass', encoding='utf-8')
            timings = root / 'timings.jsonl'
            pattern = 'test_*.py' if profile == 'debug' else 'test_*process.py'
            command = [sys.executable, str(Path(__file__).with_name('test_runner.py')),
                       '--ci-python-shard', str(index), 'discover', '-s', 'scripts', '-p', pattern, '-v']
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=20,
                env={**{key: value for key, value in os.environ.items() if key != 'PYTHONPATH'},
                     'TOR_TEST_PROFILE': profile, 'TOR_TEST_TIMINGS': str(timings),
                     'GITHUB_SHA': 'a' * 40, 'GITHUB_RUN_ID': '123'})
            rows = [json.loads(line) for line in timings.read_text().splitlines()] if timings.exists() else []
            return result, rows

    def test_debug_actual_dispatch_retains_pure_and_process_full_union(self):
        results = [self.execute(index, profile='debug') for index in (1, 2)]
        self.assertTrue(all(result.returncode == 0 for result, _ in results),
                        [result.stderr for result, _ in results])
        expected = ['test_future_pure.Owner.test_owner', 'test_future_process.Owner.test_owner',
                    'test_rogue_earned_route_process.Owner.test_owner']
        completed = [row['test_id'] for _, rows in results for row in rows if row['kind'] == 'test']
        self.assertEqual(sorted(completed), sorted(expected))
        for _, rows in results:
            self.assertEqual(sorted(rows[0]['shard']['full_test_ids']), sorted(expected))
            self.assertTrue(all(row['profile'] == 'debug' for row in rows))

    def test_actual_discovery_dispatch_runs_exactly_one_owner_per_shard(self):
        results = [self.execute(index) for index in (1, 2)]
        self.assertTrue(all(result.returncode == 0 for result, _ in results),
                        [result.stderr for result, _ in results])
        full = results[0][1][0]['shard']['full_test_ids']
        self.assertEqual(results[1][1][0]['shard']['full_test_ids'], full)
        selected = [name for _, rows in results for name in rows[0]['test_ids']]
        completed = [row['test_id'] for _, rows in results for row in rows if row['kind'] == 'test']
        self.assertEqual(sorted(selected), sorted(full))
        self.assertEqual(sorted(completed), sorted(full))
        self.assertEqual(len(completed), 2)

    def test_actual_shard_failure_remains_failure_and_has_no_success_footer(self):
        result, rows = self.execute(1, failing=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(rows[-1]['kind'], 'suite')
        self.assertFalse(rows[-1]['successful'])
        self.assertEqual(rows[-1]['failures'], 1)


if __name__ == '__main__':
    unittest.main()
